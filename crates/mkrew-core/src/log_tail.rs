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

/// Reads the last `limit` bytes of `files` as one stream. A cut stream starts after its first
/// line break, so the replay does not begin inside an escape sequence. A file that is gone
/// (rotated away meanwhile) is skipped.
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
    if cut && let Some(newline) = bytes.iter().position(|&byte| byte == b'\n') {
        bytes.drain(..=newline);
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
}
