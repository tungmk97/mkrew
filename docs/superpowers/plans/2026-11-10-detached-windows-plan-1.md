# Detached windows - Plan 1 (machinery + terminal tabs) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A terminal tab of the main window can live in its own OS window: drag it out (or Shift+F4), drag it between windows, type in it, split and close panes, Move back, Move all back, Close, and the windows come back after a restart.

**Architecture:** `Shell` stays the only owner of state.
A pure model (`mkrew_ui::detached`) records which terminal tab sits in which detached window; a pure geometry module (`mkrew_ui::window_place`) picks drop targets and window bounds.
Each detached OS window has a thin root view `DetachedWindow` that asks `Shell::render_host` to draw it and forwards its actions to `Shell::run`.
The host of any code is read from its `window` (`Shell::host_of(window)`), never from a stored "current window"; code that must act on "the tab of this window" uses `Shell::tab_scope(window)` / `Shell::with_tab(window, f)`.

**Tech Stack:** Rust, gpui-pre 0.3.7 (multi-window: `App::open_window`, `AnyWindowHandle::update`, `App::defer`), serde_json for `windows.json`.

**Spec:** `docs/superpowers/specs/2026-11-10-detached-windows-design.md` (Plan 1 row of §9).

## Global Constraints

- Work in a worktree `.worktrees/detached-windows` on branch `feat/detached-windows` (superpowers:using-git-worktrees); commit with `git -c user.email=tungnv975@gmail.com commit`, Conventional Commits, no co-author line, only the files of the task.
- No protocol change: `PROTOCOL_VERSION` stays 53; no migration.
- ADR number 0096 (recheck `git ls-tree` of every branch for `docs/decisions/0096*` before writing it).
- Plan 1 detaches terminal tabs only. File tabs keep today's behavior: no "Move to new window" on file tabs, their drags stay inside the main window.
- Default key of `Tab: Move to new window`: `shift-f4`.
- UI: every color, spacing, radius, size from `crates/mkrew-ui/src/theme.rs`; new tokens `DETACHED_MIN_WIDTH = 480.0`, `DETACHED_MIN_HEIGHT = 320.0`, `DETACHED_OFFSET = 32.0`. No hex colors or raw px numbers outside `theme.rs`. Dark and light themes.
- No `unwrap`/`expect` outside tests unless provably infallible (comment why).
- Never the em dash character in code, docs or UI text; use `-`.
- Markdown docs: one sentence per line.
- Verify: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (fmt check, clippy `-D warnings`, tests). Fast loop: `cargo test -p mkrew-ui <filter>`. If the full test build crashes rustc (STATUS_STACK_BUFFER_OVERRUN), run fmt check + clippy, then `cargo test --workspace -j 4`.
- `cargo fmt --all` may touch other agents' files: commit only the files of the task.
- gpui names in this plan were read from gpui-pre 0.3.7 on 2026-11-10 (`app.rs:1350 open_window`, `app.rs:2070 defer`, `window.rs:7333 AnyWindowHandle::update`, `context.rs:402 observe_window_bounds`, `context.rs:420 observe_window_activation`, `context.rs:439 observe_window_visibility`, `window.rs:2697 bounds`, `window.rs:2420 window_bounds`, `window.rs:6658 on_window_should_close`, `window.rs:2876 set_window_title`, `app.rs:1406 displays`, `app.rs:1411 primary_display`).
  `App::window_stack` returns `None` on Windows: do not use it.

## Review Focus

1. A pane click, a composer click or a drop into a pane of a detached window must not change the main window's active tab (`Tabs::focus` sets `Tabs.active`). Pinned by `focus_pane_leaves_the_active_tab` (Task 2) and by `keep_main_active_visible` in `sync_panes` (Task 6), smoke step in Task 11.
2. A key pressed in a detached window must never act on the main window's tabs (Ctrl+Shift+W closing the main tab). Pinned by `run` routing to `run_detached` first (Task 8) and `detached_commands_are_an_explicit_list` (Task 3), smoke step in Task 8.
3. A reconnect clears `Shell.tabs`; detached tabs must not be pruned before their workspace's `tab.list` answers again. Pinned by `prune_keeps_tabs_of_unloaded_workspaces` (Task 1).
4. A saved window on a display that is gone (laptop undocked) must open on a display that exists. Pinned by `bounds_off_every_display_move_to_the_primary` (Task 4).
5. Closing the main window while detached windows are open must quit the app and keep their sessions alive (no `tab.close`). Pinned by `quit_closes_no_tab` (Task 1) and the lifecycle smoke in Task 11.

---

### Task 1: Pure model `mkrew_ui::detached` + ADR 0096

**Files:**
- Create: `crates/mkrew-ui/src/detached.rs`
- Create: `crates/mkrew-ui/src/detached/tests.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (add `pub mod detached;` in alphabetical order, after `pub mod desktop;`)
- Create: `docs/decisions/0096-detached-windows.md`

**Interfaces:**
- Produces:
  - `pub struct WindowKey(pub u64)` (`Copy, Eq, Hash, Ord, Debug`)
  - `pub enum HostId { Main, Detached(WindowKey) }` (`Copy, Eq, Hash, Debug`)
  - `pub enum DockTab { Terminal { project: Ulid, tab_id: Ulid } }` (`Clone, Copy, Eq, Hash, Debug, Serialize, Deserialize`, `#[serde(rename_all = "snake_case")]`)
  - `pub struct DetachedWindow { pub key: WindowKey, pub tabs: Vec<DockTab>, pub active: usize, pub visible: bool }` (named `Window` in code to avoid clashing with the gpui view: `pub struct Window`)
  - `pub struct Detached` with `new() -> Self`, `windows() -> &[Window]`, `window(key) -> Option<&Window>`, `host_of(&DockTab) -> HostId`, `is_detached(&DockTab) -> bool`, `active(key) -> Option<DockTab>`, `open(tab: DockTab) -> WindowKey`, `move_tab(tab, to: HostId, index: Option<usize>) -> Moved`, `move_within(key, from, to)`, `select(key, index) -> bool`, `cycle(key, forward: bool)`, `remove_window(key) -> Vec<DockTab>`, `prune(keep: impl Fn(&DockTab) -> Keep) -> Vec<WindowKey>`, `set_visible(key, bool)`, `visible_tabs() -> Vec<DockTab>`, `tabs_of(key) -> Vec<DockTab>`, `quit_closes() -> Vec<DockTab>`.
  - `pub enum Keep { Yes, No, Unknown }` (Unknown = its workspace is not loaded: keep).
  - `pub struct Moved { pub from: HostId, pub emptied: Option<WindowKey> }`.

- [ ] **Step 1: Write the failing tests** in `crates/mkrew-ui/src/detached/tests.rs`:

```rust
use super::*;

fn term() -> DockTab {
    DockTab::Terminal {
        project: Ulid::new(),
        tab_id: Ulid::new(),
    }
}

#[test]
fn a_new_window_holds_its_tab_and_shows_it() {
    let mut detached = Detached::new();
    let tab = term();
    let key = detached.open(tab);
    assert_eq!(detached.host_of(&tab), HostId::Detached(key));
    assert_eq!(detached.active(key), Some(tab));
    assert!(detached.is_detached(&tab));
}

#[test]
fn a_tab_lives_in_one_window_only() {
    let mut detached = Detached::new();
    let (a, b) = (term(), term());
    let first = detached.open(a);
    let second = detached.open(b);
    detached.move_tab(a, HostId::Detached(second), Some(0));
    assert_eq!(detached.host_of(&a), HostId::Detached(second));
    assert_eq!(detached.tabs_of(second), vec![a, b]);
    assert!(detached.window(first).is_none(), "the emptied window is dropped");
}

#[test]
fn moving_inserts_at_the_index_and_selects_the_tab() {
    let mut detached = Detached::new();
    let (a, b, c) = (term(), term(), term());
    let key = detached.open(a);
    detached.move_tab(b, HostId::Detached(key), None);
    detached.move_tab(c, HostId::Detached(key), Some(1));
    assert_eq!(detached.tabs_of(key), vec![a, c, b]);
    assert_eq!(detached.active(key), Some(c));
}

#[test]
fn moving_to_main_forgets_the_tab_and_reports_the_emptied_window() {
    let mut detached = Detached::new();
    let a = term();
    let key = detached.open(a);
    let moved = detached.move_tab(a, HostId::Main, None);
    assert_eq!(moved.from, HostId::Detached(key));
    assert_eq!(moved.emptied, Some(key));
    assert_eq!(detached.host_of(&a), HostId::Main);
}

#[test]
fn the_active_tab_passes_right_then_left_when_it_leaves() {
    let mut detached = Detached::new();
    let (a, b, c) = (term(), term(), term());
    let key = detached.open(a);
    detached.move_tab(b, HostId::Detached(key), None);
    detached.move_tab(c, HostId::Detached(key), None);
    assert!(detached.select(key, 1));
    detached.move_tab(b, HostId::Main, None);
    assert_eq!(detached.active(key), Some(c));
    detached.move_tab(c, HostId::Main, None);
    assert_eq!(detached.active(key), Some(a));
}

#[test]
fn reorder_and_cycle_stay_inside_the_window() {
    let mut detached = Detached::new();
    let (a, b, c) = (term(), term(), term());
    let key = detached.open(a);
    detached.move_tab(b, HostId::Detached(key), None);
    detached.move_tab(c, HostId::Detached(key), None);
    detached.move_within(key, 2, 0);
    assert_eq!(detached.tabs_of(key), vec![c, a, b]);
    assert!(detached.select(key, 2));
    detached.cycle(key, true);
    assert_eq!(detached.active(key), Some(c), "wraps to the first");
    detached.cycle(key, false);
    assert_eq!(detached.active(key), Some(b), "wraps to the last");
    assert!(!detached.select(key, 9));
}

#[test]
fn prune_keeps_tabs_of_unloaded_workspaces() {
    let mut detached = Detached::new();
    let (gone, unknown, kept) = (term(), term(), term());
    let key = detached.open(gone);
    detached.move_tab(unknown, HostId::Detached(key), None);
    let other = detached.open(kept);
    let emptied = detached.prune(|tab| match *tab {
        t if t == gone => Keep::No,
        t if t == unknown => Keep::Unknown,
        _ => Keep::Yes,
    });
    assert!(emptied.is_empty());
    assert_eq!(detached.tabs_of(key), vec![unknown]);
    assert_eq!(detached.tabs_of(other), vec![kept]);
    let emptied = detached.prune(|_| Keep::No);
    assert_eq!(emptied.len(), 2);
    assert!(detached.windows().is_empty());
}

#[test]
fn hidden_windows_show_no_tab() {
    let mut detached = Detached::new();
    let (a, b) = (term(), term());
    let one = detached.open(a);
    let two = detached.open(b);
    detached.set_visible(two, false);
    assert_eq!(detached.visible_tabs(), vec![a]);
    detached.set_visible(two, true);
    assert_eq!(detached.visible_tabs(), vec![a, b]);
    let _ = one;
}

#[test]
fn quit_closes_no_tab() {
    let mut detached = Detached::new();
    detached.open(term());
    assert!(detached.quit_closes().is_empty());
}

#[test]
fn removing_a_window_returns_its_tabs_in_order() {
    let mut detached = Detached::new();
    let (a, b) = (term(), term());
    let key = detached.open(a);
    detached.move_tab(b, HostId::Detached(key), None);
    assert_eq!(detached.remove_window(key), vec![a, b]);
    assert!(detached.window(key).is_none());
}

#[test]
fn window_keys_are_never_reused() {
    let mut detached = Detached::new();
    let a = term();
    let first = detached.open(a);
    detached.move_tab(a, HostId::Main, None);
    let second = detached.open(a);
    assert_ne!(first, second);
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-ui detached::tests`
Expected: compile error, module `detached` not found.

- [ ] **Step 3: Implement** `crates/mkrew-ui/src/detached.rs`:

```rust
//! Which terminal tab sits in which detached window (spec 2026-11-10 §4.1, ADR 0096).
//! Pure: the shell owns one [`Detached`] and draws from it. A tab in no window is in the
//! main window.

use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// A detached window, local to this UI process (never reused within it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowKey(pub u64);

/// The window a tab, a render or an input belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostId {
    Main,
    Detached(WindowKey),
}

/// A tab that can leave the main window. Plan 2 adds file tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockTab {
    Terminal { project: Ulid, tab_id: Ulid },
}

impl DockTab {
    pub fn project(&self) -> Ulid {
        match *self {
            DockTab::Terminal { project, .. } => project,
        }
    }
}

/// One detached window: its strip, left to right, and the tab it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub key: WindowKey,
    pub tabs: Vec<DockTab>,
    /// Index into `tabs`; `tabs` is never empty (an emptied window is dropped).
    pub active: usize,
    /// False while the OS window is minimized or hidden: its tab is detached (SPEC §10).
    pub visible: bool,
}

/// What [`Detached::prune`] asks about each tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    Yes,
    No,
    /// Its workspace's tabs are not loaded (yet, or again after a reconnect): kept.
    Unknown,
}

/// Where a moved tab came from, and the window it left empty (now dropped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moved {
    pub from: HostId,
    pub emptied: Option<WindowKey>,
}

#[derive(Debug, Default)]
pub struct Detached {
    windows: Vec<Window>,
    next: u64,
}

impl Detached {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn windows(&self) -> &[Window] {
        &self.windows
    }

    pub fn window(&self, key: WindowKey) -> Option<&Window> {
        self.windows.iter().find(|window| window.key == key)
    }

    fn window_mut(&mut self, key: WindowKey) -> Option<&mut Window> {
        self.windows.iter_mut().find(|window| window.key == key)
    }

    pub fn host_of(&self, tab: &DockTab) -> HostId {
        self.windows
            .iter()
            .find(|window| window.tabs.contains(tab))
            .map_or(HostId::Main, |window| HostId::Detached(window.key))
    }

    pub fn is_detached(&self, tab: &DockTab) -> bool {
        self.host_of(tab) != HostId::Main
    }

    pub fn active(&self, key: WindowKey) -> Option<DockTab> {
        let window = self.window(key)?;
        window.tabs.get(window.active).copied()
    }

    pub fn tabs_of(&self, key: WindowKey) -> Vec<DockTab> {
        self.window(key).map(|window| window.tabs.clone()).unwrap_or_default()
    }

    /// A new window showing `tab` (taken from wherever it was).
    pub fn open(&mut self, tab: DockTab) -> WindowKey {
        self.take(&tab);
        let key = WindowKey(self.next);
        self.next += 1;
        self.windows.push(Window {
            key,
            tabs: vec![tab],
            active: 0,
            visible: true,
        });
        key
    }

    /// Moves `tab` to `to` (at `index`, else the end) and shows it there. Moving to
    /// [`HostId::Main`] forgets it. A window left without tabs is dropped.
    pub fn move_tab(&mut self, tab: DockTab, to: HostId, index: Option<usize>) -> Moved {
        let (from, emptied) = self.take(&tab);
        if let HostId::Detached(key) = to
            && let Some(window) = self.window_mut(key)
        {
            let at = index.unwrap_or(window.tabs.len()).min(window.tabs.len());
            window.tabs.insert(at, tab);
            window.active = at;
        }
        Moved { from, emptied }
    }

    /// Removes `tab` from its window; returns that window and whether it emptied (dropped).
    fn take(&mut self, tab: &DockTab) -> (HostId, Option<WindowKey>) {
        let Some(at) = self.windows.iter().position(|window| window.tabs.contains(tab)) else {
            return (HostId::Main, None);
        };
        let window = &mut self.windows[at];
        let key = window.key;
        if let Some(index) = window.tabs.iter().position(|t| t == tab) {
            window.tabs.remove(index);
            // The tab right of a removed active one, else the one left of it.
            if index < window.active || window.active >= window.tabs.len() {
                window.active = window.active.saturating_sub(1);
            }
        }
        if window.tabs.is_empty() {
            self.windows.remove(at);
            return (HostId::Detached(key), Some(key));
        }
        (HostId::Detached(key), None)
    }

    /// Reorders a window's strip; the moved tab stays selected if it was.
    pub fn move_within(&mut self, key: WindowKey, from: usize, to: usize) {
        let Some(window) = self.window_mut(key) else {
            return;
        };
        if from >= window.tabs.len() || to >= window.tabs.len() {
            return;
        }
        let shown = window.tabs[window.active];
        let tab = window.tabs.remove(from);
        window.tabs.insert(to, tab);
        window.active = window.tabs.iter().position(|t| *t == shown).unwrap_or(0);
    }

    pub fn select(&mut self, key: WindowKey, index: usize) -> bool {
        match self.window_mut(key) {
            Some(window) if index < window.tabs.len() => {
                window.active = index;
                true
            }
            _ => false,
        }
    }

    pub fn cycle(&mut self, key: WindowKey, forward: bool) {
        if let Some(window) = self.window_mut(key) {
            let len = window.tabs.len();
            window.active = if forward {
                (window.active + 1) % len
            } else {
                (window.active + len - 1) % len
            };
        }
    }

    /// Drops the window and returns its tabs, strip order.
    pub fn remove_window(&mut self, key: WindowKey) -> Vec<DockTab> {
        match self.windows.iter().position(|window| window.key == key) {
            Some(at) => self.windows.remove(at).tabs,
            None => Vec::new(),
        }
    }

    /// Drops tabs that `keep` says are gone; returns the windows left empty (dropped).
    pub fn prune(&mut self, keep: impl Fn(&DockTab) -> Keep) -> Vec<WindowKey> {
        let gone: Vec<DockTab> = (self.windows.iter())
            .flat_map(|window| window.tabs.iter().copied())
            .filter(|tab| keep(tab) == Keep::No)
            .collect();
        gone.iter()
            .filter_map(|tab| self.take(tab).1)
            .collect()
    }

    pub fn set_visible(&mut self, key: WindowKey, visible: bool) {
        if let Some(window) = self.window_mut(key) {
            window.visible = visible;
        }
    }

    /// The tab each visible window shows (attached; the others are detached, SPEC §10).
    pub fn visible_tabs(&self) -> Vec<DockTab> {
        (self.windows.iter())
            .filter(|window| window.visible)
            .filter_map(|window| window.tabs.get(window.active).copied())
            .collect()
    }

    /// Quitting the app keeps every detached tab alive (spec §6.2): nothing to close.
    pub fn quit_closes(&self) -> Vec<DockTab> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mkrew-ui detached::tests`
Expected: PASS (11 tests).

- [ ] **Step 5: Write ADR** `docs/decisions/0096-detached-windows.md` (one sentence per line):

```markdown
# 0096 - Terminal tabs (later files) move into their own windows

- Date: 2026-11-10
- Status: accepted (amends SPEC §10 "One window." and §13)
- Milestone: none (human request 2026-11-10, spec `docs/superpowers/specs/2026-11-10-detached-windows-design.md`)

## Context

The user wants a terminal tab or an Editor file on a second screen, like IntelliJ's "Open in New Window".
`Shell` is the root view of the only window and owns every piece of UI state; many functions read "the shown workspace's active tab".

## Decision

- Approach A: `Shell` stays the single owner of state; each detached window is a thin `DetachedWindow` root view that asks `Shell::render_host` to draw it.
- The host of a render or an input is `Shell::host_of(window)`, a lookup of the window handle; no "current window" is stored.
- A detached window's keys go through `Shell::run`, which routes them to `run_detached`: an explicit list of tab and pane commands, so no key there can act on the main window's tabs.
- `mkrew_ui::detached` records which tab sits in which window; `Tabs.active` stays the main window's and never names a detached tab.
- The source window decides a drop (gpui-pre-windows captures the mouse on button down), from the windows' screen bounds and the strips recorded at paint.
- Detached windows are saved locally in `<data>/windows.json`, not in the daemon (no protocol change).
- Closing a detached window closes its tabs (ADR 0041 confirmation); quitting keeps them.
- Plan 1 detaches terminal tabs; Plan 2 adds Editor file tabs.

## Alternatives

- One `Workbench` entity per window: cleanest boundaries, but rewrites most of `shell/`.
- One process per window (like machine windows, ADR 0089): unsaved file text cannot move between processes.
- A stored "current host" set at each entry point: async continuations and listeners would read a stale host.

## Consequences

- `focused_session`, `composer_session` and `focused_agent` take the window.
- Menu anchors are recorded per window.
- Voice input in a detached window is not handled (Plan 1).
- Tests: `detached::tests`, `window_place::tests`, `tabs::tests` (`with_active`, `focus_pane`), keymap and palette tests.
```

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/detached.rs crates/mkrew-ui/src/detached/tests.rs crates/mkrew-ui/src/lib.rs docs/decisions/0096-detached-windows.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): detached window model (ADR 0096)"
```

---

### Task 2: `Tabs::with_active`, `Tabs::focus_pane`, `Tabs::activate_skipping`

**Files:**
- Modify: `crates/mkrew-ui/src/tabs.rs` (in `impl Tabs`, after `pub fn focus` at about line 128-138)
- Test: `crates/mkrew-ui/src/tabs/tests.rs`

**Interfaces:**
- Produces:
  - `pub fn with_active<R>(&mut self, tab_id: Ulid, f: impl FnOnce(&mut Tabs) -> R) -> Option<R>`: runs `f` as if `tab_id` were active, then restores the active tab (re-fixed if `f` removed it). `None` if no such tab.
  - `pub fn focus_pane(&mut self, tab_id: Ulid, session: Ulid) -> bool`: focuses `session` in `tab_id` without changing `active`.
  - `pub fn activate_skipping(&mut self, hidden: impl Fn(Ulid) -> bool)`: when the active tab is hidden, the next visible one (right, else left) becomes active, else none.

- [ ] **Step 1: Write the failing tests** (append to `tabs/tests.rs`):

```rust
#[test]
fn with_active_acts_on_another_tab_and_restores_the_active_one() {
    let project = Ulid::new();
    let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, pane(a));
    let split = json!({"split":"vertical","ratio":0.5,"children":[pane(b), pane(c)]});
    let two = wire(project, "two", 2.0, split);
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone()]);
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id));

    let zoomed = tabs.with_active(two.id, Tabs::toggle_zoom);
    assert_eq!(zoomed, Some(true));
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id), "active restored");
    assert!(tabs.tab(two.id).and_then(Tab::zoomed).is_some(), "the change stays");
    assert_eq!(tabs.with_active(Ulid::new(), Tabs::toggle_zoom), None);
}

#[test]
fn focus_pane_leaves_the_active_tab() {
    let project = Ulid::new();
    let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, pane(a));
    let split = json!({"split":"vertical","ratio":0.5,"children":[pane(b), pane(c)]});
    let two = wire(project, "two", 2.0, split);
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone()]);

    assert!(tabs.focus_pane(two.id, c));
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id));
    assert_eq!(tabs.tab(two.id).and_then(Tab::focused), Some(c));
    assert!(!tabs.focus_pane(two.id, a), "a is not in that tab");
}

#[test]
fn activate_skipping_moves_off_a_hidden_tab() {
    let project = Ulid::new();
    let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, pane(a));
    let two = wire(project, "two", 2.0, pane(b));
    let three = wire(project, "three", 3.0, pane(c));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone(), three.clone()]);
    assert!(tabs.activate(two.id));

    tabs.activate_skipping(|id| id == two.id);
    assert_eq!(tabs.active().map(|t| t.id), Some(three.id), "right first");
    tabs.activate_skipping(|id| id == three.id || id == two.id);
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id), "else left");
    tabs.activate_skipping(|_| true);
    assert!(tabs.active().is_none(), "every tab hidden");
    tabs.activate_skipping(|_| false);
    assert!(tabs.active().is_none(), "nothing to move off");
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-ui tabs::tests::with_active tabs::tests::focus_pane tabs::tests::activate_skipping`
Expected: compile errors, the three methods do not exist.

- [ ] **Step 3: Implement** in `impl Tabs` after `focus`:

```rust
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

    /// Focuses `session`'s pane in `tab_id` without making that tab active.
    pub fn focus_pane(&mut self, tab_id: Ulid, session: Ulid) -> bool {
        let Some(tab) = self.tab_mut(tab_id).filter(|tab| tab.contains(session)) else {
            return false;
        };
        tab.focused = Some(session);
        tab.fix_focus();
        true
    }

    /// While the active tab is `hidden` (shown in a detached window), the next tab that is not
    /// becomes active (right of it, else left), or none.
    pub fn activate_skipping(&mut self, hidden: impl Fn(Ulid) -> bool) {
        let Some(active) = self.active.filter(|id| hidden(*id)) else {
            return;
        };
        let at = self.tabs.iter().position(|tab| tab.id == active).unwrap_or(0);
        let right = self.tabs[at..].iter().find(|tab| !hidden(tab.id));
        let left = self.tabs[..at].iter().rev().find(|tab| !hidden(tab.id));
        self.active = right.or(left).map(|tab| tab.id);
    }
```

Note: `fix_active` picks the first tab when the active one is gone; `activate_skipping` runs after it in `Shell::keep_main_active_visible` (Task 6), so a removed active tab never lands on a detached one.

- [ ] **Step 4: Run tests**

Run: `cargo test -p mkrew-ui tabs::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/tabs.rs crates/mkrew-ui/src/tabs/tests.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): scoped tab operations for detached windows"
```

---

### Task 3: Commands: `Tab: Move to new window` (Shift+F4), palette extras, detached command list

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs` (`enum Command` after `MoveTabRight` at about line 198; `all()` after `Command::MoveTabRight`; `name()`; `default_keys()`; `spec_defaults` test at about line 1126)
- Modify: `crates/mkrew-ui/src/palette.rs` (`Context` struct at about line 21-93: new field; `enum Extra` + `ALL` length 44 to 46; `Extra::title`; `Extra::available`; `pub fn title(command)` at about line 346; `pub fn available(command, context)` at about line 463)
- Modify: `crates/mkrew-ui/src/detached.rs` (add `pub fn handles(command: Command) -> bool`)
- Test: `crates/mkrew-ui/src/palette/tests.rs`, `crates/mkrew-ui/src/detached/tests.rs`, keymap tests in `keymap.rs`
- Modify: the one place that builds `palette::Context` (`crates/mkrew-ui/src/shell/navigation.rs` `open_palette`, about line 395): add `detached_window: false` for now (Task 8 sets it).

**Interfaces:**
- Produces: `Command::MoveTabToNewWindow` (name `move_tab_to_new_window`, default `shift-f4`, context `None`), `Extra::TabMoveToMainWindow` ("Tab: Move to main window"), `Extra::WindowMoveAllBack` ("Window: Move all tabs back"), `palette::Context.detached_window: bool`, `mkrew_ui::detached::handles(Command) -> bool`.

- [ ] **Step 1: Write the failing tests.**

In `keymap.rs` tests, add to `spec_defaults` (same style as its other lines):

```rust
        assert_eq!(keymap.label(Command::MoveTabToNewWindow), "Shift+F4");
```

and a new test:

```rust
    #[test]
    fn move_tab_to_new_window_is_window_wide() {
        assert_eq!(Command::MoveTabToNewWindow.context(), None);
        assert_eq!(
            Command::from_name("move_tab_to_new_window"),
            Some(Command::MoveTabToNewWindow)
        );
    }
```

In `palette/tests.rs`:

```rust
#[test]
fn window_extras_show_only_in_a_detached_window() {
    let main = context(View::Terminals);
    let detached = Context {
        detached_window: true,
        ..context(View::Terminals)
    };
    for extra in [Extra::TabMoveToMainWindow, Extra::WindowMoveAllBack] {
        assert!(!extra.available(&main), "{extra:?} in the main window");
        assert!(extra.available(&detached), "{extra:?} in a detached window");
    }
    assert!(available(Command::MoveTabToNewWindow, &main));
    let no_tabs = Context {
        tabs: 0,
        ..context(View::Terminals)
    };
    assert!(!available(Command::MoveTabToNewWindow, &no_tabs));
    assert!(
        !available(Command::MoveTabToNewWindow, &context(View::Editor)),
        "file tabs move in Plan 2"
    );
}
```

The palette test helper `fn context(view: View) -> Context` (tests.rs:7-48) builds every field: add `detached_window: false,` to it.

In `detached/tests.rs`:

```rust
#[test]
fn detached_commands_are_an_explicit_list() {
    use crate::keymap::Command;
    use crate::split::{Direction, Preset};
    for command in [
        Command::NextTab,
        Command::PreviousTab,
        Command::Tab(3),
        Command::CloseTab,
        Command::MoveTabLeft,
        Command::MoveTabRight,
        Command::SplitRight,
        Command::SplitDown,
        Command::SplitAgent,
        Command::SplitShell,
        Command::Layout(Preset::Grid),
        Command::EqualizePanes,
        Command::Focus(Direction::Left),
        Command::Resize(Direction::Up),
        Command::ZoomPane,
        Command::ClosePane,
        Command::RenamePane,
        Command::RenameTab,
        Command::OpenComposer,
        Command::CommandPalette,
        Command::ContextMenu,
        Command::ToggleFullScreen,
        Command::MoveTabToNewWindow,
    ] {
        assert!(handles(command), "{command:?}");
    }
    for command in [
        Command::NewShellTab,
        Command::ShowBoard,
        Command::ToggleSidebar,
        Command::Workspace(1),
        Command::SaveFile,
        Command::VoiceInput,
        Command::NextInbox,
    ] {
        assert!(!handles(command), "{command:?}");
    }
}
```

(`crate::split::Direction` / `Preset` are the paths `keymap.rs` imports; check the `use` lines at the top of `keymap.rs` and use the same ones.)

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-ui move_tab_to_new_window window_extras detached_commands`
Expected: compile errors.

- [ ] **Step 3: Implement.**

`keymap.rs`, in `enum Command` after `MoveTabRight`:

```rust
    /// Move the active terminal tab into a new window of its own (spec 2026-11-10, ADR 0096).
    MoveTabToNewWindow,
```

`all()`: add `Command::MoveTabToNewWindow,` right after `Command::MoveTabRight,`.
`name()`: `Command::MoveTabToNewWindow => "move_tab_to_new_window".into(),`.
`default_keys()`: `Command::MoveTabToNewWindow => "shift-f4".into(),`.
`context()`: nothing (falls to `_ => None`).

`palette.rs`:
- `Context`: add after `machine: bool,`:

```rust
    /// The palette was opened in a detached window (ADR 0096): only its commands are listed.
    pub detached_window: bool,
```

- `enum Extra`: add `TabMoveToMainWindow,` and `WindowMoveAllBack,` after `TabCloseRight,`; `ALL: [Self; 46]` with the two entries after `Self::TabCloseRight`.
- `Extra::title`:

```rust
            Self::TabMoveToMainWindow => "Tab: Move to main window",
            Self::WindowMoveAllBack => "Window: Move all tabs back",
```

- `Extra::available`:

```rust
            Self::TabMoveToMainWindow | Self::WindowMoveAllBack => context.detached_window,
```

- `pub fn title(command)`: `Command::MoveTabToNewWindow => "Tab: Move to new window".into(),`
- `pub fn available(command, context)`: `Command::MoveTabToNewWindow => context.workspace && context.view == View::Terminals && context.tabs > 0,`

`detached.rs` (add `use crate::keymap::Command;` at the top):

```rust
/// The commands a detached window acts on (spec 2026-11-10 §4.2); every other key there does
/// nothing, so none can act on the main window's tabs.
pub fn handles(command: Command) -> bool {
    matches!(
        command,
        Command::NextTab
            | Command::PreviousTab
            | Command::Tab(_)
            | Command::CloseTab
            | Command::MoveTabLeft
            | Command::MoveTabRight
            | Command::SplitRight
            | Command::SplitDown
            | Command::SplitAgent
            | Command::SplitShell
            | Command::Layout(_)
            | Command::EqualizePanes
            | Command::Focus(_)
            | Command::Resize(_)
            | Command::ZoomPane
            | Command::ClosePane
            | Command::RenamePane
            | Command::RenameTab
            | Command::OpenComposer
            | Command::CommandPalette
            | Command::ContextMenu
            | Command::ToggleFullScreen
            | Command::MoveTabToNewWindow
    )
}
```

`shell/navigation.rs` `open_palette`: add `detached_window: false,` to the `palette::Context { ... }` literal after `machine: self.target.is_machine(),`.

Shell's `Shell::run` must compile: add the arm `Command::MoveTabToNewWindow => {}` before `Command::CloseFileTab => {}` at the end of the `match *command` in `shell/sessions.rs` (Task 8 replaces it), and in `run_palette_pick` (`shell/navigation.rs`, the `match extra` at about line 760) add `Extra::TabMoveToMainWindow | Extra::WindowMoveAllBack => {}` (Task 8 replaces it).

- [ ] **Step 4: Run tests**

Run: `cargo test -p mkrew-ui keymap palette detached`
Expected: PASS, including `every_default_parses_and_is_unique`, `names_round_trip`, `palette_only_actions_follow_the_view_and_state` (the new extras are off with the default context).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette/tests.rs crates/mkrew-ui/src/detached.rs crates/mkrew-ui/src/detached/tests.rs crates/mkrew-ui/src/shell/navigation.rs crates/mkrew-ui/src/shell/sessions.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): move-to-window commands and the detached command list"
```

---

### Task 4: Pure geometry `mkrew_ui::window_place` + theme tokens

**Files:**
- Create: `crates/mkrew-ui/src/window_place.rs`
- Create: `crates/mkrew-ui/src/window_place/tests.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod window_place;`, alphabetical)
- Modify: `crates/mkrew-ui/src/theme.rs` (after `WINDOW_MIN_HEIGHT` at about line 184)

**Interfaces:**
- Produces:
  - `pub struct Strip { pub bounds: Bounds<Pixels>, pub chips: Vec<Bounds<Pixels>> }` (window-local, recorded at paint)
  - `pub struct Placed<H> { pub host: H, pub window: Bounds<Pixels>, pub strip: Option<Strip> }` (screen `window`)
  - `pub enum Target<H> { Strip { host: H, index: usize }, Window(H), NewWindow(Point<Pixels>) }`
  - `pub fn drop_target<H: Copy + PartialEq>(point: Point<Pixels>, source: H, windows: &[Placed<H>]) -> Option<Target<H>>` (`windows` most recently activated first; `None` while `point` is inside `source`)
  - `pub fn new_window_bounds(at: Option<Point<Pixels>>, size: Size<Pixels>, near: Bounds<Pixels>, displays: &[Bounds<Pixels>]) -> Bounds<Pixels>`
  - `pub fn fit_on_displays(bounds: Bounds<Pixels>, displays: &[Bounds<Pixels>], primary: Option<Bounds<Pixels>>) -> Bounds<Pixels>`
  - theme: `DETACHED_MIN_WIDTH`, `DETACHED_MIN_HEIGHT`, `DETACHED_OFFSET`.

- [ ] **Step 1: Write the failing tests** `window_place/tests.rs`:

```rust
use gpui::{Bounds, point, px, size};

use super::*;

fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
}

fn strip(chips: &[(f32, f32)]) -> Strip {
    Strip {
        bounds: rect(0.0, 40.0, 800.0, 36.0),
        chips: chips.iter().map(|&(x, w)| rect(x, 44.0, w, 28.0)).collect(),
    }
}

#[test]
fn inside_the_source_window_is_not_a_window_drop() {
    let windows = [Placed { host: 1, window: rect(0.0, 0.0, 800.0, 600.0), strip: None }];
    assert_eq!(drop_target(point(px(10.0), px(10.0)), 1, &windows), None);
}

#[test]
fn over_another_strip_picks_the_gap_under_the_pointer() {
    let windows = [
        Placed { host: 2, window: rect(1000.0, 0.0, 800.0, 600.0), strip: Some(strip(&[(8.0, 100.0), (112.0, 100.0)])) },
        Placed { host: 1, window: rect(0.0, 0.0, 800.0, 600.0), strip: None },
    ];
    // Left half of the second chip: before it.
    let at = drop_target(point(px(1000.0 + 130.0), px(60.0)), 1, &windows);
    assert_eq!(at, Some(Target::Strip { host: 2, index: 1 }));
    // Right of every chip: the end.
    let at = drop_target(point(px(1000.0 + 600.0), px(60.0)), 1, &windows);
    assert_eq!(at, Some(Target::Strip { host: 2, index: 2 }));
    // Below the strip: the window, at the end.
    let at = drop_target(point(px(1000.0 + 300.0), px(300.0)), 1, &windows);
    assert_eq!(at, Some(Target::Window(2)));
}

#[test]
fn overlapping_windows_go_to_the_most_recently_activated() {
    let windows = [
        Placed { host: 3, window: rect(100.0, 100.0, 400.0, 400.0), strip: None },
        Placed { host: 2, window: rect(0.0, 0.0, 800.0, 600.0), strip: None },
        Placed { host: 1, window: rect(2000.0, 0.0, 800.0, 600.0), strip: None },
    ];
    assert_eq!(drop_target(point(px(200.0), px(200.0)), 1, &windows), Some(Target::Window(3)));
}

#[test]
fn outside_every_window_opens_a_new_one_at_the_pointer() {
    let windows = [Placed { host: 1, window: rect(0.0, 0.0, 800.0, 600.0), strip: None }];
    let at = point(px(1200.0), px(300.0));
    assert_eq!(drop_target(at, 1, &windows), Some(Target::NewWindow(at)));
}

#[test]
fn a_new_window_starts_at_the_pointer_and_stays_on_its_display() {
    let displays = [rect(0.0, 0.0, 1920.0, 1080.0), rect(1920.0, 0.0, 1280.0, 1024.0)];
    let near = rect(100.0, 100.0, 1200.0, 760.0);
    let placed = new_window_bounds(Some(point(px(2000.0), px(100.0))), size(px(900.0), px(500.0)), near, &displays);
    assert_eq!(placed, rect(2000.0, 100.0, 900.0, 500.0));
    // Too far right: pushed back onto the second display.
    let placed = new_window_bounds(Some(point(px(3100.0), px(900.0))), size(px(900.0), px(500.0)), near, &displays);
    assert_eq!(placed, rect(2300.0, 524.0, 900.0, 500.0));
    // Smaller than the minimum: grown.
    let placed = new_window_bounds(Some(point(px(10.0), px(10.0))), size(px(100.0), px(100.0)), near, &displays);
    assert_eq!(placed.size, size(px(theme::DETACHED_MIN_WIDTH), px(theme::DETACHED_MIN_HEIGHT)));
}

#[test]
fn a_new_window_without_pointer_opens_offset_from_the_source() {
    let displays = [rect(0.0, 0.0, 1920.0, 1080.0)];
    let near = rect(100.0, 100.0, 1200.0, 760.0);
    let placed = new_window_bounds(None, size(px(900.0), px(500.0)), near, &displays);
    let offset = theme::DETACHED_OFFSET;
    assert_eq!(placed, rect(100.0 + offset, 100.0 + offset, 900.0, 500.0));
}

#[test]
fn bounds_off_every_display_move_to_the_primary() {
    let primary = rect(0.0, 0.0, 1920.0, 1080.0);
    let displays = [primary];
    let saved = rect(2500.0, 100.0, 800.0, 600.0);
    let fitted = fit_on_displays(saved, &displays, Some(primary));
    assert_eq!(fitted, rect(560.0, 240.0, 800.0, 600.0), "centered on the primary");
    let on_screen = rect(100.0, 100.0, 800.0, 600.0);
    assert_eq!(fit_on_displays(on_screen, &displays, Some(primary)), on_screen);
    assert_eq!(fit_on_displays(saved, &[], None), saved, "no display known: unchanged");
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-ui window_place::tests`
Expected: compile error.

- [ ] **Step 3: Implement.**

`theme.rs` after `WINDOW_MIN_HEIGHT`:

```rust
/// Smallest detached window (ADR 0096): a tab strip and one pane fit.
pub const DETACHED_MIN_WIDTH: f32 = 480.0;
pub const DETACHED_MIN_HEIGHT: f32 = 320.0;
/// Where a detached window opened from the keyboard goes: this far right and down of the
/// window it came from.
pub const DETACHED_OFFSET: f32 = 32.0;
```

`window_place.rs`:

```rust
//! Where a dragged tab lands and where a detached window opens (spec 2026-11-10 §5.1, §6.2).
//! Screen coordinates are the windows' `bounds()` (gpui's global space).

use gpui::{Bounds, Pixels, Point, Size, point, px, size};

use crate::theme;

/// A window's tab strip as painted: the strip and its chips, window-local.
#[derive(Debug, Clone, PartialEq)]
pub struct Strip {
    pub bounds: Bounds<Pixels>,
    pub chips: Vec<Bounds<Pixels>>,
}

/// A window on screen and its strip.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed<H> {
    pub host: H,
    pub window: Bounds<Pixels>,
    pub strip: Option<Strip>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target<H> {
    /// Into that window's strip, before chip `index` (`index == chips` = the end).
    Strip { host: H, index: usize },
    /// Into that window, at the end of its strip.
    Window(H),
    /// A new window with its top left here.
    NewWindow(Point<Pixels>),
}

/// The target of a tab released at screen `point` from `source`. `windows` are ordered most
/// recently activated first (the frontmost of overlapping Mkrew windows wins). `None` while
/// the point is inside the source window: its own handlers (reorder, split) decide.
// ponytail: other apps' windows are ignored (no OS z-order query); add one if drops through
// other apps confuse users. Mixed-DPI displays can be off by the scale difference.
pub fn drop_target<H: Copy + PartialEq>(
    point: Point<Pixels>,
    source: H,
    windows: &[Placed<H>],
) -> Option<Target<H>> {
    let Some(over) = windows.iter().find(|placed| placed.window.contains(&point)) else {
        return Some(Target::NewWindow(point));
    };
    if over.host == source {
        return None;
    }
    let local = point - over.window.origin;
    match &over.strip {
        Some(strip) if strip.bounds.contains(&local) => {
            let index = (strip.chips.iter())
                .position(|chip| local.x < chip.origin.x + chip.size.width / 2.0)
                .unwrap_or(strip.chips.len());
            Some(Target::Strip { host: over.host, index })
        }
        _ => Some(Target::Window(over.host)),
    }
}

/// A new detached window of `size` (at least the minimum): its top left at `at` (the release
/// point), else offset from `near` (the window it comes from); kept on the display it starts on.
pub fn new_window_bounds(
    at: Option<Point<Pixels>>,
    size_wanted: Size<Pixels>,
    near: Bounds<Pixels>,
    displays: &[Bounds<Pixels>],
) -> Bounds<Pixels> {
    let size = size(
        size_wanted.width.max(px(theme::DETACHED_MIN_WIDTH)),
        size_wanted.height.max(px(theme::DETACHED_MIN_HEIGHT)),
    );
    let offset = px(theme::DETACHED_OFFSET);
    let origin = at.unwrap_or(point(near.origin.x + offset, near.origin.y + offset));
    let display = (displays.iter())
        .find(|display| display.contains(&origin))
        .or_else(|| displays.iter().find(|display| display.contains(&near.origin)));
    let mut bounds = Bounds::new(origin, size);
    if let Some(display) = display {
        bounds.origin.x = bounds.origin.x.min(display.right() - size.width).max(display.origin.x);
        bounds.origin.y = bounds.origin.y.min(display.bottom() - size.height).max(display.origin.y);
    }
    bounds
}

/// Saved bounds that intersect no display (a monitor that is gone) move to the center of the
/// primary display; others stay.
pub fn fit_on_displays(
    bounds: Bounds<Pixels>,
    displays: &[Bounds<Pixels>],
    primary: Option<Bounds<Pixels>>,
) -> Bounds<Pixels> {
    if displays.iter().any(|display| display.intersects(&bounds)) {
        return bounds;
    }
    match primary {
        Some(primary) => Bounds::centered_at(primary.center(), bounds.size),
        None => bounds,
    }
}

#[cfg(test)]
mod tests;
```

Check: `Bounds::centered_at` is at `geometry.rs:882`, `center()` exists on `Bounds`; `Pixels` supports `/ 2.0` and `.min`/`.max` (used in the codebase, e.g. `theme.rs`). If `Point - Point` is not implemented for `Pixels`, write `point(point.x - over.window.origin.x, point.y - over.window.origin.y)`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p mkrew-ui window_place::tests`
Expected: PASS (7 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/window_place.rs crates/mkrew-ui/src/window_place/tests.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/theme.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): drop targets and bounds for detached windows"
```

---

### Task 5: `windows.json` (save, load, version, atomic write)

**Files:**
- Modify: `crates/mkrew-ui/src/detached.rs` (persistence section after `impl Detached`)
- Test: `crates/mkrew-ui/src/detached/tests.rs`

**Interfaces:**
- Produces:
  - `pub const WINDOWS_FILE: &str = "windows.json";`
  - `pub fn windows_file(instance: Option<&str>) -> String` (`windows-<instance_key>.json` for a machine window, like `inbox::viewed_file`)
  - `#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)] pub struct SavedBounds { pub x: f32, pub y: f32, pub width: f32, pub height: f32 }`
  - `#[derive(Debug, Clone, PartialEq)] pub struct SavedWindow { pub bounds: SavedBounds, pub maximized: bool, pub tabs: Vec<DockTab>, pub active: usize }`
  - `pub fn to_json(windows: &[SavedWindow]) -> String`
  - `pub fn from_json(text: &str) -> Result<Vec<SavedWindow>, String>`
  - `pub fn save(path: &Path, windows: &[SavedWindow]) -> std::io::Result<()>` (temp file + rename)

- [ ] **Step 1: Write the failing tests:**

```rust
fn saved(tabs: Vec<DockTab>) -> SavedWindow {
    SavedWindow {
        bounds: SavedBounds { x: 1920.0, y: 80.0, width: 1100.0, height: 760.0 },
        maximized: false,
        tabs,
        active: 0,
    }
}

#[test]
fn windows_round_trip() {
    let windows = vec![saved(vec![term(), term()])];
    let json = to_json(&windows);
    assert!(json.contains("\"version\":1"), "{json}");
    assert!(json.contains("\"terminal\""), "{json}");
    assert_eq!(from_json(&json), Ok(windows));
}

#[test]
fn unknown_version_and_broken_files_are_errors() {
    assert!(from_json(r#"{"version":2,"windows":[]}"#).is_err());
    assert!(from_json("not json").is_err());
    assert!(from_json("").is_err());
}

#[test]
fn unknown_tab_kinds_are_skipped_and_empty_windows_dropped() {
    let project = Ulid::new();
    let tab_id = Ulid::new();
    let json = format!(
        r#"{{"version":1,"windows":[
            {{"bounds":{{"x":0,"y":0,"width":800,"height":600}},"maximized":true,
              "tabs":[{{"file":{{"project":"{project}","path":"a.rs"}}}},
                      {{"terminal":{{"project":"{project}","tab_id":"{tab_id}"}}}}],"active":1}},
            {{"bounds":{{"x":0,"y":0,"width":800,"height":600}},"maximized":false,
              "tabs":[{{"file":{{"project":"{project}","path":"b.rs"}}}}],"active":0}}]}}"#
    );
    let windows = from_json(&json).unwrap();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].tabs, vec![DockTab::Terminal { project, tab_id }]);
    assert_eq!(windows[0].active, 0, "re-based onto the kept tabs");
    assert!(windows[0].maximized);
}

#[test]
fn save_writes_through_a_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(WINDOWS_FILE);
    let windows = vec![saved(vec![term()])];
    save(&path, &windows).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(from_json(&text), Ok(windows));
    let names: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names.len(), 1, "no temp file left: {names:?}");
}

#[test]
fn a_machine_window_has_its_own_file() {
    assert_eq!(windows_file(None), "windows.json");
    let file = windows_file(Some("machine:box"));
    assert!(file.starts_with("windows-") && file.ends_with(".json"), "{file}");
    assert_ne!(file, windows_file(Some("machine:other")));
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-ui detached::tests`
Expected: compile errors.

- [ ] **Step 3: Implement** (append to `detached.rs` before `#[cfg(test)]`, add `use std::path::Path;`):

```rust
/// This computer's detached windows, in the data dir (UI-only: the daemon never reads it).
pub const WINDOWS_FILE: &str = "windows.json";
const VERSION: u32 = 1;

/// The windows file of a window: [`WINDOWS_FILE`] for this computer's window (`None`), its own
/// `windows-<key>.json` for a machine window (same key as [`crate::inbox::viewed_file`]).
pub fn windows_file(instance: Option<&str>) -> String {
    instance.map_or_else(
        || WINDOWS_FILE.to_owned(),
        |id| format!("windows-{}.json", crate::desktop::instance_key("", id)),
    )
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// One saved window: restore bounds, maximized, strip, shown tab.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedWindow {
    pub bounds: SavedBounds,
    pub maximized: bool,
    pub tabs: Vec<DockTab>,
    pub active: usize,
}

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    windows: Vec<FileWindow>,
}

#[derive(Serialize, Deserialize)]
struct FileWindow {
    bounds: SavedBounds,
    #[serde(default)]
    maximized: bool,
    /// Raw, so a tab kind this build does not know (a newer build's) is skipped, not fatal.
    tabs: Vec<serde_json::Value>,
    #[serde(default)]
    active: usize,
}

pub fn to_json(windows: &[SavedWindow]) -> String {
    let file = File {
        version: VERSION,
        windows: (windows.iter())
            .map(|window| FileWindow {
                bounds: window.bounds.clone(),
                maximized: window.maximized,
                tabs: (window.tabs.iter())
                    .filter_map(|tab| serde_json::to_value(tab).ok())
                    .collect(),
                active: window.active,
            })
            .collect(),
    };
    // A struct of plain fields always serializes.
    serde_json::to_string(&file).unwrap_or_default()
}

pub fn from_json(text: &str) -> Result<Vec<SavedWindow>, String> {
    let file: File = serde_json::from_str(text).map_err(|error| error.to_string())?;
    if file.version != VERSION {
        return Err(format!("unknown version {}", file.version));
    }
    Ok((file.windows.into_iter())
        .filter_map(|window| {
            let shown = window.tabs.get(window.active).cloned();
            let tabs: Vec<DockTab> = (window.tabs.into_iter())
                .filter_map(|tab| serde_json::from_value(tab).ok())
                .collect();
            let shown = shown.and_then(|tab| serde_json::from_value::<DockTab>(tab).ok());
            let active = shown
                .and_then(|shown| tabs.iter().position(|tab| *tab == shown))
                .unwrap_or(0);
            (!tabs.is_empty()).then_some(SavedWindow {
                bounds: window.bounds,
                maximized: window.maximized,
                tabs,
                active,
            })
        })
        .collect())
}

/// Writes through `<file>.tmp` and a rename, so a crash never leaves half a file.
pub fn save(path: &Path, windows: &[SavedWindow]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension("json.tmp");
    let result = std::fs::write(&temp, to_json(windows)).and_then(|()| std::fs::rename(&temp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}
```

Also add a `Detached` method used by the shell (append inside `impl Detached`):

```rust
    /// Rebuilds the windows from a save (keys renumbered). Returns the keys in file order.
    pub fn restore(&mut self, saved: &[SavedWindow]) -> Vec<WindowKey> {
        saved
            .iter()
            .filter_map(|window| {
                let mut tabs = window.tabs.iter().copied().filter(|tab| !self.is_detached(tab));
                let first = tabs.next()?;
                let key = self.open(first);
                for tab in tabs {
                    self.move_tab(tab, HostId::Detached(key), None);
                }
                self.select(key, window.active);
                Some(key)
            })
            .collect()
    }
```

and its test:

```rust
#[test]
fn restore_rebuilds_windows_and_skips_duplicates() {
    let (a, b) = (term(), term());
    let mut detached = Detached::new();
    let keys = detached.restore(&[
        SavedWindow { active: 1, ..saved(vec![a, b]) },
        saved(vec![a]),
    ]);
    assert_eq!(keys.len(), 1, "a tab already restored is not restored twice");
    assert_eq!(detached.tabs_of(keys[0]), vec![a, b]);
    assert_eq!(detached.active(keys[0]), Some(b));
}
```

`tempfile` is a dev-dependency of mkrew-ui already (`Cargo.toml` `[dev-dependencies]`).

- [ ] **Step 4: Run tests**

Run: `cargo test -p mkrew-ui detached::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/detached.rs crates/mkrew-ui/src/detached/tests.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): save and load detached windows"
```

---

### Task 6: Host plumbing in `Shell` (no visible change yet)

This task threads the window through the functions a detached window reaches, adds the host lookup, and keeps the main window's behavior identical.
After it, the app behaves exactly as before (no detached window exists yet).

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs` (`struct Shell` fields; `use mkrew_ui::detached::{self, Detached, DockTab, HostId, WindowKey};`)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (`Shell::new`: field init after `toasts: Toasts::default(),` at about line 483; record the main window handle)
- Create: `crates/mkrew-ui/src/shell/hosts.rs` (`mod hosts;` in `shell.rs`)
- Modify (signature threading, listed below): `shell/focus.rs`, `shell/composer.rs`, `shell/menus.rs`, `shell/menu_render.rs`, `shell/navigation.rs`, `shell/peek.rs`, `shell/send_dialog.rs`, `shell/sessions.rs`, `shell/sidebar.rs`, `shell/tabs.rs`, `shell/connection.rs`, `shell/settings.rs`, `shell/slash.rs`, `shell/pane_render.rs`, `shell/inspector.rs`, `shell/workspace.rs`, `main.rs`
- Modify: `shell/menu_render.rs` (`MenuAnchors` per window)

**Interfaces:**
- Consumes: Task 1 (`Detached`, `HostId`, `WindowKey`, `DockTab`), Task 2 (`Tabs::with_active`, `focus_pane`, `activate_skipping`).
- Produces (all `pub(crate)` on `Shell`):
  - fields: `detached: Detached`, `main_window: AnyWindowHandle`, `windows: HashMap<AnyWindowHandle, WindowKey>`, `window_handles: HashMap<WindowKey, AnyWindowHandle>`, `hosts: HashMap<HostId, HostUi>`, `menu_host: HostId`, `palette_host: HostId`, `activation: Vec<HostId>` (most recent first), `main_visible: bool`, `link_subscriptions: HashMap<Ulid, Subscription>`, `windows_dirty: bool`, `restore: Option<Restore>`
  - `struct HostUi { pub(crate) bounds: Bounds<Pixels>, pub(crate) restore: Bounds<Pixels>, pub(crate) maximized: bool, pub(crate) strip: Rc<RefCell<Option<window_place::Strip>>>, pub(crate) content: Rc<Cell<Size<Pixels>>> }` (`Default`)
  - `fn host_of(&self, window: &Window) -> HostId`
  - `fn detached_key(&self, window: &Window) -> Option<WindowKey>`
  - `fn tab_scope(&self, window: &Window) -> Option<(Ulid, Ulid)>` (project, tab id shown in that window)
  - `fn with_tab<R>(&mut self, window: &Window, f: impl FnOnce(&mut Tabs) -> R) -> Option<R>`
  - `fn focused_session(&self, window: &Window) -> Option<Ulid>` (replaces the window-less one)
  - `fn composer_session(&self, window: &Window) -> Option<Ulid>`, `fn focused_agent(&self, window: &Window) -> Option<Ulid>`
  - `fn focus_session_pane(&mut self, session: Ulid)`
  - `fn keep_main_active_visible(&mut self)`
  - `fn in_window(&self, host: HostId, cx: &mut Context<'_, Self>, f: impl FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>) + 'static)`
  - `fn terminal_host(&self, project: Ulid, tab_id: Ulid) -> HostId`

- [ ] **Step 1: Add the fields** to `struct Shell` (main.rs, after `toasts: Toasts,`):

```rust
    /// Which terminal tab is in which detached window (ADR 0096).
    detached: Detached,
    /// The main window, for work that must run there (a file link from a detached pane).
    main_window: gpui::AnyWindowHandle,
    /// The detached OS windows and their keys, both ways.
    windows: HashMap<gpui::AnyWindowHandle, WindowKey>,
    window_handles: HashMap<WindowKey, gpui::AnyWindowHandle>,
    /// Per window: screen bounds, maximized, the strip and content size painted last.
    hosts: HashMap<HostId, shell::hosts::HostUi>,
    /// The window the open popover menu / the palette belongs to (spec §4.3).
    menu_host: HostId,
    palette_host: HostId,
    /// Mkrew windows, most recently activated first (drop target order, toast placement).
    activation: Vec<HostId>,
    /// The main window is shown (not minimized or hidden): its panes stay attached.
    main_visible: bool,
    /// `OpenLink` of each attached pane, kept so a move to another window subscribes again.
    link_subscriptions: HashMap<Ulid, gpui::Subscription>,
    /// A detached window moved, resized or changed tabs: saved on the next status tick.
    windows_dirty: bool,
    /// Saved windows waiting for their workspaces' tabs (startup).
    restore: Option<shell::hosts::Restore>,
```

In `Shell::new` (connection.rs) before the struct literal: `let main_window = window.window_handle();` and in the literal after `toasts: Toasts::default(),`:

```rust
            detached: Detached::new(),
            main_window,
            windows: HashMap::new(),
            window_handles: HashMap::new(),
            hosts: HashMap::from([(HostId::Main, shell::hosts::HostUi::default())]),
            menu_host: HostId::Main,
            palette_host: HostId::Main,
            activation: vec![HostId::Main],
            main_visible: true,
            link_subscriptions: HashMap::new(),
            windows_dirty: false,
            restore: None,
```

Update the existing visibility observer in `Shell::new` (connection.rs, `cx.observe_window_visibility(window, |shell, _, window, cx| {`) to record it:

```rust
        cx.observe_window_visibility(window, |shell, visibility, window, cx| {
            shell.main_visible = visibility == gpui::WindowVisibility::Visible;
            shell.sync_panes(window, cx);
            cx.notify();
        })
        .detach();
```

and the activation observer to keep the order:

```rust
        cx.observe_window_activation(window, |shell, window, cx| {
            if window.is_window_active() {
                shell.activated(HostId::Main);
                shell.viewed.activated();
                shell.desktop.flash(false);
            }
            cx.notify();
        })
        .detach();
        cx.observe_window_bounds(window, |shell, window, _| {
            shell.record_bounds(HostId::Main, window);
        })
        .detach();
```

- [ ] **Step 2: Create `shell/hosts.rs`:**

```rust
//! Which window code runs for, and the per-window state (spec 2026-11-10 §4.2, §4.3).

use crate::*;

/// What each window shows on its own.
#[derive(Default)]
pub(crate) struct HostUi {
    /// Screen bounds (`Window::bounds`), restore bounds when maximized.
    pub(crate) bounds: Bounds<Pixels>,
    /// What `windows.json` keeps: the restore rect (`Window::window_bounds`).
    pub(crate) restore: Bounds<Pixels>,
    pub(crate) maximized: bool,
    /// The tab strip as painted last (window-local), for drops from other windows.
    pub(crate) strip: Rc<RefCell<Option<mkrew_ui::window_place::Strip>>>,
    /// The tab content area as painted last: the size of a window torn off from here.
    pub(crate) content: Rc<Cell<gpui::Size<Pixels>>>,
}

/// Saved windows waiting until every workspace they show has its tabs loaded.
pub(crate) struct Restore {
    pub(crate) windows: Vec<detached::SavedWindow>,
    pub(crate) waiting: HashSet<Ulid>,
}

impl Shell {
    pub(crate) fn host_of(&self, window: &Window) -> HostId {
        self.detached_key(window).map_or(HostId::Main, HostId::Detached)
    }

    pub(crate) fn detached_key(&self, window: &Window) -> Option<WindowKey> {
        self.windows.get(&window.window_handle()).copied()
    }

    /// Where a terminal tab is shown.
    pub(crate) fn terminal_host(&self, project: Ulid, tab_id: Ulid) -> HostId {
        self.detached.host_of(&DockTab::Terminal { project, tab_id })
    }

    /// The terminal tab `window` shows: the main window's shown workspace and active tab, or
    /// the detached window's active tab.
    pub(crate) fn tab_scope(&self, window: &Window) -> Option<(Ulid, Ulid)> {
        match self.host_of(window) {
            HostId::Main => {
                let project = self.workspace.filter(|_| self.view() == View::Terminals)?;
                Some((project, self.tabs.get(&project)?.active()?.id))
            }
            HostId::Detached(key) => match self.detached.active(key)? {
                DockTab::Terminal { project, tab_id } => Some((project, tab_id)),
            },
        }
    }

    /// `f` on the tabs of `window`'s tab, with that tab active for the call. In the main
    /// window the active tab is the shown one and changes stick (a tab switch); in a detached
    /// window the main window's active tab is given back.
    pub(crate) fn with_tab<R>(
        &mut self,
        window: &Window,
        f: impl FnOnce(&mut Tabs) -> R,
    ) -> Option<R> {
        match self.host_of(window) {
            HostId::Main => {
                let project = self.workspace?;
                Some(f(self.tabs.get_mut(&project)?))
            }
            HostId::Detached(_) => {
                let (project, tab_id) = self.tab_scope(window)?;
                self.tabs.get_mut(&project)?.with_active(tab_id, f)
            }
        }
    }

    /// The pane that has (or gets) the keyboard in `window`.
    pub(crate) fn focused_session(&self, window: &Window) -> Option<Ulid> {
        let (project, tab_id) = self.tab_scope(window)?;
        self.tabs.get(&project)?.tab(tab_id)?.focused()
    }

    /// Focuses `session`'s pane in its tab; a tab in a detached window does not become the
    /// main window's active tab (spec §4.4).
    pub(crate) fn focus_session_pane(&mut self, session: Ulid) {
        let Some((project, tab_id)) = (self.tabs.iter())
            .find_map(|(project, tabs)| Some((*project, tabs.tab_of(session)?.id)))
        else {
            return;
        };
        let detached = self.terminal_host(project, tab_id) != HostId::Main;
        if let Some(tabs) = self.tabs.get_mut(&project) {
            if detached {
                tabs.focus_pane(tab_id, session);
            } else {
                tabs.focus(session);
            }
        }
    }

    /// The main window's active tab never names a detached tab (spec §4.1).
    pub(crate) fn keep_main_active_visible(&mut self) {
        for (project, tabs) in self.tabs.iter_mut() {
            let project = *project;
            let detached = &self.detached;
            tabs.activate_skipping(|tab_id| {
                detached.is_detached(&DockTab::Terminal { project, tab_id })
            });
        }
    }

    /// Runs `f` with `host`'s window once the current update is over (the `Shell` and the
    /// window that started it are busy until then).
    pub(crate) fn in_window(
        &self,
        host: HostId,
        cx: &mut Context<'_, Self>,
        f: impl FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>) + 'static,
    ) {
        let handle = match host {
            HostId::Main => Some(self.main_window),
            HostId::Detached(key) => self.window_handles.get(&key).copied(),
        };
        let Some(handle) = handle else {
            return;
        };
        let this = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                let _ = this.update(cx, |shell, cx| f(shell, window, cx));
            });
        });
    }

    pub(crate) fn activated(&mut self, host: HostId) {
        self.activation.retain(|known| *known != host);
        self.activation.insert(0, host);
    }

    pub(crate) fn record_bounds(&mut self, host: HostId, window: &Window) {
        let (bounds, maximized) = match window.window_bounds() {
            gpui::WindowBounds::Maximized(bounds) => (bounds, true),
            gpui::WindowBounds::Windowed(bounds) | gpui::WindowBounds::Fullscreen(bounds) => {
                (bounds, false)
            }
        };
        let ui = self.hosts.entry(host).or_default();
        ui.bounds = window.bounds();
        ui.restore = bounds;
        ui.maximized = maximized;
        if host != HostId::Main {
            self.windows_dirty = true;
        }
    }
}
```


- [ ] **Step 3: Thread the window through `focused_session`.**
Delete the old window-less `focused_session` in `shell/focus.rs` (about line 128-134).
Every caller passes the `window` it already has; where a function has no `window`, add a `window: &Window` parameter and pass it from its callers (the compiler lists them).
Callers to convert (from `grep -rn 'focused_session()' crates/mkrew-ui/src`, 2026-11-10):

| File:line | Context |
| --- | --- |
| `shell/composer.rs:8`, `:14` | `composer_session`, `focused_agent`: add `window: &Window` to both, pass it down |
| `shell/connection.rs:1121` | `focused: self.focused_session()` |
| `shell/focus.rs:251` | `focus_pane` |
| `shell/focus.rs:344` | `attach` continuation (`window` from `update_in`) |
| `shell/menus.rs:204` | `tab_menu_rows`: add `window: &Window` (its callers `open_menu_rows`, `render_tab_menu` get it) |
| `shell/menu_render.rs:392`, `:616` | `keyboard_owner`, `menu_focus_state` (both have `window`) |
| `shell/navigation.rs:378`, `:404`, `:436`, `:515`, `:780`, `:801`, `:826` | `open_palette`, `open_theme_picker`, `run_palette_pick` |
| `shell/peek.rs:75`, `:84`, `:99`, `:146` | peek (main window only; pass its window) |
| `shell/send_dialog.rs:200` | send dialog |
| `shell/sessions.rs:359` | `run` `RenamePane` |
| `shell/sidebar.rs:71`, `:374`, `:872` | sidebar rows (main window) |
| `shell/tabs.rs:49`, `:98`, `:719` | `split`, `apply_layout`, `close_focused_pane` |
| `main.rs:928` | `Render for Shell` |

Same for `composer_session` (callers: `shell/composer.rs:42`, `:173`, `:444`, `shell/navigation.rs:821`, `shell/send_dialog.rs:255`, `shell/sessions.rs:274`, `shell/settings.rs:20`, `:36`, `shell/slash.rs:100`) and `focused_agent` (grep `focused_agent()`).
`open_menu_rows` gets `window: &Window` (it calls `tab_menu_rows`); its callers are `Render for Shell` and `finish_menu` (both have `window`).
`focused_session` keeps its old behavior in the main window: `tab_scope` returns `None` outside the Terminals view, as the old function did.

- [ ] **Step 4: Replace `active_tabs_mut()` at the sites a detached window reaches.**

| Site | New code |
| --- | --- |
| `shell/pane_render.rs:433` (divider drag) | `.with_tab(window, \|tabs\| tabs.drag_divider(divider.index, ratio)).unwrap_or(false)` |
| `shell/pane_render.rs:492` (pane click) | `shell.focus_session_pane(session);` |
| `shell/composer.rs:853` (composer click) | `shell.focus_session_pane(session);` |
| `shell/navigation.rs:147` (`type_into_pane`) | `self.focus_session_pane(session);` |
| `shell/tabs.rs:431` (`open_pane_menu`) | `self.focus_session_pane(session_id);` |
| `shell/tabs.rs:215` (`end_pane_drag`) | `let Some((project_id, _)) = self.tab_scope(window) else { return; }; if let Some(write) = self.with_tab(window, Tabs::write_active).flatten() { self.save_layout(project_id, write, window, cx); }` |
| `shell/inspector.rs:161` (`move_focus`) | `.with_tab(window, \|tabs\| tabs.focus_neighbour(direction)).unwrap_or(false)` |
| `shell/sessions.rs:385`, `:396`, `:403` (Resize, Equalize, Zoom in `run`) | `self.with_tab(window, ...)` with the same closures; `project_id` from `self.tab_scope(window)` |

The others (`pane_render.rs:116` main strip click, `tabs.rs:25` `switch_tab`, `tabs.rs:178` `open_tab_menu`, `tabs.rs:708` `close_other_tabs`) are main-window only and stay.

- [ ] **Step 5: Make the remaining tab helpers take the scope from the window.**
- `new_pane_with` (`shell/sessions.rs:31-41`): replace the `self.workspace` / `tabs.active()` lookup with `let Some((project_id, tab_id)) = self.tab_scope(window) else { return; };`.
- `apply_layout` (`shell/tabs.rs:59-74`): same replacement for `project_id` and `tab_id`; its `focused_session()` becomes `focused_session(window)`.
- `start_rename` (`shell/tabs.rs:153-166`): `let Some((project, tab_id)) = self.tab_scope(window) else { return; }; let Some(tab) = self.tabs.get(&project).and_then(|tabs| tabs.tab(tab_id)) else { return; };`.
- `close_tabs` (`shell/tabs.rs:617`): new signature `close_tabs(&mut self, project_id: Ulid, tab_ids: Vec<Ulid>, window, cx)`; delete its `let Some(project_id) = self.workspace` lines; callers pass `self.workspace` (main window: `close_tab`, `close_other_tabs`) or the detached tab's project.
  `close_tab(tab_id, window, cx)` becomes `close_tab(project_id, tab_id, window, cx)`; callers: `pane_render.rs:179` (pass `project_id` in scope), `menus.rs` "Close" row (the `TabMenu` caller has the project through `self.workspace`; Task 8 gives detached rows their own), `sessions.rs` `CloseTab` arm.
- `place_session` (`shell/workspace.rs:268-287`): before `if tabs.focus(session)`, handle a detached tab:

```rust
        if let Some(tab_id) = tabs.tab_of(session).map(|tab| tab.id)
            && self.detached.is_detached(&DockTab::Terminal { project: project_id, tab_id })
        {
            if let Some(tabs) = self.tabs.get_mut(&project_id) {
                tabs.focus_pane(tab_id, session);
            }
            return;
        }
```

(re-borrow `tabs` after the check; the borrow checker needs the lookup split from the mutation.)

- [ ] **Step 6: `sync_panes` sees every window.**
Replace `shell/focus.rs` `sync_panes` with:

```rust
    /// Attaches the sessions on screen in any window (live ones, or already attached; none
    /// for a hidden window or while the main window shows another view) and detaches all others
    /// (SPEC §10), then focuses the focused pane of `window`.
    pub(crate) fn sync_panes(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.keep_main_active_visible();
        self.prune_detached(cx);
        let mut visible = on_screen(
            self.main_visible,
            self.workspace
                .filter(|_| self.view() == View::Terminals)
                .and_then(|id| self.tabs.get(&id)),
        );
        let here = self.host_of(window);
        let mut elsewhere: Vec<(HostId, Ulid)> = Vec::new();
        for tab in self.detached.visible_tabs() {
            let DockTab::Terminal { project, tab_id } = tab;
            let Some(shown) = self.tabs.get(&project).and_then(|tabs| tabs.tab(tab_id)) else {
                continue;
            };
            let host = self.detached.host_of(&tab);
            for session in shown.shown() {
                visible.push(session);
                if host != here {
                    elsewhere.push((host, session));
                }
            }
        }
        let main_sessions: Vec<Ulid> = visible.clone();
        for session in self.router.wanted() {
            if visible.contains(&session) {
                continue;
            }
            if self.router.forget(session)
                && let Some(connection) = &self.connection
            {
                detach(connection, session, cx);
            }
            self.panes.remove(&session);
            self.link_subscriptions.remove(&session);
        }
        self.attach_errors.retain(|id, _| visible.contains(id));
        for &session in &main_sessions {
            let skip = self.router.is_wanted(session)
                || self.attach_errors.contains_key(&session)
                || self.is_exited(session);
            if skip {
                continue;
            }
            match elsewhere.iter().find(|(_, s)| *s == session) {
                // A pane is built in the window that shows it.
                Some(&(host, _)) => self.in_window(host, cx, move |shell, window, cx| {
                    if !shell.router.is_wanted(session) {
                        shell.attach(session, window, cx);
                    }
                }),
                None => self.attach(session, window, cx),
            }
        }
        self.focus_pane(window, cx);
    }
```

`prune_detached` (in `shell/hosts.rs`):

```rust
    /// Drops detached tabs that are gone (closed, workspace archived); a workspace whose tabs
    /// are not loaded (yet, or again after a reconnect) keeps them. Emptied windows close.
    pub(crate) fn prune_detached(&mut self, cx: &mut Context<'_, Self>) {
        // The sidebar's list is the daemon's once connected: a workspace missing there is
        // archived. Before that (startup, reconnect) nothing is pruned for it.
        let listed = self.status == Status::Connected;
        let emptied = self.detached.prune(|tab| {
            let DockTab::Terminal { project, tab_id } = *tab;
            let archived = listed && !self.sidebar.projects().iter().any(|p| p.id == project);
            match self.tabs.get(&project) {
                _ if archived => detached::Keep::No,
                Some(tabs) if tabs.tab(tab_id).is_some() => detached::Keep::Yes,
                Some(_) => detached::Keep::No,
                None => detached::Keep::Unknown,
            }
        });
        for key in emptied {
            self.close_os_window(key, cx);
        }
    }

    /// Removes the OS window of `key` (its tabs already left the model).
    pub(crate) fn close_os_window(&mut self, key: WindowKey, cx: &mut Context<'_, Self>) {
        let Some(handle) = self.window_handles.remove(&key) else {
            return;
        };
        self.windows.remove(&handle);
        self.hosts.remove(&HostId::Detached(key));
        self.activation.retain(|host| *host != HostId::Detached(key));
        self.windows_dirty = true;
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        });
    }
```

`attach` (`shell/focus.rs`): store the `OpenLink` subscription instead of detaching it:

```rust
                let link = cx.subscribe_in(
                    &pane,
                    window,
                    move |shell, _, link: &OpenLink, window, cx| {
                        shell.open_terminal_link(session, link, window, cx);
                    },
                );
                shell.link_subscriptions.insert(session, link);
```

`reset_connection_state` (`shell/connection.rs:776`): add `self.link_subscriptions.clear();` next to `self.panes.clear();`.

- [ ] **Step 7: Menu anchors per window** (`shell/menu_render.rs:539-611`).
Key the map by window: `MenuAnchors(Rc<RefCell<HashMap<(gpui::WindowId, Anchor), Bounds<Pixels>>>>)`.
`clear(&self, window: gpui::WindowId)` keeps other windows' entries and this window's sidebar ones: `retain(|(id, key), _| *id != window || key.in_sidebar())`; `clear_sidebar` keeps its body (all windows; only the main window has a sidebar).
`record_bounds`: `canvas(move |bounds, window, _| { anchors.0.borrow_mut().insert((window.window_handle().window_id(), key), bounds); }, ...)`.
`bottom_left(&self, window: gpui::WindowId, key: &Anchor)`; `menu_anchor` passes `window.window_handle().window_id()` for both lookups.
`Render for Shell` calls `cx.global::<MenuAnchors>().clear(window.window_handle().window_id());`.
Grep `clear_sidebar` / `bottom_left` for the remaining callers.

- [ ] **Step 8: Build, test, smoke**

Run: `cargo build -p mkrew-ui` then `cargo test -p mkrew-ui` then `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`.
Expected: green.
Smoke: run the app (`cargo run -p mkrew-ui`), open two tabs with split panes, click panes, Alt+arrows, Ctrl+Shift+M zoom, drag a divider, Shift+F10 on a pane, Ctrl+Shift+P: all behave as before.

- [ ] **Step 9: Commit**

```bash
git add crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/shell/
git -c user.email=tungnv975@gmail.com commit -m "refactor(ui): the window decides which tab code acts on"
```

---

### Task 7: `TerminalPane::rehost`

**Files:**
- Modify: `crates/mkrew-ui/src/terminal/view.rs` (`struct TerminalPane` at about line 47-82, `with_emulator` at about line 178-220)

**Interfaces:**
- Produces: `pub fn rehost(&mut self, window: &mut Window, cx: &mut Context<'_, Self>)`.

- [ ] **Step 1: Implement.** Add a field `window_subscriptions: [gpui::Subscription; 2],` to `TerminalPane` (doc: "Appearance and blur of the window the pane is in; replaced by [`TerminalPane::rehost`]."). In `with_emulator`, replace the two `.detach()` calls:

```rust
        let focus_handle = cx.focus_handle();
        let window_subscriptions = Self::window_subscriptions(&focus_handle, window, cx);
```

and set `window_subscriptions,` in the struct literal. Add:

```rust
    /// The pane now shows in `window` (ADR 0096): its appearance and blur follow that window.
    /// Emulator, scrollback, selection, copy mode and focus handle stay.
    pub fn rehost(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.window_subscriptions = Self::window_subscriptions(&self.focus_handle, window, cx);
        cx.notify();
    }

    fn window_subscriptions(
        focus_handle: &FocusHandle,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> [gpui::Subscription; 2] {
        let appearance = cx.observe_window_appearance(window, |_, _, cx| cx.notify());
        // Labels belong to the keys that type them: leaving the pane ends them.
        let blur = cx.on_blur(focus_handle, window, |pane, _, cx| {
            if pane.hints.is_some() {
                pane.stop_hints();
                cx.notify();
            }
        });
        [appearance, blur]
    }
```

- [ ] **Step 2: Build and test**

Run: `cargo test -p mkrew-ui terminal`
Expected: PASS (no behavior change; `rehost` is used from Task 8).

- [ ] **Step 3: Commit**

```bash
git add crates/mkrew-ui/src/terminal/view.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): a terminal pane can move to another window"
```

---

### Task 8: Detached window: open, render, commands, move back

**Files:**
- Create: `crates/mkrew-ui/src/shell/detached_view.rs` (`pub(crate) mod detached_view;` in `shell.rs`): the `DetachedWindow` root view
- Create: `crates/mkrew-ui/src/shell/detached_render.rs` (`mod detached_render;`): `render_host`, title bar, strip
- Create: `crates/mkrew-ui/src/shell/detached_actions.rs` (`mod detached_actions;`): open, move, run_detached, menus
- Modify: `shell/sessions.rs` (`run` routing; the `MoveTabToNewWindow` arm), `shell/navigation.rs` (palette for a detached window, extras), `shell/menus.rs` (tab menu rows), `main.rs` (`Render for Shell`: menus and palette only when their host is Main; toasts; the modal focus fix), `shell/navigation.rs` `open_terminal_link`
- Modify: `crates/mkrew-ui/src/shell/pane_render.rs` (`render_tab` takes the tab, not the `Tabs`)

**Interfaces:**
- Consumes: Tasks 1-7.
- Produces:
  - `pub(crate) struct DetachedWindow { shell: WeakEntity<Shell>, key: WindowKey, focus_handle: FocusHandle, _subscriptions: Vec<Subscription> }` implementing `Render`
  - `Shell::move_tab_to_new_window(&mut self, tab: DockTab, at: Option<Point<Pixels>>, window: &mut Window, cx)`
  - `Shell::move_tab_to(&mut self, tab: DockTab, to: HostId, index: Option<usize>, window: &mut Window, cx)`
  - `Shell::move_all_back(&mut self, key: WindowKey, window, cx)`
  - `Shell::run_detached(&mut self, key: WindowKey, command: Command, window, cx)`
  - `Shell::render_host(&mut self, key: WindowKey, window: &mut Window, cx) -> AnyElement`
  - `Shell::window_opened(&mut self, key, handle, window, cx)`

- [ ] **Step 1: `render_tab` takes a tab.** In `shell/pane_render.rs:331`, change the signature to `render_tab(&self, project_id: Ulid, tab: Option<&Tab>, now, composer_focused, voice_ring, theme, cx)` and replace each `tabs.active()` inside it with `tab`.
The empty-state "New agent / New shell" buttons call `new_pane`, which now reads the window's scope (Task 6), so they work in both windows.
`render_main` (chrome.rs:59) passes `project_id, tabs.active()`.
Record the content size for torn-off windows: wrap the returned `div` with a child `canvas` that stores its size in `self.hosts[host].content` (`HostUi.content.set(bounds.size)` in the prepaint closure); the caller passes the `Rc<Cell<Size<Pixels>>>` in (add a parameter `content: Rc<Cell<gpui::Size<Pixels>>>`).

- [ ] **Step 2: `DetachedWindow`** (`shell/detached_view.rs`):

```rust
//! The root view of a detached window (ADR 0096): `Shell` draws it and handles its input.

use crate::*;

pub(crate) struct DetachedWindow {
    shell: WeakEntity<Shell>,
    key: WindowKey,
    focus_handle: FocusHandle,
    _subscriptions: Vec<gpui::Subscription>,
}

impl DetachedWindow {
    pub(crate) fn new(
        shell: Entity<Shell>,
        key: WindowKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let weak = shell.downgrade();
        let subscriptions = vec![
            // Every `Shell` change may change what this window shows.
            cx.observe(&shell, |_, _, cx| cx.notify()),
            cx.observe_window_appearance(window, |_, window, cx| {
                theme::refresh(cx.theme().mode, window, cx);
                cx.notify();
            }),
            cx.observe_window_activation(window, move |this, window, cx| {
                if window.is_window_active() {
                    let _ = this.shell.update(cx, |shell, _| {
                        shell.activated(HostId::Detached(key));
                    });
                }
            }),
            cx.observe_window_bounds(window, move |this, window, cx| {
                let _ = this.shell.update(cx, |shell, _| {
                    shell.record_bounds(HostId::Detached(key), window);
                });
            }),
            cx.observe_window_visibility(window, move |this, visibility, window, cx| {
                let shown = visibility == gpui::WindowVisibility::Visible;
                let _ = this.shell.update(cx, |shell, cx| {
                    shell.detached.set_visible(key, shown);
                    shell.sync_panes(window, cx);
                });
            }),
        ];
        let close = weak.clone();
        window.on_window_should_close(cx, move |window, cx| {
            close
                .update(cx, |shell, cx| shell.detached_should_close(key, window, cx))
                .unwrap_or(true)
        });
        Self {
            shell: weak,
            key,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }
}

impl Render for DetachedWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let key = self.key;
        let content = self
            .shell
            .update(cx, |shell, cx| shell.render_host(key, window, cx))
            .unwrap_or_else(|_| div().into_any_element());
        let shell = self.shell.clone();
        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .key_context(TERMINAL_CONTEXT)
            .on_action(cx.listener(move |_, action: &Run, window, cx| {
                let _ = shell.update(cx, |shell, cx| shell.run(action, window, cx));
            }))
            .child(content)
    }
}
```

`TERMINAL_CONTEXT` on the root keeps the window's key context like a focused pane's when no pane has focus; the pane's own `key_context` is set by its element as today.

- [ ] **Step 3: Opening a window** (`shell/detached_actions.rs`):

```rust
//! Moving terminal tabs between windows and the keys of a detached window (ADR 0096).

use crate::*;

impl Shell {
    /// Shift+F4, the tab menu, the palette, or a drop outside every window (`at` = release
    /// point): `tab` gets a window of its own.
    pub(crate) fn move_tab_to_new_window(
        &mut self,
        tab: DockTab,
        at: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let source = self.host_of(window);
        let ui = self.hosts.entry(source).or_default();
        let (near, size) = (ui.bounds, ui.content.get());
        let displays: Vec<Bounds<Pixels>> = cx.displays().iter().map(|d| d.bounds()).collect();
        let bounds = mkrew_ui::window_place::new_window_bounds(at, size, near, &displays);
        let moved = self.detached.move_tab(tab, HostId::Main, None);
        let key = self.detached.open(tab);
        self.after_move(moved, cx);
        self.open_os_window(key, bounds, false, window, cx);
    }

    /// Opens the OS window of `key` (its tabs are in the model already). Off the current
    /// update: the new window draws `Shell`, which is busy until this update ends.
    pub(crate) fn open_os_window(
        &mut self,
        key: WindowKey,
        bounds: Bounds<Pixels>,
        maximized: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.hosts.insert(HostId::Detached(key), HostUi::default());
        self.windows_dirty = true;
        self.sync_panes(window, cx);
        let shell = cx.entity();
        let mode = cx.theme().mode;
        cx.spawn(async move |this, cx| {
            let options = WindowOptions {
                window_bounds: Some(if maximized {
                    WindowBounds::Maximized(bounds)
                } else {
                    WindowBounds::Windowed(bounds)
                }),
                titlebar: Some(TitlebarOptions {
                    title: Some("Mkrew".into()),
                    appears_transparent: CUSTOM_TITLE_BAR,
                    traffic_light_position: cfg!(target_os = "macos").then(|| {
                        let (x, y) = theme::MAC_TRAFFIC_LIGHT_POS;
                        point(px(x), px(y))
                    }),
                }),
                window_min_size: Some(size(
                    px(theme::DETACHED_MIN_WIDTH),
                    px(theme::DETACHED_MIN_HEIGHT),
                )),
                app_owns_titlebar_drag: cfg!(target_os = "macos"),
                ..Default::default()
            };
            let opened = cx.open_window(options, |window, cx| {
                theme::refresh(mode, window, cx);
                cx.new(|cx| DetachedWindow::new(shell, key, window, cx))
            });
            match opened {
                Ok(handle) => {
                    let handle: gpui::AnyWindowHandle = handle.into();
                    let _ = handle.update(cx, |_, window, cx| {
                        let _ = this.update(cx, |shell, cx| {
                            shell.window_opened(key, handle, window, cx);
                        });
                    });
                }
                Err(error) => {
                    let _ = this.update(cx, |shell, cx| {
                        tracing::warn!("cannot open a window: {error:#}");
                        shell.detached.remove_window(key);
                        shell.hosts.remove(&HostId::Detached(key));
                        shell.toast_error(format!("Cannot open a window: {error:#}"), cx);
                    });
                }
            }
        })
        .detach();
    }

    /// The OS window of `key` exists: its panes move in, its pane gets the keyboard.
    pub(crate) fn window_opened(
        &mut self,
        key: WindowKey,
        handle: gpui::AnyWindowHandle,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.windows.insert(handle, key);
        self.window_handles.insert(key, handle);
        self.activated(HostId::Detached(key));
        self.record_bounds(HostId::Detached(key), window);
        self.rehost_window(key, window, cx);
        self.sync_panes(window, cx);
        cx.notify();
    }

    /// Every attached pane of `key`'s tabs now lives in `window`.
    fn rehost_window(&mut self, key: WindowKey, window: &mut Window, cx: &mut Context<'_, Self>) {
        for tab in self.detached.tabs_of(key) {
            self.rehost_tab(tab, window, cx);
        }
    }

    pub(crate) fn rehost_tab(&mut self, tab: DockTab, window: &mut Window, cx: &mut Context<'_, Self>) {
        let DockTab::Terminal { project, tab_id } = tab;
        let sessions = (self.tabs.get(&project))
            .and_then(|tabs| tabs.tab(tab_id))
            .map(Tab::sessions)
            .unwrap_or_default();
        for session in sessions {
            let Some(pane) = self.panes.get(&session).cloned() else {
                continue;
            };
            pane.update(cx, |pane, cx| pane.rehost(window, cx));
            let link = cx.subscribe_in(&pane, window, move |shell, _, link: &OpenLink, window, cx| {
                shell.open_terminal_link(session, link, window, cx);
            });
            self.link_subscriptions.insert(session, link);
        }
    }

    /// Moves `tab` to `to` (strip `index`, else the end): its panes follow, `to` shows it and
    /// takes the keyboard. The main window also switches to the tab's workspace.
    pub(crate) fn move_tab_to(
        &mut self,
        tab: DockTab,
        to: HostId,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let moved = self.detached.move_tab(tab, to, index);
        self.after_move(moved, cx);
        let DockTab::Terminal { project, tab_id } = tab;
        if to == HostId::Main {
            if let Some(tabs) = self.tabs.get_mut(&project) {
                tabs.activate(tab_id);
            }
            self.views.insert(project, View::Terminals);
        }
        self.windows_dirty = true;
        self.in_window(to, cx, move |shell, window, cx| {
            shell.rehost_tab(tab, window, cx);
            if to == HostId::Main {
                shell.activate_workspace(project, window, cx);
            }
            shell.sync_panes(window, cx);
            window.activate_window();
            cx.notify();
        });
        self.sync_panes(window, cx);
        cx.notify();
    }

    /// "Move all back": every tab of `key` returns to the main window, in strip order.
    pub(crate) fn move_all_back(&mut self, key: WindowKey, window: &mut Window, cx: &mut Context<'_, Self>) {
        let tabs = self.detached.tabs_of(key);
        let last = tabs.last().copied();
        for tab in tabs {
            let moved = self.detached.move_tab(tab, HostId::Main, None);
            self.after_move(moved, cx);
        }
        if let Some(tab) = last {
            // The last one is shown; the others wait in the main strip.
            self.move_tab_to(tab, HostId::Main, None, window, cx);
        }
    }

    /// A move left a window empty: its OS window goes.
    fn after_move(&mut self, moved: detached::Moved, cx: &mut Context<'_, Self>) {
        if let Some(key) = moved.emptied {
            self.close_os_window(key, cx);
        }
    }
}
```

Note `move_all_back`: the tabs left before the last one moved to the main window too (the loop), so `move_tab_to` on the last one finds it in Main already: `Detached::move_tab` to Main of a tab not detached is a no-op returning `(Main, None)`, then the rest runs (activation, rehost of that tab only).
Rehost the other tabs too: change the loop to collect them and run `self.in_window(HostId::Main, cx, move |shell, window, cx| for tab in tabs { shell.rehost_tab(tab, window, cx) })` before the final `move_tab_to`.

`HostUi` import: `use crate::shell::hosts::HostUi;` at the top of the file; `DetachedWindow` from `crate::shell::detached_view::DetachedWindow`; `CUSTOM_TITLE_BAR` is a private const in `main.rs`: make it `pub(crate) const`.

- [ ] **Step 4: Keys of a detached window.** At the very top of `Shell::run` (`shell/sessions.rs`), before the reconnect check:

```rust
        if let Some(key) = self.detached_key(window) {
            return self.run_detached(key, *command, window, cx);
        }
```

Replace the placeholder arm from Task 3 with:

```rust
            Command::MoveTabToNewWindow => {
                if let Some((project, tab_id)) = self.tab_scope(window) {
                    let tab = DockTab::Terminal { project, tab_id };
                    self.move_tab_to_new_window(tab, None, window, cx);
                }
            }
```

`run_detached` (in `detached_actions.rs`):

```rust
    /// The keys of a detached window: only [`detached::handles`]' commands act, on this
    /// window's tab; the rest do nothing here (spec §4.2).
    pub(crate) fn run_detached(
        &mut self,
        key: WindowKey,
        command: Command,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if !detached::handles(command) || (self.reconnecting() && !self.modal_open()) {
            return;
        }
        let menu_was_open = self.menu_open();
        self.dismiss_menus(window, cx);
        self.finish_rename(true, window, cx);
        if command == Command::CommandPalette && self.palette.is_some() {
            return self.close_palette(window, cx);
        }
        if self.modal_open() {
            cx.propagate();
            return;
        }
        // The composer keeps its keys (Ctrl+Shift+Enter would read as Ctrl+Enter: send).
        if self.composer_focus.is_focused(window) && self.composer_session(window).is_some() {
            if command != Command::OpenComposer {
                cx.propagate();
            }
            return;
        }
        let Some((project, tab_id)) = self.tab_scope(window) else {
            return;
        };
        let tab = DockTab::Terminal { project, tab_id };
        match command {
            Command::NextTab => self.detached.cycle(key, true),
            Command::PreviousTab => self.detached.cycle(key, false),
            Command::Tab(n) => {
                self.detached.select(key, usize::from(n) - 1);
            }
            Command::MoveTabLeft | Command::MoveTabRight => {
                let tabs = self.detached.tabs_of(key);
                if let Some(at) = tabs.iter().position(|t| *t == tab) {
                    let to = if command == Command::MoveTabLeft {
                        at.checked_sub(1)
                    } else {
                        (at + 1 < tabs.len()).then_some(at + 1)
                    };
                    if let Some(to) = to {
                        self.detached.move_within(key, at, to);
                    }
                }
            }
            Command::CloseTab => self.close_tabs(project, vec![tab_id], window, cx),
            Command::SplitRight => self.split(Axis::Vertical, window, cx),
            Command::SplitDown => self.split(Axis::Horizontal, window, cx),
            Command::SplitAgent => self.new_pane(SessionKind::Agent, Axis::Vertical, window, cx),
            Command::SplitShell => self.new_pane(SessionKind::Shell, Axis::Vertical, window, cx),
            Command::Layout(preset) => self.apply_layout(preset, window, cx),
            Command::EqualizePanes | Command::Resize(_) => {
                let write = match command {
                    Command::Resize(direction) => {
                        self.with_tab(window, |tabs| tabs.resize_focused(direction))
                    }
                    _ => self.with_tab(window, Tabs::equalize_active),
                };
                if let Some(write) = write.flatten() {
                    self.save_layout(project, write, window, cx);
                }
            }
            Command::ZoomPane => {
                self.with_tab(window, Tabs::toggle_zoom);
            }
            Command::Focus(direction) => {
                self.with_tab(window, |tabs| tabs.focus_neighbour(direction));
            }
            Command::ClosePane => self.close_focused_pane(window, cx),
            Command::RenamePane => {
                if let Some(session) = self.focused_session(window) {
                    self.start_pane_rename(session, window, cx);
                }
            }
            Command::RenameTab => self.start_rename(window, cx),
            Command::OpenComposer => self.open_composer(window, cx),
            Command::CommandPalette => self.open_palette(window, cx),
            Command::ContextMenu => {
                if !menu_was_open && let Some(session) = self.focused_session(window) {
                    let at = self.menu_anchor(&Anchor::PaneHeader(session), window, cx);
                    self.open_pane_menu(session, at, window, cx);
                }
            }
            Command::ToggleFullScreen => window.toggle_fullscreen(),
            Command::MoveTabToNewWindow => {
                // Already alone: nothing to do.
                if self.detached.tabs_of(key).len() > 1 {
                    self.move_tab_to_new_window(tab, None, window, cx);
                }
            }
            _ => {}
        }
        self.windows_dirty = true;
        self.sync_panes(window, cx);
        cx.notify();
    }
```

`open_composer(window, cx)`: it already takes `window`; with Task 6 it uses `focused_agent(window)`.

- [ ] **Step 5: Popovers per window.**
- `begin_menu` (menu_render.rs:403): add `self.menu_host = self.host_of(window);` after `self.close_menus();`.
- `open_palette` (navigation.rs:376): set `self.palette_host = self.host_of(window);`; build `palette::Context` from the window's scope:
  `let workspace = match self.host_of(window) { HostId::Main => self.workspace, HostId::Detached(_) => self.tab_scope(window).map(|(project, _)| project) };`,
  `view: if self.detached_key(window).is_some() { View::Terminals } else { self.view() }`,
  `tabs`: in a detached window `self.detached.tabs_of(key).len()`,
  `active_tab`: from `self.tab_scope(window)`,
  `detached_window: self.detached_key(window).is_some()`.
  After `entries` are built, in a detached window keep only what acts there:

```rust
        if self.detached_key(window).is_some() {
            entries.retain(|entry| match &entry.action {
                palette::Action::Run(command) => detached::handles(*command),
                palette::Action::Extra(extra) => matches!(
                    extra,
                    Extra::TabMoveToMainWindow | Extra::WindowMoveAllBack | Extra::TabPin
                ),
                palette::Action::GoTo(palette::Target::Session(_)) => true,
                _ => false,
            });
        }
```

- `run_palette_pick` (navigation.rs:633): before `let workspace = self.workspace;`, handle the window extras (replacing the Task 3 placeholder arm, which is deleted):

```rust
        if let Some(key) = self.detached_key(window) {
            match action {
                palette::Action::Extra(Extra::TabMoveToMainWindow) => {
                    if let Some((project, tab_id)) = self.tab_scope(window) {
                        let tab = DockTab::Terminal { project, tab_id };
                        self.move_tab_to(tab, HostId::Main, None, window, cx);
                    }
                    return;
                }
                palette::Action::Extra(Extra::WindowMoveAllBack) => {
                    return self.move_all_back(key, window, cx);
                }
                palette::Action::Extra(Extra::TabPin) => {
                    if let Some((project, tab_id)) = self.tab_scope(window) {
                        let pinned = (self.tabs.get(&project))
                            .and_then(|tabs| tabs.tab(tab_id))
                            .is_some_and(|tab| tab.pinned);
                        self.pin_tab(tab_id, !pinned, window, cx);
                    }
                    return;
                }
                _ => {}
            }
        }
```

- `Render for Shell` (main.rs ~1103-1192): the `tab_menu`, `layout_menu`, `pane_menu`, `share_menu` filters get `&& self.menu_host == HostId::Main`; the palette `.children` gets `.filter(|_| self.palette_host == HostId::Main)`.
  The modal focus fix at the top of `render` (`if self.modal_open() && !window.has_active_prompt() && !self.dialog_focus.contains_focused(window, cx)`) runs only when `self.palette.is_none() || self.palette_host == HostId::Main`.
  Toasts render only when `self.activation.first() == Some(&HostId::Main)`.
- `open_terminal_link` (navigation.rs:285): a file link from a detached pane opens in the main window:

```rust
            OpenLink::File { path, line } => {
                let Some((project_id, target, _)) = self.link_scope(session) else {
                    return;
                };
                let (path, line) = (path.clone(), *line);
                if self.host_of(window) != HostId::Main {
                    return self.in_window(HostId::Main, cx, move |shell, window, cx| {
                        window.activate_window();
                        shell.open_file_at(project_id, target, path, line, window, cx);
                    });
                }
                self.open_file_at(project_id, target, path, line, window, cx);
            }
```

- [ ] **Step 6: Tab menu rows.** In `tab_menu_rows` (`shell/menus.rs:194`), when `self.menu_host` is `HostId::Detached(key)`, drop "Close others" and "Close tabs to the right" and add, before "Close":

```rust
            MenuItem::new(
                "Move to main window",
                act(move |shell, window, cx| {
                    shell.move_tab_to(tab, HostId::Main, None, window, cx);
                }),
            )
            .row(),
```

In both windows add "Move to new window" (keys `self.keymap.label(Command::MoveTabToNewWindow)`, disabled in a detached window holding one tab) and, with two or more windows, one row per other window "Move to window N" (N = 1-based position in `self.detached.windows()`, the main window first as "Move to main window" when in a detached window).
`tab` is `DockTab::Terminal { project, tab_id: menu.tab_id }` where `project` is `self.terminal_project(menu.tab_id)`; add to `hosts.rs`:

```rust
    /// The workspace of a loaded terminal tab.
    pub(crate) fn terminal_project(&self, tab_id: Ulid) -> Option<Ulid> {
        (self.tabs.iter()).find_map(|(project, tabs)| tabs.tab(tab_id).map(|_| *project))
    }
```

The "Close" row calls `shell.close_tab(project, tab_id, window, cx)`.

- [ ] **Step 7: `render_host`** (`shell/detached_render.rs`). It mirrors `Render for Shell` for one window:

```rust
//! What a detached window shows (ADR 0096): title bar, its tab strip, the active tab.

use crate::*;

impl Shell {
    pub(crate) fn render_host(
        &mut self,
        key: WindowKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme().clone();
        let now = status::now_ms();
        window.set_rem_size(px(cx.global::<theme::Fonts>().ui_size));
        cx.global::<MenuAnchors>().clear(window.window_handle().window_id());
        let host = HostId::Detached(key);
        let scope = self.tab_scope(window);
        let title = scope
            .and_then(|(project, tab_id)| {
                let tab = self.tabs.get(&project)?.tab(tab_id)?;
                let name = self.sidebar.projects().iter().find(|p| p.id == project)?.name.clone();
                let label = session_label(tab, self.sidebar.sessions()).unwrap_or(&tab.title);
                Some(format!("{label} - {name}"))
            })
            .unwrap_or_else(|| "Mkrew".to_owned());
        window.set_window_title(&title);
        let content = self.hosts.entry(host).or_default().content.clone();
        let body = match scope {
            Some((project, tab_id)) => {
                let tab = self.tabs.get(&project).and_then(|tabs| tabs.tab(tab_id));
                self.render_tab(
                    project,
                    tab,
                    now,
                    self.composer_focus.is_focused(window),
                    None,
                    &theme,
                    content,
                    cx,
                )
                .into_any_element()
            }
            None => message(&theme, None, "Loading…".into(), None).flex_1().into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .font_family(theme.ui_font_family.clone())
            .child(self.render_detached_title_bar(key, &title, &theme, window, cx))
            .children(self.render_reconnect_banner(&theme, cx))
            .child(self.render_detached_strip(key, now, &theme, cx))
            .child(body)
            .children(self.reconnecting().then(inert_layer))
            .children(self.detached_overlays(host, &theme, window, cx))
            .into_any_element()
    }
}
```

`detached_overlays` returns the menu overlays (`render_tab_menu`, `render_layout_menu`, `render_pane_menu`, `render_share_menu`, each only when `self.menu_host == host && !self.modal_open()`), the palette (when `self.palette_host == host`), and the toasts (when `self.activation.first() == Some(&host)`), in that order, as `Vec<gpui::AnyElement>`.

`render_detached_title_bar`: same structure as `render_title_bar` (`shell/chrome.rs:244-421`): `app_logo(theme)`, the title text (muted, truncated), a drag area (`window_control_area(WindowControlArea::Drag)`, macOS `start_window_move` as in chrome.rs:283-302), a "Move all back" `icon_button("move-all-back", Icon::Minimize2, theme)` with `tooltip(theme, "Move all tabs back to the main window", "")` calling `shell.move_all_back(key, window, cx)`, then the rule and the three `window_control`s when `DRAWN_CONTROLS && !window.is_fullscreen()` (make `DRAWN_CONTROLS` `pub(crate)`).
Height `theme::rems_px(theme::TITLE_BAR_HEIGHT)`, `bg(rgb(theme.chrome))`, bottom border `theme.divider`, as the main bar.

`render_detached_strip`: same chip as `render_tab_bar` (`pane_render.rs:86-183`), for `self.detached.tabs_of(key)`:
- label: `session_label(tab, sessions).unwrap_or(&tab.title)`, prefixed by the workspace name in `theme.text_muted` and `font_size::META` (spec: each chip shows its workspace);
- active = `self.detached.active(key)`;
- click: `shell.detached.select(key, index); shell.sync_panes(window, cx); cx.notify();` (double click: rename as in the main strip);
- right click: `shell.open_tab_menu(tab_id, event.position, window, cx)` (with Task 6 `open_tab_menu` leaves the main window's active tab alone when the tab is detached: replace its `active_tabs_mut().activate(tab_id)` by `if self.terminal_host(project, tab_id) == HostId::Main { ... }`);
- ✕: `shell.close_tab(project, tab_id, window, cx)`;
- reorder: `self.reorderable(element, DraggedItem { list: ReorderList::Detached(key), index, title }, true, space::XS, theme, cx)`; add the variant `Detached(WindowKey)` to `ReorderList` (`shell/reorder.rs`) and to `drop_reordered`: `ReorderList::Detached(key) => { self.detached.move_within(key, item.index, to); self.windows_dirty = true; }`;
- the strip and chip bounds are recorded into `self.hosts[host].strip` with a `canvas` like `record_bounds` (Task 9 reads them).
- right end: the layout button and ⋯ as in the main strip (`open_layout_menu` with Task 6 reads the window's tab: change its `self.workspace` lookup to `self.tab_scope(window)`).
- No `+` button: new tabs are created in the main window.

Main strip: `render_tab_bar` skips detached tabs: `.children(tabs.tabs().iter().enumerate().filter(|(_, tab)| self.terminal_host(project_id, tab.id) == HostId::Main).map(...))`; keep `place` from the unfiltered enumerate so `DraggedItem.index` stays the index in the pinned group (the hidden tabs keep their slot, which `drop_tab` expects).

- [ ] **Step 8: Build, test, smoke**

Run: `cargo build -p mkrew-ui`, `cargo test -p mkrew-ui`, `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`.
Smoke (record what you saw): Shift+F4 on a tab with a split, type in both panes, Ctrl+Shift+M, Alt+arrows, Alt+Shift+= split, Ctrl+Shift+W in the detached window (main tabs untouched), right-click the chip → Move to main window, palette in the detached window lists only its commands, Move all back button; dark and light.

- [ ] **Step 9: Commit**

```bash
git add crates/mkrew-ui/src/
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): terminal tabs in detached windows"
```

---

### Task 9: Drag a tab out and between windows

**Files:**
- Create: `crates/mkrew-ui/src/shell/tear_off.rs` (`mod tear_off;`)
- Modify: `shell/pane_render.rs` (`render_tab_bar`: record strip and chips; draw the drop hint), `shell/detached_render.rs` (same for the detached strip), `main.rs` (`Render for Shell`: the tear-off catcher), `shell/detached_render.rs` (catcher)

**Interfaces:**
- Consumes: `window_place::{drop_target, Placed, Strip, Target}` (Task 4), `move_tab_to`, `move_tab_to_new_window` (Task 8).
- Produces: `Shell.drop_hint: Option<(HostId, usize)>` field; `fn tear_off_catcher(cx: &Context<'_, Shell>) -> impl IntoElement`; `fn dragged_dock_tab(&self) -> Option<DockTab>`.

- [ ] **Step 1: Implement** `shell/tear_off.rs`:

```rust
//! A terminal tab released outside its window (ADR 0096). gpui-pre-windows keeps the mouse
//! captured from button down to up, so the source window gets the release wherever it is and
//! decides; other windows see no hover, so `Shell.drop_hint` paints their insertion line.

use crate::*;
use mkrew_ui::window_place::{self, Placed, Target};

impl Shell {
    /// The terminal tab a strip drag carries, from the reorder state its `on_drag` set.
    pub(crate) fn dragged_dock_tab(&self) -> Option<DockTab> {
        let reorder = self.reorder?;
        match reorder.list {
            ReorderList::Tabs(project, pinned) => {
                let tab = (self.tabs.get(&project)?.tabs().iter())
                    .filter(|tab| tab.pinned == pinned)
                    .nth(reorder.from)?;
                Some(DockTab::Terminal { project, tab_id: tab.id })
            }
            ReorderList::Detached(key) => self.detached.tabs_of(key).get(reorder.from).copied(),
            _ => None,
        }
    }

    /// Every Mkrew window with its screen bounds and strip, most recently activated first.
    fn placed_windows(&self) -> Vec<Placed<HostId>> {
        (self.activation.iter())
            .filter_map(|host| {
                let ui = self.hosts.get(host)?;
                Some(Placed {
                    host: *host,
                    window: ui.bounds,
                    strip: ui.strip.borrow().clone(),
                })
            })
            .collect()
    }

    fn release_target(&self, position: Point<Pixels>, window: &Window) -> Option<Target<HostId>> {
        let source = self.host_of(window);
        let screen = point(
            window.bounds().origin.x + position.x,
            window.bounds().origin.y + position.y,
        );
        window_place::drop_target(screen, source, &self.placed_windows())
    }

    fn tear_off_move(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<'_, Self>) {
        let hint = (self.dragged_dock_tab().is_some())
            .then(|| self.release_target(position, window))
            .flatten()
            .and_then(|target| match target {
                Target::Strip { host, index } => Some((host, index)),
                _ => None,
            });
        if self.drop_hint != hint {
            self.drop_hint = hint;
            cx.notify();
        }
    }

    fn tear_off_release(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<'_, Self>) -> bool {
        self.drop_hint = None;
        let Some(tab) = self.dragged_dock_tab() else {
            return false;
        };
        let Some(target) = self.release_target(position, window) else {
            return false;
        };
        self.reorder = None;
        let screen = point(
            window.bounds().origin.x + position.x,
            window.bounds().origin.y + position.y,
        );
        match target {
            Target::Strip { host, index } => self.move_tab_to(tab, host, Some(index), window, cx),
            Target::Window(host) => self.move_tab_to(tab, host, None, window, cx),
            Target::NewWindow(_) => self.move_tab_to_new_window(tab, Some(screen), window, cx),
        }
        true
    }
}

/// Watches the whole window during a tab drag (capture phase, before every element): pointer
/// moves update the drop hint; a release outside the window moves the tab.
pub(crate) fn tear_off_catcher(cx: &Context<'_, Shell>) -> impl IntoElement + use<> {
    let view = cx.entity();
    canvas(
        |_, _, _| (),
        move |_, (), window, _| {
            let moving = view.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture && cx.has_active_drag() {
                    moving.update(cx, |shell, cx| shell.tear_off_move(event.position, window, cx));
                }
            });
            let releasing = view.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture || !cx.has_active_drag() {
                    return;
                }
                let moved = releasing.update(cx, |shell, cx| {
                    shell.tear_off_release(event.position, window, cx)
                });
                if moved {
                    cx.stop_propagation();
                }
            });
        },
    )
    .absolute()
    .size_full()
}
```

Inside the source window (`drop_target` returns `None`) nothing changes: the existing `on_drop` handlers run.
`dragged_dock_tab` only reads the reorder state of tab strips: sidebar rows and file tabs never tear off.

- [ ] **Step 2: Wire it.**
- Field `drop_hint: Option<(HostId, usize)>` on `Shell` (init `None`).
- `Render for Shell`: `.child(tear_off_catcher(cx))` as the first child of the root `div` (it is absolute and paints nothing).
- `render_host`: the same as the first child of its root.
- `render_tab_bar` and `render_detached_strip`: record `Strip { bounds, chips }` window-local into `self.hosts[host].strip`:
  the strip `div` gets `.child(record_strip(ui.strip.clone(), None))` and each chip `.child(record_strip(ui.strip.clone(), Some(index)))` where:

```rust
/// Records the strip (`None`) or chip `index`'s bounds, window-local, at prepaint.
pub(crate) fn record_strip(
    strip: Rc<RefCell<Option<mkrew_ui::window_place::Strip>>>,
    chip: Option<usize>,
) -> impl IntoElement {
    canvas(
        move |bounds, _, _| {
            let mut strip = strip.borrow_mut();
            let recorded = strip.get_or_insert_with(|| mkrew_ui::window_place::Strip {
                bounds,
                chips: Vec::new(),
            });
            match chip {
                None => {
                    recorded.bounds = bounds;
                    recorded.chips.clear();
                }
                Some(index) => {
                    recorded.chips.truncate(index);
                    recorded.chips.push(bounds);
                }
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}
```

  (The strip records before its chips: prepaint goes parent first.)
- Drop hint: in both strips, when `self.drop_hint == Some((host, index))`, draw the same accent line as `reorderable` (`theme::DROP_LINE_WIDTH`, `theme.accent`) left of chip `index` (right of the last chip when `index == len`).
- The main strip's chip `index` for `Target::Strip` is the visible chip index; `move_tab_to(.., HostId::Main, Some(index), ..)` puts the tab active; to also place it, after the move translate the visible index to `Tabs::move_tab(tab_id, to)` within its pinned group and send `Request::TabMove(params)` like `drop_tab` (`shell/reorder.rs:789-807`). Implement that in `move_tab_to` for `to == HostId::Main` with `index: Some(_)`.

- [ ] **Step 3: Build, test, smoke**

Run: `cargo test -p mkrew-ui`, `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`.
Smoke: drag a main tab outside the window (new window at the pointer, same content size), drag a detached chip onto the main strip between two tabs (accent line shows in the main window while dragging; tab lands there), drag between two detached windows, reorder inside a detached strip, drag inside the main strip (unchanged reorder), drag a tab onto a pane (unchanged split), drag a file tab outside (nothing happens).

- [ ] **Step 4: Commit**

```bash
git add crates/mkrew-ui/src/
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): drag terminal tabs out of and between windows"
```

---

### Task 10: Close, quit, tray, reconnect, save and restore

**Files:**
- Modify: `shell/detached_actions.rs` (`detached_should_close`), `shell/files.rs` (`window_should_close`, `close_when_saved`), `shell/attention.rs` (`quit_ui`, `restore_window`), `desktop.rs`, `desktop/win32.rs`, `desktop/none.rs`, `main.rs` (startup: keep the window handle, `on_window_closed`), `shell/connection.rs` (status tick save; `refresh` reload; startup restore), `shell/workspace.rs` (`tabs_loaded`, `tabs_load_failed`: restore progress), `shell/hosts.rs` (save/restore)

**Interfaces:**
- Produces: `Shell::detached_should_close(key, window, cx) -> bool`, `Shell::save_windows(cx)`, `Shell::start_restore(window, cx)`, `Shell::restore_progress(project, window, cx)`, `desktop::set_window_shown(window: &gpui::Window, shown: bool)`.

- [ ] **Step 1: Closing a detached window closes its tabs.**

```rust
    /// The ✕ of a detached window (spec §5.4): its tabs close for real (one question for the
    /// running agents, ADR 0041). The window goes once its last tab did (`prune_detached`).
    pub(crate) fn detached_should_close(
        &mut self,
        key: WindowKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let mut by_project: HashMap<Ulid, Vec<Ulid>> = HashMap::new();
        for DockTab::Terminal { project, tab_id } in self.detached.tabs_of(key) {
            by_project.entry(project).or_default().push(tab_id);
        }
        for (project, tab_ids) in by_project {
            self.close_tabs(project, tab_ids, window, cx);
        }
        false
    }
```

`close_tabs` asks once per call; with tabs of two workspaces in one window the question can come twice.
Make `close_tabs` take `Vec<(Ulid, Ulid)>` (project, tab) instead, asking once for all of them, and loop over the pairs in its continuation (each pair uses its own project for `tabs.remove` and the empty-workspace `create_tab`).
Update its callers from Task 6 to pass pairs.

- [ ] **Step 2: Quit and the main window's ✕.**
- `main.rs`: keep the handle: `let main = match opened { Ok(main) => main, Err(error) => { ...; return; } };` and replace `on_window_closed`:

```rust
            // Sessions live in the daemon, so closing the main window ends the app, detached
            // windows included (they come back on the next start, ADR 0096).
            let main_id = main.window_id();
            cx.on_window_closed(move |cx, closed| {
                if closed == main_id || cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
```

- `window_should_close` (files.rs:604) and `close_when_saved`: call `self.save_windows(cx);` right before `self.close_machines();` in both.
- `quit_ui` (attention.rs:84): `self.save_windows(cx);` before `self.close_machines();`.
- `hide_on_close` branch of `window_should_close`: after `self.desktop.hide_window();` hide the detached windows:

```rust
            for &handle in self.windows.keys() {
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |_, window, _| desktop::set_window_shown(window, false));
                });
            }
```

- `restore_window` (attention.rs:48): after `self.desktop.show_window();` the same loop with `true`.
- `desktop/win32.rs`, a free function next to `Os`:

```rust
/// Shows or hides any Mkrew window (detached windows follow the main one to the tray).
pub fn set_window_shown(window: &gpui::Window, shown: bool) {
    if let Ok(RawWindowHandle::Win32(handle)) =
        HasWindowHandle::window_handle(window).map(|handle| handle.as_raw())
    {
        let hwnd = HWND(handle.hwnd.get() as *mut c_void);
        // SAFETY: `hwnd` is the live window gpui just handed us.
        unsafe {
            let _ = ShowWindow(hwnd, if shown { SW_SHOW } else { SW_HIDE });
        }
    }
}
```

  `desktop/none.rs`: `pub fn set_window_shown(_window: &gpui::Window, _shown: bool) {}`; `desktop.rs`: `pub use os::set_window_shown;`.

- [ ] **Step 3: Save.** In `shell/hosts.rs`:

```rust
    /// Writes `windows.json` (spec §6.1); a failure is a toast.
    pub(crate) fn save_windows(&mut self, cx: &mut Context<'_, Self>) {
        self.windows_dirty = false;
        let windows: Vec<detached::SavedWindow> = (self.detached.windows().iter())
            .filter_map(|window| {
                let ui = self.hosts.get(&HostId::Detached(window.key))?;
                let bounds = ui.restore;
                Some(detached::SavedWindow {
                    bounds: detached::SavedBounds {
                        x: f32::from(bounds.origin.x),
                        y: f32::from(bounds.origin.y),
                        width: f32::from(bounds.size.width),
                        height: f32::from(bounds.size.height),
                    },
                    maximized: ui.maximized,
                    tabs: window.tabs.clone(),
                    active: window.active,
                })
            })
            .collect();
        if let Err(error) = detached::save(&self.windows_path(), &windows) {
            tracing::warn!("cannot save the detached windows: {error}");
            self.toast_error(format!("Cannot save the detached windows: {error}"), cx);
        }
    }

    fn windows_path(&self) -> PathBuf {
        let instance = self.machine_name().map(|name| format!("machine:{name}"));
        self.paths.data_dir.join(detached::windows_file(instance.as_deref()))
    }
```

Status tick (connection.rs:122-176), inside `update_in` after the toast expiry: `if shell.windows_dirty { shell.save_windows(cx); }`.
Machine instance id: check `launch::instance_id` for the exact `machine:<name>` form used by `viewed_file` (connection.rs:244) and pass the same string.

- [ ] **Step 4: Restore at startup and after a reconnect.**

```rust
    /// Reads `windows.json` once the workspaces are listed: loads the tabs of every workspace
    /// a saved window shows; the windows open once all answered (`restore_progress`).
    pub(crate) fn start_restore(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let text = match std::fs::read_to_string(self.windows_path()) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => return tracing::warn!("cannot read the detached windows: {error}"),
        };
        let windows = match detached::from_json(&text) {
            Ok(windows) if !windows.is_empty() => windows,
            Ok(_) => return,
            Err(error) => return tracing::warn!("ignoring the detached windows file: {error}"),
        };
        let waiting: HashSet<Ulid> = (windows.iter())
            .flat_map(|w| w.tabs.iter().map(DockTab::project))
            .filter(|project| !self.tabs.contains_key(project))
            .collect();
        for &project in &waiting {
            self.load_tabs(project, window, cx);
        }
        self.restore = Some(Restore { windows, waiting });
        self.restore_progress(None, window, cx);
    }

    /// `project`'s tabs answered (or failed): once none is awaited, the saved windows open
    /// with the tabs that still exist.
    pub(crate) fn restore_progress(
        &mut self,
        project: Option<Ulid>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(restore) = self.restore.as_mut() else {
            return;
        };
        if let Some(project) = project {
            restore.waiting.remove(&project);
        }
        if !restore.waiting.is_empty() {
            return;
        }
        let Some(restore) = self.restore.take() else {
            return;
        };
        let displays: Vec<Bounds<Pixels>> = cx.displays().iter().map(|d| d.bounds()).collect();
        let primary = cx.primary_display().map(|d| d.bounds());
        for saved in restore.windows {
            let tabs: Vec<DockTab> = (saved.tabs.iter().copied())
                .filter(|tab| {
                    let DockTab::Terminal { project, tab_id } = *tab;
                    self.tabs.get(&project).is_some_and(|tabs| tabs.tab(tab_id).is_some())
                })
                .collect();
            let shown = saved.tabs.get(saved.active).copied();
            let Some(key) = self.detached.restore(&[detached::SavedWindow { tabs, ..saved.clone() }]).first().copied() else {
                continue;
            };
            if let Some(index) = shown.and_then(|shown| self.detached.tabs_of(key).iter().position(|t| *t == shown)) {
                self.detached.select(key, index);
            }
            let b = &saved.bounds;
            let bounds = Bounds::new(point(px(b.x), px(b.y)), size(px(b.width), px(b.height)));
            let bounds = mkrew_ui::window_place::fit_on_displays(bounds, &displays, primary);
            self.open_os_window(key, bounds, saved.maximized, window, cx);
        }
    }
```

Calls:
- `refresh` (connection.rs:835): after `shell.sidebar.load(projects, sessions);`, on the first successful listing of this process (`self.restored_once: bool` field, init `false`): `if !shell.restored_once { shell.restored_once = true; shell.start_restore(window, cx); }`.
  After a reconnect (`kept.is_some()`), reload the tabs of every workspace a detached window shows: `for project in shell.detached.windows().iter().flat_map(|w| w.tabs.iter().map(DockTab::project)).collect::<HashSet<_>>() { if !shell.tabs.contains_key(&project) { shell.load_tabs(project, window, cx); } }`.
- `tabs_loaded` (workspace.rs:168) and `tabs_load_failed` (workspace.rs:193, add `window`/`cx` parameters and update its callers): `self.restore_progress(Some(project_id), window, cx);`.
- `reset_connection_state`: keep `detached` (tabs come back with `tab.list`; `prune_detached` keeps a workspace's tabs while it is not loaded).

`load_tabs` of a workspace that has no tab creates one (`create_tab`): accepted, as showing that workspace would.

- [ ] **Step 5: Build, test, smoke**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`.
Smoke:
1. Detach two tabs into two windows (one maximized on the second display), quit with the tray menu Quit: no confirmation, `windows.json` has two windows; start again: both reopen at their places with their tabs and scrollback from the daemon snapshot.
2. Close a detached window with ✕ while an agent runs in it: one confirmation; Cancel keeps everything; Close removes the tabs and the window.
3. Main ✕ with close-to-tray on: every window hides; tray Open shows them all.
4. Stop the daemon (`mk daemon stop`) with a detached window open: banner + inert; start it: the tab comes back in the detached window.
5. Edit `windows.json` to a bounds `x: 9000`: the window opens centered on the primary display.
6. Break `windows.json` (`{`): the app starts normally, the log has the warning, the next detach rewrites the file.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): close, quit, tray and restore for detached windows"
```

---

### Task 11: Documents and full smoke

**Files:**
- Modify: `docs/SPEC.md` §10 (line 796-800: "One window." becomes the main window plus detached windows; new subsection "Detached windows" after the tab and pane rules) and §13 (the shortcuts table: `Shift+F4` Tab: Move to new window)
- Modify: `docs/HUMAN_CHECKS.md` (new entry at the top of `## To verify (non-blocking)`)
- Modify: `docs/PROGRESS.md` ("Notes for the next session": one bullet at the top)

- [ ] **Step 1: SPEC §10.** Replace "One window." with:

```markdown
The main window, plus any number of detached windows holding terminal tabs (ADR 0096).
```

and add the subsection (one sentence per line):

```markdown
### Detached windows (ADR 0096)

- A terminal tab moves into a window of its own by dragging its chip outside every Mkrew window (the new window opens at the pointer), with `Shift+F4`, or from the tab menu and the palette.
- A detached window has a title bar (the shown tab and its workspace, Move all back, the window controls), a tab strip with each tab's workspace, and the tab's panes.
- Tabs move between windows by dragging their chip onto another window (onto its strip: at the gap under the pointer) or with the tab menu's Move to main window / Move to window N.
- Its keys act on its own tab only: tab and pane commands of §13, the palette (its commands only), the context menu, the composer.
- Closing a detached window closes its tabs (one confirmation for running agents); quitting Mkrew keeps them and reopens the windows on the next start (`<data>/windows.json`).
- A hidden or minimized detached window detaches its panes like the main window.
- File tabs stay in the main window (until the Editor part of the spec ships).
```

- [ ] **Step 2: SPEC §13**: add the row `| Shift+F4 | Tab: Move to new window |` in the tabs group (same table format as its neighbours).

- [ ] **Step 3: HUMAN_CHECKS** entry:

```markdown
### To verify - 2026-11-10 Detached windows for terminal tabs (ADR 0096)

- [ ] **Tear off (dark + light):** drag a tab with a split out of the main window: a window opens at the pointer with the same size; both panes keep their scrollback and take keys.
- [ ] **Between windows:** drag the chip onto the main strip between two tabs (the accent line shows there while dragging), onto another detached window, and reorder inside a detached strip.
- [ ] **Keys stay home:** in a detached window, Ctrl+Shift+W, Alt+Shift+=, Ctrl+Shift+M, Alt+arrows, F2 act on that window's tab; the main window's tabs do not change.
- [ ] **Menus and palette:** right-click a detached chip: Move to main window, Move to new window, Move to window N; Ctrl+Shift+P there lists only the tab and pane commands.
- [ ] **Move all back:** the title bar button returns every tab in strip order and closes the window.
- [ ] **Close:** ✕ with a running agent asks once; Cancel keeps all; Close removes the tabs and the window.
- [ ] **Restart:** quit with two detached windows (one maximized on the second display) and start again: both come back; unplug the second display first: the window opens on the primary display.
- [ ] **Tray:** with close to tray on, ✕ on the main window hides every window; tray Open brings them all back.
- [ ] **Reconnect:** `mk daemon stop` then start: the detached window shows the banner, then its tab again.
- [ ] **Links:** Ctrl+click a file path in a detached pane: the main window comes forward with the file in the Editor.
```

- [ ] **Step 4: PROGRESS note** (top of "Notes for the next session"):

```markdown
- 2026-11-10 detached windows, Plan 1 (branch `feat/detached-windows`, plan docs/superpowers/plans/2026-11-10-detached-windows-plan-1.md, ADR 0096, no protocol/migration): `mkrew_ui::detached` (model, `handles`, `windows.json`), `window_place` (drop target, bounds), `Tabs::with_active` / `focus_pane` / `activate_skipping`, `shell/hosts.rs` (`host_of`, `tab_scope`, `with_tab`, `in_window`, save/restore), `shell/detached_view.rs`, `detached_render.rs`, `detached_actions.rs`, `tear_off.rs`; `focused_session` / `composer_session` / `focused_agent` take the window; menu anchors per window. Plan 2 (Editor file tabs) is written after this merges. Checks in HUMAN_CHECKS "2026-11-10 Detached windows".
```

- [ ] **Step 5: Full verification and smoke**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`.
Expected: green.
Run the app and walk every HUMAN_CHECKS line above on Windows, with screenshots of: a detached window (dark), the same (light), the drop hint on the main strip, the detached palette.
Fix anything that looks off before committing.

- [ ] **Step 6: Commit**

```bash
git add docs/SPEC.md docs/HUMAN_CHECKS.md docs/PROGRESS.md
git -c user.email=tungnv975@gmail.com commit -m "docs: detached windows for terminal tabs (ADR 0096)"
```
