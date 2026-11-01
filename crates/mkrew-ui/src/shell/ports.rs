//! The Ports dialog (spec 2026-11-01): every listening port of the daemon's machine by
//! workspace, a jump to its pane, and Stop.

use mkrew_protocol::{PortRow, PortStopParams};
use mkrew_ui::ports::{self, PortsDialog};

use crate::*;

impl Shell {
    /// Opens the dialog and asks `port.list` now and every [`theme::PORTS_TICK`] until it
    /// closes; nothing runs while it is closed.
    pub(crate) fn open_ports(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.connection.is_none() {
            return;
        }
        self.close_menus();
        self.ports_dialog = Some(PortsDialog::default());
        self.dialog_serial += 1;
        let serial = self.dialog_serial;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
        self.poll_ports(window, cx);
        cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(theme::PORTS_TICK).await;
                let open = this.update_in(cx, |shell, window, cx| {
                    let open = shell.ports_dialog.is_some() && shell.dialog_serial == serial;
                    if open {
                        shell.poll_ports(window, cx);
                    }
                    open
                });
                if !matches!(open, Ok(true)) {
                    return;
                }
            }
        })
        .detach();
    }

    pub(crate) fn close_ports(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.ports_dialog = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    fn poll_ports(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(dialog) = self.ports_dialog.as_mut() else {
            return;
        };
        if !dialog.begin_poll() {
            return;
        }
        let listed = connection.call::<Vec<PortRow>>(Request::PortList);
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await.map_err(|error| format!("{error:#}"));
            let _ = this.update(cx, |shell, cx| {
                if let Some(dialog) = shell.ports_dialog.as_mut() {
                    dialog.listed(listed);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// ■ or Delete on a row: a run command stops at once, any other row asks in the footer.
    fn ask_port_stop(&mut self, row: PortRow, window: &mut Window, cx: &mut Context<'_, Self>) {
        if ports::asks_first(&row) {
            if let Some(dialog) = self.ports_dialog.as_mut() {
                dialog.confirm = Some(row);
            }
            cx.notify();
        } else {
            self.stop_port(row.port, row.pid, window, cx);
        }
    }

    fn stop_port(&mut self, port: u16, pid: u32, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if let Some(dialog) = self.ports_dialog.as_mut() {
            dialog.confirm = None;
        }
        cx.notify();
        let stopped = connection.request(Request::PortStop(PortStopParams { port, pid }));
        cx.spawn_in(window, async move |this, cx| {
            let failure = match stopped.await {
                Ok(Outcome::Result(_)) => None,
                Ok(Outcome::Error(body)) => ports::stop_failure(pid, &body),
                Err(error) => Some(format!("Can't stop pid {pid}: {error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Some(dialog) = shell.ports_dialog.as_mut() {
                    dialog.error = failure;
                }
                shell.poll_ports(window, cx);
            });
        })
        .detach();
    }

    /// A row with a session: its workspace and pane; the dialog closes.
    fn go_to_port(&mut self, row: &PortRow, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(session) = row.session_id else {
            return;
        };
        self.ports_dialog = None;
        self.open_session(session, window, cx);
        cx.notify();
    }

    pub(crate) fn ports_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.dialog_nav(event, true, window, cx, |_, _| {}) {
            return;
        }
        cx.stop_propagation();
        let Some(dialog) = self.ports_dialog.as_mut() else {
            return;
        };
        let asking = dialog.confirm.is_some();
        match ports::key(&event.keystroke) {
            Some(ports::Key::Close) if asking => {
                dialog.confirm = None;
                cx.notify();
            }
            Some(ports::Key::Close) => self.close_ports(window, cx),
            // While the footer asks, only Tab, its buttons and Esc act.
            _ if asking => {}
            Some(ports::Key::Move(offset)) => {
                dialog.select_by(offset, self.sidebar.projects());
                cx.notify();
            }
            Some(ports::Key::Open) => {
                if let Some(row) = dialog.selected_row().cloned() {
                    self.go_to_port(&row, window, cx);
                }
            }
            Some(ports::Key::Stop) => {
                if let Some(row) = dialog.selected_row().cloned() {
                    self.ask_port_stop(row, window, cx);
                }
            }
            None => {}
        }
    }

    pub(crate) fn render_ports(
        &self,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let Some(dialog) = &self.ports_dialog else {
            return div().into_any_element();
        };
        let hover = theme.hover;
        let mut list = div()
            .id("ports-list")
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .max_h(px(theme::PORTS_LIST_HEIGHT))
            .overflow_y_scroll();
        let mut index = 0usize;
        for group in ports::groups(&dialog.rows, self.sidebar.projects()) {
            list = list.child(
                div()
                    .pt(px(space::S))
                    .text_size(font_size::OVERLINE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text_muted))
                    .child(group.title),
            );
            for row in group.rows {
                let selected = dialog.selected == Some((row.port, row.pid));
                let clickable = row.session_id.is_some();
                let go = row.clone();
                let stop = row.clone();
                list = list.child(
                    div()
                        .id(SharedString::from(format!("port-row-{index}")))
                        .flex()
                        .items_center()
                        .gap(px(space::M))
                        .px(px(space::S))
                        .py(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .text_size(font_size::BODY)
                        .when(selected, |el| el.bg(rgb(theme.surface_raised)))
                        .when(!selected && clickable, |el| {
                            el.cursor_pointer().hover(move |style| style.bg(rgb(hover)))
                        })
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.go_to_port(&go, window, cx);
                        }))
                        .child(
                            div()
                                .w(px(space::XXL * 2.0))
                                .font_weight(FontWeight(weight::MEDIUM))
                                .text_color(rgb(theme.text))
                                .child(format!(":{}", row.port)),
                        )
                        .child(
                            div()
                                .text_color(rgb(theme.text_dim))
                                .child(ports::place(&row)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(rgb(theme.text))
                                .child(ports::source(&row)),
                        )
                        .child(
                            div()
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child(format!("pid {}", row.pid)),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("port-stop-{index}")))
                                .px(px(space::XS))
                                .rounded(px(radius::SMALL))
                                .cursor_pointer()
                                .text_color(rgb(theme.danger_text))
                                .hover(|style| style.bg(rgb(theme.danger_bg)))
                                .tooltip(tooltip(theme, "Stop", "Delete"))
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    cx.stop_propagation();
                                    shell.ask_port_stop(stop.clone(), window, cx);
                                }))
                                .child("■"),
                        ),
                );
                index += 1;
            }
        }
        let muted = |text: &'static str| div().text_color(rgb(theme.text_muted)).child(text);
        let body = if !dialog.loaded {
            muted("Loading…").into_any_element()
        } else if dialog.rows.is_empty() {
            muted("No ports are listening.").into_any_element()
        } else {
            list.into_any_element()
        };
        let error = dialog.error.clone().map(|text| {
            div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                .child(div().text_color(rgb(theme.text)).child(text))
        });
        let footer = match &dialog.confirm {
            Some(row) => {
                let (port, pid) = (row.port, row.pid);
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child(ports::question(row)),
                    )
                    .child(
                        button("port-cancel", "Cancel", theme, false)
                            .focus_stop(theme)
                            .tooltip(tooltip(theme, "Keep it running", "Esc"))
                            .on_click(cx.listener(|shell, _, _, cx| {
                                if let Some(dialog) = shell.ports_dialog.as_mut() {
                                    dialog.confirm = None;
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        danger_button("port-stop", "Stop", theme)
                            .focus_stop(theme)
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                shell.stop_port(port, pid, window, cx);
                            })),
                    )
            }
            None => div().flex().justify_end().child(
                button("ports-close", "Close", theme, false)
                    .focus_stop(theme)
                    .tooltip(tooltip(theme, "Close", "Esc"))
                    .on_click(cx.listener(|shell, _, window, cx| shell.close_ports(window, cx))),
            ),
        };
        let panel = div()
            .id("ports-dialog")
            .w(px(theme::PORTS_DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .dialog_root(
                &self.dialog_focus,
                false,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::ports_key_down),
            )
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Ports"),
            )
            .children(error)
            .child(body)
            .child(footer);
        Self::modal_shade("ports-shade", panel, cx)
    }
}
