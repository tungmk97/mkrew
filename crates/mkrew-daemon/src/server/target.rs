//! The folder a `fs.*` / `git.*` request works on (SPEC §20E.1): the project folder, or with
//! `worktree_task_id` the project's folder inside that task's worktree.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mkrew_protocol::{ErrorCode, Outcome, Project};
use ulid::Ulid;

use super::{Daemon, store_outcome, task_dir};
use crate::{docs, files};

/// A resolved request target.
pub(super) struct Target {
    pub project: Project,
    /// The project folder or the project's folder inside the task's worktree, then the child
    /// repository folder `repo` in it.
    pub dir: PathBuf,
    /// Echoed in `git.changed`.
    pub worktree_task_id: Option<Ulid>,
    /// The child repository folder (ADR 0029): reply paths get `repo/` in front.
    pub repo: Option<String>,
}

impl Target {
    /// The key of the target's file watcher and `fs.find` list.
    pub fn key(&self) -> Ulid {
        self.worktree_task_id.unwrap_or(self.project.id)
    }
}

/// The child repository folder a request names (ADR 0029): `None` for none or blank;
/// `invalid_request` unless it is one plain folder name other than `.git`.
pub(super) fn repo_name(repo: Option<&str>) -> Result<Option<String>, Outcome> {
    let Some(repo) = repo.map(str::trim).filter(|repo| !repo.is_empty()) else {
        return Ok(None);
    };
    match docs::relative_path(repo) {
        Some((name, _)) if !name.contains('/') && !name.eq_ignore_ascii_case(".git") => {
            Ok(Some(name))
        }
        _ => {
            let message = format!("repo {repo:?} must be a folder directly in the project folder");
            Err(Outcome::error(ErrorCode::InvalidRequest, message))
        }
    }
}

impl Daemon {
    /// Resolves `project_id` + `worktree_task_id` + `repo`. The task must belong to the project
    /// and have a worktree that still exists (`invalid_state` otherwise). `repo` is refused
    /// with a task worktree (ADR 0029: worktrees hold one repository).
    pub(super) async fn target(
        &self,
        project_id: Ulid,
        worktree_task_id: Option<Ulid>,
        repo: Option<&str>,
    ) -> Result<Target, Outcome> {
        let repo = repo_name(repo)?;
        if repo.is_some() && worktree_task_id.is_some() {
            let message = "repo cannot be combined with worktree_task_id";
            return Err(Outcome::error(ErrorCode::InvalidRequest, message));
        }
        let project = self
            .store()
            .get_project(project_id)
            .map_err(|e| store_outcome::<()>(Err(e)))?;
        let Some(task_id) = worktree_task_id else {
            if let Some(repo) = &repo {
                // SPEC §20E.1: one of `git.repos`; git would otherwise walk up to the parent repo.
                if !files::child_repos(Path::new(&project.path)).contains(repo) {
                    let message = format!("{repo} is not a git repository of this workspace");
                    return Err(Outcome::error(ErrorCode::InvalidRequest, message));
                }
            }
            let mut dir = PathBuf::from(&project.path);
            if let Some(repo) = &repo {
                dir.push(repo);
            }
            return Ok(Target {
                project,
                dir,
                worktree_task_id,
                repo,
            });
        };
        let task = self
            .store()
            .get_task(task_id)
            .map_err(|e| store_outcome::<()>(Err(e)))?;
        if task.project_id != project_id {
            let message = format!("task {task_id} is not in project {project_id}");
            return Err(Outcome::error(ErrorCode::InvalidRequest, message));
        }
        let Some(root) = task.worktree_path.map(PathBuf::from) else {
            let message = format!("task {task_id} has no worktree");
            return Err(Outcome::error(ErrorCode::InvalidState, message));
        };
        let git = Arc::clone(&self.git);
        let project_dir = PathBuf::from(&project.path);
        let dir =
            tokio::task::spawn_blocking(move || task_dir(&*git, &project_dir, Some(&root))).await;
        match dir {
            Ok(Ok(dir)) => Ok(Target {
                project,
                dir,
                worktree_task_id,
                repo: None,
            }),
            Ok(Err(e)) => {
                let message = format!("task {task_id}'s worktree: {e}");
                Err(Outcome::error(ErrorCode::InvalidState, message))
            }
            Err(e) => Err(Outcome::error(
                ErrorCode::Internal,
                format!("resolving task {task_id}'s worktree failed: {e}"),
            )),
        }
    }

    /// [`Daemon::target`], kept watched so its view refreshes on `git.changed` and
    /// `fs.find`'s cached list is invalidated. The project folder is watched, never a child
    /// repository alone: the watch is keyed by the project.
    pub(super) async fn watched_target(
        &self,
        project_id: Ulid,
        worktree_task_id: Option<Ulid>,
        repo: Option<&str>,
    ) -> Result<Target, Outcome> {
        let target = self.target(project_id, worktree_task_id, repo).await?;
        match target.worktree_task_id {
            None => self
                .hub
                .keep_watched(target.project.id, Path::new(&target.project.path)),
            Some(task_id) => {
                self.hub
                    .keep_worktree_watched(target.project.id, task_id, &target.dir)
            }
        }
        Ok(target)
    }
}
