//! The folder a `fs.*` / `git.*` request works on (SPEC §20E.1): the project folder, or with
//! `worktree_task_id` the project's folder inside that task's worktree.

use std::path::PathBuf;
use std::sync::Arc;

use mkrew_protocol::{ErrorCode, Outcome, Project};
use ulid::Ulid;

use super::{Daemon, store_outcome, task_dir};

/// A resolved request target.
pub(super) struct Target {
    pub project: Project,
    /// The project folder or the project's folder inside the task's worktree.
    pub dir: PathBuf,
    /// Echoed in `git.changed`.
    pub worktree_task_id: Option<Ulid>,
}

impl Target {
    /// The key of the target's file watcher and `fs.find` list.
    pub fn key(&self) -> Ulid {
        self.worktree_task_id.unwrap_or(self.project.id)
    }
}

impl Daemon {
    /// Resolves `project_id` + `worktree_task_id`. The task must belong to the project and
    /// have a worktree that still exists (`invalid_state` otherwise).
    pub(super) async fn target(
        &self,
        project_id: Ulid,
        worktree_task_id: Option<Ulid>,
    ) -> Result<Target, Outcome> {
        let project = self
            .store()
            .get_project(project_id)
            .map_err(|e| store_outcome::<()>(Err(e)))?;
        let Some(task_id) = worktree_task_id else {
            let dir = PathBuf::from(&project.path);
            return Ok(Target {
                project,
                dir,
                worktree_task_id,
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
    /// `fs.find`'s cached list is invalidated.
    pub(super) async fn watched_target(
        &self,
        project_id: Ulid,
        worktree_task_id: Option<Ulid>,
    ) -> Result<Target, Outcome> {
        let target = self.target(project_id, worktree_task_id).await?;
        match target.worktree_task_id {
            None => self.hub.keep_watched(target.project.id, &target.dir),
            Some(task_id) => {
                self.hub
                    .keep_worktree_watched(target.project.id, task_id, &target.dir)
            }
        }
        Ok(target)
    }
}
