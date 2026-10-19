//! The Git view's reads (SPEC §12B, ADR 0007): `git.status`, `git.diff`, `git.log` and
//! `git.show` on top of a [`GitBackend`], with the caches that keep a refresh cheap; plus the
//! checks before `git.push`. Blocking; the server runs them in
//! `spawn_blocking`.

use std::cell::OnceCell;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread::{self, ScopedJoinHandle};
use std::time::SystemTime;

use mkrew_core::viewed::{Mark, Resolved, resolve, text_hash};
use mkrew_protocol::{
    ChangedFile, Commit, CommitDetail, FileDiff, GitDiffParams, GitMode, GitStatus,
};

use crate::git::{GitBackend, Repo, Side};

/// Bigger diffs are sent as a summary (counts only, SPEC §12B).
pub const MAX_DIFF_BYTES: usize = 1024 * 1024;

/// Cached diffs kept before the cache starts over (each ≤ about 1 MB of diff, parsed).
const MAX_CACHED_DIFFS: usize = 64;

/// Cached diff hashes kept before the cache starts over (a hash is a few bytes).
const MAX_CACHED_HASHES: usize = 4096;

#[derive(Debug, thiserror::Error)]
pub enum ViewError {
    #[error("the project folder is not inside a git repository")]
    NotARepo,
    #[error("no base to compare against (set one with git.set_base)")]
    NoMergeBase,
    #[error("{0:?} names no commit")]
    UnknownCommit(String),
    #[error("HEAD is detached: there is no branch to push")]
    NoBranch,
    #[error("branch {0:?} has no upstream to push to")]
    NoUpstream(String),
    #[error(transparent)]
    Git(#[from] io::Error),
}

/// Per-folder state shared by all connections.
#[derive(Default)]
pub struct GitCache {
    repos: Mutex<HashMap<PathBuf, Repo>>,
    merge_bases: Mutex<HashMap<PathBuf, MergeBase>>,
    diffs: Mutex<HashMap<DiffKey, FileDiff>>,
    /// The hash of each diff's text (`git.viewed`, ADR 0065), keyed like `diffs`; `None` = an
    /// empty diff. Separate from `diffs` so checking many marks never evicts the diff on
    /// screen.
    hashes: Mutex<HashMap<DiffKey, Option<String>>>,
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
    /// A tree (a turn snapshot) → the working state.
    Tree(String),
}

impl Target {
    /// `now`: a snapshot of the working state, used by [`Target::Tree`] only.
    fn side<'a>(&'a self, now: &'a str) -> Side<'a> {
        match self {
            Target::Unstaged => Side::Unstaged,
            Target::Staged => Side::Staged,
            Target::Since(commit) => Side::Since(commit),
            Target::Commit(commit) => Side::Commit(commit),
            Target::Tree(tree) => Side::Between(tree, now),
        }
    }
}

impl GitCache {
    pub(crate) fn repo(&self, git: &dyn GitBackend, dir: &Path) -> Result<Repo, ViewError> {
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
/// are part of the working tree, so base mode lists them too. `since` (base mode) replaces the
/// merge-base with that commit, or with that tree (a turn snapshot): the tree is then compared
/// with `snapshot()` of the working state, which holds the untracked files already (one
/// snapshot per call: see HUMAN_CHECKS T11.5 for its cost).
pub fn status(
    git: &dyn GitBackend,
    cache: &GitCache,
    dir: &Path,
    configured: Option<&str>,
    mode: GitMode,
    since: Option<&str>,
    snapshot: &dyn Fn() -> io::Result<String>,
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
        match since
            .map(|since| since_target(git, dir, since))
            .transpose()?
        {
            Some(Target::Tree(tree)) => {
                let now = snapshot()?;
                status.against_base = git.changes(dir, Side::Between(&tree, &now))?;
                status.merge_base = Some(tree);
            }
            since => {
                if let Some(Target::Since(commit)) = since {
                    status.merge_base = Some(commit);
                } else if let (Some(head), Some((_, base))) = (&status.head, &base) {
                    status.merge_base = cache.merge_base(git, dir, head, base)?;
                }
                if let Some(merge_base) = &status.merge_base {
                    let mut files = git.changes(dir, Side::Since(merge_base))?;
                    files.append(&mut status.untracked);
                    status.against_base = files;
                }
            }
        }
        status.staged.clear();
        status.unstaged.clear();
        status.untracked.clear();
    }
    Ok(status)
}

/// `git.diff` of one file. `path` is already checked to stay inside `dir`. Cached by target,
/// HEAD, the file's mtime/size and the index's mtime. A `since` tree is compared with
/// `snapshot()` of the working state (taken on a cache miss only).
pub fn diff(
    git: &dyn GitBackend,
    cache: &GitCache,
    dir: &Path,
    configured: Option<&str>,
    params: &GitDiffParams,
    path: &str,
    snapshot: &dyn Fn() -> io::Result<String>,
) -> Result<FileDiff, ViewError> {
    let key = diff_key(git, cache, dir, configured, params, path)?;
    if let Some(cached) = lock(&cache.diffs).get(&key) {
        return Ok(cached.clone());
    }
    let diff = file_diff(&diff_text(git, dir, &key, snapshot)?, path);
    let mut diffs = lock(&cache.diffs);
    if diffs.len() >= MAX_CACHED_DIFFS {
        diffs.clear();
    }
    diffs.insert(key, diff.clone());
    Ok(diff)
}

/// The hash of the text `git.diff` of `params` would show (ADR 0065): what a viewed mark
/// stores. `None` = the file has no diff. Cached like [`diff`].
pub fn diff_hash(
    git: &dyn GitBackend,
    cache: &GitCache,
    dir: &Path,
    configured: Option<&str>,
    params: &GitDiffParams,
    path: &str,
    snapshot: &dyn Fn() -> io::Result<String>,
) -> Result<Option<String>, ViewError> {
    let key = diff_key(git, cache, dir, configured, params, path)?;
    cached_hash(git, cache, dir, key, snapshot)
}

/// `git.viewed`: which of `marks` (one repository's stored marks of `list` = mode and `since`,
/// ADR 0065) still stand, and which are stale. HEAD and the targets are resolved once, and each
/// file's hash comes from the cache unless the file, the index or HEAD changed; Working
/// changes read the staged side only for a mark the unstaged side did not explain. A `since`
/// tree's working snapshot is taken at most once per call.
pub fn viewed(
    git: &dyn GitBackend,
    cache: &GitCache,
    dir: &Path,
    configured: Option<&str>,
    list: (GitMode, Option<&str>),
    marks: Vec<Mark>,
    snapshot: &dyn Fn() -> io::Result<String>,
) -> Result<Resolved, ViewError> {
    let repo = cache.repo(git, dir)?;
    let head = git.head(dir)?;
    let (mode, since) = list;
    let unstaged = target_of(
        git,
        cache,
        dir,
        configured,
        (mode, false, since),
        head.as_deref(),
    )?;
    let staged = (mode == GitMode::Working)
        .then(|| {
            target_of(
                git,
                cache,
                dir,
                configured,
                (mode, true, since),
                head.as_deref(),
            )
        })
        .transpose()?;
    let sides: &[bool] = if staged.is_some() {
        &[false, true]
    } else {
        &[false]
    };
    let memo = OnceCell::new();
    let snapshot = || -> io::Result<String> {
        if let Some(now) = memo.get() {
            return Ok(String::clone(now));
        }
        let now = snapshot()?;
        Ok(String::clone(memo.get_or_init(|| now)))
    };
    resolve(marks, sides, |path, is_staged| {
        let target = match (&staged, is_staged) {
            (Some(staged), true) => staged.clone(),
            _ => unstaged.clone(),
        };
        let key = DiffKey {
            dir: dir.to_owned(),
            path: path.to_owned(),
            target,
            stamp: Some(stamp_of(&repo, dir, head.clone(), path)),
        };
        cached_hash(git, cache, dir, key, &snapshot)
    })
}

/// The cache key of the diff `params` asks for (the same for [`diff`] and [`diff_hash`]).
fn diff_key(
    git: &dyn GitBackend,
    cache: &GitCache,
    dir: &Path,
    configured: Option<&str>,
    params: &GitDiffParams,
    path: &str,
) -> Result<DiffKey, ViewError> {
    let repo = cache.repo(git, dir)?;
    let (target, stamp) = match &params.commit {
        // Resolved first: a moving name (`HEAD~1`) must not hit a stale entry, and ranges or
        // trees are refused.
        Some(commit) => (Target::Commit(commit_id(git, dir, commit)?), None),
        None => {
            let head = git.head(dir)?;
            let want = (params.mode, params.staged, params.since.as_deref());
            let target = target_of(git, cache, dir, configured, want, head.as_deref())?;
            (target, Some(stamp_of(&repo, dir, head, path)))
        }
    };
    Ok(DiffKey {
        dir: dir.to_owned(),
        path: path.to_owned(),
        target,
        stamp,
    })
}

/// What a working-tree diff of `want` = (mode, staged side, `since`) compares; `head` is the
/// current HEAD.
fn target_of(
    git: &dyn GitBackend,
    cache: &GitCache,
    dir: &Path,
    configured: Option<&str>,
    want: (GitMode, bool, Option<&str>),
    head: Option<&str>,
) -> Result<Target, ViewError> {
    Ok(match want {
        (GitMode::Working, false, _) => Target::Unstaged,
        (GitMode::Working, true, _) => Target::Staged,
        (GitMode::Base, _, Some(since)) => since_target(git, dir, since)?,
        (GitMode::Base, _, None) => {
            let head = head.ok_or(ViewError::NoMergeBase)?;
            let (_, base) = resolve_base(git, dir, configured)?.ok_or(ViewError::NoMergeBase)?;
            let merge_base = cache
                .merge_base(git, dir, head, &base)?
                .ok_or(ViewError::NoMergeBase)?;
            Target::Since(merge_base)
        }
    })
}

/// What a cached working-tree diff of `path` depends on besides its target.
fn stamp_of(repo: &Repo, dir: &Path, head: Option<String>, path: &str) -> Stamp {
    Stamp {
        head,
        file: std::fs::symlink_metadata(dir.join(path))
            .ok()
            .and_then(|m| Some((m.modified().ok()?, m.len()))),
        index: modified(&repo.git_dir.join("index")),
    }
}

/// What git prints for the diff `key` names; an untracked file shows as all-new.
fn diff_text(
    git: &dyn GitBackend,
    dir: &Path,
    key: &DiffKey,
    snapshot: &dyn Fn() -> io::Result<String>,
) -> Result<String, ViewError> {
    let now = match key.target {
        Target::Tree(_) => snapshot()?,
        _ => String::new(),
    };
    let mut text = git.file_diff(dir, &key.path, key.target.side(&now))?;
    if text.is_empty()
        && matches!(key.target, Target::Unstaged | Target::Since(_))
        && dir.join(&key.path).is_file()
        && !git.is_tracked(dir, &key.path)?
    {
        text = git.untracked_diff(dir, &key.path)?;
    }
    Ok(text)
}

/// [`text_hash`] of the diff `key` names, cached.
fn cached_hash(
    git: &dyn GitBackend,
    cache: &GitCache,
    dir: &Path,
    key: DiffKey,
    snapshot: &dyn Fn() -> io::Result<String>,
) -> Result<Option<String>, ViewError> {
    if let Some(hash) = lock(&cache.hashes).get(&key) {
        return Ok(hash.clone());
    }
    let hash = text_hash(&diff_text(git, dir, &key, snapshot)?);
    let mut hashes = lock(&cache.hashes);
    if hashes.len() >= MAX_CACHED_HASHES {
        hashes.clear();
    }
    hashes.insert(key, hash.clone());
    Ok(hash)
}

/// `git.log`: up to `limit` commits touching `path` (already checked to stay inside `dir`; the
/// whole folder when `None`), newest first; none before the first commit.
pub fn log(
    git: &dyn GitBackend,
    cache: &GitCache,
    dir: &Path,
    limit: u32,
    path: Option<&str>,
) -> Result<Vec<Commit>, ViewError> {
    cache.repo(git, dir)?;
    if git.head(dir)?.is_none() {
        return Ok(Vec::new());
    }
    Ok(git.log(dir, limit, path)?)
}

/// `git.show`: the commit `commit` names and the files under `dir` it changed (first parent
/// for merges).
pub fn show(
    git: &dyn GitBackend,
    cache: &GitCache,
    dir: &Path,
    commit: &str,
) -> Result<CommitDetail, ViewError> {
    cache.repo(git, dir)?;
    let id = commit_id(git, dir, commit)?;
    let (info, files) = thread::scope(|s| {
        let files = s.spawn(|| git.changes(dir, Side::Commit(&id)));
        (git.commit_info(dir, &id), joined(files))
    });
    Ok(CommitDetail {
        commit: info?,
        files: files?,
    })
}

/// `git.push`: the current branch to its upstream, fast-forward only.
pub fn push(git: &dyn GitBackend, dir: &Path) -> Result<(), ViewError> {
    let branch = git.current_branch(dir)?.ok_or(ViewError::NoBranch)?;
    let short = branch
        .name
        .strip_prefix("refs/heads/")
        .unwrap_or(&branch.name);
    let upstream = branch
        .upstream
        .as_ref()
        .ok_or_else(|| ViewError::NoUpstream(short.to_owned()))?;
    Ok(git.push(dir, &branch.name, upstream)?)
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

/// The full id of the commit `name` names.
fn commit_id(git: &dyn GitBackend, dir: &Path, name: &str) -> Result<String, ViewError> {
    git.commit_of(dir, name)?
        .ok_or_else(|| ViewError::UnknownCommit(name.to_owned()))
}

/// What a `since` names: a commit ([`Target::Since`]) or else a tree ([`Target::Tree`]).
fn since_target(git: &dyn GitBackend, dir: &Path, name: &str) -> Result<Target, ViewError> {
    if let Some(commit) = git.commit_of(dir, name)? {
        return Ok(Target::Since(commit));
    }
    git.tree_of(dir, name)?
        .map(Target::Tree)
        .ok_or_else(|| ViewError::UnknownCommit(name.to_owned()))
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
mod tests;
