//! Broadcast and undo dialogs.

use crate::*;

impl Shell {
    /// "Broadcast to agents…" over the live agent sessions of `project_id`, all checked.
    pub(crate) fn open_broadcast(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        self.broadcast = Some(Broadcast::new(project_id, self.sidebar.sessions()));
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    pub(crate) fn close_broadcast(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.broadcast = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    /// Typing edits the text, ↑ / ↓ move between sessions, Tab checks one, Enter sends, Esc
    /// closes.
    pub(crate) fn broadcast_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(dialog) = self.broadcast.as_mut() else {
            return;
        };
        if let Some(key) = task_dialog::dialog_key(&event.keystroke) {
            copy_selection(&key, &dialog.text, cx);
        }
        match dialog.key(&event.keystroke) {
            broadcast::Outcome::Close => return self.close_broadcast(window, cx),
            broadcast::Outcome::Send => return self.send_broadcast(window, cx),
            broadcast::Outcome::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    dialog.insert(&text);
                }
            }
            broadcast::Outcome::Edited => {}
        }
        cx.notify();
    }

    /// One `session.send` per checked session; each row shows its own result. A session that
    /// is working gets the text queued by the daemon (SPEC §8), shown as "queued".
    pub(crate) fn send_broadcast(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let serial = self.dialog_serial;
        let Some(dialog) = self.broadcast.as_mut() else {
            return;
        };
        for params in dialog.begin() {
            let session_id = params.session_id;
            let sent = connection.call::<SessionSendResult>(Request::SessionSend(params));
            cx.spawn_in(window, async move |this, cx| {
                let result = sent
                    .await
                    .map(|sent| sent.queued)
                    .map_err(|error| format!("{error:#}"));
                let _ = this.update(cx, |shell, cx| {
                    if let Err(error) = &result {
                        tracing::warn!(%session_id, "broadcast session.send failed: {error}");
                    }
                    if shell.dialog_serial != serial {
                        return;
                    }
                    if let Some(dialog) = shell.broadcast.as_mut() {
                        dialog.sent(session_id, result);
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    /// "Undo" on a Review / Done card: shows the modal confirmation and asks the daemon what
    /// undoing would discard (`task.undo_preview`).
    pub(crate) fn open_undo_dialog(
        &mut self,
        task_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(task) = self
            .board
            .task(task_id)
            .filter(|_| self.connection.is_some())
        else {
            return;
        };
        self.workspace_menu = None;
        self.task_dialog = None;
        self.undo_dialog = Some(UndoDialog::new(task));
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        self.load_undo_preview(window, cx);
    }

    /// (Re)reads the open undo dialog's preview.
    pub(crate) fn load_undo_preview(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(dialog) = self.undo_dialog.as_mut() else {
            return;
        };
        let serial = self.dialog_serial;
        dialog.preview = Preview::Loading;
        cx.notify();
        let previewed = connection.call::<UndoPreview>(dialog.preview_request());
        cx.spawn_in(window, async move |this, cx| {
            let preview = match previewed.await {
                Ok(preview) => Preview::loaded(preview),
                Err(error) => Preview::Failed(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                if let Some(dialog) = shell.open_undo(serial) {
                    dialog.preview = preview;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The open undo dialog, if it is still the one opened as `serial`.
    pub(crate) fn open_undo(&mut self, serial: u64) -> Option<&mut UndoDialog> {
        self.undo_dialog
            .as_mut()
            .filter(|_| self.dialog_serial == serial)
    }

    /// Closes the undo dialog, back to the Turns dialog it was opened from, if any.
    pub(crate) fn close_undo_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.undo_dialog = None;
        self.turns_dialog = self.turns_return.take();
        self.focus_pane(window, cx);
        cx.notify();
    }

    pub(crate) fn undo_dialog_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.dialog_nav(event, true, window, cx, |_, _| {}) {
            return;
        }
        cx.stop_propagation();
        if undo_dialog::cancels(&event.keystroke) {
            self.close_undo_dialog(window, cx);
        }
    }

    /// The destructive confirm: `task.undo` / `turn.restore` with exactly the previewed files.
    /// On failure the dialog shows why and the preview is read again (the daemon refuses a
    /// stale one).
    pub(crate) fn confirm_undo(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let serial = self.dialog_serial;
        let Some(dialog) = self.undo_dialog.as_mut() else {
            return;
        };
        let Ok(request) = dialog.confirm_request(self.sidebar.sessions()) else {
            return;
        };
        let target = dialog.target.clone();
        dialog.busy = true;
        dialog.error = None;
        cx.notify();
        let undone = connection.request(request);
        cx.spawn_in(window, async move |this, cx| {
            let failure = match undone.await {
                Ok(Outcome::Result(_)) => None,
                Ok(Outcome::Error(body)) => Some(body.message),
                Err(error) => Some(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| match failure {
                None => {
                    if shell.open_undo(serial).is_some() {
                        // Done: back to the pane, not to the Turns dialog it may have left.
                        shell.turns_return = None;
                        shell.close_undo_dialog(window, cx);
                    }
                }
                Some(error) => {
                    tracing::warn!(?target, "undo failed: {error}");
                    if let Some(dialog) = shell.open_undo(serial) {
                        dialog.undo_failed(&error);
                        shell.load_undo_preview(window, cx);
                    } else {
                        // Esc closed the dialog while it ran: no row left to say why.
                        shell.toast_error(format!("Undo failed: {error}"), cx);
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The "Undo task" confirmation: what is discarded (files, then the diff), and a
    /// destructive confirm that stays disabled until the preview allows it.
    pub(crate) fn render_undo_dialog(
        &self,
        dialog: &UndoDialog,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let label = |text: String| {
            div()
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        // No border: the raised surface sets the boxes apart inside the dialog (§14).
        let scroll_box = |id: &'static str, height: f32| {
            div()
                .id(id)
                .max_h(px(height))
                .flex()
                .flex_col()
                .py(px(space::XS))
                .rounded(px(radius::MEDIUM))
                .bg(rgb(theme.surface_raised))
                .overflow_scroll()
        };
        let error_row = |error: String| {
            div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                .child(div().text_color(rgb(theme.text)).child(error))
        };
        let nothing_changed = dialog.nothing_changed();
        let mut body: Vec<gpui::AnyElement> = Vec::new();
        let mut intro = format!(
            "Restores the workspace folder to how it was before {} ran",
            dialog.subject
        );
        // A task's checkpoint is a commit, a turn's a snapshot tree.
        let (checkpoint, cli) = match &dialog.target {
            undo_dialog::Target::Task(task_id) => {
                ("commit", Some(format!("mk task undo {task_id}")))
            }
            undo_dialog::Target::Turn(_) => ("snapshot", None),
        };
        match &dialog.preview {
            Preview::Loading => body.push(label("Loading the changes…".into()).into_any_element()),
            Preview::Failed(error) => body.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(
                        error_row(format!("Cannot list the changes: {error}"))
                            .flex_1()
                            .min_w_0(),
                    )
                    .child(
                        button("undo-retry", "Retry", theme, false)
                            .tooltip(tooltip(theme, "List the changes again", ""))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.load_undo_preview(window, cx);
                            })),
                    )
                    .into_any_element(),
            ),
            Preview::Loaded(_) if nothing_changed => {
                intro = format!("Nothing changed since {} ran", dialog.subject);
            }
            Preview::Loaded(shown) => {
                let commit: String = shown.preview.checkpoint.chars().take(7).collect();
                intro = format!("{intro} ({checkpoint} {commit})");
                body.push(label(format!("Files ({})", shown.files.len())).into_any_element());
                body.push(
                    scroll_box("undo-files", theme::UNDO_FILES_HEIGHT)
                        .children(shown.files.iter().map(|file| {
                            let (marker, effect) = file.change.label();
                            div()
                                .flex()
                                .gap(px(space::S))
                                .px(px(space::S))
                                .text_size(font_size::META)
                                .child(
                                    div()
                                        .flex_none()
                                        .font_family(theme.terminal_font_family.clone())
                                        .text_color(rgb(theme.text_muted))
                                        .child(marker),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .text_color(rgb(theme.text))
                                        .child(file.path.clone()),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .text_color(rgb(theme.text_muted))
                                        .child(effect),
                                )
                        }))
                        .into_any_element(),
                );
                body.push(label("Diff".into()).into_any_element());
                body.push(
                    scroll_box("undo-diff", theme::UNDO_DIFF_HEIGHT)
                        // `surface`, so hunk rows (`diff_hunk_header`) show as a band.
                        .bg(rgb(theme.surface))
                        .font_family(theme.terminal_font_family.clone())
                        .text_size(font_size::META)
                        .when(shown.lines.is_empty(), |el| {
                            el.child(
                                div()
                                    .px(px(space::S))
                                    .font_family(theme.ui_font_family.clone())
                                    .text_color(rgb(theme.text_muted))
                                    .child("No tracked file changed."),
                            )
                        })
                        .children(shown.lines.iter().map(|line| diff_row(line, theme)))
                        .into_any_element(),
                );
                if shown.cut() {
                    let more = match cli {
                        Some(cli) => {
                            format!("The diff is cut short here; `{cli}` prints all of it.")
                        }
                        None => "The diff is cut short here.".to_owned(),
                    };
                    body.push(label(more).into_any_element());
                }
            }
        }
        let confirm = dialog.confirm_request(self.sidebar.sessions());
        // Only the reason the user can act on (a live terminal, a working agent); loading,
        // failures and "nothing changed" already show in the body, "Discarding…" on the button.
        let hint = confirm.as_ref().err().copied().filter(|_| {
            matches!(dialog.preview, Preview::Loaded(_)) && !dialog.busy && !nothing_changed
        });
        let panel = div()
            .id("undo-dialog")
            .w(px(theme::UNDO_DIALOG_WIDTH))
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
                cx.listener(Self::undo_dialog_key_down),
            )
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(dialog.heading()),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child(if nothing_changed {
                        format!("{intro}: there is nothing to undo.")
                    } else {
                        format!("{intro}. The changes below are discarded and cannot be recovered.")
                    }),
            )
            .children(body)
            .children(dialog.error.clone().map(error_row))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .pt(px(space::S))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .children(hint),
                    )
                    .when(nothing_changed, |el| {
                        el.child(
                            button("undo-close", "Close", theme, true)
                                .focus_stop(theme)
                                .tooltip(tooltip(theme, "Close", "Esc"))
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    shell.close_undo_dialog(window, cx);
                                })),
                        )
                    })
                    .when(!nothing_changed, |el| {
                        el.child(
                            button("undo-cancel", "Cancel", theme, false)
                                .focus_stop(theme)
                                .tooltip(tooltip(theme, "Keep the changes", "Esc"))
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    shell.close_undo_dialog(window, cx);
                                })),
                        )
                        .child(match confirm {
                            Ok(_) => danger_button("undo-confirm", "Discard changes", theme)
                                .focus_stop(theme)
                                .tooltip(tooltip(theme, "Discard every change listed above", ""))
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    shell.confirm_undo(window, cx);
                                })),
                            Err(_) => {
                                let label = if dialog.busy {
                                    "Discarding…"
                                } else {
                                    "Discard changes"
                                };
                                disabled_button("undo-confirm", label, theme, false)
                                    .bg(rgb(theme.diff_del_bg))
                                    .text_color(rgb(theme.diff_del_text))
                            }
                        })
                    }),
            );
        Self::modal_shade("undo-dialog-shade", panel, cx)
    }

    /// "Broadcast to agents": a checkbox row per live agent session (with its last result),
    /// the text, then Cancel and Send (Enter).
    pub(crate) fn render_broadcast(
        &self,
        dialog: &Broadcast,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let hover = theme.surface_raised;
        let busy = dialog.busy();
        let rows = dialog.rows.iter().enumerate().map(|(index, row)| {
            let result = row.delivery.as_ref().map(|delivery| {
                let icon_color = match delivery {
                    BroadcastDelivery::Failed(_) => theme.status_exited,
                    _ => theme.text_muted,
                };
                div()
                    .id(("broadcast-result", index))
                    .flex_none()
                    .max_w(px(theme::BROADCAST_RESULT_WIDTH))
                    .flex()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(div().text_color(rgb(icon_color)).child(delivery.icon()))
                    .child(div().min_w_0().truncate().child(delivery.text()))
                    .tooltip(tooltip(theme, &delivery.text(), ""))
            });
            div()
                .id(("broadcast-row", index))
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::XS))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .border(px(theme::BORDER_WIDTH))
                .border_color(gpui::transparent_black())
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text))
                .when(index == dialog.cursor, |el| {
                    el.border_color(rgb(theme.accent))
                })
                .when(busy, |el| el.opacity(opacity::DISABLED))
                .when(!busy, |el| {
                    el.cursor_pointer()
                        .hover(move |style| style.bg(rgb(hover)))
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .tooltip(tooltip(theme, "Check or uncheck", "Tab"))
                })
                .on_click(cx.listener(move |shell, _, _, cx| {
                    if let Some(dialog) = shell.broadcast.as_mut() {
                        dialog.toggle(index);
                    }
                    cx.notify();
                }))
                .child(
                    div()
                        .size(px(space::M))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(radius::SMALL))
                        .border(px(theme::BORDER_WIDTH))
                        .border_color(rgb(theme.border))
                        .text_size(font_size::META)
                        .text_color(rgb(theme.accent))
                        .child(if row.checked { "✓" } else { "" }),
                )
                .child(div().flex_1().min_w_0().truncate().child(row.label.clone()))
                .children(result)
        });
        let send_label = if busy { "Sending…" } else { "Send" };
        let footer = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(space::S))
            .pt(px(space::S));
        let footer = if dialog.rows.is_empty() {
            footer
                .child(
                    button("broadcast-close", "Close", theme, false)
                        .tooltip(tooltip(theme, "Close", "Esc"))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_broadcast(window, cx);
                        })),
                )
                .child(
                    button("broadcast-terminals", "Show terminals", theme, true)
                        .tooltip(tooltip(theme, "Start an agent there, then broadcast", ""))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_broadcast(window, cx);
                            shell.show_view(View::Terminals, window, cx);
                        })),
                )
        } else {
            footer
                .child(
                    button("broadcast-cancel", "Cancel", theme, false)
                        .tooltip(tooltip(theme, "Close the dialog", "Esc"))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_broadcast(window, cx);
                        })),
                )
                .child(if dialog.can_send() {
                    button("broadcast-send", send_label, theme, true)
                        .tooltip(tooltip(theme, "Send to the checked sessions", "Enter"))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.send_broadcast(window, cx);
                        }))
                } else {
                    disabled_button("broadcast-send", send_label, theme, true).tooltip(tooltip(
                        theme,
                        "Type a message and check a session",
                        "",
                    ))
                })
        };
        let panel = div()
            .id("broadcast-dialog")
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
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::broadcast_key_down))
            .relative()
            .child(self.typing(&self.dialog_focus, Self::broadcast_key_down, cx))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Broadcast to agents"),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child(if dialog.rows.is_empty() {
                        "No live agent sessions in this workspace."
                    } else {
                        "Sends the text, then Enter, to every checked session. A working agent \
                         gets it when its turn ends."
                    }),
            )
            .when(!dialog.rows.is_empty(), |el| {
                el.child(
                    div()
                        .id("broadcast-sessions")
                        .max_h(px(theme::SEND_SESSIONS_HEIGHT))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children(rows),
                )
                .child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child("↑↓ move · Tab check · Enter send"),
                )
                .child(caret_field(
                    "broadcast-text",
                    &dialog.text,
                    "e.g. run the tests",
                    true,
                    theme,
                    cx,
                    |shell, caret| {
                        if let Some(dialog) = shell.broadcast.as_mut() {
                            dialog.text.click(caret.at, caret.extend, caret.clicks);
                        }
                    },
                ))
            })
            .child(footer);
        Self::modal_shade("broadcast-shade", panel, cx)
    }
}
