//! Pasted images (SPEC §20B.5): `attachment.save` writes `<dir>/<session_id>/<ulid>.<ext>`;
//! the hourly backup check ([`crate::backup`]) deletes the dirs of sessions that ended more
//! than [`KEEP_MS`] ago.

use std::io;
use std::path::{Path, PathBuf};

use ulid::Ulid;

/// Accepted file extensions, compared exactly: clients send them lowercase.
pub const EXTS: [&str; 4] = ["png", "jpg", "gif", "webp"];
/// Largest accepted file: 10 MiB.
pub const MAX_BYTES: usize = 10 * 1024 * 1024;
/// Attachments of a session are kept this long after it ended: 7 days.
pub const KEEP_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// Why `attachment.save` params are rejected, as a message for `invalid_request`.
pub fn check(ext: &str, len: usize) -> Result<(), String> {
    if !EXTS.contains(&ext) {
        return Err(format!(
            "ext must be one of {}, got {ext:?}",
            EXTS.join(", ")
        ));
    }
    if len == 0 {
        return Err("data is empty".into());
    }
    if len > MAX_BYTES {
        return Err(format!("data is {len} bytes, the limit is {MAX_BYTES}"));
    }
    Ok(())
}

/// Writes `data` to a new file in the session's dir and returns its path. `ext` must have
/// passed [`check`].
pub fn save(dir: &Path, session_id: Ulid, ext: &str, data: &[u8]) -> io::Result<PathBuf> {
    let dir = dir.join(session_id.to_string());
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.{ext}", Ulid::new()));
    std::fs::write(&path, data)?;
    Ok(path)
}

/// Deletes the dir of every session that ended more than [`KEEP_MS`] before `now_ms`;
/// `ended_at` gives a session's end (`None` = live or unknown, kept). Other entries are left
/// alone. A dir that cannot be deleted (e.g. a file still open on Windows) is logged and
/// retried at the next check; the others still go. Returns the deleted dirs.
pub fn cleanup(
    dir: &Path,
    now_ms: i64,
    mut ended_at: impl FnMut(Ulid) -> Option<i64>,
) -> io::Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut removed = Vec::new();
    for entry in entries.flatten() {
        let Some(id) = entry
            .file_name()
            .to_str()
            .and_then(|name| Ulid::from_string(name).ok())
        else {
            continue;
        };
        let expired = ended_at(id).is_some_and(|ended| now_ms - ended > KEEP_MS);
        if expired && entry.file_type().is_ok_and(|t| t.is_dir()) {
            let path = entry.path();
            match std::fs::remove_dir_all(&path) {
                Ok(()) => removed.push(path),
                Err(e) => tracing::warn!(path = %path.display(), "delete attachments: {e}"),
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_whitelisted_non_empty_files_up_to_10_mib_pass() {
        for ext in EXTS {
            assert_eq!(check(ext, 1), Ok(()));
        }
        assert_eq!(check("png", MAX_BYTES), Ok(()));
        assert!(check("png", MAX_BYTES + 1).is_err());
        assert!(check("png", 0).is_err());
        for ext in ["PNG", "jpeg", "exe", "", "png/../x", "../png"] {
            assert!(check(ext, 1).is_err(), "{ext}");
        }
    }

    #[test]
    fn saved_files_land_in_the_session_dir_under_new_names() {
        let dir = tempfile::tempdir().unwrap();
        let session = Ulid::new();
        let first = save(dir.path(), session, "png", b"one").unwrap();
        let second = save(dir.path(), session, "png", b"two").unwrap();
        assert_ne!(first, second);
        for (path, data) in [(&first, "one"), (&second, "two")] {
            assert_eq!(path.parent().unwrap(), dir.path().join(session.to_string()));
            assert_eq!(path.extension().unwrap(), "png");
            let stem = path.file_stem().unwrap().to_str().unwrap();
            assert!(Ulid::from_string(stem).is_ok(), "{stem}");
            assert_eq!(std::fs::read_to_string(path).unwrap(), data);
        }
    }

    #[test]
    fn cleanup_deletes_only_sessions_ended_more_than_7_days_ago() {
        let dir = tempfile::tempdir().unwrap();
        let now = 100 * KEEP_MS;
        let (old, recent, live, unknown) = (Ulid::new(), Ulid::new(), Ulid::new(), Ulid::new());
        for id in [old, recent, live, unknown] {
            save(dir.path(), id, "png", b"x").unwrap();
        }
        std::fs::create_dir(dir.path().join("notes")).unwrap();
        std::fs::write(dir.path().join(Ulid::new().to_string()), "a file").unwrap();

        let removed = cleanup(dir.path(), now, |id| {
            if id == old {
                Some(now - KEEP_MS - 1)
            } else if id == recent {
                Some(now - KEEP_MS)
            } else {
                None
            }
        })
        .unwrap();

        assert_eq!(removed, [dir.path().join(old.to_string())]);
        assert!(!dir.path().join(old.to_string()).exists());
        for id in [recent, live, unknown] {
            assert!(dir.path().join(id.to_string()).is_dir(), "{id}");
        }
        assert!(dir.path().join("notes").is_dir());
    }

    #[test]
    fn cleanup_without_the_dir_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("attachments");
        assert!(cleanup(&missing, 0, |_| Some(0)).unwrap().is_empty());
    }
}
