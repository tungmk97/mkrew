//! `mkrew`: the desktop app (SPEC §10). Connects to the daemon (starting it if needed), lists the
//! workspaces and their sessions in the sidebar and shows the active workspace's terminal tabs,
//! each a split tree of panes. Only the panes on screen are attached.

// No console window next to the app in release builds (debug keeps stderr visible).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    AnyView, App, Bounds, Context, CursorStyle, Entity, FocusHandle, FontWeight, KeyDownEvent,
    MouseButton, PathPromptOptions, PromptLevel, SharedString, Subscription, TitlebarOptions,
    Window, WindowAppearance, WindowBounds, WindowOptions, anchored, deferred, div, px, relative,
    rgb, size,
};
use mkrew_core::Paths;
use mkrew_protocol::{
    AgentProfilesResult, Event, Outcome, Project, ProjectAddParams, Request, Session,
    SessionAttachResult, SessionKillParams, SessionKind, SessionListParams, SessionRef,
    SessionSpawnResult, SessionState, StateSource, TabCreateParams, TabListParams, TabRef,
    TabSetLayoutParams, TerminalTab,
};
use mkrew_ui::client::{Connection, Incoming};
use mkrew_ui::keymap::{Command, Keymap, Run};
use mkrew_ui::lifecycle::{self, CloseAction, CloseChoice};
use mkrew_ui::router::{Delivery, Router};
use mkrew_ui::sidebar::{self, Change, Row, SessionRow, Sidebar, WorkspaceRow};
use mkrew_ui::split::Axis;
use mkrew_ui::status;
use mkrew_ui::tabs::{Tab, Tabs, newest_unplaced, on_screen, rename_params};
use mkrew_ui::terminal::find::{FindKey, find_key};
use mkrew_ui::terminal::view::TerminalPane;
use mkrew_ui::theme::{self, Theme, font_size, opacity, radius, space, weight};
use mkrew_ui::workspace_menu::{self, Agents, WorkspaceMenu};
use mkrew_ui::zed;
use tracing_subscriber::EnvFilter;
use ulid::Ulid;

/// Longest wait for the frame showing fed output before draining again anyway (e.g. the
/// window is minimized and never paints).
const PAINT_TIMEOUT: Duration = Duration::from_millis(50);
/// Most incoming frames handled per drain, so one drain never starves painting.
const DRAIN_BATCH: usize = 256;

/// Title of a workspace's first tab; later ones are numbered.
const FIRST_TAB_TITLE: &str = "Terminal";

fn main() {
    let filter = EnvFilter::try_from_env("MKREW_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .init();
    let paths = match Paths::from_env() {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("mkrew: {error}");
            std::process::exit(2);
        }
    };
    let (keymap, keymap_errors) = Keymap::load(&paths.keymap_file());
    for error in &keymap_errors {
        tracing::warn!("{error}; using the default for it");
    }
    gpui_platform::application().run(move |cx: &mut App| {
        cx.bind_keys(keymap.key_bindings());
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(theme::WINDOW_WIDTH), px(theme::WINDOW_HEIGHT)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("Mkrew".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let opened = cx.open_window(options, |window, cx| {
            cx.new(|cx| Shell::new(paths, keymap, &keymap_errors, window, cx))
        });
        if let Err(error) = opened {
            eprintln!("mkrew: cannot open window: {error:#}");
            cx.quit();
            return;
        }
        // Sessions live in the daemon, so closing the window simply ends the app.
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}

enum Status {
    Connecting,
    Connected,
    Disconnected,
    /// The daemon cannot be reached.
    Failed(String),
    /// Listing workspaces or a workspace's tabs failed.
    ListFailed(String),
}

struct Shell {
    status: Status,
    /// Dropping it closes the connection.
    connection: Option<Rc<Connection>>,
    sidebar: Sidebar,
    /// A `project.list` + `session.list` is in flight.
    refreshing: bool,
    /// Another refresh was asked for while one was in flight.
    refresh_again: bool,
    /// Last failure of a sidebar action (adding a workspace), shown under its header.
    sidebar_error: Option<String>,
    /// The workspace whose tabs are on screen.
    workspace: Option<Ulid>,
    /// Tabs per workspace, loaded when the workspace is first shown.
    tabs: HashMap<Ulid, Tabs>,
    /// Workspaces whose `tab.list` (or first `tab.create`) is in flight.
    loading_tabs: HashSet<Ulid>,
    /// `tab.updated` events of loading workspaces: the `tab.list` reply may predate them, so
    /// they are applied after it (events and replies arrive on different channels).
    early_tab_events: Vec<Event>,
    /// Session to show once its workspace's tabs are loaded (clicked in the sidebar).
    reveal: Option<Ulid>,
    /// Last failure of a tab action (new tab), shown in the tab bar.
    tab_error: Option<String>,
    /// Which session frames go to a pane, and buffering while attaches are in flight.
    router: Router,
    /// Attached panes of the sessions on screen.
    panes: HashMap<Ulid, Entity<TerminalPane>>,
    /// Failed `session.attach`es of sessions on screen, shown in place of their pane.
    attach_errors: HashMap<Ulid, String>,
    /// Given to panes; signalled after each terminal paint.
    painted: async_channel::Sender<()>,
    /// Focused while no pane is, so window-level actions (Add workspace) still dispatch.
    focus_handle: FocusHandle,
    /// The tab whose title is being edited inline, and the draft title.
    renaming: Option<(Ulid, String)>,
    /// Focused while renaming; losing it commits the draft.
    rename_focus: FocusHandle,
    _rename_blur: Subscription,
    /// The bound shortcuts, for tooltips and labels.
    keymap: Keymap,
    /// What was wrong with `keymap.toml` at start (those entries use their defaults).
    keymap_error: Option<String>,
    /// The open workspace menu (Open in Zed, default agent).
    workspace_menu: Option<WorkspaceMenu>,
}

impl Shell {
    fn new(
        paths: Paths,
        keymap: Keymap,
        keymap_errors: &[mkrew_ui::keymap::KeymapError],
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        cx.observe_window_appearance(window, |_, _, cx| cx.notify())
            .detach();
        // Minimized → detach every pane; restored → re-attach from daemon snapshots (§10).
        cx.observe_window_visibility(window, |shell, _, window, cx| {
            shell.sync_panes(window, cx);
            cx.notify();
        })
        .detach();
        // Spinner and elapsed/idle times move on their own (§11).
        cx.spawn_in(window, async move |this, cx| {
            let mut last = status::now_ms();
            loop {
                cx.background_executor().timer(theme::STATUS_TICK).await;
                let now = status::now_ms();
                let ticked = this.update_in(cx, |shell, window, cx| {
                    if window.is_visible()
                        && status::redraw_due(shell.sidebar.sessions(), last, now)
                    {
                        cx.notify();
                    }
                });
                last = now;
                if ticked.is_err() {
                    return;
                }
            }
        })
        .detach();
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        let rename_focus = cx.focus_handle();
        let rename_blur = cx.on_blur(&rename_focus, window, |shell, window, cx| {
            shell.finish_rename(true, window, cx);
        });
        let (painted, painted_rx) = async_channel::bounded(1);
        cx.spawn_in(window, async move |this, cx| {
            // Blocking (polls a starting daemon for up to 10 s): off the UI thread.
            let connected = cx
                .background_executor()
                .spawn(async move { mkrew_ui::connect::connect(&paths) })
                .await;
            let connection = match connected {
                Ok(connection) => Rc::new(connection),
                Err(error) => {
                    let _ = this.update(cx, |shell, cx| {
                        shell.status = Status::Failed(format!("{error:#}"));
                        cx.notify();
                    });
                    return;
                }
            };
            let incoming = connection.incoming();
            tracing::info!(
                daemon_version = %connection.hello().daemon_version,
                "connected to the daemon"
            );
            let loaded = this.update_in(cx, |shell, window, cx| {
                shell.connection = Some(connection);
                shell.refresh(window, cx);
            });
            if loaded.is_err() {
                return;
            }
            // Drain at once when frames arrive (low key latency); after feeding output, wait
            // until that frame is painted so a flood is fed once per frame.
            while let Ok(first) = incoming.recv().await {
                // A paint from an earlier frame must not end the wait below.
                let _ = painted_rx.try_recv();
                let mut batch = vec![first];
                while batch.len() < DRAIN_BATCH
                    && let Ok(next) = incoming.try_recv()
                {
                    batch.push(next);
                }
                match this.update_in(cx, |shell, window, cx| shell.dispatch(batch, window, cx)) {
                    Ok(true) => {
                        let painted = async {
                            let _ = painted_rx.recv().await;
                        };
                        let timeout = cx.background_executor().timer(PAINT_TIMEOUT);
                        futures_lite::future::or(painted, timeout).await;
                    }
                    Ok(false) => {}
                    Err(_) => return,
                }
            }
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.status = Status::Disconnected;
                shell.connection = None;
                shell.workspace = None;
                shell.tabs.clear();
                shell.loading_tabs.clear();
                shell.early_tab_events.clear();
                shell.reveal = None;
                shell.tab_error = None;
                shell.router.clear();
                shell.panes.clear();
                shell.attach_errors.clear();
                shell.renaming = None;
                shell.workspace_menu = None;
                window.focus(&shell.focus_handle, cx);
                cx.notify();
            });
        })
        .detach();
        Self {
            status: Status::Connecting,
            connection: None,
            sidebar: Sidebar::default(),
            refreshing: false,
            refresh_again: false,
            sidebar_error: None,
            workspace: None,
            tabs: HashMap::new(),
            loading_tabs: HashSet::new(),
            early_tab_events: Vec::new(),
            reveal: None,
            tab_error: None,
            router: Router::default(),
            panes: HashMap::new(),
            attach_errors: HashMap::new(),
            painted,
            focus_handle,
            renaming: None,
            rename_focus,
            _rename_blur: rename_blur,
            keymap,
            keymap_error: keymap_errors
                .first()
                .map(|first| match keymap_errors.len() {
                    1 => format!("{first}; using the default. Fix keymap.toml and restart."),
                    n => format!(
                        "{first} (+{} more); using the defaults. Fix keymap.toml and restart.",
                        n - 1
                    ),
                }),
            workspace_menu: None,
        }
    }

    /// Re-reads workspaces and sessions. The first time, shows the workspace of the most
    /// recently started live session.
    fn refresh(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if self.refreshing {
            self.refresh_again = true;
            return;
        }
        self.refreshing = true;
        self.sidebar.begin_load();
        let projects = connection.call::<Vec<Project>>(Request::ProjectList);
        let sessions =
            connection.call::<Vec<Session>>(Request::SessionList(SessionListParams::default()));
        cx.spawn_in(window, async move |this, cx| {
            let listed = match (projects.await, sessions.await) {
                (Ok(projects), Ok(sessions)) => Ok((projects, sessions)),
                (Err(error), _) | (_, Err(error)) => Err(error),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.refreshing = false;
                match listed {
                    Ok((projects, sessions)) => {
                        shell.sidebar.load(projects, sessions);
                        shell.status = Status::Connected;
                        let current = shell
                            .workspace
                            .filter(|id| shell.sidebar.projects().iter().any(|p| p.id == *id));
                        match current.or_else(|| shell.initial_workspace()) {
                            Some(project_id) => {
                                // A new session (e.g. spawned by `mk`) fills an empty tab.
                                shell.fill_empty_tab(project_id, window, cx);
                                shell.activate_workspace(project_id, window, cx);
                            }
                            None => {
                                shell.workspace = None;
                                shell.sync_panes(window, cx);
                            }
                        }
                    }
                    Err(error) => {
                        tracing::warn!("listing workspaces failed: {error:#}");
                        shell.sidebar.cancel_load();
                        shell.list_failed(format!("{error:#}"));
                    }
                }
                cx.notify();
                if std::mem::take(&mut shell.refresh_again) {
                    shell.refresh(window, cx);
                }
            });
        })
        .detach();
    }

    /// The workspace of the most recently started live session, else the first one.
    fn initial_workspace(&self) -> Option<Ulid> {
        self.sidebar
            .sessions()
            .iter()
            .filter(|session| session.state != SessionState::Exited)
            .max_by_key(|session| session.started_at)
            .map(|session| session.project_id)
            .or_else(|| self.sidebar.projects().first().map(|project| project.id))
    }

    /// Shows the tabs of `project_id`, loading them the first time.
    fn activate_workspace(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace = Some(project_id);
        if !self.tabs.contains_key(&project_id) {
            self.load_tabs(project_id, window, cx);
        }
        self.sync_panes(window, cx);
        cx.notify();
    }

    /// `tab.list` for a workspace; a workspace without tabs gets one.
    fn load_tabs(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if !self.loading_tabs.insert(project_id) {
            return;
        }
        let listed =
            connection.call::<Vec<TerminalTab>>(Request::TabList(TabListParams { project_id }));
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await;
            let _ = this.update_in(cx, |shell, window, cx| match listed {
                Ok(list) if list.is_empty() => shell.create_tab(project_id, None, window, cx),
                Ok(list) => {
                    let mut tabs = Tabs::new(project_id);
                    tabs.load(list);
                    shell.tabs.insert(project_id, tabs);
                    shell.tabs_loaded(project_id, window, cx);
                }
                Err(error) => {
                    tracing::warn!(%project_id, "tab.list failed: {error:#}");
                    shell.loading_tabs.remove(&project_id);
                    shell.early_tab_events.clear();
                    if shell.project_of_reveal() == Some(project_id) {
                        shell.reveal = None;
                    }
                    shell.list_failed(format!("{error:#}"));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Adds a tab to a workspace and activates it; `start` then spawns a session into it.
    fn create_tab(
        &mut self,
        project_id: Ulid,
        start: Option<SessionKind>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.tab_error = None;
        let count = self
            .tabs
            .get(&project_id)
            .map_or(0, |tabs| tabs.tabs().len());
        let title = match count {
            0 => FIRST_TAB_TITLE.to_owned(),
            count => format!("{FIRST_TAB_TITLE} {}", count + 1),
        };
        let created = connection.call::<TerminalTab>(Request::TabCreate(TabCreateParams {
            project_id,
            title,
            position: None,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let created = created.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let first = shell.loading_tabs.contains(&project_id);
                match created {
                    Ok(tab) => {
                        let tab_id = tab.id;
                        let tabs = shell
                            .tabs
                            .entry(project_id)
                            .or_insert_with(|| Tabs::new(project_id));
                        // Its `tab.updated` echo is then a no-op.
                        tabs.apply(&Event::TabUpdated(tab));
                        tabs.activate(tab_id);
                        if first {
                            shell.tabs_loaded(project_id, window, cx);
                        } else {
                            shell.sync_panes(window, cx);
                        }
                        if let Some(kind) = start
                            && shell.workspace == Some(project_id)
                        {
                            shell.new_pane(kind, Axis::Vertical, window, cx);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "tab.create failed: {error:#}");
                        let error = format!("{error:#}");
                        if first {
                            shell.loading_tabs.remove(&project_id);
                            shell.list_failed(error);
                        } else {
                            shell.tab_error = Some(format!("Cannot add a tab: {error}"));
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The tabs of a workspace arrived: show the session waiting for them, else fill an empty
    /// first tab, and attach what is on screen.
    fn tabs_loaded(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.loading_tabs.remove(&project_id);
        let (early, rest) = std::mem::take(&mut self.early_tab_events)
            .into_iter()
            .partition(
                |event| matches!(event, Event::TabUpdated(tab) if tab.project_id == project_id),
            );
        self.early_tab_events = rest;
        if let Some(tabs) = self.tabs.get_mut(&project_id) {
            for event in early {
                tabs.apply(&event);
            }
        }
        if let Some(session) = self.reveal
            && self.project_of(session) == Some(project_id)
        {
            self.reveal = None;
            self.place_session(project_id, session, Axis::Vertical, window, cx);
        }
        self.fill_empty_tab(project_id, window, cx);
        self.sync_panes(window, cx);
        cx.notify();
    }

    fn project_of(&self, session: Ulid) -> Option<Ulid> {
        self.sidebar
            .sessions()
            .iter()
            .find(|s| s.id == session)
            .map(|s| s.project_id)
    }

    fn project_of_reveal(&self) -> Option<Ulid> {
        self.reveal.and_then(|session| self.project_of(session))
    }

    /// A list failed; ignored once disconnected (that status says more).
    fn list_failed(&mut self, error: String) {
        if self.connection.is_some() {
            self.status = Status::ListFailed(error);
        }
    }

    fn is_exited(&self, session: Ulid) -> bool {
        self.sidebar
            .sessions()
            .iter()
            .any(|s| s.id == session && s.state == SessionState::Exited)
    }

    /// Shows a session picked in the sidebar: focuses its pane, or places it next to the
    /// focused pane of its workspace's active tab.
    fn open_session(&mut self, session: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.project_of(session) else {
            return;
        };
        if self.tabs.contains_key(&project_id) {
            self.place_session(project_id, session, Axis::Vertical, window, cx);
        } else {
            self.reveal = Some(session);
        }
        self.activate_workspace(project_id, window, cx);
    }

    /// Focuses the pane of `session` (activating its tab) or splits the focused pane along
    /// `axis` to show it.
    fn place_session(
        &mut self,
        project_id: Ulid,
        session: Ulid,
        axis: Axis,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(tabs) = self.tabs.get_mut(&project_id) else {
            return;
        };
        if tabs.focus(session) {
            return;
        }
        if let Some(write) = tabs.split_focused(axis, session) {
            self.save_layout(project_id, write, window, cx);
        }
    }

    /// An active tab without panes shows the workspace's newest live session not in any tab.
    fn fill_empty_tab(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let newest = self
            .tabs
            .get(&project_id)
            .and_then(|tabs| newest_unplaced(tabs, self.sidebar.sessions()));
        if let Some(session) = newest {
            self.place_session(project_id, session, Axis::Vertical, window, cx);
        }
    }

    /// Persists a layout change (`tab.set_layout`).
    fn save_layout(
        &mut self,
        project_id: Ulid,
        write: TabSetLayoutParams,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let saved = connection.call::<()>(Request::TabSetLayout(write.clone()));
        cx.spawn_in(window, async move |this, cx| {
            if let Err(error) = saved.await {
                tracing::warn!(tab_id = %write.tab_id, "tab.set_layout failed: {error:#}");
                let _ = this.update(cx, |shell, cx| {
                    if let Some(tabs) = shell.tabs.get_mut(&project_id) {
                        tabs.write_failed(&write);
                    }
                    shell.tab_error = Some(format!("Cannot save the layout: {error:#}"));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn active_tabs_mut(&mut self) -> Option<&mut Tabs> {
        self.tabs.get_mut(&self.workspace?)
    }

    /// The pane keyboard input goes to: the focused pane of the active tab.
    fn focused_session(&self) -> Option<Ulid> {
        let tabs = self.tabs.get(&self.workspace?)?;
        tabs.active()?.focused()
    }

    /// Attaches the sessions on screen (live ones, or already attached; none while the window
    /// is minimized) and detaches all others (SPEC §10), then focuses the focused pane.
    fn sync_panes(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let visible = on_screen(
            window.is_visible(),
            self.workspace.and_then(|id| self.tabs.get(&id)),
        );
        for session in self.router.wanted() {
            if visible.contains(&session) {
                continue;
            }
            if self.router.forget(session)
                && let Some(connection) = &self.connection
            {
                detach(connection, session, cx);
            }
            self.panes.remove(&session);
        }
        self.attach_errors.retain(|id, _| visible.contains(id));
        for &session in &visible {
            let skip = self.router.is_wanted(session)
                || self.attach_errors.contains_key(&session)
                || self.is_exited(session);
            if !skip {
                self.attach(session, window, cx);
            }
        }
        self.focus_pane(window, cx);
    }

    fn focus_pane(&self, window: &mut Window, cx: &mut Context<'_, Self>) {
        match self
            .focused_session()
            .and_then(|session| self.panes.get(&session))
        {
            Some(pane) => pane.update(cx, |pane, cx| pane.focus(window, cx)),
            None => window.focus(&self.focus_handle, cx),
        }
    }

    /// `session.attach`; frames arriving before the reply are buffered by the router.
    fn attach(&mut self, session: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(generation) = self.router.begin_attach(session) else {
            return;
        };
        let attach = connection.call::<SessionAttachResult>(Request::SessionAttach(SessionRef {
            session_id: session,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let attached = attach.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let attached = match attached {
                    Ok(attached) => attached,
                    Err(error) => {
                        if shell.router.fail_attach(session, generation) {
                            tracing::warn!(%session, "session.attach failed: {error:#}");
                            shell.attach_errors.insert(session, format!("{error:#}"));
                            cx.notify();
                        }
                        return;
                    }
                };
                let Some(buffered) = shell.router.finish_attach(session, generation) else {
                    // Hidden meanwhile (and not wanted again, or the router would have kept
                    // this attach): drop the daemon's attachment.
                    if !shell.router.is_wanted(session) {
                        detach(&connection, session, cx);
                    }
                    return;
                };
                let snapshot = buffered.snapshot.unwrap_or(attached.snapshot);
                let painted = shell.painted.clone();
                let pane = cx.new(|cx| {
                    let mut pane =
                        TerminalPane::new(session, &snapshot, connection, painted, window, cx);
                    pane.feed(&buffered.output);
                    if let Some(exit_code) = buffered.exited {
                        pane.set_exited(exit_code);
                    }
                    pane
                });
                shell.panes.insert(session, pane);
                if shell.focused_session() == Some(session) {
                    shell.focus_pane(window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Picks a folder and adds it as a workspace (`project.add`).
    fn add_workspace(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.sidebar_error = None;
        cx.notify();
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add workspace".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let path = match picked.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) | Err(_) => None,
                Ok(Err(error)) => {
                    let _ = this.update(cx, |shell, cx| {
                        shell.sidebar_error =
                            Some(format!("Cannot open the folder picker: {error:#}"));
                        cx.notify();
                    });
                    None
                }
            };
            let Some(path) = path else {
                return;
            };
            let added = connection
                .call::<Project>(Request::ProjectAdd(ProjectAddParams {
                    path: path.to_string_lossy().into_owned(),
                    name: None,
                }))
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                match added {
                    Ok(project) => {
                        let project_id = project.id;
                        shell.sidebar_error = None;
                        shell.sidebar.add_project(project);
                        shell.activate_workspace(project_id, window, cx);
                    }
                    Err(error) => {
                        shell.sidebar_error = Some(format!("Cannot add the workspace: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Opens the menu of `project_id` (closing any other) and lists the agent profiles for it.
    fn open_workspace_menu(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.workspace_menu = Some(WorkspaceMenu::new(project_id));
        cx.notify();
        let listed = connection.call::<AgentProfilesResult>(Request::AgentProfiles);
        cx.spawn_in(window, async move |this, cx| {
            let agents = match listed.await {
                Ok(listed) => Agents::Loaded(listed.profiles),
                Err(error) => Agents::Failed(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                // Unless the menu closed (or moved to another workspace) meanwhile.
                if let Some(menu) = shell
                    .workspace_menu
                    .as_mut()
                    .filter(|menu| menu.project_id == project_id)
                {
                    menu.agents = agents;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Opens the folder of `project_id` in Zed; a failure (e.g. `zed` missing) shows in the
    /// sidebar.
    fn open_in_zed(&mut self, project_id: Ulid, cx: &mut Context<'_, Self>) {
        self.workspace_menu = None;
        let Some(project) = self.sidebar.projects().iter().find(|p| p.id == project_id) else {
            return;
        };
        match zed::open(Path::new(&project.path), None) {
            Ok(()) => self.sidebar_error = None,
            Err(error) => {
                tracing::warn!(%project_id, "open in Zed failed: {error}");
                self.sidebar_error = Some(error.to_string());
            }
        }
        cx.notify();
    }

    /// Makes `agent` the default agent of `project_id` (`project.update`); the sidebar picks
    /// the change up from `project.updated`.
    fn set_default_agent(
        &mut self,
        project_id: Ulid,
        agent: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        cx.notify();
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let params = workspace_menu::set_default_agent(project_id, &agent);
        let updated = connection.call::<Project>(Request::ProjectUpdate(params));
        cx.spawn_in(window, async move |this, cx| {
            let updated = updated.await;
            let _ = this.update(cx, |shell, cx| {
                match updated {
                    Ok(_) => shell.sidebar_error = None,
                    Err(error) => {
                        tracing::warn!(%project_id, "project.update failed: {error:#}");
                        shell.sidebar_error =
                            Some(format!("Cannot change the default agent: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Starts an agent (the workspace's default agent) or a shell in the workspace folder and
    /// shows it next to the focused pane of the active tab, split along `axis`.
    fn new_pane(
        &mut self,
        kind: SessionKind,
        axis: Axis,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(project_id) = self.workspace else {
            return;
        };
        let Some(tab_id) = self
            .tabs
            .get(&project_id)
            .and_then(|tabs| tabs.active())
            .map(|tab| tab.id)
        else {
            return;
        };
        self.tab_error = None;
        cx.notify();
        let params = lifecycle::spawn_params(project_id, Some(tab_id), kind);
        let spawned = connection.call::<SessionSpawnResult>(Request::SessionSpawn(params));
        cx.spawn_in(window, async move |this, cx| {
            let spawned = spawned.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                match spawned {
                    Ok(spawned) => {
                        // Next to the focused pane of the tab active now (the user may have
                        // switched tabs meanwhile), unless the daemon placed it already.
                        shell.place_session(project_id, spawned.session_id, axis, window, cx);
                        shell.sync_panes(window, cx);
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "session.spawn failed: {error:#}");
                        let what = match kind {
                            SessionKind::Agent => "an agent",
                            SessionKind::Shell => "a shell",
                        };
                        shell.tab_error = Some(format!("Cannot start {what}: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A shortcut (SPEC §13). Terminal commands (paste) are handled by the focused pane.
    fn run(&mut self, Run(command): &Run, window: &mut Window, cx: &mut Context<'_, Self>) {
        // Shortcuts still fire while a title is edited: save it first (closing its tab, …).
        self.finish_rename(true, window, cx);
        match *command {
            Command::AddWorkspace => self.add_workspace(window, cx),
            Command::Workspace(n) => {
                let nth = self.sidebar.projects().get(usize::from(n) - 1);
                if let Some(project_id) = nth.map(|project| project.id) {
                    self.activate_workspace(project_id, window, cx);
                }
            }
            Command::NextWorkspace => self.cycle_workspace(1, window, cx),
            Command::PreviousWorkspace => self.cycle_workspace(-1, window, cx),
            Command::NewShellTab => self.new_tab(SessionKind::Shell, window, cx),
            Command::NewAgentTab => self.new_tab(SessionKind::Agent, window, cx),
            Command::NextTab => self.switch_tab(|tabs| tabs.activate_offset(1), window, cx),
            Command::PreviousTab => self.switch_tab(|tabs| tabs.activate_offset(-1), window, cx),
            Command::Tab(n) => self.switch_tab(
                |tabs| {
                    tabs.activate_nth(usize::from(n) - 1);
                },
                window,
                cx,
            ),
            Command::RenameTab => self.start_rename(window, cx),
            Command::CloseTab => self.close_tab(window, cx),
            Command::SplitRight => self.split(Axis::Vertical, window, cx),
            Command::SplitDown => self.split(Axis::Horizontal, window, cx),
            Command::Focus(direction) => {
                if self
                    .active_tabs_mut()
                    .is_some_and(|tabs| tabs.focus_neighbour(direction))
                {
                    // A focus move ends a zoom: the hidden panes come back (and re-attach).
                    self.sync_panes(window, cx);
                    cx.notify();
                }
            }
            Command::Resize(direction) => {
                let Some(project_id) = self.workspace else {
                    return;
                };
                let write = self
                    .active_tabs_mut()
                    .and_then(|tabs| tabs.resize_focused(direction));
                if let Some(write) = write {
                    self.save_layout(project_id, write, window, cx);
                    cx.notify();
                }
            }
            Command::ZoomPane => {
                if self.active_tabs_mut().is_some_and(Tabs::toggle_zoom) {
                    self.sync_panes(window, cx);
                    cx.notify();
                }
            }
            Command::ClosePane => self.close_focused_pane(window, cx),
            Command::OpenInZed => {
                if let Some(project_id) = self.workspace {
                    self.open_in_zed(project_id, cx);
                }
            }
            // Handled by the focused terminal pane.
            Command::Copy
            | Command::Paste
            | Command::PageUp
            | Command::PageDown
            | Command::Find => {}
        }
    }

    fn cycle_workspace(&mut self, offset: isize, window: &mut Window, cx: &mut Context<'_, Self>) {
        if let Some(project_id) = self.sidebar.workspace_at_offset(self.workspace, offset) {
            self.activate_workspace(project_id, window, cx);
        }
    }

    /// Changes the active tab of the shown workspace.
    fn switch_tab(
        &mut self,
        change: impl FnOnce(&mut Tabs),
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(tabs) = self.active_tabs_mut() {
            change(tabs);
            self.sync_panes(window, cx);
            cx.notify();
        }
    }

    /// A new tab in the shown workspace with an agent or a shell in it.
    fn new_tab(&mut self, kind: SessionKind, window: &mut Window, cx: &mut Context<'_, Self>) {
        if let Some(project_id) = self.workspace
            && self.tabs.contains_key(&project_id)
        {
            self.create_tab(project_id, Some(kind), window, cx);
        }
    }

    /// Splits the focused pane with a new session of the same kind.
    fn split(&mut self, axis: Axis, window: &mut Window, cx: &mut Context<'_, Self>) {
        let focused = self
            .focused_session()
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));
        let kind = lifecycle::split_kind(focused);
        self.new_pane(kind, axis, window, cx);
    }

    /// Starts editing the active tab's title inline, starting from the current one.
    fn start_rename(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(tab) = self
            .workspace
            .and_then(|id| self.tabs.get(&id))
            .and_then(|tabs| tabs.active())
        else {
            return;
        };
        self.renaming = Some((tab.id, tab.title.clone()));
        window.focus(&self.rename_focus, cx);
        cx.notify();
    }

    /// Ends the inline edit: `commit` sends `tab.rename` (the title updates with the
    /// `tab.updated` event); blank or unchanged drafts are dropped. Focus goes back to the pane.
    fn finish_rename(&mut self, commit: bool, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some((tab_id, draft)) = self.renaming.take() else {
            return;
        };
        // Any workspace: switching workspaces mid-edit commits through the blur.
        let params = self
            .tabs
            .values()
            .find_map(|tabs| tabs.tab(tab_id))
            .and_then(|tab| rename_params(tab, &draft));
        if let (true, Some(params), Some(connection)) = (commit, params, self.connection.clone()) {
            self.tab_error = None;
            let renamed = connection.call::<TerminalTab>(Request::TabRename(params));
            cx.spawn_in(window, async move |this, cx| {
                let renamed = renamed.await;
                let _ = this.update(cx, |shell, cx| {
                    match renamed {
                        // Shown at once, without waiting for the `tab.updated` echo.
                        Ok(tab) => {
                            if let Some(tabs) = shell.tabs.get_mut(&tab.project_id) {
                                tabs.apply(&Event::TabUpdated(tab));
                            }
                        }
                        Err(error) => {
                            tracing::warn!(%tab_id, "tab.rename failed: {error:#}");
                            shell.tab_error = Some(format!("Cannot rename the tab: {error:#}"));
                        }
                    }
                    cx.notify();
                });
            })
            .detach();
        }
        if self.rename_focus.is_focused(window) {
            self.focus_pane(window, cx);
        }
        cx.notify();
    }

    /// Keys typed while renaming edit the draft; Enter commits, Esc cancels.
    fn rename_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = find_key(&event.keystroke) else {
            return;
        };
        let Some((_, draft)) = self.renaming.as_mut() else {
            return;
        };
        match key {
            FindKey::Close => self.finish_rename(false, window, cx),
            FindKey::Older | FindKey::Newer => self.finish_rename(true, window, cx),
            FindKey::Backspace => {
                draft.pop();
                cx.notify();
            }
            FindKey::Insert(text) => {
                draft.push_str(&text);
                cx.notify();
            }
        }
    }

    /// `tab.close` for the active tab. Its sessions keep running (reachable from the sidebar);
    /// closing the last tab leaves a fresh empty one.
    fn close_tab(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(project_id) = self.workspace else {
            return;
        };
        let Some(tab_id) = self
            .tabs
            .get(&project_id)
            .and_then(|tabs| tabs.active())
            .map(|tab| tab.id)
        else {
            return;
        };
        self.tab_error = None;
        let closed = connection.call::<()>(Request::TabClose(TabRef { tab_id }));
        cx.spawn_in(window, async move |this, cx| {
            let closed = closed.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                match closed {
                    Ok(()) => {
                        let Some(tabs) = shell.tabs.get_mut(&project_id) else {
                            return;
                        };
                        tabs.remove(tab_id);
                        if tabs.is_empty() {
                            shell.create_tab(project_id, None, window, cx);
                        }
                        shell.sync_panes(window, cx);
                    }
                    Err(error) => {
                        tracing::warn!(%tab_id, "tab.close failed: {error:#}");
                        shell.tab_error = Some(format!("Cannot close the tab: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn close_focused_pane(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if let Some(session) = self.focused_session() {
            self.close_pane(session, window, cx);
        }
    }

    /// Closes the pane of `session`: an exited session's pane just goes, a shell is killed,
    /// a running agent asks whether to kill it or only hide the pane (SPEC §10).
    fn close_pane(&mut self, session: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let known = self.sidebar.sessions().iter().find(|s| s.id == session);
        match lifecycle::close_action(known) {
            CloseAction::Remove => self.remove_pane(session, window, cx),
            CloseAction::Kill => {
                self.remove_pane(session, window, cx);
                self.kill(session, window, cx);
            }
            CloseAction::Ask => {
                let answer = window.prompt(
                    PromptLevel::Warning,
                    "This session is still running.",
                    Some("Kill it, or hide the pane and keep it running in the sidebar."),
                    &CloseChoice::ANSWERS,
                    cx,
                );
                cx.spawn_in(window, async move |this, cx| {
                    let choice = CloseChoice::from_answer(answer.await.ok());
                    let _ = this.update_in(cx, |shell, window, cx| match choice {
                        CloseChoice::Kill => {
                            shell.remove_pane(session, window, cx);
                            // It may have exited while the dialog was open.
                            if !shell.is_exited(session) {
                                shell.kill(session, window, cx);
                            }
                        }
                        CloseChoice::Hide => shell.remove_pane(session, window, cx),
                        CloseChoice::Cancel => {}
                    });
                })
                .detach();
            }
        }
    }

    /// Takes the pane of `session` out of its tab's layout (the session itself is untouched).
    fn remove_pane(&mut self, session: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let closed = self
            .tabs
            .values_mut()
            .find_map(|tabs| Some((tabs.project_id(), tabs.close_pane(session)?)));
        if let Some((project_id, write)) = closed {
            self.save_layout(project_id, write, window, cx);
        }
        self.sync_panes(window, cx);
        cx.notify();
    }

    /// `session.kill`; a failure is shown in the tab bar.
    fn kill(&mut self, session_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let killed = connection.call::<()>(Request::SessionKill(SessionKillParams {
            session_id,
            force: false,
        }));
        cx.spawn_in(window, async move |this, cx| {
            if let Err(error) = killed.await {
                tracing::warn!(%session_id, "session.kill failed: {error:#}");
                let _ = this.update(cx, |shell, cx| {
                    shell.tab_error = Some(format!("Cannot kill the session: {error:#}"));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Routes a batch of incoming frames; returns whether a pane got new content.
    fn dispatch(
        &mut self,
        batch: Vec<Incoming>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let mut fed: Vec<Entity<TerminalPane>> = Vec::new();
        let mut layout_changed = false;
        for incoming in batch {
            if let Incoming::Event(event) = &incoming {
                match self.sidebar.apply(event) {
                    Change::None => {}
                    Change::Redraw => cx.notify(),
                    // A new session (e.g. spawned by `mk`): re-list.
                    Change::Refresh => self.refresh(window, cx),
                }
                for tabs in self.tabs.values_mut() {
                    layout_changed |= tabs.apply(event);
                }
                if let Event::TabUpdated(tab) = event
                    && self.loading_tabs.contains(&tab.project_id)
                {
                    self.early_tab_events.push(event.clone());
                }
            }
            let Some(delivery) = self.router.route(incoming) else {
                continue;
            };
            let Some(pane) = self.panes.get(&delivery.session_id()).cloned() else {
                continue;
            };
            pane.update(cx, |pane, _| match delivery {
                Delivery::Output { data, .. } => pane.feed(&data),
                Delivery::Snapshot { snapshot, .. } => pane.load_snapshot(&snapshot),
                Delivery::Exited { exit_code, .. } => pane.set_exited(exit_code),
            });
            if !fed.iter().any(|p| p.entity_id() == pane.entity_id()) {
                fed.push(pane);
            }
        }
        if layout_changed {
            self.sync_panes(window, cx);
            cx.notify();
        }
        for pane in &fed {
            pane.update(cx, |_, cx| cx.notify());
        }
        !fed.is_empty()
    }

    /// One short sentence, plus optional detail shown smaller below it.
    fn status_line(&self) -> (SharedString, Option<SharedString>) {
        match &self.status {
            Status::Connecting => ("Connecting to the daemon…".into(), None),
            Status::Connected if self.sidebar.is_empty() => (
                "No workspaces yet.".into(),
                Some("Add a project folder from the sidebar to get started.".into()),
            ),
            Status::Connected => ("Opening the workspace…".into(), None),
            Status::Disconnected => (
                "The daemon connection closed. Restart Mkrew to reconnect.".into(),
                None,
            ),
            Status::Failed(error) => (
                "Cannot reach the daemon.".into(),
                Some(error.clone().into()),
            ),
            Status::ListFailed(error) => (
                "Cannot load the workspace.".into(),
                Some(error.clone().into()),
            ),
        }
    }

    fn render_sidebar(
        &self,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let shown = self.focused_session();
        let connected = self.connection.is_some();
        let add_workspace_keys = self.keymap.label(Command::AddWorkspace);
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .px(px(space::M))
            .py(px(space::S))
            .child(
                div()
                    .text_size(px(font_size::META))
                    .font_weight(FontWeight(weight::MEDIUM))
                    .text_color(rgb(theme.text_muted))
                    .child("Workspaces"),
            )
            .when(connected, |header| {
                header.child(
                    button("add-workspace", "+", theme, false)
                        .tooltip(tooltip(theme, "Add workspace", add_workspace_keys.clone()))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.add_workspace(window, cx);
                        })),
                )
            });
        let errors = [&self.sidebar_error, &self.keymap_error].map(|error| {
            error.clone().map(|error| {
                div()
                    .px(px(space::M))
                    .pb(px(space::S))
                    .text_size(px(font_size::META))
                    .text_color(rgb(theme.text_muted))
                    .child(format!("✕ {error}"))
            })
        });
        let body = if connected && self.sidebar.is_empty() {
            // Empty state: one sentence plus the primary action with its shortcut.
            div()
                .flex()
                .flex_col()
                .gap(px(space::S))
                .px(px(space::M))
                .py(px(space::S))
                .child(
                    div()
                        .text_size(px(font_size::BODY))
                        .text_color(rgb(theme.text_muted))
                        .child("No workspaces yet."),
                )
                .child(
                    button("add-first-workspace", "Add workspace", theme, true)
                        .child(
                            div()
                                .text_size(px(font_size::META))
                                .opacity(opacity::SECONDARY)
                                .child(add_workspace_keys),
                        )
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.add_workspace(window, cx);
                        })),
                )
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .px(px(space::XS))
                .children(self.sidebar.rows().into_iter().map(|row| {
                    match row {
                        Row::Workspace(row) => {
                            let active = self.workspace == Some(row.project_id);
                            let menu = self
                                .workspace_menu
                                .as_ref()
                                .filter(|menu| menu.project_id == row.project_id);
                            let row = workspace_row(row, active, theme, cx);
                            match menu {
                                // The menu drops down from below its row.
                                Some(menu) => div()
                                    .child(row)
                                    .child(self.render_workspace_menu(menu, theme, cx))
                                    .into_any_element(),
                                None => row,
                            }
                        }
                        Row::Session(row) => {
                            let selected = shown == Some(row.session_id);
                            session_row(row, selected, now, theme, cx)
                        }
                        Row::NoSessions { .. } => div()
                            .pl(px(space::XL))
                            .py(px(space::XS))
                            .text_size(px(font_size::META))
                            .text_color(rgb(theme.text_muted))
                            .child("No sessions yet.")
                            .into_any_element(),
                    }
                }))
                .into_any_element()
        };
        div()
            .id("sidebar")
            .flex_none()
            .w(px(theme::SIDEBAR_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .bg(rgb(theme.surface))
            .font_family(theme::UI_FONT)
            .child(header)
            .children(errors.into_iter().flatten())
            .child(body)
    }

    /// The workspace menu: "Open in Zed" and the default agent for new agent panes. Floats over
    /// the sidebar; a click outside closes it.
    fn render_workspace_menu(
        &self,
        menu: &WorkspaceMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let project_id = menu.project_id;
        let current = self
            .sidebar
            .projects()
            .iter()
            .find(|project| project.id == project_id)
            .and_then(|project| project.default_agent.as_deref());
        let muted_line = |text: String| {
            div()
                .px(px(space::S))
                .py(px(space::XS))
                .text_size(px(font_size::META))
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let agents: Vec<gpui::AnyElement> = match &menu.agents {
            Agents::Loading => vec![muted_line("Loading agents…".into()).into_any_element()],
            Agents::Failed(error) => {
                vec![
                    div()
                        .flex()
                        .pl(px(space::S))
                        .child(
                            div()
                                .py(px(space::XS))
                                .text_size(px(font_size::META))
                                .text_color(rgb(theme.status_exited))
                                .child("✕"),
                        )
                        .child(
                            muted_line(format!("Cannot list the agents: {error}"))
                                .pl(px(space::XS)),
                        )
                        .into_any_element(),
                ]
            }
            Agents::Loaded(profiles) => {
                let choices = workspace_menu::agent_choices(profiles, current);
                if choices.is_empty() {
                    vec![
                        muted_line("No agent profiles. Add one to the profiles folder.".into())
                            .into_any_element(),
                    ]
                } else {
                    choices
                        .into_iter()
                        .map(|choice| {
                            let id = SharedString::from(format!("default-agent-{}", choice.id));
                            let agent = choice.id;
                            menu_item(id, theme)
                                .child(
                                    div()
                                        .w(px(space::M))
                                        .text_color(rgb(theme.accent))
                                        .child(if choice.selected { "✓" } else { "" }),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .when(choice.selected, |el| {
                                            el.text_color(rgb(theme.text))
                                                .font_weight(FontWeight(weight::MEDIUM))
                                        })
                                        .child(choice.label),
                                )
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    shell.set_default_agent(project_id, agent.clone(), window, cx);
                                }))
                                .into_any_element()
                        })
                        .collect()
                }
            }
        };
        let panel = div()
            .id("workspace-menu")
            .w(px(theme::MENU_WIDTH))
            .flex()
            .flex_col()
            .p(px(space::XS))
            .rounded(px(radius::MEDIUM))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface_raised))
            .shadow_md()
            .font_family(theme::UI_FONT)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.workspace_menu = None;
                cx.notify();
            }))
            .child(
                menu_item("open-in-zed".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child("Open in Zed"),
                    )
                    .child(
                        div()
                            .text_size(px(font_size::META))
                            .child(self.keymap.label(Command::OpenInZed)),
                    )
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        shell.open_in_zed(project_id, cx);
                    })),
            )
            .child(
                div()
                    .h(px(theme::BORDER_WIDTH))
                    .my(px(space::XS))
                    .bg(rgb(theme.border)),
            )
            .child(
                div()
                    .px(px(space::S))
                    .pt(px(space::XS))
                    .text_size(px(font_size::META))
                    .font_weight(FontWeight(weight::MEDIUM))
                    .text_color(rgb(theme.text_muted))
                    .child("Default agent for new panes"),
            )
            .children(agents);
        deferred(
            anchored()
                .snap_to_window_with_margin(px(space::S))
                .child(div().pt(px(space::XS)).child(panel)),
        )
        .into_any_element()
    }

    fn render_main(&self, now: i64, theme: &Theme, cx: &mut Context<'_, Self>) -> gpui::AnyElement {
        if self.connection.is_some()
            && let Some(project_id) = self.workspace
            && let Some(tabs) = self.tabs.get(&project_id)
        {
            return div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .child(self.render_tab_bar(project_id, tabs, now, theme, cx))
                .child(self.render_tab(tabs, now, theme, cx))
                .into_any_element();
        }
        let (sentence, detail) = self.status_line();
        message(theme, Some("Mkrew"), sentence, detail)
            .flex_1()
            .h_full()
            .into_any_element()
    }

    fn render_tab_bar(
        &self,
        project_id: Ulid,
        tabs: &Tabs,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let active = tabs.active().map(|tab| tab.id);
        let hover = theme.surface_raised;
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .px(px(space::S))
            .py(px(space::XS))
            .bg(rgb(theme.surface))
            .font_family(theme::UI_FONT)
            .children(tabs.tabs().iter().map(|tab| {
                let tab_id = tab.id;
                let is_active = active == Some(tab_id);
                if let Some((_, draft)) = self.renaming.as_ref().filter(|(id, _)| *id == tab_id) {
                    return self.render_rename(draft, theme, cx);
                }
                // The most urgent state among the tab's panes (SPEC §11).
                let urgent = status::most_urgent(self.sidebar.sessions(), &tab.sessions());
                div()
                    .id(SharedString::from(format!("tab-{tab_id}")))
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .px(px(space::S))
                    .py(px(space::XS))
                    .rounded(px(radius::SMALL))
                    // Same box as the rename field, so the bar does not shift while renaming.
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(gpui::transparent_black())
                    .cursor_pointer()
                    .whitespace_nowrap()
                    .text_size(px(font_size::BODY))
                    .when(is_active, |el| {
                        el.bg(rgb(theme.surface_raised))
                            .font_weight(FontWeight(weight::MEDIUM))
                            .text_color(rgb(theme.text))
                    })
                    .when(!is_active, |el| {
                        el.text_color(rgb(theme.text_muted))
                            .hover(move |style| style.bg(rgb(hover)))
                            .active(|style| style.opacity(opacity::ACTIVE))
                    })
                    .on_click(
                        cx.listener(move |shell, event: &gpui::ClickEvent, window, cx| {
                            if let Some(tabs) = shell.active_tabs_mut() {
                                tabs.activate(tab_id);
                            }
                            shell.sync_panes(window, cx);
                            if event.click_count() == 2 {
                                shell.start_rename(window, cx);
                            }
                            cx.notify();
                        }),
                    )
                    .when_some(urgent, |el, session| {
                        el.tooltip(status_tooltip(
                            theme,
                            session.state,
                            session.state_source,
                            session.state_changed_at,
                        ))
                        .child(
                            div()
                                .text_color(rgb(sidebar::status_color(theme, session.state)))
                                .child(status::icon(session.state, now)),
                        )
                    })
                    .child(tab.title.clone())
                    .into_any_element()
            }))
            .child(
                button("new-tab", "+", theme, false)
                    .tooltip(tooltip(theme, "New tab", ""))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.create_tab(project_id, None, window, cx);
                    })),
            )
            // Pushes the notices and pane actions to the right.
            .child(div().flex_1())
            .children(
                self.tab_error
                    .clone()
                    .map(|error| notice(theme.text, format!("✕ {error}"))),
            )
            .when(
                tabs.active().and_then(|tab| tab.zoomed()).is_some(),
                |bar| {
                    bar.child(notice(theme.text_muted, "⤢ Pane zoomed".to_owned()))
                        .child(
                            button("unzoom", "Show all panes", theme, false)
                                .tooltip(tooltip(
                                    theme,
                                    "Show all panes",
                                    self.keymap.label(Command::ZoomPane),
                                ))
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    shell.run(&Run(Command::ZoomPane), window, cx);
                                })),
                        )
                },
            )
            .children(
                lifecycle::running_warning(self.sidebar.sessions(), project_id)
                    .map(|warning| notice(theme.text_muted, format!("⚠ {warning}"))),
            )
            .child(new_pane_button(
                "new-agent-pane",
                "+ Agent",
                SessionKind::Agent,
                theme,
                cx,
            ))
            .child(new_pane_button(
                "new-shell-pane",
                "+ Shell",
                SessionKind::Shell,
                theme,
                cx,
            ))
            .when(self.focused_session().is_some(), |bar| {
                bar.child(
                    button("close-pane", "Close pane", theme, false)
                        .tooltip(tooltip(
                            theme,
                            "Close the focused pane",
                            self.keymap.label(Command::ClosePane),
                        ))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_focused_pane(window, cx);
                        })),
                )
            })
    }

    /// The title being renamed inline: the draft with a caret, in the active tab's place.
    fn render_rename(
        &self,
        draft: &str,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        div()
            .id("tab-rename")
            .flex()
            .items_center()
            .min_w(px(theme::TAB_RENAME_MIN_WIDTH))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(radius::SMALL))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.pane_focus_border))
            .bg(rgb(theme.surface_raised))
            .whitespace_nowrap()
            .text_size(px(font_size::BODY))
            .font_weight(FontWeight(weight::MEDIUM))
            .text_color(rgb(theme.text))
            .cursor(CursorStyle::IBeam)
            .track_focus(&self.rename_focus)
            .on_key_down(cx.listener(Self::rename_key_down))
            .child(draft.to_owned())
            .child(
                div()
                    .flex_none()
                    .w(px(theme::BORDER_WIDTH))
                    .h(px(font_size::BODY))
                    .bg(rgb(theme.text)),
            )
            .into_any_element()
    }

    /// The active tab's split tree: each pane placed at its fraction of the area.
    fn render_tab(
        &self,
        tabs: &Tabs,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        if tabs.active().and_then(|tab| tab.layout()).is_none() {
            return message(
                theme,
                None,
                "This tab has no panes.".into(),
                Some("Start an agent or a shell, or pick a session in the sidebar.".into()),
            )
            .flex_1()
            .child(
                div()
                    .flex()
                    .gap(px(space::S))
                    .child(
                        button("empty-new-agent", "New agent", theme, true).on_click(cx.listener(
                            |shell, _, window, cx| {
                                shell.new_pane(SessionKind::Agent, Axis::Vertical, window, cx);
                            },
                        )),
                    )
                    .child(new_pane_button(
                        "empty-new-shell",
                        "New shell",
                        SessionKind::Shell,
                        theme,
                        cx,
                    )),
            );
        }
        let focused = tabs.active().and_then(|tab| tab.focused());
        let rects = tabs.active().map(Tab::rects).unwrap_or_default();
        let several = rects.len() > 1;
        let half_gap = px(theme::PANE_HALF_GAP);
        div()
            .flex_1()
            .min_h_0()
            .p(half_gap)
            .child(div().size_full().relative().children(rects.into_iter().map(
                |(session, rect)| {
                    let is_focused = focused == Some(session);
                    div()
                        .absolute()
                        .left(relative(rect.x as f32))
                        .top(relative(rect.y as f32))
                        .w(relative(rect.w as f32))
                        .h(relative(rect.h as f32))
                        .p(half_gap)
                        .child(self.pane_frame(session, several, is_focused, now, theme, cx))
                },
            )))
    }

    /// One pane: the terminal (or why it is not shown), its border, and the unfocused dim.
    fn pane_frame(
        &self,
        session: Ulid,
        several: bool,
        is_focused: bool,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let content = match self.panes.get(&session) {
            Some(pane) => pane.clone().into_any_element(),
            None => {
                let (sentence, detail) = self.pane_placeholder(session);
                message(theme, None, sentence, detail)
                    .size_full()
                    .into_any_element()
            }
        };
        let info = self.sidebar.sessions().iter().find(|s| s.id == session);
        let border = status::pane_border(theme, info.map(|s| s.state), several, is_focused);
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _, window, cx| {
                    if let Some(tabs) = shell.active_tabs_mut() {
                        tabs.focus(session);
                    }
                    shell.focus_pane(window, cx);
                    cx.notify();
                }),
            )
            .children(info.map(|info| pane_header(info, now, theme)))
            .child(div().flex_1().min_h_0().child(content))
            .when(several && !is_focused, |el| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(rgb(theme.bg))
                        .opacity(theme.unfocused_pane_dim),
                )
            })
            // Drawn above the terminal so its corners stay rounded.
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(px(radius::MEDIUM))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(border)),
            )
    }

    /// Why a pane on screen has no terminal yet.
    fn pane_placeholder(&self, session: Ulid) -> (SharedString, Option<SharedString>) {
        // An exit explains a failed attach better than its error does.
        if let Some(s) = self
            .sidebar
            .sessions()
            .iter()
            .find(|s| s.id == session && s.state == SessionState::Exited)
        {
            let text = match s.exit_code {
                Some(code) => format!("✕ Session exited (code {code})."),
                None => "✕ Session exited.".to_owned(),
            };
            return (text.into(), None);
        }
        match self.attach_errors.get(&session) {
            Some(error) => (
                "✕ Cannot attach the session.".into(),
                Some(error.clone().into()),
            ),
            None => ("Attaching…".into(), None),
        }
    }
}

/// A centered message: optional title, one sentence, optional smaller detail.
fn message(
    theme: &Theme,
    title: Option<&'static str>,
    sentence: SharedString,
    detail: Option<SharedString>,
) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(space::S))
        .p(px(space::XL))
        .font_family(theme::UI_FONT)
        .children(title.map(|title| {
            div()
                .text_size(px(font_size::VIEW_TITLE))
                .font_weight(FontWeight(weight::SEMIBOLD))
                .text_color(rgb(theme.text))
                .child(title)
        }))
        .child(
            div()
                .text_size(px(font_size::BODY))
                .text_color(rgb(theme.text_muted))
                .child(sentence),
        )
        .children(detail.map(|detail| {
            div()
                .max_w(px(theme::READABLE_WIDTH))
                .text_size(px(font_size::META))
                .text_color(rgb(theme.text_muted))
                .child(detail)
        }))
}

/// A one-line notice in the tab bar, shortened to fit; `color` sets errors apart from warnings.
fn notice(color: u32, text: String) -> gpui::Div {
    div()
        .min_w_0()
        .overflow_hidden()
        .text_ellipsis()
        .whitespace_nowrap()
        .text_size(px(font_size::META))
        .text_color(rgb(color))
        .child(text)
}

/// A quiet button starting a new agent or shell pane.
fn new_pane_button(
    id: &'static str,
    label: &'static str,
    kind: SessionKind,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Stateful<gpui::Div> {
    let tip = match kind {
        SessionKind::Agent => "New pane with the workspace's default agent",
        SessionKind::Shell => "New shell pane in the workspace folder",
    };
    button(id, label, theme, false)
        .tooltip(tooltip(theme, tip, ""))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.new_pane(kind, Axis::Vertical, window, cx);
        }))
}

/// Fire-and-forget `session.detach`; a failure is logged.
fn detach(connection: &Connection, session_id: Ulid, cx: &mut Context<'_, Shell>) {
    let reply = connection.request(Request::SessionDetach(SessionRef { session_id }));
    cx.background_executor()
        .spawn(async move {
            match reply.await {
                Ok(Outcome::Result(_)) => {}
                Ok(Outcome::Error(body)) => {
                    tracing::warn!(%session_id, "session.detach failed: {}", body.message);
                }
                Err(error) => tracing::warn!(%session_id, "session.detach failed: {error:#}"),
            }
        })
        .detach();
}

/// A small text button: primary (accent) or quiet (hover shows a surface).
fn button(
    id: &'static str,
    label: &'static str,
    theme: &Theme,
    primary: bool,
) -> gpui::Stateful<gpui::Div> {
    let base = div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .text_size(px(font_size::BODY))
        .child(label);
    if primary {
        base.bg(rgb(theme.accent))
            .text_color(rgb(theme.surface))
            .hover(|style| style.opacity(opacity::HOVER))
            .active(|style| style.opacity(opacity::ACTIVE))
    } else {
        let hover = theme.surface_raised;
        base.text_color(rgb(theme.text_muted))
            .hover(move |style| style.bg(rgb(hover)))
            .active(|style| style.opacity(opacity::ACTIVE))
    }
}

/// A full-width row of a popover menu; callers add the label (and a shortcut on the right).
fn menu_item(id: SharedString, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    // One step above the menu's own `surface_raised`.
    let hover = theme.border;
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .text_size(px(font_size::BODY))
        .text_color(rgb(theme.text_muted))
        .hover(move |style| style.bg(rgb(hover)))
        .active(|style| style.opacity(opacity::ACTIVE))
}

fn workspace_row(
    row: WorkspaceRow,
    active: bool,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let project_id = row.project_id;
    let hover = theme.surface_raised;
    div()
        .id(SharedString::from(format!("workspace-{project_id}")))
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .when(active, |el| el.bg(rgb(theme.surface_raised)))
        .hover(move |style| style.bg(rgb(hover)))
        .tooltip(tooltip(theme, &row.path, ""))
        // The first click shows the workspace; clicking the shown one expands/collapses it.
        .on_click(cx.listener(move |shell, _, window, cx| {
            if shell.workspace == Some(project_id) {
                shell.sidebar.toggle(project_id);
                cx.notify();
            } else {
                shell.activate_workspace(project_id, window, cx);
            }
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |shell, _, window, cx| {
                shell.open_workspace_menu(project_id, window, cx);
            }),
        )
        .child(
            div()
                .text_size(px(font_size::META))
                .text_color(rgb(theme.text_muted))
                .child(if row.expanded { "▾" } else { "▸" }),
        )
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(px(font_size::BODY))
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text))
                .child(row.name),
        )
        .when(row.working > 0, |el| {
            let working = sidebar::status_display(SessionState::Working);
            el.child(
                div()
                    .text_size(px(font_size::META))
                    .text_color(rgb(theme.status_working))
                    .child(format!("{} {}", working.icon, row.working)),
            )
        })
        .when(row.needs_input, |el| {
            el.child(
                div()
                    .text_size(px(font_size::META))
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.status_needs_input))
                    .child(sidebar::status_display(SessionState::NeedsInput).icon),
            )
        })
        .when(active, |el| {
            el.child(
                button("workspace-menu-button", "⋯", theme, false)
                    .px(px(space::XS))
                    .py_0()
                    .tooltip(tooltip(
                        theme,
                        "Workspace menu (or right-click the row)",
                        "",
                    ))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        // Not a click on the row (which would collapse it).
                        cx.stop_propagation();
                        shell.open_workspace_menu(project_id, window, cx);
                    })),
            )
        })
        .into_any_element()
}

fn session_row(
    row: SessionRow,
    selected: bool,
    now: i64,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let session_id = row.session_id;
    let status_text = row.status_text(now);
    let name = match &row.title {
        Some(title) => format!("{} · {title}", row.agent),
        None => row.agent.clone(),
    };
    let hover = theme.surface_raised;
    // An exited session cannot be attached ("Run again" / "Resume" come later).
    let live = row.state != SessionState::Exited;
    div()
        .id(SharedString::from(format!("session-{session_id}")))
        .flex()
        .items_center()
        .gap(px(space::S))
        .pl(px(space::XL))
        .pr(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::SMALL))
        .when(selected, |el| el.bg(rgb(theme.surface_raised)))
        .when(live, |el| {
            el.cursor_pointer()
                .hover(move |style| style.bg(rgb(hover)))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    shell.open_session(session_id, window, cx);
                }))
        })
        .child(
            div()
                .text_size(px(font_size::BODY))
                .text_color(rgb(sidebar::status_color(theme, row.state)))
                .child(status::icon(row.state, now)),
        )
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(px(font_size::BODY))
                .text_color(rgb(theme.text))
                // Selection uses the accent; hover only the surface shift.
                .when(selected, |el| {
                    el.font_weight(FontWeight(weight::MEDIUM))
                        .text_color(rgb(theme.accent))
                })
                .child(name),
        )
        .child(
            div()
                .id(SharedString::from(format!("session-status-{session_id}")))
                .flex_none()
                .text_size(px(font_size::META))
                .text_color(rgb(theme.text_muted))
                .tooltip(status_tooltip(
                    theme,
                    row.state,
                    row.state_source,
                    row.state_changed_at,
                ))
                .child(status_text),
        )
        .into_any_element()
}

/// A pane's header (SPEC §11): status icon, agent name, session title, status text and time;
/// hovering the status shows its source and last change.
fn pane_header(session: &Session, now: i64, theme: &Theme) -> gpui::Div {
    let (state, changed_at) = (session.state, session.state_changed_at);
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .py(px(space::XS))
        .font_family(theme::UI_FONT)
        .text_size(px(font_size::META))
        .child(
            div()
                .flex_none()
                .text_color(rgb(sidebar::status_color(theme, state)))
                .child(status::icon(state, now)),
        )
        .child(
            div()
                .flex_none()
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text))
                .child(session.agent_profile.clone()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_color(rgb(theme.text_muted))
                .children(session.title.clone()),
        )
        .child(
            div()
                .id(SharedString::from(format!("pane-status-{}", session.id)))
                .flex_none()
                .text_color(rgb(theme.text_muted))
                .tooltip(status_tooltip(
                    theme,
                    state,
                    session.state_source,
                    changed_at,
                ))
                .child(status::text(state, session.exit_code, changed_at, now)),
        )
}

/// Tooltip of a status: its source and how long ago it changed, aged when it opens (a
/// done or needs-input status is not redrawn while it sits there).
fn status_tooltip(
    theme: &Theme,
    state: SessionState,
    source: StateSource,
    changed_at: i64,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let theme = theme.clone();
    move |_, cx| {
        let text = status::hover(state, source, changed_at, status::now_ms()).into();
        let theme = theme.clone();
        cx.new(|_| Tooltip {
            theme,
            text,
            keys: SharedString::default(),
        })
        .into()
    }
}

/// Builds a tooltip showing `text` and, if any, its keyboard shortcut.
fn tooltip(
    theme: &Theme,
    text: &str,
    keys: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let theme = theme.clone();
    let text = SharedString::from(text.to_owned());
    let keys = keys.into();
    move |_, cx| {
        let (theme, text, keys) = (theme.clone(), text.clone(), keys.clone());
        cx.new(|_| Tooltip { theme, text, keys }).into()
    }
}

struct Tooltip {
    theme: Theme,
    text: SharedString,
    keys: SharedString,
}

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = &self.theme;
        div()
            .flex()
            .gap(px(space::S))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(radius::SMALL))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface_raised))
            .shadow_sm()
            .font_family(theme::UI_FONT)
            .text_size(px(font_size::META))
            .text_color(rgb(theme.text))
            .child(self.text.clone())
            .when(!self.keys.is_empty(), |el| {
                el.child(
                    div()
                        .text_color(rgb(theme.text_muted))
                        .child(self.keys.clone()),
                )
            })
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let theme = Theme::for_appearance(dark);
        // One clock reading per frame, so every spinner and time agrees.
        let now = status::now_ms();
        div()
            .size_full()
            .flex()
            .bg(rgb(theme.bg))
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::run))
            .child(self.render_sidebar(now, &theme, cx))
            .child(self.render_main(now, &theme, cx))
    }
}
