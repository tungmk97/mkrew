//! Drag to reorder, one look for the sidebar's rows and the tab strip: a small chip follows the
//! pointer, the dragged item stays dim in its place and an accent line marks the gap it lands in
//! (the half of the item under the pointer picks the side).

use crate::*;

/// The list an item moves in; it never leaves its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReorderList {
    /// A workspace's AGENTS, TERMINAL or RUN rows.
    Rows(Ulid, RowGroup),
    /// A workspace's pinned (`true`) or unpinned tabs.
    Tabs(Ulid, bool),
}

/// Payload of a reorder drag: the `index`-th item of `list`.
#[derive(Debug, Clone)]
pub(crate) struct DraggedItem {
    pub(crate) list: ReorderList,
    pub(crate) index: usize,
    pub(crate) title: SharedString,
}

/// The reorder drag in progress: the pointer is over item `target` of `list`, on its `after`
/// half.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Reorder {
    pub(crate) list: ReorderList,
    pub(crate) from: usize,
    pub(crate) target: usize,
    pub(crate) after: bool,
}

impl Reorder {
    /// The dragged item's new index; `None` while the gap is right beside it.
    fn to(self) -> Option<usize> {
        sidebar::drop_index(self.from, self.target + usize::from(self.after))
    }
}

impl Shell {
    /// Wraps `item`'s element so it can be dragged within its list. `horizontal`: the list runs
    /// left to right (tabs); `gap`: the space between its items, where the drop line is centered.
    pub(crate) fn reorderable(
        &self,
        element: gpui::AnyElement,
        item: DraggedItem,
        horizontal: bool,
        gap: f32,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let (list, index) = (item.list, item.index);
        let current = self
            .reorder
            .filter(|reorder| reorder.list == list && cx.has_active_drag());
        let dragging = current.is_some_and(|reorder| reorder.from == index);
        let line = current
            .filter(|reorder| reorder.target == index && reorder.to().is_some())
            .map(|reorder| reorder.after);
        let view = cx.entity().downgrade();
        let style = PopupStyle::of(theme);
        let accent = theme.accent;
        // Centered in the gap: half of it plus half the line outside the item's edge.
        let outside = px(-(gap + theme::DROP_LINE_WIDTH) / 2.0);
        div()
            .id(SharedString::from(format!("reorder-{list:?}-{index}")))
            .relative()
            .when(dragging, |el| el.opacity(opacity::DRAG_SOURCE))
            .on_drag(item, move |item, offset, _, cx| {
                let start = Reorder {
                    list: item.list,
                    from: item.index,
                    target: item.index,
                    after: false,
                };
                let _ = view.update(cx, |shell, cx| {
                    shell.reorder = Some(start);
                    cx.notify();
                });
                let (style, title) = (style.clone(), item.title.clone());
                cx.new(|_| DragChip {
                    style,
                    accent,
                    title,
                    offset,
                })
            })
            .on_drag_move(cx.listener(
                move |shell, event: &DragMoveEvent<DraggedItem>, window, cx| {
                    let (position, bounds) = (event.event.position, event.bounds);
                    let dragged = event.drag(cx);
                    if dragged.list != list || !bounds.contains(&position) {
                        return;
                    }
                    let from = dragged.index;
                    cx.set_active_drag_cursor_style(CursorStyle::ClosedHand, window);
                    let center = bounds.center();
                    let next = Reorder {
                        list,
                        from,
                        target: index,
                        after: if horizontal {
                            position.x > center.x
                        } else {
                            position.y > center.y
                        },
                    };
                    if shell.reorder != Some(next) {
                        shell.reorder = Some(next);
                        cx.notify();
                    }
                },
            ))
            .child(element)
            .children(line.map(|after| {
                let bar = div().absolute().rounded_full().bg(rgb(accent));
                let width = px(theme::DROP_LINE_WIDTH);
                match (horizontal, after) {
                    (true, false) => bar.top_0().bottom_0().w(width).left(outside),
                    (true, true) => bar.top_0().bottom_0().w(width).right(outside),
                    (false, false) => bar.left_0().right_0().h(width).top(outside),
                    (false, true) => bar.left_0().right_0().h(width).bottom(outside),
                }
            }))
            .into_any_element()
    }

    /// A reorder drag was released over its list (the sidebar or the tab strip): the item moves
    /// to the gap the line showed.
    pub(crate) fn drop_reordered(
        &mut self,
        item: &DraggedItem,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let reorder = self.reorder.take();
        cx.notify();
        let Some(to) = reorder
            .filter(|reorder| reorder.list == item.list && reorder.from == item.index)
            .and_then(Reorder::to)
        else {
            return;
        };
        match item.list {
            ReorderList::Rows(project_id, group) => {
                self.drop_row(project_id, group, item.index, to, window, cx);
            }
            ReorderList::Tabs(project_id, pinned) => {
                self.drop_tab(project_id, pinned, item.index, to, window, cx);
            }
        }
    }

    /// The `from`-th tab of `project_id`'s pinned or unpinned ones goes to place `to` among
    /// them: shown at once, saved with `tab.move`.
    fn drop_tab(
        &mut self,
        project_id: Ulid,
        pinned: bool,
        from: usize,
        to: usize,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(tabs) = self.tabs.get_mut(&project_id) else {
            return;
        };
        let tab_id = (tabs.tabs().iter())
            .filter(|tab| tab.pinned == pinned)
            .nth(from)
            .map(|tab| tab.id);
        let Some(params) = tab_id.and_then(|tab_id| tabs.move_tab(tab_id, to)) else {
            return;
        };
        self.send_edit(
            Request::TabMove(params),
            "Cannot move the tab",
            |_: &mut Self, _: serde_json::Value| {},
            window,
            cx,
        );
    }
}

/// What follows the pointer during a reorder drag: the item's title on a raised chip, starting
/// just left of the pointer wherever the item was grabbed.
pub(crate) struct DragChip {
    style: PopupStyle,
    accent: u32,
    title: SharedString,
    /// Where the pointer grabbed the item, from its top-left corner.
    offset: Point<Pixels>,
}

impl Render for DragChip {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = &self.style;
        div()
            .pl(px((f32::from(self.offset.x) - space::L).max(0.0)))
            .child(
                div()
                    .max_w(theme::rems_px(theme::TITLE_CHIP_MAX_WIDTH))
                    .px(px(space::S))
                    .py(px(space::XS))
                    .rounded(px(radius::MEDIUM))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(self.accent))
                    .bg(rgb(theme.surface_raised))
                    .shadow_md()
                    .font_family(theme.ui_font_family.clone())
                    .text_size(font_size::BODY)
                    .font_weight(FontWeight(weight::MEDIUM))
                    .text_color(rgb(theme.text))
                    .truncate()
                    .child(self.title.clone()),
            )
    }
}
