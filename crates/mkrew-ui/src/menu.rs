//! The model of a popover menu and what a key does in it (SPEC §13, ADR 0085). Pure and tested
//! headlessly: `shell/menu_render.rs` draws it and applies the answers.

use gpui::{Keystroke, SharedString};
use mkrew_protocol::WORKSPACE_COLORS;
use ulid::Ulid;

use crate::board::View;
use crate::icons::Icon;
use crate::sidebar::{Badge, Target};
use crate::split::Preset;

/// Swatches of a color row: "no color", then the workspace palette.
pub const SWATCHES: usize = WORKSPACE_COLORS.len() + 1;

/// The color of swatch `ix` (`None` = no color).
pub fn swatch_color(ix: usize) -> Option<&'static str> {
    ix.checked_sub(1)
        .and_then(|at| WORKSPACE_COLORS.get(at).copied())
}

/// The swatch showing `color`; an unknown color shows as "no color".
pub fn swatch_index(color: Option<&str>) -> usize {
    color
        .and_then(|color| WORKSPACE_COLORS.iter().position(|name| *name == color))
        .map_or(0, |at| at + 1)
}

/// How a row's label is colored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Normal,
    /// Discards or stops something.
    Danger,
    /// The menu's primary action.
    Accent,
}

/// A command's state dot in the Run menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dot {
    Running,
    Exited,
    Stopped,
}

/// What stands before a row's label.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Lead {
    #[default]
    None,
    Icon(Icon),
    Agent(Badge),
    /// A workspace's letter avatar; `color` is a workspace color name.
    Avatar {
        name: String,
        color: Option<String>,
    },
    Layout(Preset),
    Dot(Dot),
}

/// One selectable row.
#[derive(Clone)]
pub struct MenuItem<A> {
    pub label: SharedString,
    /// A muted second line.
    pub sub: Option<SharedString>,
    /// Muted text at the right.
    pub detail: Option<SharedString>,
    /// Shortcut text (`Keymap::label`).
    pub keys: Option<String>,
    /// `Some` draws the ✓ column (checked or not).
    pub check: Option<bool>,
    pub enabled: bool,
    /// A multi-select row: Enter / Space run it and the menu stays open.
    pub toggle: bool,
    pub tone: Tone,
    pub lead: Lead,
    pub tip: Option<SharedString>,
    pub action: A,
}

impl<A> MenuItem<A> {
    pub fn new(label: impl Into<SharedString>, action: A) -> Self {
        Self {
            label: label.into(),
            sub: None,
            detail: None,
            keys: None,
            check: None,
            enabled: true,
            toggle: false,
            tone: Tone::Normal,
            lead: Lead::None,
            tip: None,
            action,
        }
    }

    /// A builder, not `std::ops::Sub`.
    #[allow(clippy::should_implement_trait)]
    pub fn sub(mut self, text: impl Into<SharedString>) -> Self {
        self.sub = Some(text.into());
        self
    }

    pub fn detail(mut self, text: impl Into<SharedString>) -> Self {
        self.detail = Some(text.into());
        self
    }

    /// An empty text (an unbound command) shows nothing.
    pub fn keys(mut self, keys: impl Into<String>) -> Self {
        let keys = keys.into();
        self.keys = (!keys.is_empty()).then_some(keys);
        self
    }

    pub fn checked(mut self, on: bool) -> Self {
        self.check = Some(on);
        self
    }

    pub fn enabled(mut self, on: bool) -> Self {
        self.enabled = on;
        self
    }

    pub fn toggle(mut self) -> Self {
        self.toggle = true;
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    pub fn lead(mut self, lead: Lead) -> Self {
        self.lead = lead;
        self
    }

    pub fn tip(mut self, text: impl Into<SharedString>) -> Self {
        self.tip = Some(text.into());
        self
    }

    pub fn row(self) -> MenuRow<A> {
        MenuRow::Item(self)
    }
}

/// One row of a popover menu. `A` is what a row does (in the binary a closure).
pub enum MenuRow<A> {
    Item(MenuItem<A>),
    /// A row of color swatches: ←/→ move between them, Enter picks (`action` gets the swatch).
    Swatches {
        current: usize,
        count: usize,
        enabled: bool,
        action: A,
    },
    Separator,
    /// A muted group title; not selectable.
    Heading(SharedString),
    /// A muted line (Loading agents…, an error); not selectable.
    Note(SharedString),
}

impl<A> MenuRow<A> {
    /// The workspace palette with `color` marked.
    pub fn swatches(color: Option<&str>, action: A) -> Self {
        MenuRow::Swatches {
            current: swatch_index(color),
            count: SWATCHES,
            enabled: true,
            action,
        }
    }

    /// Whether the keyboard can stop on this row.
    pub fn selectable(&self) -> bool {
        match self {
            MenuRow::Item(item) => item.enabled,
            MenuRow::Swatches { enabled, .. } => *enabled,
            MenuRow::Separator | MenuRow::Heading(_) | MenuRow::Note(_) => false,
        }
    }
}

/// What a key does to an open menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKey {
    /// Move the selection to this row.
    Select(usize),
    /// Move the swatch cursor of this row to this swatch.
    Swatch(usize, usize),
    /// Run this row (and the swatch picked, 0 for an item), then close the menu.
    Activate(usize, usize),
    /// Run this multi-select row; the menu stays open.
    Toggle(usize),
    Close,
    Ignore,
}

/// The first selectable row.
pub fn first<A>(rows: &[MenuRow<A>]) -> Option<usize> {
    rows.iter().position(MenuRow::selectable)
}

/// The last selectable row.
pub fn last<A>(rows: &[MenuRow<A>]) -> Option<usize> {
    rows.iter().rposition(MenuRow::selectable)
}

/// The selectable row `delta` (+1 / -1) steps from `from`, wrapping; `None` when no row is
/// selectable. Without a `from`, +1 gives the first and -1 the last.
pub fn step<A>(rows: &[MenuRow<A>], from: Option<usize>, delta: isize) -> Option<usize> {
    let len = rows.len() as isize;
    if len == 0 {
        return None;
    }
    let mut at = match from {
        Some(at) => (at as isize).min(len - 1),
        None if delta > 0 => -1,
        None => len,
    };
    for _ in 0..len {
        at = (at + delta).rem_euclid(len);
        if rows[at as usize].selectable() {
            return Some(at as usize);
        }
    }
    None
}

/// The action of row `at` (an item or a swatch row).
pub fn action_of<A>(rows: &[MenuRow<A>], at: usize) -> Option<&A> {
    match rows.get(at)? {
        MenuRow::Item(item) => Some(&item.action),
        MenuRow::Swatches { action, .. } => Some(action),
        _ => None,
    }
}

/// Where the swatch cursor starts when row `at` is selected: on the current swatch.
pub fn swatch_start<A>(rows: &[MenuRow<A>], at: usize) -> usize {
    match rows.get(at) {
        Some(MenuRow::Swatches { current, count, .. }) => (*current).min(count.saturating_sub(1)),
        _ => 0,
    }
}

/// The first selectable row right under `heading`, before the next separator or heading.
pub fn after_heading<A>(rows: &[MenuRow<A>], heading: &str) -> Option<usize> {
    let start = rows
        .iter()
        .position(|row| matches!(row, MenuRow::Heading(text) if &**text == heading))?
        + 1;
    rows[start..]
        .iter()
        .take_while(|row| !matches!(row, MenuRow::Separator | MenuRow::Heading(_)))
        .position(MenuRow::selectable)
        .map(|at| start + at)
}

/// The next enabled item after `from` (wrapping) whose label starts with `letter`.
fn ahead<A>(rows: &[MenuRow<A>], from: Option<usize>, letter: char) -> Option<usize> {
    let len = rows.len();
    let start = from.map_or(0, |at| at + 1);
    (0..len).map(|n| (start + n) % len).find(|&at| {
        matches!(&rows[at], MenuRow::Item(item) if item.enabled
            && item.label.chars().next().is_some_and(|c| c.to_lowercase().eq(letter.to_lowercase())))
    })
}

fn swatch_move<A>(
    rows: &[MenuRow<A>],
    selected: Option<usize>,
    swatch: usize,
    right: bool,
) -> MenuKey {
    let Some(at) = selected else {
        return MenuKey::Ignore;
    };
    let Some(MenuRow::Swatches {
        count,
        enabled: true,
        ..
    }) = rows.get(at)
    else {
        return MenuKey::Ignore;
    };
    let last = count.saturating_sub(1);
    let from = swatch.min(last);
    let to = if right {
        (from + 1).min(last)
    } else {
        from.saturating_sub(1)
    };
    if to == swatch {
        MenuKey::Ignore
    } else {
        MenuKey::Swatch(at, to)
    }
}

fn activate<A>(rows: &[MenuRow<A>], selected: Option<usize>, swatch: usize) -> MenuKey {
    let Some(at) = selected else {
        return MenuKey::Ignore;
    };
    match rows.get(at) {
        Some(MenuRow::Item(item)) if item.enabled => {
            if item.toggle {
                MenuKey::Toggle(at)
            } else {
                MenuKey::Activate(at, 0)
            }
        }
        Some(MenuRow::Swatches {
            count,
            enabled: true,
            ..
        }) if *count > 0 => MenuKey::Activate(at, swatch.min(count - 1)),
        _ => MenuKey::Ignore,
    }
}

/// What `keystroke` does in a menu showing `rows` with `selected` and the swatch cursor `swatch`.
/// A `held` key (auto-repeat, e.g. of the Enter that opened the menu) does nothing; so does any
/// Ctrl / Alt / Cmd combination (bound commands close the menu before they act).
pub fn key<A>(
    keystroke: &Keystroke,
    held: bool,
    rows: &[MenuRow<A>],
    selected: Option<usize>,
    swatch: usize,
) -> MenuKey {
    let mods = keystroke.modifiers;
    if held || mods.control || mods.alt || mods.platform || mods.function {
        return MenuKey::Ignore;
    }
    // Rows change under the selection (agents load, a filter narrows the list).
    let selected = selected.filter(|at| *at < rows.len());
    let picked = |to: Option<usize>| match to {
        Some(at) if Some(at) != selected => MenuKey::Select(at),
        _ => MenuKey::Ignore,
    };
    match keystroke.key.as_str() {
        "escape" => MenuKey::Close,
        "down" => picked(step(rows, selected, 1)),
        "up" => picked(step(rows, selected, -1)),
        "home" => picked(first(rows)),
        "end" => picked(last(rows)),
        "left" | "right" => swatch_move(rows, selected, swatch, keystroke.key == "right"),
        "enter" | "space" => activate(rows, selected, swatch),
        key => {
            let mut chars = key.chars();
            match (chars.next(), chars.next()) {
                (Some(letter), None) if letter.is_alphanumeric() => {
                    picked(ahead(rows, selected, letter))
                }
                _ => MenuKey::Ignore,
            }
        }
    }
}

/// What a context menu opens on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    Pane(Ulid),
    FileRow {
        path: String,
        is_dir: bool,
    },
    FileTab(String),
    SidebarRow(Target),
    Card(Ulid),
    /// No owner: the active workspace's menu.
    Workspace,
}

/// What has the keyboard, as far as the context menu cares (gathered by the shell).
#[derive(Debug, Clone, Default)]
pub struct Focus {
    pub view: View,
    /// The session whose terminal pane has the keyboard.
    pub terminal: Option<Ulid>,
    /// The sidebar filter has the keyboard and highlights this row.
    pub sidebar_row: Option<Target>,
    pub files_tree_focused: bool,
    /// The file tree's cursor row and whether it is a folder.
    pub files_cursor: Option<(String, bool)>,
    /// The active file tab of the Editor view.
    pub file_tab: Option<String>,
    /// The Board's selected card, when this workspace has it.
    pub card: Option<Ulid>,
}

/// The menu a right-click would open on what has the keyboard (SPEC §13). The tree is a
/// focusable element, so its focus (not the shown view) says it has the keyboard.
pub fn owner(focus: &Focus) -> Owner {
    if let Some(session) = focus.terminal {
        return Owner::Pane(session);
    }
    if let Some(target) = focus.sidebar_row {
        return Owner::SidebarRow(target);
    }
    if focus.files_tree_focused
        && let Some((path, is_dir)) = &focus.files_cursor
    {
        return Owner::FileRow {
            path: path.clone(),
            is_dir: *is_dir,
        };
    }
    match focus.view {
        View::Editor => focus
            .file_tab
            .clone()
            .map_or(Owner::Workspace, Owner::FileTab),
        View::Board => focus.card.map_or(Owner::Workspace, Owner::Card),
        _ => Owner::Workspace,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Ulid {
        Ulid::from(n)
    }

    #[test]
    fn the_owner_is_what_has_the_keyboard() {
        let pane = Focus {
            view: View::Terminals,
            terminal: Some(id(1)),
            ..Focus::default()
        };
        assert_eq!(owner(&pane), Owner::Pane(id(1)));
        let filter = Focus {
            sidebar_row: Some(Target::Workspace(id(2))),
            ..Focus::default()
        };
        assert_eq!(owner(&filter), Owner::SidebarRow(Target::Workspace(id(2))));
        let tree = Focus {
            view: View::Editor,
            files_tree_focused: true,
            files_cursor: Some(("src".into(), true)),
            file_tab: Some("README.md".into()),
            ..Focus::default()
        };
        assert_eq!(
            owner(&tree),
            Owner::FileRow {
                path: "src".into(),
                is_dir: true
            }
        );
        let editor = Focus {
            files_tree_focused: false,
            ..tree.clone()
        };
        assert_eq!(owner(&editor), Owner::FileTab("README.md".into()));
        let board = Focus {
            view: View::Board,
            card: Some(id(3)),
            ..Focus::default()
        };
        assert_eq!(owner(&board), Owner::Card(id(3)));
        // `owner` trusts the flag (the shell sets it only while the tree is painted), in any view.
        let panel = Focus {
            view: View::Terminals,
            ..tree
        };
        assert!(matches!(owner(&panel), Owner::FileRow { .. }));
        // A hidden tree (flag off, cursor kept) never wins over the shown Board's card.
        let hidden_tree = Focus {
            view: View::Board,
            files_tree_focused: false,
            files_cursor: Some(("src".into(), true)),
            card: Some(id(4)),
            ..Focus::default()
        };
        assert_eq!(owner(&hidden_tree), Owner::Card(id(4)));
    }

    #[test]
    fn the_owner_falls_back_to_the_workspace() {
        // Nothing focused, a filter with nothing highlighted, a tree without a cursor and
        // without a tab, a Board without a selected card, Docs and Git: the workspace menu.
        assert_eq!(owner(&Focus::default()), Owner::Workspace);
        for view in [View::Terminals, View::Docs, View::Diff] {
            let focus = Focus {
                view,
                ..Focus::default()
            };
            assert_eq!(owner(&focus), Owner::Workspace);
        }
        let bare_tree = Focus {
            view: View::Editor,
            files_tree_focused: true,
            ..Focus::default()
        };
        assert_eq!(owner(&bare_tree), Owner::Workspace);
        let board = Focus {
            view: View::Board,
            ..Focus::default()
        };
        assert_eq!(owner(&board), Owner::Workspace);
        // A file tab does not own the keyboard while another view is shown.
        let hidden_tab = Focus {
            view: View::Terminals,
            file_tab: Some("a".into()),
            ..Focus::default()
        };
        assert_eq!(owner(&hidden_tab), Owner::Workspace);
    }

    fn item(label: &'static str) -> MenuRow<u8> {
        MenuItem::new(label, 0u8).row()
    }

    fn off(label: &'static str) -> MenuRow<u8> {
        MenuItem::new(label, 0u8).enabled(false).row()
    }

    fn press(keys: &str, rows: &[MenuRow<u8>], selected: Option<usize>, swatch: usize) -> MenuKey {
        key(
            &Keystroke::parse(keys).unwrap(),
            false,
            rows,
            selected,
            swatch,
        )
    }

    /// Item, separator, note, disabled item, item.
    fn mixed() -> Vec<MenuRow<u8>> {
        vec![
            item("Pin"),
            MenuRow::Separator,
            MenuRow::Note("Loading".into()),
            off("Rename"),
            item("Close"),
        ]
    }

    #[test]
    fn down_and_up_wrap_and_skip_what_cannot_be_selected() {
        let rows = mixed();
        assert_eq!(press("down", &rows, None, 0), MenuKey::Select(0));
        assert_eq!(press("down", &rows, Some(0), 0), MenuKey::Select(4));
        assert_eq!(press("down", &rows, Some(4), 0), MenuKey::Select(0));
        assert_eq!(press("up", &rows, Some(0), 0), MenuKey::Select(4));
        assert_eq!(press("up", &rows, Some(4), 0), MenuKey::Select(0));
        assert_eq!(press("up", &rows, None, 0), MenuKey::Select(4));
    }

    #[test]
    fn home_and_end_go_to_the_first_and_last_enabled_row() {
        let rows = vec![MenuRow::Separator, item("A"), off("B"), item("C"), off("D")];
        assert_eq!(press("home", &rows, Some(3), 0), MenuKey::Select(1));
        assert_eq!(press("end", &rows, Some(1), 0), MenuKey::Select(3));
        assert_eq!(press("home", &rows, Some(1), 0), MenuKey::Ignore);
    }

    #[test]
    fn left_and_right_walk_the_swatches_without_wrapping() {
        let rows = vec![item("Pin"), MenuRow::swatches(Some("blue"), 0)];
        let blue = swatch_index(Some("blue"));
        assert_eq!(
            press("right", &rows, Some(1), blue),
            MenuKey::Swatch(1, blue + 1)
        );
        assert_eq!(
            press("left", &rows, Some(1), blue),
            MenuKey::Swatch(1, blue - 1)
        );
        assert_eq!(press("left", &rows, Some(1), 0), MenuKey::Ignore);
        assert_eq!(
            press("right", &rows, Some(1), SWATCHES - 1),
            MenuKey::Ignore
        );
        assert_eq!(
            press("enter", &rows, Some(1), blue),
            MenuKey::Activate(1, blue)
        );
        // On a plain item the arrows do nothing.
        assert_eq!(press("right", &rows, Some(0), 0), MenuKey::Ignore);
        assert_eq!(swatch_start(&rows, 1), blue);
        assert_eq!(swatch_start(&rows, 0), 0);
    }

    #[test]
    fn a_letter_selects_the_next_enabled_row_that_starts_with_it() {
        let rows = vec![
            item("Pin"),
            item("Split right"),
            off("Split hidden"),
            item("Split down"),
            item("Rename"),
        ];
        assert_eq!(press("s", &rows, None, 0), MenuKey::Select(1));
        assert_eq!(press("s", &rows, Some(1), 0), MenuKey::Select(3));
        assert_eq!(press("s", &rows, Some(3), 0), MenuKey::Select(1), "wraps");
        assert_eq!(press("shift-s", &rows, Some(1), 0), MenuKey::Select(3));
        assert_eq!(press("r", &rows, Some(1), 0), MenuKey::Select(4));
        assert_eq!(press("z", &rows, Some(1), 0), MenuKey::Ignore);
        assert_eq!(press("ctrl-s", &rows, None, 0), MenuKey::Ignore);
    }

    #[test]
    fn type_ahead_ignores_case_and_accents_do_not_break_it() {
        let rows = vec![item("Đà Nẵng"), item("Api")];
        assert_eq!(press("đ", &rows, Some(1), 0), MenuKey::Select(0));
        assert_eq!(press("a", &rows, Some(0), 0), MenuKey::Select(1));
    }

    #[test]
    fn a_held_key_does_nothing() {
        let rows = vec![item("Pin"), item("Split")];
        for keys in ["enter", "space", "escape", "down", "s"] {
            let held = key(&Keystroke::parse(keys).unwrap(), true, &rows, Some(0), 0);
            assert_eq!(held, MenuKey::Ignore, "{keys}");
        }
    }

    #[test]
    fn enter_and_space_run_the_selected_row_and_esc_closes() {
        let rows = vec![item("Pin"), off("Rename")];
        assert_eq!(press("enter", &rows, Some(0), 0), MenuKey::Activate(0, 0));
        assert_eq!(press("space", &rows, Some(0), 0), MenuKey::Activate(0, 0));
        assert_eq!(
            press("enter", &rows, Some(1), 0),
            MenuKey::Ignore,
            "disabled"
        );
        assert_eq!(
            press("enter", &rows, None, 0),
            MenuKey::Ignore,
            "nothing selected"
        );
        assert_eq!(press("escape", &rows, None, 0), MenuKey::Close);
    }

    #[test]
    fn fan_out_rows_toggle_and_stay_open_but_the_run_row_runs() {
        let rows = vec![
            MenuRow::Heading("Run with several agents".into()),
            MenuItem::new("claude", 0u8).toggle().checked(false).row(),
            MenuItem::new("omp", 0u8).toggle().checked(true).row(),
            MenuItem::new("codex", 0u8).toggle().enabled(false).row(),
            MenuItem::new("Run 2 agents", 0u8).tone(Tone::Accent).row(),
        ];
        assert_eq!(press("space", &rows, Some(1), 0), MenuKey::Toggle(1));
        assert_eq!(press("enter", &rows, Some(2), 0), MenuKey::Toggle(2));
        assert_eq!(
            press("space", &rows, Some(3), 0),
            MenuKey::Ignore,
            "a full picker blocks the rest"
        );
        assert_eq!(press("enter", &rows, Some(4), 0), MenuKey::Activate(4, 0));
        assert_eq!(
            press("down", &rows, Some(2), 0),
            MenuKey::Select(4),
            "skips the blocked row"
        );
    }

    #[test]
    fn a_stale_selection_is_treated_as_none() {
        let rows = vec![item("Pin"), item("Close")];
        assert_eq!(press("down", &rows, Some(9), 0), MenuKey::Select(0));
        assert_eq!(press("enter", &rows, Some(9), 0), MenuKey::Ignore);
        assert_eq!(step(&rows, Some(9), -1), Some(0));
    }

    #[test]
    fn a_menu_without_selectable_rows_only_closes() {
        let rows: Vec<MenuRow<u8>> = vec![
            MenuRow::Heading("Run with".into()),
            MenuRow::Note("Loading agents…".into()),
            off("Merge back…"),
        ];
        for keys in [
            "down", "up", "home", "end", "enter", "space", "left", "right", "p",
        ] {
            assert_eq!(press(keys, &rows, None, 0), MenuKey::Ignore, "{keys}");
        }
        assert_eq!(press("escape", &rows, None, 0), MenuKey::Close);
        let empty: Vec<MenuRow<u8>> = Vec::new();
        assert_eq!(press("down", &empty, None, 0), MenuKey::Ignore);
        assert_eq!(first(&empty), None);
        assert_eq!(last(&empty), None);
    }

    #[test]
    fn swatch_helpers_round_trip() {
        for ix in 0..SWATCHES {
            assert_eq!(swatch_index(swatch_color(ix)), ix);
        }
        assert_eq!(swatch_color(0), None);
        assert_eq!(swatch_color(SWATCHES), None);
        assert_eq!(swatch_index(Some("not-a-color")), 0);
    }

    #[test]
    fn action_of_finds_items_and_swatch_rows_only() {
        let rows = vec![
            MenuItem::new("Pin", 7u8).row(),
            MenuRow::Separator,
            MenuRow::swatches(None, 9u8),
        ];
        assert_eq!(action_of(&rows, 0), Some(&7));
        assert_eq!(action_of(&rows, 1), None);
        assert_eq!(action_of(&rows, 2), Some(&9));
        assert_eq!(action_of(&rows, 3), None);
    }

    #[test]
    fn after_heading_stops_at_the_next_group() {
        let rows = vec![
            item("Pin"),
            MenuRow::Heading("Default shell".into()),
            MenuRow::Note("Loading".into()),
            MenuRow::Separator,
            MenuRow::Heading("Color".into()),
            MenuRow::swatches(None, 0),
        ];
        // The shell list has not loaded: the swatches under the next heading are not its rows.
        assert_eq!(after_heading(&rows, "Default shell"), None);
        assert_eq!(after_heading(&rows, "Color"), Some(5));
        assert_eq!(after_heading(&rows, "Missing"), None);
        let loaded = vec![
            MenuRow::Heading("Default shell".into()),
            off("cmd"),
            item("pwsh"),
            MenuRow::Separator,
        ];
        assert_eq!(after_heading(&loaded, "Default shell"), Some(2));
    }
}
