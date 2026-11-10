//! A terminal tab released outside its window (ADR 0096). gpui-pre-windows keeps the mouse
//! captured from button down to up, so the source window gets the release wherever it is and
//! decides; other windows see no hover, so `Shell.drop_hint` paints their insertion line.

use crate::*;
use mkrew_ui::window_place::{self, Placed, Strip, Target};

impl Shell {
    /// The terminal tab a strip drag carries, from the reorder state its `on_drag` set.
    pub(crate) fn dragged_dock_tab(&self) -> Option<DockTab> {
        let reorder = self.reorder?;
        match reorder.list {
            ReorderList::Tabs(project, pinned) => {
                let tab = (self.tabs.get(&project)?.tabs().iter())
                    .filter(|tab| tab.pinned == pinned)
                    .nth(reorder.from)?;
                Some(DockTab::Terminal {
                    project,
                    tab_id: tab.id,
                })
            }
            ReorderList::Detached(key) => self.detached.tabs_of(key).get(reorder.from).copied(),
            _ => None,
        }
    }

    /// Every Mkrew window with its screen bounds and strip, most recently activated first.
    fn placed_windows(&self) -> Vec<Placed<HostId>> {
        (self.activation.iter())
            .filter_map(|host| {
                let ui = self.hosts.get(host)?;
                Some(Placed {
                    host: *host,
                    window: ui.bounds,
                    strip: ui.strip.borrow().clone(),
                })
            })
            .collect()
    }

    /// `position` (window-local) in screen space. `Window::bounds` is the client rect on
    /// Windows (gpui-pre-windows `WM_MOVE` / `WM_SIZE`), the space elements paint in.
    fn screen_point(position: Point<Pixels>, window: &Window) -> Point<Pixels> {
        window.bounds().origin + position
    }

    fn release_target(&self, screen: Point<Pixels>, window: &Window) -> Option<Target<HostId>> {
        window_place::drop_target(screen, self.host_of(window), &self.placed_windows())
    }

    fn tear_off_move(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let hint = (self.dragged_dock_tab().is_some())
            .then(|| self.release_target(Self::screen_point(position, window), window))
            .flatten()
            .and_then(|target| match target {
                Target::Strip { host, index } => Some((host, index)),
                _ => None,
            });
        if self.drop_hint != hint {
            self.drop_hint = hint;
            cx.notify();
        }
    }

    fn tear_off_release(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        self.drop_hint = None;
        let Some(tab) = self.dragged_dock_tab() else {
            return false;
        };
        let screen = Self::screen_point(position, window);
        let Some(target) = self.release_target(screen, window) else {
            return false;
        };
        self.reorder = None;
        match target {
            Target::Strip { host, index } => self.move_tab_to(tab, host, Some(index), window, cx),
            Target::Window(host) => self.move_tab_to(tab, host, None, window, cx),
            Target::NewWindow(_) => self.move_tab_to_new_window(tab, Some(screen), window, cx),
        }
        true
    }

    /// The accent line of the drop in `host`'s strip, if the drag hovers it: before chip slot
    /// `index`, or after the last chip. A child of the (relative) strip.
    pub(crate) fn drop_hint_line(&self, host: HostId, theme: &Theme) -> Option<gpui::Div> {
        let (_, index) = self.drop_hint.filter(|(hint, _)| *hint == host)?;
        let strip = self.hosts.get(&host)?.strip.borrow().clone()?;
        let line = window_place::drop_line(&strip, index, space::XS)?;
        Some(
            div()
                .absolute()
                .rounded_full()
                .bg(rgb(theme.accent))
                .left(line.origin.x)
                .top(line.origin.y)
                .w(line.size.width)
                .h(line.size.height),
        )
    }
}

/// Watches the whole window during a tab drag (capture phase, before every element): pointer
/// moves update the drop hint; a release outside the window moves the tab.
pub(crate) fn tear_off_catcher(cx: &Context<'_, Shell>) -> impl IntoElement + use<> {
    let view = cx.entity();
    canvas(
        |_, _, _| (),
        move |_, (), window, _| {
            let moving = view.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture && cx.has_active_drag() {
                    moving.update(cx, |shell, cx| {
                        shell.tear_off_move(event.position, window, cx);
                    });
                }
            });
            let releasing = view.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture || !cx.has_active_drag() {
                    return;
                }
                let moved = releasing.update(cx, |shell, cx| {
                    shell.tear_off_release(event.position, window, cx)
                });
                if moved {
                    cx.stop_propagation();
                }
            });
        },
    )
    .absolute()
    .size_full()
}

/// Records the strip (`None`) or chip slot `index`'s bounds, window-local, at prepaint. Slots
/// of tabs that are not drawn stay empty (`Bounds::default()`).
pub(crate) fn record_strip(
    strip: Rc<RefCell<Option<Strip>>>,
    chip: Option<usize>,
) -> impl IntoElement {
    canvas(
        move |bounds, _, _| {
            let mut strip = strip.borrow_mut();
            let recorded = strip.get_or_insert_with(|| Strip {
                bounds,
                chips: Vec::new(),
            });
            match chip {
                None => {
                    recorded.bounds = bounds;
                    recorded.chips.clear();
                }
                Some(index) => {
                    if recorded.chips.len() <= index {
                        recorded.chips.resize(index + 1, Bounds::default());
                    }
                    recorded.chips[index] = bounds;
                }
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}
