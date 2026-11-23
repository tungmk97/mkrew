//! `tsp.snapshot` and `tsp.event` (spec 2026-11-23-omp-tsp-surface-design 6.4): both go to the
//! session task, which owns the TSP engine.

use mkrew_protocol::{ErrorCode, Outcome, TspEventParams, TspSnapshotParams};
use tokio::sync::oneshot;

use super::{Daemon, ok};
use crate::session::Command;

impl Daemon {
    /// The surfaces of a live session; a session without TSP answers `off` and no surfaces.
    pub(super) async fn tsp_snapshot(&self, params: TspSnapshotParams) -> Outcome {
        let id = params.session_id;
        let Some(session) = self.hub.sender(id) else {
            return self.not_live(id);
        };
        let (reply, snapshot) = oneshot::channel();
        if session.send(Command::TspSnapshot { reply }).await.is_err() {
            return self.not_live(id);
        }
        match snapshot.await {
            Ok(snapshot) => ok(&snapshot),
            // The session ended while the request was on its way.
            Err(_) => self.not_live(id),
        }
    }

    /// Writes a client's event to the agent. A refused event (not in the P1 list, no TSP in
    /// this session) is an `invalid_request` with the reason.
    pub(super) async fn tsp_event(&self, params: TspEventParams) -> Outcome {
        let id = params.session_id;
        let Some(session) = self.hub.sender(id) else {
            return self.not_live(id);
        };
        let (reply, done) = oneshot::channel();
        let command = Command::TspEvent {
            surface: params.surface,
            event: params.event,
            reply,
        };
        if session.send(command).await.is_err() {
            return self.not_live(id);
        }
        match done.await {
            Ok(Ok(())) => ok(&()),
            Ok(Err(message)) => Outcome::error(ErrorCode::InvalidRequest, message),
            Err(_) => self.not_live(id),
        }
    }
}
