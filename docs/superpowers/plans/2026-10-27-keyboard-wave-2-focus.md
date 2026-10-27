# Keyboard-only dialogs (wave 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every control of a modal dialog (buttons, chips, checkboxes, links) can be reached with Tab / Shift+Tab, shows a focus ring, and is pressed with Enter or Space; destructive confirmations run from the keyboard only after Tab moved onto them.

**Architecture:** gpui keeps the focus handle of an element that has `tab_index(0)` in its element state, so a control becomes a Tab stop with no field on `Shell` and no `use_keyed_state`.
A new extension trait `DialogControl` (`shell/widgets.rs`) adds the tab stop, the ring and the chip arrows; a pure module `dialog_focus` decides what a key does while a control or the dialog's text has the keyboard.
Each dialog's existing `on_key_down` calls `Shell::dialog_nav` first; the dialogs' text fields stay virtual (one `dialog_focus` handle plus a `field` / `focus` value in the model) and Tab walks them first, then the buttons, then wraps.

**Tech Stack:** Rust, GPUI (`gpui-pre` 0.3.7), existing `mkrew-ui` crate only; insta snapshots for the theme.
No protocol, storage or daemon change.

**Spec:** `docs/superpowers/specs/2026-10-27-keyboard-only-design.md` (section "Wave 2 - Focusable controls in dialogs").
Executors read the spec and this plan.
Waves run in order 1, 2, 3, 4 on master: wave 1 has landed before this plan starts, so line numbers below are from master before wave 1 and executors MUST re-locate each edit by the quoted text.

## Verified GPUI facts (gpui-pre-0.3.7)

- `tab_stop.rs`: `TabStopMap` orders stops by tab path, then by paint order. All handles here use index 0, so Tab order is paint order. `next` / `prev` wrap: past the last stop `next(None)` returns the first stop, before the first `prev(None)` returns the last.
- `elements/div.rs:809` `tab_index(i)` sets `focusable`, `tab_index` and `tab_stop = true`; at prepaint (`div.rs:2249-2263`) an element with an id and no tracked handle gets a handle stored in its element state (`.tab_stop(self.tab_stop)`), and paint inserts it into the frame's tab stops (`div.rs:2553`). So `tab_index(0)` on an id'd element is enough: no `use_keyed_state` (`window.rs:4156`) is needed. An explicit `track_focus(handle)` uses the handle's own `tab_stop` flag instead.
- `window.rs:2336` `focus_next` / `focus_prev` act on `rendered_frame.tab_stops` and call `window.focus`. They return silently when focus is disabled.
- There is no default Tab binding or action in gpui. Tab arrives as a `KeyDownEvent` (`key == "tab"`, `shift` for Shift+Tab) in the focused element's `on_key_down` chain, exactly like the existing dialog handlers receive it. No action binding and no `keymap.toml` entry is needed.
- `div.rs:1289` `focus_visible(f)` applies `f` only while the element is focused and `window.last_input_was_keyboard()` (`window.rs:3240`, set by `KeyDown`, cleared by `MouseDown` / `MouseMove`). So a mouse click never shows the ring.
- `div.rs:3010-3075`: a focused element with a click listener records `Enter` / `Space` on key down (`window.focus_generation`) and fires its click listeners with `ClickEvent::Keyboard` on the matching key up. Auto-repeating key downs record again, so a held Enter would click on release.
- Key listeners run capture phase root to leaf, then bubble leaf to root (`window.rs:6049-6078`). A `cx.stop_propagation()` in the capture phase skips the whole bubble phase, including gpui's own key-down recorder on the control. `KeyDownEvent::is_held` marks auto-repeat.
- `div.rs:2768`: a control with a tracked handle takes focus on mouse down. `Shell::modal_shade` (`shell/focus.rs:199`) then calls `focus_pane`, which focuses `dialog_focus` again (`focus.rs:111`), so after any click in a dialog the dialog itself has the keyboard, as today.
- A focused handle that is released (its element left the frame) leaves `window.focus` on a dead id; `window.focused(cx)` then returns `None` and key events skip the dialog's listeners. Task 3 adds a recovery in `Shell::render`.

## Global Constraints

- Every color, spacing, radius, font size and duration comes from `crates/mkrew-ui/src/theme.rs` tokens. No hex or raw pixel literal in components (mkrew-ui-design skill, `.claude/skills/mkrew-ui-design/SKILL.md`).
- No `unwrap` / `expect` outside tests unless provably infallible (comment why). `cargo fmt --all`; `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Pure logic gets unit tests (pattern: `crates/mkrew-ui/src/confirm.rs` tests). No test calls a real agent or the network. Do not write tests that pin wording, wiring or defaults.
- Fast loop: `cargo test -p mkrew-ui <filter>`. Full verify at the end of the wave: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL). On OOM or linker errors add `-j 2` to cargo.
- Commits: Conventional Commits (`feat(ui): ...`), `git -c user.email=tungnv975@gmail.com commit`, stage only the task's files, never push, never touch CHANGELOG.md.
- Docs for this wave: SPEC.md sections 13 and 14; append a section to `docs/decisions/0085-keyboard-only-operation.md` (wave 1 created it; re-check the number before writing); `docs/HUMAN_CHECKS.md` "To verify" entries for dark and light.
- Markdown: one sentence per physical line; never the em dash character, use "-".
- Destructive actions are never the default focus and never run on a bare Enter that opened the dialog; they run only when the keyboard was moved onto them (Tab) and then Enter or Space is pressed; auto-repeat does nothing.
- Every focusable element shows a visible focus ring from `theme.rs` tokens, only while keyboard focused (`focus_visible`), in both themes.
- New default keys must be unique across all commands. This wave adds no `Command`, so `keymap.toml` and `Command::all()` do not change.
- Cross-wave names (keep exact): `DialogControl::{focusable, focusable_bordered, chip_arrows, dialog_root}` in `shell/widgets.rs`; `Shell::{dialog_nav, dialog_guard}` in `shell/focus.rs`; `mkrew_ui::dialog_focus::{nav, Nav, blocks_repeat, group_step, Step}`; `Theme.focus_ring`. They were announced to the sibling plans.

## Review Focus

1. An auto-repeating Enter / Space (the Enter that opened a dialog, held a moment too long) must never click a focused control, least of all a destructive one: `blocks_repeat` test in Task 2.
2. Enter or Space on a focused Cancel / Close / Retry must not run the dialog's own Enter action (task Save, broadcast Send, Undo confirm), while Ctrl+Enter still does: `nav` tests in Task 2.
3. Tab at the end of a dialog's own field chain leaves for the buttons, Shift+Tab from the first field reaches the last button, and coming back lands on the first / last field: `at_edge` / `enter` tests in Tasks 3, 9 and 11.
4. In Broadcast, Space typed in the message must stay a space until an arrow key moved the keyboard to the session list: test in Task 7.
5. In the Run setup picker, Enter on a folder under the cursor takes that folder, Enter with no cursor still takes the listed folder, Enter on a folder in the env-file picker opens it instead of picking, and ← at the workspace root does nothing: tests in Task 9.
6. A focused control that disappears (a button swapped for its disabled twin while saving, the next step of the worktree dialog) must not strand the keyboard so that Esc stops working: no pure test is possible; the recovery in `Shell::render` is exercised in the Task 13 smoke run.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/mkrew-ui/src/dialog_focus.rs` (new) | Pure: `nav` (Tab / absorb / pass), `blocks_repeat`, `group_step` and their tests |
| `crates/mkrew-ui/src/shell/widgets.rs` | `DialogControl` trait: tab stop, ring, chip arrows, dialog root wiring |
| `crates/mkrew-ui/src/shell/focus.rs` | `Shell::dialog_nav`, `Shell::dialog_guard` |
| `crates/mkrew-ui/src/theme.rs` + `snapshots/` | `Theme.focus_ring` token and the two theme snapshots |
| `crates/mkrew-ui/src/main.rs`, `shell/connection.rs` | import, dead-focus recovery in `render`, `run_picker_scroll` field |
| `task_dialog.rs`, `broadcast.rs`, `run_setup.rs`, `settings_dialog.rs`, `turns_dialog.rs`, `undo_dialog.rs` | Pure dialog models: edge / enter, Space in the list, picker cursor, `d`, the new undo rule |
| `shell/task_dialog.rs`, `ask_dialog.rs`, `send_dialog.rs`, `dialogs.rs`, `phone_access.rs`, `workspace_actions.rs`, `git_dialogs.rs`, `worktrees.rs`, `run.rs`, `settings.rs`, `history.rs`, `git_render.rs` | Make each dialog's controls focusable and route its keys through `dialog_nav` |
| `docs/decisions/0085-keyboard-only-operation.md`, `docs/SPEC.md`, `docs/HUMAN_CHECKS.md` | Docs |

Dialogs not touched: Confirm prompt (already works), command palette, quick open, prompt picker, History (they have no buttons that need Tab).
`icon_button` and `segment` return `Stateful<Div>` too, so `DialogControl` covers them; no dialog uses them today (the only `icon_button` in a dialog file is the title bar's phone icon), so no call site changes.

---

### Task 1: `focus_ring` theme token

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (struct next to `pane_focus_border` ~line 688; constructors `dark` ~868, `light` ~961, `classic_dark` ~1053, `classic_light` ~1123, the all-zero constructor ~1199, `from_palette` ~1380)
- Modify: `crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_dark.snap`
- Modify: `crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_light.snap`

**Interfaces:**
- Produces: `pub focus_ring: u32` on `Theme`. Used by Task 3 (`DialogControl`).
- The ring is a 1 px border, so no width token is added: `theme::BORDER_WIDTH` already is that width.

- [ ] **Step 1: Add the field** - in the `Theme` struct, right after `pub pane_focus_border: u32,`:

```rust
    /// The ring of a keyboard-focused dialog control (ADR 0085): a 1 px border in the text
    /// color, so it reads on `surface` and on an accent button in every theme.
    pub focus_ring: u32,
```

- [ ] **Step 2: Set it in every constructor** - add one line right after each `pane_focus_border:` line, using that constructor's own `text` value:

```rust
            // Theme::dark (text: 0xECEDEF)
            focus_ring: 0xECEDEF,
            // Theme::light (text: 0x1B1D22)
            focus_ring: 0x1B1D22,
            // classic_dark (text: 0xE4E5E9)
            focus_ring: 0xE4E5E9,
            // classic_light (text: 0x1B1D22)
            focus_ring: 0x1B1D22,
            // the all-zero constructor (pane_focus_border: 0)
            focus_ring: 0,
            // from_palette (text: p.fg)
            focus_ring: p.fg,
```

Run: `cargo build -p mkrew-ui`
Expected: compiles; if a `Theme { .. }` literal was missed, rustc names it ("missing field `focus_ring`"); add the line there with that literal's `text` value.

- [ ] **Step 3: Run the golden test and see it fail**

Run: `cargo test -p mkrew-ui the_mkrew_themes_never_change`
Expected: FAIL, both snapshots differ only by the new `focus_ring:` lines.

- [ ] **Step 4: Accept the snapshots**

`cargo-insta` is not installed on this machine, so write the snapshots with insta's environment switch (equivalent to `cargo insta accept`):

Run: `INSTA_UPDATE=always cargo test -p mkrew-ui the_mkrew_themes_never_change`
Then: `git diff --stat crates/mkrew-ui/src/snapshots`
Expected: two files, 2 insertions each (the top-level theme and its nested `classic` theme), and nothing else changed:
`mkrew_dark.snap`: `focus_ring: 15527407,` (top) and `focus_ring: 15001065,` (classic).
`mkrew_light.snap`: `focus_ring: 1776930,` twice.
If `cargo insta` is available, `cargo insta accept` gives the same result.

- [ ] **Step 5: Re-run without the switch**

Run: `cargo test -p mkrew-ui theme`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_dark.snap crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_light.snap
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): focus ring theme token"
```

---

### Task 2: `dialog_focus` pure module

**Files:**
- Create: `crates/mkrew-ui/src/dialog_focus.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (add `pub mod dialog_focus;` after `pub mod diagram;`)

**Interfaces:**
- Produces (used by Tasks 3 to 11):
  - `pub enum Nav { Move(bool), Absorb, Pass }`
  - `pub fn nav(keystroke: &Keystroke, on_control: bool, edge: bool) -> Nav`
  - `pub fn blocks_repeat(keystroke: &Keystroke, held: bool) -> bool`
  - `pub enum Step { Prev, Next, Stay }`
  - `pub fn group_step(keystroke: &Keystroke, index: usize, len: usize) -> Option<Step>`

- [ ] **Step 1: Write the module with its tests first** - create `crates/mkrew-ui/src/dialog_focus.rs`:

```rust
//! Keyboard rules for the controls of a modal dialog (ADR 0085): which key moves the focus,
//! which keys a focused button takes, which auto-repeats are dropped. Pure and tested
//! headlessly; `shell/focus.rs` applies them and `shell/widgets.rs` draws the ring.

use gpui::Keystroke;

/// What a key press means to the dialog's controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    /// Tab (`true`) / Shift+Tab (`false`): focus the next / previous control.
    Move(bool),
    /// A focused control takes the key (Enter and Space click it, typing does nothing): the
    /// dialog's own key handler must not see it.
    Absorb,
    /// Not ours: the dialog's key handler decides (Esc, arrows, Ctrl+Enter, its own text).
    Pass,
}

/// `on_control`: a control, not the dialog's own text, has the keyboard. `edge`: the dialog's
/// own fields have none left in the Tab direction (always true for a dialog without fields), so
/// Tab leaves them for the controls.
pub fn nav(keystroke: &Keystroke, on_control: bool, edge: bool) -> Nav {
    let m = keystroke.modifiers;
    if m.control || m.alt || m.platform {
        return Nav::Pass;
    }
    match keystroke.key.as_str() {
        "tab" if on_control || edge => Nav::Move(!m.shift),
        _ if !on_control => Nav::Pass,
        "escape" | "up" | "down" | "left" | "right" | "home" | "end" | "pageup" | "pagedown" => {
            Nav::Pass
        }
        _ => Nav::Absorb,
    }
}

/// Whether a key down must be dropped before a focused control sees it. gpui clicks a focused
/// control on the release of an Enter / Space it saw pressed, and an auto-repeating key down
/// records again: the Enter that opened a dialog, held a moment too long, would click whatever
/// has focus. Only unmodified Enter / Space matter; a held key in a text field is typing and is
/// not routed here (the caller checks that a control has the keyboard).
pub fn blocks_repeat(keystroke: &Keystroke, held: bool) -> bool {
    held && !keystroke.modifiers.modified() && matches!(keystroke.key.as_str(), "enter" | "space")
}

/// Where ← / → send the keyboard inside a row of chips.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Prev,
    Next,
    /// At an end of the row: the key is taken and nothing moves.
    Stay,
}

/// What ← / → do on chip `index` of a row of `len`; `None` for every other key.
pub fn group_step(keystroke: &Keystroke, index: usize, len: usize) -> Option<Step> {
    if keystroke.modifiers.modified() {
        return None;
    }
    match keystroke.key.as_str() {
        "left" => Some(if index > 0 { Step::Prev } else { Step::Stay }),
        "right" => Some(if index + 1 < len {
            Step::Next
        } else {
            Step::Stay
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(keys: &str) -> Keystroke {
        Keystroke::parse(keys).unwrap()
    }

    #[test]
    fn tab_moves_from_a_control_and_from_the_edge_of_the_fields_only() {
        assert_eq!(nav(&press("tab"), true, false), Nav::Move(true));
        assert_eq!(nav(&press("shift-tab"), true, false), Nav::Move(false));
        assert_eq!(nav(&press("tab"), false, true), Nav::Move(true));
        assert_eq!(nav(&press("shift-tab"), false, true), Nav::Move(false));
        // In the middle of the fields the dialog's own Tab (the next field) acts.
        assert_eq!(nav(&press("tab"), false, false), Nav::Pass);
        assert_eq!(nav(&press("shift-tab"), false, false), Nav::Pass);
    }

    #[test]
    fn a_focused_control_takes_enter_space_and_typing_but_not_the_dialogs_keys() {
        for keys in ["enter", "space", "a", "backspace", "shift-a"] {
            assert_eq!(nav(&press(keys), true, false), Nav::Absorb, "{keys}");
        }
        for keys in [
            "escape",
            "up",
            "down",
            "left",
            "right",
            "home",
            "end",
            "pageup",
            "pagedown",
            "ctrl-enter",
            "ctrl-a",
            "ctrl-tab",
        ] {
            assert_eq!(nav(&press(keys), true, false), Nav::Pass, "{keys}");
        }
    }

    #[test]
    fn the_dialogs_own_text_keeps_every_key_but_the_edge_tab() {
        for keys in ["enter", "space", "a", "escape", "up", "ctrl-enter"] {
            assert_eq!(nav(&press(keys), false, true), Nav::Pass, "{keys}");
        }
    }

    #[test]
    fn only_an_unmodified_repeating_enter_or_space_is_dropped() {
        for keys in ["enter", "space"] {
            assert!(blocks_repeat(&press(keys), true), "{keys}");
            assert!(!blocks_repeat(&press(keys), false), "{keys}: the first press");
        }
        for keys in ["ctrl-enter", "shift-space", "escape", "tab", "a"] {
            assert!(!blocks_repeat(&press(keys), true), "{keys}");
        }
    }

    #[test]
    fn arrows_move_inside_a_row_of_chips_and_stop_at_its_ends() {
        let step = |keys: &str, index| group_step(&press(keys), index, 3);
        assert_eq!(step("left", 1), Some(Step::Prev));
        assert_eq!(step("right", 1), Some(Step::Next));
        assert_eq!(step("left", 0), Some(Step::Stay));
        assert_eq!(step("right", 2), Some(Step::Stay));
        assert_eq!(step("up", 1), None);
        assert_eq!(step("shift-left", 1), None);
        assert_eq!(step("enter", 1), None);
        assert_eq!(group_step(&press("right"), 0, 1), Some(Step::Stay), "a row of one");
    }
}
```

- [ ] **Step 2: Register it** - in `crates/mkrew-ui/src/lib.rs`, after `pub mod diagram;` add:

```rust
pub mod dialog_focus;
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p mkrew-ui dialog_focus`
Expected: 5 tests PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/mkrew-ui/src/dialog_focus.rs crates/mkrew-ui/src/lib.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): dialog focus rules (Tab, absorbed keys, held-key guard)"
```

---

### Task 3: Dialog focus plumbing and the task dialog

The plumbing has no consumer without a dialog (clippy `-D warnings` rejects unused items), so this task wires it into the first dialog.

**Files:**
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (new trait after `danger_button`, ~line 214)
- Modify: `crates/mkrew-ui/src/shell/focus.rs` (two methods in `impl Shell`, after `modal_open`)
- Modify: `crates/mkrew-ui/src/main.rs` (import ~line 63; `dialog_focus` field doc ~line 685; `render` ~line 710)
- Modify: `crates/mkrew-ui/src/task_dialog.rs` (`at_edge`, `enter`, test)
- Modify: `crates/mkrew-ui/src/shell/task_dialog.rs` (`task_dialog_key_down` ~line 55; `render_task_dialog` ~lines 241-504)

**Interfaces:**
- Consumes: `Theme.focus_ring` (Task 1), `dialog_focus::{nav, Nav, blocks_repeat, group_step, Step}` (Task 2).
- Produces (used by Tasks 4 to 11):
  - `trait DialogControl` implemented for `gpui::Stateful<gpui::Div>`:
    - `fn focusable(self, theme: &Theme) -> Self` - tab stop, ring, transparent 1 px border.
    - `fn focusable_bordered(self, theme: &Theme) -> Self` - tab stop and ring for a control that already draws a 1 px border.
    - `fn chip_arrows(self, index: usize, len: usize) -> Self`
    - `fn dialog_root(self, focus: &FocusHandle, has_text: bool, guard: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static, on_key: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static) -> Self`
  - `Shell::dialog_guard(&mut self, &KeyDownEvent, &mut Window, &mut Context<'_, Self>)` (a `cx.listener` target).
  - `Shell::dialog_nav(&mut self, event: &KeyDownEvent, edge: bool, window: &mut Window, cx: &mut Context<'_, Self>, enter: impl FnOnce(&mut Self, bool)) -> bool` - `true` = handled, the dialog's key handler returns.
  - `TaskDialog::at_edge(&self, forward: bool) -> bool`, `TaskDialog::enter(&mut self, forward: bool)`.
- Call-site recipe used by every later task: replace a panel's two lines
  `.track_focus(&self.dialog_focus)` and `.on_key_down(cx.listener(Self::<handler>))`
  with
  `.dialog_root(&self.dialog_focus, <has_text>, cx.listener(Self::dialog_guard), cx.listener(Self::<handler>))`;
  put `if self.dialog_nav(event, <edge>, window, cx, |_, _| {}) { return; }` first in the handler; append `.focusable(theme)` right after each enabled button constructor (before `.tooltip`).
  `has_text` is true when the dialog has its own text field (the root is then the first Tab stop, so Shift+Tab can return to it); `edge` is true when Tab always leaves the dialog's fields (one field or none).

- [ ] **Step 1: Write the failing test** - in `crates/mkrew-ui/src/task_dialog.rs`, inside `mod tests`, next to the other `SwitchField` tests:

```rust
    #[test]
    fn tab_leaves_the_fields_from_the_prompt_and_shift_tab_from_the_title() {
        let mut dialog = TaskDialog::new(Ulid::new());
        assert!(!dialog.at_edge(true), "Title to Prompt stays in the fields");
        assert!(dialog.at_edge(false), "Shift+Tab from the first field leaves");
        dialog.key(DialogKey::SwitchField);
        assert_eq!(dialog.field, Field::Prompt);
        assert!(dialog.at_edge(true), "Tab from the last field leaves");
        assert!(!dialog.at_edge(false));
        dialog.enter(true);
        assert_eq!(dialog.field, Field::Title, "Tab from the last button");
        dialog.enter(false);
        assert_eq!(dialog.field, Field::Prompt, "Shift+Tab from the first button");
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p mkrew-ui tab_leaves_the_fields`
Expected: FAIL to compile (`no method named at_edge`).

- [ ] **Step 3: Implement the model** - in `impl TaskDialog` (task_dialog.rs), after `selected_text`:

```rust
    /// Tab (`forward`) / Shift+Tab has no field left in that direction: the keyboard leaves the
    /// text fields for the buttons ([`crate::dialog_focus::nav`]). Between the title and the
    /// prompt [`DialogKey::SwitchField`] switches fields as before.
    pub fn at_edge(&self, forward: bool) -> bool {
        matches!(
            (self.field, forward),
            (Field::Prompt, true) | (Field::Title, false)
        )
    }

    /// The keyboard came back to the text fields from the buttons: Tab lands on the title,
    /// Shift+Tab on the prompt.
    pub fn enter(&mut self, forward: bool) {
        self.field = if forward { Field::Title } else { Field::Prompt };
    }
```

Run: `cargo test -p mkrew-ui tab_leaves_the_fields`
Expected: PASS.

- [ ] **Step 4: The extension trait** - in `shell/widgets.rs`, after `danger_button` and before `kbd`:

```rust
/// Keyboard focus for the controls of a modal dialog (ADR 0085). gpui keeps the focus handle
/// of an element that has a tab index in its element state, so a control needs no field on
/// `Shell`. Every control here is a `Stateful<Div>` ([`button`], [`secondary_button`],
/// [`quiet_button`], [`danger_button`], [`icon_button`], [`segment`], chips, check rows).
pub(crate) trait DialogControl: Sized {
    /// A Tab stop that shows the `focus_ring` while the keyboard has it. It keeps a 1 px
    /// border, transparent at rest, so the ring shifts nothing. Enter / Space click it (gpui).
    fn focusable(self, theme: &Theme) -> Self;
    /// [`DialogControl::focusable`] for a control that already draws its own 1 px border (a
    /// secondary button, a chip): only the ring color changes.
    fn focusable_bordered(self, theme: &Theme) -> Self;
    /// ← / → on a focused chip move the keyboard to its neighbour in a row of `len`; the
    /// ends keep it.
    fn chip_arrows(self, index: usize, len: usize) -> Self;
    /// The panel of a modal dialog: it has the keyboard through `focus` (a Tab stop when the
    /// dialog has `has_text` fields, so Shift+Tab can come back to them), `guard` runs first
    /// in the capture phase ([`Shell::dialog_guard`]) and `on_key` gets every key.
    fn dialog_root(
        self,
        focus: &FocusHandle,
        has_text: bool,
        guard: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
        on_key: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self;
}

impl DialogControl for gpui::Stateful<gpui::Div> {
    fn focusable(self, theme: &Theme) -> Self {
        self.border(px(theme::BORDER_WIDTH))
            .border_color(gpui::transparent_black())
            .focusable_bordered(theme)
    }

    fn focusable_bordered(self, theme: &Theme) -> Self {
        let ring = rgb(theme.focus_ring);
        self.tab_index(0)
            .focus_visible(move |style| style.border_color(ring))
    }

    fn chip_arrows(self, index: usize, len: usize) -> Self {
        self.on_key_down(move |event, window, cx| {
            let Some(step) = dialog_focus::group_step(&event.keystroke, index, len) else {
                return;
            };
            cx.stop_propagation();
            match step {
                dialog_focus::Step::Prev => window.focus_prev(cx),
                dialog_focus::Step::Next => window.focus_next(cx),
                dialog_focus::Step::Stay => {}
            }
        })
    }

    fn dialog_root(
        self,
        focus: &FocusHandle,
        has_text: bool,
        guard: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
        on_key: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.track_focus(&focus.clone().tab_stop(has_text))
            .capture_key_down(guard)
            .on_key_down(on_key)
    }
}
```

- [ ] **Step 5: The shell side** - in `shell/focus.rs`, inside `impl Shell`, after `modal_open`:

```rust
    /// Drops an auto-repeating Enter / Space while a control has the keyboard: gpui clicks a
    /// focused control on the release of an Enter / Space it saw pressed, so the Enter that
    /// opened a dialog, held a moment too long, must not click what has focus
    /// ([`dialog_focus::blocks_repeat`]). In the dialog's own text a held Enter is typing.
    pub(crate) fn dialog_guard(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if !self.dialog_focus.is_focused(window)
            && dialog_focus::blocks_repeat(&event.keystroke, event.is_held)
        {
            cx.stop_propagation();
        }
    }

    /// The dialog-level keys of `event` ([`dialog_focus::nav`]): Tab / Shift+Tab move to the
    /// next / previous control and wrap inside the dialog, and a focused control takes Enter,
    /// Space and typing. `edge`: the dialog's own fields have none left in that direction. When
    /// the move lands back on the dialog's own text, `enter(self, forward)` puts its caret on the
    /// first (Tab) or last (Shift+Tab) field. `true` = handled: the dialog's own key handler
    /// must return.
    pub(crate) fn dialog_nav(
        &mut self,
        event: &KeyDownEvent,
        edge: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
        enter: impl FnOnce(&mut Self, bool),
    ) -> bool {
        let on_control = !self.dialog_focus.is_focused(window);
        match dialog_focus::nav(&event.keystroke, on_control, edge) {
            Nav::Pass => false,
            Nav::Absorb => {
                cx.stop_propagation();
                true
            }
            Nav::Move(forward) => {
                cx.stop_propagation();
                if forward {
                    window.focus_next(cx);
                } else {
                    window.focus_prev(cx);
                }
                if self.dialog_focus.is_focused(window) {
                    enter(self, forward);
                }
                cx.notify();
                true
            }
        }
    }
```

- [ ] **Step 6: `main.rs`** - three edits.

Import, after `use mkrew_ui::desktop::{Desktop, TrayEvent};`:

```rust
use mkrew_ui::dialog_focus::{self, Nav};
```

The field doc at `dialog_focus: FocusHandle,`:

```rust
    /// Focused while a dialog is open; the dialog's keys edit its draft. A dialog's panel tracks
    /// it (`DialogControl::dialog_root`); its buttons take the keyboard with Tab.
    dialog_focus: FocusHandle,
```

In `Shell::render`, right after `window.set_rem_size(px(cx.global::<theme::Fonts>().ui_size));`:

```rust
        // A dialog control that vanished while focused (a button swapped for its disabled twin
        // while saving, the next step of a dialog) would leave the keyboard on nothing and Esc
        // would miss the dialog: the dialog itself takes the keyboard back.
        if self.modal_open() && window.focused(cx).is_none() {
            window.focus(&self.dialog_focus, cx);
        }
```

- [ ] **Step 7: The task dialog's keys** - in `shell/task_dialog.rs`, replace the doc comment and the start of `task_dialog_key_down`:

```rust
    /// Keys typed in the task dialog edit the focused field; Tab / Shift+Tab move between the
    /// title and the prompt, then through the buttons; Enter (title) or Ctrl+Enter saves, Esc
    /// cancels.
    pub(crate) fn task_dialog_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let edge = (self.task_dialog.as_ref())
            .is_some_and(|dialog| dialog.at_edge(!event.keystroke.modifiers.shift));
        if self.dialog_nav(event, edge, window, cx, |shell, forward| {
            if let Some(dialog) = shell.task_dialog.as_mut() {
                dialog.enter(forward);
            }
        }) {
            return;
        }
        cx.stop_propagation();
```

(The rest of the function stays.)

- [ ] **Step 8: The task dialog's controls** - in `render_task_dialog`:

Agent chips: replace the `Agents::Loaded(profiles) => dialog.agent_choices(profiles, default_agent).into_iter().map(|choice| { ... }).collect(),` arm with the same body made a focusable row. The element chain keeps every existing call; only the wrapper and the last two calls are new:

```rust
            Agents::Loaded(profiles) => {
                let choices = dialog.agent_choices(profiles, default_agent);
                let len = choices.len();
                choices
                    .into_iter()
                    .enumerate()
                    .map(|(index, choice)| {
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
                                    cx.notify();
                                }
                            }))
                            .focusable_bordered(theme)
                            .chip_arrows(index, len)
                            .into_any_element()
                    })
                    .collect()
            }
```

"Run in a worktree" row: inside `.when(!locked, |el| { ... })`, add `.focusable(theme)` after the `.on_click(...)` call (a locked row is not a stop):

```rust
            .when(!locked, |el| {
                el.cursor_pointer()
                    .text_color(rgb(theme.text))
                    .hover(move |style| style.bg(rgb(hover)))
                    .active(|style| style.opacity(opacity::ACTIVE))
                    .on_click(cx.listener(|shell, _, _, cx| {
                        if let Some(dialog) = shell.task_dialog.as_mut() {
                            dialog.toggle_worktree();
                        }
                        cx.notify();
                    }))
                    .focusable(theme)
            })
```

Buttons (add `.focusable(theme)` right after the constructor; enabled branches only, the `disabled_button` twins stay):

```rust
button("task-from-prompt", "From prompt…", theme, false)
    .focusable(theme)
button("task-delete", "Delete", theme, false)
    .focusable(theme)
button("task-cancel", "Cancel", theme, false)
    .focusable(theme)
button("task-save", save_label, theme, true)
    .focusable(theme)
```

Dialog root: replace the `.when(self.prompt_picker.is_none(), |el| { ... })` block with:

```rust
            // The prompt picker over it has the keyboard meanwhile.
            .when(self.prompt_picker.is_none(), |el| {
                el.dialog_root(
                    &self.dialog_focus,
                    true,
                    cx.listener(Self::dialog_guard),
                    cx.listener(Self::task_dialog_key_down),
                )
                .relative()
                .child(self.typing(&self.dialog_focus, Self::task_dialog_key_down, cx))
            })
```

- [ ] **Step 9: Build and lint**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.
Run: `cargo test -p mkrew-ui task_dialog`
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/shell/focus.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/task_dialog.rs crates/mkrew-ui/src/shell/task_dialog.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): task dialog controls take Tab and show a focus ring"
```

---

### Task 4: Ask, Send and Docs dialogs

**Files:**
- Modify: `crates/mkrew-ui/src/shell/git_render.rs` (`chip_row` ~lines 567-615 and its Git caller ~line 549)
- Modify: `crates/mkrew-ui/src/shell/ask_dialog.rs` (`ask_key_down` ~line 47; `render_ask` chips, footer, root)
- Modify: `crates/mkrew-ui/src/shell/send_dialog.rs` (`doc_dialog_key_down` ~line 92; `doc_dialog_panel` ~line 235; `render_send_dialog` footer; `render_changes` close)
- `crates/mkrew-ui/src/ask.rs` needs no change: it already ignores `SwitchField` (line 95).

**Interfaces:**
- Consumes: `DialogControl`, `Shell::dialog_nav`, `Shell::dialog_guard` (Task 3).
- Produces: `Shell::chip_row(heading, chips, tab_stops: bool, theme, on_pick, cx)`.

- [ ] **Step 1: `chip_row` takes `tab_stops`** - replace the function in `git_render.rs` (the Git "Showing" row passes `false`: it is not in a dialog):

```rust
    /// A chooser row: `heading`, then one chip per choice. A click on an inactive chip calls
    /// `on_pick` with its `pick`. With `tab_stops` (a dialog's row) every chip is a Tab stop
    /// with the focus ring, ← / → move between the chips of the row and Enter / Space pick.
    pub(crate) fn chip_row<P: Clone + 'static>(
        heading: &'static str,
        chips: Vec<Chip<P>>,
        tab_stops: bool,
        theme: &Theme,
        on_pick: impl Fn(&mut Self, P, &mut Window, &mut Context<'_, Self>) + Clone + 'static,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let hover = theme.surface;
        let len = chips.len();
        let chips = chips.into_iter().enumerate().map(|(index, chip)| {
            let Chip {
                id,
                label,
                tip,
                active,
                pick,
            } = chip;
            let on_pick = on_pick.clone();
            div()
                .id(id)
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .tooltip(tooltip(theme, &tip, ""))
                .when(active, |el| {
                    el.bg(rgb(theme.surface_raised))
                        .font_weight(FontWeight(weight::MEDIUM))
                        .text_color(rgb(theme.text))
                })
                .when(!active, |el| {
                    el.cursor_pointer()
                        .text_color(rgb(theme.text_muted))
                        .hover(move |style| style.bg(rgb(hover)))
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            on_pick(shell, pick.clone(), window, cx);
                        }))
                })
                .when(tab_stops, |el| el.focusable(theme).chip_arrows(index, len))
                .child(label)
        });
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(space::XS))
            .text_size(font_size::META)
            .child(div().text_color(rgb(theme.text_muted)).child(heading))
            .children(chips)
    }
```

In the Git caller (`Some(Self::chip_row("Showing", chips, theme, ...`) add the argument after `chips,`:

```rust
        Some(Self::chip_row(
            "Showing",
            chips,
            false,
            theme,
```

- [ ] **Step 2: Ask dialog** - in `ask_dialog.rs`:

Both `Self::chip_row(` calls ("Agent" and "Web search") get `true,` after `chips,`:

```rust
                Self::chip_row(
                    "Agent",
                    chips,
                    true,
                    theme,
```

```rust
            Self::chip_row(
                "Web search",
                chips,
                true,
                theme,
```

Keys, at the top of `ask_key_down` (one text field, so Tab always leaves it):

```rust
        if self.dialog_nav(event, true, window, cx, |_, _| {}) {
            return;
        }
        cx.stop_propagation();
```

Footer buttons (`.focusable(theme)` right after each constructor):

```rust
button("ask-copy", "Copy answer", theme, false)
    .focusable(theme)
button("ask-close", "Close", theme, false)
    .focusable(theme)
button("ask-send", ask_label, theme, true)
    .focusable(theme)
```

Root: replace

```rust
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::ask_key_down))
```

with

```rust
            .dialog_root(
                &self.dialog_focus,
                true,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::ask_key_down),
            )
```

- [ ] **Step 3: Send and Docs dialogs** - in `send_dialog.rs`:

`doc_dialog_key_down` (no text field: Tab always leaves), first lines:

```rust
        if self.dialog_nav(event, true, window, cx, |_, _| {}) {
            return;
        }
        cx.stop_propagation();
```

`doc_dialog_panel` root:

```rust
            .dialog_root(
                &self.dialog_focus,
                false,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::doc_dialog_key_down),
            )
```

Buttons, `.focusable(theme)` right after the constructor:

```rust
button("send-close", "Close", theme, false)
    .focusable(theme)
button("send-terminals", "Show terminals", theme, true)
    .focusable(theme)
button("send-cancel", "Cancel", theme, false)
    .focusable(theme)
button("send-confirm", "Open composer", theme, true)
    .focusable(theme)
button("send-confirm", "Send", theme, true)
    .focusable(theme)
button("changes-close", "Close", theme, true)
    .focusable(theme)
```

The dialog opens with the keyboard on the dialog itself, where ↑ / ↓ pick the session and Enter sends, as before; Tab reaches Cancel, then Send.

- [ ] **Step 4: Build and lint**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/shell/git_render.rs crates/mkrew-ui/src/shell/ask_dialog.rs crates/mkrew-ui/src/shell/send_dialog.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Ask, Send and Docs dialog controls take Tab"
```

---

### Task 5: Destructive confirmations (Undo, Git discard, worktree, Compare)

This task changes the rule at `undo_dialog.rs:282`: discarding no longer "takes an explicit click".

**Files:**
- Modify: `crates/mkrew-ui/src/undo_dialog.rs` (`cancels` doc ~line 282; test `only_escape_cancels` ~line 509)
- Modify: `crates/mkrew-ui/src/shell/dialogs.rs` (`undo_dialog_key_down` ~line 136; `render_undo_dialog` buttons and root ~lines 375-455)
- Modify: `crates/mkrew-ui/src/shell/git_dialogs.rs` (`git_discard_key_down` ~line 28; `render_git_discard` ~lines 225-360)
- Modify: `crates/mkrew-ui/src/shell/worktrees.rs` (`compare_key_down` ~line 91; `worktree_dialog_key_down` ~line 248; `render_worktree_dialog` ~lines 430-660; `render_compare` ~lines 672-825)

**Interfaces:**
- Consumes: Task 3 recipe. None of these dialogs has a text field: `has_text = false`, `edge = true`.
- The keyboard starts on the dialog itself, where Enter and Space do nothing, so no destructive button is ever the first focus.

- [ ] **Step 1: Replace the rule and its test** - in `undo_dialog.rs`:

```rust
/// Esc closes the dialog. Enter and Space on the dialog itself do nothing: discarding runs only
/// from the focused "Discard changes" button, after Tab moved the keyboard onto it (ADR 0085).
pub fn cancels(keystroke: &Keystroke) -> bool {
    keystroke.key == "escape"
}
```

```rust
    #[test]
    fn only_escape_cancels() {
        assert!(cancels(&Keystroke::parse("escape").unwrap()));
        for keys in ["enter", "space", "ctrl-enter"] {
            assert!(!cancels(&Keystroke::parse(keys).unwrap()), "{keys}");
        }
    }
```

Run: `cargo test -p mkrew-ui undo_dialog`
Expected: PASS.

- [ ] **Step 2: Undo dialog** - in `shell/dialogs.rs`:

```rust
    pub(crate) fn undo_dialog_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.dialog_nav(event, true, window, cx, |_, _| {}) {
            return;
        }
        cx.stop_propagation();
        if undo_dialog::cancels(&event.keystroke) {
            self.close_undo_dialog(window, cx);
        }
    }
```

In `render_undo_dialog`, the root:

```rust
            .dialog_root(
                &self.dialog_focus,
                false,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::undo_dialog_key_down),
            )
```

and the buttons (`.focusable(theme)` after the constructor; the `Err(_)` disabled twin stays):

```rust
button("undo-close", "Close", theme, true)
    .focusable(theme)
button("undo-cancel", "Cancel", theme, false)
    .focusable(theme)
Ok(_) => danger_button("undo-confirm", "Discard changes", theme)
    .focusable(theme)
```

Order on screen is Cancel, then Discard changes, so Tab, Tab, Enter discards and nothing else does.

- [ ] **Step 3: Git discard** - in `shell/git_dialogs.rs`, `git_discard_key_down`:

```rust
        if self.dialog_nav(event, true, window, cx, |_, _| {}) {
            return;
        }
        cx.stop_propagation();
```

In `render_git_discard`: root

```rust
            .dialog_root(
                &self.dialog_focus,
                false,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::git_discard_key_down),
            )
```

buttons:

```rust
button("git-discard-retry", "Retry", theme, false)
    .focusable(theme)
button("git-discard-cancel", "Cancel", theme, false)
    .focusable(theme)
danger_button("git-discard-confirm", "Discard changes", theme)
    .focusable(theme)
```

- [ ] **Step 4: Worktree and Compare** - in `shell/worktrees.rs`:

`compare_key_down` and `worktree_dialog_key_down` each start with the nav call (before their `cx.stop_propagation();`):

```rust
        if self.dialog_nav(event, true, window, cx, |_, _| {}) {
            return;
        }
```

Roots (`render_worktree_dialog` and `render_compare`):

```rust
            .dialog_root(
                &self.dialog_focus,
                false,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::worktree_dialog_key_down),
            )
```

```rust
            .dialog_root(
                &self.dialog_focus,
                false,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::compare_key_down),
            )
```

Worktree buttons (`.focusable(theme)` immediately after the constructor, enabled branches only):

```rust
button("worktree-retry", "Retry", theme, false)
    .focusable(theme)
button("worktree-git", "Open Git view", theme, false)
    .focusable(theme)
// the `cancel` closure:
button(id, text, theme, false)
    .focusable(theme)
danger_button("worktree-remove-others", offer, theme)
    .focusable(theme)
button("worktree-keep", "Keep files", theme, false)
    .focusable(theme)
danger_button("worktree-confirm", "Discard files", theme)
    .focusable(theme)
danger_button("worktree-discard", "Discard…", theme)
    .focusable(theme)
(true, true) => danger_button("worktree-confirm", text, theme)
    .focusable(theme)
(true, false) => button("worktree-confirm", text, theme, true)
    .focusable(theme)
```

Compare buttons (each column is under its own `compare-<task id>` parent, so the repeated element ids stay unique):

```rust
quiet_button("compare-git", "Git view", theme, theme.border)
    .focusable(theme)
quiet_button("compare-pick", "Pick", theme, theme.border)
    .focusable(theme)
button("compare-close", "Close", theme, false)
    .focusable(theme)
```

When "Discard…" moves the worktree dialog to its next step, the focused button leaves the frame; `Shell::render` returns the keyboard to the dialog (Task 3). Tab then reaches "Keep files" first, the destructive "Discard files" second.

- [ ] **Step 5: Build and lint**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/undo_dialog.rs crates/mkrew-ui/src/shell/dialogs.rs crates/mkrew-ui/src/shell/git_dialogs.rs crates/mkrew-ui/src/shell/worktrees.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): destructive confirmations run from the keyboard after Tab"
```

---

### Task 6: Turns `d` = Diff since

**Files:**
- Modify: `crates/mkrew-ui/src/turns_dialog.rs` (`Key`, `key`, test ~lines 145-160, 303)
- Modify: `crates/mkrew-ui/src/shell/history.rs` (`turns_dialog_key_down` ~line 119; the `turn-diff` tooltip ~line 534)

**Interfaces:**
- Produces: `turns_dialog::Key::DiffSince`.
- The row buttons stay mouse-only (up to dozens of rows); the selected row gets the key instead.

- [ ] **Step 1: Write the failing test** - replace the key test in `turns_dialog.rs` (the block asserting `parse("escape")`, `parse("up")`, ...):

```rust
        let parse = |text| key(&Keystroke::parse(text).unwrap());
        assert_eq!(parse("escape"), Some(Key::Close));
        assert_eq!(parse("up"), Some(Key::Move(-1)));
        assert_eq!(parse("down"), Some(Key::Move(1)));
        assert_eq!(parse("enter"), Some(Key::Restore));
        assert_eq!(parse("d"), Some(Key::DiffSince));
        assert_eq!(parse("ctrl-d"), None);
        assert_eq!(parse("ctrl-enter"), None);
        assert_eq!(parse("a"), None);
```

Run: `cargo test -p mkrew-ui turns_dialog`
Expected: FAIL to compile (`no variant DiffSince`).

- [ ] **Step 2: Implement** - in `turns_dialog.rs`:

```rust
/// Esc closes the dialog; ↑ / ↓ pick a turn; Enter restores before it; D shows the changes
/// since it started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Close,
    Move(isize),
    Restore,
    DiffSince,
}

pub fn key(keystroke: &Keystroke) -> Option<Key> {
    let modified = keystroke.modifiers.modified();
    match keystroke.key.as_str() {
        "escape" => Some(Key::Close),
        "up" if !modified => Some(Key::Move(-1)),
        "down" if !modified => Some(Key::Move(1)),
        "enter" if !modified => Some(Key::Restore),
        "d" if !modified => Some(Key::DiffSince),
        _ => None,
    }
}
```

Run: `cargo test -p mkrew-ui turns_dialog`
Expected: PASS.

- [ ] **Step 3: The shell** - in `history.rs`, `turns_dialog_key_down`, add an arm before `None => {}`:

```rust
            Some(turns_dialog::Key::DiffSince) => {
                let Some(dialog) = &self.turns_dialog else {
                    return;
                };
                let diffable = match &dialog.listing {
                    turns_dialog::Listing::Loaded(turns) => turns
                        .get(dialog.selected)
                        .is_some_and(|turn| turns_dialog::diff_blocked(turn).is_none()),
                    _ => false,
                };
                if let Some(n) = dialog.selected_turn().filter(|_| diffable) {
                    self.diff_since_turn(n, window, cx);
                }
            }
```

and the tooltip of the enabled "Diff since" button shows the key on the selected row, like "Restore before" shows Enter:

```rust
                                None => button("turn-diff", "Diff since", theme, false)
                                    .tooltip(tooltip(
                                        theme,
                                        &format!("Show the changes since turn {n} started"),
                                        if selected { "D" } else { "" },
                                    ))
```

- [ ] **Step 4: Build and lint**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/turns_dialog.rs crates/mkrew-ui/src/shell/history.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): D shows the diff since the selected turn"
```

---

### Task 7: Broadcast (Space ticks, Tab leaves)

**Files:**
- Modify: `crates/mkrew-ui/src/broadcast.rs` (`Broadcast` struct ~line 73, `new` ~line 96, `key` ~line 128, tests ~lines 313-345)
- Modify: `crates/mkrew-ui/src/shell/dialogs.rs` (`broadcast_key_down` ~line 28; `render_broadcast` ~lines 460-638)

**Interfaces:**
- Produces: `Broadcast.in_list: bool` (the keyboard is on the session list: Space checks the cursor row; any other key returns it to the message).
- Tab is the dialog's focus key now ([`Shell::dialog_nav`]); it no longer ticks.

- [ ] **Step 1: Write the failing tests** - in `broadcast.rs` tests, replace the first lines of `keys_move_the_cursor_toggle_rows_and_edit_the_text` (up to and including the second `press(&mut dialog, "tab"); assert!(dialog.rows[1].checked);`) with:

```rust
        let mut dialog = two();
        assert_eq!(press(&mut dialog, "up"), Outcome::Edited);
        assert_eq!(dialog.cursor, 0);
        press(&mut dialog, "down");
        press(&mut dialog, "down");
        assert_eq!(dialog.cursor, 1);
        // Space checks the row the arrows picked; Tab is the dialog's focus key, not a tick.
        assert_eq!(press(&mut dialog, "space"), Outcome::Edited);
        assert!(!dialog.rows[1].checked);
        press(&mut dialog, "space");
        assert!(dialog.rows[1].checked);
        assert_eq!(press(&mut dialog, "tab"), Outcome::Edited);
        assert!(dialog.rows[1].checked);
```

(The test continues unchanged from `dialog.toggle(0);`.)

Add a new test after it:

```rust
    #[test]
    fn space_is_a_space_in_the_message_until_an_arrow_key_picks_the_list() {
        let mut dialog = two();
        let space = || {
            let mut key = Keystroke::parse("space").unwrap();
            key.key_char = Some(" ".into());
            key
        };
        let letter = |c: &str| {
            let mut key = Keystroke::parse("a").unwrap();
            key.key = c.into();
            key.key_char = Some(c.into());
            key
        };
        // Typing: Space is a space and nothing is unchecked.
        dialog.key(&letter("a"));
        dialog.key(&space());
        assert_eq!(dialog.text, "a ");
        assert!(dialog.rows.iter().all(|row| row.checked));
        // An arrow key hands the keyboard to the list: Space checks, types nothing.
        press(&mut dialog, "down");
        assert!(dialog.in_list);
        dialog.key(&space());
        assert!(!dialog.rows[1].checked);
        assert_eq!(dialog.text, "a ");
        // Typing returns to the message.
        dialog.key(&letter("b"));
        assert!(!dialog.in_list);
        dialog.key(&space());
        assert_eq!(dialog.text, "a b ");
        assert!(!dialog.rows[1].checked, "still unchecked, not toggled back");
    }
```

Run: `cargo test -p mkrew-ui broadcast`
Expected: FAIL to compile (`no field in_list`).

- [ ] **Step 2: Implement** - in `broadcast.rs`:

Struct:

```rust
/// The open dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Broadcast {
    pub project_id: Ulid,
    pub rows: Vec<Row>,
    /// The text being typed (one line; Enter sends).
    pub text: Input,
    /// The row ↑ / ↓ move and Space toggles.
    pub cursor: usize,
    /// ↑ / ↓ handed the keyboard to the session list (Space checks the cursor row); typing
    /// takes it back (Space is a space).
    pub in_list: bool,
}
```

In `Broadcast::new`, the struct literal gains `in_list: false,` after `cursor: 0,`.

`key`:

```rust
    /// Applies a key press: ↑ / ↓ move the cursor and hand the keyboard to the list, where Space
    /// checks the cursor's row; any other key goes back to the text. Tab is the dialog's focus
    /// key (`Shell::dialog_nav`), not a tick.
    pub fn key(&mut self, keystroke: &Keystroke) -> Outcome {
        if !keystroke.modifiers.modified() {
            match keystroke.key.as_str() {
                "up" => {
                    self.cursor = self.cursor.saturating_sub(1);
                    self.in_list = true;
                    return Outcome::Edited;
                }
                "down" => {
                    self.cursor = (self.cursor + 1).min(self.rows.len().saturating_sub(1));
                    self.in_list = true;
                    return Outcome::Edited;
                }
                "space" if self.in_list => {
                    self.toggle(self.cursor);
                    return Outcome::Edited;
                }
                _ => {}
            }
        }
        self.in_list = false;
        match dialog_key(keystroke) {
            Some(DialogKey::Cancel) => Outcome::Close,
            Some(DialogKey::Enter | DialogKey::Save) => Outcome::Send,
            Some(DialogKey::Paste) => Outcome::Paste,
            Some(DialogKey::Insert(text)) => {
                self.insert(&text);
                Outcome::Edited
            }
            Some(DialogKey::Copy | DialogKey::SwitchField) | None => Outcome::Edited,
            // The caller copies the selection before Cut.
            Some(key) => {
                if !self.busy() {
                    self.text.edit(&key);
                }
                Outcome::Edited
            }
        }
    }
```

Also update the `Outcome::Send` doc in that file if it mentions Tab (it says "Enter: send to every checked session", unchanged).

Run: `cargo test -p mkrew-ui broadcast`
Expected: PASS.

- [ ] **Step 3: The shell** - in `shell/dialogs.rs`:

`broadcast_key_down`, first lines (the message is the only field: Tab always leaves it):

```rust
        if self.dialog_nav(event, true, window, cx, |_, _| {}) {
            return;
        }
        cx.stop_propagation();
```

Also fix its doc comment: "Typing edits the text, ↑ / ↓ pick a session, Space checks it, Tab moves to the buttons, Enter sends, Esc closes."

`render_broadcast` changes:

- The cursor ring only while the list has the keyboard:

```rust
                .when(dialog.in_list && index == dialog.cursor, |el| {
                    el.border_color(rgb(theme.accent))
                })
```

- Row tooltip key: `.tooltip(tooltip(theme, "Check or uncheck", "Space"))`.
- The hint line: `.child("↑↓ pick a session · Space checks it · Tab to the buttons · Enter sends")`.
- Footer buttons (`.focusable(theme)` right after the constructor):

```rust
button("broadcast-close", "Close", theme, false)
    .focusable(theme)
button("broadcast-terminals", "Show terminals", theme, true)
    .focusable(theme)
button("broadcast-cancel", "Cancel", theme, false)
    .focusable(theme)
button("broadcast-send", send_label, theme, true)
    .focusable(theme)
```

- Root (the message field only exists while there are sessions):

```rust
            .dialog_root(
                &self.dialog_focus,
                !dialog.rows.is_empty(),
                cx.listener(Self::dialog_guard),
                cx.listener(Self::broadcast_key_down),
            )
```

- [ ] **Step 4: Build and lint**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/broadcast.rs crates/mkrew-ui/src/shell/dialogs.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): broadcast ticks with Space and Tab reaches the buttons"
```

---

### Task 8: Phone access, Add workspace, Git base

**Files:**
- Modify: `crates/mkrew-ui/src/shell/phone_access.rs` (`phone_access_key_down` ~line 30; `render_phone_access` ~lines 192-410)
- Modify: `crates/mkrew-ui/src/shell/workspace_actions.rs` (`add_workspace_key_down` ~line 34; `render_add_workspace` ~lines 772-893)
- Modify: `crates/mkrew-ui/src/shell/git_dialogs.rs` (`git_base_key_down` ~line 126; `render_git_base` ~lines 440-484)
- `base_dialog.rs` needs no change: `Tab` already maps to `None` there (`base_dialog::key`).

**Interfaces:**
- Consumes: Task 3 recipe. Phone has no text field (`has_text = false`); Add workspace and Git base each have one text field (`has_text = true`, `edge = true`).
- Phone: Start becomes Stop (a different element id) after a click, so the keyboard returns to the dialog; Tab reaches the new button first.

- [ ] **Step 1: Phone access** - `phone_access_key_down`:

```rust
        if self.dialog_nav(event, true, window, cx, |_, _| {}) {
            return;
        }
        cx.stop_propagation();
        if undo_dialog::cancels(&event.keystroke) {
            self.close_phone_access(window, cx);
        }
```

Root in `render_phone_access`:

```rust
            .dialog_root(
                &self.dialog_focus,
                false,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::phone_access_key_down),
            )
```

Buttons (`.focusable(theme)` after the constructor, in this order on screen):

```rust
Switch::Start => button("phone-start", "Start", theme, true)
    .focusable(theme)
Switch::Stop => button("phone-stop", "Stop", theme, false)
    .focusable(theme)
SetUp::Ready => button("phone-setup", "Set up", theme, false)
    .focusable(theme)
PushSwitch::TurnOn => button("phone-push-on", "Turn on", theme, false)
    .focusable(theme)
PushSwitch::TurnOff => button("phone-push-off", "Turn off", theme, false)
    .focusable(theme)
button("phone-copy", if phone.copied { "Copied" } else { "Copy link" }, theme, false)
    .focusable(theme)
button("phone-close", "Close", theme, false)
    .focusable(theme)
```

(`phone-copy` keeps its multi-line argument layout; only `.focusable(theme)` is added before `.tooltip(`.)

- [ ] **Step 2: Add workspace** - `add_workspace_key_down`:

```rust
        if self.dialog_nav(event, true, window, cx, |_, _| {}) {
            return;
        }
        cx.stop_propagation();
```

Root:

```rust
            .dialog_root(
                &self.dialog_focus,
                true,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::add_workspace_key_down),
            )
```

Controls (the secondary button already draws a border):

```rust
secondary_button(
    "add-workspace-folder",
    icon_text(Icon::FolderPlus, "Open a folder…", theme.text_dim),
    theme,
)
.focusable_bordered(theme)
quiet_button("add-workspace-browse", "Browse…", theme, theme.hover)
    .focusable(theme)
button("add-workspace-cancel", "Cancel", theme, false)
    .focusable(theme)
button("add-workspace-clone", "Clone", theme, true)
    .focusable(theme)
```

Update the doc comment of `add_workspace_key_down` to "Typing edits the URL, Enter clones, Tab reaches Open a folder, Browse and the buttons, Esc closes (a running clone goes on)."

- [ ] **Step 3: Git base** - `git_base_key_down`:

```rust
        if self.dialog_nav(event, true, window, cx, |_, _| {}) {
            return;
        }
        cx.stop_propagation();
```

Root:

```rust
            .dialog_root(
                &self.dialog_focus,
                true,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::git_base_key_down),
            )
```

Buttons:

```rust
button("git-base-cancel", "Cancel", theme, false)
    .focusable(theme)
button("git-base-auto", "Auto-detect", theme, false)
    .focusable(theme)
button("git-base-use", use_label, theme, true)
    .focusable(theme)
```

- [ ] **Step 4: Build and lint**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/shell/phone_access.rs crates/mkrew-ui/src/shell/workspace_actions.rs crates/mkrew-ui/src/shell/git_dialogs.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Phone access, Add workspace and Git base controls take Tab"
```

---

### Task 9: Run setup model (Shift+Tab, edges, picker cursor)

**Files:**
- Modify: `crates/mkrew-ui/src/run_setup.rs` (imports; `Picker` ~line 59; `Outcome` ~line 99; `key` ~line 123; `open_picker` ~line 190; `browse` ~line 212; `pick` ~line 243; `next_focus` ~line 402; tests)

**Interfaces:**
- Produces (used by Task 10):
  - `Picker.cursor: Option<usize>` and `Picker::listed_entries(&self) -> &[FsEntry]`.
  - `Outcome::List(String)` - the picker shows another folder: the caller runs `fs.list` for it. `Outcome` loses `Copy`.
  - `RunSetup::tab(&mut self, forward: bool)`, `RunSetup::at_edge(&self, forward: bool) -> bool`, `RunSetup::enter(&mut self, forward: bool)`.
- `RunSetup::key(DialogKey::SwitchField)` still means "Tab forward", so the existing wrap test stays valid.

- [ ] **Step 1: Write the failing tests** - append to `mod tests` in `run_setup.rs` (uses the existing `project`, `be`, `entry` and `moved` helpers):

```rust
    #[test]
    fn shift_tab_walks_back_and_the_ends_leave_the_fields() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.add_card();
        setup.focus = Focus::Field(0, Field::Name);
        assert!(setup.at_edge(false), "Shift+Tab from the first field leaves");
        assert!(!setup.at_edge(true));
        setup.focus = Focus::Field(1, Field::Name);
        setup.tab(false);
        assert_eq!(setup.focus, Focus::Exit(0));
        setup.tab(false);
        assert_eq!(setup.focus, Focus::Field(0, Field::Env));
        setup.focus = Focus::Autostart;
        assert!(setup.at_edge(true), "Tab from the last stop leaves");
        assert!(!setup.at_edge(false));
        setup.tab(false);
        assert_eq!(setup.focus, Focus::Exit(1));
        setup.enter(true);
        assert_eq!(setup.focus, Focus::Field(0, Field::Name), "Tab from the last button");
        setup.enter(false);
        assert_eq!(setup.focus, Focus::Autostart, "Shift+Tab from the first button");
        // The picker has no fields: Tab always goes on to its buttons.
        setup.open_picker(0, Field::Dir);
        assert!(setup.at_edge(true) && setup.at_edge(false));
    }

    #[test]
    fn the_picker_cursor_walks_the_entries_and_enter_takes_the_one_under_it() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.open_picker(0, Field::Dir);
        setup.listed("", Ok(vec![entry("EA-BE", true), entry("EA-FE", true)]));
        let cursor = |setup: &RunSetup| setup.picker.as_ref().unwrap().cursor;
        assert_eq!(cursor(&setup), None);
        for _ in 0..3 {
            setup.key(moved(Motion::Down, false));
        }
        assert_eq!(cursor(&setup), Some(1), "clamped to the last entry");
        setup.key(moved(Motion::Up, false));
        assert_eq!(cursor(&setup), Some(0));
        // → opens the folder under the cursor; the new listing has no cursor yet.
        assert_eq!(
            setup.key(moved(Motion::Right, false)),
            Outcome::List("EA-BE".into())
        );
        assert_eq!(cursor(&setup), None);
        setup.listed("EA-BE", Ok(vec![entry("EA-BE/api", true)]));
        // ← goes to the parent, and does nothing at the workspace root.
        assert_eq!(
            setup.key(moved(Motion::Left, false)),
            Outcome::List(String::new())
        );
        assert_eq!(setup.key(moved(Motion::Left, false)), Outcome::Edited);
        // Enter with the cursor on a folder takes that folder, not the listed one.
        setup.listed("", Ok(vec![entry("EA-FE", true)]));
        setup.key(moved(Motion::Down, false));
        assert_eq!(setup.key(DialogKey::Enter), Outcome::Edited);
        assert_eq!(setup.cards[0].dir, "EA-FE");
        assert!(setup.picker.is_none());
    }

    #[test]
    fn in_the_env_file_picker_enter_opens_a_folder_and_picks_a_file() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        // The picker starts in the card's folder.
        setup.open_picker(0, Field::EnvFile);
        setup.listed(
            "EA-BE",
            Ok(vec![entry("EA-BE/api", true), entry("EA-BE/.env", false)]),
        );
        setup.key(moved(Motion::Down, false));
        assert_eq!(
            setup.key(DialogKey::Enter),
            Outcome::List("EA-BE/api".into()),
            "Enter on a folder opens it"
        );
        assert!(setup.picker.is_some());
        setup.listed("EA-BE/api", Ok(vec![entry("EA-BE/api/.env", false)]));
        setup.key(moved(Motion::Down, false));
        assert_eq!(setup.key(DialogKey::Enter), Outcome::Edited);
        assert_eq!(setup.cards[0].env_file, "EA-BE/api/.env");
        assert!(setup.picker.is_none());
        assert_eq!(setup.focus, Focus::Field(0, Field::EnvFile));
    }
```

Run: `cargo test -p mkrew-ui run_setup`
Expected: FAIL to compile (`no method named at_edge`, `no field cursor`, `no variant List`).

- [ ] **Step 2: Implement the picker** - in `run_setup.rs`:

Imports (top of file): add

```rust
use crate::editor::Motion;
```

`Picker`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker {
    pub card: usize,
    pub field: Field,
    /// The listed folder, relative to the workspace (`""` = its root, where ↑ Up stops).
    pub dir: String,
    /// What `fs.list` answered (only folders when picking a folder); `None` while it runs.
    pub entries: Option<Result<Vec<FsEntry>, String>>,
    /// The entry ↑ / ↓ picked, if any (a new listing clears it).
    pub cursor: Option<usize>,
}

impl Picker {
    pub fn at_root(&self) -> bool {
        self.dir.is_empty()
    }

    /// The listed entries (none while loading or after an error).
    pub fn listed_entries(&self) -> &[FsEntry] {
        match &self.entries {
            Some(Ok(entries)) => entries,
            _ => &[],
        }
    }
}
```

`Outcome`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Edited,
    Cancel,
    Save,
    /// The picker shows another folder: `fs.list` it.
    List(String),
}
```

`key` - replace the `if let Some(picker) = &self.picker { ... }` block at its start and the `SwitchField` arm:

```rust
    pub fn key(&mut self, key: DialogKey) -> Outcome {
        if self.picker.is_some() {
            return match key {
                DialogKey::Cancel => {
                    self.picker = None;
                    Outcome::Edited
                }
                DialogKey::Move {
                    motion: Motion::Down,
                    ..
                } => {
                    self.picker_step(true);
                    Outcome::Edited
                }
                DialogKey::Move {
                    motion: Motion::Up, ..
                } => {
                    self.picker_step(false);
                    Outcome::Edited
                }
                DialogKey::Move {
                    motion: Motion::Right,
                    ..
                } => self.picker_open(),
                DialogKey::Move {
                    motion: Motion::Left,
                    ..
                } => self.browse_up().map_or(Outcome::Edited, Outcome::List),
                DialogKey::Enter | DialogKey::Save => self.picker_enter(),
                _ => Outcome::Edited,
            };
        }
        match key {
            DialogKey::Cancel => Outcome::Cancel,
            DialogKey::Save => Outcome::Save,
            DialogKey::Enter => match self.focus {
                Focus::Field(i, Field::Env) => {
                    self.cards[i].env.key(DialogKey::Enter);
                    Outcome::Edited
                }
                _ => Outcome::Save,
            },
            DialogKey::SwitchField => {
                self.tab(true);
                Outcome::Edited
            }
            // ... the arms from "The caller reads the clipboard" down stay as they are.
```

Helpers, in `impl RunSetup` next to `browse_up`:

```rust
    /// ↓ / ↑ in the picker: the next / previous entry (the first / last when there is no
    /// cursor yet).
    fn picker_step(&mut self, down: bool) {
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        let count = picker.listed_entries().len();
        if count == 0 {
            return;
        }
        picker.cursor = Some(match (picker.cursor, down) {
            (None, true) => 0,
            (None, false) => count - 1,
            (Some(at), true) => (at + 1).min(count - 1),
            (Some(at), false) => at.saturating_sub(1),
        });
    }

    /// → in the picker: opens the folder under the cursor.
    fn picker_open(&mut self) -> Outcome {
        let folder = self.picker.as_ref().and_then(|picker| {
            let entry = picker.listed_entries().get(picker.cursor?)?;
            entry.is_dir.then(|| entry.path.clone())
        });
        folder
            .and_then(|dir| self.browse(dir))
            .map_or(Outcome::Edited, Outcome::List)
    }

    /// Enter in the picker. Picking a folder: the folder under the cursor, else the listed
    /// one. Picking an env file: a file under the cursor is taken, a folder under it is opened,
    /// nothing under it does nothing.
    fn picker_enter(&mut self) -> Outcome {
        let Some(picker) = self.picker.as_ref() else {
            return Outcome::Edited;
        };
        let under = picker
            .cursor
            .and_then(|at| picker.listed_entries().get(at))
            .map(|entry| (entry.path.clone(), entry.is_dir));
        match (picker.field, under) {
            (Field::Dir, Some((path, _))) => self.pick(Some(&path)),
            (Field::Dir, None) => self.pick(None),
            (_, Some((path, true))) => {
                return self.browse(path).map_or(Outcome::Edited, Outcome::List);
            }
            (_, Some((path, false))) => self.pick(Some(&path)),
            (_, None) => {}
        }
        Outcome::Edited
    }
```

`open_picker` builds the picker with a cursor:

```rust
        self.picker = Some(Picker {
            card,
            field,
            dir: dir.clone(),
            entries: None,
            cursor: None,
        });
```

`browse` clears it:

```rust
    pub fn browse(&mut self, dir: String) -> Option<String> {
        let picker = self.picker.as_mut()?;
        picker.dir = dir.clone();
        picker.entries = None;
        picker.cursor = None;
        Some(dir)
    }
```

`pick` takes a folder path too:

```rust
        match (picker.field, path) {
            (Field::Dir, None) if picker.dir.is_empty() => card.dir = ".".into(),
            (Field::Dir, None) => card.dir = picker.dir.into(),
            (Field::Dir, Some(path)) => card.dir = path.into(),
            (Field::EnvFile, Some(path)) => card.env_file = path.into(),
            _ => {}
        }
```

- [ ] **Step 3: Implement the field chain edges** - in `impl RunSetup`, next to `next_focus`:

```rust
    /// Tab (`forward`) / Shift+Tab along the fields, the two checkboxes and back.
    pub fn tab(&mut self, forward: bool) {
        self.focus = if forward {
            self.next_focus()
        } else {
            self.prev_focus()
        };
    }

    /// Tab (`forward`) / Shift+Tab has no field left in that direction: the keyboard leaves the
    /// fields for the buttons ([`crate::dialog_focus::nav`]). The picker has no fields.
    pub fn at_edge(&self, forward: bool) -> bool {
        if self.picker.is_some() {
            return true;
        }
        match (self.focus, forward) {
            (Focus::Autostart, true) => true,
            (Focus::Autostart, false) => self.cards.is_empty(),
            (Focus::Field(0, Field::Name), false) => true,
            _ => false,
        }
    }

    /// The keyboard came back to the fields from the buttons: Tab lands on the first field,
    /// Shift+Tab on "Start with the daemon".
    pub fn enter(&mut self, forward: bool) {
        if self.picker.is_some() {
            return;
        }
        self.focus = if forward && !self.cards.is_empty() {
            Focus::Field(0, Field::Name)
        } else {
            Focus::Autostart
        };
    }

    fn prev_focus(&self) -> Focus {
        match self.focus {
            Focus::Autostart => match self.cards.len() {
                0 => Focus::Autostart,
                count => Focus::Exit(count - 1),
            },
            Focus::Field(i, field) => {
                let at = Field::ORDER.iter().position(|f| *f == field).unwrap_or(0);
                match at.checked_sub(1) {
                    Some(prev) => Focus::Field(i, Field::ORDER[prev]),
                    None if i == 0 => Focus::Autostart,
                    None => Focus::Exit(i - 1),
                }
            }
            Focus::Exit(i) => Focus::Field(i, Field::Env),
        }
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-ui run_setup`
Expected: PASS, including the existing `tab_walks_every_field_then_autostart_then_wraps` and the picker tests.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/run_setup.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): run setup Shift+Tab and keyboard picking in its picker"
```

---

### Task 10: Run setup shell

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs` (field `run_picker_scroll` after `run_setup_scroll` ~line 440)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (init after `run_setup_scroll` ~line 221)
- Modify: `crates/mkrew-ui/src/shell/run.rs` (`run_setup_key_down` ~line 162; `list_run_picker` ~line 197; `render_run_setup` ~lines 560-1050; `render_run_picker` ~lines 1060-1249)

**Interfaces:**
- Consumes: Task 3 recipe; Task 9 (`RunSetup::{tab, at_edge, enter}`, `Outcome::List`, `Picker.cursor`, `Picker::listed_entries`).
- Run setup keeps its virtual field chain (Name, Folder, Command, Env file, Env, "Close the pane", the next card, "Start with the daemon"). At the end of that chain Tab leaves for the real controls in screen order: the open card's Duplicate and Remove, the two Browse buttons, Add command, Cancel, Save. Shift+Tab goes the other way.

- [ ] **Step 1: Scroll handle for the picker** - `main.rs`, after `run_setup_scroll: ScrollHandle,`:

```rust
    /// Keeps the Run setup picker's cursor row in view.
    run_picker_scroll: ScrollHandle,
```

`connection.rs`, after `run_setup_scroll: ScrollHandle::new(),`:

```rust
            run_picker_scroll: ScrollHandle::new(),
```

- [ ] **Step 2: The key handler** - replace `run_setup_key_down` in `run.rs`:

```rust
    pub(crate) fn run_setup_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let edge = (self.run_setup.as_ref())
            .is_some_and(|setup| setup.at_edge(!event.keystroke.modifiers.shift));
        if self.dialog_nav(event, edge, window, cx, |shell, forward| {
            if let Some(setup) = shell.run_setup.as_mut() {
                setup.enter(forward);
            }
        }) {
            return;
        }
        cx.stop_propagation();
        // Shift+Tab walks the fields back (`dialog_key` knows only Tab).
        if event.keystroke.key == "tab" && event.keystroke.modifiers.shift {
            if let Some(setup) = self.run_setup.as_mut() {
                setup.tab(false);
            }
            self.reveal_run_card();
            cx.notify();
            return;
        }
        let Some(key) = task_dialog::dialog_key(&event.keystroke) else {
            return;
        };
        let paste = (key == DialogKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(setup) = self.run_setup.as_mut() else {
            return;
        };
        if matches!(key, DialogKey::Copy | DialogKey::Cut)
            && setup.picker.is_none()
            && let Some(text) = setup.selected_text()
        {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.to_owned()));
        }
        if let Some(text) = paste
            && setup.picker.is_none()
        {
            setup.insert(&text);
        }
        let outcome = setup.key(key);
        // The picker's cursor row, else the focused card, stays in view.
        let target = match &setup.picker {
            Some(picker) => picker.cursor,
            None => setup.focused_card(),
        };
        let in_picker = setup.picker.is_some();
        match outcome {
            run_setup::Outcome::Cancel => return self.close_run_setup(window, cx),
            run_setup::Outcome::Save => return self.save_run_setup(window, cx),
            run_setup::Outcome::List(dir) => {
                return self.list_run_picker(Some(dir), window, cx);
            }
            run_setup::Outcome::Edited => {}
        }
        if let Some(i) = target {
            if in_picker {
                self.run_picker_scroll.scroll_to_item(i);
            } else {
                self.run_setup_scroll.scroll_to_item(i);
            }
        }
        cx.notify();
    }
```

In `list_run_picker`, when the answer lands, scroll the list back to its top - after `setup.listed(&dir, listed);`:

```rust
                if let Some(setup) = shell.run_setup.as_mut() {
                    setup.listed(&dir, listed);
                    shell.run_picker_scroll.scroll_to_item(0);
                    cx.notify();
                }
```

- [ ] **Step 3: The setup panel's controls** - in `render_run_setup`:

Card actions: only an open card's buttons are Tab stops (a folded card shows them on hover only):

```rust
            let card_action = |id: &'static str, glyph: Icon, tip: &'static str| {
                let action = quiet_button(
                    id,
                    icon_svg(glyph, theme::ICON_SIZE_SMALL, theme.text_muted),
                    theme,
                    theme.surface_raised,
                )
                .tooltip(tooltip(theme, tip, ""));
                if open == Some(i) {
                    action.focusable(theme)
                } else {
                    action
                }
            };
```

The `browse` closure's button:

```rust
                    quiet_button(id, "Browse…", theme, theme.surface_raised)
                        .focusable(theme)
                        .tooltip(tooltip(theme, hint, ""))
```

"Add command" (it draws its own 1 px border): append after its `.on_click(...)`:

```rust
                .on_click(cx.listener(|shell, _, _, cx| {
                    if let Some(setup) = shell.run_setup.as_mut() {
                        setup.add_card();
                        shell.reveal_run_card();
                    }
                    cx.notify();
                }))
                .focusable_bordered(theme),
```

Footer:

```rust
button("run-cancel", "Cancel", theme, false)
    .focusable(theme)
button("run-save", "Save", theme, true)
    .focusable(theme)
```

Root (`.id("run-setup")` panel):

```rust
            .dialog_root(
                &self.dialog_focus,
                true,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::run_setup_key_down),
            )
```

The "Close the pane when it ends" and "Start with the daemon" rows stay virtual stops of the field chain (Space flips them), as before.

- [ ] **Step 4: The picker** - in `render_run_picker`:

The list follows the cursor:

```rust
        let mut list = div()
            .id("run-picker-list")
            .flex()
            .flex_col()
            .max_h(px(theme::RUN_SETUP_MAX_HEIGHT))
            .overflow_y_scroll()
            .track_scroll(&self.run_picker_scroll);
```

The cursor row, in the `entries.iter().enumerate().map(|(index, entry)| { ... })` row builder, add after `.hover(move |style| style.bg(rgb(hover)))`:

```rust
                    .when(picker.cursor == Some(index), |el| {
                        el.bg(rgb(theme.surface_raised))
                    })
```

Buttons:

```rust
quiet_button(
    "run-picker-up",
    icon_text(Icon::ArrowUp, "Up", theme.text_muted),
    theme,
    theme.surface_raised,
)
.focusable(theme)
button("run-picker-cancel", "Cancel", theme, false)
    .focusable(theme)
button("run-picker-use", "Use this folder", theme, true)
    .focusable(theme)
```

A hint under the list (add after `.child(list)`):

```rust
            .child(
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(if folder {
                        "↑↓ choose · → open · ← up · Enter takes the folder under the cursor, else this folder · Esc back"
                    } else {
                        "↑↓ choose · → open · ← up · Enter picks the file or opens the folder · Esc back"
                    }),
            )
```

Root:

```rust
            .dialog_root(
                &self.dialog_focus,
                false,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::run_setup_key_down),
            )
```

The picker has no text field; its keyboard starts on the dialog, where ↑ ↓ → ← and Enter act on the cursor, and Tab reaches Up, Cancel and "Use this folder".

- [ ] **Step 5: Build and lint**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.
Run: `cargo test -p mkrew-ui run_setup`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/run.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): run setup and its picker take Tab, Shift+Tab and arrows"
```

---

### Task 11: Settings (Reset, Fix in Zed, links)

**Files:**
- Modify: `crates/mkrew-ui/src/settings_dialog.rs` (`step_focus` ~line 331; new methods; tests)
- Modify: `crates/mkrew-ui/src/shell/settings.rs` (`settings_key_down` ~line 33; `settings_link` ~line 326; Reset ~line 573; root ~line 270)

**Interfaces:**
- Produces: `SettingsDialog::chain_edge(&self, back: bool, keymap: &Keymap) -> bool`, `SettingsDialog::leave(&mut self) -> Effect`, `SettingsDialog::enter(&mut self, forward: bool, keymap: &Keymap)`.
- The model keeps its Tab chain (page list, search, rows). At its ends Tab leaves for the real controls in screen order: "Fix in Zed" of the banner, the Reset buttons, "Edit settings.toml in Zed", "Edit keymap.toml in Zed".
- While a shortcut is being captured or a font / theme list is open, Tab stays with the model (a bare Tab is rejected as a shortcut and cancels the capture, as the existing test pins).

- [ ] **Step 1: Write the failing test** - in `settings_dialog.rs` `mod tests`:

```rust
    #[test]
    fn tab_leaves_the_page_list_and_rows_at_their_ends() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        assert!(d.chain_edge(true, &keymap), "Shift+Tab from the page list leaves");
        assert!(!d.chain_edge(false, &keymap));
        let last = d.rows(&keymap).len() - 1;
        for _ in 0..=last {
            d.key(&key("tab"), None, &keymap);
        }
        assert_eq!(d.focus, Focus::Row(last));
        assert!(d.chain_edge(false, &keymap), "Tab from the last row leaves");
        assert!(!d.chain_edge(true, &keymap));
        d.enter(true, &keymap);
        assert_eq!(d.focus, Focus::Tabs, "Tab from the last button");
        d.enter(false, &keymap);
        assert_eq!(d.focus, Focus::Row(last), "Shift+Tab from the first button");
        // A shortcut being captured keeps Tab.
        d.set_tab(Tab::Keyboard);
        d.click(Focus::Row(0), &keymap);
        assert!(d.capture.is_some());
        assert!(!d.chain_edge(false, &keymap) && !d.chain_edge(true, &keymap));
    }
```

Run: `cargo test -p mkrew-ui tab_leaves_the_page_list`
Expected: FAIL to compile (`no method named chain_edge`).

- [ ] **Step 2: Implement** - in `impl SettingsDialog`, after `step_focus`:

```rust
    /// Tab (`!back`) / Shift+Tab has no stop left in the page list, search box and rows: the
    /// keyboard leaves them for the dialog's buttons ([`crate::dialog_focus::nav`]). Never while
    /// a shortcut is being captured or a list pick is open: those keep Tab.
    pub fn chain_edge(&self, back: bool, keymap: &Keymap) -> bool {
        if self.capture.is_some() || self.list_pick.is_some() {
            return false;
        }
        if back {
            return self.focus == Focus::Tabs;
        }
        let rows = self.rows(keymap).len();
        match self.focus {
            Focus::Row(i) => i + 1 >= rows,
            Focus::Search => rows == 0,
            Focus::Tabs => self.tab != Tab::Keyboard && rows == 0,
        }
    }

    /// Writes a draft before the keyboard leaves for the buttons.
    pub fn leave(&mut self) -> Effect {
        self.commit_draft()
    }

    /// The keyboard came back from the buttons: Tab lands on the page list, Shift+Tab on the
    /// last row.
    pub fn enter(&mut self, forward: bool, keymap: &Keymap) {
        let last = match self.rows(keymap).len() {
            0 if self.tab == Tab::Keyboard => Focus::Search,
            0 => Focus::Tabs,
            count => Focus::Row(count - 1),
        };
        // `leave` already wrote any draft, so there is no effect to apply.
        let _ = self.move_focus(if forward { Focus::Tabs } else { last }, keymap);
    }
```

Run: `cargo test -p mkrew-ui settings_dialog`
Expected: PASS.

- [ ] **Step 3: The shell keys** - in `settings.rs`, the start of `settings_key_down`:

```rust
        let back = event.keystroke.modifiers.shift;
        let edge = (self.settings_dialog.as_ref())
            .is_some_and(|dialog| dialog.chain_edge(back, &self.keymap));
        if edge && event.keystroke.key == "tab" && self.dialog_focus.is_focused(window) {
            // Leaving the rows writes the draft of the row it leaves, like Tab to the next row.
            let effect = self.settings_dialog.as_mut().map(SettingsDialog::leave);
            if let Some(effect) = effect {
                self.apply_settings_effect(effect, window, cx);
            }
        }
        let keymap = self.keymap.clone();
        if self.dialog_nav(event, edge, window, cx, move |shell, forward| {
            if let Some(dialog) = shell.settings_dialog.as_mut() {
                dialog.enter(forward, &keymap);
            }
        }) {
            return;
        }
        cx.stop_propagation();
```

(The block above replaces the function's first line, the old `cx.stop_propagation();`. The later `let keymap = self.keymap.clone();` stays: it shadows the first one, which the closure moved.)

- [ ] **Step 4: The controls** - in `settings.rs`:

Links (banner "Fix in Zed" and the two footer links all go through `settings_link`):

```rust
        div()
            .id(id)
            .cursor_pointer()
            .text_color(rgb(theme.accent_text))
            .hover(|style| style.underline())
            .on_click(cx.listener(move |shell, _, _, cx| on_click(shell, cx)))
            .focusable(theme)
            .child(label)
```

Reset (in `binding_control`), after its `.on_click(...)`:

```rust
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            cx.stop_propagation();
                            shell.settings_do(|d, _| d.reset(command), window, cx);
                        }))
                        .focusable(theme)
                        .child("Reset"),
```

Root:

```rust
            .dialog_root(
                &self.dialog_focus,
                true,
                cx.listener(Self::dialog_guard),
                cx.listener(Self::settings_key_down),
            )
```

The settings tab buttons, rows and switches stay virtual stops of the model's chain (they already show their own selection ring).

- [ ] **Step 5: Build and lint**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/settings_dialog.rs crates/mkrew-ui/src/shell/settings.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): settings Reset, Fix in Zed and links take Tab"
```

---

### Task 12: ADR 0085, SPEC and HUMAN_CHECKS

**Files:**
- Modify: `docs/decisions/0085-keyboard-only-operation.md` (append a section; wave 1 created the file from `docs/decisions/0000-template.md`)
- Modify: `docs/SPEC.md` (§13, §14)
- Modify: `docs/HUMAN_CHECKS.md`

**Interfaces:** none (docs).

- [ ] **Step 1: ADR** - check that `docs/decisions/0085-keyboard-only-operation.md` exists (wave 1 creates it; if it does not, stop and tell the user wave 1 has not landed). Append at the end of the file:

```markdown

## Wave 2 - focusable controls in dialogs (2026-10-27)

### Decision

- The controls of a modal dialog are Tab stops through gpui's tab index.
  A control with `tab_index(0)` keeps its focus handle in its element state, so no field on `Shell` and no `use_keyed_state` is needed.
- The dialog's panel keeps `dialog_focus`.
  It is the first Tab stop when the dialog has text fields, so Shift+Tab can come back to them.
- Tab and Shift+Tab are handled in each dialog's `on_key_down` (`Shell::dialog_nav`), not bound in `keymap.toml`.
  gpui has no default Tab binding, and the key only means something inside a dialog.
- The text fields stay virtual (a `field` or `focus` value in the dialog model and one `dialog_focus` handle).
  Tab walks them first; at the end of that chain it moves on to the buttons, and from the last button it wraps back.
- A focused control takes Enter and Space (gpui clicks it on release) and every typed key.
  Esc, arrow keys and Ctrl combinations still reach the dialog.
- The ring is a 1 px border in `Theme.focus_ring` (the text color) over the control's own border, shown by `focus_visible` only after keyboard input.
- Chips of a row are each a Tab stop, and ← / → move between them.
- Initial focus is the dialog itself (or its first text field), where Enter keeps its old meaning.
  The spec said "else the primary button"; the primary button of several dialogs is destructive, so no button gets the first focus.
- Broadcast: Space checks the row the arrows picked; Space in the message types a space until an arrow key moves the keyboard to the list.
- A control that disappears while focused (a button swapped for its disabled twin) returns the keyboard to the dialog, so Esc keeps working.

### Destructive confirmations (replaces the rule of `undo_dialog.rs`)

Before: Undo Discard, worktree Remove and Discard files, Git Discard and Compare Pick took only a click ("discarding takes an explicit click").
Now: they also run when Tab moved the keyboard onto the button and Enter or Space is pressed.
They are never the first focus, and an auto-repeating Enter or Space is dropped before a control sees it (`dialog_focus::blocks_repeat`), like the held-key guard of the confirm prompt.
The daemon still re-checks every destructive request (previews and fingerprints), so nothing else changed.

### Consequences

- SPEC §13 and §14 describe the dialog keys and the ring.
- Turns: D shows the diff since the selected turn.
  Run setup: Shift+Tab walks back, and its picker takes ↑ ↓ → ← and Enter on the entry under the cursor.
```

- [ ] **Step 2: SPEC §13** - append a paragraph to §13 after the paragraph that starts with "Alt+Shift+A / Alt+Shift+S and F2 no longer reach a focused terminal" (find it by that text; do not reorder wave 1, 3 or 4 additions):

```markdown
Modal dialogs (ADR 0085): Tab and Shift+Tab move through the dialog's controls and wrap inside it.
The order is the dialog's text fields first, then its options (chips, checkboxes), then its buttons left to right.
Enter or Space on a focused control clicks it; ← / → move between the chips of a row; Esc closes the dialog from anywhere in it.
Tab is not in `keymap.toml`: like the keys of the confirm prompt it only means something inside a dialog.
A destructive button (Discard changes, Remove worktree, Discard files) is never the first focus and does not run on the Enter that opened the dialog.
It runs when Tab moved onto it and Enter or Space is pressed; an auto-repeating key does nothing.
Turns: D shows the changes since the selected turn.
Broadcast: ↑ / ↓ pick a session, Space checks it.
Run setup: Shift+Tab walks back; in its folder picker ↑ / ↓ move, → opens a folder, ← goes up, Enter takes the folder or env file under the cursor.
```

- [ ] **Step 3: SPEC §14** - add a bullet after the "Focused pane: faint 1 px accent border; unfocused panes dimmed ~5%." bullet:

```markdown
- Focus ring (ADR 0085): a dialog control that has the keyboard shows a 1 px border in `focus-ring` (the text color) over its own border, only while it was reached with the keyboard (`focus_visible`), in both themes. A click never shows it.
```

- [ ] **Step 4: HUMAN_CHECKS** - under "To verify (non-blocking)", add the newest entry first:

```markdown
### To verify - 2026-10-27 Keyboard-only dialogs (ADR 0085, wave 2)

- [ ] **Focus ring (dark + light):** open the task dialog (palette: New task). Press Tab until each control has the keyboard: the title, the prompt, From prompt, every agent chip, Run in a worktree, Cancel, Create task, then the title again. Each control shows a 1 px ring in the text color; a mouse click on the same controls shows none. Shift+Tab goes the other way.
- [ ] **Chips:** with an agent chip focused press ← / → (moves along the row, stops at the ends) and Enter or Space (picks it). The Ask AI dialog's Agent and Web search rows do the same.
- [ ] **Destructive confirmations:** open the Undo dialog of a Review card. No button has a ring and Enter does nothing. Tab once: Cancel. Tab twice: Discard changes (red text, ring). Enter discards. Repeat with Remove worktree, Discard files and Git Discard. Open each with the Enter key held a moment: nothing is clicked.
- [ ] **Broadcast:** type a message with spaces (they stay spaces); press ↓ and Space to uncheck a session; type again (Space types a space); Tab reaches Cancel and Send.
- [ ] **Run setup:** Tab walks the fields, both checkboxes, then Duplicate, Remove, Browse, Add command, Cancel, Save; Shift+Tab goes back; Browse opens the picker: ↓ ↑ move, → opens a folder, ← goes up, Enter picks the folder or the .env file under the cursor.
- [ ] **Settings and others:** Tab through Settings reaches Reset and Fix in Zed after the rows. Phone access, Add workspace, Git base, Compare and the Send dialog reach all their buttons with Tab. Turns: D opens the diff since the selected turn.
- [ ] **A button that vanishes:** in the worktree dialog focus Discard, press Enter (the step changes). Esc still closes the dialog.
```

- [ ] **Step 5: Commit**

```bash
git add docs/decisions/0085-keyboard-only-operation.md docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "docs(spec): keyboard-only dialogs, wave 2 (ADR 0085)"
```

---

### Task 13: Full verify and smoke run

**Files:**
- Modify: `docs/HUMAN_CHECKS.md` (record what could not be automated)

**Interfaces:** none.

- [ ] **Step 1: Full verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: `verify: OK` (fmt, clippy `-D warnings`, all tests with `INSTA_UPDATE=no`). On OOM or linker errors re-run with `-j 2` (set `CARGO_BUILD_JOBS=2`). Fix and re-run until green.
Literal check on the wave's diff (no hex or raw pixel in components):
`git diff <first wave 2 commit>~1 -- crates/mkrew-ui/src/shell | grep -nE '^\+.*(#[0-9a-fA-F]{6}|px\([0-9])'` -> no hits.

- [ ] **Step 2: Build the dev exe**

Run: `cargo build -p mkrew-ui`
Expected: `Finished`. If a locked exe blocks the link, rename it to `*.locked-old.exe` (never kill a daemon or agent process).

- [ ] **Step 3: Launch in a sandbox**

Create three temp dirs and start the app with them (never the real data):

```bash
SANDBOX="$(mktemp -d)"
mkdir -p "$SANDBOX/data" "$SANDBOX/config"
MKREW_DATA_DIR="$SANDBOX/data" MKREW_CONFIG_DIR="$SANDBOX/config" MKREW_SOCKET="$SANDBOX/mkrew.sock" "$PWD/target/debug/mkrew.exe"
```

(Use `"$PWD/..."`: the omp bash resolves a relative program path against the session start folder.)

- [ ] **Step 4: Keyboard-only run, dark then light** - with the mouse untouched, do each action from the HUMAN_CHECKS entry of Task 12:
  1. Command palette -> New task: Tab around the whole dialog and back with Shift+Tab; pick an agent chip with ← / → and Space; toggle "Run in a worktree" with Space; Create task with Ctrl+Enter.
  2. Ask AI: Tab to the Web search chips, ← / →, Enter.
  3. Undo a Review card: confirm no ring at open, Tab, Tab, Enter discards; hold Enter while opening and confirm nothing is clicked.
  4. Broadcast: Space typed in the message stays a space; ↓ Space unchecks.
  5. Run setup: Tab around, Shift+Tab, Browse, ↓ ↓ → ← Enter in the picker.
  6. Settings: Tab to Reset and Fix in Zed.
  7. Worktree dialog: Discard, Enter, then Esc still closes it (the dead-focus recovery).
  8. Turns: D.
  Switch the theme (Settings, or the title bar toggle) and repeat 1 and 3.

- [ ] **Step 5: Record the result** - in the Task 12 entry of `docs/HUMAN_CHECKS.md`, tick nothing that was not observed. For each check that could not be performed here (no GUI automation, no Review card or worktree to undo, light theme not seen), keep its checkbox unticked and add one line under it saying what was not run and why. Delete no check.

- [ ] **Step 6: Commit**

```bash
git add docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "docs(spec): record the wave 2 smoke run"
```

If nothing in HUMAN_CHECKS changed, skip the commit.

---

## Self-Review

**Spec coverage** (spec section "Wave 2"):

| Spec item | Task |
|---|---|
| GPUI tab stops, `focus_next` / `focus_prev`, `focus_visible` | Verified facts; Task 3 |
| Handles from `use_keyed_state` | Not needed: `tab_index(0)` keeps the handle in element state (Verified facts; ADR) |
| Focusable variant for button / secondary / quiet / danger / icon_button / segment, chips, checkbox rows | Task 3 (`DialogControl`), call sites in Tasks 3 to 11; icon_button and segment have no dialog call site |
| Only controls inside a modal dialog are tab stops | Each dialog's controls only; Git "Showing" chips pass `tab_stops: false` (Task 4) |
| Dialog root binds Tab / Shift+Tab, replaces hand-written Tab (task, run setup, broadcast) | `dialog_nav` in Tasks 3, 4, 5, 7, 8, 10, 11; Title/Prompt switch kept for the middle of the chain; run setup gets Shift+Tab (Tasks 9, 10); broadcast Tab no longer ticks (Task 7) |
| Order: fields, options, buttons, Cancel before primary | Paint order with the root as first stop; existing footers are Cancel then primary |
| Space toggles checkbox / chip; ← / → in a chip group | Keyboard click (gpui) + `chip_arrows` (Tasks 3, 4) |
| Initial focus never destructive | Keyboard starts on the dialog itself (Tasks 3 to 11) |
| Focus ring tokens, theme snapshots | Task 1 |
| Destructive confirmations change, `undo_dialog.rs:282` and its test, ADR | Tasks 5 and 12 |
| Dialogs: Task, Ask, Undo, Phone, Send, Docs | Tasks 3, 4, 5, 8 |
| Broadcast | Task 7 |
| Add workspace | Task 8 |
| Run setup and picker | Tasks 9, 10 |
| Worktree, Compare | Task 5 |
| Git discard, Git base | Tasks 5, 8 |
| Settings Reset / Fix in Zed | Task 11 |
| Turns `d` | Task 6 |
| SPEC §13, §14, HUMAN_CHECKS, smoke run | Tasks 12, 13 |

**Placeholder scan:** no "TBD" or "similar to"; every edit shows its code or its exact call.

**Type consistency:** `dialog_nav(event, edge, window, cx, enter)` and `dialog_root(&FocusHandle, has_text, guard, on_key)` are used with the same shapes in every task; `Nav`, `Step`, `group_step`, `blocks_repeat` come from Task 2; `RunSetup::{tab, at_edge, enter}` and `Outcome::List` are defined in Task 9 and used in Task 10; `SettingsDialog::{chain_edge, leave, enter}` are defined and used in Task 11; `chip_row` gained one `bool` argument in Task 4 and both of its callers (Ask, Git) are updated there.

**Spec gaps and conflicts found while planning:**
- The spec asks for handles from `use_keyed_state`; gpui's `tab_index(0)` already keeps the handle in element state, so no handle plumbing is added.
- The spec says initial focus is "else the primary button"; several dialogs' primary button is destructive or absent, so the plan starts the keyboard on the dialog itself (Enter keeps its old meaning) and no button gets first focus.
- The spec's "Broadcast: Space ticks" collides with typing a space in the message; the plan resolves it with a list mode that an arrow key enters and typing leaves.
- Compare "Pick" is listed with the destructive confirmations but only opens the Merge back dialog; it is a plain Tab stop here.
- The ring reuses `BORDER_WIDTH` (1 px) instead of adding a width token; only the color token `focus_ring` is new.
- `cargo-insta` is not installed on this machine; Task 1 updates the snapshots with `INSTA_UPDATE=always` (same result as `cargo insta accept`).
