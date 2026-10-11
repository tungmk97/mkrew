//! State of the right-click menus of the Terminals view (SPEC §10): the tab menu (also opened by
//! the tab strip's `⋯`), the split button's layout menu and, from the pane header, the pane menu.
//! `main.rs` renders them.

use gpui::{Pixels, Point};
use ulid::Ulid;

/// The open menu of one tab, at `position` (window coordinates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabMenu {
    pub tab_id: Ulid,
    pub position: Point<Pixels>,
}

/// The open menu of one pane (Rename, Change color), at `position` (window coordinates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaneMenu {
    pub session_id: Ulid,
    pub position: Point<Pixels>,
}

/// The split button's menu (Split right / down, the layouts) for one tab, at `position`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutMenu {
    pub tab_id: Ulid,
    pub position: Point<Pixels>,
}
