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
    /// `window`'s own bounds are read live (the recorded ones lag a move or resize).
    fn placed_windows(&self, window: &Window) -> Vec<Placed<HostId>> {
        let source = self.host_of(window);
        (self.activation.iter())
            .filter_map(|host| {
                let ui = self.hosts.get(host)?;
                Some(Placed {
                    host: *host,
                    window: if *host == source {
                        window.bounds()
                    } else {
                        ui.bounds
                    },
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

    /// Where `tab` released at `screen` lands. In the main strip a tab of a workspace that is
    /// not shown goes to the end of its group (its chips are not there); a shown one keeps to
    /// its pin group.
    fn release_target(
        &self,
        tab: DockTab,
        screen: Point<Pixels>,
        window: &Window,
    ) -> Option<Target<HostId>> {
        let target =
            window_place::drop_target(screen, self.host_of(window), &self.placed_windows(window))?;
        let (
            Target::Strip {
                host: HostId::Main,
                index,
            },
            DockTab::Terminal { project, tab_id },
        ) = (target, tab)
        else {
            return Some(target);
        };
        let Some(tabs) = self
            .tabs
            .get(&project)
            .filter(|_| self.workspace == Some(project))
        else {
            return Some(Target::Window(HostId::Main));
        };
        let shown = || {
            (tabs.tabs().iter()).filter(|other| {
                other.id != tab_id && self.terminal_host(project, other.id) == HostId::Main
            })
        };
        let pinned = tabs.tab(tab_id).is_some_and(|tab| tab.pinned);
        let pinned_chips = shown().filter(|other| other.pinned).count();
        Some(Target::Strip {
            host: HostId::Main,
            index: window_place::clamp_to_group(index, pinned_chips, shown().count(), pinned),
        })
    }

    fn tear_off_move(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let hint = self
            .dragged_dock_tab()
            .and_then(|tab| self.release_target(tab, Self::screen_point(position, window), window))
            .and_then(|target| match target {
                Target::Strip { host, index } => Some((host, index)),
                _ => None,
            });
        if self.drop_hint != hint {
            self.drop_hint = hint;
            cx.notify();
        }
    }

    /// The drag is over: no hint, no stale reorder state for the next (unrelated) drag.
    pub(crate) fn end_tab_drag(&mut self, cx: &mut Context<'_, Self>) {
        let dirty = self.drop_hint.take().is_some() | self.reorder.take().is_some();
        self.tab_drag = false;
        if dirty {
            cx.notify();
        }
    }

    /// A tab drag released: moves the tab when it ended outside its window's own handlers.
    fn tear_off_release(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let tab = self.dragged_dock_tab().filter(|_| self.tab_drag);
        let screen = Self::screen_point(position, window);
        let target = tab.and_then(|tab| self.release_target(tab, screen, window));
        let (Some(tab), Some(target)) = (tab, target) else {
            // Inside its own window: the on_drop handlers run (a handled drop ends the drag),
            // else the bubble listener ends it.
            if self.drop_hint.take().is_some() {
                cx.notify();
            }
            return false;
        };
        self.end_tab_drag(cx);
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

/// Watches the whole window during a tab drag: pointer moves of a `DraggedItem` drag (gpui
/// checks the payload type; sidebar grips, panes, cards and dividers never reach it) update
/// the drop hint; a release outside the window moves the tab. The mouse-up hooks: capture
/// (before every element) decides the move; bubble ends the drag unless a handled `on_drop`
/// stopped propagation first (`drop_reordered` / `drop_tab_on_pane` end it themselves).
pub(crate) fn tear_off_catcher(cx: &Context<'_, Shell>) -> impl IntoElement + use<> {
    let view = cx.entity();
    div()
        .absolute()
        .size_full()
        .on_drag_move(
            cx.listener(|shell, event: &DragMoveEvent<DraggedItem>, window, cx| {
                shell.tab_drag = true;
                shell.tear_off_move(event.event.position, window, cx);
            }),
        )
        .child(
            canvas(
                |_, _, _| (),
                move |_, (), window, _| {
                    let releasing = view.clone();
                    window.on_mouse_event(
                        move |event: &MouseUpEvent, phase, window, cx| match phase {
                            DispatchPhase::Capture => {
                                if !cx.has_active_drag() {
                                    return;
                                }
                                let moved = releasing.update(cx, |shell, cx| {
                                    shell.tear_off_release(event.position, window, cx)
                                });
                                if moved {
                                    cx.stop_propagation();
                                }
                            }
                            DispatchPhase::Bubble => {
                                releasing.update(cx, |shell, cx| shell.end_tab_drag(cx));
                            }
                        },
                    );
                },
            )
            .absolute()
            .size_full(),
        )
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
