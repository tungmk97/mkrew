//! Raw PTY output on disk (SPEC §8): `<sessions>/<id>.log`, rotated to `<id>.log.1` at 5 MB,
//! so a session keeps at most ~10 MB of history.

use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

pub const ROTATE_AT_BYTES: u64 = 5 * 1024 * 1024;

/// The log file of session `id` in `sessions_dir`.
pub fn path(sessions_dir: &Path, id: impl std::fmt::Display) -> PathBuf {
    sessions_dir.join(format!("{id}.log"))
}

/// The existing files of the log at `path`, oldest first: the rotated `.log.1`, then `.log`.
pub fn files(path: &Path) -> Vec<PathBuf> {
    [rotated(path), path.to_path_buf()]
        .into_iter()
        .filter(|p| p.is_file())
        .collect()
}

fn rotated(path: &Path) -> PathBuf {
    let mut old = path.to_path_buf().into_os_string();
    old.push(".1");
    old.into()
}

pub struct SessionLog {
    path: PathBuf,
    file: BufWriter<File>,
    written: u64,
    limit: u64,
}

impl SessionLog {
    pub fn create(path: &Path) -> io::Result<Self> {
        Self::with_limit(path, ROTATE_AT_BYTES)
    }

    fn with_limit(path: &Path, limit: u64) -> io::Result<Self> {
        let (file, written) = open(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            file,
            written,
            limit,
        })
    }

    /// Appends `bytes` (buffered); rotates first if they would cross the limit.
    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.written > 0 && self.written + bytes.len() as u64 > self.limit {
            self.rotate()?;
        }
        self.file.write_all(bytes)?;
        self.written += bytes.len() as u64;
        Ok(())
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file.flush()?;
        // std opens files with FILE_SHARE_DELETE on Windows, so the open file can be renamed.
        std::fs::rename(&self.path, rotated(&self.path))?;
        self.file = open(&self.path)?.0;
        self.written = 0;
        Ok(())
    }
}

fn open(path: &Path) -> io::Result<(BufWriter<File>, u64)> {
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    let len = file.metadata()?.len();
    Ok((BufWriter::new(file), len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_at_the_limit_keeping_one_old_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.log");
        let mut log = SessionLog::with_limit(&path, 10).unwrap();
        log.write(b"aaaaaa").unwrap();
        log.write(b"bbbb").unwrap(); // exactly at the limit: no rotation
        log.write(b"cc").unwrap();
        log.write(b"dddddddddddd").unwrap();
        log.flush().unwrap();
        let old = dir.path().join("s.log.1");
        assert_eq!(std::fs::read(&old).unwrap(), b"cc");
        assert_eq!(std::fs::read(&path).unwrap(), b"dddddddddddd");
        log.write(b"e").unwrap();
        log.flush().unwrap();
        assert_eq!(std::fs::read(&old).unwrap(), b"dddddddddddd");
        assert_eq!(std::fs::read(&path).unwrap(), b"e");
    }

    #[test]
    fn files_lists_existing_parts_oldest_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = super::path(dir.path(), "s");
        assert!(files(&path).is_empty());
        let mut log = SessionLog::with_limit(&path, 4).unwrap();
        log.write(b"aaaa").unwrap();
        log.flush().unwrap();
        assert_eq!(files(&path), std::slice::from_ref(&path));
        log.write(b"b").unwrap();
        log.flush().unwrap();
        assert_eq!(files(&path), [dir.path().join("s.log.1"), path]);
    }
}
