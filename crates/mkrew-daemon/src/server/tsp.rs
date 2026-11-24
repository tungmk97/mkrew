//! `tsp.snapshot` and `tsp.event` (spec 2026-11-23-omp-tsp-surface-design 6.4): both go to the
//! session task, which owns the TSP engine.

use mkrew_protocol::{
    ErrorCode, MAX_FRAME_LEN, Outcome, TspEventParams, TspSnapshot, TspSnapshotParams,
};
use tokio::sync::oneshot;

use super::{Daemon, ok};
use crate::session::Command;

/// Room kept in the frame for the reply's own fields around the result.
const ENVELOPE: usize = 4096;

/// The snapshot if it fits `limit` bytes of JSON; else without its blobs (an `image` node whose blob
/// is missing draws its alt text); `None` when even that is too big.
fn fit_snapshot(mut snapshot: TspSnapshot, limit: usize) -> Option<TspSnapshot> {
    let fits = |s: &TspSnapshot| serde_json::to_vec(s).is_ok_and(|json| json.len() <= limit);
    if fits(&snapshot) {
        return Some(snapshot);
    }
    snapshot.blobs.clear();
    fits(&snapshot).then_some(snapshot)
}

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
            Ok(snapshot) => match fit_snapshot(snapshot, MAX_FRAME_LEN - ENVELOPE) {
                Some(snapshot) => ok(&snapshot),
                None => {
                    tracing::warn!(session_id = %id, "tsp snapshot over the frame limit");
                    Outcome::error(ErrorCode::Internal, "snapshot too large")
                }
            },
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

#[cfg(test)]
mod tests {
    use mkrew_protocol::{TspBlob, TspNode, TspState, TspSurface};

    use super::*;

    fn snapshot(text: usize, blob: usize) -> TspSnapshot {
        let mut root = TspNode {
            id: "s:1".into(),
            k: "col".into(),
            p: serde_json::Map::new(),
            c: Vec::new(),
        };
        root.p.insert("text".into(), "x".repeat(text).into());
        TspSnapshot {
            state: TspState::Live,
            surfaces: vec![TspSurface {
                id: "s:1".into(),
                mode: "inline".into(),
                role: None,
                title: None,
                read_only: false,
                seq: 1,
                root,
            }],
            blobs: vec![TspBlob {
                id: "b".into(),
                mime: "image/png".into(),
                data: vec![7; blob],
            }],
            reason: None,
        }
    }

    #[test]
    fn a_snapshot_over_the_limit_loses_its_blobs_and_then_is_refused() {
        let small = snapshot(10, 10);
        assert_eq!(fit_snapshot(small.clone(), 4096), Some(small));

        let heavy_blob = snapshot(10, 4000);
        let kept = fit_snapshot(heavy_blob.clone(), 1024).expect("fits without the blob");
        assert!(kept.blobs.is_empty());
        assert_eq!(kept.surfaces, heavy_blob.surfaces);

        assert_eq!(fit_snapshot(snapshot(2000, 10), 1024), None);
    }
}
