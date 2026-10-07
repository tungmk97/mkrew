//! `turn.list`, `turn.restore_preview`, `turn.restore` (SPEC §20C.2), `session.files`
//! (SPEC §20C.4) and `session.usage` (SPEC §20E.3).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;

use mkrew_protocol::{
    ErrorCode, Outcome, SessionFilesParams, SessionState, SessionUsageEvent, TurnRef,
    TurnRestoreParams,
};
use ulid::Ulid;

use super::{Daemon, ok, store_outcome};
use crate::undo;

/// Where and to what `turn.restore` resets.
struct RestoreTarget {
    project: Ulid,
    dir: PathBuf,
    start_tree: String,
    working: bool,
}

impl Daemon {
    /// The session's persisted turns, oldest first.
    pub(super) fn turn_list(&self, session_id: Ulid) -> Outcome {
        let store = self.store();
        store_outcome(
            store
                .get_session(session_id)
                .and_then(|_| store.list_turns(session_id)),
        )
    }

    /// What `turn.restore` would discard: everything since turn `n` started.
    pub(super) async fn turn_restore_preview(&self, turn: TurnRef) -> Outcome {
        let target = match self.restore_target(&turn) {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        let hub = Arc::clone(&self.hub);
        let previewed = tokio::task::spawn_blocking(move || {
            let (dir, start) = (&target.dir, &target.start_tree);
            undo::preview_turn(&*hub.git, &hub.snapshots, target.project, dir, start)
        })
        .await;
        match previewed {
            Ok(Ok(preview)) => ok(&preview),
            Ok(Err(e)) => Outcome::error(ErrorCode::Internal, e.to_string()),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("restore preview failed: {e}")),
        }
    }

    /// Puts the project folder back to how it was before turn `n`: only with `confirm`, a
    /// matching fingerprint and while the session is not `working` (rules of `task.undo`).
    pub(super) async fn turn_restore(&self, params: TurnRestoreParams) -> Outcome {
        if !params.confirm {
            let message = "turn.restore discards changes; it needs confirm: true";
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
        let turn = TurnRef {
            session_id: params.session_id,
            n: params.n,
        };
        let target = match self.restore_target(&turn) {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        if target.working {
            let message = format!("session {} is working; wait or stop it", params.session_id);
            return Outcome::error(ErrorCode::InvalidState, message);
        }
        let hub = Arc::clone(&self.hub);
        let restored = tokio::task::spawn_blocking(move || {
            let (dir, start) = (&target.dir, &target.start_tree);
            let (previewed, created) = (&params.fingerprint, &params.created);
            let (git, snapshots) = (&*hub.git, &hub.snapshots);
            undo::restore_turn(
                git,
                snapshots,
                target.project,
                dir,
                start,
                previewed,
                created,
            )
        })
        .await;
        match restored {
            Ok(Ok(undo::Undone::Done)) => {
                tracing::info!(session = %turn.session_id, n = turn.n, "restored before turn");
                ok(&())
            }
            Ok(Ok(undo::Undone::Stale)) => {
                let message = "the changes differ from the preview; preview again";
                Outcome::error(ErrorCode::InvalidState, message)
            }
            Ok(Err(e)) => Outcome::error(ErrorCode::Internal, e.to_string()),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("restore failed: {e}")),
        }
    }

    /// `session.files` (SPEC §20C.4): per session of the project, the files of its turns that
    /// ended since `since_ms` plus its hook-reported edits since then (the running turn's).
    pub(super) fn session_files(&self, params: SessionFilesParams) -> Outcome {
        let (project_id, since) = (params.project_id, params.since_ms);
        let found = {
            let store = self.store();
            store.get_project(project_id).and_then(|_| {
                let turns = store.project_turns(project_id, since)?;
                let edits = store.project_session_events(project_id, "edited", since)?;
                Ok((turns, edits))
            })
        };
        let (turns, edits) = match found {
            Ok(found) => found,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        let mut files: BTreeMap<Ulid, BTreeSet<String>> = BTreeMap::new();
        for (session, turn) in turns {
            let paths = turn.files.into_iter().map(|f| f.path);
            files.entry(session).or_default().extend(paths);
        }
        for (session, event) in edits {
            let paths = event.payload.get("paths").and_then(|p| p.as_array());
            let paths = paths.into_iter().flatten().filter_map(|p| p.as_str());
            files
                .entry(session)
                .or_default()
                .extend(paths.map(str::to_owned));
        }
        files.retain(|_, paths| !paths.is_empty());
        ok(&files)
    }

    /// The newest recorded token usage of each session of the project (SPEC §20E.3).
    pub(super) fn session_usage(&self, project_id: Ulid) -> Outcome {
        let found = {
            let store = self.store();
            store
                .get_project(project_id)
                .and_then(|_| store.project_session_events(project_id, crate::usage::EVENT_KIND, 0))
        };
        let events = match found {
            Ok(events) => events,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        // Oldest first: later events replace earlier ones.
        let mut newest: BTreeMap<Ulid, SessionUsageEvent> = BTreeMap::new();
        for (session, event) in events {
            match serde_json::from_value(event.payload) {
                Ok(usage) => {
                    newest.insert(session, usage);
                }
                Err(e) => tracing::warn!(%session, "unreadable usage event: {e}"),
            }
        }
        ok(&newest.into_values().collect::<Vec<_>>())
    }

    /// The session's project folder and the turn's start tree; `invalid_state` without one.
    fn restore_target(&self, turn: &TurnRef) -> Result<RestoreTarget, Outcome> {
        let found = {
            let store = self.store();
            store.get_session(turn.session_id).and_then(|session| {
                let project = store.get_project(session.project_id)?;
                let start = store.get_turn(turn.session_id, turn.n)?.start_tree;
                let worktree = match session.task_id {
                    Some(task_id) => store.get_task(task_id)?.worktree,
                    None => false,
                };
                Ok((session, project, start, worktree))
            })
        };
        let (session, project, start, worktree) = found.map_err(|e| store_outcome::<()>(Err(e)))?;
        // Its snapshots may be of the task's worktree: never write them to the project folder.
        if worktree {
            let message = "restoring turns of a worktree task is not supported yet";
            return Err(Outcome::error(ErrorCode::InvalidState, message));
        }
        // A stored value is never handed to git unless it is a full object id.
        let Some(start_tree) = start.filter(|t| undo::is_commit_id(t)) else {
            let message = format!(
                "turn {} of session {} has no snapshot (not a git repo, or older than 7 days)",
                turn.n, turn.session_id
            );
            return Err(Outcome::error(ErrorCode::InvalidState, message));
        };
        Ok(RestoreTarget {
            project: project.id,
            dir: PathBuf::from(project.path),
            start_tree,
            working: session.state == SessionState::Working,
        })
    }
}
