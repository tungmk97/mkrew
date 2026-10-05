//! `git.*` handlers (SPEC §7, §12B).

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mkrew_protocol::{
    ErrorCode, Event, GitChangedEvent, GitCommitParams, GitCommitResult, GitDiffParams,
    GitDiscardParams, GitSetBaseParams, GitStageParams, GitStatusParams, HunkTarget, Outcome,
    Project,
};
use serde::Serialize;
use tokio::task::JoinError;

use super::{Daemon, ok, store_outcome};
use crate::docs;
use crate::git::{GitBackend, Repo};
use crate::git_view::{self, ViewError};

impl Daemon {
    pub(super) async fn git_status(&self, params: GitStatusParams) -> Outcome {
        let project = match self.store().get_project(params.project_id) {
            Ok(project) => project,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        // The Git view is open: follow its changes with `git.changed`.
        self.hub
            .keep_watched(project.id, &PathBuf::from(&project.path));
        let (git, cache) = (Arc::clone(&self.git), Arc::clone(&self.git_cache));
        let status = tokio::task::spawn_blocking(move || {
            let Project { path, base_ref, .. } = project;
            git_view::status(
                &*git,
                &cache,
                &PathBuf::from(path),
                base_ref.as_deref(),
                params.mode,
            )
        })
        .await;
        view_outcome("git.status", status)
    }

    pub(super) async fn git_diff(&self, params: GitDiffParams) -> Outcome {
        let Some((path, _)) = docs::relative_path(&params.path) else {
            let message = format!(
                "path {:?} must be relative to the project root",
                params.path
            );
            return Outcome::error(ErrorCode::InvalidRequest, message);
        };
        if let Some(commit) = &params.commit
            && let Err(reason) = check_ref(commit)
        {
            return Outcome::error(ErrorCode::InvalidRequest, reason);
        }
        let project = match self.store().get_project(params.project_id) {
            Ok(project) => project,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        let (git, cache) = (Arc::clone(&self.git), Arc::clone(&self.git_cache));
        let diff = tokio::task::spawn_blocking(move || {
            let Project {
                path: dir,
                base_ref,
                ..
            } = project;
            let dir = PathBuf::from(dir);
            git_view::diff(&*git, &cache, &dir, base_ref.as_deref(), &params, &path)
        })
        .await;
        view_outcome("git.diff", diff)
    }

    /// `git.stage` / `git.unstage` (`unstage`): whole files by path, or one hunk of the
    /// unstaged (stage) or staged (unstage) diff applied to the index.
    pub(super) async fn git_stage(&self, params: GitStageParams, unstage: bool) -> Outcome {
        let method = if unstage { "git.unstage" } else { "git.stage" };
        let paths = match relative_paths(&params.paths) {
            Ok(paths) => paths,
            Err(outcome) => return outcome,
        };
        let hunk = match params.hunk {
            Some(HunkTarget { path, hunk }) if paths.is_empty() => {
                match relative_paths(std::slice::from_ref(&path)) {
                    Ok(mut path) => Some((path.remove(0), hunk)),
                    Err(outcome) => return outcome,
                }
            }
            None if !paths.is_empty() => None,
            _ => {
                let message = format!("{method} needs either paths or a hunk");
                return Outcome::error(ErrorCode::InvalidRequest, message);
            }
        };
        self.git_change(
            method,
            params.project_id,
            move |git, dir, repo| match &hunk {
                Some((path, hunk)) => {
                    let patch = mkrew_core::git::hunk_patch(path, hunk);
                    git.apply_cached(dir, repo, &patch, unstage)
                }
                None if unstage => git.unstage(dir, &paths),
                None => git.stage(dir, &paths),
            },
        )
        .await
    }

    /// `git.discard`: the unstaged changes of `paths` are lost; tracked files go back to their
    /// index version, untracked ones are deleted. Refused without `confirm: true`.
    pub(super) async fn git_discard(&self, params: GitDiscardParams) -> Outcome {
        if !params.confirm {
            let message = "git.discard loses changes: it needs confirm: true";
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
        let paths = match relative_paths(&params.paths) {
            Ok(paths) if !paths.is_empty() => paths,
            Ok(_) => return Outcome::error(ErrorCode::InvalidRequest, "git.discard needs paths"),
            Err(outcome) => return outcome,
        };
        self.git_change("git.discard", params.project_id, move |git, dir, _| {
            let mut tracked = Vec::new();
            let mut untracked = Vec::new();
            for path in paths {
                if git.is_tracked(dir, &path)? {
                    tracked.push(path);
                } else {
                    untracked.push(path);
                }
            }
            if !tracked.is_empty() {
                git.discard(dir, &tracked)?;
            }
            git.clean(dir, &untracked)
        })
        .await
    }

    /// `git.commit` of the whole index (also changes staged outside the project folder).
    pub(super) async fn git_commit(&self, params: GitCommitParams) -> Outcome {
        if params.message.trim().is_empty() {
            let message = "git.commit needs a message";
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
        let message = params.message;
        self.git_change("git.commit", params.project_id, move |git, dir, _| {
            git.commit(dir, &message)
                .map(|commit| GitCommitResult { commit })
        })
        .await
    }

    /// Runs `change` on the project's folder off the async runtime. The index mtime moves, so
    /// cached diffs go stale by themselves. Success broadcasts `git.changed` right away: the
    /// watcher misses a `.git` dir outside the project folder (repo subfolder, worktree).
    async fn git_change<T: Serialize + Send + 'static>(
        &self,
        method: &str,
        project_id: ulid::Ulid,
        change: impl FnOnce(&dyn GitBackend, &Path, &Repo) -> io::Result<T> + Send + 'static,
    ) -> Outcome {
        let project = match self.store().get_project(project_id) {
            Ok(project) => project,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        let (git, cache) = (Arc::clone(&self.git), Arc::clone(&self.git_cache));
        let result = tokio::task::spawn_blocking(move || {
            let dir = PathBuf::from(project.path);
            let repo = cache.repo(&*git, &dir)?;
            Ok(change(&*git, &dir, &repo)?)
        })
        .await;
        if let Ok(Ok(_)) = &result {
            let _ = self
                .hub
                .events
                .send(Event::GitChanged(GitChangedEvent { project_id }));
        }
        view_outcome(method, result)
    }

    /// Sets the Git view base of a project (`None` or blank = auto-detect) and broadcasts
    /// `project.updated`. The ref is not checked against the repo: it may only appear after
    /// a fetch.
    pub(super) fn git_set_base(&self, params: GitSetBaseParams) -> Outcome {
        let base = params.base_ref.as_deref().map(str::trim);
        let base = base.filter(|b| !b.is_empty());
        if let Some(base) = base
            && let Err(reason) = check_ref(base)
        {
            return Outcome::error(ErrorCode::InvalidRequest, reason);
        }
        let updated = self.store().set_project_base(params.project_id, base);
        if let Ok(project) = &updated {
            let _ = self.hub.events.send(Event::ProjectUpdated(project.clone()));
        }
        store_outcome(updated.map(|_| ()))
    }
}

/// Not a repo / no base = `invalid_state`; unknown commit = `not_found`; git failing =
/// `internal` with its stderr.
fn view_outcome<T: Serialize>(
    method: &str,
    result: Result<Result<T, ViewError>, JoinError>,
) -> Outcome {
    match result {
        Ok(Ok(value)) => ok(&value),
        Ok(Err(e @ (ViewError::NotARepo | ViewError::NoMergeBase))) => {
            Outcome::error(ErrorCode::InvalidState, e.to_string())
        }
        Ok(Err(e @ ViewError::UnknownCommit(_))) => {
            Outcome::error(ErrorCode::NotFound, e.to_string())
        }
        Ok(Err(ViewError::Git(e))) => {
            Outcome::error(ErrorCode::Internal, format!("{method} failed: {e}"))
        }
        Err(e) => Outcome::error(ErrorCode::Internal, format!("{method} failed: {e}")),
    }
}

/// The project-relative, `/`-separated form of each path; `invalid_request` for an absolute
/// path or one leaving the project.
fn relative_paths(paths: &[String]) -> Result<Vec<String>, Outcome> {
    paths
        .iter()
        .map(|path| {
            docs::relative_path(path)
                .map(|(path, _)| path)
                .ok_or_else(|| {
                    let message = format!("path {path:?} must be relative to the project root");
                    Outcome::error(ErrorCode::InvalidRequest, message)
                })
        })
        .collect()
}

/// Rejects what can never be a ref and would change a git command line: a leading `-` (read
/// as an option), whitespace and control characters.
fn check_ref(base: &str) -> Result<(), String> {
    if base.starts_with('-') {
        return Err(format!("invalid base ref {base:?}: starts with '-'"));
    }
    if base.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(format!(
            "invalid base ref {base:?}: contains whitespace or control characters"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::check_ref;

    #[test]
    fn base_refs_that_git_could_read_as_options_are_rejected() {
        for ok in ["main", "origin/main", "release/1.2", "HEAD~3", "a1b2c3"] {
            assert_eq!(check_ref(ok), Ok(()), "{ok}");
        }
        for bad in ["-f", "--force", "a b", "a\tb", "a\nb"] {
            assert!(check_ref(bad).is_err(), "{bad:?}");
        }
    }
}
