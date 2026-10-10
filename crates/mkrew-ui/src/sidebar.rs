//! Workspace sidebar view-model (SPEC §10, §11): workspaces, and per expanded workspace its
//! sessions with status. Pure and tested headlessly; `main.rs` renders the rows.

use std::collections::{HashMap, HashSet};

use mkrew_protocol::{Event, Project, Session, SessionState, StateSource};
use ulid::Ulid;

use crate::history;
use crate::inbox::{self, Viewed};
use crate::status;
use crate::text_box::Input;
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
        SessionState::Working => ("•••", "working"),
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

/// The badge before an agent name (ADR 0025): the profile's `icon`, else the first letter of
/// its id, uppercased (profiles written before `icon` existed, or not loaded yet).
pub fn agent_badge(icon: Option<&str>, agent: &str) -> String {
    match icon {
        Some(icon) => icon.to_owned(),
        None => agent
            .chars()
            .next()
            .map(|c| c.to_uppercase().collect())
            .unwrap_or_default(),
    }
}

/// The chips under the filter box.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Chip {
    #[default]
    All,
    Working,
    /// [`inbox::needs_you`]: needs input, or done and not yet viewed.
    NeedsYou,
}

impl Chip {
    fn admits(self, session: &Session, viewed: &Viewed) -> bool {
        match self {
            Chip::All => true,
            Chip::Working => session.state == SessionState::Working,
            Chip::NeedsYou => inbox::needs_you(session, viewed),
        }
    }
}

/// The chip numbers: live sessions matching the filter text (the chip itself does not shrink them).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub all: usize,
    pub working: usize,
    pub needs_you: usize,
}

/// What the sidebar draws.
#[derive(Debug)]
pub struct View {
    pub counts: Counts,
    pub rows: Vec<Row>,
}

/// Whether the (lowercase) `query` matches a session: every whitespace-separated word must be
/// found in one of `fields` (workspace name, agent, title), ignoring case.
// ponytail: lowercases the three fields per session per frame while a query is typed; cache
// the lowercase strings on `Sidebar::load` if a few hundred sessions ever make that visible.
pub fn matches(query: &str, fields: [&str; 3]) -> bool {
    let fields = fields.map(str::to_lowercase);
    query
        .split_whitespace()
        .all(|word| fields.iter().any(|field| field.contains(word)))
}

/// The workspace row's text: `1 working`, `2 idle`, `1 working · 1 needs you · 2 idle`; `None`
/// when none of the three exists (done, starting and unknown sessions are not named here).
pub fn workspace_summary(working: usize, asking: usize, idle: usize) -> Option<String> {
    let parts: Vec<String> = [(working, "working"), (asking, "needs you"), (idle, "idle")]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, what)| format!("{count} {what}"))
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
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
    /// Sessions in `working`.
    pub working: usize,
    /// Any session needs input (shown as `?`).
    pub needs_input: bool,
    /// `1 working` / `2 idle` …, `None` without sessions in those states.
    pub summary: Option<String>,
    /// A `WORKSPACE_COLORS` name.
    pub color: Option<String>,
    /// Pinned workspaces lead the list (SPEC §10).
    pub pinned: bool,
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
    /// Pinned sessions lead their workspace's list (SPEC §10).
    pub pinned: bool,
}

impl SessionRow {
    /// The only text on the row's right: the elapsed time of a working session or the idle
    /// time of an idle one (`17m`). The status icon already says the state; the hover says it
    /// in words.
    pub fn time_text(&self, now_ms: i64) -> Option<String> {
        status::time(self.state, self.state_changed_at, now_ms)
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
    /// Pinned first, then in `project.list` (age) order; archived projects are left out.
    projects: Vec<Project>,
    sessions: Vec<Session>,
    expanded: HashSet<Ulid>,
    /// The filter box text (client-side, SPEC §10); in memory only.
    filter: Input,
    chip: Chip,
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
        self.sort_projects();
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

    pub fn filter(&self) -> &Input {
        &self.filter
    }

    pub fn filter_mut(&mut self) -> &mut Input {
        &mut self.filter
    }

    pub fn chip(&self) -> Chip {
        self.chip
    }

    pub fn set_chip(&mut self, chip: Chip) {
        self.chip = chip;
    }

    /// A query or a chip other than `All` narrows the list.
    pub fn filtering(&self) -> bool {
        !self.filter.trim().is_empty() || self.chip != Chip::All
    }

    pub fn any_expanded(&self) -> bool {
        self.projects.iter().any(|p| self.expanded.contains(&p.id))
    }

    /// The header button: something open → close everything, else open everything.
    pub fn toggle_all(&mut self) {
        if self.any_expanded() {
            self.expanded.clear();
        } else {
            self.expanded = self.projects.iter().map(|p| p.id).collect();
        }
    }

    /// Makes `project_id`'s session rows visible: opened, and no query or chip hides them
    /// (the keyboard peek hangs on a row).
    pub fn reveal(&mut self, project_id: Ulid) {
        self.expanded.insert(project_id);
        self.filter.clear();
        self.chip = Chip::All;
    }

    /// A session the user removed (`session.remove` answered ok): gone from the rows now.
    pub fn remove_session(&mut self, session_id: Ulid) {
        self.sessions.retain(|session| session.id != session_id);
    }

    /// The title from a `session.rename` reply: only the title, because the reply can be older
    /// than state events already applied.
    pub fn set_title(&mut self, session_id: Ulid, title: Option<String>) {
        self.update_session(session_id, |session| session.title = title);
    }

    /// The color from a `session.set_color` reply: only the color, like [`Sidebar::set_title`].
    pub fn set_color(&mut self, session_id: Ulid, color: Option<String>) {
        self.update_session(session_id, |session| session.color = color);
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
            Event::SessionUpdated(updated) => {
                self.update_session(updated.id, |session| *session = (**updated).clone())
            }
            Event::SessionTerminalTitle(event) => {
                self.update_session(event.session_id, |session| {
                    session.terminal_title.clone_from(&event.title);
                })
            }
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
            | Event::SessionPlan(_)
            | Event::AiGenerated(_) => Change::None,
        }
    }

    /// Rows to show and the chip numbers, in one pass over the sessions (O(sessions) plus the
    /// newest-first sort inside each workspace). Each workspace, then (if open, or while a filter
    /// is active) its live sessions that pass the filter and the chip. While filtering,
    /// workspaces without such a session are left out.
    pub fn view(&self, viewed: &Viewed) -> View {
        let filtering = self.filtering();
        let mut counts = Counts::default();
        let mut rows = Vec::new();
        for (project, sessions) in self.grouped() {
            for session in &sessions {
                counts.all += 1;
                counts.working += usize::from(session.state == SessionState::Working);
                counts.needs_you += usize::from(inbox::needs_you(session, viewed));
            }
            let shown: Vec<&Session> = sessions
                .iter()
                .copied()
                .filter(|session| self.chip.admits(session, viewed))
                .collect();
            if filtering && shown.is_empty() {
                continue;
            }
            let expanded = filtering || self.expanded.contains(&project.id);
            let count = |state| sessions.iter().filter(|s| s.state == state).count();
            let (working, asking, idle) = (
                count(SessionState::Working),
                count(SessionState::NeedsInput),
                count(SessionState::Idle),
            );
            rows.push(Row::Workspace(WorkspaceRow {
                project_id: project.id,
                name: project.name.clone(),
                path: project.path.clone(),
                expanded,
                working,
                needs_input: asking > 0,
                summary: workspace_summary(working, asking, idle),
                color: project.color.clone(),
                pinned: project.pinned,
            }));
            if !expanded {
                continue;
            }
            if shown.is_empty() {
                rows.push(Row::NoSessions {
                    project_id: project.id,
                });
            }
            rows.extend(shown.into_iter().map(|session| {
                Row::Session(SessionRow {
                    session_id: session.id,
                    project_id: session.project_id,
                    agent: session.agent_profile.clone(),
                    title: history::title(session).map(str::to_owned),
                    state: session.state,
                    state_source: session.state_source,
                    state_changed_at: session.state_changed_at,
                    pinned: session.pinned,
                })
            }));
        }
        View { counts, rows }
    }

    /// [`Sidebar::view`]'s rows with no `done` session counted as viewed: what the tests and the
    /// pane-lifecycle check read (only the `Needs you` chip looks at `viewed`).
    pub fn rows(&self) -> Vec<Row> {
        self.view(&Viewed::default()).rows
    }

    /// Per workspace (in list order): its live sessions matching the filter text, newest first.
    /// An exited session cannot be opened: no row (History and the Inbox read it from
    /// `sessions()`; ADR 0033).
    fn grouped(&self) -> Vec<(&Project, Vec<&Session>)> {
        let query = self.filter.trim().to_lowercase();
        let mut by_project: HashMap<Ulid, Vec<&Session>> = HashMap::new();
        for session in self
            .sessions
            .iter()
            .filter(|session| session.state != SessionState::Exited)
        {
            by_project
                .entry(session.project_id)
                .or_default()
                .push(session);
        }
        self.projects
            .iter()
            .map(|project| {
                let mut sessions = by_project.remove(&project.id).unwrap_or_default();
                if !query.is_empty() {
                    sessions.retain(|session| {
                        let fields = [
                            project.name.as_str(),
                            session.agent_profile.as_str(),
                            history::title(session).unwrap_or(""),
                        ];
                        matches(&query, fields)
                    });
                }
                // Pinned first, then newest first.
                sessions.sort_by_key(|session| {
                    (
                        !session.pinned,
                        std::cmp::Reverse((session.started_at, session.id)),
                    )
                });
                (project, sessions)
            })
            .collect()
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
        self.sort_projects();
    }

    /// Pinned workspaces first. Within a group the daemon's order is by age (`created_at`), so
    /// an unpinned workspace returns to its own place; the sort is stable for equal ages.
    fn sort_projects(&mut self) {
        self.projects.sort_by_key(|p| (!p.pinned, p.created_at));
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
            default_shell: None,
            review_globs: None,
            created_at: 0,
            archived: false,
            base_ref: None,
            color: None,
            pinned: false,
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
            terminal_title: None,
            color: None,
            native_session_id: None,
            state,
            state_source: StateSource::Daemon,
            state_changed_at: started_at,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at,
            ended_at: None,
            pinned: false,
            plan_path: None,
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
    fn agent_badge_uses_the_profile_icon_else_the_first_letter() {
        assert_eq!(agent_badge(Some("π"), "omp"), "π");
        assert_eq!(agent_badge(None, "claude"), "C");
        // Uppercasing works beyond ASCII.
        assert_eq!(agent_badge(None, "ølm"), "Ø");
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
    fn a_renamed_session_shows_its_new_title_and_an_unknown_one_refreshes() {
        let api = project("api");
        let live = session(&api, 1, SessionState::Working);
        let mut sidebar = Sidebar::default();
        sidebar.load(vec![api.clone()], vec![live.clone()]);
        let renamed = Session {
            title: Some("Fix login".into()),
            ..live
        };
        assert_eq!(
            sidebar.apply(&Event::SessionUpdated(Box::new(renamed))),
            Change::Redraw
        );
        let Row::Session(row) = &sidebar.rows()[1] else {
            panic!("expected the session row");
        };
        assert_eq!(row.title.as_deref(), Some("Fix login"));
        let stranger = session(&api, 2, SessionState::Working);
        assert_eq!(
            sidebar.apply(&Event::SessionUpdated(Box::new(stranger))),
            Change::Refresh
        );
    }

    #[test]
    fn a_rename_reply_sets_only_the_title() {
        let api = project("api");
        let live = session(&api, 1, SessionState::Working);
        let mut sidebar = Sidebar::default();
        sidebar.load(vec![api.clone()], vec![live.clone()]);
        // The state moved on after the reply's snapshot was taken.
        sidebar.apply(&Event::SessionExited(mkrew_protocol::SessionExitedEvent {
            session_id: live.id,
            exit_code: Some(3),
        }));
        sidebar.set_title(live.id, Some("Fix login".into()));
        let session = &sidebar.sessions()[0];
        assert_eq!(session.title.as_deref(), Some("Fix login"));
        assert_eq!(session.state, SessionState::Exited);
        assert_eq!(session.exit_code, Some(3));
    }

    #[test]
    fn a_color_reply_sets_only_the_color() {
        let api = project("api");
        let live = session(&api, 1, SessionState::Working);
        let mut sidebar = Sidebar::default();
        sidebar.load(vec![api.clone()], vec![live.clone()]);
        sidebar.apply(&Event::SessionExited(mkrew_protocol::SessionExitedEvent {
            session_id: live.id,
            exit_code: Some(0),
        }));
        sidebar.set_color(live.id, Some("teal".into()));
        let shown = &sidebar.sessions()[0];
        assert_eq!(shown.color.as_deref(), Some("teal"));
        assert_eq!(
            shown.state,
            SessionState::Exited,
            "the reply's state is not applied"
        );
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

    /// Pinned workspaces lead (SPEC §10), each group in its usual (age) order; unpinning puts
    /// one back where its age says, not at the head of the unpinned ones. Ctrl+Tab between
    /// workspaces follows the same order.
    #[test]
    fn pinned_workspaces_lead_and_unpinning_restores_the_old_order() {
        let (a, b, c) = (
            Project {
                created_at: 1,
                ..project("a")
            },
            Project {
                created_at: 2,
                ..project("b")
            },
            Project {
                created_at: 3,
                ..project("c")
            },
        );
        let mut sidebar = Sidebar::default();
        sidebar.load(vec![a.clone(), b.clone(), c.clone()], Vec::new());
        let names = |sidebar: &Sidebar| -> Vec<String> {
            sidebar
                .rows()
                .iter()
                .filter_map(|row| match row {
                    Row::Workspace(row) => Some(row.name.clone()),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(names(&sidebar), ["a", "b", "c"]);

        sidebar.apply(&Event::ProjectUpdated(Project {
            pinned: true,
            ..c.clone()
        }));
        sidebar.apply(&Event::ProjectUpdated(Project {
            pinned: true,
            ..b.clone()
        }));
        assert_eq!(names(&sidebar), ["b", "c", "a"]);
        assert!(workspace(&sidebar.rows()[0]).pinned);
        assert_eq!(sidebar.workspace_at_offset(Some(c.id), 1), Some(a.id));

        sidebar.apply(&Event::ProjectUpdated(Project {
            pinned: false,
            ..b.clone()
        }));
        assert_eq!(names(&sidebar), ["c", "a", "b"]);

        // A fresh list puts pinned workspaces first too.
        let mut reloaded = Sidebar::default();
        reloaded.load(vec![a, Project { pinned: true, ..c }], Vec::new());
        assert_eq!(names(&reloaded), ["c", "a"]);
    }

    /// A pinned session stays above newer ones; within each group it is newest first.
    #[test]
    fn pinned_sessions_lead_their_workspace_newest_first() {
        let api = project("api");
        let old = session(&api, 1, SessionState::Idle);
        let mid = session(&api, 2, SessionState::Working);
        let new = session(&api, 3, SessionState::Working);
        let mut sidebar = Sidebar::default();
        sidebar.load(vec![api], vec![old.clone(), mid.clone(), new.clone()]);
        assert_eq!(session_ids(&sidebar.rows()), [new.id, mid.id, old.id]);

        sidebar.apply(&Event::SessionUpdated(Box::new(Session {
            pinned: true,
            ..old.clone()
        })));
        assert_eq!(session_ids(&sidebar.rows()), [old.id, new.id, mid.id]);
        sidebar.apply(&Event::SessionUpdated(Box::new(Session {
            pinned: true,
            ..mid.clone()
        })));
        assert_eq!(session_ids(&sidebar.rows()), [mid.id, old.id, new.id]);
        let rows = sidebar.rows();
        assert!(matches!(&rows[1], Row::Session(row) if row.pinned));
        assert!(matches!(&rows[3], Row::Session(row) if !row.pinned));
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

    /// An exited session cannot be opened, so it has no row; History, the Inbox and the pane
    /// placeholder still read it from `sessions()`. A session this list never saw asks for a
    /// refresh.
    #[test]
    fn an_exited_session_leaves_the_rows_but_stays_listed() {
        let api = project("api");
        let s = session(&api, 1, SessionState::Working);
        let mut sidebar = Sidebar::default();
        sidebar.load(vec![api.clone()], vec![s.clone()]);
        assert_eq!(session_ids(&sidebar.rows()), vec![s.id]);

        let exited = Event::SessionExited(SessionExitedEvent {
            session_id: s.id,
            exit_code: Some(1),
        });
        assert_eq!(sidebar.apply(&exited), Change::Redraw);
        assert_eq!(sidebar.rows()[1], Row::NoSessions { project_id: api.id });
        let kept = &sidebar.sessions()[0];
        assert_eq!(
            (kept.state, kept.exit_code),
            (SessionState::Exited, Some(1))
        );

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
                    summary: None,
                    color: None,
                    pinned: false,
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

    #[test]
    fn the_filter_matches_workspace_agent_and_title() {
        assert!(matches("api", ["API server", "claude", ""]));
        assert!(matches("codex", ["api", "codex", ""]));
        assert!(matches("login", ["api", "claude", "Fix Login"]));
        assert!(!matches("login", ["api", "claude", ""]));
        // Every word has to hit some field, not the same one.
        assert!(matches("api login", ["api", "claude", "Fix login"]));
        assert!(!matches("api nope", ["api", "claude", "Fix login"]));
        assert!(matches("", ["api", "claude", ""]));
    }

    #[test]
    fn a_filter_keeps_matching_sessions_and_opens_their_workspace() {
        let (api, web) = (project("api"), project("web"));
        let login = Session {
            title: Some("Fix login".into()),
            ..session(&api, 1, SessionState::Idle)
        };
        let other = session(&api, 2, SessionState::Idle);
        let codex = Session {
            agent_profile: "codex".into(),
            ..session(&web, 3, SessionState::Working)
        };
        let mut sidebar = Sidebar::default();
        sidebar.load(
            vec![api.clone(), web.clone()],
            vec![login.clone(), other, codex.clone()],
        );
        sidebar.toggle_all();
        assert!(!sidebar.any_expanded(), "everything was open, now closed");
        assert_eq!(sidebar.rows().len(), 2);

        sidebar.filter_mut().insert("login");
        let rows = sidebar.rows();
        assert_eq!(rows.len(), 2, "api and its one match: {rows:?}");
        assert!(workspace(&rows[0]).expanded, "a filter opens what it keeps");
        assert_eq!(session_ids(&rows), vec![login.id]);

        // A workspace name matches all of its sessions; so does an agent name.
        for query in ["WEB", "codex"] {
            sidebar.filter_mut().clear();
            sidebar.filter_mut().insert(query);
            assert_eq!(session_ids(&sidebar.rows()), vec![codex.id], "{query}");
        }
        sidebar.filter_mut().clear();
        sidebar.filter_mut().insert("zzz");
        assert!(sidebar.rows().is_empty());

        // Clearing the filter brings the user's own open/closed state back.
        sidebar.filter_mut().clear();
        assert!(
            sidebar
                .rows()
                .iter()
                .all(|row| !matches!(row, Row::Session(_)))
        );
        assert_eq!(sidebar.rows().len(), 2);
    }

    #[test]
    fn chips_count_what_matches_the_query_and_narrow_the_rows() {
        let api = project("api");
        let working = session(&api, 1, SessionState::Working);
        let asking = session(&api, 2, SessionState::NeedsInput);
        let done = session(&api, 3, SessionState::Done);
        let seen = session(&api, 4, SessionState::Done);
        let idle = session(&api, 5, SessionState::Idle);
        let mut sidebar = Sidebar::default();
        sidebar.load(
            vec![api.clone()],
            vec![
                working.clone(),
                asking.clone(),
                done.clone(),
                seen.clone(),
                idle,
            ],
        );
        let mut viewed = Viewed::default();
        viewed.view(&seen);

        let counts = sidebar.view(&viewed).counts;
        assert_eq!(
            counts,
            Counts {
                all: 5,
                working: 1,
                needs_you: 2
            }
        );
        // "Needs you" is exactly what the Inbox lists.
        let listed = crate::inbox::entries(sidebar.projects(), sidebar.sessions(), &viewed);
        assert_eq!(counts.needs_you, listed.len());

        sidebar.set_chip(Chip::NeedsYou);
        assert_eq!(
            session_ids(&sidebar.view(&viewed).rows),
            vec![done.id, asking.id]
        );
        sidebar.set_chip(Chip::Working);
        let view = sidebar.view(&viewed);
        assert_eq!(session_ids(&view.rows), vec![working.id]);
        assert_eq!(
            view.counts.all, 5,
            "chips count, they do not shrink their own numbers"
        );

        // The query narrows the counts too.
        sidebar.set_chip(Chip::All);
        sidebar.filter_mut().insert("nothing like it");
        assert_eq!(sidebar.view(&viewed).counts, Counts::default());
    }

    #[test]
    fn the_workspace_summary_names_what_is_going_on() {
        assert_eq!(workspace_summary(1, 0, 0).as_deref(), Some("1 working"));
        assert_eq!(workspace_summary(0, 0, 2).as_deref(), Some("2 idle"));
        assert_eq!(
            workspace_summary(1, 1, 2).as_deref(),
            Some("1 working · 1 needs you · 2 idle")
        );
        assert_eq!(workspace_summary(0, 0, 0), None);
    }

    #[test]
    fn a_workspace_row_carries_its_summary_and_a_session_row_only_a_time() {
        let api = project("api");
        let mut sidebar = Sidebar::default();
        sidebar.load(
            vec![api.clone()],
            vec![
                session(&api, 1, SessionState::Working),
                session(&api, 2, SessionState::Idle),
                session(&api, 3, SessionState::Idle),
            ],
        );
        let rows = sidebar.rows();
        assert_eq!(
            workspace(&rows[0]).summary.as_deref(),
            Some("1 working · 2 idle")
        );

        let row = |state| SessionRow {
            session_id: Ulid::new(),
            project_id: api.id,
            agent: "claude".into(),
            title: None,
            state,
            state_source: StateSource::Daemon,
            state_changed_at: 0,
            pinned: false,
        };
        // The status icon says the state; the right side is only the time (working / idle).
        assert_eq!(
            row(SessionState::Working).time_text(120_000).as_deref(),
            Some("2m")
        );
        assert_eq!(
            row(SessionState::Idle).time_text(17 * 60_000).as_deref(),
            Some("17m")
        );
        assert_eq!(row(SessionState::NeedsInput).time_text(120_000), None);
        assert_eq!(row(SessionState::Done).time_text(120_000), None);
    }

    #[test]
    fn toggle_all_closes_when_any_is_open_and_reveal_clears_what_hides_a_session() {
        let (api, web) = (project("api"), project("web"));
        let s = session(&web, 1, SessionState::Idle);
        let mut sidebar = Sidebar::default();
        sidebar.load(vec![api.clone(), web.clone()], vec![s.clone()]);
        assert!(sidebar.any_expanded(), "web has a live session");
        sidebar.toggle_all();
        assert!(!sidebar.any_expanded());
        sidebar.toggle_all();
        assert!(workspace(&sidebar.rows()[0]).expanded && sidebar.any_expanded());

        sidebar.toggle_all();
        sidebar.filter_mut().insert("api");
        sidebar.set_chip(Chip::Working);
        assert!(sidebar.filtering());
        sidebar.reveal(web.id);
        assert!(!sidebar.filtering());
        assert_eq!(session_ids(&sidebar.rows()), vec![s.id]);
    }
}
