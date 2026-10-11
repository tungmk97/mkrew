//! Task worktrees (SPEC §20E.1, ADR 0010): a `mkrew/…` branch checked out in its own folder
//! under the data dir, plus copies of the git-ignored files `.worktreeinclude` names.
//! Merge back and removal are user actions (ADR 0010). Blocking: callers run every function
//! but the pure ones in `spawn_blocking`.

use std::io;
use std::path::{Component, Path, PathBuf};

use mkrew_core::Paths;
use ulid::Ulid;

use crate::git::{GitBackend, Merged};

/// Longest branch slug (SPEC §20E.1).
const MAX_SLUG: usize = 30;

/// The repo-root file listing what to copy into a new worktree.
const INCLUDE_FILE: &str = ".worktreeinclude";

/// `<data>/worktrees/<project_id>/<task_id>`.
pub fn dir(paths: &Paths, project: Ulid, task: Ulid) -> PathBuf {
    paths
        .worktrees_dir()
        .join(project.to_string())
        .join(task.to_string())
}

/// `mkrew/<slug>-<short ulid>`: the slug of `title`, then the end of the task id (its random
/// part, so tasks created in the same millisecond still differ).
pub fn branch_name(title: &str, task: Ulid) -> String {
    let id = task.to_string().to_ascii_lowercase();
    format!("mkrew/{}-{}", slug(title), &id[id.len() - 8..])
}

/// ASCII lowercase letters and digits, every other run of characters one `-`, no `-` at either
/// end, at most [`MAX_SLUG`] characters; `task` when nothing is left.
pub fn slug(title: &str) -> String {
    let mut slug = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    // ASCII only: byte length = character count.
    slug.truncate(MAX_SLUG);
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "task".into()
    } else {
        slug.into()
    }
}

/// The entries of a `.worktreeinclude`: one literal relative path per line (a trailing `/` is
/// dropped), blank lines and `#` comments skipped.
pub fn parse_include(text: &str) -> Vec<String> {
    // Some Windows editors start the file with a byte order mark.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.trim_end_matches(['/', '\\']).to_owned())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Adds the worktree of `branch` at `dest` from HEAD of the repo containing `project_dir`, then
/// copies the `.worktreeinclude` entries into it. Returns one warning per entry that was
/// skipped (not a path inside the repo, missing, not ignored, copy failed).
pub fn create(
    git: &dyn GitBackend,
    project_dir: &Path,
    dest: &Path,
    branch: &str,
) -> io::Result<Vec<String>> {
    let root = git.toplevel(project_dir)?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    git.worktree_add(project_dir, dest, branch)?;
    let entries = match std::fs::read_to_string(root.join(INCLUDE_FILE)) {
        Ok(text) => parse_include(&text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Ok(vec![format!("{INCLUDE_FILE}: {e}")]),
    };
    let mut warnings = Vec::new();
    let mut candidates = Vec::new();
    for entry in entries {
        if !inside(&entry) {
            warnings.push(format!("{entry}: not a relative path inside the repo"));
        } else if std::fs::symlink_metadata(root.join(&entry)).is_err() {
            warnings.push(format!("{entry}: not found"));
        } else {
            candidates.push(entry);
        }
    }
    // The worktree exists now: a failure here only skips the copies.
    let ignored = match git.ignored(&root, &candidates) {
        Ok(ignored) => ignored,
        Err(e) => {
            warnings.push(format!("{INCLUDE_FILE}: {e}"));
            return Ok(warnings);
        }
    };
    for entry in candidates {
        if !ignored.contains(&entry) {
            warnings.push(format!("{entry}: not ignored by git"));
        } else if let Err(e) = copy(&root.join(&entry), &dest.join(&entry)) {
            warnings.push(format!("{entry}: {e}"));
        }
    }
    Ok(warnings)
}

/// The project's folder inside the worktree at `root`: the same subfolder of the repo as
/// `project_dir` is of the user's checkout; `None` when `project_dir` is not in a repo.
pub fn project_dir(
    git: &dyn GitBackend,
    project_dir: &Path,
    root: &Path,
) -> io::Result<Option<PathBuf>> {
    let Some(repo) = git.repo(project_dir)? else {
        return Ok(None);
    };
    // One component at a time: no `/` inside a Windows path.
    let parts = repo.prefix.split('/').filter(|part| !part.is_empty());
    Ok(Some(
        parts.fold(root.to_path_buf(), |dir, part| dir.join(part)),
    ))
}

/// Outcome of [`merge`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Merge {
    Merged,
    /// Aborted: the user's checkout is as before. Repo-relative paths.
    Conflicts(Vec<String>),
    /// Not started, and why.
    Refused(String),
}

/// Merges `branch` (checked out in the worktree at `root`) into the user's checkout holding
/// `project_dir` (SPEC §20E.1): only when neither side has uncommitted changes (untracked
/// files count in the worktree: they would not be merged) and the checkout is on a branch.
pub fn merge(
    git: &dyn GitBackend,
    project_dir: &Path,
    root: &Path,
    branch: &str,
) -> io::Result<Merge> {
    if !root.is_dir() {
        return Ok(Merge::Refused(format!(
            "worktree {} is missing",
            root.display()
        )));
    }
    let dirty = git.uncommitted(root, true)?;
    if !dirty.is_empty() {
        return Ok(Merge::Refused(format!(
            "the worktree has uncommitted changes ({}); commit them there first",
            listed(&dirty)
        )));
    }
    let dirty = git.uncommitted(project_dir, false)?;
    if !dirty.is_empty() {
        return Ok(Merge::Refused(format!(
            "the project's checkout has uncommitted changes ({})",
            listed(&dirty)
        )));
    }
    if git.current_branch(project_dir)?.is_none() {
        let message = "the project's checkout is on a detached HEAD";
        return Ok(Merge::Refused(message.into()));
    }
    Ok(match git.merge(project_dir, branch)? {
        Merged::Done => Merge::Merged,
        Merged::Conflicts(files) => Merge::Conflicts(files),
        Merged::Refused(why) => Merge::Refused(why),
    })
}

/// Outcome of [`remove`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Remove {
    /// `branch_kept`: git refused `branch -d` (not merged).
    Removed { branch_kept: bool },
    /// Nothing done: the worktree's uncommitted files (repo-relative).
    Dirty(Vec<String>),
}

/// Removes the worktree at `root` of the repo holding `project_dir`, then deletes its branch
/// unless it is not merged (SPEC §20E.1). A folder already deleted by hand is pruned.
/// `release` runs right before git deletes the folder (to close handles into it).
pub fn remove(
    git: &dyn GitBackend,
    project_dir: &Path,
    root: &Path,
    branch: Option<&str>,
    release: impl FnOnce(),
) -> io::Result<Remove> {
    if root.is_dir() {
        let dirty = git.uncommitted(root, true)?;
        if !dirty.is_empty() {
            return Ok(Remove::Dirty(dirty));
        }
        release();
        git.worktree_remove(project_dir, root)?;
    } else {
        git.worktree_prune(project_dir)?;
    }
    let branch_kept = match branch {
        Some(branch) => !git.delete_branch(project_dir, branch)?,
        None => false,
    };
    Ok(Remove::Removed { branch_kept })
}

/// The first few `paths`, comma-separated, with a count of the rest.
pub fn listed(paths: &[String]) -> String {
    const SHOWN: usize = 5;
    let mut text = paths[..paths.len().min(SHOWN)].join(", ");
    if paths.len() > SHOWN {
        text.push_str(&format!(" and {} more", paths.len() - SHOWN));
    }
    text
}

/// Relative, with only normal components: it cannot leave the folder it is joined to.
fn inside(path: &str) -> bool {
    Path::new(path)
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// Copies a file, or a folder recursively (links inside a folder are copied as what they point
/// to; a link to a folder fails). A file already in the worktree is never overwritten: git
/// calls a folder holding tracked files not ignored, so this is only a safety net.
fn copy(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !std::fs::symlink_metadata(from)?.is_dir() {
        if std::fs::symlink_metadata(to).is_ok() {
            return Ok(());
        }
        return std::fs::copy(from, to).map(drop);
    }
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        copy(&entry.path(), &to.join(entry.file_name()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
