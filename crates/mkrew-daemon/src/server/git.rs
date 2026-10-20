//! `git.*` handlers (SPEC §7, §12B).

use std::path::Path;
use std::sync::Arc;

use mkrew_core::viewed::{self, Mark};
use mkrew_protocol::{
    ChangedFile, ErrorCode, Event, GitChangedEvent, GitCommitParams, GitCommitResult,
    GitDiffParams, GitDiscardParams, GitLogParams, GitRemoteParams, GitSetBaseParams,
    GitSetViewedParams, GitShowParams, GitStageParams, GitStatusParams, HunkTarget, Outcome,
    Project, ProjectRef, ViewedFile,
};
use mkrew_store::ViewedScope;
use serde::Serialize;
use tokio::task::JoinError;
use ulid::Ulid;

use super::target::{Target, repo_name};
use super::{Daemon, ok, store_outcome};
use crate::git::{GitBackend, Repo};
use crate::git_view::{self, ViewError};
use crate::{docs, files};

impl Daemon {
    pub(super) async fn git_status(&self, params: GitStatusParams) -> Outcome {
        if let Some(since) = &params.since
            && let Err(reason) = check_ref(since)
        {
            return Outcome::error(ErrorCode::InvalidRequest, reason);
        }
        // The Git view is open: follow its changes with `git.changed`.
        let target = match self
            .watched_target(
                params.project_id,
                params.worktree_task_id,
                params.repo.as_deref(),
            )
            .await
        {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        let (hub, cache) = (Arc::clone(&self.hub), Arc::clone(&self.git_cache));
        let status = tokio::task::spawn_blocking(move || {
            let Target {
                project: Project { id, base_ref, .. },
                dir,
                repo,
                ..
            } = target;
            git_view::status(
                &*hub.git,
                &cache,
                &dir,
                base_ref.as_deref(),
                params.mode,
                params.since.as_deref(),
                &|| hub.snapshots.tree(&*hub.git, id, &dir),
            )
            .map(|mut status| {
                for files in [
                    &mut status.staged,
                    &mut status.unstaged,
                    &mut status.untracked,
                    &mut status.against_base,
                ] {
                    prefix_files(repo.as_deref(), files);
                }
                status
            })
        })
        .await;
        view_outcome("git.status", status)
    }

    pub(super) async fn git_diff(&self, params: GitDiffParams) -> Outcome {
        let path = match project_path(params.repo.as_deref(), &params.path) {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        if let Some(commit) = params.commit.as_ref().or(params.since.as_ref())
            && let Err(reason) = check_ref(commit)
        {
            return Outcome::error(ErrorCode::InvalidRequest, reason);
        }
        let target = match self
            .target(
                params.project_id,
                params.worktree_task_id,
                params.repo.as_deref(),
            )
            .await
        {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        let (hub, cache) = (Arc::clone(&self.hub), Arc::clone(&self.git_cache));
        let diff = tokio::task::spawn_blocking(move || {
            let Target {
                project: Project { id, base_ref, .. },
                dir,
                repo,
                ..
            } = target;
            let snapshot = || hub.snapshots.tree(&*hub.git, id, &dir);
            git_view::diff(
                &*hub.git,
                &cache,
                &dir,
                base_ref.as_deref(),
                &params,
                &path,
                &snapshot,
            )
            .map(|mut diff| {
                diff.path = in_repo(repo.as_deref(), &diff.path);
                diff.old_path = diff.old_path.map(|old| in_repo(repo.as_deref(), &old));
                diff
            })
        })
        .await;
        view_outcome("git.diff", diff)
    }

    /// `git.viewed` (ADR 0065): which files of the listed changes (`params` as in
    /// `git.status`) are still marked as viewed. A mark whose file's diff text changed, or
    /// whose file no longer changes, is deleted here; marks of other lists are not touched.
    pub(super) async fn git_viewed(&self, params: GitStatusParams) -> Outcome {
        if let Some(since) = &params.since
            && let Err(reason) = check_ref(since)
        {
            return Outcome::error(ErrorCode::InvalidRequest, reason);
        }
        let target = match self
            .target(
                params.project_id,
                params.worktree_task_id,
                params.repo.as_deref(),
            )
            .await
        {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        let repo_key = target.repo.clone().unwrap_or_default();
        let list = viewed::list_key(params.mode, params.since.as_deref());
        let scope = ViewedScope {
            project_id: params.project_id,
            worktree_task_id: params.worktree_task_id,
            repo: &repo_key,
            list: &list,
        };
        let stored = self.store().viewed_marks(&scope);
        let marks: Vec<Mark> = match stored {
            Ok(stored) => stored
                .into_iter()
                .map(|(path, hash)| Mark { path, hash })
                .collect(),
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        if marks.is_empty() {
            return ok(&Vec::<ViewedFile>::new());
        }
        let (hub, cache) = (Arc::clone(&self.hub), Arc::clone(&self.git_cache));
        let Target {
            project: Project { id, base_ref, .. },
            dir,
            repo,
            ..
        } = target;
        let (mode, since) = (params.mode, params.since);
        let resolved = tokio::task::spawn_blocking(move || {
            git_view::viewed(
                &*hub.git,
                &cache,
                &dir,
                base_ref.as_deref(),
                (mode, since.as_deref()),
                marks,
                &|| hub.snapshots.tree(&*hub.git, id, &dir),
            )
        })
        .await;
        let result = resolved.map(|inner| {
            inner.map(|resolved| {
                let store = self.store();
                for stale in &resolved.stale {
                    let hash = Some(stale.hash.as_str());
                    if let Err(e) = store.remove_viewed(&scope, &stale.path, hash) {
                        tracing::warn!(
                            project_id = %scope.project_id,
                            "stale viewed mark not deleted: {e}"
                        );
                    }
                }
                drop(store);
                resolved
                    .viewed
                    .into_iter()
                    .map(|file| ViewedFile {
                        path: in_repo(repo.as_deref(), &file.path),
                        staged: file.staged,
                    })
                    .collect::<Vec<_>>()
            })
        });
        view_outcome("git.viewed", result)
    }

    /// `git.set_viewed` (ADR 0065): hashes the file's diff as it is now and stores the mark
    /// (`invalid_state` for a file with no diff), or removes it: the mark of that exact text,
    /// else (no diff any more) every mark of the file in the list.
    pub(super) async fn git_set_viewed(&self, params: GitSetViewedParams) -> Outcome {
        let GitSetViewedParams {
            diff: params,
            viewed: on,
        } = params;
        if params.commit.is_some() {
            let message = "a commit's files cannot be marked as viewed";
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
        if let Some(since) = &params.since
            && let Err(reason) = check_ref(since)
        {
            return Outcome::error(ErrorCode::InvalidRequest, reason);
        }
        let path = match project_path(params.repo.as_deref(), &params.path) {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        let target = match self
            .target(
                params.project_id,
                params.worktree_task_id,
                params.repo.as_deref(),
            )
            .await
        {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        let repo_key = target.repo.clone().unwrap_or_default();
        let list = viewed::list_key(params.mode, params.since.as_deref());
        let (hub, cache) = (Arc::clone(&self.hub), Arc::clone(&self.git_cache));
        let Target {
            project: Project { id, base_ref, .. },
            dir,
            ..
        } = target;
        let hashed = {
            let (params, path) = (params.clone(), path.clone());
            tokio::task::spawn_blocking(move || {
                let snapshot = || hub.snapshots.tree(&*hub.git, id, &dir);
                git_view::diff_hash(
                    &*hub.git,
                    &cache,
                    &dir,
                    base_ref.as_deref(),
                    &params,
                    &path,
                    &snapshot,
                )
            })
            .await
        };
        let hash = match hashed {
            Ok(Ok(hash)) => hash,
            other => return view_outcome("git.set_viewed", other),
        };
        let scope = ViewedScope {
            project_id: params.project_id,
            worktree_task_id: params.worktree_task_id,
            repo: &repo_key,
            list: &list,
        };
        let stored = match (on, hash) {
            (true, Some(hash)) => self.store().add_viewed(&scope, &path, &hash),
            (true, None) => {
                let message = format!("{path} has no changes to mark as viewed");
                return Outcome::error(ErrorCode::InvalidState, message);
            }
            (false, hash) => self.store().remove_viewed(&scope, &path, hash.as_deref()),
        };
        store_outcome(stored)
    }

    /// `git.stage` / `git.unstage` (`unstage`): whole files by path, or one hunk of the
    /// unstaged (stage) or staged (unstage) diff applied to the index.
    pub(super) async fn git_stage(&self, params: GitStageParams, unstage: bool) -> Outcome {
        let method = if unstage { "git.unstage" } else { "git.stage" };
        let paths = match relative_paths(&params.paths)
            .and_then(|paths| repo_paths(params.repo.as_deref(), paths))
        {
            Ok(paths) => paths,
            Err(outcome) => return outcome,
        };
        let hunk = match params.hunk {
            Some(HunkTarget { path, hunk }) if paths.is_empty() => {
                match project_path(params.repo.as_deref(), &path) {
                    Ok(path) => Some((path, hunk)),
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
            (params.project_id, params.worktree_task_id, params.repo),
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
        let paths = match relative_paths(&params.paths)
            .and_then(|paths| repo_paths(params.repo.as_deref(), paths))
        {
            Ok(paths) if !paths.is_empty() => paths,
            Ok(_) => return Outcome::error(ErrorCode::InvalidRequest, "git.discard needs paths"),
            Err(outcome) => return outcome,
        };
        let target = (params.project_id, params.worktree_task_id, params.repo);
        self.git_change("git.discard", target, move |git, dir, _| {
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
        let target = (params.project_id, params.worktree_task_id, params.repo);
        self.git_change("git.commit", target, move |git, dir, _| {
            git.commit(dir, &message)
                .map(|commit| GitCommitResult { commit })
        })
        .await
    }

    /// `git.log`: newest first, the whole folder or one path in it.
    pub(super) async fn git_log(&self, params: GitLogParams) -> Outcome {
        let path = match params
            .path
            .as_deref()
            .map(|path| project_path(params.repo.as_deref(), path))
        {
            None => None,
            Some(Ok(path)) => Some(path),
            Some(Err(outcome)) => return outcome,
        };
        let dir = match self
            .target(
                params.project_id,
                params.worktree_task_id,
                params.repo.as_deref(),
            )
            .await
        {
            Ok(target) => target.dir,
            Err(outcome) => return outcome,
        };
        let (git, cache) = (Arc::clone(&self.git), Arc::clone(&self.git_cache));
        let log = tokio::task::spawn_blocking(move || {
            git_view::log(&*git, &cache, &dir, params.limit, path.as_deref())
        })
        .await;
        view_outcome("git.log", log)
    }

    /// `git.show`: one commit and the files of the project folder it changed.
    pub(super) async fn git_show(&self, params: GitShowParams) -> Outcome {
        if let Err(reason) = check_ref(&params.commit) {
            return Outcome::error(ErrorCode::InvalidRequest, reason);
        }
        let target = match self
            .target(
                params.project_id,
                params.worktree_task_id,
                params.repo.as_deref(),
            )
            .await
        {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        let (dir, repo) = (target.dir, target.repo);
        let (git, cache) = (Arc::clone(&self.git), Arc::clone(&self.git_cache));
        let detail = tokio::task::spawn_blocking(move || {
            git_view::show(&*git, &cache, &dir, &params.commit).map(|mut detail| {
                prefix_files(repo.as_deref(), &mut detail.files);
                detail
            })
        })
        .await;
        view_outcome("git.show", detail)
    }

    /// `git.fetch` (refreshes the base and ahead/behind) and `git.push` (current branch to
    /// its upstream, never forced). Both may take long; other requests are not held up.
    pub(super) async fn git_remote(&self, params: GitRemoteParams, push: bool) -> Outcome {
        let method = if push { "git.push" } else { "git.fetch" };
        let target = (params.project_id, params.worktree_task_id, params.repo);
        self.git_change(method, target, move |git, dir, _| {
            if push {
                git_view::push(git, dir)
            } else {
                Ok(git.fetch(dir)?)
            }
        })
        .await
    }

    /// Runs `change` on the target's folder (project or task worktree) off the async runtime.
    /// The index mtime moves, so cached diffs go stale by themselves. Success broadcasts
    /// `git.changed` right away: the watcher misses a `.git` dir outside the project folder
    /// (repo subfolder, worktree).
    async fn git_change<T: Serialize + Send + 'static, E: Into<ViewError>>(
        &self,
        method: &str,
        (project_id, worktree_task_id, repo): (Ulid, Option<Ulid>, Option<String>),
        change: impl FnOnce(&dyn GitBackend, &Path, &Repo) -> Result<T, E> + Send + 'static,
    ) -> Outcome {
        let dir = match self
            .target(project_id, worktree_task_id, repo.as_deref())
            .await
        {
            Ok(target) => target.dir,
            Err(outcome) => return outcome,
        };
        let (git, cache) = (Arc::clone(&self.git), Arc::clone(&self.git_cache));
        let result = tokio::task::spawn_blocking(move || {
            let repo = cache.repo(&*git, &dir)?;
            change(&*git, &dir, &repo).map_err(Into::into)
        })
        .await;
        if let Ok(Ok(_)) = &result {
            let _ = self.hub.events.send(Event::GitChanged(GitChangedEvent {
                project_id,
                worktree_task_id,
            }));
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

    /// `git.repos` (ADR 0029): `""` when the project folder is in a git repository, then each
    /// direct child folder holding `.git`. Read on every call: folders come and go.
    pub(super) async fn git_repos(&self, params: ProjectRef) -> Outcome {
        let target = match self.target(params.project_id, None, None).await {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        let git = Arc::clone(&self.git);
        let repos = tokio::task::spawn_blocking(move || {
            let root = target.dir;
            let in_repo = matches!(git.repo(&root), Ok(Some(_)));
            let mut repos = files::child_repos(&root);
            if in_repo {
                repos.insert(0, String::new());
            }
            repos
        })
        .await;
        match repos {
            Ok(repos) => ok(&repos),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("git.repos failed: {e}")),
        }
    }
}

/// Not a repo / no base / nothing to push to = `invalid_state`; unknown commit = `not_found`;
/// git failing = `internal` with its stderr.
fn view_outcome<T: Serialize>(
    method: &str,
    result: Result<Result<T, ViewError>, JoinError>,
) -> Outcome {
    match result {
        Ok(Ok(value)) => ok(&value),
        Ok(Err(
            e @ (ViewError::NotARepo
            | ViewError::NoMergeBase
            | ViewError::NoBranch
            | ViewError::NoUpstream(_)),
        )) => Outcome::error(ErrorCode::InvalidState, e.to_string()),
        Ok(Err(e @ ViewError::UnknownCommit(_))) => {
            Outcome::error(ErrorCode::NotFound, e.to_string())
        }
        Ok(Err(ViewError::Git(e))) => {
            Outcome::error(ErrorCode::Internal, format!("{method} failed: {e}"))
        }
        Err(e) => Outcome::error(ErrorCode::Internal, format!("{method} failed: {e}")),
    }
}

/// The project-relative, `/`-separated form of `path`; `invalid_request` for an absolute
/// path or one leaving the project.
fn relative(path: &str) -> Result<String, Outcome> {
    docs::relative_path(path)
        .map(|(path, _)| path)
        .ok_or_else(|| {
            let message = format!("path {path:?} must be relative to the project root");
            Outcome::error(ErrorCode::InvalidRequest, message)
        })
}

/// [`relative`] for each of `paths`.
fn relative_paths(paths: &[String]) -> Result<Vec<String>, Outcome> {
    paths.iter().map(|path| relative(path)).collect()
}

/// [`relative`], then made repo-relative by [`repo_path`].
fn project_path(repo: Option<&str>, path: &str) -> Result<String, Outcome> {
    relative(path).and_then(|path| repo_path(repo, path))
}

/// One request path made repo-relative (ADR 0029): it must lie inside `repo` and loses its
/// `repo/` prefix before git sees it; `invalid_request` otherwise. No repo: unchanged.
fn repo_path(repo: Option<&str>, path: String) -> Result<String, Outcome> {
    let Some(repo) = repo_name(repo)? else {
        return Ok(path);
    };
    match path
        .strip_prefix(repo.as_str())
        .and_then(|rest| rest.strip_prefix('/'))
    {
        Some(inner) if docs::relative_path(inner).is_some() => Ok(inner.to_owned()),
        _ => {
            let message = format!("path {path:?} is not in repo {repo:?}");
            Err(Outcome::error(ErrorCode::InvalidRequest, message))
        }
    }
}

/// [`repo_path`] for each of `paths`.
fn repo_paths(repo: Option<&str>, paths: Vec<String>) -> Result<Vec<String>, Outcome> {
    paths
        .into_iter()
        .map(|path| repo_path(repo, path))
        .collect()
}

/// A reply path back to project-relative (ADR 0029): `repo/` in front.
fn in_repo(repo: Option<&str>, path: &str) -> String {
    repo.map_or_else(|| path.to_owned(), |repo| format!("{repo}/{path}"))
}

/// [`in_repo`] for the paths of each file.
fn prefix_files(repo: Option<&str>, files: &mut [ChangedFile]) {
    for file in files {
        file.path = in_repo(repo, &file.path);
        if let Some(old) = &mut file.old_path {
            *old = in_repo(repo, old);
        }
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
    use super::{check_ref, repo_paths};

    #[test]
    fn base_refs_that_git_could_read_as_options_are_rejected() {
        for ok in ["main", "origin/main", "release/1.2", "HEAD~3", "a1b2c3"] {
            assert_eq!(check_ref(ok), Ok(()), "{ok}");
        }
        for bad in ["-f", "--force", "a b", "a\tb", "a\nb"] {
            assert!(check_ref(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn repo_paths_must_lie_in_the_repo_and_lose_its_prefix() {
        let paths = |p: &[&str]| p.iter().map(|p| (*p).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            repo_paths(None, paths(&["a/b"])).ok(),
            Some(paths(&["a/b"]))
        );
        assert_eq!(
            repo_paths(Some("fe"), paths(&["fe/src/a.ts", "fe/x"])).ok(),
            Some(paths(&["src/a.ts", "x"]))
        );
        // The repo folder itself, a sibling sharing its prefix, another repo, the root.
        for bad in ["fe", "fe/", "fex/a", "be/a", "a", "fe//x"] {
            assert!(repo_paths(Some("fe"), paths(&[bad])).is_err(), "{bad}");
        }
        // A drive-letter component is only invalid where `relative_path` rejects it.
        #[cfg(windows)]
        assert!(repo_paths(Some("fe"), paths(&["fe/C:/x"])).is_err());
        // The repo name is checked too.
        for bad in ["../x", "fe/src", ".git", "FE/.git"] {
            assert!(repo_paths(Some(bad), paths(&["x/a"])).is_err(), "{bad}");
        }
    }
}
