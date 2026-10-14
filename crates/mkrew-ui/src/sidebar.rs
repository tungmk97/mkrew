//! Workspace sidebar view-model (SPEC §10, §11): workspaces, and per expanded workspace its
//! sessions with status. Run sessions are not session rows: the sidebar's RUN group shows
//! them (from [`crate::run::summary`]). Pure and tested headlessly; `main.rs` renders the rows.

use std::collections::{HashMap, HashSet};

use mkrew_protocol::{Event, Project, Session, SessionKind, SessionState, StateSource};
use ulid::Ulid;

use crate::history;
use crate::icons::AgentLogo;
use crate::inbox::{self, Viewed};
use crate::status;
use crate::text_box::Input;
use crate::theme::{self, Theme};

/// `[ui] sidebar_width` as drawn: kept between the limits.
pub fn saved_width(px: u16) -> f32 {
    f32::from(px).clamp(theme::SIDEBAR_MIN_WIDTH, theme::SIDEBAR_MAX_WIDTH)
}

/// The width while the right edge is dragged to `pointer_x` (window coordinates): the sidebar
/// is flush with the left edge. At most half of the window, so the terminals keep room.
pub fn width_from_pointer(window_width: f32, pointer_x: f32) -> f32 {
    let max = theme::SIDEBAR_MAX_WIDTH
        .min(window_width / 2.0)
        .max(theme::SIDEBAR_MIN_WIDTH);
    pointer_x.clamp(theme::SIDEBAR_MIN_WIDTH, max)
}

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

/// What an agent chip shows (ADR 0025, ADR 0058).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Badge {
    Logo(AgentLogo),
    Text(String),
}

/// The badge before an agent name: a shipped agent's logo, else the profile's `icon`, else the
/// first letter of its id, uppercased (profiles written before `icon` existed, or not loaded
/// yet). The logo wins over `icon`: copied profiles still carry the old sample glyph.
pub fn agent_badge(icon: Option<&str>, agent: &str) -> Badge {
    if let Some(logo) = AgentLogo::of(agent) {
        return Badge::Logo(logo);
    }
    Badge::Text(match icon {
        Some(icon) => icon.to_owned(),
        None => agent
            .chars()
            .next()
            .map(|c| c.to_uppercase().collect())
            .unwrap_or_default(),
    })
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
            Chip::Working => agent_in(session, SessionState::Working),
            Chip::NeedsYou => inbox::needs_you(session, viewed),
        }
    }
}

/// An agent session in `state`. A shell's working/idle flips with every keystroke, so the
/// sidebar never counts or marks it as working or idle.
fn agent_in(session: &Session, state: SessionState) -> bool {
    session.kind == SessionKind::Agent && session.state == state
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

/// Hover text of a workspace in the compact rail: the name, then its summary (if any) and its
/// folder, one per line.
pub fn workspace_tip(row: &WorkspaceRow) -> String {
    let mut lines = vec![row.name.as_str()];
    lines.extend(row.summary.as_deref());
    lines.push(&row.path);
    lines.join("\n")
}

/// Hover text of a session in the compact rail: `agent · full title` (or just the agent), then
/// the status in words, `working · 2m`. Rows are live sessions only, so there is no exit code.
pub fn session_tip(row: &SessionRow, now_ms: i64) -> String {
    let name = match &row.title {
        Some(title) => format!("{} · {title}", row.agent),
        None => row.agent.clone(),
    };
    let state = status::text(row.state, None, row.state_changed_at, now_ms);
    format!("{name}\n{state}")
}

/// How the sidebar shows (SPEC §10): full width, the compact rail, or hidden. Hiding keeps the
/// width it comes back at. In memory only: not remembered across restarts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Layout {
    hidden: bool,
    compact: bool,
}

impl Layout {
    pub fn hidden(self) -> bool {
        self.hidden
    }

    /// The compact rail is on screen (false while hidden).
    pub fn compact(self) -> bool {
        !self.hidden && self.compact
    }

    /// Toggle sidebar: hide, or show again at the last width.
    pub fn toggle_hidden(&mut self) {
        self.hidden = !self.hidden;
    }

    /// Something the sidebar draws needs it (the peek, the workspace menu): shown at the last width.
    pub fn show(&mut self) {
        self.hidden = false;
    }

    /// Toggle compact sidebar: full width ↔ rail; a hidden sidebar comes back as the rail.
    pub fn toggle_compact(&mut self) {
        self.compact = self.hidden || !self.compact;
        self.hidden = false;
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
    /// Agent sessions in `working`, `needs input` and `idle`.
    pub working: usize,
    pub asking: usize,
    pub idle: usize,
    /// Any session needs input (shown as `?`).
    pub needs_input: bool,
    /// `1 working` / `2 idle` …, `None` without sessions in those states.
    pub summary: Option<String>,
    /// A `WORKSPACE_COLORS` name.
    pub color: Option<String>,
    /// Pinned workspaces lead the list (SPEC §10).
    pub pinned: bool,
    /// No live session and no run command: drawn on the sidebar's "N empty workspaces" line.
    pub empty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    pub session_id: Ulid,
    pub project_id: Ulid,
    /// Agent profile id (`shell` for shell sessions).
    pub agent: String,
    pub title: Option<String>,
    /// A terminal (shell profile): its row keeps `powershell · title`, the chip alone does not
    /// say which shell it is.
    pub shell: bool,
    pub state: SessionState,
    pub state_source: StateSource,
    /// Unix ms of the last state change.
    pub state_changed_at: i64,
    /// Pinned sessions lead their workspace's list (SPEC §10).
    pub pinned: bool,
    /// Finished (done, or idle right after working) and not viewed since: the row is
    /// highlighted until its pane is focused ([`inbox::needs_you`]).
    pub unseen: bool,
}

impl SessionRow {
    /// The state mark before the chip: none while idle (the time says how long) unless the
    /// agent just finished unseen (then done's), and none for a shell's working (it is just
    /// typing). The caller keeps the slot, so chips stay aligned.
    pub fn mark(&self) -> Option<SessionState> {
        match self.state {
            SessionState::Idle if self.unseen => Some(SessionState::Done),
            SessionState::Idle => None,
            SessionState::Working if self.shell => None,
            state => Some(state),
        }
    }

    /// The only text on the row's right: the elapsed time of a working agent or the idle
    /// time of an idle one (`17m`); none for a shell. The hover says the state in words.
    pub fn time_text(&self, now_ms: i64) -> Option<String> {
        if self.shell {
            return None;
        }
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
        self.clear_filter();
    }

    /// Drops the filter text and the chip (the compact rail draws neither control).
    pub fn clear_filter(&mut self) {
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

    /// Positions from a sidebar drag, shown before the `session.updated` echoes arrive.
    pub fn set_position(&mut self, session_id: Ulid, position: f64) {
        self.update_session(session_id, |session| session.position = position);
    }

    /// The live session ids of `project_id`'s AGENTS (`shell` false) or TERMINAL group, in
    /// sidebar order (unfiltered: dragging is off while filtering).
    pub fn group_order(&self, project_id: Ulid, shell: bool) -> Vec<Ulid> {
        self.rows()
            .into_iter()
            .filter_map(|row| match row {
                Row::Session(row) if row.project_id == project_id && row.shell == shell => {
                    Some(row.session_id)
                }
                _ => None,
            })
            .collect()
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
                counts.working += usize::from(agent_in(session, SessionState::Working));
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
            let count = |state| sessions.iter().filter(|s| agent_in(s, state)).count();
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
                asking,
                idle,
                needs_input: asking > 0,
                summary: workspace_summary(working, asking, idle),
                color: project.color.clone(),
                pinned: project.pinned,
                empty: sessions.is_empty() && project.run_commands.is_empty(),
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
                    shell: session.kind == SessionKind::Shell,
                    state: session.state,
                    state_source: session.state_source,
                    state_changed_at: session.state_changed_at,
                    pinned: session.pinned,
                    unseen: session.state != SessionState::NeedsInput
                        && inbox::needs_you(session, viewed),
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

    /// Per workspace (in list order): its live sessions matching the filter text, agents before
    /// terminals (the sidebar's AGENTS and TERMINAL groups), each pinned first, then by dragged
    /// `position`, then newest first (a new session has position 0: on top).
    /// An exited session cannot be opened: no row (History and the Inbox read it from
    /// `sessions()`; ADR 0033). Run sessions are left out (the RUN group shows them).
    fn grouped(&self) -> Vec<(&Project, Vec<&Session>)> {
        let query = self.filter.trim().to_lowercase();
        let mut by_project: HashMap<Ulid, Vec<&Session>> = HashMap::new();
        for session in self.sessions.iter().filter(|session| {
            session.state != SessionState::Exited
                && mkrew_core::run::command_name(session).is_none()
        }) {
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
                // Agents, then terminals; within each pinned first, then dragged position,
                // then newest first.
                sessions.sort_by(|a, b| {
                    let key = |s: &Session| (s.kind == SessionKind::Shell, !s.pinned);
                    key(a)
                        .cmp(&key(b))
                        .then(a.position.total_cmp(&b.position))
                        .then((b.started_at, b.id).cmp(&(a.started_at, a.id)))
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

/// The item at `from` dragged onto `to` takes that slot; the new order as positions `1..=n`
/// (so every row of the group sorts after the 0 of a session never moved).
pub fn reordered<T: Copy>(order: &[T], from: usize, to: usize) -> Vec<(T, f64)> {
    let mut order = order.to_vec();
    if from < order.len() && to < order.len() {
        let moved = order.remove(from);
        order.insert(to, moved);
    }
    (order.into_iter().enumerate())
        .map(|(i, item)| (item, (i + 1) as f64))
        .collect()
}

/// The item at `from` dropped into the gap before `slot` (`0..=len`): its new index, `None` if
/// that gap is right beside it (nothing moves).
pub fn drop_index(from: usize, slot: usize) -> Option<usize> {
    match slot {
        _ if slot == from || slot == from + 1 => None,
        _ if slot > from => Some(slot - 1),
        _ => Some(slot),
    }
}

#[cfg(test)]
mod tests;
