//! Turns (SPEC §20.0): one request→answer cycle of an agent session.
//!
//! A turn starts when the session enters `working` from any other state (the spec lists
//! `starting`, `idle`, `done`, `needs_input`; `unknown` only follows `starting`, so it counts
//! like it) and ends at the next
//! `done`, `idle`, `needs_input` or `exited`. A `needs_input` → `working` resume (an approval)
//! reopens the turn that `needs_input` ended, with the same number, start and files. Turns are
//! numbered per session from 1. The daemon's session actor owns one [`TurnTracker`].

use std::collections::BTreeSet;

use mkrew_protocol::SessionState;

/// One turn. Times are the caller's clock (unix ms in the daemon).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    pub n: u32,
    pub started_at: i64,
    /// `None` while the turn is open.
    pub ended_at: Option<i64>,
    /// Paths attributed to the session during the turn, deduplicated and sorted.
    pub files: BTreeSet<String>,
}

/// What a state change did to the turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnEvent {
    Started(u32),
    /// `needs_input` → `working`: the turn ended by `needs_input` continues.
    Resumed(u32),
    /// The turn as it ended (`ended_at` set). A resumed turn can end again, with the same `n`.
    Ended(Turn),
}

#[derive(Debug, Default)]
pub struct TurnTracker {
    state: Option<SessionState>,
    count: u32,
    turn: Option<Turn>,
    /// The last turn ended at `needs_input` and the session has not left it since.
    resumable: bool,
}

impl TurnTracker {
    /// The session entered `state` at `at`.
    pub fn on_state(&mut self, state: SessionState, at: i64) -> Option<TurnEvent> {
        let prev = self.state.replace(state);
        if prev == Some(state) {
            return None;
        }
        let resumable = std::mem::take(&mut self.resumable);
        match state {
            SessionState::Working => match &mut self.turn {
                Some(turn) if turn.ended_at.is_none() => None,
                Some(turn) if resumable && prev == Some(SessionState::NeedsInput) => {
                    turn.ended_at = None;
                    Some(TurnEvent::Resumed(turn.n))
                }
                _ => {
                    self.count += 1;
                    self.turn = Some(Turn {
                        n: self.count,
                        started_at: at,
                        ended_at: None,
                        files: BTreeSet::new(),
                    });
                    Some(TurnEvent::Started(self.count))
                }
            },
            SessionState::Done
            | SessionState::Idle
            | SessionState::NeedsInput
            | SessionState::Exited => {
                let turn = self.turn.as_mut().filter(|t| t.ended_at.is_none())?;
                turn.ended_at = Some(at);
                self.resumable = state == SessionState::NeedsInput;
                Some(TurnEvent::Ended(turn.clone()))
            }
            SessionState::Starting | SessionState::Unknown => None,
        }
    }

    /// Attributes `paths` to the open turn; ignored when no turn is open.
    pub fn add_files<I>(&mut self, paths: I)
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        if let Some(turn) = self.turn.as_mut().filter(|t| t.ended_at.is_none()) {
            turn.files.extend(paths.into_iter().map(Into::into));
        }
    }

    /// The current turn (open, or the last one that ended).
    pub fn turn(&self) -> Option<&Turn> {
        self.turn.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SessionState::*;

    /// Feeds `states` at times 1, 2, 3, … and returns the events.
    fn run(tracker: &mut TurnTracker, states: &[SessionState]) -> Vec<Option<TurnEvent>> {
        states
            .iter()
            .enumerate()
            .map(|(i, &s)| tracker.on_state(s, i as i64 + 1))
            .collect()
    }

    fn ended(n: u32, started_at: i64, ended_at: i64, files: &[&str]) -> Option<TurnEvent> {
        Some(TurnEvent::Ended(Turn {
            n,
            started_at,
            ended_at: Some(ended_at),
            files: files.iter().map(|f| f.to_string()).collect(),
        }))
    }

    #[test]
    fn turns_start_on_working_and_end_on_done_idle_or_exit() {
        let mut t = TurnTracker::default();
        let events = run(
            &mut t,
            &[Starting, Working, Done, Working, Idle, Working, Exited],
        );
        assert_eq!(
            events,
            [
                None,
                Some(TurnEvent::Started(1)),
                ended(1, 2, 3, &[]),
                Some(TurnEvent::Started(2)),
                ended(2, 4, 5, &[]),
                Some(TurnEvent::Started(3)),
                ended(3, 6, 7, &[]),
            ]
        );
    }

    #[test]
    fn approval_resumes_the_same_turn() {
        let mut t = TurnTracker::default();
        let events = run(&mut t, &[Working, NeedsInput, Working, Done]);
        assert_eq!(
            events,
            [
                Some(TurnEvent::Started(1)),
                ended(1, 1, 2, &[]),
                Some(TurnEvent::Resumed(1)),
                ended(1, 1, 4, &[]),
            ]
        );
    }

    #[test]
    fn needs_input_without_an_open_turn_starts_a_new_one() {
        // Done ended turn 1; a later needs_input → working is a new request, not a resume.
        let mut t = TurnTracker::default();
        let events = run(&mut t, &[Working, Done, NeedsInput, Working]);
        assert_eq!(events[2], None);
        assert_eq!(events[3], Some(TurnEvent::Started(2)));
        // Leaving needs_input for another state also drops the resume.
        let mut t = TurnTracker::default();
        let events = run(&mut t, &[Working, NeedsInput, Unknown, Working]);
        assert_eq!(events[3], Some(TurnEvent::Started(2)));
        // A startup prompt (e.g. folder trust) before any turn: the answer starts turn 1.
        let mut t = TurnTracker::default();
        let events = run(&mut t, &[Starting, NeedsInput, Working]);
        assert_eq!(events[2], Some(TurnEvent::Started(1)));
    }

    #[test]
    fn a_repeated_needs_input_still_resumes() {
        let mut t = TurnTracker::default();
        let events = run(&mut t, &[Working, NeedsInput, NeedsInput, Working]);
        assert_eq!(events[3], Some(TurnEvent::Resumed(1)));
    }

    #[test]
    fn repeated_and_unknown_states_do_not_split_a_turn() {
        let mut t = TurnTracker::default();
        let events = run(&mut t, &[Working, Working, Unknown, Working, Done, Done]);
        assert_eq!(
            events,
            [
                Some(TurnEvent::Started(1)),
                None,
                None,
                None,
                ended(1, 1, 5, &[]),
                None,
            ]
        );
    }

    #[test]
    fn states_without_a_turn_emit_nothing() {
        let mut t = TurnTracker::default();
        let events = run(&mut t, &[Starting, Idle, Done, Exited]);
        assert!(events.iter().all(Option::is_none), "{events:?}");
        assert!(t.turn().is_none());
    }

    #[test]
    fn files_are_deduplicated_and_kept_across_a_resume() {
        let mut t = TurnTracker::default();
        t.add_files(["before.rs"]); // no turn yet: ignored
        t.on_state(Working, 1);
        t.add_files(["b.rs", "a.rs"]);
        t.add_files(vec!["a.rs".to_string()]);
        t.on_state(NeedsInput, 2);
        t.add_files(["paused.rs"]); // turn not open: ignored
        t.on_state(Working, 3);
        t.add_files(["c.rs"]);
        assert_eq!(
            t.on_state(Done, 4),
            ended(1, 1, 4, &["a.rs", "b.rs", "c.rs"])
        );
        t.add_files(["late.rs"]);
        assert_eq!(t.on_state(Working, 5), Some(TurnEvent::Started(2)));
        assert!(t.turn().unwrap().files.is_empty());
    }
}
