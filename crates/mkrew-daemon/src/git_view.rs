//! The Git view's reads (SPEC §12B, ADR 0007): `git.status` and `git.diff` on top of a
//! [`GitBackend`], with the caches that keep a refresh cheap. Blocking; the server runs them in
//! `spawn_blocking`.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread::{self, ScopedJoinHandle};
use std::time::SystemTime;

use mkrew_protocol::{ChangedFile, FileDiff, GitDiffParams, GitMode, GitStatus};

use crate::git::{GitBackend, Repo, Side};

/// Bigger diffs are sent as a summary (counts only, SPEC §12B).
pub const MAX_DIFF_BYTES: usize = 1024 * 1024;

/// Cached diffs kept before the cache starts over (each ≤ about 1 MB of diff, parsed).
const MAX_CACHED_DIFFS: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum ViewError {
    #[error("the project folder is not inside a git repository")]
    NotARepo,
    #[error("no base to compare against (set one with git.set_base)")]
    NoMergeBase,
    #[error("{0:?} names no commit")]
    UnknownCommit(String),
    #[error(transparent)]
    Git(#[from] io::Error),
}

/// Per-folder state shared by all connections.
#[derive(Default)]
pub struct GitCache {
    repos: Mutex<HashMap<PathBuf, Repo>>,
    merge_bases: Mutex<HashMap<PathBuf, MergeBase>>,
    diffs: Mutex<HashMap<DiffKey, FileDiff>>,
}

/// `git merge-base head base`, valid until HEAD or the base moves.
#[derive(Debug, Clone)]
struct MergeBase {
    head: String,
    base: String,
    merge_base: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DiffKey {
    dir: PathBuf,
    path: String,
    target: Target,
    /// `None` for a commit's diff, which never changes.
    stamp: Option<Stamp>,
}

/// What a cached working-tree diff depends on besides its target.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Stamp {
    head: Option<String>,
    /// The file's mtime and size; `None` = missing.
    file: Option<(SystemTime, u64)>,
    /// The index's mtime (staging changes it).
    index: Option<SystemTime>,
}

/// Owned [`Side`], for the cache key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Target {
    Unstaged,
    Staged,
    Since(String),
    Commit(String),
}

impl Target {
    fn side(&self) -> Side<'_> {
        match self {
            Target::Unstaged => Side::Unstaged,
            Target::Staged => Side::Staged,
            Target::Since(commit) => Side::Since(commit),
            Target::Commit(commit) => Side::Commit(commit),
        }
    }
}

impl GitCache {
    fn repo(&self, git: &dyn GitBackend, dir: &Path) -> Result<Repo, ViewError> {
        if let Some(repo) = lock(&self.repos).get(dir) {
            return Ok(repo.clone());
        }
        let repo = git.repo(dir)?.ok_or(ViewError::NotARepo)?;
        lock(&self.repos).insert(dir.to_owned(), repo.clone());
        Ok(repo)
    }

    fn merge_base(
        &self,
        git: &dyn GitBackend,
        dir: &Path,
        head: &str,
        base: &str,
    ) -> io::Result<Option<String>> {
        if let Some(cached) = lock(&self.merge_bases).get(dir)
            && cached.head == head
            && cached.base == base
        {
            return Ok(cached.merge_base.clone());
        }
        let merge_base = git.merge_base(dir, head, base)?;
        let cached = MergeBase {
            head: head.to_owned(),
            base: base.to_owned(),
            merge_base: merge_base.clone(),
        };
        lock(&self.merge_bases).insert(dir.to_owned(), cached);
        Ok(merge_base)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Only caches: whatever a panicking holder left is still a valid cache.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `git.status` of the folder `dir` (paths relative to it). `configured` = `project.base_ref`.
/// The status, the base lookup and (working mode) the staged/unstaged line counts run in
/// parallel; base mode then adds the cached merge-base and the diff since it. Untracked files
/// are part of the working tree, so base mode lists them too.
pub fn status(
    git: &dyn GitBackend,
    cache: &GitCache,
    dir: &Path,
    configured: Option<&str>,
    mode: GitMode,
) -> Result<GitStatus, ViewError> {
    let repo = cache.repo(git, dir)?;
    let (status, base, counts) = thread::scope(|s| {
        let base = s.spawn(|| resolve_base(git, dir, configured));
        let counts = (mode == GitMode::Working).then(|| {
            (
                s.spawn(|| git.changes(dir, Side::Staged)),
                s.spawn(|| git.changes(dir, Side::Unstaged)),
            )
        });
        let status = git.status(dir, &repo);
        let counts = counts.map(|(staged, unstaged)| (joined(staged), joined(unstaged)));
        (status, joined(base), counts)
    });
    let mut status = match status {
        Ok(status) => status,
        Err(e) => {
            // The repo may be gone; look it up again next time.
            lock(&cache.repos).remove(dir);
            return Err(e.into());
        }
    };
    let base = base?;
    status.base_auto = configured.is_none();
    status.base_ref = match &base {
        Some((name, _)) => Some(name.clone()),
        None => configured.map(str::to_owned),
    };
    if let Some((staged, unstaged)) = counts {
        add_counts(&mut status.staged, &staged?);
        add_counts(&mut status.unstaged, &unstaged?);
    }
    if mode == GitMode::Base {
        if let (Some(head), Some((_, base))) = (&status.head, &base) {
            status.merge_base = cache.merge_base(git, dir, head, base)?;
        }
        if let Some(merge_base) = &status.merge_base {
            let mut files = git.changes(dir, Side::Since(merge_base))?;
            files.append(&mut status.untracked);
            status.against_base = files;
        }
        status.staged.clear();
        status.unstaged.clear();
        status.untracked.clear();
    }
    Ok(status)
}

/// `git.diff` of one file. `path` is already checked to stay inside `dir`. Cached by target,
/// HEAD, the file's mtime/size and the index's mtime.
pub fn diff(
    git: &dyn GitBackend,
    cache: &GitCache,
    dir: &Path,
    configured: Option<&str>,
    params: &GitDiffParams,
    path: &str,
) -> Result<FileDiff, ViewError> {
    let repo = cache.repo(git, dir)?;
    let (target, stamp) = match &params.commit {
        // Resolved first: a moving name (`HEAD~1`) must not hit a stale entry, and ranges or
        // trees are refused.
        Some(commit) => {
            let id = git
                .commit_of(dir, commit)?
                .ok_or_else(|| ViewError::UnknownCommit(commit.clone()))?;
            (Target::Commit(id), None)
        }
        None => {
            let head = git.head(dir)?;
            let target = match (params.mode, params.staged) {
                (GitMode::Working, false) => Target::Unstaged,
                (GitMode::Working, true) => Target::Staged,
                (GitMode::Base, _) => {
                    let head = head.as_deref().ok_or(ViewError::NoMergeBase)?;
                    let (_, base) =
                        resolve_base(git, dir, configured)?.ok_or(ViewError::NoMergeBase)?;
                    let merge_base = cache
                        .merge_base(git, dir, head, &base)?
                        .ok_or(ViewError::NoMergeBase)?;
                    Target::Since(merge_base)
                }
            };
            let stamp = Stamp {
                head,
                file: std::fs::symlink_metadata(dir.join(path))
                    .ok()
                    .and_then(|m| Some((m.modified().ok()?, m.len()))),
                index: modified(&repo.git_dir.join("index")),
            };
            (target, Some(stamp))
        }
    };
    let key = DiffKey {
        dir: dir.to_owned(),
        path: path.to_owned(),
        target,
        stamp,
    };
    if let Some(cached) = lock(&cache.diffs).get(&key) {
        return Ok(cached.clone());
    }
    let mut text = git.file_diff(dir, path, key.target.side())?;
    if text.is_empty()
        && matches!(key.target, Target::Unstaged | Target::Since(_))
        && dir.join(path).is_file()
        && !git.is_tracked(dir, path)?
    {
        text = git.untracked_diff(dir, path)?;
    }
    let diff = file_diff(&text, path);
    let mut diffs = lock(&cache.diffs);
    if diffs.len() >= MAX_CACHED_DIFFS {
        diffs.clear();
    }
    diffs.insert(key, diff.clone());
    Ok(diff)
}

/// The diff of one file from git's output; over [`MAX_DIFF_BYTES`] only its counts, taken
/// without building hunks.
fn file_diff(text: &str, path: &str) -> FileDiff {
    if text.len() > MAX_DIFF_BYTES {
        let mut diff = FileDiff {
            path: path.to_owned(),
            too_large: true,
            ..FileDiff::default()
        };
        // Header lines (`---`/`+++`) come before the first hunk.
        let body = text.find("\n@@").map_or("", |at| &text[at..]);
        for line in body.lines() {
            match line.as_bytes().first() {
                Some(b'+') => diff.added += 1,
                Some(b'-') => diff.removed += 1,
                _ => {}
            }
        }
        return diff;
    }
    mkrew_core::git::parse_diff(text)
        .into_iter()
        .next()
        .unwrap_or_else(|| FileDiff {
            path: path.to_owned(),
            ..FileDiff::default()
        })
}

/// The configured base (if it names a commit) or the auto-detected one: name and commit.
fn resolve_base(
    git: &dyn GitBackend,
    dir: &Path,
    configured: Option<&str>,
) -> io::Result<Option<(String, String)>> {
    match configured {
        Some(name) => Ok(git
            .commit_of(dir, name)?
            .map(|commit| (name.to_owned(), commit))),
        None => git.auto_base(dir),
    }
}

/// Copies the line counts of `counted` onto the same paths in `files`.
fn add_counts(files: &mut [ChangedFile], counted: &[ChangedFile]) {
    let counts: HashMap<&str, &ChangedFile> =
        counted.iter().map(|f| (f.path.as_str(), f)).collect();
    for file in files {
        if let Some(counted) = counts.get(file.path.as_str()) {
            file.added = counted.added;
            file.removed = counted.removed;
        }
    }
}

fn joined<T>(handle: ScopedJoinHandle<'_, io::Result<T>>) -> io::Result<T> {
    handle
        .join()
        .unwrap_or_else(|_| Err(io::Error::other("git thread panicked")))
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use mkrew_protocol::DiffLineKind;
    use ulid::Ulid;

    use super::*;
    use crate::git::GitCli;

    /// `git` in `dir` with a throwaway identity; returns trimmed stdout.
    fn git(dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?} failed");
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    /// A repo on `main` with `app/` (the project folder) and a file outside it.
    fn repo() -> (tempfile::TempDir, PathBuf) {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        git(root, &["init", "-q", "-b", "main"]);
        // Byte-exact diffs whatever the machine's global autocrlf says.
        git(root, &["config", "core.autocrlf", "false"]);
        let dir = root.join("app");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(root.join("outside.txt"), "a\n").unwrap();
        std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
        std::fs::write(dir.join("b.txt"), "b\n").unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "one"]);
        (repo, dir)
    }

    fn working(cache: &GitCache, dir: &Path) -> GitStatus {
        status(&GitCli, cache, dir, None, GitMode::Working).unwrap()
    }

    fn base(cache: &GitCache, dir: &Path, configured: Option<&str>) -> GitStatus {
        status(&GitCli, cache, dir, configured, GitMode::Base).unwrap()
    }

    fn summary(files: &[ChangedFile]) -> Vec<String> {
        files
            .iter()
            .map(|f| {
                let counts = match (f.added, f.removed) {
                    (Some(a), Some(r)) => format!(" +{a} -{r}"),
                    _ => String::new(),
                };
                format!("{:?} {}{counts}", f.kind, f.path)
            })
            .collect()
    }

    #[test]
    fn working_status_groups_counted_changes_of_the_folder_only() {
        let (repo, dir) = repo();
        let root = repo.path();
        std::fs::write(dir.join("a.txt"), "one\n2\nthree\n").unwrap();
        git(&dir, &["add", "a.txt"]);
        std::fs::write(dir.join("a.txt"), "one\n2\nthree\nfour\n").unwrap();
        std::fs::remove_file(dir.join("b.txt")).unwrap();
        std::fs::create_dir(dir.join("new")).unwrap();
        std::fs::write(dir.join("new").join("c d.txt"), "c\n").unwrap();
        std::fs::write(root.join("outside.txt"), "b\n").unwrap();
        std::fs::write(root.join("outside-new.txt"), "b\n").unwrap();

        let cache = GitCache::default();
        let status = working(&cache, &dir);
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.head, Some(git(root, &["rev-parse", "HEAD"])));
        assert_eq!((status.upstream, status.ahead, status.behind), (None, 0, 0));
        assert_eq!(status.base_ref.as_deref(), Some("main"));
        assert!(status.base_auto);
        assert_eq!(status.merge_base, None);
        assert_eq!(summary(&status.staged), ["Modified a.txt +2 -1"]);
        assert_eq!(
            summary(&status.unstaged),
            ["Modified a.txt +1 -0", "Deleted b.txt +0 -1"]
        );
        assert_eq!(summary(&status.untracked), ["Untracked new/c d.txt"]);
        assert!(status.against_base.is_empty());

        // Not a repo; a folder that becomes one is found on the next call.
        let plain = tempfile::tempdir().unwrap();
        let error = super::status(&GitCli, &cache, plain.path(), None, GitMode::Working);
        assert!(matches!(error, Err(ViewError::NotARepo)), "{error:?}");
        git(plain.path(), &["init", "-q", "-b", "main"]);
        let fresh = working(&cache, plain.path());
        assert_eq!((fresh.branch.as_deref(), fresh.head), (Some("main"), None));
        assert_eq!(fresh.base_ref, None);
    }

    #[test]
    fn base_mode_diffs_from_the_cached_merge_base() {
        let (repo, dir) = repo();
        let root = repo.path();
        let fork = git(root, &["rev-parse", "HEAD"]);
        git(root, &["checkout", "-q", "-b", "feature"]);
        std::fs::write(dir.join("b.txt"), "b\nfeature\n").unwrap();
        git(root, &["commit", "-q", "-am", "feature"]);
        git(root, &["checkout", "-q", "main"]);
        std::fs::write(dir.join("a.txt"), "main moved on\n").unwrap();
        git(root, &["commit", "-q", "-am", "main"]);
        git(root, &["checkout", "-q", "feature"]);
        std::fs::write(dir.join("new.txt"), "n\n").unwrap();

        let cache = GitCache::default();
        let status = base(&cache, &dir, None);
        assert_eq!(status.base_ref.as_deref(), Some("main"));
        assert_eq!(status.merge_base.as_deref(), Some(fork.as_str()));
        // Only this branch's changes (not main's a.txt), plus untracked files.
        assert_eq!(
            summary(&status.against_base),
            ["Modified b.txt +1 -0", "Untracked new.txt"]
        );
        assert!(status.staged.is_empty() && status.unstaged.is_empty());
        assert!(status.untracked.is_empty());

        // Cached until HEAD or the base moves: a planted answer is returned as is…
        let head = status.head.clone().unwrap();
        lock(&cache.merge_bases).get_mut(&dir).unwrap().merge_base = Some(head.clone());
        assert_eq!(base(&cache, &dir, None).merge_base, Some(head));
        // …and recomputed once HEAD moves.
        git(root, &["commit", "-q", "--allow-empty", "-m", "more"]);
        assert_eq!(base(&cache, &dir, None).merge_base, Some(fork.clone()));

        // `origin/HEAD` wins over `main`; a configured base wins over both.
        git(root, &["update-ref", "refs/remotes/origin/trunk", &fork]);
        git(
            root,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/trunk",
            ],
        );
        let remote = base(&cache, &dir, None);
        assert_eq!(remote.base_ref.as_deref(), Some("origin/trunk"));
        assert_eq!(remote.merge_base, Some(fork));
        let configured = base(&cache, &dir, Some("feature~1"));
        assert_eq!(configured.base_ref.as_deref(), Some("feature~1"));
        assert!(!configured.base_auto);
        assert_eq!(summary(&configured.against_base), ["Untracked new.txt"]);
        // A base that names nothing: reported, nothing to compare.
        let missing = base(&cache, &dir, Some("develop"));
        assert_eq!(missing.base_ref.as_deref(), Some("develop"));
        assert_eq!((missing.merge_base, missing.against_base), (None, vec![]));
    }

    fn params(mode: GitMode, staged: bool, commit: Option<&str>) -> GitDiffParams {
        GitDiffParams {
            project_id: Ulid::new(),
            path: String::new(),
            mode,
            staged,
            commit: commit.map(str::to_owned),
        }
    }

    fn lines(diff: &FileDiff) -> Vec<String> {
        let sign = |kind| match kind {
            DiffLineKind::Context => ' ',
            DiffLineKind::Add => '+',
            DiffLineKind::Del => '-',
        };
        diff.hunks
            .iter()
            .flat_map(|h| &h.lines)
            .map(|l| format!("{}{}", sign(l.kind), l.text))
            .collect()
    }

    #[test]
    fn diffs_of_each_side_are_cached_until_the_file_or_index_changes() {
        let (repo, dir) = repo();
        let root = repo.path();
        let first = git(root, &["rev-parse", "HEAD"]);
        std::fs::write(dir.join("a.txt"), "one\nTWO\n").unwrap();
        git(&dir, &["add", "a.txt"]);
        std::fs::write(dir.join("a.txt"), "one\nTWO\nthree\n").unwrap();
        std::fs::write(dir.join("new.txt"), "n1\nn2\n").unwrap();

        let cache = GitCache::default();
        let diff = |path: &str, p: &GitDiffParams| {
            super::diff(&GitCli, &cache, &dir, None, p, path).unwrap()
        };
        let unstaged = diff("a.txt", &params(GitMode::Working, false, None));
        assert_eq!(lines(&unstaged), [" one", " TWO", "+three"]);
        assert_eq!((unstaged.added, unstaged.removed), (1, 0));
        let staged = diff("a.txt", &params(GitMode::Working, true, None));
        assert_eq!(lines(&staged), [" one", "-two", "+TWO"]);
        let untracked = diff("new.txt", &params(GitMode::Working, false, None));
        assert_eq!(untracked.path, "new.txt");
        assert_eq!(lines(&untracked), ["+n1", "+n2"]);
        assert_eq!(lock(&cache.diffs).len(), 3);
        // Same request again: served from the cache.
        assert_eq!(diff("a.txt", &params(GitMode::Working, true, None)), staged);
        assert_eq!(lock(&cache.diffs).len(), 3);

        // Staging changes the index: both sides are recomputed.
        git(&dir, &["add", "a.txt"]);
        assert_eq!(
            diff("a.txt", &params(GitMode::Working, false, None)).hunks,
            []
        );
        let staged = diff("a.txt", &params(GitMode::Working, true, None));
        assert_eq!(lines(&staged), [" one", "-two", "+TWO", "+three"]);
        let against = diff("a.txt", &params(GitMode::Base, false, None));
        assert_eq!(against, staged);

        // A commit's own diff; an unchanged file has no hunks.
        git(root, &["commit", "-q", "-m", "two"]);
        let second = git(root, &["rev-parse", "HEAD"]);
        let commit = diff("a.txt", &params(GitMode::Working, false, Some(&second)));
        assert_eq!(lines(&commit), lines(&staged));
        assert_eq!(
            diff("b.txt", &params(GitMode::Working, false, Some(&second))),
            FileDiff {
                path: "b.txt".into(),
                ..FileDiff::default()
            }
        );
        let root_commit = diff("b.txt", &params(GitMode::Working, false, Some(&first)));
        assert_eq!(lines(&root_commit), ["+b"]);
        // Names are resolved before the cache: `HEAD` follows new commits.
        let by_name = diff("a.txt", &params(GitMode::Working, false, Some("HEAD")));
        assert_eq!(by_name, commit);
        std::fs::write(dir.join("a.txt"), "1\n").unwrap();
        git(root, &["commit", "-q", "-am", "three"]);
        let moved = diff("a.txt", &params(GitMode::Working, false, Some("HEAD")));
        assert_eq!(lines(&moved), ["-one", "-TWO", "-three", "+1"]);
        for bad in ["HEAD~1..HEAD", "nope"] {
            let error = super::diff(
                &GitCli,
                &cache,
                &dir,
                None,
                &params(GitMode::Working, false, Some(bad)),
                "a.txt",
            );
            assert!(
                matches!(error, Err(ViewError::UnknownCommit(_))),
                "{error:?}"
            );
        }
    }

    #[test]
    fn diffs_over_the_limit_keep_only_their_counts() {
        let line = "x".repeat(99);
        let header = "diff --git a/big b/big\n--- a/big\n+++ b/big\n";
        let small = file_diff(&format!("{header}@@ -0,0 +1,1 @@\n+{line}\n"), "big");
        assert_eq!(
            (small.added, small.too_large, small.hunks.len()),
            (1, false, 1)
        );
        let count = MAX_DIFF_BYTES / 100 + 1;
        let added = format!("+{line}\n").repeat(count);
        let big = file_diff(&format!("{header}@@ -0,0 +1,{count} @@\n{added}"), "big");
        assert!(big.too_large);
        let count = u32::try_from(count).unwrap();
        assert_eq!((big.added, big.removed, big.hunks.len()), (count, 0, 0));
        let removed = format!("-{line}\n").repeat(usize::try_from(count).unwrap());
        let both = file_diff(
            &format!("{header}@@ -1,{count} +1,{count} @@\n{removed}{added}"),
            "big",
        );
        assert_eq!(
            (both.added, both.removed, both.path.as_str()),
            (count, count, "big")
        );
        assert_eq!(file_diff("", "gone").path, "gone");
    }
}
