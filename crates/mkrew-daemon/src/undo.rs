//! "Undo task" (ADR 0005): restore the project folder to the task's checkpoint and delete the
//! untracked files created since. Blocking; the server runs it in `spawn_blocking`.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::path::Path;
use std::time::UNIX_EPOCH;

use mkrew_protocol::{UNDO_DIFF_LIMIT, UndoPreview};

use crate::git::{Changed, GitBackend};

/// What [`undo`] would discard. `since` = Unix ms the checkpoint was taken.
pub fn preview(
    git: &dyn GitBackend,
    dir: &Path,
    checkpoint: &str,
    since: i64,
) -> io::Result<UndoPreview> {
    let mut diff = git.diff(dir, checkpoint)?;
    let files = git.changed_files(dir, checkpoint)?;
    let fingerprint = fingerprint(&files, &diff);
    let diff_truncated = diff.len() > UNDO_DIFF_LIMIT;
    if diff_truncated {
        let mut end = UNDO_DIFF_LIMIT;
        while !diff.is_char_boundary(end) {
            end -= 1;
        }
        diff.truncate(end);
    }
    let (added, changed): (Vec<_>, Vec<_>) = files.into_iter().partition(|c| c.added);
    Ok(UndoPreview {
        checkpoint: checkpoint.to_owned(),
        fingerprint,
        changed: changed.into_iter().map(|c| c.path).collect(),
        added: added.into_iter().map(|c| c.path).collect(),
        created: created_since(dir, git.untracked_files(dir)?, since),
        diff,
        diff_truncated,
    })
}

/// What [`undo`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum Undone {
    Done,
    /// The tracked changes no longer match the previewed fingerprint; nothing was touched.
    Stale,
}

/// Restores the tracked files to `checkpoint` if they still match the previewed
/// `fingerprint`, then deletes the untracked files created since that are also in `confirmed`
/// (the preview's `created`): a file that appeared after the preview was shown is kept.
pub fn undo(
    git: &dyn GitBackend,
    dir: &Path,
    checkpoint: &str,
    since: i64,
    previewed: &str,
    confirmed: &[String],
) -> io::Result<Undone> {
    let files = git.changed_files(dir, checkpoint)?;
    if fingerprint(&files, &git.diff(dir, checkpoint)?) != previewed {
        return Ok(Undone::Stale);
    }
    git.restore(dir, checkpoint)?;
    let created: Vec<String> = created_since(dir, git.untracked_files(dir)?, since)
        .into_iter()
        .filter(|path| confirmed.contains(path))
        .collect();
    if created.is_empty() {
        return Ok(Undone::Done);
    }
    git.clean(dir, &created).map_err(|e| {
        io::Error::other(format!(
            "files restored; deleting created files failed: {e}"
        ))
    })?;
    Ok(Undone::Done)
}

/// Hash of the tracked changes. `DefaultHasher` uses fixed keys, so it is stable for one
/// daemon build; a mismatch after an upgrade only refuses the undo.
fn fingerprint(files: &[Changed], diff: &str) -> String {
    let mut hasher = DefaultHasher::new();
    for file in files {
        (&file.path, file.added).hash(&mut hasher);
    }
    diff.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// A full commit id as stored by `task.run`; anything else is never handed to git, so a
/// stored value can not turn into an option.
pub fn is_commit_id(checkpoint: &str) -> bool {
    matches!(checkpoint.len(), 40 | 64) && checkpoint.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The `paths` (relative to `dir`) last modified at or after `since` (Unix ms). Files that
/// existed before the run but were edited by it count as created: git has no record of them.
fn created_since(dir: &Path, paths: Vec<String>, since: i64) -> Vec<String> {
    paths
        .into_iter()
        .filter(|path| modified_ms(&dir.join(path)).is_some_and(|ms| ms >= since))
        .collect()
}

fn modified_ms(path: &Path) -> Option<i64> {
    // The link itself: `git clean` deletes links, not their targets.
    let modified = std::fs::symlink_metadata(path).ok()?.modified().ok()?;
    let ms = modified.duration_since(UNIX_EPOCH).ok()?.as_millis();
    i64::try_from(ms).ok()
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::*;
    use crate::git::{Repo, Side};
    use mkrew_protocol::{ChangedFile, GitStatus};

    fn now_ms() -> i64 {
        i64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap()
    }

    /// A diff of `.0` bytes ending in a multi-byte char; one changed and one added file.
    struct BigDiff(usize);

    impl GitBackend for BigDiff {
        fn head(&self, _: &Path) -> io::Result<Option<String>> {
            Ok(None)
        }
        fn changed_files(&self, _: &Path, _: &str) -> io::Result<Vec<Changed>> {
            let changed = |path: &str, added| Changed {
                path: path.into(),
                added,
            };
            Ok(vec![changed("a.txt", false), changed("b.txt", true)])
        }
        fn diff(&self, _: &Path, _: &str) -> io::Result<String> {
            Ok(format!("{}é", "x".repeat(self.0 - 2)))
        }
        fn untracked_files(&self, _: &Path) -> io::Result<Vec<String>> {
            Ok(Vec::new())
        }
        fn restore(&self, _: &Path, _: &str) -> io::Result<()> {
            Ok(())
        }
        fn clean(&self, _: &Path, _: &[String]) -> io::Result<()> {
            Ok(())
        }
        fn repo(&self, _: &Path) -> io::Result<Option<Repo>> {
            unreachable!()
        }
        fn status(&self, _: &Path, _: &Repo) -> io::Result<GitStatus> {
            unreachable!()
        }
        fn changes(&self, _: &Path, _: Side<'_>) -> io::Result<Vec<ChangedFile>> {
            unreachable!()
        }
        fn auto_base(&self, _: &Path) -> io::Result<Option<(String, String)>> {
            unreachable!()
        }
        fn commit_of(&self, _: &Path, _: &str) -> io::Result<Option<String>> {
            unreachable!()
        }
        fn merge_base(&self, _: &Path, _: &str, _: &str) -> io::Result<Option<String>> {
            unreachable!()
        }
        fn file_diff(&self, _: &Path, _: &str, _: Side<'_>) -> io::Result<String> {
            unreachable!()
        }
        fn untracked_diff(&self, _: &Path, _: &str) -> io::Result<String> {
            unreachable!()
        }
        fn is_tracked(&self, _: &Path, _: &str) -> io::Result<bool> {
            unreachable!()
        }
        fn stage(&self, _: &Path, _: &[String]) -> io::Result<()> {
            unreachable!()
        }
        fn unstage(&self, _: &Path, _: &[String]) -> io::Result<()> {
            unreachable!()
        }
        fn apply_cached(&self, _: &Path, _: &Repo, _: &str, _: bool) -> io::Result<()> {
            unreachable!()
        }
        fn discard(&self, _: &Path, _: &[String]) -> io::Result<()> {
            unreachable!()
        }
        fn commit(&self, _: &Path, _: &str) -> io::Result<String> {
            unreachable!()
        }
    }

    #[test]
    fn preview_cuts_big_diffs_on_a_char_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let fits = preview(&BigDiff(UNDO_DIFF_LIMIT), dir.path(), "c", 0).unwrap();
        assert!(!fits.diff_truncated);
        assert_eq!(fits.diff.len(), UNDO_DIFF_LIMIT);
        assert_eq!(fits.changed, ["a.txt"]);
        assert_eq!(fits.added, ["b.txt"]);
        let cut = preview(&BigDiff(UNDO_DIFF_LIMIT + 1), dir.path(), "c", 0).unwrap();
        assert!(cut.diff_truncated);
        assert_eq!(cut.diff.len(), UNDO_DIFF_LIMIT - 1);
    }

    #[test]
    fn only_files_modified_since_count_as_created() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old.txt");
        std::fs::write(&old, "").unwrap();
        let file = std::fs::File::options().write(true).open(&old).unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(3600))
            .unwrap();
        drop(file);
        let since = now_ms() - 60_000;
        std::fs::write(dir.path().join("new.txt"), "").unwrap();
        let paths = vec!["old.txt".into(), "new.txt".into(), "gone.txt".into()];
        assert_eq!(created_since(dir.path(), paths, since), ["new.txt"]);
    }

    #[test]
    fn commit_ids_are_full_hex() {
        assert!(is_commit_id(&"a".repeat(40)));
        assert!(is_commit_id(&"0F".repeat(32)));
        assert!(!is_commit_id("HEAD"));
        assert!(!is_commit_id(&format!("--{}", "a".repeat(38))));
        assert!(!is_commit_id(&"g".repeat(40)));
    }
}
