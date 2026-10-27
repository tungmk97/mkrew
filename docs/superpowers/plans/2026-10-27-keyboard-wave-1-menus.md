# Keyboard wave 1: context menus Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every popover menu of the app takes the keyboard (↑/↓, Home/End, ←/→ on swatches, type-ahead, Enter/Space, Esc), opens from the keyboard with `Shift+F10` on what has the keyboard, and the Board card gets a menu holding its hover actions.

**Architecture:** A pure model in `crates/mkrew-ui/src/menu.rs` (`MenuRow<A>`, `key()`, `step()`, `owner()`) is unit-tested like `confirm.rs`.
One free renderer `render_menu` in a new `shell/menu_render.rs` draws every menu from `Vec<MenuRow<MenuAction>>`, wires click and key to the same `Rc` action, and owns focus (`Shell.menu_focus`, key context `Menu`, `menu_return`).
Each existing `render_*_menu` is cut down to a `*_rows()` builder; `Command::ContextMenu` resolves its owner with the pure `owner()` and anchors the menu on bounds recorded by `canvas` prepaint closures.

**Tech Stack:** Rust, GPUI (`gpui-pre` 0.3.7), existing `mkrew-ui` crate only. No protocol, storage or daemon change.

**Spec:** `docs/superpowers/specs/2026-10-27-keyboard-only-design.md` (section "Wave 1 - Keyboard context menus", "Testing", "SPEC and docs changes").

## Global Constraints

- Every color, spacing, radius, font size and duration comes from `crates/mkrew-ui/src/theme.rs` tokens; no hex or raw pixel literal in components (`.claude/skills/mkrew-ui-design/SKILL.md`, SPEC §14).
- No `unwrap` / `expect` outside tests unless provably infallible (comment why). `cargo fmt --all`; `cargo clippy --workspace --all-targets -- -D warnings` clean at the end of the wave.
- Pure logic gets unit tests (pattern: `crates/mkrew-ui/src/confirm.rs` tests, keymap tests). No test calls a real agent or the network. Do not write tests that pin wording, wiring or defaults.
- Fast loop: `cargo test -p mkrew-ui --lib <filter>`; full verify at the end of the wave: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL). On OOM or linker errors add `-j 2`.
- New default key: `shift-f10` = `Command::ContextMenu`, name `context_menu`, no context (global), unique across all commands. The Windows Menu key arrives as `menu`; users bind it in Settings → Keyboard.
- Keys that only mean something inside a menu (↑/↓, Home/End, ←/→, Enter, Space, Esc, letters) are NOT in `keymap.toml`.
- Closing a menu (Esc, an action, a click outside, any bound command) refocuses the element that had the keyboard before the menu opened.
- A held (auto-repeat) key does nothing in a menu, so the key that opened it cannot pick a row.
- Docs: `docs/decisions/0085-keyboard-only-operation.md` is created by this wave from `docs/decisions/0000-template.md` (waves 2-4 append sections; re-check `docs/decisions/` for the next free number before writing it, `0085` is expected). SPEC §13 and §10 are updated in the same wave. `docs/HUMAN_CHECKS.md` gets a "To verify" entry for dark and light. Never touch `CHANGELOG.md`.
- Markdown: one sentence per physical line; never the em dash character, use "-".
- Commits: Conventional Commits (`feat(ui): ...`), `git -c user.email=tungnv975@gmail.com commit`, stage only the task's files, never push.
- Other waves run after this one on master. Files they also edit (`shell/widgets.rs`, `shell/focus.rs`, `shell/chrome.rs`, `main.rs` imports, `git_render.rs`): re-locate code by the quoted text in this plan, not by line numbers.

## Decisions that refine the spec

- `MenuRow::Item` carries a `MenuItem<A>` struct (label, `sub`, `detail`, `keys`, `check: Option<bool>`, `enabled`, `toggle`, `tone`, `lead`, `tip`, `action`) and the enum has a `Heading` variant, because the current menus draw agent chips, workspace avatars, layout thumbnails, status dots, group headings and tooltips that the spec's minimal `Item` cannot express.
- `MenuKey` and `key()` keep the exact names and arguments of the spec: `key(keystroke, held, rows, selected, swatch) -> MenuKey` with `Select(usize) | Swatch(usize, usize) | Activate(usize, usize) | Toggle(usize) | Close | Ignore`.
- `Shell::close_menus()` keeps its signature (other waves call it). The new `Shell::dismiss_menus(window, cx)` closes and gives the keyboard back; it is what Esc, a click outside, an action and `Shell::run` use.
- The workspace menu floats at an anchor point at the window root (like the tab and pane menus) instead of dropping down inside the sidebar row, so `open_workspace_menu(project_id, anchor, by_keyboard, window, cx)` can place it under the title bar's switcher.
- The Run menu's per-command inline ↻ / ■ / ▶ icon buttons become rows (a command row starts or restarts it, a "Stop <name>" row stops it), because an inline button has no keyboard path.
- `Keymap::key_bindings` also emits `NoAction` bindings in context `Menu` for every context-limited plain or Shift-only key (the Board's arrows, Enter, `e`), so a Board key cannot move a card while a Board menu has the keyboard.
- The Git "⋯" popover (`GitMenu::More`) uses the same `menu_panel` and is not named in the spec; it moves to the renderer with the other two Git popovers.
- The Git popovers join `close_menus`, so a bound command closes them like the other menus.

## Review Focus

1. The keyboard must never stay on a menu that is gone (owner pane closed, workspace switched, dialog opened): `dismiss_menus` restores focus to `menu_return` or falls back to `focus_pane`. Exercised in the Task 11 smoke step (focus is not unit-testable).
2. The selected index goes stale when rows change under it (agents load asynchronously, a fan-out tick disables rows, a repository filter shrinks the list): `key()` must treat an out-of-range `selected` as none. Pinned in Task 1 (`a_stale_selection_is_treated_as_none`).
3. A menu with nothing selectable (Loading agents…, an empty card menu, every row disabled) must ignore ↑/↓/Home/End/Enter and still close on Esc. Pinned in Task 1 (`a_menu_without_selectable_rows_only_closes`).
4. A Board key (↑/↓/←/→, Enter, `e`) pressed while a Board menu is open must move the menu, not the selected card. Pinned in Task 2 (`an_open_menu_keeps_the_single_keys_of_every_view_for_itself`).
5. `Shift+F10` with the owner not painted, no focus at all, a filter box with nothing highlighted, or the Files tree without a cursor must still open a menu (the workspace menu at the switcher). Pinned in Task 10 (`the_owner_falls_back_to_the_workspace`).

---

## File Structure

| File | Responsibility |
| --- | --- |
| `crates/mkrew-ui/src/menu.rs` (new) | Pure model: `MenuRow`, `MenuItem`, `Lead`, `Tone`, `Dot`, `MenuKey`, `key`, `step`, `first`, `last`, `action_of`, `after_heading`, swatch helpers, `Owner`, `Focus`, `owner`. Unit tests. |
| `crates/mkrew-ui/src/shell/menu_render.rs` (new) | `MenuAction`, `Rows`, `act`, `act_swatch`, `Place`, `MenuState`, free `render_menu`, `Shell::{begin_menu, finish_menu, dismiss_menus, menu_key_down, run_menu_item, open_menu_rows, menu_open, menu_select_after, menu_state}`, anchors (`Anchor`, `MenuAnchors`, `record_bounds`, `menu_anchor`), `Shell::context_menu`. |
| `crates/mkrew-ui/src/shell/menus.rs` | `*_rows` builders and thin `render_*_menu` of the tab, layout, pane, share, workspace menus; `agent_menu_rows`. |
| `crates/mkrew-ui/src/shell/widgets.rs` | `color_swatch` gains a keyboard-cursor ring. |
| `crates/mkrew-ui/src/shell/chrome.rs`, `run.rs` | Switcher, "+ New" and Run menu rows. |
| `crates/mkrew-ui/src/shell/file_tabs_render.rs`, `workspace_actions.rs`, `files_render.rs` | File and file tab menus, openers, anchor recording. |
| `crates/mkrew-ui/src/shell/cards.rs`, `board.rs` | Card menu, Run with, fan-out rows; `CardDo`; hover buttons call `card_do`. |
| `crates/mkrew-ui/src/shell/git_menus.rs`, `git_diff.rs`, `git.rs` | Git By:, More, Repositories popovers on the renderer; Repositories ↑/↓. |
| `crates/mkrew-ui/src/keymap.rs`, `palette.rs`, `shell/sessions.rs` | `MENU_CONTEXT`, menu shield bindings, `Command::ContextMenu`. |
| `crates/mkrew-ui/src/main.rs`, `shell.rs`, `shell/connection.rs`, `shell/focus.rs` | New `Shell` fields, imports, `close_menus`, render sites. |
| `docs/decisions/0085-keyboard-only-operation.md` (new), `docs/SPEC.md`, `docs/HUMAN_CHECKS.md` | Docs. |

---

### Task 1: `menu.rs` pure model and key function

**Files:**
- Create: `crates/mkrew-ui/src/menu.rs`
- Modify: `crates/mkrew-ui/src/lib.rs:60` (add `pub mod menu;` after `pub mod markdown;`)

**Interfaces:**
- Consumes: `gpui::{Keystroke, SharedString}`, `crate::icons::Icon`, `crate::sidebar::{Badge, Target}`, `crate::split::Preset`, `crate::board::View`, `mkrew_protocol::WORKSPACE_COLORS`, `ulid::Ulid`.
- Produces (used by Tasks 3-10):
  - `pub struct MenuItem<A>` with `new(label, action)`, builders `sub`, `detail`, `keys`, `checked`, `enabled`, `toggle`, `tone`, `lead`, `tip`, and `row() -> MenuRow<A>`.
  - `pub enum MenuRow<A> { Item(MenuItem<A>), Swatches { current: usize, count: usize, enabled: bool, action: A }, Separator, Heading(SharedString), Note(SharedString) }`, `MenuRow::swatches(current: Option<&str>, action: A)`, `selectable()`.
  - `pub enum Tone { Normal, Danger, Accent }`, `pub enum Dot { Running, Exited, Stopped }`, `pub enum Lead { None, Icon(Icon), Agent(Badge), Avatar { name: String, color: Option<String> }, Layout(Preset), Dot(Dot) }`.
  - `pub enum MenuKey { Select(usize), Swatch(usize, usize), Activate(usize, usize), Toggle(usize), Close, Ignore }`.
  - `pub fn key<A>(keystroke: &Keystroke, held: bool, rows: &[MenuRow<A>], selected: Option<usize>, swatch: usize) -> MenuKey`.
  - `pub fn step<A>(rows: &[MenuRow<A>], from: Option<usize>, delta: isize) -> Option<usize>`, `first`, `last`, `action_of`, `after_heading`, `swatch_start`.
  - `pub const SWATCHES: usize`, `swatch_color(ix) -> Option<&'static str>`, `swatch_index(color: Option<&str>) -> usize`.
  - `pub enum Owner`, `pub struct Focus`, `pub fn owner(&Focus) -> Owner` are added in Task 10.

- [ ] **Step 1: Write the module with its tests**

Create `crates/mkrew-ui/src/menu.rs`:

```rust
//! The model of a popover menu and what a key does in it (SPEC §13, ADR 0085). Pure and tested
//! headlessly: `shell/menu_render.rs` draws it and applies the answers.

use gpui::{Keystroke, SharedString};
use mkrew_protocol::WORKSPACE_COLORS;

use crate::icons::Icon;
use crate::sidebar::Badge;
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
    Avatar { name: String, color: Option<String> },
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

#[cfg(test)]
mod tests {
    use super::*;

    fn item(label: &'static str) -> MenuRow<u8> {
        MenuItem::new(label, 0u8).row()
    }

    fn off(label: &'static str) -> MenuRow<u8> {
        MenuItem::new(label, 0u8).enabled(false).row()
    }

    fn press(keys: &str, rows: &[MenuRow<u8>], selected: Option<usize>, swatch: usize) -> MenuKey {
        key(&Keystroke::parse(keys).unwrap(), false, rows, selected, swatch)
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
        assert_eq!(press("right", &rows, Some(1), blue), MenuKey::Swatch(1, blue + 1));
        assert_eq!(press("left", &rows, Some(1), blue), MenuKey::Swatch(1, blue - 1));
        assert_eq!(press("left", &rows, Some(1), 0), MenuKey::Ignore);
        assert_eq!(press("right", &rows, Some(1), SWATCHES - 1), MenuKey::Ignore);
        assert_eq!(press("enter", &rows, Some(1), blue), MenuKey::Activate(1, blue));
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
        assert_eq!(press("enter", &rows, Some(1), 0), MenuKey::Ignore, "disabled");
        assert_eq!(press("enter", &rows, None, 0), MenuKey::Ignore, "nothing selected");
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
        assert_eq!(press("space", &rows, Some(3), 0), MenuKey::Ignore, "a full picker blocks the rest");
        assert_eq!(press("enter", &rows, Some(4), 0), MenuKey::Activate(4, 0));
        assert_eq!(press("down", &rows, Some(2), 0), MenuKey::Select(4), "skips the blocked row");
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
        for keys in ["down", "up", "home", "end", "enter", "space", "left", "right", "p"] {
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
```

- [ ] **Step 2: Register the module**

In `crates/mkrew-ui/src/lib.rs`, add `pub mod menu;` on the line after `pub mod markdown;`.

- [ ] **Step 3: Run the tests**

Run: `cargo test -p mkrew-ui --lib menu::`
Expected: PASS, 13 tests in `menu::tests`.

- [ ] **Step 4: Commit**

```bash
git add crates/mkrew-ui/src/menu.rs crates/mkrew-ui/src/lib.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): pure popover menu model and key function"
```

---

### Task 2: Menu shield bindings in `keymap.rs`

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs:170` (new const next to the other contexts), `crates/mkrew-ui/src/keymap.rs:823-848` (`Keymap::key_bindings`), `Keymap` impl (new `menu_shielded`), tests module.

**Interfaces:**
- Produces: `pub const MENU_CONTEXT: &str = "Menu"`, `Keymap::menu_shielded(&self) -> impl Iterator<Item = &Keystroke>`; `key_bindings` also returns the `NoAction` bindings.

Why: GPUI runs key *bindings* before `on_key_down` listeners.
The Board binds ↑↓←→, Enter and `e` (`Command::Card`) in context `Board`, which is on the window root, so a menu opened over the Board would never see those keys.
A `NoAction` binding in context `Menu` (the deepest context, so it ranks first) suppresses the shallower binding; the key then reaches `menu_key_down`.

- [ ] **Step 1: Write the failing test**

Append to the `tests` module of `keymap.rs`:

```rust
    #[test]
    fn an_open_menu_keeps_the_single_keys_of_every_view_for_itself() {
        let keymap = Keymap::defaults();
        let shielded: Vec<String> = keymap.menu_shielded().map(Keystroke::unparse).collect();
        let has = |keys: &str| shielded.iter().any(|shielded| shielded == keys);
        for keys in [
            "up", "down", "left", "right", "enter", "e", "shift-left", "shift-right", "s", "b",
        ] {
            assert!(has(keys), "{keys} must stay with the menu");
        }
        // Shortcuts keep working: they close the menu, then act.
        for keys in ["ctrl-enter", "ctrl-shift-p", "alt-1", "f2", "ctrl-alt-m"] {
            assert!(!has(keys), "{keys} must stay bound");
        }
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p mkrew-ui --lib an_open_menu_keeps`
Expected: FAIL to compile with "no method named `menu_shielded`".

- [ ] **Step 3: Implement**

Add the constant after `COMMENTS_CONTEXT` (`keymap.rs`, next to `pub const GIT_CONTEXT`):

```rust
/// Key context of an open popover menu (`Shell::menu_focus`).
pub const MENU_CONTEXT: &str = "Menu";
```

Add to `impl Keymap` (before `key_bindings`):

```rust
    /// Keys an open menu keeps for itself: every context-limited binding on a plain or Shift-only
    /// key (the Board's arrows, Enter and `e`, the Git single keys, later waves' single keys).
    /// Ctrl / Alt / Cmd keys stay bound, so a shortcut still closes the menu and acts.
    pub fn menu_shielded(&self) -> impl Iterator<Item = &Keystroke> {
        self.bindings
            .iter()
            .filter(|(command, keys)| {
                let m = keys.modifiers;
                command.context().is_some() && !(m.control || m.alt || m.platform || m.function)
            })
            .map(|(_, keys)| keys)
    }
```

In `key_bindings`, change the head `self.bindings` + `.collect()` so the result is built, shield bindings are appended, then returned. Replace the whole body of `pub fn key_bindings(&self, mapper: &dyn gpui::PlatformKeyboardMapper) -> Vec<gpui::KeyBinding>` with the existing closure unchanged inside this frame:

```rust
    pub fn key_bindings(&self, mapper: &dyn gpui::PlatformKeyboardMapper) -> Vec<gpui::KeyBinding> {
        let mut bindings: Vec<gpui::KeyBinding> = self
            .bindings
            .iter()
            .filter_map(|(command, keystroke)| {
                // Never fails in practice: the keystroke was parsed and is unparsed back, the
                // contexts are plain identifiers, `COMMENTS_CONTEXT` or `FIND_CONTEXT` (all
                // loaded in the tests).
                let context = match command
                    .context()
                    .map(gpui::KeyBindingContextPredicate::parse)
                {
                    None => None,
                    Some(Ok(predicate)) => Some(predicate.into()),
                    Some(Err(error)) => {
                        tracing::warn!("{command:?}: bad key context: {error}");
                        return None;
                    }
                };
                let keys = keystroke.unparse();
                let run = Box::new(Run(*command));
                gpui::KeyBinding::load(&keys, run, context, true, None, mapper)
                    .inspect_err(|error| tracing::warn!("{command:?}: cannot bind {keys}: {error}"))
                    .ok()
            })
            .collect();
        // The deepest context wins, and `NoAction` hides the shallower binding of the same key.
        if let Ok(predicate) = gpui::KeyBindingContextPredicate::parse(MENU_CONTEXT) {
            let menu: std::rc::Rc<gpui::KeyBindingContextPredicate> = predicate.into();
            bindings.extend(self.menu_shielded().filter_map(|keystroke| {
                let keys = keystroke.unparse();
                gpui::KeyBinding::load(
                    &keys,
                    Box::new(gpui::NoAction),
                    Some(menu.clone()),
                    true,
                    None,
                    mapper,
                )
                .ok()
            }));
        }
        bindings
    }
```

- [ ] **Step 4: Run the test and the keymap suite**

Run: `cargo test -p mkrew-ui --lib keymap`
Expected: PASS (the new test and every existing keymap test).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/keymap.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): an open menu shields the single keys of the views"
```

---

### Task 3: Shell plumbing, the renderer, and the Share menu as first user

**Files:**
- Create: `crates/mkrew-ui/src/shell/menu_render.rs`
- Modify: `crates/mkrew-ui/src/shell.rs:8` (add `pub(crate) mod menu_render;` after `pub(crate) mod menus;`)
- Modify: `crates/mkrew-ui/src/main.rs`: imports (`~:77` keymap import adds `MENU_CONTEXT`; `use shell::menus::*;` neighbours), the `struct Shell` fields (near `workspace_menu`, `~:421`), `use mkrew_ui::menu::...`
- Modify: `crates/mkrew-ui/src/shell/connection.rs:212` (field init next to `workspace_menu: None`) and `~:340` (next to `filter_focus: cx.focus_handle()`)
- Modify: `crates/mkrew-ui/src/shell/focus.rs:29-42` (`close_menus`)
- Modify: `crates/mkrew-ui/src/shell/sessions.rs:141` (`run`: dismiss instead of close)
- Modify: `crates/mkrew-ui/src/shell/widgets.rs:688-720` (`color_swatch` gets `focused`)
- Modify: `crates/mkrew-ui/src/shell/menus.rs` (`render_share_menu` → rows + opener), `crates/mkrew-ui/src/shell/pane_render.rs:732` (share button click)

**Interfaces:**
- Consumes: Task 1's `menu::*`.
- Produces (used by Tasks 4-10):
  - `pub(crate) type MenuAction = Rc<dyn Fn(&mut Shell, usize, &mut Window, &mut Context<'_, Shell>)>`, `pub(crate) type Rows = Vec<MenuRow<MenuAction>>`.
  - `pub(crate) fn act(f: impl Fn(&mut Shell, &mut Window, &mut Context<'_, Shell>) + 'static) -> MenuAction`.
  - `pub(crate) fn act_swatch(f: impl Fn(&mut Shell, Option<&'static str>, &mut Window, &mut Context<'_, Shell>) + 'static) -> MenuAction`.
  - `pub(crate) enum Place { At(Point<Pixels>), Dropdown }`.
  - `pub(crate) struct MenuState<'a> { pub focus: &'a FocusHandle, pub selected: Option<usize>, pub swatch: usize }` (`Clone, Copy`), `Shell::menu_state(&self) -> MenuState<'_>`.
  - `pub(crate) fn render_menu(state: MenuState<'_>, id: &'static str, rows: Rows, header: Option<gpui::AnyElement>, place: Place, theme: &Theme, cx: &mut Context<'_, Shell>) -> gpui::AnyElement`.
  - `Shell::begin_menu(&mut self, window, cx)`: call first in every opener (after any focus change the opener makes itself); it closes other menus and remembers the keyboard owner.
  - `Shell::finish_menu(&mut self, by_keyboard: bool, window, cx)`: call after the opener set its field; selects the first row when `by_keyboard`, focuses `menu_focus`, notifies.
  - `Shell::dismiss_menus(&mut self, window, cx)`, `Shell::menu_open(&self) -> bool`, `Shell::open_menu_rows(&self) -> Option<Rows>` (one arm per menu, added by each task), `Shell::menu_select_after(&mut self, heading: &'static str)`.
  - `Shell` fields: `menu_focus: FocusHandle`, `menu_return: Option<(FocusHandle, Option<Ulid>)>`, `menu_selected: Option<usize>`, `menu_swatch: usize`, `menu_pending: Option<&'static str>`.
  - `color_swatch(id_prefix, color, current, theme, focused: bool, cx, pick)`.

- [ ] **Step 1: Add the `Shell` fields and imports**

In `main.rs`, add to the `keymap::{ ... }` import list `MENU_CONTEXT`, and add after `use mkrew_ui::lifecycle::...`:

```rust
use mkrew_ui::menu::{self as menu_model, Dot, Lead, MenuItem, MenuKey, MenuRow, Tone};
```

and after `use shell::menus::*;`:

```rust
use shell::menu_render::*;
```

In `struct Shell`, after the `tab_menu: Option<TabMenu>,` field and its doc, add:

```rust
    /// Focused while a popover menu is open (any menu, opened by mouse or keyboard): its key
    /// context is `Menu` and [`Shell::menu_key_down`] drives the rows.
    menu_focus: FocusHandle,
    /// What had the keyboard before the first open menu took it (and the terminal session
    /// whose pane it was); Esc, an action or a click outside gives it back.
    menu_return: Option<(FocusHandle, Option<Ulid>)>,
    /// The row the keyboard stands on, and the swatch cursor inside a swatch row.
    menu_selected: Option<usize>,
    menu_swatch: usize,
    /// A group heading whose first row gets the selection as soon as the rows exist (the
    /// agent lists load after the menu opens).
    menu_pending: Option<&'static str>,
```

In `connection.rs`, in the struct literal after `tab_menu: None,` add:

```rust
            menu_focus: cx.focus_handle(),
            menu_return: None,
            menu_selected: None,
            menu_swatch: 0,
            menu_pending: None,
```

(`cx` is the `Context<Shell>` of the constructor, the same one that builds `filter_focus: cx.focus_handle()`.)

- [ ] **Step 2: `close_menus` resets the menu state**

In `shell/focus.rs`, replace the body of `close_menus` (keep the doc, change the last sentence to say it does not move the keyboard):

```rust
    /// Closes every transient popover menu without moving the keyboard; see
    /// [`Self::dismiss_menus`] for the version that gives it back. Called by every bound command
    /// through `dismiss_menus`.
    pub(crate) fn close_menus(&mut self) {
        self.tab_menu = None;
        self.layout_menu = None;
        self.pane_menu = None;
        self.share_menu = None;
        self.workspace_menu = None;
        self.file_menu = None;
        self.file_tab_menu = None;
        self.run_menu = None;
        self.workspace_switcher = false;
        self.new_menu = false;
        self.run_with = None;
        self.fan_out = None;
        self.git_menu = None;
        self.menu_return = None;
        self.menu_selected = None;
        self.menu_swatch = 0;
        self.menu_pending = None;
    }
```

In `shell/sessions.rs` `run`, replace

```rust
        // A shortcut ends every open popover (they never own the keyboard).
        self.close_menus();
```

with

```rust
        // A shortcut ends every open popover and gives the keyboard back before it acts.
        self.dismiss_menus(window, cx);
```

- [ ] **Step 3: `color_swatch` shows the keyboard cursor**

In `shell/widgets.rs`, change the signature and the ring of `color_swatch`:

```rust
pub(crate) fn color_swatch(
    id_prefix: &'static str,
    color: Option<&'static str>,
    current: Option<&str>,
    theme: &Theme,
    focused: bool,
    cx: &mut Context<'_, Shell>,
    pick: impl Fn(&mut Shell, Option<&'static str>, &mut Window, &mut Context<'_, Shell>) + 'static,
) -> gpui::AnyElement {
    let fill = color.and_then(|name| theme.workspace_color(name));
    let ring = if focused {
        theme.accent
    } else if color == current {
        theme.text
    } else {
        fill.unwrap_or(theme.text_muted)
    };
```

and update the doc line to add "`focused` rings it in the accent (the keyboard cursor)". The remaining body is unchanged. The two old callers (`menus.rs` workspace and pane menus) are removed in Tasks 4 and 6; until then add `false` as the new argument after `theme` in both calls so the crate builds.

- [ ] **Step 4: Write `menu_render.rs`**

Create `crates/mkrew-ui/src/shell/menu_render.rs` (anchors are added in Task 6, `context_menu` in Task 10):

```rust
//! The one renderer of every popover menu, the key handler that goes with it and the focus
//! rules (ADR 0085). Each menu builds `Rows`; a click and a key reach the same `action`.

use crate::*;

/// What a row does; the number is the swatch picked (0 for an item).
pub(crate) type MenuAction = Rc<dyn Fn(&mut Shell, usize, &mut Window, &mut Context<'_, Shell>)>;
pub(crate) type Rows = Vec<MenuRow<MenuAction>>;

/// An action that ignores the swatch number (every item).
pub(crate) fn act(
    f: impl Fn(&mut Shell, &mut Window, &mut Context<'_, Shell>) + 'static,
) -> MenuAction {
    Rc::new(
        move |shell: &mut Shell, _: usize, window: &mut Window, cx: &mut Context<'_, Shell>| {
            f(shell, window, cx);
        },
    )
}

/// The action of a color row: it gets the picked color (`None` = no color).
pub(crate) fn act_swatch(
    f: impl Fn(&mut Shell, Option<&'static str>, &mut Window, &mut Context<'_, Shell>) + 'static,
) -> MenuAction {
    Rc::new(
        move |shell: &mut Shell, ix: usize, window: &mut Window, cx: &mut Context<'_, Shell>| {
            f(shell, menu_model::swatch_color(ix), window, cx);
        },
    )
}

/// Where a menu is drawn.
pub(crate) enum Place {
    /// At a window point (a right-click, or the bottom-left of what owns the menu).
    At(Point<Pixels>),
    /// Under the button that holds it (drawn in its place in the tree).
    Dropdown,
}

/// The parts of the `Shell` a menu draws with (the board's free fns get it from `CardContext`).
#[derive(Clone, Copy)]
pub(crate) struct MenuState<'a> {
    pub(crate) focus: &'a FocusHandle,
    pub(crate) selected: Option<usize>,
    pub(crate) swatch: usize,
}

/// The ✓ column, the lead, the label (and its second line), the detail and the shortcut.
fn item_row(
    state: MenuState<'_>,
    id: &'static str,
    ix: usize,
    item: MenuItem<MenuAction>,
    slot: bool,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let row_id = SharedString::from(format!("{id}-{ix}"));
    let enabled = item.enabled;
    let selected = state.selected == Some(ix);
    let color = match item.tone {
        Tone::Normal => theme.text,
        // The destructive color of `danger_button`.
        Tone::Danger => theme.diff_del_text,
        Tone::Accent => theme.accent_text,
    };
    let checked = item.check == Some(true);
    let lead = lead_element(&item.lead, id, ix, color, theme);
    let (action, toggle) = (item.action.clone(), item.toggle);
    // A row that cannot act: the same box without the hover, faded, no click.
    let frame = if enabled {
        menu_item(row_id, theme)
    } else {
        div()
            .id(row_id)
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(radius::SMALL))
            .text_size(font_size::BODY)
            .text_color(rgb(theme.text_muted))
            .opacity(opacity::DISABLED)
    };
    frame
        .when(selected, |el| el.bg(rgb(theme.selected)))
        .when(slot, |el| {
            el.child(
                div()
                    .flex_none()
                    .w(px(space::M))
                    .text_color(rgb(theme.accent_text))
                    .child(if checked { "✓" } else { "" }),
            )
        })
        .children(lead)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .truncate()
                        .text_color(rgb(color))
                        .when(checked, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                        .child(item.label),
                )
                .children(item.sub.map(|sub| meta_line(theme, sub).truncate())),
        )
        .children(
            item.detail
                .map(|detail| div().flex_none().text_size(font_size::META).child(detail)),
        )
        .children(
            item.keys
                .map(|keys| div().flex_none().text_size(font_size::META).child(keys)),
        )
        .when_some(item.tip, |el, tip| el.tooltip(tooltip(theme, &tip, "")))
        .when(enabled, |el| {
            el.on_click(cx.listener(move |shell, _, window, cx| {
                // Not a click on what lies under the menu (a card, a pane).
                cx.stop_propagation();
                shell.run_menu_item(&action, 0, toggle, window, cx);
            }))
        })
        .into_any_element()
}

fn lead_element(
    lead: &Lead,
    id: &str,
    ix: usize,
    color: u32,
    theme: &Theme,
) -> Option<gpui::AnyElement> {
    Some(match lead {
        Lead::None => return None,
        Lead::Icon(glyph) => {
            icon_svg(*glyph, theme::ICON_SIZE_SMALL, color).into_any_element()
        }
        Lead::Agent(badge) => agent_chip(badge.clone(), theme.text, theme).into_any_element(),
        Lead::Avatar { name, color } => workspace_avatar(
            name,
            color.as_deref().and_then(|name| theme.workspace_color(name)),
            theme,
        )
        .into_any_element(),
        Lead::Layout(preset) => layout_thumb(*preset, theme).into_any_element(),
        Lead::Dot(dot) => {
            let (color, hollow) = match dot {
                Dot::Running => (theme.status_done, false),
                Dot::Exited => (theme.status_exited, false),
                Dot::Stopped => (theme.status_idle, true),
            };
            status_dot(SharedString::from(format!("{id}-dot-{ix}")), color, hollow, false)
        }
    })
}

/// A row of color swatches; the keyboard cursor rings one of them.
fn swatch_row(
    state: MenuState<'_>,
    id: &'static str,
    ix: usize,
    (current, count, enabled): (usize, usize, bool),
    action: MenuAction,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let selected = state.selected == Some(ix);
    let current = menu_model::swatch_color(current);
    div()
        .flex()
        .flex_wrap()
        .gap(px(space::S))
        .p(px(space::S))
        .rounded(px(radius::SMALL))
        .when(selected, |el| el.bg(rgb(theme.selected)))
        .when(!enabled, |el| el.opacity(opacity::DISABLED))
        .children((0..count).map(|at| {
            let action = action.clone();
            color_swatch(
                id,
                menu_model::swatch_color(at),
                current,
                theme,
                selected && at == state.swatch,
                cx,
                move |shell, _, window, cx| {
                    if enabled {
                        shell.run_menu_item(&action, at, false, window, cx);
                    }
                },
            )
        }))
        .into_any_element()
}

/// Draws `rows` as one popover. Items, swatches, headings, notes and separators come from
/// `rows`; `header` (the Repositories search box) sits above them. The panel takes the keyboard
/// (key context `Menu`, [`Shell::menu_key_down`]) and a click outside dismisses it.
pub(crate) fn render_menu(
    state: MenuState<'_>,
    id: &'static str,
    rows: Rows,
    header: Option<gpui::AnyElement>,
    place: Place,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let slot = rows
        .iter()
        .any(|row| matches!(row, MenuRow::Item(item) if item.check.is_some()));
    let rows: Vec<gpui::AnyElement> = rows
        .into_iter()
        .enumerate()
        .map(|(ix, row)| match row {
            MenuRow::Separator => div()
                .h(px(theme::BORDER_WIDTH))
                .my(px(space::XS))
                .bg(rgb(theme.border))
                .into_any_element(),
            MenuRow::Heading(text) => div()
                .px(px(space::S))
                .pt(px(space::XS))
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_muted))
                .child(text)
                .into_any_element(),
            MenuRow::Note(text) => div()
                .px(px(space::S))
                .py(px(space::XS))
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(text)
                .into_any_element(),
            MenuRow::Item(item) => item_row(state, id, ix, item, slot, theme, cx),
            MenuRow::Swatches {
                current,
                count,
                enabled,
                action,
            } => swatch_row(state, id, ix, (current, count, enabled), action, theme, cx),
        })
        .collect();
    let panel = menu_panel(id, theme)
        .track_focus(state.focus)
        .key_context(MENU_CONTEXT)
        .on_key_down(cx.listener(Shell::menu_key_down))
        .on_mouse_down_out(cx.listener(|shell, _, window, cx| shell.dismiss_menus(window, cx)))
        .children(header)
        .children(rows);
    match place {
        Place::At(position) => deferred(
            anchored()
                .position(position)
                .snap_to_window_with_margin(px(space::S))
                .child(panel),
        )
        .into_any_element(),
        Place::Dropdown => deferred(
            anchored()
                .snap_to_window_with_margin(px(space::S))
                .child(div().pt(px(space::XS)).child(panel)),
        )
        .into_any_element(),
    }
}

impl Shell {
    pub(crate) fn menu_state(&self) -> MenuState<'_> {
        MenuState {
            focus: &self.menu_focus,
            selected: self.menu_selected,
            swatch: self.menu_swatch,
        }
    }

    /// Whether any popover menu is open.
    pub(crate) fn menu_open(&self) -> bool {
        self.tab_menu.is_some()
            || self.layout_menu.is_some()
            || self.pane_menu.is_some()
            || self.share_menu.is_some()
            || self.workspace_menu.is_some()
            || self.file_menu.is_some()
            || self.file_tab_menu.is_some()
            || self.run_menu.is_some()
            || self.workspace_switcher
            || self.new_menu
            || self.run_with.is_some()
            || self.fan_out.is_some()
            || self.git_menu.is_some()
    }

    /// The rows of the open menu, rebuilt: the keyboard decides on the same rows the screen
    /// shows. One arm per menu.
    pub(crate) fn open_menu_rows(&self) -> Option<Rows> {
        if let Some(menu) = &self.share_menu {
            return Some(self.share_menu_rows(menu));
        }
        None
    }

    /// What has the keyboard now, and the terminal session when it is that session's pane.
    fn keyboard_owner(
        &self,
        window: &Window,
        cx: &App,
    ) -> Option<(FocusHandle, Option<Ulid>)> {
        let terminal = self.focused_session().filter(|session| {
            self.panes
                .get(session)
                .is_some_and(|pane| pane.read(cx).has_focus(window, cx))
        });
        window.focused(cx).map(|focus| (focus, terminal))
    }

    /// First step of every opener: closes any other menu and remembers where the keyboard is
    /// (a menu that is still open keeps what its predecessor remembered). Call it right before
    /// setting the menu's own field, after the opener's own focus changes.
    pub(crate) fn begin_menu(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let back = if self.menu_focus.contains_focused(window, cx) {
            self.menu_return.take()
        } else {
            self.keyboard_owner(window, cx)
        };
        self.close_menus();
        self.menu_return = back;
    }

    /// Last step of every opener, after its field is set: the menu takes the keyboard (opened by
    /// mouse or keyboard); a keyboard open also selects the first enabled row.
    pub(crate) fn finish_menu(
        &mut self,
        by_keyboard: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let rows = self.open_menu_rows();
        self.menu_selected = rows
            .as_deref()
            .filter(|_| by_keyboard)
            .and_then(menu_model::first);
        self.menu_swatch = match (&rows, self.menu_selected) {
            (Some(rows), Some(at)) => menu_model::swatch_start(rows, at),
            _ => 0,
        };
        window.focus(&self.menu_focus, cx);
        cx.notify();
    }

    /// Closes every menu and gives the keyboard back to what had it before the first opened
    /// (unless the keyboard already moved on). Nothing happens when no menu is open.
    pub(crate) fn dismiss_menus(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if !self.menu_open() {
            return;
        }
        let back = self.menu_return.take();
        let held = self.menu_focus.contains_focused(window, cx);
        self.close_menus();
        if held {
            match back {
                Some((focus, session))
                    if session.is_none_or(|session| self.panes.contains_key(&session)) =>
                {
                    window.focus(&focus, cx);
                }
                _ => self.focus_pane(window, cx),
            }
        }
        cx.notify();
    }

    /// A click or Enter on a row: the menu closes first (the keyboard goes back), then the
    /// action runs, so an action that opens a dialog, a field or another menu takes the
    /// keyboard from there. A multi-select row only runs and the menu stays.
    pub(crate) fn run_menu_item(
        &mut self,
        action: &MenuAction,
        swatch: usize,
        toggle: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if toggle {
            action(self, swatch, window, cx);
            cx.notify();
            return;
        }
        self.dismiss_menus(window, cx);
        action(self, swatch, window, cx);
    }

    /// Keys of the open menu (the panel has the keyboard): see [`menu_model::key`].
    pub(crate) fn menu_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(rows) = self.open_menu_rows() else {
            return self.dismiss_menus(window, cx);
        };
        match menu_model::key(
            &event.keystroke,
            event.is_held,
            &rows,
            self.menu_selected,
            self.menu_swatch,
        ) {
            MenuKey::Select(at) => {
                self.menu_selected = Some(at);
                self.menu_swatch = menu_model::swatch_start(&rows, at);
            }
            MenuKey::Swatch(_, to) => self.menu_swatch = to,
            MenuKey::Activate(at, swatch) => {
                if let Some(action) = menu_model::action_of(&rows, at).cloned() {
                    return self.run_menu_item(&action, swatch, false, window, cx);
                }
            }
            MenuKey::Toggle(at) => {
                if let Some(action) = menu_model::action_of(&rows, at).cloned() {
                    return self.run_menu_item(&action, 0, true, window, cx);
                }
            }
            MenuKey::Close => return self.dismiss_menus(window, cx),
            MenuKey::Ignore => return,
        }
        cx.notify();
    }

    /// Selects the first row under `heading` (now, or as soon as the rows exist). The workspace
    /// menu's agent and shell lists load after it opens.
    pub(crate) fn menu_select_after(&mut self, heading: &'static str) {
        self.menu_pending = Some(heading);
        self.apply_menu_pending();
    }

    /// Applies a pending [`Self::menu_select_after`] when its rows are there.
    pub(crate) fn apply_menu_pending(&mut self) {
        let Some(heading) = self.menu_pending else {
            return;
        };
        let Some(rows) = self.open_menu_rows() else {
            return;
        };
        if let Some(at) = menu_model::after_heading(&rows, heading) {
            self.menu_selected = Some(at);
            self.menu_swatch = menu_model::swatch_start(&rows, at);
            self.menu_pending = None;
        }
    }
}
```

`layout_thumb` in `menus.rs` is private; change `fn layout_thumb` to `pub(crate) fn layout_thumb` (the `use shell::menus::*` re-export then makes it visible here).

- [ ] **Step 5: Migrate the Share menu**

In `shell/menus.rs`, replace `render_share_menu` (the whole method, `~:688-735`, doc "The Share menu of an agent pane: …") with:

```rust
    /// The Share menu of an agent pane: "Follow in Zed" (✓ while on).
    pub(crate) fn share_menu_rows(&self, menu: &PaneMenu) -> Rows {
        let session_id = menu.session_id;
        vec![
            MenuItem::new(
                "Follow in Zed",
                act(move |shell, _, cx| shell.toggle_follow(session_id, cx)),
            )
            .checked(self.follow.is_on(session_id))
            .row(),
        ]
    }

    pub(crate) fn render_share_menu(
        &self,
        menu: &PaneMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        render_menu(
            self.menu_state(),
            "share-menu",
            self.share_menu_rows(menu),
            None,
            Place::At(menu.position),
            theme,
            cx,
        )
    }

    /// Opens the Share menu of `session_id` at `position`.
    pub(crate) fn open_share_menu(
        &mut self,
        session_id: Ulid,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.begin_menu(window, cx);
        self.share_menu = Some(PaneMenu {
            session_id,
            position,
        });
        self.finish_menu(window.last_input_was_keyboard(), window, cx);
    }
```

In `open_menu_rows` (Step 4) the share arm already exists. In `pane_render.rs`, replace the share button's click (`.on_click(cx.listener(move |shell, event: &gpui::ClickEvent, _, cx| { shell.close_menus(); shell.share_menu = Some(PaneMenu { ... }); cx.notify(); }))`) with:

```rust
        .on_click(cx.listener(move |shell, event: &gpui::ClickEvent, window, cx| {
            shell.open_share_menu(session, event.position(), window, cx);
        }));
```

Add to `toggle_follow`'s callers nothing: the runner closes the menu first.

- [ ] **Step 6: Build and lint**

Run: `cargo build -p mkrew-ui`
Expected: `Finished` (fix any import the compiler names; the glob `use crate::*` brings the `main.rs` imports).

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings except possibly `dead_code` for `menu_select_after` / `apply_menu_pending` (used from Task 6); if clippy fails only on those two, continue.

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-ui/src/shell/menu_render.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/focus.rs crates/mkrew-ui/src/shell/sessions.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/shell/menus.rs crates/mkrew-ui/src/shell/pane_render.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): one menu renderer with keyboard focus; Share menu on it"
```

---

### Task 4: Pane, tab and layout menus

**Files:**
- Modify: `crates/mkrew-ui/src/shell/menus.rs` (`render_tab_menu` `~:284`, `render_layout_menu` `~:480`, `render_pane_menu` `~:560`)
- Modify: `crates/mkrew-ui/src/shell/tabs.rs:170-200` (`open_tab_menu`, `open_layout_menu`), `:420-439` (`open_pane_menu`)
- Modify: `crates/mkrew-ui/src/shell/pane_render.rs:266-268` (layout button click)
- Modify: `crates/mkrew-ui/src/shell/menu_render.rs` (`open_menu_rows` arms)

**Interfaces:**
- Consumes: Task 3's `Rows`, `act`, `act_swatch`, `render_menu`, `Place`, `begin_menu`, `finish_menu`.
- Produces: `Shell::{tab_menu_rows(&TabMenu) -> Rows, layout_menu_rows(&LayoutMenu) -> Rows, pane_menu_rows(&PaneMenu) -> Option<Rows>}`; `open_layout_menu(position, window, cx)`.

- [ ] **Step 1: Replace the three renderers with builders**

In `menus.rs`, replace `render_tab_menu` with:

```rust
    /// The tab menu (right-click a tab, or the tab bar's ⋯): Pin, Split right / down, Rename,
    /// Close, Close others, Close tabs to the right (pinned tabs stay); on the active tab also
    /// the focused pane's Rename pane, Close pane and, for an agent, Follow in Zed (✓ while on)
    /// and Turns….
    pub(crate) fn tab_menu_rows(&self, menu: &TabMenu) -> Rows {
        let tab_id = menu.tab_id;
        let pinned = (self.tabs.values())
            .find_map(|tabs| tabs.tab(tab_id))
            .is_some_and(|tab| tab.pinned);
        let active_tab = (self.workspace)
            .and_then(|project_id| self.tabs.get(&project_id))
            .and_then(|tabs| tabs.active())
            .map(|tab| tab.id);
        let pane = self
            .focused_session()
            .filter(|_| active_tab == Some(tab_id))
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));
        let tabs = (self.workspace).and_then(|project_id| self.tabs.get(&project_id));
        let has_others = tabs.is_some_and(|tabs| !tabs.close_targets(tab_id, false).is_empty());
        let has_right = tabs.is_some_and(|tabs| !tabs.close_targets(tab_id, true).is_empty());
        let key = |command: Command| self.keymap.label(command);
        let run = |command: Command| {
            act(move |shell, window, cx| shell.run(&Run(command), window, cx))
        };
        let mut rows: Rows = vec![
            MenuItem::new(
                if pinned { "Unpin" } else { "Pin" },
                act(move |shell, window, cx| shell.pin_tab(tab_id, !pinned, window, cx)),
            )
            .row(),
            MenuRow::Separator,
            MenuItem::new("Split right", run(Command::SplitRight))
                .keys(key(Command::SplitRight))
                .row(),
            MenuItem::new("Split down", run(Command::SplitDown))
                .keys(key(Command::SplitDown))
                .row(),
            MenuItem::new(
                "Rename",
                act(|shell, window, cx| shell.start_rename(window, cx)),
            )
            .keys(key(Command::RenameTab))
            .row(),
            MenuItem::new(
                "Close",
                act(move |shell, window, cx| shell.close_tab(tab_id, window, cx)),
            )
            .keys(key(Command::CloseTab))
            .row(),
            MenuItem::new(
                "Close others",
                act(move |shell, window, cx| shell.close_other_tabs(tab_id, false, window, cx)),
            )
            .enabled(has_others)
            .row(),
            MenuItem::new(
                "Close tabs to the right",
                act(move |shell, window, cx| shell.close_other_tabs(tab_id, true, window, cx)),
            )
            .enabled(has_right)
            .row(),
        ];
        if let Some(info) = pane {
            let session = info.id;
            let agent = info.kind == SessionKind::Agent;
            let renamable = mkrew_core::run::command_name(info).is_none();
            rows.push(MenuRow::Separator);
            if renamable {
                rows.push(
                    MenuItem::new(
                        "Rename pane",
                        act(move |shell, window, cx| shell.start_pane_rename(session, window, cx)),
                    )
                    .keys(key(Command::RenamePane))
                    .row(),
                );
            }
            rows.push(
                MenuItem::new(
                    "Close pane",
                    act(|shell, window, cx| shell.close_focused_pane(window, cx)),
                )
                .keys(key(Command::ClosePane))
                .row(),
            );
            if agent {
                let following = self.follow.is_on(session);
                rows.push(
                    MenuItem::new(
                        "Follow in Zed",
                        act(move |shell, _, cx| shell.toggle_follow(session, cx)),
                    )
                    .detail(if following { "✓" } else { "" })
                    .row(),
                );
            }
            if agent && self.features.turns {
                rows.push(
                    MenuItem::new(
                        "Turns…",
                        act(move |shell, window, cx| shell.open_turns(session, window, cx)),
                    )
                    .tip("The agent's turns: restore the files to before one")
                    .row(),
                );
            }
        }
        rows
    }

    pub(crate) fn render_tab_menu(
        &self,
        menu: &TabMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        render_menu(
            self.menu_state(),
            "tab-menu",
            self.tab_menu_rows(menu),
            None,
            Place::At(menu.position),
            theme,
            cx,
        )
    }
```

Replace `render_layout_menu` with:

```rust
    /// The split button's menu (SPEC §10): Split right, Split down, Equalize panes, then the
    /// layouts, each with its thumbnail and shortcut. A layout holding fewer panes than the tab
    /// has is greyed (its tooltip says why).
    pub(crate) fn layout_menu_rows(&self, menu: &LayoutMenu) -> Rows {
        let panes = (self.tabs.values())
            .find_map(|tabs| tabs.tab(menu.tab_id))
            .map_or(0, |tab| tab.sessions().len());
        let key = |command: Command| self.keymap.label(command);
        let run = |command: Command| {
            act(move |shell, window, cx| shell.run(&Run(command), window, cx))
        };
        let mut rows: Rows = vec![
            MenuItem::new("Split right", run(Command::SplitRight))
                .keys(key(Command::SplitRight))
                .row(),
            MenuItem::new("Split down", run(Command::SplitDown))
                .keys(key(Command::SplitDown))
                .row(),
            MenuItem::new("Equalize panes", run(Command::EqualizePanes))
                .keys(key(Command::EqualizePanes))
                .row(),
            MenuRow::Separator,
            MenuRow::Heading("Layouts".into()),
        ];
        rows.extend(Preset::ALL.into_iter().map(|preset| {
            let command = Command::Layout(preset);
            let fits = panes <= preset.panes();
            let item = MenuItem::new(preset.label(), run(command))
                .lead(Lead::Layout(preset))
                .keys(key(command))
                .enabled(fits);
            let item = if fits {
                item
            } else {
                item.tip(format!("This tab has {panes} panes; close some first"))
            };
            item.row()
        }));
        rows
    }

    pub(crate) fn render_layout_menu(
        &self,
        menu: &LayoutMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        render_menu(
            self.menu_state(),
            "layout-menu",
            self.layout_menu_rows(menu),
            None,
            Place::At(menu.position),
            theme,
            cx,
        )
    }
```

Replace `render_pane_menu` with:

```rust
    /// The pane menu: Pin, Mark unread, Rename (greyed for a run pane, which is named by its
    /// command) and the color swatches of the workspace palette. `None` when the pane is gone.
    pub(crate) fn pane_menu_rows(&self, menu: &PaneMenu) -> Option<Rows> {
        let session_id = menu.session_id;
        let session = self
            .sidebar
            .sessions()
            .iter()
            .find(|s| s.id == session_id)?;
        let renamable = mkrew_core::run::command_name(session).is_none();
        let pinned = session.pinned;
        let markable = inbox::can_mark_unread(session, &self.viewed);
        Some(vec![
            MenuItem::new(
                if pinned { "Unpin" } else { "Pin" },
                act(move |shell, window, cx| shell.pin_session(session_id, !pinned, window, cx)),
            )
            .row(),
            MenuItem::new(
                "Mark unread",
                act(move |shell, _, cx| shell.mark_unread(session_id, cx)),
            )
            .enabled(markable)
            .row(),
            MenuRow::Separator,
            MenuItem::new(
                "Rename",
                act(move |shell, window, cx| shell.start_pane_rename(session_id, window, cx)),
            )
            .keys(self.keymap.label(Command::RenamePane))
            .enabled(renamable)
            .row(),
            MenuRow::Separator,
            MenuRow::Heading("Change color".into()),
            MenuRow::swatches(
                session.color.as_deref(),
                act_swatch(move |shell, color, window, cx| {
                    shell.set_pane_color(session_id, color, window, cx);
                }),
            ),
        ])
    }

    pub(crate) fn render_pane_menu(
        &self,
        menu: &PaneMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        let rows = self.pane_menu_rows(menu)?;
        Some(render_menu(
            self.menu_state(),
            "pane-menu",
            rows,
            None,
            Place::At(menu.position),
            theme,
            cx,
        ))
    }
```

The call site in `main.rs` (`.and_then(|menu| self.render_pane_menu(menu, &theme, cx))`) keeps working. `mark_unread` today starts with `self.pane_menu = None;` (`tabs.rs:359`); delete that line (the runner already closed the menu).

- [ ] **Step 2: Openers**

In `tabs.rs` replace `open_tab_menu` and `open_layout_menu` and `open_pane_menu`:

```rust
    /// Opens the menu of `tab_id` at `position`: the tab becomes the active one first, because
    /// Split and Rename act on the active tab.
    pub(crate) fn open_tab_menu(
        &mut self,
        tab_id: Ulid,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(tabs) = self.active_tabs_mut() {
            tabs.activate(tab_id);
        }
        self.sync_panes(window, cx);
        // After the tab switch: the keyboard goes back to the pane that is on screen now.
        self.begin_menu(window, cx);
        self.tab_menu = Some(TabMenu { tab_id, position });
        self.finish_menu(window.last_input_was_keyboard(), window, cx);
    }

    /// Opens the split button's menu for the active tab at `position` (SPEC §10).
    pub(crate) fn open_layout_menu(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(tab_id) = (self.workspace)
            .and_then(|id| self.tabs.get(&id))
            .and_then(|tabs| tabs.active())
            .map(|tab| tab.id)
        else {
            return;
        };
        self.begin_menu(window, cx);
        self.layout_menu = Some(LayoutMenu { tab_id, position });
        self.finish_menu(window.last_input_was_keyboard(), window, cx);
    }
```

```rust
    /// Opens the menu of `session_id`'s pane at `position`, focusing the pane.
    pub(crate) fn open_pane_menu(
        &mut self,
        session_id: Ulid,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(tabs) = self.active_tabs_mut() {
            tabs.focus(session_id);
        }
        // Keyboard focus follows the highlighted pane (typed keys, F2, Ctrl+W).
        self.focus_pane(window, cx);
        self.begin_menu(window, cx);
        self.pane_menu = Some(PaneMenu {
            session_id,
            position,
        });
        self.finish_menu(window.last_input_was_keyboard(), window, cx);
    }
```

In `pane_render.rs` the layout button click becomes:

```rust
                    .on_click(cx.listener(|shell, event: &gpui::ClickEvent, window, cx| {
                        shell.open_layout_menu(event.position(), window, cx);
                    })),
```

- [ ] **Step 3: Add the `open_menu_rows` arms**

In `menu_render.rs` `open_menu_rows`, before the share arm add:

```rust
        if let Some(menu) = &self.tab_menu {
            return Some(self.tab_menu_rows(menu));
        }
        if let Some(menu) = &self.layout_menu {
            return Some(self.layout_menu_rows(menu));
        }
        if let Some(menu) = &self.pane_menu {
            return self.pane_menu_rows(menu);
        }
```

- [ ] **Step 4: Build and lint**

Run: `cargo build -p mkrew-ui` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: `Finished`; clippy clean apart from the Task 3 `dead_code` pair.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/shell/menus.rs crates/mkrew-ui/src/shell/tabs.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/menu_render.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): tab, layout and pane menus take the keyboard"
```

---

### Task 5: Workspace switcher, "+ New" and Run menus

**Files:**
- Modify: `crates/mkrew-ui/src/shell/chrome.rs` (`render_workspace_switcher`: the `button`'s `.on_click` and the `let menu = (self.workspace_switcher ...)` block; `render_new_button`: its `.on_click` and `let menu = (self.new_menu ...)` block)
- Modify: `crates/mkrew-ui/src/shell/run.rs:283-364` (`render_run_button`: click and menu site), `:366-559` (`render_run_menu` → `run_menu_rows`)
- Modify: `crates/mkrew-ui/src/shell/menu_render.rs` (`open_menu_rows` arms)

**Interfaces:**
- Consumes: Task 3 helpers.
- Produces: `Shell::{open_switcher, open_new_menu, open_run_menu}(window, cx)` (open_run_menu takes `project_id` first), `switcher_rows(project_id) -> Rows`, `new_menu_rows() -> Rows`, `run_menu_rows(project_id) -> Rows`.

- [ ] **Step 1: Openers and rows in `chrome.rs`**

Add to `impl Shell` in `chrome.rs`:

```rust
    pub(crate) fn open_switcher(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.begin_menu(window, cx);
        self.workspace_switcher = true;
        self.finish_menu(window.last_input_was_keyboard(), window, cx);
    }

    pub(crate) fn open_new_menu(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.begin_menu(window, cx);
        self.new_menu = true;
        self.finish_menu(window.last_input_was_keyboard(), window, cx);
    }

    /// Every workspace (the shown one checked), then "Add workspace…".
    pub(crate) fn switcher_rows(&self, project_id: Ulid) -> Rows {
        let mut rows: Rows = self
            .sidebar
            .projects()
            .iter()
            .map(|project| {
                let id = project.id;
                MenuItem::new(
                    project.name.clone(),
                    act(move |shell, window, cx| shell.activate_workspace(id, window, cx)),
                )
                .lead(Lead::Avatar {
                    name: project.name.clone(),
                    color: project.color.clone(),
                })
                .checked(id == project_id)
                .row()
            })
            .collect();
        rows.push(MenuRow::Separator);
        rows.push(
            MenuItem::new(
                "Add workspace…",
                act(|shell, window, cx| shell.add_workspace(window, cx)),
            )
            .row(),
        );
        rows
    }

    /// One row per installed agent profile (the default first and checked, with its shortcut),
    /// the plain "Agent" row while none is listed, then "Shell".
    pub(crate) fn new_menu_rows(&self) -> Rows {
        let mut rows: Rows = Vec::new();
        for agent in self.pane_agents() {
            let profile = agent.id.clone();
            let item = MenuItem::new(
                agent.name.clone(),
                act(move |shell, window, cx| {
                    shell.show_view(View::Terminals, window, cx);
                    shell.new_pane_with(
                        SessionKind::Agent,
                        Some(profile.clone()),
                        Axis::Vertical,
                        window,
                        cx,
                    );
                }),
            )
            .lead(Lead::Agent(sidebar::agent_badge(
                agent.icon.as_deref(),
                &agent.id,
            )))
            .checked(agent.default);
            rows.push(
                if agent.default {
                    item.keys(self.keymap.label(Command::SplitAgent))
                } else {
                    item
                }
                .row(),
            );
        }
        if rows.is_empty() {
            rows.push(
                MenuItem::new(
                    "Agent",
                    act(|shell, window, cx| {
                        shell.show_view(View::Terminals, window, cx);
                        shell.new_pane(SessionKind::Agent, Axis::Vertical, window, cx);
                    }),
                )
                .keys(self.keymap.label(Command::SplitAgent))
                .row(),
            );
        }
        rows.push(
            MenuItem::new(
                "Shell",
                // The title bar shows it in every view; the new pane is seen in Terminals.
                act(|shell, window, cx| {
                    shell.show_view(View::Terminals, window, cx);
                    shell.new_pane(SessionKind::Shell, Axis::Vertical, window, cx);
                }),
            )
            .keys(self.keymap.label(Command::SplitShell))
            .row(),
        );
        rows
    }
```

In `render_workspace_switcher`: change the button's click to

```rust
            // An open menu already closed on this mouse down (outside it): the click reopens it.
            .on_click(cx.listener(|shell, _, window, cx| {
                shell.open_switcher(window, cx);
            }))
```

and replace the whole `let menu = (self.workspace_switcher && !self.modal_open()).then(|| { ... deferred(...) });` block (from `let menu =` through its closing `});`) with:

```rust
        let menu = (self.workspace_switcher && !self.modal_open()).then(|| {
            render_menu(
                self.menu_state(),
                "workspace-switch-panel",
                self.switcher_rows(project_id),
                None,
                Place::Dropdown,
                theme,
                cx,
            )
        });
```

In `render_new_button`: change its `.on_click(cx.listener(|shell, _, _, cx| { shell.new_menu = true; cx.notify(); }))` to `.on_click(cx.listener(|shell, _, window, cx| { shell.open_new_menu(window, cx); }))`, and replace the `let menu = (self.new_menu && !self.modal_open()).then(|| { ... });` block with:

```rust
        let menu = (self.new_menu && !self.modal_open()).then(|| {
            render_menu(
                self.menu_state(),
                "new-pane-panel",
                self.new_menu_rows(),
                None,
                Place::Dropdown,
                theme,
                cx,
            )
        });
```

(The final line `div().child(button).children(menu).into_any_element()` is unchanged. In the switcher the last line `.child(div().child(button).children(menu))` is unchanged.)

- [ ] **Step 2: The Run menu**

In `run.rs`, add the opener and replace `render_run_menu` (`~:366-559`, doc "`RUNNING IN <WORKSPACE>`: …") with the builder; keep its doc:

```rust
    pub(crate) fn open_run_menu(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.begin_menu(window, cx);
        self.run_menu = Some(project_id);
        self.finish_menu(window.last_input_was_keyboard(), window, cx);
    }

    /// `RUNNING IN <WORKSPACE>`: one row per command (state dot, name, ports or state; Enter
    /// starts it, or restarts it while it runs, and a running command has a "Stop" row under
    /// it); then Restart all / Stop all while anything runs, Start all while nothing does,
    /// Show Run tab, and Run setup… (which runs the commands after saving when none were set up
    /// yet).
    pub(crate) fn run_menu_rows(&self, project_id: Ulid) -> Rows {
        let summary = self.run_summary(project_id);
        let live = summary.any_live();
        let workspace = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .map_or(String::new(), |project| project.name.to_uppercase());
        let key = |command: Command| self.keymap.label(command);
        let run = move |action: RunAction, name: Option<String>| {
            act(move |shell, window, cx| {
                shell.run_action(project_id, action, name.clone(), window, cx);
            })
        };
        let mut rows: Rows = vec![MenuRow::Heading(
            if live {
                format!("RUNNING IN {workspace}")
            } else {
                format!("RUN IN {workspace}")
            }
            .into(),
        )];
        for row in &summary.rows {
            let dot = match row.state {
                RowState::Running(_) => menu_model::Dot::Running,
                RowState::Exited => menu_model::Dot::Exited,
                RowState::Stopped => menu_model::Dot::Stopped,
            };
            if matches!(row.state, RowState::Running(_)) {
                let ports: Vec<String> = row.ports.iter().map(|p| format!(":{p}")).collect();
                rows.push(
                    MenuItem::new(
                        row.name.clone(),
                        run(RunAction::Restart, Some(row.name.clone())),
                    )
                    .lead(Lead::Dot(dot))
                    .detail(ports.join(" "))
                    .tip("Restart this command")
                    .row(),
                );
                rows.push(
                    MenuItem::new(
                        format!("Stop {}", row.name),
                        run(RunAction::Stop, Some(row.name.clone())),
                    )
                    .lead(Lead::Icon(Icon::CircleStop))
                    .tone(Tone::Danger)
                    .tip("Stop this command")
                    .row(),
                );
            } else {
                // Its state in words (never color alone).
                let state = run::state_text(&row.state)
                    .trim_start_matches(['○', '✕', ' '])
                    .to_owned();
                rows.push(
                    MenuItem::new(
                        row.name.clone(),
                        run(RunAction::Start, Some(row.name.clone())),
                    )
                    .lead(Lead::Dot(dot))
                    .detail(state)
                    .tip("Run this command")
                    .row(),
                );
            }
        }
        if !summary.rows.is_empty() {
            rows.push(MenuRow::Separator);
        }
        let commands = rows.len();
        if live {
            rows.push(
                MenuItem::new("Restart all", run(RunAction::Restart, None))
                    .lead(Lead::Icon(Icon::RotateCw))
                    .keys(key(Command::RunRestart))
                    .row(),
            );
            rows.push(
                MenuItem::new("Stop all", run(RunAction::Stop, None))
                    .lead(Lead::Icon(Icon::CircleStop))
                    .tone(Tone::Danger)
                    .keys(key(Command::RunStop))
                    .row(),
            );
        } else if !summary.rows.is_empty() {
            rows.push(
                MenuItem::new("Start all", run(RunAction::Start, None))
                    .lead(Lead::Icon(Icon::Play))
                    .keys(key(Command::RunStart))
                    .row(),
            );
        }
        if rows.len() > commands {
            rows.push(MenuRow::Separator);
        }
        // Nothing set up yet: Save in the setup also runs them (the old Run button's click).
        let run_after = summary.rows.is_empty();
        rows.push(
            MenuItem::new(
                "Show Run tab",
                act(move |shell, window, cx| shell.show_run_tab(project_id, window, cx)),
            )
            .lead(Lead::Icon(Icon::SquareTerminal))
            .row(),
        );
        rows.push(
            MenuItem::new(
                "Run setup…",
                act(move |shell, window, cx| {
                    shell.open_run_setup(project_id, run_after, window, cx);
                }),
            )
            .lead(Lead::Icon(Icon::Settings))
            .row(),
        );
        rows
    }
```

In `render_run_button`: the pill's `.on_click(cx.listener(move |shell, _, _, cx| { shell.run_menu = Some(project_id); cx.notify(); }))` becomes `.on_click(cx.listener(move |shell, _, window, cx| { shell.open_run_menu(project_id, window, cx); }))`, and the open block becomes:

```rust
        let open = self.run_menu == Some(project_id) && !self.modal_open();
        div()
            .child(pill)
            .when(open, |el| {
                el.child(render_menu(
                    self.menu_state(),
                    "run-menu-panel",
                    self.run_menu_rows(project_id),
                    None,
                    Place::Dropdown,
                    theme,
                    cx,
                ))
            })
            .into_any_element()
```

Note `show_run_tab` and `open_run_setup` already set `self.run_menu = None` themselves; keep that.

- [ ] **Step 3: Add the `open_menu_rows` arms**

In `menu_render.rs` `open_menu_rows`, add before `None`:

```rust
        if let Some(project_id) = self.run_menu {
            return Some(self.run_menu_rows(project_id));
        }
        if self.workspace_switcher {
            return self.workspace.map(|project_id| self.switcher_rows(project_id));
        }
        if self.new_menu {
            return Some(self.new_menu_rows());
        }
```

- [ ] **Step 4: Build and lint**

Run: `cargo build -p mkrew-ui` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: `Finished`; unused imports from the removed renderers (`agent_chip`, `kbd`) may need pruning only if they are now unused in that file.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/shell/chrome.rs crates/mkrew-ui/src/shell/run.rs crates/mkrew-ui/src/shell/menu_render.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): switcher, + New and Run menus take the keyboard"
```

---

### Task 6: Workspace menu (floating, anchored) and the anchor registry

**Files:**
- Modify: `crates/mkrew-ui/src/workspace_menu.rs:7-40` (`WorkspaceMenu` gets a position)
- Modify: `crates/mkrew-ui/src/shell/menus.rs:1-185` (`render_workspace_menu` → rows), `:720-745` (delete `with_workspace_menu`), `:760-850` (`agent_rows` → `agent_menu_rows`)
- Modify: `crates/mkrew-ui/src/shell/workspace_actions.rs:282-315` (`open_workspace_menu`)
- Modify: `crates/mkrew-ui/src/shell/sidebar.rs` (`with_workspace_menu` callers `~:358`, `~:725`, `~:918`; the four `open_workspace_menu` callers `~:1210`, `~:1281`, `~:1538`, `~:1554`)
- Modify: `crates/mkrew-ui/src/shell/sessions.rs:122` (`ask_for_default`), `crates/mkrew-ui/src/shell/navigation.rs:643-645` (palette extras)
- Modify: `crates/mkrew-ui/src/main.rs` (render site, `Shell::render` anchors clear, `cx.set_global(MenuAnchors::default())`), `crates/mkrew-ui/src/shell/chrome.rs` (record the switcher), `crates/mkrew-ui/src/shell/menu_render.rs` (anchors, arm)

**Interfaces:**
- Produces:
  - `WorkspaceMenu { project_id, agents, position: Point<Pixels> }`, `WorkspaceMenu::new(project_id, position)` (derive drops `Eq`).
  - `Shell::open_workspace_menu(&mut self, project_id: Ulid, anchor: Point<Pixels>, by_keyboard: bool, window: &mut Window, cx: &mut Context<'_, Self>)`.
  - `Shell::open_workspace_menu_at_switcher(&mut self, project_id: Ulid, window, cx)` (anchor = the switcher's bottom-left, keyboard open).
  - `pub(crate) const DEFAULT_AGENT_HEADING: &str = "Default agent for new panes"`, `pub(crate) const DEFAULT_SHELL_HEADING: &str = "Default shell for new panes"` in `menus.rs`.
  - `agent_menu_rows(agents, kind, current, pick) -> Rows`.
  - Anchors: `pub(crate) enum Anchor { Switcher }` (Task 10 adds the other variants), `MenuAnchors` global, `record_bounds(key, cx) -> impl IntoElement`, `Shell::menu_anchor(&self, key: &Anchor, window: &Window, cx: &App) -> Point<Pixels>`.

- [ ] **Step 1: `WorkspaceMenu` has a position**

In `workspace_menu.rs` replace the struct and `new`:

```rust
/// The open menu of one workspace, floating at `position` (window coordinates).
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceMenu {
    pub project_id: Ulid,
    pub agents: Agents,
    pub position: gpui::Point<gpui::Pixels>,
}
```

```rust
impl WorkspaceMenu {
    pub fn new(project_id: Ulid, position: gpui::Point<gpui::Pixels>) -> Self {
        Self {
            project_id,
            agents: Agents::Loading,
            position,
        }
    }
}
```

- [ ] **Step 2: The anchor registry in `menu_render.rs`**

Append to `menu_render.rs`:

```rust
/// What a menu opened from the keyboard hangs from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Anchor {
    /// The title bar's workspace switcher: where every owner falls back to.
    Switcher,
}

/// Where owners were laid out in the last frame (recorded by [`record_bounds`] during
/// prepaint, cleared at the start of every `Shell` render).
#[derive(Clone, Default)]
pub(crate) struct MenuAnchors(Rc<RefCell<HashMap<Anchor, Bounds<Pixels>>>>);

impl gpui::Global for MenuAnchors {}

impl MenuAnchors {
    pub(crate) fn clear(&self) {
        self.0.borrow_mut().clear();
    }

    fn bottom_left(&self, key: &Anchor) -> Option<Point<Pixels>> {
        self.0.borrow().get(key).map(Bounds::bottom_left)
    }
}

/// A zero-cost layer that records its parent's bounds under `key` while the frame is laid
/// out. Put it as a child of the owner element; the owner must be positioned (`relative`).
pub(crate) fn record_bounds(key: Anchor, cx: &App) -> impl IntoElement + use<> {
    let anchors = cx.global::<MenuAnchors>().clone();
    canvas(
        move |bounds, _, _| anchors.0.borrow_mut().insert(key, bounds),
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

impl Shell {
    /// The bottom-left of `key`'s owner as painted in the last frame; else of the title bar's
    /// switcher; else just under the title bar.
    pub(crate) fn menu_anchor(&self, key: &Anchor, window: &Window, cx: &App) -> Point<Pixels> {
        let anchors = cx.global::<MenuAnchors>();
        anchors
            .bottom_left(key)
            .or_else(|| anchors.bottom_left(&Anchor::Switcher))
            .unwrap_or_else(|| {
                point(
                    px(space::M),
                    theme::rems_px(theme::TITLE_BAR_HEIGHT).to_pixels(window.rem_size()),
                )
            })
    }
}
```

Add to the `std` imports at the top of `main.rs` if missing: `use std::cell::{Cell, RefCell};` (it has `Cell`; add `RefCell`).

The canvas prepaint closure returns the `Option<Bounds>` that `insert` yields; that value is passed to the paint closure, which ignores it (type is inferred; if the compiler complains, end the prepaint closure with `{ anchors.0.borrow_mut().insert(key, bounds); }`).

In `main.rs` `fn main`, right after `cx.set_global(ActiveKeymap(keymap.clone()));` add:

```rust
            cx.set_global(MenuAnchors::default());
```

In `impl Render for Shell`, right after `self.typing_target.replace(None);` add:

```rust
        // Menus opened from the keyboard hang from what this frame lays out.
        cx.global::<MenuAnchors>().clear();
```

In `chrome.rs` `render_workspace_switcher`, make the `button` record itself: in the `div().id("workspace-switch")` chain add `.relative()` after `.id("workspace-switch")` and a last child:

```rust
            .child(record_bounds(Anchor::Switcher, cx))
```

(placed after the final `.child(icon_svg(Icon::ChevronDown, ...))` of that button; `cx` is `&mut Context`, which derefs to `&App`).

- [ ] **Step 3: Workspace menu rows**

In `menus.rs` add near the top of the file (after `use crate::*;`):

```rust
/// The headings of the workspace menu's lists (`Shell::menu_select_after` takes them).
pub(crate) const DEFAULT_AGENT_HEADING: &str = "Default agent for new panes";
pub(crate) const DEFAULT_SHELL_HEADING: &str = "Default shell for new panes";
```

Replace `render_workspace_menu` with:

```rust
    /// The workspace menu: "Pin", "Open in Zed", "Run setup…", the default agent and the default
    /// shell for new panes, the workspace color and "Remove workspace…". Floats at the click
    /// (or under the title bar's switcher).
    pub(crate) fn workspace_menu_rows(&self, menu: &WorkspaceMenu) -> Rows {
        let project_id = menu.project_id;
        let project = self
            .sidebar
            .projects()
            .iter()
            .find(|project| project.id == project_id);
        let pinned = project.is_some_and(|project| project.pinned);
        let key = |command: Command| self.keymap.label(command);
        let mut rows: Rows = vec![
            MenuItem::new(
                if pinned { "Unpin" } else { "Pin" },
                act(move |shell, window, cx| shell.pin_workspace(project_id, !pinned, window, cx)),
            )
            .row(),
            MenuRow::Separator,
            MenuItem::new(
                "Open in Zed",
                act(move |shell, _, cx| shell.open_in_zed(project_id, cx)),
            )
            .keys(key(Command::OpenInZed))
            .row(),
            MenuItem::new(
                "Open in File Explorer",
                act(move |shell, _, cx| shell.open_workspace_in_explorer(project_id, cx)),
            )
            .row(),
            MenuRow::Separator,
            MenuItem::new(
                "Run setup…",
                act(move |shell, window, cx| shell.open_run_setup(project_id, false, window, cx)),
            )
            .row(),
            MenuRow::Separator,
            MenuRow::Heading(DEFAULT_AGENT_HEADING.into()),
        ];
        rows.extend(agent_menu_rows(
            &menu.agents,
            SessionKind::Agent,
            project.and_then(|project| project.default_agent.as_deref()),
            move |shell, agent, window, cx| shell.set_default_agent(project_id, agent, window, cx),
        ));
        rows.push(MenuRow::Separator);
        rows.push(MenuRow::Heading(DEFAULT_SHELL_HEADING.into()));
        // Loading and errors already show once, under the agents.
        if let Agents::Loaded(_) = &menu.agents {
            rows.extend(agent_menu_rows(
                &menu.agents,
                SessionKind::Shell,
                project.and_then(|project| project.default_shell.as_deref()),
                move |shell, id, window, cx| shell.set_default_shell(project_id, id, window, cx),
            ));
        }
        rows.push(MenuRow::Separator);
        rows.push(MenuRow::Heading("Color".into()));
        rows.push(MenuRow::swatches(
            project.and_then(|project| project.color.as_deref()),
            act_swatch(move |shell, color, window, cx| {
                shell.set_workspace_color(project_id, color, window, cx);
            }),
        ));
        rows.push(MenuRow::Separator);
        rows.push(
            MenuItem::new(
                "Remove workspace…",
                act(move |shell, window, cx| shell.archive_workspace(project_id, window, cx)),
            )
            .tone(Tone::Danger)
            .row(),
        );
        rows
    }

    pub(crate) fn render_workspace_menu(
        &self,
        menu: &WorkspaceMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        render_menu(
            self.menu_state(),
            "workspace-menu",
            self.workspace_menu_rows(menu),
            None,
            Place::At(menu.position),
            theme,
            cx,
        )
    }
```

Delete `with_workspace_menu` (`~:720-745`, doc "`row` with the workspace menu dropping down below it ...").

Replace the free fn `agent_rows` with:

```rust
/// The rows of a profile list (agents or shells) in a menu (`current` checked): loading, the
/// error, an empty hint or one item per profile; Enter or a click calls `pick` with the
/// profile id.
pub(crate) fn agent_menu_rows(
    agents: &Agents,
    kind: SessionKind,
    current: Option<&str>,
    pick: impl Fn(&mut Shell, String, &mut Window, &mut Context<'_, Shell>) + Clone + 'static,
) -> Rows {
    match agents {
        Agents::Loading => vec![MenuRow::Note("Loading agents…".into())],
        Agents::Failed(error) => {
            vec![MenuRow::Note(
                format!("✕ Cannot list the agents: {error}").into(),
            )]
        }
        Agents::Loaded(profiles) => {
            let choices = workspace_menu::agent_choices(profiles, kind, current);
            if choices.is_empty() {
                let empty = match kind {
                    SessionKind::Agent => "No agent profiles. Add one to the profiles folder.",
                    SessionKind::Shell => {
                        "No shell found on PATH. Add a shell profile to the profiles folder."
                    }
                };
                return vec![MenuRow::Note(empty.into())];
            }
            choices
                .into_iter()
                .map(|choice| {
                    let agent = choice.id;
                    let pick = pick.clone();
                    let picked = agent.clone();
                    let item = MenuItem::new(
                        choice.label,
                        act(move |shell, window, cx| pick(shell, picked.clone(), window, cx)),
                    )
                    .checked(choice.selected);
                    if agent.is_empty() {
                        item
                    } else {
                        item.lead(Lead::Agent(sidebar::agent_badge(
                            choice.icon.as_deref(),
                            &agent,
                        )))
                    }
                    .row()
                })
                .collect()
        }
    }
}
```

- [ ] **Step 4: Opener and callers**

Replace `open_workspace_menu` in `workspace_actions.rs` (keep the doc style):

```rust
    /// Opens the menu of `project_id` floating at `anchor` (closing any other) and lists the
    /// agent profiles for it. `by_keyboard` selects its first row.
    pub(crate) fn open_workspace_menu(
        &mut self,
        project_id: Ulid,
        anchor: Point<Pixels>,
        by_keyboard: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.begin_menu(window, cx);
        self.workspace_menu = Some(WorkspaceMenu::new(project_id, anchor));
        self.finish_menu(by_keyboard || window.last_input_was_keyboard(), window, cx);
        let listed = connection.call::<AgentProfilesResult>(Request::AgentProfiles);
        cx.spawn_in(window, async move |this, cx| {
            let agents = match listed.await {
                Ok(listed) => Agents::Loaded(listed.profiles),
                Err(error) => Agents::Failed(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                // Unless the menu closed (or moved to another workspace) meanwhile.
                let Some(menu) = shell
                    .workspace_menu
                    .as_mut()
                    .filter(|menu| menu.project_id == project_id)
                else {
                    return;
                };
                menu.agents = agents;
                shell.apply_menu_pending();
                cx.notify();
            });
        })
        .detach();
    }

    /// The same menu under the title bar's workspace switcher: the palette, "pick a default
    /// first" and Shift+F10 without an owner.
    pub(crate) fn open_workspace_menu_at_switcher(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let at = self.menu_anchor(&Anchor::Switcher, window, cx);
        self.open_workspace_menu(project_id, at, true, window, cx);
    }
```

(`Point` and `Pixels` are imported at the crate root.) The old `self.sidebar_layout.show();` and `self.file_menu = None;` lines are gone: the menu floats at the root, so the sidebar need not be shown.

In `sidebar.rs`: the three `with_workspace_menu` uses drop the wrapper:
- `~:357-358`: `let row = workspace_row(row, false, theme, cx); self.with_workspace_menu(row, project_id, theme, cx)` becomes `workspace_row(row, false, theme, cx)` (remove the now unused `let project_id = row.project_id;` there if nothing else reads it).
- `~:724-725`: `let row = workspace_row(row, highlight, theme, cx); let header = self.with_workspace_menu(row, project_id, theme, cx);` becomes `let header = workspace_row(row, highlight, theme, cx);`.
- `~:917-918`: `let row = compact_workspace_row(row, highlight, theme, cx); list.push(self.with_workspace_menu(row, project_id, theme, cx));` becomes `list.push(compact_workspace_row(row, highlight, theme, cx));`.

The four openers become (closures get the event):
- `~:1207-1212` (`workspace_row` right-click): `cx.listener(move |shell, event: &MouseDownEvent, window, cx| { shell.open_workspace_menu(project_id, event.position, false, window, cx); })`.
- `~:1278-1282` (the ⋯ button): `.on_click(cx.listener(move |shell, event: &gpui::ClickEvent, window, cx| { cx.stop_propagation(); shell.open_workspace_menu(project_id, event.position(), false, window, cx); }))`.
- `~:1535-1540` (`compact_workspace_row` right-click): same as the first.
- `~:1553-1555` (the compact row's second opener): same as the second, using the closure's event type for that element (`MouseDownEvent` if it is an `on_mouse_down`, `ClickEvent` if an `on_click`).

In `sessions.rs` `ask_for_default`, replace `self.open_workspace_menu(project_id, window, cx);` with `self.open_workspace_menu_at_switcher(project_id, window, cx);`.

In `navigation.rs` replace the arm

```rust
                    Extra::WorkspaceDefaultAgent | Extra::WorkspaceMenu => {
                        self.open_workspace_menu(project_id, window, cx);
                    }
```

with

```rust
                    Extra::WorkspaceDefaultAgent => {
                        self.open_workspace_menu_at_switcher(project_id, window, cx);
                        self.menu_select_after(DEFAULT_AGENT_HEADING);
                    }
                    Extra::WorkspaceMenu => {
                        self.open_workspace_menu_at_switcher(project_id, window, cx);
                    }
```

- [ ] **Step 5: Render site and `open_menu_rows` arm**

In `main.rs` `Shell::render`, after the `.children(self.prompt_picker ...)` block and before the tab menu block, add:

```rust
                    .children(
                        self.workspace_menu
                            .as_ref()
                            .filter(|_| !self.modal_open())
                            .map(|menu| self.render_workspace_menu(menu, &theme, cx)),
                    )
```

In `menu_render.rs` `open_menu_rows` add:

```rust
        if let Some(menu) = &self.workspace_menu {
            return Some(self.workspace_menu_rows(menu));
        }
```

- [ ] **Step 6: Build and lint**

Run: `cargo build -p mkrew-ui` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: `Finished`, clean (the `menu_select_after` dead_code is gone now). Run `cargo test -p mkrew-ui --lib workspace_menu` to confirm the pure tests still pass.

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-ui/src/workspace_menu.rs crates/mkrew-ui/src/shell crates/mkrew-ui/src/main.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): workspace menu floats at an anchor and takes the keyboard"
```

---

### Task 7: Files tree and file tab menus

**Files:**
- Modify: `crates/mkrew-ui/src/shell/menus.rs` (`render_file_menu` `~:188-278`)
- Modify: `crates/mkrew-ui/src/shell/file_tabs_render.rs:6-20` (`open_file_tab_menu`), `:122-127` (right-click), `:187-280` (`render_file_tab_menu`)
- Modify: `crates/mkrew-ui/src/shell/workspace_actions.rs` (`open_file_menu` `~:344`), `crates/mkrew-ui/src/shell/files_render.rs:166-177`, `:1024-1030` (callers)
- Modify: `crates/mkrew-ui/src/shell/menu_render.rs` (`open_menu_rows` arms)

**Interfaces:**
- Produces: `file_menu_rows(&FileMenu) -> Rows`, `file_tab_menu_rows(&FileTabMenu) -> Rows`; `open_file_menu(project_id, path, is_dir, position, window, cx)`, `open_file_tab_menu(project_id, path, position, window, cx)`.

- [ ] **Step 1: File menu rows**

In `menus.rs` replace `render_file_menu` with:

```rust
    /// The Files tree's right-click menu: New file…, New folder…, Copy path, Add to .gitignore
    /// (rows only), Open in File Explorer, Open in Zed.
    pub(crate) fn file_menu_rows(&self, menu: &FileMenu) -> Rows {
        let (project_id, target, is_dir) = (menu.project_id, menu.target, menu.is_dir);
        let root = menu.path.is_empty();
        // In the folder clicked, or next to the file clicked.
        let dir = if is_dir {
            menu.path.as_str()
        } else {
            menu.path.rsplit_once('/').map_or("", |(dir, _)| dir)
        };
        let new_entry = |label: &'static str, folder: bool| {
            let entry = NewEntry {
                project_id,
                target,
                dir: dir.to_owned(),
                folder,
            };
            MenuItem::new(
                label,
                act(move |shell, window, cx| shell.start_new_entry(entry.clone(), window, cx)),
            )
            .row()
        };
        let path = menu.path.clone();
        let copy = path.clone();
        let mut rows: Rows = vec![
            new_entry("New file…", false),
            new_entry("New folder…", true),
            MenuRow::Separator,
            MenuItem::new(
                "Copy path",
                act(move |shell, _, cx| shell.copy_file_path(project_id, target, &copy, cx)),
            )
            .row(),
        ];
        // The folder itself cannot ignore itself.
        if !root {
            let ignore = path.clone();
            rows.push(
                MenuItem::new(
                    "Add to .gitignore",
                    act(move |shell, window, cx| {
                        shell.ignore_file_path(project_id, target, &ignore, is_dir, window, cx);
                    }),
                )
                .row(),
            );
        }
        let explorer = path.clone();
        rows.push(MenuRow::Separator);
        rows.push(
            MenuItem::new(
                "Open in File Explorer",
                act(move |shell, _, cx| {
                    shell.open_path_in_explorer(project_id, target, &explorer, is_dir, cx);
                }),
            )
            .row(),
        );
        rows.push(
            MenuItem::new(
                "Open in Zed",
                act(move |shell, _, cx| shell.open_path_in_zed(project_id, target, &path, cx)),
            )
            .row(),
        );
        rows
    }

    pub(crate) fn render_file_menu(
        &self,
        menu: &FileMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        render_menu(
            self.menu_state(),
            "file-menu",
            self.file_menu_rows(menu),
            None,
            Place::At(menu.position),
            theme,
            cx,
        )
    }
```

In `workspace_actions.rs` replace `open_file_menu`:

```rust
    /// Right click on a Files tree row (`path`) or on the tree itself (`""`), or Shift+F10 on
    /// the cursor row.
    pub(crate) fn open_file_menu(
        &mut self,
        project_id: Ulid,
        path: String,
        is_dir: bool,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let target = (self.files.get(&project_id)).and_then(|view| view.worktree_task_id);
        self.begin_menu(window, cx);
        self.file_menu = Some(FileMenu {
            project_id,
            target,
            path,
            is_dir,
            position,
        });
        self.finish_menu(window.last_input_was_keyboard(), window, cx);
    }
```

In `files_render.rs`, both listeners get the window: the blank-tree one (`~:166-177`) becomes `cx.listener(move |shell, event: &gpui::MouseDownEvent, window, cx| { shell.open_file_menu(project_id, String::new(), true, event.position, window, cx); })` (keep the existing argument order, adding `window` before `cx`), and `file_row`'s (`~:1024-1030`) becomes `cx.listener(move |shell, event: &gpui::MouseDownEvent, window, cx| { shell.open_file_menu(project_id, menu_path.clone(), is_dir, event.position, window, cx); cx.stop_propagation(); })`.

- [ ] **Step 2: File tab menu rows**

In `file_tabs_render.rs` replace `open_file_tab_menu` and `render_file_tab_menu`:

```rust
    pub(crate) fn open_file_tab_menu(
        &mut self,
        project_id: Ulid,
        path: String,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.begin_menu(window, cx);
        self.file_tab_menu = Some(FileTabMenu {
            project_id,
            path,
            position,
        });
        self.finish_menu(window.last_input_was_keyboard(), window, cx);
    }
```

```rust
    /// The tab menu: Close, Close others, Close to the right, Close all, Pin / Unpin, Copy path.
    pub(crate) fn file_tab_menu_rows(&self, menu: &FileTabMenu) -> Rows {
        let project_id = menu.project_id;
        let view = self.files.get(&project_id);
        let target = view.and_then(|view| view.worktree_task_id);
        let pinned = (view.and_then(|view| view.tab_for(&menu.path))).is_some_and(|tab| tab.pinned);
        let close = |label: &'static str, which: Close, keys: String| {
            let path = menu.path.clone();
            let enabled =
                view.is_some_and(|view| !view.close_targets(&menu.path, which).is_empty());
            MenuItem::new(
                label,
                act(move |shell, window, cx| {
                    shell.close_file_tabs(project_id, &path, which, window, cx);
                }),
            )
            .keys(keys)
            .enabled(enabled)
            .row()
        };
        let (pin_path, copy_path) = (menu.path.clone(), menu.path.clone());
        vec![
            close("Close", Close::One, self.keymap.label(Command::CloseFileTab)),
            close("Close others", Close::Others, String::new()),
            close("Close to the right", Close::Right, String::new()),
            close("Close all", Close::All, String::new()),
            MenuRow::Separator,
            MenuItem::new(
                if pinned { "Unpin" } else { "Pin" },
                act(move |shell, _, cx| shell.pin_file_tab(project_id, &pin_path, !pinned, cx)),
            )
            .row(),
            MenuItem::new(
                "Copy path",
                act(move |shell, _, cx| {
                    shell.copy_file_path(project_id, target, &copy_path, cx);
                }),
            )
            .row(),
        ]
    }

    pub(crate) fn render_file_tab_menu(
        &self,
        menu: &FileTabMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        render_menu(
            self.menu_state(),
            "file-tab-menu",
            self.file_tab_menu_rows(menu),
            None,
            Place::At(menu.position),
            theme,
            cx,
        )
    }
```

The tab's right-click (`~:118-129`) passes the window: `cx.listener(move |shell, event: &MouseDownEvent, window, cx| { cx.stop_propagation(); shell.open_file_tab_menu(project_id, menu_path.clone(), event.position, window, cx); })`.

- [ ] **Step 3: `open_menu_rows` arms**

```rust
        if let Some(menu) = &self.file_menu {
            return Some(self.file_menu_rows(menu));
        }
        if let Some(menu) = &self.file_tab_menu {
            return Some(self.file_tab_menu_rows(menu));
        }
```

- [ ] **Step 4: Build and lint**

Run: `cargo build -p mkrew-ui` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: `Finished`, clean.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/shell
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Files tree and file tab menus take the keyboard"
```

---

### Task 8: Board card menu, Run with and fan-out pickers

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs` (new `CardMenu` struct next to `FileMenu` `~:299`; `card_menu` field; `Shell::render` site; `CardContext` build in `board.rs`)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (init `card_menu: None`)
- Modify: `crates/mkrew-ui/src/shell/focus.rs` (`close_menus` adds `card_menu`), `crates/mkrew-ui/src/shell/menu_render.rs` (`menu_open`, `open_menu_rows`)
- Modify: `crates/mkrew-ui/src/shell/board.rs` (`CardDo`, `card_do`, `card_menu_rows`, `open_card_menu`, `run_fan_out`, `open_run_with`, `open_fan_out`, `CardContext` literal `~:607`)
- Modify: `crates/mkrew-ui/src/shell/cards.rs` (`CardContext` `~:5-30`, hover buttons `~:380-500`, `card_action` `~:771-815`, `run_with_button` `~:517-583`, `fan_out_button` `~:587-736`)

**Interfaces:**
- Produces: `struct CardMenu { task_id: Ulid, position: Point<Pixels> }`; `pub(crate) enum CardDo { Approve, Undo, Changes, Merge, RemoveWorktree, Compare, RunWith, FanOut }`; `Shell::card_do(&mut self, task_id, what: CardDo, window, cx)`; `Shell::open_card_menu(task_id, position, window, cx)`; `Shell::card_menu_rows(&CardMenu) -> Rows`; `run_with_rows(menu: &RunWith, current: Option<&str>) -> Rows`, `fan_out_rows(menu: &FanOutPicker) -> Rows` (free fns in `cards.rs`); `Shell::run_fan_out(&mut self, params: TaskFanoutParams, window, cx)`; `CardContext.menu: MenuState<'a>`.

- [ ] **Step 1: State**

In `main.rs`, after `struct FileTabMenu { ... }` add:

```rust
/// The menu of a Board card (SPEC §10): its hover buttons as rows, for the keyboard.
struct CardMenu {
    task_id: Ulid,
    /// Window position: the card's bottom-left.
    position: gpui::Point<gpui::Pixels>,
}
```

Add the field after `tab_menu` (with the doc): `/// The open menu of a Board card (Shift+F10 on the selected card).` / `card_menu: Option<CardMenu>,`; init `card_menu: None,` in `connection.rs` next to `tab_menu: None,`. In `focus.rs` `close_menus` add `self.card_menu = None;` before `self.git_menu = None;`. In `menu_render.rs` add `|| self.card_menu.is_some()` to `menu_open` and this arm to `open_menu_rows`:

```rust
        if let Some(menu) = &self.card_menu {
            return Some(self.card_menu_rows(menu));
        }
```

Plus the root render site in `main.rs` `Shell::render`, after the workspace menu block:

```rust
                    .children(
                        self.card_menu
                            .as_ref()
                            .filter(|_| self.view() == View::Board && !self.modal_open())
                            .map(|menu| self.render_card_menu(menu, &theme, cx)),
                    )
```

- [ ] **Step 2: `CardDo` and the card menu in `board.rs`**

Add to `shell/board.rs` (top, after `use crate::*;`):

```rust
/// What a card's buttons and its menu do; both call [`Shell::card_do`], so a click and a key
/// never diverge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CardDo {
    Approve,
    Undo,
    Changes,
    Merge,
    RemoveWorktree,
    Compare,
    RunWith,
    FanOut,
}
```

and in `impl Shell`:

```rust
    /// A card's action (button or menu row): the card becomes the selected one first, so
    /// Ctrl+Enter and the palette act on it.
    pub(crate) fn card_do(
        &mut self,
        task_id: Ulid,
        what: CardDo,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.selected_card = Some(task_id);
        match what {
            CardDo::Approve => self.approve_task(task_id, window, cx),
            CardDo::Undo => self.open_undo_dialog(task_id, window, cx),
            CardDo::Changes => self.view_task_changes(task_id, window, cx),
            CardDo::Merge => {
                self.open_worktree_dialog(worktree_dialog::Kind::Merge, task_id, window, cx);
            }
            CardDo::RemoveWorktree => {
                self.open_worktree_dialog(worktree_dialog::Kind::Remove, task_id, window, cx);
            }
            CardDo::Compare => self.open_compare(task_id, window, cx),
            CardDo::RunWith => self.open_run_with(task_id, window, cx),
            CardDo::FanOut => self.open_fan_out(task_id, window, cx),
        }
    }

    /// The card as the board draws it (its flags decide which actions show).
    fn card_of(&self, task_id: Ulid) -> Option<Card> {
        let project_id = self.board.task(task_id)?.project_id;
        let default_agent = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .and_then(|project| project.default_agent.as_deref());
        self.board
            .columns(project_id, default_agent, self.sidebar.sessions())
            .into_iter()
            .flat_map(|column| column.cards)
            .find(|card| card.task_id == task_id)
    }

    /// Approve, Undo…, Changes, Merge back…, Remove worktree…, Compare…, Run with…, Run with
    /// several agents…: each shown when its hover button would be.
    pub(crate) fn card_menu_rows(&self, menu: &CardMenu) -> Rows {
        let task_id = menu.task_id;
        let Some(card) = self.card_of(task_id) else {
            return vec![MenuRow::Note("This card is gone.".into())];
        };
        let idle = !self.starting.contains(&task_id);
        let row = |label: &'static str, what: CardDo| {
            MenuItem::new(
                label,
                act(move |shell, window, cx| shell.card_do(task_id, what, window, cx)),
            )
            .row()
        };
        let mut rows: Rows = Vec::new();
        if card.action() == Some(board::CardAction::Approve) {
            rows.push(row("Approve", CardDo::Approve));
        }
        if card.can_undo() {
            rows.push(row("Undo…", CardDo::Undo));
        }
        if card.can_view_changes() {
            rows.push(row("Changes", CardDo::Changes));
        }
        if card.manages_worktree() {
            rows.push(row("Merge back…", CardDo::Merge));
            rows.push(row("Remove worktree…", CardDo::RemoveWorktree));
        }
        if card.can_compare() {
            rows.push(row("Compare…", CardDo::Compare));
        }
        if card.can_run_with() && idle {
            rows.push(row("Run with…", CardDo::RunWith));
        }
        if card.can_fan_out() && idle {
            rows.push(row("Run with several agents…", CardDo::FanOut));
        }
        if rows.is_empty() {
            rows.push(MenuRow::Note("No actions for this card.".into()));
        }
        rows
    }

    pub(crate) fn render_card_menu(
        &self,
        menu: &CardMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        render_menu(
            self.menu_state(),
            "card-menu",
            self.card_menu_rows(menu),
            None,
            Place::At(menu.position),
            theme,
            cx,
        )
    }

    /// Opens the menu of `task_id`'s card at `position` (its bottom-left).
    pub(crate) fn open_card_menu(
        &mut self,
        task_id: Ulid,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.begin_menu(window, cx);
        self.card_menu = Some(CardMenu { task_id, position });
        self.finish_menu(window.last_input_was_keyboard(), window, cx);
    }
```

In `open_run_with` and `open_fan_out` replace the lines `self.selected_card = Some(task_id); self.run_with = Some(RunWith::new(task_id)); self.fan_out = None; cx.notify();` (and the fan-out twin) with:

```rust
        self.selected_card = Some(task_id);
        self.begin_menu(window, cx);
        self.run_with = Some(RunWith::new(task_id));
        self.finish_menu(window.last_input_was_keyboard(), window, cx);
```

```rust
        self.selected_card = Some(task_id);
        self.begin_menu(window, cx);
        self.fan_out = Some(FanOutPicker::new(task_id));
        self.finish_menu(window.last_input_was_keyboard(), window, cx);
```

(`begin_menu` already closes the other picker.) In the spawned completions of both, after `menu.agents = agents;` the existing `cx.notify()` stays. `run_fan_out` takes the params:

```rust
    /// The picker's run row (SPEC §20E.2): `task.fanout` with the ticked agents; the daemon
    /// creates and runs one child task each, and moves the card to Running. A failure shows on
    /// the board.
    pub(crate) fn run_fan_out(
        &mut self,
        params: TaskFanoutParams,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.notify();
        let Some(connection) = self.connection.clone() else {
            return;
        };
```

(the rest of the body is unchanged; delete the two lines `let Some(params) = self.fan_out.as_ref().and_then(FanOutPicker::request) else { return; };` and `self.fan_out = None;` that the new signature and the dismiss-first runner replace.)

In the `CardContext { ... }` literal in `render_board` add `menu: self.menu_state(),`.

- [ ] **Step 3: `cards.rs`**

Add to `CardContext` after `fan_out`:

```rust
    /// The keyboard state of the open picker (see [`render_menu`]).
    pub(crate) menu: MenuState<'a>,
```

Replace the click bodies of the hover buttons so they call `card_do`:

- Undo (`~:395-401`): `.on_click(cx.listener(move |shell, _, window, cx| { cx.stop_propagation(); shell.card_do(task_id, CardDo::Undo, window, cx); }))`.
- Changes (`~:415-421`): same with `CardDo::Changes`.
- The worktree pair: change the tuple's last element from `worktree_dialog::Kind::Merge` / `Remove` to `CardDo::Merge` / `CardDo::RemoveWorktree`, rename the `kind` binding to `what`, and the click to `shell.card_do(task_id, what, window, cx);` (remove the `shell.selected_card = ...; shell.open_worktree_dialog(kind, ...)` lines).
- Compare (`~:485-491`): `shell.card_do(task_id, CardDo::Compare, window, cx);`.
- `card_action` (`~:807-814`):

```rust
        .on_click(cx.listener(move |shell, _, window, cx| {
            cx.stop_propagation();
            match action {
                CardAction::Run => {
                    shell.selected_card = Some(task_id);
                    shell.run_task(task_id, window, cx);
                }
                CardAction::Approve => shell.card_do(task_id, CardDo::Approve, window, cx),
            }
        }))
```

Replace `run_with_button`'s tail (everything from `let Some(menu) = menu else {` to the end) and its click:

```rust
    .on_click(cx.listener(move |shell, _, window, cx| {
        cx.stop_propagation();
        shell.card_do(task_id, CardDo::RunWith, window, cx);
    }));
    let Some(menu) = menu else {
        return div().child(button);
    };
    div().child(button).child(render_menu(
        cards.menu,
        "run-with-menu",
        run_with_rows(menu, current),
        None,
        Place::Dropdown,
        theme,
        cx,
    ))
}

/// The "Run with…" picker: the agent profiles (`current` checked); picking one changes the
/// task's agent and runs it.
pub(crate) fn run_with_rows(menu: &RunWith, current: Option<&str>) -> Rows {
    let task_id = menu.task_id;
    let mut rows: Rows = vec![MenuRow::Heading("Run with".into())];
    rows.extend(agent_menu_rows(
        &menu.agents,
        SessionKind::Agent,
        current,
        move |shell, agent, window, cx| shell.run_task_with(task_id, agent, window, cx),
    ));
    rows
}
```

Replace `fan_out_button`'s click with `shell.card_do(task_id, CardDo::FanOut, window, cx);` (keeping `cx.stop_propagation()`), and its tail (from `let choices = match &menu.agents {` to the end) with:

```rust
    let Some(menu) = menu else {
        return div().child(button);
    };
    div().child(button).child(render_menu(
        cards.menu,
        "fan-out-menu",
        fan_out_rows(menu),
        None,
        Place::Dropdown,
        theme,
        cx,
    ))
}

/// The "Run with several agents…" picker: one tick row per agent profile (Space or Enter
/// ticks, the menu stays), then the run row (Enter runs). Past the maximum the unticked rows
/// are disabled.
pub(crate) fn fan_out_rows(menu: &FanOutPicker) -> Rows {
    let mut rows: Rows = vec![MenuRow::Heading("Run with several agents".into())];
    let choices = match &menu.agents {
        Agents::Loaded(profiles) => {
            workspace_menu::agent_choices(profiles, SessionKind::Agent, None)
        }
        _ => Vec::new(),
    };
    if choices.is_empty() {
        // Loading, the error or the empty hint; there is nothing to tick.
        rows.extend(agent_menu_rows(
            &menu.agents,
            SessionKind::Agent,
            None,
            |_, _, _, _| {},
        ));
        return rows;
    }
    if choices.len() < TaskFanoutParams::MIN_PROFILES {
        rows.push(MenuRow::Note(
            "Needs at least 2 agent profiles. Add one to the profiles folder.".into(),
        ));
        return rows;
    }
    rows.extend(choices.into_iter().map(|choice| {
        let picked = menu.is_picked(&choice.id);
        let blocked = !picked && menu.is_full();
        let agent = choice.id;
        MenuItem::new(
            choice.label,
            act(move |shell, _, _| {
                if let Some(menu) = shell.fan_out.as_mut() {
                    menu.toggle(&agent);
                }
            }),
        )
        .toggle()
        .checked(picked)
        .enabled(!blocked)
        .row()
    }));
    rows.push(MenuRow::Separator);
    let run = menu.request();
    rows.push(
        MenuItem::new(
            menu.button_label(),
            act({
                let run = run.clone();
                move |shell, window, cx| {
                    if let Some(params) = run.clone() {
                        shell.run_fan_out(params, window, cx);
                    }
                }
            }),
        )
        .tone(Tone::Accent)
        .enabled(run.is_some())
        .row(),
    );
    rows
}
```

Add the card-menu arm's helper rows to `open_menu_rows` for the two pickers:

```rust
        if let Some(menu) = &self.run_with {
            return Some(run_with_rows(menu, None));
        }
        if let Some(menu) = &self.fan_out {
            return Some(fan_out_rows(menu));
        }
```

(`current` only marks the ✓ column, so the keyboard's rows have the same indices and actions as the screen's.)

- [ ] **Step 4: Build and lint**

Run: `cargo build -p mkrew-ui` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: `Finished`, clean. `worktree_dialog::Kind` is now only used in `card_do`; unused imports in `cards.rs` (`worktree_dialog`, `TaskDialog` no) are pruned if the compiler names them.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Board card menu; Run with and fan-out pickers take the keyboard"
```

---

### Task 9: Git By:, More and Repositories popovers

**Files:**
- Modify: `crates/mkrew-ui/src/shell/git_menus.rs:6-28` (`open_git_menu`, delete `close_git_menu`), `:144-265` (Repositories), `:267-315` (search keys), `:422-564` (By:)
- Modify: `crates/mkrew-ui/src/shell/git_diff.rs:490-577` (More), `crates/mkrew-ui/src/shell/git.rs:497-501` (Esc)
- Modify: `crates/mkrew-ui/src/shell/menu_render.rs` (`open_menu_rows` arms)

**Interfaces:**
- Produces: `Shell::{git_changed_by_rows(project_id, &GitView) -> Rows, git_more_rows(project_id, &GitView) -> Rows, git_repo_rows(project_id, active: &Option<String>) -> Rows}`.

- [ ] **Step 1: Opener**

In `git_menus.rs` replace `open_git_menu` and delete `close_git_menu`:

```rust
    /// Opens `menu` (closing any other popover). The Repositories menu starts with an empty
    /// search that has the keyboard and its first row selected.
    pub(crate) fn open_git_menu(
        &mut self,
        menu: GitMenu,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.begin_menu(window, cx);
        self.git_menu = Some(menu);
        if menu == GitMenu::Repos {
            self.git_repo_query.clear();
            self.finish_menu(true, window, cx);
            window.focus(&self.git_repo_query_focus, cx);
        } else {
            self.finish_menu(window.last_input_was_keyboard(), window, cx);
        }
    }
```

In `git.rs` `git_key_down` replace `return self.close_git_menu(window, cx);` with `return self.dismiss_menus(window, cx);`.

- [ ] **Step 2: Repositories rows, render, ↑/↓**

Replace `render_git_repo_menu` and `git_repo_query_key_down` in `git_menus.rs`:

```rust
    /// "Repositories": the heading, then one row per repository matching the search, with its
    /// branch and Clean / N changes; Enter or a click shows it.
    pub(crate) fn git_repo_rows(&self, project_id: Ulid, active: &Option<String>) -> Rows {
        let repos = self
            .git_repos
            .get(&project_id)
            .map_or(&[][..], Vec::as_slice);
        let mut rows: Rows = vec![MenuRow::Heading("Repositories".into())];
        let listed = git_view::repo_choices(repos, self.workspace_name(project_id))
            .into_iter()
            .filter(|(_, label)| palette::fuzzy(&self.git_repo_query, label).is_some())
            .map(|(repo, label)| {
                let summary = self.git_summary().and_then(|summary| {
                    let name = repo.as_deref().unwrap_or("");
                    summary.repos.iter().find(|summary| summary.repo == name)
                });
                let branch = summary
                    .map(|summary| summary.branch.clone().unwrap_or_else(|| "detached".into()))
                    .unwrap_or_default();
                let state = match summary.map(RepoSummary::changed) {
                    Some(0) => "Clean".to_owned(),
                    Some(n) => format!("{n} changes"),
                    None => String::new(),
                };
                let shown = *active == repo;
                let item = MenuItem::new(
                    label.to_owned(),
                    act(move |shell, window, cx| {
                        shell.set_git_repo(project_id, repo.clone(), window, cx);
                    }),
                )
                .checked(shown)
                .detail(state);
                if branch.is_empty() {
                    item
                } else {
                    item.sub(branch)
                }
                .row()
            })
            .collect::<Vec<_>>();
        if listed.is_empty() {
            rows.push(MenuRow::Note("No repository matches.".into()));
        }
        rows.extend(listed);
        rows
    }

    fn render_git_repo_menu(
        &self,
        project_id: Ulid,
        view: &GitView,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let focused = self.git_repo_query_focus.is_focused(window);
        let search = caret_field(
            "git-repos-search",
            &self.git_repo_query,
            "Search repositories",
            focused,
            theme,
            cx,
            |shell, caret| {
                shell
                    .git_repo_query
                    .click(caret.at, caret.extend, caret.clicks);
            },
        )
        .track_focus(&self.git_repo_query_focus)
        .on_key_down(cx.listener(Self::git_repo_query_key_down))
        .relative()
        .child(self.typing(
            &self.git_repo_query_focus,
            Self::git_repo_query_key_down,
            cx,
        ))
        .on_click(cx.listener(|shell, _, window, cx| {
            window.focus(&shell.git_repo_query_focus, cx);
            cx.notify();
        }))
        .into_any_element();
        render_menu(
            self.menu_state(),
            "git-repos-panel",
            self.git_repo_rows(project_id, &view.repo),
            Some(search),
            Place::Dropdown,
            theme,
            cx,
        )
    }

    /// The Repositories search: typing filters (the first match is selected), ↑ / ↓ move the
    /// selection through the filtered rows, Enter shows the selected one, Esc clears the search
    /// (then closes the menu).
    pub(crate) fn git_repo_query_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let (Some(project_id), Some(key)) = (self.workspace, quick_open::key(&event.keystroke))
        else {
            return;
        };
        let paste = (key == quick_open::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let active = self.git.get(&project_id).and_then(|view| view.repo.clone());
        match key {
            quick_open::Key::Close if !self.git_repo_query.is_empty() => {
                self.git_repo_query.clear();
            }
            quick_open::Key::Close => return self.dismiss_menus(window, cx),
            quick_open::Key::Open => {
                let rows = self.git_repo_rows(project_id, &active);
                let action = self
                    .menu_selected
                    .and_then(|at| menu_model::action_of(&rows, at))
                    .cloned();
                self.dismiss_menus(window, cx);
                if let Some(action) = action {
                    action(self, 0, window, cx);
                }
                return;
            }
            quick_open::Key::Move(by) => {
                let rows = self.git_repo_rows(project_id, &active);
                self.menu_selected = menu_model::step(&rows, self.menu_selected, by as isize);
                return cx.notify();
            }
            quick_open::Key::Edit(edit) => {
                copy_selection(&edit, &self.git_repo_query, cx);
                self.git_repo_query.edit(&edit);
            }
            quick_open::Key::Paste => {
                self.git_repo_query
                    .insert(&paste.unwrap_or_default().replace(['\r', '\n'], " "));
            }
            quick_open::Key::Insert(text) => {
                self.git_repo_query.insert(&text);
            }
        }
        let rows = self.git_repo_rows(project_id, &active);
        self.menu_selected = menu_model::first(&rows);
        cx.notify();
    }
```

(Keep the rest of the file. `view.repo` is the `Option<String>` the old code compared with.)

- [ ] **Step 3: By: and More rows**

Replace `render_git_changed_by_menu` in `git_menus.rs`:

```rust
    /// The "By" menu: All sessions, then AGENTS (sessions named by their agent) and OTHER
    /// SESSIONS (named or ended ones); the shown choice is checked.
    pub(crate) fn git_changed_by_rows(&self, project_id: Ulid, view: &GitView) -> Rows {
        let filtered = view.changed_by.session;
        let sessions = self.sidebar.sessions();
        let item = |label: String, session: Option<Ulid>| {
            let tip = match session {
                None => "Every changed file".to_owned(),
                Some(_) => format!("Only the files {label} changed in the last 24 h"),
            };
            MenuItem::new(
                label,
                act(move |shell, window, cx| {
                    shell.filter_git_changed_by(project_id, session, window, cx);
                }),
            )
            .checked(filtered == session)
            .tip(tip)
            .row()
        };
        // An agent session without a name of its own is listed by its agent (`omp #1`).
        let (agents, others): (Vec<_>, Vec<_>) =
            view.changers(sessions).into_iter().partition(|changer| {
                sessions
                    .iter()
                    .find(|session| session.id == changer.session_id)
                    .is_some_and(|session| history::title(session).is_none())
            });
        let mut rows: Rows = vec![item("All sessions".into(), None)];
        for (heading, list) in [("AGENTS", agents), ("OTHER SESSIONS", others)] {
            if list.is_empty() {
                continue;
            }
            rows.push(MenuRow::Heading(heading.into()));
            rows.extend(
                list.into_iter()
                    .map(|changer| item(changer.label, Some(changer.session_id))),
            );
        }
        rows
    }
```

and change the caller in `git_changed_by` (`.when(open, |el| el.child(self.render_git_changed_by_menu(project_id, view, changers, theme, cx)))`) to:

```rust
                .when(open, |el| {
                    el.child(render_menu(
                        self.menu_state(),
                        "git-changed-by-panel",
                        self.git_changed_by_rows(project_id, view),
                        None,
                        Place::Dropdown,
                        theme,
                        cx,
                    ))
                }),
```

(the local `changers` variable is still used by the `changers.is_empty()` check above it; `view.changers(sessions)` is recomputed inside the rows.)

In `git_diff.rs` replace `render_git_more_menu` with:

```rust
    /// The diff toolbar's "⋯": comment on the picked lines, send the open comments. A row that
    /// cannot act now is greyed with its reason in the label.
    pub(crate) fn git_more_rows(&self, project_id: Ulid, view: &GitView) -> Rows {
        let can_comment =
            matches!(view.diff, git_view::Diff::Shown(_)) && view.comments.selection.is_some();
        let sendable = match &view.comments.list {
            comments::List::Loaded(list) => {
                doc_dialog::sendable(list, view.comments.selection).len()
            }
            _ => 0,
        };
        let send_label = match sendable {
            0 => "Send to agent (no open comments)".to_owned(),
            1 => "Send 1 open comment to agent…".to_owned(),
            n => format!("Send {n} open comments to agent…"),
        };
        vec![
            MenuItem::new(
                if can_comment {
                    "Comment on selected lines"
                } else {
                    "Comment (select lines first)"
                },
                act(move |shell, window, cx| {
                    shell.start_comment(project_id, CommentContext::Diff, window, cx);
                }),
            )
            .keys(self.keymap.label(Command::AddComment))
            .enabled(can_comment)
            .row(),
            MenuItem::new(
                send_label,
                act(move |shell, window, cx| {
                    shell.open_send_dialog(project_id, CommentContext::Diff, window, cx);
                }),
            )
            .keys(self.keymap.label(Command::SendComments))
            .enabled(sendable > 0)
            .row(),
        ]
    }
```

and its call site in `git_diff_more` (`el.child(self.render_git_more_menu(project_id, view, theme, cx))`) becomes:

```rust
            .when(open, |el| {
                el.child(render_menu(
                    self.menu_state(),
                    "git-diff-more-panel",
                    self.git_more_rows(project_id, view),
                    None,
                    Place::Dropdown,
                    theme,
                    cx,
                ))
            })
```

- [ ] **Step 4: `open_menu_rows` arms**

```rust
        if let (Some(menu), Some(project_id)) = (self.git_menu, self.workspace) {
            let view = self.git.get(&project_id)?;
            return Some(match menu {
                GitMenu::ChangedBy => self.git_changed_by_rows(project_id, view),
                GitMenu::More => self.git_more_rows(project_id, view),
                GitMenu::Repos => self.git_repo_rows(project_id, &view.repo),
            });
        }
```

- [ ] **Step 5: Build and lint**

Run: `cargo build -p mkrew-ui` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: `Finished`, clean (unused `meta_line` / `chip` helpers, if any, pruned).

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/shell
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Git popovers on the menu renderer; Repositories search moves with arrows"
```

---

### Task 10: `Command::ContextMenu` (Shift+F10), owner resolution and anchors

**Files:**
- Modify: `crates/mkrew-ui/src/menu.rs` (`Owner`, `Focus`, `owner` + tests)
- Modify: `crates/mkrew-ui/src/keymap.rs` (`Command::ContextMenu` in the enum `~:20`, `all()` `~:289`, `name()` `~:380`, `default_keys()` `~:493`, tests)
- Modify: `crates/mkrew-ui/src/palette.rs:288` (title), `:322` (available)
- Modify: `crates/mkrew-ui/src/shell/sessions.rs` (`run`)
- Modify: `crates/mkrew-ui/src/shell/menu_render.rs` (`Anchor` variants, `context_menu`)
- Modify (anchor recording): `crates/mkrew-ui/src/shell/pane_render.rs` (`pane_header` `~:802`, single-pane block in `render_tab_bar` `~:247`), `files_render.rs` (`file_row`), `file_tabs_render.rs` (tab element), `sidebar.rs` (`workspace_row`, `compact_workspace_row`, `session_row`, `compact_session_row`), `cards.rs` (`task_card`)

**Interfaces:**
- Consumes: all earlier tasks' openers.
- Produces: `Command::ContextMenu`; `menu::{Owner, Focus, owner}`; `Anchor::{PaneHeader(Ulid), FileRow(String), FileTab(String), WorkspaceRow(Ulid), SessionRow(Ulid), Card(Ulid)}`; `Shell::context_menu(window, cx)`.

Owner table (spec "Open"):

| Keyboard is in | Owner | Anchor |
| --- | --- | --- |
| A terminal pane | `Pane(session)` | the pane header, else the tab strip's pane block (single pane) |
| The sidebar filter with a highlighted row | `SidebarRow(Target)` | that row |
| The Files tree with a cursor row (Files view) | `FileRow { path, is_dir }` | the cursor row |
| Files editor / preview, or the tree without a cursor, with an active tab | `FileTab(path)` | the active file tab |
| The Board with a selected card of this workspace | `Card(task_id)` | the card |
| Anything else | `Workspace` | the title bar's switcher |

- [ ] **Step 1: Write the failing owner tests**

Append to `crates/mkrew-ui/src/menu.rs` (before `#[cfg(test)]`), the model:

```rust
/// What a context menu opens on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    Pane(Ulid),
    FileRow { path: String, is_dir: bool },
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
    /// The Files tree's cursor row and whether it is a folder.
    pub files_cursor: Option<(String, bool)>,
    /// The active Files tab.
    pub file_tab: Option<String>,
    /// The Board's selected card, when this workspace has it.
    pub card: Option<Ulid>,
}

/// The menu a right-click would open on what has the keyboard (SPEC §13).
pub fn owner(focus: &Focus) -> Owner {
    if let Some(session) = focus.terminal {
        return Owner::Pane(session);
    }
    if let Some(target) = focus.sidebar_row {
        return Owner::SidebarRow(target);
    }
    match focus.view {
        View::Files => {
            if focus.files_tree_focused
                && let Some((path, is_dir)) = &focus.files_cursor
            {
                return Owner::FileRow {
                    path: path.clone(),
                    is_dir: *is_dir,
                };
            }
            focus.file_tab.clone().map_or(Owner::Workspace, Owner::FileTab)
        }
        View::Board => focus.card.map_or(Owner::Workspace, Owner::Card),
        _ => Owner::Workspace,
    }
}
```

Add `use ulid::Ulid;`, `use crate::board::View;`, `use crate::sidebar::Target;` to the imports of `menu.rs`. Add to the `tests` module:

```rust
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
            view: View::Files,
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
    }

    #[test]
    fn the_owner_falls_back_to_the_workspace() {
        // Nothing focused, a filter with nothing highlighted, a tree without a cursor and
        // without a tab, a Board without a selected card, Docs and Git: the workspace menu.
        assert_eq!(owner(&Focus::default()), Owner::Workspace);
        for view in [View::Terminals, View::Docs, View::Git] {
            let focus = Focus {
                view,
                ..Focus::default()
            };
            assert_eq!(owner(&focus), Owner::Workspace);
        }
        let bare_tree = Focus {
            view: View::Files,
            files_tree_focused: true,
            ..Focus::default()
        };
        assert_eq!(owner(&bare_tree), Owner::Workspace);
        let board = Focus {
            view: View::Board,
            ..Focus::default()
        };
        assert_eq!(owner(&board), Owner::Workspace);
        // A tree focus with a cursor but another view shown never picks a file row.
        let stale = Focus {
            view: View::Terminals,
            files_tree_focused: true,
            files_cursor: Some(("a".into(), false)),
            ..Focus::default()
        };
        assert_eq!(owner(&stale), Owner::Workspace);
    }
```

`View` must implement `Default` (it does: `#[default]` Terminals) and `Docs` / `Git` variants exist (`View::Docs`, `View::Git` are used in `palette/tests.rs`).

- [ ] **Step 2: Run the owner tests**

Run: `cargo test -p mkrew-ui --lib menu::`
Expected: PASS, 15 tests (the two new ones included). If `Ulid::from(u128)` is not available in the pinned `ulid` version use `Ulid(n)` (the tuple constructor is public).

- [ ] **Step 3: The command in `keymap.rs` and the palette**

In `keymap.rs`: add to `enum Command` (after `AskAi`):

```rust
    /// Open the menu a right-click would open on what has the keyboard (SPEC §13); pressed
    /// while a menu is open it closes it.
    ContextMenu,
```

add `Command::ContextMenu,` after `Command::AskAi,` in `all()`; `Command::ContextMenu => "context_menu".into(),` in `name()` after `AskAi`; `Command::ContextMenu => "shift-f10".into(),` in `default_keys()` after `AskAi`. `context()` keeps `_ => None` (global). Add to `spec_defaults`:

```rust
        assert_eq!(bound("shift-f10"), Some(Command::ContextMenu));
        assert_eq!(Command::ContextMenu.context(), None);
        assert_eq!(keymap.label(Command::ContextMenu), "Shift+F10");
```

In `palette.rs` `title()` add before the closing brace: `Command::ContextMenu => "Global: Open context menu".into(),`; in `available()` add `| Command::ContextMenu` to the arm that returns `context.workspace` (the one ending `| Command::NewTask => context.workspace`).

In `shell/sessions.rs` `run`, in the `match *command` add near `Command::AskAi`:

```rust
            Command::ContextMenu => {}
```

(the real handling is earlier, see Step 5; this arm only keeps the match exhaustive for the unreachable case).

- [ ] **Step 4: Record the owners' bounds**

Extend `Anchor` in `menu_render.rs`:

```rust
pub(crate) enum Anchor {
    /// The title bar's workspace switcher: where every owner falls back to.
    Switcher,
    PaneHeader(Ulid),
    FileRow(String),
    FileTab(String),
    WorkspaceRow(Ulid),
    SessionRow(Ulid),
    Card(Ulid),
}
```

Recording sites (each adds `.relative()` if the element is not already positioned, and one child):
- `pane_render.rs` `pane_header`: on the root `div()` after `.font_family(theme.ui_font_family.clone())` add `.relative().child(record_bounds(Anchor::PaneHeader(session), cx))` (put it as the last `.child` before `.on_mouse_down`).
- `pane_render.rs` `render_tab_bar`, the `.when_some(single, |bar, info| { bar.child(div() ... ) })` block: on that inner `div()` add `.relative().child(record_bounds(Anchor::PaneHeader(info.id), cx))` as its last child (a single pane has no header, this block is its status strip).
- `files_render.rs` `file_row`: after `.when(at_cursor || opened, ...)` add `.when(at_cursor, |el| el.relative().child(record_bounds(Anchor::FileRow(path.to_string()), cx)))` (use the row's path variable that the click closure clones, `entry.path`).
- `file_tabs_render.rs` tab element: after `.when(tab.transient && !dirty, |el| el.italic())` add `.when(is_active, |el| el.relative().child(record_bounds(Anchor::FileTab(tab.path.clone()), cx)))`.
- `sidebar.rs`: in `workspace_row` and `compact_workspace_row` add after the `.hover(...)` call `.when(active, |el| el.relative().child(record_bounds(Anchor::WorkspaceRow(project_id), cx)))`; in `session_row` and `compact_session_row` add `.when(selected, |el| el.relative().child(record_bounds(Anchor::SessionRow(session_id), cx)))` after their `.hover(...)`.
- `cards.rs` `task_card`: after `.group(group.clone())` add `.when(selected, |el| el.relative().child(record_bounds(Anchor::Card(task_id), cx)))`.

Add to `menu_render.rs`, the owner gathering and the command:

```rust
impl Shell {
    /// What has the keyboard, for [`menu_model::owner`].
    fn menu_focus_state(
        &self,
        project_id: Ulid,
        window: &Window,
        cx: &App,
    ) -> menu_model::Focus {
        let terminal = self.focused_session().filter(|session| {
            self.panes
                .get(session)
                .is_some_and(|pane| pane.read(cx).has_focus(window, cx))
        });
        let files = self.files.get(&project_id);
        menu_model::Focus {
            view: self.view(),
            terminal,
            sidebar_row: self
                .filter_focus
                .is_focused(window)
                .then(|| self.sidebar.highlighted(&self.sidebar.view(&self.viewed).rows))
                .flatten(),
            files_tree_focused: self.files_focus.is_focused(window),
            files_cursor: files.and_then(|view| {
                let cursor = view.cursor.as_deref()?;
                view.rows().iter().find_map(|row| match row {
                    files_view::Row::Entry { entry, .. } if entry.path == cursor => {
                        Some((entry.path.clone(), entry.is_dir))
                    }
                    _ => None,
                })
            }),
            file_tab: files.and_then(|view| view.file().map(str::to_owned)),
            card: (self.view() == View::Board && !self.jira_side_shown())
                .then_some(self.selected_card)
                .flatten()
                .filter(|&id| {
                    self.board
                        .task(id)
                        .is_some_and(|task| task.project_id == project_id)
                }),
        }
    }

    /// `Command::ContextMenu`: the menu a right-click would open on what has the keyboard,
    /// under that owner's bottom-left (the switcher's when the owner was not painted).
    pub(crate) fn context_menu(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.workspace else {
            return;
        };
        let owner = menu_model::owner(&self.menu_focus_state(project_id, window, cx));
        let key = match &owner {
            menu_model::Owner::Pane(session) => Anchor::PaneHeader(*session),
            menu_model::Owner::FileRow { path, .. } => Anchor::FileRow(path.clone()),
            menu_model::Owner::FileTab(path) => Anchor::FileTab(path.clone()),
            menu_model::Owner::SidebarRow(sidebar::Target::Workspace(id)) => {
                Anchor::WorkspaceRow(*id)
            }
            menu_model::Owner::SidebarRow(sidebar::Target::Session(id)) => {
                Anchor::SessionRow(*id)
            }
            menu_model::Owner::Card(task_id) => Anchor::Card(*task_id),
            menu_model::Owner::Workspace => Anchor::Switcher,
        };
        let at = self.menu_anchor(&key, window, cx);
        match owner {
            menu_model::Owner::Pane(session) => self.open_pane_menu(session, at, window, cx),
            menu_model::Owner::FileRow { path, is_dir } => {
                self.open_file_menu(project_id, path, is_dir, at, window, cx);
            }
            menu_model::Owner::FileTab(path) => {
                self.open_file_tab_menu(project_id, path, at, window, cx);
            }
            menu_model::Owner::SidebarRow(sidebar::Target::Workspace(id)) => {
                self.open_workspace_menu(id, at, true, window, cx);
            }
            menu_model::Owner::SidebarRow(sidebar::Target::Session(id)) => {
                // As the right-click does: the pane is shown first (its menu renames the header).
                self.open_session(id, window, cx);
                self.open_pane_menu(id, at, window, cx);
            }
            menu_model::Owner::Card(task_id) => self.open_card_menu(task_id, at, window, cx),
            menu_model::Owner::Workspace => {
                self.open_workspace_menu(project_id, at, true, window, cx);
            }
        }
    }
}
```

`menu_model::Owner` is reachable as `menu_model::Owner` because `main.rs` imports `mkrew_ui::menu as menu_model`.

- [ ] **Step 5: `run` handles the key before the focus guards**

In `shell/sessions.rs` `run`, replace the first lines after the voice check:

```rust
        // A shortcut ends every open popover and gives the keyboard back before it acts.
        self.dismiss_menus(window, cx);
```

with

```rust
        // A shortcut ends every open popover and gives the keyboard back before it acts. The
        // context menu key pressed while a menu is open only closes it.
        let menu_was_open = self.menu_open();
        self.dismiss_menus(window, cx);
```

and, right after the `if *command == Command::CommandPalette && self.palette.is_some() { ... }` block and before `if self.modal_open() {`, add:

```rust
        // The context menu acts before the focused-field guards below: it must work from a
        // pane, the Files tree or the sidebar filter, and never takes the key from a dialog.
        if *command == Command::ContextMenu {
            if self.modal_open() {
                cx.propagate();
            } else if !menu_was_open {
                self.context_menu(window, cx);
            }
            return;
        }
```

- [ ] **Step 6: Run the tests, build and lint**

Run: `cargo test -p mkrew-ui --lib keymap` and `cargo test -p mkrew-ui --lib palette`
Expected: PASS (`every_default_parses_and_is_unique` covers `shift-f10`; palette `commands(&none)` is unchanged because `ContextMenu` needs a workspace).

Run: `cargo build -p mkrew-ui` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: `Finished`, clean.

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-ui/src
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Shift+F10 opens the context menu of what has the keyboard"
```

---

### Task 11: ADR 0085, SPEC §13 / §10, HUMAN_CHECKS, verify and smoke run

**Files:**
- Create: `docs/decisions/0085-keyboard-only-operation.md`
- Modify: `docs/SPEC.md` (§13 table `~:1299` and note `~:1352-1355`; §10 Board bullet `~:862`; §11 pane menu bullet `~:1032`; §10 tab menu `~:882`; §12A right-click menu `~:1161-1168`)
- Modify: `docs/HUMAN_CHECKS.md:24` (new entry above the first "To verify" entry)

- [ ] **Step 1: Create the ADR**

Check `docs/decisions/` for the next free number (expected `0085`), then create `docs/decisions/0085-keyboard-only-operation.md`:

```markdown
# 0085 - Keyboard-only operation

- Date: 2026-10-27
- Status: accepted
- Milestone: none (user request: every action done often must work without the mouse)

## Context

An audit of `crates/mkrew-ui` (2026-10-27) found four root causes that leave many actions mouse-only.
A: context menus open only on right-click and an open menu has no focus, no arrows, no Enter, no Esc.
B: buttons, chips and checkboxes never take focus.
C: lines and terminal text can be picked only with the mouse.
D: single actions have no command.
Design: `docs/superpowers/specs/2026-10-27-keyboard-only-design.md`.
The work ships in four waves; each wave appends its section here.

## Options

1. A key handler per menu: twelve copies of the same rules, and a click and a key can diverge.
2. One menu model (`menu.rs`, pure and tested) and one renderer in the shell; each menu only builds rows. (Chosen for wave 1.)
3. GPUI tab stops inside menus: arrows, type-ahead and swatches are not tab-stop behavior.

## Decision

### Wave 1 - Keyboard context menus

- Every popover menu (tab, layout, pane, share, workspace, file, file tab, Run, workspace switcher, "+ New", Run with, fan-out, Board card, and the Git By:, More and Repositories popovers) is a list of `MenuRow`s drawn by one renderer.
- A click and a key call the same action, so they cannot diverge.
- Every open menu takes the keyboard (`menu_focus`, key context `Menu`), opened by mouse or keyboard.
- Keys: ↑/↓ and Home/End select enabled rows, ←/→ move on a swatch row, Enter and Space run the row, a letter selects the next row starting with it, Esc closes.
- A held key does nothing, so the key that opened a menu cannot pick a row.
- Closing (Esc, an action, a click outside, any bound command) gives the keyboard back to what had it before.
- New command `context_menu` (`Shift+F10`, global) opens the menu a right-click would open on what has the keyboard (terminal pane, Files cursor row, active file tab, sidebar filter row, selected Board card), else the workspace menu under the title bar's switcher.
  Pressed again it closes the menu.
- Board keys are shielded while a menu is open: `Keymap::key_bindings` adds `NoAction` bindings in context `Menu` for plain and Shift-only keys of context-limited commands.
- The workspace menu floats at an anchor instead of dropping inside the sidebar row.
- The Run menu's inline restart / stop icons become rows (a running command has a "Stop" row).
- The Board card gets a menu with its hover actions (Approve, Undo…, Changes, Merge back…, Remove worktree…, Compare…, Run with…, Run with several agents…); the hover buttons stay.
- Repositories search: ↑/↓ move the selection through the filtered rows; Enter shows the selected one.

## Consequences

- SPEC §13 (Shift+F10, menu keys), §10 (popover menus, card menu) change.
- `Shift+F10` no longer reaches a focused terminal; rebind `context_menu` in Settings → Keyboard if a program needs it.
- The Windows Menu key arrives as `menu`: bind it in Settings → Keyboard.
- Waves 2 to 4 (focusable dialog controls, picking by keyboard, single actions) append their sections.
```

- [ ] **Step 2: SPEC §13**

In `docs/SPEC.md` §13 table, after the row `| Global | Ask AI: a question outside every project (§20H) | Ctrl+Shift+G |` add:

```markdown
| Global | Open the context menu of what has the keyboard (the menu a right-click opens: terminal pane, Files cursor row or tab, sidebar row, Board card; else the workspace menu under the switcher); again closes it | Shift+F10 (the Menu key: bind `context_menu` in Settings) |
```

Replace the row `| Task / Workspace | Merge back / remove worktree of the selected task; pin, open in File Explorer, set default agent, remove, workspace menu of the active workspace | command palette only |` with the same text ending `| command palette only (the menu items also by Shift+F10) |`.

After the paragraph that ends "`rename_pane` in\nSettings → Keyboard if needed." add:

```markdown
Shift+F10 does not reach a focused terminal either; rebind `context_menu` if a program needs it.
While a popover menu is open it has the keyboard (key context `Menu`, not in `keymap.toml`):
↑ / ↓ select the next / previous enabled row, wrapping; Home / End the first / last;
← / → move between the swatches of a color row; Enter or Space run the row and close the menu
(a multi-select row of the fan-out picker ticks and stays; Enter on its Run row runs);
a letter selects the next row starting with it; Esc closes and gives the keyboard back.
A held (auto-repeat) key does nothing.
The Board's single keys stay with the menu while it is open.
```

- [ ] **Step 3: SPEC §10 / §11 / §12A**

§10 Board bullet (starts `- **Board:** columns by task status, drag & drop.`): append at the end of that bullet's last line ("…(§20I).") a new sentence line:

```markdown
  Shift+F10 on the selected card opens its menu: Approve, Undo…, Changes, Merge back…, Remove worktree…, Compare…, Run with…, Run with several agents… (each as its hover button; the buttons stay and call the same functions).
```

§10 tab menu sentence (`⋯`, and a right-click on a tab, open the tab menu): after "…so the menu acts on the tab it was opened on." keep, and add the sentence "Every popover menu below opens by mouse or keyboard, takes the keyboard while it is open and gives it back when it closes (§13)." as a new line directly under the layout menu paragraph's end line (`if a start fails, the sessions that started are placed as Split right places them.`).

§11 pane menu bullet: change `- Pane menu (right-click a split pane's header or a sidebar session row):` to `- Pane menu (right-click a split pane's header or a sidebar session row, or Shift+F10 with the pane focused):` and add **Pin** to its list by inserting "**Pin**, " before "**Rename**".

§12A: in the "Right-click menu on tree rows" bullet change the last sentence `A click outside closes the menu.` to `A click outside or Esc closes the menu; Shift+F10 opens it on the cursor row (on the active tab's menu while the editor has the keyboard).`

- [ ] **Step 4: HUMAN_CHECKS**

Insert above the first `### To verify` entry of `docs/HUMAN_CHECKS.md` (the "Voice recording animation" one):

```markdown
### To verify - 2026-10-27 Keyboard context menus (ADR 0085, wave 1)

- [ ] **Selected row in both themes:** open each menu (tab, layout, pane, share, workspace, file, file tab, Run, switcher, + New, Run with, fan-out, card, Git By:, ⋯, Repositories) and press ↓; the selected row has the `selected` fill and stays readable in dark and light, including the swatch ring (accent) on the color rows.
- [ ] **Shift+F10 per owner, dark and light:** terminal pane (menu under the pane header), Files tree cursor row, Files editor (active tab's menu), sidebar filter with a highlighted row, Board with a selected card (card menu), anything else (workspace menu under the title bar switcher). Check that each menu sits at the owner's bottom-left and that Esc returns typing to where you were (the terminal types again).
- [ ] **Menu key:** bind `context_menu` to `menu` in Settings → Keyboard and check it opens the same menus (Windows only).
- [ ] **Held key:** hold Enter in the palette on "Workspace: Open workspace menu"; the menu opens with its first row selected and nothing runs while the key repeats.
- [ ] **Fan-out picker by keyboard:** Shift+F10 on a Todo card → "Run with several agents…" → Space ticks agents, ↓ reaches the Run row, Enter runs; the other rows go grey at the cap.
- [ ] **Board keys while a menu is open:** with a card menu open, ↑/↓/Enter move the menu and never the selected card.
- [ ] **Repositories search:** with several repositories, open the Repositories menu, type to filter, ↑/↓ move the selection, Enter shows the selected one.
- [ ] **Visual changes to look at:** the workspace menu now floats at the click (it used to drop inside the sidebar row); the workspace switcher's ✓ is in a left column; Run menu commands are rows (Enter starts or restarts, a "Stop <name>" row stops) instead of inline icon buttons; a menu error line shows "✕" in muted text instead of red; the Repositories rows show "N changes" muted instead of amber.
- [ ] **Not automated:** keyboard-only driving of the real window (the smoke step used `SendKeys` where it worked); what it could not drive is listed here when the run ends.
```

- [ ] **Step 5: Full verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt, clippy (`-D warnings`) and tests all pass. On OOM or linker errors re-run with `-j 2`. Fix any insta snapshot diff caused by `theme.rs` only if this wave touched it (it did not; a snapshot diff means something else changed).

- [ ] **Step 6: Smoke run**

Build and launch the dev exe in a sandbox:

```bash
cargo build -p mkrew-ui
SB="$(mktemp -d)"; mkdir -p "$SB/data" "$SB/config"
MKREW_DATA_DIR="$SB/data" MKREW_CONFIG_DIR="$SB/config" MKREW_SOCKET="$SB/mkrew.sock" "$PWD/target/debug/mkrew.exe" &
```

Drive it keyboard-only. If the window can be reached, send keys with Windows `SendKeys` and read a screenshot (`System.Drawing` `CopyFromScreen` to a temp PNG, then `read` the PNG):

```bash
powershell -NoProfile -Command "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('+{F10}'); Start-Sleep -Milliseconds 300; [System.Windows.Forms.SendKeys]::SendWait('{DOWN}{DOWN}')"
```

Check, in order:
1. `Shift+F10` with no workspace open does nothing; add a workspace with `Ctrl+Shift+K`, then `Shift+F10` opens the workspace menu under the title bar switcher with its first row selected.
2. ↓ ↓ moves, `s` jumps to the next row starting with "s", Esc closes; typing in the terminal works again without a click.
3. Open the tab menu (`Shift+F10` in a terminal gives the pane menu: Pin, Mark unread, Rename, color swatches; ← → on the swatch row; Enter picks).
4. In the Files view: `Shift+F10` on the tree cursor row shows New file…, New folder…, Copy path; Esc closes.
5. In the Board with a selected card: `Shift+F10` shows the card menu; Esc closes; the card selection did not move while the menu was open.
6. `Shift+F10` while the menu is open closes it.

Record in `docs/HUMAN_CHECKS.md` (the "Not automated" line of the entry from Step 4) every check that could not be driven, then kill only the sandbox `mkrew.exe` you started (never the daemon or any agent process): `taskkill //PID <pid>`.

- [ ] **Step 7: Commit**

```bash
git add docs/decisions/0085-keyboard-only-operation.md docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "docs: ADR 0085, SPEC and human checks for keyboard context menus"
```

---

## Self-review

**Spec coverage.**
- `menu.rs` pure model, key fn, unit tests (wrap, skip disabled / separators / notes, Home/End, swatches ←/→, type-ahead, held key, fan-out Toggle): Task 1.
- One renderer reusing `menu_item`, `menu_panel`, `color_swatch`; every menu of `Shell::close_menus` migrated (tab, layout, pane, share Task 3-4; workspace Task 6; file, file tab Task 7; Run, switcher, "+ New" Task 5; Run with, fan-out Task 8) plus Git By:, Repositories (and More) Task 9.
- `menu_focus`, return focus, Esc, mouse-opened menus also focused: Task 3 (`begin_menu` / `finish_menu` / `dismiss_menus`, every opener calls them).
- `Command::ContextMenu`, owner table, anchors from prepaint bounds (`canvas`, the pattern of `preview.rs` and `field.rs`; `on_children_prepainted` is not needed because every owner is a single element): Tasks 6 and 10.
- Card menu with the hover buttons' actions: Task 8.
- Repositories ↑/↓: Task 9.
- ADR 0085, SPEC §13 / §10 (and §11, §12A mentions), HUMAN_CHECKS, smoke: Task 11.

**Placeholder scan.** No TBD / TODO; every step shows code or an exact edit with the quoted text to find. Steps that say "same as the first" (the four sidebar `open_workspace_menu` callers, Task 6 Step 4) name the exact closure shapes in the two preceding bullets.

**Type consistency.** `MenuAction`, `Rows`, `act`, `act_swatch`, `Place`, `MenuState`, `render_menu`, `begin_menu`, `finish_menu`, `dismiss_menus`, `open_menu_rows`, `menu_select_after`, `Anchor`, `menu_anchor`, `record_bounds` are defined once (Tasks 3 and 6) and used with those exact signatures in Tasks 4-10. `open_file_menu`, `open_file_tab_menu`, `open_layout_menu` gain `window` in Tasks 4 and 7 and every caller is listed there. `run_fan_out` takes `TaskFanoutParams` (Task 8) and its only caller is the Run row. `WorkspaceMenu::new(project_id, position)` (Task 6) has one caller.

**Review Focus.** Lines 2, 3, 4, 5 have named tests (Tasks 1, 2, 10); line 1 is the Task 11 smoke step.

**Cross-wave contracts kept.** `menu.rs`, `MenuKey` and `key()` as named; `Shell::render_menu` is a free `render_menu(state, id, rows, header, place, theme, cx)` (not a method returning `Stateful<Div>`, because the Board's free fns need it); `Shell.menu_focus` with key context `Menu`; `Shell.menu_return` (focus plus terminal session, like `palette_return`); `Command::ContextMenu`; `open_workspace_menu(project_id, anchor, by_keyboard, window, cx)`.
Added for wave 4: `open_workspace_menu_at_switcher`, `menu_select_after(DEFAULT_SHELL_HEADING)` (heading rows are `MenuRow::Heading`, not `Note`), `agent_menu_rows`.
