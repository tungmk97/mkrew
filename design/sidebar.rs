//! Redesigned workspace sidebar for GPUI.
//!
//! Drop this into your crate and render `Sidebar` inside your root view.
//! Uses only gpui primitives (no `ui` crate), so it should compile against a
//! recent gpui (2025+ API: `Window` + `Context<Self>`).

use std::time::Duration;

use gpui::{
    div, prelude::*, pulsating_between, px, rgb, Animation, AnimationExt, AnyElement,
    Context, FontWeight, SharedString, Window,
};

use crate::theme::*;

// ── Model ────────────────────────────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Working,
    Idle,
    NeedsInput,
}

impl SessionStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Working => "Working",
            Self::Idle => "Idle",
            Self::NeedsInput => "Needs input",
        }
    }
    fn label_color(self) -> u32 {
        match self {
            Self::Working => ACCENT_TEXT,
            Self::Idle => SUBTLE,
            Self::NeedsInput => WARNING,
        }
    }
}

pub struct Session {
    pub name: SharedString,
    pub status: SessionStatus,
    pub elapsed: SharedString,
}

pub struct Workspace {
    pub name: SharedString,
    pub color: u32,
    pub expanded: bool,
    pub sessions: Vec<Session>,
}

pub struct Sidebar {
    pub workspaces: Vec<Workspace>,
    pub selected: Option<(usize, usize)>,
    pub inbox: Vec<SharedString>,
}

impl Sidebar {
    pub fn demo() -> Self {
        let s = |name: &str, status, elapsed: &str| Session {
            name: name.to_string().into(),
            status,
            elapsed: elapsed.to_string().into(),
        };
        Self {
            workspaces: vec![
                Workspace { name: "EYS".into(), color: 0x7fb0ff, expanded: false, sessions: vec![] },
                Workspace {
                    name: "mkrew".into(),
                    color: 0x9a9fff,
                    expanded: true,
                    sessions: vec![s("omp", SessionStatus::Working, "6s")],
                },
                Workspace {
                    name: "LMS".into(),
                    color: 0xf0848a,
                    expanded: true,
                    sessions: vec![s("omp", SessionStatus::Idle, "50m")],
                },
            ],
            selected: Some((1, 0)),
            inbox: vec![],
        }
    }
}

// ── Rendering ────────────────────────────────────────────────────────────────
impl Render for Sidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .w(px(280.))
            .h_full()
            .bg(rgb(SIDEBAR))
            .border_r_1()
            .border_color(rgb(DIVIDER))
            .text_color(rgb(TEXT))
            .text_size(px(13.))
            .child(self.render_header())
            .child(
                div()
                    .id("workspace-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .px_2()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .children(
                        self.workspaces
                            .iter()
                            .enumerate()
                            .map(|(ix, ws)| self.render_workspace(ix, ws, cx)),
                    ),
            )
            .child(self.render_inbox())
    }
}

impl Sidebar {
    fn render_header(&self) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(44.))
            .pl_4()
            .pr_3()
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(MUTED))
                    .child("Workspaces"),
            )
            .child(icon_button("new-workspace", "+"))
    }

    fn render_workspace(&self, ix: usize, ws: &Workspace, cx: &Context<Self>) -> AnyElement {
        let working = ws
            .sessions
            .iter()
            .filter(|s| s.status == SessionStatus::Working)
            .count();
        let initial: SharedString = ws
            .name
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default()
            .into();

        let header = div()
            .id(("ws", ix))
            .group("ws-row")
            .flex()
            .items_center()
            .gap_2()
            .h(px(30.))
            .px_2()
            .rounded_md()
            .cursor_pointer()
            .hover(|s| s.bg(rgb(HOVER)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.workspaces[ix].expanded = !this.workspaces[ix].expanded;
                cx.notify();
            }))
            // chevron — swap for your Icon(ChevronRight/Down) if you have svg assets
            .child(
                div()
                    .w(px(10.))
                    .text_size(px(10.))
                    .text_color(rgb(SUBTLE))
                    .child(if ws.expanded { "▾" } else { "▸" }),
            )
            // colored avatar
            .child(
                div()
                    .size(px(18.))
                    .flex_none()
                    .rounded(px(5.))
                    .bg(tint(ws.color, 0x2e))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(ws.color))
                    .child(initial),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::MEDIUM)
                    .child(ws.name.clone()),
            )
            .when(working > 0, |el| el.child(working_badge(ix, working)))
            .when(!ws.expanded && working == 0 && !ws.sessions.is_empty(), |el| {
                el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(SUBTLE))
                        .child(ws.sessions.len().to_string()),
                )
            })
            // "more" button, only visible on row hover
            .child(
                div()
                    .opacity(0.)
                    .group_hover("ws-row", |s| s.opacity(1.))
                    .child(icon_button(("ws-more", ix), "⋯")),
            );

        div()
            .flex()
            .flex_col()
            .child(header)
            .when(ws.expanded && !ws.sessions.is_empty(), |el| {
                el.child(
                    div()
                        .ml(px(16.))
                        .pl_2()
                        .my_0p5()
                        .border_l_1()
                        .border_color(rgb(BORDER))
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .children(
                            ws.sessions
                                .iter()
                                .enumerate()
                                .map(|(jx, s)| self.render_session(ix, jx, s, cx)),
                        ),
                )
            })
            .into_any_element()
    }

    fn render_session(
        &self,
        ws_ix: usize,
        ix: usize,
        session: &Session,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let selected = self.selected == Some((ws_ix, ix));

        div()
            .id(("session", ws_ix * 10_000 + ix))
            .flex()
            .items_center()
            .gap_2()
            .h(px(28.))
            .px_2()
            .rounded_md()
            .cursor_pointer()
            .text_color(rgb(if selected { TEXT } else { TEXT_DIM }))
            .when(selected, |el| el.bg(rgb(SELECTED)))
            .when(!selected, |el| el.hover(|s| s.bg(rgb(HOVER))))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected = Some((ws_ix, ix));
                cx.notify();
            }))
            .child(status_dot(("dot", ws_ix * 10_000 + ix), session.status))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .when(selected, |el| el.font_weight(FontWeight::MEDIUM))
                    .child(session.name.clone()),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(rgb(session.status.label_color()))
                    .child(session.status.label()),
            )
            .child(
                div()
                    .min_w(px(24.))
                    .flex()
                    .justify_end()
                    .text_size(px(11.))
                    .text_color(rgb(SUBTLE))
                    .child(session.elapsed.clone()),
            )
    }

    fn render_inbox(&self) -> impl IntoElement {
        let empty = self.inbox.is_empty();
        let (icon_bg, icon_fg, icon) = if empty {
            (SUCCESS_BG, SUCCESS, "✓")
        } else {
            (WARNING_BG, WARNING, "!")
        };
        let message: SharedString = if empty {
            "Nothing needs you right now.".into()
        } else {
            format!("{} waiting for you", self.inbox.len()).into()
        };

        div()
            .m_2()
            .p_3()
            .rounded_lg()
            .bg(rgb(0x1a1b20))
            .border_1()
            .border_color(rgb(DIVIDER))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(
                div()
                    .size(px(28.))
                    .flex_none()
                    .rounded(px(8.))
                    .bg(rgb(icon_bg))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(icon_fg))
                    .font_weight(FontWeight::BOLD)
                    .child(icon),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .child(div().font_weight(FontWeight::MEDIUM).child("Inbox"))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.))
                            .text_color(rgb(MUTED))
                            .child(message),
                    ),
            )
    }
}

// ── Small components ─────────────────────────────────────────────────────────

fn icon_button(id: impl Into<gpui::ElementId>, glyph: &'static str) -> impl IntoElement {
    div()
        .id(id)
        .size(px(22.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .cursor_pointer()
        .text_color(rgb(MUTED))
        .hover(|s| s.bg(rgb(HOVER)).text_color(rgb(TEXT)))
        .child(glyph)
}

fn working_badge(ix: usize, count: usize) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(5.))
        .px(px(7.))
        .py(px(1.))
        .rounded_full()
        .bg(rgb(ACCENT_BG))
        .text_size(px(11.))
        .text_color(rgb(ACCENT_TEXT))
        .child(
            div()
                .size(px(6.))
                .rounded_full()
                .bg(rgb(ACCENT))
                .with_animation(
                    ("badge-pulse", ix),
                    pulse_animation(),
                    |el, delta| el.opacity(delta),
                ),
        )
        .child(count.to_string())
}

fn status_dot(id: impl Into<gpui::ElementId>, status: SessionStatus) -> AnyElement {
    let base = div().size(px(8.)).flex_none().rounded_full();
    match status {
        SessionStatus::Working => base
            .bg(rgb(ACCENT))
            .with_animation(id, pulse_animation(), |el, delta| el.opacity(delta))
            .into_any_element(),
        SessionStatus::NeedsInput => base.bg(rgb(WARNING)).into_any_element(),
        SessionStatus::Idle => base
            .border(px(1.5))
            .border_color(rgb(SUBTLE))
            .into_any_element(),
    }
}

fn pulse_animation() -> Animation {
    Animation::new(Duration::from_millis(1400))
        .repeat()
        .with_easing(pulsating_between(0.35, 1.0))
}

// ── Standalone preview ───────────────────────────────────────────────────────
// fn main() {
//     gpui::Application::new().run(|cx: &mut gpui::App| {
//         cx.open_window(gpui::WindowOptions::default(), |_, cx| cx.new(|_| Sidebar::demo()))
//             .unwrap();
//     });
// }
