//! Which window code runs for, and the per-window state (spec 2026-11-10 §4.2, §4.3; Space
//! windows: ADR 0113).

use crate::*;

/// What each window shows on its own.
#[derive(Default)]
pub(crate) struct HostUi {
    /// Screen bounds (`Window::bounds`), restore bounds when maximized.
    pub(crate) bounds: Bounds<Pixels>,
    /// What `windows.json` keeps: the restore rect (`Window::window_bounds`).
    pub(crate) restore: Bounds<Pixels>,
    /// Windowed, maximized or in full screen (`windows.json` keeps it).
    pub(crate) placement: Placement,
    /// The tab strip as painted last (window-local), for drops from other windows.
    pub(crate) strip: Rc<RefCell<Option<mkrew_ui::window_place::Strip>>>,
    /// Scrolls the tab area of this window's title bar.
    pub(crate) scroll: ScrollHandle,
    /// The chip last scrolled into view (a tab or view key): it scrolls again only when the
    /// active chip changes, so the wheel is free in between.
    pub(crate) scrolled: RefCell<Option<SharedString>>,
    /// The tab content area as painted last: the size of a window torn off from here.
    pub(crate) content: Rc<Cell<gpui::Size<Pixels>>>,
    /// The window title last set: `set_window_title` is a system call, made only on a change.
    pub(crate) title: String,
    /// The macOS traffic light position set last (`sync_traffic_lights`): AppKit relayouts the
    /// buttons on every call, so only a change is sent.
    pub(crate) traffic_light: Option<(f32, f32)>,
    /// The window is shown (not minimized or hidden): its panes stay attached. Space windows
    /// only; detached ones keep it in `Detached`.
    pub(crate) visible: bool,
    /// The workspace a Space window shows (`None`: the empty state).
    pub(crate) workspace: Option<Ulid>,
    /// The right panel of a Space window: open or not, and its tab.
    pub(crate) right_panel: inspector::RightPanel,
    /// The scroll and list state of what the window draws.
    pub(crate) view: HostView,
    /// The sidebar of a Space window, a cached child view (created on its first frame).
    pub(crate) sidebar_view: Option<Entity<shell::sidebar_view::SidebarView>>,
}

/// The scroll and list state a window keeps for the lists it draws: one window scrolling a
/// shared handle would move it in the other window too.
pub(crate) struct HostView {
    /// The panel's file tree.
    pub(crate) files_scroll: UniformListScrollHandle,
    /// The panel's Git file list.
    pub(crate) git_files_scroll: UniformListScrollHandle,
    /// The Docs view's preview page and source list.
    pub(crate) doc_page: preview::Page,
    pub(crate) doc_source_scroll: UniformListScrollHandle,
    /// The comment threads beside a doc or diff.
    pub(crate) comments_scroll: ScrollHandle,
    /// The Jira list of the Board.
    pub(crate) jira_scroll: ScrollHandle,
}

impl Default for HostView {
    fn default() -> Self {
        HostView {
            files_scroll: UniformListScrollHandle::new(),
            git_files_scroll: UniformListScrollHandle::new(),
            doc_page: preview::Page::default(),
            doc_source_scroll: UniformListScrollHandle::new(),
            comments_scroll: ScrollHandle::new(),
            jira_scroll: ScrollHandle::new(),
        }
    }
}

impl Shell {
    /// The window `window` is: a detached one, else its Space window. A window this Shell does
    /// not know yet (its first frame) counts as the primary Space window.
    pub(crate) fn host_of(&self, window: &Window) -> HostId {
        if let Some(key) = self.detached_key(window) {
            return HostId::Detached(key);
        }
        let space = self.spaces.get(&window.window_handle()).copied();
        HostId::Space(space.unwrap_or_else(|| self.primary_or_first()))
    }

    /// The workspace a window shows: a Space window's, or a detached window's shown tab's.
    pub(crate) fn workspace_of(&self, host: HostId) -> Option<Ulid> {
        match host {
            HostId::Space(_) => self.hosts.get(&host)?.workspace,
            HostId::Detached(key) => self.detached.active(key).map(|tab| tab.project()),
        }
    }

    /// The Space window that serves `window`: itself, or for a detached window the Space
    /// window of its shown tab's workspace, else the primary one. Workspace-wide actions and
    /// views (`view`, the panel, Git, the Board) run there.
    pub(crate) fn space_host(&self, window: &Window) -> HostId {
        match self.host_of(window) {
            host @ HostId::Space(_) => host,
            HostId::Detached(key) => (self.detached.active(key))
                .and_then(|tab| self.space_of(tab.project()))
                .map_or_else(|| self.primary_host(), HostId::Space),
        }
    }

    /// The workspace of the Space window that serves `window` ([`Self::space_host`]).
    pub(crate) fn workspace_in(&self, window: &Window) -> Option<Ulid> {
        self.workspace_of(self.space_host(window))
    }

    /// Every Space window (opened or being opened) with its workspace, by key.
    pub(crate) fn spaces_shown(&self) -> impl Iterator<Item = (SpaceKey, Option<Ulid>)> + use<> {
        let mut spaces: Vec<(SpaceKey, Option<Ulid>)> = (self.hosts.iter())
            .filter_map(|(host, ui)| Some((host.space()?, ui.workspace)))
            .collect();
        spaces.sort_by_key(|(key, _)| *key);
        spaces.into_iter()
    }

    /// The Space window showing `project` (one at most in either mode).
    pub(crate) fn space_of(&self, project: Ulid) -> Option<SpaceKey> {
        self.spaces_shown()
            .find_map(|(key, shown)| (shown == Some(project)).then_some(key))
    }

    /// The workspaces some Space window shows.
    pub(crate) fn on_screen_workspaces(&self) -> Vec<Ulid> {
        let mut shown: Vec<Ulid> = self.spaces_shown().filter_map(|(_, shown)| shown).collect();
        shown.sort();
        shown.dedup();
        shown
    }

    /// The Space window used last: where Settings, Turns… and app menu actions open.
    pub(crate) fn primary(&self) -> Option<SpaceKey> {
        (self.activation.iter())
            .find_map(|host| host.space())
            .or_else(|| self.spaces_shown().next().map(|(key, _)| key))
    }

    /// [`Self::primary`]; a Shell has a Space window from `main()` until it quits, and
    /// `SpaceKey(0)` is the first one's key.
    pub(crate) fn primary_or_first(&self) -> SpaceKey {
        self.primary().unwrap_or(SpaceKey(0))
    }

    /// The primary Space window as a host.
    pub(crate) fn primary_host(&self) -> HostId {
        HostId::Space(self.primary_or_first())
    }

    /// Whether Space window `host` is shown (not minimized or hidden).
    pub(crate) fn space_visible(&self, host: HostId) -> bool {
        self.hosts.get(&host).is_some_and(|ui| ui.visible)
    }

    /// The right panel of `host` (closed for a window that has none).
    pub(crate) fn panel(&self, host: HostId) -> inspector::RightPanel {
        self.hosts
            .get(&host)
            .map(|ui| ui.right_panel)
            .unwrap_or_default()
    }

    /// The scroll and list state of what `host` draws.
    pub(crate) fn host_view(&self, host: HostId) -> &HostView {
        self.hosts.get(&host).map_or(&self.no_view, |ui| &ui.view)
    }

    /// [`Self::host_view`] of the Space window showing `project`, if any.
    pub(crate) fn project_view(&self, project: Ulid) -> Option<&HostView> {
        let key = self.space_of(project)?;
        Some(self.host_view(HostId::Space(key)))
    }

    pub(crate) fn detached_key(&self, window: &Window) -> Option<WindowKey> {
        self.windows.get(&window.window_handle()).copied()
    }

    /// The workspace of a loaded terminal tab.
    pub(crate) fn terminal_project(&self, tab_id: Ulid) -> Option<Ulid> {
        (self.tabs.iter()).find_map(|(project, tabs)| tabs.tab(tab_id).map(|_| *project))
    }

    /// Where a terminal tab is shown: its detached window, else the Space window of its
    /// workspace (none while no Space window shows that workspace).
    pub(crate) fn terminal_host(&self, project: Ulid, tab_id: Ulid) -> Option<HostId> {
        let tab = DockTab::Terminal { project, tab_id };
        match self.detached.window_of(&tab) {
            Some(key) => Some(HostId::Detached(key)),
            None => self.space_of(project).map(HostId::Space),
        }
    }

    /// Whether a terminal tab sits in a detached window.
    pub(crate) fn is_detached_tab(&self, project: Ulid, tab_id: Ulid) -> bool {
        self.detached
            .is_detached(&DockTab::Terminal { project, tab_id })
    }

    /// The workspace and tab whose layout holds `session`.
    pub(crate) fn tab_of_session(&self, session: Ulid) -> Option<(Ulid, Ulid)> {
        (self.tabs.iter()).find_map(|(project, tabs)| Some((*project, tabs.tab_of(session)?.id)))
    }

    /// How a window names its shown tab, "<tab> - <workspace>": its title and the "Move to
    /// window" rows.
    pub(crate) fn tab_title(&self, project: Ulid, tab_id: Ulid) -> Option<String> {
        let tab = self.tabs.get(&project)?.tab(tab_id)?;
        let name = &(self.sidebar.projects().iter())
            .find(|p| p.id == project)?
            .name;
        let file = file_label(tab, self.documents.get(&project)).map(|label| label.name);
        let label = file
            .as_deref()
            .or_else(|| diff_label(tab))
            .or_else(|| {
                web_label(tab)
                    .and_then(|id| self.browsers.get(&id))
                    .map(|b| b.page.label())
            })
            .or_else(|| session_label(tab, self.sidebar.sessions()))
            .unwrap_or(&tab.title);
        Some(format!("{label} - {name}"))
    }

    /// The window that shows the tab holding `session` (the primary one if no window does).
    pub(crate) fn session_host(&self, session: Ulid) -> HostId {
        self.tab_of_session(session)
            .and_then(|(project, tab_id)| self.terminal_host(project, tab_id))
            .unwrap_or_else(|| self.primary_host())
    }

    /// Whether a tab of `project` is shown in the same window as `tab_id`: the tabs that
    /// "Close others" and its siblings may reach.
    pub(crate) fn same_window(&self, project: Ulid, tab_id: Ulid) -> impl Fn(Ulid) -> bool + '_ {
        let host = self.terminal_host(project, tab_id);
        move |other| self.terminal_host(project, other) == host
    }

    /// The terminal tab `window` shows: a Space window's shown workspace and active tab, or
    /// the detached window's active tab.
    pub(crate) fn tab_scope(&self, window: &Window) -> Option<(Ulid, Ulid)> {
        match self.host_of(window) {
            host @ HostId::Space(_) => {
                let project =
                    (self.workspace_of(host)).filter(|_| self.view(host) == View::Terminals)?;
                Some((project, self.tabs.get(&project)?.active()?.id))
            }
            HostId::Detached(key) => match self.detached.active(key)? {
                DockTab::Terminal { project, tab_id } => Some((project, tab_id)),
            },
        }
    }

    /// `f` on the tabs of `window`'s tab, with that tab active for the call. In a Space window
    /// the active tab is the shown one and changes stick (a tab switch); in a detached window
    /// the Space window's active tab is given back.
    pub(crate) fn with_tab<R>(
        &mut self,
        window: &Window,
        f: impl FnOnce(&mut Tabs) -> R,
    ) -> Option<R> {
        match self.host_of(window) {
            HostId::Space(_) => {
                let project = self.workspace_in(window)?;
                Some(f(self.tabs.get_mut(&project)?))
            }
            HostId::Detached(_) => {
                let (project, tab_id) = self.tab_scope(window)?;
                self.tabs.get_mut(&project)?.with_active(tab_id, f)
            }
        }
    }

    /// The pane that has (or gets) the keyboard in `window`, of any kind.
    pub(crate) fn focused_key(&self, window: &Window) -> Option<PaneKey> {
        let (project, tab_id) = self.tab_scope(window)?;
        self.tabs.get(&project)?.tab(tab_id)?.focused().cloned()
    }

    /// The session pane that has (or gets) the keyboard in `window`: the focused pane when it
    /// is a session, `None` when a file or the diff has the focus.
    pub(crate) fn focused_session(&self, window: &Window) -> Option<Ulid> {
        self.focused_key(window)?.session()
    }

    /// The workspace and file of the focused pane of `window`, when it is a file pane.
    pub(crate) fn focused_doc(&self, window: &Window) -> Option<(Ulid, FileKey)> {
        let (project, tab_id) = self.tab_scope(window)?;
        match self.tabs.get(&project)?.tab(tab_id)?.focused()? {
            PaneKey::File(file) => Some((project, file.clone())),
            _ => None,
        }
    }

    /// The active workspace, when the focused pane of `window` is its Diff leaf. A Diff leaf of
    /// another workspace (a detached window) never counts: Git keys and comment commands act
    /// on the active workspace only.
    pub(crate) fn focused_diff(&self, window: &Window) -> Option<Ulid> {
        let (project, tab_id) = self.tab_scope(window)?;
        let tab = self.tabs.get(&project)?.tab(tab_id)?;
        (tab.focused() == Some(&PaneKey::Diff) && self.workspace_in(window) == Some(project))
            .then_some(project)
    }

    pub(crate) fn focused_file(&self, window: &Window) -> Option<FileKey> {
        self.focused_doc(window).map(|(_, path)| path)
    }

    /// The file the right panel and prompt variables mean: the shown workspace's active tab's
    /// focused pane, when it is a file (also while a detached window has the keyboard).
    pub(crate) fn panel_file(&self, project_id: Ulid) -> Option<FileKey> {
        match self.tabs.get(&project_id)?.active()?.focused()? {
            PaneKey::File(file) => Some(file.clone()),
            _ => None,
        }
    }

    /// Focuses the pane of `key` in its tab; a tab in a detached window does not become the
    /// main window's active tab (spec §4.4).
    pub(crate) fn focus_pane_key(&mut self, project: Ulid, key: &PaneKey) {
        let Some(tab_id) = (self.tabs.get(&project))
            .and_then(|tabs| tabs.tab_with(key))
            .map(|tab| tab.id)
        else {
            return;
        };
        let detached = self.is_detached_tab(project, tab_id);
        if let Some(tabs) = self.tabs.get_mut(&project) {
            if detached {
                tabs.focus_pane(tab_id, key);
            } else {
                tabs.focus(key);
            }
        }
    }

    pub(crate) fn focus_session_pane(&mut self, session: Ulid) {
        if let Some((project, _)) = self.tab_of_session(session) {
            self.focus_pane_key(project, &PaneKey::Session(session));
        }
    }

    /// The main window's active tab never names a detached tab (spec §4.1).
    pub(crate) fn keep_main_active_visible(&mut self) {
        for (project, tabs) in self.tabs.iter_mut() {
            let project = *project;
            let detached = &self.detached;
            tabs.activate_skipping(|tab_id| {
                detached.is_detached(&DockTab::Terminal { project, tab_id })
            });
        }
    }

    /// Runs `f` with `host`'s window once the current update is over (the `Shell` and the
    /// window that started it are busy until then).
    pub(crate) fn in_window(
        &self,
        host: HostId,
        cx: &mut Context<'_, Self>,
        f: impl FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>) + 'static,
    ) {
        let Some(handle) = self.handle_of(host) else {
            return;
        };
        let this = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |shell, cx| f(shell, window, cx));
            });
        });
    }

    /// The OS window of `host`, while it is open.
    pub(crate) fn handle_of(&self, host: HostId) -> Option<gpui::AnyWindowHandle> {
        match host {
            HostId::Space(key) => self.space_handles.get(&key).copied(),
            HostId::Detached(key) => self.window_handles.get(&key).copied(),
        }
    }

    pub(crate) fn activated(&mut self, host: HostId) {
        self.activation.retain(|known| *known != host);
        self.activation.insert(0, host);
    }

    /// Whether any Mkrew window, Space or detached, is the OS's active one.
    pub(crate) fn any_window_active(&self, cx: &App) -> bool {
        cx.active_window().is_some_and(|active| {
            self.spaces.contains_key(&active) || self.windows.contains_key(&active)
        })
    }

    /// The window that draws the toasts (see [`detached::toast_host`]).
    pub(crate) fn toast_host(&self) -> HostId {
        let shown = |host| match host {
            HostId::Space(_) => self.hosts.get(&host).is_some_and(|ui| ui.visible),
            HostId::Detached(key) => self.detached.window(key).is_some_and(|w| w.visible),
        };
        detached::toast_host(&self.activation, shown, self.primary_host())
    }

    pub(crate) fn record_bounds(&mut self, host: HostId, window: &Window) {
        let (placement, restore) = Placement::of(window.window_bounds());
        let ui = self.hosts.entry(host).or_default();
        ui.bounds = window.bounds();
        ui.restore = restore;
        ui.placement = placement;
        if host.space().is_some() {
            self.main_dirty = true;
        } else {
            self.windows_dirty = true;
        }
    }

    /// Writes `main-window.json`: where the primary Space window restores to and whether it is
    /// maximized or in full screen (SPEC §20L). Not before its size is known.
    pub(crate) fn save_main_frame(&mut self) {
        self.main_dirty = false;
        let Some(ui) = self.hosts.get(&self.primary_host()) else {
            return;
        };
        if ui.restore.size.width <= px(0.0) {
            return;
        }
        let (maximized, fullscreen) = ui.placement.flags();
        let frame = detached::SavedFrame {
            bounds: window_place::to_saved(ui.restore),
            maximized,
            fullscreen,
        };
        if let Err(error) = detached::save_frame(&self.main_frame_path, &frame) {
            tracing::warn!("cannot save the main window's place: {error}");
        }
    }

    /// What every way out of the app does: the places of the detached windows and of the main
    /// one are written, the machine windows are asked to quit.
    pub(crate) fn save_before_exit(&mut self, cx: &mut Context<'_, Self>) {
        self.save_windows(cx);
        self.save_main_frame();
        self.close_machines();
    }

    /// Puts `window`'s macOS traffic lights where the title bar's height at `ui_size` wants them
    /// (`[ui] font_size` scales the bar, the lights do not). macOS only: compiled and linted by
    /// the macOS CI job; elsewhere `traffic_light_pos` is `None` and nothing happens.
    #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
    pub(crate) fn sync_traffic_lights(&mut self, host: HostId, window: &Window, ui_size: f32) {
        let Some(pos) = theme::traffic_light_pos(Os::CURRENT, ui_size) else {
            return;
        };
        let ui = self.hosts.entry(host).or_default();
        if ui.traffic_light.replace(pos) != Some(pos) {
            #[cfg(target_os = "macos")]
            window.set_traffic_light_position(point(px(pos.0), px(pos.1)));
        }
    }

    /// Drops detached tabs that are gone (closed, workspace archived); a workspace whose tabs
    /// are not loaded (yet, or again after a reconnect) keeps them. Emptied windows close.
    pub(crate) fn prune_detached(&mut self, cx: &mut Context<'_, Self>) {
        // The sidebar's list is the daemon's once connected: a workspace missing there is
        // archived. Before that (startup, reconnect) nothing is pruned for it.
        let listed = matches!(self.status, Status::Connected);
        let before = self.detached.tab_count();
        let emptied = self.detached.prune(|tab| {
            let DockTab::Terminal { project, tab_id } = *tab;
            let archived = listed && !self.sidebar.projects().iter().any(|p| p.id == project);
            match self.tabs.get(&project) {
                _ if archived => detached::Keep::No,
                Some(tabs) if tabs.tab(tab_id).is_some() => detached::Keep::Yes,
                Some(_) => detached::Keep::No,
                None => detached::Keep::Unknown,
            }
        });
        // A window that kept its other tabs has a new strip to save.
        if self.detached.tab_count() != before {
            self.windows_dirty = true;
        }
        for key in emptied {
            self.close_os_window(key, cx);
        }
    }

    /// Removes the OS window of `key` (its tabs already left the model).
    pub(crate) fn close_os_window(&mut self, key: WindowKey, cx: &mut Context<'_, Self>) {
        let Some(handle) = self.window_handles.remove(&key) else {
            return;
        };
        self.windows.remove(&handle);
        self.hosts.remove(&HostId::Detached(key));
        self.activation
            .retain(|host| *host != HostId::Detached(key));
        self.windows_dirty = true;
        self.drop_hint = None;
        self.reorder = None;
        self.tab_drag = false;
        // A popover of the closing window has no one to draw it: close it, and give the
        // keyboard back in the main window.
        let gone = HostId::Detached(key);
        let menu = self.menu_host == gone && self.menu_open();
        let palette = self.palette_host == gone && self.palette.is_some();
        let picker = self.picker_host == gone && self.picker_open();
        if menu {
            self.close_menus();
        }
        if palette {
            self.palette = None;
            self.palette_host = self.primary_host();
            self.palette_return = None;
        }
        if picker {
            self.prompt_picker = None;
            self.slash_picker = None;
        }
        let primary = self.primary_host();
        if self.menu_host == gone {
            self.menu_host = primary;
        }
        if self.palette_host == gone {
            self.palette_host = primary;
        }
        if self.picker_host == gone {
            self.picker_host = self.primary_host();
        }
        if menu || palette || picker {
            self.in_window(self.primary_host(), cx, |shell, window, cx| {
                shell.restore_theme_pick(window, cx);
                shell.focus_pane(window, cx);
            });
        }
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        });
    }

    /// Writes `windows.json` (spec §6.1): the live windows, plus the saved ones a pending
    /// restore has not opened yet, so neither is lost. Not before the file was read (an app
    /// that never got that far must not erase it). A failed write is a toast (once) and is
    /// tried again on the next tick.
    pub(crate) fn save_windows(&mut self, cx: &mut Context<'_, Self>) {
        if !self.restored_once {
            return;
        }
        self.windows_dirty = false;
        let live: Vec<detached::SavedWindow> = (self.detached.windows().iter())
            .filter_map(|window| {
                let ui = self.hosts.get(&HostId::Detached(window.key))?;
                let (maximized, fullscreen) = ui.placement.flags();
                Some(detached::SavedWindow {
                    bounds: window_place::to_saved(ui.restore),
                    maximized,
                    fullscreen,
                    tabs: window.tabs.clone(),
                    active: window.active,
                })
            })
            .collect();
        let pending = self.restore.as_deref().unwrap_or(&[]);
        let windows = detached::merge_saved(live, pending);
        let layout = detached::Layout {
            spaces: vec![],
            windows,
        };
        match detached::save(&self.windows_path, &layout) {
            Ok(()) => self.windows_save_failed = false,
            Err(error) => {
                tracing::warn!("cannot save the detached windows: {error}");
                self.windows_dirty = true;
                if !std::mem::replace(&mut self.windows_save_failed, true) {
                    self.toast_error(format!("Cannot save the detached windows: {error}"), cx);
                }
            }
        }
    }

    /// Reads `windows.json` once the workspaces are listed: loads the tabs of every workspace
    /// a saved window shows; the windows open once all answered (`restore_progress`).
    pub(crate) fn start_restore(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let text = match std::fs::read_to_string(&self.windows_path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => return tracing::warn!("cannot read the detached windows: {error}"),
        };
        let windows = match detached::from_json(&text).map(|layout| layout.windows) {
            Ok(windows) if !windows.is_empty() => windows,
            Ok(_) => return,
            Err(error) => return tracing::warn!("ignoring the detached windows file: {error}"),
        };
        self.restore = Some(windows);
        self.load_restore_tabs(window, cx);
        self.restore_progress(window, cx);
    }

    /// The pending restore's workspaces whose tabs are not loaded: saved tabs of a workspace the
    /// daemon no longer lists are gone (dropped); one that did not answer (failed, or the
    /// connection was lost) keeps waiting.
    fn restore_waiting(&mut self) -> HashSet<Ulid> {
        let listed: HashSet<Ulid> = self.sidebar.projects().iter().map(|p| p.id).collect();
        let Some(restore) = self.restore.as_mut() else {
            return HashSet::new();
        };
        *restore = std::mem::take(restore)
            .into_iter()
            .map(|saved| saved.keeping(|tab| listed.contains(&tab.project())))
            .filter(|saved| !saved.tabs.is_empty())
            .collect();
        detached::waiting_projects(restore, |project| self.tabs.contains_key(&project))
    }

    fn load_restore_tabs(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        for project in self.restore_waiting() {
            self.load_tabs(project, window, cx);
        }
    }

    /// A workspace's tabs answered: once none is awaited, the saved windows open with the tabs
    /// that still exist.
    pub(crate) fn restore_progress(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.restore.is_none() || !self.restore_waiting().is_empty() {
            return;
        }
        let Some(restore) = self.restore.take() else {
            return;
        };
        let displays: Vec<Bounds<Pixels>> =
            cx.displays().iter().map(|d| d.visible_bounds()).collect();
        let primary = cx.primary_display().map(|d| d.visible_bounds());
        for saved in restore {
            let saved = saved.keeping(|tab| {
                let DockTab::Terminal { project, tab_id } = *tab;
                self.tabs
                    .get(&project)
                    .is_some_and(|tabs| tabs.tab(tab_id).is_some())
            });
            let b = saved.bounds.clone();
            let placement = Placement::from_flags(saved.maximized, saved.fullscreen);
            let Some(key) = self.detached.restore(&[saved]).first().copied() else {
                continue;
            };
            let bounds = Bounds::new(point(px(b.x), px(b.y)), size(px(b.width), px(b.height)));
            // As the main window: cut to its display, title bar reachable, in its placement.
            let (placement, bounds) =
                mkrew_ui::window_place::Placement::of(mkrew_ui::window_place::restored(
                    bounds,
                    placement,
                    size(
                        px(mkrew_ui::theme::DETACHED_MIN_WIDTH),
                        px(mkrew_ui::theme::DETACHED_MIN_HEIGHT),
                    ),
                    &displays,
                    primary,
                ));
            self.open_os_window(key, bounds, placement, self.primary_host(), window, cx);
        }
    }

    /// After a reconnect (or a failed answer) the tabs are read again: those of every listed
    /// workspace a detached window or a pending restore shows.
    pub(crate) fn reload_detached_tabs(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.load_restore_tabs(window, cx);
        let listed: HashSet<Ulid> = self.sidebar.projects().iter().map(|p| p.id).collect();
        let shown = (self.detached.windows().iter())
            .flat_map(|w| w.tabs.iter().map(DockTab::project))
            .filter(|project| listed.contains(project) && !self.tabs.contains_key(project))
            .collect::<HashSet<_>>();
        for project in shown {
            self.load_tabs(project, window, cx);
        }
    }

    /// Shows or hides every detached window (they follow the main one to the tray).
    pub(crate) fn set_detached_shown(&self, shown: bool, cx: &mut Context<'_, Self>) {
        for &handle in self.windows.keys() {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| {
                    mkrew_ui::desktop::set_window_shown(window, shown);
                });
            });
        }
    }
}
