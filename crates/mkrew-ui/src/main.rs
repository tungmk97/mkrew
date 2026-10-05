//! `mkrew`: the desktop app (SPEC §10). For now an empty window that connects to the daemon,
//! starting it if needed; workspaces and terminals follow in later M2 tasks.

// No console window next to the app in release builds (debug keeps stderr visible).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, FontWeight, SharedString, TitlebarOptions, Window, WindowAppearance,
    WindowBounds, WindowOptions, div, px, rgb, size,
};
use mkrew_core::Paths;
use mkrew_ui::client::Connection;
use mkrew_ui::theme::{self, Theme, font_size, space, weight};

fn main() {
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
    Connected { daemon_version: String },
    Disconnected,
    Failed(String),
}

struct Shell {
    status: Status,
    /// Kept for the views of later tasks; dropping it closes the connection.
    connection: Option<Connection>,
}

impl Shell {
    fn new(paths: Paths, window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        cx.observe_window_appearance(window, |_, _, cx| cx.notify())
            .detach();
        cx.spawn(async move |this, cx| {
            // Blocking (polls a starting daemon for up to 10 s): off the UI thread.
            let connected = cx
                .background_executor()
                .spawn(async move { mkrew_ui::connect::connect(&paths) })
                .await;
            let connection = match connected {
                Ok(connection) => connection,
                Err(error) => {
                    let _ = this.update(cx, |shell, cx| {
                        shell.status = Status::Failed(format!("{error:#}"));
                        cx.notify();
                    });
                    return;
                }
            };
            let incoming = connection.incoming();
            let attached = this.update(cx, |shell, cx| {
                shell.status = Status::Connected {
                    daemon_version: connection.hello().daemon_version.clone(),
                };
                shell.connection = Some(connection);
                cx.notify();
            });
            if attached.is_err() {
                return;
            }
            // Nothing consumes events yet (sidebar and terminals do, T2.3+); draining keeps
            // responses flowing and notices when the daemon goes away.
            while incoming.recv().await.is_ok() {}
            let _ = this.update(cx, |shell, cx| {
                shell.status = Status::Disconnected;
                shell.connection = None;
                cx.notify();
            });
        })
        .detach();
        Self {
            status: Status::Connecting,
            connection: None,
        }
    }

    /// One short sentence, plus optional detail shown smaller below it.
    fn status_line(&self) -> (SharedString, Option<SharedString>) {
        match &self.status {
            Status::Connecting => ("Connecting to the daemon…".into(), None),
            Status::Connected { daemon_version } => (
                "Connected.".into(),
                Some(format!("mkrew-daemon {daemon_version}").into()),
            ),
            Status::Disconnected => (
                "The daemon connection closed. Restart Mkrew to reconnect.".into(),
                None,
            ),
            Status::Failed(error) => (
                "Cannot reach the daemon.".into(),
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
