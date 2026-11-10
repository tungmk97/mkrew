//! Moving terminal tabs between windows and the keys of a detached window (ADR 0096).

use crate::shell::detached_view::DetachedWindow;
use crate::shell::hosts::HostUi;
use crate::*;

impl Shell {
    /// Shift+F4, the tab menu, the palette, or a drop outside every window (`at` = release
    /// point): `tab` gets a window of its own.
    pub(crate) fn move_tab_to_new_window(
        &mut self,
        tab: DockTab,
        at: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let source = self.host_of(window);
        let ui = self.hosts.entry(source).or_default();
        let (near, size) = (ui.bounds, ui.content.get());
        let displays: Vec<Bounds<Pixels>> = cx.displays().iter().map(|d| d.bounds()).collect();
        let bounds = mkrew_ui::window_place::new_window_bounds(at, size, near, &displays);
        let moved = self.detached.move_tab(tab, HostId::Main, None);
        let key = self.detached.open(tab);
        self.after_move(moved, cx);
        self.open_os_window(key, bounds, false, window, cx);
    }

    /// Opens the OS window of `key` (its tabs are in the model already). Off the current
    /// update: the new window draws `Shell`, which is busy until this update ends.
    pub(crate) fn open_os_window(
        &mut self,
        key: WindowKey,
        bounds: Bounds<Pixels>,
        maximized: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.hosts.insert(HostId::Detached(key), HostUi::default());
        self.windows_dirty = true;
        self.sync_panes_if_alive(window, cx);
        let shell = cx.entity();
        let mode = cx.theme().mode;
        cx.spawn(async move |this, cx| {
            let options = WindowOptions {
                window_bounds: Some(if maximized {
                    WindowBounds::Maximized(bounds)
                } else {
                    WindowBounds::Windowed(bounds)
                }),
                titlebar: Some(TitlebarOptions {
                    title: Some("Mkrew".into()),
                    appears_transparent: CUSTOM_TITLE_BAR,
                    traffic_light_position: cfg!(target_os = "macos").then(|| {
                        let (x, y) = theme::MAC_TRAFFIC_LIGHT_POS;
                        point(px(x), px(y))
                    }),
                }),
                window_min_size: Some(size(
                    px(theme::DETACHED_MIN_WIDTH),
                    px(theme::DETACHED_MIN_HEIGHT),
                )),
                app_owns_titlebar_drag: cfg!(target_os = "macos"),
                ..Default::default()
            };
            let opened = cx.open_window(options, |window, cx| {
                theme::refresh(mode, window, cx);
                cx.new(|cx| DetachedWindow::new(shell, key, window, cx))
            });
            match opened {
                Ok(handle) => {
                    let handle: gpui::AnyWindowHandle = handle.into();
                    let _ = handle.update(cx, |_, window, cx| {
                        let _ = this.update(cx, |shell, cx| {
                            shell.window_opened(key, handle, window, cx);
                        });
                    });
                }
                Err(error) => {
                    let _ = this.update(cx, |shell, cx| {
                        tracing::warn!("cannot open a window: {error:#}");
                        shell.detached.remove_window(key);
                        shell.hosts.remove(&HostId::Detached(key));
                        shell.toast_error(format!("Cannot open a window: {error:#}"), cx);
                    });
                }
            }
        })
        .detach();
    }

    /// The OS window of `key` exists: its panes move in, its pane gets the keyboard.
    pub(crate) fn window_opened(
        &mut self,
        key: WindowKey,
        handle: gpui::AnyWindowHandle,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.windows.insert(handle, key);
        self.window_handles.insert(key, handle);
        self.activated(HostId::Detached(key));
        self.record_bounds(HostId::Detached(key), window);
        self.rehost_window(key, window, cx);
        // After the handle is known: a pane of this window is built in it (`in_window`).
        self.sync_panes(window, cx);
        cx.notify();
    }

    /// `window` is still one of ours: not a detached window that `close_os_window` just let
    /// go (it is gone once the current update ends, and `host_of` reads it as the main one).
    fn sync_panes_if_alive(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let handle = window.window_handle();
        if handle == self.main_window || self.windows.contains_key(&handle) {
            self.sync_panes(window, cx);
        }
    }

    /// Every attached pane of `key`'s tabs now lives in `window`.
    fn rehost_window(&mut self, key: WindowKey, window: &mut Window, cx: &mut Context<'_, Self>) {
        for tab in self.detached.tabs_of(key) {
            self.rehost_tab(tab, window, cx);
        }
    }

    /// The attached panes of `tab` moved to `window`: they draw there, and their link clicks
    /// answer from there.
    pub(crate) fn rehost_tab(
        &mut self,
        tab: DockTab,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let DockTab::Terminal { project, tab_id } = tab;
        let sessions = (self.tabs.get(&project))
            .and_then(|tabs| tabs.tab(tab_id))
            .map(Tab::sessions)
            .unwrap_or_default();
        for session in sessions {
            let Some(pane) = self.panes.get(&session).cloned() else {
                continue;
            };
            pane.update(cx, |pane, cx| pane.rehost(window, cx));
            let link = cx.subscribe_in(
                &pane,
                window,
                move |shell, _, link: &OpenLink, window, cx| {
                    shell.open_terminal_link(session, link, window, cx);
                },
            );
            self.link_subscriptions.insert(session, link);
        }
    }

    /// Moves `tab` to `to` (strip `index`, else the end): its panes follow, `to` shows it and
    /// takes the keyboard. The main window also switches to the tab's workspace.
    pub(crate) fn move_tab_to(
        &mut self,
        tab: DockTab,
        to: HostId,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let moved = self.detached.move_tab(tab, to, index);
        self.after_move(moved, cx);
        let DockTab::Terminal { project, tab_id } = tab;
        if to == HostId::Main {
            // The strip shows the shown workspace's tabs: a drop between its chips lands
            // there; a tab of another workspace keeps its place.
            let params = index
                .filter(|_| self.workspace == Some(project))
                .and_then(|at| {
                    let tabs = self.tabs.get(&project)?;
                    let anchor = (tabs.tabs().iter())
                        .filter(|other| {
                            other.id != tab_id
                                && self.terminal_host(project, other.id) == HostId::Main
                        })
                        .nth(at)
                        .map(|other| other.id);
                    self.tabs.get_mut(&project)?.move_tab_before(tab_id, anchor)
                });
            if let Some(tabs) = self.tabs.get_mut(&project) {
                tabs.activate(tab_id);
            }
            self.views.insert(project, View::Terminals);
            if let Some(params) = params {
                self.send_edit(
                    Request::TabMove(params),
                    "Cannot move the tab",
                    |_: &mut Self, _: serde_json::Value| {},
                    window,
                    cx,
                );
            }
        }
        self.windows_dirty = true;
        self.in_window(to, cx, move |shell, window, cx| {
            shell.rehost_tab(tab, window, cx);
            if to == HostId::Main {
                shell.activate_workspace(project, window, cx);
            }
            shell.sync_panes(window, cx);
            window.activate_window();
            cx.notify();
        });
        self.sync_panes_if_alive(window, cx);
        cx.notify();
    }

    /// "Move all back": every tab of `key` returns to the main window, in strip order.
    pub(crate) fn move_all_back(
        &mut self,
        key: WindowKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let tabs = self.detached.tabs_of(key);
        let last = tabs.last().copied();
        for tab in &tabs {
            let moved = self.detached.move_tab(*tab, HostId::Main, None);
            self.after_move(moved, cx);
        }
        let Some(last) = last else {
            return;
        };
        // The last one is shown; the others wait in the main strip, their panes with them.
        self.in_window(HostId::Main, cx, move |shell, window, cx| {
            for tab in tabs {
                shell.rehost_tab(tab, window, cx);
            }
        });
        self.move_tab_to(last, HostId::Main, None, window, cx);
    }

    /// A move left a window empty: its OS window goes.
    fn after_move(&mut self, moved: detached::Moved, cx: &mut Context<'_, Self>) {
        if let Some(key) = moved.emptied {
            self.close_os_window(key, cx);
        }
    }

    /// The moves a tab menu offers (spec §4.3): to the main window (from a detached one), to a
    /// new window (not for the only tab of a detached window) and to every other detached
    /// window, numbered as the windows are.
    pub(crate) fn move_rows(&self, tab: DockTab) -> Rows {
        let host = self.menu_host;
        let move_to = |label: String, to: HostId| {
            MenuItem::new(
                label,
                act(move |shell, window, cx| shell.move_tab_to(tab, to, None, window, cx)),
            )
        };
        let mut rows: Rows = Vec::new();
        if host != HostId::Main {
            rows.push(move_to("Move to main window".into(), HostId::Main).row());
        }
        let alone = matches!(host, HostId::Detached(key) if self.detached.tabs_of(key).len() < 2);
        rows.push(
            MenuItem::new(
                "Move to new window",
                act(move |shell, window, cx| {
                    shell.move_tab_to_new_window(tab, None, window, cx);
                }),
            )
            .keys(self.keymap.label(Command::MoveTabToNewWindow))
            .enabled(!alone)
            .row(),
        );
        for (at, other) in self.detached.windows().iter().enumerate() {
            let to = HostId::Detached(other.key);
            if to != host {
                rows.push(move_to(format!("Move to window {}", at + 1), to).row());
            }
        }
        rows
    }

    /// The ✕ of a detached window (spec §5.4): its tabs close for real (one question for the
    /// running agents, ADR 0041). The window goes once its last tab did (`prune_detached`).
    pub(crate) fn detached_should_close(
        &mut self,
        key: WindowKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let mut by_project: HashMap<Ulid, Vec<Ulid>> = HashMap::new();
        for DockTab::Terminal { project, tab_id } in self.detached.tabs_of(key) {
            by_project.entry(project).or_default().push(tab_id);
        }
        for (project, tab_ids) in by_project {
            self.close_tabs(project, tab_ids, window, cx);
        }
        false
    }

    /// The keys of a detached window: only [`detached::handles`]' commands act, on this
    /// window's tab; the rest do nothing here (spec §4.2).
    pub(crate) fn run_detached(
        &mut self,
        key: WindowKey,
        command: Command,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if !detached::handles(command) || (self.reconnecting() && !self.modal_open()) {
            return;
        }
        let menu_was_open = self.menu_open();
        self.dismiss_menus(window, cx);
        self.finish_rename(true, window, cx);
        if command == Command::CommandPalette && self.palette.is_some() {
            return self.close_palette(window, cx);
        }
        if self.modal_open() {
            cx.propagate();
            return;
        }
        // The composer keeps its keys (Ctrl+Shift+Enter would read as Ctrl+Enter: send).
        if self.composer_focus.is_focused(window) && self.composer_session(window).is_some() {
            if command != Command::OpenComposer {
                cx.propagate();
            }
            return;
        }
        let Some((project, tab_id)) = self.tab_scope(window) else {
            return;
        };
        let tab = DockTab::Terminal { project, tab_id };
        match command {
            Command::NextTab => self.detached.cycle(key, true),
            Command::PreviousTab => self.detached.cycle(key, false),
            Command::Tab(n) => {
                self.detached.select(key, usize::from(n) - 1);
            }
            Command::MoveTabLeft | Command::MoveTabRight => {
                let tabs = self.detached.tabs_of(key);
                if let Some(at) = tabs.iter().position(|t| *t == tab) {
                    let to = if command == Command::MoveTabLeft {
                        at.checked_sub(1)
                    } else {
                        (at + 1 < tabs.len()).then_some(at + 1)
                    };
                    if let Some(to) = to {
                        self.detached.move_within(key, at, to);
                    }
                }
            }
            Command::CloseTab => self.close_tabs(project, vec![tab_id], window, cx),
            Command::SplitRight => self.split(Axis::Vertical, window, cx),
            Command::SplitDown => self.split(Axis::Horizontal, window, cx),
            Command::SplitAgent => self.new_pane(SessionKind::Agent, Axis::Vertical, window, cx),
            Command::SplitShell => self.new_pane(SessionKind::Shell, Axis::Vertical, window, cx),
            Command::Layout(preset) => self.apply_layout(preset, window, cx),
            Command::EqualizePanes | Command::Resize(_) => {
                let write = match command {
                    Command::Resize(direction) => {
                        self.with_tab(window, |tabs| tabs.resize_focused(direction))
                    }
                    _ => self.with_tab(window, Tabs::equalize_active),
                };
                if let Some(write) = write.flatten() {
                    self.save_layout(project, write, window, cx);
                }
            }
            Command::ZoomPane => {
                self.with_tab(window, Tabs::toggle_zoom);
            }
            Command::Focus(direction) => {
                self.with_tab(window, |tabs| tabs.focus_neighbour(direction));
            }
            Command::ClosePane => self.close_focused_pane(window, cx),
            Command::RenamePane => {
                if let Some(session) = self.focused_session(window) {
                    self.start_pane_rename(session, window, cx);
                }
            }
            Command::RenameTab => self.start_rename(window, cx),
            Command::OpenComposer => self.open_composer(window, cx),
            Command::CommandPalette => self.open_palette(window, cx),
            Command::ContextMenu => {
                if !menu_was_open && let Some(session) = self.focused_session(window) {
                    let at = self.menu_anchor(&Anchor::PaneHeader(session), window, cx);
                    self.open_pane_menu(session, at, window, cx);
                }
            }
            Command::ToggleFullScreen => window.toggle_fullscreen(),
            // Already alone: nothing to do.
            Command::MoveTabToNewWindow if self.detached.tabs_of(key).len() > 1 => {
                self.move_tab_to_new_window(tab, None, window, cx);
            }
            _ => {}
        }
        self.windows_dirty = true;
        self.sync_panes_if_alive(window, cx);
        cx.notify();
    }
}
