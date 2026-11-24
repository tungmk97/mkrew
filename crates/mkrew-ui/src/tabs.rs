//! Terminal tabs of one workspace (SPEC §10): tab order, the active tab, each tab's split tree
//! and focused pane. Pure and tested headlessly; layout edits return the `tab.set_layout`
//! params to send, and `visible()` + [`attach_diff`] tell which sessions to (de)attach.

use std::collections::VecDeque;

use mkrew_protocol::{
    Event, Session, SessionKind, SessionState, TabMoveParams, TabRenameParams, TabSetLayoutParams,
    TerminalTab,
};
use ulid::Ulid;

use crate::documents::{self, Doc, Documents};
use crate::split::{
    Axis, Direction, Divider, Edge, Leaf, Node, PaneKey, Preset, RESIZE_STEP, Rect,
};

/// Title of a workspace's first tab; later ones are numbered.
pub const FIRST_TAB_TITLE: &str = "Terminal";

#[derive(Debug, Clone, PartialEq)]
pub struct Tab {
    pub id: Ulid,
    pub title: String,
    pub position: f64,
    /// Pinned tabs lead the strip (SPEC §10).
    pub pinned: bool,
    /// `None` = a tab without panes. Private so every change goes through `write()`.
    layout: Option<Node>,
    /// Local only (not persisted): the pane keyboard input goes to.
    focused: Option<PaneKey>,
    /// Local only: the pane filling the whole tab (zoom); always the focused pane.
    zoomed: Option<PaneKey>,
    /// Layouts sent with `tab.set_layout`, oldest first, whose `tab.updated` echo has not
    /// arrived yet. While any are pending the local layout is the newest one, so layouts in
    /// other `tab.updated` events (renames, moves, older echoes) are not applied.
    pending: VecDeque<serde_json::Value>,
}

impl Tab {
    fn from_wire(tab: TerminalTab) -> Self {
        let layout = Node::from_json(&tab.layout).unwrap_or_else(|error| {
            tracing::warn!(tab_id = %tab.id, "ignoring the stored layout: {error}");
            None
        });
        let mut tab = Tab {
            id: tab.id,
            title: tab.title,
            position: tab.position,
            pinned: tab.pinned,
            layout,
            focused: None,
            zoomed: None,
            pending: VecDeque::new(),
        };
        tab.fix_focus();
        tab
    }

    pub fn layout(&self) -> Option<&Node> {
        self.layout.as_ref()
    }

    pub fn focused(&self) -> Option<&PaneKey> {
        self.focused.as_ref()
    }

    pub fn zoomed(&self) -> Option<&PaneKey> {
        self.zoomed.as_ref()
    }

    /// The focused pane's session; `None` when a file or the diff has the focus.
    pub fn focused_session(&self) -> Option<Ulid> {
        self.focused.as_ref()?.session()
    }

    /// Every pane in tree order, files and the diff included.
    pub fn keys(&self) -> Vec<PaneKey> {
        self.layout.as_ref().map(Node::keys).unwrap_or_default()
    }

    /// The session panes only, in tree order.
    pub fn sessions(&self) -> Vec<Ulid> {
        self.layout.as_ref().map(Node::sessions).unwrap_or_default()
    }

    /// Panes on screen: the zoomed pane alone, else every pane.
    pub fn shown(&self) -> Vec<PaneKey> {
        match &self.zoomed {
            Some(key) => vec![key.clone()],
            None => self.keys(),
        }
    }

    /// Sessions on screen: the sessions among [`Self::shown`] (the ones to attach).
    pub fn shown_sessions(&self) -> Vec<Ulid> {
        self.shown().iter().filter_map(PaneKey::session).collect()
    }

    /// Area of each pane on screen: the zoomed pane fills the tab.
    pub fn rects(&self) -> Vec<(PaneKey, Rect)> {
        match (&self.zoomed, &self.layout) {
            (Some(key), _) => vec![(key.clone(), Rect::WHOLE)],
            (None, Some(node)) => node.rects(),
            (None, None) => Vec::new(),
        }
    }

    /// Dividers drawn over the panes; none while a pane is zoomed.
    pub fn dividers(&self) -> Vec<Divider> {
        match (&self.zoomed, &self.layout) {
            (None, Some(node)) => node.dividers(),
            _ => Vec::new(),
        }
    }

    pub fn contains(&self, key: &PaneKey) -> bool {
        self.layout.as_ref().is_some_and(|node| node.contains(key))
    }

    /// Keeps the focus on a pane of the tab: the first pane if it was lost. A zoom ends when
    /// the focus leaves the zoomed pane.
    fn fix_focus(&mut self) {
        if !self.focused.as_ref().is_some_and(|key| self.contains(key)) {
            self.focused = self.keys().into_iter().next();
        }
        if self.zoomed != self.focused {
            self.zoomed = None;
        }
    }

    /// Records a local layout change and returns the request persisting it.
    fn write(&mut self) -> TabSetLayoutParams {
        let layout = Node::to_json(self.layout.as_ref());
        self.pending.push_back(layout.clone());
        TabSetLayoutParams {
            tab_id: self.id,
            layout,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tabs {
    project_id: Ulid,
    /// Pinned first, then ordered by position.
    tabs: Vec<Tab>,
    active: Option<Ulid>,
}

impl Tabs {
    pub fn new(project_id: Ulid) -> Self {
        Tabs {
            project_id,
            tabs: Vec::new(),
            active: None,
        }
    }

    pub fn project_id(&self) -> Ulid {
        self.project_id
    }

    /// Replaces the tabs with a `tab.list` reply, keeping the active tab and focus where they
    /// still exist. Unreadable layouts load as empty tabs.
    pub fn load(&mut self, tabs: Vec<TerminalTab>) {
        let old = std::mem::take(&mut self.tabs);
        self.tabs = tabs
            .into_iter()
            .filter(|tab| tab.project_id == self.project_id)
            .map(|wire| {
                let mut tab = Tab::from_wire(wire);
                if let Some(previous) = old.iter().find(|old| old.id == tab.id) {
                    tab.focused = previous.focused.clone();
                    tab.zoomed = previous.zoomed.clone();
                    tab.fix_focus();
                }
                tab
            })
            .collect();
        self.sort();
        self.fix_active();
    }

    /// Applies a `tab.updated` event of this workspace; returns whether anything changed.
    pub fn apply(&mut self, event: &Event) -> bool {
        let Event::TabUpdated(wire) = event else {
            return false;
        };
        if wire.project_id != self.project_id {
            return false;
        }
        let incoming = Tab::from_wire(wire.clone());
        match self.tabs.iter_mut().find(|tab| tab.id == wire.id) {
            Some(tab) => {
                let before = tab.clone();
                tab.title = incoming.title;
                tab.position = incoming.position;
                tab.pinned = incoming.pinned;
                if let Some(echo) = tab.pending.iter().position(|sent| *sent == wire.layout) {
                    // Writes are applied in order, so earlier ones were superseded.
                    tab.pending.drain(..=echo);
                } else if tab.pending.is_empty() {
                    tab.layout = incoming.layout;
                    tab.fix_focus();
                }
                let changed = *tab != before;
                self.sort();
                self.fix_active();
                changed
            }
            None => {
                self.tabs.push(incoming);
                self.sort();
                self.fix_active();
                true
            }
        }
    }

    /// A `tab.set_layout` failed: its echo will never arrive.
    pub fn write_failed(&mut self, write: &TabSetLayoutParams) {
        if let Some(tab) = self.tab_mut(write.tab_id)
            && let Some(index) = tab.pending.iter().position(|sent| *sent == write.layout)
        {
            tab.pending.remove(index);
        }
    }

    /// Drops a closed tab (the daemon sends no event for `tab.close`). The tab after it, else
    /// the one before, becomes active if it was.
    pub fn remove(&mut self, tab_id: Ulid) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return;
        };
        self.tabs.remove(index);
        if self.active == Some(tab_id) {
            let next = self.tabs.get(index).or_else(|| self.tabs.last());
            self.active = next.map(|tab| tab.id);
        }
    }

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn tab(&self, tab_id: Ulid) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.id == tab_id)
    }

    fn tab_mut(&mut self, tab_id: Ulid) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.id == tab_id)
    }

    /// The tab whose layout holds `session`.
    pub fn tab_of(&self, session: Ulid) -> Option<&Tab> {
        self.tab_with(&PaneKey::Session(session))
    }

    /// The tab whose layout holds `key`, of any kind. One workspace holds a file or the diff
    /// in at most one tab (ADR 0108), main window and detached windows alike: they all draw
    /// from this one `Tabs`.
    pub fn tab_with(&self, key: &PaneKey) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.contains(key))
    }

    /// The mode saved for `session`'s pane (see [`Node::chat_of`]).
    pub fn chat_of(&self, session: Ulid) -> Option<bool> {
        self.tab_of(session)?.layout.as_ref()?.chat_of(session)
    }

    /// Saves `chat` on `session`'s pane: the write to send, `None` when nothing changed or no
    /// tab holds it.
    pub fn set_chat(&mut self, session: Ulid, chat: Option<bool>) -> Option<TabSetLayoutParams> {
        let tab_id = self.tab_of(session)?.id;
        let tab = self.tab_mut(tab_id)?;
        let changed = tab.layout.as_mut()?.set_chat(session, chat);
        changed.then(|| tab.write())
    }

    pub fn active(&self) -> Option<&Tab> {
        self.active.and_then(|id| self.tab(id))
    }

    fn active_mut(&mut self) -> Option<&mut Tab> {
        let id = self.active?;
        self.tab_mut(id)
    }

    /// Returns false if there is no such tab.
    pub fn activate(&mut self, tab_id: Ulid) -> bool {
        let exists = self.tab(tab_id).is_some();
        if exists {
            self.active = Some(tab_id);
        }
        exists
    }

    /// Activates the tab `offset` places after the active one, wrapping around, among the tabs
    /// that are not `hidden` (shown in a detached window).
    pub fn activate_offset(&mut self, offset: isize, hidden: impl Fn(Ulid) -> bool) {
        let shown: Vec<Ulid> = (self.tabs.iter())
            .map(|tab| tab.id)
            .filter(|id| !hidden(*id))
            .collect();
        let Some(index) = self
            .active
            .and_then(|id| shown.iter().position(|shown| *shown == id))
        else {
            return;
        };
        let next = (index as isize + offset).rem_euclid(shown.len() as isize) as usize;
        self.active = Some(shown[next]);
    }

    /// Activates the `index`-th (0-based) tab that is not `hidden`; false if there is none.
    pub fn activate_nth(&mut self, index: usize, hidden: impl Fn(Ulid) -> bool) -> bool {
        let Some(tab_id) = (self.tabs.iter())
            .map(|tab| tab.id)
            .filter(|id| !hidden(*id))
            .nth(index)
        else {
            return false;
        };
        self.active = Some(tab_id);
        true
    }

    /// Moves `tab_id` to the `to`-th place among the tabs of its pin group (pinned tabs only move
    /// among pinned ones): a position between its new neighbors, applied at once. Returns the
    /// `tab.move` to send; `None` if nothing moves.
    // ponytail: midpoints halve the gap per move; ~50 moves into one gap exhaust f64. Renumber
    // the group if that ever shows.
    pub fn move_tab(&mut self, tab_id: Ulid, to: usize) -> Option<TabMoveParams> {
        let pinned = self.tab(tab_id)?.pinned;
        let others: Vec<f64> = (self.tabs.iter())
            .filter(|tab| tab.pinned == pinned && tab.id != tab_id)
            .map(|tab| tab.position)
            .collect();
        let to = to.min(others.len());
        let position = match (to.checked_sub(1).map(|i| others[i]), others.get(to)) {
            (Some(before), Some(after)) => (before + after) / 2.0,
            (Some(before), None) => before + 1.0,
            (None, Some(after)) => after - 1.0,
            (None, None) => return None,
        };
        let tab = self.tab_mut(tab_id)?;
        tab.position = position;
        self.sort();
        Some(TabMoveParams { tab_id, position })
    }

    /// Moves `tab_id` before `anchor` (the end of its pin group when `None`); an anchor of the
    /// other pin group means the start (anchor pinned) or the end of the tab's own group.
    pub fn move_tab_before(&mut self, tab_id: Ulid, anchor: Option<Ulid>) -> Option<TabMoveParams> {
        let pinned = self.tab(tab_id)?.pinned;
        let mut group = (self.tabs.iter()).filter(|tab| tab.pinned == pinned && tab.id != tab_id);
        let to = match anchor.and_then(|id| self.tab(id)) {
            Some(anchor) if anchor.pinned == pinned => group.position(|tab| tab.id == anchor.id),
            Some(anchor) if anchor.pinned => Some(0),
            _ => None,
        };
        let to = to.unwrap_or(usize::MAX);
        self.move_tab(tab_id, to)
    }

    /// Moves the active tab one place (`by` -1 / 1) along the tabs of its pin group that are
    /// not `hidden` (shown in a detached window): Move tab left / right never steps over a tab
    /// the strip does not show. Applied at once; returns the `tab.move` to send, `None` at
    /// either end.
    pub fn move_active(
        &mut self,
        by: isize,
        hidden: impl Fn(Ulid) -> bool,
    ) -> Option<TabMoveParams> {
        let active = self.active()?;
        let (id, pinned) = (active.id, active.pinned);
        let group: Vec<Ulid> = (self.tabs.iter())
            .filter(|tab| tab.pinned == pinned)
            .map(|tab| tab.id)
            .collect();
        let shown: Vec<Ulid> = (group.iter().copied())
            .filter(|tab| !hidden(*tab))
            .collect();
        let from = shown.iter().position(|tab| *tab == id)?;
        let neighbour = shown[crate::sidebar::step_index(shown.len(), from, by)?];
        // Left: before the neighbour. Right: before whatever follows it, or at the end.
        let anchor = if by < 0 {
            Some(neighbour)
        } else {
            group
                .iter()
                .skip_while(|tab| **tab != neighbour)
                .nth(1)
                .copied()
        };
        self.move_tab_before(id, anchor)
    }

    /// The tabs "Close others" (`right_only` false) or "Close tabs to the right" closes from
    /// `tab_id`'s menu: the unpinned ones besides it, or only those after it, that `hosted`
    /// accepts (the ones in the same window).
    pub fn close_targets(
        &self,
        tab_id: Ulid,
        right_only: bool,
        hosted: impl Fn(Ulid) -> bool,
    ) -> Vec<Ulid> {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return Vec::new();
        };
        (self.tabs.iter().enumerate())
            .filter(|&(i, tab)| {
                !tab.pinned && i != index && (!right_only || i > index) && hosted(tab.id)
            })
            .map(|(_, tab)| tab.id)
            .collect()
    }

    /// Sessions shown on screen: the session panes of the active tab (only the zoomed one
    /// while a pane is zoomed). All others stay detached.
    pub fn visible(&self) -> Vec<Ulid> {
        self.active().map(Tab::shown_sessions).unwrap_or_default()
    }

    /// Activates the tab holding `key` and focuses its pane; false if no tab holds it.
    pub fn focus(&mut self, key: &PaneKey) -> bool {
        let Some(tab_id) = self.tab_with(key).map(|tab| tab.id) else {
            return false;
        };
        self.active = Some(tab_id);
        if let Some(tab) = self.tab_mut(tab_id) {
            tab.focused = Some(key.clone());
            tab.fix_focus();
        }
        true
    }

    /// Runs `f` as if `tab_id` were the active tab, then gives the active tab back (a detached
    /// window acts on its own tab without moving the main window's, spec 2026-11-10 §4.1).
    pub fn with_active<R>(&mut self, tab_id: Ulid, f: impl FnOnce(&mut Tabs) -> R) -> Option<R> {
        self.tab(tab_id)?;
        let saved = self.active.replace(tab_id);
        let result = f(self);
        self.active = saved;
        if self.active.is_some_and(|id| self.tab(id).is_none()) {
            self.fix_active();
        }
        Some(result)
    }

    /// Focuses `key`'s pane in `tab_id` without making that tab active.
    pub fn focus_pane(&mut self, tab_id: Ulid, key: &PaneKey) -> bool {
        let Some(tab) = self.tab_mut(tab_id).filter(|tab| tab.contains(key)) else {
            return false;
        };
        tab.focused = Some(key.clone());
        tab.fix_focus();
        true
    }

    /// While the active tab is `hidden` (shown in a detached window), the next tab that is not
    /// becomes active (right of it, else left), or none.
    pub fn activate_skipping(&mut self, hidden: impl Fn(Ulid) -> bool) {
        let Some(active) = self.active.filter(|id| hidden(*id)) else {
            return;
        };
        let at = self
            .tabs
            .iter()
            .position(|tab| tab.id == active)
            .unwrap_or(0);
        let right = self.tabs[at..].iter().find(|tab| !hidden(tab.id));
        let left = self.tabs[..at].iter().rev().find(|tab| !hidden(tab.id));
        self.active = right.or(left).map(|tab| tab.id);
    }

    /// Moves the focus in the active tab (ending a zoom); false if there is no pane that way.
    pub fn focus_neighbour(&mut self, direction: Direction) -> bool {
        let Some(tab) = self.active_mut() else {
            return false;
        };
        let next = match (&tab.layout, &tab.focused) {
            (Some(node), Some(from)) => node.neighbour(from, direction),
            _ => None,
        };
        let Some(next) = next else {
            return false;
        };
        tab.focused = Some(next);
        tab.fix_focus();
        true
    }

    /// Zooms the focused pane of the active tab to fill it, or ends the zoom. UI-only (not
    /// persisted). False if there is nothing to zoom: no pane, or the only pane.
    pub fn toggle_zoom(&mut self) -> bool {
        let Some(tab) = self.active_mut() else {
            return false;
        };
        if tab.zoomed.is_some() {
            tab.zoomed = None;
            return true;
        }
        if tab.keys().len() < 2 {
            return false;
        }
        tab.zoomed = tab.focused.clone();
        tab.zoomed.is_some()
    }

    /// Moves the divider next to the focused pane of the active tab by one step. `None` (no
    /// write) if no divider moved, or while a pane is zoomed (the dividers are not shown).
    pub fn resize_focused(&mut self, direction: Direction) -> Option<TabSetLayoutParams> {
        let tab = self.active_mut()?;
        if tab.zoomed.is_some() {
            return None;
        }
        let focused = tab.focused.as_ref()?;
        if !tab.layout.as_mut()?.resize(focused, direction, RESIZE_STEP) {
            return None;
        }
        Some(tab.write())
    }

    /// Moves divider `index` of the active tab while it is dragged. Not written (one write
    /// per drag, [`Self::write_active`]); false if nothing moved.
    pub fn drag_divider(&mut self, index: usize, ratio: f64) -> bool {
        self.active_mut()
            .and_then(|tab| tab.layout.as_mut())
            .is_some_and(|node| node.set_ratio(index, ratio))
    }

    /// Persists the active tab's layout (after a divider drag).
    pub fn write_active(&mut self) -> Option<TabSetLayoutParams> {
        Some(self.active_mut()?.write())
    }

    /// Gives the active tab's panes equal shares ([`Node::equalize`]). `None` if nothing moved.
    pub fn equalize_active(&mut self) -> Option<TabSetLayoutParams> {
        let tab = self.active_mut()?;
        if !tab.layout.as_mut()?.equalize() {
            return None;
        }
        Some(tab.write())
    }

    /// Places `new` next to the focused pane of the active tab (or as the only pane of an
    /// empty tab) and focuses it. `None` if there is no active tab or `new` is already shown.
    /// (A session spawned with `tab_id` into an empty tab is placed by the daemon already.)
    pub fn split_focused(&mut self, axis: Axis, new: Ulid) -> Option<TabSetLayoutParams> {
        self.split_focused_chat(axis, new, None)
    }

    /// [`Self::split_focused`] with the new pane's saved mode `chat` (ADR 0105: a new agent
    /// pane is born with the default of the moment, so a later change of the default never
    /// flips it).
    pub fn split_focused_chat(
        &mut self,
        axis: Axis,
        new: Ulid,
        chat: Option<bool>,
    ) -> Option<TabSetLayoutParams> {
        if self.tab_of(new).is_some() {
            return None;
        }
        let tab = self.active_mut()?;
        let leaf = Node::Leaf(Leaf::Session { session: new, chat });
        match (&mut tab.layout, &tab.focused) {
            (Some(node), Some(target)) => {
                if !node.split_with(target, axis, leaf) {
                    return None;
                }
            }
            (layout @ None, _) => *layout = Some(leaf),
            // `fix_focus` keeps a pane focused whenever the tab has one.
            (Some(_), None) => return None,
        }
        tab.focused = Some(PaneKey::Session(new));
        tab.fix_focus();
        Some(tab.write())
    }

    /// Saves a mode on every pane that has none: `mode(session)` says which (`None` leaves the
    /// pane unset). One write per tab that changed, so a pick costs one `tab.set_layout` per
    /// tab, not one per pane. Panes that already have a mode are never touched.
    pub fn pin_unset(&mut self, mode: impl Fn(Ulid) -> Option<bool>) -> Vec<TabSetLayoutParams> {
        let mut writes = Vec::new();
        for tab in &mut self.tabs {
            let Some(layout) = tab.layout.as_mut() else {
                continue;
            };
            let unset: Vec<Ulid> = (layout.sessions().into_iter())
                .filter(|id| layout.chat_of(*id).is_none())
                .collect();
            let mut changed = false;
            for id in unset {
                if let Some(chat) = mode(id) {
                    changed |= layout.set_chat(id, Some(chat));
                }
            }
            if changed {
                writes.push(tab.write());
            }
        }
        writes
    }

    /// Moves the pane of `key` next to the pane of `target` on `edge` (the moved leaf takes that
    /// half of the target), within its tab or from another one. The whole leaf travels: a file
    /// keeps its Markdown mode, a session its chat flag; a dragged preview file is kept (it is
    /// no longer the preview tab, spec 2026-11-23 section 6.1). The moved pane is
    /// focused and its new tab activated. Returns the writes, the source tab's first when it
    /// differs; `None` if nothing moves (the same pane, or either is in no tab).
    pub fn move_pane(
        &mut self,
        key: &PaneKey,
        target: &PaneKey,
        edge: Edge,
    ) -> Option<Vec<TabSetLayoutParams>> {
        if key == target {
            return None;
        }
        let from = self.tab_with(key)?.id;
        let to = self.tab_with(target)?.id;
        let mut leaf = self.tab(from)?.layout.as_ref()?.leaf_of(key)?.clone();
        documents::keep_leaf(&mut leaf);
        let mut writes = Vec::new();
        let source = self.tab_mut(from)?;
        let closed = source.layout.take()?.close(key);
        source.layout = closed.remaining;
        if source.focused.as_ref() == Some(key) {
            source.focused = closed.focus;
        }
        source.fix_focus();
        if from != to {
            writes.push(source.write());
        }
        let tab = self.tab_mut(to)?;
        // `target` is still in the tab (it is not `key`), which no longer holds `key`.
        let placed = (tab.layout.as_mut())
            .is_some_and(|node| node.insert_at(target, edge, Node::Leaf(leaf)));
        debug_assert!(placed, "the moved pane always fits next to its target");
        tab.focused = Some(key.clone());
        tab.fix_focus();
        writes.push(tab.write());
        self.active = Some(to);
        Some(writes)
    }

    /// Moves every pane of tab `source` next to the pane of `target` in another tab (as
    /// [`Self::move_pane`]), keeping their layout; `source` is left without panes, for the
    /// caller to close once both writes are saved. The target tab is activated with the first
    /// moved pane focused. A preview file among them is kept, as in [`Self::move_pane`].
    /// Returns the writes, the target tab's first; `None` if nothing moves
    /// (`target` is in `source` or in no tab, or `source` has no panes).
    pub fn merge_tab(
        &mut self,
        source: Ulid,
        target: &PaneKey,
        edge: Edge,
    ) -> Option<[TabSetLayoutParams; 2]> {
        let to = self.tab_with(target)?.id;
        if to == source {
            return None;
        }
        let mut moved = self.tab(source)?.layout.clone()?;
        let keys = moved.keys();
        for key in &keys {
            if let Some(leaf) = moved.leaf_mut(key) {
                documents::keep_leaf(leaf);
            }
        }
        let first = keys.into_iter().next()?;
        let tab = self.tab_mut(to)?;
        if !tab.layout.as_mut()?.insert_at(target, edge, moved) {
            return None;
        }
        tab.focused = Some(first);
        tab.fix_focus();
        let placed = tab.write();
        let emptied = self.tab_mut(source)?;
        emptied.layout = None;
        emptied.fix_focus();
        let emptied = emptied.write();
        self.active = Some(to);
        Some([placed, emptied])
    }

    /// Pane `key` leaves its tab, whole leaf (chat flag, Markdown mode; a preview file is
    /// kept), for the empty tab `into`, where it becomes the root and takes the focus: a pane
    /// dragged onto the tab strip gets a tab of its own. Returns the writes, `into`'s first, then the source's, so
    /// a failure between them duplicates the pane rather than losing it.
    /// `None`, nothing changed, if no tab holds `key`, `into` is missing, is the source or has
    /// panes, or `key` is the only pane of its tab.
    pub fn move_to_empty_tab(
        &mut self,
        key: &PaneKey,
        into: Ulid,
    ) -> Option<[TabSetLayoutParams; 2]> {
        let source = self.tab_with(key)?;
        if source.id == into || source.keys().len() < 2 || self.tab(into)?.layout().is_some() {
            return None;
        }
        let mut leaf = source.layout()?.leaf_of(key)?.clone();
        documents::keep_leaf(&mut leaf);
        let out = self.close_leaf(key)?;
        // `into` exists and is empty (checked above), so this cannot refuse.
        let inn = self.insert_leaf(into, None, Edge::Right, leaf)?;
        Some([inn, out])
    }

    /// Lays tab `tab_id` out as `preset` (SPEC §10 layouts): its panes keep their tree
    /// order, whole (a file keeps its Markdown mode, a session its chat flag), then come the
    /// `new` sessions it does not show yet. `None` (nothing changes) unless that makes
    /// exactly the preset's panes, or when a new session is shown in another tab. A zoom
    /// ends; the focus stays.
    pub fn arrange(
        &mut self,
        tab_id: Ulid,
        preset: Preset,
        new: &[Ulid],
    ) -> Option<TabSetLayoutParams> {
        if new
            .iter()
            .any(|id| self.tab_of(*id).is_some_and(|tab| tab.id != tab_id))
        {
            return None;
        }
        let tab = self.tab_mut(tab_id)?;
        let mut leaves = tab.layout.as_ref().map(Node::leaves).unwrap_or_default();
        for id in new {
            let key = PaneKey::Session(*id);
            if !leaves.iter().any(|leaf| leaf.has_key(&key)) {
                leaves.push(Leaf::session(*id));
            }
        }
        tab.layout = Some(preset.build(&leaves)?);
        tab.zoomed = None;
        tab.fix_focus();
        Some(tab.write())
    }

    /// Removes the pane of `key` from whichever tab holds it; focus moves to the pane that
    /// took its place. `None` if no tab holds it. A tab left without panes stays (the caller
    /// closes it).
    pub fn close_leaf(&mut self, key: &PaneKey) -> Option<TabSetLayoutParams> {
        let tab_id = self.tab_with(key)?.id;
        let tab = self.tab_mut(tab_id)?;
        let closed = tab.layout.take()?.close(key);
        tab.layout = closed.remaining;
        if tab.focused.as_ref() == Some(key) {
            tab.focused = closed.focus;
        }
        tab.fix_focus();
        Some(tab.write())
    }

    /// Places `leaf` in `tab_id` on `edge` of the pane of `target` (the new leaf takes that
    /// half), or, with no `target`, as the root of an empty tab, and focuses it. The tab is not
    /// activated. `None` (nothing changes) if the workspace already holds the leaf's key in
    /// any tab (a file or the diff has one pane per workspace, ADR 0108), the tab is unknown,
    /// `target` is not in that tab, or a root is placed in a tab that has panes.
    pub fn insert_leaf(
        &mut self,
        tab_id: Ulid,
        target: Option<&PaneKey>,
        edge: Edge,
        leaf: Leaf,
    ) -> Option<TabSetLayoutParams> {
        let key = leaf.key();
        if self.tab_with(&key).is_some() {
            return None;
        }
        let tab = self.tab_mut(tab_id)?;
        let node = Node::Leaf(leaf);
        match (target, &mut tab.layout) {
            (Some(target), Some(root)) => {
                if !root.insert_at(target, edge, node) {
                    return None;
                }
            }
            (None, layout @ None) => *layout = Some(node),
            _ => return None,
        }
        tab.focused = Some(key);
        tab.fix_focus();
        Some(tab.write())
    }

    /// Changes the leaf of `key` with `change` (a Markdown mode, a preview flag, a chat flag):
    /// the write to send, `None` when no tab holds it or `change` left it as it was. A `change`
    /// that would change which pane the leaf is (its key) is refused: the leaf stays as it was
    /// and nothing is written.
    pub fn update_leaf(
        &mut self,
        key: &PaneKey,
        change: impl FnOnce(&mut Leaf),
    ) -> Option<TabSetLayoutParams> {
        let tab_id = self.tab_with(key)?.id;
        let tab = self.tab_mut(tab_id)?;
        let leaf = tab.layout.as_mut()?.leaf_mut(key)?;
        let before = leaf.clone();
        change(leaf);
        if !leaf.has_key(key) {
            *leaf = before;
            return None;
        }
        let changed = *leaf != before;
        changed.then(|| tab.write())
    }

    /// The tab that holds the workspace's preview file (the leaf with `preview` set), which
    /// the next single click in the Files tree replaces.
    pub fn preview_tab(&self) -> Option<&Tab> {
        self.tabs.iter().find(|tab| {
            (tab.layout.as_ref()).is_some_and(|node| {
                node.any_leaf(|leaf| matches!(leaf, Leaf::File { preview: true, .. }))
            })
        })
    }

    /// The `position` for a new (unpinned) tab that opens right after the active one: the
    /// midpoint between the active tab and the next tab (as [`Self::move_tab`] places a tab),
    /// or one past it when the active tab is last. While the active tab is pinned, the new
    /// tab goes first among the unpinned tabs: their first position - 1.0, or 1.0 when there
    /// is none. `None` without tabs.
    pub fn position_after_active(&self) -> Option<f64> {
        let active = self.active()?;
        if active.pinned {
            let first = self.tabs.iter().find(|tab| !tab.pinned);
            return Some(first.map_or(1.0, |tab| tab.position - 1.0));
        }
        let next = (self.tabs.iter())
            .skip_while(|tab| tab.id != active.id)
            .nth(1);
        Some(match next {
            Some(next) => (active.position + next.position) / 2.0,
            None => active.position + 1.0,
        })
    }

    /// Removes panes whose session `keep` rejects (e.g. sessions the daemon no longer knows)
    /// and returns the layout writes to send. Files and the diff are not sessions: they stay,
    /// and so does a tab that holds one.
    pub fn prune(&mut self, keep: impl Fn(Ulid) -> bool) -> Vec<TabSetLayoutParams> {
        let mut writes = Vec::new();
        for tab in &mut self.tabs {
            let gone: Vec<Ulid> = tab.sessions().into_iter().filter(|id| !keep(*id)).collect();
            if gone.is_empty() {
                continue;
            }
            for session in gone {
                tab.layout = tab
                    .layout
                    .take()
                    .and_then(|node| node.close(&PaneKey::Session(session)).remaining);
            }
            tab.fix_focus();
            writes.push(tab.write());
        }
        writes
    }

    /// Pinned tabs first, then by position (pinning leaves `position` alone, so unpinning
    /// puts a tab back).
    fn sort(&mut self) {
        self.tabs.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then_with(|| a.position.total_cmp(&b.position))
                .then_with(|| a.id.cmp(&b.id))
        });
    }

    fn fix_active(&mut self) {
        if self.active.is_none_or(|id| self.tab(id).is_none()) {
            self.active = self.tabs.first().map(|tab| tab.id);
        }
    }
}

/// The session an empty active tab shows: the newest live session of the workspace that is in
/// no tab yet. `None` if the active tab has panes (or there is no active tab).
pub fn newest_unplaced(tabs: &Tabs, sessions: &[Session]) -> Option<Ulid> {
    if tabs.active()?.layout().is_some() {
        return None;
    }
    sessions
        .iter()
        .filter(|s| {
            s.project_id == tabs.project_id
                && s.state != SessionState::Exited
                && tabs.tab_of(s.id).is_none()
        })
        .max_by_key(|s| s.started_at)
        .map(|s| s.id)
}

/// The sessions on screen (SPEC §10): the visible panes of the active workspace's tabs, and
/// none while the window is hidden (minimized), so every pane is detached until it is restored.
pub fn on_screen(window_visible: bool, workspace: Option<&Tabs>) -> Vec<Ulid> {
    match workspace {
        Some(tabs) if window_visible => tabs.visible(),
        _ => Vec::new(),
    }
}

/// The `tab.rename` for an edited title, trimmed; `None` if it is blank or unchanged.
pub fn rename_params(tab: &Tab, draft: &str) -> Option<TabRenameParams> {
    let title = draft.trim();
    (!title.is_empty() && title != tab.title).then(|| TabRenameParams {
        tab_id: tab.id,
        title: title.to_owned(),
    })
}

/// The status icon in front of a tab chip: only a state the user should notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabStatus {
    Working,
    NeedsInput,
}

/// The icon of one pane in a tab chip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaneIcon {
    /// An agent pane; the profile id (its badge is looked up by the caller).
    Agent(String),
    Shell,
    Run,
    /// A file pane; its stored path (the caller draws the file icon).
    File(String),
    /// The diff pane.
    Diff,
    /// A browser pane.
    Web,
}

/// What a tab chip shows in front of its title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabIcons {
    /// `None` unless a pane needs input or an agent works.
    pub status: Option<TabStatus>,
    pub panes: Vec<PaneIcon>,
    /// Panes beyond [`MAX_PANE_ICONS`], shown as `+N`.
    pub more: usize,
}

/// Most pane icons an active tab chip shows before `+N`.
pub const MAX_PANE_ICONS: usize = 3;

/// The icons of `tab`'s chip. Status: needs input beats a working agent; every other state
/// (idle, done, exited, starting) shows none. Pane icons: the active tab shows one per pane in
/// pane order (at most [`MAX_PANE_ICONS`], then `more`), an inactive tab only its focused
/// pane. Panes without a known session are skipped.
pub fn tab_icons(tab: &Tab, sessions: &[Session], active: bool) -> TabIcons {
    let ids = tab.sessions();
    let known = |id: &Ulid| sessions.iter().find(|s| s.id == *id);
    let state = |wanted: SessionState| {
        ids.iter().filter_map(known).any(|s| {
            s.state == wanted && (wanted != SessionState::Working || s.kind == SessionKind::Agent)
        })
    };
    let status = if state(SessionState::NeedsInput) {
        Some(TabStatus::NeedsInput)
    } else if state(SessionState::Working) {
        Some(TabStatus::Working)
    } else {
        None
    };
    let icon = |s: &Session| match s.kind {
        SessionKind::Agent => PaneIcon::Agent(s.agent_profile.clone()),
        _ if mkrew_core::run::command_name(s).is_some() => PaneIcon::Run,
        _ => PaneIcon::Shell,
    };
    let pane_icon = |key: &PaneKey| match key {
        PaneKey::Session(id) => known(id).map(icon),
        PaneKey::File(file) => Some(PaneIcon::File(file.path.clone())),
        PaneKey::Diff => Some(PaneIcon::Diff),
        PaneKey::Web(_) => Some(PaneIcon::Web),
    };
    let mut panes: Vec<PaneIcon> = if active {
        tab.keys().iter().filter_map(pane_icon).collect()
    } else {
        tab.focused().into_iter().filter_map(pane_icon).collect()
    };
    let more = panes.len().saturating_sub(MAX_PANE_ICONS);
    panes.truncate(MAX_PANE_ICONS);
    TabIcons {
        status,
        panes,
        more,
    }
}

/// Whether `tab` still has its default name: `Terminal` or `Terminal N`.
fn has_default_title(tab: &Tab) -> bool {
    match tab.title.strip_prefix(FIRST_TAB_TITLE) {
        Some("") => true,
        Some(rest) => rest
            .strip_prefix(' ')
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())),
        None => false,
    }
}

/// The session title a tab shows instead of its own: the title of its focused session pane (a
/// focused file or the diff shows its own title from Task 4 / 5 on; no pane header shows a
/// title), while the tab keeps its default `Terminal` / `Terminal N` name. A name the user gave
/// the tab always wins. It follows the focus; `None` for a tab without session panes or a
/// focused pane without a title.
pub fn session_label<'a>(tab: &Tab, sessions: &'a [Session]) -> Option<&'a str> {
    let focused = tab.focused_session().filter(|_| has_default_title(tab))?;
    sessions
        .iter()
        .find(|s| s.id == focused)
        .and_then(crate::history::title)
}

/// The title a tab shows while its focused pane is the diff and it keeps its default name (a
/// name the user gave the tab always wins), like [`session_label`].
pub fn diff_label(tab: &Tab) -> Option<&'static str> {
    (tab.focused() == Some(&PaneKey::Diff) && has_default_title(tab)).then_some("Diff")
}

/// The browser pane whose page names the tab: the focused pane is a browser and the tab keeps
/// its default name (a name the user gave the tab always wins), like [`diff_label`]. The
/// caller looks the page title up.
pub fn web_label(tab: &Tab) -> Option<Ulid> {
    match tab.focused()? {
        PaneKey::Web(id) if has_default_title(tab) => Some(*id),
        _ => None,
    }
}

/// Where a web link clicked in a tab goes (spec 4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebPlace {
    /// The tab's browser pane (the first in tree order) loads it.
    Load(Ulid),
    /// A new browser pane right of this pane.
    Split(PaneKey),
    /// No tab is shown: a new tab.
    NewTab,
}

/// Where a link clicked in `tab` (`None`: no tab shown) goes; `from` is the pane it was in.
pub fn web_place(tab: Option<&Tab>, from: Option<&PaneKey>) -> WebPlace {
    let Some(tab) = tab else {
        return WebPlace::NewTab;
    };
    let keys = tab.keys();
    if let Some(id) = keys.iter().find_map(|key| match key {
        PaneKey::Web(id) => Some(*id),
        _ => None,
    }) {
        return WebPlace::Load(id);
    }
    match from.filter(|key| keys.contains(key)).or(tab.focused()) {
        Some(key) => WebPlace::Split(key.clone()),
        None => WebPlace::NewTab,
    }
}

/// What a tab chip shows for a focused file pane: its name, a dot while it has unsaved text,
/// italics while it is the preview tab (SPEC §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLabel {
    pub name: String,
    pub dirty: bool,
    pub preview: bool,
}

/// The file label of `tab`: its focused pane is a file and the tab keeps its default name (a
/// name the user gave the tab always wins, as for [`session_label`]).
pub fn file_label(tab: &Tab, docs: Option<&Documents>) -> Option<FileLabel> {
    if !has_default_title(tab) {
        return None;
    }
    let key = tab.focused()?;
    let PaneKey::File(file) = key else {
        return None;
    };
    let preview = matches!(
        tab.layout()?.leaf_of(key),
        Some(Leaf::File { preview: true, .. })
    );
    Some(FileLabel {
        name: (file.path.rsplit('/').next().unwrap_or(&file.path)).to_owned(),
        dirty: docs
            .and_then(|docs| docs.get(file))
            .is_some_and(Doc::is_dirty),
        preview,
    })
}

/// The pane a double-click on `tab` renames (SPEC §11): its focused pane while the tab keeps
/// its default name, because the tab then shows that pane's title. `None` renames the tab
/// itself: a name the user gave it, a tab without panes, or a focused run pane (named by its
/// command, never renamed).
pub fn double_click_renames(tab: &Tab, sessions: &[Session]) -> Option<Ulid> {
    let focused = tab.focused_session().filter(|_| has_default_title(tab))?;
    let session = sessions.iter().find(|s| s.id == focused)?;
    mkrew_core::run::command_name(session)
        .is_none()
        .then_some(focused)
}

/// One child of the title bar's tab area, in paint order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AreaItem {
    /// The 1 px rule between two groups.
    Rule,
    /// The `n`th terminal tab this window draws (detached tabs are not counted).
    Tab(usize),
}

/// The children of the tab area: the tabs (`pinned[n]` is whether the `n`th drawn tab is
/// pinned; pinned ones lead) and a rule between the pinned tabs and the rest. A rule only sits
/// between two groups.
pub fn area_items(pinned: &[bool]) -> Vec<AreaItem> {
    let mut items = Vec::with_capacity(pinned.len() + 1);
    for (at, &pin) in pinned.iter().enumerate() {
        if at > 0 && pinned[at - 1] && !pin {
            items.push(AreaItem::Rule);
        }
        items.push(AreaItem::Tab(at));
    }
    items
}

/// The index in `items` of the child to scroll into view: the active tab (`active` is its
/// drawn slot).
pub fn scroll_index(items: &[AreaItem], active: Option<usize>) -> Option<usize> {
    items
        .iter()
        .position(|item| matches!(item, AreaItem::Tab(at) if active == Some(*at)))
}

/// Sessions to attach (in `after` only) and to detach (in `before` only) when the visible
/// panes change from `before` to `after`.
pub fn attach_diff(before: &[Ulid], after: &[Ulid]) -> (Vec<Ulid>, Vec<Ulid>) {
    let attach = after
        .iter()
        .filter(|id| !before.contains(id))
        .copied()
        .collect();
    let detach = before
        .iter()
        .filter(|id| !after.contains(id))
        .copied()
        .collect();
    (attach, detach)
}

#[cfg(test)]
mod tests;
