//! The "Turns…" dialog of an agent pane (SPEC §20C.2): the session's turns (`turn.list`), newest
//! first, each with when it ran, its files and +/− lines, "Diff since" (the Git view since the
//! turn's start tree, §20C.3) and "Restore before turn n" (the preview and confirm are the undo
//! dialog's). Pure and tested headlessly; `main.rs` renders it.

use gpui::Keystroke;
use mkrew_protocol::{Session, SessionState, Turn};
use ulid::Ulid;

use crate::status;

/// The turns being shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listing {
    Loading,
    Loaded(Vec<Turn>),
    Failed(String),
}

/// The open dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnsDialog {
    pub session_id: Ulid,
    /// The session's agent profile, e.g. `claude`.
    pub agent: String,
    pub listing: Listing,
    /// Index of the selected turn in the loaded list (↑ / ↓, Enter restores before it).
    pub selected: usize,
}

impl TurnsDialog {
    pub fn new(session: &Session) -> Self {
        Self {
            session_id: session.id,
            agent: session.agent_profile.clone(),
            listing: Listing::Loading,
            selected: 0,
        }
    }

    /// `turn.list` answered (oldest first): kept newest first, the newest selected.
    pub fn loaded(&mut self, mut turns: Vec<Turn>) {
        turns.reverse();
        self.listing = Listing::Loaded(turns);
        self.selected = 0;
    }

    /// Moves the selection by `offset`, clamped to the list.
    pub fn select_by(&mut self, offset: isize) {
        if let Listing::Loaded(turns) = &self.listing {
            let last = turns.len().saturating_sub(1);
            self.selected = self.selected.saturating_add_signed(offset).min(last);
        }
    }

    /// The selected turn's `n`.
    pub fn selected_turn(&self) -> Option<u32> {
        match &self.listing {
            Listing::Loaded(turns) => turns.get(self.selected).map(|turn| turn.n),
            _ => None,
        }
    }
}

/// One turn's line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub n: u32,
    /// `Turn 3`.
    pub title: String,
    /// `12m ago · took 4m`, or `started 2m ago · running`.
    pub when: String,
    /// `3 files`, `no files`.
    pub files: String,
    /// `+12 −3`; `None` when no file reported line counts.
    pub lines: Option<String>,
    /// Why "Restore before" is disabled, if it is.
    pub blocked: Option<&'static str>,
    /// Why "Diff since" is disabled, if it is.
    pub diff_blocked: Option<&'static str>,
}

/// The rows of `turns` (in the order given) at `now_ms`; `state` = the session's state.
pub fn rows(turns: &[Turn], state: SessionState, now_ms: i64) -> Vec<Row> {
    turns
        .iter()
        .map(|turn| {
            let when = match turn.ended_at {
                Some(ended) => format!(
                    "ended {} ago · took {}",
                    status::format_duration(now_ms - ended),
                    status::format_duration(ended - turn.started_at),
                ),
                None => format!(
                    "started {} ago · running",
                    status::format_duration(now_ms - turn.started_at)
                ),
            };
            let files = match turn.files.len() {
                0 => "no files".to_owned(),
                1 => "1 file".to_owned(),
                n => format!("{n} files"),
            };
            let counted = turn
                .files
                .iter()
                .any(|file| file.added.is_some() || file.removed.is_some());
            let sum = |count: fn(&mkrew_protocol::TurnFile) -> Option<u32>| {
                turn.files.iter().filter_map(count).sum::<u32>()
            };
            let lines = counted
                .then(|| format!("+{} −{}", sum(|file| file.added), sum(|file| file.removed)));
            Row {
                n: turn.n,
                title: format!("Turn {}", turn.n),
                when,
                files,
                lines,
                blocked: restore_blocked(turn, state),
                diff_blocked: diff_blocked(turn),
            }
        })
        .collect()
}

const NO_SNAPSHOT: &str = "No snapshot: the workspace is not a git repository.";

/// Why the folder cannot be restored to before `turn` now (the daemon refuses the same).
pub fn restore_blocked(turn: &Turn, state: SessionState) -> Option<&'static str> {
    if turn.start_tree.is_none() {
        Some(NO_SNAPSHOT)
    } else if state == SessionState::Working {
        Some("Wait until the agent finishes its turn.")
    } else {
        None
    }
}

/// Why the Git view cannot show the changes since `turn` (SPEC §20C.3): it has no start tree.
pub fn diff_blocked(turn: &Turn) -> Option<&'static str> {
    turn.start_tree.is_none().then_some(NO_SNAPSHOT)
}

/// Esc closes the dialog; ↑ / ↓ pick a turn; Enter restores before it; D shows the changes
/// since it started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Close,
    Move(isize),
    Restore,
    DiffSince,
}

pub fn key(keystroke: &Keystroke) -> Option<Key> {
    let modified = keystroke.modifiers.modified();
    match keystroke.key.as_str() {
        "escape" => Some(Key::Close),
        "up" if !modified => Some(Key::Move(-1)),
        "down" if !modified => Some(Key::Move(1)),
        "enter" if !modified => Some(Key::Restore),
        "d" if !modified => Some(Key::DiffSince),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{SessionKind, StateSource, TurnFile};

    use super::*;

    const MIN: i64 = 60 * 1000;

    fn file(path: &str, added: Option<u32>, removed: Option<u32>) -> TurnFile {
        TurnFile {
            path: path.into(),
            added,
            removed,
        }
    }

    fn turn(n: u32, started: i64, ended: Option<i64>, tree: bool, files: Vec<TurnFile>) -> Turn {
        Turn {
            n,
            started_at: started,
            ended_at: ended,
            start_tree: tree.then(|| "abc".into()),
            end_tree: None,
            files,
        }
    }

    fn session() -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id: Ulid::new(),
            agent_profile: "claude".into(),
            kind: SessionKind::Agent,
            title: None,
            terminal_title: None,
            color: None,
            native_session_id: None,
            state: SessionState::Done,
            state_source: StateSource::Daemon,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
            pinned: false,
            position: 0.0,
            plan_path: None,
        }
    }

    #[test]
    fn turns_are_listed_newest_first() {
        let mut dialog = TurnsDialog::new(&session());
        assert_eq!(dialog.listing, Listing::Loading);
        assert_eq!(dialog.selected_turn(), None);
        dialog.loaded(vec![
            turn(1, 0, Some(MIN), true, vec![]),
            turn(2, MIN, Some(2 * MIN), true, vec![]),
            turn(3, 2 * MIN, None, true, vec![]),
        ]);
        let Listing::Loaded(turns) = &dialog.listing else {
            panic!("expected loaded turns");
        };
        let ns: Vec<u32> = turns.iter().map(|turn| turn.n).collect();
        assert_eq!(ns, [3, 2, 1]);
        assert_eq!(dialog.agent, "claude");
        assert_eq!(dialog.selected_turn(), Some(3));
        dialog.select_by(-1);
        assert_eq!(dialog.selected_turn(), Some(3));
        dialog.select_by(1);
        assert_eq!(dialog.selected_turn(), Some(2));
        dialog.select_by(5);
        assert_eq!(dialog.selected_turn(), Some(1));
    }

    #[test]
    fn rows_show_time_files_and_lines() {
        let turns = [
            turn(3, 10 * MIN, None, true, vec![file("a.rs", Some(1), None)]),
            turn(
                2,
                4 * MIN,
                Some(9 * MIN),
                true,
                vec![
                    file("a.rs", Some(10), Some(2)),
                    file("b.rs", Some(2), Some(1)),
                    file("bin.png", None, None),
                ],
            ),
            turn(
                1,
                MIN,
                Some(3 * MIN / 2),
                false,
                vec![file("c.rs", None, None)],
            ),
        ];
        // A running turn waiting for an answer.
        let lines: Vec<String> = rows(&turns, SessionState::NeedsInput, 12 * MIN)
            .into_iter()
            .map(|row| {
                format!(
                    "{} | {} | {} | {} | {}",
                    row.title,
                    row.when,
                    row.files,
                    row.lines.unwrap_or_default(),
                    row.blocked.unwrap_or("restorable"),
                )
            })
            .collect();
        insta::assert_snapshot!(lines.join("\n"));
    }

    #[test]
    fn restore_is_blocked_while_working_or_without_a_snapshot() {
        let snapshotted = turn(1, 0, Some(MIN), true, vec![]);
        assert_eq!(restore_blocked(&snapshotted, SessionState::Done), None);
        assert_eq!(restore_blocked(&snapshotted, SessionState::Exited), None);
        assert_eq!(
            restore_blocked(&snapshotted, SessionState::Working),
            Some("Wait until the agent finishes its turn.")
        );
        assert!(restore_blocked(&turn(1, 0, None, false, vec![]), SessionState::Done).is_some());
    }

    #[test]
    fn diff_since_needs_a_snapshot_only() {
        assert_eq!(diff_blocked(&turn(1, 0, None, true, vec![])), None);
        assert_eq!(
            diff_blocked(&turn(1, 0, None, false, vec![])),
            Some(NO_SNAPSHOT)
        );
    }

    #[test]
    fn keys_close_move_and_restore() {
        let parse = |text| key(&Keystroke::parse(text).unwrap());
        assert_eq!(parse("escape"), Some(Key::Close));
        assert_eq!(parse("up"), Some(Key::Move(-1)));
        assert_eq!(parse("down"), Some(Key::Move(1)));
        assert_eq!(parse("enter"), Some(Key::Restore));
        assert_eq!(parse("d"), Some(Key::DiffSince));
        assert_eq!(parse("ctrl-d"), None);
        assert_eq!(parse("ctrl-enter"), None);
        assert_eq!(parse("a"), None);
    }
}
