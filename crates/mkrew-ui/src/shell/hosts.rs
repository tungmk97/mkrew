//! Which window code runs for, and the per-window state (spec 2026-11-10 §4.2, §4.3).

use crate::*;

/// What each window shows on its own.
#[derive(Default)]
pub(crate) struct HostUi {
    /// Screen bounds (`Window::bounds`), restore bounds when maximized.
    pub(crate) bounds: Bounds<Pixels>,
    /// What `windows.json` keeps: the restore rect (`Window::window_bounds`).
    pub(crate) restore: Bounds<Pixels>,
    pub(crate) maximized: bool,
    /// The tab strip as painted last (window-local), for drops from other windows.
    pub(crate) strip: Rc<RefCell<Option<mkrew_ui::window_place::Strip>>>,
    /// The tab content area as painted last: the size of a window torn off from here.
    pub(crate) content: Rc<Cell<gpui::Size<Pixels>>>,
}

/// Saved windows waiting until every workspace they show has its tabs loaded.
#[expect(dead_code, reason = "read by the detached window tasks that follow")]
pub(crate) struct Restore {
    pub(crate) windows: Vec<detached::SavedWindow>,
    pub(crate) waiting: HashSet<Ulid>,
}

impl Shell {
    pub(crate) fn host_of(&self, window: &Window) -> HostId {
        self.detached_key(window)
            .map_or(HostId::Main, HostId::Detached)
    }

    pub(crate) fn detached_key(&self, window: &Window) -> Option<WindowKey> {
        self.windows.get(&window.window_handle()).copied()
    }

    /// The workspace of a loaded terminal tab.
    pub(crate) fn terminal_project(&self, tab_id: Ulid) -> Option<Ulid> {
        (self.tabs.iter()).find_map(|(project, tabs)| tabs.tab(tab_id).map(|_| *project))
    }

    /// Where a terminal tab is shown.
    pub(crate) fn terminal_host(&self, project: Ulid, tab_id: Ulid) -> HostId {
        self.detached
            .host_of(&DockTab::Terminal { project, tab_id })
    }

    /// The terminal tab `window` shows: the main window's shown workspace and active tab, or
    /// the detached window's active tab.
    pub(crate) fn tab_scope(&self, window: &Window) -> Option<(Ulid, Ulid)> {
        match self.host_of(window) {
            HostId::Main => {
                let project = self.workspace.filter(|_| self.view() == View::Terminals)?;
                Some((project, self.tabs.get(&project)?.active()?.id))
            }
            HostId::Detached(key) => match self.detached.active(key)? {
                DockTab::Terminal { project, tab_id } => Some((project, tab_id)),
            },
        }
    }

    /// `f` on the tabs of `window`'s tab, with that tab active for the call. In the main
    /// window the active tab is the shown one and changes stick (a tab switch); in a detached
    /// window the main window's active tab is given back.
    pub(crate) fn with_tab<R>(
        &mut self,
        window: &Window,
        f: impl FnOnce(&mut Tabs) -> R,
    ) -> Option<R> {
        match self.host_of(window) {
            HostId::Main => {
                let project = self.workspace?;
                Some(f(self.tabs.get_mut(&project)?))
            }
            HostId::Detached(_) => {
                let (project, tab_id) = self.tab_scope(window)?;
                self.tabs.get_mut(&project)?.with_active(tab_id, f)
            }
        }
    }

    /// The pane that has (or gets) the keyboard in `window`.
    pub(crate) fn focused_session(&self, window: &Window) -> Option<Ulid> {
        let (project, tab_id) = self.tab_scope(window)?;
        self.tabs.get(&project)?.tab(tab_id)?.focused()
    }

    /// Focuses `session`'s pane in its tab; a tab in a detached window does not become the
    /// main window's active tab (spec §4.4).
    pub(crate) fn focus_session_pane(&mut self, session: Ulid) {
        let Some((project, tab_id)) = (self.tabs.iter())
            .find_map(|(project, tabs)| Some((*project, tabs.tab_of(session)?.id)))
        else {
            return;
        };
        let detached = self.terminal_host(project, tab_id) != HostId::Main;
        if let Some(tabs) = self.tabs.get_mut(&project) {
            if detached {
                tabs.focus_pane(tab_id, session);
            } else {
                tabs.focus(session);
            }
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
        let handle = match host {
            HostId::Main => Some(self.main_window),
            HostId::Detached(key) => self.window_handles.get(&key).copied(),
        };
        let Some(handle) = handle else {
            return;
        };
        let this = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |shell, cx| f(shell, window, cx));
            });
        });
    }

    pub(crate) fn activated(&mut self, host: HostId) {
        self.activation.retain(|known| *known != host);
        self.activation.insert(0, host);
    }

    pub(crate) fn record_bounds(&mut self, host: HostId, window: &Window) {
        let (bounds, maximized) = match window.window_bounds() {
            gpui::WindowBounds::Maximized(bounds) => (bounds, true),
            gpui::WindowBounds::Windowed(bounds) | gpui::WindowBounds::Fullscreen(bounds) => {
                (bounds, false)
            }
        };
        let ui = self.hosts.entry(host).or_default();
        ui.bounds = window.bounds();
        ui.restore = bounds;
        ui.maximized = maximized;
        if host != HostId::Main {
            self.windows_dirty = true;
        }
    }

    /// Drops detached tabs that are gone (closed, workspace archived); a workspace whose tabs
    /// are not loaded (yet, or again after a reconnect) keeps them. Emptied windows close.
    pub(crate) fn prune_detached(&mut self, cx: &mut Context<'_, Self>) {
        // The sidebar's list is the daemon's once connected: a workspace missing there is
        // archived. Before that (startup, reconnect) nothing is pruned for it.
        let listed = matches!(self.status, Status::Connected);
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
        // A popover of the closing window has no one to draw it: close it, and give the
        // keyboard back in the main window.
        let gone = HostId::Detached(key);
        let menu = self.menu_host == gone && self.menu_open();
        let palette = self.palette_host == gone && self.palette.is_some();
        if menu {
            self.close_menus();
        }
        if palette {
            self.palette = None;
            self.palette_host = HostId::Main;
            self.palette_return = None;
        }
        self.menu_host = if self.menu_host == gone {
            HostId::Main
        } else {
            self.menu_host
        };
        self.palette_host = if self.palette_host == gone {
            HostId::Main
        } else {
            self.palette_host
        };
        if menu || palette {
            self.in_window(HostId::Main, cx, |shell, window, cx| {
                shell.restore_theme_pick(window, cx);
                shell.focus_pane(window, cx);
            });
        }
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        });
    }
}
