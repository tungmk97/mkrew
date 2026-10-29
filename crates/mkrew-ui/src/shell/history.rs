//! Follow mode, the turns dialog and session history.

use crate::*;

impl Shell {
    /// "Follow in Zed" of an agent pane (SPEC §20C.5): turns following its edits on or off.
    /// The wait is read from `settings.toml` when it turns on.
    pub(crate) fn toggle_follow(&mut self, session_id: Ulid, cx: &mut Context<'_, Self>) {
        if self.local_only(cx) {
            return;
        }
        // A missing or broken settings file waits the default (the daemon reports it).
        let debounce_ms = mkrew_core::settings::load(&self.settings_file)
            .unwrap_or_default()
            .zed
            .follow_debounce_ms;
        self.follow.toggle(session_id, debounce_ms);
        cx.notify();
    }

    /// Opens a followed session's edit in Zed once `wait` passed without a later edit; a
    /// failure is a toast (following stays on).
    pub(crate) fn follow_wait(
        &mut self,
        wait: follow::Wait,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(wait.delay).await;
            let _ = this.update(cx, |shell, cx| {
                let Some(edit) = shell.follow.due(wait) else {
                    return;
                };
                let session_id = wait.session_id;
                let Some(full) = shell
                    .sidebar
                    .sessions()
                    .iter()
                    .find(|s| s.id == session_id)
                    .and_then(|s| shell.workspace_file(s.project_id, &edit.path))
                else {
                    return;
                };
                // The PATH lookup and process spawn stay off the UI thread: this fires on
                // every followed edit, not on a click. A failure is a toast; the same one
                // restarts instead of stacking, so a missing Zed is one card, not one per edit.
                let (path, line) = (edit.path, edit.line);
                cx.spawn(async move |this, cx| {
                    let opened = cx
                        .background_executor()
                        .spawn(async move { zed::open(&full, line).map_err(|e| e.to_string()) })
                        .await;
                    if let Err(error) = opened {
                        tracing::warn!(%session_id, path, "follow in Zed failed: {error}");
                        let _ = this.update(cx, |shell, cx| {
                            shell.toast_error(format!("Cannot follow in Zed: {error}"), cx);
                        });
                    }
                })
                .detach();
            });
        })
        .detach();
    }

    /// "Turns…" of an agent pane (SPEC §20C.2): lists the session's turns (`turn.list`).
    pub(crate) fn open_turns(
        &mut self,
        session_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(session) = self
            .sidebar
            .sessions()
            .iter()
            .find(|s| s.id == session_id && s.kind == SessionKind::Agent)
        else {
            return;
        };
        self.workspace_menu = None;
        self.task_dialog = None;
        self.undo_dialog = None;
        self.turns_dialog = Some(TurnsDialog::new(session));
        self.dialog_serial += 1;
        let serial = self.dialog_serial;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
        let listed = connection.call::<Vec<Turn>>(Request::TurnList(SessionRef { session_id }));
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await;
            let _ = this.update(cx, |shell, cx| {
                let Some(dialog) = shell
                    .turns_dialog
                    .as_mut()
                    .filter(|_| shell.dialog_serial == serial)
                else {
                    return;
                };
                match listed {
                    Ok(turns) => dialog.loaded(turns),
                    Err(error) => {
                        dialog.listing = turns_dialog::Listing::Failed(format!("{error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn close_turns_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.turns_dialog = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    pub(crate) fn turns_dialog_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        match turns_dialog::key(&event.keystroke) {
            Some(turns_dialog::Key::Close) => self.close_turns_dialog(window, cx),
            Some(turns_dialog::Key::Move(offset)) => {
                if let Some(dialog) = self.turns_dialog.as_mut() {
                    dialog.select_by(offset);
                    cx.notify();
                }
            }
            Some(turns_dialog::Key::Restore) => {
                let Some(dialog) = &self.turns_dialog else {
                    return;
                };
                let state = self.session_state(dialog.session_id);
                let restorable = match &dialog.listing {
                    turns_dialog::Listing::Loaded(turns) => turns
                        .get(dialog.selected)
                        .is_some_and(|turn| turns_dialog::restore_blocked(turn, state).is_none()),
                    _ => false,
                };
                if let Some(n) = dialog.selected_turn().filter(|_| restorable) {
                    self.restore_before_turn(n, window, cx);
                }
            }
            Some(turns_dialog::Key::DiffSince) => {
                let Some(dialog) = &self.turns_dialog else {
                    return;
                };
                let diffable = match &dialog.listing {
                    turns_dialog::Listing::Loaded(turns) => turns
                        .get(dialog.selected)
                        .is_some_and(|turn| turns_dialog::diff_blocked(turn).is_none()),
                    _ => false,
                };
                if let Some(n) = dialog.selected_turn().filter(|_| diffable) {
                    self.diff_since_turn(n, window, cx);
                }
            }
            None => {}
        }
    }

    /// `session_id`'s state (exited once it is no longer listed).
    pub(crate) fn session_state(&self, session_id: Ulid) -> SessionState {
        self.sidebar
            .sessions()
            .iter()
            .find(|s| s.id == session_id)
            .map_or(SessionState::Exited, |s| s.state)
    }

    /// "Restore before turn n": replaces the Turns dialog with the undo dialog's preview and
    /// confirm (`turn.restore_preview` / `turn.restore`).
    pub(crate) fn restore_before_turn(
        &mut self,
        n: u32,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(dialog) = self.turns_dialog.take() else {
            return;
        };
        let Some(session) = self
            .sidebar
            .sessions()
            .iter()
            .find(|s| s.id == dialog.session_id)
        else {
            self.close_turns_dialog(window, cx);
            return;
        };
        self.undo_dialog = Some(UndoDialog::for_turn(session, n));
        self.turns_return = Some(dialog);
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        self.load_undo_preview(window, cx);
    }

    /// "Diff since" on a Turns dialog row (SPEC §20C.3): closes the dialog and shows the Git
    /// view since the turn's start tree.
    pub(crate) fn diff_since_turn(
        &mut self,
        n: u32,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(dialog) = self.turns_dialog.take() else {
            return;
        };
        cx.notify();
        let tree = match &dialog.listing {
            turns_dialog::Listing::Loaded(turns) => turns
                .iter()
                .find(|turn| turn.n == n)
                .and_then(|turn| turn.start_tree.clone()),
            _ => None,
        };
        let sessions = self.sidebar.sessions();
        // The Git view shows the active workspace's changes.
        let session = sessions
            .iter()
            .find(|s| s.id == dialog.session_id && Some(s.project_id) == self.workspace);
        let since = tree.zip(session).map(|(tree, session)| {
            let since = Since::turn(session, sessions, n, tree);
            (session.project_id, session.task_id, since)
        });
        match since {
            Some((project_id, task_id, since)) => {
                // A worktree task's turns ran in its worktree: compared there.
                let target = task_id.filter(|&id| {
                    self.board
                        .task(id)
                        .is_some_and(|task| task.worktree_path.is_some())
                });
                self.retarget_git(project_id, target, None);
                self.show_git_since(project_id, since, window, cx);
            }
            None => self.focus_pane(window, cx),
        }
    }

    /// "History" in the tab bar: lists the workspace's finished sessions (`session.list`) and
    /// replays the newest one's log.
    pub(crate) fn show_history(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(project) = self.sidebar.projects().iter().find(|p| p.id == project_id) else {
            return;
        };
        self.workspace_menu = None;
        self.task_dialog = None;
        self.undo_dialog = None;
        self.history = Some(HistoryDialog::new(project_id, project.name.clone()));
        self.history_pane = None;
        self.dialog_serial += 1;
        let serial = self.dialog_serial;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
        let listed = connection.call::<Vec<Session>>(Request::SessionList(SessionListParams {
            project_id: Some(project_id),
        }));
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let Some(dialog) = shell.open_history(serial) else {
                    return;
                };
                match listed {
                    Ok(sessions) => {
                        if let Some(session) = dialog.load(sessions) {
                            shell.load_history_log(session, window, cx);
                        }
                    }
                    Err(error) => dialog.listing = Listing::Failed(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The open history, if it is still the one opened as `serial`.
    pub(crate) fn open_history(&mut self, serial: u64) -> Option<&mut HistoryDialog> {
        self.history
            .as_mut()
            .filter(|_| self.dialog_serial == serial)
    }

    /// Selects `session` in the open history and replays the tail of its log
    /// (`session.history`, files read off the UI thread).
    pub(crate) fn select_history(
        &mut self,
        session: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self
            .history
            .as_mut()
            .is_some_and(|dialog| dialog.select(session))
        {
            self.load_history_log(session, window, cx);
        }
    }

    /// Reads the selected `session`'s log into a new replay pane.
    pub(crate) fn load_history_log(
        &mut self,
        session: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some((cols, rows)) = (self.history.as_ref())
            .and_then(|dialog| dialog.selected_session())
            .filter(|selected| selected.id == session)
            .map(|selected| (selected.cols.max(1), selected.rows.max(1)))
        else {
            return;
        };
        let serial = self.dialog_serial;
        self.history_pane = None;
        // The dialog keeps the keyboard while no pane shows.
        window.focus(&self.dialog_focus, cx);
        cx.notify();
        let tail =
            connection.call::<SessionLogTail>(Request::SessionLogTail(SessionLogTailParams {
                session_id: session,
                max_bytes: history::TAIL_BYTES,
            }));
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            // Replay the log into the emulator on the background executor: the advance over up
            // to 1 MiB of output is what used to freeze the window.
            let replayed = match tail.await {
                Ok(tail) => Ok(background
                    .spawn(async move {
                        let emulator = (!tail.data.is_empty()).then(|| {
                            let mut emulator = mkrew_ui::terminal::emulator::Emulator::for_replay(
                                cols.into(),
                                rows.into(),
                            );
                            emulator.advance(&tail.data);
                            emulator
                        });
                        (tail.cut, emulator)
                    })
                    .await),
                Err(error) => Err(format!("Cannot read the log: {error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let Some(dialog) = shell.open_history(serial) else {
                    return;
                };
                let Some(selected) = dialog
                    .selected_session()
                    .filter(|selected| selected.id == session)
                    .cloned()
                else {
                    // Another session was picked meanwhile.
                    return;
                };
                match replayed {
                    Ok((_, None)) => dialog.log = Log::Empty,
                    Ok((cut, Some(emulator))) => {
                        dialog.log = Log::Shown { cut };
                        let painted = shell.painted.clone();
                        let pane = cx.new(|cx| {
                            TerminalPane::replay(
                                &selected, emulator, connection, painted, window, cx,
                            )
                        });
                        shell.history_pane = Some(pane);
                        shell.focus_pane(window, cx);
                    }
                    Err(error) => {
                        tracing::warn!(%session, "session history failed: {error}");
                        dialog.log = Log::Failed(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn close_history(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.history = None;
        self.history_pane = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    /// "Resume" in History (ADR 0055): relaunches the selected agent session into its
    /// conversation, then closes the dialog and gives the new pane the keyboard.
    pub(crate) fn resume_history(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connected_or_toast(cx) else {
            return;
        };
        let Some(dialog) = self.history.as_mut() else {
            return;
        };
        let Some(session) = dialog.selected_session() else {
            return;
        };
        if dialog.resuming || history::resume_blocked(session).is_some() {
            return;
        }
        let (project_id, session_id) = (dialog.project_id, session.id);
        dialog.resuming = true;
        dialog.resume_error = None;
        let tab_id = (self.tabs.get(&project_id))
            .and_then(|tabs| tabs.active())
            .map(|tab| tab.id);
        let serial = self.dialog_serial;
        cx.notify();
        let resumed =
            connection.call::<SessionSpawnResult>(Request::SessionResume(SessionResumeParams {
                session_id,
                tab_id,
            }));
        cx.spawn_in(window, async move |this, cx| {
            let resumed = resumed.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let Some(dialog) = shell.open_history(serial) else {
                    return;
                };
                dialog.resuming = false;
                match resumed {
                    Ok(resumed) => {
                        shell.close_history(window, cx);
                        shell.views.insert(project_id, View::Terminals);
                        // Next to the focused pane of the active tab, unless the daemon put
                        // it in an empty tab or in the old pane's place; `place_session`
                        // focuses it, and the pane takes the keyboard once attached.
                        if shell.tabs.contains_key(&project_id) {
                            let id = resumed.session_id;
                            shell.place_session(project_id, id, Axis::Vertical, window, cx);
                        } else {
                            shell.reveal = Some(resumed.session_id);
                        }
                        shell.activate_workspace(project_id, window, cx);
                    }
                    Err(error) => {
                        tracing::warn!(%session_id, "session.resume failed: {error:#}");
                        dialog.resume_error = Some(format!("Cannot resume: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Esc closes the history, ↑ / ↓ pick another session (also while its pane has the
    /// keyboard: a finished session's pane passes keys on).
    pub(crate) fn history_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        match history::key(&event.keystroke) {
            Some(history::Key::Close) => self.close_history(window, cx),
            Some(history::Key::Move(offset)) => {
                let next = self
                    .history
                    .as_ref()
                    .and_then(|dialog| dialog.neighbour(offset));
                if let Some(session) = next {
                    self.select_history(session, window, cx);
                }
            }
            Some(history::Key::Resume) => self.resume_history(window, cx),
            None => {}
        }
    }

    /// "Turns": the session's turns, newest first, each with "Restore before" (disabled, with
    /// the reason as its tooltip, while the agent works or without a snapshot).
    pub(crate) fn render_turns_dialog(
        &self,
        dialog: &TurnsDialog,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let muted = |text: String| {
            div()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let state = self.session_state(dialog.session_id);
        let hover = theme.surface_raised;
        let list: gpui::AnyElement = match &dialog.listing {
            turns_dialog::Listing::Loading => muted("Loading the turns…".into()).into_any_element(),
            turns_dialog::Listing::Failed(error) => div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                .child(
                    div()
                        .text_color(rgb(theme.text))
                        .child(format!("Cannot list the turns: {error}")),
                )
                .into_any_element(),
            turns_dialog::Listing::Loaded(turns) if turns.is_empty() => {
                muted("No turns yet: they are recorded as the agent works.".into())
                    .into_any_element()
            }
            turns_dialog::Listing::Loaded(turns) => div()
                .id("turns-list")
                .max_h(px(theme::TURNS_LIST_HEIGHT))
                .flex()
                .flex_col()
                .overflow_y_scroll()
                .children(
                    turns_dialog::rows(turns, state, now)
                        .into_iter()
                        .enumerate()
                        .map(|(index, row)| {
                            let n = row.n;
                            let selected = index == dialog.selected;
                            let details = [Some(row.when), Some(row.files), row.lines]
                                .into_iter()
                                .flatten()
                                .collect::<Vec<_>>()
                                .join(" · ");
                            let what = format!("Restore the files to before turn {n} (preview)");
                            let restore = match row.blocked {
                                None => button("turn-restore", "Restore before", theme, false)
                                    .tooltip(tooltip(
                                        theme,
                                        &what,
                                        if selected { "Enter" } else { "" },
                                    ))
                                    .on_click(cx.listener(move |shell, _, window, cx| {
                                        shell.restore_before_turn(n, window, cx);
                                    })),
                                Some(reason) => {
                                    disabled_button("turn-restore", "Restore before", theme, false)
                                        .tooltip(tooltip(theme, reason, ""))
                                }
                            };
                            let diff = match row.diff_blocked {
                                None => button("turn-diff", "Diff since", theme, false)
                                    .tooltip(tooltip(
                                        theme,
                                        &format!("Show the changes since turn {n} started"),
                                        if selected { "D" } else { "" },
                                    ))
                                    .on_click(cx.listener(move |shell, _, window, cx| {
                                        shell.diff_since_turn(n, window, cx);
                                    })),
                                Some(reason) => {
                                    disabled_button("turn-diff", "Diff since", theme, false)
                                        .tooltip(tooltip(theme, reason, ""))
                                }
                            };
                            div()
                                .id(SharedString::from(format!("turn-{n}")))
                                .flex()
                                .items_center()
                                .gap(px(space::S))
                                .px(px(space::S))
                                .py(px(space::XS))
                                .rounded(px(radius::SMALL))
                                .when(selected, |el| el.bg(rgb(theme.surface_raised)))
                                .when(!selected, |el| el.hover(move |style| style.bg(rgb(hover))))
                                .on_click(cx.listener(move |shell, _, _, cx| {
                                    if let Some(dialog) = shell.turns_dialog.as_mut() {
                                        dialog.selected = index;
                                        cx.notify();
                                    }
                                }))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .child(
                                            div()
                                                .text_size(font_size::BODY)
                                                .font_weight(FontWeight(weight::MEDIUM))
                                                .text_color(rgb(theme.text))
                                                .child(row.title),
                                        )
                                        .child(
                                            div()
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .whitespace_nowrap()
                                                .text_size(font_size::META)
                                                .text_color(rgb(theme.text_muted))
                                                .child(details),
                                        ),
                                )
                                .child(diff.flex_none())
                                .child(restore.flex_none())
                        }),
                )
                .into_any_element(),
        };
        let panel = div()
            .id("turns-dialog")
            .w(px(theme::TURNS_DIALOG_WIDTH))
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
            .on_key_down(cx.listener(Self::turns_dialog_key_down))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(format!("Turns of {}", dialog.agent)),
            )
            .child(list)
            .child(
                div().flex().justify_end().pt(px(space::S)).child(
                    button("turns-close", "Close", theme, true)
                        .tooltip(tooltip(theme, "Close", "Esc"))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_turns_dialog(window, cx);
                        })),
                ),
            );
        Self::modal_shade("turns-dialog-shade", panel, cx)
    }

    /// The workspace's finished sessions (left) and the selected one's log, replayed
    /// read-only (right).
    pub(crate) fn render_history(
        &self,
        dialog: &HistoryDialog,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let muted = |text: String| {
            div()
                .text_size(font_size::BODY)
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
        let hover = theme.surface_raised;
        let list: Vec<gpui::AnyElement> = match &dialog.listing {
            Listing::Loading => vec![muted("Loading the sessions…".into()).into_any_element()],
            Listing::Failed(error) => {
                vec![error_row(format!("Cannot list the sessions: {error}")).into_any_element()]
            }
            Listing::Loaded(sessions) if sessions.is_empty() => {
                vec![muted("No finished sessions in this workspace yet.".into()).into_any_element()]
            }
            Listing::Loaded(sessions) => sessions
                .iter()
                .map(|session| {
                    let id = session.id;
                    let selected = dialog.selected == Some(id);
                    div()
                        .id(SharedString::from(format!("history-{id}")))
                        .flex()
                        .flex_col()
                        .px(px(space::S))
                        .py(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .cursor_pointer()
                        .when(selected, |el| el.bg(rgb(theme.surface_raised)))
                        .when(!selected, |el| {
                            el.hover(move |style| style.bg(rgb(hover)))
                                .active(|style| style.opacity(opacity::ACTIVE))
                        })
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.select_history(id, window, cx);
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(space::XS))
                                .text_size(font_size::BODY)
                                .child(
                                    div()
                                        .flex_none()
                                        .text_color(rgb(if history::failed(session) {
                                            sidebar::status_color(theme, SessionState::Exited)
                                        } else {
                                            theme.text_muted
                                        }))
                                        .child(status::icon(SessionState::Exited)),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .text_color(rgb(theme.text))
                                        .when(selected, |el| {
                                            el.font_weight(FontWeight(weight::MEDIUM))
                                        })
                                        .child(history::label(session)),
                                ),
                        )
                        .child(
                            div()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child(history::details(session, now)),
                        )
                        .into_any_element()
                })
                .collect(),
        };
        let log: Option<gpui::AnyElement> = match (&dialog.log, dialog.selected_session()) {
            (_, None) => None,
            (Log::Loading, Some(_)) => Some(muted("Loading the log…".into()).into_any_element()),
            (Log::Empty, Some(_)) => Some(muted("No output was logged.".into()).into_any_element()),
            (Log::Failed(error), Some(_)) => Some(error_row(error.clone()).into_any_element()),
            (Log::Shown { cut }, Some(session)) => Some(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap(px(space::S))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .rounded(px(radius::MEDIUM))
                            .overflow_hidden()
                            .children(self.history_pane.clone()),
                    )
                    .when(*cut, |el| {
                        el.child(
                            div()
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child(format!(
                                    "Older output is left out; mk session history {} prints \
                                     all of it.",
                                    session.id
                                )),
                        )
                    })
                    .into_any_element(),
            ),
        };
        let resume = dialog.selected_session().map(|session| {
            let label = icon_text(
                Icon::Play,
                if dialog.resuming {
                    "Resuming…"
                } else {
                    "Resume"
                },
                button_fg(theme, true),
            );
            let blocked = history::resume_blocked(session)
                .or(dialog.resuming.then_some("Starting the agent…"));
            match blocked {
                None => button("history-resume", label, theme, true)
                    .tooltip(tooltip(
                        theme,
                        "Continue this conversation in a new pane",
                        "Enter",
                    ))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.resume_history(window, cx);
                    })),
                Some(reason) => disabled_button("history-resume", label, theme, true)
                    .tooltip(tooltip(theme, reason, "")),
            }
        });
        let panel = div()
            .id("history-dialog")
            .w(relative(theme::HISTORY_DIALOG_FRACTION))
            .h(relative(theme::HISTORY_DIALOG_FRACTION))
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
            .on_key_down(cx.listener(Self::history_key_down))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_size(font_size::CARD_TITLE)
                            .font_weight(FontWeight(weight::SEMIBOLD))
                            .text_color(rgb(theme.text))
                            .child(format!("History · {}", dialog.workspace)),
                    )
                    .children(resume)
                    .child(
                        button("history-close", "Close", theme, false)
                            .tooltip(tooltip(theme, "Close", "Esc"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.close_history(window, cx);
                            })),
                    ),
            )
            .children(dialog.resume_error.clone().map(error_row))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap(px(space::M))
                    .child(
                        div()
                            .id("history-list")
                            .flex_none()
                            .w(px(theme::HISTORY_LIST_WIDTH))
                            .flex()
                            .flex_col()
                            .gap(px(space::XS))
                            .overflow_y_scroll()
                            .children(list),
                    )
                    .child(div().flex_1().min_w_0().flex().flex_col().children(log)),
            );
        Self::modal_shade("history-shade", panel, cx)
    }
}
