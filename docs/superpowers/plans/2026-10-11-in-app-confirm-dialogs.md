# In-app Confirmation Dialogs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every confirmation the app shows with `window.prompt` (close a pane of a running agent, close a tab, remove a session, stop a session, delete a task, remove a workspace, save changes before leaving a file) is drawn inside the Mkrew window in Mkrew's style (SPEC §14) instead of the native Windows/macOS message box.

**Architecture:** GPUI already supports this: `App::set_prompt_builder` replaces the platform dialog for **every** `window.prompt` call with a GPUI view that the window paints on top of everything (`gpui-pre 0.3.7`, `window.rs` `build_custom_prompt`, painted after the root in `window.rs:3545`). So the 7 call sites stay as they are. A pure model in the library (`mkrew_ui::confirm`: button order, Esc/Enter/Tab keys) gets unit tests; one view in the binary (`shell/confirm_prompt.rs`) draws it with the existing `button` / `secondary_button` / `danger_button` widgets and theme tokens, and `main.rs` registers it once. Destructive confirms switch their call to `PromptLevel::Critical`, which draws the first answer as a `danger_button`.

**Tech Stack:** Rust, GPUI (`gpui-pre 0.3.7`), crate `mkrew-ui` only. No protocol, daemon or migration change.

**Spec:** the user's request in this conversation (2026-10-11), verbatim: *"hiện tại 1 vài model confirm, diaglog ví dụ modal để close pane đang kiểu dùng modal default của window thì phải, tôi muốn custom all những cái này theo mkrew UI style"*. Canonical design rules: `docs/SPEC.md` §14 (this plan adds one bullet there) and `.claude/skills/mkrew-ui-design/SKILL.md`.

## Global Constraints

- Scope: every `window.prompt(...)` in `crates/mkrew-ui/src/shell/` (7 calls: `tabs.rs` ×3, `inspector.rs`, `task_dialog.rs`, `workspace_actions.rs`, `files.rs`). The OS **file/folder pickers** (`cx.prompt_for_paths` in `workspace_actions.rs`) stay native — they are not confirmations.
- The answer index contract does not change: `window.prompt` still returns the index into the answers array (`CloseChoice::ANSWERS`, `REMOVE_ANSWERS`, `DELETE_ANSWERS`, `STOP_ANSWERS`, `LEAVE_ANSWERS`), and a dismissed prompt still yields `Err` → `None` → cancel. Do not touch `CloseChoice::from_answer`, `confirms_delete`, `leave_choice`.
- Every color, spacing, radius, font size and width comes from `crates/mkrew-ui/src/theme.rs` (SPEC §14). The only new token is `theme::CONFIRM_DIALOG_WIDTH`. No hex colors or raw pixel numbers outside `theme.rs`.
- Spacing only `space::XS/S/M/L/XL/XXL`; radius only `radius::*`. Dialog radius = `radius::LARGE` (SPEC §14: 12 for dialogs).
- Both themes (dark/light) must work; read colors only through `cx.theme()`.
- Keyboard: Esc = the Cancel answer; Enter or Space = the selected button; Tab / → move right, Shift+Tab / ← move left, wrapping. Default selection = answer 0 (same default as the native dialog). The selected button shows a visible 1 px ring (SPEC §14 principle 7).
- Layout: buttons right-aligned; Cancel leftmost, answer 0 rightmost (same as the task dialog: `Cancel` then the primary button on the right).
- A click on the shade does nothing (same as `Shell::modal_shade`); only a button or a key answers.
- GPUI rule: call `.hover()` at most once per element. `button`, `secondary_button`, `danger_button` already call it — do not add another.
- No `unwrap/expect` outside tests.
- Commits: stage only the files this plan touches (`git add <paths>`), Conventional Commits, never push. Leave the human's uncommitted files alone (`docs/INBOX.md`, `PERF_REVIEW.md`, `docs/BUILD_VI.md`, `.github/workflows/*`, `packaging/`, `Cargo.lock`, other plans/specs).
- Build with `-j 4` (disk C: is nearly full; do not create new target dirs). If a build fails with "Access is denied" on an `.exe`, rename that file to `*.locked-old.exe`. Never kill the daemon or the agents.
- In the omp shell `bash` is WSL: run the verify script as `sh scripts/verify.sh`.

## Review Focus

1. **Async focus steal while the prompt is open.** A pane attaching or a reconnect calls `Shell::focus_pane`, which moves the keyboard to a terminal; Esc/Enter then go to the agent instead of the prompt. Expected: the prompt keeps the keyboard until answered. Pinned by the `has_active_prompt` guard in `Shell::focus_pane` (Task 2 Step 4) and the HUMAN_CHECKS line "Keyboard stays in the prompt".
2. **Prompt over another modal** (Delete in the Edit task dialog). Expected: the confirm draws over the task dialog; Cancel returns the keyboard to the task dialog (Esc then closes the task dialog, typing edits its fields). Pinned by HUMAN_CHECKS line "Over the task dialog" (Task 2).
3. **Window close with unsaved Files changes** (`Shell::window_should_close` → `leave_file`). Expected: the in-app "Save changes to …?" shows inside the still-open window; Cancel keeps the window open, Don't save closes it. Pinned by HUMAN_CHECKS line "Unsaved on close" (Task 2).
4. **A prompt without a Cancel answer, or with no answers.** Expected: Esc does nothing, no panic, no index out of range. Pinned by `without_cancel_esc_does_nothing_and_without_buttons_every_key_is_ignored` (Task 1).
5. **Long text and large fonts.** `leave_file`'s detail holds a full project path; workspace names can be long; `[ui] font_size` can be large. Expected: message and detail wrap inside the card, buttons stay visible on one row, nothing overflows the window. Pinned by HUMAN_CHECKS line "Long text" (Task 2).

---

### Task 1: Pure confirm model (button order and keys)

**Files:**
- Create: `crates/mkrew-ui/src/confirm.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (add `pub mod confirm;` in alphabetical order, after `pub mod composer;`; add "the in-app confirmation dialog," to the crate doc list, after "the broadcast dialog,")
- Modify: `crates/mkrew-ui/src/theme.rs` (new token after `DIALOG_WIDTH`, around line 185)
- Test: `crates/mkrew-ui/src/confirm.rs` (`mod tests` at the bottom)

**Interfaces:**
- Consumes: `gpui::PromptButton` (`is_cancel()`, `label()`, `From<&str>`: `"Cancel"` → `PromptButton::Cancel`, other text → `PromptButton::Other`), `gpui::Keystroke` (`key: String`, `modifiers.shift: bool`, `Keystroke::parse`).
- Produces (used by Task 2):
  - `pub fn cancel_index(buttons: &[PromptButton]) -> Option<usize>`
  - `pub fn visual_order(buttons: &[PromptButton]) -> Vec<usize>` — answer indices left to right.
  - `pub enum Key { Answer(usize), Select(usize), Ignore }` (`Debug, Clone, Copy, PartialEq, Eq`)
  - `pub fn key(keystroke: &Keystroke, order: &[usize], selected: usize, cancel: Option<usize>) -> Key`
  - `pub const CONFIRM_DIALOG_WIDTH: f32` in `theme.rs`

- [ ] **Step 1: Write the failing tests**

Create `crates/mkrew-ui/src/confirm.rs` with the module doc, the import and the tests only (it will not compile until Step 3 — that is the expected failure):

```rust
//! The in-app confirmation dialog behind every `window.prompt` (SPEC §14): where each answer
//! sits and what the keys do. The binary draws it (`shell/confirm_prompt.rs`).

use gpui::{Keystroke, PromptButton};

#[cfg(test)]
mod tests {
    use super::*;

    fn buttons(labels: &[&str]) -> Vec<PromptButton> {
        labels.iter().map(|label| PromptButton::from(*label)).collect()
    }

    fn press(keys: &str, order: &[usize], selected: usize, cancel: Option<usize>) -> Key {
        key(&Keystroke::parse(keys).unwrap(), order, selected, cancel)
    }

    #[test]
    fn cancel_sits_left_and_the_first_answer_right() {
        let close = buttons(&["Kill session", "Hide pane", "Cancel"]);
        assert_eq!(cancel_index(&close), Some(2));
        assert_eq!(visual_order(&close), vec![2, 1, 0]);
        assert_eq!(visual_order(&buttons(&["Remove", "Cancel"])), vec![1, 0]);
        // Cancel stays leftmost wherever the caller put it.
        assert_eq!(visual_order(&buttons(&["Cancel", "Save", "Don't save"])), vec![0, 2, 1]);
        // No Cancel: last answer to first.
        let plain = buttons(&["Yes", "No"]);
        assert_eq!(cancel_index(&plain), None);
        assert_eq!(visual_order(&plain), vec![1, 0]);
    }

    #[test]
    fn esc_answers_cancel_enter_and_space_answer_the_selected() {
        let order = [2, 1, 0];
        assert_eq!(press("escape", &order, 0, Some(2)), Key::Answer(2));
        assert_eq!(press("enter", &order, 1, Some(2)), Key::Answer(1));
        assert_eq!(press("space", &order, 0, Some(2)), Key::Answer(0));
        assert_eq!(press("a", &order, 0, Some(2)), Key::Ignore);
    }

    #[test]
    fn tab_and_arrows_walk_the_visual_order_and_wrap() {
        let order = [2, 1, 0];
        // 0 is rightmost: moving right wraps to the leftmost (Cancel).
        assert_eq!(press("tab", &order, 0, Some(2)), Key::Select(2));
        assert_eq!(press("right", &order, 0, Some(2)), Key::Select(2));
        assert_eq!(press("shift-tab", &order, 0, Some(2)), Key::Select(1));
        assert_eq!(press("left", &order, 0, Some(2)), Key::Select(1));
        // Leftmost: moving left wraps to the rightmost.
        assert_eq!(press("left", &order, 2, Some(2)), Key::Select(0));
        assert_eq!(press("tab", &order, 2, Some(2)), Key::Select(1));
    }

    #[test]
    fn without_cancel_esc_does_nothing_and_without_buttons_every_key_is_ignored() {
        assert_eq!(press("escape", &[1, 0], 0, None), Key::Ignore);
        for keys in ["enter", "space", "tab", "left", "escape"] {
            assert_eq!(press(keys, &[], 0, None), Key::Ignore, "{keys}");
        }
    }
}
```

Add `pub mod confirm;` to `crates/mkrew-ui/src/lib.rs` right after `pub mod composer;`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mkrew-ui --lib confirm -j 4`
Expected: FAIL to compile with `cannot find function 'key'` / `cannot find type 'Key'` / `cannot find function 'visual_order'`.

- [ ] **Step 3: Write the implementation**

Insert above `#[cfg(test)]` in `crates/mkrew-ui/src/confirm.rs`:

```rust
/// The answer Esc picks: the caller's "Cancel" (`PromptButton::Cancel`), if any.
pub fn cancel_index(buttons: &[PromptButton]) -> Option<usize> {
    buttons.iter().position(PromptButton::is_cancel)
}

/// Answer indices left to right: Cancel first, then the others last to first, so answer 0
/// (the default) sits on the right like every Mkrew dialog's primary button.
pub fn visual_order(buttons: &[PromptButton]) -> Vec<usize> {
    let cancel = cancel_index(buttons);
    cancel
        .into_iter()
        .chain((0..buttons.len()).rev().filter(|ix| Some(*ix) != cancel))
        .collect()
}

/// What a key does in the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Close the dialog with this answer.
    Answer(usize),
    /// Move the keyboard ring to this answer.
    Select(usize),
    Ignore,
}

/// Esc answers Cancel; Enter / Space answer the selected button; Tab / → move right and
/// Shift+Tab / ← move left along `order` (from [`visual_order`]), wrapping.
pub fn key(keystroke: &Keystroke, order: &[usize], selected: usize, cancel: Option<usize>) -> Key {
    if order.is_empty() {
        return Key::Ignore;
    }
    let step = |right: bool| {
        let len = order.len();
        let at = order.iter().position(|ix| *ix == selected).unwrap_or(0);
        let next = if right { (at + 1) % len } else { (at + len - 1) % len };
        Key::Select(order[next])
    };
    match keystroke.key.as_str() {
        "escape" => cancel.map_or(Key::Ignore, Key::Answer),
        "enter" | "space" => Key::Answer(selected),
        "tab" if keystroke.modifiers.shift => step(false),
        "tab" | "right" => step(true),
        "left" => step(false),
        _ => Key::Ignore,
    }
}
```

In `crates/mkrew-ui/src/theme.rs`, right after `pub const DIALOG_WIDTH: f32 = 520.0;`:

```rust

/// Width of a confirmation dialog (every `window.prompt`, drawn in the window).
pub const CONFIRM_DIALOG_WIDTH: f32 = 420.0;
```

In `crates/mkrew-ui/src/lib.rs`'s crate doc, add `the in-app confirmation dialog,` after `the broadcast dialog,`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p mkrew-ui --lib confirm -j 4`
Expected: PASS, 4 tests (`cancel_sits_left_and_the_first_answer_right`, `esc_answers_cancel_enter_and_space_answer_the_selected`, `tab_and_arrows_walk_the_visual_order_and_wrap`, `without_cancel_esc_does_nothing_and_without_buttons_every_key_is_ignored`).

If `Keystroke::parse("space")` yields a key other than `"space"` in this gpui version, read `gpui-pre-0.3.7/src/platform/keystroke.rs` and match what `parse` produces (do not guess); a live Space key event uses the same name.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/confirm.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/theme.rs
git commit -m "feat(ui): confirm dialog model (button order, Esc/Enter/Tab keys)"
```

---

### Task 2: Draw every `window.prompt` in the window

**Files:**
- Create: `crates/mkrew-ui/src/shell/confirm_prompt.rs`
- Modify: `crates/mkrew-ui/src/shell.rs` (module list, lines 5–38: add `pub(crate) mod confirm_prompt;` next to `mod comments;`)
- Modify: `crates/mkrew-ui/src/main.rs:160-166` (register the builder in `main`, before `cx.open_window`)
- Modify: `crates/mkrew-ui/src/shell/focus.rs:90` (`focus_pane` guard)
- Modify (one word each, `PromptLevel::Warning` → `PromptLevel::Critical`): `crates/mkrew-ui/src/shell/tabs.rs:423`, `:489`, `:571`; `crates/mkrew-ui/src/shell/inspector.rs:476`; `crates/mkrew-ui/src/shell/task_dialog.rs:152`; `crates/mkrew-ui/src/shell/workspace_actions.rs:508`. Leave `crates/mkrew-ui/src/shell/files.rs:542` at `Warning` (Save is not destructive).
- Modify: `docs/SPEC.md` §14 (one bullet after "In-app toasts", line ~1234)
- Modify: `docs/HUMAN_CHECKS.md` ("To verify" section, new block at the top, line ~18)

**Interfaces:**
- Consumes (Task 1): `mkrew_ui::confirm::{cancel_index, visual_order, key, Key}`, `theme::CONFIRM_DIALOG_WIDTH`.
- Consumes (existing, `crates/mkrew-ui/src/shell/widgets.rs`, reachable through `use crate::*;`):
  - `button(id: &'static str, label: impl Into<SharedString>, theme: &Theme, primary: bool) -> Stateful<Div>`
  - `secondary_button(id: &'static str, label: impl Into<SharedString>, theme: &Theme) -> Stateful<Div>` (already has a `theme.border` border)
  - `danger_button(id: &'static str, label: impl Into<SharedString>, theme: &Theme) -> Stateful<Div>`
  - `tooltip(theme: &Theme, text: &str, keys: impl Into<SharedString>)`
- Consumes (gpui 0.3.7): `App::set_prompt_builder(Fn(PromptLevel, &str, Option<&str>, &[PromptButton], PromptHandle, &mut Window, &mut App) -> RenderablePromptHandle)`; `PromptHandle::with_view(view: Entity<V>, window, cx)` where `V: EventEmitter<PromptResponse> + Focusable + Render` — it focuses the view, and on the first `PromptResponse(ix)` sends `ix`, removes the prompt and restores the previous focus; `Window::has_active_prompt() -> bool`.
- Produces: `pub(crate) fn build(...) -> RenderablePromptHandle` in `shell::confirm_prompt`.

- [ ] **Step 1: Write the view**

Create `crates/mkrew-ui/src/shell/confirm_prompt.rs`:

```rust
//! Every `window.prompt` drawn inside the window in Mkrew's style (SPEC §14) instead of the OS
//! message box. `main` registers [`build`] once with `App::set_prompt_builder`; the call sites
//! keep calling `window.prompt` and reading the answer index.

use crate::*;
use gpui::{
    EventEmitter, FocusHandle, Focusable, PromptButton, PromptHandle, PromptResponse,
    RenderablePromptHandle,
};
use mkrew_ui::confirm;

/// Element ids of the buttons, by answer index.
// ponytail: fixed ids because `button()` takes `&'static str`; no prompt has more than 3
// answers today. Raise the count if one ever needs more (extra answers are not drawn).
const BUTTON_IDS: [&str; 4] = ["confirm-0", "confirm-1", "confirm-2", "confirm-3"];

pub(crate) struct ConfirmPrompt {
    level: PromptLevel,
    message: SharedString,
    detail: Option<SharedString>,
    buttons: Vec<PromptButton>,
    /// Answer indices left to right ([`confirm::visual_order`]).
    order: Vec<usize>,
    /// The answer Enter picks; it shows the keyboard ring.
    selected: usize,
    focus: FocusHandle,
}

/// The `App::set_prompt_builder` hook.
pub(crate) fn build(
    level: PromptLevel,
    message: &str,
    detail: Option<&str>,
    actions: &[PromptButton],
    handle: PromptHandle,
    window: &mut Window,
    cx: &mut App,
) -> RenderablePromptHandle {
    let buttons: Vec<PromptButton> = actions.iter().take(BUTTON_IDS.len()).cloned().collect();
    let order = confirm::visual_order(&buttons);
    let view = cx.new(|cx| ConfirmPrompt {
        level,
        message: message.to_owned().into(),
        detail: detail.map(|detail| detail.to_owned().into()),
        buttons,
        order,
        selected: 0,
        focus: cx.focus_handle(),
    });
    handle.with_view(view, window, cx)
}

impl EventEmitter<PromptResponse> for ConfirmPrompt {}

impl Focusable for ConfirmPrompt {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl ConfirmPrompt {
    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<'_, Self>) {
        cx.stop_propagation();
        let cancel = confirm::cancel_index(&self.buttons);
        match confirm::key(&event.keystroke, &self.order, self.selected, cancel) {
            confirm::Key::Answer(ix) => cx.emit(PromptResponse(ix)),
            confirm::Key::Select(ix) => {
                self.selected = ix;
                cx.notify();
            }
            confirm::Key::Ignore => {}
        }
    }
}

impl Render for ConfirmPrompt {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let critical = self.level == PromptLevel::Critical;
        let buttons: Vec<_> = self
            .order
            .iter()
            .map(|&ix| {
                // `order` only holds indices of `buttons`.
                let answer = &self.buttons[ix];
                let id = BUTTON_IDS[ix];
                let label = answer.label().clone();
                let secondary = ix != 0 && !answer.is_cancel();
                let frame = if ix == 0 && critical {
                    danger_button(id, label, &theme)
                } else if ix == 0 {
                    button(id, label, &theme, true)
                } else if secondary {
                    secondary_button(id, label, &theme)
                } else {
                    button(id, label, &theme, false).tooltip(tooltip(&theme, "Cancel", "Esc"))
                };
                // Every button keeps a 1 px border, so the keyboard ring never shifts the row.
                let ring = (ix == self.selected)
                    .then_some(if ix == 0 { theme.text } else { theme.accent });
                let frame = match ring {
                    Some(color) => frame.border(px(theme::BORDER_WIDTH)).border_color(rgb(color)),
                    None if secondary => frame,
                    None => frame
                        .border(px(theme::BORDER_WIDTH))
                        .border_color(gpui::transparent_black()),
                };
                frame
                    .on_click(cx.listener(move |_, _, _, cx| cx.emit(PromptResponse(ix))))
                    .into_any_element()
            })
            .collect();
        let panel = div()
            .id("confirm-dialog")
            .w(px(theme::CONFIRM_DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(self.message.clone()),
            )
            .children(self.detail.clone().map(|detail| {
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text_muted))
                    .child(detail)
            }))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(space::S))
                    .pt(px(space::S))
                    .children(buttons),
            );
        // The shade blocks the window behind it; a click on it answers nothing (Esc or a
        // button does). The window stretches this view over its whole area.
        div()
            .id("confirm-shade")
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(theme.scrim))
            .occlude()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .child(panel)
    }
}
```

Notes for the implementer:
- If the compiler reports that one of `KeyDownEvent`, `Render`, `IntoElement`, `FontWeight`, `SharedString`, `PromptLevel`, `App`, `Window`, `Context` is not reachable through `use crate::*;`, add it to the explicit `use gpui::{…}` list in this file — do not edit `main.rs` imports for it.
- Run `cargo fmt --all`; it may reflow the `ring` / `frame` lines.

Add to `crates/mkrew-ui/src/shell.rs`, in the module list next to `mod comments;`:

```rust
pub(crate) mod confirm_prompt;
```

- [ ] **Step 2: Register it in `main`**

In `crates/mkrew-ui/src/main.rs`, inside `gpui_platform::application().run(move |cx: &mut App| { … })`, right after the `cx.set_global(theme::Fonts::from_settings(…));` statement and before `let options = WindowOptions {`:

```rust
        // Confirmations are drawn in the window in Mkrew's style, never the OS dialog (§14).
        cx.set_prompt_builder(shell::confirm_prompt::build);
```

- [ ] **Step 3: Mark the destructive confirms `Critical`**

Change `PromptLevel::Warning` to `PromptLevel::Critical` in exactly these six calls (find each by its message text; line numbers are from 2026-10-11):

| File | Line | Message |
|---|---|---|
| `crates/mkrew-ui/src/shell/tabs.rs` | 423 | `"This tab has a running agent."` |
| `crates/mkrew-ui/src/shell/tabs.rs` | 489 | `"This session is still running."` (close pane: Kill / Hide / Cancel) |
| `crates/mkrew-ui/src/shell/tabs.rs` | 571 | `"This session is still running."` (remove from sidebar) |
| `crates/mkrew-ui/src/shell/inspector.rs` | 476 | `"This session is still running."` (Stop session) |
| `crates/mkrew-ui/src/shell/task_dialog.rs` | 152 | `"Delete this task?"` |
| `crates/mkrew-ui/src/shell/workspace_actions.rs` | 508 | `format!("Remove “{name}” from Mkrew?")` |

Leave `crates/mkrew-ui/src/shell/files.rs` (`"Save changes to {name}?"`) at `PromptLevel::Warning`: its answer 0 is Save, drawn as the accent primary button.

- [ ] **Step 4: Keep the keyboard in the prompt**

In `crates/mkrew-ui/src/shell/focus.rs`, `pub(crate) fn focus_pane(&self, window: &mut Window, cx: &mut Context<'_, Self>)` (line ~90): add as the first statement of the body, above the existing "A modal dialog keeps the keyboard…" comment:

```rust
        // An open confirmation keeps the keyboard; it hands focus back when answered.
        if window.has_active_prompt() {
            return;
        }
```

- [ ] **Step 5: Build, lint and test**

Run: `cargo fmt --all` then `sh scripts/verify.sh`
Expected: fmt check clean, `clippy --workspace --all-targets -D warnings` clean, all tests pass (including the 4 `confirm::tests` from Task 1 and the existing `dialog_answers_map_to_choices`).

Then check the new view for literals: `git diff -U0 -- crates/mkrew-ui/src/shell/confirm_prompt.rs` plus the untracked file itself — search it for `#[0-9A-Fa-f]{6}` and `px\([0-9]` with the `grep` tool.
Expected: no hits.

- [ ] **Step 6: Smoke run**

Run: `cargo build -p mkrew-ui -j 4`, then start the app (`"$PWD/target/debug/mkrew.exe"`; if the exe is locked, rename the old one to `mkrew.locked-old.exe` first). With an agent pane running, click the pane's ✕.
Expected: no Windows message box; a Mkrew card appears over a dimmed window with "This session is still running.", the detail line, and buttons `Cancel · Hide pane · Kill session` (Kill session in the danger colors, ringed). Esc closes it and the pane stays. Repeat, press Shift+Tab then Enter → `Hide pane`: the pane closes, the session stays in the sidebar.
If the app cannot be driven from this session, say so in the report and rely on the HUMAN_CHECKS block from Step 7.

- [ ] **Step 7: Docs**

In `docs/SPEC.md` §14, after the bullet `- In-app toasts: bottom-right, auto-hide after 4 s, never blocking.`, add:

```markdown
- Confirmations (`window.prompt`) are drawn in the window, never as the OS message box
  (`App::set_prompt_builder`, `shell/confirm_prompt.rs`): the modal shade, a `surface` card
  (`CONFIRM_DIALOG_WIDTH`, radius 12, faint border, light shadow), the question as a card
  title, the detail in `text-muted`, buttons on the right with Cancel leftmost and the first
  answer rightmost (accent; the danger button for `PromptLevel::Critical`). Esc = Cancel,
  Enter/Space = the ringed button (default: the first answer), Tab/←/→ move the ring. A click
  on the shade does nothing. OS file/folder pickers stay native.
```

In `docs/HUMAN_CHECKS.md`, under `## To verify (non-blocking)`, insert above the first `### To verify — …` block:

```markdown
### To verify — 2026-10-11 in-app confirmation dialogs (plan docs/superpowers/plans/2026-10-11-in-app-confirm-dialogs.md)

- [ ] **Look (dark and light):** close the pane of a running agent, close a tab with a running
  agent, ✕ a running session in the sidebar, Stop in the session panel, Delete in Edit task,
  Remove workspace, and leave an edited file in Files. Each shows a Mkrew card over a dimmed
  window (no Windows message box). Destructive answers (Kill session, Remove, Delete, Stop
  session) are red; Save is the accent button; Cancel is on the left.
- [ ] **Keys:** Esc = Cancel; Tab / → / ← / Shift+Tab move the ring and wrap; Enter and Space
  pick the ringed button. A click on the dim area does nothing.
- [ ] **Keyboard stays in the prompt:** open the close-pane prompt while another agent is
  starting; Esc still closes the prompt (it never reaches a terminal). After Cancel, typing
  goes to the pane that had focus before.
- [ ] **Over the task dialog:** Edit task → Delete → Cancel: the task dialog is still open and
  its fields take typing; Esc then closes the task dialog.
- [ ] **Unsaved on close:** edit a file in Files, close the window: the in-app "Save changes to
  …?" shows; Cancel keeps the window; Don't save closes it.
- [ ] **Long text:** a deep file path in the save prompt and a long workspace name in Remove
  wrap inside the card at `[ui] font_size = 18`; the buttons stay on one row and visible.
- [ ] **Title bar:** while a prompt is open, the ─ □ ✕ window buttons and dragging the title bar
  behave acceptably (note what happens).
```

- [ ] **Step 8: Commit**

```bash
git add crates/mkrew-ui/src/shell/confirm_prompt.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/focus.rs crates/mkrew-ui/src/shell/tabs.rs crates/mkrew-ui/src/shell/inspector.rs crates/mkrew-ui/src/shell/task_dialog.rs crates/mkrew-ui/src/shell/workspace_actions.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git commit -m "feat(ui): draw confirmations in-app in Mkrew style instead of the OS dialog"
```
