//! `mkrew`: the desktop app (SPEC §10). Connects to the daemon (starting it if needed), lists the
//! workspaces and their sessions in the sidebar and shows one session in a terminal pane; tabs
//! and splits follow in later M2 tasks.

// No console window next to the app in release builds (debug keeps stderr visible).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    AnyView, App, Bounds, Context, Entity, FocusHandle, FontWeight, KeyBinding, PathPromptOptions,
    SharedString, TitlebarOptions, Window, WindowAppearance, WindowBounds, WindowOptions, actions,
    div, px, rgb, size,
};
use mkrew_core::Paths;
use mkrew_protocol::{
    Event, Outcome, Project, ProjectAddParams, Request, ScreenSnapshot, Session,
    SessionAttachResult, SessionListParams, SessionRef, SessionState,
};
use mkrew_ui::client::{Connection, Incoming};
use mkrew_ui::sidebar::{self, Change, Row, SessionRow, Sidebar, WorkspaceRow};
use mkrew_ui::terminal::view::TerminalPane;
use mkrew_ui::theme::{self, Theme, font_size, opacity, radius, space, weight};
use tracing_subscriber::EnvFilter;
use ulid::Ulid;

/// Longest wait for the frame showing fed output before draining again anyway (e.g. the
/// window is minimized and never paints).
const PAINT_TIMEOUT: Duration = Duration::from_millis(50);
/// Most incoming frames handled per drain, so one drain never starves painting.
const DRAIN_BATCH: usize = 256;

actions!(mkrew, [AddWorkspace]);

/// SPEC §13 default; the full keymap arrives with T2.7.
const ADD_WORKSPACE_KEYS: &str = "ctrl-shift-k";
const ADD_WORKSPACE_LABEL: &str = "Ctrl+Shift+K";

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
    gpui_platform::application().run(move |cx: &mut App| {
        cx.bind_keys([KeyBinding::new(ADD_WORKSPACE_KEYS, AddWorkspace, None)]);
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
            cx.new(|cx| Shell::new(paths, window, cx))
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
    /// `session.attach` in flight.
    Opening,
    /// Connected, but no session is open.
    NoSessions,
    Disconnected,
    /// The daemon cannot be reached.
    Failed(String),
    /// Listing or attaching sessions failed.
    OpenFailed(String),
}

/// A `session.attach` in flight: output and snapshots that arrive before its pane exists.
struct Attaching {
    session_id: Ulid,
    /// Tells this attach's reply apart from an older one for the same session.
    generation: u64,
    snapshot: Option<ScreenSnapshot>,
    output: Vec<u8>,
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
    pane: Option<Entity<TerminalPane>>,
    attaching: Option<Attaching>,
    /// Generation of the last `session.attach` sent.
    attach_generation: u64,
    /// Given to panes; signalled after each terminal paint.
    painted: async_channel::Sender<()>,
    /// Focused while no pane is, so window-level actions (Add workspace) still dispatch.
    focus_handle: FocusHandle,
}

impl Shell {
    fn new(paths: Paths, window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        cx.observe_window_appearance(window, |_, _, cx| cx.notify())
            .detach();
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
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
                shell.pane = None;
                shell.attaching = None;
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
            pane: None,
            attaching: None,
            attach_generation: 0,
            painted,
            focus_handle,
        }
    }

    /// Re-reads workspaces and sessions. With nothing open yet, opens the most recently
    /// started live session.
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
                        let latest = sessions
                            .iter()
                            .filter(|session| session.state != SessionState::Exited)
                            .max_by_key(|session| session.started_at)
                            .map(|session| session.id);
                        shell.sidebar.load(projects, sessions);
                        let idle = shell.pane.is_none()
                            && shell.attaching.is_none()
                            && matches!(shell.status, Status::Connecting | Status::NoSessions);
                        if idle {
                            match latest {
                                Some(session_id) => shell.open_session(session_id, window, cx),
                                None => shell.status = Status::NoSessions,
                            }
                        }
                    }
                    Err(error) => {
                        tracing::warn!("listing workspaces failed: {error:#}");
                        shell.sidebar.cancel_load();
                        if shell.pane.is_none() {
                            shell.status = Status::OpenFailed(format!("{error:#}"));
                        }
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

    /// Shows `session_id` in the pane, detaching the session shown before.
    fn open_session(&mut self, session_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let shown = self.pane.as_ref().map(|pane| pane.read(cx).session_id());
        let pending = self
            .attaching
            .as_ref()
            .map(|attaching| attaching.session_id);
        if shown == Some(session_id) || pending == Some(session_id) {
            return;
        }
        if let Some(previous) = shown {
            self.pane = None;
            window.focus(&self.focus_handle, cx);
            detach(&connection, previous, cx);
        }
        // A pending attach that is replaced detaches itself once its reply arrives.
        self.attach_generation += 1;
        let generation = self.attach_generation;
        self.attaching = Some(Attaching {
            session_id,
            generation,
            snapshot: None,
            output: Vec::new(),
        });
        self.set_status(Status::Opening, cx);
        let attach = connection
            .call::<SessionAttachResult>(Request::SessionAttach(SessionRef { session_id }));
        cx.spawn_in(window, async move |this, cx| {
            let attached = attach.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                if shell
                    .attaching
                    .as_ref()
                    .is_none_or(|attaching| attaching.generation != generation)
                {
                    // Replaced meanwhile. The daemon keeps one attachment per session, so only
                    // detach if this session is not wanted again (A → B → A clicks).
                    let shown = shell.pane.as_ref().map(|pane| pane.read(cx).session_id());
                    let pending = shell.attaching.as_ref().map(|a| a.session_id);
                    if attached.is_ok() && shown != Some(session_id) && pending != Some(session_id)
                    {
                        detach(&connection, session_id, cx);
                    }
                    return;
                }
                let Some(attaching) = shell.attaching.take() else {
                    return;
                };
                let attached = match attached {
                    Ok(attached) => attached,
                    Err(error) => {
                        tracing::warn!(%session_id, "session.attach failed: {error:#}");
                        shell.set_status(Status::OpenFailed(format!("{error:#}")), cx);
                        return;
                    }
                };
                let snapshot = attaching.snapshot.unwrap_or(attached.snapshot);
                let painted = shell.painted.clone();
                shell.pane = Some(cx.new(|cx| {
                    let mut pane =
                        TerminalPane::new(session_id, &snapshot, connection, painted, window, cx);
                    pane.feed(&attaching.output);
                    pane
                }));
                cx.notify();
            });
        })
        .detach();
    }

    /// Picks a folder and adds it as a workspace (`project.add`).
    fn add_workspace(&mut self, _: &AddWorkspace, window: &mut Window, cx: &mut Context<'_, Self>) {
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
            let _ = this.update(cx, |shell, cx| {
                match added {
                    Ok(project) => {
                        shell.sidebar_error = None;
                        shell.sidebar.add_project(project);
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

    fn set_status(&mut self, status: Status, cx: &mut Context<'_, Self>) {
        self.status = status;
        cx.notify();
    }

    /// Routes a batch of incoming frames; returns whether a pane got new content.
    fn dispatch(
        &mut self,
        batch: Vec<Incoming>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let pane = self.pane.clone();
        let pane_id = pane.as_ref().map(|pane| pane.read(cx).session_id());
        let mut fed = false;
        for incoming in batch {
            match incoming {
                Incoming::Output { session_id, data } => {
                    if let Some(pane) = pane.as_ref().filter(|_| pane_id == Some(session_id)) {
                        pane.update(cx, |pane, _| pane.feed(&data));
                        fed = true;
                    } else if let Some(attaching) = self.attaching_mut(session_id) {
                        attaching.output.extend_from_slice(&data);
                    }
                }
                Incoming::Event(event) => {
                    match self.sidebar.apply(&event) {
                        Change::None => {}
                        Change::Redraw => cx.notify(),
                        // A new session (e.g. spawned by `mk`): re-list; opens it if idle.
                        Change::Refresh => self.refresh(window, cx),
                    }
                    match event {
                        Event::SessionSnapshot(event) => {
                            let session_id = event.session_id;
                            if let Some(pane) =
                                pane.as_ref().filter(|_| pane_id == Some(session_id))
                            {
                                pane.update(cx, |pane, _| pane.load_snapshot(&event.snapshot));
                                fed = true;
                            } else if let Some(attaching) = self.attaching_mut(session_id) {
                                attaching.snapshot = Some(event.snapshot);
                                attaching.output.clear();
                            }
                        }
                        Event::SessionExited(event) => {
                            if let Some(pane) =
                                pane.as_ref().filter(|_| pane_id == Some(event.session_id))
                            {
                                pane.update(cx, |pane, _| pane.set_exited(event.exit_code));
                                fed = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        if fed && let Some(pane) = &pane {
            pane.update(cx, |_, cx| cx.notify());
        }
        fed
    }

    fn attaching_mut(&mut self, session_id: Ulid) -> Option<&mut Attaching> {
        self.attaching
            .as_mut()
            .filter(|attaching| attaching.session_id == session_id)
    }

    /// One short sentence, plus optional detail shown smaller below it.
    fn status_line(&self) -> (SharedString, Option<SharedString>) {
        match &self.status {
            Status::Connecting => ("Connecting to the daemon…".into(), None),
            Status::Opening => ("Opening the session…".into(), None),
            Status::NoSessions => (
                "No running sessions.".into(),
                Some(
                    "Pick one in the sidebar, or start one with mk session spawn <project>.".into(),
                ),
            ),
            Status::Disconnected => (
                "The daemon connection closed. Restart Mkrew to reconnect.".into(),
                None,
            ),
            Status::Failed(error) => (
                "Cannot reach the daemon.".into(),
                Some(error.clone().into()),
            ),
            Status::OpenFailed(error) => (
                "Cannot open the session.".into(),
                Some(error.clone().into()),
            ),
        }
    }

    fn render_sidebar(&self, theme: &Theme, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let shown = self
            .pane
            .as_ref()
            .map(|pane| pane.read(cx).session_id())
            .or(self
                .attaching
                .as_ref()
                .map(|attaching| attaching.session_id));
        let connected = self.connection.is_some();
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
                        .tooltip(tooltip(theme, "Add workspace", ADD_WORKSPACE_LABEL))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.add_workspace(&AddWorkspace, window, cx);
                        })),
                )
            });
        let error = self.sidebar_error.clone().map(|error| {
            div()
                .px(px(space::M))
                .pb(px(space::S))
                .text_size(px(font_size::META))
                .text_color(rgb(theme.text_muted))
                .child(format!("✕ {error}"))
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
                                .child(ADD_WORKSPACE_LABEL),
                        )
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.add_workspace(&AddWorkspace, window, cx);
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
                        Row::Workspace(row) => workspace_row(row, theme, cx),
                        Row::Session(row) => {
                            let selected = shown == Some(row.session_id);
                            session_row(row, selected, theme, cx)
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
            .children(error)
            .child(body)
    }

    fn render_main(&self, theme: &Theme) -> impl IntoElement {
        if let Some(pane) = &self.pane {
            return div().flex_1().h_full().flex().child(pane.clone());
        }
        let (sentence, detail) = self.status_line();
        div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(space::S))
            .p(px(space::XL))
            .font_family(theme::UI_FONT)
            .child(
                div()
                    .text_size(px(font_size::VIEW_TITLE))
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Mkrew"),
            )
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

fn workspace_row(
    row: WorkspaceRow,
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
        .hover(move |style| style.bg(rgb(hover)))
        .tooltip(tooltip(theme, &row.path, ""))
        .on_click(cx.listener(move |shell, _, _, cx| {
            shell.sidebar.toggle(project_id);
            cx.notify();
        }))
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
        .into_any_element()
}

fn session_row(
    row: SessionRow,
    selected: bool,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let session_id = row.session_id;
    let status = sidebar::status_display(row.state);
    let status_text = row.status_text();
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
                .child(status.icon),
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
                .flex_none()
                .text_size(px(font_size::META))
                .text_color(rgb(theme.text_muted))
                .child(status_text),
        )
        .into_any_element()
}

/// Builds a tooltip showing `text` and, if any, its keyboard shortcut.
fn tooltip(
    theme: &Theme,
    text: &str,
    keys: &'static str,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let theme = theme.clone();
    let text = SharedString::from(text.to_owned());
    move |_, cx| {
        let (theme, text) = (theme.clone(), text.clone());
        cx.new(|_| Tooltip { theme, text, keys }).into()
    }
}

struct Tooltip {
    theme: Theme,
    text: SharedString,
    keys: &'static str,
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
                el.child(div().text_color(rgb(theme.text_muted)).child(self.keys))
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
        div()
            .size_full()
            .flex()
            .bg(rgb(theme.bg))
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::add_workspace))
            .child(self.render_sidebar(&theme, cx))
            .child(self.render_main(&theme))
    }
}
