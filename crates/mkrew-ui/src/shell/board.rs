//! Board view actions: card drops, approve, run / run with / fan-out, follow-up.

use crate::*;

impl Shell {
    /// A card was dropped into the `status` column right before `before` (`None` = at the
    /// end): `task.move` it (and renumber the column if its gap collapsed). Each reply is
    /// shown at once, without waiting for the `task.updated` echo.
    pub(crate) fn drop_card(
        &mut self,
        task_id: Ulid,
        status: TaskStatus,
        before: Option<Ulid>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let moves = self.board.drop_moves(task_id, status, before);
        self.move_cards(task_id, moves, "Cannot move the card", window, cx);
    }

    /// Approve on a Review card: it moves to the end of Done.
    pub(crate) fn approve_task(
        &mut self,
        task_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let moves = self.board.approve_moves(task_id);
        self.move_cards(task_id, moves, "Cannot approve the task", window, cx);
    }

    /// "Changes" on a Review card: the Git view with the task's changes since its checkpoint.
    pub(crate) fn view_task_changes(
        &mut self,
        task_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(task) = self.board.task(task_id) else {
            return;
        };
        let Some(checkpoint) = task.checkpoint.clone() else {
            return;
        };
        let project_id = task.project_id;
        // The board only shows the active workspace's cards.
        if self.workspace != Some(project_id) {
            return;
        }
        let since = Since::task(task_id, &task.title, checkpoint);
        // A worktree task's checkpoint is its worktree's HEAD: compared there.
        let target = task.worktree_path.is_some().then_some(task_id);
        self.retarget_git(project_id, target, None);
        self.show_git_since(project_id, since, window, cx);
    }

    /// The Git view listing the changes since `since`'s checkpoint.
    pub(crate) fn show_git_since(
        &mut self,
        project_id: Ulid,
        since: Since,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let requests = self.git_view(project_id).show_since(since);
        self.git_files_scroll.scroll_to_item(0, ScrollStrategy::Top);
        // Showing the Git view refreshes it (the same requests) unless it is shown already.
        if self.view() == View::Git {
            self.git_requests(project_id, requests, window, cx);
        }
        self.show_view(View::Git, window, cx);
    }

    /// Sends the `task.move`s of one action on `task_id`'s card; a failure shows `what` and
    /// the error on its board.
    pub(crate) fn move_cards(
        &mut self,
        task_id: Ulid,
        moves: Vec<TaskMoveParams>,
        what: &'static str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(project_id) = self.board.task(task_id).map(|task| task.project_id) else {
            return;
        };
        let moves: Vec<_> = moves
            .into_iter()
            .map(|params| connection.call::<Task>(Request::TaskMove(params)))
            .collect();
        if moves.is_empty() {
            return;
        }
        // Cleared once per action: a later move of the same batch must not hide a failure.
        self.card_error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            for moved in moves {
                let moved = moved.await;
                let _ = this.update(cx, |shell, cx| {
                    match moved {
                        Ok(task) => {
                            shell.board.apply(&Event::TaskUpdated(task));
                        }
                        Err(error) => {
                            tracing::warn!(%task_id, "task.move failed: {error:#}");
                            shell.card_error = Some((project_id, format!("{what}: {error:#}")));
                        }
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Run on a Todo card (or Ctrl+Enter on the selected one): `task.run`. The card follows
    /// its session through `task.updated`; clicking it opens the terminal.
    pub(crate) fn run_task(
        &mut self,
        task_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(project_id) = self
            .board
            .task(task_id)
            .filter(|_| self.board.can_run(task_id) && !self.starting.contains(&task_id))
            .map(|task| task.project_id)
        else {
            return;
        };
        self.starting.insert(task_id);
        self.card_error = None;
        cx.notify();
        let ran =
            connection.call::<SessionSpawnResult>(Request::TaskRun(lifecycle::run_params(task_id)));
        cx.spawn_in(window, async move |this, cx| {
            let ran = ran.await;
            let _ = this.update(cx, |shell, cx| {
                shell.starting.remove(&task_id);
                if let Err(error) = ran {
                    tracing::warn!(%task_id, "task.run failed: {error:#}");
                    shell.card_error =
                        Some((project_id, format!("Cannot run the task: {error:#}")));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Run with…" on a card: opens its profile picker (closing any other) and lists the agent
    /// profiles for it.
    pub(crate) fn open_run_with(
        &mut self,
        task_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.selected_card = Some(task_id);
        self.run_with = Some(RunWith::new(task_id));
        self.fan_out = None;
        cx.notify();
        let listed = connection.call::<AgentProfilesResult>(Request::AgentProfiles);
        cx.spawn_in(window, async move |this, cx| {
            let agents = match listed.await {
                Ok(listed) => Agents::Loaded(listed.profiles),
                Err(error) => Agents::Failed(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                // Unless the picker closed (or moved to another card) meanwhile.
                if let Some(menu) = shell
                    .run_with
                    .as_mut()
                    .filter(|menu| menu.task_id == task_id)
                {
                    menu.agents = agents;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// A profile picked in "Run with…" (SPEC §20B.6): `task.update` of the task's agent, then
    /// `task.run`; the run waits for the update, and a failure stops the sequence and shows on
    /// the board.
    pub(crate) fn run_task_with(
        &mut self,
        task_id: Ulid,
        profile: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.run_with = None;
        cx.notify();
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(task) = self
            .board
            .task(task_id)
            .filter(|_| {
                // A run may have started since the picker opened.
                self.board.can_run_with(task_id, self.sidebar.sessions())
                    && !self.starting.contains(&task_id)
            })
            .cloned()
        else {
            return;
        };
        let project_id = task.project_id;
        let requests = lifecycle::run_with_requests(&task, &profile);
        self.starting.insert(task_id);
        self.card_error = None;
        cx.spawn_in(window, async move |this, cx| {
            let mut failed = None;
            for request in requests {
                let result = match request {
                    Request::TaskUpdate(_) => match connection.call::<Task>(request).await {
                        Ok(updated) => {
                            // Shown at once, without waiting for the `task.updated` echo.
                            let _ = this.update(cx, |shell, cx| {
                                shell.board.apply(&Event::TaskUpdated(updated));
                                cx.notify();
                            });
                            Ok(())
                        }
                        Err(error) => Err(format!("Cannot change the task's agent: {error:#}")),
                    },
                    request => connection
                        .call::<SessionSpawnResult>(request)
                        .await
                        .map(|_| ())
                        .map_err(|error| format!("Cannot run the task: {error:#}")),
                };
                if let Err(error) = result {
                    failed = Some(error);
                    break;
                }
            }
            let _ = this.update(cx, |shell, cx| {
                shell.starting.remove(&task_id);
                if let Some(error) = failed {
                    tracing::warn!(%task_id, "run with {profile} failed: {error}");
                    shell.card_error = Some((project_id, error));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Run with several agents…" on a card: opens its multi-agent picker (closing any other)
    /// and lists the agent profiles for it.
    pub(crate) fn open_fan_out(
        &mut self,
        task_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.selected_card = Some(task_id);
        self.fan_out = Some(FanOutPicker::new(task_id));
        self.run_with = None;
        cx.notify();
        let listed = connection.call::<AgentProfilesResult>(Request::AgentProfiles);
        cx.spawn_in(window, async move |this, cx| {
            let agents = match listed.await {
                Ok(listed) => Agents::Loaded(listed.profiles),
                Err(error) => Agents::Failed(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                // Unless the picker closed (or moved to another card) meanwhile.
                if let Some(menu) = shell
                    .fan_out
                    .as_mut()
                    .filter(|menu| menu.task_id == task_id)
                {
                    menu.agents = agents;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The picker's run button (SPEC §20E.2): `task.fanout` with the ticked agents; the daemon
    /// creates and runs one child task each, and moves the card to Running. A failure shows on
    /// the board.
    pub(crate) fn run_fan_out(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(params) = self.fan_out.as_ref().and_then(FanOutPicker::request) else {
            return;
        };
        self.fan_out = None;
        cx.notify();
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let task_id = params.task_id;
        let Some(project_id) = self
            .board
            .task(task_id)
            .filter(|_| {
                // A run may have started since the picker opened.
                self.board.can_fan_out(task_id, self.sidebar.sessions())
                    && !self.starting.contains(&task_id)
            })
            .map(|task| task.project_id)
        else {
            return;
        };
        self.starting.insert(task_id);
        self.card_error = None;
        let call = connection.call::<Vec<Task>>(Request::TaskFanout(params));
        cx.spawn_in(window, async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |shell, cx| {
                shell.starting.remove(&task_id);
                match result {
                    // Shown at once, without waiting for the `task.updated` echoes.
                    Ok(children) => {
                        for child in children {
                            shell.board.apply(&Event::TaskUpdated(child));
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%task_id, "task.fanout failed: {error:#}");
                        shell.card_error = Some((
                            project_id,
                            format!("Cannot run with several agents: {error:#}"),
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A click on a card selects it and shows its session's terminal (none for Todo cards).
    pub(crate) fn card_clicked(
        &mut self,
        task_id: Ulid,
        session: Option<Ulid>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.selected_card = Some(task_id);
        // Cards take no focus: leave another card's follow-up field.
        window.focus(&self.focus_handle, cx);
        cx.notify();
        if let Some(session) = session {
            self.open_session(session, window, cx);
        }
    }

    /// Gives the follow-up field of `task_id`'s card the keyboard (keeping its draft).
    pub(crate) fn open_follow_up(
        &mut self,
        task_id: Ulid,
        session_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.selected_card = Some(task_id);
        if self
            .follow_up
            .as_ref()
            .is_none_or(|draft| draft.task_id != task_id || draft.session_id != session_id)
        {
            self.follow_up = Some(FollowUp::new(task_id, session_id));
        }
        window.focus(&self.follow_up_focus, cx);
        cx.notify();
    }

    /// Whether the follow-up draft's card still shows its field.
    pub(crate) fn follow_up_shown(&self) -> bool {
        self.follow_up.as_ref().is_some_and(|draft| {
            self.board
                .takes_follow_up(draft.task_id, draft.session_id, self.sidebar.sessions())
        })
    }

    /// Keys typed in the follow-up field: Enter is a new line, Ctrl+Enter sends, Esc gives the
    /// keyboard back (the blur keeps a typed draft on its card and drops an empty one).
    pub(crate) fn follow_up_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = task_dialog::dialog_key(&event.keystroke) else {
            return;
        };
        let paste = (key == DialogKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(draft) = self.follow_up.as_mut() else {
            return;
        };
        if let Some(text) = paste {
            draft.insert(&text);
        }
        copy_selection(&key, draft.text.input(), cx);
        match draft.key(key) {
            follow_up::Outcome::Edited => {
                // ponytail: follows the caret only at the end; a caret moved above the
                // visible lines is not scrolled into view (needs line-height maths).
                let input = draft.text.input();
                if input.cursor() == input.len() {
                    self.follow_up_scroll.scroll_to_bottom();
                }
                cx.notify();
            }
            follow_up::Outcome::Leave => {
                self.focus_pane(window, cx);
                cx.notify();
            }
            follow_up::Outcome::Send => self.send_follow_up(window, cx),
        }
    }

    /// `session.send` of the draft (text + Enter); the field empties once it is written.
    pub(crate) fn send_follow_up(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(draft) = self.follow_up.as_mut() else {
            return;
        };
        let Some(params) = draft.send_params() else {
            return;
        };
        let (task_id, session_id) = (draft.task_id, draft.session_id);
        draft.busy = true;
        draft.error = None;
        cx.notify();
        let sent = connection.request(Request::SessionSend(params));
        cx.spawn_in(window, async move |this, cx| {
            let failure = match sent.await {
                Ok(Outcome::Result(_)) => None,
                Ok(Outcome::Error(body)) => Some(body.message),
                Err(error) => Some(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                // The field was closed or moved to another session meanwhile.
                let Some(draft) = shell
                    .follow_up
                    .as_mut()
                    .filter(|d| (d.task_id, d.session_id) == (task_id, session_id))
                else {
                    return;
                };
                draft.busy = false;
                match failure {
                    None => draft.text.clear(),
                    Some(error) => {
                        tracing::warn!(%session_id, "session.send failed: {error}");
                        draft.error = Some(format!("Cannot send: {error}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The workspace's task board: one column per status (SPEC §10); cards drag between and
    /// within columns.
    pub(crate) fn render_board(
        &self,
        project_id: Ulid,
        now: i64,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let project = self.sidebar.projects().iter().find(|p| p.id == project_id);
        let empty = |sentence: &str, detail: Option<String>| {
            message(
                theme,
                None,
                sentence.to_owned().into(),
                detail.map(Into::into),
            )
            .flex_1()
            .into_any_element()
        };
        let header = self.render_board_header(project_id, theme, window, cx);
        let source = self
            .jira
            .get(&project_id)
            .map_or(Source::Tasks, |view| view.source);
        let framed = |header: Option<gpui::Div>, child: gpui::AnyElement| {
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .font_family(theme.ui_font_family.clone())
                .children(header)
                .child(child)
                .into_any_element()
        };
        if self.jira_on && source == Source::Jira {
            let list = self.render_jira(project_id, now, theme, window, cx);
            return framed(header, list);
        }
        if !self.board.is_loaded() {
            let shown = match &self.board_error {
                Some(error) => empty("Cannot load the tasks.", Some(error.clone())),
                None => empty("Loading tasks…", None),
            };
            return framed(header, shown);
        }
        if !self.board.has_tasks(project_id) {
            let shown = message(theme, None, "No tasks yet.".into(), None)
                .flex_1()
                .child(self.new_task_button(project_id, true, theme, cx))
                .into_any_element();
            return framed(header, shown);
        }
        let default_agent = project.and_then(|p| p.default_agent.as_deref());
        let columns = self
            .board
            .columns(project_id, default_agent, self.sidebar.sessions());
        let cards = CardContext {
            selected: self.selected_card,
            starting: &self.starting,
            follow_up: self.follow_up.as_ref(),
            follow_up_focus: &self.follow_up_focus,
            follow_up_scroll: &self.follow_up_scroll,
            ime_marked: &self.ime_marked,
            typing_target: &self.typing_target,
            follow_up_focused: self.follow_up_focus.is_focused(window),
            run_keys: self.keymap.label(Command::RunTask),
            conflicts: &self.conflicts,
            run_with: self.run_with.as_ref(),
            fan_out: self.fan_out.as_ref(),
            usages: &self.usages,
            sessions: self.sidebar.sessions(),
            agent_icons: &self.agent_icons,
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .font_family(theme.ui_font_family.clone())
            .children(header)
            // A later `task.list` failed: the cards may be stale.
            .children(self.board_error.clone().map(|error| {
                notice(theme.text, format!("✕ Cannot reload the tasks: {error}"))
                    .px(px(space::L))
                    .pt(px(space::S))
            }))
            .children(self.card_error.as_ref().and_then(|(project, error)| {
                (*project == project_id).then(|| {
                    notice(theme.text, format!("✕ {error}"))
                        .px(px(space::L))
                        .pt(px(space::S))
                })
            }))
            .child(
                div()
                    .id("board")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap(px(space::M))
                    .px(px(space::L))
                    .pb(px(space::L))
                    .pt(px(space::S))
                    .overflow_x_scroll()
                    .children(columns.into_iter().map(|column| {
                        let new_task = (column.status == TaskStatus::Todo).then(|| {
                            self.new_task_button(project_id, false, theme, cx)
                                .into_any_element()
                        });
                        board_column(column, &cards, now, new_task, theme, cx)
                    })),
            )
            .into_any_element()
    }
}
