//! Task board rules (SPEC §5): how a task's status follows its session, and where a dropped
//! card goes in its column.
//!
//! Status rules apply only while the task is active (`running` / `needs_input`): `todo`,
//! `review` and `done` are where the user (or a finished run) put the card, and a late session
//! event must not pull it back out.

use mkrew_protocol::{SessionState, TaskStatus};

/// The task's new `(status, error_flag)` after its session moved to `state`; `None` = leave the
/// task alone. `exit_code` is the session's, read only for [`SessionState::Exited`] (`None` =
/// killed or unknown, which counts as a failure).
pub fn on_session_state(
    status: TaskStatus,
    state: SessionState,
    exit_code: Option<i32>,
) -> Option<(TaskStatus, bool)> {
    // Run pressed: the task's session starts, whatever column the card was in (also when it
    // already shows running, so a flag left by an earlier run is cleared).
    if state == SessionState::Starting {
        return Some((TaskStatus::Running, false));
    }
    if !matches!(status, TaskStatus::Running | TaskStatus::NeedsInput) {
        return None;
    }
    let next = match state {
        SessionState::NeedsInput => (TaskStatus::NeedsInput, false),
        SessionState::Working => (TaskStatus::Running, false),
        SessionState::Done => (TaskStatus::Review, false),
        SessionState::Exited if exit_code == Some(0) => (TaskStatus::Review, false),
        SessionState::Exited => (TaskStatus::NeedsInput, true),
        SessionState::Starting | SessionState::Idle | SessionState::Unknown => return None,
    };
    // Exiting non-zero from needs_input keeps the column but sets the flag.
    (next.0 != status || next.1).then_some(next)
}

/// The status after the user approves the task: only a card in review can be approved.
pub fn approve(status: TaskStatus) -> Option<TaskStatus> {
    (status == TaskStatus::Review).then_some(TaskStatus::Done)
}

/// A position strictly between the neighbours of a drop slot (`None` = no card on that side);
/// `None` when the gap has collapsed and the column needs [`renumbered`] positions.
pub fn position_between(prev: Option<f64>, next: Option<f64>) -> Option<f64> {
    let position = match (prev, next) {
        (None, None) => 0.0,
        (Some(prev), None) => prev + 1.0,
        (None, Some(next)) => next - 1.0,
        (Some(prev), Some(next)) => prev + (next - prev) / 2.0,
    };
    let after_prev = prev.is_none_or(|prev| position > prev);
    let before_next = next.is_none_or(|next| position < next);
    (position.is_finite() && after_prev && before_next).then_some(position)
}

/// Fresh positions `0, 1, 2, …` for a column of `count` cards, in board order.
pub fn renumbered(count: usize) -> Vec<f64> {
    (0..count).map(|index| index as f64).collect()
}

/// Where a card dropped into a column goes.
#[derive(Debug, Clone, PartialEq)]
pub enum Placement {
    /// Only the dropped card moves, to this position.
    At(f64),
    /// The gap collapsed: every card of the column (dropped card included, in board order)
    /// gets the position at its index.
    Renumber(Vec<f64>),
}

/// Placement of a card dropped at `index` among `column`, the ascending positions of the
/// column's other cards (the dragged card left out). `index` past the end = last.
pub fn place(column: &[f64], index: usize) -> Placement {
    let index = index.min(column.len());
    let prev = index.checked_sub(1).map(|prev| column[prev]);
    let next = column.get(index).copied();
    match position_between(prev, next) {
        Some(position) => Placement::At(position),
        None => Placement::Renumber(renumbered(column.len() + 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SessionState as S;
    use TaskStatus as T;

    #[test]
    fn run_starts_the_task() {
        for status in [T::Todo, T::Running, T::NeedsInput, T::Review, T::Done] {
            assert_eq!(
                on_session_state(status, S::Starting, None),
                Some((T::Running, false))
            );
        }
    }

    #[test]
    fn session_needs_input_moves_the_task() {
        assert_eq!(
            on_session_state(T::Running, S::NeedsInput, None),
            Some((T::NeedsInput, false))
        );
        assert_eq!(on_session_state(T::NeedsInput, S::NeedsInput, None), None);
    }

    #[test]
    fn session_working_again_resumes_the_task() {
        assert_eq!(
            on_session_state(T::NeedsInput, S::Working, None),
            Some((T::Running, false))
        );
        assert_eq!(on_session_state(T::Running, S::Working, None), None);
    }

    #[test]
    fn finished_session_sends_the_task_to_review() {
        for status in [T::Running, T::NeedsInput] {
            assert_eq!(
                on_session_state(status, S::Done, None),
                Some((T::Review, false))
            );
            assert_eq!(
                on_session_state(status, S::Exited, Some(0)),
                Some((T::Review, false))
            );
        }
    }

    #[test]
    fn failed_exit_flags_the_task() {
        for status in [T::Running, T::NeedsInput] {
            assert_eq!(
                on_session_state(status, S::Exited, Some(1)),
                Some((T::NeedsInput, true))
            );
            assert_eq!(
                on_session_state(status, S::Exited, Some(-1)),
                Some((T::NeedsInput, true))
            );
            assert_eq!(
                on_session_state(status, S::Exited, None),
                Some((T::NeedsInput, true))
            );
        }
    }

    #[test]
    fn quiet_states_leave_the_task_alone() {
        for status in [T::Running, T::NeedsInput] {
            for state in [S::Idle, S::Unknown] {
                assert_eq!(on_session_state(status, state, None), None);
            }
        }
    }

    #[test]
    fn cards_the_user_placed_ignore_late_session_events() {
        for status in [T::Todo, T::Review, T::Done] {
            for state in [S::Working, S::NeedsInput, S::Idle, S::Done, S::Unknown] {
                assert_eq!(on_session_state(status, state, None), None);
            }
            assert_eq!(on_session_state(status, S::Exited, Some(0)), None);
            assert_eq!(on_session_state(status, S::Exited, Some(2)), None);
        }
    }

    #[test]
    fn only_review_can_be_approved() {
        assert_eq!(approve(T::Review), Some(T::Done));
        for status in [T::Todo, T::Running, T::NeedsInput, T::Done] {
            assert_eq!(approve(status), None);
        }
    }

    #[test]
    fn positions_between_neighbours() {
        assert_eq!(position_between(None, None), Some(0.0));
        assert_eq!(position_between(Some(3.0), None), Some(4.0));
        assert_eq!(position_between(None, Some(3.0)), Some(2.0));
        assert_eq!(position_between(Some(1.0), Some(2.0)), Some(1.5));
        assert_eq!(position_between(Some(-2.0), Some(-1.0)), Some(-1.5));
    }

    #[test]
    fn collapsed_gap_has_no_position() {
        let prev = 1.0_f64;
        let next = prev.next_up();
        assert_eq!(position_between(Some(prev), Some(next)), None);
        assert_eq!(position_between(Some(prev), Some(prev)), None);
        // Too large for `+ 1.0` to move the value.
        assert_eq!(position_between(Some(1e17), None), None);
        assert_eq!(position_between(None, Some(-1e17)), None);
        assert_eq!(position_between(Some(f64::MAX), None), None);
    }

    #[test]
    fn repeated_drops_in_one_gap_end_in_a_renumber() {
        let mut column = vec![0.0, 1.0];
        let mut drops = 0;
        loop {
            match place(&column, 1) {
                Placement::At(position) => {
                    assert!(column[0] < position && position < column[1]);
                    column[1] = position;
                    drops += 1;
                }
                Placement::Renumber(positions) => {
                    assert_eq!(positions, vec![0.0, 1.0, 2.0]);
                    break;
                }
            }
        }
        assert!(drops > 40, "only {drops} drops before renumbering");
    }

    #[test]
    fn place_at_the_ends_and_in_empty_columns() {
        assert_eq!(place(&[], 0), Placement::At(0.0));
        assert_eq!(place(&[], 5), Placement::At(0.0));
        assert_eq!(place(&[2.0, 4.0], 0), Placement::At(1.0));
        assert_eq!(place(&[2.0, 4.0], 1), Placement::At(3.0));
        assert_eq!(place(&[2.0, 4.0], 2), Placement::At(5.0));
        assert_eq!(place(&[2.0, 4.0], 9), Placement::At(5.0));
    }

    #[test]
    fn renumbered_is_board_order() {
        assert_eq!(renumbered(0), Vec::<f64>::new());
        assert_eq!(renumbered(3), vec![0.0, 1.0, 2.0]);
    }
}
