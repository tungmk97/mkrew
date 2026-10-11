//! Pane lifecycle decisions (SPEC §10): what a new pane spawns, what closing a pane does to its
//! session, and when a workspace runs too many sessions. Pure and tested headlessly.

use mkrew_protocol::{
    Request, Session, SessionKind, SessionRenameParams, SessionSetColorParams, SessionSpawnParams,
    SessionState, Task, TaskFanoutParams, TaskRunParams, TaskUpdateParams,
};
use ulid::Ulid;

use crate::workspace_menu::Agents;

/// Above this many running sessions in one workspace the tab bar shows a warning.
pub const RUNNING_WARN_ABOVE: usize = 12;

/// Size a new session starts with; its pane resizes it once laid out.
const SPAWN_COLS: u16 = 80;
const SPAWN_ROWS: u16 = 24;

/// `session.spawn` for a new pane: an agent from the workspace's default profile or its default
/// shell, started in the project folder (the daemon's cwd for every session of a project).
pub fn spawn_params(
    project_id: Ulid,
    tab_id: Option<Ulid>,
    kind: SessionKind,
) -> SessionSpawnParams {
    SessionSpawnParams {
        project_id,
        // `None` = the project's `default_agent` for an agent, its `default_shell` for a shell.
        agent_profile: None,
        kind,
        tab_id,
        title: None,
        cols: SPAWN_COLS,
        rows: SPAWN_ROWS,
    }
}

/// The `session.rename` for an edited pane title, trimmed; blank clears the title. `None`
/// when nothing changes (ADR 0031).
pub fn session_rename_params(session: &Session, draft: &str) -> Option<SessionRenameParams> {
    let title = Some(draft.trim()).filter(|title| !title.is_empty());
    (title != session.title.as_deref()).then(|| SessionRenameParams {
        session_id: session.id,
        title: title.map(str::to_owned),
    })
}

/// The `session.set_color` for a picked color (`None` = no color); `None` when nothing
/// changes (ADR 0037).
pub fn session_color_params(
    session: &Session,
    color: Option<&str>,
) -> Option<SessionSetColorParams> {
    (color != session.color.as_deref()).then(|| SessionSetColorParams {
        session_id: session.id,
        color: color.map(str::to_owned),
    })
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

/// `task.fanout` for "Run with several agents…" (SPEC §20E.2): the children's sessions start at
/// the size of a new pane, like a card's Run.
pub fn fanout_params(task_id: Ulid, agent_profiles: Vec<String>) -> TaskFanoutParams {
    TaskFanoutParams {
        task_id,
        agent_profiles,
        cols: SPAWN_COLS,
        rows: SPAWN_ROWS,
    }
}

/// The open "Run with…" picker of one card (SPEC §20B.6): the agent profiles, fetched when it
/// opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunWith {
    pub task_id: Ulid,
    pub agents: Agents,
}

impl RunWith {
    pub fn new(task_id: Ulid) -> Self {
        Self {
            task_id,
            agents: Agents::Loading,
        }
    }
}

/// "Run with `profile`" on `task`'s card, in the order they are sent (each after the previous
/// one succeeded): `task.update` of its agent (skipped when it already is `profile`), then
/// `task.run`.
pub fn run_with_requests(task: &Task, profile: &str) -> Vec<Request> {
    let update = (task.agent_profile.as_deref() != Some(profile)).then(|| {
        Request::TaskUpdate(TaskUpdateParams {
            task_id: task.id,
            title: None,
            prompt: None,
            agent_profile: Some(Some(profile.to_owned())),
            worktree: None,
        })
    });
    update
        .into_iter()
        .chain([Request::TaskRun(run_params(task.id))])
        .collect()
}

/// What a split shortcut starts next to the focused pane: the same kind of session (an agent
/// next to an agent, a shell next to a shell); a shell if nothing is focused.
pub fn split_kind(focused: Option<&Session>) -> SessionKind {
    focused.map_or(SessionKind::Shell, |session| session.kind)
}

/// "an agent" / "a shell", for messages about starting one.
pub fn kind_noun(kind: SessionKind) -> &'static str {
    match kind {
        SessionKind::Agent => "an agent",
        SessionKind::Shell => "a shell",
    }
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
            terminal_title: None,
            color: None,
            native_session_id: None,
            state,
            state_source: mkrew_protocol::StateSource::Daemon,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
            pinned: false,
            plan_path: None,
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
    fn session_rename_params_trim_clear_and_skip_unchanged() {
        let mut named = session(Ulid::new(), SessionKind::Agent, SessionState::Working);
        let params = session_rename_params(&named, "  Fix login  ").unwrap();
        assert_eq!(params.session_id, named.id);
        assert_eq!(params.title.as_deref(), Some("Fix login"));
        // Blank on an untitled pane: nothing to do.
        assert_eq!(session_rename_params(&named, "   "), None);
        named.title = Some("Fix login".into());
        assert_eq!(session_rename_params(&named, " Fix login "), None);
        // Blank clears an existing title.
        let cleared = session_rename_params(&named, "  ").unwrap();
        assert_eq!(cleared.title, None);
    }

    #[test]
    fn session_color_params_skip_unchanged_and_clear_with_none() {
        let mut pane = session(Ulid::new(), SessionKind::Agent, SessionState::Working);
        let params = session_color_params(&pane, Some("teal")).unwrap();
        assert_eq!(params.session_id, pane.id);
        assert_eq!(params.color.as_deref(), Some("teal"));
        // No color on an uncolored pane: nothing to send.
        assert_eq!(session_color_params(&pane, None), None);
        pane.color = Some("teal".into());
        assert_eq!(session_color_params(&pane, Some("teal")), None);
        let cleared = session_color_params(&pane, None).unwrap();
        assert_eq!(cleared.color, None);
    }

    #[test]
    fn run_with_updates_the_agent_then_runs() {
        let mut task = Task {
            id: Ulid::new(),
            project_id: Ulid::new(),
            title: "t".into(),
            prompt: "p".into(),
            agent_profile: None,
            status: mkrew_protocol::TaskStatus::Review,
            position: 1.0,
            checkpoint: None,
            error_flag: false,
            worktree: false,
            worktree_path: None,
            branch: None,
            fanout_of: None,
            created_at: 0,
            updated_at: 0,
        };
        let update = Request::TaskUpdate(TaskUpdateParams {
            task_id: task.id,
            title: None,
            prompt: None,
            agent_profile: Some(Some("codex".into())),
            worktree: None,
        });
        let run = Request::TaskRun(run_params(task.id));
        assert_eq!(
            run_with_requests(&task, "codex"),
            vec![update.clone(), run.clone()]
        );
        task.agent_profile = Some("claude".into());
        assert_eq!(run_with_requests(&task, "codex"), vec![update, run.clone()]);
        // Already its agent: only the run.
        task.agent_profile = Some("codex".into());
        assert_eq!(run_with_requests(&task, "codex"), vec![run]);
        assert_eq!(RunWith::new(task.id).agents, Agents::Loading);
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
