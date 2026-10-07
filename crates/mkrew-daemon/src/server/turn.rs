//! `turn.list`, `turn.restore_preview`, `turn.restore` (SPEC §20C.2).

use std::path::PathBuf;
use std::sync::Arc;

use mkrew_protocol::{ErrorCode, Outcome, SessionState, TurnRef, TurnRestoreParams};
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

    /// The session's project folder and the turn's start tree; `invalid_state` without one.
    fn restore_target(&self, turn: &TurnRef) -> Result<RestoreTarget, Outcome> {
        let found = {
            let store = self.store();
            store.get_session(turn.session_id).and_then(|session| {
                let project = store.get_project(session.project_id)?;
                let start = store.get_turn(turn.session_id, turn.n)?.start_tree;
                Ok((session, project, start))
            })
        };
        let (session, project, start) = found.map_err(|e| store_outcome::<()>(Err(e)))?;
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
