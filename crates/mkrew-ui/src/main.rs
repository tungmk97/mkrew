//! `mkrew`: the desktop app (SPEC §10). Connects to the daemon (starting it if needed) and shows
//! the most recent running session in a terminal pane; workspaces, tabs and splits follow in
//! later M2 tasks.

// No console window next to the app in release builds (debug keeps stderr visible).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, Entity, FontWeight, SharedString, TitlebarOptions, Window,
    WindowAppearance, WindowBounds, WindowOptions, div, px, rgb, size,
};
use mkrew_core::Paths;
use mkrew_protocol::{
    Event, Request, ScreenSnapshot, Session, SessionAttachResult, SessionListParams, SessionRef,
    SessionState,
};
use mkrew_ui::client::{Connection, Incoming};
use mkrew_ui::terminal::view::TerminalPane;
use mkrew_ui::theme::{self, Theme, font_size, space, weight};
use tracing_subscriber::EnvFilter;
use ulid::Ulid;

/// Longest wait for the frame showing fed output before draining again anyway (e.g. the
/// window is minimized and never paints).
const PAINT_TIMEOUT: Duration = Duration::from_millis(50);
/// Most incoming frames handled per drain, so one drain never starves painting.
const DRAIN_BATCH: usize = 256;

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
    /// `session.list` / `session.attach` in flight.
    Opening,
    /// Connected, but there is no live session to show.
    NoSessions,
    Disconnected,
    /// The daemon cannot be reached.
    Failed(String),
    /// Listing or attaching the session failed.
    OpenFailed(String),
}

/// A `session.attach` in flight: output and snapshots that arrive before its pane exists.
struct Attaching {
    session_id: Ulid,
    snapshot: Option<ScreenSnapshot>,
    output: Vec<u8>,
}

struct Shell {
    status: Status,
    /// Dropping it closes the connection.
    connection: Option<Rc<Connection>>,
    pane: Option<Entity<TerminalPane>>,
    attaching: Option<Attaching>,
    /// Given to panes; signalled after each terminal paint.
    painted: async_channel::Sender<()>,
}

impl Shell {
    fn new(paths: Paths, window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        cx.observe_window_appearance(window, |_, _, cx| cx.notify())
            .detach();
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
            let attached = this.update_in(cx, |shell, window, cx| {
                shell.connection = Some(connection);
                shell.attach_latest(window, cx);
            });
            if attached.is_err() {
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
            let _ = this.update(cx, |shell, cx| {
                shell.status = Status::Disconnected;
                shell.connection = None;
                shell.pane = None;
                shell.attaching = None;
                cx.notify();
            });
        })
        .detach();
        Self {
            status: Status::Connecting,
            connection: None,
            pane: None,
            attaching: None,
            painted,
        }
    }

    /// Attaches the most recently started live session, if any (the sidebar replaces this).
    fn attach_latest(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.set_status(Status::Opening, cx);
        let list =
            connection.call::<Vec<Session>>(Request::SessionList(SessionListParams::default()));
        cx.spawn_in(window, async move |this, cx| {
            let sessions = match list.await {
                Ok(sessions) => sessions,
                Err(error) => {
                    tracing::warn!("session.list failed: {error:#}");
                    let _ = this.update(cx, |shell, cx| {
                        shell.set_status(Status::OpenFailed(format!("{error:#}")), cx);
                    });
                    return;
                }
            };
            let Some(session) = sessions
                .into_iter()
                .filter(|session| session.state != SessionState::Exited)
                .max_by_key(|session| session.started_at)
            else {
                let _ = this.update(cx, |shell, cx| shell.set_status(Status::NoSessions, cx));
                return;
            };
            let session_id = session.id;
            let started = this.update(cx, |shell, _| {
                shell.attaching = Some(Attaching {
                    session_id,
                    snapshot: None,
                    output: Vec::new(),
                });
            });
            if started.is_err() {
                return;
            }
            let attach = connection
                .call::<SessionAttachResult>(Request::SessionAttach(SessionRef { session_id }));
            let attached = match attach.await {
                Ok(attached) => attached,
                Err(error) => {
                    tracing::warn!(%session_id, "session.attach failed: {error:#}");
                    let _ = this.update(cx, |shell, cx| {
                        shell.attaching = None;
                        shell.set_status(Status::OpenFailed(format!("{error:#}")), cx);
                    });
                    return;
                }
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let Some(attaching) = shell.attaching.take() else {
                    return;
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
                Incoming::Event(Event::SessionSnapshot(event)) => {
                    let session_id = event.session_id;
                    if let Some(pane) = pane.as_ref().filter(|_| pane_id == Some(session_id)) {
                        pane.update(cx, |pane, _| pane.load_snapshot(&event.snapshot));
                        fed = true;
                    } else if let Some(attaching) = self.attaching_mut(session_id) {
                        attaching.snapshot = Some(event.snapshot);
                        attaching.output.clear();
                    }
                }
                Incoming::Event(Event::SessionExited(event)) => {
                    if let Some(pane) = pane.as_ref().filter(|_| pane_id == Some(event.session_id))
                    {
                        pane.update(cx, |pane, _| pane.set_exited(event.exit_code));
                        fed = true;
                    }
                }
                // A session appeared while there was nothing to show: open it.
                Incoming::Event(Event::SessionState(_))
                    if matches!(self.status, Status::NoSessions) && self.pane.is_none() =>
                {
                    self.attach_latest(window, cx);
                }
                Incoming::Event(_) => {}
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
            Status::Opening => ("Opening the latest session…".into(), None),
            Status::NoSessions => (
                "No running sessions.".into(),
                Some("Start one with mk session spawn <project>; it opens here.".into()),
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
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<'_, Self>) -> impl IntoElement {
        let dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let theme = Theme::for_appearance(dark);
        if let Some(pane) = &self.pane {
            return div()
                .size_full()
                .flex()
                .bg(rgb(theme.bg))
                .child(pane.clone());
        }
        let (sentence, detail) = self.status_line();
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(space::S))
            .p(px(space::XL))
            .bg(rgb(theme.bg))
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
