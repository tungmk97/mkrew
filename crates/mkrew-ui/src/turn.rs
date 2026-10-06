//! Turn summaries (SPEC §20A.3): the last `session.turn` of each session and the status text
//! of a `done` session, `done · 3 files · 4m`, shown on its Inbox row and pane header. Pure;
//! `main.rs` renders the text.

use std::collections::HashMap;

use mkrew_protocol::{Event, SessionState, SessionTurnEvent};
use ulid::Ulid;

use crate::status;

/// The latest turn of each session, from `session.turn` events.
#[derive(Debug, Default)]
pub struct Turns {
    last: HashMap<Ulid, SessionTurnEvent>,
}

impl Turns {
    /// Keeps a `session.turn`; returns whether the shown summaries changed. The same turn can
    /// end twice (needs_input → resumed → ends again): the later event wins; an older turn
    /// never replaces a newer one (`n` only grows: a session never outlives its daemon).
    pub fn apply(&mut self, event: &Event) -> bool {
        let Event::SessionTurn(turn) = event else {
            return false;
        };
        if self
            .last
            .get(&turn.session_id)
            .is_some_and(|last| last.n > turn.n)
        {
            return false;
        }
        self.last.insert(turn.session_id, turn.clone());
        true
    }

    pub fn get(&self, session_id: Ulid) -> Option<&SessionTurnEvent> {
        self.last.get(&session_id)
    }
}

/// `3 files · 4m`: the files the turn touched (left out when none) and how long it took.
pub fn summary(turn: &SessionTurnEvent) -> String {
    let took = status::format_duration(turn.ended_at - turn.started_at);
    match turn.files.len() {
        0 => took,
        1 => format!("1 file · {took}"),
        n => format!("{n} files · {took}"),
    }
}

/// [`status::text`], followed by the last turn's [`summary`] while the session is `done`.
pub fn status_text(
    state: SessionState,
    exit_code: Option<i32>,
    changed_at: i64,
    now_ms: i64,
    turn: Option<&SessionTurnEvent>,
) -> String {
    let text = status::text(state, exit_code, changed_at, now_ms);
    match turn {
        Some(turn) if state == SessionState::Done => format!("{text} · {}", summary(turn)),
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::TurnFile;

    use super::*;

    const SEC: i64 = 1000;

    fn turn(session_id: Ulid, n: u32, files: usize, took: i64) -> SessionTurnEvent {
        SessionTurnEvent {
            session_id,
            n,
            started_at: 1_000 * SEC,
            ended_at: 1_000 * SEC + took,
            files: (0..files)
                .map(|i| TurnFile {
                    path: format!("src/f{i}.rs"),
                    added: None,
                    removed: None,
                })
                .collect(),
        }
    }

    #[test]
    fn summaries_count_files_and_time() {
        let id = Ulid::new();
        let lines: Vec<String> = [
            (0, 12 * SEC),
            (1, 59 * SEC),
            (3, 4 * 60 * SEC),
            (12, 65 * 60 * SEC),
        ]
        .into_iter()
        .map(|(files, took)| summary(&turn(id, 1, files, took)))
        .collect();
        insta::assert_snapshot!(lines.join("\n"));
    }

    #[test]
    fn only_done_shows_the_turn() {
        let id = Ulid::new();
        let last = turn(id, 1, 3, 4 * 60 * SEC);
        let now = 2_000 * SEC;
        assert_eq!(
            status_text(SessionState::Done, None, 0, now, Some(&last)),
            "done · 3 files · 4m"
        );
        assert_eq!(status_text(SessionState::Done, None, 0, now, None), "done");
        assert_eq!(
            status_text(SessionState::NeedsInput, None, 0, now, Some(&last)),
            "needs input"
        );
        assert_eq!(
            status_text(SessionState::Exited, Some(1), 0, now, Some(&last)),
            "exited (1)"
        );
    }

    #[test]
    fn the_latest_turn_wins() {
        let (a, b) = (Ulid::new(), Ulid::new());
        let mut turns = Turns::default();
        assert!(
            !turns.apply(&Event::TaskDeleted(mkrew_protocol::TaskDeletedEvent {
                task_id: a,
                project_id: b,
            }))
        );

        assert!(turns.apply(&Event::SessionTurn(turn(a, 1, 1, SEC))));
        // The same turn ends again after a resume: later wins.
        assert!(turns.apply(&Event::SessionTurn(turn(a, 1, 2, SEC))));
        assert_eq!(turns.get(a).map(|t| t.files.len()), Some(2));
        assert!(turns.apply(&Event::SessionTurn(turn(a, 2, 0, SEC))));
        // A late older turn is ignored.
        assert!(!turns.apply(&Event::SessionTurn(turn(a, 1, 5, SEC))));
        assert_eq!(turns.get(a).map(|t| t.n), Some(2));
        assert_eq!(turns.get(b), None);
    }
}
