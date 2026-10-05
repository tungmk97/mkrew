//! `git.*` handlers (SPEC §7, §12B).

use std::path::PathBuf;
use std::sync::Arc;

use mkrew_protocol::{
    ErrorCode, Event, GitDiffParams, GitSetBaseParams, GitStatusParams, Outcome, Project,
};
use serde::Serialize;
use tokio::task::JoinError;

use super::{Daemon, ok, store_outcome};
use crate::docs;
use crate::git_view::{self, ViewError};

impl Daemon {
    pub(super) async fn git_status(&self, params: GitStatusParams) -> Outcome {
        let project = match self.store().get_project(params.project_id) {
            Ok(project) => project,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
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
