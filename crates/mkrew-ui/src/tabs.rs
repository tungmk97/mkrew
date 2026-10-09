//! Terminal tabs of one workspace (SPEC §10): tab order, the active tab, each tab's split tree
//! and focused pane. Pure and tested headlessly; layout edits return the `tab.set_layout`
//! params to send, and `visible()` + [`attach_diff`] tell which sessions to (de)attach.

use std::collections::VecDeque;

use mkrew_protocol::{
    Event, Session, SessionState, TabRenameParams, TabSetLayoutParams, TerminalTab,
};
use ulid::Ulid;

use crate::split::{Axis, Direction, Node, RESIZE_STEP, Rect};

#[derive(Debug, Clone, PartialEq)]
pub struct Tab {
    pub id: Ulid,
    pub title: String,
    pub position: f64,
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
    /// Ordered by position.
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

    fn sort(&mut self) {
        self.tabs.sort_by(|a, b| {
            a.position
                .total_cmp(&b.position)
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
mod tests {
    use serde_json::json;

    use super::*;

    fn wire(
        project_id: Ulid,
        title: &str,
        position: f64,
        layout: serde_json::Value,
    ) -> TerminalTab {
        TerminalTab {
            id: Ulid::new(),
            project_id,
            title: title.to_owned(),
            position,
            layout,
        }
    }

    fn pane(id: Ulid) -> serde_json::Value {
        json!({"session": id.to_string()})
    }

    #[test]
    fn load_orders_by_position_and_activates_the_first_tab() {
        let project = Ulid::new();
        let [a, b] = [Ulid::new(), Ulid::new()];
        let mut tabs = Tabs::new(project);
        let second = wire(project, "two", 2.0, pane(b));
        let first = wire(project, "one", 1.0, pane(a));
        let foreign = wire(Ulid::new(), "other", 0.0, serde_json::Value::Null);
        tabs.load(vec![second.clone(), first.clone(), foreign]);

        let titles: Vec<&str> = tabs.tabs().iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["one", "two"]);
        assert_eq!(tabs.active().map(|t| t.id), Some(first.id));
        assert_eq!(tabs.visible(), vec![a]);
        assert_eq!(tabs.active().and_then(Tab::focused), Some(a));
    }

    #[test]
    fn load_keeps_active_tab_and_focus() {
        let project = Ulid::new();
        let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
        let one = wire(project, "one", 1.0, pane(a));
        let split = json!({"split":"vertical","ratio":0.5,"children":[pane(b), pane(c)]});
        let two = wire(project, "two", 2.0, split);
        let mut tabs = Tabs::new(project);
        tabs.load(vec![one.clone(), two.clone()]);
        assert!(tabs.focus(c));

        tabs.load(vec![one, two.clone()]);
        assert_eq!(tabs.active().map(|t| t.id), Some(two.id));
        assert_eq!(tabs.active().and_then(Tab::focused), Some(c));
    }

    #[test]
    fn hidden_window_shows_no_session() {
        let project = Ulid::new();
        let [a, b] = [Ulid::new(), Ulid::new()];
        let split = json!({"split":"vertical","ratio":0.5,"children":[pane(a), pane(b)]});
        let mut tabs = Tabs::new(project);
        tabs.load(vec![wire(project, "main", 1.0, split)]);

        assert_eq!(on_screen(true, Some(&tabs)), vec![a, b]);
        assert!(on_screen(false, Some(&tabs)).is_empty());
        assert!(on_screen(true, None).is_empty());
        assert!(on_screen(false, None).is_empty());
    }

    #[test]
    fn unreadable_layout_loads_as_an_empty_tab() {
        let project = Ulid::new();
        let mut tabs = Tabs::new(project);
        tabs.load(vec![wire(
            project,
            "broken",
            1.0,
            json!({"split": "diagonal"}),
        )]);
        assert_eq!(tabs.tabs()[0].layout(), None);
        assert!(tabs.visible().is_empty());
    }

    #[test]
    fn split_close_and_persist_round_trip() {
        let project = Ulid::new();
        let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
        let tab = wire(project, "main", 1.0, serde_json::Value::Null);
        let mut tabs = Tabs::new(project);
        tabs.load(vec![tab.clone()]);

        let first = tabs.split_focused(Axis::Vertical, a).unwrap();
        assert_eq!(first.layout, pane(a));
        tabs.split_focused(Axis::Vertical, b).unwrap();
        let write = tabs.split_focused(Axis::Horizontal, c).unwrap();
        assert_eq!(write.tab_id, tab.id);
        assert_eq!(
            write.layout,
            json!({"split":"vertical","ratio":0.5,"children":[
                pane(a), {"split":"horizontal","ratio":0.5,"children":[pane(b), pane(c)]}
            ]})
        );
        assert_eq!(tabs.active().and_then(Tab::focused), Some(c));
        // A session is shown in one pane only.
        assert_eq!(tabs.split_focused(Axis::Vertical, a), None);

        // Reloading what was written restores the same tree (UI restart).
        let mut restored = Tabs::new(project);
        restored.load(vec![TerminalTab {
            layout: write.layout.clone(),
            ..tab.clone()
        }]);
        assert_eq!(restored.visible(), vec![a, b, c]);

        let write = tabs.close_pane(c).unwrap();
        assert_eq!(
            write.layout,
            json!({"split":"vertical","ratio":0.5,"children":[pane(a), pane(b)]})
        );
        assert_eq!(tabs.active().and_then(Tab::focused), Some(b));
        tabs.close_pane(b).unwrap();
        let write = tabs.close_pane(a).unwrap();
        assert_eq!(write.layout, serde_json::Value::Null);
        assert_eq!(tabs.active().and_then(Tab::focused), None);
        assert_eq!(tabs.close_pane(a), None);
    }

    /// A tab `a | b` (side by side), focused on `b`.
    fn two_panes() -> (Tabs, Ulid, Ulid) {
        let project = Ulid::new();
        let [a, b] = [Ulid::new(), Ulid::new()];
        let mut tabs = Tabs::new(project);
        tabs.load(vec![wire(project, "main", 1.0, serde_json::Value::Null)]);
        tabs.split_focused(Axis::Vertical, a).unwrap();
        tabs.split_focused(Axis::Vertical, b).unwrap();
        (tabs, a, b)
    }

    #[test]
    fn resize_writes_the_moved_divider() {
        let (mut tabs, a, b) = two_panes();
        let write = tabs.resize_focused(Direction::Left).unwrap();
        assert_eq!(
            write.layout,
            json!({"split":"vertical","ratio":0.45,"children":[pane(a), pane(b)]})
        );
        // No stacked split: nothing moves, nothing is written.
        assert_eq!(tabs.resize_focused(Direction::Up), None);
        // The echo of the write keeps the local layout.
        let echo = TerminalTab {
            id: write.tab_id,
            project_id: tabs.project_id(),
            title: "main".into(),
            position: 1.0,
            layout: write.layout.clone(),
        };
        tabs.apply(&Event::TabUpdated(echo));
        let layout = tabs.active().and_then(Tab::layout);
        assert_eq!(Node::to_json(layout), write.layout);
    }

    #[test]
    fn zoom_shows_only_the_focused_pane_until_focus_or_layout_changes() {
        let (mut tabs, a, b) = two_panes();
        assert!(tabs.toggle_zoom());
        assert_eq!(tabs.active().and_then(Tab::zoomed), Some(b));
        assert_eq!(tabs.visible(), vec![b]);
        let rects = tabs.active().unwrap().rects();
        assert_eq!(rects.len(), 1);
        assert_eq!((rects[0].0, rects[0].1.w, rects[0].1.h), (b, 1.0, 1.0));
        // Hidden dividers are not resized while zoomed.
        assert_eq!(tabs.resize_focused(Direction::Left), None);
        assert!(tabs.toggle_zoom());
        assert_eq!(tabs.visible(), vec![a, b]);

        // Moving the focus ends the zoom.
        tabs.toggle_zoom();
        assert!(tabs.focus_neighbour(Direction::Left));
        assert_eq!(tabs.active().and_then(Tab::zoomed), None);
        assert_eq!(tabs.visible(), vec![a, b]);

        // So does a new pane (it must be visible) and closing the zoomed pane.
        tabs.toggle_zoom();
        let c = Ulid::new();
        tabs.split_focused(Axis::Horizontal, c).unwrap();
        assert_eq!(tabs.visible(), vec![a, c, b]);
        tabs.toggle_zoom();
        tabs.close_pane(c).unwrap();
        assert_eq!(tabs.active().and_then(Tab::zoomed), None);
        assert_eq!(tabs.visible(), vec![a, b]);

        // Focusing the zoomed pane from the sidebar keeps the zoom; another pane ends it.
        tabs.toggle_zoom();
        assert!(tabs.focus(a));
        assert_eq!(tabs.visible(), vec![a]);
        assert!(tabs.focus(b));
        assert_eq!(tabs.visible(), vec![a, b]);
    }

    #[test]
    fn a_single_pane_does_not_zoom() {
        let (mut tabs, a, _) = two_panes();
        let mut single = Tabs::new(tabs.project_id());
        assert!(!single.toggle_zoom());
        tabs.close_pane(a).unwrap();
        assert!(!tabs.toggle_zoom());
        assert_eq!(tabs.active().and_then(Tab::zoomed), None);
    }

    #[test]
    fn rename_params_trim_and_skip_blank_or_unchanged_titles() {
        let (tabs, ..) = two_panes();
        let tab = tabs.active().unwrap();
        let params = rename_params(tab, "  build  ").unwrap();
        assert_eq!(params.tab_id, tab.id);
        assert_eq!(params.title, "build");
        assert_eq!(rename_params(tab, "   "), None);
        assert_eq!(rename_params(tab, " main "), None);
    }

    #[test]
    fn focus_moves_between_neighbours() {
        let project = Ulid::new();
        let [a, b] = [Ulid::new(), Ulid::new()];
        let mut tabs = Tabs::new(project);
        tabs.load(vec![wire(project, "main", 1.0, pane(a))]);
        tabs.split_focused(Axis::Vertical, b);
        assert!(tabs.focus_neighbour(Direction::Left));
        assert_eq!(tabs.active().and_then(Tab::focused), Some(a));
        assert!(!tabs.focus_neighbour(Direction::Left));
        assert!(!tabs.focus_neighbour(Direction::Up));
        assert!(tabs.focus_neighbour(Direction::Right));
        assert_eq!(tabs.active().and_then(Tab::focused), Some(b));
    }

    #[test]
    fn echoes_of_own_writes_do_not_revert_newer_local_layouts() {
        let project = Ulid::new();
        let [a, b] = [Ulid::new(), Ulid::new()];
        let tab = wire(project, "main", 1.0, serde_json::Value::Null);
        let mut tabs = Tabs::new(project);
        tabs.load(vec![tab.clone()]);
        let first = tabs.split_focused(Axis::Vertical, a).unwrap();
        tabs.split_focused(Axis::Vertical, b).unwrap();

        // The echo of the first write arrives after the second local edit.
        let echo = |layout: serde_json::Value| {
            Event::TabUpdated(TerminalTab {
                layout,
                ..tab.clone()
            })
        };
        tabs.apply(&echo(first.layout));
        assert_eq!(tabs.visible(), vec![a, b]);
        tabs.apply(&echo(
            json!({"split":"vertical","ratio":0.5,"children":[pane(a), pane(b)]}),
        ));
        assert_eq!(tabs.visible(), vec![a, b]);

        // With nothing pending, an update from elsewhere (e.g. `mk` spawning) applies.
        assert!(tabs.apply(&echo(pane(b))));
        assert_eq!(tabs.visible(), vec![b]);
        assert_eq!(tabs.active().and_then(Tab::focused), Some(b));
    }

    #[test]
    fn failed_write_stops_waiting_for_its_echo() {
        let project = Ulid::new();
        let [a, b] = [Ulid::new(), Ulid::new()];
        let tab = wire(project, "main", 1.0, serde_json::Value::Null);
        let mut tabs = Tabs::new(project);
        tabs.load(vec![tab.clone()]);
        let write = tabs.split_focused(Axis::Vertical, a).unwrap();
        tabs.write_failed(&write);
        assert!(tabs.apply(&Event::TabUpdated(TerminalTab {
            layout: pane(b),
            ..tab
        })));
        assert_eq!(tabs.visible(), vec![b]);
    }

    #[test]
    fn rename_while_a_write_is_pending_keeps_the_local_layout() {
        let project = Ulid::new();
        let [a, b] = [Ulid::new(), Ulid::new()];
        let tab = wire(project, "main", 1.0, pane(a));
        let mut tabs = Tabs::new(project);
        tabs.load(vec![tab.clone()]);
        let write = tabs.split_focused(Axis::Vertical, b).unwrap();

        // A rename (e.g. from `mk`) processed before the write carries the old layout.
        assert!(tabs.apply(&Event::TabUpdated(TerminalTab {
            title: "renamed".to_owned(),
            ..tab.clone()
        })));
        assert_eq!(tabs.visible(), vec![a, b]);
        assert_eq!(tabs.active().map(|t| t.title.as_str()), Some("renamed"));

        // The echo still clears the pending write; later remote layouts apply again.
        tabs.apply(&Event::TabUpdated(TerminalTab {
            title: "renamed".to_owned(),
            layout: write.layout,
            ..tab.clone()
        }));
        assert!(tabs.apply(&Event::TabUpdated(TerminalTab {
            title: "renamed".to_owned(),
            ..tab
        })));
        assert_eq!(tabs.visible(), vec![a]);
    }

    #[test]
    fn updates_add_rename_and_reorder_tabs() {
        let project = Ulid::new();
        let one = wire(project, "one", 1.0, serde_json::Value::Null);
        let mut tabs = Tabs::new(project);
        tabs.load(vec![one.clone()]);

        let zero = wire(project, "zero", 0.5, serde_json::Value::Null);
        assert!(tabs.apply(&Event::TabUpdated(zero.clone())));
        assert!(tabs.apply(&Event::TabUpdated(TerminalTab {
            title: "renamed".to_owned(),
            ..one.clone()
        })));
        let titles: Vec<&str> = tabs.tabs().iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["zero", "renamed"]);
        // The active tab stays the same when another one is added before it.
        assert_eq!(tabs.active().map(|t| t.id), Some(one.id));
        // Unchanged and foreign updates are no-ops.
        assert!(!tabs.apply(&Event::TabUpdated(zero)));
        assert!(!tabs.apply(&Event::TabUpdated(wire(
            Ulid::new(),
            "x",
            0.0,
            serde_json::Value::Null
        ))));
    }

    #[test]
    fn remove_and_cycle_tabs() {
        let project = Ulid::new();
        let [one, two, three] = [
            wire(project, "1", 1.0, serde_json::Value::Null),
            wire(project, "2", 2.0, serde_json::Value::Null),
            wire(project, "3", 3.0, serde_json::Value::Null),
        ];
        let mut tabs = Tabs::new(project);
        tabs.load(vec![one.clone(), two.clone(), three.clone()]);
        tabs.activate_offset(-1);
        assert_eq!(tabs.active().map(|t| t.id), Some(three.id));
        tabs.activate_offset(1);
        assert_eq!(tabs.active().map(|t| t.id), Some(one.id));
        assert!(tabs.activate(two.id));
        assert!(!tabs.activate(Ulid::new()));
        assert!(tabs.activate_nth(2));
        assert_eq!(tabs.active().map(|t| t.id), Some(three.id));
        assert!(!tabs.activate_nth(3));
        assert_eq!(tabs.active().map(|t| t.id), Some(three.id));
        assert!(tabs.activate_nth(1));

        tabs.remove(two.id);
        assert_eq!(tabs.active().map(|t| t.id), Some(three.id));
        tabs.remove(three.id);
        assert_eq!(tabs.active().map(|t| t.id), Some(one.id));
        tabs.remove(one.id);
        assert!(tabs.active().is_none());
        assert!(tabs.is_empty());
        tabs.activate_offset(1);
    }

    #[test]
    fn removing_an_inactive_tab_keeps_the_active_one() {
        let project = Ulid::new();
        let [one, two, three] = [
            wire(project, "1", 1.0, serde_json::Value::Null),
            wire(project, "2", 2.0, serde_json::Value::Null),
            wire(project, "3", 3.0, serde_json::Value::Null),
        ];
        let mut tabs = Tabs::new(project);
        tabs.load(vec![one.clone(), two.clone(), three.clone()]);
        assert!(tabs.activate(three.id));
        tabs.remove(one.id);
        assert_eq!(tabs.active().map(|t| t.id), Some(three.id));
        assert_eq!(tabs.tabs().len(), 2);
    }

    #[test]
    fn prune_drops_panes_of_unknown_sessions() {
        let project = Ulid::new();
        let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
        let split = json!({"split":"horizontal","ratio":0.5,"children":[pane(a), pane(b)]});
        let one = wire(project, "one", 1.0, split);
        let two = wire(project, "two", 2.0, pane(c));
        let mut tabs = Tabs::new(project);
        tabs.load(vec![one.clone(), two.clone()]);

        let writes = tabs.prune(|id| id == a);
        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0].tab_id, one.id);
        assert_eq!(writes[0].layout, pane(a));
        assert_eq!(writes[1].tab_id, two.id);
        assert_eq!(writes[1].layout, serde_json::Value::Null);
        assert!(tabs.prune(|_| true).is_empty());
    }

    #[test]
    fn focus_activates_the_holding_tab() {
        let project = Ulid::new();
        let [a, b] = [Ulid::new(), Ulid::new()];
        let one = wire(project, "one", 1.0, pane(a));
        let two = wire(project, "two", 2.0, pane(b));
        let mut tabs = Tabs::new(project);
        tabs.load(vec![one, two.clone()]);
        assert!(tabs.focus(b));
        assert_eq!(tabs.active().map(|t| t.id), Some(two.id));
        assert_eq!(tabs.tab_of(b).map(|t| t.id), Some(two.id));
        assert!(!tabs.focus(Ulid::new()));
    }

    fn session(project_id: Ulid, state: SessionState, started_at: i64) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id,
            agent_profile: "fake".to_owned(),
            kind: mkrew_protocol::SessionKind::Agent,
            title: None,
            native_session_id: None,
            state,
            state_source: mkrew_protocol::StateSource::Daemon,
            state_changed_at: started_at,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at,
            ended_at: None,
        }
    }

    #[test]
    fn empty_tab_takes_the_newest_live_unplaced_session() {
        let project = Ulid::new();
        let placed = session(project, SessionState::Working, 50);
        let older = session(project, SessionState::Idle, 10);
        let newest = session(project, SessionState::NeedsInput, 30);
        let exited = session(project, SessionState::Exited, 40);
        let foreign = session(Ulid::new(), SessionState::Idle, 60);
        let sessions = [
            placed.clone(),
            older.clone(),
            newest.clone(),
            exited,
            foreign,
        ];
        let mut tabs = Tabs::new(project);
        assert_eq!(newest_unplaced(&tabs, &sessions), None, "no active tab");
        let one = wire(project, "one", 1.0, pane(placed.id));
        let two = wire(project, "two", 2.0, serde_json::Value::Null);
        tabs.load(vec![one, two.clone()]);
        assert_eq!(
            newest_unplaced(&tabs, &sessions),
            None,
            "active tab has panes"
        );
        tabs.activate(two.id);
        assert_eq!(newest_unplaced(&tabs, &sessions), Some(newest.id));
        tabs.split_focused(Axis::Vertical, newest.id);
        assert_eq!(newest_unplaced(&tabs, &sessions), None);
    }

    #[test]
    fn attach_diff_lists_new_and_hidden_sessions() {
        let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
        assert_eq!(attach_diff(&[a, b], &[b, c]), (vec![c], vec![a]));
        assert_eq!(attach_diff(&[a], &[a]), (vec![], vec![]));
    }
}
