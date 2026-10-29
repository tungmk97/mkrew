//! "Link plan" in a machine window: a typed path on the other computer instead of the OS file
//! picker (spec §6.4). The daemon checks the file; its refusal shows under the field.

use crate::*;

impl Shell {
    /// Opens the dialog for `session_id`'s plan.
    pub(crate) fn open_plan_path(
        &mut self,
        session_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(machine) = self.machine_name().map(str::to_owned) else {
            return;
        };
        self.plan_error = None;
        self.plan_dialog = Some(PlanDialog::new(session_id, machine));
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    pub(crate) fn close_plan_path(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.plan_dialog = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    /// Typing edits the path, Enter links it, Esc closes.
    pub(crate) fn plan_path_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.dialog_nav(event, true, window, cx, |_, _| {}) {
            return;
        }
        cx.stop_propagation();
        let Some(key) = base_dialog::key(&event.keystroke) else {
            return;
        };
        // A control has the keyboard: the path takes nothing but Esc / Enter.
        if self.dialog_control_focused(window)
            && !matches!(key, base_dialog::Key::Close | base_dialog::Key::Save)
        {
            return;
        }
        let paste = (key == base_dialog::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(dialog) = self.plan_dialog.as_mut() else {
            return;
        };
        match key {
            base_dialog::Key::Close => return self.close_plan_path(window, cx),
            base_dialog::Key::Save => return self.link_typed_plan(window, cx),
            base_dialog::Key::Paste => {
                if let Some(text) = paste {
                    dialog.insert(&text);
                }
            }
            base_dialog::Key::Edit(_) | base_dialog::Key::Insert(_) => {
                if let base_dialog::Key::Edit(edit) = &key {
                    copy_selection(edit, &dialog.path, cx);
                }
                dialog.key(&key);
            }
        }
        cx.notify();
    }

    /// `session.link_plan` with the typed path; on success the dialog closes (the daemon's
    /// `session.updated` / `session.plan` show the plan), else its refusal shows in the dialog.
    pub(crate) fn link_typed_plan(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connected_or_toast(cx) else {
            return;
        };
        let serial = self.dialog_serial;
        let Some(params) = self.plan_dialog.as_mut().and_then(PlanDialog::begin) else {
            cx.notify();
            return;
        };
        let session_id = params.session_id;
        let sent = connection.request(Request::SessionLinkPlan(params));
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let failure = match sent.await {
                Ok(Outcome::Result(_)) => None,
                Ok(Outcome::Error(body)) => Some(body.message),
                Err(error) => Some(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let open = shell
                    .plan_dialog
                    .as_mut()
                    .filter(|_| shell.dialog_serial == serial);
                match failure {
                    None => {
                        if open.is_some() {
                            shell.close_plan_path(window, cx);
                        }
                    }
                    Some(error) => {
                        tracing::warn!(%session_id, "session.link_plan failed: {error}");
                        match open {
                            Some(dialog) => dialog.failed(error),
                            None => shell.plan_error = Some((session_id, error)),
                        }
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    /// "Link plan": "Plan file on <machine>", the typed path, Cancel and "Link" (Enter).
    pub(crate) fn render_plan_path(
        &self,
        dialog: &PlanDialog,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let error_row = |error: String| {
            div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                .child(div().text_color(rgb(theme.text)).child(error))
        };
        let panel = div()
            .id("plan-path-dialog")
            .w(px(theme::DIALOG_WIDTH))
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
                true,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::plan_path_key_down),
            )
            .relative()
            .child(self.typing(&self.dialog_focus, Self::plan_path_key_down, cx))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Link plan"),
            )
            .child(meta_line(
                theme,
                format!("Plan file on {}", dialog.machine).into(),
            ))
            .child(caret_field(
                "plan-path-field",
                &dialog.path,
                "Path of the Markdown file",
                self.dialog_focus.is_focused(window),
                theme,
                cx,
                |shell, caret| {
                    if let Some(dialog) = shell.plan_dialog.as_mut() {
                        dialog.path.click(caret.at, caret.extend, caret.clicks);
                    }
                },
            ))
            .children(dialog.error.clone().map(error_row))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(space::S))
                    .pt(px(space::S))
                    .child(
                        button("plan-path-cancel", "Cancel", theme, false)
                            .focus_stop(theme)
                            .tooltip(tooltip(theme, "Keep the plan as it is", "Esc"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.close_plan_path(window, cx);
                            })),
                    )
                    .child(if dialog.busy {
                        disabled_button("plan-path-link", "Linking…", theme, true)
                    } else {
                        button("plan-path-link", "Link", theme, true)
                            .focus_stop(theme)
                            .tooltip(tooltip(
                                theme,
                                "Link the file as this session's plan",
                                "Enter",
                            ))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.link_typed_plan(window, cx);
                            }))
                    }),
            );
        Self::modal_shade("plan-path-shade", panel, cx)
    }
}
