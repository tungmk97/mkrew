//! Routes incoming session frames (SPEC §7, §10): output, snapshots and exits go to the pane of
//! an attached session, are buffered while its `session.attach` is in flight, and are dropped
//! for every other session. Pure and tested headlessly; the window owns the panes.

use std::collections::{HashMap, HashSet};

use mkrew_protocol::{Event, ScreenSnapshot};
use ulid::Ulid;

use crate::client::Incoming;

/// What arrived for a session between its `session.attach` request and reply.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Buffered {
    /// A newer screen than the attach reply's (the daemon's output queue overflowed).
    pub snapshot: Option<ScreenSnapshot>,
    /// Output after `snapshot` (or after the reply's snapshot).
    pub output: Vec<u8>,
    /// `Some(exit_code)` if the session exited meanwhile.
    pub exited: Option<Option<i32>>,
}

/// A frame for the pane of an attached session.
#[derive(Debug, Clone, PartialEq)]
pub enum Delivery {
    Output {
        session_id: Ulid,
        data: Vec<u8>,
    },
    Snapshot {
        session_id: Ulid,
        snapshot: ScreenSnapshot,
    },
    Exited {
        session_id: Ulid,
        exit_code: Option<i32>,
    },
}

impl Delivery {
    pub fn session_id(&self) -> Ulid {
        match self {
            Delivery::Output { session_id, .. }
            | Delivery::Snapshot { session_id, .. }
            | Delivery::Exited { session_id, .. } => *session_id,
        }
    }
}

/// A `session.attach` in flight.
#[derive(Debug)]
struct Attaching {
    generation: u64,
    buffered: Buffered,
    /// Hidden before the reply: the reply detaches unless it is wanted again first.
    cancelled: bool,
}

#[derive(Debug, Default)]
pub struct Router {
    /// Sessions with a pane.
    shown: HashSet<Ulid>,
    attaching: HashMap<Ulid, Attaching>,
    /// Generation of the last attach begun (tells replies from before a [`Router::clear`]).
    generation: u64,
}

impl Router {
    /// The session has a pane or a wanted attach in flight.
    pub fn is_wanted(&self, session: Ulid) -> bool {
        self.shown.contains(&session)
            || self
                .attaching
                .get(&session)
                .is_some_and(|attaching| !attaching.cancelled)
    }

    /// Sessions with a pane or a wanted attach in flight.
    pub fn wanted(&self) -> Vec<Ulid> {
        let attaching = self
            .attaching
            .iter()
            .filter(|(_, attaching)| !attaching.cancelled)
            .map(|(id, _)| id);
        self.shown.iter().chain(attaching).copied().collect()
    }

    /// Starts buffering for a `session.attach` about to be sent; returns its generation, or
    /// `None` if none is needed: the session is shown, being attached, or its cancelled attach
    /// is still in flight (it is wanted again and that reply will show it, so a second attach
    /// cannot replay output already in its snapshot).
    pub fn begin_attach(&mut self, session: Ulid) -> Option<u64> {
        if self.shown.contains(&session) {
            return None;
        }
        if let Some(attaching) = self.attaching.get_mut(&session) {
            attaching.cancelled = false;
            return None;
        }
        self.generation += 1;
        let attaching = Attaching {
            generation: self.generation,
            buffered: Buffered::default(),
            cancelled: false,
        };
        self.attaching.insert(session, attaching);
        Some(self.generation)
    }

    /// The `session.attach` of `generation` succeeded: the session is shown from now on and the
    /// frames buffered meanwhile are returned. `None` if the attach was cancelled (the caller
    /// detaches) or predates a [`Router::clear`].
    pub fn finish_attach(&mut self, session: Ulid, generation: u64) -> Option<Buffered> {
        let attaching = self.take(session, generation)?;
        if attaching.cancelled {
            return None;
        }
        self.shown.insert(session);
        Some(attaching.buffered)
    }

    /// The `session.attach` of `generation` failed; true if the session is still wanted (the
    /// caller shows the error).
    pub fn fail_attach(&mut self, session: Ulid, generation: u64) -> bool {
        self.take(session, generation)
            .is_some_and(|attaching| !attaching.cancelled)
    }

    fn take(&mut self, session: Ulid, generation: u64) -> Option<Attaching> {
        match self.attaching.get(&session) {
            Some(attaching) if attaching.generation == generation => {
                self.attaching.remove(&session)
            }
            _ => None,
        }
    }

    /// Stops routing frames to `session`. Returns true if it was shown, so the caller detaches
    /// it now (a cancelled attach detaches when its reply arrives).
    pub fn forget(&mut self, session: Ulid) -> bool {
        if let Some(attaching) = self.attaching.get_mut(&session) {
            attaching.cancelled = true;
        }
        self.shown.remove(&session)
    }

    /// Forgets every session (the connection closed).
    pub fn clear(&mut self) {
        self.shown.clear();
        self.attaching.clear();
    }

    /// Routes one incoming frame: a delivery for a shown pane, or `None` when it was buffered
    /// for an attach in flight, is not session content, or is for a session nobody shows.
    pub fn route(&mut self, incoming: Incoming) -> Option<Delivery> {
        let delivery = match incoming {
            Incoming::Output { session_id, data } => Delivery::Output { session_id, data },
            Incoming::Event(Event::SessionSnapshot(event)) => Delivery::Snapshot {
                session_id: event.session_id,
                snapshot: event.snapshot,
            },
            Incoming::Event(Event::SessionExited(event)) => Delivery::Exited {
                session_id: event.session_id,
                exit_code: event.exit_code,
            },
            Incoming::Event(_) => return None,
        };
        let session_id = delivery.session_id();
        if self.shown.contains(&session_id) {
            return Some(delivery);
        }
        // Cancelled attaches keep buffering: they may be wanted again before their reply.
        if let Some(Attaching { buffered, .. }) = self.attaching.get_mut(&session_id) {
            match delivery {
                Delivery::Output { data, .. } => buffered.output.extend_from_slice(&data),
                Delivery::Snapshot { snapshot, .. } => {
                    buffered.snapshot = Some(snapshot);
                    buffered.output.clear();
                }
                Delivery::Exited { exit_code, .. } => buffered.exited = Some(exit_code),
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{
        CursorSnapshot, SessionExitedEvent, SessionSnapshotEvent, SessionState, SessionStateEvent,
        StateSource, TermModes,
    };

    use super::*;

    fn snapshot(cols: u16) -> ScreenSnapshot {
        ScreenSnapshot {
            cols,
            rows: 1,
            lines: vec![],
            history: vec![],
            cursor: CursorSnapshot {
                row: 0,
                col: 0,
                visible: true,
            },
            modes: TermModes::default(),
        }
    }

    fn output(session_id: Ulid, data: &[u8]) -> Incoming {
        Incoming::Output {
            session_id,
            data: data.to_vec(),
        }
    }

    fn snapshot_event(session_id: Ulid, cols: u16) -> Incoming {
        Incoming::Event(Event::SessionSnapshot(SessionSnapshotEvent {
            session_id,
            snapshot: snapshot(cols),
        }))
    }

    fn exited(session_id: Ulid, exit_code: Option<i32>) -> Incoming {
        Incoming::Event(Event::SessionExited(SessionExitedEvent {
            session_id,
            exit_code,
        }))
    }

    #[test]
    fn frames_go_to_shown_panes_only() {
        let [shown, other] = [Ulid::new(), Ulid::new()];
        let mut router = Router::default();
        let generation = router.begin_attach(shown).unwrap();
        assert_eq!(
            router.finish_attach(shown, generation),
            Some(Buffered::default())
        );

        assert_eq!(
            router.route(output(shown, b"hi")),
            Some(Delivery::Output {
                session_id: shown,
                data: b"hi".to_vec()
            })
        );
        assert_eq!(
            router.route(snapshot_event(shown, 7)),
            Some(Delivery::Snapshot {
                session_id: shown,
                snapshot: snapshot(7)
            })
        );
        assert_eq!(
            router.route(exited(shown, Some(3))),
            Some(Delivery::Exited {
                session_id: shown,
                exit_code: Some(3)
            })
        );
        assert_eq!(router.route(output(other, b"x")), None);
        assert_eq!(router.route(exited(other, None)), None);
        let state = Incoming::Event(Event::SessionState(SessionStateEvent {
            session_id: shown,
            state: SessionState::Idle,
            source: StateSource::Screen,
            changed_at: 1,
        }));
        assert_eq!(router.route(state), None);
    }

    #[test]
    fn frames_before_the_attach_reply_are_buffered() {
        let session = Ulid::new();
        let mut router = Router::default();
        let generation = router.begin_attach(session).unwrap();
        assert_eq!(router.begin_attach(session), None);

        assert_eq!(router.route(output(session, b"lost")), None);
        // A snapshot replaces everything before it.
        router.route(snapshot_event(session, 9));
        router.route(output(session, b"a"));
        router.route(output(session, b"b"));
        router.route(exited(session, Some(0)));

        let buffered = router.finish_attach(session, generation).unwrap();
        assert_eq!(buffered.snapshot, Some(snapshot(9)));
        assert_eq!(buffered.output, b"ab");
        assert_eq!(buffered.exited, Some(Some(0)));
        assert!(router.is_wanted(session));
        assert_eq!(router.begin_attach(session), None);
    }

    #[test]
    fn cancelled_attach_reply_detaches_unless_wanted_again() {
        let session = Ulid::new();
        let mut router = Router::default();
        let generation = router.begin_attach(session).unwrap();
        // Hidden before the reply: no detach yet (the reply handler does it).
        assert!(!router.forget(session));
        assert!(!router.is_wanted(session));
        assert!(router.wanted().is_empty());
        router.route(output(session, b"a"));
        assert_eq!(router.finish_attach(session, generation), None);
        assert!(!router.is_wanted(session));

        // Hidden and shown again (A → B → A) before the reply: no second attach is sent (its
        // snapshot would already hold output buffered for the first), the first reply shows it.
        let generation = router.begin_attach(session).unwrap();
        router.route(output(session, b"b"));
        router.forget(session);
        router.route(output(session, b"c"));
        assert_eq!(router.begin_attach(session), None);
        assert!(router.is_wanted(session));
        let buffered = router.finish_attach(session, generation).unwrap();
        assert_eq!(buffered.output, b"bc");
    }

    #[test]
    fn failed_attach_reports_whether_it_is_still_wanted() {
        let [a, b] = [Ulid::new(), Ulid::new()];
        let mut router = Router::default();
        let first = router.begin_attach(a).unwrap();
        assert!(router.fail_attach(a, first));
        assert!(!router.is_wanted(a));
        let second = router.begin_attach(b).unwrap();
        router.forget(b);
        assert!(!router.fail_attach(b, second));
        // Replies from before a `clear` (reconnect) match nothing.
        let stale = router.begin_attach(a).unwrap();
        router.clear();
        let current = router.begin_attach(a).unwrap();
        assert_eq!(router.finish_attach(a, stale), None);
        assert!(router.is_wanted(a));
        assert!(router.finish_attach(a, current).is_some());
    }

    #[test]
    fn forget_reports_shown_sessions_for_detaching() {
        let [a, b] = [Ulid::new(), Ulid::new()];
        let mut router = Router::default();
        let generation = router.begin_attach(a).unwrap();
        router.finish_attach(a, generation);
        router.begin_attach(b);
        let mut wanted = router.wanted();
        wanted.sort();
        let mut expected = vec![a, b];
        expected.sort();
        assert_eq!(wanted, expected);

        assert!(router.forget(a));
        assert!(!router.forget(a));
        assert_eq!(router.route(output(a, b"x")), None);
        router.clear();
        assert!(router.wanted().is_empty());
    }
}
