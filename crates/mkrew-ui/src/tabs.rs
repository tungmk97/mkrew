//! Terminal tabs of one workspace (SPEC §10): tab order, the active tab, each tab's split tree
//! and focused pane. Pure and tested headlessly; layout edits return the `tab.set_layout`
//! params to send, and `visible()` + [`attach_diff`] tell which sessions to (de)attach.

use std::collections::VecDeque;

use mkrew_protocol::{
    Event, Session, SessionKind, SessionState, TabMoveParams, TabRenameParams, TabSetLayoutParams,
    TerminalTab,
};
use ulid::Ulid;

use crate::split::{Axis, Direction, Divider, Node, Preset, RESIZE_STEP, Rect};

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
    focused: Option<Ulid>,
    /// Local only: the pane filling the whole tab (zoom); always the focused pane.
    zoomed: Option<Ulid>,
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

    pub fn focused(&self) -> Option<Ulid> {
        self.focused
    }

    pub fn zoomed(&self) -> Option<Ulid> {
        self.zoomed
    }

    pub fn sessions(&self) -> Vec<Ulid> {
        self.layout.as_ref().map(Node::sessions).unwrap_or_default()
    }

    /// The one pane on screen (zoomed, or the only pane), which then has no header.
    pub fn single(&self) -> Option<Ulid> {
        match (self.zoomed, self.layout.as_ref()) {
            (Some(session), _) | (None, Some(&Node::Pane { session })) => Some(session),
            _ => None,
        }
    }

    /// Sessions on screen: the zoomed pane alone, else every pane.
    pub fn shown(&self) -> Vec<Ulid> {
        match self.zoomed {
            Some(session) => vec![session],
            None => self.sessions(),
        }
    }

    /// Area of each pane on screen: the zoomed pane fills the tab.
    pub fn rects(&self) -> Vec<(Ulid, Rect)> {
        match (self.zoomed, &self.layout) {
            (Some(session), _) => vec![(
                session,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 1.0,
                    h: 1.0,
                },
            )],
            (None, Some(node)) => node.rects(),
            (None, None) => Vec::new(),
        }
    }

    /// Dividers drawn over the panes; none while a pane is zoomed.
    pub fn dividers(&self) -> Vec<Divider> {
        match (self.zoomed, &self.layout) {
            (None, Some(node)) => node.dividers(),
            _ => Vec::new(),
        }
    }

    pub fn contains(&self, session: Ulid) -> bool {
        self.layout
            .as_ref()
            .is_some_and(|node| node.contains(session))
    }

    /// Keeps the focus on a pane of the tab: the first pane if it was lost. A zoom ends when
    /// the focus leaves the zoomed pane.
    fn fix_focus(&mut self) {
        if !self.focused.is_some_and(|id| self.contains(id)) {
            self.focused = self.sessions().first().copied();
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
                    tab.focused = previous.focused;
                    tab.zoomed = previous.zoomed;
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
        self.tabs.iter().find(|tab| tab.contains(session))
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

    /// Activates the tab `offset` places after the active one, wrapping around.
    pub fn activate_offset(&mut self, offset: isize) {
        let Some(index) = self
            .active
            .and_then(|id| self.tabs.iter().position(|t| t.id == id))
        else {
            return;
        };
        let len = self.tabs.len() as isize;
        let next = (index as isize + offset).rem_euclid(len) as usize;
        self.active = Some(self.tabs[next].id);
    }

    /// Activates the tab at `index` (0-based); false if there is none.
    pub fn activate_nth(&mut self, index: usize) -> bool {
        let Some(tab_id) = self.tabs.get(index).map(|tab| tab.id) else {
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

    /// The tabs "Close others" (`right_only` false) or "Close tabs to the right" closes from
    /// `tab_id`'s menu: the unpinned ones besides it, or only those after it.
    pub fn close_targets(&self, tab_id: Ulid, right_only: bool) -> Vec<Ulid> {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
            return Vec::new();
        };
        (self.tabs.iter().enumerate())
            .filter(|&(i, tab)| !tab.pinned && i != index && (!right_only || i > index))
            .map(|(_, tab)| tab.id)
            .collect()
    }

    /// Sessions shown on screen: the panes of the active tab (only the zoomed one while a pane
    /// is zoomed). All others stay detached.
    pub fn visible(&self) -> Vec<Ulid> {
        self.active().map(Tab::shown).unwrap_or_default()
    }

    /// Activates the tab holding `session` and focuses its pane; false if no tab holds it.
    pub fn focus(&mut self, session: Ulid) -> bool {
        let Some(tab_id) = self.tab_of(session).map(|tab| tab.id) else {
            return false;
        };
        self.active = Some(tab_id);
        if let Some(tab) = self.tab_mut(tab_id) {
            tab.focused = Some(session);
            tab.fix_focus();
        }
        true
    }

    /// Moves the focus in the active tab (ending a zoom); false if there is no pane that way.
    pub fn focus_neighbour(&mut self, direction: Direction) -> bool {
        let Some(tab) = self.active_mut() else {
            return false;
        };
        let next = match (&tab.layout, tab.focused) {
            (Some(node), Some(from)) => node.neighbour(from, direction),
            _ => None,
        };
        if next.is_some() {
            tab.focused = next;
            tab.fix_focus();
        }
        next.is_some()
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
        if tab.sessions().len() < 2 {
            return false;
        }
        tab.zoomed = tab.focused;
        tab.zoomed.is_some()
    }

    /// Moves the divider next to the focused pane of the active tab by one step. `None` (no
    /// write) if no divider moved, or while a pane is zoomed (the dividers are not shown).
    pub fn resize_focused(&mut self, direction: Direction) -> Option<TabSetLayoutParams> {
        let tab = self.active_mut()?;
        if tab.zoomed.is_some() {
            return None;
        }
        let focused = tab.focused?;
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
        if self.tab_of(new).is_some() {
            return None;
        }
        let tab = self.active_mut()?;
        match (&mut tab.layout, tab.focused) {
            (Some(node), Some(target)) => {
                if !node.split(target, axis, new) {
                    return None;
                }
            }
            (layout @ None, _) => *layout = Some(Node::pane(new)),
            // `fix_focus` keeps a pane focused whenever the tab has one.
            (Some(_), None) => return None,
        }
        tab.focused = Some(new);
        tab.fix_focus();
        Some(tab.write())
    }

    /// Lays tab `tab_id` out as `preset` (SPEC §10 layouts): its panes keep their tree
    /// order, then come the `new` sessions it does not show yet. `None` (nothing changes)
    /// unless that makes exactly the preset's panes, or when a new session is shown in
    /// another tab. A zoom ends; the focus stays.
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
        let mut sessions = tab.sessions();
        for id in new {
            if !sessions.contains(id) {
                sessions.push(*id);
            }
        }
        tab.layout = Some(preset.build(&sessions)?);
        tab.zoomed = None;
        tab.fix_focus();
        Some(tab.write())
    }

    /// Removes the pane of `session` from whichever tab holds it; focus moves to the pane that
    /// took its place. `None` if no tab holds it.
    pub fn close_pane(&mut self, session: Ulid) -> Option<TabSetLayoutParams> {
        let tab_id = self.tab_of(session)?.id;
        let tab = self.tab_mut(tab_id)?;
        let closed = tab.layout.take()?.close(session);
        tab.layout = closed.remaining;
        if tab.focused == Some(session) {
            tab.focused = closed.focus;
        }
        tab.fix_focus();
        Some(tab.write())
    }

    /// Removes panes whose session `keep` rejects (e.g. sessions the daemon no longer knows)
    /// and returns the layout writes to send.
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
                    .and_then(|node| node.close(session).remaining);
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
    if tabs.active().is_none() || !tabs.visible().is_empty() {
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

/// What a tab's icon shows (SPEC §10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabKind {
    /// An agent pane; the profile id (its badge is looked up by the caller).
    Agent(String),
    Shell,
    Empty,
}

/// The kind of the tab's focused pane (`fix_focus` keeps one focused whenever the tab has
/// panes). `Empty` without panes or when no session is known for the focused one.
pub fn tab_kind(tab: &Tab, sessions: &[Session]) -> TabKind {
    let focused = tab
        .focused()
        .and_then(|id| sessions.iter().find(|s| s.id == id));
    match focused {
        Some(session) if session.kind == SessionKind::Agent => {
            TabKind::Agent(session.agent_profile.clone())
        }
        Some(_) => TabKind::Shell,
        None => TabKind::Empty,
    }
}

/// The session title a tab shows instead of its own: while one pane fills it (no pane header
/// shows that title) and the tab keeps its default `Terminal` / `Terminal N` name. A name the
/// user gave the tab always wins.
pub fn session_label<'a>(tab: &Tab, sessions: &'a [Session]) -> Option<&'a str> {
    let default = match tab.title.strip_prefix(FIRST_TAB_TITLE) {
        Some("") => true,
        Some(rest) => rest
            .strip_prefix(' ')
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())),
        None => false,
    };
    let session = tab.single().filter(|_| default)?;
    sessions
        .iter()
        .find(|s| s.id == session)
        .and_then(crate::history::title)
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
