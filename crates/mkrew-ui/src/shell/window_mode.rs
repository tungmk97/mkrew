//! Space windows (ADR 0115): open, close and restore them. Each shows one workspace through
//! `Shell::render_space`; mode 1 has one, mode 2 one per open workspace.

use crate::shell::hosts::HostUi;
use crate::shell::space_view::SpaceWindow;
use crate::*;
use mkrew_ui::window_mode::{
    Choice, OpenTarget, PickOutcome, PickRow, SplitPlan, WorkspacePicker, need_you, open_target,
    plan_combine,
};

/// What runs where an opened workspace lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Then {
    Nothing,
    /// The session's pane comes forward (placed next to the focused pane if not shown).
    Session(Ulid),
}

/// The "Open <workspace>" question of mode 2 (spec 4.2), drawn in the window that asked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OpenAsk {
    pub(crate) project: Ulid,
    pub(crate) host: HostId,
    pub(crate) then: Then,
    /// Under the switcher when it asked from there, else centered.
    pub(crate) at: Option<Point<Pixels>>,
    /// "Don't ask again" is ticked.
    pub(crate) remember: bool,
}

/// What runs in a Space window once it opened.
pub(crate) type AfterOpen = Box<dyn FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>)>;

/// The options of a Space window: `main()`'s first one and every one opened later.
pub(crate) fn space_window_options(
    bounds: WindowBounds,
    title: SharedString,
    ui_size: f32,
) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(bounds),
        titlebar: Some(TitlebarOptions {
            title: Some(title),
            appears_transparent: CUSTOM_TITLE_BAR,
            traffic_light_position: theme::traffic_light_pos(Os::CURRENT, ui_size)
                .map(|(x, y)| point(px(x), px(y))),
        }),
        window_min_size: Some(size(
            px(theme::WINDOW_MIN_WIDTH),
            px(theme::WINDOW_MIN_HEIGHT),
        )),
        // macOS: we move the window from our own bar, so AppKit must not delay the click.
        app_owns_titlebar_drag: cfg!(target_os = "macos"),
        ..Default::default()
    }
}

impl Shell {
    /// Runs `f` in the primary Space window; `None` once no Space window is left.
    pub(crate) fn with_primary<R>(
        this: &gpui::WeakEntity<Shell>,
        cx: &mut gpui::AsyncApp,
        f: impl FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>) -> R,
    ) -> Option<R> {
        let handle = this
            .read_with(cx, |shell, _| {
                (shell.primary()).and_then(|key| shell.space_handles.get(&key).copied())
            })
            .ok()??;
        handle
            .update(cx, |_, window, cx| {
                this.update(cx, |shell, cx| f(shell, window, cx)).ok()
            })
            .ok()?
    }

    /// Runs a reply's `f` in the window that asked (`cx` of `spawn_in`), or in the primary
    /// Space window once that one is gone: in mode 2 a window closes while its requests are in
    /// flight, and the busy marks they clear must not stay set. `None` once no window is left.
    pub(crate) fn answer<R>(
        this: &gpui::WeakEntity<Shell>,
        cx: &mut gpui::AsyncWindowContext,
        f: impl FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>) -> R,
    ) -> Option<R> {
        if cx.window_handle().update(cx, |_, _, _| ()).is_ok() {
            return this.update_in(cx, f).ok();
        }
        Shell::with_primary(this, cx, f)
    }

    /// Opens a Space window showing `project`. Its `HostUi` exists at once, so `space_of`
    /// already finds it while the OS window opens (a second request focuses it). `then` runs
    /// in it once open.
    pub(crate) fn open_space_window(
        &mut self,
        project: Option<Ulid>,
        bounds: Bounds<Pixels>,
        placement: Placement,
        then: Option<AfterOpen>,
        cx: &mut Context<'_, Self>,
    ) -> SpaceKey {
        let key = SpaceKey(self.next_space);
        self.next_space += 1;
        let ui = HostUi {
            bounds,
            restore: bounds,
            placement,
            workspace: project,
            right_panel: self.panel_default,
            visible: true,
            ..HostUi::default()
        };
        self.hosts.insert(HostId::Space(key), ui);
        self.windows_dirty = true;
        let shell = cx.entity();
        let mode = cx.theme().mode;
        let ui_size = cx.global::<theme::Fonts>().ui_size;
        cx.spawn(async move |this, cx| {
            let options =
                space_window_options(placement.window_bounds(bounds), "Mkrew".into(), ui_size);
            let opened = cx.open_window(options, |window, cx| {
                theme::refresh(mode, window, cx);
                cx.new(|cx| SpaceWindow::new(shell, key, window, cx))
            });
            match opened {
                Ok(handle) => {
                    let handle: gpui::AnyWindowHandle = handle.into();
                    let _ = handle.update(cx, |_, window, cx| {
                        let _ = this.update(cx, |shell, cx| {
                            shell.space_opened(key, handle, window, cx);
                            if let Some(project) = project {
                                // As if it switched there: tabs, Git and Files load.
                                shell.hosts.entry(HostId::Space(key)).or_default().workspace = None;
                                shell.activate_workspace(HostId::Space(key), project, window, cx);
                            }
                            if let Some(then) = then {
                                then(shell, window, cx);
                            }
                            let pending = (shell.hosts.get(&HostId::Space(key)))
                                .map(|ui| ui.pending.take())
                                .unwrap_or_default();
                            for f in pending {
                                f(shell, window, cx);
                            }
                        });
                    });
                }
                Err(error) => {
                    let _ = this.update(cx, |shell, cx| {
                        tracing::warn!("cannot open a window: {error:#}");
                        shell.hosts.remove(&HostId::Space(key));
                        shell.windows_dirty = true;
                        let name = project.map_or("Mkrew", |id| shell.workspace_name(id));
                        let text = format!("Cannot open a window for {name}: {error:#}");
                        shell.toast_error(text, cx);
                    });
                }
            }
        })
        .detach();
        key
    }

    /// The OS window of Space window `key` exists: it is known, used last, and gets the badge.
    pub(crate) fn space_opened(
        &mut self,
        key: SpaceKey,
        handle: gpui::AnyWindowHandle,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.spaces.insert(handle, key);
        self.space_handles.insert(key, handle);
        self.activated(HostId::Space(key));
        self.record_bounds(HostId::Space(key), window);
        self.desktop.attach(window);
        self.sync_panes(window, cx);
        cx.notify();
    }

    /// Forgets Space window `key` (its OS window is closing or gone): its popovers close,
    /// nothing touches its HWND any more, and its workspace's watchers may go.
    fn forget_space(&mut self, key: SpaceKey, window: &Window, cx: &mut Context<'_, Self>) {
        if let Some(handle) = self.space_handles.remove(&key) {
            self.spaces.remove(&handle);
        }
        self.forget_host(HostId::Space(key), cx);
        if self
            .open_ask
            .is_some_and(|ask| ask.host == HostId::Space(key))
        {
            self.open_ask = None;
        }
        self.desktop.detach(window);
        self.sync_watches(cx);
        cx.notify();
    }

    /// Closes Space window `key` from code (a workspace that went away, a combine).
    pub(crate) fn close_space_window(&mut self, key: SpaceKey, cx: &mut Context<'_, Self>) {
        self.in_window(HostId::Space(key), cx, move |shell, window, cx| {
            shell.forget_space(key, window, cx);
            window.remove_window();
        });
    }

    /// Mode 2: the Space windows whose workspace is no longer listed close, but the last open
    /// one (the caller shows the default workspace there). Returns the closed ones.
    pub(crate) fn prune_spaces(&mut self, cx: &mut Context<'_, Self>) -> Vec<SpaceKey> {
        if self.window_mode != WindowMode::PerWorkspace {
            return Vec::new();
        }
        let projects = self.sidebar.projects();
        let spaces: Vec<(SpaceKey, Option<Ulid>)> = self.spaces_shown().collect();
        let mut gone: Vec<SpaceKey> = (spaces.iter())
            .filter(|(_, shown)| shown.is_some_and(|id| projects.iter().all(|p| p.id != id)))
            .map(|(key, _)| *key)
            .collect();
        if gone.len() == spaces.len() {
            gone.pop();
        }
        for key in &gone {
            self.close_space_window(*key, cx);
        }
        gone
    }

    /// The ✕ of Space window `key`: another Space window stays, so only this one goes; the
    /// last one does what the old main window's ✕ did (hide to the tray, ask about unsaved
    /// edits, or save and end the app).
    pub(crate) fn space_should_close(
        &mut self,
        key: SpaceKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        if self.space_handles.keys().any(|other| *other != key) {
            self.forget_space(key, window, cx);
            return true;
        }
        self.last_window_should_close(window, cx)
    }

    /// The saved Space windows of `window_mode = per_workspace` (ADR 0115), but the last one,
    /// whose workspace the open first window shows: each opens with its workspace while that
    /// is listed, in the saved order (the one used longest ago first), and the first window,
    /// the one used last, gets the keyboard back once they are up.
    pub(crate) fn restore_spaces(&mut self, cx: &mut Context<'_, Self>) {
        let saved = std::mem::take(&mut self.saved_spaces);
        let Some((_, others)) = saved.split_last() else {
            return;
        };
        let displays: Vec<Bounds<Pixels>> =
            cx.displays().iter().map(|d| d.visible_bounds()).collect();
        let primary = cx.primary_display().map(|d| d.visible_bounds());
        let projects = self.sidebar.projects();
        let mut seen: HashSet<Ulid> = HashSet::new();
        let opening: Vec<&detached::SavedSpace> = (others.iter())
            .filter(|saved| projects.iter().any(|p| p.id == saved.workspace))
            .filter(|saved| self.space_of(saved.workspace).is_none())
            .filter(|saved| seen.insert(saved.workspace))
            .collect();
        let first = self.primary_host();
        let last = opening.len();
        for (step, saved) in (1..).zip(opening) {
            let (placement, bounds) = Placement::of(window_place::restored(
                window_place::saved_rect(&saved.bounds),
                Placement::from_flags(saved.maximized, saved.fullscreen),
                size(px(theme::WINDOW_MIN_WIDTH), px(theme::WINDOW_MIN_HEIGHT)),
                &displays,
                primary,
            ));
            let then = (step == last).then(|| -> AfterOpen {
                Box::new(move |shell, _, cx| {
                    shell.in_window(first, cx, |_, window, _| window.activate_window());
                })
            });
            self.open_space_window(Some(saved.workspace), bounds, placement, then, cx);
        }
    }

    /// Opens `project` the way the window mode says (spec 4.2): mode 1 switches the window
    /// that asked (the primary one from a detached window); mode 2 focuses its window, opens
    /// a new one, replaces the asking window's workspace, or asks. `then` runs where it lands.
    /// `ctrl` takes the other choice than the saved one.
    pub(crate) fn route_workspace(
        &mut self,
        project: Ulid,
        then: Then,
        ctrl: bool,
        at: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let host = self.space_host(window);
        let spaces: Vec<(SpaceKey, Option<Ulid>)> = self.spaces_shown().collect();
        // An empty window (no workspace yet) takes it without a question.
        let target = if self.workspace_of(host).is_none() && self.space_of(project).is_none() {
            OpenTarget::Replace
        } else {
            open_target(project, self.window_mode, self.open_pref, ctrl, &spaces)
        };
        match target {
            OpenTarget::Switch | OpenTarget::Replace => {
                self.show_in(host, project, then, window, cx)
            }
            OpenTarget::Focus(key) => {
                self.in_host(HostId::Space(key), window, cx, move |shell, window, cx| {
                    shell.run_then(project, then, window, cx);
                });
            }
            OpenTarget::New => self.open_new_space(host, project, then, cx),
            OpenTarget::Ask => {
                self.close_menus();
                self.open_ask = Some(OpenAsk {
                    project,
                    host,
                    then,
                    at,
                    remember: false,
                });
                self.dialog_serial += 1;
                self.in_host(host, window, cx, |shell, window, cx| {
                    window.focus(&shell.dialog_focus, cx);
                    cx.notify();
                });
            }
        }
    }

    /// Space window `host` shows `project`, comes forward and runs `then`.
    fn show_in(
        &mut self,
        host: HostId,
        project: Ulid,
        then: Then,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.in_host(host, window, cx, move |shell, window, cx| match then {
            Then::Nothing => shell.activate_workspace(host, project, window, cx),
            Then::Session(session) => shell.show_session_here(project, session, window, cx),
        });
    }

    /// A new Space window for `project`, cascading from `from`, running `then` once open.
    fn open_new_space(
        &mut self,
        from: HostId,
        project: Ulid,
        then: Then,
        cx: &mut Context<'_, Self>,
    ) {
        let (bounds, placement) = self.new_space_bounds(from, 1, cx);
        let after: AfterOpen =
            Box::new(move |shell, window, cx| shell.run_then(project, then, window, cx));
        self.open_space_window(Some(project), bounds, placement, Some(after), cx);
    }

    fn run_then(
        &mut self,
        project: Ulid,
        then: Then,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Then::Session(session) = then {
            self.show_session_here(project, session, window, cx);
        }
    }

    /// The Space window `window` shows the pane of `session` of `project_id`: focused, or
    /// placed next to the focused pane of the workspace's active tab (revealed once its tabs
    /// load).
    pub(crate) fn show_session_here(
        &mut self,
        project_id: Ulid,
        session: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.views.insert(project_id, View::Terminals);
        if self.tabs.contains_key(&project_id) {
            self.place_session(project_id, session, None, Axis::Vertical, window, cx);
        } else {
            self.reveal = Some(session);
        }
        self.activate_workspace(self.space_host(window), project_id, window, cx);
    }

    /// Runs `f` in `host`'s window: at once when that is `window`, else after activating it.
    pub(crate) fn in_host(
        &mut self,
        host: HostId,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
        f: impl FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>) + 'static,
    ) {
        if self.host_of(window) == host {
            return f(self, window, cx);
        }
        self.in_window(host, cx, move |shell, window, cx| {
            window.activate_window();
            f(shell, window, cx);
        });
    }

    /// A Space window draws the sidebar: mode 1 only (ADR 0115), unless hidden.
    pub(crate) fn sidebar_drawn(&self) -> bool {
        self.window_mode == WindowMode::Single && !self.sidebar_layout.hidden()
    }

    /// Whether something of `project` asked in `window` runs there: a Space window that
    /// shows it, or can (mode 1's single window switches; mode 2's empty one takes it).
    pub(crate) fn acts_here(&self, project: Ulid, window: &Window) -> bool {
        let host = self.host_of(window);
        host.space().is_some()
            && (self.window_mode == WindowMode::Single
                || self.workspace_of(host).is_none_or(|shown| shown == project))
    }

    /// Runs `f` in a Space window showing `project`, making one show it first, for something
    /// the user asked inside that workspace (a file, a diff, a checkout, a tab moved back):
    /// the window of `project`; else mode 1's single window (or an empty one of mode 2)
    /// switches to it; else mode 2 opens a new one without asking.
    pub(crate) fn in_workspace(
        &mut self,
        project: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
        f: impl FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>) + 'static,
    ) {
        let here = self.space_host(window);
        let host = match self.space_of(project) {
            Some(key) => HostId::Space(key),
            None if self.window_mode == WindowMode::Single || self.workspace_of(here).is_none() => {
                here
            }
            None => {
                let (bounds, placement) = self.new_space_bounds(here, 1, cx);
                self.open_space_window(Some(project), bounds, placement, Some(Box::new(f)), cx);
                return;
            }
        };
        self.in_host(host, window, cx, move |shell, window, cx| {
            if shell.workspace_of(host) != Some(project) {
                shell.activate_workspace(host, project, window, cx);
            }
            f(shell, window, cx);
        });
    }

    /// Where the `step`-th new window goes: the size of `from`, offset `step` × 32 px right
    /// and down, windowed, kept on a connected screen.
    pub(crate) fn new_space_bounds(
        &self,
        from: HostId,
        step: usize,
        cx: &App,
    ) -> (Bounds<Pixels>, Placement) {
        let fallback = Bounds::new(
            point(px(0.0), px(0.0)),
            size(px(theme::WINDOW_WIDTH), px(theme::WINDOW_HEIGHT)),
        );
        let base = self.hosts.get(&from).map_or(fallback, |ui| ui.restore);
        let offset = px(space::XXL * step as f32);
        let moved = Bounds::new(
            point(base.origin.x + offset, base.origin.y + offset),
            base.size,
        );
        let displays: Vec<Bounds<Pixels>> =
            cx.displays().iter().map(|d| d.visible_bounds()).collect();
        let primary = cx.primary_display().map(|d| d.visible_bounds());
        let (placement, bounds) = Placement::of(window_place::restored(
            moved,
            Placement::Windowed,
            size(px(theme::WINDOW_MIN_WIDTH), px(theme::WINDOW_MIN_HEIGHT)),
            &displays,
            primary,
        ));
        (bounds, placement)
    }

    /// The question's answer: `choice` runs (a new window, or this window shows it), and
    /// "Don't ask again" saves it as `[ui] open_workspace`.
    pub(crate) fn answer_open_ask(
        &mut self,
        choice: Choice,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(ask) = self.open_ask.take() else {
            return;
        };
        if ask.remember {
            self.open_pref = choice.remembered();
            let value = format!("\"{}\"", self.open_pref.name());
            if let Err(error) =
                mkrew_core::settings::write_ui_value(&self.settings_file, "open_workspace", &value)
            {
                self.settings_write_failed("the open workspace choice", error, cx);
            }
        }
        self.focus_pane(window, cx);
        match choice {
            Choice::NewWindow => self.open_new_space(ask.host, ask.project, ask.then, cx),
            Choice::ThisWindow => self.show_in(ask.host, ask.project, ask.then, window, cx),
        }
        cx.notify();
    }

    pub(crate) fn cancel_open_ask(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.open_ask = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    fn toggle_open_ask_remember(&mut self, cx: &mut Context<'_, Self>) {
        if let Some(ask) = self.open_ask.as_mut() {
            ask.remember = !ask.remember;
        }
        cx.notify();
    }

    /// Tab moves between the check row and the buttons (Space or Enter presses the focused
    /// one); otherwise Enter opens a new window, Space ticks "Don't ask again", Esc cancels.
    pub(crate) fn open_ask_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.dialog_nav(event, true, window, cx, |_, _| {}) {
            return;
        }
        cx.stop_propagation();
        if event.keystroke.modifiers.modified() {
            return;
        }
        match event.keystroke.key.as_str() {
            "enter" => self.answer_open_ask(Choice::NewWindow, window, cx),
            "space" => self.toggle_open_ask_remember(cx),
            "escape" => self.cancel_open_ask(window, cx),
            _ => {}
        }
    }

    /// "Open <workspace>": under the switcher when asked from there, else centered.
    pub(crate) fn render_open_ask(
        &self,
        ask: &OpenAsk,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let check = div()
            .id("open-ask-remember")
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(radius::SMALL))
            .text_size(font_size::BODY)
            .text_color(rgb(theme.text))
            .focus_stop(theme)
            .pressable(Look::Row, true, theme)
            .on_press(
                true,
                cx.listener(|shell, _, _, cx| shell.toggle_open_ask_remember(cx)),
            )
            .child(
                div()
                    .size(px(space::M))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(radius::SMALL))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(theme.control_border))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.accent))
                    .child(if ask.remember { "✓" } else { "" }),
            )
            .child(div().flex_1().child("Don't ask again"));
        let buttons = div()
            .flex()
            .justify_end()
            .gap(px(space::S))
            .child(
                button("open-ask-this", "This window", theme, false)
                    .focus_stop(theme)
                    .tooltip(tooltip(theme, "Show it in this window", "Ctrl+click"))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.answer_open_ask(Choice::ThisWindow, window, cx);
                    })),
            )
            .child(
                button("open-ask-new", "New window", theme, true)
                    .focus_stop_filled(theme)
                    .tooltip(tooltip(theme, "Open it in a window of its own", "Enter"))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.answer_open_ask(Choice::NewWindow, window, cx);
                    })),
            );
        let panel = floating_panel("open-ask", theme)
            .w(px(theme::CONFIRM_DIALOG_WIDTH))
            .p(px(space::L))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .font_family(theme.ui_font_family.clone())
            .dialog_root(
                &self.dialog_focus,
                false,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::open_ask_key_down),
            )
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(format!("Open {}", self.workspace_name(ask.project))),
            )
            .child(check)
            .child(buttons);
        match ask.at {
            Some(at) => div()
                .id("open-ask-layer")
                .absolute()
                .inset_0()
                .bg(gpui::rgba(theme.scrim))
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|shell, _, window, cx| {
                        cx.stop_propagation();
                        shell.cancel_open_ask(window, cx);
                    }),
                )
                .child(deferred(
                    anchored()
                        .position(at)
                        .snap_to_window_with_margin(px(space::S))
                        .child(
                            panel
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
                        ),
                ))
                .into_any_element(),
            None => Self::modal_shade("open-ask-shade", panel, cx),
        }
    }

    /// "Open workspaces in windows" (spec 4.1); with one workspace or none the mode changes
    /// at once.
    pub(crate) fn open_split_picker(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let host = self.space_host(window);
        let shown = self.workspace_of(host);
        let listed = self.shown_workspaces(host);
        if listed.len() <= 1 {
            let keep = shown
                .or_else(|| listed.first().copied())
                .unwrap_or_default();
            return self.split_into_windows(SplitPlan { keep, open: vec![] }, window, cx);
        }
        let projects = self.sidebar.projects();
        let sessions = self.sidebar.sessions();
        let count = |project: Ulid, state: SessionState| {
            (sessions.iter())
                .filter(|s| {
                    s.project_id == project && s.kind == SessionKind::Agent && s.state == state
                })
                .count()
        };
        let rows = (listed.iter())
            .filter_map(|id| projects.iter().find(|project| project.id == *id))
            .map(|project| PickRow {
                id: project.id,
                name: project.name.clone(),
                color: project.color.clone(),
                working: count(project.id, SessionState::Working),
                asking: count(project.id, SessionState::NeedsInput),
            })
            .collect();
        self.close_menus();
        self.split_picker = Some(WorkspacePicker::new(rows, shown));
        self.palette_host = host;
        self.dialog_serial += 1;
        self.in_host(host, window, cx, |shell, window, cx| {
            window.focus(&shell.dialog_focus, cx);
            cx.notify();
        });
    }

    pub(crate) fn split_picker_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.dialog_nav(event, true, window, cx, |_, _| {}) {
            return;
        }
        cx.stop_propagation();
        let Some(picker) = self.split_picker.as_mut() else {
            return;
        };
        match picker.key(&event.keystroke) {
            PickOutcome::Close => return self.close_split_picker(window, cx),
            PickOutcome::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    picker.insert(&text);
                }
            }
            PickOutcome::Done(plan) => {
                self.close_split_picker(window, cx);
                self.split_into_windows(plan, window, cx);
            }
            PickOutcome::Edited => {}
        }
        cx.notify();
    }

    fn close_split_picker(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.split_picker = None;
        self.palette_host = self.primary_host();
        self.focus_pane(window, cx);
        cx.notify();
    }

    /// The primary button or Enter: the confirmation step first with several ticked.
    fn confirm_split_picker(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(picker) = self.split_picker.as_mut() else {
            return;
        };
        if let PickOutcome::Done(plan) = picker.confirm() {
            self.close_split_picker(window, cx);
            self.split_into_windows(plan, window, cx);
        }
        cx.notify();
    }

    /// The asking window shows `plan.keep`; every other ticked workspace opens in a new
    /// window, cascading from it; the mode is saved.
    pub(crate) fn split_into_windows(
        &mut self,
        plan: SplitPlan,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let host = self.space_host(window);
        self.set_window_mode(WindowMode::PerWorkspace, cx);
        if !plan.keep.is_nil() && self.workspace_of(host) != Some(plan.keep) {
            self.in_host(host, window, cx, move |shell, window, cx| {
                shell.activate_workspace(host, plan.keep, window, cx);
            });
        }
        let open: Vec<Ulid> = (plan.open.into_iter())
            .filter(|project| self.space_of(*project).is_none())
            .collect();
        let last = open.len();
        for (step, project) in (1..).zip(open) {
            let (bounds, placement) = self.new_space_bounds(host, step, cx);
            // The asking window keeps the keyboard once the new ones are up.
            let then = (step == last).then(|| -> AfterOpen {
                Box::new(move |shell, _, cx| {
                    shell.in_window(host, cx, |_, window, _| window.activate_window());
                })
            });
            self.open_space_window(Some(project), bounds, placement, then, cx);
        }
    }

    /// Mode 1 again (spec 4.3): the asking window keeps its workspace and shows the sidebar;
    /// every other Space window closes. Detached windows stay.
    pub(crate) fn combine_windows(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let HostId::Space(keep) = self.space_host(window) else {
            return;
        };
        let spaces: Vec<SpaceKey> = self.spaces_shown().map(|(key, _)| key).collect();
        let closing = plan_combine(keep, &spaces);
        for key in &closing {
            self.close_space_window(*key, cx);
        }
        self.set_window_mode(WindowMode::Single, cx);
        if !closing.is_empty() {
            self.toast_info(format!("Combined {} windows", closing.len() + 1), cx);
        }
        cx.notify();
    }

    /// Sets and saves `[ui] window_mode`.
    pub(crate) fn set_window_mode(&mut self, mode: WindowMode, cx: &mut Context<'_, Self>) {
        self.window_mode = mode;
        let value = format!("\"{}\"", mode.name());
        if let Err(error) =
            mkrew_core::settings::write_ui_value(&self.settings_file, "window_mode", &value)
        {
            self.settings_write_failed("the window mode", error, cx);
        }
        self.windows_dirty = true;
        // Every Space window draws again: sidebar and title follow the mode.
        cx.notify();
    }

    /// The picker: a filter, one row per workspace (its ✓, avatar, name and what its agents
    /// do), then Cancel and the primary button; or the confirmation step with Back.
    pub(crate) fn render_split_picker(
        &self,
        picker: &WorkspacePicker,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let body = if picker.confirming() {
            let text = picker.confirmation(|id| self.workspace_name(id).to_owned());
            div()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text))
                .child(text)
                .into_any_element()
        } else {
            let rows = (picker.visible().into_iter().enumerate()).map(|(index, row)| {
                let id = row.id;
                let color = row.color.as_deref().and_then(|c| theme.workspace_color(c));
                let meta = mkrew_ui::sidebar::workspace_summary(row.working, row.asking, 0);
                list_row(
                    ("split-row", index),
                    index == picker.cursor() && picker.in_list(),
                    theme,
                )
                .tooltip(tooltip(theme, "Tick or untick", "Space"))
                .on_click(cx.listener(move |shell, _, _, cx| {
                    if let Some(picker) = shell.split_picker.as_mut() {
                        picker.toggle(id);
                    }
                    cx.notify();
                }))
                .child(
                    div()
                        .size(px(space::M))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(radius::SMALL))
                        .border(px(theme::BORDER_WIDTH))
                        .border_color(rgb(theme.control_border))
                        .text_size(font_size::META)
                        .text_color(rgb(theme.accent))
                        .child(if picker.is_ticked(id) { "✓" } else { "" }),
                )
                .child(super::sidebar::workspace_avatar(&row.name, color, theme))
                .child(div().flex_1().min_w_0().truncate().child(row.name.clone()))
                .children(meta.map(|meta| {
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(space::XS))
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(status_dot(
                            if row.asking > 0 {
                                theme.status_needs_input
                            } else {
                                theme.status_working
                            },
                            false,
                        ))
                        .child(meta)
                }))
            });
            div()
                .flex()
                .flex_col()
                .gap(px(space::S))
                .child(search_box(
                    caret_field(
                        "split-filter",
                        picker.filter_input(),
                        "Filter workspaces",
                        self.dialog_focus.is_focused(window) && !picker.in_list(),
                        theme,
                        cx,
                        |shell, caret| {
                            if let Some(picker) = shell.split_picker.as_mut() {
                                picker.click(caret.at, caret.extend, caret.clicks);
                            }
                        },
                    ),
                    None,
                    self.dialog_focus.is_focused(window),
                    theme,
                ))
                .child(
                    div()
                        .id("split-rows")
                        .max_h(px(theme::SEND_SESSIONS_HEIGHT))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children(rows),
                )
                .child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child("↑↓ pick · Space ticks · Alt+A ticks all · Enter opens"),
                )
                .into_any_element()
        };
        let label = picker.button_label();
        let primary = if picker.can_confirm() {
            button("split-open", label, theme, true)
                .focus_stop_filled(theme)
                .tooltip(tooltip(theme, "Open the ticked workspaces", "Enter"))
                .on_click(
                    cx.listener(|shell, _, window, cx| shell.confirm_split_picker(window, cx)),
                )
                .into_any_element()
        } else {
            disabled_button("split-open", label, theme, true)
                .tooltip(tooltip(theme, "Tick a workspace", ""))
                .into_any_element()
        };
        let secondary = if picker.confirming() {
            button("split-back", "Back", theme, false)
                .focus_stop(theme)
                .tooltip(tooltip(theme, "Back to the list", "Esc"))
                .on_click(cx.listener(|shell, _, _, cx| {
                    if let Some(picker) = shell.split_picker.as_mut() {
                        picker.back();
                    }
                    cx.notify();
                }))
        } else {
            button("split-cancel", "Cancel", theme, false)
                .focus_stop(theme)
                .tooltip(tooltip(theme, "Stay in one window", "Esc"))
                .on_click(cx.listener(|shell, _, window, cx| shell.close_split_picker(window, cx)))
        };
        let panel = floating_panel("split-picker", theme)
            .w(px(theme::DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .font_family(theme.ui_font_family.clone())
            .dialog_root(
                &self.dialog_focus,
                true,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::split_picker_key_down),
            )
            .relative()
            .child(self.typing(&self.dialog_focus, Self::split_picker_key_down, cx))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Open workspaces in windows"),
            )
            .child(body)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(space::S))
                    .pt(px(space::S))
                    .child(secondary)
                    .child(primary),
            );
        Self::modal_shade("split-picker-shade", panel, cx)
    }

    /// The title bar chip of a Space window in mode 2 (ADR 0115): sessions of the other
    /// workspaces that need the user, `● N need you · <place>`; it never shrinks (the tab area
    /// gives way first). A click lists them. `None` while no other workspace needs the user.
    pub(crate) fn render_need_you_chip(
        &self,
        host: HostId,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        if self.window_mode != WindowMode::PerWorkspace || host.space().is_none() {
            return None;
        }
        let need = need_you(&self.inbox_entries(), self.workspace_of(host))?;
        let open = self.need_you_menu && self.menu_host == host;
        let chip = div()
            .id("need-you")
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .h(theme::rems_px(theme::CONTROL_HEIGHT))
            .px(px(space::S))
            .rounded(px(radius::PILL))
            .bg(rgb(theme.warning_bg))
            .text_size(font_size::META)
            .text_color(rgb(theme.status_needs_input))
            .pressable_on(Look::Chip, theme.warning_bg, theme)
            // Not while its menu is open: the tooltip would cover the rows.
            .when(!open, |el| {
                el.tooltip(tooltip(
                    theme,
                    "Sessions in other workspaces need you",
                    self.keymap.label(Command::NextInbox),
                ))
            })
            .on_press(
                true,
                cx.listener(|shell, _, window, cx| shell.open_need_you(window, cx)),
            )
            .child(status_dot(theme.status_needs_input, false))
            .child(need.label);
        let menu = (open && !self.modal_open()).then(|| {
            render_menu(
                self.menu_state(),
                "need-you-menu",
                self.need_you_rows(host),
                None,
                Place::Dropdown,
                theme,
                cx,
            )
        });
        Some(
            div()
                .flex_none()
                .child(chip)
                .children(menu)
                .into_any_element(),
        )
    }

    pub(crate) fn open_need_you(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.begin_menu(window, cx);
        self.need_you_menu = true;
        self.menu_host = self.host_of(window);
        self.finish_menu(window.last_input_was_keyboard(), window, cx);
    }

    /// One row per session of another workspace that needs the user, newest first; picking
    /// one shows the session where it lives (`open_session`: its torn-off window, else its
    /// workspace's window the way the window mode says).
    pub(crate) fn need_you_rows(&self, host: HostId) -> Rows {
        let Some(need) = need_you(&self.inbox_entries(), self.workspace_of(host)) else {
            return Rows::new();
        };
        let projects = self.sidebar.projects();
        (need.rows.into_iter())
            .map(|row| {
                let project = projects.iter().find(|project| project.id == row.project);
                let session = row.session;
                MenuItem::new(
                    row.text,
                    act(move |shell, window, cx| shell.open_session(session, window, cx)),
                )
                .lead(Lead::Avatar {
                    name: project.map_or_else(String::new, |project| project.name.clone()),
                    color: project.and_then(|project| project.color.clone()),
                })
                .row()
            })
            .collect()
    }
}
