//! TSP transport (spec 6.1): split PTY output into plain bytes and TSP messages, decode the
//! messages omp sends and encode the replies we write back.
//!
//! Framing is `ESC _ tsp ; <verb> { ; <key>=<value> } ; <body> ESC \` (reference section 1),
//! or the same after `ESC ] 877 ; tsp ;` ended by `ESC \` or `BEL`.

use std::collections::{HashMap, HashSet};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use mkrew_protocol::{TspBlob, TspNode, TspSurface};
use serde::Deserialize;
use serde_json::Value;

pub const APC_PREFIX: &[u8] = b"\x1b_tsp;";
pub const OSC_PREFIX: &[u8] = b"\x1b]877;tsp;";
/// Longest message body (the bytes between the prefix and the terminator) we keep.
pub const MAX_MESSAGE: usize = 4 * 1024 * 1024;

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
/// Chunked messages open at once; a stream that opens more is garbage.
const MAX_OPEN_CHUNKS: usize = 16;
/// Dropped chunk ids remembered at once; a stream that drops more is garbage.
const MAX_POISONED: usize = 256;

/// One raw TSP message: `verb`, `k=v` params in order, body bytes (chunks already joined).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raw {
    pub verb: u8,
    pub params: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Piece {
    Plain(Vec<u8>),
    Message(Raw),
}

/// A chunked message so far: the first chunk's params and the joined body.
#[derive(Debug)]
struct Chunked {
    params: Vec<(String, String)>,
    body: Vec<u8>,
}

#[derive(Debug)]
pub struct Splitter {
    /// An unfinished message, or a tail that may become the start of one. Starts with ESC.
    buf: Vec<u8>,
    /// Bytes of `buf` already searched for a terminator.
    scanned: usize,
    /// Inside an oversized message that was dropped: bytes are discarded up to its terminator.
    skipping: bool,
    /// The skipped message is in the OSC envelope (`BEL` ends it too).
    skip_osc: bool,
    /// The last skipped byte was `ESC`, so a `\` at the start of the next chunk ends the message.
    skip_esc: bool,
    chunks: HashMap<String, Chunked>,
    /// Chunk ids whose message was dropped; their chunks are discarded up to the final one.
    poisoned: HashSet<String>,
    dropped: usize,
    max_message: usize,
    max_chunks: usize,
}

impl Default for Splitter {
    fn default() -> Self {
        Self {
            buf: Vec::new(),
            scanned: 0,
            skipping: false,
            skip_osc: false,
            skip_esc: false,
            chunks: HashMap::new(),
            poisoned: HashSet::new(),
            dropped: 0,
            max_message: MAX_MESSAGE,
            max_chunks: MAX_OPEN_CHUNKS,
        }
    }
}

impl Splitter {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    fn with_limits(max_message: usize, max_chunks: usize) -> Self {
        Self {
            max_message,
            max_chunks,
            ..Self::default()
        }
    }

    /// How many messages were dropped (over 4 MiB or malformed) since the last call.
    pub fn take_dropped(&mut self) -> usize {
        std::mem::take(&mut self.dropped)
    }

    /// Splits one output chunk; keeps an unfinished message (or a prefix that may start one) for the next call.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Piece> {
        if bytes.is_empty() {
            return Vec::new();
        }
        // Copy only when something is held; otherwise scan the incoming slice in place.
        let mut owned = std::mem::take(&mut self.buf);
        let joined = !owned.is_empty();
        if joined {
            owned.extend_from_slice(bytes);
        }
        let data: &[u8] = if joined { &owned } else { bytes };
        let mut at = 0;
        if self.skipping {
            let end = if self.skip_esc && data[0] == b'\\' {
                Some(1)
            } else {
                find_end(data, 0, self.skip_osc).map(|(end, term)| end + term)
            };
            let Some(end) = end else {
                self.skip_esc = data.last() == Some(&ESC);
                return Vec::new();
            };
            self.skipping = false;
            at = end;
        }
        let mut pieces = Vec::new();
        let mut plain = Vec::new();
        let mut hold = false;
        while at < data.len() {
            let Some(off) = data[at..].iter().position(|&b| b == ESC) else {
                plain.extend_from_slice(&data[at..]);
                at = data.len();
                break;
            };
            plain.extend_from_slice(&data[at..at + off]);
            at += off;
            let rest = &data[at..];
            let (prefix, osc) = if rest.starts_with(APC_PREFIX) {
                (APC_PREFIX.len(), false)
            } else if rest.starts_with(OSC_PREFIX) {
                (OSC_PREFIX.len(), true)
            } else if APC_PREFIX.starts_with(rest) || OSC_PREFIX.starts_with(rest) {
                // The tail is an incomplete escape sequence, which a terminal shows nothing for
                // yet, so holding it until the next chunk delays no visible output.
                hold = true;
                break;
            } else {
                plain.push(ESC);
                at += 1;
                continue;
            };
            // Resume the terminator search where the last call stopped (one byte back for `ESC \`).
            let from = if at == 0 {
                prefix.max(self.scanned.saturating_sub(1))
            } else {
                prefix
            };
            let Some((end, term)) = find_end(rest, from, osc) else {
                // A trailing ESC may be the start of the terminator, not body.
                let trailing_esc = rest.last() == Some(&ESC);
                if rest.len() - prefix - usize::from(trailing_esc) > self.max_message {
                    self.dropped += 1;
                    self.skipping = true;
                    self.skip_osc = osc;
                    self.skip_esc = trailing_esc;
                    at = data.len();
                } else {
                    hold = true;
                }
                break;
            };
            let body = &rest[prefix..end];
            at += end + term;
            if body.len() > self.max_message {
                self.dropped += 1;
            } else if let Some(message) = self.message(body) {
                if !plain.is_empty() {
                    pieces.push(Piece::Plain(std::mem::take(&mut plain)));
                }
                pieces.push(Piece::Message(message));
            }
        }
        if !plain.is_empty() {
            pieces.push(Piece::Plain(plain));
        }
        if hold {
            self.scanned = data.len() - at;
            if joined {
                owned.drain(..at);
                self.buf = owned;
            } else {
                self.buf = data[at..].to_vec();
            }
        } else {
            self.scanned = 0;
        }
        pieces
    }

    /// Remembers `id` so its remaining chunks are discarded up to the final one.
    fn poison(&mut self, id: String) {
        if self.poisoned.len() >= MAX_POISONED {
            self.poisoned.clear();
        }
        self.poisoned.insert(id);
    }

    /// Parses one message body and joins chunks: `None` while a chunked message is incomplete or when malformed.
    fn message(&mut self, body: &[u8]) -> Option<Raw> {
        let Some(mut message) = parse(body) else {
            self.dropped += 1;
            return None;
        };
        let Some(pos) = message.params.iter().position(|(k, _)| k == "c") else {
            return Some(message);
        };
        let id = message.params.remove(pos).1;
        let more = match message.params.iter().position(|(k, _)| k == "m") {
            Some(pos) => message.params.remove(pos).1 == "1",
            None => false,
        };
        if self.poisoned.contains(&id) {
            if !more {
                self.poisoned.remove(&id);
            }
            return None;
        }
        if self.chunks.len() >= self.max_chunks && !self.chunks.contains_key(&id) {
            let evicted: Vec<String> = self.chunks.drain().map(|(id, _)| id).collect();
            for id in evicted {
                self.poison(id);
            }
            self.dropped += 1;
        }
        let held = self.chunks.entry(id.clone()).or_insert_with(|| Chunked {
            params: std::mem::take(&mut message.params),
            body: Vec::new(),
        });
        held.body.extend_from_slice(&message.body);
        if held.body.len() > self.max_message {
            self.chunks.remove(&id);
            if more {
                self.poison(id);
            }
            self.dropped += 1;
            return None;
        }
        if more {
            return None;
        }
        let done = self.chunks.remove(&id)?;
        Some(Raw {
            verb: message.verb,
            params: done.params,
            body: done.body,
        })
    }
}

/// Typed message omp to terminal.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Hello {
        versions: Vec<u64>,
        features: Vec<String>,
    },
    BlobsQuery {
        ids: Vec<String>,
    },
    /// The new surface with an empty `col` root named like the surface and `seq` 0.
    Open(TspSurface),
    Close {
        surface: String,
        keep: bool,
    },
    /// The palette is read by nobody (spec 7.2); the message is only recognized.
    Theme,
    Frame {
        surface: String,
        seq: u64,
        ops: Vec<Value>,
    },
    Blob(TspBlob),
    Unknown {
        verb: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    #[error("tsp `{verb}` message: {reason}")]
    Body { verb: char, reason: String },
    #[error("tsp message body of {len} bytes is over the limit of {limit}")]
    TooLarge { len: usize, limit: usize },
}

#[derive(Deserialize)]
struct Query {
    q: String,
    #[serde(default)]
    v: Vec<u64>,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    ids: Vec<String>,
}

#[derive(Deserialize)]
struct Open {
    id: String,
    mode: String,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Deserialize)]
struct Close {
    id: String,
    #[serde(default = "keep_by_default")]
    keep: bool,
}

fn keep_by_default() -> bool {
    true
}

#[derive(Deserialize)]
struct Frame {
    sf: String,
    s: u64,
    ops: Vec<Value>,
}

fn bad(raw: &Raw, reason: impl ToString) -> WireError {
    WireError::Body {
        verb: raw.verb as char,
        reason: reason.to_string(),
    }
}

fn parse_body<T: serde::de::DeserializeOwned>(raw: &Raw) -> Result<T, WireError> {
    serde_json::from_slice(&raw.body).map_err(|e| bad(raw, e))
}

pub fn decode(raw: &Raw) -> Result<Incoming, WireError> {
    match raw.verb {
        b'q' => {
            let query: Query = parse_body(raw)?;
            Ok(match query.q.as_str() {
                "hello" => Incoming::Hello {
                    versions: query.v,
                    features: query.features,
                },
                "blobs" => Incoming::BlobsQuery { ids: query.ids },
                _ => Incoming::Unknown { verb: raw.verb },
            })
        }
        b'o' => {
            let open: Open = parse_body(raw)?;
            Ok(Incoming::Open(TspSurface {
                root: TspNode {
                    id: open.id.clone(),
                    k: "col".to_owned(),
                    p: serde_json::Map::new(),
                    c: Vec::new(),
                },
                id: open.id,
                mode: open.mode,
                role: open.role,
                title: open.title,
                read_only: false,
                seq: 0,
            }))
        }
        b'x' => {
            let close: Close = parse_body(raw)?;
            Ok(Incoming::Close {
                surface: close.id,
                keep: close.keep,
            })
        }
        b't' => Ok(Incoming::Theme),
        b'f' => {
            let frame: Frame = parse_body(raw)?;
            Ok(Incoming::Frame {
                surface: frame.sf,
                seq: frame.s,
                ops: frame.ops,
            })
        }
        b'b' => {
            let param = |key: &str| {
                raw.params
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.clone())
                    .ok_or_else(|| bad(raw, format!("missing `{key}` param")))
            };
            Ok(Incoming::Blob(TspBlob {
                id: param("id")?,
                mime: param("mime")?,
                data: STANDARD.decode(&raw.body).map_err(|e| bad(raw, e))?,
            }))
        }
        verb => Ok(Incoming::Unknown { verb }),
    }
}

/// `ESC _ tsp ; <verb> ; <json> ESC \`; the JSON must be at most `limit` bytes (the hello's `apc`).
/// One message always: a reply is never chunked (omp cannot read a chunked hello reply).
pub fn encode(verb: u8, body: &Value, limit: usize) -> Result<Vec<u8>, WireError> {
    let json = serde_json::to_vec(body).map_err(|e| WireError::Body {
        verb: verb as char,
        reason: e.to_string(),
    })?;
    if json.len() > limit {
        return Err(WireError::TooLarge {
            len: json.len(),
            limit,
        });
    }
    let mut out = Vec::with_capacity(APC_PREFIX.len() + json.len() + 4);
    out.extend_from_slice(APC_PREFIX);
    out.push(verb);
    out.push(b';');
    out.extend_from_slice(&json);
    out.extend_from_slice(b"\x1b\\");
    Ok(out)
}

/// Position and length of the terminator (`ESC \`, or `BEL` for the OSC envelope) at or after `from`.
fn find_end(hay: &[u8], from: usize, osc: bool) -> Option<(usize, usize)> {
    let mut at = from;
    while at < hay.len() {
        match hay[at] {
            ESC if hay.get(at + 1) == Some(&b'\\') => return Some((at, 2)),
            BEL if osc => return Some((at, 1)),
            _ => at += 1,
        }
    }
    None
}

/// `<verb>[;k=v]*;<body>` as omp's `Pye` reads it; `None` unless the verb is one byte.
fn parse(body: &[u8]) -> Option<Raw> {
    let semi = body.iter().position(|&b| b == b';');
    if semi.unwrap_or(body.len()) != 1 {
        return None;
    }
    let verb = body[0];
    let mut params = Vec::new();
    if semi.is_none() {
        return Some(Raw {
            verb,
            params,
            body: Vec::new(),
        });
    }
    let mut at = 2;
    while let Some(off) = body[at..].iter().position(|&b| b == b';') {
        let Some(param) = split_param(&body[at..at + off]) else {
            break;
        };
        params.push(param);
        at += off + 1;
    }
    Some(Raw {
        verb,
        params,
        body: body[at..].to_vec(),
    })
}

/// One `key=value` segment: key `[A-Za-z0-9_-]+`, value printable ASCII without `;`.
fn split_param(segment: &[u8]) -> Option<(String, String)> {
    let eq = segment.iter().position(|&b| b == b'=')?;
    let (key, value) = (&segment[..eq], &segment[eq + 1..]);
    let key_ok = !key.is_empty()
        && key
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'));
    let value_ok = value
        .iter()
        .all(|&b| (0x21..=0x7e).contains(&b) && b != b';');
    (key_ok && value_ok).then(|| {
        (
            String::from_utf8_lossy(key).into_owned(),
            String::from_utf8_lossy(value).into_owned(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tsp::fixture;
    use serde_json::json;

    fn raw(verb: u8, params: &[(&str, &str)], body: &str) -> Raw {
        Raw {
            verb,
            params: params
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            body: body.as_bytes().to_vec(),
        }
    }

    /// Joins adjacent `Plain` pieces: where a stream was cut is not part of the result.
    fn merged(pieces: impl IntoIterator<Item = Piece>) -> Vec<Piece> {
        let mut out: Vec<Piece> = Vec::new();
        for piece in pieces {
            if let (Piece::Plain(add), Some(Piece::Plain(last))) = (&piece, out.last_mut()) {
                last.extend_from_slice(add);
                continue;
            }
            out.push(piece);
        }
        out
    }

    fn run(chunks: &[&[u8]]) -> Vec<Piece> {
        let mut splitter = Splitter::new();
        merged(chunks.iter().flat_map(|chunk| splitter.feed(chunk)))
    }

    const KITTY: &[u8] = b"\x1b_Gf=100,a=T;AAAA\x1b\\";

    /// ANSI, a kitty graphics APC, an OSC title, TSP in both envelopes (BEL and ST ended) and
    /// one body cut in two chunks with plain text between them, plus the pieces it must give.
    fn mixed() -> (Vec<u8>, Vec<Piece>) {
        let frame = r#"{"sf":"s:1","s":1,"ops":[["del","a"]]}"#;
        // `Ü` is C3 9C: the second byte is the 8-bit ST, which must not end a message.
        let long = r#"{"sf":"s:1","s":2,"ops":[["set","x",{"text":"héllo; Ü world"}]]}"#;
        let (head, tail) = long.split_at(20);
        let hello = r#"{"q":"hello","v":[1],"app":"omp","features":[]}"#;
        let blobs = r#"{"q":"blobs","ids":["a"]}"#;
        let mut stream = Vec::new();
        stream.extend_from_slice(b"hi \x1b[31mred\x1b[0m ");
        stream.extend_from_slice(KITTY);
        stream.extend_from_slice(format!("\x1b_tsp;f;{frame}\x1b\\").as_bytes());
        stream.extend_from_slice("midÜ\x1b]0;title\x07".as_bytes());
        stream.extend_from_slice(format!("\x1b]877;tsp;q;{hello}\x07").as_bytes());
        stream.extend_from_slice(format!("\x1b]877;tsp;q;{blobs}\x1b\\").as_bytes());
        stream.extend_from_slice(format!("\x1b_tsp;f;c=7;m=1;{head}\x1b\\").as_bytes());
        stream.extend_from_slice(b"xx");
        stream.extend_from_slice(format!("\x1b_tsp;f;c=7;{tail}\x1b\\").as_bytes());
        stream.extend_from_slice(b"tail");
        let want = vec![
            Piece::Plain([b"hi \x1b[31mred\x1b[0m ".as_slice(), KITTY].concat()),
            Piece::Message(raw(b'f', &[], frame)),
            Piece::Plain("midÜ\x1b]0;title\x07".as_bytes().to_vec()),
            Piece::Message(raw(b'q', &[], hello)),
            Piece::Message(raw(b'q', &[], blobs)),
            Piece::Plain(b"xx".to_vec()),
            Piece::Message(raw(b'f', &[], long)),
            Piece::Plain(b"tail".to_vec()),
        ];
        (stream, want)
    }

    #[test]
    fn a_mixed_stream_splits_into_plain_bytes_and_messages() {
        let (stream, want) = mixed();
        assert_eq!(run(&[&stream]), want);
    }

    #[test]
    fn every_split_position_gives_the_same_pieces() {
        let (stream, want) = mixed();
        for at in 0..=stream.len() {
            assert_eq!(run(&[&stream[..at], &stream[at..]]), want, "split at {at}");
        }
    }

    #[test]
    fn a_stream_fed_byte_by_byte_gives_the_same_pieces() {
        let (stream, want) = mixed();
        let chunks: Vec<&[u8]> = stream.chunks(1).collect();
        assert_eq!(run(&chunks), want);
    }

    #[test]
    fn a_partial_prefix_is_held_until_it_resolves() {
        let mut s = Splitter::new();
        assert_eq!(s.feed(b"abc\x1b_t"), vec![Piece::Plain(b"abc".to_vec())]);
        assert_eq!(
            s.feed(b"sp;f;{}\x1b\\"),
            vec![Piece::Message(raw(b'f', &[], "{}"))]
        );
        // The held bytes turn out not to be TSP: they come back as plain bytes.
        assert!(s.feed(b"\x1b_t").is_empty());
        assert_eq!(s.feed(b"xyz"), vec![Piece::Plain(b"\x1b_txyz".to_vec())]);
        // A lone ESC at the end of a chunk is held too, and released with the next byte.
        assert_eq!(s.feed(b"a\x1b"), vec![Piece::Plain(b"a".to_vec())]);
        assert_eq!(s.feed(b"[0m"), vec![Piece::Plain(b"\x1b[0m".to_vec())]);
    }

    #[test]
    fn another_apc_passes_through_as_plain_bytes() {
        let bytes = [KITTY, b"ok"].concat();
        assert_eq!(run(&[&bytes]), vec![Piece::Plain(bytes.clone())]);
    }

    #[test]
    fn params_stop_at_the_first_segment_that_is_not_key_value() {
        let got = run(&[
            b"\x1b_tsp;b;id=ab;mime=image/png;QUJD==\x1b\\",
            b"\x1b_tsp;f;{\"a\":\"x=1;y\"}\x1b\\",
            b"\x1b_tsp;e\x1b\\",
        ]);
        assert_eq!(
            got,
            vec![
                Piece::Message(raw(b'b', &[("id", "ab"), ("mime", "image/png")], "QUJD==")),
                Piece::Message(raw(b'f', &[], r#"{"a":"x=1;y"}"#)),
                Piece::Message(raw(b'e', &[], "")),
            ]
        );
    }

    #[test]
    fn chunks_join_in_order_and_keep_the_first_chunks_params() {
        let got = run(&[
            b"\x1b_tsp;b;id=ab;mime=image/png;c=1;m=1;QUJD\x1b\\",
            b"\x1b_tsp;b;c=1;REVG\x1b\\",
        ]);
        assert_eq!(
            got,
            vec![Piece::Message(raw(
                b'b',
                &[("id", "ab"), ("mime", "image/png")],
                "QUJDREVG"
            ))]
        );
    }

    #[test]
    fn interleaved_chunk_ids_are_joined_separately() {
        let got = run(&[
            b"\x1b_tsp;f;c=1;m=1;{\"a\":\x1b\\",
            b"\x1b_tsp;f;c=2;m=1;{\"b\":\x1b\\",
            b"\x1b_tsp;f;c=2;2}\x1b\\",
            b"\x1b_tsp;f;c=1;1}\x1b\\",
        ]);
        assert_eq!(
            got,
            vec![
                Piece::Message(raw(b'f', &[], r#"{"b":2}"#)),
                Piece::Message(raw(b'f', &[], r#"{"a":1}"#)),
            ]
        );
    }

    #[test]
    fn a_message_with_a_bad_verb_is_dropped_not_leaked_as_plain() {
        let mut s = Splitter::new();
        assert_eq!(
            s.feed(b"\x1b_tsp;zz;{}\x1b\\ok"),
            vec![Piece::Plain(b"ok".to_vec())]
        );
        assert_eq!(s.take_dropped(), 1);
    }

    #[test]
    fn an_unterminated_message_over_4_mib_is_dropped_and_the_splitter_resets() {
        let mut s = Splitter::new();
        assert!(s.feed(APC_PREFIX).is_empty());
        let mib = vec![b'a'; 1024 * 1024];
        for _ in 0..4 {
            assert!(s.feed(&mib).is_empty());
        }
        // Exactly 4 MiB of body is still held.
        assert_eq!(s.take_dropped(), 0);
        // One more byte: dropped, nothing leaks out as plain bytes.
        assert!(s.feed(b"a").is_empty());
        assert_eq!(s.take_dropped(), 1);
        assert_eq!(s.take_dropped(), 0);
        // The rest of the dropped message is skipped up to its terminator; then the splitter is usable again.
        assert!(s.feed(b"aaa\x1b\\").is_empty());
        assert_eq!(
            s.feed(b"\x1b_tsp;f;{}\x1b\\"),
            vec![Piece::Message(raw(b'f', &[], "{}"))]
        );
    }

    #[test]
    fn an_oversized_message_is_skipped_to_its_terminator_across_feeds() {
        let mut s = Splitter::with_limits(16, 16);
        let mut got = s.feed(APC_PREFIX);
        got.extend(s.feed(b"f;aaaaaaaaaa"));
        assert!(got.is_empty());
        assert_eq!(s.take_dropped(), 0);
        // Over the limit: dropped, and the rest of the message is not plain output.
        assert!(s.feed(b"aaaaaaaaaa").is_empty());
        assert_eq!(s.take_dropped(), 1);
        assert!(s.feed(b"aaaaaaaaaa").is_empty());
        // The terminator split across feeds, then text after it passes through.
        assert!(s.feed(b"a\x1b").is_empty());
        assert_eq!(s.feed(b"\\after"), vec![Piece::Plain(b"after".to_vec())]);
        assert_eq!(s.take_dropped(), 0);
    }

    #[test]
    fn a_body_of_exactly_the_limit_is_kept_when_its_escape_arrives_later() {
        let mut s = Splitter::with_limits(16, 16);
        let mut got = s.feed(&[APC_PREFIX, b"f;", &[b'a'; 14]].concat());
        got.extend(s.feed(b"\x1b"));
        got.extend(s.feed(b"\\"));
        assert_eq!(s.take_dropped(), 0);
        assert_eq!(got.len(), 1);
        assert!(matches!(&got[0], Piece::Message(m) if m.body.len() == 14));
    }

    #[test]
    fn chunks_over_the_join_limit_are_dropped_up_to_the_final_one() {
        let mut s = Splitter::with_limits(32, 16);
        let chunk = |c: &str, more: bool, body: &str| {
            let m = if more { "m=1;" } else { "" };
            format!("\x1b_tsp;f;c={c};{m}{body}\x1b\\").into_bytes()
        };
        assert!(s.feed(&chunk("1", true, &"a".repeat(20))).is_empty());
        assert!(s.feed(&chunk("1", true, &"a".repeat(20))).is_empty());
        assert_eq!(s.take_dropped(), 1);
        assert!(s.feed(&chunk("1", true, "bb")).is_empty());
        // The final chunk of the dropped message does not come out as a message.
        assert!(s.feed(&chunk("1", false, "cc")).is_empty());
        // The id is free again afterwards.
        assert_eq!(
            s.feed(&chunk("1", false, "{}")),
            vec![Piece::Message(raw(b'f', &[], "{}"))]
        );
        assert_eq!(s.take_dropped(), 0);
    }

    #[test]
    fn chunks_of_messages_evicted_by_too_many_open_ids_are_dropped() {
        let mut s = Splitter::with_limits(MAX_MESSAGE, 2);
        let chunk = |c: &str, more: bool, body: &str| {
            let m = if more { "m=1;" } else { "" };
            format!("\x1b_tsp;f;c={c};{m}{body}\x1b\\").into_bytes()
        };
        assert!(s.feed(&chunk("1", true, "a")).is_empty());
        assert!(s.feed(&chunk("2", true, "b")).is_empty());
        // A third id evicts the open ones.
        assert!(s.feed(&chunk("3", true, "c")).is_empty());
        assert_eq!(s.take_dropped(), 1);
        assert!(s.feed(&chunk("1", false, "x")).is_empty());
        assert!(s.feed(&chunk("2", true, "y")).is_empty());
        assert!(s.feed(&chunk("2", false, "z")).is_empty());
        assert_eq!(
            s.feed(&chunk("3", false, "d")),
            vec![Piece::Message(raw(b'f', &[], "cd"))]
        );
    }

    #[test]
    fn a_terminated_message_is_bounded_by_the_same_limit() {
        let wire = |payload: &[u8]| [APC_PREFIX, payload, b"\x1b\\".as_slice()].concat();
        let mut payload = b"f;".to_vec();
        payload.extend(vec![b'a'; MAX_MESSAGE - 2]);
        let mut s = Splitter::new();
        let got = s.feed(&wire(&payload));
        assert!(matches!(&got[..], [Piece::Message(m)] if m.body.len() == MAX_MESSAGE - 2));
        payload.push(b'a');
        assert!(s.feed(&wire(&payload)).is_empty());
        assert_eq!(s.take_dropped(), 1);
    }

    fn decoded(verb: u8, params: &[(&str, &str)], body: &str) -> Result<Incoming, WireError> {
        decode(&raw(verb, params, body))
    }

    #[test]
    fn decode_reads_the_hello_and_the_blobs_query() {
        let hello = r#"{"q":"hello","v":[1,2],"app":"omp","features":["edit","undo","send"]}"#;
        assert_eq!(
            decoded(b'q', &[], hello),
            Ok(Incoming::Hello {
                versions: vec![1, 2],
                features: vec!["edit".into(), "undo".into(), "send".into()],
            })
        );
        assert_eq!(
            decoded(b'q', &[], r#"{"q":"blobs","ids":["a","b"]}"#),
            Ok(Incoming::BlobsQuery {
                ids: vec!["a".into(), "b".into()]
            })
        );
        // A query kind we do not know is ignored, not an error.
        assert_eq!(
            decoded(b'q', &[], r#"{"q":"later"}"#),
            Ok(Incoming::Unknown { verb: b'q' })
        );
    }

    #[test]
    fn decode_open_builds_an_empty_surface_with_its_own_id_as_root() {
        let body = r#"{"id":"s:2","mode":"screen","title":"Agents","role":"omp.hub","adopt":true}"#;
        assert_eq!(
            decoded(b'o', &[], body),
            Ok(Incoming::Open(TspSurface {
                id: "s:2".into(),
                mode: "screen".into(),
                role: Some("omp.hub".into()),
                title: Some("Agents".into()),
                read_only: false,
                seq: 0,
                root: TspNode {
                    id: "s:2".into(),
                    k: "col".into(),
                    p: Default::default(),
                    c: Vec::new(),
                },
            }))
        );
        let Ok(Incoming::Open(bare)) = decoded(b'o', &[], r#"{"id":"s:1","mode":"inline"}"#) else {
            panic!("open without title and role must decode");
        };
        assert_eq!((bare.role, bare.title), (None, None));
    }

    #[test]
    fn decode_close_reads_keep_and_defaults_to_keeping() {
        let close = |body| decoded(b'x', &[], body);
        assert_eq!(
            close(r#"{"id":"s:1","keep":false}"#),
            Ok(Incoming::Close {
                surface: "s:1".into(),
                keep: false
            })
        );
        assert_eq!(
            close(r#"{"id":"s:1","keep":true}"#),
            Ok(Incoming::Close {
                surface: "s:1".into(),
                keep: true
            })
        );
        assert_eq!(
            close(r#"{"id":"s:3"}"#),
            Ok(Incoming::Close {
                surface: "s:3".into(),
                keep: true
            })
        );
    }

    #[test]
    fn decode_frame_keeps_the_ops_verbatim_and_rejects_a_bad_body() {
        let body = r#"{"sf":"s:1","s":3,"ops":[["settle","a"],["suspend"]]}"#;
        assert_eq!(
            decoded(b'f', &[], body),
            Ok(Incoming::Frame {
                surface: "s:1".into(),
                seq: 3,
                ops: vec![json!(["settle", "a"]), json!(["suspend"])],
            })
        );
        for broken in [
            r#"{"sf":"s:1","s":3,"ops":"x"}"#,
            r#"{"sf":"s:1","ops":[]}"#,
            "not json",
        ] {
            assert!(
                matches!(
                    decoded(b'f', &[], broken),
                    Err(WireError::Body { verb: 'f', .. })
                ),
                "{broken}"
            );
        }
    }

    #[test]
    fn decode_blob_reads_id_and_mime_from_params_and_base64_from_the_body() {
        let params = [("id", "ab12"), ("mime", "image/png")];
        assert_eq!(
            decoded(b'b', &params, "QUJD"),
            Ok(Incoming::Blob(TspBlob {
                id: "ab12".into(),
                mime: "image/png".into(),
                data: b"ABC".to_vec(),
            }))
        );
        assert!(matches!(
            decoded(b'b', &[("id", "ab12")], "QUJD"),
            Err(WireError::Body { verb: 'b', .. })
        ));
        assert!(matches!(
            decoded(b'b', &params, "not base64!"),
            Err(WireError::Body { verb: 'b', .. })
        ));
    }

    #[test]
    fn decode_ignores_the_palette_and_unknown_verbs() {
        assert_eq!(decoded(b't', &[], "{}"), Ok(Incoming::Theme));
        assert_eq!(
            decoded(b'z', &[], "{}"),
            Ok(Incoming::Unknown { verb: b'z' })
        );
    }

    #[test]
    fn encode_frames_one_message() {
        assert_eq!(
            encode(b'e', &json!({"ev": "x"}), 64).unwrap(),
            b"\x1b_tsp;e;{\"ev\":\"x\"}\x1b\\".to_vec()
        );
    }

    #[test]
    fn encode_refuses_a_body_over_the_limit_and_accepts_one_at_the_limit() {
        let body = json!({"ev": "x"}); // 10 bytes of JSON
        assert_eq!(
            encode(b'e', &body, 9),
            Err(WireError::TooLarge { len: 10, limit: 9 })
        );
        assert!(encode(b'e', &body, 10).is_ok());
    }

    #[test]
    fn an_encoded_body_never_contains_a_raw_escape_and_splits_back_to_the_same_json() {
        let body = json!({"ev": "error", "msg": "bad \u{1b}\\ and \u{7} bytes; a=b"});
        let wire = encode(b'e', &body, MAX_MESSAGE).unwrap();
        let inner = &wire[APC_PREFIX.len() + 2..wire.len() - 2];
        assert!(!inner.contains(&ESC) && !inner.contains(&BEL));
        let mut s = Splitter::new();
        let mut pieces = s.feed(&wire);
        let Some(Piece::Message(message)) = pieces.pop() else {
            panic!("expected one message");
        };
        assert!(pieces.is_empty());
        assert_eq!(message.verb, b'e');
        assert_eq!(
            serde_json::from_slice::<Value>(&message.body).unwrap(),
            body
        );
    }

    #[test]
    fn the_startup_fixture_round_trips_through_encode_split_and_decode() {
        let mut s = Splitter::new();
        let mut seqs = Vec::new();
        let mut opened = None;
        for (i, (verb, body)) in fixture::out_lines().into_iter().enumerate() {
            let wire = encode(verb, &body, MAX_MESSAGE).unwrap();
            let mut pieces = s.feed(&wire);
            assert_eq!(pieces.len(), 1, "line {i}");
            let Some(Piece::Message(message)) = pieces.pop() else {
                panic!("line {i} did not come back as a message");
            };
            match decode(&message).unwrap() {
                Incoming::Open(surface) => {
                    assert_eq!(i, 0);
                    opened = Some(surface);
                }
                Incoming::Theme => assert_eq!(i, 1),
                Incoming::Frame { surface, seq, ops } => {
                    assert_eq!(surface, "s:1");
                    assert!(!ops.is_empty());
                    if seq == 1 {
                        assert_eq!(
                            ops[0],
                            json!(["add", "main", "s:1", null, {"id": "main", "k": "col"}])
                        );
                    }
                    seqs.push(seq);
                }
                other => panic!("line {i}: unexpected {other:?}"),
            }
        }
        assert_eq!(seqs, (1..=10).collect::<Vec<u64>>());
        let surface = opened.unwrap();
        assert_eq!(
            (
                surface.id.as_str(),
                surface.mode.as_str(),
                surface.role.as_deref(),
                surface.title.as_deref()
            ),
            ("s:1", "inline", Some("omp.session"), Some("omp"))
        );
    }
}
