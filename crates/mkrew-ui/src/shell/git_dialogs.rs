//! Git discard and base dialogs.

use crate::*;

impl Shell {
    /// "Discard…" on an Unstaged / Untracked file: selects it (so its diff, what is lost,
    /// loads) and opens the confirmation.
    pub(crate) fn open_git_discard(
        &mut self,
        project_id: Ulid,
        target: git_view::Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(discard) = self.git_view(project_id).discard(&target) else {
            return;
        };
        self.select_git_file(project_id, target, window, cx);
        self.git_discard = Some(discard);
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    pub(crate) fn close_git_discard(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.git_discard = None;
        window.focus(&self.git_focus, cx);
        cx.notify();
    }

    pub(crate) fn git_discard_key_down(
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
            self.close_git_discard(window, cx);
        }
    }

    /// The destructive confirm: `git.discard` of the dialog's file, `confirm: true`.
    pub(crate) fn confirm_git_discard(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connected_or_toast(cx) else {
            return;
        };
        let serial = self.dialog_serial;
        let Some(dialog) = self.git_discard.as_mut().filter(|dialog| !dialog.busy) else {
            return;
        };
        dialog.busy = true;
        dialog.error = None;
        let project_id = dialog.project_id;
        let discarded = connection.request(Request::GitDiscard(dialog.params()));
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let failure = match discarded.await {
                Ok(Outcome::Result(_)) => None,
                Ok(Outcome::Error(body)) => Some(body.message),
                Err(error) => Some(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let open = shell
                    .git_discard
                    .as_mut()
                    .filter(|_| shell.dialog_serial == serial);
                match failure {
                    None => {
                        if open.is_some() {
                            shell.close_git_discard(window, cx);
                        }
                        shell.refresh_git(project_id, window, cx);
                    }
                    Some(error) => {
                        tracing::warn!(%project_id, "git.discard failed: {error}");
                        if let Some(dialog) = open {
                            dialog.busy = false;
                            dialog.error = Some(error);
                        } else {
                            shell.toast_error(format!("Discard failed: {error}"), cx);
                        }
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    /// The base menu of the Git view of `project_id`, prefilled with its configured base.
    pub(crate) fn open_git_base(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let status = match self.git.get(&project_id).map(|view| &view.status) {
            Some(git_view::Status::Loaded(status)) => Some(&**status),
            _ => None,
        };
        self.git_base = Some(BaseDialog::new(project_id, status));
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    pub(crate) fn close_git_base(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.git_base = None;
        window.focus(&self.git_focus, cx);
        cx.notify();
    }

    /// Typing edits the ref, Enter uses it (empty = auto-detect), Esc closes.
    pub(crate) fn git_base_key_down(
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
        // A control has the keyboard: the ref takes nothing but Esc / Enter.
        if self.dialog_control_focused(window)
            && !matches!(key, base_dialog::Key::Close | base_dialog::Key::Save)
        {
            return;
        }
        let paste = (key == base_dialog::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(dialog) = self.git_base.as_mut() else {
            return;
        };
        match key {
            base_dialog::Key::Close => return self.close_git_base(window, cx),
            base_dialog::Key::Save => return self.set_git_base(false, window, cx),
            base_dialog::Key::Paste => {
                if let Some(text) = paste {
                    dialog.insert(&text);
                }
            }
            base_dialog::Key::Edit(_) | base_dialog::Key::Insert(_) => {
                if let base_dialog::Key::Edit(edit) = &key {
                    copy_selection(edit, &dialog.text, cx);
                }
                dialog.key(&key);
            }
        }
        cx.notify();
    }

    /// `git.set_base` with the typed ref, or back to auto-detection (`auto`); on success the
    /// menu closes and the Git view reads its status again.
    pub(crate) fn set_git_base(
        &mut self,
        auto: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connected_or_toast(cx) else {
            return;
        };
        let serial = self.dialog_serial;
        let Some(params) = self.git_base.as_mut().and_then(|dialog| dialog.begin(auto)) else {
            return;
        };
        let project_id = params.project_id;
        let sent = connection.request(Request::GitSetBase(params));
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let failure = match sent.await {
                Ok(Outcome::Result(_)) => None,
                Ok(Outcome::Error(body)) => Some(body.message),
                Err(error) => Some(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let open = shell
                    .git_base
                    .as_mut()
                    .filter(|_| shell.dialog_serial == serial);
                match failure {
                    None => {
                        if open.is_some() {
                            shell.close_git_base(window, cx);
                        }
                        if shell.git.contains_key(&project_id) {
                            shell.refresh_git(project_id, window, cx);
                        }
                    }
                    Some(error) => {
                        tracing::warn!(%project_id, "git.set_base failed: {error}");
                        if let Some(dialog) = open {
                            dialog.failed(error);
                        } else {
                            shell.toast_error(format!("Cannot set the base: {error}"), cx);
                        }
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    /// "Discard changes?" of a Git view file: what is lost (the file's diff), Cancel and the
    /// danger confirm, enabled once that diff is shown. Looks like the undo dialog.
    pub(crate) fn render_git_discard(
        &self,
        dialog: &git_view::Discard,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let view = self.git.get(&dialog.project_id);
        let shown = view.and_then(|view| view.shown_diff(&dialog.target));
        let muted = |text: String| {
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let error_row = |error: String| {
            div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                .child(div().text_color(rgb(theme.text)).child(error))
        };
        // The file left its group meanwhile (e.g. an agent reverted or staged it).
        let gone = view.is_none_or(|view| view.file(&dialog.target).is_none());
        let failed = view.and_then(|view| match &view.diff {
            git_view::Diff::Failed(error) if view.selected.as_ref() == Some(&dialog.target) => {
                Some(error.clone())
            }
            _ => None,
        });
        let body = match (&shown, failed) {
            _ if gone => muted("This file no longer has these changes.".into()).into_any_element(),
            (Some(shown), _) if shown.diff.binary || shown.diff.too_large => muted(format!(
                "{} {} lines added, {} removed.",
                if shown.diff.binary {
                    "A binary file."
                } else {
                    "Too large to show here."
                },
                shown.diff.added,
                shown.diff.removed
            ))
            .into_any_element(),
            (Some(shown), _) => {
                let project_id = dialog.project_id;
                let more = shown
                    .rows
                    .len()
                    .saturating_sub(git_view::DISCARD_PREVIEW_ROWS);
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .child(
                        // `surface`, so hunk rows (`diff_hunk_header`) show as a band.
                        div()
                            .id("git-discard-diff")
                            .max_h(px(theme::UNDO_DIFF_HEIGHT))
                            .flex()
                            .flex_col()
                            .py(px(space::XS))
                            .rounded(px(radius::MEDIUM))
                            .bg(rgb(theme.surface))
                            .overflow_scroll()
                            .children(
                                (0..shown.rows.len().min(git_view::DISCARD_PREVIEW_ROWS)).map(
                                    |index| {
                                        git_diff_preview_row(shown, index, project_id, theme, cx)
                                    },
                                ),
                            ),
                    )
                    .when(more > 0, |el| {
                        el.child(muted(format!("… and {more} more lines.")))
                    })
                    .into_any_element()
            }
            (None, Some(error)) => {
                let project_id = dialog.project_id;
                let target = dialog.target.clone();
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(
                        error_row(format!("Cannot show what is lost: {error}"))
                            .flex_1()
                            .min_w_0(),
                    )
                    .child(
                        button("git-discard-retry", "Retry", theme, false)
                            .focus_stop(theme)
                            .tooltip(tooltip(theme, "Read the changes again", ""))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                let diff = shell.git_view(project_id).select(target.clone());
                                let requests = git_view::Requests {
                                    diff,
                                    ..git_view::Requests::default()
                                };
                                shell.git_requests(project_id, requests, window, cx);
                            })),
                    )
                    .into_any_element()
            }
            (None, None) => muted("Loading the changes…".into()).into_any_element(),
        };
        let ready = shown.is_some() && !gone && !dialog.busy;
        let panel = div()
            .id("git-discard-dialog")
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
                cx.listener(Self::git_discard_key_down),
            )
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Discard changes?"),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child(dialog.sentence()),
            )
            .child(body)
            .children(dialog.error.clone().map(error_row))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(space::S))
                    .pt(px(space::S))
                    .child(
                        button("git-discard-cancel", "Cancel", theme, false)
                            .focus_stop(theme)
                            .tooltip(tooltip(theme, "Keep the changes", "Esc"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.close_git_discard(window, cx);
                            })),
                    )
                    .child(if ready {
                        danger_button("git-discard-confirm", "Discard changes", theme)
                            .focus_stop(theme)
                            .tooltip(tooltip(theme, "Discard the changes shown above", ""))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.confirm_git_discard(window, cx);
                            }))
                    } else {
                        let label = if dialog.busy {
                            "Discarding…"
                        } else {
                            "Discard changes"
                        };
                        disabled_button("git-discard-confirm", label, theme, false)
                            .bg(rgb(theme.diff_del_bg))
                            .text_color(rgb(theme.diff_del_text))
                    }),
            );
        Self::modal_shade("git-discard-shade", panel, cx)
    }

    /// The Git base menu: the base in use, a field for another ref, then Cancel, "Auto-detect"
    /// and "Use this base" (Enter; an empty field also auto-detects).
    pub(crate) fn render_git_base(
        &self,
        dialog: &BaseDialog,
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
        let use_label = if dialog.busy {
            "Saving…"
        } else {
            "Use this base"
        };
        let panel = div()
            .id("git-base-dialog")
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
                cx.listener(Self::git_base_key_down),
            )
            .relative()
            .child(self.typing(&self.dialog_focus, Self::git_base_key_down, cx))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Base branch"),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child(
                        "“Against base” shows everything on this branch since it left the base.",
                    ),
            )
            .when(!dialog.current.is_empty(), |el| {
                el.child(meta_line(theme, dialog.current.clone().into()))
            })
            .child(caret_field(
                "git-base-ref",
                &dialog.text,
                "A branch or ref, e.g. develop or origin/main",
                self.dialog_focus.is_focused(window),
                theme,
                cx,
                |shell, caret| {
                    if let Some(dialog) = shell.git_base.as_mut() {
                        dialog.text.click(caret.at, caret.extend, caret.clicks);
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
                        button("git-base-cancel", "Cancel", theme, false)
                            .focus_stop(theme)
                            .tooltip(tooltip(theme, "Keep the current base", "Esc"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.close_git_base(window, cx);
                            })),
                    )
                    // Both look disabled while a save is in flight.
                    .child(if dialog.busy {
                        disabled_button("git-base-auto", "Auto-detect", theme, false)
                    } else {
                        button("git-base-auto", "Auto-detect", theme, false)
                            .focus_stop(theme)
                            .tooltip(tooltip(
                                theme,
                                "Use origin/HEAD, else main, else master",
                                "",
                            ))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.set_git_base(true, window, cx);
                            }))
                    })
                    .child(if dialog.busy {
                        disabled_button("git-base-use", use_label, theme, true)
                    } else {
                        button("git-base-use", use_label, theme, true)
                            .focus_stop(theme)
                            .tooltip(tooltip(theme, "Compare with the typed ref", "Enter"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.set_git_base(false, window, cx);
                            }))
                    }),
            );
        Self::modal_shade("git-base-shade", panel, cx)
    }
}
