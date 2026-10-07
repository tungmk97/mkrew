//! Follow in Zed (SPEC §20C.5): while on for a session, each `session.edited` of it opens the
//! edited file in Zed, debounced by `zed.follow_debounce_ms` (the last path wins: a new edit
//! restarts the wait). Off by default; kept in UI memory only. Pure; `main.rs` runs the timer
//! and `zed`.

use std::collections::HashMap;
use std::time::Duration;

use mkrew_protocol::Event;
use ulid::Ulid;

/// The file to open: project-relative path (`/` separators) and the edit's line, if known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub path: String,
    pub line: Option<u32>,
}

/// A followed session: its wait and the latest edit not opened yet.
#[derive(Debug)]
struct Followed {
    delay: Duration,
    pending: Option<(u64, Edit)>,
}

/// An edit waiting for its delay: once `delay` passed, `due(session_id, serial)` says whether
/// it is still the last one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wait {
    pub session_id: Ulid,
    pub serial: u64,
    pub delay: Duration,
}

/// The sessions followed in Zed.
#[derive(Debug, Default)]
pub struct Follow {
    sessions: HashMap<Ulid, Followed>,
    serial: u64,
}

impl Follow {
    pub fn is_on(&self, session_id: Ulid) -> bool {
        self.sessions.contains_key(&session_id)
    }

    /// Turns following `session_id` on (waiting `debounce_ms` after each edit) or off (dropping
    /// a waiting edit); returns whether it is now on.
    pub fn toggle(&mut self, session_id: Ulid, debounce_ms: u64) -> bool {
        if self.sessions.remove(&session_id).is_some() {
            return false;
        }
        let followed = Followed {
            delay: Duration::from_millis(debounce_ms),
            pending: None,
        };
        self.sessions.insert(session_id, followed);
        true
    }

    /// Keeps a `session.edited` of a followed session as its waiting edit (replacing an older
    /// one) and returns the wait to start; `None` for other events and sessions.
    pub fn apply(&mut self, event: &Event) -> Option<Wait> {
        let Event::SessionEdited(edited) = event else {
            return None;
        };
        let followed = self.sessions.get_mut(&edited.session_id)?;
        self.serial += 1;
        let edit = Edit {
            path: edited.path.clone(),
            line: edited.line,
        };
        followed.pending = Some((self.serial, edit));
        Some(Wait {
            session_id: edited.session_id,
            serial: self.serial,
            delay: followed.delay,
        })
    }

    /// After a wait: the edit to open, if no later edit came and following is still on.
    pub fn due(&mut self, wait: Wait) -> Option<Edit> {
        let followed = self.sessions.get_mut(&wait.session_id)?;
        if followed
            .pending
            .as_ref()
            .is_none_or(|(serial, _)| *serial != wait.serial)
        {
            return None;
        }
        followed.pending.take().map(|(_, edit)| edit)
    }
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{SessionEditedEvent, SessionExitedEvent};

    use super::*;

    fn edited(session_id: Ulid, path: &str, line: Option<u32>) -> Event {
        Event::SessionEdited(SessionEditedEvent {
            session_id,
            path: path.into(),
            line,
        })
    }

    fn edit(path: &str, line: Option<u32>) -> Edit {
        Edit {
            path: path.into(),
            line,
        }
    }

    #[test]
    fn it_is_off_by_default_and_toggles() {
        let mut follow = Follow::default();
        let session = Ulid::new();
        assert!(!follow.is_on(session));
        assert_eq!(follow.apply(&edited(session, "a.rs", None)), None);
        assert!(follow.toggle(session, 1500));
        assert!(follow.is_on(session));
        assert!(!follow.toggle(session, 1500));
        assert!(!follow.is_on(session));
    }

    #[test]
    fn a_lone_edit_opens_after_the_delay() {
        let mut follow = Follow::default();
        let session = Ulid::new();
        follow.toggle(session, 250);
        let wait = follow
            .apply(&edited(session, "src/a.rs", Some(7)))
            .expect("followed");
        assert_eq!(wait.delay, Duration::from_millis(250));
        assert_eq!(wait.session_id, session);
        assert_eq!(follow.due(wait), Some(edit("src/a.rs", Some(7))));
        // Opened once.
        assert_eq!(follow.due(wait), None);
    }

    #[test]
    fn the_last_path_wins() {
        let mut follow = Follow::default();
        let session = Ulid::new();
        follow.toggle(session, 1500);
        let first = follow.apply(&edited(session, "a.rs", Some(1))).unwrap();
        let second = follow.apply(&edited(session, "b.rs", None)).unwrap();
        assert_eq!(follow.due(first), None);
        assert_eq!(follow.due(second), Some(edit("b.rs", None)));
    }

    #[test]
    fn sessions_wait_independently() {
        let mut follow = Follow::default();
        let (one, two) = (Ulid::new(), Ulid::new());
        follow.toggle(one, 100);
        follow.toggle(two, 100);
        let a = follow.apply(&edited(one, "a.rs", None)).unwrap();
        let b = follow.apply(&edited(two, "b.rs", None)).unwrap();
        assert_eq!(follow.due(a), Some(edit("a.rs", None)));
        assert_eq!(follow.due(b), Some(edit("b.rs", None)));
    }

    #[test]
    fn turning_off_drops_a_waiting_edit() {
        let mut follow = Follow::default();
        let session = Ulid::new();
        follow.toggle(session, 100);
        let wait = follow.apply(&edited(session, "a.rs", None)).unwrap();
        follow.toggle(session, 100);
        assert_eq!(follow.due(wait), None);
        // Back on: the old wait stays stale.
        follow.toggle(session, 100);
        assert_eq!(follow.due(wait), None);
    }

    #[test]
    fn other_events_are_ignored() {
        let mut follow = Follow::default();
        let session = Ulid::new();
        follow.toggle(session, 100);
        let exited = Event::SessionExited(SessionExitedEvent {
            session_id: session,
            exit_code: Some(0),
        });
        assert_eq!(follow.apply(&exited), None);
    }
}
