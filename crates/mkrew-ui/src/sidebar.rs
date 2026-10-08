//! Workspace sidebar view-model (SPEC §10, §11): workspaces, and per expanded workspace its
//! sessions with status. Pure and tested headlessly; `main.rs` renders the rows.

use std::collections::HashSet;

use mkrew_protocol::{Event, Project, Session, SessionState, StateSource};
use ulid::Ulid;

use crate::status;
use crate::theme::Theme;

/// How a session state is shown: always icon + text, never color alone (SPEC §11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusDisplay {
    pub icon: &'static str,
    pub label: &'static str,
}

pub fn status_display(state: SessionState) -> StatusDisplay {
    let (icon, label) = match state {
        SessionState::Starting => ("◌", "starting"),
        SessionState::Working => ("◐", "working"),
        SessionState::NeedsInput => ("?", "needs input"),
        SessionState::Idle => ("○", "idle"),
        SessionState::Done => ("✓", "done"),
        SessionState::Exited => ("✕", "exited"),
        SessionState::Unknown => ("·", "unknown"),
    };
    StatusDisplay { icon, label }
}

/// Status color token of `state` (status colors are only used for agent states).
pub fn status_color(theme: &Theme, state: SessionState) -> u32 {
    match state {
        SessionState::Working => theme.status_working,
        SessionState::NeedsInput => theme.status_needs_input,
        SessionState::Done => theme.status_done,
        SessionState::Exited => theme.status_exited,
        SessionState::Idle => theme.status_idle,
        SessionState::Starting | SessionState::Unknown => theme.text_muted,
    }
}

/// One visible sidebar row, top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Workspace(WorkspaceRow),
    Session(SessionRow),
    /// An expanded workspace without sessions.
    NoSessions {
        project_id: Ulid,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRow {
    pub project_id: Ulid,
    pub name: String,
    pub path: String,
    pub expanded: bool,
    /// Sessions in `working` (shown as a count, SPEC §11).
    pub working: usize,
    /// Any session needs input (shown as `?`).
    pub needs_input: bool,
    /// A `WORKSPACE_COLORS` name.
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    pub session_id: Ulid,
    pub project_id: Ulid,
    /// Agent profile id (`shell` for shell sessions).
    pub agent: String,
    pub title: Option<String>,
    pub state: SessionState,
    pub state_source: StateSource,
    /// Unix ms of the last state change.
    pub state_changed_at: i64,
    pub exit_code: Option<i32>,
}

impl SessionRow {
    /// The status text at `now_ms`, e.g. `working · 2m`, `needs input` or `exited (1)`.
    pub fn status_text(&self, now_ms: i64) -> String {
        status::text(self.state, self.exit_code, self.state_changed_at, now_ms)
    }
}

/// What the owner does after [`Sidebar::apply`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Nothing the sidebar shows changed.
    None,
    /// Re-render.
    Redraw,
    /// An unknown session reported: re-read `session.list`.
    Refresh,
}

#[derive(Debug, Default)]
pub struct Sidebar {
    /// In `project.list` order; archived projects are left out.
    projects: Vec<Project>,
    sessions: Vec<Session>,
    expanded: HashSet<Ulid>,
    /// Set once the first project list was loaded (decides the initial expansion).
    loaded: bool,
    /// Events applied since [`Sidebar::begin_load`]: the lists may predate them, so
    /// [`Sidebar::load`] replays them (events arrive on another channel than replies).
    since_load: Option<Vec<Event>>,
}

impl Sidebar {
    pub fn is_empty(&self) -> bool {
        self.projects.is_empty()
    }

    /// Workspaces in `project.list` order (archived ones left out).
    pub fn projects(&self) -> &[Project] {
        &self.projects
    }

    /// The workspace `offset` places after `current` in sidebar order, wrapping around; the
    /// first one if `current` is unknown. `None` without workspaces.
    pub fn workspace_at_offset(&self, current: Option<Ulid>, offset: isize) -> Option<Ulid> {
        let len = self.projects.len() as isize;
        if len == 0 {
            return None;
        }
        let next = match current.and_then(|id| self.projects.iter().position(|p| p.id == id)) {
            Some(index) => (index as isize + offset).rem_euclid(len) as usize,
            None => 0,
        };
        Some(self.projects[next].id)
    }

    /// Every known session, as last listed and updated by events.
    pub fn sessions(&self) -> &[Session] {
        &self.sessions
    }

    /// A `project.list` + `session.list` was just sent.
    pub fn begin_load(&mut self) {
        self.since_load = Some(Vec::new());
    }

    /// The lists could not be read: stop recording events.
    pub fn cancel_load(&mut self) {
        self.since_load = None;
    }

    /// Replaces workspaces and sessions with a fresh `project.list` + `session.list`, then
    /// replays the events applied since [`Sidebar::begin_load`]. The first load expands every
    /// workspace that has a live session.
    pub fn load(&mut self, projects: Vec<Project>, sessions: Vec<Session>) {
        self.projects = projects.into_iter().filter(|p| !p.archived).collect();
        self.sessions = sessions;
        for event in self.since_load.take().unwrap_or_default() {
            self.apply(&event);
        }
        if !self.loaded {
            self.loaded = true;
            self.expanded = self
                .sessions
                .iter()
                .filter(|session| session.state != SessionState::Exited)
                .map(|session| session.project_id)
                .collect();
        }
    }

    /// A workspace the user just added: shown and expanded.
    pub fn add_project(&mut self, project: Project) {
        self.expanded.insert(project.id);
        self.upsert_project(project);
    }

    pub fn toggle(&mut self, project_id: Ulid) {
        if !self.expanded.remove(&project_id) {
            self.expanded.insert(project_id);
        }
    }

    /// A session the user removed (`session.remove` answered ok): gone from the rows now.
    pub fn remove_session(&mut self, session_id: Ulid) {
        self.sessions.retain(|session| session.id != session_id);
    }

    /// Applies a daemon event.
    pub fn apply(&mut self, event: &Event) -> Change {
        if let Some(since_load) = &mut self.since_load
            && !matches!(
                event,
                Event::TabUpdated(_)
                    | Event::TaskUpdated(_)
                    | Event::TaskDeleted(_)
                    | Event::SessionSnapshot(_)
                    | Event::NotificationClicked(_)
                    | Event::ConflictDetected(_)
                    | Event::CommentUpdated(_)
                    | Event::AiGenerated(_)
            )
        {
            since_load.push(event.clone());
        }
        match event {
            Event::SessionState(event) => self.update_session(event.session_id, |session| {
                session.state = event.state;
                session.state_source = event.source;
                session.state_changed_at = event.changed_at;
            }),
            Event::SessionExited(event) => self.update_session(event.session_id, |session| {
                session.state = SessionState::Exited;
                session.exit_code = event.exit_code;
            }),
            Event::ProjectUpdated(project) => {
                self.upsert_project(project.clone());
                Change::Redraw
            }
            Event::TabUpdated(_)
            | Event::TaskUpdated(_)
            | Event::TaskDeleted(_)
            | Event::SessionSnapshot(_)
            | Event::NotificationClicked(_)
            | Event::ConflictDetected(_)
            | Event::DocChanged(_)
            | Event::CommentUpdated(_)
            | Event::GitChanged(_)
            | Event::SessionTurn(_)
            | Event::SessionQueue(_)
            | Event::SessionEdited(_)
            | Event::SessionUsage(_)
            | Event::SessionPorts(_)
            | Event::AiGenerated(_) => Change::None,
        }
    }

    /// Rows to show: each workspace, then (if expanded) its sessions, newest first.
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for project in &self.projects {
            let mut sessions: Vec<&Session> = self
                .sessions
                .iter()
                .filter(|session| session.project_id == project.id)
                .collect();
            sessions.sort_by_key(|session| std::cmp::Reverse((session.started_at, session.id)));
            let expanded = self.expanded.contains(&project.id);
            rows.push(Row::Workspace(WorkspaceRow {
                project_id: project.id,
                name: project.name.clone(),
                path: project.path.clone(),
                expanded,
                working: sessions
                    .iter()
                    .filter(|session| session.state == SessionState::Working)
                    .count(),
                needs_input: sessions
                    .iter()
                    .any(|session| session.state == SessionState::NeedsInput),
                color: project.color.clone(),
            }));
            if !expanded {
                continue;
            }
            if sessions.is_empty() {
                rows.push(Row::NoSessions {
                    project_id: project.id,
                });
            }
            rows.extend(sessions.into_iter().map(|session| {
                Row::Session(SessionRow {
                    session_id: session.id,
                    project_id: session.project_id,
                    agent: session.agent_profile.clone(),
                    title: session.title.clone(),
                    state: session.state,
                    state_source: session.state_source,
                    state_changed_at: session.state_changed_at,
                    exit_code: session.exit_code,
                })
            }));
        }
        rows
    }

    fn update_session(&mut self, session_id: Ulid, update: impl FnOnce(&mut Session)) -> Change {
        match self.sessions.iter_mut().find(|s| s.id == session_id) {
            Some(session) => {
                update(session);
                Change::Redraw
            }
            None => Change::Refresh,
        }
    }

    fn upsert_project(&mut self, project: Project) {
        let existing = self.projects.iter().position(|p| p.id == project.id);
        match (existing, project.archived) {
            (Some(index), true) => {
                self.projects.remove(index);
            }
            (Some(index), false) => self.projects[index] = project,
            (None, false) => self.projects.push(project),
            (None, true) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{SessionExitedEvent, SessionKind, SessionStateEvent};

    use super::*;

    fn project(name: &str) -> Project {
        Project {
            id: Ulid::new(),
            name: name.to_owned(),
            path: format!("C:\\src\\{name}"),
            run_commands: Vec::new(),
            run_autostart: false,
            default_agent: None,
            review_globs: None,
            created_at: 0,
            archived: false,
            base_ref: None,
            color: None,
        }
    }

    fn session(project: &Project, started_at: i64, state: SessionState) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id: project.id,
            agent_profile: "claude".to_owned(),
            kind: SessionKind::Agent,
            title: None,
            native_session_id: None,
            state,
            state_source: StateSource::Daemon,
            state_changed_at: started_at,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at,
            ended_at: None,
        }
    }

    fn workspace(row: &Row) -> &WorkspaceRow {
        match row {
            Row::Workspace(row) => row,
            other => panic!("expected a workspace row, got {other:?}"),
        }
    }

    fn session_ids(rows: &[Row]) -> Vec<Ulid> {
        rows.iter()
            .filter_map(|row| match row {
                Row::Session(row) => Some(row.session_id),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn every_state_has_an_icon_and_text() {
        let states = [
            SessionState::Starting,
            SessionState::Working,
            SessionState::NeedsInput,
            SessionState::Idle,
            SessionState::Done,
            SessionState::Exited,
            SessionState::Unknown,
        ];
        let icons: HashSet<&str> = states.iter().map(|s| status_display(*s).icon).collect();
        assert_eq!(icons.len(), states.len(), "icons must be distinct");
        for state in states {
            let display = status_display(state);
            assert!(!display.icon.is_empty() && !display.label.is_empty());
        }
        assert_eq!(status_display(SessionState::NeedsInput).icon, "?");
        assert_eq!(status_display(SessionState::Done).icon, "✓");
        let theme = Theme::dark();
        assert_eq!(
            status_color(&theme, SessionState::NeedsInput),
            theme.status_needs_input
        );
    }

    #[test]
    fn first_load_expands_workspaces_with_live_sessions_newest_first() {
        let (api, web) = (project("api"), project("web"));
        let old = session(&api, 1, SessionState::Idle);
        let new = session(&api, 2, SessionState::Working);
        let exited = session(&web, 3, SessionState::Exited);
        let mut sidebar = Sidebar::default();
        sidebar.load(
            vec![api.clone(), web.clone()],
            vec![old.clone(), new.clone(), exited],
        );

        let rows = sidebar.rows();
        assert_eq!(rows.len(), 4, "api + 2 sessions + collapsed web: {rows:?}");
        assert!(workspace(&rows[0]).expanded);
        assert_eq!(session_ids(&rows), vec![new.id, old.id]);
        let web_row = workspace(&rows[3]);
        assert_eq!(web_row.name, "web");
        assert!(!web_row.expanded);

        // Later loads keep what the user expanded or collapsed.
        sidebar.toggle(api.id);
        sidebar.load(vec![api, web], vec![old, new]);
        assert!(!workspace(&sidebar.rows()[0]).expanded);
    }

    #[test]
    fn workspaces_cycle_in_sidebar_order() {
        let mut sidebar = Sidebar::default();
        assert_eq!(sidebar.workspace_at_offset(None, 1), None);
        let (a, b, c) = (project("a"), project("b"), project("c"));
        let ids = [a.id, b.id, c.id];
        sidebar.load(vec![a, b, c], Vec::new());
        assert_eq!(sidebar.workspace_at_offset(Some(ids[0]), 1), Some(ids[1]));
        assert_eq!(sidebar.workspace_at_offset(Some(ids[2]), 1), Some(ids[0]));
        assert_eq!(sidebar.workspace_at_offset(Some(ids[0]), -1), Some(ids[2]));
        assert_eq!(sidebar.workspace_at_offset(None, -1), Some(ids[0]));
        assert_eq!(
            sidebar.workspace_at_offset(Some(Ulid::new()), 1),
            Some(ids[0])
        );
    }

    #[test]
    fn workspace_row_counts_working_and_flags_needs_input() {
        let api = project("api");
        let a = session(&api, 1, SessionState::Working);
        let b = session(&api, 2, SessionState::Working);
        let c = session(&api, 3, SessionState::Idle);
        let mut sidebar = Sidebar::default();
        sidebar.load(vec![api], vec![a, b, c.clone()]);
        let row = workspace(&sidebar.rows()[0]).clone();
        assert_eq!((row.working, row.needs_input), (2, false));

        let change = sidebar.apply(&Event::SessionState(SessionStateEvent {
            session_id: c.id,
            state: SessionState::NeedsInput,
            source: StateSource::Hook,
            changed_at: 9,
        }));
        assert_eq!(change, Change::Redraw);
        let row = workspace(&sidebar.rows()[0]).clone();
        assert_eq!((row.working, row.needs_input), (2, true));
        let updated = &sidebar.sessions()[2];
        assert_eq!(
            (updated.state_source, updated.state_changed_at),
            (StateSource::Hook, 9)
        );
        // The row carries what its status hover shows.
        let shown = sidebar.rows().iter().find_map(|row| match row {
            Row::Session(row) if row.session_id == c.id => {
                Some((row.state_source, row.state_changed_at))
            }
            _ => None,
        });
        assert_eq!(shown, Some((StateSource::Hook, 9)));
    }

    #[test]
    fn events_newer_than_the_lists_survive_a_load() {
        let api = project("api");
        let s = session(&api, 1, SessionState::Working);
        let mut sidebar = Sidebar::default();
        sidebar.load(vec![api.clone()], vec![s.clone()]);

        // The state changes after `session.list` was answered, but its event is handled first.
        sidebar.begin_load();
        sidebar.apply(&Event::SessionState(SessionStateEvent {
            session_id: s.id,
            state: SessionState::NeedsInput,
            source: StateSource::Screen,
            changed_at: 2,
        }));
        sidebar.load(vec![api], vec![s]);
        assert!(workspace(&sidebar.rows()[0]).needs_input);
        assert!(sidebar.since_load.is_none());
    }

    #[test]
    fn exit_shows_the_code_and_unknown_sessions_ask_for_a_refresh() {
        let api = project("api");
        let s = session(&api, 1, SessionState::Working);
        let mut sidebar = Sidebar::default();
        sidebar.load(vec![api.clone()], vec![s.clone()]);

        let exited = Event::SessionExited(SessionExitedEvent {
            session_id: s.id,
            exit_code: Some(1),
        });
        assert_eq!(sidebar.apply(&exited), Change::Redraw);
        let Row::Session(row) = &sidebar.rows()[1] else {
            panic!("expected a session row");
        };
        assert_eq!(row.status_text(1_000), "exited (1)");

        let spawned = Event::SessionState(SessionStateEvent {
            session_id: Ulid::new(),
            state: SessionState::Starting,
            source: StateSource::Daemon,
            changed_at: 1,
        });
        assert_eq!(sidebar.apply(&spawned), Change::Refresh);
    }

    #[test]
    fn added_and_updated_workspaces_show_up_and_archived_ones_leave() {
        let mut sidebar = Sidebar::default();
        sidebar.load(Vec::new(), Vec::new());
        assert!(sidebar.is_empty());

        let mut api = project("api");
        sidebar.add_project(api.clone());
        assert_eq!(
            sidebar.rows(),
            vec![
                Row::Workspace(WorkspaceRow {
                    project_id: api.id,
                    name: "api".to_owned(),
                    path: api.path.clone(),
                    expanded: true,
                    working: 0,
                    needs_input: false,
                    color: None,
                }),
                Row::NoSessions { project_id: api.id },
            ]
        );
        // The daemon's echo of the same add does not duplicate it.
        assert_eq!(
            sidebar.apply(&Event::ProjectUpdated(api.clone())),
            Change::Redraw
        );
        assert_eq!(sidebar.rows().len(), 2);

        api.name = "backend".to_owned();
        sidebar.apply(&Event::ProjectUpdated(api.clone()));
        assert_eq!(workspace(&sidebar.rows()[0]).name, "backend");

        api.archived = true;
        sidebar.apply(&Event::ProjectUpdated(api));
        assert!(sidebar.is_empty());
    }
}
