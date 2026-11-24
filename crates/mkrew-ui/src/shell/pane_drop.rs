//! Drops on panes and on the tab strip (SPEC §10, §20B.5).
//! A Files tree row, a tab (from the tab area) or a pane (by the grip on its top edge) dragged
//! over a pane lights the half of the pane on the side of the edge nearest to the pointer; the
//! drop puts the dragged leaf there.
//! One handler set serves every drag type ([`PaneDragged`]); the pane frame and its drag strip
//! both carry it, because a drop only reaches the element under the pointer.

use crate::*;

/// Payload of a pane drag: the pane `key` of workspace `project_id`.
#[derive(Debug, Clone)]
pub(crate) struct DraggedPane {
    pub(crate) project_id: Ulid,
    pub(crate) key: PaneKey,
    pub(crate) title: SharedString,
}

/// Where a drag over a pane lands: on the `edge` side of `target`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PaneDrop {
    pub(crate) target: PaneKey,
    pub(crate) edge: Edge,
}

/// A drag payload that may land on a pane.
pub(crate) trait PaneDragged: Sized + 'static {
    /// Whether the drag may drop on `target`'s pane of workspace `project_id`.
    fn takes(&self, shell: &Shell, project_id: Ulid, target: &PaneKey) -> bool;

    /// The drop: the dragged leaf goes to the `edge` side of `target`.
    fn land(
        &self,
        shell: &mut Shell,
        project_id: Ulid,
        target: PaneKey,
        edge: Edge,
        window: &mut Window,
        cx: &mut Context<'_, Shell>,
    );

    /// Runs on every release over the pane, before `land`, also when nothing lands.
    fn ended(&self, _shell: &mut Shell, _cx: &mut Context<'_, Shell>) {}
}

impl PaneDragged for DraggedPane {
    fn takes(&self, shell: &Shell, project_id: Ulid, target: &PaneKey) -> bool {
        self.project_id == project_id
            && self.key != *target
            && (shell.tabs.get(&project_id)).is_some_and(|tabs| tabs.tab_with(&self.key).is_some())
    }

    fn land(
        &self,
        shell: &mut Shell,
        project_id: Ulid,
        target: PaneKey,
        edge: Edge,
        window: &mut Window,
        cx: &mut Context<'_, Shell>,
    ) {
        shell.drop_pane(project_id, &self.key, &target, edge, window, cx);
    }
}

impl PaneDragged for DraggedItem {
    fn takes(&self, shell: &Shell, project_id: Ulid, target: &PaneKey) -> bool {
        shell.dragged_tab(self).is_some_and(|(project, tab)| {
            project == project_id && !tab.contains(target) && tab.layout().is_some()
        })
    }

    fn land(
        &self,
        shell: &mut Shell,
        _project_id: Ulid,
        target: PaneKey,
        edge: Edge,
        window: &mut Window,
        cx: &mut Context<'_, Shell>,
    ) {
        shell.drop_tab_on_pane(self, &target, edge, window, cx);
    }

    fn ended(&self, shell: &mut Shell, cx: &mut Context<'_, Shell>) {
        shell.end_tab_drag(cx);
    }
}

impl PaneDragged for DraggedPath {
    /// Files only: a folder row opens nothing on a pane (it still drops on a composer).
    fn takes(&self, _shell: &Shell, project_id: Ulid, target: &PaneKey) -> bool {
        !self.is_dir
            && self.project_id == project_id
            && !matches!(target, PaneKey::File(file) if *file == self.file())
    }

    fn land(
        &self,
        shell: &mut Shell,
        project_id: Ulid,
        target: PaneKey,
        edge: Edge,
        window: &mut Window,
        cx: &mut Context<'_, Shell>,
    ) {
        shell.drop_leaf(
            project_id,
            Some((target, edge)),
            Leaf::of_file(self.file()),
            window,
            cx,
        );
    }
}

/// The element id of a pane's frame: it scopes the fixed ids of what the pane draws, so two
/// panes of one kind (two files) never share a click state.
pub(crate) fn pane_id(key: &PaneKey) -> SharedString {
    SharedString::from(format!("pane-{key:?}"))
}

/// The element id (and hover group) of a pane's grip.
pub(crate) fn grip_id(key: &PaneKey) -> SharedString {
    SharedString::from(format!("pane-grip-{key:?}"))
}

impl Shell {
    /// What the chip of a dragged pane says: a session's title, a file's name, "Diff".
    fn pane_title(&self, key: &PaneKey) -> SharedString {
        match key {
            PaneKey::Session(id) => (self.sidebar.sessions().iter())
                .find(|s| s.id == *id)
                .map(|info| {
                    run::pane_title(info, self.ports.get(info.id))
                        .unwrap_or_else(|| info.agent_profile.clone())
                })
                .unwrap_or_default()
                .into(),
            PaneKey::File(file) => (file.path.rsplit('/').next().unwrap_or(&file.path))
                .to_owned()
                .into(),
            PaneKey::Diff => "Diff".into(),
            PaneKey::Web(id) => (self.browsers.get(id))
                .map_or("New browser", |browser| browser.page.label())
                .to_owned()
                .into(),
        }
    }

    /// Makes `handle` (the top-edge strip of the pane `key`) draggable onto another pane.
    pub(crate) fn draggable_pane(
        &self,
        handle: gpui::Div,
        project_id: Ulid,
        key: &PaneKey,
        theme: &Theme,
    ) -> gpui::Stateful<gpui::Div> {
        let pane = DraggedPane {
            project_id,
            key: key.clone(),
            title: self.pane_title(key),
        };
        let style = PopupStyle::of(theme);
        let accent = theme.accent;
        handle
            .id(grip_id(key))
            .on_drag(pane, move |pane, offset, _, cx| {
                let (style, title) = (style.clone(), pane.title.clone());
                cx.new(|_| DragChip {
                    style,
                    accent,
                    title,
                    offset,
                })
            })
    }

    /// Makes the frame of the pane `key` take every drag: the half that lights up, the drops, and
    /// the OS files (`drop_hover` + `drop_catcher`).
    /// Call it before the frame gets its other children: the catcher must paint first.
    pub(crate) fn pane_drop_target(
        &self,
        frame: gpui::Div,
        project_id: Ulid,
        key: &PaneKey,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let frame = Self::hover_of::<DraggedPane, _>(frame, project_id, key, cx);
        let frame = Self::hover_of::<DraggedItem, _>(frame, project_id, key, cx);
        let frame = Self::hover_of::<DraggedPath, _>(frame, project_id, key, cx);
        let frame = Self::hover_of::<DraggedNew, _>(frame, project_id, key, cx);
        let frame = Self::pane_drops(frame, project_id, key, cx);
        let target = DropTarget::Pane(key.clone());
        frame
            .on_drag_move::<ExternalPaths>(cx.listener({
                let target = target.clone();
                move |shell, event: &DragMoveEvent<ExternalPaths>, _, cx| {
                    shell.hover_os_drop(&target, event, cx);
                }
            }))
            .child(drop_catcher(target, cx))
    }

    /// The drops of every drag type on the pane `key`: one listener per payload type.
    /// The pane frame has them, and so does every element above the pane that blocks the mouse
    /// (the drag strip): a drop only reaches the element under the pointer.
    pub(crate) fn pane_drops<E: gpui::InteractiveElement>(
        el: E,
        project_id: Ulid,
        key: &PaneKey,
        cx: &mut Context<'_, Self>,
    ) -> E {
        let el = Self::drop_of::<DraggedPane, E>(el, project_id, key, cx);
        let el = Self::drop_of::<DraggedItem, E>(el, project_id, key, cx);
        let el = Self::drop_of::<DraggedPath, E>(el, project_id, key, cx);
        Self::drop_of::<DraggedNew, E>(el, project_id, key, cx)
    }

    fn hover_of<T: PaneDragged, E: gpui::InteractiveElement>(
        el: E,
        project_id: Ulid,
        key: &PaneKey,
        cx: &mut Context<'_, Self>,
    ) -> E {
        let key = key.clone();
        el.on_drag_move(cx.listener(move |shell, event: &DragMoveEvent<T>, _, cx| {
            let takes = event.drag(cx).takes(shell, project_id, &key);
            shell.hover_pane_drop(&key, takes, event, cx);
        }))
    }

    fn drop_of<T: PaneDragged, E: gpui::InteractiveElement>(
        el: E,
        project_id: Ulid,
        key: &PaneKey,
        cx: &mut Context<'_, Self>,
    ) -> E {
        let key = key.clone();
        el.on_drop(cx.listener(move |shell, dragged: &T, window, cx| {
            dragged.ended(shell, cx);
            let edge = shell.take_pane_drop(&key);
            cx.notify();
            // No drop while a dialog is open.
            if let Some(edge) = edge.filter(|_| !shell.modal_open()) {
                dragged.land(shell, project_id, key.clone(), edge, window, cx);
            }
        }))
    }

    /// The overlay on the half of the pane `key` a drop would fill: the side of the edge nearest
    /// to the pointer.
    pub(crate) fn pane_drop_overlay(
        &self,
        key: &PaneKey,
        theme: &Theme,
        cx: &Context<'_, Self>,
    ) -> Option<gpui::Div> {
        let drop =
            (self.pane_drop.as_ref()).filter(|drop| drop.target == *key && cx.has_active_drag())?;
        let half = mkrew_ui::drop_zone::edge_half(drop.edge);
        Some(
            div()
                .absolute()
                .left(relative(half.x as f32))
                .top(relative(half.y as f32))
                .w(relative(half.w as f32))
                .h(relative(half.h as f32))
                .rounded(px(radius::MEDIUM))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.accent))
                .bg(theme.tint(theme.accent)),
        )
    }

    /// The workspace and tab a tab-strip drag carries.
    fn dragged_tab(&self, item: &DraggedItem) -> Option<(Ulid, &Tab)> {
        let ReorderList::Tabs(project_id, pinned) = item.list else {
            return None;
        };
        let tab = (self.tabs.get(&project_id)?.tabs().iter())
            .filter(|tab| tab.pinned == pinned)
            .nth(item.index)?;
        Some((project_id, tab))
    }

    /// A drag that `takes` (may drop on `target`'s pane) moved: over the pane it picks the edge,
    /// off the pane it lets go of it. Nothing lights up while a dialog is open.
    fn hover_pane_drop<T: 'static>(
        &mut self,
        target: &PaneKey,
        takes: bool,
        event: &DragMoveEvent<T>,
        cx: &mut Context<'_, Self>,
    ) {
        let (position, bounds) = (event.event.position, event.bounds);
        let next = if takes && !self.modal_open() && bounds.contains(&position) {
            let at = position - bounds.origin;
            // Off the tab area: no reorder line.
            if let Some(reorder) = &mut self.reorder {
                reorder.target = reorder.from;
                reorder.after = false;
            }
            Some(PaneDrop {
                target: target.clone(),
                edge: Edge::nearest(at.x / bounds.size.width, at.y / bounds.size.height),
            })
        } else if (self.pane_drop.as_ref()).is_some_and(|drop| drop.target == *target) {
            None
        } else {
            return;
        };
        if self.pane_drop != next {
            self.pane_drop = next;
            cx.notify();
        }
    }

    /// An OS file drag moved over `target` (a pane or the tab strip): it is the drop target
    /// while the pointer is inside and no dialog covers it (see `Shell::drop_hover`).
    pub(crate) fn hover_os_drop(
        &mut self,
        target: &DropTarget,
        event: &DragMoveEvent<ExternalPaths>,
        cx: &mut Context<'_, Self>,
    ) {
        // A modal dialog covers the pane: its own box takes the drop, and this catcher
        // (painted first) must not take the slot from it.
        if !self.modal_open() && event.bounds.contains(&event.event.position) {
            self.drop_hover = Some((target.clone(), event.drag(cx).clone()));
        } else if (self.drop_hover.as_ref()).is_some_and(|(hovered, _)| hovered == target) {
            self.drop_hover = None;
        }
    }

    /// The edge of `target` the drop showed, taken.
    pub(crate) fn take_pane_drop(&mut self, target: &PaneKey) -> Option<Edge> {
        let drop = self.pane_drop.take()?;
        (drop.target == *target).then_some(drop.edge)
    }

    /// Pane `moved` was dropped on the `edge` of `target`: it moves there, whole leaf, saved at
    /// once. A move never asks about unsaved edits: the file stays open.
    pub(crate) fn drop_pane(
        &mut self,
        project_id: Ulid,
        moved: &PaneKey,
        target: &PaneKey,
        edge: Edge,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        // A drop in a detached window leaves the main window's active tab alone.
        let detached = (self.tabs.get(&project_id))
            .and_then(|tabs| tabs.tab_with(target))
            .map(|tab| tab.id)
            .filter(|tab_id| self.terminal_host(project_id, *tab_id) != HostId::Main);
        let writes = (self.tabs.get_mut(&project_id)).and_then(|tabs| match detached {
            Some(tab_id) => {
                (tabs.with_active(tab_id, |tabs| tabs.move_pane(moved, target, edge))).flatten()
            }
            None => tabs.move_pane(moved, target, edge),
        });
        // The source tab left without panes closes (a detached one stays, as `remove_leaf`).
        let emptied = (writes.iter().flatten())
            .find(|write| write.layout.is_null())
            .map(|write| write.tab_id)
            .filter(|tab_id| self.terminal_host(project_id, *tab_id) == HostId::Main);
        self.sync_panes(window, cx);
        self.focus_pane(window, cx);
        let writes = writes.unwrap_or_default();
        match emptied {
            Some(source) => self.save_then_close(project_id, writes, source, window, cx),
            None => {
                for write in writes {
                    self.save_layout(project_id, write, window, cx);
                }
            }
        }
    }

    /// A tab was dropped on the `edge` of `target`: its panes move there, then the emptied tab
    /// closes.
    pub(crate) fn drop_tab_on_pane(
        &mut self,
        item: &DraggedItem,
        target: &PaneKey,
        edge: Edge,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some((project_id, source)) = self.dragged_tab(item).map(|(p, tab)| (p, tab.id)) else {
            return;
        };
        let Some(writes) =
            (self.tabs.get_mut(&project_id)).and_then(|tabs| tabs.merge_tab(source, target, edge))
        else {
            return;
        };
        self.sync_panes(window, cx);
        self.focus_pane(window, cx);
        self.save_then_close(project_id, Vec::from(writes), source, window, cx);
    }

    /// Saves `writes` one after another, then closes tab `source`, which they left without
    /// panes (not `close_tab`: a move never asks about unsaved edits).
    /// The close goes last: `tab.close` removes the sessions of the tab's saved layout
    /// (ADR 0041), so it must only run once that layout is empty.
    fn save_then_close(
        &mut self,
        project_id: Ulid,
        writes: Vec<TabSetLayoutParams>,
        source: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            for write in writes {
                let saved = connection.call::<()>(Request::TabSetLayout(write.clone()));
                if let Err(error) = saved.await {
                    tracing::warn!(tab_id = %write.tab_id, "tab.set_layout failed: {error:#}");
                    let _ = this.update(cx, |shell, cx| {
                        if let Some(tabs) = shell.tabs.get_mut(&project_id) {
                            tabs.write_failed(&write);
                        }
                        shell.toast_error(format!("Cannot save the layout: {error:#}"), cx);
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
                        shell.toast_error(format!("Cannot close the tab: {error:#}"), cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A file (a Files tree row) dropped on the workspace: `at` = a pane and the edge to split
    /// it on, `None` = a new tab after the active one.
    /// A file that is already open is not opened twice (one pane per file, spec 4.2): dropped on
    /// a pane it moves its pane there, dropped on the strip its tab is activated.
    /// Only file and diff leaves are placed by a drop; sessions come from `DraggedPane` or the
    /// `+` row.
    pub(crate) fn drop_leaf(
        &mut self,
        project_id: Ulid,
        at: Option<(PaneKey, Edge)>,
        leaf: Leaf,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        use mkrew_ui::documents::Place;
        let key = leaf.key();
        let open = (self.tabs.get(&project_id)).is_some_and(|tabs| tabs.tab_with(&key).is_some());
        if let Some((target, edge)) = &at {
            if *target == key {
                return;
            }
            if open {
                return self.drop_pane(project_id, &key, target, *edge, window, cx);
            }
        }
        let place = match at {
            Some((target, edge)) => Place::Split(target, edge),
            None => Place::NewTab,
        };
        match leaf {
            Leaf::File { file, task, .. } => {
                let file = FileKey::new(file, task);
                self.open_document(project_id, file, None, place, window, cx);
            }
            Leaf::Diff { .. } => self.open_diff(project_id, place, window, cx),
            Leaf::Web { web, id } => {
                self.place_leaf(project_id, Leaf::Web { web, id }, place, window, cx);
            }
            Leaf::Session { .. } => {
                tracing::warn!(%project_id, "a drop never places a session leaf");
            }
        }
    }

    /// Pane `key` dropped on the tab strip: it leaves its tab for a new one right after the
    /// active tab. A pane alone in its tab already is a tab of its own: nothing happens.
    pub(crate) fn pane_to_new_tab(
        &mut self,
        project_id: Ulid,
        key: &PaneKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let shares_its_tab = (self.tabs.get(&project_id))
            .and_then(|tabs| tabs.tab_with(key))
            .is_some_and(|tab| tab.keys().len() > 1);
        if !shares_its_tab {
            return;
        }
        let key = key.clone();
        let position = (self.tabs.get(&project_id)).and_then(Tabs::position_after_active);
        self.create_tab_then(
            project_id,
            position,
            move |shell, tab_id, window, cx| {
                // `tab.create` took a moment: the pane may have moved or closed since, which
                // leaves the new tab empty and the layouts as they were.
                let writes = (shell.tabs.get_mut(&project_id))
                    .and_then(|tabs| tabs.move_to_empty_tab(&key, tab_id));
                for write in writes.into_iter().flatten() {
                    shell.save_layout(project_id, write, window, cx);
                }
                shell.sync_panes(window, cx);
                shell.focus_pane(window, cx);
            },
            window,
            cx,
        );
    }

    /// Makes the main window's tab strip take a pane (to its own tab), a Files tree file row
    /// (a new tab) and OS files (opened as new tabs), and tint while one hovers.
    /// A tab drag keeps the reorder drop the strip already has.
    pub(crate) fn strip_drops(
        &self,
        strip: gpui::Div,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let tint = theme.tint(theme.accent);
        strip
            // A folder row opens nothing there: no tint, no drop.
            .can_drop(|dragged, _, _| {
                dragged
                    .downcast_ref::<DraggedPath>()
                    .is_none_or(|path| !path.is_dir)
            })
            .drag_over::<DraggedPane>(move |style, _, _, _| style.bg(tint))
            .drag_over::<DraggedPath>(move |style, _, _, _| style.bg(tint))
            .drag_over::<DraggedNew>(move |style, _, _, _| style.bg(tint))
            .on_drop(cx.listener(|shell, pane: &DraggedPane, window, cx| {
                if shell.workspace == Some(pane.project_id) && !shell.modal_open() {
                    shell.pane_to_new_tab(pane.project_id, &pane.key, window, cx);
                }
            }))
            .on_drop(cx.listener(|shell, item: &DraggedNew, window, cx| {
                shell.drop_new_on_strip(item, window, cx);
            }))
            .on_drop(cx.listener(|shell, path: &DraggedPath, window, cx| {
                if shell.workspace == Some(path.project_id) && !shell.modal_open() {
                    let leaf = Leaf::of_file(path.file());
                    shell.drop_leaf(path.project_id, None, leaf, window, cx);
                }
            }))
            .on_drag_move::<ExternalPaths>(cx.listener(
                |shell, event: &DragMoveEvent<ExternalPaths>, _, cx| {
                    shell.hover_os_drop(&DropTarget::Strip, event, cx);
                },
            ))
            .child(drop_catcher(DropTarget::Strip, cx))
    }
}
