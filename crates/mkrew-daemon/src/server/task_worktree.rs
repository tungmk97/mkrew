//! `task.merge` and `task.remove_worktree` (SPEC §20E.1, ADR 0010).

use std::path::PathBuf;
use std::sync::Arc;

use mkrew_protocol::{
    ErrorCode, Outcome, RemoveWorktreeResult, SessionState, TaskConfirmParams, TaskMergeResult,
};
use ulid::Ulid;

use super::{Daemon, ok, store_outcome};
use crate::worktree::{self, Merge, Remove};

/// Merges and removals run one at a time, so one never sees (and aborts) another's merge.
static GIT_WRITES: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A task's worktree as recorded.
struct Recorded {
    project_dir: PathBuf,
    root: PathBuf,
    branch: Option<String>,
    live: bool,
}

impl Daemon {
    /// Merges the task's branch into the user's checkout (`git merge --no-ff`): only with
    /// `confirm` and when both sides are clean; a conflict aborts the merge.
    pub(super) async fn task_merge(&self, params: TaskConfirmParams) -> Outcome {
        if !params.confirm {
            let message = "task.merge changes your checkout; it needs confirm: true";
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
        let recorded = match self.recorded_worktree(params.task_id) {
            Ok(recorded) => recorded,
            Err(outcome) => return outcome,
        };
        let Some(branch) = recorded.branch else {
            let message = format!("task {} has no branch to merge", params.task_id);
            return Outcome::error(ErrorCode::InvalidState, message);
        };
        let _one_at_a_time = GIT_WRITES.lock().await;
        let git = Arc::clone(&self.git);
        let merged = tokio::task::spawn_blocking(move || {
            worktree::merge(&*git, &recorded.project_dir, &recorded.root, &branch)
        })
        .await;
        let task_id = params.task_id;
        match merged {
            Ok(Ok(Merge::Merged)) => {
                tracing::info!(%task_id, "worktree merged");
                ok(&TaskMergeResult {
                    merged: true,
                    conflicts: Vec::new(),
                })
            }
            Ok(Ok(Merge::Conflicts(conflicts))) => {
                tracing::info!(%task_id, ?conflicts, "worktree merge aborted on conflicts");
                ok(&TaskMergeResult {
                    merged: false,
                    conflicts,
                })
            }
            Ok(Ok(Merge::Refused(why))) => Outcome::error(ErrorCode::InvalidState, why),
            Ok(Err(e)) => Outcome::error(ErrorCode::Internal, format!("merge failed: {e}")),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("merge failed: {e}")),
        }
    }

    /// Removes the task's worktree (refused with uncommitted changes or a live session) and
    /// deletes its branch with `-d`; the task keeps `worktree` on but loses path and branch.
    pub(super) async fn task_remove_worktree(&self, params: TaskConfirmParams) -> Outcome {
        if !params.confirm {
            let message = "task.remove_worktree deletes a folder; it needs confirm: true";
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
        let task_id = params.task_id;
        let recorded = match self.recorded_worktree(task_id) {
            Ok(recorded) => recorded,
            Err(outcome) => return outcome,
        };
        if recorded.live {
            let message = format!("task {task_id} is running in its worktree; stop it first");
            return Outcome::error(ErrorCode::InvalidState, message);
        }
        let _one_at_a_time = GIT_WRITES.lock().await;
        let hub = Arc::clone(&self.hub);
        let removed = tokio::task::spawn_blocking(move || {
            let branch = recorded.branch.as_deref();
            // An open watcher handle can make the removal fail on Windows.
            let release = || hub.unwatch_worktree(task_id);
            worktree::remove(
                &*hub.git,
                &recorded.project_dir,
                &recorded.root,
                branch,
                release,
            )
        })
        .await;
        let branch_kept = match removed {
            Ok(Ok(Remove::Removed { branch_kept })) => branch_kept,
            Ok(Ok(Remove::Dirty(files))) => {
                let message = format!(
                    "the worktree has uncommitted changes ({}); commit or discard them first",
                    worktree::listed(&files)
                );
                return Outcome::error(ErrorCode::InvalidState, message);
            }
            Ok(Err(e)) => {
                let message = format!("removing the worktree failed: {e}");
                return Outcome::error(ErrorCode::Internal, message);
            }
            Err(e) => {
                let message = format!("removing the worktree failed: {e}");
                return Outcome::error(ErrorCode::Internal, message);
            }
        };
        tracing::info!(%task_id, branch_kept, "worktree removed");
        let task = self.store().set_task_worktree(task_id, None);
        if let Err(e) = &task {
            tracing::error!(%task_id, "worktree removal not recorded: {e}");
            return self.task_updated(task);
        }
        self.task_updated(task);
        ok(&RemoveWorktreeResult { branch_kept })
    }

    /// The task's worktree, project folder and whether a session of it is live;
    /// `invalid_state` for a task without a worktree.
    fn recorded_worktree(&self, task_id: Ulid) -> Result<Recorded, Outcome> {
        let found = {
            let store = self.store();
            store.get_task(task_id).and_then(|task| {
                let project = store.get_project(task.project_id)?;
                let live = store
                    .list_sessions(Some(project.id))?
                    .iter()
                    .any(|s| s.task_id == Some(task.id) && s.state != SessionState::Exited);
                Ok((task, project, live))
            })
        };
        let (task, project, live) = found.map_err(|e| store_outcome::<()>(Err(e)))?;
        let Some(root) = task.worktree_path else {
            let message = format!("task {task_id} has no worktree");
            return Err(Outcome::error(ErrorCode::InvalidState, message));
        };
        Ok(Recorded {
            project_dir: PathBuf::from(project.path),
            root: PathBuf::from(root),
            branch: task.branch,
            live,
        })
    }
}
