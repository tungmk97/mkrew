//! "Undo task" (ADR 0005): restore the project folder to the task's checkpoint and delete the
//! untracked files created since. Blocking; the server runs it in `spawn_blocking`.

use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::path::Path;
use std::time::UNIX_EPOCH;

use mkrew_protocol::{UNDO_DIFF_LIMIT, UndoPreview};
use ulid::Ulid;

use crate::git::{Changed, GitBackend};
use crate::snapshot::Snapshots;

/// What [`undo`] would discard. `since` = Unix ms the checkpoint was taken.
pub fn preview(
    git: &dyn GitBackend,
    dir: &Path,
    checkpoint: &str,
    since: i64,
) -> io::Result<UndoPreview> {
    let diff = git.diff(dir, checkpoint, None)?;
    let files = git.changed_files(dir, checkpoint, None)?;
    let fingerprint = fingerprint(&files, &diff);
    let (diff, diff_truncated) = cut(diff);
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

/// What [`restore_turn`] would discard (SPEC §20C.2): everything that differs between the
/// turn's start snapshot `start` and a snapshot of the folder now. Files absent from `start`
/// are all `created` (tracked or not: snapshots hold untracked files too); `added` is empty.
pub fn preview_turn(
    git: &dyn GitBackend,
    snapshots: &Snapshots,
    project: Ulid,
    dir: &Path,
    start: &str,
) -> io::Result<UndoPreview> {
    let (files, diff) = since_turn(git, snapshots, project, dir, start)?;
    let fingerprint = fingerprint(&files, &diff);
    let (diff, diff_truncated) = cut(diff);
    let (created, changed): (Vec<_>, Vec<_>) = files.into_iter().partition(|c| c.added);
    Ok(UndoPreview {
        checkpoint: start.to_owned(),
        fingerprint,
        changed: changed.into_iter().map(|c| c.path).collect(),
        added: Vec::new(),
        created: created.into_iter().map(|c| c.path).collect(),
        diff,
        diff_truncated,
    })
}

/// Writes the files changed or deleted since `start` back from it, through a temp index
/// (the real index and HEAD are untouched), if the changes still match the previewed
/// `fingerprint`; then deletes the files created since that are also in `confirmed`.
pub fn restore_turn(
    git: &dyn GitBackend,
    snapshots: &Snapshots,
    project: Ulid,
    dir: &Path,
    start: &str,
    previewed: &str,
    confirmed: &[String],
) -> io::Result<Undone> {
    let (files, diff) = since_turn(git, snapshots, project, dir, start)?;
    if fingerprint(&files, &diff) != previewed {
        return Ok(Undone::Stale);
    }
    let (created, changed): (Vec<_>, Vec<_>) = files.into_iter().partition(|c| c.added);
    let changed: Vec<String> = changed.into_iter().map(|c| c.path).collect();
    snapshots.checkout(git, project, dir, start, &changed)?;
    let confirmed: HashSet<&str> = confirmed.iter().map(String::as_str).collect();
    for path in created
        .iter()
        .filter(|c| confirmed.contains(c.path.as_str()))
    {
        // `git clean` would keep files the agent staged; these paths come from git, inside
        // `dir`.
        match std::fs::remove_file(dir.join(&path.path)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => {
                return Err(io::Error::other(format!(
                    "files restored; deleting {} failed: {e}",
                    path.path
                )));
            }
            _ => {}
        }
    }
    Ok(Undone::Done)
}

/// The files and diff from `start` to a snapshot of the folder now.
fn since_turn(
    git: &dyn GitBackend,
    snapshots: &Snapshots,
    project: Ulid,
    dir: &Path,
    start: &str,
) -> io::Result<(Vec<Changed>, String)> {
    let now = snapshots.tree(git, project, dir)?;
    let files = git.changed_files(dir, start, Some(&now))?;
    let diff = git.diff(dir, start, Some(&now))?;
    Ok((files, diff))
}

/// `diff` cut at [`UNDO_DIFF_LIMIT`] on a char boundary; whether it was cut.
fn cut(mut diff: String) -> (String, bool) {
    let truncated = diff.len() > UNDO_DIFF_LIMIT;
    if truncated {
        let mut end = UNDO_DIFF_LIMIT;
        while !diff.is_char_boundary(end) {
            end -= 1;
        }
        diff.truncate(end);
    }
    (diff, truncated)
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
    let files = git.changed_files(dir, checkpoint, None)?;
    if fingerprint(&files, &git.diff(dir, checkpoint, None)?) != previewed {
        return Ok(Undone::Stale);
    }
    git.restore(dir, checkpoint)?;
    let confirmed: HashSet<&str> = confirmed.iter().map(String::as_str).collect();
    let created: Vec<String> = created_since(dir, git.untracked_files(dir)?, since)
        .into_iter()
        .filter(|path| confirmed.contains(path.as_str()))
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
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    use super::*;
    use crate::git::{Branch, GitCli, Repo, Side, Upstream};
    use mkrew_protocol::{ChangedFile, Commit, GitStatus};

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
        fn changed_files(&self, _: &Path, _: &str, _: Option<&str>) -> io::Result<Vec<Changed>> {
            let changed = |path: &str, added| Changed {
                path: path.into(),
                added,
            };
            Ok(vec![changed("a.txt", false), changed("b.txt", true)])
        }
        fn diff(&self, _: &Path, _: &str, _: Option<&str>) -> io::Result<String> {
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
        fn tree_of(&self, _: &Path, _: &str) -> io::Result<Option<String>> {
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
        fn log(&self, _: &Path, _: u32, _: Option<&str>) -> io::Result<Vec<Commit>> {
            unreachable!()
        }
        fn commit_info(&self, _: &Path, _: &str) -> io::Result<Commit> {
            unreachable!()
        }
        fn current_branch(&self, _: &Path) -> io::Result<Option<Branch>> {
            unreachable!()
        }
        fn fetch(&self, _: &Path) -> io::Result<()> {
            unreachable!()
        }
        fn push(&self, _: &Path, _: &str, _: &Upstream) -> io::Result<()> {
            unreachable!()
        }
        fn index_path(&self, _: &Path) -> io::Result<std::path::PathBuf> {
            unreachable!()
        }
        fn snapshot(&self, _: &Path, _: &Path) -> io::Result<String> {
            unreachable!()
        }
        fn update_ref(&self, _: &Path, _: &str, _: &str) -> io::Result<()> {
            unreachable!()
        }
        fn delete_refs(&self, _: &Path, _: &str) -> io::Result<()> {
            unreachable!()
        }
        fn checkout_tree(&self, _: &Path, _: &Path, _: &str, _: &[String]) -> io::Result<()> {
            unreachable!()
        }
        fn toplevel(&self, _: &Path) -> io::Result<PathBuf> {
            unreachable!()
        }
        fn worktree_add(&self, _: &Path, _: &Path, _: &str) -> io::Result<()> {
            unreachable!()
        }
        fn ignored(&self, _: &Path, _: &[String]) -> io::Result<Vec<String>> {
            unreachable!()
        }
        fn uncommitted(&self, _: &Path, _: bool) -> io::Result<Vec<String>> {
            unreachable!()
        }
        fn merge(&self, _: &Path, _: &str) -> io::Result<crate::git::Merged> {
            unreachable!()
        }
        fn worktree_remove(&self, _: &Path, _: &Path) -> io::Result<()> {
            unreachable!()
        }
        fn worktree_prune(&self, _: &Path) -> io::Result<()> {
            unreachable!()
        }
        fn delete_branch(&self, _: &Path, _: &str) -> io::Result<bool> {
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

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?} failed");
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    /// SPEC §20C.2: the project is a subfolder of the repo; restore puts back edited, deleted
    /// and created files (tracked or not) of that folder only, leaves HEAD and the real index
    /// alone, keeps created files the preview did not list, and refuses a stale preview.
    #[test]
    fn restoring_a_turn_puts_the_folder_back_without_touching_the_index() {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        let dir = root.join("app");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        git(root, &["init", "-q", "-b", "main"]);
        git(root, &["config", "core.autocrlf", "false"]);
        std::fs::write(root.join("outside.txt"), "a\n").unwrap();
        std::fs::write(dir.join("edited.txt"), "one\n").unwrap();
        std::fs::write(dir.join("sub").join("deleted.txt"), "keep\n").unwrap();
        std::fs::write(dir.join(".gitignore"), "*.log\n").unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "one"]);
        // Untracked before the turn: part of the snapshot, so it comes back.
        std::fs::write(dir.join("notes.txt"), "draft\n").unwrap();
        let (snapshots, project) = (Snapshots::default(), Ulid::new());
        let start = snapshots.tree(&GitCli, project, &dir).unwrap();

        std::fs::write(root.join("outside.txt"), "b\n").unwrap();
        std::fs::write(dir.join("edited.txt"), "two\n").unwrap();
        std::fs::remove_file(dir.join("sub").join("deleted.txt")).unwrap();
        std::fs::write(dir.join("notes.txt"), "rewritten\n").unwrap();
        std::fs::write(dir.join("staged.txt"), "new\n").unwrap();
        git(&dir, &["add", "staged.txt"]);
        std::fs::write(dir.join("created.txt"), "new\n").unwrap();
        std::fs::write(dir.join("build.log"), "ignored\n").unwrap();
        let head = git(root, &["rev-parse", "HEAD"]);
        let index = std::fs::read(root.join(".git").join("index")).unwrap();

        let shown = preview_turn(&GitCli, &snapshots, project, &dir, &start).unwrap();
        assert_eq!(shown.checkpoint, start);
        assert_eq!(
            shown.changed,
            ["edited.txt", "notes.txt", "sub/deleted.txt"]
        );
        assert!(shown.added.is_empty());
        assert_eq!(shown.created, ["created.txt", "staged.txt"]);
        assert!(shown.diff.contains("+two") && !shown.diff.contains("outside"));

        std::fs::write(dir.join("late.txt"), "after the preview\n").unwrap();
        assert_eq!(
            restore_turn(&GitCli, &snapshots, project, &dir, &start, "0", &[]).unwrap(),
            Undone::Stale
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("edited.txt")).unwrap(),
            "two\n"
        );
        // The late file changes the fingerprint too: preview again, keep it unconfirmed.
        let shown = preview_turn(&GitCli, &snapshots, project, &dir, &start).unwrap();
        let confirmed = ["created.txt".to_owned(), "staged.txt".to_owned()];
        let done = restore_turn(
            &GitCli,
            &snapshots,
            project,
            &dir,
            &start,
            &shown.fingerprint,
            &confirmed,
        );
        assert_eq!(done.unwrap(), Undone::Done);

        let read = |path: &str| std::fs::read_to_string(dir.join(path)).unwrap();
        assert_eq!(read("edited.txt"), "one\n");
        assert_eq!(read("sub/deleted.txt"), "keep\n");
        assert_eq!(read("notes.txt"), "draft\n");
        assert!(!dir.join("staged.txt").exists());
        assert!(!dir.join("created.txt").exists());
        assert!(dir.join("late.txt").exists());
        assert!(dir.join("build.log").exists());
        assert_eq!(
            std::fs::read_to_string(root.join("outside.txt")).unwrap(),
            "b\n"
        );
        assert_eq!(git(root, &["rev-parse", "HEAD"]), head);
        assert_eq!(
            std::fs::read(root.join(".git").join("index")).unwrap(),
            index
        );
        let leftovers: Vec<_> = std::fs::read_dir(root.join(".git"))
            .unwrap()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|name| name.contains("mkrew"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
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
