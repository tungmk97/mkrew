# Inbox 2026-10-08: Keys, Fonts, View Rail Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix five items from `docs/INBOX.md`:
- Ctrl+A in the commit box.
- Ctrl+C copy in the terminal.
- Ctrl+Backspace word delete in the terminal (Ctrl+A in the terminal included).
- Font family and size settings for the app and the terminal.
- More room for the main content: the view bar moves to a right rail, and Run moves inline with History / + Agent.

**Architecture:**
- All key fixes go through the shared mappers:
  - `task_dialog::dialog_key` + `TextBox` for text fields.
  - `terminal::input::key_to_bytes` + `TerminalPane::key_down` for the terminal.
- Fonts:
  - Two new `settings.toml` sections, `[ui]` and `[terminal]`, live in `mkrew_core::settings`.
  - The UI publishes them as a GPUI `Global` (`theme::Fonts`). Both `Theme` builders read that global.
  - UI text sizes become `Rems`, scaled with `window.set_rem_size`.
  - A 500 ms mtime check reloads the settings live.
- The view switcher becomes a right column, `render_view_rail`. The Run button moves into the Terminals tab bar, and a compact copy sits at the foot of the rail.

**Tech Stack:** Rust 2024, GPUI (`gpui-pre =0.3.7`), alacritty_terminal 0.26, serde/toml. No new crates.

**Spec:** `docs/SPEC.md` (§10, §12B, §13, §14, §20.0, §20B.1, cited per task). There is no separate design doc. This plan comes from `docs/INBOX.md` (2026-10-08) and the investigation quoted under each task. Open questions are listed at the end, each with the default this plan uses.

**Inbox mapping:**

| Inbox line | Task |
| --- | --- |
| Ctrl+A in the git commit box | 1 |
| In terminal, Ctrl+C cannot copy | 2 |
| In terminal, Ctrl+Backspace / Ctrl+A in OMP | 3 |
| Config font style, font size for app and terminal | 4, 5 |
| Move the top bar; Run inline with History / + Agent | 6 |

## Global Constraints

- Windows native is first-class: ConPTY, named pipes, `PATHEXT`, `dunce` paths (AGENTS.md, SPEC §2.4).
- "An agent is just a terminal command. No per-agent code in the core." (SPEC §2.1). Terminal key bytes stay generic xterm, never omp-specific.
- Every color, spacing, radius and font size comes from `crates/mkrew-ui/src/theme.rs` (SPEC §14).
- No `unwrap`/`expect` outside tests unless provably infallible, with a comment saying why.
- `thiserror` in libraries, `anyhow` with context in binaries, `tracing` for logs.
- No test may call a real agent or the network.
- Deviating from the spec requires an ADR in `docs/decisions/` and a SPEC update in the same change. Next free ADR numbers: 0022, 0023, 0024.
- Few dependencies: add no new crates.
- GPUI rule: set `.hover()` at most once per element (`button()`/`quiet_button()` already set it).
- **User preference: never commit.** Leave every change uncommitted. The last step of each task prints the Conventional Commit message the user may use; do not run `git commit`.
- Leave the existing uncommitted human edits alone (`.github/workflows/*`, `Cargo.lock`, `crates/mkrew-ui/Cargo.toml`, `packaging/mkrew.iss`, `docs/BUILD_VI.md`).
- **Inbox:** when a task passes verification, change its `docs/INBOX.md` line from `[~]` to `[x]` (see the mapping above). Tasks 4–5 tick their line after Task 5.
- Verify on Windows with Git Bash: `& "C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`. If rustc crashes on mkrew-ui test builds (STATUS_STACK_BUFFER_OVERRUN), run these three in order:
  1. `cargo fmt --all -- --check`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace -j 4`
- If a build fails with "Access is denied" on an `.exe` (a running daemon or fake agent holds it), rename it to `*.locked-old.exe`. Never kill the user's sessions.
- `cargo insta` is not installed. Write accepted `.snap` files by hand.

## Review Focus

1. **A stale terminal selection turns an intended ^C into a copy.** A user selects text, then types a command, then presses Ctrl+C to stop it. Expected: the program gets ^C. Typing must clear the selection. This lives in `TerminalPane::key_down` (GPUI, no headless test); checked manually in Task 7 ("Select, type a letter, then Ctrl+C → it interrupts").
2. **Select-all, then paste, IME or Ctrl+Enter.** After Ctrl+A in the commit box:
   - a paste or a typed char replaces everything;
   - Ctrl+Enter commits the full text (not an empty one);
   - Esc keeps the text.

   Pinned by Task 1 `ctrl_a_then_backspace_typing_or_paste_replaces_the_whole_draft` and `select_all_does_not_change_commit_or_leave`.
3. **A broken or out-of-range `settings.toml`** (`font_size = 4`, a typo in a key). Expected: the UI keeps the last good fonts, and the daemon reports the error with `file:line:col`. Pinned by Task 4 `font_size_outside_8_to_32_is_an_error` and Task 5 `a_broken_settings_file_keeps_the_last_fonts`.
4. **A font family that is not installed, or a proportional one.** Expected: the terminal falls back to the platform list, and the grid never becomes proportional. Pinned by Task 5 `a_configured_family_is_used_only_when_installed`.
5. **The Run button outside Terminals.** After the view bar is gone, Run must still be reachable in Board/Docs/Files/Git, and the Run menu ▾ must still open. Checked manually in Task 7 (no headless GPUI layout tests exist).

---

## File Structure

| File | Change | Task |
| --- | --- | --- |
| `crates/mkrew-ui/src/task_dialog.rs` | `DialogKey::SelectAll`; mapper; exhaustive match | 1 |
| `crates/mkrew-ui/src/text_box.rs` | `all_selected` flag | 1 |
| `crates/mkrew-ui/src/git_actions.rs` | `CommitBox.text: TextBox` (removes duplicated editing code) | 1 |
| `crates/mkrew-ui/src/{base_dialog,quick_open,broadcast,peek,comments,run_setup}.rs` | `SelectAll` arm | 1 |
| `crates/mkrew-ui/src/main.rs` | `text_field(.., selected)`; commit box call | 1 |
| `crates/mkrew-ui/src/terminal/input.rs` | `is_copy_key`; Ctrl+Backspace = ESC DEL | 2, 3 |
| `crates/mkrew-ui/src/terminal/view.rs` | Ctrl+C copies a selection; typing clears it; `theme_for` reads fonts | 2, 5 |
| `crates/mkrew-core/src/settings.rs` | `[ui]`, `[terminal]` `FontSettings`; `template()` | 4 |
| `crates/mkrew-ui/src/theme.rs` | `font_size` as `Rems`; `Fonts` global; `Theme.ui_font_family` | 5 |
| `crates/mkrew-ui/src/keymap.rs` | `ensure_file(path, contents)` | 5 |
| `crates/mkrew-ui/src/palette.rs` | `Extra::OpenSettings` | 5 |
| `crates/mkrew-ui/src/preview.rs` | family is a `SharedString` | 5 |
| `crates/mkrew-ui/src/board.rs` | `View::command` | 6 |
| `crates/mkrew-ui/src/main.rs` | view rail, Run in tab bar, New task in Todo header | 6 |
| `docs/SPEC.md`, `docs/decisions/0022-0024`, `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md`, `docs/INBOX.md` | spec, ADRs, checks, notes | 2–7 |

---

### Task 1: Ctrl+A selects all in text boxes (commit box first)

Inbox: `In the git commit box: cannot Ctrl+A to select all to remove`. Spec: §12B, §20B.1.

**Root cause:**
- `task_dialog::dialog_key` (task_dialog.rs:55-58) returns `None` for every plain Ctrl+letter except Ctrl+V:
  ```rust
  if m.platform || (m.control && !m.alt) { return None; }
  ```
  A test pins this: `assert_eq!(key("ctrl-a"), None)` at l.332.
- No text model has a selection.
- `CommitBox` (git_actions.rs:24) holds a bare `String`. It duplicates `TextBox::key`/`insert`.

**Fix:**
- Add a `DialogKey::SelectAll` variant.
- Add an `all_selected` flag on `TextBox`: the next edit replaces everything.
- `CommitBox` wraps `TextBox`.
- `text_field` draws a selection highlight.

Every `TextBox` user gets the fix for free: composer, follow-up, task prompt and Run env. Single-line `String` fields (palette, find, peek, broadcast, comments, base ref) ignore `SelectAll` in this task (see open question 1).

**Files:**
- Modify: `crates/mkrew-ui/src/task_dialog.rs` (enum l.27-40, `dialog_key` l.43-64, `TaskDialog::key` l.145-173, test l.332)
- Modify: `crates/mkrew-ui/src/text_box.rs`
- Modify: `crates/mkrew-ui/src/git_actions.rs` (l.22-123 and its tests)
- Modify: `crates/mkrew-ui/src/base_dialog.rs:29`, `quick_open.rs:44`, `broadcast.rs:154`, `peek.rs:125`, `comments.rs:165`, `run_setup.rs:111-112`
- Modify: `crates/mkrew-ui/src/main.rs` (`text_field` l.13768, its 14 callers, commit box l.9531, l.1911-1913)

**Interfaces:**
- Produces:
  - `DialogKey::SelectAll`
  - `TextBox::all_selected(&self) -> bool`
  - `CommitBox.text: TextBox` (was `String`)
  - `fn text_field(id, text: &str, placeholder, focused: bool, selected: bool, theme) -> Stateful<Div>`

- [ ] **Step 1: Write the failing tests**

In `crates/mkrew-ui/src/task_dialog.rs` `keys_map_to_dialog_actions`, replace `assert_eq!(key("ctrl-a"), None);` with:

```rust
        assert_eq!(key("ctrl-a"), Some(DialogKey::SelectAll));
        assert_eq!(key("cmd-a"), Some(DialogKey::SelectAll));
        assert_eq!(key("ctrl-b"), None, "other Ctrl+letters still type nothing");
```

(If the `key` helper in that test module does not parse `cmd`, use the modifier word it accepts for `platform`. Check the helper at l.278.)

In `crates/mkrew-ui/src/text_box.rs` `mod tests`, add:

```rust
    #[test]
    fn select_all_makes_the_next_edit_replace_everything() {
        let mut text = TextBox::default();
        text.insert("one\ntwo");
        text.key(DialogKey::SelectAll);
        assert!(text.all_selected());
        text.key(DialogKey::Backspace);
        assert_eq!(text.text(), "", "Backspace removes everything");
        assert!(!text.all_selected());

        text.key(DialogKey::SelectAll);
        assert!(!text.all_selected(), "nothing to select in an empty box");

        text.insert("old");
        text.key(DialogKey::SelectAll);
        text.key(DialogKey::Enter);
        assert_eq!(text.text(), "\n", "Enter replaces the selection with a new line");
    }
```

In `crates/mkrew-ui/src/git_actions.rs` `mod tests`, add:

```rust
    #[test]
    fn ctrl_a_then_backspace_typing_or_paste_replaces_the_whole_draft() {
        let mut draft = CommitBox::default();
        type_text(&mut draft, "wip\nmore");
        assert_eq!(draft.key(DialogKey::SelectAll), Outcome::Edited);
        draft.key(DialogKey::Backspace);
        assert_eq!(draft.text.text(), "", "Backspace removes everything");

        type_text(&mut draft, "old");
        draft.key(DialogKey::SelectAll);
        type_text(&mut draft, "fix: x");
        assert_eq!(draft.text.text(), "fix: x", "typing replaces the selection");
        draft.key(DialogKey::Backspace);
        assert_eq!(draft.text.text(), "fix: ", "the selection is gone after one edit");

        draft.key(DialogKey::SelectAll);
        draft.insert("pasted");
        assert_eq!(draft.text.text(), "pasted", "paste replaces the selection");
    }

    #[test]
    fn select_all_does_not_change_commit_or_leave() {
        let mut draft = CommitBox::default();
        draft.insert("Ship it");
        draft.key(DialogKey::SelectAll);
        assert_eq!(draft.key(DialogKey::Cancel), Outcome::Leave);
        assert_eq!(draft.text.text(), "Ship it", "Esc keeps the text");
        assert_eq!(draft.key(DialogKey::Save), Outcome::Commit);
        assert_eq!(
            draft.begin(Ulid::new(), 1).map(|p| p.message),
            Some("Ship it".to_owned()),
            "Ctrl+Enter commits the whole selected text"
        );
        let mut busy = CommitBox { busy: true, ..CommitBox::default() };
        busy.key(DialogKey::SelectAll);
        assert!(!busy.text.all_selected(), "frozen while committing");
    }
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p mkrew-ui --lib task_dialog text_box git_actions -j 4`
Expected: compile error `no variant named SelectAll` (and `no method all_selected`).

- [ ] **Step 3: Add the variant and map it**

`crates/mkrew-ui/src/task_dialog.rs`: in `enum DialogKey`, after `Paste,` add:

```rust
    /// Ctrl+A (Cmd+A): the next edit replaces the whole text.
    SelectAll,
```

In `dialog_key`, after the `"v" if command …` arm, add:

```rust
        "a" if command && !m.alt && !m.shift => return Some(DialogKey::SelectAll),
```

In `TaskDialog::key`, replace `DialogKey::Paste => {}` with:

```rust
            DialogKey::SelectAll => {
                if self.field == Field::Prompt {
                    self.prompt.key(DialogKey::SelectAll);
                }
            }
            DialogKey::Paste => {}
```

- [ ] **Step 4: Add select-all to `TextBox`**

Replace the struct and the `clear`/`key`/`insert` methods in `crates/mkrew-ui/src/text_box.rs` with:

```rust
/// The text of a multi-line box; line breaks are always `\n`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextBox {
    text: String,
    /// Ctrl+A was pressed: the next edit replaces the whole text.
    all_selected: bool,
}
```

```rust
    /// Whether the whole text is selected (drawn highlighted; the next edit replaces it).
    pub fn all_selected(&self) -> bool {
        self.all_selected
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.all_selected = false;
    }

    /// Applies `key`; the caller inserts the clipboard text on [`DialogKey::Paste`].
    pub fn key(&mut self, key: DialogKey) -> Outcome {
        match key {
            DialogKey::Cancel => return Outcome::Cancel,
            DialogKey::Save => return Outcome::Submit,
            DialogKey::SelectAll => self.all_selected = !self.text.is_empty(),
            DialogKey::Enter => self.insert("\n"),
            DialogKey::Backspace => {
                if std::mem::take(&mut self.all_selected) {
                    self.text.clear();
                } else {
                    self.text.pop();
                }
            }
            DialogKey::Insert(text) => self.insert(&text),
            DialogKey::SwitchField | DialogKey::Paste => {}
        }
        Outcome::Edited
    }

    /// Appends typed or pasted text (replacing a select-all), turning `\r\n` and `\r` line
    /// breaks into `\n`.
    pub fn insert(&mut self, text: &str) {
        if std::mem::take(&mut self.all_selected) {
            self.text.clear();
        }
        self.text
            .push_str(&text.replace("\r\n", "\n").replace('\r', "\n"));
    }
```

Update the module doc (l.1-3): "Text is typed and deleted at the end, like the other fields; Ctrl+A selects all, and the next edit replaces it."

- [ ] **Step 5: `CommitBox` wraps `TextBox`**

In `crates/mkrew-ui/src/git_actions.rs`, add `use crate::text_box::TextBox;`. Then change the field to `pub text: TextBox,` and replace `key` and `insert` with:

```rust
    /// Applies a key; Enter is a new line. Paste is the caller's (see [`CommitBox::insert`]).
    pub fn key(&mut self, key: DialogKey) -> Outcome {
        match key {
            DialogKey::Cancel => Outcome::Leave,
            DialogKey::Save => Outcome::Commit,
            _ if self.busy || self.generating.is_some() => Outcome::Edited,
            key => {
                self.text.key(key);
                self.error = None;
                Outcome::Edited
            }
        }
    }

    /// Types or pastes `text` (CRLF becomes LF); ignored while committing or generating.
    pub fn insert(&mut self, text: &str) {
        if self.busy || self.generating.is_some() {
            return;
        }
        self.text.insert(text);
        self.error = None;
    }
```

The other methods change as follows:
- `begin`: `let message = self.text.text().trim();`
- `committed` `Ok(())` arm: `self.text.clear()`
- `generated` `Ok(text)` arm:
  ```rust
  Ok(text) => {
      self.text.clear();
      self.text.insert(&text);
  }
  ```

Existing tests:
- `commit.text, "…"` / `draft.text, "…"` become `commit.text.text(), "…"` (l.213, 225, 239, 265, 282, 288, 316).
- The struct literals `CommitBox { text: "wip".into(), ..CommitBox::default() }` (l.203, 232) become:

```rust
        let mut commit = CommitBox::default();
        commit.insert("wip");
```

Note a behavior change: `SwitchField` and `SelectAll` now clear `error` too. `nothing_staged_or_no_message_is_explained_instead_of_sent` still passes because it only checks Backspace and typing.

- [ ] **Step 6: Exhaustive matches**

- `base_dialog.rs:29` and `quick_open.rs:44`: `DialogKey::SwitchField | DialogKey::SelectAll => None,`
- `broadcast.rs:154`: `Some(DialogKey::SelectAll) | None => Outcome::Edited,`
- `peek.rs:125`: `Some(DialogKey::SwitchField | DialogKey::SelectAll) | None => Outcome::Edited,`
- `comments.rs:165`: `DialogKey::SwitchField | DialogKey::Paste | DialogKey::SelectAll => {}`
- `run_setup.rs`: before `DialogKey::Paste => Outcome::Edited,` add:

```rust
            DialogKey::SelectAll => {
                if let Focus::Field(i, Field::Env) = self.focus {
                    self.cards[i].env.key(DialogKey::SelectAll);
                }
                Outcome::Edited
            }
```

`composer.rs` and `follow_up.rs` forward through wildcard arms and need no change.

- [ ] **Step 7: Draw the selection**

In `crates/mkrew-ui/src/main.rs`, add a `selected: bool` parameter to `text_field` after `focused`. Change the line rows (l.13814-13818) to:

```rust
                div()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .min_w_0()
                            .when(selected, |el| el.bg(rgb(theme.terminal_selection_bg)))
                            .child(line.to_owned()),
                    )
                    .when(focused && !selected && i == last, |row| row.child(caret()))
```

Update the doc comment: "…a caret at the end while `focused`, the lines highlighted while `selected` (Ctrl+A)…".

Callers:
- The commit box (l.9531) passes `draft.text.text()` and `draft.text.all_selected()`.
- Callers that hold a `TextBox` (composer, follow-up, task prompt, run env) pass its `all_selected()`.
- All other callers pass `false`.

Fix compile errors at l.1911-1913. `std::mem::take(&mut old.commit.text)` keeps working because `TextBox: Default`.

- [ ] **Step 8: Run the tests and confirm they pass**

Run: `cargo test -p mkrew-ui --lib -j 4`
Expected: PASS.

- [ ] **Step 9: Verify and tick**

Run the full verification from Global Constraints. In `docs/INBOX.md`, tick the commit-box line `[x]`.
Suggested message: `fix(ui): Ctrl+A selects the whole text in the commit box and multi-line boxes`

---

### Task 2: Ctrl+C copies the terminal selection (ADR 0022)

Inbox: `In terminal, Ctrl+C now cannot be used to copy text, please fix`. Spec: §10 (terminal rendering), §13.

**Current behavior:** this is not a regression.
- `keymap.rs:288` binds `Command::Copy` to `ctrl-shift-c` only.
- Plain Ctrl+C is unbound on purpose. It reaches `TerminalPane::key_down` (view.rs:217-225) and `control_byte` turns it into `0x03`.
- SPEC §13 l.846-848 and l.871 say "Copy / paste | Ctrl+Shift+C / Ctrl+Shift+V".

The user expects Windows Terminal behavior: Ctrl+C copies when text is selected, else it sends ^C. A second effect: alacritty drops a selection when the selected lines are erased (`clear_line`, `swap_alt`, `clear_screen`), so a selection over an omp spinner can vanish. That is out of scope; it is recorded in the ADR.

**Fix:**
- In `TerminalPane::key_down`, plain Ctrl+C with a selection copies it and stops; without a selection it is still ^C.
- Sending any key bytes clears the selection. A stale selection therefore never swallows a later ^C (Review Focus 1).

**Files:**
- Modify: `crates/mkrew-ui/src/terminal/input.rs` (new `is_copy_key` + test)
- Modify: `crates/mkrew-ui/src/terminal/view.rs` (`key_down` l.217-225, `run` Copy arm l.327-333)
- Create: `docs/decisions/0022-terminal-ctrl-c-copies-a-selection.md`
- Modify: `docs/SPEC.md` §13 (l.846-848, l.871)

**Interfaces:**
- Produces:
  - `pub fn is_copy_key(keystroke: &Keystroke) -> bool` in `terminal::input`
  - `fn copy_selection(&mut self, cx: &mut Context<'_, Self>) -> bool` on `TerminalPane`

- [ ] **Step 1: Write the failing tests**

In `crates/mkrew-ui/src/terminal/input.rs` `mod tests`:

```rust
    #[test]
    fn only_plain_ctrl_c_is_the_copy_key() {
        assert!(is_copy_key(&key("ctrl-c", None)));
        assert!(!is_copy_key(&key("ctrl-shift-c", None)), "bound Copy, handled by the keymap");
        assert!(!is_copy_key(&key("ctrl-alt-c", None)), "AltGr / Alt+^C");
        assert!(!is_copy_key(&key("c", Some("c"))));
        assert!(!is_copy_key(&key("ctrl-x", None)));
        // Without a selection it is still ^C for the program.
        assert_eq!(bytes("ctrl-c", None, NORMAL), Some(vec![0x03]));
    }
```

The "typing clears the selection" rule lives in `TerminalPane::key_down` (GPUI) and has no headless test. Task 7 checks it by hand. Do not add an emulator test that only calls `clear_selection()` and checks it cleared: it pins nothing.

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p mkrew-ui --lib terminal:: -j 4`
Expected: compile error `cannot find function is_copy_key`.

- [ ] **Step 3: Implement `is_copy_key`**

`crates/mkrew-ui/src/terminal/input.rs`, after `key_to_bytes`:

```rust
/// Plain Ctrl+C: copies the selection when there is one, else it is ^C for the program
/// (like Windows Terminal, ADR 0022).
pub fn is_copy_key(keystroke: &Keystroke) -> bool {
    let m = keystroke.modifiers;
    keystroke.key == "c" && m.control && !m.shift && !m.alt && !m.platform
}
```

- [ ] **Step 4: Use it in the pane**

`crates/mkrew-ui/src/terminal/view.rs`: import `is_copy_key` next to `key_to_bytes` (l.32). Add a method to `impl TerminalPane`:

```rust
    /// Copies the selection to the clipboard and clears it; `false` when nothing is selected.
    fn copy_selection(&mut self, cx: &mut Context<'_, Self>) -> bool {
        let Some(text) = self.emulator.selection_text() else {
            return false;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.emulator.clear_selection();
        cx.notify();
        true
    }
```

Replace `key_down` (l.217-225) with:

```rust
    /// Keys go to the session. Keystrokes bound in the keymap (SPEC §13) never get here: their
    /// action handler stops them first. Plain Ctrl+C copies a selection instead (ADR 0022);
    /// any key sent clears the selection, so a later Ctrl+C interrupts again.
    fn key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.exited.is_some() {
            return;
        }
        if is_copy_key(&event.keystroke) && self.copy_selection(cx) {
            cx.stop_propagation();
            return;
        }
        if let Some(data) = key_to_bytes(&event.keystroke, self.emulator.input_mode()) {
            cx.stop_propagation();
            self.emulator.clear_selection();
            self.input(data, cx);
        }
    }
```

Replace the `Command::Copy` arm (l.327-333) with:

```rust
            Command::Copy => {
                self.copy_selection(cx);
            }
```

- [ ] **Step 5: ADR and SPEC**

Create `docs/decisions/0022-terminal-ctrl-c-copies-a-selection.md` from `0000-template.md`:
- Date 2026-10-08, Status accepted, Milestone: inbox 2026-10-08.
- Context: the user expects Windows Terminal behavior. SPEC §13 intercepts only bound keys. Also record that alacritty drops a selection when its lines are erased (`clear_line`, `swap_alt`, `clear_screen` in alacritty_terminal 0.26 `term/mod.rs`); that is not changed here.
- Options:
  1. Ctrl+C copies when there is a selection.
  2. Ctrl+Shift+C only, plus a hint.
  3. Copy on mouse-up.
- Decision: option 1. Typing clears the selection.
- Consequences: the §13 text below.

`docs/SPEC.md` §13:
- l.846-848: after "everything else goes to the agent." add "One exception: plain Ctrl+C copies while terminal text is selected (ADR 0022); without a selection it reaches the agent as ^C. Any key sent to the agent clears the selection."
- Table row l.871: `| Terminal | Copy / paste | Ctrl+Shift+C (or Ctrl+C while text is selected) / Ctrl+Shift+V |`.

- [ ] **Step 6: Run the tests and confirm they pass**

Run: `cargo test -p mkrew-ui --lib terminal:: keymap -j 4`
Expected: PASS. `agent_keys_are_unbound_and_reach_the_pty` still passes because Ctrl+C stays unbound in the keymap.

- [ ] **Step 7: Verify and tick**

Run the full verification. Tick the Ctrl+C inbox line `[x]` only after the manual check in Task 7 is listed.
Suggested message: `feat(ui): Ctrl+C copies the terminal selection, like Windows Terminal (ADR 0022)`

---

### Task 3: Ctrl+Backspace deletes a word in the terminal

Inbox: `In terminal when typing e.g in OMP cannot Ctrl + Backspace to delete a text now just delete by character. And cannot Ctrl + A to select all to remove`. Spec: §10 (terminal rendering).

**Root cause:**
- `input.rs:93` `"backspace" if ctrl => vec![0x08]`.
- omp's TUI (pi-tui `keys.ts:1287`) reads `\x08` as plain Backspace unless `WT_SESSION` is set. Its word delete is bound to `ctrl+w` / `alt+backspace`.
- `\x1b\x7f` (ESC DEL, what Alt+Backspace sends) is word rubout in readline, pi-tui and Claude Code.

**Ctrl+A** stays `\x01`. Every terminal sends that, and omp and readline read it as "line start". A terminal "select all" cannot delete an app's input buffer. To clear the input in omp, use Ctrl+U, or Ctrl+C (omp clears the editor). This task changes no code for Ctrl+A; it records the answer in HUMAN_CHECKS (Task 7) and in this plan.

**Files:**
- Modify: `crates/mkrew-ui/src/terminal/input.rs:93` and test `editing_keys` l.238

- [ ] **Step 1: Change the failing test first**

In `editing_keys`, replace the l.238 line with:

```rust
        // Word rubout: ESC DEL (= Alt+Backspace), what readline, pi-tui and Claude Code bind.
        assert_eq!(bytes("ctrl-backspace", None, NORMAL), Some(b"\x1b\x7f".to_vec()));
        assert_eq!(bytes("alt-backspace", None, NORMAL), Some(b"\x1b\x7f".to_vec()));
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `cargo test -p mkrew-ui --lib terminal::input -j 4`
Expected: FAIL, `left: Some([8])`, `right: Some([27, 127])`.

- [ ] **Step 3: Implement**

`crates/mkrew-ui/src/terminal/input.rs:93`:

```rust
        // Word rubout (ESC DEL, = Alt+Backspace): `\x08` reads as a plain Backspace in most
        // TUIs outside Windows Terminal.
        "backspace" if ctrl => b"\x1b\x7f".to_vec(),
```

Do not change `crates/spike-term` (the M0 spike).

- [ ] **Step 4: Run it and confirm it passes**

Run: `cargo test -p mkrew-ui --lib terminal::input -j 4`
Expected: PASS.

- [ ] **Step 5: Verify and tick**

Run the full verification. Tick the Ctrl+Backspace inbox line `[x]` and append ` (Ctrl+A stays line start; clear with Ctrl+U)`.
Suggested message: `fix(ui): Ctrl+Backspace deletes a word in the terminal`

---

### Task 4: `[ui]` and `[terminal]` font settings in mkrew-core

Inbox: `Add a feature to config font style, font size... for the app and for the terminal` (part 1/2). Spec: §14, §20.0.

**Current state:**
- `mkrew_core::settings::Settings` (settings.rs:16-21) has `notify`, `awake`, `composer` and `zed`, with `deny_unknown_fields`.
- The daemon parses the same struct, so the new keys must live here. The daemon then validates them and reports errors through `daemon.status.settings_error` with no daemon change.
- Sizes are integer px, so `Settings` stays `Eq`.

**Files:**
- Modify: `crates/mkrew-core/src/settings.rs`
- Modify: `docs/SPEC.md` §20.0 (l.1248-1262), §14 (l.944-949)
- Create: `docs/decisions/0024-font-settings.md` (written in Task 5 Step 9, after the scaling approach is proven)

**Interfaces:**
- Produces (in `mkrew_core::settings`):
  - `pub struct FontSettings { pub font_family: String, pub font_size: FontSize }` (Default: `""`, 13)
  - `pub struct FontSize(pub u16)`: 8–32, checked when parsed
  - `Settings.ui: FontSettings`, `Settings.terminal: FontSettings`
  - `pub fn template() -> &'static str`: the commented file Task 5 writes

- [ ] **Step 1: Write the failing tests**

In `crates/mkrew-core/src/settings.rs` `mod tests`:

```rust
    #[test]
    fn font_size_outside_8_to_32_is_an_error() {
        let message = parse_err("[terminal]\nfont_size = 4\n");
        assert!(message.starts_with("settings.toml:2:"), "{message}");
        assert!(message.contains("8–32"), "{message}");
        let message = parse_err("[ui]\nfont_size = 33\n");
        assert!(message.contains("8–32"), "{message}");
        let ok = parse(
            Path::new("settings.toml"),
            "[ui]\nfont_size = 8\n[terminal]\nfont_size = 32\nfont_family = \"Consolas\"\n",
        )
        .unwrap();
        assert_eq!(ok.ui.font_size, FontSize(8));
        assert_eq!(ok.terminal.font_size, FontSize(32));
        assert_eq!(ok.terminal.font_family, "Consolas");
        assert_eq!(ok.ui.font_family, "", "a missing family is the built-in default");
    }

    #[test]
    fn the_template_parses_to_the_defaults_commented_or_not() {
        assert_eq!(parse(Path::new("settings.toml"), template()).unwrap(), Settings::default());
        let uncommented: String = template()
            .lines()
            .map(|line| line.strip_prefix("# ").filter(|l| l.contains(" = ")).unwrap_or(line))
            .map(|line| format!("{line}\n"))
            .collect();
        assert_eq!(
            parse(Path::new("settings.toml"), &uncommented).unwrap(),
            Settings::default(),
            "{uncommented}"
        );
    }
```

Extend `missing_file_is_defaults` with:

```rust
        assert_eq!(settings.ui, FontSettings { font_family: String::new(), font_size: FontSize(13) });
        assert_eq!(settings.terminal, settings.ui);
```

Extend `full_file_parses`: add `[ui]\nfont_family = "Inter"\nfont_size = 14\n\n[terminal]\nfont_family = "JetBrains Mono"\nfont_size = 15\n` to `src`. Add the two fields to the expected `Settings { … }`.

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p mkrew-core settings`
Expected: compile errors `FontSize`, `FontSettings`, `template` not found.

- [ ] **Step 3: Implement**

Add the fields to `Settings` after `zed`:

```rust
    /// `[ui]`: the app's font (SPEC §14, §20.0).
    pub ui: FontSettings,
    /// `[terminal]`: the terminal font; also used by the Files and Git code views.
    pub terminal: FontSettings,
```

After `ZedSettings`:

```rust
/// `[ui]` / `[terminal]` font. An empty family is the built-in default (SPEC §14).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FontSettings {
    pub font_family: String,
    pub font_size: FontSize,
}

impl Default for FontSettings {
    fn default() -> Self {
        Self {
            font_family: String::new(),
            font_size: FontSize(13),
        }
    }
}

/// A font size in px, 8–32 (checked when parsed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "u16")]
pub struct FontSize(pub u16);

impl TryFrom<u16> for FontSize {
    type Error = String;

    fn try_from(px: u16) -> Result<Self, String> {
        if (8..=32).contains(&px) {
            Ok(Self(px))
        } else {
            Err(format!("font_size must be 8–32 px, got {px}"))
        }
    }
}

/// The commented `settings.toml` written when the user opens settings and none exists:
/// every key at its default (SPEC §20.0).
pub fn template() -> &'static str {
    "# Mkrew settings (SPEC §20.0). Uncomment a line to change it; changes apply on save.\n\
     # A mistake is reported in the command palette and the last good settings stay.\n\
     \n\
     [notify]\n\
     # needs_input = true\n\
     # done = true\n\
     # sound = true\n\
     \n\
     [awake]\n\
     # keep_awake = true\n\
     \n\
     [composer]\n\
     # file_ref = \"@{path}\"\n\
     \n\
     [zed]\n\
     # follow_debounce_ms = 1500\n\
     \n\
     # App font: an installed family name (\"\" = the system UI font); size 8–32 px.\n\
     [ui]\n\
     # font_family = \"\"\n\
     # font_size = 13\n\
     \n\
     # Terminal, Files and Git code font: pick a monospace family (\"\" = Cascadia Code /\n\
     # SF Mono / JetBrains Mono, the first installed); size 8–32 px.\n\
     [terminal]\n\
     # font_family = \"\"\n\
     # font_size = 13\n"
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p mkrew-core settings`
Expected: PASS. If the out-of-range error does not start with `settings.toml:2:`, check that `parse` maps `e.span()`. toml attaches the value span to `try_from` errors. Read the actual message and fix the assertion only if the position is still on line 2.

- [ ] **Step 5: SPEC**

`docs/SPEC.md`:
- §20.0: append to the toml block:

```toml

[ui]
font_family = ""            # §14; "" = system UI font; other UI sizes keep their ratio to it
font_size = 13              # px, 8–32

[terminal]
font_family = ""            # §14; "" = first installed of the platform list; use a monospace family
font_size = 13              # px, 8–32; also the Files and Git code views
```

- §20.0 prose (l.1245-1246): replace "The UI reads it the same way." with "The UI reads it the same way and re-reads it within 500 ms of a change (fonts apply live, ADR 0024). Palette `Global: Open settings` creates the commented file if missing and opens it in Zed."
- §14 l.944-945: replace the line with "UI font = system font (Segoe UI Variable / SF / Inter) unless `[ui] font_family` is set. Sizes 12 meta, 13 body, 15 card title, 20 view title at the default `[ui] font_size = 13`; other sizes keep their ratio to body (ADR 0024). Weights 400/500/600. No all-caps text."
- §14 l.948-949: after "the first installed one, 13 px" add "; `[terminal] font_family` / `font_size` override them (a family that is not installed is ignored)".

- [ ] **Step 6: Verify**

Run: `cargo test -p mkrew-core && cargo test -p mkrew-daemon settings -j 4`
Expected: PASS. The daemon's settings tests still pass, and the new keys are not reported as unknown.
Suggested message: `feat(core): [ui] and [terminal] font settings`

---

### Task 5: Fonts apply to the app and the terminal, live (ADR 0024)

Inbox: font settings (part 2/2). Spec: §10 (monospace cell size), §14, §20.0.

**Current state:**
- `theme::font_size` holds `f32` constants. There are 193 `.text_size(px(font_size::X))` and 3 `.h(px(font_size::X))` sites.
- `theme::UI_FONT` is a const, used at 36 sites.
- The terminal family is picked once into `static TERMINAL_FONT: OnceLock` (theme.rs:258-281).
- `Theme.terminal_font_family: &'static str` and `terminal_font_size = font_size::BODY`.
- `Theme` is built per frame in `Shell::render` (main.rs:16145) and in `terminal::view::theme_for` (view.rs:571).
- `TerminalPane::render` already clears its row cache when the theme changes (view.rs:580-584). The next prepaint re-measures the cell and calls `fit`, which resizes the PTY. **A live terminal font change needs no new resize code.**

**Fix:**
- A `theme::Fonts` GPUI `Global`, built from `Settings` and the installed font list.
- `Theme::with_fonts` copies it in.
- UI sizes become `Rems` relative to body. `Shell::render` calls `window.set_rem_size(px(fonts.ui_size))`.
- The 500 ms status tick reloads `settings.toml` when its mtime changes.
- A palette entry `Global: Open settings`.

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (l.31-37, 64-65, 235-281, 325-326, 457-458, tests)
- Modify: `crates/mkrew-ui/src/main.rs` (l.130 startup, l.220/369/551 Shell fields, l.383-405 tick, l.3433-3436 palette handler, l.3770 `open_keymap_settings`, l.16145 render; all `px(font_size::` and `theme::UI_FONT` sites)
- Modify: `crates/mkrew-ui/src/terminal/view.rs` (l.127, 571-584; 5 `px(font_size::` and 2 `UI_FONT` sites; l.852)
- Modify: `crates/mkrew-ui/src/preview.rs` (l.231, 257, 297, 378, 496: the family is a `SharedString`, so add `.clone()`)
- Modify: `crates/mkrew-ui/src/keymap.rs` (`ensure_file` l.366, its tests l.922/926)
- Modify: `crates/mkrew-ui/src/palette.rs` (`Extra` l.49-117, test l.836)
- Create: `docs/decisions/0024-font-settings.md`

**Interfaces:**
- Consumes: `mkrew_core::settings::{Settings, FontSettings, FontSize, template}` (Task 4).
- Produces:
  - `theme::font_size::{META, BODY, CARD_TITLE, VIEW_TITLE}: gpui::Rems`
  - `pub struct theme::Fonts { ui_family: SharedString, ui_size: f32, terminal_family: SharedString, terminal_size: f32 }`, which implements `gpui::Global`
  - `Fonts::from_settings(settings: &Settings, installed: &[String]) -> Fonts`
  - `pub fn theme::reload_fonts(path: &Path, installed: &[String], last: &Fonts) -> Option<Fonts>`
  - `Theme::with_fonts(self, fonts: &Fonts) -> Theme`
  - `Theme.ui_font_family: SharedString`
  - `Theme.terminal_font_family: SharedString` (was `&'static str`)
  - `keymap::ensure_file(path: &Path, contents: &str) -> io::Result<()>`
  - `palette::Extra::OpenSettings`

- [ ] **Step 1: Write the failing tests**

In `crates/mkrew-ui/src/theme.rs` `mod tests`:

```rust
    use mkrew_core::settings::{FontSize, Settings};

    #[test]
    fn a_configured_family_is_used_only_when_installed() {
        let mut settings = Settings::default();
        settings.terminal.font_family = "JetBrains Mono".into();
        settings.terminal.font_size = FontSize(18);
        settings.ui.font_family = "Inter".into();
        settings.ui.font_size = FontSize(15);
        let installed = vec!["JetBrains Mono".to_owned(), TERMINAL_FONTS[0].to_owned()];

        let fonts = Fonts::from_settings(&settings, &installed);
        assert_eq!(fonts.terminal_family.as_ref(), "JetBrains Mono");
        assert_eq!((fonts.terminal_size, fonts.ui_size), (18.0, 15.0));
        assert_eq!(fonts.ui_family.as_ref(), "Inter", "UI families fall back in GPUI itself");

        let fonts = Fonts::from_settings(&settings, &installed[1..]);
        assert_eq!(
            fonts.terminal_family.as_ref(),
            TERMINAL_FONTS[0],
            "a missing family never makes the grid proportional"
        );
    }

    #[test]
    fn default_settings_give_the_spec_fonts() {
        let fonts = Fonts::from_settings(&Settings::default(), &[]);
        assert_eq!(fonts.ui_family.as_ref(), UI_FONT);
        assert_eq!(fonts.terminal_family.as_ref(), TERMINAL_FONTS[0]);
        assert_eq!((fonts.ui_size, fonts.terminal_size), (13.0, 13.0));
        let theme = Theme::dark().with_fonts(&fonts);
        assert_eq!(theme.terminal_font_size, 13.0);
        assert_eq!(theme, Theme::dark(), "the default theme already uses the default fonts");
    }

    #[test]
    fn a_broken_settings_file_keeps_the_last_fonts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let last = Fonts::from_settings(&Settings::default(), &[]);
        std::fs::write(&path, "[terminal]\nfont_size = 18\n").unwrap();
        let grown = reload_fonts(&path, &[], &last).expect("changed");
        assert_eq!(grown.terminal_size, 18.0);
        std::fs::write(&path, "[terminal]\nfont_size = 4\n").unwrap();
        assert_eq!(reload_fonts(&path, &[], &grown), None, "out of range: keep the last fonts");
        std::fs::write(&path, "[terminal]\nfont_size = 18\n").unwrap();
        assert_eq!(reload_fonts(&path, &[], &grown), None, "unchanged: no redraw");
    }
```

(`tempfile` is already a dev-dependency of mkrew-ui. Do not edit `crates/mkrew-ui/Cargo.toml`: it holds the user's uncommitted changes.)

In `crates/mkrew-ui/src/keymap.rs` tests (l.922/926), change the calls to `ensure_file(&path, &template())`. In `palette.rs` add to the existing uniqueness test (l.836): `assert!(titles.contains(&"Global: Open settings"));`.

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p mkrew-ui --lib theme keymap palette -j 4`
Expected: compile errors `Fonts`, `reload_fonts`, `with_fonts` not found, and `ensure_file` takes 1 argument.

- [ ] **Step 3: `Fonts` in theme.rs**

Replace `pub mod font_size` (l.31-37) with:

```rust
/// UI font sizes relative to body text (1 rem = `[ui] font_size`, default 13 px, set per
/// frame with `Window::set_rem_size`): 12 meta, 13 body, 15 card title, 20 view title at the
/// default (SPEC §14, ADR 0024).
pub mod font_size {
    use gpui::{Rems, rems};

    pub const META: Rems = rems(12.0 / 13.0);
    pub const BODY: Rems = rems(1.0);
    pub const CARD_TITLE: Rems = rems(15.0 / 13.0);
    pub const VIEW_TITLE: Rems = rems(20.0 / 13.0);
}
```

Keep `pub const UI_FONT` (l.65). It is now only the default family name. Delete `TERMINAL_FONT`, `init_terminal_font` and `terminal_font_family()` (l.257-281). Keep `pick_family`. Add:

```rust
/// The fonts of `settings.toml` (SPEC §20.0), as a GPUI global read by every [`Theme`].
#[derive(Clone, Debug, PartialEq)]
pub struct Fonts {
    pub ui_family: SharedString,
    pub ui_size: f32,
    pub terminal_family: SharedString,
    pub terminal_size: f32,
}

impl gpui::Global for Fonts {}

impl Fonts {
    /// `installed` = `cx.text_system().all_font_names()`. A terminal family that is not
    /// installed would silently become a proportional UI font, so it falls back to the list.
    pub fn from_settings(settings: &Settings, installed: &[String]) -> Self {
        let terminal = &settings.terminal.font_family;
        let terminal_family = if !terminal.is_empty() && installed.iter().any(|n| n == terminal) {
            SharedString::from(terminal.clone())
        } else {
            pick_family(TERMINAL_FONTS, installed).into()
        };
        let ui_family = if settings.ui.font_family.is_empty() {
            UI_FONT.into()
        } else {
            settings.ui.font_family.clone().into()
        };
        Self {
            ui_family,
            ui_size: settings.ui.font_size.0.into(),
            terminal_family,
            terminal_size: settings.terminal.font_size.0.into(),
        }
    }
}

/// The fonts of `path` when they differ from `last`; `None` when unchanged or when the file
/// is broken (the daemon reports it, §20.0, and the last good fonts stay).
pub fn reload_fonts(path: &Path, installed: &[String], last: &Fonts) -> Option<Fonts> {
    let settings = mkrew_core::settings::load(path).ok()?;
    let fonts = Fonts::from_settings(&settings, installed);
    (fonts != *last).then_some(fonts)
}
```

Imports at the top of theme.rs: `use std::path::Path;`, `use gpui::SharedString;`, `use mkrew_core::settings::Settings;`.

In `Theme`:
- Replace `pub terminal_font_family: &'static str,` with `pub terminal_font_family: SharedString,`.
- Add `pub ui_font_family: SharedString,` above it.

In `shared()` (l.457-458):

```rust
            ui_font_family: UI_FONT.into(),
            terminal_font_family: pick_family(TERMINAL_FONTS, &[]).into(),
            terminal_font_size: 13.0,
```

Add to `impl Theme`:

```rust
    /// This theme with the fonts of `settings.toml`.
    pub fn with_fonts(mut self, fonts: &Fonts) -> Self {
        self.ui_font_family = fonts.ui_family.clone();
        self.terminal_font_family = fonts.terminal_family.clone();
        self.terminal_font_size = fonts.terminal_size;
        self
    }
```

Note on `default_settings_give_the_spec_fonts`: it holds only because `pick_family(.., &[])` returns `TERMINAL_FONTS[0]` on both sides.

- [ ] **Step 4: Mechanical call-site changes**

Use `xd://ast_edit` or careful search and replace, then check that it compiles:
1. `px(font_size::X)` → `font_size::X` and `px(theme::font_size::X)` → `theme::font_size::X` in `crates/mkrew-ui/src` (196 sites: 193 `.text_size(..)`, 3 `.h(..)`). `.text_size(..)` takes `Into<AbsoluteLength>` and `.h(..)` takes `Into<Length>`. `Rems` implements both (gpui-pre 0.3.7 geometry.rs:3333, 3773).
2. `.font_family(theme::UI_FONT)` → `.font_family(theme.ui_font_family.clone())` (36 sites in main.rs, 2 in view.rs). Where no `theme` is in scope, the compiler says so: pass `theme` down, as the neighbouring functions do.
3. `theme.terminal_font_family` reads in main.rs (7353, 9873, 10699, 10727, 10879, 11537, 12693) and preview.rs (231, 257, 297, 378, 496): add `.clone()` where the compiler asks.
4. view.rs:852: `font(theme.terminal_font_family.clone())`.

- [ ] **Step 5: Publish and apply the global**

`main.rs:130`: replace `theme::init_terminal_font(&cx.text_system().all_font_names());` with:

```rust
        // A broken file: the defaults until it is fixed (the daemon reports it, §20.0).
        let settings = mkrew_core::settings::load(&paths.settings_file()).unwrap_or_default();
        cx.set_global(theme::Fonts::from_settings(
            &settings,
            &cx.text_system().all_font_names(),
        ));
```

`Shell::render` (l.16145):

```rust
        let fonts = cx.global::<theme::Fonts>().clone();
        window.set_rem_size(px(fonts.ui_size));
        let theme = Theme::for_appearance(dark).with_fonts(&fonts);
```

`terminal/view.rs`: change `theme_for` to take the app:

```rust
fn theme_for(appearance: WindowAppearance, cx: &App) -> Theme {
    Theme::for_appearance(matches!(
        appearance,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    ))
    .with_fonts(cx.global::<theme::Fonts>())
}
```

Update its two callers (l.127 and l.580) to `theme_for(window.appearance(), cx)`. The existing `theme != self.theme` check then clears the row cache, and prepaint re-fits the PTY.

Side effect to check by eye in Step 10: GPUI's default text size is `rems(1.)`. Text that sets no size moves from 16 px to 13 px.

- [ ] **Step 6: Live reload on the status tick**

Add a `Shell` field next to `settings_file` (l.220): `settings_modified: Option<std::time::SystemTime>,`. Initialise it at l.551 with:

```rust
std::fs::metadata(&settings_file).and_then(|m| m.modified()).ok()
```

Inside the `STATUS_TICK` loop's `update_in` closure (l.388), before the redraw check:

```rust
                    // ponytail: mtime poll on the 500 ms tick; a file watcher if it ever shows
                    // in a profile.
                    let modified = std::fs::metadata(&shell.settings_file)
                        .and_then(|m| m.modified())
                        .ok();
                    if modified != shell.settings_modified {
                        shell.settings_modified = modified;
                        let installed = cx.text_system().all_font_names();
                        let last = cx.global::<theme::Fonts>().clone();
                        if let Some(fonts) = theme::reload_fonts(&shell.settings_file, &installed, &last) {
                            cx.set_global(fonts);
                            cx.refresh_windows();
                        }
                    }
```

- [ ] **Step 7: Palette `Global: Open settings`**

`keymap.rs` `ensure_file`: add a `contents: &str` parameter and write it instead of `template()`. Update the doc comment ("Writes `contents`…"). Callers: main.rs:3771 passes `&keymap::template()`, and the tests pass `&template()`.

`palette.rs`:
- Add `OpenSettings` to `Extra`, at the end of `ALL` (`[Self; 15]`).
- Title: `Self::OpenSettings => "Global: Open settings",`.
- Availability: `Self::OpenSettings => true,`.

`main.rs` palette handler (l.3433): add a match arm before the generic `palette::Action::Extra(extra)` arm, because that arm returns early without a workspace:

```rust
            palette::Action::Extra(palette::Extra::OpenSettings) => self.open_settings(cx),
```

Add the method next to `open_keymap_settings`:

```rust
    /// Palette "Global: Open settings": writes the commented `settings.toml` template if the
    /// file is missing and opens it in Zed; fonts apply on save (ADR 0024).
    fn open_settings(&mut self, cx: &mut Context<'_, Self>) {
        let opened = keymap::ensure_file(&self.settings_file, mkrew_core::settings::template())
            .map_err(|error| format!("Cannot create settings.toml: {error}"))
            .and_then(|()| zed::open(&self.settings_file, None).map_err(|error| error.to_string()));
        match opened {
            Ok(()) => self.sidebar_error = None,
            Err(error) => {
                tracing::warn!(path = %self.settings_file.display(), "open settings failed: {error}");
                self.sidebar_error = Some(error);
            }
        }
        cx.notify();
    }
```

- [ ] **Step 8: Run the tests and confirm they pass**

Run: `cargo test -p mkrew-ui --lib -j 4`
Expected: PASS.

- [ ] **Step 9: ADR 0024**

Create `docs/decisions/0024-font-settings.md`:
- Context: the inbox request. Sizes were constants; the terminal family was fixed at startup.
- Options:
  1. rems + `set_rem_size`
  2. `Theme` size fields × scale
  3. terminal only
- Decision: option 1. GPUI's native mechanism, mechanical edits, and the existing theme-change path resizes the PTY.
- Reload: mtime poll on the 500 ms tick, not a watcher crate and not a protocol event.
- Consequences:
  - The terminal size also drives the Files/Git code views.
  - Docs preview sizes (ADR 0021) stay fixed.
  - Spacing stays in px.
  - Default-size text moves 16 → 13 px.

- [ ] **Step 10: Verify**

Run the full verification. Then do a smoke run:
1. Start the app.
2. Palette → `Global: Open settings`. Zed opens a commented file.
3. Uncomment `[terminal] font_size = 13` and set it to 18. Within ~1 s the terminal glyphs grow and an agent redraws to fewer columns.
4. Set `[ui] font_size = 15`. The sidebar and board text grow.
5. Set `font_size = 4`. Nothing changes, and the palette shows the settings error.
6. Restore the values.

Tick the font inbox line `[x]`.
Suggested message: `feat(ui): font family and size settings for the app and terminal, applied live (ADR 0024)`

---

### Task 6: View rail on the right; Run inline with History / + Agent (ADR 0023)

Inbox: `I want more view on the main content … top bar … right bar. The Run button can move inline with the History, + Agent bar`. Spec: §10, §13, §14.

**Current state:**
- `Shell::render_view_bar` (main.rs:7556-7606) is the first child of `render_main` (l.7532), about 37 px tall.
- It holds the 5 view tabs, a spacer, `+ New task` (Board with tasks) and `render_run_button` (l.7610).
- The History / + Agent row is `render_tab_bar` (l.12938). It only shows in Terminals.
- No right panel exists. The root is `render_sidebar` + `render_main` (l.16179-16180).
- View switching through `Command::Show*` (keymap / palette) does not touch the bar.
- SPEC §10 l.655 draws the tabs on top. l.693 says Run is "at the right end of the view bar, in every view".

**Change (defaults from the open questions):**
- `render_view_bar` becomes `render_view_rail`: a `VIEW_RAIL_WIDTH` column right of `render_main`. It holds the 5 view labels at META size, then a spacer, then a compact Run (`▶` / `●` / `✕` + `▾`) in every view except Terminals.
- In Terminals, the full Run button sits in `render_tab_bar` right before History.
- `+ New task` on a non-empty board becomes a quiet button in the Todo column header.
- About 37 px of height is regained in every view, for about 76 px of width.

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (`VIEW_RAIL_WIDTH`)
- Modify: `crates/mkrew-ui/src/board.rs` (`View::command` + test)
- Modify: `crates/mkrew-ui/src/main.rs`:
  - `render_main` 7526-7532
  - `render_view_bar` 7555-7606
  - `render_run_button` 7610-7676
  - `render_tab_bar` 13052
  - `board_column` 15069-15103 and its caller in `render_board`
  - root 16179-16180
- Create: `docs/decisions/0023-view-rail.md`
- Modify: `docs/SPEC.md` §10 (diagram l.653-664, Board l.674, Run l.693), `docs/HUMAN_CHECKS.md:471` wording

**Interfaces:**
- Consumes: `theme::font_size::*` as `Rems` and `theme.ui_font_family` (Task 5).
- Produces:
  - `board::View::command(self) -> keymap::Command`
  - `fn render_view_rail(&self, shown: View, theme: &Theme, cx) -> gpui::Div`
  - `fn render_run_button(&self, project_id, compact: bool, theme, cx) -> AnyElement`
  - `fn board_column(column, cards, now, new_task: Option<AnyElement>, theme, cx) -> AnyElement`

- [ ] **Step 1: Write the failing test**

In `crates/mkrew-ui/src/board.rs` `mod tests`:

```rust
    #[test]
    fn each_view_maps_to_its_own_show_command() {
        use crate::keymap::Command;
        let commands: Vec<Command> = View::ALL.into_iter().map(View::command).collect();
        assert_eq!(
            commands,
            [
                Command::ShowBoard,
                Command::ShowTerminals,
                Command::ShowDocs,
                Command::ShowFiles,
                Command::ShowGit
            ]
        );
    }
```

- [ ] **Step 2: Run it and confirm it fails**

Run: `cargo test -p mkrew-ui --lib board -j 4`
Expected: compile error `no function command`.

- [ ] **Step 3: `View::command`**

In `board.rs` `impl View` (next to `title`, l.22-50):

```rust
    /// The keymap command that shows this view (its rail tooltip shows the keys).
    pub fn command(self) -> crate::keymap::Command {
        use crate::keymap::Command;
        match self {
            View::Board => Command::ShowBoard,
            View::Terminals => Command::ShowTerminals,
            View::Docs => Command::ShowDocs,
            View::Files => Command::ShowFiles,
            View::Git => Command::ShowGit,
        }
    }
```

Run `cargo test -p mkrew-ui --lib board -j 4`. Expected: PASS.

- [ ] **Step 4: The rail**

`theme.rs`, next to `SIDEBAR_WIDTH`:

```rust
/// Width of the view rail right of the main area (view switcher + compact Run, ADR 0023).
pub const VIEW_RAIL_WIDTH: f32 = 76.0;
```

`main.rs`:
- Delete `.child(self.render_view_bar(view, theme, cx))` from `render_main` (l.7532).
- Replace `render_view_bar` (l.7555-7606) with:

```rust
    /// The workspace's view switcher (SPEC §10, ADR 0023): a column right of the main area
    /// with Board / Terminals / Docs / Files / Git, and a compact Run at its foot outside
    /// Terminals (where the full Run sits in the tab bar).
    fn render_view_rail(&self, shown: View, theme: &Theme, cx: &mut Context<'_, Self>) -> gpui::Div {
        let hover = theme.surface_raised;
        div()
            .flex_none()
            .w(px(theme::VIEW_RAIL_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .p(px(space::XS))
            .border_l(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .font_family(theme.ui_font_family.clone())
            .children(View::ALL.into_iter().map(|view| {
                let active = view == shown;
                div()
                    .id(SharedString::from(format!("view-{}", view.title())))
                    .px(px(space::S))
                    .py(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .cursor_pointer()
                    .text_size(font_size::META)
                    .when(active, |el| {
                        el.bg(rgb(theme.surface_raised))
                            .font_weight(FontWeight(weight::MEDIUM))
                            .text_color(rgb(theme.text))
                    })
                    .when(!active, |el| {
                        el.text_color(rgb(theme.text_muted))
                            .hover(move |style| style.bg(rgb(hover)))
                            .active(|style| style.opacity(opacity::ACTIVE))
                    })
                    .tooltip(tooltip(theme, view.title(), self.keymap.label(view.command())))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.show_view(view, window, cx);
                    }))
                    .child(view.title())
            }))
            .when_some(self.workspace, |el, project_id| {
                el.child(div().flex_1()).when(shown != View::Terminals, |el| {
                    el.child(self.render_run_button(project_id, true, theme, cx))
                })
            })
    }
```

- Root `Shell::render` (l.16179-16180): after `.child(self.render_main(now, &theme, window, cx))` add:

```rust
            .when(
                self.workspace.is_some() && self.connection.is_some(),
                |el| el.child(self.render_view_rail(self.view(), &theme, cx)),
            )
```

This mirrors `render_main`'s own guard at l.7518, so the rail never shows next to the "Mkrew" status message.

- If "Terminals" does not fit 76 px at META size in the smoke run (Step 8), raise `VIEW_RAIL_WIDTH` to the measured width. Do not shrink the text.

- [ ] **Step 5: Compact Run and Run in the tab bar**

In `render_run_button`:
- Add a `compact: bool` parameter after `project_id`.
- Change the main button label and tooltip:

```rust
        // The rail is narrow: only the state icon; the tooltip carries the full label.
        let label = if compact {
            summary.label().chars().next().map(String::from).unwrap_or_default()
        } else {
            summary.label()
        };
        let tip = match (summary.rows.is_empty(), compact) {
            (true, _) => "Set up the commands to run".to_owned(),
            (false, true) => format!("{}\n{}", summary.label(), summary.tooltip()),
            (false, false) => summary.tooltip(),
        };
        let main = button("run-main", label, theme, !live)
```

- Change `if live {` (the ↻ ■ loop) to `if live && !compact {`, so the rail shows only the icon + `▾` and Restart/Stop stay in the ▾ menu.
- Add `.when(compact, |el| el.flex_wrap())` to the `bar` div, so `▶ ▾` wraps instead of overflowing the rail.

`summary.label()` always starts with `▶`, `●` or `✕` (run.rs:132-137), so the first char is the state icon.

In `render_tab_bar`, right before the History button (l.13052), add:

```rust
            .child(self.render_run_button(project_id, false, theme, cx))
```

- [ ] **Step 6: New task in the Todo column header**

`board_column` (l.15069) is a free function with no access to `Shell`. Add a `new_task: Option<gpui::AnyElement>` parameter after `now`. In its header row (l.15085-15102), after the count child, add:

```rust
                .children(new_task.map(|button| div().ml_auto().child(button))),
```

In `render_board`, the call is `.map(|column| board_column(column, &cards, now, theme, cx))` (main.rs:8133). Pass:
- `Some(self.new_task_button(project_id, false, theme, cx).into_any_element())` for the Todo column (compare `column.status` with the Todo variant of the status enum `board::Column.status` uses; read it in `board.rs`);
- `None` for the others.

The old `+ New task` in the view bar is already gone (Step 4).

- [ ] **Step 7: ADR 0023 and SPEC**

Create `docs/decisions/0023-view-rail.md`:
- Context: the user wants more main-area height (inbox 2026-10-08).
- Options:
  - A: a right rail
  - B: views in the left sidebar
  - C: a compact strip in the tab bar
- Decision: A. About 37 px is regained in every view; shortcuts and the palette are unchanged.
- Consequences: Run is full-size in the Terminals tab bar and compact at the rail foot elsewhere; New task moves to the Todo header; the rail costs about 76 px of width (about 9 terminal columns).

`docs/SPEC.md` §10:
- Replace the diagram (l.653-664) with:

```
┌──────────────┬──────────────────────────────────────┬───────────┐
│ WORKSPACES   │  Todo │ Running │ Needs input │ …    │ Board     │
│ ▾ api      2 │                                      │ Terminals │
│   ● claude   │  …or the terminal tabs/panes         │ Docs      │
│   ? codex    │    (tab bar: … ▶ Run · History ·     │ Files     │
│ ▸ web     ?  │     + Agent · + Shell)               │ Git       │
│ ▸ infra      │  …or the Markdown review view        │           │
│ INBOX (2)    │                                      │           │
│ web: approve?│                                      │ ▶ ▾       │
└──────────────┴──────────────────────────────────────┴───────────┘
```

- Add a bullet after "Views per workspace": "**View rail** (ADR 0023): the views are a column right of the main area; the active one is highlighted, tooltips show their shortcuts (§13)."
- l.693: replace "at the right end of the view bar, in every view:" with "in the Terminals tab bar before History, and as a compact `[▶│▾]` / `[●│▾]` / `[✕│▾]` (full label in the tooltip, ↻ ■ in the menu) at the foot of the view rail in the other views:".
- Board bullet (l.674): add "New task: the empty board's button, or `New task` in the Todo column header."

`docs/HUMAN_CHECKS.md:471`: change "tab in the view bar" to "entry in the view rail".

- [ ] **Step 8: Verify**

Run the full verification. Then do a smoke run:
- The main area starts at the top in every view.
- The rail switches views, and its tooltips show Ctrl+Shift+1..5.
- In Terminals, `▶ Run` sits before History, and ▾ opens the menu.
- In Board/Docs/Files/Git, the compact Run is at the rail foot and ▾ opens the menu.
- `New task` shows in the Todo header.

Take light and dark screenshots into `docs/screenshots/` (SPEC §14 design check, `mkrew-ui-design` skill). Tick the view-bar inbox line `[x]`.
Suggested message: `feat(ui): view rail on the right; Run inline with History and + Agent (ADR 0023)`

---

### Task 7: Manual checks and notes

**Files:**
- Modify: `docs/HUMAN_CHECKS.md` (new section under "To verify")
- Modify: `docs/PROGRESS.md` ("Notes for the next session": one line)

- [ ] **Step 1: HUMAN_CHECKS**

Add `### To verify — 2026-10-08 inbox: keys, fonts, view rail (plan docs/superpowers/plans/2026-10-08-inbox-keys-fonts-view-rail.md)` with these checks:

```markdown
- [ ] Git commit box: type two lines, Ctrl+A highlights them, Backspace empties the box; Ctrl+A then
  typing or Ctrl+V replaces the text. Same in the composer and the card follow-up.
- [ ] Terminal: drag-select text, Ctrl+C → it is on the clipboard (paste into Zed). Without a
  selection Ctrl+C still stops `ping -t 127.0.0.1` in a shell pane. Select, type a letter, then
  Ctrl+C → it interrupts (the selection was cleared).
- [ ] In omp and in Claude Code: type `hello world`, Ctrl+Backspace deletes `world` only. In a
  pwsh pane Ctrl+Backspace also deletes one word [INFERENCE: PSReadLine maps ESC DEL to
  BackwardKillWord]; note it if it does not.
- [ ] Ctrl+A in omp moves to the line start (by design, terminals send ^A); Ctrl+U clears the input.
- [ ] Palette → `Global: Open settings` opens a commented settings.toml in Zed. `[terminal]
  font_size = 18` → glyphs grow within ~1 s, a running agent redraws to fewer columns.
  `[ui] font_size = 15` → sidebar/board text grows. `font_family = "Consolas"` changes the
  terminal; a family that is not installed changes nothing. `font_size = 4` → nothing changes,
  the palette shows the error.
- [ ] Views are a rail on the right; the main area starts at the top. Tooltips show
  Ctrl+Shift+1..5. In Terminals `▶ Run` sits before History; in other views the compact Run at
  the rail foot opens its ▾ menu. `New task` is in the Todo column header. Light + dark.
```

- [ ] **Step 2: PROGRESS note**

Append to "Notes for the next session":

```markdown
- 2026-10-08 inbox (plan docs/superpowers/plans/2026-10-08-inbox-keys-fonts-view-rail.md): `DialogKey::SelectAll` + `TextBox` select-all, terminal Ctrl+C copies a selection (ADR 0022), Ctrl+Backspace = ESC DEL, `[ui]`/`[terminal]` font settings with rems + `theme::Fonts` global (ADR 0024), view rail (ADR 0023); manual checks in HUMAN_CHECKS. Outside the milestone loop.
```

Suggested message: `docs: manual checks and notes for the 2026-10-08 inbox`

---

## Open questions (defaults used above)

| # | Question | Options | Default in this plan |
| --- | --- | --- | --- |
| 1 | Ctrl+A in single-line fields (palette, quick open, find, comments, peek, broadcast)? | a) only multi-line `TextBox` fields · b) all fields (about 10 files, a flag per model) · c) a shared `LineBox` model first | **a** |
| 2 | Terminal Ctrl+C | a) copies when text is selected, else ^C · b) keep Ctrl+Shift+C only · c) copy on mouse-up | **a** |
| 3 | Ctrl+Backspace bytes | a) `ESC DEL` · b) `^W` · c) keep `^H` + set `WT_SESSION` | **a** |
| 4 | Ctrl+A in the terminal | a) keep `^A` (line start) · b) add a palette-only "Terminal: Select all" over the scrollback | **a** |
| 5 | How UI text scales | a) rems + `set_rem_size` · b) `Theme` size fields × scale · c) terminal only | **a** |
| 6 | Font zoom keys (Ctrl+= / Ctrl+-) | a) none, edit the file (applies live) · b) session-only zoom · c) saved zoom (needs `toml_edit`) | **a** |
| 7 | View switcher placement | A) right rail · B) left sidebar · C) compact strip in the tab bar | **A** |
| 8 | Run outside Terminals | a) compact at the rail foot · b) Terminals only + palette · c) always compact in the rail | **a** |
| 9 | `New task` on a non-empty board | a) Todo column header · b) removed (Ctrl+Shift+N / palette remain) · c) in the rail | **a** |
