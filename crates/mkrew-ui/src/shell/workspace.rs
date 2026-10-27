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

    /// Shows the tabs of `project_id`, loading them the first time.
    pub(crate) fn activate_workspace(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let switched = self.workspace != Some(project_id);
        self.workspace = Some(project_id);
        self.workspace_switcher = false;
        if switched {
            self.close_menus();
            self.end_inspector_drag(cx);
            // Nothing is kept for the workspace that is no longer shown.
            self.git_summary = None;
        }
        if switched || self.git_summary.is_none() {
            self.load_git_summary(project_id, window, cx);
        }
        if !self.tabs.contains_key(&project_id) {
            self.load_tabs(project_id, window, cx);
        }
        if switched && self.view() == View::Docs {
            self.load_docs(project_id, window, cx);
        }
        if switched && self.files_on_screen() {
            self.refresh_files(project_id, window, cx);
        }
        if switched && self.git_shown() {
            self.refresh_git(project_id, window, cx);
        }
        self.sync_panes(window, cx);
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

    /// Adds a tab to a workspace and activates it; `start` then spawns a session into it.
    pub(crate) fn create_tab(
        &mut self,
        project_id: Ulid,
        start: Option<SessionKind>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(kind) = start
            && self.ask_for_default(project_id, kind, window, cx)
        {
            return;
        }
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.tab_error = None;
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
            position: None,
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
                        if let Some(kind) = start
                            && shell.workspace == Some(project_id)
                        {
                            shell.new_pane(kind, Axis::Vertical, window, cx);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "tab.create failed: {error:#}");
                        let error = format!("{error:#}");
                        if first {
                            shell.tabs_load_failed(project_id);
                            shell.list_failed(error);
                        } else {
                            shell.tab_error = Some(format!("Cannot add a tab: {error}"));
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
            self.place_session(project_id, session, Axis::Vertical, window, cx);
        }
        self.fill_empty_tab(project_id, window, cx);
        self.sync_panes(window, cx);
        cx.notify();
    }

    /// `project_id`'s tabs failed to load: it stops loading and its buffered `tab.updated`
    /// events go (other workspaces still loading keep theirs).
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

    /// Shows a session picked in the sidebar: focuses its pane, or places it next to the
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
        self.views.insert(project_id, View::Terminals);
        if self.tabs.contains_key(&project_id) {
            self.place_session(project_id, session, Axis::Vertical, window, cx);
        } else {
            self.reveal = Some(session);
        }
        self.activate_workspace(project_id, window, cx);
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
            Some(Jump::Workspace(project_id)) => self.activate_workspace(project_id, window, cx),
            None => {}
        }
    }

    /// Focuses the pane of `session` (activating its tab) or splits the focused pane along
    /// `axis` to show it.
    pub(crate) fn place_session(
        &mut self,
        project_id: Ulid,
        session: Ulid,
        axis: Axis,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(tabs) = self.tabs.get_mut(&project_id) else {
            return;
        };
        if tabs.focus(session) {
            return;
        }
        if let Some(write) = tabs.split_focused(axis, session) {
            self.save_layout(project_id, write, window, cx);
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
            self.place_session(project_id, session, Axis::Vertical, window, cx);
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
                    shell.tab_error = Some(format!("Cannot save the layout: {error:#}"));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(crate) fn active_tabs_mut(&mut self) -> Option<&mut Tabs> {
        self.tabs.get_mut(&self.workspace?)
    }

    /// The view of the active workspace; Docs reads as Terminals while `[features] docs` is
    /// off (ADR 0046), the Editor while the workspace has no open file, the Diff while it has
    /// no open diff.
    pub(crate) fn view(&self) -> View {
        let Some(id) = self.workspace else {
            return View::default();
        };
        self.views
            .get(&id)
            .copied()
            .filter(|&view| match view {
                View::Docs => self.features.docs,
                View::Editor => self.has_file_tabs(id),
                View::Diff => self.diff_open(id),
                View::Board | View::Terminals => true,
            })
            .unwrap_or_default()
    }

    /// The workspace has open file tabs (the Editor view exists).
    pub(crate) fn has_file_tabs(&self, project_id: Ulid) -> bool {
        self.files
            .get(&project_id)
            .is_some_and(|view| !view.tabs().is_empty())
    }

    /// The workspace has an open diff (the Diff view exists).
    pub(crate) fn diff_open(&self, project_id: Ulid) -> bool {
        self.git.get(&project_id).is_some_and(GitView::diff_open)
    }

    /// Switches the active workspace to `view`; leaving Terminals detaches its panes. Showing
    /// Docs re-lists its files (agents may have written new ones); showing the Editor refreshes
    /// the tree and gives the editor the keyboard; Git (the panel's tab or the diff) coming on
    /// screen refreshes its changes.
    pub(crate) fn show_view(
        &mut self,
        view: View,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self.workspace else {
            return;
        };
        if view == View::Docs && !self.features.docs {
            return;
        }
        self.close_menus();
        let tree_before = self.files_on_screen();
        let git_before = self.git_shown();
        let tree_focused = self.files_focus.is_focused(window);
        let shown = self.views.insert(project_id, view).unwrap_or_default();
        if !self.panel_view() {
            // The panel (and its grip) leaves with the views that draw it: end a drag in progress.
            self.end_inspector_drag(cx);
        }
        if view == View::Docs && shown != View::Docs {
            self.load_docs(project_id, window, cx);
        }
        if self.files_on_screen() && !tree_before {
            self.refresh_files(project_id, window, cx);
        }
        if self.git_shown() && !git_before {
            self.refresh_git(project_id, window, cx);
        }
        if view == View::Diff {
            window.focus(&self.git_focus, cx);
        }
        if shown != view {
            self.sync_panes(window, cx);
            cx.notify();
        }
        // After `sync_panes` (its focus would win). A tree click keeps the keyboard in the
        // tree; any other way in types in the editor.
        if view == View::Editor {
            let target = if tree_focused {
                &self.files_focus
            } else {
                &self.file_editor_focus
            };
            window.focus(target, cx);
        }
    }
}
