//! Workspaces and tabs: activate, load, place sessions, save layout, views.

use crate::*;

impl Shell {
    /// The listed workspace of the most recently started live session, else the first one.
    /// Sessions of an archived workspace may still be live (stopping), so they do not count.
    pub(crate) fn initial_workspace(&self) -> Option<Ulid> {
        let projects = self.sidebar.projects();
        self.sidebar
            .sessions()
            .iter()
            .filter(|session| session.state != SessionState::Exited)
            .filter(|session| projects.iter().any(|p| p.id == session.project_id))
            .max_by_key(|session| session.started_at)
            .map(|session| session.project_id)
            .or_else(|| projects.first().map(|project| project.id))
    }

    /// Space window `host` shows `project` (a new session, e.g. spawned by `mk`, fills an empty
    /// tab first), or the empty state for `None`.
    pub(crate) fn show_in_space(
        &mut self,
        host: HostId,
        project: Option<Ulid>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match project {
            Some(project_id) => {
                self.fill_empty_tab(project_id, window, cx);
                self.activate_workspace(host, project_id, window, cx);
            }
            None => {
                self.hosts.entry(host).or_default().workspace = None;
                self.sync_panes(window, cx);
            }
        }
    }

    /// Shows the tabs of `project_id` in Space window `host`, loading them the first time.
    pub(crate) fn activate_workspace(
        &mut self,
        host: HostId,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let before = self.workspace_of(host);
        let switched = before != Some(project_id);
        self.hosts.entry(host).or_default().workspace = Some(project_id);
        self.workspace_switcher = false;
        if switched {
            if self.menu_host == host {
                self.close_menus();
            }
            self.end_inspector_drag(cx);
            // Nothing is kept for a workspace that no window shows any more.
            if before.is_some_and(|old| !self.on_screen_workspaces().contains(&old)) {
                self.git_summary = None;
            }
        }
        // First: the daemon answers one connection's requests in order, so an attach sent
        // after the git/files loads below waits for all of them (a git.status can take 1 s).
        self.sync_panes(window, cx);
        if switched || self.git_summary.is_none() {
            self.load_git_summary(project_id, window, cx);
        }
        if !self.tabs.contains_key(&project_id) {
            self.load_tabs(project_id, window, cx);
        }
        if switched && self.view(host) == View::Docs {
            self.load_docs(project_id, window, cx);
        }
        if switched && self.files_on_screen(host) {
            self.refresh_files(project_id, window, cx);
        }
        if switched && self.git_shown(host) {
            self.refresh_git(project_id, window, cx);
        }
        self.sync_watches(cx);
        cx.notify();
    }

    /// `tab.list` for a workspace; a workspace without tabs gets one.
    pub(crate) fn load_tabs(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if !self.loading_tabs.insert(project_id) {
            return;
        }
        let listed =
            connection.call::<Vec<TerminalTab>>(Request::TabList(TabListParams { project_id }));
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await;
            let _ = this.update_in(cx, |shell, window, cx| match listed {
                Ok(list) if list.is_empty() => shell.create_tab(project_id, None, window, cx),
                Ok(list) => {
                    let mut tabs = Tabs::new(project_id);
                    tabs.load(list);
                    shell.tabs.insert(project_id, tabs);
                    shell.tabs_loaded(project_id, window, cx);
                }
                Err(error) => {
                    tracing::warn!(%project_id, "tab.list failed: {error:#}");
                    shell.tabs_load_failed(project_id);
                    if shell.project_of_reveal() == Some(project_id) {
                        shell.reveal = None;
                    }
                    shell.list_failed(format!("{error:#}"));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Adds a tab to a workspace and activates it; `start` (a kind, and an agent profile or
    /// the workspace's default of that kind) then spawns a session into it.
    pub(crate) fn create_tab(
        &mut self,
        project_id: Ulid,
        start: Option<(SessionKind, Option<String>)>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some((kind, None)) = &start
            && self.ask_for_default(project_id, *kind, window, cx)
        {
            return;
        }
        // Right after the active tab (`None` while the workspace has no tab: the first one).
        let position = (self.tabs.get(&project_id)).and_then(Tabs::position_after_active);
        self.create_tab_then(
            project_id,
            position,
            move |shell, _tab_id, window, cx| {
                if let Some((kind, profile)) = start
                    && shell.workspace_in(window) == Some(project_id)
                {
                    shell.new_pane_with(kind, profile, Axis::Vertical, window, cx);
                }
            },
            window,
            cx,
        );
    }

    /// `tab.create` at `position` (`None`: last), the tab applied and activated, then `then`
    /// with its id. A failure toasts (or fails the workspace's first load) and `then` never
    /// runs.
    pub(crate) fn create_tab_then(
        &mut self,
        project_id: Ulid,
        position: Option<f64>,
        then: impl FnOnce(&mut Shell, Ulid, &mut Window, &mut Context<'_, Shell>) + 'static,
        window: &mut Window,
        cx: &mut Context<'_, Shell>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let count = self
            .tabs
            .get(&project_id)
            .map_or(0, |tabs| tabs.tabs().len());
        let title = match count {
            0 => FIRST_TAB_TITLE.to_owned(),
            count => format!("{FIRST_TAB_TITLE} {}", count + 1),
        };
        let created = connection.call::<TerminalTab>(Request::TabCreate(TabCreateParams {
            project_id,
            title,
            position,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let created = created.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let first = shell.loading_tabs.contains(&project_id);
                match created {
                    Ok(tab) => {
                        let tab_id = tab.id;
                        let tabs = shell
                            .tabs
                            .entry(project_id)
                            .or_insert_with(|| Tabs::new(project_id));
                        // Its `tab.updated` echo is then a no-op.
                        tabs.apply(&Event::TabUpdated(tab));
                        tabs.activate(tab_id);
                        if first {
                            shell.tabs_loaded(project_id, window, cx);
                        } else {
                            shell.sync_panes(window, cx);
                        }
                        then(shell, tab_id, window, cx);
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "tab.create failed: {error:#}");
                        let error = format!("{error:#}");
                        if first {
                            shell.tabs_load_failed(project_id);
                            shell.list_failed(error);
                        } else {
                            shell.toast_error(format!("Cannot add a tab: {error}"), cx);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The tabs of a workspace arrived: show the session waiting for them, else fill an empty
    /// first tab, and attach what is on screen.
    pub(crate) fn tabs_loaded(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.loading_tabs.remove(&project_id);
        let (early, rest) = std::mem::take(&mut self.early_tab_events)
            .into_iter()
            .partition(
                |event| matches!(event, Event::TabUpdated(tab) if tab.project_id == project_id),
            );
        self.early_tab_events = rest;
        if let Some(tabs) = self.tabs.get_mut(&project_id) {
            for event in early {
                tabs.apply(&event);
            }
        }
        if let Some(session) = self.reveal
            && self.project_of(session) == Some(project_id)
        {
            self.reveal = None;
            self.place_session(project_id, session, None, Axis::Vertical, window, cx);
        }
        self.fill_empty_tab(project_id, window, cx);
        self.sync_panes(window, cx);
        self.restore_progress(window, cx);
        cx.notify();
    }

    /// `project_id`'s tabs failed to load: it stops loading and its buffered `tab.updated`
    /// events go (other workspaces still loading keep theirs). A restore keeps waiting for it
    /// (the next listing loads it again): only an answer may decide which saved tabs are gone.
    pub(crate) fn tabs_load_failed(&mut self, project_id: Ulid) {
        self.loading_tabs.remove(&project_id);
        self.early_tab_events.retain(
            |event| !matches!(event, Event::TabUpdated(tab) if tab.project_id == project_id),
        );
    }

    pub(crate) fn project_of(&self, session: Ulid) -> Option<Ulid> {
        self.sidebar
            .sessions()
            .iter()
            .find(|s| s.id == session)
            .map(|s| s.project_id)
    }

    pub(crate) fn project_of_reveal(&self) -> Option<Ulid> {
        self.reveal.and_then(|session| self.project_of(session))
    }

    pub(crate) fn is_exited(&self, session: Ulid) -> bool {
        self.sidebar
            .sessions()
            .iter()
            .any(|s| s.id == session && s.state == SessionState::Exited)
    }

    /// Shows a session picked in the sidebar, the palette or a notification. A pane in a
    /// detached window comes forward there (its tab is selected, its window activated); from a
    /// detached window a session of the main one activates the main window (never a silent
    /// change of the window we are in). Otherwise it focuses its pane, or places it next to the
    /// focused pane of its workspace's active tab.
    pub(crate) fn open_session(
        &mut self,
        session: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self.project_of(session) else {
            return;
        };
        let held = self.tab_of_session(session);
        if let Some((project, tab_id)) = held
            && let Some(host @ HostId::Detached(key)) = self.terminal_host(project, tab_id)
        {
            let dock = DockTab::Terminal { project, tab_id };
            if let Some(at) = self.detached.tabs_of(key).iter().position(|t| *t == dock) {
                self.detached.select(key, at);
            }
            self.focus_session_pane(session);
            self.windows_dirty = true;
            self.in_window(host, cx, move |shell, window, cx| {
                window.activate_window();
                shell.sync_panes(window, cx);
                cx.notify();
            });
            cx.notify();
            return;
        }
        if self.host_of(window).space().is_none() {
            self.in_window(self.primary_host(), cx, move |shell, window, cx| {
                window.activate_window();
                shell.open_session(session, window, cx);
            });
            return;
        }
        self.views.insert(project_id, View::Terminals);
        if self.tabs.contains_key(&project_id) {
            self.place_session(project_id, session, None, Axis::Vertical, window, cx);
        } else {
            self.reveal = Some(session);
        }
        self.activate_workspace(self.space_host(window), project_id, window, cx);
    }

    /// An OS notification was clicked (§7): raises the window (showing it again if it was hidden
    /// to the tray, restoring it if minimized) and shows the session's pane, or at least its workspace if the session is not listed yet.
    pub(crate) fn notification_clicked(
        &mut self,
        session: Ulid,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.restore_window(window, cx);
        let (projects, sessions) = (self.sidebar.projects(), self.sidebar.sessions());
        match inbox::jump(projects, sessions, session, project_id) {
            Some(Jump::Session(session)) => self.open_session(session, window, cx),
            Some(Jump::Workspace(project_id)) => {
                self.activate_workspace(self.space_host(window), project_id, window, cx)
            }
            None => {}
        }
    }

    /// Focuses the pane of `session` (activating its tab) or splits the focused pane along
    /// `axis` to show it: in `tab` when that is shown in a detached window (a split started
    /// there must not land in the main window's tab, spec §4.2), else in the workspace's
    /// active tab.
    pub(crate) fn place_session(
        &mut self,
        project_id: Ulid,
        session: Ulid,
        tab: Option<Ulid>,
        axis: Axis,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let held = (self.tabs.get(&project_id))
            .and_then(|tabs| tabs.tab_of(session))
            .map(|tab| tab.id);
        if let Some(tab_id) = held
            && self.is_detached_tab(project_id, tab_id)
        {
            if let Some(tabs) = self.tabs.get_mut(&project_id) {
                tabs.focus_pane(tab_id, &PaneKey::Session(session));
            }
            return;
        }
        let detached = tab.filter(|id| self.is_detached_tab(project_id, *id));
        // A new agent pane is born with the default of the moment (ADR 0105), so a later
        // change of `[ui] chat_default` never flips it.
        let chat = self.born_chat(session);
        let Some(tabs) = self.tabs.get_mut(&project_id) else {
            return;
        };
        if tabs.focus(&PaneKey::Session(session)) {
            return;
        }
        let write = match detached {
            Some(tab_id) => (tabs
                .with_active(tab_id, |tabs| tabs.split_focused_chat(axis, session, chat)))
            .flatten(),
            None => tabs.split_focused_chat(axis, session, chat),
        };
        if let Some(write) = write {
            self.save_layout(project_id, write, window, cx);
        }
    }

    /// The saved mode a new pane of `session` is born with: an agent gets the default of the
    /// moment (ADR 0105), so a later change of `[ui] chat_default` never flips it.
    fn born_chat(&self, session: Ulid) -> Option<bool> {
        (self.sidebar.sessions().iter())
            .any(|s| s.id == session && s.kind == SessionKind::Agent)
            .then_some(self.chat_default)
    }

    /// The tab of workspace `project_id` that holds the pane `key` (any window).
    pub(crate) fn tab_holding(&self, project_id: Ulid, key: &PaneKey) -> Option<Ulid> {
        (self.tabs.get(&project_id)?.tab_with(key)).map(|tab| tab.id)
    }

    /// Shows `session` on `edge` of `target`'s pane in tab `tab_id` of `project_id` (a drop
    /// from the `+` row).
    /// A session the daemon or an echo placed already stays; when `target` left the tab while
    /// the session started, it lands beside the focused pane like any new pane.
    pub(crate) fn place_session_at(
        &mut self,
        (project_id, tab_id): (Ulid, Ulid),
        session: Ulid,
        target: &PaneKey,
        edge: Edge,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let chat = self.born_chat(session);
        let Some(tabs) = self.tabs.get_mut(&project_id) else {
            return;
        };
        if tabs.tab_of(session).is_some() {
            return;
        }
        let write = tabs.insert_leaf(tab_id, Some(target), edge, Leaf::Session { session, chat });
        match write {
            Some(write) => self.save_layout(project_id, write, window, cx),
            None => self.place_session(
                project_id,
                session,
                Some(tab_id),
                Axis::Vertical,
                window,
                cx,
            ),
        }
    }

    /// An active tab without panes shows the workspace's newest live session not in any tab.
    pub(crate) fn fill_empty_tab(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let newest = self
            .tabs
            .get(&project_id)
            .and_then(|tabs| newest_unplaced(tabs, self.sidebar.sessions()));
        if let Some(session) = newest {
            self.place_session(project_id, session, None, Axis::Vertical, window, cx);
        }
    }

    /// Persists a layout change (`tab.set_layout`).
    pub(crate) fn save_layout(
        &mut self,
        project_id: Ulid,
        write: TabSetLayoutParams,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let saved = connection.call::<()>(Request::TabSetLayout(write.clone()));
        cx.spawn_in(window, async move |this, cx| {
            if let Err(error) = saved.await {
                tracing::warn!(tab_id = %write.tab_id, "tab.set_layout failed: {error:#}");
                let _ = this.update(cx, |shell, cx| {
                    if let Some(tabs) = shell.tabs.get_mut(&project_id) {
                        tabs.write_failed(&write);
                    }
                    shell.toast_error(format!("Cannot save the layout: {error:#}"), cx);
                });
            }
        })
        .detach();
    }

    /// The tabs of the workspace `window` shows.
    pub(crate) fn active_tabs_mut(&mut self, window: &Window) -> Option<&mut Tabs> {
        let project = self.workspace_in(window)?;
        self.tabs.get_mut(&project)
    }

    /// The view of the workspace `host` shows; Docs reads as Terminals while `[features] docs`
    /// is off (ADR 0046).
    pub(crate) fn view(&self, host: HostId) -> View {
        let Some(id) = self.workspace_of(host) else {
            return View::default();
        };
        self.views
            .get(&id)
            .copied()
            .filter(|&view| match view {
                View::Docs => self.features.docs,
                View::Board | View::Terminals => true,
            })
            .unwrap_or_default()
    }

    /// Switches the workspace `window` shows to `view`; leaving Terminals detaches its panes.
    /// Showing Docs re-lists its files (agents may have written new ones); Git (the panel's
    /// tab) coming on screen refreshes its changes (a diff pane does it in `sync_panes`).
    pub(crate) fn show_view(
        &mut self,
        view: View,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let host = self.space_host(window);
        let Some(project_id) = self.workspace_of(host) else {
            return;
        };
        if view == View::Docs && !self.features.docs {
            return;
        }
        self.close_menus();
        let tree_before = self.files_on_screen(host);
        let git_before = self.git_panel_shown(host);
        let shown = self.views.insert(project_id, view).unwrap_or_default();
        if !self.panel_view(host) {
            // The panel (and its grip) leaves with the views that draw it: end a drag in progress.
            self.end_inspector_drag(cx);
        }
        if view == View::Docs && shown != View::Docs {
            self.load_docs(project_id, window, cx);
        }
        if self.files_on_screen(host) && !tree_before {
            self.refresh_files(project_id, window, cx);
        }
        if self.git_panel_shown(host) && !git_before {
            self.refresh_git(project_id, window, cx);
        }
        self.sync_watches(cx);
        if shown != view {
            self.sync_panes(window, cx);
            cx.notify();
        }
    }

    /// Gives the daemon back the file watchers no window shows anything for any more
    /// (`watch.release`, [`watches`]). Cheap: it compares two small sets.
    pub(crate) fn sync_watches(&mut self, cx: &mut Context<'_, Self>) {
        // Every file leaf of every workspace, in any tab or window, shown or not.
        let mut needs = watches::Needs::of(None, None, None, documents::open_worktrees(&self.tabs));
        for (key, project) in self.spaces_shown() {
            let host = HostId::Space(key);
            let files = (self.files_on_screen(host))
                .then(|| project.and_then(|id| self.files.get(&id)))
                .flatten()
                .and_then(|view| view.worktree_task_id);
            let git = (self.git_shown(host))
                .then(|| project.and_then(|id| self.git.get(&id)))
                .flatten()
                .and_then(|view| view.worktree_task_id);
            needs = needs.union(watches::Needs::of(project, files, git, []));
        }
        let released = self.watch_needs.released(&needs);
        self.watch_needs = needs;
        let Some(connection) = self.connection.clone() else {
            return;
        };
        for params in released {
            let project_id = params.project_id;
            let reply = connection.request(Request::WatchRelease(params));
            cx.spawn(async move |_, _| match reply.await {
                Ok(Outcome::Result(_)) => {}
                Ok(Outcome::Error(body)) => {
                    tracing::warn!(%project_id, "watch.release failed: {}", body.message);
                }
                Err(error) => tracing::warn!(%project_id, "watch.release failed: {error:#}"),
            })
            .detach();
        }
    }
}
