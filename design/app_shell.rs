//! The main window layout: sidebar | (toolbar, tab strip, content).
//!
//! Module layout expected:
//!   mod theme;      // theme.rs
//!   mod sidebar;    // sidebar.rs
//!   mod app_shell;  // this file

use gpui::{
    div, prelude::*, px, rgb, AnyElement, Context, Div, Entity, FontWeight, SharedString,
    Stateful, Window,
};

use crate::sidebar::Sidebar;
use crate::theme::*;

// ── Model ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    Board,
    Terminals,
    Docs,
    Files,
    Git,
}

impl View {
    const ALL: [View; 5] = [View::Board, View::Terminals, View::Docs, View::Files, View::Git];

    fn label(self) -> &'static str {
        match self {
            View::Board => "Board",
            View::Terminals => "Terminals",
            View::Docs => "Docs",
            View::Files => "Files",
            View::Git => "Git",
        }
    }
}

pub struct Tab {
    pub title: SharedString,
    pub panes: usize,
}

pub struct AppShell {
    sidebar: Entity<Sidebar>,
    view: View,
    tabs: Vec<Tab>,
    active_tab: usize,
}

impl AppShell {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            sidebar: cx.new(|_| Sidebar::demo()),
            view: View::Terminals,
            tabs: vec![Tab { title: "Terminal".into(), panes: 0 }],
            active_tab: 0,
        }
    }

    // Hook these up to your real actions.
    fn new_agent(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }
    fn new_shell(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }
    fn run(&mut self, cx: &mut Context<Self>) {
        cx.notify();
    }
}

// ── Root layout ──────────────────────────────────────────────────────────────

impl Render for AppShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .text_size(px(13.))
            .child(self.sidebar.clone())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(self.render_toolbar(cx))
                    .when(self.view == View::Terminals, |el| {
                        el.child(self.render_tab_strip(cx))
                    })
                    .child(div().flex_1().min_h_0().child(self.render_content(cx))),
            )
    }
}

impl AppShell {
    // ── Toolbar: view switcher on the left, actions on the right ────────────
    fn render_toolbar(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .flex_none()
            .h(px(44.))
            .px_3()
            .flex()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(rgb(DIVIDER))
            .child(self.render_view_switcher(cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(ghost_button("history", "History"))
                    .child(
                        ghost_button("add-agent", "+ Agent")
                            .on_click(cx.listener(|this, _, _, cx| this.new_agent(cx))),
                    )
                    .child(
                        ghost_button("add-shell", "+ Shell")
                            .on_click(cx.listener(|this, _, _, cx| this.new_shell(cx))),
                    )
                    .child(div().w(px(6.)))
                    .child(self.render_run_button(cx)),
            )
    }

    fn render_view_switcher(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .p(px(2.))
            .gap(px(2.))
            .rounded_lg()
            .bg(rgb(SURFACE))
            .border_1()
            .border_color(rgb(BORDER))
            .children(View::ALL.into_iter().map(|view| {
                let active = self.view == view;
                div()
                    .id(view.label())
                    .h(px(26.))
                    .px_3()
                    .flex()
                    .items_center()
                    .rounded_md()
                    .cursor_pointer()
                    .text_color(rgb(if active { TEXT } else { MUTED }))
                    .when(active, |el| {
                        el.bg(rgb(SEGMENT_ON)).font_weight(FontWeight::MEDIUM)
                    })
                    .when(!active, |el| el.hover(|s| s.text_color(rgb(TEXT))))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.view = view;
                        cx.notify();
                    }))
                    .child(view.label())
            }))
    }

    /// Split button: main "Run" action + a chevron for the run-config menu.
    fn render_run_button(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .rounded_md()
            .overflow_hidden()
            .child(
                div()
                    .id("run")
                    .h(px(28.))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .bg(rgb(ACCENT))
                    .hover(|s| s.bg(rgb(ACCENT_HOVER)))
                    .cursor_pointer()
                    .text_color(rgb(ON_ACCENT))
                    .font_weight(FontWeight::MEDIUM)
                    .on_click(cx.listener(|this, _, _, cx| this.run(cx)))
                    .child(div().text_size(px(10.)).child("▶"))
                    .child("Run"),
            )
            .child(
                div()
                    .id("run-menu")
                    .h(px(28.))
                    .w(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(ACCENT))
                    .hover(|s| s.bg(rgb(ACCENT_HOVER)))
                    .border_l_1()
                    .border_color(tint(0xffffff, 0x40))
                    .cursor_pointer()
                    .text_color(rgb(ON_ACCENT))
                    .text_size(px(10.))
                    .child("▾"),
            )
    }

    // ── Tab strip (Terminals view only) ──────────────────────────────────────
    fn render_tab_strip(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .flex_none()
            .h(px(38.))
            .px_2()
            .flex()
            .items_center()
            .gap_1()
            .border_b_1()
            .border_color(rgb(DIVIDER))
            .children(self.tabs.iter().enumerate().map(|(ix, tab)| {
                let active = ix == self.active_tab;
                div()
                    .id(("tab", ix))
                    .group("tab")
                    .h(px(28.))
                    .pl_3()
                    .pr(px(6.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded_md()
                    .border_1()
                    .cursor_pointer()
                    .when(active, |el| {
                        el.bg(rgb(SURFACE))
                            .border_color(rgb(BORDER))
                            .font_weight(FontWeight::MEDIUM)
                    })
                    .when(!active, |el| {
                        el.border_color(gpui::transparent_black())
                            .text_color(rgb(MUTED))
                            .hover(|s| s.bg(rgb(HOVER)).text_color(rgb(TEXT)))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.active_tab = ix;
                        cx.notify();
                    }))
                    .child(tab.title.clone())
                    .child(
                        div()
                            .id(("tab-close", ix))
                            .size(px(16.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_sm()
                            .text_size(px(11.))
                            .text_color(rgb(SUBTLE))
                            .when(!active, |el| {
                                el.opacity(0.).group_hover("tab", |s| s.opacity(1.))
                            })
                            .hover(|s| s.bg(rgb(HOVER)).text_color(rgb(TEXT)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                if this.tabs.len() > 1 {
                                    this.tabs.remove(ix);
                                    this.active_tab = this.active_tab.min(this.tabs.len() - 1);
                                    cx.notify();
                                }
                            }))
                            .child("✕"),
                    )
            }))
            .child(
                ghost_button("new-tab", "+").on_click(cx.listener(|this, _, _, cx| {
                    this.tabs.push(Tab { title: "Terminal".into(), panes: 0 });
                    this.active_tab = this.tabs.len() - 1;
                    cx.notify();
                })),
            )
    }

    // ── Content ──────────────────────────────────────────────────────────────
    fn render_content(&self, cx: &Context<Self>) -> AnyElement {
        match self.view {
            View::Terminals => {
                let empty = self.tabs.get(self.active_tab).map_or(true, |t| t.panes == 0);
                if empty {
                    self.render_empty_terminals(cx).into_any_element()
                } else {
                    // Replace with your pane grid.
                    div().size_full().into_any_element()
                }
            }
            other => empty_state(
                ">_",
                format!("{} is empty", other.label()),
                "Nothing to show here yet.",
            )
            .into_any_element(),
        }
    }

    fn render_empty_terminals(&self, cx: &Context<Self>) -> impl IntoElement {
        let (agent_keys, shell_keys) = if cfg!(target_os = "macos") {
            ("⌘⇧A", "⌘⇧S")
        } else {
            ("Ctrl+Shift+A", "Ctrl+Shift+S")
        };

        empty_state(
            ">_",
            "No panes in this tab",
            "Start an agent or a shell, or pick a session from the sidebar.",
        )
        .child(
            div()
                .mt_3()
                .flex()
                .gap_2()
                .child(
                    primary_button("empty-new-agent", "New agent", agent_keys)
                        .on_click(cx.listener(|this, _, _, cx| this.new_agent(cx))),
                )
                .child(
                    secondary_button("empty-new-shell", "New shell", shell_keys)
                        .on_click(cx.listener(|this, _, _, cx| this.new_shell(cx))),
                ),
        )
    }
}

// ── Reusable pieces ──────────────────────────────────────────────────────────

/// Centered empty state; returns the inner column so callers can append actions.
fn empty_state(
    glyph: &'static str,
    title: impl Into<SharedString>,
    body: impl Into<SharedString>,
) -> Div {
    div().size_full().flex().items_center().justify_center().child(
        div()
            .max_w(px(320.))
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .child(
                div()
                    .size(px(44.))
                    .rounded_lg()
                    .bg(rgb(SURFACE))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(ACCENT_TEXT))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(glyph),
            )
            .child(
                div()
                    .mt_1()
                    .text_size(px(15.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(title.into()),
            )
            .child(
                div()
                    .text_center()
                    .text_color(rgb(MUTED))
                    .line_height(px(20.))
                    .child(body.into()),
            ),
    )
}

fn ghost_button(id: &'static str, label: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(28.))
        .px_2()
        .flex()
        .items_center()
        .rounded_md()
        .cursor_pointer()
        .text_color(rgb(MUTED))
        .hover(|s| s.bg(rgb(HOVER)).text_color(rgb(TEXT)))
        .child(label)
}

fn primary_button(id: &'static str, label: &'static str, keys: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(32.))
        .px_3()
        .flex()
        .items_center()
        .gap_2()
        .rounded_md()
        .bg(rgb(ACCENT))
        .hover(|s| s.bg(rgb(ACCENT_HOVER)))
        .cursor_pointer()
        .text_color(rgb(ON_ACCENT))
        .font_weight(FontWeight::MEDIUM)
        .child(label)
        .child(kbd(keys, tint(0xffffff, 0x2a), ON_ACCENT))
}

fn secondary_button(id: &'static str, label: &'static str, keys: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(32.))
        .px_3()
        .flex()
        .items_center()
        .gap_2()
        .rounded_md()
        .bg(rgb(SURFACE))
        .border_1()
        .border_color(rgb(BORDER))
        .hover(|s| s.bg(rgb(HOVER)))
        .cursor_pointer()
        .child(label)
        .child(kbd(keys, rgb(BORDER), MUTED))
}

fn kbd(keys: &'static str, bg: impl Into<gpui::Hsla>, fg: u32) -> impl IntoElement {
    div()
        .px_1()
        .rounded_sm()
        .bg(bg.into())
        .text_size(px(11.))
        .font_weight(FontWeight::NORMAL)
        .text_color(rgb(fg))
        .child(keys)
}

// ── Entry point example ──────────────────────────────────────────────────────
// fn main() {
//     gpui::Application::new().run(|cx: &mut gpui::App| {
//         cx.open_window(gpui::WindowOptions::default(), |_, cx| cx.new(AppShell::new))
//             .unwrap();
//     });
// }
