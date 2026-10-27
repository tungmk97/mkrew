# Keyboard-only wave 4: single actions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every single action of wave 4 (the ten bound commands, the Jira keys, the sidebar filter keys and the new palette entries) can be done without the mouse by calling the same function the mouse handler calls.

**Architecture:** Each new `Command` is added in `keymap.rs` (variant, `all`, `name`, `default_keys`, `context`), titled and gated in `palette.rs`, and dispatched in `Shell::run` to an existing `Shell` method; the few that had no method (reading width, move tab, move workspace, copy last error) get one, and the click handler is changed to call it. Pure decisions (next thread, step index, chip step, Jira row walk, approve target, palette availability) live in pure modules with unit tests, like `confirm.rs`. Each new `palette::Extra` is a variant with a title, an `available` rule over `palette::Context`, and an arm in `Shell::run_palette_pick`; run commands are dynamic entries like `agent_entries` (index into the live `run::Summary`, like `Action::NewAgentPane(usize)`).

**Tech Stack:** Rust, GPUI (`gpui-pre` 0.3.7), existing `mkrew-ui` crate only. No protocol, storage or daemon change.

**Spec:** `docs/superpowers/specs/2026-10-27-keyboard-only-design.md` (section "Wave 4 - Single actions")

## Global Constraints

- Every color, spacing, radius, font size and duration comes from `crates/mkrew-ui/src/theme.rs` tokens; no literals in components (`.claude/skills/mkrew-ui-design/SKILL.md`).
- No `unwrap` / `expect` outside tests unless provably infallible (comment why).
- `cargo fmt --all`; `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Tests: pure logic gets unit tests (pattern: `crates/mkrew-ui/src/confirm.rs` tests, keymap tests); no test calls a real agent or network; do not write tests that pin wording, wiring or defaults.
- New default keys are unique across all commands (`Keymap::parse` ignores context; `every_default_parses_and_is_unique` covers it); no plain `Ctrl+<letter>` outside Files; no `Ctrl+Alt+<letter>` where a terminal has focus (the comment keys are `Docs || Git`, where no terminal has focus).
- Keys a text field also uses (`a`, `w`, `Alt+Enter`, `F8`, `Ctrl+Alt+R`, `Ctrl+Alt+Enter`) act only while the view's list / tree / diff has the keyboard; while a text field, a comment draft or the Files editor has it, the key stays with that field.
- Global keys that now reach no terminal: `Alt+0`, `Ctrl+Shift+PgUp`, `Ctrl+Shift+PgDn` (SPEC §13 note, Task 8).
- Tooltips and menus show the shortcut; the palette lists every new action.
- Fast loop: `cargo test -p mkrew-ui <filter>`; end of wave: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL); on OOM or linker errors add `-j 2`.
- Commits: Conventional Commits (`feat(ui): ...`), `git -c user.email=tungnv975@gmail.com commit`, stage only the task's files, never push.
- Docs: SPEC §13 and §10, `docs/decisions/0081-keyboard-only-operation.md` (wave 1 created it; this wave appends a section), `docs/HUMAN_CHECKS.md` "To verify" entry for dark and light. Never touch CHANGELOG.md.
- Markdown: one sentence per physical line; never the em dash character, use "-".
- Run these waves in order 1, 2, 3, 4 on master. This plan assumes wave 1 (`Shell::open_workspace_menu_at_switcher`, `menu_select_after`, `DEFAULT_SHELL_HEADING`) and wave 3 (`Shell::reveal_pick`, new `Command` variants) are merged.

## Cross-wave names this plan consumes (confirmed with the sibling plans)

- Wave 1: `Shell::open_workspace_menu(&mut self, project_id: Ulid, anchor: Point<Pixels>, by_keyboard: bool, window, cx)`, `Shell::open_workspace_menu_at_switcher(project_id, window, cx)` (opens it at the title bar switcher by keyboard), `Shell::menu_select_after(&mut self, heading: &'static str)` (selects the first row after the `MenuRow::Heading(heading)`), const `DEFAULT_SHELL_HEADING` in `shell/menus.rs` (value `"Default shell for new panes"`). `Shell::close_menus()` keeps its no-arg signature.
- Wave 3: `Shell::reveal_pick(&mut self, project_id: Ulid, context: CommentContext, cx)` in `shell/picking.rs`; it reads `comments.selection` at call time and may be called right after `pick_lines`. `Comments::pick` resets wave 3's `head` field, so reveal uses `selection.end`.
- Wave 3 appends `PickDown`, `PickUp` after `SendComments` and `CopyMode`, `LinkHints` after `SendSelection` in `Command::all()`; wave 4 appends its commands after the last non-`Card` entry the earlier waves left.

## File Structure

| File | Change |
|---|---|
| `crates/mkrew-ui/src/keymap.rs` | 10 new `Command` variants, contexts |
| `crates/mkrew-ui/src/palette.rs` | titles, `available`, `Context` fields, 22 `Extra`s, `Action::RunCommand`, `run_entries` |
| `crates/mkrew-ui/src/palette/tests.rs` | availability tests |
| `crates/mkrew-ui/src/sidebar.rs` (+ `sidebar/tests.rs`) | `Chip::step`, browse highlight, `set_expanded`, `step_index` |
| `crates/mkrew-ui/src/board.rs` (+ `board/tests.rs`) | `Board::approve_target` |
| `crates/mkrew-ui/src/jira_board.rs` | `Source::other`, `columns`, `select_step`, `JiraView.selected` |
| `crates/mkrew-ui/src/comments.rs` | thread stepping, reply / resolve targets, margin index |
| `crates/mkrew-ui/src/tabs.rs`, `files_view.rs` | `active_place` |
| `crates/mkrew-ui/src/toast.rs` | `last_error`, `clear` |
| `crates/mkrew-ui/src/shell/sessions.rs` | `run` arms and guards |
| `crates/mkrew-ui/src/shell/navigation.rs` | palette `Context`, picks |
| `crates/mkrew-ui/src/shell/tabs.rs`, `reorder.rs`, `pane_render.rs`, `comments.rs`, `jira.rs`, `board.rs`, `cards.rs`, `git_diff.rs`, `git_menus.rs`, `chrome.rs`, `toasts.rs`, `files.rs`, `sidebar.rs`, `peek.rs` | handlers and tooltips |
| `crates/mkrew-ui/src/main.rs`, `shell/connection.rs` | Board key context in Jira, `/` handler untouched, scroll handles |
| `docs/SPEC.md`, `docs/decisions/0081-keyboard-only-operation.md`, `docs/HUMAN_CHECKS.md` | docs |

## Review Focus

1. A key shared with a text field (`a` in the Jira filter or card follow-up, `w` in the Git commit box, `Alt+Enter` in the Files editor, `F8` / `Ctrl+Alt+R` in a comment draft) must type or stay with the field and never act: Task 2 adds the Jira field guard in `Shell::run`, Task 5 gates `OpenKept` on `files_focus`, Task 3 relies on the existing `comment_focus` guard; checked in the Task 9 smoke list.
2. `F8` with no threads, a `Loading` / `Failed` list, a selection that matches no thread, or threads without lines must do nothing and never panic: tests `step_thread_*` in Task 3.
3. Move tab left / right at the first / last tab, and across the pinned / unpinned border, must not move or wrap: tests `step_index_stops_at_the_ends` (Task 4) and `active_place_counts_inside_the_pin_group` (Task 4).
4. A palette pick of "Run: Stop <command>" after the command list changed (index past the end, or the row now stopped) must do nothing wrong: the handler resolves the row by index at pick time and ignores a missing row; test `run_entries_follow_each_commands_state` (Task 7).
5. Reply on a resolved thread must not open a draft, and Resolve on a reopened one must resolve: tests `reply_and_resolve_targets_follow_the_status` (Task 3).
6. `Tab` / `Shift+Tab` in the sidebar filter must wrap through the three chips and never type a tab: test `chips_step_and_wrap` (Task 1); `Left` / `Right` must keep moving the caret while the box holds text: Task 1 acts on them only when the box is empty.

---

### Task 1: `FocusSidebar` (Alt+0) and the sidebar filter keys

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs:21-168` (variant), `:200-293` (`all`), `:296-382` (`name`), `:391-495` (`default_keys`)
- Modify: `crates/mkrew-ui/src/palette.rs:195-289` (`title`), `:293-375` (`available`), `:21-54` (`Context` unchanged here)
- Modify: `crates/mkrew-ui/src/sidebar.rs:89-106` (`Chip`), `:324-340` (`Sidebar`), `:432-472`, `:770-788` (`targets`)
- Modify: `crates/mkrew-ui/src/shell/sessions.rs:140-143` (guard), `:281+` (arm)
- Modify: `crates/mkrew-ui/src/shell/peek.rs:14-59` (add `focus_sidebar` after `toggle_compact_sidebar`)
- Modify: `crates/mkrew-ui/src/shell/tabs.rs:508-560` (`filter_key_down`)
- Modify: `crates/mkrew-ui/src/shell/sidebar.rs:381-384`, `:409` (highlight gating)
- Test: `crates/mkrew-ui/src/sidebar/tests.rs`

**Interfaces:**
- Produces: `Command::FocusSidebar` (name `focus_sidebar`, default `alt-0`, no context); `Chip::step(self, delta: isize) -> Chip`; `Sidebar::browse_from(&mut self, rows: &[Row], session: Option<Ulid>, workspace: Option<Ulid>)`; `Sidebar::set_expanded(&mut self, project_id: Ulid, open: bool)`; `sidebar::step_index(len: usize, from: usize, by: isize) -> Option<usize>` (used by Task 4 and Task 6); `Shell::focus_sidebar(&mut self, window, cx)`.

Design notes: `/` keeps its behavior (empty box, Enter gives the keyboard back).
Alt+0 puts the filter in browse mode: with an empty box the highlight is on the active row, up / down walk every visible row (workspaces too, so a collapsed workspace can be reached), Enter opens the highlighted row.
Left / right collapse / expand the highlighted row's workspace only while the box is empty, because with text they move the caret.
The compact rail draws no filter, so Alt+0 widens it (saved like Ctrl+Alt+B); a hidden sidebar is shown first.

- [ ] **Step 1: Write the failing tests** - append to `crates/mkrew-ui/src/sidebar/tests.rs`:

```rust
#[test]
fn chips_step_and_wrap() {
    assert_eq!(Chip::All.step(1), Chip::Working);
    assert_eq!(Chip::NeedsYou.step(1), Chip::All);
    assert_eq!(Chip::All.step(-1), Chip::NeedsYou);
    assert_eq!(Chip::Working.step(-1), Chip::All);
}

#[test]
fn step_index_stops_at_the_ends() {
    assert_eq!(step_index(3, 1, 1), Some(2));
    assert_eq!(step_index(3, 1, -1), Some(0));
    assert_eq!(step_index(3, 2, 1), None);
    assert_eq!(step_index(3, 0, -1), None);
    assert_eq!(step_index(0, 0, 1), None);
}

#[test]
fn browsing_highlights_the_active_row_and_walks_every_row() {
    let (api, web) = (project("api"), project("web"));
    let a = session(&api, 1, SessionState::Idle);
    let w = session(&web, 1, SessionState::Idle);
    let mut sidebar = Sidebar::default();
    sidebar.load(vec![api.clone(), web.clone()], vec![a.clone(), w.clone()]);
    let viewed = Viewed::default();
    let rows = sidebar.view(&viewed).rows;
    assert_eq!(sidebar.highlighted(&rows), None, "an empty box not browsing");

    sidebar.browse_from(&rows, Some(w.id), None);
    assert_eq!(sidebar.highlighted(&rows), Some(Target::Session(w.id)));
    // Every row is a target while browsing: workspace, its session, next workspace, its session.
    sidebar.move_highlight(&rows, -1);
    assert_eq!(sidebar.highlighted(&rows), Some(Target::Workspace(web.id)));
    sidebar.move_highlight(&rows, -1);
    assert_eq!(sidebar.highlighted(&rows), Some(Target::Session(a.id)));

    // The active workspace when no session is focused.
    sidebar.browse_from(&rows, None, Some(api.id));
    assert_eq!(sidebar.highlighted(&rows), Some(Target::Workspace(api.id)));

    // Typing leaves browse mode: the filter rules apply again.
    sidebar.filter_mut().insert("zzz");
    assert_eq!(sidebar.highlighted(&sidebar.view(&viewed).rows), None);
}

#[test]
fn collapsing_and_expanding_a_workspace_by_id() {
    let api = project("api");
    let a = session(&api, 1, SessionState::Idle);
    let mut sidebar = Sidebar::default();
    sidebar.load(vec![api.clone()], vec![a.clone()]);
    let viewed = Viewed::default();
    assert_eq!(session_ids(&sidebar.view(&viewed).rows), [a.id]);
    sidebar.set_expanded(api.id, false);
    assert_eq!(session_ids(&sidebar.view(&viewed).rows), Vec::<Ulid>::new());
    sidebar.set_expanded(api.id, true);
    assert_eq!(session_ids(&sidebar.view(&viewed).rows), [a.id]);
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p mkrew-ui chips_step step_index browsing_highlights collapsing_and_expanding`
Expected: FAIL to compile (`Chip::step`, `step_index`, `browse_from`, `set_expanded` not defined). Run the four filters one `cargo test` per filter if the multi-filter form is rejected.

- [ ] **Step 3: Implement the pure part** in `crates/mkrew-ui/src/sidebar.rs`.

Below the existing `impl Chip` block add:

```rust
impl Chip {
    pub const ALL: [Chip; 3] = [Chip::All, Chip::Working, Chip::NeedsYou];

    /// The chip `delta` places along All / Working / Needs you, wrapping (Tab / Shift+Tab).
    pub fn step(self, delta: isize) -> Chip {
        let at = Self::ALL.iter().position(|chip| *chip == self).unwrap_or(0);
        // `ALL` has three entries, so the remainder is in range.
        let next = (at as isize + delta).rem_euclid(Self::ALL.len() as isize) as usize;
        Self::ALL[next]
    }
}
```

In `struct Sidebar` add after `highlight`:

```rust
    /// Alt+0 put the filter in browse mode: with an empty box the highlight shows (on every
    /// visible row, workspaces too). Typing or clearing the filter ends it.
    browse: bool,
```

Replace `filter_mut`, `highlighted`, `move_highlight` and `clear_filter` bodies:

```rust
    pub fn filter_mut(&mut self) -> &mut Input {
        // The text changes the matches: the highlight starts again at the first.
        self.highlight = 0;
        self.browse = false;
        &mut self.filter
    }

    /// What Enter in the filter box opens: the highlighted row (the first match until ↑/↓
    /// moves it). While browsing with an empty box any row can be highlighted; otherwise
    /// None while the box is empty.
    pub fn highlighted(&self, rows: &[Row]) -> Option<Target> {
        let every_row = self.filter.trim().is_empty() && self.browse;
        if self.filter.trim().is_empty() && !self.browse {
            return None;
        }
        let targets = targets(rows, every_row);
        targets
            .get(self.highlight.min(targets.len().saturating_sub(1)))
            .copied()
    }

    /// ↑ (`-1`) / ↓ (`1`) in the filter box: the highlight moves one target and stops at the
    /// ends (no wrap).
    pub fn move_highlight(&mut self, rows: &[Row], delta: isize) {
        let every_row = self.filter.trim().is_empty() && self.browse;
        let last = targets(rows, every_row).len().saturating_sub(1);
        self.highlight = self
            .highlight
            .min(last)
            .saturating_add_signed(delta)
            .min(last);
    }

    /// Alt+0: browse mode with the highlight on `session`'s row, else `workspace`'s, else the
    /// first row.
    pub fn browse_from(&mut self, rows: &[Row], session: Option<Ulid>, workspace: Option<Ulid>) {
        self.browse = true;
        let all = targets(rows, true);
        let at = |target: Target| all.iter().position(|t| *t == target);
        self.highlight = session
            .and_then(|id| at(Target::Session(id)))
            .or_else(|| workspace.and_then(|id| at(Target::Workspace(id))))
            .unwrap_or(0);
    }

    /// Opens or closes one workspace (← / → in the filter box).
    pub fn set_expanded(&mut self, project_id: Ulid, open: bool) {
        if open {
            self.expanded.insert(project_id);
        } else {
            self.expanded.remove(&project_id);
        }
    }
```

In `clear_filter` add `self.browse = false;`. Replace `targets` (bottom of the file):

```rust
/// The visible session rows in order; workspaces only when no session is listed. With
/// `every_row` (browsing) both kinds, in screen order.
fn targets(rows: &[Row], every_row: bool) -> Vec<Target> {
    if every_row {
        return rows
            .iter()
            .filter_map(|row| match row {
                Row::Session(row) => Some(Target::Session(row.session_id)),
                Row::Workspace(row) => Some(Target::Workspace(row.project_id)),
                _ => None,
            })
            .collect();
    }
    let sessions: Vec<Target> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Session(row) => Some(Target::Session(row.session_id)),
            _ => None,
        })
        .collect();
    if !sessions.is_empty() {
        return sessions;
    }
    rows.iter()
        .filter_map(|row| match row {
            Row::Workspace(row) => Some(Target::Workspace(row.project_id)),
            _ => None,
        })
        .collect()
}
```

Next to `drop_index` add:

```rust
/// The place `by` steps from `from` in a list of `len`, `None` past either end (no wrap).
/// Move tab and Move workspace use it; the result is the final index
/// (what [`drop_index`] gives a drag).
pub fn step_index(len: usize, from: usize, by: isize) -> Option<usize> {
    from.checked_add_signed(by).filter(|to| *to < len)
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p mkrew-ui sidebar`
Expected: PASS (new tests and the existing sidebar tests, including the filter highlight tests).

- [ ] **Step 5: Add the command** in `crates/mkrew-ui/src/keymap.rs`.

Variant (after the last variant the earlier waves left, before the closing brace):

```rust
    /// Show the sidebar if hidden (the compact rail widens: it has no filter) and give its
    /// filter the keyboard with the active row highlighted; again while it has the keyboard
    /// gives it back to the pane.
    FocusSidebar,
```

`all()`: add `Command::FocusSidebar,` after `Command::AskAi,` (or the last entry of the list before `all.extend(CardKey::ALL...)`). `name()`: `Command::FocusSidebar => "focus_sidebar".into(),`. `default_keys()`: `Command::FocusSidebar => "alt-0".into(),`. `context()` needs no change (global).

In `crates/mkrew-ui/src/palette.rs`: `title`: `Command::FocusSidebar => "Sidebar: Focus the filter".into(),`; `available`: add `Command::FocusSidebar` to the `=> true` group that holds `ToggleSidebar` (the handler does nothing without workspaces).

- [ ] **Step 6: Wire the handler.**

`crates/mkrew-ui/src/shell/peek.rs`, after `toggle_compact_sidebar`:

```rust
    /// Alt+0: the sidebar's filter takes the keyboard with the active row highlighted. A
    /// hidden sidebar is shown, and the compact rail (no filter box) widens first.
    pub(crate) fn focus_sidebar(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.sidebar.is_empty() {
            return;
        }
        if self.sidebar_layout.hidden() {
            self.toggle_sidebar(window, cx);
        }
        if self.sidebar_layout.compact() {
            self.toggle_compact_sidebar(window, cx);
        }
        let rows = self.sidebar.view(&self.viewed).rows;
        self.sidebar
            .browse_from(&rows, self.focused_session(), self.workspace);
        window.focus(&self.filter_focus, cx);
        cx.notify();
    }
```

`crates/mkrew-ui/src/shell/sessions.rs`: directly after the voice-input early return (before `self.close_menus()`) nothing changes; add this block right before the `// Same for the sidebar filter while it has the keyboard` comment (line ~213), so Alt+0 again leaves the filter:

```rust
        if *command == Command::FocusSidebar && self.filter_focus.is_focused(window) {
            return self.focus_pane(window, cx);
        }
```

and in the final `match *command`, next to `Command::ToggleCompactSidebar`:

```rust
            Command::FocusSidebar => self.focus_sidebar(window, cx),
```

Also add `Command::FocusSidebar` to nothing in the `view() != Terminals` list (it works in every view).

- [ ] **Step 7: Chips and ← / → in `filter_key_down`** (`crates/mkrew-ui/src/shell/tabs.rs`). Replace the first block (`if let step @ ("up" | "down") = ... { ... return; }`) with:

```rust
        match event.keystroke.key.as_str() {
            step @ ("up" | "down") => {
                let rows = self.sidebar.view(&self.viewed).rows;
                self.sidebar
                    .move_highlight(&rows, if step == "up" { -1 } else { 1 });
                cx.notify();
                return;
            }
            // Tab / Shift+Tab walk the All / Working / Needs you chips (never typed).
            "tab" => {
                let by = if event.keystroke.modifiers.shift { -1 } else { 1 };
                self.sidebar.set_chip(self.sidebar.chip().step(by));
                cx.notify();
                return;
            }
            // With an empty box ← / → have no caret to move: they fold / unfold the
            // highlighted row's workspace. With text they stay the caret's.
            side @ ("left" | "right")
                if self.sidebar.filter().is_empty()
                    && !self.sidebar.filtering()
                    && !event.keystroke.modifiers.modified() =>
            {
                let rows = self.sidebar.view(&self.viewed).rows;
                let project_id = match self.sidebar.highlighted(&rows) {
                    Some(sidebar::Target::Workspace(id)) => Some(id),
                    Some(sidebar::Target::Session(id)) => self.project_of(id),
                    None => None,
                };
                if let Some(project_id) = project_id {
                    self.sidebar.set_expanded(project_id, side == "right");
                    // The highlight stays on the workspace (its sessions may have gone).
                    let rows = self.sidebar.view(&self.viewed).rows;
                    self.sidebar.browse_from(&rows, None, Some(project_id));
                }
                cx.notify();
                return;
            }
            _ => {}
        }
```

(`Input` derefs to `str`, as `self.filter.trim()` in `Sidebar::filtering` shows, so `.is_empty()` is the text length check.)

- [ ] **Step 8: Highlight only while the filter is on screen** (`crates/mkrew-ui/src/shell/sidebar.rs`). Replace line ~382 and delete the later `let filter_focused = ...` at line ~409 (it moves up):

```rust
        let filter_focused = self.filter_focus.is_focused(window);
        // ↑/↓ in the filter box move it; Enter opens it (`filter_key_down`). Browse mode
        // (Alt+0) shows it only while the box has the keyboard.
        let highlighted = self
            .sidebar
            .highlighted(&view.rows)
            .filter(|_| filter_focused || self.sidebar.filtering());
```

- [ ] **Step 9: Build and run the fast tests**

Run: `cargo test -p mkrew-ui sidebar`
Run: `cargo build -p mkrew-ui`
Expected: PASS and a clean build.

- [ ] **Step 10: Commit**

```bash
cargo fmt --all
git add crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/sidebar.rs crates/mkrew-ui/src/sidebar/tests.rs crates/mkrew-ui/src/shell/sessions.rs crates/mkrew-ui/src/shell/peek.rs crates/mkrew-ui/src/shell/tabs.rs crates/mkrew-ui/src/shell/sidebar.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): focus the sidebar filter with Alt+0, Tab chips and fold with arrows"
```

---

### Task 2: `ApproveCard`, Jira keys and `ShowBoard` toggle

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs` (variant, `all`, `name`, `default_keys`, `context` line 504)
- Modify: `crates/mkrew-ui/src/palette.rs` (`Context.approve_card`, `title`, `available`)
- Modify: `crates/mkrew-ui/src/palette/tests.rs:7-27` (`context()` helper gets the new field)
- Modify: `crates/mkrew-ui/src/board.rs:526-535` (`approve_target`)
- Modify: `crates/mkrew-ui/src/jira_board.rs` (`Source::other`, `columns`, `select_step`, `JiraView.selected`)
- Modify: `crates/mkrew-ui/src/main.rs:741-744` (Board context also while Jira shows), `:508-511` (`jira_scroll` field)
- Modify: `crates/mkrew-ui/src/shell/connection.rs:262-263` (init `jira_scroll`)
- Modify: `crates/mkrew-ui/src/shell/sessions.rs:154-171` (Jira field guard), `:502-504` (arms)
- Modify: `crates/mkrew-ui/src/shell/jira.rs` (`jira_key`, `toggle_board_source`, selected look, `track_scroll`, `open_issue` selects)
- Modify: `crates/mkrew-ui/src/shell/cards.rs:6-23`, `:785-793` (Approve tooltip key)
- Modify: `crates/mkrew-ui/src/shell/board.rs:607-619` (`approve_keys`)
- Modify: `crates/mkrew-ui/src/shell/navigation.rs:343-388` (`approve_card`)
- Test: `crates/mkrew-ui/src/board/tests.rs`, `crates/mkrew-ui/src/jira_board.rs` tests module

**Interfaces:**
- Consumes: `Board::approve_moves`, `Shell::approve_task(task_id, window, cx)` (`shell/board.rs:22`, the Approve button's function), `Shell::set_board_source`, `Shell::open_issue`.
- Produces: `Command::ApproveCard` (`approve_card`, `a`, context `Board`); `Board::approve_target(&self, selected: Option<Ulid>, project_id: Ulid) -> Option<Ulid>`; `Source::other`; `jira_board::columns`, `jira_board::select_step`; `JiraView.selected: Option<String>`; `Shell::jira_key`, `Shell::toggle_board_source`.

Design notes: the Board key context (`main.rs:741`) is now also on while Jira shows, so the `Card(_)` keys reach `run`; Jira rows are selected by key, ↑/↓ walk a column, in Kanban ←/→ walk columns, in List ←/→ fold / unfold the row, Enter opens the issue (the click path `open_issue`), Shift+←/→ and `e` do nothing in Jira.
Because Jira has text fields (filter, keys), `run` hands Board-context keys to the focused field.
Kanban cards are not scrolled into view (the same known limit as Board cards, `shell/board.rs:373`); the List scrolls through `jira_scroll`.

- [ ] **Step 1: Write the failing tests.**

`crates/mkrew-ui/src/board/tests.rs` (helpers `task(project_id, title, status, position)` exist):

```rust
#[test]
fn approve_targets_only_a_selected_review_card_of_the_board() {
    let (mine, other) = (Ulid::new(), Ulid::new());
    let review = task(mine, "r", TaskStatus::Review, 1.0);
    let todo = task(mine, "t", TaskStatus::Todo, 2.0);
    let foreign = task(other, "f", TaskStatus::Review, 1.0);
    let mut board = Board::default();
    board.load(vec![review.clone(), todo.clone(), foreign.clone()]);
    assert_eq!(board.approve_target(Some(review.id), mine), Some(review.id));
    assert_eq!(board.approve_target(Some(todo.id), mine), None, "not in Review");
    assert_eq!(board.approve_target(Some(foreign.id), mine), None, "another workspace");
    assert_eq!(board.approve_target(None, mine), None);
    assert_eq!(board.approve_target(Some(Ulid::new()), mine), None, "unknown card");
}
```

Append to the `tests` module of `crates/mkrew-ui/src/jira_board.rs` (helpers `issue`, `issue_in` exist):

```rust
    #[test]
    fn the_board_source_toggles_both_ways() {
        assert_eq!(Source::Tasks.other(), Source::Jira);
        assert_eq!(Source::Jira.other(), Source::Tasks);
    }

    #[test]
    fn arrows_walk_the_list_and_the_kanban_columns() {
        let issues = [
            issue_in("A-1", "To Do"),
            issue_in("A-2", "To Do"),
            issue_in("A-3", "In Progress"),
        ];
        let none = HashSet::new();
        let list = columns(&issues, JiraLayout::List, "", &none);
        assert_eq!(list, [["A-1", "A-2", "A-3"]]);
        assert_eq!(select_step(&list, None, CardKey::Down), Some("A-1"));
        assert_eq!(select_step(&list, Some("A-1"), CardKey::Down), Some("A-2"));
        assert_eq!(select_step(&list, Some("A-1"), CardKey::Up), None, "no wrap");
        assert_eq!(select_step(&list, Some("A-3"), CardKey::Down), None);
        assert_eq!(select_step(&list, Some("A-1"), CardKey::Right), None, "one column");
        assert_eq!(select_step(&list, Some("A-1"), CardKey::Open), None);

        // Kanban: In Progress first, then To Do.
        let board = columns(&issues, JiraLayout::Kanban, "", &none);
        assert_eq!(board, [vec!["A-3"], vec!["A-1", "A-2"]]);
        assert_eq!(select_step(&board, None, CardKey::Down), Some("A-3"));
        assert_eq!(select_step(&board, Some("A-3"), CardKey::Right), Some("A-1"));
        // The same row of the next column, else its last card.
        assert_eq!(select_step(&board, Some("A-2"), CardKey::Left), Some("A-3"));
        assert_eq!(select_step(&board, Some("A-2"), CardKey::Right), None);
        // A selection that is gone starts again at the first card.
        assert_eq!(select_step(&board, Some("GONE"), CardKey::Down), Some("A-3"));
    }

    #[test]
    fn a_parent_outside_the_results_is_not_selectable() {
        let mut child = issue("A-2");
        child.parent = Some(crate::jira::Parent {
            key: "A-1".into(),
            summary: "epic".into(),
            kind: "Epic".into(),
        });
        let issues = [child];
        let list = columns(&issues, JiraLayout::List, "", &HashSet::new());
        assert_eq!(list, [["A-2"]], "the muted parent header row is skipped");
    }
```

(If the `parent` field type is not `crate::jira::Parent`, copy the construction used by the existing parent tests in this module; the rule is "the header row has `issue: None` and is not selectable".)

Imports for the tests module: `use mkrew_core::settings::JiraLayout; use crate::board::CardKey;` if not already present.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p mkrew-ui approve_targets the_board_source arrows_walk a_parent_outside`
Expected: FAIL to compile (`approve_target`, `Source::other`, `columns`, `select_step` not defined).

- [ ] **Step 3: Implement the pure parts.**

`crates/mkrew-ui/src/board.rs`, after `run_target`:

```rust
    /// The card `a` approves on `project_id`'s board: the selected one, if it is that board's
    /// and sits in Review (what its Approve button needs).
    pub fn approve_target(&self, selected: Option<Ulid>, project_id: Ulid) -> Option<Ulid> {
        selected.filter(|&id| {
            self.task(id).is_some_and(|task| {
                task.project_id == project_id && task::approve(task.status).is_some()
            })
        })
    }
```

`crates/mkrew-ui/src/jira_board.rs`: add imports `use mkrew_core::settings::JiraLayout;` and `use crate::board::CardKey;`; after `enum Source`:

```rust
impl Source {
    /// The other side (Show Board while the Board is shown switches them).
    pub fn other(self) -> Self {
        match self {
            Source::Tasks => Source::Jira,
            Source::Jira => Source::Tasks,
        }
    }
}
```

Add field to `JiraView`:

```rust
    /// The issue key the arrow keys selected (its row / card is ringed); a key no longer
    /// listed selects nothing.
    pub selected: Option<String>,
```

and, after `tree`:

```rust
/// The issue keys the arrows walk, one list per column: the List is one column (the rows that
/// are issues; a parent outside the results is only a header), the Kanban one per status.
pub fn columns<'a>(
    issues: &'a [Issue],
    layout: JiraLayout,
    filter: &str,
    collapsed: &HashSet<String>,
) -> Vec<Vec<&'a str>> {
    match layout {
        JiraLayout::List => vec![
            tree(issues, filter, collapsed)
                .into_iter()
                .filter(|row| row.issue.is_some())
                .map(|row| row.key)
                .collect(),
        ],
        JiraLayout::Kanban => sections(issues, filter)
            .into_iter()
            .map(|section| section.issues.into_iter().map(|i| i.key.as_str()).collect())
            .collect(),
    }
}

/// The issue an arrow key selects: ↑ / ↓ the neighbour in the column, ← / → the card at the
/// nearest row of the next column that has cards (Board::select_step). Nothing at an edge.
/// Without a (listed) selection any arrow selects the first issue. Other keys: `None`.
pub fn select_step<'a>(
    columns: &[Vec<&'a str>],
    selected: Option<&str>,
    key: CardKey,
) -> Option<&'a str> {
    let step: isize = match key {
        CardKey::Up | CardKey::Left => -1,
        CardKey::Down | CardKey::Right => 1,
        _ => return None,
    };
    let at = selected.and_then(|selected| {
        columns.iter().enumerate().find_map(|(col, keys)| {
            Some((col, keys.iter().position(|key| *key == selected)?))
        })
    });
    let Some((col, row)) = at else {
        return columns.iter().find_map(|keys| keys.first().copied());
    };
    if matches!(key, CardKey::Up | CardKey::Down) {
        return columns[col].get(row.checked_add_signed(step)?).copied();
    }
    let mut next = col;
    loop {
        next = next.checked_add_signed(step)?;
        let keys = columns.get(next)?;
        if let Some(last) = keys.len().checked_sub(1) {
            return Some(keys[row.min(last)]);
        }
    }
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p mkrew-ui approve_targets the_board_source arrows_walk a_parent_outside`
Expected: PASS.

- [ ] **Step 5: Add `ApproveCard`.**

`keymap.rs`: variant

```rust
    /// Board only: approve the selected Review card (its Approve button).
    ApproveCard,
```

`all()`: after `Command::FocusSidebar,`: `Command::ApproveCard,`. `name()`: `"approve_card"`. `default_keys()`: `"a"`. `context()`: change line 504 to `Command::RunTask | Command::ApproveCard | Command::Card(_) => Some(BOARD_CONTEXT),`.

`palette.rs`: field in `Context`:

```rust
    /// The selected board card is in Review (`a` approves it).
    pub approve_card: bool,
```

`title`: `Command::ApproveCard => "Task: Approve selected task".into(),`; `available`: `Command::ApproveCard => in_view(View::Board) && context.approve_card,`. In `palette/tests.rs` `context()` add `approve_card: false,`.

`shell/navigation.rs` in `open_palette`, in the `Context { ... }` literal:

```rust
            approve_card: workspace
                .is_some_and(|id| self.board.approve_target(self.selected_card, id).is_some()),
```

- [ ] **Step 6: Dispatch and the Jira guard** in `shell/sessions.rs`.

Add after the `docs_filter_focus` guard (line ~212):

```rust
        // Board keys while a Jira field has the keyboard: the field types them (the Board
        // context is on over the Jira list too, for its arrow keys).
        if self.jira_filter_focus.is_focused(window) || self.jira_keys_focus.is_focused(window) {
            if command.context() == Some(BOARD_CONTEXT) {
                cx.propagate();
                return;
            }
        }
```

Replace lines 502-504 (`RunTask if jira...`, `Card(_) if jira...`, `Card(key)`):

```rust
            Command::RunTask if self.jira_side_shown() => {}
            Command::ApproveCard if self.jira_side_shown() => {}
            Command::Card(key) if self.jira_side_shown() => self.jira_key(key, window, cx),
            Command::Card(key) => self.card_key(key, window, cx),
            Command::ApproveCard => {
                // Only bound while a board is shown; the selection may be another workspace's.
                let target = self
                    .workspace
                    .and_then(|id| self.board.approve_target(self.selected_card, id));
                if let Some(task_id) = target {
                    self.approve_task(task_id, window, cx);
                }
            }
            Command::ShowBoard if self.view() == View::Board && self.jira_on => {
                if let Some(project_id) = self.workspace {
                    self.toggle_board_source(project_id, window, cx);
                }
            }
```

and keep `Command::ShowBoard => self.show_view(View::Board, window, cx),` after it (the guarded arm must come first).

`main.rs:741-744` becomes:

```rust
            .when(self.view() == View::Board, |el| el.key_context(BOARD_CONTEXT))
```

- [ ] **Step 7: Jira handlers** in `crates/mkrew-ui/src/shell/jira.rs`.

Extend the import: `use mkrew_ui::jira_board::{Section, TreeRow, columns, is_in_progress, linked_task, section, select_step, sections, split_area, tree};` (keep the existing names, add `columns`, `select_step`). Add methods in `impl Shell`:

```rust
    /// Show Board again while the Board is shown (and Jira is set up): Tasks ↔ Jira, like the
    /// `Tasks | Jira` switch.
    pub(crate) fn toggle_board_source(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let source = (self.jira.get(&project_id)).map_or(Source::Tasks, |view| view.source);
        self.set_board_source(project_id, source.other(), window, cx);
    }

    /// A board key (`Command::Card`) while the Jira list shows: ↑ / ↓ walk the column, in the
    /// Kanban ← / → walk the columns, in the List ← / → fold / unfold the row, Enter opens the
    /// issue like a click. Shift+← / → and `e` do nothing here.
    // ponytail: Kanban cards are not scrolled into view (one scroll area per column); add a
    // per-column ScrollHandle if long columns need it. The List scrolls through `jira_scroll`.
    pub(crate) fn jira_key(
        &mut self,
        key: board::CardKey,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self
            .workspace
            .filter(|_| self.view() == View::Board && self.jira_side_shown())
        else {
            return;
        };
        let layout = self.jira_layout;
        let Some(view) = self.jira.get(&project_id) else {
            return;
        };
        let Load::Loaded(issues) = &view.load else {
            return;
        };
        let selected = view.selected.clone();
        let filter = view.filter.as_str();
        if key == board::CardKey::Open {
            let listed = |key: &String| issues.iter().any(|issue| &issue.key == key);
            if let Some(issue_key) = selected.filter(listed) {
                self.open_issue(project_id, issue_key, window, cx);
            }
            return;
        }
        let is_list = matches!(layout, JiraLayout::List);
        if is_list && matches!(key, board::CardKey::Left | board::CardKey::Right) {
            let fold = tree(issues, filter, &view.collapsed)
                .into_iter()
                .find(|row| Some(row.key) == selected.as_deref())
                // Left folds an open parent, Right unfolds a folded one.
                .filter(|row| row.has_children && row.collapsed == (key == board::CardKey::Right))
                .map(|row| row.key.to_owned());
            if let (Some(fold), Some(view)) = (fold, self.jira.get_mut(&project_id)) {
                view.toggle_collapsed(&fold);
                cx.notify();
            }
            return;
        }
        let cols = columns(issues, layout, filter, &view.collapsed);
        let Some(next) = select_step(&cols, selected.as_deref(), key).map(str::to_owned) else {
            return;
        };
        let at = is_list
            .then(|| tree(issues, filter, &view.collapsed).iter().position(|row| row.key == next))
            .flatten();
        if let Some(view) = self.jira.get_mut(&project_id) {
            view.selected = Some(next);
        }
        if let Some(at) = at {
            self.jira_scroll.scroll_to_item(at);
        }
        cx.notify();
    }
```

(Remove the `section` word from the import line above; it is a typo guard: the import keeps exactly `Section, TreeRow, columns, is_in_progress, linked_task, sections, select_step, split_area, tree`.)

In `open_issue`, first statement (a click selects the row like the keys do):

```rust
        self.jira.entry(project_id).or_default().selected = Some(key.clone());
```

Selected look. In `issue_row` (`jira.rs:737`) before `let row_hover`:

```rust
        let selected = self
            .jira
            .get(&project_id)
            .and_then(|view| view.selected.as_deref())
            == Some(row.key);
```

and on the row `div()` replace `.rounded(px(radius::MEDIUM))` with:

```rust
            .rounded(px(radius::MEDIUM))
            // Always bordered, so selecting does not shift the content.
            .border(px(theme::BORDER_WIDTH))
            .border_color(if selected {
                rgb(theme.pane_focus_border).into()
            } else {
                gpui::transparent_black()
            })
```

In `issue_card` (`jira.rs:833`) compute the same `selected` for `&issue.key` and change the always-bordered card's `.border_color(rgb(theme.surface_raised))` to `.border_color(rgb(if selected { theme.pane_focus_border } else { theme.surface_raised }))` and the hover to `.when(!selected, |el| el.hover(move |style| style.border_color(rgb(hover))))` (same pattern as `cards.rs:212-219`).

List scroll: on the `div().id("jira-issues")` add `.track_scroll(&self.jira_scroll)`. Add the field in `main.rs` next to `jira_filter_focus`:

```rust
    /// Keeps the selected Jira List row in view.
    jira_scroll: ScrollHandle,
```

and `jira_scroll: ScrollHandle::new(),` in `shell/connection.rs` next to `jira_filter_focus: cx.focus_handle(),`.

- [ ] **Step 8: Approve tooltip key.** `shell/cards.rs`: in `CardContext` add

```rust
    /// Shortcut label of Approve on the selected card.
    pub(crate) approve_keys: String,
```

in `shell/board.rs` literal (line ~616) add `approve_keys: self.keymap.label(Command::ApproveCard),`, and in `card_action` change the `Approve` arm's last tuple element from `String::new()` to `if selected { cards.approve_keys.clone() } else { String::new() }` (the key acts on the selected card only, like Run).

- [ ] **Step 9: Run and build**

Run: `cargo test -p mkrew-ui board jira_board palette`
Run: `cargo build -p mkrew-ui`
Expected: PASS; clean build. `clippy -p mkrew-ui -- -D warnings` clean.

- [ ] **Step 10: Commit**

```bash
cargo fmt --all
git add crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette/tests.rs crates/mkrew-ui/src/board.rs crates/mkrew-ui/src/board/tests.rs crates/mkrew-ui/src/jira_board.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/sessions.rs crates/mkrew-ui/src/shell/jira.rs crates/mkrew-ui/src/shell/cards.rs crates/mkrew-ui/src/shell/board.rs crates/mkrew-ui/src/shell/navigation.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): approve a card with A, Board keys on the Jira list, Show Board toggles Jira"
```

---

### Task 3: Comment threads by keyboard (`NextComment`, `PreviousComment`, `ReplyComment`, `ResolveComment`)

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs` (4 variants; `context()` line 506)
- Modify: `crates/mkrew-ui/src/palette.rs` (`title`, `available`)
- Modify: `crates/mkrew-ui/src/comments.rs:285-330` (pure API), tests module `:383+`
- Modify: `crates/mkrew-ui/src/main.rs` (`comments_scroll: ScrollHandle`), `shell/connection.rs` (init)
- Modify: `crates/mkrew-ui/src/shell/comments.rs:398-409` (`track_scroll`), `:144-157` (neighbours), new methods
- Modify: `crates/mkrew-ui/src/shell/sessions.rs` (arms next to `AddComment`, line ~481)

**Interfaces:**
- Consumes: `Shell::pick_lines`-style selection (`Comments::pick`), `Shell::start_reply(project_id, context, root, window, cx)` (the Reply button's function, `shell/comments.rs:144`), `Shell::set_comment_status(project_id, context, root, resolve, window, cx)` (the Resolve / Reopen button's function, `:266`), wave 3 `Shell::reveal_pick`.
- Produces: `Command::{NextComment, PreviousComment, ReplyComment, ResolveComment}` (`next_comment`, `previous_comment`, `reply_comment`, `resolve_comment`; `f8`, `shift-f8`, `ctrl-alt-r`, `ctrl-alt-enter`; context `Docs || Git`); `Comments::step_thread(&mut self, forward: bool) -> Option<CommentId>`, `Comments::reply_target(&self) -> Option<CommentId>`, `Comments::status_target(&self) -> Option<(CommentId, bool)>` (the bool is `resolve`), `Comments::margin_index(&self, root: CommentId) -> Option<usize>`.

Design notes: a thread is "selected" the way a mouse click on its anchor selects it, by `comments.selection == the thread's lines`, so Ctrl+Alt+M and Ctrl+Shift+L act on it afterwards.
Threads without lines (outdated ones that lost their anchor) are skipped by F8 because they cannot be selected.
`F8` and the others hand over to a focused comment draft or commit box through the existing guards in `Shell::run` (`comment_focus`, `git_commit_focus`).

- [ ] **Step 1: Write the failing tests** in the `tests` module of `comments.rs` (helper `comment(id, parent, lines, at)` exists):

```rust
    fn loaded(list: Vec<DocComment>) -> Comments {
        let mut comments = Comments::new(Default::default());
        comments.begin_list("PLAN.md");
        comments.listed("PLAN.md", Ok(list));
        comments
    }

    fn id(n: u128) -> CommentId {
        CommentId(Ulid(n))
    }

    #[test]
    fn step_thread_walks_the_anchored_threads_and_wraps() {
        let mut comments = loaded(vec![
            comment(1, None, Some((9, 9)), 1),
            comment(2, None, Some((3, 4)), 2),
            comment(3, None, None, 3),
        ]);
        // Nothing selected: F8 goes to the first, Shift+F8 to the last.
        assert_eq!(comments.step_thread(true), Some(id(2)));
        assert_eq!(comments.selection, Some(Selection { start: 3, end: 4 }));
        assert_eq!(comments.step_thread(true), Some(id(1)));
        assert_eq!(comments.step_thread(true), Some(id(2)), "wraps");
        assert_eq!(comments.step_thread(false), Some(id(1)), "wraps back");
        let mut fresh = loaded(vec![
            comment(1, None, Some((9, 9)), 1),
            comment(2, None, Some((3, 4)), 2),
        ]);
        assert_eq!(fresh.step_thread(false), Some(id(1)));
    }

    #[test]
    fn step_thread_does_nothing_without_anchored_threads() {
        assert_eq!(Comments::new(Default::default()).step_thread(true), None);
        let mut no_lines = loaded(vec![comment(1, None, None, 1)]);
        assert_eq!(no_lines.step_thread(true), None);
        assert_eq!(no_lines.selection, None);
        let mut other = loaded(vec![comment(1, None, Some((2, 2)), 1)]);
        other.pick(7, 8, false);
        // A selection that is no thread: F8 starts at the first thread.
        assert_eq!(other.step_thread(true), Some(id(1)));
    }

    #[test]
    fn reply_and_resolve_targets_follow_the_status() {
        let mut open = loaded(vec![comment(1, None, Some((2, 2)), 1)]);
        assert_eq!(open.reply_target(), None, "nothing selected");
        open.pick(2, 2, false);
        assert_eq!(open.reply_target(), Some(id(1)));
        assert_eq!(open.status_target(), Some((id(1), true)), "Open resolves");

        let mut resolved = comment(1, None, Some((2, 2)), 1);
        resolved.status = CommentStatus::Resolved;
        let mut done = loaded(vec![resolved]);
        done.pick(2, 2, false);
        assert_eq!(done.reply_target(), None, "no reply on a resolved thread");
        assert_eq!(done.status_target(), Some((id(1), false)), "Resolved reopens");
    }

    #[test]
    fn the_margin_index_counts_the_children_before_the_thread() {
        let mut comments = loaded(vec![
            comment(1, None, Some((9, 9)), 1),
            comment(2, None, Some((3, 4)), 2),
        ]);
        assert_eq!(comments.margin_index(id(2)), Some(0));
        assert_eq!(comments.margin_index(id(1)), Some(1));
        comments.error = Some("Cannot resolve".into());
        assert_eq!(comments.margin_index(id(2)), Some(1), "the error line comes first");
        comments.pick(3, 4, false);
        comments.start_quoted(Some("q".into()));
        assert_eq!(comments.margin_index(id(2)), Some(2), "so does the new-comment draft");
        assert_eq!(comments.margin_index(id(99)), None);
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p mkrew-ui step_thread reply_and_resolve margin_index`
Expected: FAIL to compile (methods not defined).

- [ ] **Step 3: Implement** in `impl Comments` (`comments.rs`, after `pick`):

```rust
    /// The threads of the loaded list that have lines (so they can be selected), in anchor
    /// order: root id, lines, status.
    fn anchored(&self) -> Vec<(CommentId, Selection, CommentStatus)> {
        let List::Loaded(list) = &self.list else {
            return Vec::new();
        };
        threads(list)
            .into_iter()
            .filter_map(|thread| {
                let (start, end) = thread.root.start_line.zip(thread.root.end_line)?;
                Some((thread.root.id, Selection { start, end }, thread.root.status))
            })
            .collect()
    }

    /// F8 (`forward`) / Shift+F8: selects the thread after / before the selected one (wrapping;
    /// the first / last when none is selected), exactly like clicking its anchor. `None`
    /// (nothing changes) without anchored threads.
    pub fn step_thread(&mut self, forward: bool) -> Option<CommentId> {
        let threads = self.anchored();
        let len = threads.len();
        if len == 0 {
            return None;
        }
        let at = self
            .selection
            .and_then(|picked| threads.iter().position(|(_, lines, _)| *lines == picked));
        let next = match (at, forward) {
            (Some(at), true) => (at + 1) % len,
            (Some(at), false) => (at + len - 1) % len,
            (None, true) => 0,
            (None, false) => len - 1,
        };
        let (root, lines, _) = threads[next];
        self.selection = Some(lines);
        Some(root)
    }

    /// The selected thread: the one anchored on exactly the selected lines.
    fn selected_thread(&self) -> Option<(CommentId, CommentStatus)> {
        let picked = self.selection?;
        self.anchored()
            .into_iter()
            .find(|(_, lines, _)| *lines == picked)
            .map(|(root, _, status)| (root, status))
    }

    /// Ctrl+Alt+R: the thread to reply to (the Reply button is hidden on a resolved one).
    pub fn reply_target(&self) -> Option<CommentId> {
        self.selected_thread()
            .filter(|(_, status)| *status != CommentStatus::Resolved)
            .map(|(root, _)| root)
    }

    /// Ctrl+Alt+Enter: the selected thread and whether the press resolves it (`false` = it
    /// reopens a resolved one), like its Resolve / Reopen button.
    pub fn status_target(&self) -> Option<(CommentId, bool)> {
        self.selected_thread()
            .map(|(root, status)| (root, !can_reopen(status)))
    }

    /// The place of `root`'s thread among the children of the margin: after the error line
    /// and the new-comment draft when those show. For scrolling it into view.
    pub fn margin_index(&self, root: CommentId) -> Option<usize> {
        let List::Loaded(list) = &self.list else {
            return None;
        };
        let at = threads(list).iter().position(|thread| thread.root.id == root)?;
        let draft = self
            .draft
            .as_ref()
            .is_some_and(|draft| matches!(draft.target, Target::New(_)));
        Some(at + usize::from(draft) + usize::from(self.error.is_some()))
    }
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p mkrew-ui comments`
Expected: PASS. (If `CommentStatus` is not in scope in the module, it is `mkrew_protocol::CommentStatus`, already used by `status_label`.)

- [ ] **Step 5: Add the four commands.** `keymap.rs` variants:

```rust
    /// Docs / Git: select the next / previous comment thread and scroll it into view.
    NextComment,
    PreviousComment,
    /// Docs / Git: open the reply field of the selected thread.
    ReplyComment,
    /// Docs / Git: resolve (or reopen) the selected thread.
    ResolveComment,
```

`all()` after `Command::ApproveCard,`: the four in that order. `name()`: `next_comment`, `previous_comment`, `reply_comment`, `resolve_comment`. `default_keys()`: `f8`, `shift-f8`, `ctrl-alt-r`, `ctrl-alt-enter`. `context()` line 506: `Command::AddComment | Command::SendComments | Command::NextComment | Command::PreviousComment | Command::ReplyComment | Command::ResolveComment => Some(COMMENTS_CONTEXT),`.

`palette.rs` `title`: `"Comments: Next comment thread"`, `"Comments: Previous comment thread"`, `"Comments: Reply to the selected thread"`, `"Comments: Resolve or reopen the selected thread"`. `available`: add all four to the arm `Command::AddComment | Command::SendComments => in_view(View::Docs) || in_view(View::Git)`.

- [ ] **Step 6: Shell methods** in `shell/comments.rs` (after `start_reply`):

```rust
    /// F8 / Shift+F8: the next / previous thread is selected (its lines picked, like a click on
    /// its anchor), scrolled into view in the doc or diff and in the margin.
    pub(crate) fn step_comment(
        &mut self,
        forward: bool,
        cx: &mut Context<'_, Self>,
    ) {
        let (Some(project_id), Some(context)) = (self.workspace, self.comment_context()) else {
            return;
        };
        let Some(comments) = self.comments_mut(project_id, context) else {
            return;
        };
        let Some(root) = comments.step_thread(forward) else {
            return;
        };
        let margin = comments.margin_index(root);
        if let Some(at) = margin {
            self.comments_scroll.scroll_to_item(at);
        }
        self.reveal_pick(project_id, context, cx);
        cx.notify();
    }

    /// Ctrl+Alt+R: the reply field of the selected thread (the Reply button).
    pub(crate) fn reply_selected_comment(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let (Some(project_id), Some(context)) = (self.workspace, self.comment_context()) else {
            return;
        };
        let root = self
            .comments_mut(project_id, context)
            .and_then(|comments| comments.reply_target());
        if let Some(root) = root {
            self.start_reply(project_id, context, root, window, cx);
        }
    }

    /// Ctrl+Alt+Enter: Resolve or Reopen on the selected thread (its button).
    pub(crate) fn resolve_selected_comment(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let (Some(project_id), Some(context)) = (self.workspace, self.comment_context()) else {
            return;
        };
        let target = self
            .comments_mut(project_id, context)
            .and_then(|comments| comments.status_target());
        if let Some((root, resolve)) = target {
            self.set_comment_status(project_id, context, root, resolve, window, cx);
        }
    }
```

Scroll handle: field in `main.rs` next to `settings_scroll`:

```rust
    /// Keeps the selected comment thread in view in the Docs / Git margin.
    comments_scroll: ScrollHandle,
```

`comments_scroll: ScrollHandle::new(),` in `shell/connection.rs`; on the margin div in `render_comments` (`shell/comments.rs:398-403`) add `.track_scroll(&self.comments_scroll)` after `.overflow_y_scroll()`.

- [ ] **Step 7: Dispatch** in `shell/sessions.rs`, after the `SendComments` arm:

```rust
            // Only bound while Docs are shown or the Git list has focus.
            Command::NextComment => self.step_comment(true, cx),
            Command::PreviousComment => self.step_comment(false, cx),
            Command::ReplyComment => self.reply_selected_comment(window, cx),
            Command::ResolveComment => self.resolve_selected_comment(window, cx),
```

(`run` already hands every command to a focused comment draft and to the Git commit box before this match, so the keys never act while typing.)

- [ ] **Step 8: Run and build**

Run: `cargo test -p mkrew-ui comments palette keymap`
Run: `cargo build -p mkrew-ui`
Expected: PASS; clean build.

- [ ] **Step 9: Commit**

```bash
cargo fmt --all
git add crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/comments.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/comments.rs crates/mkrew-ui/src/shell/sessions.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): next, previous, reply and resolve comment threads by keyboard"
```

---

### Task 4: `MoveTabLeft` / `MoveTabRight` (terminal and file tabs)

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs` (2 variants)
- Modify: `crates/mkrew-ui/src/palette.rs` (`title`, `available`)
- Modify: `crates/mkrew-ui/src/tabs.rs:303` (`active_place`), `crates/mkrew-ui/src/tabs/tests.rs`
- Modify: `crates/mkrew-ui/src/files_view.rs:604-620` (`active_place`), `crates/mkrew-ui/src/files_view/tests.rs`
- Modify: `crates/mkrew-ui/src/shell/reorder.rs:166-194` (`move_active_tab`)
- Modify: `crates/mkrew-ui/src/shell/sessions.rs:243-280` (arms)

**Interfaces:**
- Consumes: `sidebar::step_index` (Task 1), `Shell::drop_tab(project_id, pinned, from, to, window, cx)` (the drag drop's function, `shell/reorder.rs:168`), `FilesView::move_tab(pinned, from, to)`.
- Produces: `Command::{MoveTabLeft, MoveTabRight}` (`move_tab_left`, `move_tab_right`; `ctrl-shift-pageup`, `ctrl-shift-pagedown`; global); `Tabs::active_place(&self) -> Option<(bool, usize, usize)>` and `FilesView::active_place(&self)` (pinned, index inside its pin group, group length); `Shell::move_active_tab(&mut self, by: isize, window, cx)`.

Design notes: the move stays inside the tab's pin group, as the drag does (`Tabs::move_tab`, `FilesView::move_tab`).
Both end places are no-ops (no wrap).
In the Files view the command moves the active file tab; in Terminals the active terminal tab; in Board, Docs and Git it does nothing (the strip is not on screen).

- [ ] **Step 1: Write the failing tests.**

`tabs/tests.rs` (helpers `wire`, `pane`):

```rust
#[test]
fn active_place_counts_inside_the_pin_group() {
    let project = Ulid::new();
    let mut tabs = Tabs::new(project);
    let mut pinned = wire(project, "pin", 1.0, serde_json::Value::Null);
    pinned.pinned = true;
    let one = wire(project, "one", 2.0, serde_json::Value::Null);
    let two = wire(project, "two", 3.0, serde_json::Value::Null);
    tabs.load(vec![pinned.clone(), one.clone(), two.clone()]);
    assert_eq!(tabs.active_place(), Some((true, 0, 1)), "the pinned tab is first");
    tabs.activate_nth(2);
    assert_eq!(tabs.active_place(), Some((false, 1, 2)), "second of the unpinned two");
    assert_eq!(Tabs::new(project).active_place(), None, "no tab");
}
```

`files_view/tests.rs` (helpers `three`, `tab_paths`; opens with `Open::Kept`):

```rust
#[test]
fn a_file_tabs_place_is_counted_inside_its_pin_group() {
    let mut view = three();
    for path in ["README.md", "src/main.rs", "src/lib.rs"] {
        view.open(path, Open::Kept);
    }
    view.pin("README.md", true);
    // The active tab is the last opened: second of the two unpinned tabs.
    assert_eq!(view.active_place(), Some((false, 1, 2)));
    view.activate(0);
    assert_eq!(view.active_place(), Some((true, 0, 1)));
    assert_eq!(three().active_place(), None, "no tab");
}
```

(`step_index` is covered by `step_index_stops_at_the_ends`, Task 1.)

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p mkrew-ui active_place a_file_tabs_place`
Expected: FAIL to compile (`active_place` not defined).

- [ ] **Step 3: Implement.** `tabs.rs`, after `move_tab`:

```rust
    /// The active tab's pin group and place in it: (pinned, index among those tabs, how many
    /// there are). What Move tab left / right steps from.
    pub fn active_place(&self) -> Option<(bool, usize, usize)> {
        let active = self.active()?;
        let group: Vec<Ulid> = (self.tabs.iter())
            .filter(|tab| tab.pinned == active.pinned)
            .map(|tab| tab.id)
            .collect();
        let at = group.iter().position(|id| *id == active.id)?;
        Some((active.pinned, at, group.len()))
    }
```

`files_view.rs`, after `move_tab`:

```rust
    /// The active tab's pin group and place in it (see `Tabs::active_place`).
    pub fn active_place(&self) -> Option<(bool, usize, usize)> {
        let active = self.tab()?;
        let group: Vec<&str> = (self.tabs.iter())
            .filter(|tab| tab.pinned == active.pinned)
            .map(|tab| tab.path.as_str())
            .collect();
        let at = group.iter().position(|path| *path == active.path)?;
        Some((active.pinned, at, group.len()))
    }
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p mkrew-ui active_place a_file_tabs_place`
Expected: PASS.

- [ ] **Step 5: Command, handler, dispatch.**

`keymap.rs` variants:

```rust
    /// Move the active terminal tab (the active file tab in Files) one place left / right,
    /// within its pinned / unpinned group.
    MoveTabLeft,
    MoveTabRight,
```

`all()` after `Command::ResolveComment,`; names `move_tab_left` / `move_tab_right`; defaults `ctrl-shift-pageup` / `ctrl-shift-pagedown`; no context. `palette.rs` titles `"Tabs: Move tab left"`, `"Tabs: Move tab right"`; `available`: `Command::MoveTabLeft | Command::MoveTabRight => on_panes || in_view(View::Files) && context.tabs > 0` (written with parentheses: `on_panes || (in_view(View::Files) && context.tabs > 0)`; the palette `tabs` is the file tab count in Files, see `navigation.rs:369`).

`shell/reorder.rs`, after `drop_tab`:

```rust
    /// Ctrl+Shift+PgUp / PgDn (`by` -1 / 1): the active tab (the active file tab in the Files
    /// view) one place along its pin group, through the same functions a drag drop calls.
    pub(crate) fn move_active_tab(
        &mut self,
        by: isize,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self.workspace else {
            return;
        };
        if self.view() == View::Files {
            let view = self.files_view(project_id);
            let Some((pinned, from, len)) = view.active_place() else {
                return;
            };
            let Some(to) = sidebar::step_index(len, from, by) else {
                return;
            };
            view.move_tab(pinned, from, to);
            if let Some(at) = view.active_index() {
                self.file_tabs_scroll.scroll_to_item(at);
            }
            cx.notify();
            return;
        }
        let Some((pinned, from, len)) = self.tabs.get(&project_id).and_then(Tabs::active_place)
        else {
            return;
        };
        if let Some(to) = sidebar::step_index(len, from, by) {
            self.drop_tab(project_id, pinned, from, to, window, cx);
        }
    }
```

`shell/sessions.rs`: in the Files early match (line ~246) add

```rust
                Command::MoveTabLeft => return self.move_active_tab(-1, window, cx),
                Command::MoveTabRight => return self.move_active_tab(1, window, cx),
```

add `| Command::MoveTabLeft | Command::MoveTabRight` to the `// The rest act on panes that are not on screen` list (`RenameTab | RenamePane | ...`), and to the final match:

```rust
            Command::MoveTabLeft => self.move_active_tab(-1, window, cx),
            Command::MoveTabRight => self.move_active_tab(1, window, cx),
```

- [ ] **Step 6: Run and build**

Run: `cargo test -p mkrew-ui tabs files_view sidebar keymap`
Run: `cargo build -p mkrew-ui`
Expected: PASS; clean build.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all
git add crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/tabs.rs crates/mkrew-ui/src/tabs/tests.rs crates/mkrew-ui/src/files_view.rs crates/mkrew-ui/src/files_view/tests.rs crates/mkrew-ui/src/shell/reorder.rs crates/mkrew-ui/src/shell/sessions.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): move the active tab with Ctrl+Shift+PgUp and PgDn"
```

---

### Task 5: `GitToggleWrap` (W), `OpenKept` (Alt+Enter) and the tooltip fixes

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs` (2 variants; `context()` lines 507-525)
- Modify: `crates/mkrew-ui/src/palette.rs` (`title`, `available`)
- Modify: `crates/mkrew-ui/src/shell/sessions.rs` (arms)
- Modify: `crates/mkrew-ui/src/shell/git_diff.rs:353-368` (Wrap tooltip), `:583-640` (stage keys), `:788-820` (`hunk_stage_button`), `:851-915` (`span_row`), `:823-834` (`git_diff_preview_row`)
- Modify: `crates/mkrew-ui/src/shell/chrome.rs:322-326`, `:366`

**Interfaces:**
- Consumes: `Shell::set_diff_wrap(wrap, cx)` (the Wrap button's function, `shell/workspace_actions.rs:604`), `Shell::click_file(project_id, path, Open::Kept, window, cx)` (the double click's function), `FilesView::enter_target()`.
- Produces: `Command::GitToggleWrap` (`git_toggle_wrap`, `w`, context `Git`), `Command::OpenKept` (`open_kept`, `alt-enter`, context `Files`).

Design notes: `w` is bound in the `Git` context, which sits on the changed-file list only, so the commit box and the filter field type `w` (`run` also hands every command to the commit box).
`Alt+Enter` is bound in `Files`, which is the whole window while Files shows, so the arm acts only when the tree has the keyboard and otherwise lets the editor or the find fields keep it.
The hunk Stage tooltip shows `S` / `U` only on the current hunk, because those keys act on the current hunk, not the hovered one (the comment at `git_diff.rs:812`).

- [ ] **Step 1: Commands.** `keymap.rs` variants:

```rust
    /// Git only: wrap long diff lines or cut them at the edge (the Wrap button).
    GitToggleWrap,
    /// Files only: open the tree's cursor file in a kept tab (Enter opens a transient one).
    OpenKept,
```

`all()` after `Command::MoveTabRight,`; names `git_toggle_wrap`, `open_kept`; defaults `w`, `alt-enter`. `context()`: add `| Command::GitToggleWrap` to the `GIT_CONTEXT` list and `| Command::OpenKept` to the `FILES_CONTEXT` list.

`palette.rs`: titles `"Git: Toggle Wrap of the diff"`, `"Files: Open file in a kept tab"`; `available`: `Command::GitToggleWrap` joins the `in_view(View::Git)` group; `Command::OpenKept` joins the `in_view(View::Files)` group (`ToggleIgnored | SaveFile | ...`).

- [ ] **Step 2: Dispatch** in `shell/sessions.rs` final match (next to `GitToggleDiffMode`, and `SaveFile`):

```rust
            Command::GitToggleWrap => self.set_diff_wrap(!self.diff_wrap, cx),
            Command::OpenKept => {
                // Only the tree's cursor: in the editor or a find field Alt+Enter stays theirs.
                if !self.files_focus.is_focused(window) {
                    cx.propagate();
                    return;
                }
                if let Some(project_id) = self.workspace {
                    let cursor = self.files_view(project_id).enter_target();
                    if let Some(cursor) = cursor {
                        self.click_file(project_id, &cursor, Open::Kept, window, cx);
                    }
                }
            }
```

(`click_file` takes the keyboard for the tree and treats a folder like Enter does.)

- [ ] **Step 3: Tooltip fixes.**

Wrap button (`git_diff.rs:357-365`): the second `tooltip` argument `""` becomes `self.keymap.label(Command::GitToggleWrap)`.

Branch chip (`chrome.rs:366`): `.tooltip(tooltip(theme, &tip, self.keymap.label(Command::ShowGit)))`.

Workspace switcher (`chrome.rs:322-326`): the key text is the whole jump range:

```rust
            .tooltip(tooltip(
                theme,
                "Switch workspace",
                format!(
                    "{} … {}",
                    self.keymap.label(Command::Workspace(1)),
                    self.keymap.label(Command::Workspace(9))
                ),
            ))
```

Hunk Stage / Unstage (`git_diff.rs`): `hunk_stage_button` gets a `keys: SharedString` parameter before `theme` and uses `.tooltip(tooltip(theme, tip, keys))` (replace the `// No key` comment with `// The key acts on the current hunk only (n / p), so other hunks show none.`). `span_row` gets a `stage_keys: &str` parameter before `project_id` and passes `if is_current { stage_keys.to_owned().into() } else { SharedString::default() }` to `hunk_stage_button`. In `git_diff_lines` compute once before the `list(` call:

```rust
        let stage_keys = hunk_stage
            .map(|stage| match stage {
                git_view::Stage::Stage => self.keymap.label(Command::GitStage),
                git_view::Stage::Unstage => self.keymap.label(Command::GitUnstage),
            })
            .unwrap_or_default();
```

and in the row closure `span_row(&shown, at, current, hunk_stage, &stage_keys, project_id, theme, cx)` (move a clone of `stage_keys` into the closure like `row_theme`). `git_diff_preview_row` passes `""`.

- [ ] **Step 4: Build**

Run: `cargo test -p mkrew-ui keymap palette`
Run: `cargo build -p mkrew-ui`
Expected: PASS (`every_default_parses_and_is_unique` proves `w` and `alt-enter` are unique); clean build.

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
git add crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/shell/sessions.rs crates/mkrew-ui/src/shell/git_diff.rs crates/mkrew-ui/src/shell/chrome.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): toggle diff wrap with W, open a kept file tab with Alt+Enter, tooltip keys"
```

---

### Task 6: Palette entries for tabs, panes, workspaces and agents

**Files:**
- Modify: `crates/mkrew-ui/src/palette.rs:21-54` (`Context`), `:57-154` (`Extra`)
- Modify: `crates/mkrew-ui/src/palette/tests.rs:7-27`, `:375-397`, `:437-444`
- Modify: `crates/mkrew-ui/src/shell/navigation.rs:329-388` (`Context` fill), `:582-655` (picks)
- Modify: `crates/mkrew-ui/src/shell/pane_render.rs:686-695` (`toggle_reading_width`)
- Modify: `crates/mkrew-ui/src/shell/reorder.rs` (`move_workspace`)

**Interfaces:**
- Consumes: `Shell::pin_tab` (`shell/tabs.rs:373`), `close_other_tabs` (`:658`), `pin_session` (`:331`), `stop_session` (`shell/inspector.rs:453`), `open_transcript` (`:436`), `pick_plan` / `link_plan(session, None, ..)` (`shell/workspace_actions.rs:214`, `:255`), `enhance_composer` (`shell/composer.rs:135`), `drop_workspace` (private, same file as `move_workspace`), wave 1 `open_workspace_menu_at_switcher`, `menu_select_after`, `DEFAULT_SHELL_HEADING`, `sidebar::step_index`.
- Produces: `Extra::{TabPin, TabCloseOthers, TabCloseRight, PanePin, PaneReading, WorkspaceDefaultShell, WorkspaceMoveUp, WorkspaceMoveDown, AgentStop, AgentTranscript, AgentLinkPlan, AgentUnlinkPlan, AgentImprove}`; `Context` fields `tab_others`, `tab_right`, `pane`, `live_session`, `transcript`, `plan_linked`, `composer_open`; `Shell::toggle_reading_width(session, cx)`; `Shell::move_workspace(project_id, by, window, cx)`.

Available rules (spec table, with the refinements listed under the table):

| Extra | Title | Available when |
|---|---|---|
| `TabPin` | `Tab: Pin / Unpin` | Terminals view with a tab |
| `TabCloseOthers` | `Tab: Close others` | Terminals view and the active tab has other unpinned tabs |
| `TabCloseRight` | `Tab: Close tabs to the right` | Terminals view and the active tab has unpinned tabs to its right |
| `PanePin` | `Pane: Pin / Unpin` | Terminals view, a pane is focused |
| `PaneReading` | `Pane: Reading width (on / off)` | Terminals view, an agent pane is focused |
| `WorkspaceDefaultShell` | `Workspace: Set default shell…` | a workspace is active |
| `WorkspaceMoveUp` / `WorkspaceMoveDown` | `Workspace: Move up` / `Workspace: Move down` | a workspace is active |
| `AgentStop` | `Agent: Stop session…` | Terminals view, a live session is focused (`inspector::stop_action` is not `Nothing`) |
| `AgentTranscript` | `Agent: Open transcript` | Terminals view, an agent pane with a transcript is focused |
| `AgentLinkPlan` | `Agent: Link plan…` | Terminals view, an agent pane is focused (picks a file; "Change" when linked) |
| `AgentUnlinkPlan` | `Agent: Unlink plan` | as `AgentLinkPlan`, and the plan is linked (refinement: otherwise it would do nothing) |
| `AgentImprove` | `Agent: Improve prompt` | Terminals view, the focused pane's composer is open |

- [ ] **Step 1: Write the failing tests** in `palette/tests.rs`.

Add to the `context()` helper literal: `tab_others: false, tab_right: false, pane: false, live_session: false, transcript: false, plan_linked: false, composer_open: false,`.

Replace the `extras` helper so the always-available workspace and tab entries do not disturb the older assertions:

```rust
fn extras(context: &Context) -> Vec<Extra> {
    // Listed by the "workspace extras" test and the single-action test below.
    const SEPARATE: [Extra; 8] = [
        Extra::RunSetup,
        Extra::PhoneAccess,
        Extra::CheckUpdates,
        Extra::WorkspacePin,
        Extra::WorkspaceExplorer,
        Extra::WorkspaceDefaultAgent,
        Extra::WorkspaceRemove,
        Extra::WorkspaceMenu,
    ];
    const ALWAYS: [Extra; 4] = [
        Extra::TabPin,
        Extra::WorkspaceDefaultShell,
        Extra::WorkspaceMoveUp,
        Extra::WorkspaceMoveDown,
    ];
    entries(&Keymap::defaults(), context, &[], &[], &[])
        .into_iter()
        .filter_map(|entry| match entry.action {
            Action::Extra(extra) if !SEPARATE.contains(&extra) && !ALWAYS.contains(&extra) => {
                Some(extra)
            }
            _ => None,
        })
        .collect()
}
```

In `palette_only_actions_follow_the_view_and_state` the `agent_pane` expectation becomes:

```rust
    assert_eq!(
        extras(&agent_pane),
        [
            Extra::InsertPrompt,
            Extra::Turns,
            Extra::FollowInZed,
            Extra::PaneReading,
            Extra::AgentLinkPlan
        ]
    );
```

New test:

```rust
#[test]
fn tab_pane_workspace_and_agent_extras_follow_their_state() {
    let always = [
        Extra::TabPin,
        Extra::WorkspaceDefaultShell,
        Extra::WorkspaceMoveUp,
        Extra::WorkspaceMoveDown,
    ];
    let terminals = context(View::Terminals);
    for extra in always {
        assert!(extra.available(&terminals), "{extra:?}");
    }
    let none = Context {
        workspace: false,
        ..context(View::Terminals)
    };
    let no_tabs = Context {
        tabs: 0,
        ..context(View::Terminals)
    };
    for extra in always {
        assert!(!extra.available(&none), "{extra:?}");
    }
    assert!(!Extra::TabPin.available(&no_tabs), "no tab to pin");
    assert!(!Extra::TabPin.available(&context(View::Board)), "the strip is not on screen");
    assert!(Extra::WorkspaceMoveUp.available(&context(View::Board)), "any view");

    let gated = [
        (Extra::TabCloseOthers, Context { tab_others: true, ..terminals }),
        (Extra::TabCloseRight, Context { tab_right: true, ..terminals }),
        (Extra::PanePin, Context { pane: true, ..terminals }),
        (Extra::PaneReading, Context { agent_pane: true, ..terminals }),
        (Extra::AgentStop, Context { live_session: true, ..terminals }),
        (Extra::AgentTranscript, Context { transcript: true, ..terminals }),
        (Extra::AgentLinkPlan, Context { agent_pane: true, ..terminals }),
        (Extra::AgentUnlinkPlan, Context { agent_pane: true, plan_linked: true, ..terminals }),
        (Extra::AgentImprove, Context { composer_open: true, ..terminals }),
    ];
    for (extra, on) in gated {
        assert!(extra.available(&on), "{extra:?} when its state holds");
        assert!(!extra.available(&terminals), "{extra:?} without it");
        assert!(!extra.available(&Context { view: View::Board, ..on }), "{extra:?} on the Board");
    }
    let linked_shell = Context { plan_linked: true, ..terminals };
    assert!(!Extra::AgentUnlinkPlan.available(&linked_shell), "no agent pane");
}
```

(`Context` is `Copy`; struct update from a copy compiles.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p mkrew-ui tab_pane_workspace`
Expected: FAIL to compile (new variants and fields).

- [ ] **Step 3: Implement the palette side.** `Context` gets, after `run_live`:

```rust
    /// The active terminal tab has other unpinned tabs ("Close others").
    pub tab_others: bool,
    /// The active terminal tab has unpinned tabs to its right.
    pub tab_right: bool,
    /// A terminal pane is focused.
    pub pane: bool,
    /// The focused pane's session can be stopped (`inspector::stop_action`).
    pub live_session: bool,
    /// The focused agent pane's session has a transcript file.
    pub transcript: bool,
    /// The focused agent pane's session has a linked plan.
    pub plan_linked: bool,
    /// The focused pane's prompt composer is open.
    pub composer_open: bool,
```

Append to `enum Extra` (after `WorkspaceMenu`) the 13 variants in the table order, set `ALL: [Self; 31]` with them appended in the same order, and the titles from the table. `available` arms (add before the `Self::PhoneAccess | Self::CheckUpdates => true` arm):

```rust
            Self::TabPin => in_view(View::Terminals) && context.tabs > 0,
            Self::TabCloseOthers => in_view(View::Terminals) && context.tab_others,
            Self::TabCloseRight => in_view(View::Terminals) && context.tab_right,
            Self::PanePin => in_view(View::Terminals) && context.tabs > 0 && context.pane,
            Self::PaneReading | Self::AgentLinkPlan => {
                in_view(View::Terminals) && context.tabs > 0 && context.agent_pane
            }
            Self::WorkspaceDefaultShell | Self::WorkspaceMoveUp | Self::WorkspaceMoveDown => {
                context.workspace
            }
            Self::AgentStop => in_view(View::Terminals) && context.live_session,
            Self::AgentTranscript => in_view(View::Terminals) && context.transcript,
            Self::AgentUnlinkPlan => {
                in_view(View::Terminals) && context.agent_pane && context.plan_linked
            }
            Self::AgentImprove => in_view(View::Terminals) && context.composer_open,
```

(`PaneReading` and `AgentLinkPlan` share the arm; remove them from nowhere else. `Self::PaneReading | Self::AgentLinkPlan` must not also appear in the `InsertPrompt | FollowInZed` arm.)

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p mkrew-ui palette`
Expected: PASS (the shell does not compile yet; `cargo test -p mkrew-ui palette` builds the lib tests only. If the crate's test target needs the binary, finish Step 5 first and run again).

- [ ] **Step 5: Shell side.**

`shell/pane_render.rs`: add next to `reading_on`:

```rust
    /// Reading width on / off for `session`'s pane (its header button, and the palette).
    pub(crate) fn toggle_reading_width(&mut self, session: Ulid, cx: &mut Context<'_, Self>) {
        let on = (self.sidebar.sessions().iter())
            .find(|s| s.id == session)
            .is_some_and(|info| self.reading_on(info));
        self.reading_width.insert(session, !on);
        cx.notify();
    }
```

and replace the inline click body (`pane_render.rs:686-695`) with `.on_click(cx.listener(move |shell, _, _, cx| shell.toggle_reading_width(session, cx)))`.

`shell/reorder.rs`, after `drop_workspace`:

```rust
    /// Workspace: Move up / down (`by` -1 / 1): the active workspace one place along its pinned
    /// or unpinned group, the same reorder as the drag.
    pub(crate) fn move_workspace(
        &mut self,
        project_id: Ulid,
        by: isize,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(pinned) = (self.sidebar.projects().iter())
            .find(|p| p.id == project_id)
            .map(|p| p.pinned)
        else {
            return;
        };
        let order = self.sidebar.workspace_order(pinned);
        let Some(from) = order.iter().position(|id| *id == project_id) else {
            return;
        };
        if let Some(to) = sidebar::step_index(order.len(), from, by) {
            self.drop_workspace(pinned, from, to, window, cx);
        }
    }
```

`shell/navigation.rs` `open_palette`: before the `Context` literal add

```rust
        let focused = self.focused_session();
        let focused_info = focused
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id))
            .cloned();
        let active_tab = workspace
            .filter(|_| self.view() == View::Terminals)
            .and_then(|id| self.tabs.get(&id))
            .and_then(|tabs| Some((tabs, tabs.active()?.id)));
```

and in the literal:

```rust
            tab_others: active_tab
                .is_some_and(|(tabs, id)| !tabs.close_targets(id, false).is_empty()),
            tab_right: active_tab
                .is_some_and(|(tabs, id)| !tabs.close_targets(id, true).is_empty()),
            pane: focused.is_some(),
            live_session: focused_info.as_ref().is_some_and(|session| {
                inspector::stop_action(Some(session)) != inspector::StopAction::Nothing
            }),
            transcript: self.focused_agent().is_some_and(|id| {
                self.usages.get(id).and_then(|usage| usage.transcript.as_ref()).is_some()
            }),
            plan_linked: self.focused_agent().is_some_and(|id| {
                (self.sidebar.sessions().iter()).any(|s| s.id == id && s.plan_path.is_some())
            }),
            composer_open: focused.is_some_and(|id| self.composers.shown(id).is_some()),
```

(If `inspector` / `StopAction` are not in scope in `navigation.rs`, `use mkrew_ui::inspector;` is already imported by `shell/inspector.rs`, which uses `inspector::stop_action`; copy that import line.)

Picks: in the inner `match extra` of `run_palette_pick` add (the outer `palette::Action::Extra(extra)` arm already requires a workspace):

```rust
                    Extra::TabPin => {
                        let active = (self.tabs.get(&project_id))
                            .and_then(Tabs::active)
                            .map(|tab| (tab.id, tab.pinned));
                        if let Some((tab_id, pinned)) = active {
                            self.pin_tab(tab_id, !pinned, window, cx);
                        }
                    }
                    Extra::TabCloseOthers | Extra::TabCloseRight => {
                        let active = (self.tabs.get(&project_id))
                            .and_then(Tabs::active)
                            .map(|tab| tab.id);
                        if let Some(tab_id) = active {
                            let right_only = extra == Extra::TabCloseRight;
                            self.close_other_tabs(tab_id, right_only, window, cx);
                        }
                    }
                    Extra::PanePin => {
                        let pane = self.focused_session().and_then(|id| {
                            let session = self.sidebar.sessions().iter().find(|s| s.id == id)?;
                            Some((id, session.pinned))
                        });
                        if let Some((session, pinned)) = pane {
                            self.pin_session(session, !pinned, window, cx);
                        }
                    }
                    Extra::PaneReading => {
                        if let Some(session) = self.focused_agent() {
                            self.toggle_reading_width(session, cx);
                        }
                    }
                    // Opens the workspace menu at the switcher with the shell list selected.
                    Extra::WorkspaceDefaultShell => {
                        self.open_workspace_menu_at_switcher(project_id, window, cx);
                        self.menu_select_after(DEFAULT_SHELL_HEADING);
                    }
                    Extra::WorkspaceMoveUp => self.move_workspace(project_id, -1, window, cx),
                    Extra::WorkspaceMoveDown => self.move_workspace(project_id, 1, window, cx),
                    Extra::AgentStop => {
                        if let Some(session) = self.focused_session() {
                            self.stop_session(session, window, cx);
                        }
                    }
                    Extra::AgentTranscript => {
                        if let Some(session) = self.focused_agent() {
                            self.open_transcript(session, cx);
                        }
                    }
                    Extra::AgentLinkPlan => {
                        if let Some(session) = self.focused_agent() {
                            self.pick_plan(session, window, cx);
                        }
                    }
                    Extra::AgentUnlinkPlan => {
                        if let Some(session) = self.focused_agent() {
                            self.link_plan(session, None, window, cx);
                        }
                    }
                    Extra::AgentImprove => {
                        if let Some(session) = self.composer_session() {
                            self.enhance_composer(session, window, cx);
                        }
                    }
```

(`Tabs::active` returns `Option<&Tab>`; `Tab` has `id` and `pinned`. `pin_session` / `pin_tab` close their own menu and send the same requests the menu items send.)

- [ ] **Step 6: Run and build**

Run: `cargo test -p mkrew-ui palette`
Run: `cargo build -p mkrew-ui`
Expected: PASS; clean build; clippy `-D warnings` clean (`Extra` matches are exhaustive).

- [ ] **Step 7: Commit**

```bash
cargo fmt --all
git add crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette/tests.rs crates/mkrew-ui/src/shell/navigation.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/reorder.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): palette entries for tabs, panes, workspace order and agent actions"
```

---

### Task 7: Palette entries for Git, Jira, Run commands and global actions

**Files:**
- Modify: `crates/mkrew-ui/src/palette.rs` (`Context`, `Extra`, `Action::RunCommand`, `run_entries`)
- Modify: `crates/mkrew-ui/src/palette/tests.rs`
- Modify: `crates/mkrew-ui/src/toast.rs:30-95` (`last_error`, `clear`) and its tests
- Modify: `crates/mkrew-ui/src/shell/toasts.rs` (`copy_last_error`)
- Modify: `crates/mkrew-ui/src/shell/git_menus.rs` (`git_repos_listed`)
- Modify: `crates/mkrew-ui/src/shell/jira.rs:132-158` (`set_jira_mine`, `set_jira_layout` become `pub(crate)`)
- Modify: `crates/mkrew-ui/src/shell/navigation.rs` (`Context` fill, `run_entries`, picks)

**Interfaces:**
- Consumes: `Shell::open_git_base(project_id, window, cx)` (`shell/git_dialogs.rs:92`, the "Change…" button), `Shell::open_git_menu(GitMenu::Repos, window, cx)` (`shell/git_menus.rs:9`, the Repositories chevron), `Shell::set_jira_mine`, `Shell::set_jira_layout`, `Shell::load_jira` (Refresh), `Shell::show_history(project_id, window, cx)` (`shell/history.rs:234`), `Shell::start_update(window, cx)` (`shell/update.rs:98`), `Shell::run_action(project_id, action, Some(name), window, cx)` (`shell/run.rs:20`, the Run menu's row buttons), `run::Summary`.
- Produces: `Extra::{GitChangeBase, GitSwitchRepo, JiraMine, JiraLayout, JiraRefresh, CopyLastError, DismissToasts, History, InstallUpdate}`; `Action::RunCommand(RunAction, usize)`; `palette::run_entries(summary: &run::Summary) -> Vec<Entry>`; `Context` fields `git_base`, `git_repos`, `jira`, `last_error`, `toast`, `update_ready`; `Toasts::last_error(&self) -> Option<&str>`, `Toasts::clear(&mut self)`; `Shell::copy_last_error(cx)`, `Shell::git_repos_listed(project_id) -> bool`.

Available rules:

| Extra | Title | Available when |
|---|---|---|
| `GitChangeBase` | `Git: Change base…` | Git view showing "Against base" (where its button is) |
| `GitSwitchRepo` | `Git: Switch repository…` | Git view with several repositories (the repo strip shows) |
| `JiraMine` | `Jira: Toggle All / Mine` | the Board shows Jira |
| `JiraLayout` | `Jira: Toggle List / Kanban` | the Board shows Jira |
| `JiraRefresh` | `Jira: Refresh` | the Board shows Jira |
| `CopyLastError` | `Global: Copy last error` | an error toast was shown this run |
| `DismissToasts` | `Global: Dismiss notifications` | a toast is shown |
| `History` | `Global: History…` | a workspace is active (the title bar button needs one; spec said always) |
| `InstallUpdate` | `Global: Install update` | an update is ready and not already installing |

Run commands are not `Extra`s: one entry per run command in Run menu order, `Run: Start <name>` while it is stopped or exited, `Run: Restart <name>` and `Run: Stop <name>` while it runs.

- [ ] **Step 1: Write the failing tests.**

`toast.rs` tests module:

```rust
    #[test]
    fn the_last_error_outlives_its_toast() {
        let base = Instant::now();
        let mut toasts = Toasts::default();
        assert_eq!(toasts.last_error(), None);
        toasts.push(Kind::Info, "Copied", base);
        assert_eq!(toasts.last_error(), None, "an info toast is not an error");
        toasts.push(Kind::Error, "Cannot save the theme", base);
        toasts.push(Kind::Error, "Cannot push", base);
        assert_eq!(toasts.last_error(), Some("Cannot push"));
        toasts.expire(base + TOAST_DURATION);
        assert!(toasts.items().is_empty());
        assert_eq!(toasts.last_error(), Some("Cannot push"), "kept after it hid");
    }

    #[test]
    fn clear_drops_every_toast() {
        let base = Instant::now();
        let mut toasts = Toasts::default();
        toasts.push(Kind::Error, "a", base);
        toasts.push(Kind::Info, "b", base);
        toasts.clear();
        assert!(toasts.items().is_empty());
        assert_eq!(toasts.last_error(), Some("a"));
    }
```

`palette/tests.rs`: add to the `context()` helper `git_base: false, git_repos: false, jira: false, last_error: false, toast: false, update_ready: false,`; extend `SEPARATE`/`ALWAYS` in `extras()` with `Extra::History` (`ALWAYS` becomes `[Extra; 5]`) and `Extra::CopyLastError`, `Extra::DismissToasts`, `Extra::InstallUpdate` do not need it (default false). Add:

```rust
#[test]
fn git_jira_and_global_extras_follow_their_state() {
    let gated = [
        (Extra::GitChangeBase, Context { git_base: true, ..context(View::Git) }),
        (Extra::GitSwitchRepo, Context { git_repos: true, ..context(View::Git) }),
        (Extra::JiraMine, Context { jira: true, ..context(View::Board) }),
        (Extra::JiraLayout, Context { jira: true, ..context(View::Board) }),
        (Extra::JiraRefresh, Context { jira: true, ..context(View::Board) }),
        (Extra::CopyLastError, Context { last_error: true, ..context(View::Terminals) }),
        (Extra::DismissToasts, Context { toast: true, ..context(View::Terminals) }),
        (Extra::InstallUpdate, Context { update_ready: true, ..context(View::Terminals) }),
    ];
    for (extra, on) in gated {
        assert!(extra.available(&on), "{extra:?} when its state holds");
        assert!(!extra.available(&context(on.view)), "{extra:?} without it");
    }
    // The Git entries need the Git view, the Jira ones the Board.
    let wrong_view = Context { git_base: true, git_repos: true, ..context(View::Board) };
    assert!(!Extra::GitChangeBase.available(&wrong_view));
    assert!(!Extra::GitSwitchRepo.available(&wrong_view));
    assert!(!Extra::JiraRefresh.available(&Context { jira: true, ..context(View::Git) }));
    // The update and the toasts need no workspace.
    let none = Context { workspace: false, update_ready: true, toast: true, ..context(View::Board) };
    assert!(Extra::InstallUpdate.available(&none) && Extra::DismissToasts.available(&none));
    // History is the title bar button: it needs a workspace.
    assert!(Extra::History.available(&context(View::Board)));
    assert!(!Extra::History.available(&Context { workspace: false, ..context(View::Board) }));
}

#[test]
fn run_entries_follow_each_commands_state() {
    use crate::run::{Row, RowState, RunAction, Summary};
    let row = |name: &str, state| Row {
        name: name.into(),
        state,
        session_id: None,
        ports: Vec::new(),
    };
    let summary = Summary {
        rows: vec![
            row("api", RowState::Running(SessionState::Working)),
            row("web", RowState::Stopped),
            row("db", RowState::Exited),
        ],
        stray: 0,
    };
    let listed: Vec<(String, Action)> = run_entries(&summary)
        .into_iter()
        .map(|entry| (entry.title, entry.action))
        .collect();
    assert_eq!(
        listed,
        [
            ("Run: Restart api".to_owned(), Action::RunCommand(RunAction::Restart, 0)),
            ("Run: Stop api".to_owned(), Action::RunCommand(RunAction::Stop, 0)),
            ("Run: Start web".to_owned(), Action::RunCommand(RunAction::Start, 1)),
            ("Run: Start db".to_owned(), Action::RunCommand(RunAction::Start, 2)),
        ]
    );
    assert!(run_entries(&Summary::default()).is_empty());
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p mkrew-ui the_last_error clear_drops git_jira_and_global run_entries_follow`
Expected: FAIL to compile.

- [ ] **Step 3: Implement the pure parts.**

`toast.rs`: field `last_error: Option<String>` in `Toasts`; at the top of `push` (before the `expires` line):

```rust
        if kind == Kind::Error {
            self.last_error = Some(text.clone());
        }
```

and

```rust
    /// The text of the newest error toast shown this run, kept after the toast hid ("Copy last
    /// error").
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// "Dismiss notifications": every toast goes.
    pub fn clear(&mut self) {
        self.items.clear();
    }
```

(`Toasts` derives `Default`, so the new field defaults to `None`.)

`palette.rs`: `Context` gets `git_base`, `git_repos`, `jira`, `last_error`, `toast`, `update_ready` (all `bool`, with the doc comments "Git view shows Against base", "Git view lists several repositories", "The Board shows Jira", "An error toast was shown this run", "A toast is shown", "An update is ready to install"). Append to `Extra` (after `AgentImprove`) `GitChangeBase, GitSwitchRepo, JiraMine, JiraLayout, JiraRefresh, CopyLastError, DismissToasts, History, InstallUpdate`, `ALL: [Self; 40]` with them appended, titles from the table, and `available` arms:

```rust
            Self::GitChangeBase => in_view(View::Git) && context.git_base,
            Self::GitSwitchRepo => in_view(View::Git) && context.git_repos,
            Self::JiraMine | Self::JiraLayout | Self::JiraRefresh => {
                in_view(View::Board) && context.jira
            }
            Self::CopyLastError => context.last_error,
            Self::DismissToasts => context.toast,
            Self::History => context.workspace,
            Self::InstallUpdate => context.update_ready,
```

`Action` gets `RunCommand(crate::run::RunAction, usize),` with the doc "Run: Start / Restart / Stop `<command>`: the `usize`-th command of the active workspace's Run summary (like `NewAgentPane`)." and, after `agent_entries`:

```rust
/// "Run: Start / Restart / Stop <command>", one group per command of the workspace in Run menu
/// order (a running command offers Restart and Stop, any other Start), like the Run menu's
/// row buttons. The `usize` is the row's place in `summary`, resolved again when picked.
pub fn run_entries(summary: &crate::run::Summary) -> Vec<Entry> {
    use crate::run::{RowState, RunAction};
    let mut entries = Vec::new();
    for (index, row) in summary.rows.iter().enumerate() {
        let actions: &[(RunAction, &str)] = if matches!(row.state, RowState::Running(_)) {
            &[(RunAction::Restart, "Restart"), (RunAction::Stop, "Stop")]
        } else {
            &[(RunAction::Start, "Start")]
        };
        for &(action, verb) in actions {
            entries.push(Entry {
                action: Action::RunCommand(action, index),
                title: format!("Run: {verb} {}", row.name),
                shortcut: String::new(),
            });
        }
    }
    entries
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p mkrew-ui toast palette`
Expected: PASS (finish Step 5 first if the crate must compile whole).

- [ ] **Step 5: Shell side.**

`shell/toasts.rs`, after `toast_info`:

```rust
    /// "Global: Copy last error": the newest error toast's full text, even after it hid.
    pub(crate) fn copy_last_error(&mut self, cx: &mut Context<'_, Self>) {
        if let Some(text) = self.toasts.last_error().map(str::to_owned) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.toast_info("Copied", cx);
        }
    }
```

`shell/git_menus.rs`, after `close_git_menu`:

```rust
    /// The repository tabs show for `project_id` (two or more repositories, no task worktree):
    /// what "Switch repository…" needs.
    pub(crate) fn git_repos_listed(&self, project_id: Ulid) -> bool {
        self.git
            .get(&project_id)
            .is_some_and(|view| view.worktree_task_id.is_none())
            && self.git_repos.get(&project_id).is_some_and(|repos| {
                !git_view::repo_choices(repos, self.workspace_name(project_id)).is_empty()
            })
    }
```

`shell/jira.rs`: change `fn set_jira_mine` and `fn set_jira_layout` to `pub(crate) fn`.

`shell/navigation.rs` `open_palette` literal:

```rust
            git_base: workspace
                .and_then(|id| self.git.get(&id))
                .is_some_and(|view| view.section() == git_view::Section::Base),
            git_repos: workspace.is_some_and(|id| self.git_repos_listed(id)),
            jira: self.jira_side_shown(),
            last_error: self.toasts.last_error().is_some(),
            toast: !self.toasts.items().is_empty(),
            update_ready: self.update.is_some() && !self.updating,
```

After `entries.extend(palette::agent_entries(...))`:

```rust
        if workspace.is_some() {
            entries.extend(palette::run_entries(&run_state));
        }
```

Picks. Before the generic `palette::Action::Extra(extra)` arm (next to `PhoneAccess`, `CheckUpdates`; they need no workspace):

```rust
            palette::Action::Extra(Extra::CopyLastError) => self.copy_last_error(cx),
            palette::Action::Extra(Extra::DismissToasts) => {
                self.toasts.clear();
                cx.notify();
            }
            palette::Action::Extra(Extra::InstallUpdate) => self.start_update(window, cx),
            palette::Action::RunCommand(action, index) => {
                let Some(project_id) = workspace else {
                    return;
                };
                // The row is resolved now: a command removed meanwhile is simply not found.
                let name = (self.run_summary(project_id).rows.get(index)).map(|row| row.name.clone());
                if let Some(name) = name {
                    self.run_action(project_id, action, Some(name), window, cx);
                }
            }
```

and in the inner `match extra` add:

```rust
                    Extra::GitChangeBase => self.open_git_base(project_id, window, cx),
                    Extra::GitSwitchRepo => self.open_git_menu(GitMenu::Repos, window, cx),
                    Extra::JiraMine => {
                        self.set_jira_mine(project_id, !self.jira_mine, window, cx);
                    }
                    Extra::JiraLayout => {
                        let other = match self.jira_layout {
                            JiraLayout::List => JiraLayout::Kanban,
                            JiraLayout::Kanban => JiraLayout::List,
                        };
                        self.set_jira_layout(other, cx);
                    }
                    Extra::JiraRefresh => self.load_jira(project_id, window, cx),
                    Extra::History => self.show_history(project_id, window, cx),
                    // Handled above: they need no workspace.
                    Extra::CopyLastError | Extra::DismissToasts | Extra::InstallUpdate => {}
```

(`JiraLayout` is `mkrew_core::settings::JiraLayout`; add `use mkrew_core::settings::JiraLayout;` to `navigation.rs` if `use crate::*` does not bring it in. The inner `match` already needs the `PhoneAccess | CheckUpdates` handled-above arm; keep it.)

- [ ] **Step 6: Run and build**

Run: `cargo test -p mkrew-ui toast palette`
Run: `cargo build -p mkrew-ui`
Expected: PASS; clean build; clippy `-D warnings` clean.

- [ ] **Step 7: Commit**

```bash
cargo fmt --all
git add crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette/tests.rs crates/mkrew-ui/src/toast.rs crates/mkrew-ui/src/shell/toasts.rs crates/mkrew-ui/src/shell/git_menus.rs crates/mkrew-ui/src/shell/jira.rs crates/mkrew-ui/src/shell/navigation.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): palette entries for Git base and repos, Jira, run commands and global actions"
```

---

### Task 8: SPEC, ADR 0081 and HUMAN_CHECKS

**Files:**
- Modify: `docs/SPEC.md:771-777` (§10 sidebar filter), `:866` (§10 Board), `:1295-1352` (§13 table and note)
- Modify: `docs/decisions/0081-keyboard-only-operation.md` (append a section)
- Modify: `docs/HUMAN_CHECKS.md` (new "To verify" section at the top of the list, line 24)

- [ ] **Step 1: SPEC §13 table.** Insert these rows (one row per line, same column format) at the places named, and keep the table sorted by group like the neighbours:

After the `| Global | Ask AI ...` row:

```
| Global | Focus the sidebar filter with the active row highlighted (shows the sidebar if hidden, widens the compact rail; again returns to the pane) | Alt+0 |
```

After the `| Tabs | Rename / close tab ...` row:

```
| Tabs | Move the active tab (the active file tab in Files) one place left / right inside its pinned or unpinned group | Ctrl+Shift+PgUp / Ctrl+Shift+PgDn |
```

After the Board `Enter / E` row:

```
| Board | Approve the selected Review card (also: the same ↑↓←→ and Enter move and open rows of the Jira list and Kanban; ←/→ fold a List row) | A |
| Board | Show Board while the Board is shown and Jira is set up: switch Tasks / Jira | Ctrl+Shift+2 |
```

After the Docs `Toggle preview / source` row:

```
| Docs / Git | Select the next / previous comment thread (scrolled into view) | F8 / Shift+F8 |
| Docs / Git | Reply to / resolve or reopen the selected thread | Ctrl+Alt+R / Ctrl+Alt+Enter |
```

After the Files `Close the active file tab` row:

```
| Files | Open the tree's cursor file in a kept tab (Enter opens a transient one) | Alt+Enter |
```

After the Git `Toggle Unified / Split diff` row:

```
| Git | Wrap long diff lines / cut them at the edge | W |
```

Replace the row `| Task / Workspace | Merge back ... | command palette only |` text with one that also lists: "tab pin / close others / close to the right, pane pin and reading width, workspace set default shell and move up / down, agent stop / open transcript / link plan / unlink plan / improve prompt, run start / restart / stop of one command, Git change base and switch repository, Jira all / mine, list / kanban and refresh, copy last error, dismiss notifications, History, install update".

Extend the note under the table (after the existing `Alt+Shift+A / Alt+Shift+S and F2` paragraph) with one more paragraph:

```
Alt+0 and Ctrl+Shift+PgUp / PgDn no longer reach a focused terminal.
In Board, Git and Files the single keys `a`, `w` and Alt+Enter act only while the card board, the changed-file list or the file tree has the keyboard; a text field, a comment draft or the Files editor keeps them.
```

(Do not add the wave 1-3 rows; those waves add their own.)

- [ ] **Step 2: SPEC §10.** Sidebar filter paragraph (line 771-777): after `with an empty box Enter gives the keyboard back to the pane` add one sentence on its own line: `Alt+0 (§13) focuses the box with the active row highlighted: with an empty box ↑/↓ walk every visible row, ←/→ fold / unfold the highlighted row's workspace and Enter opens it; with text ←/→ move the caret.` Add before `beside it the remove-idle-agents button` a sentence: `Tab / Shift+Tab in the box step the All / Working / Needs you chips.` Board bullet (line 866): after the Jira switch sentence add: `Show Board (Ctrl+Shift+2) again switches Tasks / Jira, and the card keys walk the Jira rows.` Keep each sentence on its own physical line as in the neighbouring text.

- [ ] **Step 3: ADR 0081.** Check the file exists (wave 1 creates it; if it does not, stop and report). Append:

```markdown

## Wave 4: single actions

Ten commands and about 40 palette entries cover the actions that had no key: focus sidebar (`alt-0`), approve card (`a`), next / previous comment (`f8`, `shift-f8`), reply / resolve comment (`ctrl-alt-r`, `ctrl-alt-enter`), move tab (`ctrl-shift-pageup`, `ctrl-shift-pagedown`), diff wrap (`w`), open kept (`alt-enter`).
Each key calls the function the mouse handler calls, so a click and a key cannot diverge.
Keys a text field also uses are bound in a view context (`Board`, `Git`, `Files`, `Docs || Git`) and `Shell::run` hands them to a focused field (`jira_*`, `comment_*`, `git_commit_*`); `OpenKept` also acts only while the tree has the keyboard.
Decisions: Alt+0 widens the compact rail because the rail has no filter box and saves the layout like Ctrl+Alt+B; in the sidebar filter ←/→ fold rows only while the box is empty; the Board key context is on over the Jira list so the `Card` keys reach it (Kanban cards are not scrolled into view, the same known limit as Board cards); "Run: Start / Restart / Stop <command>" entries carry the command's index in the Run summary and resolve it when picked, like "Agent: New <Profile> pane"; "Agent: Unlink plan" is listed only when a plan is linked, and "Global: History…" needs an active workspace (it is the title bar button's function).
```

- [ ] **Step 4: HUMAN_CHECKS.** Insert under `## To verify (non-blocking)` (before the `2026-10-27 Voice recording animation` section):

```markdown
### To verify - 2026-10-27 Keyboard wave 4: single actions (ADR 0081)

- [ ] **Alt+0 (dark + light):** from a terminal, then with the sidebar hidden, then with the compact rail: the sidebar shows (rail widens), the filter takes the keyboard, the active row is highlighted. Up / Down walk workspace and session rows, Left / Right fold / unfold, Enter opens, Alt+0 again returns to the pane. With text typed, Left / Right move the caret. Tab / Shift+Tab step All / Working / Needs you.
- [ ] **Board:** select a Review card, `A` moves it to Done (the Approve tooltip shows `A`); `A` typed in the card follow-up, in the task dialog and in the sidebar filter types an `a`. Ctrl+Shift+2 on the Board with Jira set up switches Tasks / Jira. On the Jira list ↑ / ↓ move a ring, ← / → fold a List row or move Kanban columns, Enter opens the issue; typing in the Jira filter and keys fields is not stolen.
- [ ] **Comments:** in Docs and in the Git diff F8 / Shift+F8 select the next / previous thread and scroll the doc and the margin to it, Ctrl+Alt+R opens its reply field (nothing on a resolved thread), Ctrl+Alt+Enter resolves / reopens. In a comment draft none of them act.
- [ ] **Tabs:** Ctrl+Shift+PgDn / PgUp move the active terminal tab and the active file tab one place; nothing at the ends and never across the pinned border. A running agent pane is not disturbed.
- [ ] **Git and Files:** `W` toggles Wrap in the Git list (typing `w` in the commit box types); the Wrap tooltip shows `W`. Alt+Enter on a tree row opens a kept (non-italic) tab; in the editor it does not.
- [ ] **Tooltips:** hunk Stage shows `S` on the current hunk only, the Branch chip `Ctrl+Shift+4`, the workspace switcher `Alt+1 … Alt+9`, the card Approve `A` on the selected card.
- [ ] **Palette:** every wave 4 entry (tab pin / close others / right, pane pin / reading width, set default shell opens the workspace menu on the shell list, workspace move up / down, agent stop / transcript / link / unlink / improve, run start / restart / stop per command, Git change base / switch repository, Jira all / mine, list / kanban, refresh, copy last error, dismiss notifications, History, install update) shows only in its state and does what the mouse control does.
- [ ] **Not automated:** all of the above is keyboard-only GPUI input; the agent could not send key events to the app (see the smoke note in the plan).
```

- [ ] **Step 5: Commit**

```bash
git add docs/SPEC.md docs/decisions/0081-keyboard-only-operation.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "docs: keyboard wave 4 in SPEC section 10 and 13, ADR 0081 and HUMAN_CHECKS"
```

---

### Task 9: Verify and smoke run

**Files:** none modified except `docs/HUMAN_CHECKS.md` (only to record what could not be done).

- [ ] **Step 1: Full verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt, clippy `-D warnings` and every test pass. On OOM or linker errors re-run with `-j 2`. Fix any failure in the task that owns it (for example an insta snapshot of a changed tooltip), re-run, and commit the fix with its own `fix(ui): ...` commit.

- [ ] **Step 2: Build and launch in a sandbox**

```bash
cargo build -p mkrew-ui
SANDBOX="$(mktemp -d)"
export MKREW_DATA_DIR="$SANDBOX/data" MKREW_CONFIG_DIR="$SANDBOX/config" MKREW_SOCKET="$SANDBOX/mkrew.sock"
mkdir -p "$MKREW_DATA_DIR" "$MKREW_CONFIG_DIR"
MKREW_LOG=info "$PWD/target/debug/mkrew.exe"
```

(Use the absolute `"$PWD/..."` path: omp's bash resolves relative program paths against the session folder. The dev exe starts its sandbox daemon through the same variables, see the existing "Reconnect / Retry" check in `docs/HUMAN_CHECKS.md`. Never run it against the real data folder.)

- [ ] **Step 3: Walk the wave keyboard-only, dark and light.** Add two workspaces (`Ctrl+Shift+K`), open two agent tabs and a shell tab, then do, without the mouse: `Alt+0`, type, `Tab`, `Esc`; `Ctrl+Shift+3`, `Alt+Enter` on a file, `Ctrl+Shift+PgDn`; `Ctrl+Shift+4`, `W`; `Ctrl+Shift+2`, select a Review card and press `A`; Docs `F8`, `Ctrl+Alt+R`, `Ctrl+Alt+Enter`; `Ctrl+Shift+P` and run a few of the new entries (Tab: Pin, Workspace: Move down, Run: Start of a configured command, Global: Copy last error). Toggle the theme with `Ctrl+Shift+J` and repeat the look checks.

- [ ] **Step 4: Record what could not be done.** The agent cannot synthesize GPUI key events, so if any step above could not be performed by the executor, keep the matching `HUMAN_CHECKS.md` line from Task 8 unchecked and add under it one line `Smoke: not run keyboard-only by the agent (reason: ...)`; for every step that was performed and passed, tick its line. Do not tick a line that was not exercised. Quit the app, stop the sandbox daemon (`mk daemon stop` with the same three variables), and delete `$SANDBOX`.

- [ ] **Step 5: Commit** (only if `HUMAN_CHECKS.md` changed)

```bash
git add docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "docs: record keyboard wave 4 smoke run in HUMAN_CHECKS"
```

---

## Self-Review

**Spec coverage** (Wave 4 section):

| Spec item | Task |
|---|---|
| `FocusSidebar` alt-0 (show sidebar, focus filter, active row highlighted) | 1 |
| `ApproveCard` a (Board context, same `approve_task` as the button) | 2 |
| `NextComment` / `PreviousComment` f8 / shift-f8, `ReplyComment` ctrl-alt-r, `ResolveComment` ctrl-alt-enter | 3 |
| `MoveTabLeft` / `MoveTabRight` incl. file tabs, within pin group | 4 |
| `GitToggleWrap` w, `OpenKept` alt-enter | 5 |
| Jira `Card(_)` keys, Show Board toggles Tasks / Jira | 2 |
| Sidebar filter Tab / Shift+Tab chips, ←/→ | 1 |
| Every `palette::Extra` row of the table | 6 (tab, pane, workspace, agent), 7 (Git, Jira, global; run commands dynamic) |
| Tooltip fixes (hunk Stage, Branch chip, workspace switcher, card Approve) | 5, 2 |
| Text-field precedence for `a`, `w`, Alt+Enter | 2 (Jira guard), 5 (`files_focus`), existing guards; Review Focus 1 |
| Palette availability tests, keymap uniqueness by existing tests | 6, 7 and every command task |
| ADR 0081, SPEC §13 / §10, HUMAN_CHECKS, smoke | 8, 9 |
| "Set default shell…" via wave 1 | 6 (`open_workspace_menu_at_switcher`, `menu_select_after`, `DEFAULT_SHELL_HEADING`, confirmed with Wave1Menus) |

**Placeholder scan:** no "TBD" / "later" steps. One code-level hedge remains by design where the real type could not be read: the `Parent` struct construction in the Task 2 `a_parent_outside_the_results_is_not_selectable` test (the step says to copy the module's existing parent test construction), and the `inspector` import line in Task 6. Both name exactly what to copy.

**Type consistency:** `sidebar::step_index(len, from, by)` is defined in Task 1 and used by Tasks 4 and 6. `Chip::step`, `browse_from`, `set_expanded` are defined and used in Task 1 only. `Board::approve_target`, `Source::other`, `columns`, `select_step`, `JiraView.selected`, `jira_scroll` are defined and used in Task 2. `Comments::{step_thread, reply_target, status_target, margin_index}` and `comments_scroll` are defined and used in Task 3. `active_place` (Tabs and FilesView) and `move_active_tab` are Task 4. `Context` fields: Task 2 `approve_card`; Task 6 `tab_others, tab_right, pane, live_session, transcript, plan_linked, composer_open`; Task 7 `git_base, git_repos, jira, last_error, toast, update_ready`; each task extends the test helper `context()` with its own fields. `Extra::ALL` length: 18, then 31 (Task 6), then 40 (Task 7).

**Spec gaps and conflicts found while planning (also reported to Main):**

- "Alt+0 shows the sidebar if hidden" says nothing about the compact rail, which has no filter box: the plan widens it (saved like Ctrl+Alt+B).
- ←/→ "collapse / expand the highlighted workspace" clashes with caret movement: the plan acts only while the filter box is empty; the highlight with an empty box (and on workspace rows, needed to reach a collapsed workspace) needs a new browse mode in `Sidebar`.
- Making `Card(_)` reach Jira means turning the Board key context on over the Jira list, which exposes the Jira filter and keys fields to `a`, `e` and the arrows: `Shell::run` now hands Board-context keys to them.
- "Global: History…" is "always" in the spec but the title bar button needs a workspace; "Agent: Unlink plan" is refined to "only when linked"; "Git: Change base…" is available where its button is (Against base section).
- The spec's "Against base available" and "workspace switcher (Alt+1…9)" were read as above; the switcher tooltip already showed `NextWorkspace`'s key and now shows `Alt+1 … Alt+9`.
- Wave 3's `Comments::pick` resets its `head` field; the plan reveals through `reveal_pick`, which uses `selection.end`, so a thread taller than the viewport shows its bottom line.
- The agent cannot send GPUI key events, so the smoke task can only launch and run verify; the keyboard walk-through is recorded in HUMAN_CHECKS.
