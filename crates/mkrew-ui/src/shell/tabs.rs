//! Tabs and panes: switch, split, rename, pin, color, close, kill.

use crate::*;

impl Shell {
    pub(crate) fn cycle_workspace(
        &mut self,
        offset: isize,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(project_id) = self.sidebar.workspace_at_offset(self.workspace, offset) {
            self.activate_workspace(project_id, window, cx);
        }
    }

    /// Changes the active tab of the shown workspace.
    pub(crate) fn switch_tab(
        &mut self,
        change: impl FnOnce(&mut Tabs),
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(tabs) = self.active_tabs_mut() {
            change(tabs);
            self.sync_panes(window, cx);
            cx.notify();
        }
    }

    /// A new tab in the shown workspace with an agent or a shell in it.
    pub(crate) fn new_tab(
        &mut self,
        kind: SessionKind,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(project_id) = self.workspace
            && self.tabs.contains_key(&project_id)
        {
            self.create_tab(project_id, Some(kind), window, cx);
        }
    }

    /// Splits the focused pane with a new session of the same kind.
    pub(crate) fn split(&mut self, axis: Axis, window: &mut Window, cx: &mut Context<'_, Self>) {
        let focused = self
            .focused_session()
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));
        let kind = lifecycle::split_kind(focused);
        self.new_pane(kind, axis, window, cx);
    }

    /// Lays the active tab out as `preset` (SPEC §10): its panes stay, the missing ones start
    /// sessions of the focused pane's kind (as Split right) one after another, then the
    /// layout is written once. If a start fails, or the tab changed meanwhile, the sessions
    /// that did start are placed like Split right.
    pub(crate) fn apply_layout(
        &mut self,
        preset: Preset,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(project_id) = self.workspace else {
            return;
        };
        let Some((tab_id, have)) = (self.tabs.get(&project_id))
            .and_then(|tabs| tabs.active())
            .map(|tab| (tab.id, tab.sessions().len()))
        else {
            return;
        };
        if have > preset.panes() {
            self.tab_error = Some(format!(
                "This tab has {have} panes; the {} layout holds {}. Close panes first.",
                preset.label(),
                preset.panes()
            ));
            cx.notify();
            return;
        }
        let missing = preset.panes() - have;
        if missing == 0 {
            let write =
                (self.tabs.get_mut(&project_id)).and_then(|tabs| tabs.arrange(tab_id, preset, &[]));
            if let Some(write) = write {
                self.save_layout(project_id, write, window, cx);
                self.sync_panes(window, cx);
            }
            cx.notify();
            return;
        }
        let focused = (self.focused_session())
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));
        let kind = lifecycle::split_kind(focused);
        if self.starting_pane.is_some() || self.ask_for_default(project_id, kind, window, cx) {
            return;
        }
        self.starting_pane = Some(kind);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let mut started = Vec::new();
            let mut failed = None;
            for _ in 0..missing {
                let params = lifecycle::spawn_params(project_id, Some(tab_id), kind);
                match connection
                    .call::<SessionSpawnResult>(Request::SessionSpawn(params))
                    .await
                {
                    Ok(spawned) => started.push(spawned.session_id),
                    Err(error) => {
                        failed = Some(format!("{error:#}"));
                        break;
                    }
                }
            }
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.starting_pane = None;
                let write = (shell.tabs.get_mut(&project_id))
                    .and_then(|tabs| tabs.arrange(tab_id, preset, &started));
                match write {
                    Some(write) => shell.save_layout(project_id, write, window, cx),
                    None => {
                        // `place_session` focuses a session a tab shows already (the one the
                        // daemon placed in an empty tab, or one a snapshot brought in), so
                        // none is placed twice.
                        for session in started {
                            shell.place_session(project_id, session, Axis::Vertical, window, cx);
                        }
                    }
                }
                if let Some(error) = failed {
                    tracing::warn!(%project_id, "session.spawn failed: {error}");
                    let what = lifecycle::kind_noun(kind);
                    shell.tab_error = Some(format!("Cannot start {what}: {error}"));
                }
                shell.sync_panes(window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Starts editing the active tab's title inline, starting from the current one.
    pub(crate) fn start_rename(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(tab) = self
            .workspace
            .and_then(|id| self.tabs.get(&id))
            .and_then(|tabs| tabs.active())
        else {
            return;
        };
        self.renaming = Some(Renaming {
            target: RenameTarget::Tab(tab.id),
            draft: tab.title.clone().into(),
        });
        window.focus(&self.rename_focus, cx);
        cx.notify();
    }

    /// Opens the menu of `tab_id` at `position`: the tab becomes the active one first, because
    /// Split and Rename act on the active tab.
    pub(crate) fn open_tab_menu(
        &mut self,
        tab_id: Ulid,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        self.pane_menu = None;
        self.layout_menu = None;
        if let Some(tabs) = self.active_tabs_mut() {
            tabs.activate(tab_id);
        }
        self.tab_menu = Some(TabMenu { tab_id, position });
        self.sync_panes(window, cx);
        cx.notify();
    }

    /// Opens the split button's menu for the active tab at `position` (SPEC §10).
    pub(crate) fn open_layout_menu(&mut self, position: Point<Pixels>, cx: &mut Context<'_, Self>) {
        let Some(tab_id) = (self.workspace)
            .and_then(|id| self.tabs.get(&id))
            .and_then(|tabs| tabs.active())
            .map(|tab| tab.id)
        else {
            return;
        };
        self.close_menus();
        self.layout_menu = Some(LayoutMenu { tab_id, position });
        cx.notify();
    }

    /// Starts editing `session_id`'s title inline in its pane header (F2, double-click),
    /// from the user's own title. Run sessions are named by their command: left alone.
    pub(crate) fn start_pane_rename(
        &mut self,
        session_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(session) = self.sidebar.sessions().iter().find(|s| s.id == session_id) else {
            return;
        };
        if mkrew_core::run::command_name(session).is_some() {
            return;
        }
        self.renaming = Some(Renaming {
            target: RenameTarget::Session(session_id),
            draft: session.title.clone().unwrap_or_default().into(),
        });
        window.focus(&self.rename_focus, cx);
        cx.notify();
    }

    /// Ends the inline edit: `commit` sends `tab.rename` / `session.rename` (or creates the
    /// Files tree's new entry); the reply shows at once, the `tab.updated` / `session.updated`
    /// event tells other clients. Unchanged drafts and blank tab titles are dropped (a blank
    /// pane title clears it). Focus goes back to the pane (the tree for a new entry).
    pub(crate) fn finish_rename(
        &mut self,
        commit: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(Renaming { target, draft }) = self.renaming.take() else {
            return;
        };
        let files = matches!(target, RenameTarget::NewEntry(_));
        if commit {
            match target {
                RenameTarget::Tab(tab_id) => self.rename_tab(tab_id, &draft, window, cx),
                RenameTarget::Session(session_id) => {
                    self.rename_session(session_id, &draft, window, cx);
                }
                RenameTarget::NewEntry(entry) => self.create_entry(entry, &draft, window, cx),
            }
        }
        if self.rename_focus.is_focused(window) {
            if files {
                window.focus(&self.files_focus, cx);
            } else {
                self.focus_pane(window, cx);
            }
        }
        cx.notify();
    }

    pub(crate) fn rename_tab(
        &mut self,
        tab_id: Ulid,
        draft: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        // Any workspace: switching workspaces mid-edit commits through the blur.
        let params = self
            .tabs
            .values()
            .find_map(|tabs| tabs.tab(tab_id))
            .and_then(|tab| rename_params(tab, draft));
        if let Some(params) = params {
            // Shown at once, without waiting for the `tab.updated` echo.
            let shown = |shell: &mut Self, tab: TerminalTab| {
                if let Some(tabs) = shell.tabs.get_mut(&tab.project_id) {
                    tabs.apply(&Event::TabUpdated(tab));
                }
            };
            self.send_edit(
                Request::TabRename(params),
                "Cannot rename the tab",
                shown,
                window,
                cx,
            );
        }
    }

    pub(crate) fn rename_session(
        &mut self,
        session_id: Ulid,
        draft: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let session = (self.sidebar.sessions().iter()).find(|s| s.id == session_id);
        // The pane may have exited while its rename field was open.
        if session.is_some_and(|s| s.state == SessionState::Exited) {
            return;
        }
        let params = session.and_then(|session| lifecycle::session_rename_params(session, draft));
        if let Some(params) = params {
            // Shown at once (header, sidebar, Inbox), without waiting for the echo.
            let shown = |shell: &mut Self, session: Session| {
                // Title only: the reply may predate state events already applied.
                shell.sidebar.set_title(session.id, session.title);
            };
            self.send_edit(
                Request::SessionRename(params),
                "Cannot rename the pane",
                shown,
                window,
                cx,
            );
        }
    }

    /// "Pin" / "Unpin" of a session (sidebar row or pane menu): `session.pin`. The
    /// `session.updated` event that follows moves the row.
    pub(crate) fn pin_session(
        &mut self,
        session_id: Ulid,
        pinned: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.pane_menu = None;
        let params = SessionPinParams { session_id, pinned };
        self.send_edit(
            Request::SessionPin(params),
            "Cannot pin the pane",
            |_: &mut Self, _: serde_json::Value| {},
            window,
            cx,
        );
    }

    /// "Pin" / "Unpin" of a tab (tab menu): `tab.pin`. The `tab.updated` event that follows
    /// moves the tab.
    pub(crate) fn pin_tab(
        &mut self,
        tab_id: Ulid,
        pinned: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.tab_menu = None;
        let params = TabPinParams { tab_id, pinned };
        self.send_edit(
            Request::TabPin(params),
            "Cannot pin the tab",
            |_: &mut Self, _: serde_json::Value| {},
            window,
            cx,
        );
    }

    /// Sets (or clears) the color of `session_id`'s pane from its menu.
    pub(crate) fn set_pane_color(
        &mut self,
        session_id: Ulid,
        color: Option<&'static str>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.pane_menu = None;
        let params = (self.sidebar.sessions().iter())
            .find(|s| s.id == session_id)
            .and_then(|session| lifecycle::session_color_params(session, color));
        if let Some(params) = params {
            // Shown at once (header, chip), without waiting for the echo.
            let shown = |shell: &mut Self, session: Session| {
                shell.sidebar.set_color(session.id, session.color);
            };
            self.send_edit(
                Request::SessionSetColor(params),
                "Cannot change the pane color",
                shown,
                window,
                cx,
            );
        }
        cx.notify();
    }

    /// Opens the menu of `session_id`'s pane at `position`, focusing the pane.
    pub(crate) fn open_pane_menu(
        &mut self,
        session_id: Ulid,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        self.tab_menu = None;
        if let Some(tabs) = self.active_tabs_mut() {
            tabs.focus(session_id);
        }
        // Keyboard focus follows the highlighted pane (typed keys, F2, Ctrl+W).
        self.focus_pane(window, cx);
        self.pane_menu = Some(PaneMenu {
            session_id,
            position,
        });
        cx.notify();
    }

    /// Sends a `tab.rename` / `session.rename` / `session.set_color` / `session.pin` / `tab.pin`; `shown` applies the
    /// reply, a failure goes to the tab bar's error line after `failure`.
    pub(crate) fn send_edit<T: serde::de::DeserializeOwned + Send + 'static>(
        &mut self,
        request: Request,
        failure: &'static str,
        shown: impl FnOnce(&mut Self, T) + 'static,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.tab_error = None;
        let edited = connection.call::<T>(request);
        cx.spawn_in(window, async move |this, cx| {
            let edited = edited.await;
            let _ = this.update(cx, |shell, cx| {
                match edited {
                    Ok(edited) => shown(shell, edited),
                    Err(error) => {
                        tracing::warn!("{failure}: {error:#}");
                        shell.tab_error = Some(format!("{failure}: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Keys typed while renaming edit the draft; Enter commits, Esc cancels.
    pub(crate) fn rename_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = find_key(&event.keystroke) else {
            return;
        };
        let Some(renaming) = self.renaming.as_mut() else {
            return;
        };
        match key {
            FindKey::Close => self.finish_rename(false, window, cx),
            FindKey::Older | FindKey::Newer => self.finish_rename(true, window, cx),
            FindKey::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    renaming.draft.insert(&text.replace(['\r', '\n'], " "));
                    cx.notify();
                }
            }
            FindKey::Insert(text) => {
                renaming.draft.insert(&text);
                cx.notify();
            }
            // Backspace, Ctrl+Backspace, Ctrl+A.
            key => {
                key.edit(&mut renaming.draft);
                cx.notify();
            }
        }
    }

    /// Keys typed in the sidebar's filter box edit it; Esc clears it and gives the keyboard
    /// back to the pane, Enter just gives it back.
    pub(crate) fn filter_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = find_key(&event.keystroke) else {
            return;
        };
        match key {
            FindKey::Close => {
                self.sidebar.filter_mut().clear();
                self.focus_pane(window, cx);
            }
            FindKey::Older | FindKey::Newer => self.focus_pane(window, cx),
            FindKey::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.sidebar
                        .filter_mut()
                        .insert(&text.replace(['\r', '\n'], " "));
                }
            }
            FindKey::Insert(text) => {
                self.sidebar.filter_mut().insert(&text);
            }
            // Backspace, Ctrl+Backspace, Ctrl+A.
            key => {
                key.edit(self.sidebar.filter_mut());
            }
        }
        cx.notify();
    }

    /// `tab.close` for `tab_id` of the shown workspace (the active one, or any tab's ✕). The
    /// daemon removes the sessions of its panes too (ADR 0041), so a running agent among them
    /// asks first; closing the last tab leaves a fresh empty one.
    pub(crate) fn close_tab(
        &mut self,
        tab_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(project_id) = self.workspace else {
            return;
        };
        let sessions = self
            .tabs
            .get(&project_id)
            .and_then(|tabs| tabs.tab(tab_id))
            .map(Tab::sessions)
            .unwrap_or_default();
        let running_agent = sessions.iter().any(|id| {
            let known = self.sidebar.sessions().iter().find(|s| s.id == *id);
            lifecycle::close_action(known) == CloseAction::Ask
        });
        let answer = running_agent.then(|| {
            window.prompt(
                PromptLevel::Critical,
                "This tab has a running agent.",
                Some("Closing the tab stops its sessions and removes them from the sidebar."),
                &workspace_menu::REMOVE_ANSWERS,
                cx,
            )
        });
        self.tab_error = None;
        cx.spawn_in(window, async move |this, cx| {
            if let Some(answer) = answer
                && !task_dialog::confirms_delete(answer.await.ok())
            {
                return;
            }
            let closed = connection
                .call::<()>(Request::TabClose(TabRef { tab_id }))
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                match closed {
                    Ok(()) => {
                        for session in &sessions {
                            shell.sidebar.remove_session(*session);
                        }
                        let Some(tabs) = shell.tabs.get_mut(&project_id) else {
                            return;
                        };
                        tabs.remove(tab_id);
                        if tabs.is_empty() {
                            shell.create_tab(project_id, None, window, cx);
                        }
                        shell.sync_panes(window, cx);
                    }
                    Err(error) => {
                        tracing::warn!(%tab_id, "tab.close failed: {error:#}");
                        shell.tab_error = Some(format!("Cannot close the tab: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn close_focused_pane(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if let Some(session) = self.focused_session() {
            self.close_pane(session, window, cx);
        }
    }

    /// Closes the pane of `session`: an exited session's pane just goes, a shell is killed,
    /// a running agent asks whether to kill it or only hide the pane (SPEC §10).
    pub(crate) fn close_pane(
        &mut self,
        session: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let known = self.sidebar.sessions().iter().find(|s| s.id == session);
        match lifecycle::close_action(known) {
            CloseAction::Remove => self.remove_pane(session, window, cx),
            CloseAction::Kill => {
                self.remove_pane(session, window, cx);
                self.kill(session, window, cx);
            }
            CloseAction::Ask => {
                let answer = window.prompt(
                    PromptLevel::Critical,
                    "This session is still running.",
                    Some("Kill it, or hide the pane and keep it running in the sidebar."),
                    &CloseChoice::ANSWERS,
                    cx,
                );
                cx.spawn_in(window, async move |this, cx| {
                    let choice = CloseChoice::from_answer(answer.await.ok());
                    let _ = this.update_in(cx, |shell, window, cx| match choice {
                        CloseChoice::Kill => {
                            shell.remove_pane(session, window, cx);
                            // It may have exited while the dialog was open.
                            if !shell.is_exited(session) {
                                shell.kill(session, window, cx);
                            }
                        }
                        CloseChoice::Hide => shell.remove_pane(session, window, cx),
                        CloseChoice::Cancel => {}
                    });
                })
                .detach();
            }
        }
    }

    /// Takes the pane of `session` out of its tab's layout (the session itself is untouched).
    pub(crate) fn remove_pane(
        &mut self,
        session: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let closed = self
            .tabs
            .values_mut()
            .find_map(|tabs| Some((tabs.project_id(), tabs.close_pane(session)?)));
        if let Some((project_id, write)) = closed {
            self.save_layout(project_id, write, window, cx);
        }
        self.sync_panes(window, cx);
        cx.notify();
    }

    /// `session.kill`; a failure is shown in the tab bar.
    pub(crate) fn kill(
        &mut self,
        session_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let killed = connection.call::<()>(Request::SessionKill(SessionKillParams {
            session_id,
            force: false,
        }));
        cx.spawn_in(window, async move |this, cx| {
            if let Err(error) = killed.await {
                tracing::warn!(%session_id, "session.kill failed: {error:#}");
                let _ = this.update(cx, |shell, cx| {
                    shell.tab_error = Some(format!("Cannot kill the session: {error:#}"));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// ✕ on a sidebar session: a running one asks first (removing stops it); then
    /// `session.remove` hides it for good and its pane closes. Its log stays on disk.
    pub(crate) fn remove_session(
        &mut self,
        session_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let answer = (!self.is_exited(session_id)).then(|| {
            window.prompt(
                PromptLevel::Critical,
                "This session is still running.",
                Some("Removing it from the sidebar stops it."),
                &workspace_menu::REMOVE_ANSWERS,
                cx,
            )
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Some(answer) = answer
                && !task_dialog::confirms_delete(answer.await.ok())
            {
                return;
            }
            let removed = connection
                .call::<()>(Request::SessionRemove(SessionRef { session_id }))
                .await;
            let _ = this.update_in(cx, |shell, window, cx| match removed {
                Ok(()) => {
                    shell.sidebar.remove_session(session_id);
                    shell.remove_pane(session_id, window, cx);
                }
                Err(error) => {
                    tracing::warn!(%session_id, "session.remove failed: {error:#}");
                    shell.sidebar_error = Some(format!("Cannot remove the session: {error:#}"));
                    cx.notify();
                }
            });
        })
        .detach();
    }
}
