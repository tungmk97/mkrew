//! Pane lifecycle decisions (SPEC §10): what a new pane spawns, what closing a pane does to its
//! session, and when a workspace runs too many sessions. Pure and tested headlessly.

use mkrew_protocol::{Session, SessionKind, SessionSpawnParams, SessionState, TaskRunParams};
use ulid::Ulid;

/// Above this many running sessions in one workspace the tab bar shows a warning.
pub const RUNNING_WARN_ABOVE: usize = 12;

/// Size a new session starts with; its pane resizes it once laid out.
const SPAWN_COLS: u16 = 80;
const SPAWN_ROWS: u16 = 24;

/// `session.spawn` for a new pane: an agent from the workspace's default profile or the shell,
/// started in the project folder (the daemon's cwd for every session of a project).
pub fn spawn_params(
    project_id: Ulid,
    tab_id: Option<Ulid>,
    kind: SessionKind,
) -> SessionSpawnParams {
    SessionSpawnParams {
        project_id,
        // `None` = the project's `default_agent` for an agent, the `shell` profile for a shell.
        agent_profile: None,
        kind,
        tab_id,
        title: None,
        cols: SPAWN_COLS,
        rows: SPAWN_ROWS,
    }
}

/// `task.run` for a card's Run: the daemon picks the agent and folder; the session starts at
/// the size of a new pane (its pane resizes it once opened).
pub fn run_params(task_id: Ulid) -> TaskRunParams {
    TaskRunParams {
        task_id,
        cols: SPAWN_COLS,
        rows: SPAWN_ROWS,
    }
}

/// What a split shortcut starts next to the focused pane: the same kind of session (an agent
/// next to an agent, a shell next to a shell); a shell if nothing is focused.
pub fn split_kind(focused: Option<&Session>) -> SessionKind {
    focused.map_or(SessionKind::Shell, |session| session.kind)
}

/// What closing the pane of a session does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAction {
    /// The session has exited: only the pane goes.
    Remove,
    /// A live shell: the pane goes and the shell is killed.
    Kill,
    /// A live agent, or a session not listed yet (just spawned): ask whether to kill it or
    /// just hide the pane.
    Ask,
}

pub fn close_action(session: Option<&Session>) -> CloseAction {
    match session {
        None => CloseAction::Ask,
        Some(s) if s.state == SessionState::Exited => CloseAction::Remove,
        Some(s) if s.kind == SessionKind::Shell => CloseAction::Kill,
        Some(_) => CloseAction::Ask,
    }
}

/// The answer to the close dialog of a running agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseChoice {
    Kill,
    /// Close the pane; the session keeps running and stays in the sidebar.
    Hide,
    Cancel,
}

impl CloseChoice {
    /// Dialog buttons, in the order [`CloseChoice::from_answer`] reads them.
    pub const ANSWERS: [&'static str; 3] = ["Kill session", "Hide pane", "Cancel"];

    /// The button picked by index; anything else (dialog dismissed) cancels.
    pub fn from_answer(index: Option<usize>) -> Self {
        match index {
            Some(0) => CloseChoice::Kill,
            Some(1) => CloseChoice::Hide,
            _ => CloseChoice::Cancel,
        }
    }
}

/// Sessions of a workspace that have not exited.
pub fn running_count(sessions: &[Session], project_id: Ulid) -> usize {
    sessions
        .iter()
        .filter(|s| s.project_id == project_id && s.state != SessionState::Exited)
        .count()
}

/// The warning shown while a workspace runs more than [`RUNNING_WARN_ABOVE`] sessions.
pub fn running_warning(sessions: &[Session], project_id: Ulid) -> Option<String> {
    let running = running_count(sessions, project_id);
    (running > RUNNING_WARN_ABOVE).then(|| {
        format!("{running} sessions running in this workspace. Close the ones you no longer need.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(project_id: Ulid, kind: SessionKind, state: SessionState) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id,
            agent_profile: "fake".into(),
            kind,
            title: None,
            native_session_id: None,
            state,
            state_source: mkrew_protocol::StateSource::Daemon,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
        }
    }

    #[test]
    fn new_panes_use_the_default_profile_of_their_kind() {
        let (project, tab) = (Ulid::new(), Ulid::new());
        let agent = spawn_params(project, Some(tab), SessionKind::Agent);
        assert_eq!(agent.project_id, project);
        assert_eq!(agent.tab_id, Some(tab));
        assert_eq!(agent.agent_profile, None);
        assert_eq!(agent.kind, SessionKind::Agent);
        let shell = spawn_params(project, None, SessionKind::Shell);
        assert_eq!(shell.kind, SessionKind::Shell);
        assert_eq!(shell.agent_profile, None);
        assert!(shell.cols > 0 && shell.rows > 0);

        let task = Ulid::new();
        let run = run_params(task);
        assert_eq!(run.task_id, task);
        assert_eq!((run.cols, run.rows), (shell.cols, shell.rows));
    }

    #[test]
    fn splits_start_the_focused_kind() {
        let project = Ulid::new();
        let agent = session(project, SessionKind::Agent, SessionState::Idle);
        let shell = session(project, SessionKind::Shell, SessionState::Idle);
        assert_eq!(split_kind(Some(&agent)), SessionKind::Agent);
        assert_eq!(split_kind(Some(&shell)), SessionKind::Shell);
        assert_eq!(split_kind(None), SessionKind::Shell);
    }

    #[test]
    fn only_a_running_agent_asks_before_closing() {
        let project = Ulid::new();
        let agent = session(project, SessionKind::Agent, SessionState::NeedsInput);
        let shell = session(project, SessionKind::Shell, SessionState::Idle);
        let exited = session(project, SessionKind::Agent, SessionState::Exited);
        assert_eq!(close_action(Some(&agent)), CloseAction::Ask);
        assert_eq!(close_action(Some(&shell)), CloseAction::Kill);
        assert_eq!(close_action(Some(&exited)), CloseAction::Remove);
        assert_eq!(close_action(None), CloseAction::Ask);
    }

    #[test]
    fn dialog_answers_map_to_choices() {
        assert_eq!(CloseChoice::ANSWERS.len(), 3);
        assert_eq!(CloseChoice::from_answer(Some(0)), CloseChoice::Kill);
        assert_eq!(CloseChoice::from_answer(Some(1)), CloseChoice::Hide);
        assert_eq!(CloseChoice::from_answer(Some(2)), CloseChoice::Cancel);
        assert_eq!(CloseChoice::from_answer(None), CloseChoice::Cancel);
    }

    #[test]
    fn warns_above_twelve_running_sessions_per_workspace() {
        let (project, other) = (Ulid::new(), Ulid::new());
        let mut sessions: Vec<Session> = (0..RUNNING_WARN_ABOVE)
            .map(|_| session(project, SessionKind::Agent, SessionState::Working))
            .collect();
        sessions.push(session(project, SessionKind::Agent, SessionState::Exited));
        sessions.push(session(other, SessionKind::Shell, SessionState::Idle));
        assert_eq!(running_count(&sessions, project), RUNNING_WARN_ABOVE);
        assert_eq!(running_warning(&sessions, project), None);
        sessions.push(session(project, SessionKind::Shell, SessionState::Starting));
        let warning = running_warning(&sessions, project).unwrap();
        assert!(warning.starts_with("13 sessions running"), "{warning}");
        assert_eq!(running_warning(&sessions, other), None);
    }
}
