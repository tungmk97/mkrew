//! The newest bytes of a session's output log (SPEC §8), read by the desktop History dialog and
//! by `mk web` (ADR 0052).

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

/// The newest output of a session's log `files` (oldest first, as `session.history` lists
/// them), at most `limit` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tail {
    pub bytes: Vec<u8>,
    /// Older output was left out.
    pub cut: bool,
}

/// Where the replay of a cut log may start: just after its first line break (a UTF-8 char never
/// contains `\n`, and the partial line before it is dropped whole); with no line break, at the
/// first ESC (a whole escape sequence); with neither, after the continuation bytes of a cut
/// char. A fresh emulator starts with default attributes, so nothing needs resetting.
// ponytail: a tail with no line break and no ESC keeps its first bytes as they are (plain text,
// harmless); an ESC inside an OSC/DCS string would start there. Scan back from before the cut
// if a real log shows it.
pub fn replay_start(bytes: &[u8]) -> usize {
    if let Some(newline) = bytes.iter().position(|&byte| byte == b'\n') {
        return newline + 1;
    }
    if let Some(escape) = bytes.iter().position(|&byte| byte == 0x1b) {
        return escape;
    }
    // 0b10xx_xxxx: the continuation bytes of a UTF-8 char.
    bytes
        .iter()
        .position(|&byte| byte & 0xC0 != 0x80)
        .unwrap_or(bytes.len())
}

/// Reads the last `limit` bytes of `files` as one stream. A cut stream starts at
/// [`replay_start`], so the replay does not begin inside an escape sequence or a char. A file
/// that is gone (rotated away meanwhile) is skipped.
pub fn read_tail(files: &[PathBuf], limit: u64) -> io::Result<Tail> {
    let mut parts = Vec::new();
    let mut left = limit;
    let mut cut = false;
    for path in files.iter().rev() {
        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let len = file.metadata()?.len();
        if len > left {
            cut = true;
            file.seek(SeekFrom::Start(len - left))?;
        }
        let mut part = Vec::new();
        file.take(left).read_to_end(&mut part)?;
        // `read_to_end` stops at `left`: far below u64::MAX.
        left -= part.len() as u64;
        parts.push(part);
    }
    let mut bytes: Vec<u8> = parts.into_iter().rev().flatten().collect();
    if cut {
        bytes.drain(..replay_start(&bytes));
    }
    Ok(Tail { bytes, cut })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_tail_joins_the_parts_and_cuts_at_a_line_break() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("s.log.1");
        let new = dir.path().join("s.log");
        std::fs::write(&old, b"one\ntwo\n").unwrap();
        std::fs::write(&new, b"three\n").unwrap();
        let files = vec![old.clone(), new.clone()];

        let all = read_tail(&files, 1024).unwrap();
        assert_eq!(all.bytes, b"one\ntwo\nthree\n");
        assert!(!all.cut);

        // The last 9 bytes start inside "two": the partial line is dropped.
        let tail = read_tail(&files, 9).unwrap();
        assert_eq!(tail.bytes, b"three\n");
        assert!(tail.cut);

        // Exactly the newest file: the older part is left out, so it counts as cut.
        let newest = read_tail(&files, 6).unwrap();
        assert!(newest.cut);
        assert_eq!(
            newest.bytes, b"",
            "everything up to the first line break goes"
        );

        // An empty older part leaves nothing out.
        std::fs::write(&old, b"").unwrap();
        let exact = read_tail(&files, 6).unwrap();
        assert_eq!(exact.bytes, b"three\n");
        assert!(!exact.cut);

        assert_eq!(
            read_tail(&[], 1024).unwrap(),
            Tail {
                bytes: Vec::new(),
                cut: false
            }
        );

        std::fs::remove_file(&old).unwrap();
        let rotated = read_tail(&files, 1024).unwrap();
        assert_eq!(rotated.bytes, b"three\n");
        assert!(!rotated.cut);
    }

    #[test]
    fn replay_start_prefers_a_line_break_then_an_escape_then_a_char_boundary() {
        // The tail of "€" (E2 82 AC) cut after its first byte, then a whole line.
        assert_eq!(replay_start(b"\x82\xac rest\nnext"), 8);
        // A line break wins even when an escape sequence comes first.
        assert_eq!(replay_start(b"x\x1b[1my\nz"), 7);
        // No line break: the first ESC starts a whole sequence ("1;32m" is the end of one).
        assert_eq!(replay_start(b"1;32mgreen\x1b[0m"), 10);
        // Neither: only the continuation bytes of a cut char go.
        assert_eq!(replay_start(b"\x82\xacabc"), 2);
        assert_eq!(replay_start(b"\x82\xac"), 2);
        assert_eq!(replay_start(b"abc"), 0);
        assert_eq!(replay_start(b"abc\n"), 4);
        assert_eq!(replay_start(b""), 0);
    }

    /// Marks the bytes inside a CSI sequence (after its ESC, up to and including its final byte).
    fn inside_csi(stream: &[u8]) -> Vec<bool> {
        let mut inside = vec![false; stream.len()];
        let mut open = false;
        for (at, &byte) in stream.iter().enumerate() {
            if byte == 0x1b {
                open = true;
                continue;
            }
            if open {
                inside[at] = true;
                if (0x40..=0x7e).contains(&byte) && byte != b'[' {
                    open = false;
                }
            }
        }
        inside
    }

    /// SGR, a wide char, a 3-byte char, cursor moves, a last line without a break.
    const STREAM: &str = "\x1b[1;32mDone\x1b[0m \u{5f62} \u{20ac}5\r\n\x1b[2K\x1b[1Ahello\r\n\x1b[38;5;196mred\x1b[0m\r\nlast";

    #[test]
    fn a_replay_never_starts_inside_a_char_or_an_escape_sequence() {
        let stream = STREAM.as_bytes();
        let inside = inside_csi(stream);
        for cut in 0..=stream.len() {
            let start = cut + replay_start(&stream[cut..]);
            assert!(
                std::str::from_utf8(&stream[start..]).is_ok(),
                "cut {cut}: starts inside a char"
            );
            assert!(
                start == stream.len() || !inside[start],
                "cut {cut}: starts inside an escape sequence"
            );
        }
    }

    #[test]
    fn a_cut_log_without_line_breaks_starts_at_an_escape_sequence() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("s.log");
        std::fs::write(&log, b"\x1b[1mbold\x1b[0m long line no break").unwrap();
        // The last 26 bytes begin inside "bold": that text goes, the next sequence stays.
        let tail = read_tail(&[log], 26).unwrap();
        assert!(tail.cut);
        assert_eq!(tail.bytes, b"\x1b[0m long line no break");
    }

    #[test]
    fn a_big_log_is_read_as_whole_lines_within_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("s.log");
        std::fs::write(&log, b"0123456789abcdef\r\n".repeat(200_000)).unwrap();
        let limit = 1024 * 1024;
        let tail = read_tail(&[log], limit).unwrap();
        assert!(tail.cut);
        let len = tail.bytes.len() as u64;
        assert!(len <= limit && len > limit - 64, "{len} bytes");
        assert!(tail.bytes.starts_with(b"0123456789abcdef"));
    }
}
