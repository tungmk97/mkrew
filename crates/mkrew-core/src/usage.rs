//! Token usage from agent transcripts (SPEC §20E.3), read incrementally.
//!
//! Pure: the daemon reads the bytes appended to a transcript since [`UsageReader::offset`] and
//! hands them to [`UsageReader::feed`]. A partial last line is kept until its newline arrives.
//! Lines that are not JSON are skipped silently; JSON that looks like a usage line but lacks the
//! token fields gives one warning (format changed).

use std::collections::HashMap;

use serde_json::Value;

use crate::profile::UsageFormat;

/// Token totals of one agent session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Usage {
    /// Input tokens over the whole session.
    pub input: u64,
    /// Output tokens over the whole session.
    pub output: u64,
    /// Context size of the newest message.
    pub context: u64,
}

impl Usage {
    /// `context` as a whole percentage of `window` (rounded down); `None` without a window.
    pub fn context_percent(&self, window: Option<u64>) -> Option<u64> {
        window
            .filter(|w| *w > 0)
            .map(|w| self.context.saturating_mul(100) / w)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Claude,
    Codex,
}

/// Incremental reader for one transcript file.
#[derive(Debug, Clone)]
pub struct UsageReader {
    format: Format,
    /// Bytes of the file consumed so far, including `partial`.
    offset: u64,
    partial: Vec<u8>,
    usage: Option<Usage>,
    /// Claude repeats one API message's usage on every content-block line: the counts last
    /// added per `message.id` + `requestId`, replaced (not added) on a repeat. Kept for the
    /// whole session, not just the last key, since sidechain lines can interleave.
    seen: HashMap<String, (u64, u64)>,
    warning: Option<String>,
    warned: bool,
}

impl UsageReader {
    /// A reader for `format`; an unknown format gives the one warning to log instead.
    pub fn new(format: &UsageFormat) -> Result<Self, String> {
        let format = match format {
            UsageFormat::ClaudeJsonl => Format::Claude,
            UsageFormat::CodexJsonl => Format::Codex,
            UsageFormat::Unknown(name) => {
                return Err(format!("unknown usage format `{name}`: no usage shown"));
            }
        };
        Ok(Self {
            format,
            offset: 0,
            partial: Vec::new(),
            usage: None,
            seen: HashMap::new(),
            warning: None,
            warned: false,
        })
    }

    /// Where the next read of the file starts.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// The totals so far; `None` until a usage line was seen.
    pub fn usage(&self) -> Option<Usage> {
        self.usage
    }

    /// The warning about an unrecognised line, at most once per reader.
    pub fn take_warning(&mut self) -> Option<String> {
        self.warning.take()
    }

    /// Consumes bytes appended to the file at [`Self::offset`] and returns the totals so far.
    pub fn feed(&mut self, appended: &[u8]) -> Option<Usage> {
        self.offset += appended.len() as u64;
        self.partial.extend_from_slice(appended);
        let Some(end) = self.partial.iter().rposition(|b| *b == b'\n') else {
            return self.usage;
        };
        let rest = self.partial.split_off(end + 1);
        let complete = std::mem::replace(&mut self.partial, rest);
        for line in complete.split(|b| *b == b'\n') {
            let Ok(value) = serde_json::from_slice::<Value>(line) else {
                continue;
            };
            let parsed = match self.format {
                Format::Claude => self.claude_line(&value),
                Format::Codex => codex_line(&value),
            };
            match parsed {
                Line::Other => {}
                Line::Usage(usage) => self.usage = Some(usage),
                Line::Broken(what) => self.warn(what),
            }
        }
        self.usage
    }

    fn warn(&mut self, what: &str) {
        if !self.warned {
            self.warned = true;
            self.warning = Some(format!("usage transcript format changed? {what}"));
        }
    }

    /// `type: "assistant"` lines carry `message.usage`. Input counts what the model read
    /// (`input_tokens` + cache creation + cache read, like Codex's `input_tokens`); context is
    /// that sum on the newest non-sidechain message (without its output, unlike Codex's).
    fn claude_line(&mut self, value: &Value) -> Line {
        if value["type"] != "assistant" {
            return Line::Other;
        }
        let message = &value["message"];
        if message["model"] == "<synthetic>" {
            return Line::Other;
        }
        let usage = &message["usage"];
        let (Some(fresh), Some(output)) = (
            usage["input_tokens"].as_u64(),
            usage["output_tokens"].as_u64(),
        ) else {
            return Line::Broken("assistant line without message.usage token counts");
        };
        let cache = |key: &str| usage[key].as_u64().unwrap_or(0);
        let input = fresh + cache("cache_creation_input_tokens") + cache("cache_read_input_tokens");

        let mut total = self.usage.unwrap_or_default();
        let key = format!(
            "{}\n{}",
            message["id"].as_str().unwrap_or_default(),
            value["requestId"].as_str().unwrap_or_default()
        );
        // A line without ids cannot be matched to its siblings: count it on its own.
        let key = (key != "\n").then_some(key);
        if let Some((old_in, old_out)) = key.as_ref().and_then(|k| self.seen.get(k)) {
            total.input -= old_in;
            total.output -= old_out;
        }
        if let Some(key) = key {
            self.seen.insert(key, (input, output));
        }
        total.input += input;
        total.output += output;
        if value["isSidechain"] != true {
            total.context = input;
        }
        Line::Usage(total)
    }
}

enum Line {
    /// Not a usage line.
    Other,
    /// New totals.
    Usage(Usage),
    /// Looks like a usage line but the fields are not where they should be.
    Broken(&'static str),
}

/// `event_msg` lines with `payload.type: "token_count"` carry cumulative totals in
/// `info.total_token_usage` (`info` is null before the first answer); context is
/// `info.last_token_usage.total_tokens`, what Codex itself counts as in the window.
fn codex_line(value: &Value) -> Line {
    let payload = &value["payload"];
    if value["type"] != "event_msg" || payload["type"] != "token_count" {
        return Line::Other;
    }
    let info = &payload["info"];
    if info.is_null() {
        return Line::Other;
    }
    let total = &info["total_token_usage"];
    match (
        total["input_tokens"].as_u64(),
        total["output_tokens"].as_u64(),
        info["last_token_usage"]["total_tokens"].as_u64(),
    ) {
        (Some(input), Some(output), Some(context)) => Line::Usage(Usage {
            input,
            output,
            context,
        }),
        _ => Line::Broken("token_count line without total/last token usage"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE: &[u8] = include_bytes!("../tests/fixtures/usage/claude.jsonl");
    const CODEX: &[u8] = include_bytes!("../tests/fixtures/usage/codex.jsonl");

    /// Two content-block lines of one message counted once, the sidechain line in the totals
    /// but not the context, the synthetic line ignored.
    const CLAUDE_USAGE: Usage = Usage {
        input: (2 + 688 + 48215) + (3 + 9000) + (1 + 1200 + 48903),
        output: 206 + 40 + 512,
        context: 1 + 1200 + 48903,
    };

    /// The newest cumulative totals, not their sum; context = last `total_tokens`.
    const CODEX_USAGE: Usage = Usage {
        input: 51122,
        output: 1561,
        context: 35579,
    };

    fn fixtures() -> [(UsageFormat, &'static [u8], Usage); 2] {
        [
            (UsageFormat::ClaudeJsonl, CLAUDE, CLAUDE_USAGE),
            (UsageFormat::CodexJsonl, CODEX, CODEX_USAGE),
        ]
    }

    fn reader(format: &UsageFormat) -> UsageReader {
        UsageReader::new(format).unwrap()
    }

    #[test]
    fn fixtures_give_the_expected_totals() {
        for (format, bytes, expected) in fixtures() {
            let mut r = reader(&format);
            assert_eq!(r.feed(bytes), Some(expected), "{format:?}");
            assert_eq!(r.offset(), bytes.len() as u64);
            assert_eq!(r.take_warning(), None, "{format:?}");
        }
    }

    #[test]
    fn reading_in_any_chunks_gives_the_same_totals() {
        for (format, bytes, expected) in fixtures() {
            for size in [1, 7, 100, 1000] {
                let mut r = reader(&format);
                let mut fed = 0;
                for chunk in bytes.chunks(size) {
                    assert_eq!(r.offset(), fed);
                    r.feed(chunk);
                    fed += chunk.len() as u64;
                }
                assert_eq!(r.usage(), Some(expected), "{format:?} in chunks of {size}");
                assert_eq!(r.offset(), bytes.len() as u64);
            }
        }
    }

    #[test]
    fn partial_last_line_waits_for_its_newline() {
        let line = br#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":5,"output_tokens":2},"last_token_usage":{"total_tokens":7}}}}"#;
        let mut r = reader(&UsageFormat::CodexJsonl);
        assert_eq!(r.feed(line), None);
        assert_eq!(r.offset(), line.len() as u64);
        let usage = Usage {
            input: 5,
            output: 2,
            context: 7,
        };
        assert_eq!(r.feed(b"\r\n"), Some(usage));
        assert_eq!(r.feed(b"{\"type\":"), Some(usage));
    }

    #[test]
    fn turns_appended_later_add_up() {
        let (first, second) = CLAUDE.split_at(CLAUDE.len() / 2);
        let mut r = reader(&UsageFormat::ClaudeJsonl);
        let early = r.feed(first).unwrap();
        assert!(early.output < CLAUDE_USAGE.output, "{early:?}");
        assert_eq!(r.feed(second), Some(CLAUDE_USAGE));
    }

    #[test]
    fn unknown_format_is_no_reader_and_one_warning() {
        let err = UsageReader::new(&UsageFormat::Unknown("gemini".into())).unwrap_err();
        assert!(err.contains("gemini"), "{err}");
    }

    #[test]
    fn changed_format_warns_once() {
        let mut r = reader(&UsageFormat::ClaudeJsonl);
        let broken = b"{\"type\":\"assistant\",\"message\":{\"tokens\":3}}\n";
        assert_eq!(r.feed(broken), None);
        assert!(r.take_warning().unwrap().contains("message.usage"));
        r.feed(broken);
        assert_eq!(r.take_warning(), None);

        let mut r = reader(&UsageFormat::CodexJsonl);
        r.feed(b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{}}}\nnot json\n");
        assert!(r.take_warning().unwrap().contains("token_count"));
        assert_eq!(r.usage(), None);
    }

    #[test]
    fn context_percent_needs_a_window() {
        let usage = Usage {
            context: 124_000,
            ..Usage::default()
        };
        assert_eq!(usage.context_percent(Some(200_000)), Some(62));
        assert_eq!(usage.context_percent(None), None);
        assert_eq!(usage.context_percent(Some(0)), None);
    }
}
