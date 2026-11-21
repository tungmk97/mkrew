# Pressable Audit (Polish P3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every clickable element in `crates/mkrew-ui/src` takes its hover, press and disabled look from `shell/widgets.rs` (through the `Pressable` trait of Plan P1), so that after this plan only `widgets.rs` and `pressable.rs` call `.hover(` and `.active(`.

**Architecture:** Plan P1 gives `Look`, `press_style`, `Pressable::pressable` and `Pressable::on_press`, and rebuilds the shared buttons on them.
P3 first adds the few helpers the audit found missing around that contract (Task 1), then walks the 48 files by area, one commit per area (Tasks 2 to 9).
An element that is already a widget keeps its `.on_click` (its look comes from the widget).
A hand-built element moves to `.pressable(look, enabled, theme)` plus `.on_press(enabled, handler)`, or to a shared helper.
A short list of elements is exempt (grips, terminal, drop zones, scrim, text fields and a few more), each with its reason.
Task 10 writes the rule into SPEC §14, the `mkrew-ui-design` skill and the `ui-reviewer` checklist; Task 11 re-runs the inventory and `scripts/verify.sh`.

**Tech Stack:** Rust, GPUI (gpui-pre 0.3.7), `crates/mkrew-ui` (a lib crate plus the `mkrew-ui` bin crate whose `shell/` module holds the widgets).

**Spec:** `docs/superpowers/specs/2026-11-21-polish-palette-settings-design.md` §5.1 (the rule), §5.3 (the audit), §12 (docs updated).
Plan P1 (`docs/superpowers/plans/2026-11-21-polish-p1-foundation.md`) must be merged first.
Plan P2 may land before or after this plan (spec §9).

## Global Constraints

- Every color, spacing, radius, font size and duration comes from `crates/mkrew-ui/src/theme.rs` tokens; spacing 4/8/12/16/24/32; dark and light themes both work.
- No `unwrap`/`expect` outside tests unless provably infallible (comment why). `thiserror` in libs, `anyhow` in bins.
- Tests never call a real agent or the network; git tests use temporary repos (the `fn git(dir, &[..])` helpers of the existing tests).
- Repo policy forbids tests that read source text: this plan adds no source-grep test.
  The grep commands in Task 11 are an inventory step for the person executing the plan, not a test.
- Verify: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (fmt check, clippy `-D warnings`, tests). Fast loop: `cargo test -p mkrew-ui <filter>` and `cargo clippy -p mkrew-ui --all-targets -- -D warnings`.
  If the full test build crashes rustc (STATUS_STACK_BUFFER_OVERRUN), run fmt check and clippy, then `cargo test --workspace -j 4`.
- Commits: Conventional Commits, `git -c user.email=tungnv975@gmail.com commit`, only the files of the task (`cargo fmt --all` may touch other agents' files: run `git status` and add paths by name), never push.
- Docs: never use the em dash character; in long Markdown put each sentence on its own line.
- ADR number 0097 (written by P1; this plan only appends to it). `PROTOCOL_VERSION` is not touched by this plan.
- Windows native is first-class: the visual checks run on Windows.
- GPUI allows one `.hover(..)` per element and panics at paint on a second one.
  Converting an element means deleting its own `.hover(`/`.active(` first.
  Never put `.pressable(..)` on a widget that already sets a hover (`button`, `secondary_button`, `quiet_button`, `danger_button`, `icon_button`, `icon_button_in`, `segment`, `menu_item`, `row_action`, `list_row`).
- Names from P1 used verbatim: `Look { Row, Chip, Icon, Text, Raised }`, `PressStyle { hover_bg, hover_fg, underline_on_hover, active_opacity, opacity, pointer }`, `press_style(Look, bool, &Theme) -> PressStyle`, `Pressable::pressable(Look, bool, &Theme)`, `Pressable::on_press(bool, handler)`, `widgets::{button, secondary_button, quiet_button, danger_button, disabled_button, icon_button, icon_button_in, segment, menu_item, toggle, dropdown_trigger, list_row}`.
  P1 puts `Look`, `PressStyle`, `press_style` and `Pressable` in `crates/mkrew-ui/src/pressable.rs` (the lib crate, because `preview.rs` is in the lib) and re-exports them from `shell/widgets.rs`.
  If P1 chose another path, only the `use` lines in this plan change.
- Line numbers in this plan were read at commit `1b7abb5` (before P1 and P2).
  P1 and P2 shift lines in `widgets.rs`, the dialog files, `settings.rs` and `navigation.rs`.
  Find an element by its function name and element id (the tables give both); re-run the inventory command of the task to get fresh numbers.
  Edit each file from the bottom up so earlier line numbers stay valid.

## Review Focus

1. A selected, picked or active element (session row, tab chip, file row, picker entry) must stay clickable and must not fade: `enabled=false` means the DISABLED look.
   Such elements use `pressable_unless(picked, look, theme)` (Task 1).
   No unit test can reach it (an element's style cannot be read without a window); it is pinned by the "click the selected one" bullet of the HUMAN_CHECKS section written in Tasks 2, 3, 5, 6, 7, 8 and 9.
2. A hover fill that equals the selected fill or the surface under it shows nothing (in Mkrew Dark and Mkrew Light `selected == surface_raised`; menus, toasts, cards and the find bar are `surface_raised`).
   Pinned by P1 Task 4's data-driven `press_style` tests over every built-in theme (Row, Chip and Icon hover with `theme.hover`, Raised with `theme.border`); this plan only consumes them.
3. A second `.hover(` on one element panics at paint, and nothing at compile time catches it.
   Pinned by the "open the view: it paints" step of every area task, and by the sweep in Task 11 that must find `.hover(` only in `widgets.rs` and `pressable.rs`.
4. A handler must not change behavior: `on_press(false, ..)` attaches nothing, where the old code kept the click while busy (Broadcast row, send row) or for a row with nothing to go to (port row without a session).
   Pinned by Task 8 steps that read the three handlers first (`go_to_port` must return early without a session) and by the Task 8 visual check.
5. Pressing a nested control (the Edit chip on a card, a stage icon on a row, the queue X) also dims its pressed parent through `.active`, and a drag source (card, tab) must not stay dimmed after a drag.
   Pinned by the visual bullets of Tasks 2, 3, 4 and 6; fallback written in each: drop the parent's press look (`card_press` has no `.active` line in its fallback form).

---

## Inventory (baseline at commit 1b7abb5)

Command (run it with the grep tool or Git Bash from the repo root; this is how the numbers below were taken):

```bash
grep -rnE '\.on_click\(|\.on_mouse_down\(|\.hover\(|\.active\(\|' crates/mkrew-ui/src --include='*.rs'
```

Totals today: `.on_click(` 294, `.on_mouse_down(` 33, together 327 handlers in 48 files; `.hover(` 76 (10 of them in `widgets.rs`; of the other 66, one is a comment in `sidebar.rs` and 3 are on resize grips); `.active(|` style closures 44 outside tests (8 of them in `widgets.rs`).
On Windows `grep -r` prints paths with backslashes: filter with `grep -v 'widgets.rs'`, not `grep -v 'shell/widgets.rs'`.
`.active(` also matches `tabs.active()`, `detached.active(key)` and the like: `tabs.rs` 358/405/707, `hosts.rs` 79/81, `detached_actions.rs` 291, `focus.rs` 187, `pane_render.rs` 19/22/112, `detached_render.rs` 272, `history.rs` 445 and the tests. They are not styles; ignore them.

Classes used below:

- **W** (already a widget): the element is built by `button`, `secondary_button`, `quiet_button`, `danger_button`, `disabled_button`, `icon_button`, `icon_button_in`, `segment`, `menu_item`, `row_action`, `card_action`, `caret_field` and the like; the look comes from the widget and the `.on_click(` stays.
- **P** (hand-built): becomes a `Look` (or a helper) plus `.on_press(enabled, handler)`, with its own `.hover(`/`.active(`/`.cursor_pointer()` deleted.
- **EXEMPT**: no look, with a reason (table further down).

### Per file

| file | `.on_click(` | `.on_mouse_down(` | `.hover(` | W | P | EXEMPT | task |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `shell/widgets.rs` | 2 | 0 | 10 | 1 | 1 | 0 | 1 |
| `shell/chrome.rs` | 7 | 1 | 2 | 5 | 2 | 1 | 2 |
| `shell/connection.rs` | 2 | 0 | 0 | 2 | 0 | 0 | 2 |
| `shell/menu_render.rs` | 2 | 0 | 1 | 1 | 1 | 0 | 2 |
| `shell/sidebar.rs` | 22 | 5 | 12 | 11 | 11 | 5 | 2 |
| `shell/toasts.rs` | 2 | 0 | 2 | 0 | 2 | 0 | 2 |
| `shell/update.rs` | 1 | 0 | 0 | 1 | 0 | 0 | 2 |
| `shell/detached_render.rs` | 3 | 1 | 0 | 3 | 0 | 1 | 3 |
| `shell/file_tabs_render.rs` | 2 | 2 | 1 | 1 | 1 | 2 | 3 |
| `shell/focus.rs` | 0 | 1 | 0 | 0 | 0 | 1 | 3 |
| `shell/pane_render.rs` | 11 | 4 | 3 | 8 | 2 | 5 | 3 |
| `terminal/view.rs` | 1 | 2 | 1 | 0 | 1 | 2 | 3 |
| `shell/cards.rs` | 10 | 0 | 2 | 7 | 2 | 1 | 4 |
| `shell/jira.rs` | 11 | 0 | 3 | 6 | 3 | 2 | 4 |
| `field.rs` | 0 | 1 | 0 | 0 | 0 | 1 | 5 |
| `preview.rs` | 4 | 5 | 3 | 0 | 4 | 5 | 5 |
| `shell/comments.rs` | 8 | 0 | 0 | 6 | 1 | 1 | 5 |
| `shell/docs.rs` | 7 | 1 | 3 | 4 | 3 | 1 | 5 |
| `shell/files_preview.rs` | 1 | 2 | 0 | 1 | 0 | 2 | 5 |
| `shell/files_render.rs` | 20 | 5 | 2 | 15 | 3 | 7 | 5 |
| `shell/files_split.rs` | 0 | 1 | 1 | 0 | 0 | 1 | 5 |
| `shell/git_dialogs.rs` | 6 | 0 | 0 | 6 | 0 | 0 | 6 |
| `shell/git_diff.rs` | 15 | 1 | 1 | 15 | 0 | 1 | 6 |
| `shell/git_menus.rs` | 5 | 0 | 1 | 2 | 1 | 2 | 6 |
| `shell/git_render.rs` | 20 | 0 | 1 | 18 | 1 | 1 | 6 |
| `shell/git_rows.rs` | 6 | 0 | 4 | 2 | 4 | 0 | 6 |
| `shell/composer.rs` | 6 | 1 | 2 | 3 | 3 | 1 | 7 |
| `shell/history.rs` | 7 | 0 | 2 | 5 | 2 | 0 | 7 |
| `shell/inspector.rs` | 9 | 0 | 0 | 9 | 0 | 0 | 7 |
| `shell/path_field.rs` | 2 | 0 | 0 | 2 | 0 | 0 | 7 |
| `shell/peek.rs` | 2 | 0 | 0 | 1 | 0 | 1 | 7 |
| `shell/slash.rs` | 3 | 0 | 1 | 2 | 1 | 0 | 7 |
| `shell/thread_panel.rs` | 2 | 0 | 0 | 0 | 2 | 0 | 7 |
| `shell/ask_dialog.rs` | 3 | 0 | 0 | 3 | 0 | 0 | 8 |
| `shell/confirm_prompt.rs` | 1 | 0 | 0 | 1 | 0 | 0 | 8 |
| `shell/diagram_dialog.rs` | 1 | 0 | 0 | 1 | 0 | 0 | 8 |
| `shell/dialogs.rs` | 9 | 0 | 1 | 8 | 1 | 0 | 8 |
| `shell/phone_access.rs` | 8 | 0 | 0 | 8 | 0 | 0 | 8 |
| `shell/ports.rs` | 5 | 0 | 2 | 3 | 2 | 0 | 8 |
| `shell/send_dialog.rs` | 7 | 0 | 1 | 6 | 1 | 0 | 8 |
| `shell/task_dialog.rs` | 8 | 0 | 2 | 6 | 2 | 0 | 8 |
| `shell/workspace_actions.rs` | 5 | 0 | 0 | 5 | 0 | 0 | 8 |
| `shell/worktrees.rs` | 12 | 0 | 0 | 12 | 0 | 0 | 8 |
| `shell/machines.rs` | 6 | 0 | 0 | 5 | 0 | 1 | 9 |
| `shell/navigation.rs` | 2 | 0 | 2 | 0 | 2 | 0 | 9 |
| `shell/prompts_render.rs` | 7 | 0 | 1 | 4 | 1 | 2 | 9 |
| `shell/run.rs` | 14 | 0 | 6 | 8 | 6 | 0 | 9 |
| `shell/settings.rs` | 7 | 0 | 3 | 1 | 5 | 1 | 9 |
| **48 files** | **294** | **33** | | **208** | **71** | **48** | |

`.on_mouse_down(` rows of class P (`docs.rs:517`, `preview.rs:1048`) keep their mouse-down handler (they pick lines and read Shift at the press) and only take the look from `pressable_unless`.

### Look chooser (the decision every P row uses)

| The element | Use |
| --- | --- |
| A list or menu row on `bg`, `surface`, the sidebar or a dialog panel | `Look::Row` |
| A pill, chip, tab or small labelled control with its own shape | `Look::Chip` |
| An icon-only control | `Look::Icon` |
| A text link or a muted inline line | `Look::Text` |
| Any of the above that sits on `surface_raised` (menu, toast, popover, board card, comment card, find bar, code block) or inside a row that is itself hovered | `Look::Raised` (P1 Task 4) |
| A board card (task card, issue card): the hover lights the border, the fill stays, so a chip inside the card keeps its own hover | `card_press` (Task 1) |
| Selected, picked or active but still clickable | `pressable_unless(picked, look, theme)` (Task 1) |
| Really disabled (busy, locked, read only) | `pressable(look, false, theme)` with `on_press(false, ..)`, or `disabled_button` / `disabled_icon_button` |
| Not clickable right now and not "disabled" (a cleared Inbox label, a port row without a session, a comment anchor without lines) | no look at all |
| A resize grip's tint | `grip_hover` (Task 1) |
| A text-selection surface that picks lines (diff line) | `pick_surface` (Task 1) |

### Exempt elements and why

These keep their handlers and get no hover.
The 48 rows are the `.on_click(`/`.on_mouse_down(` sites; the drag, drop and move handlers below the table are not counted in the 327.

| file:line | kind | reason |
| --- | --- | --- |
| `field.rs:120` | text field | `caret_mouse`: the caret handler of every text field |
| `preview.rs:247` | link guard | `stop_propagation` around an `InteractiveText` link |
| `preview.rs:254` | link | `InteractiveText` link: the underline is a text range, `Look::Text` cannot apply |
| `preview.rs:686` | guard | `stop_propagation` on the task checkbox, which has a click (687) |
| `preview.rs:864` | guard | `stop_propagation` on the Copy chip, which has a click (865) |
| `preview.rs:971` | guard | `stop_propagation` on the Expand chip, which has a click (972) |
| `shell/cards.rs:830` | text field | follow-up field (caret_field); the click only focuses it |
| `shell/chrome.rs:344` | window drag | macOS title bar mouse down: drag and double-click maximize, not a control |
| `shell/comments.rs:745` | text field | comment draft field |
| `shell/composer.rs:851` | text field region | panel-wide mouse down focuses the composer and stops propagation; no click handler |
| `shell/detached_render.rs:182` | window drag | macOS title bar drag, not a control |
| `shell/docs.rs:295` | text field | docs filter field |
| `shell/file_tabs_render.rs:116` | middle-click | closes the tab; the chip is pressable |
| `shell/file_tabs_render.rs:123` | right-click | file tab menu; the chip is pressable |
| `shell/files_preview.rs:52` | text selection | press on the Markdown page: focus, caret, selection |
| `shell/files_preview.rs:343` | text field guard | block edit field: focus plus `stop_propagation` |
| `shell/files_render.rs:180` | right-click | folder menu on the tree container, not a control |
| `shell/files_render.rs:234` | text field guard | new-entry name field wrapper |
| `shell/files_render.rs:555` | text field | find field |
| `shell/files_render.rs:623` | text field | replace field |
| `shell/files_render.rs:844` | editor text | click puts the caret in the code row (I-beam) |
| `shell/files_render.rs:889` | editor surface | the blank area below the last line focuses the editor |
| `shell/files_render.rs:1088` | right-click | file menu; the row is pressable |
| `shell/files_split.rs:154` | resize grip | double-click resets the split; the grip hover moves to `grip_hover` |
| `shell/focus.rs:408` | scrim | `modal_shade`: swallows the click outside a dialog, no look |
| `shell/git_diff.rs:1211` | text selection | mouse down on a diff line picks it (reads Shift); its hover preview moves into `pick_surface`, not a button look |
| `shell/git_menus.rs:223` | text field | repository search field |
| `shell/git_menus.rs:349` | text field | file filter field |
| `shell/git_render.rs:936` | text field | commit message field |
| `shell/jira.rs:543` | text field | Jira filter field |
| `shell/jira.rs:657` | text field | Jira keys field |
| `shell/machines.rs:464` | text field | Name, Host and mk path fields |
| `shell/pane_render.rs:266` | right-click | tab context menu; the tab chip is pressable |
| `shell/pane_render.rs:545` | terminal surface | a click in a pane focuses it; `pane_frame` wraps the terminal |
| `shell/pane_render.rs:867` | text field guard | `stop_propagation` around the rename field; no click on the wrapper |
| `shell/pane_render.rs:884` | double-click only | a pane title renames on double-click; a pointer or hover would promise a single-click action it does not have |
| `shell/pane_render.rs:933` | right-click | pane header context menu; the header is a drag source and container |
| `shell/peek.rs:546` | text field | peek reply field |
| `shell/prompts_render.rs:226` | text field | prompt name field |
| `shell/prompts_render.rs:252` | text field | prompt body field |
| `shell/settings.rs:261` | text field | Keyboard page search field |
| `shell/sidebar.rs:204` | right-click | opens the pane menu; the row is pressable (live run row) |
| `shell/sidebar.rs:1206` | right-click | workspace menu; the row is pressable |
| `shell/sidebar.rs:1400` | right-click | pane menu; the row is pressable |
| `shell/sidebar.rs:1544` | right-click | workspace menu; the rail row is pressable |
| `shell/sidebar.rs:1606` | right-click | pane menu; the rail row is pressable |
| `terminal/view.rs:875` | text field | find bar field (caret_mouse, I-beam) |
| `terminal/view.rs:1032` | terminal surface | mouse_down/move/up of the terminal grid: selection and mouse reporting |

Drag sources, drop zones and move handlers (no click, no look, nothing to convert): `pane_drop.rs` 40 (drag), 60, 66 (drag move), 73, 76 (drop); `reorder.rs` 77 (drag), 97 (drag move); `tear_off.rs` 178 (drag move); `cards.rs` 151 (drop), 222 (drag); `file_tabs_render.rs` 50 (drop); `detached_render.rs` 331 (drop); `pane_render.rs` 40 (drop), 477 (drag move), 537 (drop), 1155-1163 (grip drag and mouse up); `sidebar.rs` 809-825 (grip), 851 (drop); `inspector.rs` 269-287 (grip); `files_render.rs` 852 (move), 1062 (drag); `files_preview.rs` 58, 347 (move), 348 (mouse down out); `files_split.rs` 140-170 (grip); `task_dialog.rs` 525 (drag move), 555 (drop); `peek.rs` 590 (mouse down out), 648 (hover listener); `toasts.rs` 101 (hover listener); `sidebar.rs` 1409 (hover listener); `terminal/view.rs` 1033-1035, 1042; `field.rs` 134 (move).
`on_hover` listeners are behavior, not style: do not touch them.
`group_hover(..)` is a separate style slot, not the single `.hover(`: the reveal of hidden row actions keeps using it (`opacity::HIDDEN` then `opacity::FULL`), and the rule of Task 10 allows it.

Only the three resize grips (`sidebar.rs:808`, `pane_render.rs:1154`, `files_split.rs:133`) and the diff line pick (`git_diff.rs:1209`) call `.hover(` without being a button; Task 1 gives them `grip_hover` and `pick_surface` in `widgets.rs`.

---

### Task 1: Additive helpers the audit needs

P1 Task 4 owns the looks and their colors: `Look { Row, Chip, Icon, Text, Raised }`, where `Row`, `Chip` and `Icon` hover with `theme.hover` and `Raised` hovers with `theme.border`, with data-driven tests over every built-in theme.
This task changes none of that.
It adds only what the audit found missing around it: a picked-but-clickable helper, `card_press`, `disabled_icon_button`, `grip_hover`, `pick_surface`, and `row_action` moved into `widgets.rs` (11 callers in 7 files, hand-built today with its own `.hover(`).

**Files:**
- Modify: `crates/mkrew-ui/src/pressable.rs` (`pressable_unless` and its pure helper `unless_look`; tests in the same module's test block, next to P1's `press_style` tests)
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (`row_action`, `grip_hover`, `pick_surface`, `card_hover_border`, `card_press`, `icon_frame`, `disabled_icon_button`, the dismiss button of `conflict_badge`)
- Modify: `crates/mkrew-ui/src/shell/git_rows.rs` (delete the `row_action` definition at 483-504)
- Modify: `docs/HUMAN_CHECKS.md` (creates the section the area tasks append to)

**Interfaces:**
- Consumes (P1 Task 4): `Look { Row, Chip, Icon, Text, Raised }`, `PressStyle`, `press_style(Look, bool, &Theme) -> PressStyle`, `trait Pressable { fn pressable(self, Look, bool, &Theme) -> Self; fn on_press(self, bool, handler) -> Self }` for `gpui::Stateful<gpui::Div>`, `widgets::icon_button_in`.
- Produces:
  - `pressable::unless_look(picked: bool, look: Look) -> Option<Look>`: `None` when `picked`, else `Some(look)` (pure, tested).
  - `Pressable::pressable_unless(self, picked: bool, look: Look, theme: &Theme) -> Self`: a picked element gets the pointer only, else `pressable(look, true, theme)`.
  - `widgets::row_action(id: SharedString, label: impl IntoElement, theme: &Theme) -> gpui::Stateful<gpui::Div>` (moved from `git_rows.rs`, same signature).
  - `widgets::grip_hover(grip: gpui::Stateful<gpui::Div>, theme: &Theme) -> gpui::Stateful<gpui::Div>`.
  - `widgets::pick_surface(el: gpui::Div, picked: bool, border: u32) -> gpui::Div`.
  - `widgets::card_hover_border(selected: bool, theme: &Theme) -> Option<u32>` (pure, tested) and `widgets::card_press(card: gpui::Stateful<gpui::Div>, selected: bool, theme: &Theme) -> gpui::Stateful<gpui::Div>`.
  - `widgets::disabled_icon_button(id: impl Into<gpui::ElementId>, glyph: Icon, color: u32, theme: &Theme) -> gpui::Stateful<gpui::Div>`.

- [ ] **Step 1: Write the failing tests.**
In `pressable.rs`'s test block:

```rust
#[test]
fn a_picked_element_gets_no_look() {
    for look in [Look::Row, Look::Chip, Look::Icon, Look::Text, Look::Raised] {
        assert_eq!(unless_look(true, look), None);
        assert_eq!(unless_look(false, look), Some(look));
    }
}
```

In a `#[cfg(test)] mod tests` at the end of `widgets.rs` (create it if P1 has not; start it with `use super::*;`):

```rust
#[test]
fn a_selected_card_does_not_light_its_border() {
    for entry in crate::theme::THEMES {
        let theme = entry.build();
        assert_eq!(card_hover_border(true, &theme), None, "{}", theme.name);
        assert_eq!(card_hover_border(false, &theme), Some(theme.border), "{}", theme.name);
        // The card's own fill is `surface_raised`: the lit border must differ from it.
        assert_ne!(theme.border, theme.surface_raised, "{}", theme.name);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mkrew-ui -- a_picked_element_gets_no_look a_selected_card_does_not_light`
Expected: FAIL to compile ("cannot find function `unless_look`", "`card_hover_border`").

- [ ] **Step 3: Add `unless_look` and `pressable_unless`** to `pressable.rs`:

```rust
/// The look of an element that may be picked (selected, active) but stays clickable: none while
/// picked, so it keeps the pointer only. `pressable(look, false, ..)` would fade it instead.
pub(crate) fn unless_look(picked: bool, look: Look) -> Option<Look> {
    (!picked).then_some(look)
}
```

Add to the `Pressable` trait `fn pressable_unless(self, picked: bool, look: Look, theme: &Theme) -> Self;` and to its impl for `gpui::Stateful<gpui::Div>`:

```rust
    fn pressable_unless(self, picked: bool, look: Look, theme: &Theme) -> Self {
        match unless_look(picked, look) {
            Some(look) => self.pressable(look, true, theme),
            None => self.cursor_pointer(),
        }
    }
```

In `widgets.rs` add the pure border helper and make `card_press` (below) use it:

```rust
/// The border a board card lights on hover: none while it is `selected`.
pub(crate) fn card_hover_border(selected: bool, theme: &Theme) -> Option<u32> {
    (!selected).then_some(theme.border)
}
```

- [ ] **Step 4: Run the tests (after Steps 5-7 the whole crate builds; this runs the two new tests only) to verify they pass**

Run: `cargo test -p mkrew-ui -- a_picked_element_gets_no_look a_selected_card_does_not_light`
Expected: PASS.

- [ ] **Step 5: Move `row_action` into `widgets.rs` on `Look::Raised`.**
Delete `row_action` and its doc comment at `git_rows.rs:483-504`; add this to `widgets.rs` (all callers reach it through `use shell::widgets::*` in `main.rs`, so no call site changes):

```rust
/// A small action inside a list row ("Stage" on a file row, "Stage hunk" on a hunk header, a
/// tab's X, or an [`icon_svg`]): no vertical padding, so the row keeps its height. It sits on a
/// row that is hovered or selected itself, so it hovers one step above them (`Look::Raised`).
pub(crate) fn row_action(
    id: SharedString,
    label: impl IntoElement,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .px(px(space::XS))
        .rounded(px(radius::SMALL))
        .font_family(theme.ui_font_family.clone())
        .text_size(font_size::META)
        .text_color(rgb(theme.text_muted))
        .pressable(Look::Raised, true, theme)
        .child(label)
}
```

- [ ] **Step 6: Add the other helpers to `widgets.rs`:**

```rust
/// The hover tint of a resize grip (sidebar, pane divider, Markdown split). The caller sets the
/// cursor and the drag handlers; a grip is not a button, so it has no `Look`.
pub(crate) fn grip_hover(
    grip: gpui::Stateful<gpui::Div>,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let tint = theme.divider;
    grip.hover(move |style| style.bg(rgb(tint)))
}

/// A surface you pick lines from with a mouse down (a diff line): a pointer and, unless it is
/// already `picked`, a border preview of the pick bar. Not a button, so no `Look`.
pub(crate) fn pick_surface(el: gpui::Div, picked: bool, border: u32) -> gpui::Div {
    el.cursor_pointer().when(!picked, |el| {
        el.hover(move |style| style.border_color(rgb(border)))
    })
}

/// A board card (task card, issue card): a pointer, the border lights on hover unless the card
/// is `selected`, and it dims while pressed. The fill does not change on hover, so a chip
/// inside the card (Edit, Undo) keeps a hover of its own that is visible.
pub(crate) fn card_press(
    card: gpui::Stateful<gpui::Div>,
    selected: bool,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    card.cursor_pointer()
        .when_some(card_hover_border(selected, theme), |el, border| {
            el.hover(move |style| style.border_color(rgb(border)))
        })
        .active(|style| style.opacity(opacity::ACTIVE))
}

/// The frame of [`icon_button_in`] and [`disabled_icon_button`].
fn icon_frame(
    id: impl Into<gpui::ElementId>,
    glyph: Icon,
    color: u32,
    enabled: bool,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .size(theme::rems_px(theme::CONTROL_HEIGHT))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::MEDIUM))
        .text_color(rgb(theme.text_muted))
        .pressable(Look::Icon, enabled, theme)
        .child(icon_svg(glyph, theme::ICON_SIZE, color))
}

/// An [`icon_button`] whose icon is drawn in `color`: a state (the phone icon is accent while
/// phone access is on).
pub(crate) fn icon_button_in(
    id: impl Into<gpui::ElementId>,
    glyph: Icon,
    color: u32,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    icon_frame(id, glyph, color, true, theme)
}

/// An icon button that cannot be used right now (the update button while it downloads):
/// faded, no hover, no pointer. Attach no click to it.
pub(crate) fn disabled_icon_button(
    id: impl Into<gpui::ElementId>,
    glyph: Icon,
    color: u32,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    icon_frame(id, glyph, color, false, theme)
}
```

Read `icon_button_in` as P1 left it first: `icon_frame` replaces its body, so keep any change P1 made to the frame (size, radius, text color) and only keep the `enabled` argument of `.pressable(Look::Icon, enabled, theme)`.
Read `button_frame` as P1 left it: it must still set no hover and no `.pressable(..)` (its doc comment says so), because Share (Task 3) and the Ignored chip (Task 5) build on it and add their own look; if P1 made it pressable, stop and tell the user.

- [ ] **Step 7: Give `conflict_badge`'s Dismiss the `Raised` look** (it sits on a raised card; it stays in `widgets.rs`).
Delete `let hover = theme.border;` (line 828) and the three lines `.cursor_pointer()` (861), `.hover(..)` (865) and `.active(..)` (866).
Add this line after the `.group_hover(group, ..)` line (864):

```rust
                .pressable(Look::Raised, true, theme)
```

Keep the tooltip, and turn the `.on_click(` at 872 into:

```rust
                .on_press(
                    true,
                    cx.listener(move |shell, _, _, cx| {
                        // Not a click on the card (which would open its terminal).
                        cx.stop_propagation();
                        dismiss(shell);
                        cx.notify();
                    }),
                )
```

`color_swatch` (its hover fades the circle with `opacity::HOVER`) and `window_control` (the OS owns its click) stay as they are: both are inside `widgets.rs`.

- [ ] **Step 8: Build, lint, test**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: clean (an unused `hover` or `opacity` import means a leftover to delete).
Run: `cargo test -p mkrew-ui`
Expected: PASS.

- [ ] **Step 9: Create the HUMAN_CHECKS section.**
In `docs/HUMAN_CHECKS.md`, under `## To verify (non-blocking)`, above the first `### To verify - 2026-11-14` heading, add:

```markdown
### To verify - 2026-11-21 Pressable audit (P3)

- [ ] **Row actions (dark + light):** hover the X of a tab chip and a stage icon on a Git file row: each shows a fill on its (already hovered) row.
  Hover a board card next to the selected one: only the unselected card lights its border.
```

- [ ] **Step 10: Commit**

```bash
git add crates/mkrew-ui/src/pressable.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/shell/git_rows.rs docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): pressable_unless, card_press and the shared helpers the audit needs"
```

**Review Focus pins (Task 1):** item 1 (`a_picked_element_gets_no_look`), item 5 (`a_selected_card_does_not_light_its_border`).

---

### Task 2: Chrome, sidebar, menus and toasts

**Files:**
- Modify: `crates/mkrew-ui/src/shell/chrome.rs`, `crates/mkrew-ui/src/shell/sidebar.rs`, `crates/mkrew-ui/src/shell/update.rs`, `crates/mkrew-ui/src/shell/toasts.rs`, `crates/mkrew-ui/src/shell/menu_render.rs`
- Read only (every handler is W): `crates/mkrew-ui/src/shell/connection.rs`
- Modify: `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: `Look`, `Pressable::{pressable, pressable_unless, on_press}`, `grip_hover`, `disabled_icon_button`, `icon_button_in` (Task 1 and P1), all through `use shell::widgets::*`.
- Produces: nothing later tasks use.

**P elements (16, convert these):**

| file:line | fn | element | Look | pattern | lines to delete | notes |
| --- | --- | --- | --- | --- | --- | --- |
| `shell/chrome.rs:594` | `render_workspace_switcher` | workspace-switch trigger | `Chip` | enabled=true | 563 (`let hover`, shared with 642), 575, 579, 580 | Keep the tooltip hidden while the dropdown is open. |
| `shell/chrome.rs:642` | `render_workspace_switcher` | title-branch pill | `Chip` | enabled=true | 628, 632 | The pill keeps `bg(theme.well)`. |
| `shell/menu_render.rs:99` | `item_row` | trailing icon button inside a menu row | `Icon` | enabled=true | 91, 92, 93, and `tint` in the tuple at 77-81 | Keep `stop_propagation`. The per-tone tint (`danger_bg`) is lost; `fg` stays. |
| `shell/sidebar.rs:119` | `render_run_group` | RUN group "Restart all" / "Stop all" icons | `Icon` | enabled=true | 115, 116, 117 | Keep `cx.stop_propagation()`. The per-action tint is lost (neutral fill). |
| `shell/sidebar.rs:164` | `render_run_group` | run row action icons (restart, stop, run) | `Icon` | enabled=true | 149 (comment), 159, 160, 161 | Delete the comment: `row_action` is no longer a reason to hand-build. Tint is lost. |
| `shell/sidebar.rs:201` | `render_run_group` | live run row opens its terminal | `Row` | `pressable_unless(selected, ..)`, only inside `when_some(live)` | 73 (`let hover`), 199, 200 | A stopped row has no click and gets no look; keep it so. |
| `shell/sidebar.rs:324` | `render_empty_workspaces` | "N empty workspaces" line | `Row` | enabled=true | 310 (`let hover`), 320, 323 |  |
| `shell/sidebar.rs:1004` | `render_sidebar_footer` | footer Inbox "N need you" | `Row` | enabled=true, inside `when(waiting > 0)` | 969 (`let hover`), 995, 996 | NOT `enabled=false` when clear: a cleared inbox is a plain label, not a disabled control. |
| `shell/sidebar.rs:1112` | `group_label` | group label (`RUN 2`) folds the group | `Row` | enabled=true | 1087 (`let hover`), 1098, 1099 | `.group(id)` stays: the run-all `group_hover` reveal needs it. |
| `shell/sidebar.rs:1203` | `workspace_row` | workspace header row | `Row` | `pressable_unless(active, ..)` | 1156 (`let hover`), 1195, 1197 | `active` is the flag that gives the row `bg(theme.selected)`. |
| `shell/sidebar.rs:1220` | `workspace_row` | workspace chevron (fold) | `Icon` | enabled=true | none (adds a look) | Today a click with no pointer or hover; this is a new affordance. Add `.pressable(Look::Icon, true, theme)` before `.on_click` becomes `.on_press`. |
| `shell/sidebar.rs:1397` | `session_row` | session row | `Row` | `pressable_unless(selected, ..)` | 1372 (`let hover`), 1391, 1392 | Right-click (1400) and `on_hover` stay. |
| `shell/sidebar.rs:1541` | `compact_workspace_row` | compact rail workspace row | `Row` | `pressable_unless(<its selected flag>, ..)` | 1514 (`let hover`), 1533, 1535 | Right-click (1544) stays. |
| `shell/sidebar.rs:1603` | `compact_session_row` | compact rail session row | `Row` | `pressable_unless(<its selected flag>, ..)` | 1581 (`let hover`), 1590, 1597 | Right-click (1606) stays. |
| `shell/toasts.rs:85` | `render_toasts` | error toast text (click copies it) | `Text` | enabled=true, error toasts only | 69 (`let hover`, shared with 118), 82, 83 | A fill on a raised card would be invisible; the Text look underlines instead. |
| `shell/toasts.rs:118` | `render_toasts` | toast close X | `Raised` | enabled=true | 113, 114, 115 |  |

**W elements (keep `.on_click(`; 20 sites, the look comes from the widget):**

- `shell/chrome.rs`: 181, 405, 418, 539, 735
- `shell/connection.rs`: 1434, 1445
- `shell/menu_render.rs`: 161
- `shell/sidebar.rs`: 409, 426, 510, 534, 542, 558, 622, 889, 1027, 1277, 1496
- `shell/update.rs`: 79

`update.rs:79` is W but needs the disabled twin (Step 4).
`widgets.rs:793` (`color_swatch`) stays in `widgets.rs` (Task 1).

**EXEMPT in these files** (see the exempt table): `chrome.rs:344`, `sidebar.rs:204, 1206, 1400, 1544, 1606`, and the grip of `sidebar.rs:808` (its `.hover(` moves to `grip_hover`).

- [ ] **Step 1: Re-run the inventory for this area and match it to the tables above.**

```bash
grep -nE '\.on_click\(|\.on_mouse_down\(|\.hover\(|\.active\(\||cursor_pointer' crates/mkrew-ui/src/shell/chrome.rs crates/mkrew-ui/src/shell/sidebar.rs crates/mkrew-ui/src/shell/update.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/toasts.rs crates/mkrew-ui/src/shell/menu_render.rs
```

Every `.hover(`, `.active(|` and `.cursor_pointer()` line must belong to a row of the P table (its "lines to delete" column) or sit in a W widget.
A line that belongs to neither is an element added since the tables were written: classify it with the Look chooser and convert it too.

- [ ] **Step 2: Convert the sidebar rows (`sidebar.rs`).**
Example, `session_row` (1372-1411): a selected row is picked, not disabled, so it keeps the pointer and takes no hover fill.
Delete `let hover = theme.hover;` (1372), `.cursor_pointer()` (1391) and `.hover(..)` (1392).

Before (1390-1403):

```rust
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(hover)))
        .when(selected, |el| {
            el.relative()
                .child(record_bounds(Anchor::SessionRow(session_id), cx))
        })
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.open_session(session_id, window, cx);
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
```

After:

```rust
        // The selected row is picked, not disabled: the pointer, no hover fill.
        .pressable_unless(selected, Look::Row, theme)
        .when(selected, |el| {
            el.relative()
                .child(record_bounds(Anchor::SessionRow(session_id), cx))
        })
        .on_press(
            true,
            cx.listener(move |shell, _, window, cx| {
                shell.open_session(session_id, window, cx);
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
```

The right-click handler and the `.on_hover(..)` listener stay as they are.
`workspace_row` (1203), `compact_workspace_row` (1541) and `compact_session_row` (1603) take the same edit with their own selected flag.

Example, the footer Inbox (969, 976-1007): a cleared inbox is a plain label, not a disabled control, so the look sits inside `when(waiting > 0)` and `enabled` stays `true`.

Before:

```rust
            // Nothing to jump to while clear: no pointer, no hover, no click.
            .when(waiting > 0, |el| {
                el.cursor_pointer()
                    .hover(move |style| style.bg(rgb(hover)))
                    .when(!compact, |el| {
                        el.tooltip(tooltip(
                            theme,
                            "Jump to next item needing attention",
                            self.keymap.label(Command::NextInbox),
                        ))
                    })
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.jump_next_inbox(window, cx);
                    }))
            });
```

After (and delete `let hover = theme.hover;` at 969):

```rust
            // Nothing to jump to while clear: no pointer, no hover, no click.
            .when(waiting > 0, |el| {
                el.pressable(Look::Row, true, theme)
                    .when(!compact, |el| {
                        el.tooltip(tooltip(
                            theme,
                            "Jump to next item needing attention",
                            self.keymap.label(Command::NextInbox),
                        ))
                    })
                    .on_press(
                        true,
                        cx.listener(|shell, _, window, cx| {
                            shell.jump_next_inbox(window, cx);
                        }),
                    )
            });
```

Example, the live run row (198-211): the `hover` goes, the look moves under the `selected` flag already in scope.

Before:

```rust
                .when_some(live, |el, session_id| {
                    el.cursor_pointer()
                        .hover(move |style| style.bg(rgb(hover)))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.open_session(session_id, window, cx);
                        }))
```

After:

```rust
                .when_some(live, |el, session_id| {
                    el.pressable_unless(selected, Look::Row, theme)
                        .on_press(
                            true,
                            cx.listener(move |shell, _, window, cx| {
                                shell.open_session(session_id, window, cx);
                            }),
                        )
```

The run action icons (164, and the run-all pair at 119) become `Look::Icon` and lose their tint.
Before (150-168 shape):

```rust
                // Not `row_action`: it sets its own hover, and a second `.hover()` panics.
                div()
                    .id(SharedString::from(format!(
                        "run-{action:?}-{project_id}-{name}"
                    )))
                    .flex_none()
                    .flex()
                    .items_center()
                    .p(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .cursor_pointer()
                    .hover(move |style| style.bg(rgb(colors.tint)))
                    .active(|style| style.opacity(opacity::ACTIVE))
                    .child(icon_svg(glyph, theme::ICON_SIZE_SMALL, colors.fg))
                    .tooltip(tooltip(theme, tip, ""))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        cx.stop_propagation();
                        shell.run_action(project_id, action, Some(name.clone()), window, cx);
                    }))
```

After (the comment line is deleted; `colors.fg` still colors the icon):

```rust
                div()
                    .id(SharedString::from(format!(
                        "run-{action:?}-{project_id}-{name}"
                    )))
                    .flex_none()
                    .flex()
                    .items_center()
                    .p(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .pressable(Look::Icon, true, theme)
                    .child(icon_svg(glyph, theme::ICON_SIZE_SMALL, colors.fg))
                    .tooltip(tooltip(theme, tip, ""))
                    .on_press(
                        true,
                        cx.listener(move |shell, _, window, cx| {
                            cx.stop_propagation();
                            shell.run_action(project_id, action, Some(name.clone()), window, cx);
                        }),
                    )
```

The run-all pair (106-123) takes the same edit; its parent keeps `.opacity(opacity::HIDDEN).group_hover(..)`.
The `workspace-chevron` (1220) has a click but no pointer or hover today: add `.pressable(Look::Icon, true, theme)` and turn its `.on_click(` into `.on_press(true, ..)`.
The group label (1112), the empty-workspaces line (324) are `Look::Row`, enabled, with no `pressable_unless` (they are never "picked").
Convert the remaining rows of the table the same way, then run `cargo check -p mkrew-ui` after each file: a leftover `let hover` shows as an unused variable.

The sidebar grip (808): `grip_hover` takes the grip with its id.

Before (799-808):

```rust
        let divider = theme.divider;
        let grip = div()
            .id("sidebar-grip")
            .absolute()
            .top_0()
            .bottom_0()
            .right_0()
            .w(px(theme::GRIP_WIDTH))
            .cursor(CursorStyle::ResizeLeftRight)
            .hover(move |style| style.bg(rgb(divider)))
```

After (keep `let divider` only if clippy reports it still used further down):

```rust
        let grip = grip_hover(div().id("sidebar-grip"), theme)
            .absolute()
            .top_0()
            .bottom_0()
            .right_0()
            .w(px(theme::GRIP_WIDTH))
            .cursor(CursorStyle::ResizeLeftRight)
```

- [ ] **Step 3: Convert the title bar (`chrome.rs`).**
`workspace-switch` (594) and `title-branch` (642) are `Look::Chip`, enabled.
Delete `let hover = theme.hover;` (563) once both are converted.

Before (574-580 and 593-596):

```rust
            .rounded(px(radius::MEDIUM))
            .cursor_pointer()
            .text_size(font_size::BODY)
            .font_weight(FontWeight(weight::SEMIBOLD))
            .text_color(rgb(theme.text))
            .hover(move |style| style.bg(rgb(hover)))
            .active(|style| style.opacity(opacity::ACTIVE))
```
```rust
            // An open menu already closed on this mouse down (outside it): the click reopens it.
            .on_click(cx.listener(|shell, _, window, cx| {
                shell.open_switcher(window, cx);
            }))
```

After:

```rust
            .rounded(px(radius::MEDIUM))
            .text_size(font_size::BODY)
            .font_weight(FontWeight(weight::SEMIBOLD))
            .text_color(rgb(theme.text))
            .pressable(Look::Chip, true, theme)
```
```rust
            // An open menu already closed on this mouse down (outside it): the click reopens it.
            .on_press(
                true,
                cx.listener(|shell, _, window, cx| {
                    shell.open_switcher(window, cx);
                }),
            )
```

`title-branch` (626-648): delete `.cursor_pointer()` (628) and `.hover(..)` (632), add `.pressable(Look::Chip, true, theme)` after the `.text_color(..)` line, and turn `.on_click(` (642) into `.on_press(true, ..)`.
Its `bg(theme.well)` stays.

- [ ] **Step 4: The update button, toasts and menu rows.**
`update.rs` (75-82): while the update downloads, the button is the disabled twin, not a live button with a faded overlay.

Before:

```rust
        let button = icon_button_in("update", Icon::CircleArrowUp, theme.accent_text, theme)
            .tooltip(tooltip(theme, &text, ""))
            .when(self.updating, |el| el.opacity(opacity::DISABLED))
            .when(!self.updating, |el| {
                el.on_click(cx.listener(|shell, _, window, cx| {
                    shell.start_update(window, cx);
                }))
            });
```

After:

```rust
        let button = if self.updating {
            disabled_icon_button("update", Icon::CircleArrowUp, theme.accent_text, theme)
        } else {
            icon_button_in("update", Icon::CircleArrowUp, theme.accent_text, theme).on_click(
                cx.listener(|shell, _, window, cx| {
                    shell.start_update(window, cx);
                }),
            )
        }
        .tooltip(tooltip(theme, &text, ""));
```

`toasts.rs`: the toast card is `surface_raised`, so the close X is `Look::Raised` and the error text (a link-like line) is `Look::Text`.
Delete `let hover = theme.hover;` (69), lines 82-83 (`.cursor_pointer()`, `.hover(..)`) and 113-115 (`.cursor_pointer()`, `.hover(..)`, `.active(..)`).

Before (81-92 and 107-122):

```rust
            let body = if is_error {
                body.cursor_pointer()
                    .hover(move |style| style.bg(rgb(hover)))
                    .tooltip(tooltip(theme, "Copy the error", ""))
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                        shell.toast_info("Copied", cx);
                    }))
                    .into_any_element()
            } else {
                body.into_any_element()
            };
```
```rust
                .child(
                    div()
                        .id(("toast-close", id))
                        .flex_none()
                        .p(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .cursor_pointer()
                        .hover(move |style| style.bg(rgb(hover)))
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .tooltip(tooltip(theme, "Dismiss", ""))
                        .child(icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted))
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            shell.toasts.dismiss(id);
                            cx.notify();
                        })),
                )
```

After:

```rust
            let body = if is_error {
                body.pressable(Look::Text, true, theme)
                    .tooltip(tooltip(theme, "Copy the error", ""))
                    .on_press(
                        true,
                        cx.listener(move |shell, _, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                            shell.toast_info("Copied", cx);
                        }),
                    )
                    .into_any_element()
            } else {
                body.into_any_element()
            };
```
```rust
                .child(
                    div()
                        .id(("toast-close", id))
                        .flex_none()
                        .p(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .pressable(Look::Raised, true, theme)
                        .tooltip(tooltip(theme, "Dismiss", ""))
                        .child(icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted))
                        .on_press(
                            true,
                            cx.listener(move |shell, _, _, cx| {
                                shell.toasts.dismiss(id);
                                cx.notify();
                            }),
                        ),
                )
```

`body` must be a `Stateful<Div>` for `pressable`: it already has `.id(("toast-text", id))` (the table's element id).
`menu_render.rs` (77-81, 91-99): the trailing icon button of a menu row is `Look::Icon` (the row itself is a `menu_item`, now `Look::Raised`, so a `Raised` fill on the button would equal the row's hover).
Delete `.cursor_pointer()` (91), `.hover(..)` (92) and `.active(..)` (93), add `.pressable(Look::Icon, true, theme)`, turn `.on_click(` (99) into `.on_press(true, ..)`, and remove `tint` from the tuple at 77-81 (`fg` stays).
This hover is the weakest in the plan (a `theme.hover` fill on a `theme.border` row): the visual step checks it in both themes.


- [ ] **Step 5: Build, lint and test.** No new test: these are styling changes, and their behavior is pinned by P1 Task 4's `press_style` tests and Task 1's helper tests.

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: clean. An "unused variable: `hover`" or "unused import" is a leftover to delete.
Run: `cargo test -p mkrew-ui`
Expected: PASS.

- [ ] **Step 6: Visual check, dark then light.** Run the app on Windows and switch the theme with the title bar toggle.
Every view of this area must paint without a panic (a second `.hover(` on one element panics at paint).
- **Rows (dark + light):** hover an unselected session row, a workspace header, a group label (`RUN 2`), a compact-rail row and the footer Inbox with something waiting: each shows a fill and a pointer.
  The selected session row and the open workspace header keep the pointer and show no hover fill; click each: it still acts and does not fade.
  With the inbox cleared, the Inbox label shows no pointer and no hover, and is not faded.
- **Icons (dark + light):** hover the run action icons of a running Run row, the Restart all / Stop all pair on a RUN group label, the workspace chevron, and the X that clears the sidebar filter: each shows a neutral fill.
  Press and hold one: the icon dims, and the row under it does not stay dimmed after release.
- **Title bar (dark + light):** hover the workspace switcher and the branch pill: a fill, and the tooltip still hides while their dropdown is open.
  While an update downloads the update icon is faded, shows no pointer and no hover.
- **Toasts and menus (dark + light):** hover the X of a toast and the text of an error toast (underline); hover a row of a context menu and the trailing icon button of a row that has one (open, delete): both are visible, and the button differs from its row.
- **Sidebar grip:** hover the right edge of the sidebar: the tint shows, drag still resizes.
Capture one screenshot per theme with the pointer on a converted element into `docs/screenshots/2026-11-21-pressable-chrome-sidebar-dark.png` and `docs/screenshots/2026-11-21-pressable-chrome-sidebar-light.png` if you can capture automatically.
Append the bullets above to the section `### To verify - 2026-11-21 Pressable audit (P3)` of `docs/HUMAN_CHECKS.md`, each as a `- [ ] **<name> (dark + light):** ...` line with its sentences on separate lines.

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-ui/src/shell/chrome.rs crates/mkrew-ui/src/shell/sidebar.rs crates/mkrew-ui/src/shell/update.rs crates/mkrew-ui/src/shell/toasts.rs crates/mkrew-ui/src/shell/menu_render.rs docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): chrome, sidebar, menus and toasts take their press look from widgets"
```

---

### Task 3: Tabs, panes and the terminal find bar

**Files:**
- Modify: `crates/mkrew-ui/src/shell/pane_render.rs`, `crates/mkrew-ui/src/shell/file_tabs_render.rs`, `crates/mkrew-ui/src/terminal/view.rs`
- Read only (every handler is W or EXEMPT): `crates/mkrew-ui/src/shell/detached_render.rs`, `crates/mkrew-ui/src/shell/focus.rs`, `crates/mkrew-ui/src/shell/pane_drop.rs`, `crates/mkrew-ui/src/shell/reorder.rs`
- Modify: `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: `Look`, `Pressable::{pressable, pressable_unless, on_press}`, `grip_hover`, `row_action` (Task 1).
  `terminal/view.rs` is in the lib crate and has no `use crate::*`: add `use crate::pressable::{Look, Pressable};` next to its other `use crate::` lines.
- Produces: nothing later tasks use.

The detached tab chips are drawn by `tab_chip` (`pane_render.rs`), so they follow the main window's chips.

**P elements (4, convert these):**

| file:line | fn | element | Look | pattern | lines to delete | notes |
| --- | --- | --- | --- | --- | --- | --- |
| `shell/file_tabs_render.rs:106` | `render_file_tabs` | file tab chip | `Chip` | `pressable_unless(is_active, ..)` | 35 (`let hover`), 86, 97, 98 |  |
| `shell/pane_render.rs:253` | `tab_chip` | tab chip (select; double-click renames) | `Chip` | `pressable_unless(is_active, ..)` | 197 (`let hover`), 239, 250, 251 | The shown tab keeps the pointer: its double-click renames. |
| `shell/pane_render.rs:840` | `pane_controls` | Share button | `Chip` | `pressable_unless(following, ..)` | 827 (`let hover`), 820, 829 | `button_frame` sets no hover, so there is no double hover. |
| `terminal/view.rs:849` | `render_find_bar` | find bar close X | `Raised` | enabled=true | 837 (`let hover`), 844, 847, 848 | The bar is `surface_raised`. Add `use crate::pressable::{Look, Pressable};`. `theme::opacity::ACTIVE` is no longer used there. |

**W elements (keep `.on_click(`; 12 sites):**

- `shell/detached_render.rs`: 212, 356, 364
- `shell/file_tabs_render.rs`: 174
- `shell/pane_render.rs`: 95, 131, 164, 172, 320, 410, 422, 803

`pane_render.rs:320` and `file_tabs_render.rs:174` are the tab X: `row_action`, `Look::Raised` (P1 Task 4) and moved to `widgets.rs` by Task 1; their `group_hover` reveal stays.
`pane_render.rs:803` (reading width) fills with `surface_raised` when on; the `Look::Icon` hover is `theme.hover`, so the hover stays visible (pinned by P1 Task 4's theme tests).

**EXEMPT in these files:** `pane_render.rs:266, 545, 867, 884, 933`, `file_tabs_render.rs:116, 123`, `detached_render.rs:182`, `focus.rs:408`, `terminal/view.rs:875, 1032`; the pane divider grip (`pane_render.rs:1154`) keeps its handlers and moves its `.hover(` to `grip_hover`.

- [ ] **Step 1: Re-run the inventory for this area and match it to the tables above.**

```bash
grep -nE '\.on_click\(|\.on_mouse_down\(|\.hover\(|\.active\(\||cursor_pointer' crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/file_tabs_render.rs crates/mkrew-ui/src/shell/detached_render.rs crates/mkrew-ui/src/terminal/view.rs
```

Every `.hover(`, `.active(|` and `.cursor_pointer()` line must belong to a row of the P table (its "lines to delete" column) or sit in a W widget.
A line that belongs to neither is an element added since the tables were written: classify it with the Look chooser and convert it too.

- [ ] **Step 2: Convert the tab chips (`pane_render.rs` `tab_chip`, `file_tabs_render.rs`).**
The shown tab is picked, not disabled: it keeps the pointer (its double-click renames) and takes no hover fill.
Delete `let hover = theme.hover;` (197 / 35), `.cursor_pointer()` (239 / 86) and the `.hover(..)` and `.active(..)` lines inside `.when(!is_active, ..)` (250-251 / 97-98).

Before (`pane_render.rs` 239-265):

```rust
            .cursor_pointer()
            .whitespace_nowrap()
            .text_size(font_size::BODY)
            .when(is_active, |el| {
                el.bg(rgb(theme.card))
                    .border_color(rgb(theme.selected))
                    .font_weight(FontWeight(weight::MEDIUM))
                    .text_color(rgb(theme.text))
            })
            .when(!is_active, |el| {
                el.text_color(rgb(theme.text_muted))
                    .hover(move |style| style.bg(rgb(hover)))
                    .active(|style| style.opacity(opacity::ACTIVE))
            })
            .on_click(
                cx.listener(move |shell, event: &gpui::ClickEvent, window, cx| {
                    select(shell);
                    shell.sync_panes(window, cx);
                    if event.click_count() == 2 {
                        match labeled {
                            Some(session) => shell.start_pane_rename(session, window, cx),
                            None => shell.start_rename(window, cx),
                        }
                    }
                    cx.notify();
                }),
            )
```

After:

```rust
            .whitespace_nowrap()
            .text_size(font_size::BODY)
            .when(is_active, |el| {
                el.bg(rgb(theme.card))
                    .border_color(rgb(theme.selected))
                    .font_weight(FontWeight(weight::MEDIUM))
                    .text_color(rgb(theme.text))
            })
            .when(!is_active, |el| el.text_color(rgb(theme.text_muted)))
            // The shown tab is picked, not disabled: the pointer for the double-click rename.
            .pressable_unless(is_active, Look::Chip, theme)
            .on_press(
                true,
                cx.listener(move |shell, event: &gpui::ClickEvent, window, cx| {
                    select(shell);
                    shell.sync_panes(window, cx);
                    if event.click_count() == 2 {
                        match labeled {
                            Some(session) => shell.start_pane_rename(session, window, cx),
                            None => shell.start_rename(window, cx),
                        }
                    }
                    cx.notify();
                }),
            )
```

The right-click (`on_mouse_down(MouseButton::Right, ..)` at 266) and the middle-click of `file_tabs_render.rs:116` stay after `on_press`.

- [ ] **Step 3: Convert the Share button (`pane_render.rs` 804-844).**
`button_frame` sets a pointer and no hover, so `pressable` adds the only hover.
Delete `let hover` (827 inside the `.when(!following, ..)` closure, and any other `let hover` feeding 829), `.active(..)` (820) and `.hover(..)` (829).

Before:

```rust
        .active(|style| style.opacity(opacity::ACTIVE))
        .when(following, |el| {
            el.bg(rgb(theme.card))
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text))
        })
        .when(!following, |el| {
            let hover = theme.hover;
            el.text_color(rgb(theme.text_muted))
                .hover(move |style| style.bg(rgb(hover)))
        })
```

After:

```rust
        .when(following, |el| {
            el.bg(rgb(theme.card))
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text))
        })
        .when(!following, |el| el.text_color(rgb(theme.text_muted)))
        // Following is picked, not disabled: it keeps the pointer (it opens the Share menu).
        .pressable_unless(following, Look::Chip, theme)
```

and turn its `.on_click(` (840) into `.on_press(true, ..)` (the closure body is unchanged).

- [ ] **Step 4: The terminal find bar X and the pane divider.**
`terminal/view.rs` (837-850): the find bar is `surface_raised`, so the X is `Look::Raised`.

Before:

```rust
        let hover = theme.border;
        let close = div()
            .id("find-close")
            .flex()
            .items_center()
            .px(px(space::XS))
            .rounded(px(radius::SMALL))
            .cursor_pointer()
            .text_size(font_size::META)
            .text_color(rgb(theme.text_muted))
            .hover(move |style| style.bg(rgb(hover)))
            .active(|style| style.opacity(theme::opacity::ACTIVE))
            .on_click(cx.listener(|pane, _, window, cx| pane.close_find(window, cx)))
            .child(icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted));
```

After:

```rust
        let close = div()
            .id("find-close")
            .flex()
            .items_center()
            .px(px(space::XS))
            .rounded(px(radius::SMALL))
            .text_size(font_size::META)
            .text_color(rgb(theme.text_muted))
            .pressable(Look::Raised, true, theme)
            .on_press(true, cx.listener(|pane, _, window, cx| pane.close_find(window, cx)))
            .child(icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted));
```

`pane_divider` (`pane_render.rs` 1125-1166): the grip keeps its drag and mouse-up handlers; only the hover moves.
Delete the `let hover = ..;` that feeds line 1154 (find it with `grep -n "let hover" crates/mkrew-ui/src/shell/pane_render.rs` inside `pane_divider`).

Before:

```rust
        .child(
            strip
                .id(("pane-divider", divider.index))
                .absolute()
                .cursor(divider_cursor(divider.axis))
                .hover(move |style| style.bg(rgb(hover)))
                .on_drag(divider, |_, _, _, cx| cx.new(|_| EmptyView))
```

After:

```rust
        .child(
            grip_hover(strip.id(("pane-divider", divider.index)), theme)
                .absolute()
                .cursor(divider_cursor(divider.axis))
                .on_drag(divider, |_, _, _, cx| cx.new(|_| EmptyView))
```


- [ ] **Step 5: Build, lint and test.** No new test: these are styling changes, and their behavior is pinned by P1 Task 4's `press_style` tests and Task 1's helper tests.

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: clean. An "unused variable: `hover`" or "unused import" is a leftover to delete.
Run: `cargo test -p mkrew-ui`
Expected: PASS.

- [ ] **Step 6: Visual check, dark then light.** Run the app on Windows and switch the theme with the title bar toggle.
Every view of this area must paint without a panic (a second `.hover(` on one element panics at paint).
- **Tab chips (dark + light):** hover an inactive tab in the main strip, in a detached window and a file tab: a fill and a pointer.
  The shown tab shows a pointer and no hover fill; double-click it: it still renames.
  Hover the X of an active and of an inactive tab chip: the X shows a fill on its chip.
  Press and hold a tab chip, then drag it: the chip does not stay dimmed after the drop.
- **Share and find bar (dark + light):** hover the Share button of a pane; with a followed share it shows a pointer and no fill and still opens its menu.
  Open the terminal find bar (Ctrl+F in a terminal) and hover its X: the fill is visible on the bar.
- **Reading width (dark + light):** with reading width on, hover its icon in the pane header: the hover is visible against the on fill.
- **Pane divider:** hover the divider between two panes: the tint shows; drag still resizes.
Capture one screenshot per theme with the pointer on a converted element into `docs/screenshots/2026-11-21-pressable-tabs-panes-dark.png` and `docs/screenshots/2026-11-21-pressable-tabs-panes-light.png` if you can capture automatically.
Append the bullets above to the section `### To verify - 2026-11-21 Pressable audit (P3)` of `docs/HUMAN_CHECKS.md`, each as a `- [ ] **<name> (dark + light):** ...` line with its sentences on separate lines.

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/file_tabs_render.rs crates/mkrew-ui/src/shell/detached_render.rs crates/mkrew-ui/src/terminal/view.rs docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): tab chips, Share and the find bar take their press look from widgets"
```

---

### Task 4: Board, Jira and the all-workspaces board

The all-workspaces board is the same `render_board` as the single-workspace board (`shell/board.rs:718-838`, scope in `board_scope`); it draws the same `task_card`s plus a workspace avatar row, and the scope switch is the `board_toggle("board-scope")` segment (`jira.rs:465`, W).
So the elements below cover both scopes.
`shell/board.rs`, `board.rs`, `board/tests.rs` and `jira_board.rs` have no handlers.

**Files:**
- Modify: `crates/mkrew-ui/src/shell/cards.rs`, `crates/mkrew-ui/src/shell/jira.rs`
- Modify: `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: `card_press`, `Look`, `Pressable::{pressable, on_press}` (Task 1); `FluentBuilder::map` from `gpui::prelude::*` (already imported by `use crate::*`).
- Produces: nothing later tasks use.

**P elements (5, convert these):**

| file:line | fn | element | Look | pattern | lines to delete | notes |
| --- | --- | --- | --- | --- | --- | --- |
| `shell/cards.rs:219` | `task_card` | task card (select, open terminal, drag source) | `card_press` | `card_press(card, selected, theme)` | 247-249, 250 | Keeps today's border-light hover (no fill change) so the Edit chip inside stays visible. |
| `shell/cards.rs:297` | `task_card` | card "Edit" chip | `Raised` | enabled=true | 280, 285-287 | Keep `group_hover` (284), the tooltip and `stop_propagation`. `edit_hover` stays: the `quiet_button`s still take it. |
| `shell/jira.rs:858` | `issue_row` | fold chevron on a List row | `Icon` | enabled=true | 846 (`let hover`), 851, 852 | Keep `stop_propagation`. |
| `shell/jira.rs:907` | `issue_row` | Jira List row (opens the issue) | `Row` | enabled=true (selected is a border only, so no `pressable_unless`) | 882 (`let row_hover`), 904, 906 | `theme.row_hover` is still used by `widgets.rs`. |
| `shell/jira.rs:978` | `issue_card` | Kanban issue card | `card_press` | `card_press(card, selected, theme)` | 952 (`let hover`), 974-977 |  |

**W elements (keep `.on_click(`; 13 sites):**

- `shell/cards.rs`: 433, 452, 478, 520, 575, 632, 788
- `shell/jira.rs`: 465, 559, 564, 601, 668, 693

**EXEMPT in these files:** `cards.rs:830`, `jira.rs:543, 657` (text fields); `cards.rs:151` (drop slot) and `cards.rs:222` (drag source) are not counted handlers.

The card hover stays a border light, not a fill, on purpose.
The Edit, Undo, Changes, Merge back and Run with chips inside a card hover to `theme.border`; if the card itself filled with a hover color while the pointer is on it (always, when the pointer is on a chip), those chip hovers would show nothing.
`card_press` also adds the press dim the other elements have.

- [ ] **Step 1: Re-run the inventory for this area and match it to the tables above.**

```bash
grep -nE '\.on_click\(|\.on_mouse_down\(|\.hover\(|\.active\(\||cursor_pointer' crates/mkrew-ui/src/shell/cards.rs crates/mkrew-ui/src/shell/jira.rs
```

Every `.hover(`, `.active(|` and `.cursor_pointer()` line must belong to a row of the P table (its "lines to delete" column) or sit in a W widget.
A line that belongs to neither is an element added since the tables were written: classify it with the Look chooser and convert it too.

- [ ] **Step 2: Convert the task card (`cards.rs` 217-251).**
`card_press` goes mid-chain with `.map(..)`; it replaces the `.when(!selected, hover)` and the `.cursor_pointer()` lines.
`edit_hover` stays declared: the `quiet_button`s at 424, 443, 474, 511 and the chips at 559, 617, 784 still take it.

Before:

```rust
        .border_color(rgb(if selected {
            theme.pane_focus_border
        } else {
            theme.surface_raised
        }))
        .when(!selected, |el| {
            el.hover(move |style| style.border_color(rgb(edit_hover)))
        })
        .cursor_pointer()
        .group(group.clone())
```

After:

```rust
        .border_color(rgb(if selected {
            theme.pane_focus_border
        } else {
            theme.surface_raised
        }))
        .map(|el| card_press(el, selected, theme))
        .group(group.clone())
```

The card's `.on_click(` (219) and `.on_drag(` (222) stay as they are: `card_press` is a widget helper, like a `button`.
Fallback, only if the visual step shows a card stuck dimmed after a drag: delete the `.active(..)` line of `card_press` in `widgets.rs` and say so in the commit message.

The Edit chip (273-305) is `Look::Raised` (it rests on the raised card; the card's own hover does not change its fill).
Delete `.cursor_pointer()` (280), the comment and `.hover(..)` (285-286) and `.active(..)` (287); keep `group_hover` (284), the tooltip and the `cx.stop_propagation()`.

Before:

```rust
                        .rounded(px(radius::SMALL))
                        .cursor_pointer()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .opacity(opacity::HIDDEN)
                        .group_hover(group.clone(), |style| style.opacity(opacity::FULL))
                        // One step above the card's own `surface_raised`.
                        .hover(move |style| style.bg(rgb(edit_hover)))
                        .active(|style| style.opacity(opacity::ACTIVE))
```

After:

```rust
                        .rounded(px(radius::SMALL))
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .opacity(opacity::HIDDEN)
                        .group_hover(group.clone(), |style| style.opacity(opacity::FULL))
                        .pressable(Look::Raised, true, theme)
```

and turn its `.on_click(` (297) into `.on_press(true, ..)` with the same closure.
`pressable(.., true, ..)` sets no base opacity, so the `HIDDEN` / `FULL` reveal still works.

- [ ] **Step 3: Convert Jira (`jira.rs`).**
The fold chevron (844-875) is `Look::Icon`, the List row (882-909) is `Look::Row`, the Kanban issue card (952-980) is `card_press`.
The List row's selected state is a border only and a selected row stays clickable, so no `pressable_unless`.

Before (fold, 846-852):

```rust
            let folded = key.clone();
            let hover = theme.hover;
            div()
                .id(("jira-fold", index))
                .flex_none()
                .rounded(px(radius::SMALL))
                .cursor_pointer()
                .hover(move |style| style.bg(rgb(hover)))
```

After:

```rust
            let folded = key.clone();
            div()
                .id(("jira-fold", index))
                .flex_none()
                .rounded(px(radius::SMALL))
                .pressable(Look::Icon, true, theme)
```

Its `.on_click(` (858) becomes `.on_press(true, ..)`; keep its `cx.stop_propagation()`.

Before (row, 882 and 903-909):

```rust
        let row_hover = theme.row_hover;
```
```rust
            .cursor_pointer()
            .text_color(rgb(theme.text))
            .hover(move |style| style.bg(rgb(row_hover)))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.open_issue(project_id, key.clone(), window, cx);
            }))
```

After (`theme.row_hover` is still used by `widgets.rs`, leave the token):

```rust
            .text_color(rgb(theme.text))
            .pressable(Look::Row, true, theme)
            .on_press(
                true,
                cx.listener(move |shell, _, window, cx| {
                    shell.open_issue(project_id, key.clone(), window, cx);
                }),
            )
```

`issue_card` (974-980): delete `let hover` (952), the `.when(!selected, hover)` block (974-976) and `.cursor_pointer()` (977), add `.map(|el| card_press(el, selected, theme))` in their place; its `.on_click(` (978) stays.


- [ ] **Step 4: Build, lint and test.** No new test: these are styling changes, and their behavior is pinned by P1 Task 4's `press_style` tests and Task 1's helper tests.

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: clean. An "unused variable: `hover`" or "unused import" is a leftover to delete.
Run: `cargo test -p mkrew-ui`
Expected: PASS.

- [ ] **Step 5: Visual check, dark then light.** Run the app on Windows and switch the theme with the title bar toggle.
Every view of this area must paint without a panic (a second `.hover(` on one element panics at paint).
- **Cards (dark + light, Board and the all-workspaces scope):** hover an unselected task card: its border lights and the pointer shows; the selected card shows no border change.
  Hover a card, then hover its Edit chip: the chip shows a fill that is visible on the card.
  Press a card and drag it to another column: the card does not stay dimmed after the drop.
  Hover the hover-only Undo, Changes and Run with chips of a card: each still shows its fill.
- **Jira (dark + light):** hover a List row (a fill) and its fold chevron (a fill that differs from the row's), and a Kanban issue card (border light).
Capture one screenshot per theme with the pointer on a converted element into `docs/screenshots/2026-11-21-pressable-board-jira-dark.png` and `docs/screenshots/2026-11-21-pressable-board-jira-light.png` if you can capture automatically.
Append the bullets above to the section `### To verify - 2026-11-21 Pressable audit (P3)` of `docs/HUMAN_CHECKS.md`, each as a `- [ ] **<name> (dark + light):** ...` line with its sentences on separate lines.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/shell/cards.rs crates/mkrew-ui/src/shell/jira.rs docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): board cards and Jira rows take their press look from widgets"
```

---

### Task 5: Files, Docs, Markdown preview and comments

**Files:**
- Modify: `crates/mkrew-ui/src/shell/files_render.rs`, `crates/mkrew-ui/src/shell/files_split.rs`, `crates/mkrew-ui/src/shell/docs.rs`, `crates/mkrew-ui/src/shell/comments.rs`, `crates/mkrew-ui/src/preview.rs`
- Read only (every handler is W or EXEMPT): `crates/mkrew-ui/src/shell/files_preview.rs`, `crates/mkrew-ui/src/field.rs`, `crates/mkrew-ui/src/shell/files.rs`, `crates/mkrew-ui/src/files_view.rs`
- Modify: `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: `Look`, `Pressable::{pressable, pressable_unless, on_press}`, `grip_hover` (Task 1).
  `preview.rs` is in the lib crate and has no `use crate::*`: add `use crate::pressable::{Look, Pressable};` next to its other `use crate::` lines.
- Produces: nothing later tasks use.

**P elements (11, convert these):**

| file:line | fn | element | Look | pattern | lines to delete | notes |
| --- | --- | --- | --- | --- | --- | --- |
| `preview.rs:687` | `list_item` | task checkbox (ticks the source) | `Icon` | enabled=true, on a new hit-target wrapper | 683-684, `.mt(px(space::XS))` of `check` (moves to the no-pick arm) | Lib crate: `use crate::pressable::{Look, Pressable};`. |
| `preview.rs:865` | `with_copy` | code block "Copy" chip | `Raised` | enabled=true | 837 (`(hover, fg)` tuple), 853, 856 | Keep `opacity(HIDDEN)` + `group_hover` (reveal). |
| `preview.rs:972` | `diagram_view` | diagram "Expand" chip | `Raised` | enabled=true | 923 (`(hover, text)` tuple), 960, 963 | Same shape as 865. |
| `preview.rs:1048` | `source` | source line in Docs Source (pick on mouse down) | `Row` | `pressable_unless(picked, ..)`; keeps `.on_mouse_down` | 1044 | The line needs `.id(("src-line", index))` and a cloned `theme` in the `move` closure. |
| `shell/comments.rs:670` | `comment_thread` | thread header anchor (picks the lines) | `Raised` | enabled=true, only inside `when_some(lines)` | 668 | Add `.px(px(space::XS)).rounded(px(radius::SMALL))` so the fill has padding. An anchor with no lines loses its pointer (it did nothing). |
| `shell/docs.rs:517` | `render_doc` | doc block in Preview (pick lines on mouse down) | `Row` | `pressable_unless(picked, ..)`; keeps `.on_mouse_down` (Shift extends) | 502 (`hover_bg` in the tuple), 514, 516 | The `wrap` closure is `move`: clone `theme` into it first. |
| `shell/docs.rs:619` | `mode_toggle` | Preview/Source/Split segment (inactive one) | `Chip` | `pressable_unless(active, ..)`; click only when inactive | 591 (`let hover`), 616-618 | The `segment` widget does not fit (height, fill). |
| `shell/docs.rs:727` | `doc_row` | doc list row | `Row` | `pressable_unless(selected, ..)` | 653 (`let hover`), 684, 686, 687 |  |
| `shell/files_render.rs:78` | `render_file_tree` | "Search files..." box (opens quick open) | `Chip` | enabled=true | 75, 76 | Built on `search_box(..).id(..)`; it has no hover today. |
| `shell/files_render.rs:95` | `render_file_tree` | "Ignored" toggle chip | `Chip` | `pressable_unless(showing_ignored, ..)` | 85, 90 (`let hover`), 92 | Built on `button_frame`, which sets no hover. |
| `shell/files_render.rs:1078` | `file_row` | file tree row | `Row` | `pressable_unless(at_cursor \|\| opened, ..)` | 968 (`let hover`), 1002, 1005-1007, 1012 | `on_press` stays unconditional. Right-click (1088) and `on_drag` (1062) stay. |

**W elements (keep `.on_click(`; 26 sites):**

- `shell/comments.rs`: 389, 434, 505, 610, 624, 641
- `shell/docs.rs`: 231, 252, 430, 446
- `shell/files_preview.rs`: 197
- `shell/files_render.rs`: 113, 131, 394, 446, 468, 489, 521, 570, 578, 586, 637, 645, 670, 729, 744

**EXEMPT in these files:** `files_render.rs:180, 234, 555, 623, 844, 889, 1088`, `files_preview.rs:52, 343`, `files_split.rs:154` (its grip `.hover(` at 133 moves to `grip_hover`), `docs.rs:295`, `comments.rs:745`, `preview.rs:247, 254, 686, 864, 971`, `field.rs:120`.
A Markdown link (`preview.rs:254`, `InteractiveText`) has its underline as a text range, so `Look::Text` cannot apply: it keeps its current look.

- [ ] **Step 1: Re-run the inventory for this area and match it to the tables above.**

```bash
grep -nE '\.on_click\(|\.on_mouse_down\(|\.hover\(|\.active\(\||cursor_pointer' crates/mkrew-ui/src/shell/files_render.rs crates/mkrew-ui/src/shell/files_preview.rs crates/mkrew-ui/src/shell/files_split.rs crates/mkrew-ui/src/shell/docs.rs crates/mkrew-ui/src/shell/comments.rs crates/mkrew-ui/src/preview.rs crates/mkrew-ui/src/field.rs
```

Every `.hover(`, `.active(|` and `.cursor_pointer()` line must belong to a row of the P table (its "lines to delete" column) or sit in a W widget.
A line that belongs to neither is an element added since the tables were written: classify it with the Look chooser and convert it too.

- [ ] **Step 2: Convert the file tree row (`files_render.rs` 968-1012, 1078).**
A row at the cursor or opened is picked: pointer, no hover fill.
Delete `let hover = theme.card;` (968), `.cursor_pointer()` (1002), the `.when(!at_cursor && !opened, hover)` block (1005-1007) and `.active(..)` (1012).

Before:

```rust
        .cursor_pointer()
        .text_size(font_size::BODY)
        .when(at_cursor || opened, |el| el.bg(rgb(theme.selected)))
        .when(!at_cursor && !opened, |el| {
            el.hover(move |style| style.bg(rgb(hover)))
        })
        .when(at_cursor, |el| {
            el.relative()
                .child(record_bounds(Anchor::FileRow(entry.path.clone()), cx))
        })
        .active(|style| style.opacity(opacity::ACTIVE))
```

After:

```rust
        .text_size(font_size::BODY)
        .when(at_cursor || opened, |el| el.bg(rgb(theme.selected)))
        .pressable_unless(at_cursor || opened, Look::Row, theme)
        .when(at_cursor, |el| {
            el.relative()
                .child(record_bounds(Anchor::FileRow(entry.path.clone()), cx))
        })
```

and turn `.on_click(` (1078) into `.on_press(true, ..)` with the same closure (the right-click at 1088 and `.on_drag(` at 1062 stay).
Hover fill goes from `theme.card` to `theme.hover`.

- [ ] **Step 3: Convert the tree header (`files_render.rs` 59-98).**
The search box looks like an input but is a button: `Look::Chip`.
The Ignored chip is built on `button_frame`; when on it is picked.

Before:

```rust
        .id("files-search")
        .flex_1()
        .min_w_0()
        .cursor_pointer()
        .active(|style| style.opacity(opacity::ACTIVE))
        .tooltip(tooltip(theme, "Find a file of this workspace", search_keys))
        .on_click(cx.listener(|shell, _, window, cx| shell.open_quick_open(window, cx)));
        let ignored = button_frame("files-ignored", "Ignored")
            .flex_none()
            .h(theme::rems_px(theme::INPUT_HEIGHT))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .text_size(font_size::META)
            .active(|style| style.opacity(opacity::ACTIVE))
            .when(showing_ignored, |el| {
                el.bg(rgb(theme.selected)).text_color(rgb(theme.text))
            })
            .when(!showing_ignored, |el| {
                let hover = theme.hover;
                el.text_color(rgb(theme.text_muted))
                    .hover(move |style| style.bg(rgb(hover)))
            })
            .tooltip(tooltip(theme, ignored_tip, ignored_keys.clone()))
            .on_click(cx.listener(move |shell, _, window, cx| {
```

After:

```rust
        .id("files-search")
        .flex_1()
        .min_w_0()
        .pressable(Look::Chip, true, theme)
        .tooltip(tooltip(theme, "Find a file of this workspace", search_keys))
        .on_press(
            true,
            cx.listener(|shell, _, window, cx| shell.open_quick_open(window, cx)),
        );
        let ignored = button_frame("files-ignored", "Ignored")
            .flex_none()
            .h(theme::rems_px(theme::INPUT_HEIGHT))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .text_size(font_size::META)
            .when(showing_ignored, |el| {
                el.bg(rgb(theme.selected)).text_color(rgb(theme.text))
            })
            .when(!showing_ignored, |el| el.text_color(rgb(theme.text_muted)))
            .pressable_unless(showing_ignored, Look::Chip, theme)
            .tooltip(tooltip(theme, ignored_tip, ignored_keys.clone()))
            .on_press(true, cx.listener(move |shell, _, window, cx| {
```

(the closure body and its closing `}));` are unchanged; run `cargo fmt` on the file afterwards.)
`open_quick_open` is P2's to rename; keep whatever name exists when you run this task.

- [ ] **Step 4: Convert Docs (`docs.rs`).**
The mode toggle segment (591-622) is `Look::Chip`, only on the inactive segment.

Before:

```rust
        let hover = theme.surface_raised;
```
```rust
                    .when(!active, |el| {
                        el.cursor_pointer()
                            .hover(move |style| style.bg(rgb(hover)))
                            .active(|style| style.opacity(opacity::ACTIVE))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                switch(shell, target, window, cx);
                            }))
                    })
```

After (and delete `let hover` at 591):

```rust
                    .when(!active, |el| {
                        el.pressable(Look::Chip, true, theme).on_press(
                            true,
                            cx.listener(move |shell, _, window, cx| {
                                switch(shell, target, window, cx);
                            }),
                        )
                    })
```

The doc block (497-537) picks lines on mouse down (Shift extends), so it keeps `.on_mouse_down(` and takes only the look.
The `wrap` closure is `move` and `'static`: give it its own copy of the theme.

Before:

```rust
                    let (picked_bg, hover_bg) = (theme.terminal_selection_bg, theme.surface_raised);
                    let wrap: preview::Wrap = Rc::new(
                        move |index: usize,
                              block: &mkrew_ui::markdown::Block,
                              element: gpui::AnyElement| {
                            let (start, end) = (block.lines.start, block.lines.end);
                            let picked = selection.is_some_and(|s| s.contains(start, end));
                            let this = this.clone();
                            div()
                                .id(("doc-block", index))
                                .px(px(space::XS))
                                .rounded(px(radius::SMALL))
                                .cursor_pointer()
                                .when(picked, |el| el.bg(rgb(picked_bg)))
                                .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover_bg))))
                                .on_mouse_down(
```

After:

```rust
                    let picked_bg = theme.terminal_selection_bg;
                    let row_theme = theme.clone();
                    let wrap: preview::Wrap = Rc::new(
                        move |index: usize,
                              block: &mkrew_ui::markdown::Block,
                              element: gpui::AnyElement| {
                            let (start, end) = (block.lines.start, block.lines.end);
                            let picked = selection.is_some_and(|s| s.contains(start, end));
                            let this = this.clone();
                            div()
                                .id(("doc-block", index))
                                .px(px(space::XS))
                                .rounded(px(radius::SMALL))
                                .when(picked, |el| el.bg(rgb(picked_bg)))
                                .pressable_unless(picked, Look::Row, &row_theme)
                                .on_mouse_down(
```

The doc list row (653, 684-687) is `Look::Row` with `pressable_unless(selected, ..)`; the same edit as the file tree row.

- [ ] **Step 5: Convert the Markdown preview (`preview.rs`).**
The task checkbox (677-694) gets a hit-target wrapper so the hover fill never covers the accent fill of a ticked box.
Delete `.mt(px(space::XS))` from `check` (about line 661); the wrapper's `p(XS)` gives the same offset.

Before:

```rust
            match pick {
                Some(_) => {
                    let (line, accent) = (item.lines.start, theme.accent);
                    check
                        .id(("task-box", line as usize))
                        .cursor_pointer()
                        .hover(move |style| style.border_color(rgb(accent)))
                        // Not a press on the page (text selection).
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            window.dispatch_action(Box::new(ToggleTask(line)), cx);
                        })
                        .into_any_element()
                }
                None => check.into_any_element(),
            }
```

After:

```rust
            match pick {
                Some(_) => {
                    let line = item.lines.start;
                    // The hit target is the box plus XS around it: the hover fill never covers
                    // the accent fill of a ticked box.
                    div()
                        .id(("task-box", line as usize))
                        .p(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .pressable(Look::Icon, true, theme)
                        // Not a press on the page (text selection).
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_press(true, move |_, window, cx| {
                            cx.stop_propagation();
                            window.dispatch_action(Box::new(ToggleTask(line)), cx);
                        })
                        .child(check)
                        .into_any_element()
                }
                None => check.mt(px(space::XS)).into_any_element(),
            }
```

The Copy chip of a code block (`with_copy`, 837-865) and the Expand chip of a diagram (`diagram_view`, 923-972) rest on a raised code block or card: `Look::Raised`, keeping their `HIDDEN` / `FULL` reveal.
Delete the `(hover, fg)` tuple (837) and `.cursor_pointer()` (853) and `.hover(..)` (856).

Before:

```rust
    let (hover, fg) = (theme.hover, theme.text);
```
```rust
            .text_color(rgb(theme.text_muted))
            .cursor_pointer()
            .opacity(theme::opacity::HIDDEN)
            .group_hover(group, |style| style.opacity(theme::opacity::FULL))
            .hover(move |style| style.bg(rgb(hover)).text_color(rgb(fg)))
```
```rust
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                window.dispatch_action(Box::new(CopyCode(copied.clone())), cx);
            }),
```

After:

```rust
            .text_color(rgb(theme.text_muted))
            .opacity(theme::opacity::HIDDEN)
            .group_hover(group, |style| style.opacity(theme::opacity::FULL))
            .pressable(Look::Raised, true, theme)
```
```rust
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_press(true, move |_, window, cx| {
                cx.stop_propagation();
                window.dispatch_action(Box::new(CopyCode(copied.clone())), cx);
            }),
```

`diagram_view` takes the same edit (delete the `(hover, text)` tuple at 923, lines 960 and 963; its `.on_click(` at 972 becomes `.on_press(true, ..)`).
The source line (1038-1053) needs an id to be pressable and a cloned theme inside the `move` closure; it keeps its mouse-down.

Before:

```rust
    let (text, muted, selection_bg) = (theme.text, theme.text_muted, theme.terminal_selection_bg);
    let line_height = theme.terminal_font_size * theme.code_line_height;
    uniform_list(id, total, move |range, _, _| {
        range
            .map(|index| {
                let line = shown.lines.get(index).map(String::as_str).unwrap_or("");
                let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
                let on_line = Rc::clone(&on_line);
                div()
                    .w_full()
                    .h(px(line_height))
                    .flex()
                    .gap(px(space::L))
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .when(selected.is_some_and(|s| s.contains(number, number)), |el| {
                        el.bg(rgb(selection_bg))
                    })
```

After:

```rust
    let (text, muted, selection_bg) = (theme.text, theme.text_muted, theme.terminal_selection_bg);
    let line_height = theme.terminal_font_size * theme.code_line_height;
    let row_theme = theme.clone();
    uniform_list(id, total, move |range, _, _| {
        range
            .map(|index| {
                let line = shown.lines.get(index).map(String::as_str).unwrap_or("");
                let number = u32::try_from(index + 1).unwrap_or(u32::MAX);
                let on_line = Rc::clone(&on_line);
                let picked = selected.is_some_and(|s| s.contains(number, number));
                div()
                    .id(("src-line", index))
                    .w_full()
                    .h(px(line_height))
                    .flex()
                    .gap(px(space::L))
                    .whitespace_nowrap()
                    .when(picked, |el| el.bg(rgb(selection_bg)))
                    .pressable_unless(picked, Look::Row, &row_theme)
```

- [ ] **Step 6: Comments and the Markdown split grip.**
The thread header anchor (`comments.rs` 662-673) is clickable only when it has lines; with no lines it must show no pointer (it did nothing).
It rests on a raised comment card: `Look::Raised`, with padding so the fill has room.

Before:

```rust
                    .text_size(font_size::META)
                    .cursor_pointer()
                    .when_some(lines, |el, (start, end)| {
                        el.on_click(cx.listener(move |shell, _, _, cx| {
                            shell.pick_lines(project_id, context, start, end, false, cx);
                        }))
                    })
```

After:

```rust
                    .px(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .text_size(font_size::META)
                    .when_some(lines, |el, (start, end)| {
                        el.pressable(Look::Raised, true, theme).on_press(
                            true,
                            cx.listener(move |shell, _, _, cx| {
                                shell.pick_lines(project_id, context, start, end, false, cx);
                            }),
                        )
                    })
```

`files_split.rs` (124-133): the grip keeps its handlers; its hover moves to `grip_hover`.

Before:

```rust
        let divider = theme.divider;
        let grip = div()
            .id("md-split-grip")
            .flex_none()
            .w(px(theme::GRIP_WIDTH))
            .h_full()
            .flex()
            .justify_center()
            .cursor(CursorStyle::ResizeLeftRight)
            .hover(move |style| style.bg(rgb(divider)))
            .child(div().w(px(theme::BORDER_WIDTH)).h_full().bg(rgb(divider)))
```

After (`divider` is still used by the child line):

```rust
        let divider = theme.divider;
        let grip = grip_hover(div().id("md-split-grip"), theme)
            .flex_none()
            .w(px(theme::GRIP_WIDTH))
            .h_full()
            .flex()
            .justify_center()
            .cursor(CursorStyle::ResizeLeftRight)
            .child(div().w(px(theme::BORDER_WIDTH)).h_full().bg(rgb(divider)))
```


- [ ] **Step 7: Build, lint and test.** No new test: these are styling changes, and their behavior is pinned by P1 Task 4's `press_style` tests and Task 1's helper tests.

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: clean. An "unused variable: `hover`" or "unused import" is a leftover to delete.
Run: `cargo test -p mkrew-ui`
Expected: PASS.

- [ ] **Step 8: Visual check, dark then light.** Run the app on Windows and switch the theme with the title bar toggle.
Every view of this area must paint without a panic (a second `.hover(` on one element panics at paint).
- **Files (dark + light):** hover a file tree row: a fill and a pointer; the row at the cursor and the opened row show a pointer and no fill, and a click on either still opens it.
  Hover the "Search files..." box and the Ignored chip (fill); turn Ignored on: it shows the selected fill and a pointer.
- **Docs (dark + light):** hover a doc list row, the inactive Preview / Source / Split segment, and a block in Preview: each shows a fill; click a block, Shift+click another: both lines are picked as before.
  Hover a source line in Docs Source: a fill; click and Shift+click still pick.
- **Preview extras (dark + light):** hover the Copy chip of a code block and the Expand chip of a diagram: each shows a fill on its block.
  In the Files preview hover a task checkbox: a fill around the box, and a ticked box keeps its accent fill; click ticks the source.
- **Comments and split:** hover a comment thread header with lines (fill, click picks them) and one without lines (no pointer).
  Hover the Markdown split grip: the tint shows; drag and double-click still work.
Capture one screenshot per theme with the pointer on a converted element into `docs/screenshots/2026-11-21-pressable-files-docs-dark.png` and `docs/screenshots/2026-11-21-pressable-files-docs-light.png` if you can capture automatically.
Append the bullets above to the section `### To verify - 2026-11-21 Pressable audit (P3)` of `docs/HUMAN_CHECKS.md`, each as a `- [ ] **<name> (dark + light):** ...` line with its sentences on separate lines.

- [ ] **Step 9: Commit**

```bash
git add crates/mkrew-ui/src/shell/files_render.rs crates/mkrew-ui/src/shell/files_split.rs crates/mkrew-ui/src/shell/docs.rs crates/mkrew-ui/src/shell/comments.rs crates/mkrew-ui/src/preview.rs docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): files, docs, preview and comments take their press look from widgets"
```

---

### Task 6: Git

**Files:**
- Modify: `crates/mkrew-ui/src/shell/git_render.rs`, `crates/mkrew-ui/src/shell/git_diff.rs`, `crates/mkrew-ui/src/shell/git_menus.rs`, `crates/mkrew-ui/src/shell/git_rows.rs`
- Read only (every handler is W): `crates/mkrew-ui/src/shell/git_dialogs.rs`; `shell/git.rs`, `shell/git_viewed.rs` and `src/git_view/` have no handlers.
- Modify: `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: `Look`, `Pressable::{pressable, pressable_unless, on_press}`, `pick_surface`, `row_action` (Task 1).
- Produces: nothing later tasks use.

**P elements (6, convert these):**

| file:line | fn | element | Look | pattern | lines to delete | notes |
| --- | --- | --- | --- | --- | --- | --- |
| `shell/git_menus.rs:102` | `git_repo_strip` | Repo tab (inactive) | `Text` | `pressable_unless(active, ..)` | 67 (`let hover`), 98, 100, 101 | `Text`, not `Row`: today the hover only turns the label to full text colour. The active tab (accent `border_b`) has no handler, so the underline never doubles the bar. |
| `shell/git_render.rs:630` | `chip_row` | "Showing" chip (`chip_row`, shared with ask_dialog) | `Chip` | `pressable_unless(active, ..)`; click only when inactive | 599 (`let hover`), 626, 628, 629 |  |
| `shell/git_rows.rs:293` | `git_file_row` | "viewed" check box on a file row | `Icon` | enabled=true, on a new wrapper | 283 | `viewed_box` stays a pure visual (git_diff uses it too). Keep `stop_propagation`. |
| `shell/git_rows.rs:333` | `git_file_row` | changed file row | `Row` | `pressable_unless(picked, ..)` | 209 (`let hover`), 222, 228, 229 | Hint (160) and Note (180) rows share `row_frame` and stay hover-free. |
| `shell/git_rows.rs:366` | `group_header` | group header (fold) | `Row` | enabled=true | 352 (`let hover`), 355, 356 | Gains a press look (it had none). |
| `shell/git_rows.rs:477` | `git_commit_row` | History commit row | `Row` | `pressable_unless(picked, ..)` | 425 (`let hover`), 439, 445, 446 |  |

**W elements (keep `.on_click(`; 43 sites; 4 of them are `row_action`, which is `Look::Raised` (Task 1 moved it to `widgets.rs`), and 1 is `hunk_stage_button`, built on it):**

- `shell/git_dialogs.rs`: 309, 368, 376, 479, 494, 504
- `shell/git_diff.rs`: 65, 88, 108, 199, 223, 343, 375, 393, 409, 438, 483, 505, 728, 738, 818
- `shell/git_menus.rs`: 129, 450
- `shell/git_render.rs`: 49, 66, 89, 101, 110, 129, 344, 398, 434, 509, 521, 705, 733, 746, 807, 819, 870, 896
- `shell/git_rows.rs`: 324, 397

**EXEMPT in these files:** `git_render.rs:936`, `git_menus.rs:223, 349` (text fields) and `git_diff.rs:1211` (the diff line pick: a mouse down that reads Shift, whose hover preview moves into `pick_surface`).
The inert Fetch icons at `git_render.rs:335, 338` and the hand-built disabled Push at `git_render.rs:375` have no handler and are not part of this audit.

- [ ] **Step 1: Re-run the inventory for this area and match it to the tables above.**

```bash
grep -nE '\.on_click\(|\.on_mouse_down\(|\.hover\(|\.active\(\||cursor_pointer' crates/mkrew-ui/src/shell/git_render.rs crates/mkrew-ui/src/shell/git_diff.rs crates/mkrew-ui/src/shell/git_dialogs.rs crates/mkrew-ui/src/shell/git_menus.rs crates/mkrew-ui/src/shell/git_rows.rs
```

Every `.hover(`, `.active(|` and `.cursor_pointer()` line must belong to a row of the P table (its "lines to delete" column) or sit in a W widget.
A line that belongs to neither is an element added since the tables were written: classify it with the Look chooser and convert it too.

- [ ] **Step 2: Rename the local `list_row` of `git_rows.rs`.**
`git_rows.rs:109` defines a private `fn list_row(id: SharedString)`, the plain frame of every row.
P1 adds `widgets::list_row(id, selected, theme)`; the local one shadows it, which compiles and confuses the next reader.
It is also the frame of the non-clickable Hint (160) and Note (180) rows, which must stay without a look.
Rename the definition (109) and its five uses in this file (160, 180, 212, 353, 438) to `row_frame`:

```rust
/// The box every row of the changed-file list shares (uniform list: rows are sized from the
/// first one).
fn row_frame(id: SharedString) -> gpui::Stateful<gpui::Div> {
```

Run `grep -n "list_row" crates/mkrew-ui/src/shell/git_rows.rs`: it must print nothing.

- [ ] **Step 3: Convert the rows (`git_rows.rs`).**
Picked rows (a changed file, a History commit) keep a pointer and their fill; the group header is never picked.
Example, the History commit row (425, 438-447, 477-479):

Before:

```rust
    let hover = theme.hover;
```
```rust
    list_row(SharedString::from(format!("git-commit-{}", commit.hash)))
        .cursor_pointer()
        .text_size(font_size::BODY)
        .when(picked, |el| {
            el.bg(rgb(theme.hover))
                .when(focused, |el| el.border_color(rgb(theme.accent)))
        })
        .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover))))
        .active(|style| style.opacity(opacity::ACTIVE))
        .tooltip(tooltip(theme, &tip, ""))
```
```rust
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.select_git_commit(project_id, hash.clone(), window, cx);
        }))
```

After (and delete `let hover` at 425):

```rust
    row_frame(SharedString::from(format!("git-commit-{}", commit.hash)))
        .text_size(font_size::BODY)
        .when(picked, |el| {
            el.bg(rgb(theme.hover))
                .when(focused, |el| el.border_color(rgb(theme.accent)))
        })
        .pressable_unless(picked, Look::Row, theme)
        .tooltip(tooltip(theme, &tip, ""))
```
```rust
        .on_press(
            true,
            cx.listener(move |shell, _, window, cx| {
                shell.select_git_commit(project_id, hash.clone(), window, cx);
            }),
        )
```

The picked fill (`theme.hover`) now equals the hover fill (`Look::Row` hovers with `theme.hover`), which is today's behavior too; a picked row simply no longer changes on hover.
The changed-file row (209-229, 333) takes the same edit (`pressable_unless(picked, Look::Row, theme)`, delete 209, 222, 228, 229).
The group header (352-356, 366) is `.pressable(Look::Row, true, theme)` with `.group(id.clone())` kept for its "Stage all" reveal; delete 352, 355, 356; it gains a press look.

The "viewed" check box (278-300, 293) has a pointer and no hover today and has an accent fill when on, so the look goes on a wrapper and `viewed_box` stays a pure visual (`git_diff.rs:423` uses it too).

Before:

```rust
            el.child(
                viewed_box(viewed, theme)
                    .id(SharedString::from(format!("{id}-viewed")))
                    .cursor_pointer()
                    .tooltip(tooltip(
                        theme,
                        if viewed {
                            "Mark as not viewed"
                        } else {
                            "Mark as viewed"
                        },
                        "",
                    ))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        // Not a click on the row (which would select it).
                        cx.stop_propagation();
                        window.focus(&shell.git_focus, cx);
                        shell.toggle_git_viewed(project_id, target.clone(), window, cx);
                    })),
            )
```

After:

```rust
            el.child(
                div()
                    .id(SharedString::from(format!("{id}-viewed")))
                    .flex_none()
                    .p(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .pressable(Look::Icon, true, theme)
                    .tooltip(tooltip(
                        theme,
                        if viewed {
                            "Mark as not viewed"
                        } else {
                            "Mark as viewed"
                        },
                        "",
                    ))
                    .on_press(
                        true,
                        cx.listener(move |shell, _, window, cx| {
                            // Not a click on the row (which would select it).
                            cx.stop_propagation();
                            window.focus(&shell.git_focus, cx);
                            shell.toggle_git_viewed(project_id, target.clone(), window, cx);
                        }),
                    )
                    .child(viewed_box(viewed, theme)),
            )
```

The wrapper adds 8 px of width; the row height (34 px) does not change.

- [ ] **Step 4: Convert the chips and the repo tab.**
The chip row (`git_render.rs` 599-636) is shared with `ask_dialog.rs` (agent and web chips): the active chip is picked and has no handler today.

Before:

```rust
        let hover = theme.surface;
```
```rust
                .when(!active, |el| {
                    el.cursor_pointer()
                        .text_color(rgb(theme.text_muted))
                        .hover(move |style| style.bg(rgb(hover)))
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            on_pick(shell, pick.clone(), window, cx);
                        }))
                })
```

After (and delete `let hover` at 599):

```rust
                .when(!active, |el| {
                    el.text_color(rgb(theme.text_muted))
                        .pressable(Look::Chip, true, theme)
                        .on_press(
                            true,
                            cx.listener(move |shell, _, window, cx| {
                                on_pick(shell, pick.clone(), window, cx);
                            }),
                        )
                })
```

The Repo tab (`git_menus.rs` 67, 97-106) hovers by text color today: `Look::Text`, only on the inactive tab (the active tab has an accent bar and no handler).

Before:

```rust
                    .when(!active, |el| {
                        el.cursor_pointer()
                            .text_color(rgb(theme.text_muted))
                            .hover(move |style| style.text_color(rgb(hover)))
                            .active(|style| style.opacity(opacity::ACTIVE))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                window.focus(&shell.git_focus, cx);
                                shell.set_git_repo(project_id, repo.clone(), window, cx);
                            }))
                    })
```

After (and delete `let hover = theme.text;` at 67):

```rust
                    .when(!active, |el| {
                        el.text_color(rgb(theme.text_muted))
                            .pressable(Look::Text, true, theme)
                            .on_press(
                                true,
                                cx.listener(move |shell, _, window, cx| {
                                    window.focus(&shell.git_focus, cx);
                                    shell.set_git_repo(project_id, repo.clone(), window, cx);
                                }),
                            )
                    })
```

- [ ] **Step 5: Move the diff line pick's hover into `pick_surface` (`git_diff.rs` 1199-1217).**

Before:

```rust
    el.cursor_pointer()
        .when(!picked.is_some_and(|p| p.contains(number, number)), |el| {
            el.hover(move |style| style.border_color(rgb(border)))
        })
        .on_mouse_down(
```

After:

```rust
    pick_surface(
        el,
        picked.is_some_and(|p| p.contains(number, number)),
        border,
    )
    .on_mouse_down(
```

The mouse-down handler (it reads `event.modifiers.shift` at the press) stays unchanged.


- [ ] **Step 6: Build, lint and test.** No new test: these are styling changes, and their behavior is pinned by P1 Task 4's `press_style` tests and Task 1's helper tests.

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: clean. An "unused variable: `hover`" or "unused import" is a leftover to delete.
Run: `cargo test -p mkrew-ui`
Expected: PASS.

- [ ] **Step 7: Visual check, dark then light.** Run the app on Windows and switch the theme with the title bar toggle.
Every view of this area must paint without a panic (a second `.hover(` on one element panics at paint).
- **Rows (dark + light):** hover an unpicked changed file row, a History commit row and a group header: a fill and a pointer; the picked row keeps its fill and a pointer, and a click on it still selects.
  Hover a Hint row ("nothing staged") and a commit Note row: no pointer, no hover.
- **Row actions (dark + light):** on a hovered file row hover the stage icon, the "viewed" check box and "Stage all" on a group header: each shows its own fill on the hovered row.
  Click the stage icon: the file moves and the row under it was not selected by the click.
- **Chips and tabs (dark + light):** hover an inactive "Showing" chip, a Repo tab (text turns full color, with an underline) and the Ask dialog's agent chips (they share `chip_row`); the active chip and tab show no look.
- **Diff pick (dark + light):** hover a diff line: the border preview shows; mouse down and Shift+mouse down still pick a range.
Capture one screenshot per theme with the pointer on a converted element into `docs/screenshots/2026-11-21-pressable-git-dark.png` and `docs/screenshots/2026-11-21-pressable-git-light.png` if you can capture automatically.
Append the bullets above to the section `### To verify - 2026-11-21 Pressable audit (P3)` of `docs/HUMAN_CHECKS.md`, each as a `- [ ] **<name> (dark + light):** ...` line with its sentences on separate lines.

- [ ] **Step 8: Commit**

```bash
git add crates/mkrew-ui/src/shell/git_render.rs crates/mkrew-ui/src/shell/git_diff.rs crates/mkrew-ui/src/shell/git_menus.rs crates/mkrew-ui/src/shell/git_rows.rs docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): git rows, chips and tabs take their press look from widgets"
```

---

### Task 7: Inspector, thread, composer, pickers and history

**Files:**
- Modify: `crates/mkrew-ui/src/shell/thread_panel.rs`, `crates/mkrew-ui/src/shell/composer.rs`, `crates/mkrew-ui/src/shell/slash.rs`, `crates/mkrew-ui/src/shell/history.rs`
- Read only (every handler is W or EXEMPT): `crates/mkrew-ui/src/shell/inspector.rs`, `crates/mkrew-ui/src/shell/peek.rs`, `crates/mkrew-ui/src/shell/path_field.rs`
- Modify: `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: `Look`, `Pressable::{pressable, pressable_unless, on_press}` (Task 1 and P1).
- Produces: nothing later tasks use.

**P elements (8, convert these):**

| file:line | fn | element | Look | pattern | lines to delete | notes |
| --- | --- | --- | --- | --- | --- | --- |
| `shell/composer.rs:568` | `render_prompt_picker` | prompt picker row | `Row` | `pressable_unless(picked, ..)` inside `frame` | 534 (`let hover`), 556, 557, 558 | One edit inside the `frame` closure covers 568 and 596. |
| `shell/composer.rs:596` | `render_prompt_picker` | "Manage prompts..." row | `Row` | same `frame` | (same lines) |  |
| `shell/composer.rs:835` | `composer_panel` | queued message X | `Icon` | enabled=true | 794 (`let hover`), 830, 832, 833 | Keep `stop_propagation`: the panel has a mouse-down at 851. |
| `shell/history.rs:605` | `render_turns_dialog` | turns dialog row (selects it) | `Row` | `pressable_unless(selected, ..)` | 528 (`let hover`), 604 | Gains a pointer and a press look (it had neither). |
| `shell/history.rs:725` | `render_history` | history list row | `Row` | `pressable_unless(selected, ..)` | 698 (`let hover`), 719, 722, 723 |  |
| `shell/slash.rs:287` | `render_slash_picker` | slash command row | `Row` | `pressable_unless(picked, ..)` | 235 (`let hover`), 284, 285, 286 |  |
| `shell/thread_panel.rs:155` | `render_thread_section` | "Earlier turns: Open transcript" line | `Text` | enabled=true | 154 | `meta_line(..)` is a `Div`; `.id(..)` makes it `Stateful`. |
| `shell/thread_panel.rs:194` | `render_thread_section` | "+3 tool calls" disclosure | `Text` | enabled=true | 185 |  |

**W elements (keep `.on_click(`; 22 sites):**

- `shell/composer.rs`: 686, 917, 949
- `shell/history.rs`: 572, 587, 667, 825, 869
- `shell/inspector.rs`: 228, 247, 321, 452, 527, 538, 550, 593, 616
- `shell/path_field.rs`: 192, 206
- `shell/peek.rs`: 521
- `shell/slash.rs`: 371, 408

`inspector.rs:452` is W through `row_action` (`Look::Raised`, moved to `widgets.rs` by Task 1).
`inspector.rs:228` (the right panel toggle) fills with `surface_raised` while open; the `Look::Icon` hover is `theme.hover`, which differs (pinned by P1 Task 4's theme tests), so no edit is needed.
`inspector.rs:527, 538` call `quiet_button(.., theme.hover)`: the fourth argument must keep working (P1 keeps the signature).
The inspector grip (`inspector.rs:269-287`) has no hover and stays.

**EXEMPT in these files:** `composer.rs:851` (panel-wide mouse down that focuses the composer), `peek.rs:546` (text field); `peek.rs:590` (mouse down out) and `peek.rs:648` (hover listener) are behavior, not styles.

- [ ] **Step 1: Re-run the inventory for this area and match it to the tables above.**

```bash
grep -nE '\.on_click\(|\.on_mouse_down\(|\.hover\(|\.active\(\||cursor_pointer' crates/mkrew-ui/src/shell/inspector.rs crates/mkrew-ui/src/shell/thread_panel.rs crates/mkrew-ui/src/shell/composer.rs crates/mkrew-ui/src/shell/slash.rs crates/mkrew-ui/src/shell/peek.rs crates/mkrew-ui/src/shell/history.rs crates/mkrew-ui/src/shell/path_field.rs
```

Every `.hover(`, `.active(|` and `.cursor_pointer()` line must belong to a row of the P table (its "lines to delete" column) or sit in a W widget.
A line that belongs to neither is an element added since the tables were written: classify it with the Look chooser and convert it too.

- [ ] **Step 2: Convert the prompt picker rows (`composer.rs` 534-559) and the slash picker (`slash.rs` 235, 263-287).**
One `frame` closure draws every picker row, so one edit covers `composer.rs:568` and `:596`.
The picked row is the keyboard's choice and has a focus ring; it stays clickable.
Delete `let hover = theme.surface_raised;` (534) and the `.when(!picked, hover)`, `.cursor_pointer()` and `.active(..)` lines (556-558).

Before:

```rust
                .when(picked, |el| el.bg(rgb(theme.surface_raised)))
                .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover))))
                .cursor_pointer()
                .active(|style| style.opacity(opacity::ACTIVE))
        };
```

After:

```rust
                .when(picked, |el| el.bg(rgb(theme.surface_raised)))
                .pressable_unless(picked, Look::Row, theme)
        };
```

At the two call sites (568, 596) turn `.on_click(cx.listener(..))` into `.on_press(true, cx.listener(..))`; the listener bodies do not change.
`slash.rs` 283-287 takes the same edit: delete `let hover` (235) and lines 284-286, add `.pressable_unless(picked, Look::Row, theme)` after `.when(picked, ..bg..)`, and turn `.on_click(` (287) into `.on_press(true, ..)`.

- [ ] **Step 3: Convert the queue X (`composer.rs` 822-840).**
`Look::Icon`; the click keeps `cx.stop_propagation()` because the panel has a mouse down at 851.
Delete `let hover = theme.border;` (794), `.cursor_pointer()` (830), `.hover(..)` (832) and `.active(..)` (833).

Before:

```rust
                            .rounded(px(radius::SMALL))
                            .cursor_pointer()
                            .text_color(rgb(theme.text_muted))
                            .hover(move |style| style.bg(rgb(hover)))
                            .active(|style| style.opacity(opacity::ACTIVE))
                            .tooltip(tooltip(theme, "Remove from the queue (not sent)", ""))
                            .on_click(cx.listener(move |shell, _, _, cx| {
                                cx.stop_propagation();
                                shell.unqueue(session, send_id, cx);
                            }))
```

After:

```rust
                            .rounded(px(radius::SMALL))
                            .text_color(rgb(theme.text_muted))
                            .pressable(Look::Icon, true, theme)
                            .tooltip(tooltip(theme, "Remove from the queue (not sent)", ""))
                            .on_press(
                                true,
                                cx.listener(move |shell, _, _, cx| {
                                    cx.stop_propagation();
                                    shell.unqueue(session, send_id, cx);
                                }),
                            )
```

- [ ] **Step 4: Convert the history rows (`history.rs`).**
The history list row (712-727) and the turns dialog row (596-610) are picked when selected.
Delete `let hover` (698 / 528), `.cursor_pointer()` (719), the `.when(!selected, hover..)` blocks (722-723 / 604).
The turns row had no pointer and no press look before; it gains both.

Before (history list row):

```rust
                        .cursor_pointer()
                        .when(selected, |el| el.bg(rgb(theme.surface_raised)))
                        .when(!selected, |el| {
                            el.hover(move |style| style.bg(rgb(hover)))
                                .active(|style| style.opacity(opacity::ACTIVE))
                        })
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.select_history(id, window, cx);
                        }))
```

After:

```rust
                        .when(selected, |el| el.bg(rgb(theme.surface_raised)))
                        .pressable_unless(selected, Look::Row, theme)
                        .on_press(
                            true,
                            cx.listener(move |shell, _, window, cx| {
                                shell.select_history(id, window, cx);
                            }),
                        )
```

The turns row (605) is the same with its own `selected` flag and its closure body unchanged; the Restore and Diff buttons inside it are W and keep today's behavior (their click also selects the row).

- [ ] **Step 5: Convert the thread panel (`thread_panel.rs`).**
Two muted lines are links: `Look::Text` (the underline and full text color on hover).
Delete `.cursor_pointer()` at 154 and 185.

Before (151-156):

```rust
        let earlier = page.truncated.then(|| {
            meta_line(theme, "Earlier turns: Open transcript".into())
                .id("thread-earlier")
                .cursor_pointer()
                .on_click(cx.listener(move |shell, _, _, cx| shell.open_transcript(id, cx)))
        });
```

After:

```rust
        let earlier = page.truncated.then(|| {
            meta_line(theme, "Earlier turns: Open transcript".into())
                .id("thread-earlier")
                .pressable(Look::Text, true, theme)
                .on_press(
                    true,
                    cx.listener(move |shell, _, _, cx| shell.open_transcript(id, cx)),
                )
        });
```

Before (181-201, the tool call disclosure):

```rust
                                .cursor_pointer()
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
```
```rust
                                .on_click(cx.listener(move |shell, _, _, cx| {
```

After: delete the `.cursor_pointer()` line, add `.pressable(Look::Text, true, theme)` after `.text_color(..)`, and turn `.on_click(` into `.on_press(true, ..)` (the closure body is unchanged, wrap it as in the block above).


- [ ] **Step 6: Build, lint and test.** No new test: these are styling changes, and their behavior is pinned by P1 Task 4's `press_style` tests and Task 1's helper tests.

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: clean. An "unused variable: `hover`" or "unused import" is a leftover to delete.
Run: `cargo test -p mkrew-ui`
Expected: PASS.

- [ ] **Step 7: Visual check, dark then light.** Run the app on Windows and switch the theme with the title bar toggle.
Every view of this area must paint without a panic (a second `.hover(` on one element panics at paint).
- **Thread (dark + light):** hover "Earlier turns: Open transcript" and a "+N tool calls" line: each underlines and turns to full text color.
- **Pickers (dark + light):** open the prompt picker and the slash picker: hover an unpicked row (a fill and a pointer); the keyboard-picked row keeps its ring and fill, and a click on it still picks.
- **Queue and history (dark + light):** queue a second message while an agent works and hover its X (a fill; clicking it does not dim or select the panel); open History and the turns dialog and hover an unselected row (a fill and a pointer); the selected row keeps a pointer and no fill change.
- **Inspector:** with the right panel open hover its title bar toggle: the hover is visible against the open fill.
Capture one screenshot per theme with the pointer on a converted element into `docs/screenshots/2026-11-21-pressable-inspector-thread-dark.png` and `docs/screenshots/2026-11-21-pressable-inspector-thread-light.png` if you can capture automatically.
Append the bullets above to the section `### To verify - 2026-11-21 Pressable audit (P3)` of `docs/HUMAN_CHECKS.md`, each as a `- [ ] **<name> (dark + light):** ...` line with its sentences on separate lines.

- [ ] **Step 8: Commit**

```bash
git add crates/mkrew-ui/src/shell/thread_panel.rs crates/mkrew-ui/src/shell/composer.rs crates/mkrew-ui/src/shell/slash.rs crates/mkrew-ui/src/shell/history.rs docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): thread, composer, pickers and history take their press look from widgets"
```

---

### Task 8: Dialogs

P1 moved every dialog panel to `floating_panel`, so the dialog files have shifted: find each element by its function name and id.
The scrim of every dialog is `modal_shade` (`focus.rs:408`, EXEMPT); no dialog file has a handler of its own on it.

**Files:**
- Modify: `crates/mkrew-ui/src/shell/dialogs.rs`, `crates/mkrew-ui/src/shell/send_dialog.rs`, `crates/mkrew-ui/src/shell/task_dialog.rs`, `crates/mkrew-ui/src/shell/ports.rs`
- Read only (every handler is W; they keep `.on_click(`): `ask_dialog.rs`, `confirm_prompt.rs`, `diagram_dialog.rs`, `worktrees.rs`, `workspace_actions.rs`, `phone_access.rs`
- Modify: `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: `Look`, `Pressable::{pressable, pressable_unless, on_press}` (Task 1 and P1); `DialogControl::{focus_stop, focusable_bordered, chip_arrows}` (existing, return `Self`).
- Produces: nothing later tasks use.

**P elements (6, convert these):**

| file:line | fn | element | Look | pattern | lines to delete | notes |
| --- | --- | --- | --- | --- | --- | --- |
| `shell/dialogs.rs:523` | `render_broadcast` | Broadcast checkbox row | `Row` | enabled=`!busy` | 481 (`let hover`), 516-522 | The tooltip moves into `.when(!busy, ..)`. While busy the click does nothing (it toggled mid-send before). |
| `shell/ports.rs:206` | `render_ports` | port row (go to its session) | `Row` | `.when(!selected && clickable, pressable Row)`, `.when(selected && clickable, cursor_pointer)`; `on_press(clickable, ..)` | 169 (`let hover`), 203-205 | A row with no session is not "disabled": no `pressable`, no fade. |
| `shell/ports.rs:244` | `render_ports` | stop square in a port row | `Icon` | enabled=true | 240, 242 | The `danger_bg` hover tint is lost; the text stays `danger_text`. Keep `stop_propagation`. |
| `shell/send_dialog.rs:375` | `render_send_dialog` | session pick row | `Row` | `.when(!picked, \|el\| el.pressable(Look::Row, !dialog.busy, theme))` + `.when(picked, cursor_pointer)`; `on_press(!dialog.busy, ..)` | 345 (`let hover`), 370-374 | Rows fade while sending (new); the picked row keeps its own fill. |
| `shell/task_dialog.rs:331` | `render_task_dialog` | agent chip | `Chip` | `pressable_unless(choice.selected, ..)` | 308 (`let hover`), 316, 327, 328 | `focusable_bordered` and `chip_arrows` stay after `on_press`. |
| `shell/task_dialog.rs:370` | `render_task_dialog` | "Run in a worktree" row | `Row` | enabled=`!locked` | 344 (`let hover`), 365-369 | `focus_stop` stays conditional on `!locked`. A locked row now fades (new). |

**W elements (keep `.on_click(`; 53 sites; every one is a `button`, `danger_button`, `quiet_button`, `secondary_button` or `icon_button` that also carries `.focus_stop(theme)` or `.focusable_bordered(theme)`):**

- `shell/ask_dialog.rs`: 276, 284, 290
- `shell/confirm_prompt.rs`: 126
- `shell/diagram_dialog.rs`: 81
- `shell/dialogs.rs`: 294, 435, 445, 453, 558, 566, 577, 585
- `shell/phone_access.rs`: 197, 253, 263, 285, 306, 312, 370, 428
- `shell/ports.rs`: 291, 302, 311
- `shell/send_dialog.rs`: 429, 437, 456, 470, 477, 581
- `shell/task_dialog.rs`: 242, 489, 512, 605, 615, 625
- `shell/workspace_actions.rs`: 969, 1005, 1052, 1090, 1100
- `shell/worktrees.rs`: 448, 511, 519, 544, 561, 574, 590, 615, 624, 719, 740, 845

**EXEMPT in these files:** none of the handler rows; the text fields (`caret_field`), the drop catcher and `on_drag_move` of `task_dialog.rs` (525, 555) and the `modal_shade` scrims are not buttons.
`confirm_prompt.rs:116-124` and `worktrees.rs:578-579, 628-629` put `.border`, `.bg` or `.text_color` on top of a widget: they stay valid as long as the rebuilt widgets do not set a conflicting border or fill last (the visual step checks the Discard / Remove dialogs).

- [ ] **Step 1: Re-run the inventory for this area and match it to the tables above.**

```bash
grep -nE '\.on_click\(|\.on_mouse_down\(|\.hover\(|\.active\(\||cursor_pointer' crates/mkrew-ui/src/shell/dialogs.rs crates/mkrew-ui/src/shell/send_dialog.rs crates/mkrew-ui/src/shell/task_dialog.rs crates/mkrew-ui/src/shell/ports.rs crates/mkrew-ui/src/shell/ask_dialog.rs crates/mkrew-ui/src/shell/confirm_prompt.rs crates/mkrew-ui/src/shell/diagram_dialog.rs crates/mkrew-ui/src/shell/worktrees.rs crates/mkrew-ui/src/shell/workspace_actions.rs crates/mkrew-ui/src/shell/phone_access.rs
```

Every `.hover(`, `.active(|` and `.cursor_pointer()` line must belong to a row of the P table (its "lines to delete" column) or sit in a W widget.
A line that belongs to neither is an element added since the tables were written: classify it with the Look chooser and convert it too.

- [ ] **Step 2: Read the three handlers whose behavior changes (Review Focus 4).**
`on_press(false, ..)` attaches nothing, where today the click stays attached while the row only looks inert.
Read `toggle` of the Broadcast dialog (`dialog.toggle(index)`), the send dialog's pick handler and `Shell::go_to_port`.
Broadcast and send already stop being useful while busy (a toggle or pick during a send is a bug today), so blocking them is the intended result.
`go_to_port` must return without effect for a port with no session: confirm it does (`grep -n "fn go_to_port" -A 12 crates/mkrew-ui/src/shell/ports.rs`).
If it does not, give the port row `on_press(true, ..)` and keep `pressable` only inside `.when(!selected && clickable, ..)`.

- [ ] **Step 3: Convert the Broadcast row (`dialogs.rs` 481, 501-528).**
While busy the row is really disabled (it fades, no hover, no click); the tooltip only shows when it is usable.
Delete `let hover = theme.surface_raised;` (481), the `.when(busy, opacity)` line (516) and the `.when(!busy, ..)` block (517-522).

Before:

```rust
                .when(busy, |el| el.opacity(opacity::DISABLED))
                .when(!busy, |el| {
                    el.cursor_pointer()
                        .hover(move |style| style.bg(rgb(hover)))
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .tooltip(tooltip(theme, "Check or uncheck", "Space"))
                })
                .on_click(cx.listener(move |shell, _, _, cx| {
                    if let Some(dialog) = shell.broadcast.as_mut() {
                        dialog.toggle(index);
                    }
                    cx.notify();
                }))
```

After:

```rust
                .pressable(Look::Row, !busy, theme)
                .when(!busy, |el| {
                    el.tooltip(tooltip(theme, "Check or uncheck", "Space"))
                })
                .on_press(
                    !busy,
                    cx.listener(move |shell, _, _, cx| {
                        if let Some(dialog) = shell.broadcast.as_mut() {
                            dialog.toggle(index);
                        }
                        cx.notify();
                    }),
                )
```

- [ ] **Step 4: Convert the send dialog pick row (`send_dialog.rs` 345, 369-375).**
A picked row is not disabled; while the dialog is busy every row fades.
Delete `let hover` (345) and the `.when(!picked && !dialog.busy, ..)` block (370-374).

Before:

```rust
            .when(!picked && !dialog.busy, |el| {
                el.cursor_pointer()
                    .hover(move |style| style.bg(rgb(hover)))
                    .active(|style| style.opacity(opacity::ACTIVE))
            })
            .on_click(cx.listener(move |shell, _, _, cx| {
```

After:

```rust
            .when(!picked, |el| el.pressable(Look::Row, !dialog.busy, theme))
            .when(picked, |el| el.cursor_pointer())
            .on_press(!dialog.busy, cx.listener(move |shell, _, _, cx| {
```

(the closure body and its closing are unchanged; run `cargo fmt`).
The picked row keeps its `bg(surface_raised)` and its focus border.

- [ ] **Step 5: Convert the task dialog (`task_dialog.rs`).**
The agent chip (307-340) is picked when selected; the worktree row (344, 363-377) is disabled while `locked`, and `focus_stop` stays conditional on `!locked`.
Delete `let hover` (308, 344), `.cursor_pointer()` (316), the `.hover(..)` and `.active(..)` lines (327-328) and the hover block of the worktree row (365-369).

Before (agent chip):

```rust
                        let hover = theme.surface_raised;
                        let agent = choice.id;
                        div()
                            .id(SharedString::from(format!("task-agent-{agent}")))
                            .px(px(space::S))
                            .py(px(space::XS))
                            .rounded(px(radius::SMALL))
                            .border(px(theme::BORDER_WIDTH))
                            .cursor_pointer()
                            .text_size(font_size::BODY)
                            .when(choice.selected, |el| {
                                el.border_color(rgb(theme.accent))
                                    .text_color(rgb(theme.text))
                                    .font_weight(FontWeight(weight::MEDIUM))
                                    .child(format!("✓ {}", choice.label))
                            })
                            .when(!choice.selected, |el| {
                                el.border_color(rgb(theme.border))
                                    .text_color(rgb(theme.text_muted))
                                    .hover(move |style| style.bg(rgb(hover)))
                                    .active(|style| style.opacity(opacity::ACTIVE))
                                    .child(choice.label)
                            })
                            .on_click(cx.listener(move |shell, _, _, cx| {
                                if let Some(dialog) = shell.task_dialog.as_mut() {
                                    dialog.choose_agent(&agent);
                                }
                                cx.notify();
                            }))
                            .focusable_bordered(theme)
                            .chip_arrows(index, len)
```

After:

```rust
                        let agent = choice.id;
                        div()
                            .id(SharedString::from(format!("task-agent-{agent}")))
                            .px(px(space::S))
                            .py(px(space::XS))
                            .rounded(px(radius::SMALL))
                            .border(px(theme::BORDER_WIDTH))
                            .text_size(font_size::BODY)
                            .when(choice.selected, |el| {
                                el.border_color(rgb(theme.accent))
                                    .text_color(rgb(theme.text))
                                    .font_weight(FontWeight(weight::MEDIUM))
                                    .child(format!("✓ {}", choice.label))
                            })
                            .when(!choice.selected, |el| {
                                el.border_color(rgb(theme.border))
                                    .text_color(rgb(theme.text_muted))
                                    .child(choice.label)
                            })
                            .pressable_unless(choice.selected, Look::Chip, theme)
                            .on_press(
                                true,
                                cx.listener(move |shell, _, _, cx| {
                                    if let Some(dialog) = shell.task_dialog.as_mut() {
                                        dialog.choose_agent(&agent);
                                    }
                                    cx.notify();
                                }),
                            )
                            .focusable_bordered(theme)
                            .chip_arrows(index, len)
```

The "Run in a worktree" row: `.pressable(Look::Row, !locked, theme)` replaces the hover block, `.on_press(!locked, ..)` replaces `.on_click(`, and `.focus_stop(theme)` stays inside `.when(!locked, ..)`.
A locked row now fades (new; it had muted text only).

- [ ] **Step 6: Convert the ports dialog (`ports.rs` 169, 203-208, 235-248).**
A port row with no session is not "disabled": no `pressable`, no fade, and `on_press(clickable, ..)` attaches nothing.
The stop square loses its `danger_bg` hover tint (`Look::Icon` is neutral) and keeps `danger_text`.
Delete `let hover = theme.hover;` (169), `.cursor_pointer()` and `.hover(..)` of the row (203-205) and `.cursor_pointer()` (240) and `.hover(..)` (242) of the stop button.

Before:

```rust
                        .when(!selected && clickable, |el| {
                            el.cursor_pointer().hover(move |style| style.bg(rgb(hover)))
                        })
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.go_to_port(&go, window, cx);
                        }))
```
```rust
                                .rounded(px(radius::SMALL))
                                .cursor_pointer()
                                .text_color(rgb(theme.danger_text))
                                .hover(|style| style.bg(rgb(theme.danger_bg)))
                                .tooltip(tooltip(theme, "Stop", "Delete"))
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    cx.stop_propagation();
                                    shell.ask_port_stop(stop.clone(), window, cx);
                                }))
```

After:

```rust
                        .when(!selected && clickable, |el| {
                            el.pressable(Look::Row, true, theme)
                        })
                        .when(selected && clickable, |el| el.cursor_pointer())
                        .on_press(
                            clickable,
                            cx.listener(move |shell, _, window, cx| {
                                shell.go_to_port(&go, window, cx);
                            }),
                        )
```
```rust
                                .rounded(px(radius::SMALL))
                                .text_color(rgb(theme.danger_text))
                                .pressable(Look::Icon, true, theme)
                                .tooltip(tooltip(theme, "Stop", "Delete"))
                                .on_press(
                                    true,
                                    cx.listener(move |shell, _, window, cx| {
                                        cx.stop_propagation();
                                        shell.ask_port_stop(stop.clone(), window, cx);
                                    }),
                                )
```

After the edits `opacity` may be unused in `dialogs.rs`, `send_dialog.rs` and `ports.rs`: clippy says so; `task_dialog.rs` still uses `opacity::SECONDARY` and `opacity::DISABLED`.


- [ ] **Step 7: Build, lint and test.** No new test: these are styling changes, and their behavior is pinned by P1 Task 4's `press_style` tests and Task 1's helper tests.

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: clean. An "unused variable: `hover`" or "unused import" is a leftover to delete.
Run: `cargo test -p mkrew-ui`
Expected: PASS.

- [ ] **Step 8: Visual check, dark then light.** Run the app on Windows and switch the theme with the title bar toggle.
Every view of this area must paint without a panic (a second `.hover(` on one element panics at paint).
- **Row dialogs (dark + light):** open Broadcast: hover a row (a fill and a pointer); while a send is running the rows fade and do nothing.
  Open the send dialog: hover an unpicked session row (a fill); the picked row keeps its fill and a pointer; click it: still acts.
- **Task dialog (dark + light):** hover an unselected agent chip (a fill); the selected one keeps its accent border and a pointer.
  Open a task with a locked worktree choice: its row is faded with no hover and no pointer; unlocked, it hovers.
- **Ports (dark + light):** hover a port row with a session (a fill, click goes to it) and one without (no pointer, no fade); hover the stop square: a neutral fill, the square stays red.
- **Buttons in dialogs (dark + light):** open Discard changes (Git), Remove worktree and an Undo dialog: the red buttons still show their red text and the focus ring still shows with Tab.
Capture one screenshot per theme with the pointer on a converted element into `docs/screenshots/2026-11-21-pressable-dialogs-dark.png` and `docs/screenshots/2026-11-21-pressable-dialogs-light.png` if you can capture automatically.
Append the bullets above to the section `### To verify - 2026-11-21 Pressable audit (P3)` of `docs/HUMAN_CHECKS.md`, each as a `- [ ] **<name> (dark + light):** ...` line with its sentences on separate lines.

- [ ] **Step 9: Commit**

```bash
git add crates/mkrew-ui/src/shell/dialogs.rs crates/mkrew-ui/src/shell/send_dialog.rs crates/mkrew-ui/src/shell/task_dialog.rs crates/mkrew-ui/src/shell/ports.rs docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): dialog rows and chips take their press look from widgets"
```

---

### Task 9: Run, prompts, and what is left of Settings and the palette

Settings (`settings.rs`) is rewritten by P1 (spec §6) and the palette rows (`navigation.rs`) by P2 (spec §7).
This task runs after P1 (spec §9); P2 may land before or after it.
It converts only what still hand-builds a look.
If P2 has not landed, the two `navigation.rs` rows are converted here and P2 later replaces them by `list_row`.

**Files:**
- Modify: `crates/mkrew-ui/src/shell/run.rs`, `crates/mkrew-ui/src/shell/prompts_render.rs`
- Modify only if the inventory still shows a hand-built look: `crates/mkrew-ui/src/shell/settings.rs`, `crates/mkrew-ui/src/shell/navigation.rs`
- Read only (every handler is W or EXEMPT): `crates/mkrew-ui/src/shell/machines.rs`
- Modify: `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: `Look`, `Pressable::{pressable, pressable_unless, on_press}`, `toggle`, `dropdown_trigger`, `list_row` (P1), Task 1 helpers.
- Produces: nothing later tasks use.

**P elements (14, convert these; the 7 `settings.rs` and `navigation.rs` rows only if still hand-built after P1 and P2):**

| file:line | fn | element | Look | pattern | lines to delete | notes |
| --- | --- | --- | --- | --- | --- | --- |
| `shell/navigation.rs:1022` | `render_quick_open` | quick-open result row (P2 owns, P2 deletes it) | `Row` | `pressable_unless(picked, ..)` | 983 (`let hover`), 1019-1021 | Only if P2 has not removed it. |
| `shell/navigation.rs:1167` | `render_palette` | palette row (P2 owns) | `Row` | `pressable_unless(picked, ..)` | 1130 (`let hover`), 1164-1166 | Only if P2 still draws rows by hand (it should use `list_row`). |
| `shell/prompts_render.rs:98` | `prompts_list` | prompt list row | `Row` | `pressable_unless(picked, ..)` | 91 (`let hover`), 96, 97 |  |
| `shell/run.rs:371` | `render_run_button` | title bar Run pill | `Chip (idle) / Text (live)` | `.when(live, Look::Text)` and `.when(!live, Look::Chip)`, enabled=true | 333 (`let hover`), 354, 362, 366, 368 | A live pill keeps its green fill: a fill hover would turn it grey, so it underlines and brightens its text instead. |
| `shell/run.rs:793` | `render_run_setup` | folded Run command card | `Chip` | enabled=true | 785 (`let hover`), 790, 791, 792 | Nested Duplicate/Remove (`card_action`, W) keep `stop_propagation`. |
| `shell/run.rs:867` | `render_run_setup` | checkbox "Close the pane when it ends" | `Row` | look moves into the `check` closure; caller uses `.on_press(true, ..)` | 641 (`let hover`, inside `check`), 653, 656, 657 |  |
| `shell/run.rs:931` | `render_run_setup` | dashed "+ Add command" row | `Chip` | enabled=true | 913 (`let add_hover`), 925, 928, 929 | `focusable_bordered` stays. |
| `shell/run.rs:958` | `render_run_setup` | checkbox "Start with the daemon" | `Row` | same `check` closure as 867 | (same lines) |  |
| `shell/run.rs:1149` | `render_run_picker` | Run folder picker entry | `Row` | `pressable_unless(picked, ..)`, `picked = picker.cursor == Some(index)` | 1087 (`let hover`), 1116, 1118, 1122 |  |
| `shell/settings.rs:223` | `render_settings_dialog` | Settings nav tab (P1 owns) | `Row` | `pressable_unless(active, ..)` | 214, 222 |  |
| `shell/settings.rs:426` | `settings_link` | `settings_link` (P1 owns) | `Text` | enabled=true | 423, 425 | Keep the `accent_text` colour; `Look::Text` must only underline an accent link. |
| `shell/settings.rs:470` | `settings_row` | Settings row (P1 owns) | `Row` | `pressable_unless(focused, ..)`, only for rows whose click acts (`!row.click_focuses_only()`) | none |  |
| `shell/settings.rs:580` | `settings_control` | choice segment (P1 owns) | `Chip` | enabled=`!read_only` | 555-ish `cursor_pointer` | Drop the container fade when read-only: `pressable(.., false)` fades each segment. |
| `shell/settings.rs:677` | `binding_control` | shortcut "Reset" (P1 owns) | `Chip` | enabled=true | 668, 671 | Keep `stop_propagation` and `focus_stop`. |

**W elements (keep `.on_click(`; 18 sites):**

- `shell/machines.rs`: 297, 392, 400, 488, 495
- `shell/prompts_render.rs`: 74, 270, 280, 289
- `shell/run.rs`: 625, 748, 761, 1052, 1062, 1179, 1259, 1271
- `shell/settings.rs`: 750

**EXEMPT in these files:** `machines.rs:464`, `prompts_render.rs:226, 252`, `settings.rs:261` (text fields).
The Settings Bool switch (`switch-..`, 541-557) and the Font and List faux dropdown (612-638) have no handler of their own; P1 replaces them by `toggle` and `dropdown_trigger`.

- [ ] **Step 1: Re-run the inventory for this area and match it to the tables above.**

```bash
grep -nE '\.on_click\(|\.on_mouse_down\(|\.hover\(|\.active\(\||cursor_pointer' crates/mkrew-ui/src/shell/run.rs crates/mkrew-ui/src/shell/machines.rs crates/mkrew-ui/src/shell/prompts_render.rs crates/mkrew-ui/src/shell/settings.rs crates/mkrew-ui/src/shell/navigation.rs
```

Every `.hover(`, `.active(|` and `.cursor_pointer()` line must belong to a row of the P table (its "lines to delete" column) or sit in a W widget.
A line that belongs to neither is an element added since the tables were written: classify it with the Look chooser and convert it too.

- [ ] **Step 2: Convert the Run pill (`run.rs` 333-373).**
The idle pill hovers with a fill; a live pill keeps its green fill and hovers with an underline and brighter text (a fill would turn it grey).
Delete `let hover = theme.hover;` (333), `.cursor_pointer()` (354), both `.hover(..)` lines (362, 366) and `.active(..)` (368).

Before:

```rust
            .border(px(theme::BORDER_WIDTH))
            .cursor_pointer()
            .whitespace_nowrap()
            .text_size(font_size::BODY)
            .text_color(rgb(fg))
            .when(live, |el| {
                el.bg(rgb(theme.run_pill_bg))
                    .border_color(rgb(theme.run_pill_border))
                    .font_weight(FontWeight(weight::MEDIUM))
                    .hover(|style| style.opacity(opacity::HOVER))
            })
            .when(!live, |el| {
                el.border_color(rgb(theme.control_border))
                    .hover(move |style| style.bg(rgb(hover)))
            })
            .active(|style| style.opacity(opacity::ACTIVE))
            .tooltip(tooltip(theme, &tip, ""))
            // An open menu already closed on this mouse down (outside it): the click reopens it.
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.open_run_menu(project_id, window, cx);
            }))
```

After:

```rust
            .border(px(theme::BORDER_WIDTH))
            .whitespace_nowrap()
            .text_size(font_size::BODY)
            .text_color(rgb(fg))
            .when(live, |el| {
                el.bg(rgb(theme.run_pill_bg))
                    .border_color(rgb(theme.run_pill_border))
                    .font_weight(FontWeight(weight::MEDIUM))
                    .pressable(Look::Text, true, theme)
            })
            .when(!live, |el| {
                el.border_color(rgb(theme.control_border))
                    .pressable(Look::Chip, true, theme)
            })
            .tooltip(tooltip(theme, &tip, ""))
            // An open menu already closed on this mouse down (outside it): the click reopens it.
            .on_press(
                true,
                cx.listener(move |shell, _, window, cx| {
                    shell.open_run_menu(project_id, window, cx);
                }),
            )
```

The two `pressable` calls are in mutually exclusive branches, so the element never gets two hovers.
`fg` stays (the label color).

- [ ] **Step 3: Convert the Run setup page and picker (`run.rs`).**
The checkbox closure `check` (636-681) is shared by "Close the pane when it ends" (867) and "Start with the daemon" (958): the look goes inside the closure and both callers use `.on_press(true, ..)`.
Delete `let hover = theme.surface_raised;` (641), `.cursor_pointer()` (653), `.hover(..)` (656) and `.active(..)` (657).

Before:

```rust
            let hover = theme.surface_raised;
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::XS))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .border(px(theme::BORDER_WIDTH))
                .border_color(gpui::transparent_black())
                .when(focused, |el| el.border_color(rgb(theme.pane_focus_border)))
                .cursor_pointer()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text))
                .hover(move |style| style.bg(rgb(hover)))
                .active(|style| style.opacity(opacity::ACTIVE))
                .tooltip(tooltip(theme, tip, "Space"))
```

After:

```rust
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::XS))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .border(px(theme::BORDER_WIDTH))
                .border_color(gpui::transparent_black())
                .when(focused, |el| el.border_color(rgb(theme.pane_focus_border)))
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text))
                .pressable(Look::Row, true, theme)
                .tooltip(tooltip(theme, tip, "Space"))
```

The callers: turn

```rust
            .on_click(cx.listener(move |shell, _, _, cx| {
                if let Some(setup) = shell.run_setup.as_mut()
                    && let Some(card) = setup.cards.get_mut(i)
                {
                    card.exit_after = !card.exit_after;
                    setup.set_focus(Focus::Exit(i));
                }
                cx.notify();
            }));
```

into

```rust
            .on_press(
                true,
                cx.listener(move |shell, _, _, cx| {
                    if let Some(setup) = shell.run_setup.as_mut()
                        && let Some(card) = setup.cards.get_mut(i)
                    {
                        card.exit_after = !card.exit_after;
                        setup.set_focus(Focus::Exit(i));
                    }
                    cx.notify();
                }),
            );
```

and the same change at 958 for its own closure body.
The folded command card (785, 790-793) and the dashed "+ Add command" row (913, 925-931) are `Look::Chip`: delete `let hover` / `let add_hover`, `.cursor_pointer()`, `.hover(..)`, `.active(..)`, add `.pressable(Look::Chip, true, theme)` and turn the `.on_click(` into `.on_press(true, ..)`; `.focusable_bordered(theme)` on the Add row stays after it.
The folder picker entry (1087, 1108-1160) is picked at the cursor: its hover fill equals the picked fill today, so a hover on the cursor row showed nothing.

Before:

```rust
                    .rounded(px(radius::SMALL))
                    .cursor_pointer()
                    .text_size(font_size::BODY)
                    .hover(move |style| style.bg(rgb(hover)))
                    .when(picker.cursor == Some(index), |el| {
                        el.bg(rgb(theme.surface_raised))
                    })
                    .active(|style| style.opacity(opacity::ACTIVE))
```

After (and delete `let hover = theme.surface_raised;` at 1087):

```rust
                    .rounded(px(radius::SMALL))
                    .text_size(font_size::BODY)
                    .when(picker.cursor == Some(index), |el| {
                        el.bg(rgb(theme.surface_raised))
                    })
                    .pressable_unless(picker.cursor == Some(index), Look::Row, theme)
```

and `.on_click(` (1149) becomes `.on_press(true, ..)`.

- [ ] **Step 4: Convert the prompts list row (`prompts_render.rs` 79-113).**
The open prompt (`active`) is picked and stays clickable (the handler already skips re-opening it).
Delete `.cursor_pointer()` (91), `.hover(..)` (96) and `.active(..)` (97), and `let hover` if the function declares one.

Before:

```rust
                .rounded(px(radius::SMALL))
                .cursor_pointer()
                .border(px(theme::BORDER_WIDTH))
                .border_color(gpui::transparent_black())
                .when(active, |el| el.bg(rgb(theme.surface_raised)))
                .when(focused, |el| el.border_color(rgb(theme.selected_ring)))
                .hover(|style| style.bg(rgb(theme.surface_raised)))
                .active(|style| style.opacity(opacity::ACTIVE))
                .on_click(cx.listener(move |shell, _, window, cx| {
```

After:

```rust
                .rounded(px(radius::SMALL))
                .border(px(theme::BORDER_WIDTH))
                .border_color(gpui::transparent_black())
                .when(active, |el| el.bg(rgb(theme.surface_raised)))
                .when(focused, |el| el.border_color(rgb(theme.selected_ring)))
                .pressable_unless(active, Look::Row, theme)
                .on_press(true, cx.listener(move |shell, _, window, cx| {
```

(the closure body and its closing are unchanged; run `cargo fmt`).

- [ ] **Step 5: Settings and the palette: convert what P1 and P2 left.**
Run the inventory of Step 1 on `settings.rs` and `navigation.rs`.
For each remaining `.hover(`, `.active(|` or `.cursor_pointer()`, use the P table row (`settings.rs` 223, 426, 470, 580, 677; `navigation.rs` 1022, 1167) and these shapes.

A picker row (quick-open row, palette row; `let hover` at 983 / 1130, lines 1019-1021 / 1164-1166):

Before:

```rust
            .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover))))
            .cursor_pointer()
            .active(|style| style.opacity(opacity::ACTIVE))
            .on_click(cx.listener(move |shell, _, window, cx| {
```

After:

```rust
            .pressable_unless(picked, Look::Row, theme)
            .on_press(true, cx.listener(move |shell, _, window, cx| {
```

A choice segment of a Settings row (`settings.rs` 559-588): not `segment()` (its height and padding would resize the compact control).
When read only, `pressable(.., false, ..)` fades each inactive segment, so the container's own fade goes (it would fade twice).

Before:

```rust
                .when(read_only, |el| el.opacity(opacity::DISABLED))
                .children(choices.iter().map(|&choice| {
                    let active = shown == choice;
                    div()
                        .id(SharedString::from(format!(
                            "choice-{}-{choice}",
                            setting.id()
                        )))
                        .px(px(space::S))
                        .rounded(px(radius::SMALL))
                        .text_size(font_size::META)
                        .text_color(rgb(if active { theme.text } else { theme.text_muted }))
                        .when(active, |el| el.bg(rgb(theme.surface_raised)))
                        .when(!active && !read_only, |el| {
                            el.cursor_pointer().on_click(cx.listener(
                                move |shell, _, window, cx| {
                                    cx.stop_propagation();
                                    shell.settings_do(|d, _| d.choose(setting, choice), window, cx);
                                },
                            ))
                        })
                        .child(choice)
                }))
```

After:

```rust
                .children(choices.iter().map(|&choice| {
                    let active = shown == choice;
                    div()
                        .id(SharedString::from(format!(
                            "choice-{}-{choice}",
                            setting.id()
                        )))
                        .px(px(space::S))
                        .rounded(px(radius::SMALL))
                        .text_size(font_size::META)
                        .text_color(rgb(if active { theme.text } else { theme.text_muted }))
                        .when(active, |el| {
                            el.bg(rgb(theme.surface_raised))
                                .when(read_only, |el| el.opacity(opacity::DISABLED))
                        })
                        .when(!active, |el| {
                            el.pressable(Look::Chip, !read_only, theme).on_press(
                                !read_only,
                                cx.listener(move |shell, _, window, cx| {
                                    cx.stop_propagation();
                                    shell.settings_do(|d, _| d.choose(setting, choice), window, cx);
                                }),
                            )
                        })
                        .child(choice)
                }))
```

The other Settings rows: the left nav tab is `pressable_unless(active, Look::Row, theme)`; `settings_link` is `.pressable(Look::Text, true, theme)` and keeps its `accent_text` color; the shortcut Reset is `.pressable(Look::Chip, true, theme)` and keeps `stop_propagation` and `focus_stop`; the whole Settings row (470) is `pressable_unless(focused, Look::Row, theme)` only for rows whose click acts (`!row.click_focuses_only()`: Bool, Font, List and Binding rows), with `enabled` false only for a `Row::Setting` while `dialog.read_only()` (a binding row is not read only).
The Bool switch and the dropdown trigger are P1's `toggle(id, on, !read_only, theme)` and `dropdown_trigger(..)`; drop their own `opacity(DISABLED)` and `cursor_pointer` lines if they are still there.


- [ ] **Step 6: Build, lint and test.** No new test: these are styling changes, and their behavior is pinned by P1 Task 4's `press_style` tests and Task 1's helper tests.

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: clean. An "unused variable: `hover`" or "unused import" is a leftover to delete.
Run: `cargo test -p mkrew-ui`
Expected: PASS.

- [ ] **Step 7: Visual check, dark then light.** Run the app on Windows and switch the theme with the title bar toggle.
Every view of this area must paint without a panic (a second `.hover(` on one element panics at paint).
- **Run (dark + light):** hover the title bar Run pill idle (a fill) and while something runs (it keeps its green fill; the label underlines and brightens); click opens the menu.
  In Run setup hover a folded command card, the dashed "+ Add command" row and the two checkboxes: each shows a fill; open the folder picker and hover an unpicked entry (a fill); the entry at the cursor keeps its fill and a pointer.
- **Prompts (dark + light):** hover an unopened prompt row (a fill); the open prompt keeps its fill and a pointer.
- **Settings and palette remainders (dark + light):** hover a left nav tab, a link in the footer (underline, accent color stays), a Choice segment, a changed shortcut's Reset and a Settings row that acts on click: each shows its look; with `settings.toml` unparsable the choice segments are faded and do nothing.
  Open the command palette and hover an unpicked row: a fill; the picked row keeps its ring and a pointer.
Capture one screenshot per theme with the pointer on a converted element into `docs/screenshots/2026-11-21-pressable-run-prompts-settings-dark.png` and `docs/screenshots/2026-11-21-pressable-run-prompts-settings-light.png` if you can capture automatically.
Append the bullets above to the section `### To verify - 2026-11-21 Pressable audit (P3)` of `docs/HUMAN_CHECKS.md`, each as a `- [ ] **<name> (dark + light):** ...` line with its sentences on separate lines.

- [ ] **Step 8: Commit**

```bash
git add crates/mkrew-ui/src/shell/run.rs crates/mkrew-ui/src/shell/prompts_render.rs crates/mkrew-ui/src/shell/settings.rs crates/mkrew-ui/src/shell/navigation.rs docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Run, prompts and the leftover Settings and palette rows take their press look from widgets"
```

---

### Task 10: Write the rule into SPEC §14, the skill and the reviewer

**Files:**
- Modify: `docs/SPEC.md` (§14, after the "Shared controls" bullet, about line 1648)
- Modify: `.claude/skills/mkrew-ui-design/SKILL.md`
- Modify: `.claude/agents/ui-reviewer.md`
- Modify: `docs/decisions/0097-glass-panels-and-pressable.md` (written by P1; this task appends a section)

**Interfaces:**
- Consumes: the helper names of Task 1 and the Look chooser of this plan.
- Produces: the rule text that the three docs share.

No code changes, so no test; the check is that the three files carry the same names as the code (`Look`, `Raised`, `pressable_unless`, `card_press`, `grip_hover`, `row_action`).

- [ ] **Step 1: Add the rule to SPEC §14.**
Find the bullet that starts "Shared controls (`shell/widgets.rs`): segmented control" (it ends "key hint.") and insert this bullet right after it, each sentence on its own line:

```markdown
- Pressable elements (ADR 0097): every element with a click or mouse-down handler gets its hover, press, focus and disabled look from `shell/widgets.rs` and its `Pressable` trait (`pressable.rs`).
  It is either a widget (`button`, `secondary_button`, `quiet_button`, `danger_button`, `icon_button`, `segment`, `menu_item`, `row_action`, `list_row`, `toggle`, `dropdown_trigger`) or `.pressable(look, enabled, theme)` with `.on_press(enabled, handler)`.
  Only `widgets.rs` and `pressable.rs` call `.hover(` and `.active(`.
  `group_hover(..)`, which reveals hidden row actions, is allowed anywhere.
  The looks are `Row` (list and menu rows), `Chip` (pills, tabs, small labelled controls), `Icon` (icon-only controls), `Text` (links) and `Raised` (a control on a raised surface: menu, toast, card, find bar, code block).
  Board cards use `card_press`: the border lights on hover and the fill stays, so the chips inside a card keep a visible hover.
  A hover fill is never the selected fill.
  A selected or active element that is still clickable keeps the pointer only (`pressable_unless(picked, look, theme)`).
  Disabled means `opacity::DISABLED`, the default cursor, no hover, no press and no click.
  An element that is not clickable right now, but is not disabled, has no look at all.
  Not buttons, so no hover: resize grips (they tint through `grip_hover`), terminal surfaces, drop zones, the modal scrim, text fields (they keep their I-beam), editor and text-selection surfaces, window-drag areas, and the context-menu mouse down of an element that is already pressable.
```

Check: `grep -n "Pressable elements (ADR 0097)" docs/SPEC.md` prints one line.

- [ ] **Step 2: Add the rule to the skill.**
In `.claude/skills/mkrew-ui-design/SKILL.md`, after hard rule 10 add:

```markdown
11. Every element with `.on_click(` or `.on_mouse_down(` is a widget from `shell/widgets.rs` or uses `.pressable(look, enabled, theme)` plus `.on_press(enabled, handler)`.
    Never call `.hover(`, `.active(` or `.cursor_pointer()` on a component outside `shell/widgets.rs` and `pressable.rs`.
    Looks: `Row`, `Chip`, `Icon`, `Text`, `Raised` (on a raised surface), `card_press` (board cards).
    A selected row uses `.pressable_unless(picked, look, theme)`, never `enabled = false` (that fades it).
    Exempt, no hover: resize grips (`grip_hover`), the terminal, drop zones, the scrim, text fields, text-selection surfaces.
```

and at the end of "Before committing UI work" add:

```markdown
- grep your diff for `.hover(`, `.active(|` and `.cursor_pointer()` outside `shell/widgets.rs` and `pressable.rs`; a hit is a bug: convert it to a widget or `.pressable(..)`.
```

- [ ] **Step 3: Add the rule to the reviewer.**
In `.claude/agents/ui-reviewer.md` change check 3 to read `3. Interactive elements have hover, focus ring, active, disabled states, and get them from widgets (check 9).` and add after check 8:

```markdown
9. Pressable rule (SPEC §14, ADR 0097): run `git diff -U0 -- crates/mkrew-ui | grep -nE '^\+.*(\.hover\(|\.active\(\||\.cursor_pointer\(\))'`; a hit outside `shell/widgets.rs` and `pressable.rs` is a BLOCKER.
   A new `.on_click(` or `.on_mouse_down(` sits on a widget or on `.pressable(..)` plus `.on_press(..)`.
   A selected or picked row uses `pressable_unless`, never `enabled = false`.
   A widget plus `.pressable(..)` on one element is a BLOCKER (a second hover panics).
   Exempt: resize grips, terminal surfaces, drop zones, the scrim, text fields, text-selection surfaces.
```

- [ ] **Step 4: Append the audit result to ADR 0097.**
Append to `docs/decisions/0097-glass-panels-and-pressable.md`:

```markdown
## Audit (Plan P3, 2026-11-21)

Before the audit `crates/mkrew-ui/src` had 327 click or mouse-down handlers in 48 files and 76 `.hover(` calls.
Of the handlers, 208 were already on a shared widget, 71 were hand-built and moved to a `Look` or a helper, and 48 are exempt.
After it only `shell/widgets.rs` and `pressable.rs` call `.hover(` and `.active(`.

The audit added around the P1 contract:

- `Pressable::pressable_unless` for a selected element that stays clickable.
- `row_action` (moved into `widgets.rs`), `grip_hover`, `pick_surface`, `card_press` and `disabled_icon_button`.

`Look::Row`, `Look::Chip` and `Look::Icon` hover with `theme.hover` and `Look::Raised` with `theme.border` (P1 Task 4, with tests over every built-in theme), because `surface_raised` equals `selected` in Mkrew Dark and Mkrew Light.

Exempt, with no hover: resize grips, terminal surfaces, drop zones and drag sources, the modal scrim, text fields (they keep their I-beam), editor and text-selection surfaces, window-drag areas, context-menu mouse downs on an element that is already pressable, and links drawn by `InteractiveText`.
```

- [ ] **Step 5: Commit**

```bash
git add docs/SPEC.md .claude/skills/mkrew-ui-design/SKILL.md .claude/agents/ui-reviewer.md docs/decisions/0097-glass-panels-and-pressable.md
git -c user.email=tungnv975@gmail.com commit -m "docs: the pressable rule in SPEC 14, the UI skill, the reviewer and ADR 0097"
```

---

### Task 11: Final sweep and verification

**Files:**
- Modify: `docs/HUMAN_CHECKS.md` (only to add what the sweep finds)
- Modify: any file the sweep shows still hand-builds a look

**Interfaces:**
- Consumes: everything above.
- Produces: the plan's end state.

- [ ] **Step 1: Re-run the inventory and compare it with the plan.**

```bash
grep -rnE '\.hover\(|\.active\(\|' crates/mkrew-ui/src --include='*.rs'
```

Expected: every hit is in `shell\widgets.rs` or `pressable.rs` (Windows prints backslashes).
A hit anywhere else is an element the tables missed: classify it with the Look chooser, convert it, and add it to the Task commit message of its area.

```bash
grep -rnE '\.cursor_pointer\(\)' crates/mkrew-ui/src --include='*.rs'
```

Expected: hits only in `shell\widgets.rs` and `pressable.rs`.
A pointer set by hand elsewhere is a click without a look: convert it, or add it to the exempt table with its reason.

```bash
grep -rnE '\.on_click\(|\.on_mouse_down\(' crates/mkrew-ui/src --include='*.rs'
```

Expected: every hit is (a) on a chain that starts at a W widget, (b) a row of the exempt table, (c) a `card_press` card, or (d) the kept mouse down of `docs.rs` (doc block) and `preview.rs` (source line).
Any other hit is a hand-built element that kept `.on_click(`: convert it.

This is an inventory for the person running the plan; it is not a test, and no test reads source text.

- [ ] **Step 2: Run the full verification.**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt check, clippy `-D warnings` and tests all green.
If the test build crashes rustc (STATUS_STACK_BUFFER_OVERRUN), run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo test --workspace -j 4`.

- [ ] **Step 3: Walk the visual checks once more across both themes.**
Open `docs/HUMAN_CHECKS.md`, section `### To verify - 2026-11-21 Pressable audit (P3)`: it must hold the bullets of Tasks 1 to 9.
Run the app and walk every bullet in dark and light, with attention to the five Review Focus items: a selected element still clicks, a hover is visible everywhere, no view panics at paint, the three busy or locked rows do nothing, and no card or chip stays dimmed after a press or a drag.
Screenshots of each area (`docs/screenshots/2026-11-21-pressable-<area>-dark.png` and `-light.png`) go in if they were captured; otherwise the bullets are the record.

- [ ] **Step 4: Commit anything the sweep changed.**

```bash
git status
git add <only the files this task changed>
git -c user.email=tungnv975@gmail.com commit -m "fix(ui): last hand-built click looks found by the pressable sweep"
```

If the sweep changed nothing, there is nothing to commit.

---

## Coverage of the spec

| Spec | Where |
| --- | --- |
| §5.1 the rule: every click or mouse-down element gets its look from `widgets.rs`; only `widgets.rs` calls `.hover(`/`.active(` | Tasks 1 to 9 convert; Task 11 checks; Task 10 writes it |
| §5.1 the rule written in SPEC §14, the skill and the reviewer checklist | Task 10 |
| §5.3 walk 327 handlers in 48 files by area: chrome and sidebar; tabs and panes; Board, Jira and all-workspaces board; Files and Docs; Git; inspector and thread; dialogs | Tasks 2 to 8, leftovers in Task 9; the Inventory tables assign every file |
| §5.3 non-buttons get no hover and are listed with the reason | The exempt table above |
| §9 P3 ends with `scripts/verify.sh` green and screenshots or HUMAN_CHECKS lines, dark and light | Each area task (screenshot step and bullets), Task 11 |
