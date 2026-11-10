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
    /// The window title last set: `set_window_title` is a system call, made only on a change.
    pub(crate) title: String,
}

/// Saved windows waiting until every workspace they show has its tabs loaded.
pub(crate) struct Restore {
    pub(crate) windows: Vec<detached::SavedWindow>,
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
        self.drop_hint = None;
        self.reorder = None;
        self.tab_drag = false;
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
                let bounds = ui.restore;
                Some(detached::SavedWindow {
                    bounds: detached::SavedBounds {
                        x: f32::from(bounds.origin.x),
                        y: f32::from(bounds.origin.y),
                        width: f32::from(bounds.size.width),
                        height: f32::from(bounds.size.height),
                    },
                    maximized: ui.maximized,
                    tabs: window.tabs.clone(),
                    active: window.active,
                })
            })
            .collect();
        let pending = self.restore.as_ref().map_or(&[][..], |r| &r.windows);
        let windows = detached::merge_saved(live, pending);
        match detached::save(&self.windows_path, &windows) {
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
        let windows = match detached::from_json(&text) {
            Ok(windows) if !windows.is_empty() => windows,
            Ok(_) => return,
            Err(error) => return tracing::warn!("ignoring the detached windows file: {error}"),
        };
        self.restore = Some(Restore { windows });
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
        restore.windows = std::mem::take(&mut restore.windows)
            .into_iter()
            .map(|saved| saved.keeping(|tab| listed.contains(&tab.project())))
            .filter(|saved| !saved.tabs.is_empty())
            .collect();
        detached::waiting_projects(&restore.windows, |project| self.tabs.contains_key(&project))
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
        let displays: Vec<Bounds<Pixels>> = cx.displays().iter().map(|d| d.bounds()).collect();
        let primary = cx.primary_display().map(|d| d.bounds());
        for saved in restore.windows {
            let saved = saved.keeping(|tab| {
                let DockTab::Terminal { project, tab_id } = *tab;
                self.tabs
                    .get(&project)
                    .is_some_and(|tabs| tabs.tab(tab_id).is_some())
            });
            let b = saved.bounds.clone();
            let maximized = saved.maximized;
            let Some(key) = self.detached.restore(&[saved]).first().copied() else {
                continue;
            };
            let bounds = Bounds::new(point(px(b.x), px(b.y)), size(px(b.width), px(b.height)));
            let bounds = mkrew_ui::window_place::fit_on_displays(bounds, &displays, primary);
            self.open_os_window(key, bounds, maximized, window, cx);
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
