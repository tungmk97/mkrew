//! Space windows (ADR 0113): open, close and restore them. Each shows one workspace through
//! `Shell::render_space`; mode 1 has one, mode 2 one per open workspace.

use crate::shell::hosts::HostUi;
use crate::shell::space_view::SpaceWindow;
use crate::*;

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
        self.desktop.detach(window);
        self.sync_watches(cx);
        cx.notify();
    }

    /// The ✕ of Space window `key`: another Space window stays, so only this one goes; the
    /// last one does what the main window's ✕ did (hide to the tray, ask about unsaved
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

    /// The saved Space windows of `window_mode = per_workspace` (ADR 0113), but the last one,
    /// whose workspace the open first window shows: each opens with its workspace while that
    /// is listed.
    pub(crate) fn restore_spaces(&mut self, cx: &mut Context<'_, Self>) {
        let saved = std::mem::take(&mut self.saved_spaces);
        let Some((_, others)) = saved.split_last() else {
            return;
        };
        let displays: Vec<Bounds<Pixels>> =
            cx.displays().iter().map(|d| d.visible_bounds()).collect();
        let primary = cx.primary_display().map(|d| d.visible_bounds());
        for saved in others {
            let listed = self
                .sidebar
                .projects()
                .iter()
                .any(|p| p.id == saved.workspace);
            if !listed || self.space_of(saved.workspace).is_some() {
                continue;
            }
            let (placement, bounds) = Placement::of(window_place::restored(
                window_place::saved_rect(&saved.bounds),
                Placement::from_flags(saved.maximized, saved.fullscreen),
                size(px(theme::WINDOW_MIN_WIDTH), px(theme::WINDOW_MIN_HEIGHT)),
                &displays,
                primary,
            ));
            self.open_space_window(Some(saved.workspace), bounds, placement, None, cx);
        }
    }
}
