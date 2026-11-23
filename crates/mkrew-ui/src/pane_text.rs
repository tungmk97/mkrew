//! Text of a pane's status (SPEC §11): the status line of a tab chip's tooltip. Pure and tested
//! headlessly; `main.rs` draws it.

use mkrew_protocol::{SessionState, SessionTurnEvent};

use crate::turn;

/// The status line: [`turn::status_text`] (`working · 1h 25m`, `done · 3 files · 4m`) with a
/// capital first letter.
pub fn pill_text(
    state: SessionState,
    exit_code: Option<i32>,
    changed_at: i64,
    now_ms: i64,
    turn: Option<&SessionTurnEvent>,
) -> String {
    let text = turn::status_text(state, exit_code, changed_at, now_ms, turn);
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{SessionState, SessionTurnEvent, TurnFile};
    use ulid::Ulid;

    use super::*;

    const SEC: i64 = 1000;

    #[test]
    fn the_pill_names_the_state_and_how_long_it_lasts() {
        let now = 10_000 * SEC;
        let since = now - (85 * 60 * SEC);
        assert_eq!(
            pill_text(SessionState::Working, None, since, now, None),
            "Working · 1h 25m"
        );
        assert_eq!(
            pill_text(SessionState::NeedsInput, None, since, now, None),
            "Needs input"
        );
        assert_eq!(
            pill_text(SessionState::Exited, Some(1), since, now, None),
            "Exited (1)"
        );
        // A finished turn keeps its summary (SPEC §20A.3).
        let turn = SessionTurnEvent {
            session_id: Ulid::new(),
            n: 1,
            started_at: 0,
            ended_at: 4 * 60 * SEC,
            files: vec![TurnFile {
                path: "a.rs".into(),
                added: None,
                removed: None,
            }],
        };
        assert_eq!(
            pill_text(SessionState::Done, None, since, now, Some(&turn)),
            "Done · 1 file · 4m"
        );
    }
}
