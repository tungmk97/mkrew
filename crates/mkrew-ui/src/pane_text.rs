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
}
