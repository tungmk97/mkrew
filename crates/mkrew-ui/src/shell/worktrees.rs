//! Worktree dialog and the fan-out Compare dialog.

use crate::*;

impl Shell {
    /// "Merge back…" / "Remove worktree…" on a Review / Done card (SPEC §20E.1): shows the
    /// modal confirmation and reads what it lists.
    pub(crate) fn open_worktree_dialog(
        &mut self,
        kind: worktree_dialog::Kind,
        task_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(dialog) = self
            .board
            .task(task_id)
            .filter(|_| self.connection.is_some())
            .and_then(|task| WorktreeDialog::new(kind, task))
        else {
            return;
        };
        self.show_worktree_dialog(dialog, window, cx);
    }

    pub(crate) fn show_worktree_dialog(
        &mut self,
        dialog: WorktreeDialog,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        self.task_dialog = None;
        self.undo_dialog = None;
        self.compare = None;
        self.worktree_dialog = Some(dialog);
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        self.load_worktree_dialog(window, cx);
    }

    /// "Compare" on a group card whose children are all in Review (SPEC §20E.2): the modal
    /// with one column per child, each reading its changes since its checkpoint.
    pub(crate) fn open_compare(
        &mut self,
        original_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(compare) = self.board.task(original_id).and_then(|original| {
            Compare::new(original, self.board.tasks(), self.sidebar.sessions())
        }) else {
            return;
        };
        let requests = compare.status_requests();
        self.workspace_menu = None;
        self.task_dialog = None;
        self.undo_dialog = None;
        self.worktree_dialog = None;
        self.compare = Some(compare);
        self.dialog_serial += 1;
        let serial = self.dialog_serial;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
        for (task_id, request) in requests {
            let status = connection.call::<GitStatus>(request);
            cx.spawn_in(window, async move |this, cx| {
                let status = status.await.map_err(|error| format!("{error:#}"));
                let _ = this.update(cx, |shell, cx| {
                    if let Some(compare) = shell.compare.as_mut()
                        && shell.dialog_serial == serial
                    {
                        compare.counted(task_id, status);
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }

    pub(crate) fn close_compare(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.compare = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    pub(crate) fn compare_key_down(
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
            self.close_compare(window, cx);
        }
    }

    /// "Git view" of a Compare column: the child's changes since its checkpoint, on its
    /// worktree (as the card's "Changes").
    pub(crate) fn compare_git(
        &mut self,
        task_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.compare = None;
        self.view_task_changes(task_id, window, cx);
    }

    /// "Pick" of a Compare column: the Merge back dialog of that child, which then moves the
    /// original to Review and offers removing the others' worktrees.
    pub(crate) fn compare_pick(
        &mut self,
        task_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(pick) = self
            .compare
            .as_ref()
            .and_then(|compare| compare.pick(task_id))
        else {
            return;
        };
        let Some(dialog) = self
            .board
            .task(task_id)
            .and_then(|task| WorktreeDialog::for_pick(task, pick))
        else {
            return;
        };
        self.show_worktree_dialog(dialog, window, cx);
    }

    /// The Pick's offer: `task.remove_worktree` of every other child; each answer is listed.
    pub(crate) fn remove_other_worktrees(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connected_or_toast(cx) else {
            return;
        };
        let serial = self.dialog_serial;
        let sessions = self.sidebar.sessions();
        let Some(pick) = self
            .worktree_dialog
            .as_mut()
            .and_then(|dialog| dialog.pick.as_mut())
        else {
            return;
        };
        let requests = pick.start_removal(sessions);
        cx.notify();
        for (task_id, request) in requests {
            let sent = connection.call::<RemoveWorktreeResult>(request);
            cx.spawn_in(window, async move |this, cx| {
                let result = sent.await.map_err(|error| format!("{error:#}"));
                let _ = this.update(cx, |shell, cx| {
                    if let Err(error) = &result {
                        tracing::warn!(%task_id, "task.remove_worktree failed: {error}");
                    }
                    if let Some(pick) = shell
                        .open_worktree(serial)
                        .and_then(|dialog| dialog.pick.as_mut())
                    {
                        pick.removed(task_id, result);
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }

    /// (Re)reads the open worktree dialog's plan: the worktree's status (and, to merge, the
    /// checkout's status and both logs for the commit list).
    pub(crate) fn load_worktree_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(dialog) = self.worktree_dialog.as_mut() else {
            return;
        };
        let serial = self.dialog_serial;
        dialog.step = worktree_dialog::Step::Loading;
        cx.notify();
        let kind = dialog.kind;
        let worktree = connection.call::<GitStatus>(dialog.status_request(true));
        let merge = (kind == worktree_dialog::Kind::Merge).then(|| {
            (
                connection.call::<GitStatus>(dialog.status_request(false)),
                connection.call::<Vec<Commit>>(dialog.log_request(true)),
                connection.call::<Vec<Commit>>(dialog.log_request(false)),
            )
        });
        cx.spawn_in(window, async move |this, cx| {
            let read = async {
                let worktree = worktree.await?;
                let Some((checkout, worktree_log, checkout_log)) = merge else {
                    return anyhow::Ok(worktree_dialog::Step::Remove(
                        worktree_dialog::remove_plan(&worktree),
                    ));
                };
                let (checkout, worktree_log) = (checkout.await?, worktree_log.await?);
                Ok(worktree_dialog::Step::Merge(worktree_dialog::merge_plan(
                    worktree_log,
                    &checkout_log.await?,
                    &worktree,
                    &checkout,
                )))
            };
            let step = read
                .await
                .unwrap_or_else(|error| worktree_dialog::Step::Failed(format!("{error:#}")));
            let _ = this.update(cx, |shell, cx| {
                if let Some(dialog) = shell.open_worktree(serial) {
                    dialog.step = step;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The open worktree dialog, if it is still the one opened as `serial`.
    pub(crate) fn open_worktree(&mut self, serial: u64) -> Option<&mut WorktreeDialog> {
        self.worktree_dialog
            .as_mut()
            .filter(|_| self.dialog_serial == serial)
    }

    pub(crate) fn close_worktree_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.worktree_dialog = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    pub(crate) fn worktree_dialog_key_down(
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
            // Esc on the discard question goes back to the list, like "Keep files".
            match self.worktree_dialog.as_mut() {
                Some(dialog) if matches!(dialog.step, worktree_dialog::Step::ConfirmDiscard(_)) => {
                    dialog.keep_files();
                    window.focus(&self.dialog_focus, cx);
                    cx.notify();
                }
                _ => self.close_worktree_dialog(window, cx),
            }
        }
    }

    /// The confirmed step: `task.merge`, `task.remove_worktree`, or the discard before a
    /// removal (then the worktree is read again). A failure shows why and reads it again.
    pub(crate) fn confirm_worktree(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connected_or_toast(cx) else {
            return;
        };
        let serial = self.dialog_serial;
        let Some(dialog) = self.worktree_dialog.as_mut() else {
            return;
        };
        let Ok(request) = dialog.confirm_request(self.sidebar.sessions()) else {
            return;
        };
        let task_id = dialog.task_id;
        let merging = matches!(dialog.step, worktree_dialog::Step::Merge(_));
        dialog.busy = true;
        dialog.error = None;
        cx.notify();
        let sent = connection.call::<serde_json::Value>(request);
        cx.spawn_in(window, async move |this, cx| {
            let answer = sent.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let Some(dialog) = shell.open_worktree(serial) else {
                    // Closed meanwhile: a merge or removal that failed, or a merge stopped by
                    // conflicts, has no dialog to say so.
                    let unshown = match answer {
                        Err(error) => Some(format!("The worktree action failed: {error:#}")),
                        Ok(value) if merging => worktree_dialog::unshown_merge(value),
                        Ok(_) => None,
                    };
                    if let Some(message) = unshown {
                        shell.toast_error(message, cx);
                    }
                    return;
                };
                let failure = answer.and_then(|value| {
                    match &dialog.step {
                        worktree_dialog::Step::Merge(_) => {
                            dialog.merged(serde_json::from_value(value)?);
                        }
                        worktree_dialog::Step::Remove(_) => {
                            dialog.removed(serde_json::from_value(value)?);
                        }
                        // Discarded: the worktree is listed again.
                        _ => dialog.busy = false,
                    }
                    anyhow::Ok(())
                });
                let reload = match failure {
                    Ok(()) => !matches!(dialog.step, worktree_dialog::Step::Done(_)),
                    Err(error) => {
                        tracing::warn!(%task_id, "worktree action failed: {error:#}");
                        dialog.failed(&format!("{error:#}"));
                        true
                    }
                };
                // A merged Pick: the fan-out original moves to Review (SPEC §20E.2).
                if let Some(original) = dialog.picked_original().filter(|&id| {
                    shell
                        .board
                        .task(id)
                        .is_some_and(|task| task.status != TaskStatus::Review)
                }) {
                    let moves = shell.board.drop_moves(original, TaskStatus::Review, None);
                    shell.move_cards(original, moves, "Cannot move the task", window, cx);
                }
                if reload {
                    shell.load_worktree_dialog(window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Open in Git" of the worktree dialog: the Git tab and Diff view on the task's
    /// worktree, to commit or unstage there.
    pub(crate) fn worktree_dialog_git(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(dialog) = self
            .worktree_dialog
            .take_if(|dialog| self.workspace == Some(dialog.project_id))
        else {
            return;
        };
        let project_id = dialog.project_id;
        self.retarget_git(project_id, Some(dialog.task_id), None);
        self.open_git(project_id, window, cx);
    }

    /// "Merge back…" / "Remove worktree…" (SPEC §20E.1): the commits to merge or the
    /// uncommitted files blocking the removal, the confirm of the current step, then the result.
    pub(crate) fn render_worktree_dialog(
        &self,
        dialog: &WorktreeDialog,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        use worktree_dialog::{Done, Kind, Step};
        let label = |text: String| {
            div()
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
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
        // One row per file or commit: a muted mono marker, then the text (ellipsized).
        let row = |marker: Option<String>, text: String| {
            div()
                .flex()
                .gap(px(space::S))
                .px(px(space::S))
                .text_size(font_size::META)
                .children(marker.map(|marker| {
                    div()
                        .flex_none()
                        .font_family(theme.terminal_font_family.clone())
                        .text_color(rgb(theme.text_muted))
                        .child(marker)
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_color(rgb(theme.text))
                        .child(text),
                )
        };
        // No border: the raised surface sets the list apart inside the dialog (§14).
        let list = |title: String, rows: Vec<gpui::Div>| {
            div()
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .child(label(title))
                .child(
                    div()
                        .id("worktree-list")
                        .max_h(px(theme::UNDO_FILES_HEIGHT))
                        .flex()
                        .flex_col()
                        .py(px(space::XS))
                        .rounded(px(radius::MEDIUM))
                        .bg(rgb(theme.surface_raised))
                        .overflow_scroll()
                        .children(rows),
                )
        };
        let paths = |paths: &[String]| -> Vec<gpui::Div> {
            paths.iter().map(|path| row(None, path.clone())).collect()
        };
        let body = match &dialog.step {
            Step::Loading => Some(label("Loading…".into())),
            Step::Failed(error) => Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(error_row(format!("Cannot read the worktree: {error}")).flex_1())
                    .child(
                        button("worktree-retry", "Retry", theme, false)
                            .focus_stop(theme)
                            .tooltip(tooltip(theme, "Read the worktree again", ""))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.load_worktree_dialog(window, cx);
                            })),
                    ),
            ),
            Step::Merge(plan) => Some(list(
                format!("Commits ({})", plan.commits.len()),
                plan.commits
                    .iter()
                    .map(|commit| {
                        let short: String = commit.hash.chars().take(7).collect();
                        row(Some(short), commit.summary.clone())
                    })
                    .collect(),
            )),
            Step::Remove(plan) if plan.uncommitted.is_empty() => None,
            Step::Remove(plan) => Some(list(
                format!("Uncommitted ({})", plan.uncommitted.len()),
                paths(&plan.uncommitted),
            )),
            Step::ConfirmDiscard(plan) => Some(list(
                format!("Discarded ({})", plan.discardable.len()),
                paths(&plan.discardable),
            )),
            Step::Done(Done::Conflicts(conflicts)) => Some(list(
                format!("Conflicting files ({})", conflicts.len()),
                paths(conflicts),
            )),
            // A merged Pick: the original's move, then how removing the others went.
            Step::Done(Done::Merged { .. }) => dialog.pick.as_ref().map(|pick| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .children(pick.lines().into_iter().map(|line| div().child(line)))
            }),
            Step::Done(_) => None,
        };
        let confirm = dialog.confirm_request(self.sidebar.sessions());
        // Why the confirm is off, when the user can act on it.
        let hint = confirm
            .as_ref()
            .err()
            .filter(|why| !why.is_empty() && !dialog.busy)
            // A removal blocked by files already says so in the intro.
            .filter(|_| match &dialog.step {
                Step::Merge(_) => true,
                Step::Remove(plan) => plan.uncommitted.is_empty(),
                _ => false,
            })
            .cloned();
        // Commits and unstaging happen in the worktree's Git view.
        let open_git = match &dialog.step {
            Step::Merge(plan) => !plan.worktree_dirty.is_empty(),
            Step::Remove(plan) => !plan.uncommitted.is_empty(),
            _ => false,
        };
        let git_button = open_git.then(|| {
            button("worktree-git", "Open in Git", theme, false)
                .focus_stop(theme)
                .tooltip(tooltip(theme, "The Git tab on this task's worktree", ""))
                .on_click(cx.listener(|shell, _, window, cx| {
                    shell.worktree_dialog_git(window, cx);
                }))
        });
        let cancel = |id: &'static str, text: &'static str| {
            button(id, text, theme, false)
                .focus_stop(theme)
                .tooltip(tooltip(theme, "Close", "Esc"))
                .on_click(cx.listener(|shell, _, window, cx| {
                    shell.close_worktree_dialog(window, cx);
                }))
        };
        let confirm_listener = cx.listener(|shell, _, window, cx| {
            shell.confirm_worktree(window, cx);
        });
        let mut buttons: Vec<gpui::AnyElement> = Vec::new();
        buttons.extend(git_button.map(IntoElement::into_any_element));
        match &dialog.step {
            Step::Done(done) => {
                let offer = dialog
                    .pick
                    .as_ref()
                    .filter(|_| matches!(done, Done::Merged { .. }))
                    .and_then(|pick| pick.offer());
                if let Some(offer) = offer {
                    buttons.push(
                        danger_button("worktree-remove-others", offer, theme)
                            .focus_stop(theme)
                            .tooltip(tooltip(
                                theme,
                                "Delete the other agents' worktree folders (unmerged branches are kept)",
                                "",
                            ))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.remove_other_worktrees(window, cx);
                            }))
                            .into_any_element(),
                    );
                }
                buttons.push(cancel("worktree-close", "Close").into_any_element());
            }
            Step::ConfirmDiscard(_) => {
                buttons.push(
                    button("worktree-keep", "Keep files", theme, false)
                        .focus_stop(theme)
                        .tooltip(tooltip(
                            theme,
                            "Back to the list; nothing is discarded",
                            "Esc",
                        ))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            if let Some(dialog) = shell.worktree_dialog.as_mut() {
                                dialog.keep_files();
                            }
                            window.focus(&shell.dialog_focus, cx);
                            cx.notify();
                        }))
                        .into_any_element(),
                );
                buttons.push(if confirm.is_ok() {
                    danger_button("worktree-confirm", "Discard files", theme)
                        .focus_stop(theme)
                        .tooltip(tooltip(theme, "Discard every file listed above", ""))
                        .on_click(confirm_listener)
                        .into_any_element()
                } else {
                    disabled_button("worktree-confirm", "Discarding…", theme, false)
                        .bg(rgb(theme.diff_del_bg))
                        .text_color(rgb(theme.diff_del_text))
                        .into_any_element()
                });
            }
            Step::Remove(plan) if !plan.uncommitted.is_empty() => {
                buttons.push(cancel("worktree-cancel", "Cancel").into_any_element());
                if !plan.discardable.is_empty() {
                    buttons.push(
                        danger_button("worktree-discard", "Discard…", theme)
                            .focus_stop(theme)
                            .tooltip(tooltip(theme, "Choose to discard these changes", ""))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                if let Some(dialog) = shell.worktree_dialog.as_mut() {
                                    dialog.ask_discard();
                                }
                                window.focus(&shell.dialog_focus, cx);
                                cx.notify();
                            }))
                            .into_any_element(),
                    );
                }
            }
            step => {
                buttons.push(cancel("worktree-cancel", "Cancel").into_any_element());
                let remove = dialog.kind == Kind::Remove;
                let text = match (remove, dialog.busy) {
                    (false, false) => "Merge",
                    (false, true) => "Merging…",
                    (true, false) => "Remove worktree",
                    (true, true) => "Removing…",
                };
                let ready = confirm.is_ok() && !matches!(step, Step::Loading | Step::Failed(_));
                buttons.push(match (ready, remove) {
                    (true, true) => danger_button("worktree-confirm", text, theme)
                        .focus_stop(theme)
                        .tooltip(tooltip(theme, "Delete the worktree folder", ""))
                        .on_click(confirm_listener)
                        .into_any_element(),
                    (true, false) => button("worktree-confirm", text, theme, true)
                        .focus_stop(theme)
                        .tooltip(tooltip(
                            theme,
                            "Merge the commits above into your checkout",
                            "",
                        ))
                        .on_click(confirm_listener)
                        .into_any_element(),
                    // A disabled removal keeps the destructive tint, like Undo's.
                    (false, true) => disabled_button("worktree-confirm", text, theme, false)
                        .bg(rgb(theme.diff_del_bg))
                        .text_color(rgb(theme.diff_del_text))
                        .into_any_element(),
                    (false, false) => {
                        disabled_button("worktree-confirm", text, theme, true).into_any_element()
                    }
                });
            }
        }
        let panel = div()
            .id("worktree-dialog")
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
                cx.listener(Self::worktree_dialog_key_down),
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
                    .child(dialog.intro()),
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
                    .children(buttons),
            );
        Self::modal_shade("worktree-dialog-shade", panel, cx)
    }

    /// "Compare" of a group card (SPEC §20E.2): one column per child with its changes since
    /// its checkpoint, run time, its Git view and "Pick".
    pub(crate) fn render_compare(
        &self,
        compare: &Compare,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let meta = |text: String| {
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let columns = compare.columns.iter().map(|column| {
            let task_id = column.task_id;
            let git = if column.checkpoint.is_some() && column.has_worktree {
                quiet_button("compare-git", "Git view", theme, theme.border)
                    .focus_stop(theme)
                    .tooltip(tooltip(
                        theme,
                        "Its changes in the Git view, on its worktree",
                        "",
                    ))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.compare_git(task_id, window, cx);
                    }))
            } else {
                let why = if column.has_worktree {
                    "No run to compare"
                } else {
                    "Its worktree was removed"
                };
                disabled_button("compare-git", "Git view", theme, false)
                    .tooltip(tooltip(theme, why, ""))
            };
            // Quiet: up to five columns would otherwise show five accent buttons.
            let pick = if column.has_worktree {
                quiet_button("compare-pick", "Pick", theme, theme.border)
                    .focus_stop(theme)
                    .tooltip(tooltip(
                        theme,
                        "Merge this result back into your checkout",
                        "",
                    ))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.compare_pick(task_id, window, cx);
                    }))
            } else {
                disabled_button("compare-pick", "Pick", theme, false).tooltip(tooltip(
                    theme,
                    "Its worktree was removed: nothing to merge",
                    "",
                ))
            };
            let changes_color = match column.changes {
                fanout::Changes::Counted { .. } => theme.text,
                _ => theme.text_muted,
            };
            let failed = matches!(column.changes, fanout::Changes::Failed(_));
            div()
                .id(SharedString::from(format!("compare-{task_id}")))
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .p(px(space::M))
                .rounded(px(radius::MEDIUM))
                .bg(rgb(theme.surface_raised))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_size(font_size::BODY)
                        .font_weight(FontWeight(weight::MEDIUM))
                        .text_color(rgb(theme.text))
                        .child(column.agent.clone()),
                )
                .children(column.branch.clone().map(meta))
                .child(meta(match column.run_time(now) {
                    Some(time) => format!("Ran {time}"),
                    None => "Did not run".into(),
                }))
                .child(
                    div()
                        .flex()
                        .gap(px(space::XS))
                        .text_size(font_size::META)
                        .text_color(rgb(changes_color))
                        .when(failed, |el| {
                            el.child(div().text_color(rgb(theme.status_exited)).child("✕"))
                        })
                        .child(column.changes.text()),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .justify_end()
                        .gap(px(space::S))
                        .pt(px(space::S))
                        .child(git)
                        .child(pick),
                )
        });
        let panel = div()
            .id("compare-dialog")
            .w(px(theme::COMPARE_DIALOG_WIDTH))
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
                cx.listener(Self::compare_key_down),
            )
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Compare agents"),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child(format!(
                        "“{}”: changes since each run started. Pick one to merge it back into \
                         your checkout.",
                        compare.title
                    )),
            )
            .child(div().flex().gap(px(space::M)).children(columns))
            .child(
                div().flex().justify_end().pt(px(space::S)).child(
                    button("compare-close", "Close", theme, false)
                        .focus_stop(theme)
                        .tooltip(tooltip(theme, "Close", "Esc"))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_compare(window, cx);
                        })),
                ),
            );
        Self::modal_shade("compare-dialog-shade", panel, cx)
    }
}
