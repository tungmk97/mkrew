//! Text of a pane's status (SPEC §11): the task folder and the status pill. Pure and tested
//! headlessly; `main.rs` draws it.

use mkrew_protocol::{SessionState, SessionTurnEvent, Task};

use crate::turn;

/// The folder a pane works in when it is not the workspace folder: the worktree of its task
/// (SPEC §20E.1), named by its branch without the `mkrew/` prefix. `None` for every other pane.
pub fn folder(task: Option<&Task>) -> Option<String> {
    let task = task.filter(|task| task.worktree_path.is_some())?;
    let branch = task.branch.as_deref()?;
    Some(branch.strip_prefix("mkrew/").unwrap_or(branch).to_owned())
}

/// The status pill: [`turn::status_text`] (`working · 1h 25m`, `done · 3 files · 4m`) with a
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

/// What a compact pane header has room for besides the agent chip and the status pill (it
/// ellipsizes): as the pane narrows, the context share goes first, then `Share`, then the
/// reading width toggle, then the `Terminal | Chat` switch (Ctrl+Alt+T still switches).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeaderRoom {
    pub context: bool,
    pub share: bool,
    pub reading: bool,
    pub switch: bool,
}

impl HeaderRoom {
    /// Room for every part (the tab bar of a single pane).
    pub const ALL: Self = Self {
        context: true,
        share: true,
        reading: true,
        switch: true,
    };
}

// ponytail: widths in em of the UI font, measured once in logical px at 13 px, not with the
// text system; a part drops whole, so an error only moves the moment it drops. Measure if one
// drops visibly early or late.
const CHIP_EM: f32 = 1.85;
const PILL_MIN_EM: f32 = 4.0;
const SWITCH_EM: f32 = 10.15;
const READING_EM: f32 = 2.0;
const SHARE_EM: f32 = 4.6;
const CHAR_EM: f32 = 0.46;

/// [`HeaderRoom`] of a header `width` px wide at `em` px per em; `context_chars` is the length
/// of the context share label (0 without one), `switch` whether the pane offers
/// `Terminal | Chat`.
pub fn header_room(width: f32, em: f32, context_chars: usize, switch: bool) -> HeaderRoom {
    let gap = crate::theme::space::S;
    // The padding on both sides, then the chip, the (maybe empty) title and the pill.
    let mut used = 2.0 * gap + (CHIP_EM + PILL_MIN_EM) * em + 2.0 * gap;
    let mut fits = |part_em: f32| {
        let part = gap + part_em * em;
        let fits = used + part <= width;
        if fits {
            used += part;
        }
        fits
    };
    let switch = switch.then(|| fits(SWITCH_EM));
    let reading = switch != Some(false) && fits(READING_EM);
    let share = reading && fits(SHARE_EM);
    let context = share && context_chars > 0 && fits(context_chars as f32 * CHAR_EM);
    HeaderRoom {
        context,
        share,
        reading,
        switch: switch == Some(true),
    }
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{SessionState, SessionTurnEvent, Task, TaskStatus, TurnFile};
    use ulid::Ulid;

    use super::*;

    const SEC: i64 = 1000;

    fn task(worktree: bool, branch: Option<&str>) -> Task {
        Task {
            id: Ulid::new(),
            project_id: Ulid::new(),
            title: "Fix login".into(),
            prompt: String::new(),
            agent_profile: None,
            status: TaskStatus::Running,
            position: 0.0,
            checkpoint: None,
            error_flag: false,
            worktree,
            worktree_path: worktree.then(|| "/data/worktrees/p/t".to_owned()),
            branch: branch.map(str::to_owned),
            fanout_of: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn only_a_worktree_task_names_a_folder() {
        let branch = Some("mkrew/fix-login-ab12cd34");
        assert_eq!(
            folder(Some(&task(true, branch))).as_deref(),
            Some("fix-login-ab12cd34")
        );
        // The worktree is gone (or never made): the pane runs in the workspace folder.
        assert_eq!(folder(Some(&task(false, branch))), None);
        assert_eq!(folder(Some(&task(true, None))), None);
        assert_eq!(folder(None), None);
        // Not one of ours: shown whole.
        assert_eq!(
            folder(Some(&task(true, Some("feature/x")))).as_deref(),
            Some("feature/x")
        );
    }

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

    /// A narrowing header drops the context share, then Share, then the reading toggle, then
    /// the switch; a wider font needs more room.
    #[test]
    fn a_narrow_header_drops_its_parts_in_order() {
        let room = |width: f32, switch: bool| {
            let r = header_room(width, 13.0, 7, switch);
            (r.switch, r.reading, r.share, r.context)
        };
        assert_eq!(room(2_000.0, true), (true, true, true, true));
        // Each part drops at one width only, in order, and never comes back wider.
        let mut seen: Vec<_> = (0..2_000).map(|w| room(w as f32, true)).collect();
        seen.dedup();
        assert_eq!(
            seen,
            [
                (false, false, false, false),
                (true, false, false, false),
                (true, true, false, false),
                (true, true, true, false),
                (true, true, true, true),
            ]
        );
        // The 304 px split pane seen in the smoke keeps the switch and the reading toggle; a
        // pane without the switch uses its room for the others.
        assert_eq!(room(304.0, true), (true, true, false, false));
        assert_eq!(room(304.0, false), (false, true, true, true));
        let wide_font = header_room(500.0, 20.0, 7, true);
        assert!(room(500.0, true).3 && !wide_font.context);
        // Without a context share nothing is kept for it.
        assert!(!header_room(2_000.0, 13.0, 0, true).context);
    }
}
