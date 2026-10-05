//! Single-instance lock file in the data dir (SPEC §3): exactly one daemon per data dir.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("another mkrew-daemon already holds {0}")]
    AlreadyRunning(String),
    #[error("cannot lock {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
}

/// Held for the daemon's lifetime; the OS releases the lock when the process exits,
/// even after a crash.
#[derive(Debug)]
pub struct InstanceLock {
    _file: File,
}

impl InstanceLock {
    pub fn acquire(path: &Path) -> Result<Self, LockError> {
        let io_err = |source| LockError::Io {
            path: path.display().to_string(),
            source,
        };
        // No truncate: the file may belong to a running daemon until we hold the lock.
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(io_err)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(LockError::AlreadyRunning(path.display().to_string()));
            }
            Err(TryLockError::Error(e)) => return Err(io_err(e)),
        }
        // Informational only (other processes cannot read it on Windows while locked).
        file.set_len(0).map_err(io_err)?;
        write!(file, "{}", std::process::id()).map_err(io_err)?;
        Ok(Self { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_lock_fails_until_first_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mkrew.lock");
        let first = InstanceLock::acquire(&path).unwrap();
        let err = InstanceLock::acquire(&path).unwrap_err();
        assert!(matches!(err, LockError::AlreadyRunning(_)), "{err}");
        drop(first);
        InstanceLock::acquire(&path).unwrap();
    }
}
