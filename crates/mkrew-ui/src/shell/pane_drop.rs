//! Drag a tab (from the tab strip) or a pane (by its header) onto a pane to split it (SPEC §10):
//! the half of the pane under the pointer lights up, and the drop puts the dragged panes right
//! of it or below it, whichever side the pointer leans to.

use crate::*;

/// Payload of a pane header drag: the pane of `session`.
#[derive(Debug, Clone)]
pub(crate) struct DraggedPane {
    pub(crate) session: Ulid,
    pub(crate) title: SharedString,
}

/// Where a tab or pane dragged over a pane lands: beside `target`, right of it along
/// `Axis::Vertical`, below it along `Horizontal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PaneDrop {
    pub(crate) target: Ulid,
    pub(crate) axis: Axis,
}

impl Shell {
    /// Makes `header` (of `info`'s pane) draggable onto another pane.
    pub(crate) fn draggable_pane(
        &self,
        header: gpui::Div,
        info: &Session,
        theme: &Theme,
    ) -> gpui::AnyElement {
        let pane = DraggedPane {
            session: info.id,
            title: (run::pane_title(info, self.ports.get(info.id)))
                .unwrap_or_else(|| info.agent_profile.clone())
                .into(),
        };
        let style = PopupStyle::of(theme);
        let accent = theme.accent;
        header
            .id(SharedString::from(format!("pane-header-{}", info.id)))
            .on_drag(pane, move |pane, offset, _, cx| {
                let (style, title) = (style.clone(), pane.title.clone());
                cx.new(|_| DragChip {
                    style,
                    accent,
                    title,
                    offset,
                })
            })
            .into_any_element()
    }

    /// Makes the frame of `session`'s pane take dragged tabs and panes.
    pub(crate) fn pane_drop_target(
        &self,
        frame: gpui::Div,
        session: Ulid,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        frame
            .on_drag_move(
                cx.listener(move |shell, event: &DragMoveEvent<DraggedPane>, _, cx| {
                    let takes = event.drag(cx).session != session;
                    shell.hover_pane_drop(session, takes, event, cx);
                }),
            )
            .on_drag_move(
                cx.listener(move |shell, event: &DragMoveEvent<DraggedItem>, _, cx| {
                    let takes = (shell.dragged_tab(event.drag(cx)))
                        .is_some_and(|tab| !tab.contains(session) && tab.layout().is_some());
                    shell.hover_pane_drop(session, takes, event, cx);
                }),
            )
            .on_drop(cx.listener(move |shell, pane: &DraggedPane, window, cx| {
                shell.drop_pane(pane.session, session, window, cx);
            }))
            .on_drop(cx.listener(move |shell, item: &DraggedItem, window, cx| {
                shell.drop_tab_on_pane(item, session, window, cx);
            }))
    }

    /// The overlay on the half of `session`'s pane a drop would fill.
    pub(crate) fn pane_drop_overlay(
        &self,
        session: Ulid,
        theme: &Theme,
        cx: &Context<'_, Self>,
    ) -> Option<gpui::Div> {
        let drop = self
            .pane_drop
            .filter(|drop| drop.target == session && cx.has_active_drag())?;
        let half = div()
            .absolute()
            .rounded(px(radius::MEDIUM))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.accent))
            .bg(theme.tint(theme.accent));
        Some(match drop.axis {
            Axis::Vertical => half.top_0().bottom_0().right_0().w(relative(0.5)),
            Axis::Horizontal => half.left_0().right_0().bottom_0().h(relative(0.5)),
        })
    }

    /// The tab a tab-strip drag carries.
    fn dragged_tab(&self, item: &DraggedItem) -> Option<&Tab> {
        let ReorderList::Tabs(project_id, pinned) = item.list else {
            return None;
        };
        (self.tabs.get(&project_id)?.tabs().iter())
            .filter(|tab| tab.pinned == pinned)
            .nth(item.index)
    }

    /// A drag that `takes` (may drop on `target`'s pane) moved: over the pane it picks the side,
    /// off the pane it lets go of it.
    fn hover_pane_drop<T: 'static>(
        &mut self,
        target: Ulid,
        takes: bool,
        event: &DragMoveEvent<T>,
        cx: &mut Context<'_, Self>,
    ) {
        let (position, bounds) = (event.event.position, event.bounds);
        let next = if takes && bounds.contains(&position) {
            let at = position - bounds.origin;
            let axis = drop_axis(at.x / bounds.size.width, at.y / bounds.size.height);
            // Off the tab strip: no reorder line.
            if let Some(reorder) = &mut self.reorder {
                reorder.target = reorder.from;
                reorder.after = false;
            }
            Some(PaneDrop { target, axis })
        } else if self.pane_drop.is_some_and(|drop| drop.target == target) {
            None
        } else {
            return;
        };
        if self.pane_drop != next {
            self.pane_drop = next;
            cx.notify();
        }
    }

    /// The side of `target` the drop showed, taken.
    fn take_pane_drop(&mut self, target: Ulid, cx: &mut Context<'_, Self>) -> Option<Axis> {
        cx.notify();
        let drop = self.pane_drop.take()?;
        (drop.target == target).then_some(drop.axis)
    }

    /// Pane `moved` was dropped on `target`'s pane: it moves there, saved at once.
    fn drop_pane(
        &mut self,
        moved: Ulid,
        target: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(axis) = self.take_pane_drop(target, cx) else {
            return;
        };
        let Some(project_id) = self.workspace else {
            return;
        };
        let writes =
            (self.tabs.get_mut(&project_id)).and_then(|tabs| tabs.move_pane(moved, target, axis));
        for write in writes.into_iter().flatten() {
            self.save_layout(project_id, write, window, cx);
        }
        self.sync_panes(window, cx);
        self.focus_pane(window, cx);
    }

    /// A tab was dropped on `target`'s pane: its panes move there, then the emptied tab closes.
    fn drop_tab_on_pane(
        &mut self,
        item: &DraggedItem,
        target: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.reorder = None;
        let Some(axis) = self.take_pane_drop(target, cx) else {
            return;
        };
        let ReorderList::Tabs(project_id, _) = item.list else {
            return;
        };
        let Some(source) = self.dragged_tab(item).map(|tab| tab.id) else {
            return;
        };
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(writes) =
            (self.tabs.get_mut(&project_id)).and_then(|tabs| tabs.merge_tab(source, target, axis))
        else {
            return;
        };
        self.sync_panes(window, cx);
        self.focus_pane(window, cx);
        self.tab_error = None;
        // One after another, the close last: `tab.close` removes the sessions of the tab's saved
        // layout (ADR 0041), so it must only run once that layout is empty.
        cx.spawn_in(window, async move |this, cx| {
            for write in writes {
                let saved = connection.call::<()>(Request::TabSetLayout(write.clone()));
                if let Err(error) = saved.await {
                    tracing::warn!(tab_id = %write.tab_id, "tab.set_layout failed: {error:#}");
                    let _ = this.update(cx, |shell, cx| {
                        if let Some(tabs) = shell.tabs.get_mut(&project_id) {
                            tabs.write_failed(&write);
                        }
                        shell.tab_error = Some(format!("Cannot save the layout: {error:#}"));
                        cx.notify();
                    });
                    return;
                }
            }
            let closed =
                (connection.call::<()>(Request::TabClose(TabRef { tab_id: source }))).await;
            let _ = this.update_in(cx, |shell, window, cx| {
                match closed {
                    Ok(()) => {
                        if let Some(tabs) = shell.tabs.get_mut(&project_id) {
                            tabs.remove(source);
                        }
                        shell.sync_panes(window, cx);
                    }
                    Err(error) => {
                        tracing::warn!(tab_id = %source, "tab.close failed: {error:#}");
                        shell.tab_error = Some(format!("Cannot close the tab: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
