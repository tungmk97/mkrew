//! Send comments / selection dialogs and the Changes dialog.

use crate::*;

impl Shell {
    /// "Send to agent" (Ctrl+Shift+S): the shown doc's or diff's open comments (only the
    /// selected ones if the selection covers some) go to the session picked in a modal dialog.
    /// It offers the doc's last modifying session first, or in a task's changes the task's.
    pub(crate) fn open_send_dialog(
        &mut self,
        project_id: Ulid,
        context: CommentContext,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let (comments, file_path, last) = match context {
            CommentContext::Doc => {
                let Some(docs) = self.docs.get(&project_id) else {
                    return;
                };
                let Doc::Shown(_) = &docs.doc else {
                    return;
                };
                let last = docs.selected_file().and_then(|file| file.last_session_id);
                (&docs.comments, docs.selected.clone(), last)
            }
            CommentContext::Diff => {
                let Some(view) = self.git.get(&project_id) else {
                    return;
                };
                let last = view.since_session(self.sidebar.sessions());
                let path = view.selected.as_ref().map(|target| target.path.clone());
                (&view.comments, path, last)
            }
        };
        let (comments::List::Loaded(list), Some(file_path)) = (&comments.list, file_path) else {
            return;
        };
        let comment_ids = doc_dialog::sendable(list, comments.selection);
        if comment_ids.is_empty() || self.connection.is_none() {
            return;
        }
        let dialog = SendDialog {
            task: context == CommentContext::Diff && last.is_some(),
            ..SendDialog::new(project_id, file_path, comment_ids, last)
        };
        self.open_doc_dialog(DocDialog::Send(dialog), window, cx);
    }

    /// "View changes" of a sent thread: its file as sent vs as shown now.
    pub(crate) fn open_changes(
        &mut self,
        project_id: Ulid,
        root: CommentId,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(docs) = self.docs.get(&project_id) else {
            return;
        };
        let (Doc::Shown(shown), comments::List::Loaded(list)) = (&docs.doc, &docs.comments.list)
        else {
            return;
        };
        let changes = list
            .iter()
            .find(|comment| comment.id == root)
            .and_then(|root| Changes::new(root, &shown.lines));
        if let Some(changes) = changes {
            self.open_doc_dialog(DocDialog::Changes(changes), window, cx);
        }
    }

    pub(crate) fn open_doc_dialog(
        &mut self,
        dialog: DocDialog,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        self.doc_dialog = Some(dialog);
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    pub(crate) fn close_doc_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.doc_dialog = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    /// Esc closes a Docs dialog; in "Send to agent" ↑ / ↓ pick the session, Enter sends.
    pub(crate) fn doc_dialog_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.dialog_nav(event, true, window, cx, |_, _| {}) {
            return;
        }
        cx.stop_propagation();
        let key = doc_dialog::key(&event.keystroke);
        match (self.doc_dialog.as_mut(), key) {
            (_, Some(doc_dialog::Key::Close)) => self.close_doc_dialog(window, cx),
            (Some(DocDialog::Send(dialog)), Some(doc_dialog::Key::Move(by))) => {
                dialog.step(self.sidebar.sessions(), by);
                cx.notify();
            }
            (Some(DocDialog::Send(_)), Some(doc_dialog::Key::Send)) => {
                self.confirm_send_dialog(window, cx);
            }
            _ => {}
        }
    }

    /// `comment.send_to_agent`: sent → the dialog closes (the threads show `sent`); failed →
    /// it stays with the reason.
    pub(crate) fn send_comments(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let serial = self.dialog_serial;
        let Some(DocDialog::Send(dialog)) = self.doc_dialog.as_mut() else {
            return;
        };
        let Some(params) = dialog.send(self.sidebar.sessions()) else {
            return;
        };
        let project_id = dialog.project_id;
        cx.notify();
        let sent = connection.call::<Vec<DocComment>>(Request::CommentSendToAgent(params));
        cx.spawn_in(window, async move |this, cx| {
            let sent = sent.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let open = shell.dialog_serial == serial
                    && matches!(shell.doc_dialog, Some(DocDialog::Send(_)));
                match sent {
                    Ok(comments) => {
                        // Each view keeps only its own context's comments.
                        for context in [CommentContext::Doc, CommentContext::Diff] {
                            if let Some(list) = shell.comments_mut(project_id, context) {
                                for comment in &comments {
                                    list.apply_event(project_id, comment);
                                }
                            }
                        }
                        if open {
                            shell.close_doc_dialog(window, cx);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "comment.send_to_agent failed: {error:#}");
                        if let (true, Some(DocDialog::Send(dialog))) =
                            (open, shell.doc_dialog.as_mut())
                        {
                            dialog.failed(format!("Cannot send: {error:#}"));
                        } else {
                            shell.toast_error(format!("Cannot send the comments: {error:#}"), cx);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Send selection" (Ctrl+Shift+L, §20B.4): the picked lines of Files or the Git diff, or
    /// the focused terminal's selected text, go to a session picked in the send dialog (the
    /// task's session for Git task changes, else the most recently active agent).
    pub(crate) fn open_selection_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self.workspace.filter(|_| self.connection.is_some()) else {
            return;
        };
        let sessions = self.sidebar.sessions();
        let (file_path, prefill, task) = match self.view() {
            view @ (View::Editor | View::Diff) => {
                let picked = self
                    .picked_lines(project_id, view)
                    .filter(|(_, _, _, text)| !text.trim().is_empty());
                let Some((path, start, end, text)) = picked else {
                    return;
                };
                let task = self
                    .git
                    .get(&project_id)
                    .filter(|_| view == View::Diff)
                    .and_then(|git| git.since_session(sessions));
                let prefill = composer::selection_prefill(Some((&path, start, end)), &text);
                (format!("{path}:{start}-{end}"), prefill, task)
            }
            View::Terminals => {
                let pane = self
                    .focused_session()
                    .and_then(|session| self.panes.get(&session))
                    .cloned();
                // Sending ends copy mode (held output resumes); the text is read first.
                let text = pane
                    .and_then(|pane| pane.update(cx, |pane, cx| pane.take_selection_text(cx)))
                    .filter(|text| !text.trim().is_empty());
                let Some(text) = text else {
                    return;
                };
                (
                    String::new(),
                    composer::selection_prefill(None, &text),
                    None,
                )
            }
            _ => return,
        };
        let dialog = SendDialog::selection(project_id, file_path, prefill, task, sessions);
        self.open_doc_dialog(DocDialog::Send(dialog), window, cx);
    }

    /// The send dialog's Send: comments go out, a selection opens the picked session's
    /// composer with it.
    pub(crate) fn confirm_send_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        match &self.doc_dialog {
            Some(DocDialog::Send(dialog)) if dialog.prefill.is_some() => {
                self.send_selection(window, cx);
            }
            _ => self.send_comments(window, cx),
        }
    }

    /// Shows the selected session's pane with its composer, the selection typed after its
    /// draft; the user adds the instruction and sends.
    pub(crate) fn send_selection(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(DocDialog::Send(dialog)) = &self.doc_dialog else {
            return;
        };
        let (Some(prefill), Some(session)) = (
            dialog.prefill.clone(),
            dialog.target(self.sidebar.sessions()),
        ) else {
            return;
        };
        self.doc_dialog = None;
        self.composers.open(session);
        let before = self.composers.shown(session).map_or("", |d| d.text.text());
        let text = prompt_picker::on_new_line(before, &prefill);
        self.composers.insert(session, &text);
        self.composer_scroll.scroll_to_bottom();
        self.open_session(session, window, cx);
        if self.composer_session() == Some(session) {
            window.focus(&self.composer_focus, cx);
        }
        cx.notify();
    }

    /// A Docs dialog: its title, a sentence, its body and the buttons, in the shared modal
    /// frame (Esc closes it; "Send to agent" also takes ↑ / ↓ / Enter).
    pub(crate) fn doc_dialog_panel(
        &self,
        id: &'static str,
        width: f32,
        title: String,
        sentence: String,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .w(px(width))
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
                cx.listener(Self::doc_dialog_key_down),
            )
            .child(
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(title),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child(sentence),
            )
    }

    /// "Send to agent": the workspace's live agent sessions to pick from (the file's last
    /// modifying session first selected), then Cancel / Send.
    pub(crate) fn render_send_dialog(
        &self,
        dialog: &SendDialog,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let choices = dialog.choices(self.sidebar.sessions());
        let selected = dialog.selected(&choices);
        let selection = dialog.prefill.is_some();
        let count = dialog.comment_ids.len();
        let what = if selection {
            "the selection".to_owned()
        } else if count == 1 {
            "1 open comment".to_owned()
        } else {
            format!("{count} open comments")
        };
        let sentence = if choices.is_empty() {
            format!("No agent is running in this workspace to take {what}.")
        } else if selection {
            "Adds the selection to the composer of the session below; type your \
             instruction, then send."
                .to_owned()
        } else {
            format!(
                "Types {what} into the session below as one prompt (queued while it is \
                 working). It replies to each with mk comment reply."
            )
        };
        let hover = theme.surface_raised;
        let rows: Vec<gpui::AnyElement> = choices
            .iter()
            .enumerate()
            .map(|(index, choice)| {
                let session_id = choice.session_id;
                let picked = selected == Some(index);
                let last = dialog.last_session_id == Some(session_id);
                div()
                    .id(SharedString::from(format!("send-{session_id}")))
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .px(px(space::S))
                    .py(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .text_size(font_size::BODY)
                    // The pick has a focus-colored ring, so it stays visible under hover.
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(if picked {
                        theme.pane_focus_border
                    } else {
                        theme.surface
                    }))
                    .when(picked, |el| el.bg(rgb(theme.surface_raised)))
                    .when(!picked && !dialog.busy, |el| {
                        el.cursor_pointer()
                            .hover(move |style| style.bg(rgb(hover)))
                            .active(|style| style.opacity(opacity::ACTIVE))
                    })
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        if let Some(DocDialog::Send(dialog)) = shell.doc_dialog.as_mut() {
                            dialog.pick(session_id);
                            cx.notify();
                        }
                    }))
                    .child(status_mark(choice.state, now, theme))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(rgb(theme.text))
                            .when(picked, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                            .child(choice.label.clone()),
                    )
                    .when(last, |el| {
                        el.child(
                            div()
                                .flex_none()
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child(dialog.last_label()),
                        )
                    })
                    .child(
                        div()
                            .flex_none()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child(status::text(
                                choice.state,
                                None,
                                choice.state_changed_at,
                                now,
                            )),
                    )
                    .into_any_element()
            })
            .collect();
        let footer = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(space::S))
            .pt(px(space::S));
        let footer = if choices.is_empty() {
            footer
                .child(
                    button("send-close", "Close", theme, false)
                        .focus_stop(theme)
                        .tooltip(tooltip(theme, "Close", "Esc"))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_doc_dialog(window, cx);
                        })),
                )
                .child(
                    button("send-terminals", "Show terminals", theme, true)
                        .focus_stop(theme)
                        .tooltip(tooltip(theme, "Start an agent there, then send", ""))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_doc_dialog(window, cx);
                            shell.show_view(View::Terminals, window, cx);
                        })),
                )
        } else {
            footer
                .child(
                    button("send-cancel", "Cancel", theme, false)
                        .focus_stop(theme)
                        .tooltip(tooltip(
                            theme,
                            if selection {
                                "Close without sending"
                            } else {
                                "Keep the comments open"
                            },
                            "Esc",
                        ))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_doc_dialog(window, cx);
                        })),
                )
                .child(if dialog.busy {
                    disabled_button("send-confirm", "Sending…", theme, true)
                } else if selection {
                    button("send-confirm", "Open composer", theme, true)
                        .focus_stop(theme)
                        .tooltip(tooltip(
                            theme,
                            "Add the selection to this session's composer",
                            "Enter",
                        ))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.send_selection(window, cx);
                        }))
                } else {
                    button("send-confirm", "Send", theme, true)
                        .focus_stop(theme)
                        .tooltip(tooltip(theme, "Send to the selected session", "Enter"))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.send_comments(window, cx);
                        }))
                })
        };
        let panel = self
            .doc_dialog_panel(
                "send-dialog",
                theme::DIALOG_WIDTH,
                match (selection, dialog.file_path.as_str()) {
                    (true, "") => "Send selection · terminal".to_owned(),
                    (true, path) => format!("Send selection · {path}"),
                    (false, path) => format!("Send to agent · {path}"),
                },
                sentence,
                theme,
                cx,
            )
            .when(!rows.is_empty(), |el| {
                el.child(
                    div()
                        .id("send-sessions")
                        .max_h(px(theme::SEND_SESSIONS_HEIGHT))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children(rows),
                )
            })
            .children(dialog.error.clone().map(|error| {
                div()
                    .flex()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                    .child(div().text_color(rgb(theme.text)).child(error))
            }))
            .child(footer);
        Self::modal_shade("send-dialog-shade", panel, cx)
    }

    /// "View changes": the sent thread's file as sent vs as shown now.
    pub(crate) fn render_changes(
        &self,
        changes: &Changes,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let session = changes.sent_to.map(|id| {
            self.sidebar
                .sessions()
                .iter()
                .find(|session| session.id == id)
                .map_or_else(|| "an agent".to_owned(), history::label)
        });
        let anchor = if changes.anchor.is_empty() {
            String::new()
        } else {
            format!(" ({})", changes.anchor.to_lowercase())
        };
        let sent = session.map_or_else(String::new, |session| format!(" to {session}"));
        let sentence = if changes.lines.is_empty() {
            format!("Nothing changed since the comment{anchor} was sent{sent}.")
        } else {
            format!("The file as it was when the comment{anchor} was sent{sent} (−) vs now (+).")
        };
        let panel = self
            .doc_dialog_panel(
                "changes-dialog",
                theme::UNDO_DIALOG_WIDTH,
                format!("Changes · {}", changes.file_path),
                sentence,
                theme,
                cx,
            )
            .when(!changes.lines.is_empty(), |el| {
                el.child(
                    div()
                        .id("changes-diff")
                        .max_h(px(theme::CHANGES_DIFF_HEIGHT))
                        .flex()
                        .flex_col()
                        .py(px(space::XS))
                        .rounded(px(radius::MEDIUM))
                        // `surface`, so hunk rows (`diff_hunk_header`) show as a band.
                        .bg(rgb(theme.surface))
                        .overflow_scroll()
                        .font_family(theme.terminal_font_family.clone())
                        .text_size(font_size::META)
                        .children(changes.lines.iter().map(|line| diff_row(line, theme))),
                )
            })
            .when(changes.hidden > 0, |el| {
                el.child(meta_line(
                    theme,
                    format!("{} more lines are left out.", changes.hidden).into(),
                ))
            })
            .child(
                div().flex().justify_end().pt(px(space::S)).child(
                    button("changes-close", "Close", theme, true)
                        .focus_stop(theme)
                        .tooltip(tooltip(theme, "Close", "Esc"))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_doc_dialog(window, cx);
                        })),
                ),
            );
        Self::modal_shade("changes-dialog-shade", panel, cx)
    }
}
