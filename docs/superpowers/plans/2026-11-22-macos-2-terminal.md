# macOS Terminal Input Implementation Plan (Plan B)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The terminal pane behaves like a Mac terminal: Cmd+C / Cmd+V, Option keys, dead keys and IME, double / triple click selection, trackpad scrolling in full-screen programs, and a cell grid on whole device pixels.

**Architecture:** Every macOS rule is a runtime branch on `crate::keymap::Os` (Plan A, Task 2) or on a new `KeyEnv { os, option_as_meta }` GPUI global, inside pure functions that the Windows dev machine and CI unit-test.
No `#[cfg(target_os = "macos")]` block is needed anywhere in this plan, so every line is compiled and tested on Windows, Linux and by the macOS CI job (Plan A, Task 1).
`[terminal] option_as_meta` lives in `mkrew-core` settings, shows as a row of the Settings dialog from `settings_fields::SETTINGS`, and reaches every pane through the `KeyEnv` global that `apply_settings_file` refreshes on each settings change.

**Tech Stack:** Rust, GPUI (`gpui-pre` 0.3.7), `alacritty_terminal` 0.26, serde + toml settings.

**Spec:** `docs/SPEC.md` §10 (terminal rendering), §13 (keyboard), §14 (fonts), §20.0 (settings), and the audit reports `agent://MacKeyboard` (findings 1, 2, 8, 13, 16, 17) and `agent://MacTextAndMouse` (findings 7, 8, 10, 11, 12, 14, 16, 19, 21, 22), summarised in `local://mac-plan-context.md`.
ADR 0099 (`docs/decisions/0099-macos-terminal-input.md`) is written by Task 1.

**Prerequisite:** Plan A Task 2 has merged: `crates/mkrew-ui/src/keymap.rs` has `pub enum Os { Windows, Mac, Linux }` and `Os::CURRENT` (const).
Plan A also rebinds Copy / Paste to `cmd-c` / `cmd-v` on macOS.
This plan does not touch `keymap.rs` defaults.

## Global Constraints

- SPEC (`docs/SPEC.md`) is the source of truth: a behavior change updates SPEC and the ADR in the same commit as the code.
- Every change ships with tests. No test calls a real agent or the network.
- No em dash in any written text, use "-". In Markdown and doc text, each sentence on its own line.
- UI values come from `crates/mkrew-ui/src/theme.rs`. This plan adds no new visual element.
- Conventional Commits. Commit with `git -c user.email=tungnv975@gmail.com commit`. Stage only your own files (never `git add -A`). Never push.
- No `unwrap` / `expect` outside tests unless provably infallible (comment why).
- Windows native stays first-class: Windows and Linux terminal behavior does not change, with four deliberate exceptions that apply on every OS: double-click selects a word, triple-click a line, a trackpad's pixel scrolling is gentler for programs, and the grid cell is whole device pixels.
- Verify: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (fmt --check, clippy -D warnings, tests). Fast loop: `cargo test -p mkrew-ui --lib <filter>`, `cargo test -p mkrew-core <filter>`.
- Run `cargo fmt --all` before every commit.
- Line numbers below are from master on 2026-11-22 for files this plan owns (`terminal/**`, `ime.rs`, `settings*.rs`).
  Find code by function name if a number has moved.

## Review Focus

- **Option+key on macOS with `option_as_meta` off sends nothing itself.**
  The key press must reach the input method (dead key Option+E then E gives "é", Option+2 gives "€"), never `ESC` plus a composed character.
  Pinned by Task 2 (`mac_option_composes_by_default`) and Task 3 (`option_dead_key_goes_to_the_input_method_which_types_the_accent`).
- **Ctrl+C with a selection on macOS is ^C, and Ctrl+V is ^V.**
  A user who selected text and presses Ctrl+C expects an interrupt, never a copy.
  Pinned by Task 2 (`mac_ctrl_c_and_ctrl_v_are_never_copy_or_paste`).
- **A change of scroll direction in the middle of a swipe.**
  The left-over fraction of the old direction must not delay or swallow the first line of the new one.
  Pinned by Task 5 (`a_change_of_direction_drops_the_carried_fraction`).
- **Double-click on a bracket, on a blank cell, and a triple-click copy.**
  A bracket selects through its partner, a blank cell copies nothing, and a copied line never ends with a line break (pasted into a shell it would run).
  Pinned by Task 4.
- **An IME range outside, inside, or across non-BMP chars of the composing text.**
  A range past the end is cut to it, a range in the middle retypes what followed it, and ranges count UTF-16 units (an emoji is two).
  Pinned by Task 3.

---

## File Structure

| File | Change |
| --- | --- |
| `docs/decisions/0099-macos-terminal-input.md` | New ADR (Task 1). |
| `crates/mkrew-core/src/settings.rs` | `FontSettings` becomes `TerminalSettings` with `option_as_meta`; template; tests (Task 1). |
| `crates/mkrew-core/src/settings_fields.rs` | One `SETTINGS` row; one test line (Task 1). |
| `crates/mkrew-ui/src/settings_dialog.rs` | One test (Task 1). |
| `crates/mkrew-ui/src/terminal/input.rs` | `KeyEnv`, OS-aware copy / paste keys, Option handling, `wheel_lines` (Tasks 2, 5). |
| `crates/mkrew-ui/src/terminal/view.rs` | Uses `KeyEnv`, `Pick`, `wheel_lines`, `grid_cell_size`; IME key reset (Tasks 2 to 6). |
| `crates/mkrew-ui/src/terminal/emulator.rs` | `Pick`, word / line selection, `selection_text` without trailing line break (Task 4). |
| `crates/mkrew-ui/src/terminal/layout.rs` | `snap_to_device` (Task 6); two test call sites (Task 4). |
| `crates/mkrew-ui/src/ime.rs` | `Marked`, ranges, `text_for_range`, IME-first printable keys (Task 3). |
| `crates/mkrew-ui/src/shell/voice_input.rs` | Follows the new `ime::keys` signature (Task 3). |
| `crates/mkrew-ui/src/shell/connection.rs` | `apply_settings_file` refreshes `KeyEnv` (Task 2). |
| `crates/mkrew-ui/src/main.rs` | One line at start: set the `KeyEnv` global (Task 2). Plan C also edits this file. |
| `crates/mkrew-ui/src/keymap.rs` | Test module only: one call to `key_to_bytes` (Task 2). Plan A owns this file. |
| `docs/SPEC.md`, `docs/HUMAN_CHECKS.md` | Each task updates the paragraph it changes. |

---

### Task 1: ADR 0099 and the `[terminal] option_as_meta` setting

**Files:**
- Create: `docs/decisions/0099-macos-terminal-input.md`
- Modify: `crates/mkrew-core/src/settings.rs` (lines 23-24, 223-238, 430-452, 626-630, tests at 784-790, 838-840, 884-887)
- Modify: `crates/mkrew-core/src/settings_fields.rs` (after the `board_all_workspaces` row, test `values_show_as_their_toml_names`)
- Modify: `crates/mkrew-ui/src/settings_dialog.rs` (test module, after `space_toggles_a_bool_and_enter_cycles_a_choice`)
- Modify: `docs/SPEC.md` (§20.0 `[terminal]` block, near line 2088)
- Modify: `docs/HUMAN_CHECKS.md` (new entry above `### To verify - 2026-11-14 Git commit box footer on two rows`)

**Interfaces:**
- Consumes: nothing.
- Produces: `mkrew_core::settings::TerminalSettings { font_family: String, font_size: FontSize, option_as_meta: bool }` (replaces `FontSettings`), field `Settings.terminal: TerminalSettings`, const `mkrew_core::settings::DEFAULT_FONT_SIZE: FontSize`.
  Setting id `terminal.option_as_meta` (Kind `Bool`, page `General`).
  Task 2 reads `settings.terminal.option_as_meta`.

`FontSettings` is used only by `Settings.terminal` and by `UiSettings::default` (to copy the default family and size), so renaming it touches nothing outside `settings.rs`.

- [ ] **Step 1: Write the failing tests**

In `crates/mkrew-core/src/settings.rs`, in `mod tests`, replace the `settings.terminal` assertion in `missing_file_is_defaults` (lines 784-790):

```rust
        assert_eq!(
            settings.terminal,
            TerminalSettings {
                font_family: String::new(),
                font_size: FontSize(13),
                option_as_meta: false,
            }
        );
```

In `full_file_parses`, add the key to the source (lines 838-840):

```rust
[terminal]
font_family = "JetBrains Mono"
font_size = 15
option_as_meta = true
```

and to the expected value (lines 884-887):

```rust
                terminal: TerminalSettings {
                    font_family: "JetBrains Mono".into(),
                    font_size: FontSize(15),
                    option_as_meta: true,
                },
```

Add a new test after `full_file_parses`:

```rust
    #[test]
    fn option_as_meta_defaults_off_is_a_bool_and_belongs_to_terminal_only() {
        assert!(!Settings::default().terminal.option_as_meta);
        let on = parse(
            Path::new("settings.toml"),
            "[terminal]\noption_as_meta = true\n",
        )
        .unwrap();
        assert!(on.terminal.option_as_meta);
        let message = parse_err("[terminal]\noption_as_meta = \"yes\"\n");
        assert!(message.starts_with("settings.toml:2:"), "{message}");
        assert!(message.contains("invalid type"), "{message}");
        assert!(
            parse_err("[ui]\noption_as_meta = true\n").contains("unknown field `option_as_meta`")
        );
    }
```

In `crates/mkrew-core/src/settings_fields.rs`, in `values_show_as_their_toml_names`, add:

```rust
        assert_eq!(setting("terminal.option_as_meta").current(&defaults), "false");
```

In `crates/mkrew-ui/src/settings_dialog.rs`, add after `space_toggles_a_bool_and_enter_cycles_a_choice`:

```rust
    #[test]
    fn option_as_meta_is_a_switch_on_the_general_page() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        focus_on(&mut d, "terminal.option_as_meta", &keymap);
        assert_eq!(
            d.key(&typed(" "), None, &keymap),
            Effect::Write {
                setting: setting("terminal.option_as_meta"),
                text: "true".into()
            }
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mkrew-core settings`
Expected: compile error `cannot find struct, variant or union type TerminalSettings in this scope`.

- [ ] **Step 3: Implement the setting in `mkrew-core`**

In `crates/mkrew-core/src/settings.rs`, lines 23-24:

```rust
    /// `[terminal]`: the terminal font (also used by the Files and Git code views) and its keys.
    pub terminal: TerminalSettings,
```

Replace `FontSettings` and its `Default` (lines 223-238):

```rust
/// `[ui] font_size` and `[terminal] font_size` of a new install (SPEC §14).
pub const DEFAULT_FONT_SIZE: FontSize = FontSize(13);

/// `[terminal]`: the code font (an empty family is the built-in default, SPEC §14) and keys.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TerminalSettings {
    pub font_family: String,
    pub font_size: FontSize,
    /// macOS (ADR 0099): Option sends ESC plus the key (Meta) instead of typing the layout's
    /// character. No effect on other systems.
    pub option_as_meta: bool,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            font_family: String::new(),
            font_size: DEFAULT_FONT_SIZE,
            option_as_meta: false,
        }
    }
}
```

In `impl Default for UiSettings` (lines 430-436) drop the `FontSettings` copy:

```rust
impl Default for UiSettings {
    fn default() -> Self {
        Self {
            font_family: String::new(),
            font_size: DEFAULT_FONT_SIZE,
            theme: ThemeMode::System,
```

(the rest of the struct literal is unchanged, and the `let font = FontSettings::default();` line is deleted).

In `template()`, replace lines 626-630:

```text
     # Terminal, Files and Git code font: pick a monospace family (empty: Cascadia Code /\n\
     # SF Mono / JetBrains Mono, the first installed); size 8–32 px.\n\
     # Option as Meta (macOS): true makes Option send Esc plus the key, so Option+B / Option+F\n\
     # move by word in a shell; false types the layout's characters (é, @, €).\n\
     [terminal]\n\
     # font_family = \"\"\n\
     # font_size = 13\n\
     # option_as_meta = false\n\
```

(keep the following `\n\` blank line and the `[voice]` block as they are).

- [ ] **Step 4: Add the Settings dialog row**

In `crates/mkrew-core/src/settings_fields.rs`, in `SETTINGS`, directly after the `board_all_workspaces` row (before the `Appearance, "ui", "theme"` row):

```rust
    row(
        General,
        "terminal",
        "option_as_meta",
        Bool,
        "Option as Meta",
        "macOS: Option sends Esc + the key (Option+B / F move by word in a shell) instead of typing é, @ or €.",
    ),
```

The dialog builds its rows from `SETTINGS`, `Kind::Bool` already toggles on Space / Enter, and `every_template_key_has_exactly_one_setting` keeps the template and the table in step, so no UI code changes.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p mkrew-core settings`
Expected: PASS (including `every_template_key_has_exactly_one_setting`, `writing_each_default_reads_it_back_and_keeps_comments`, `option_as_meta_defaults_off_is_a_bool_and_belongs_to_terminal_only`).

Run: `cargo test -p mkrew-ui --lib settings_dialog`
Expected: PASS (including `option_as_meta_is_a_switch_on_the_general_page`).

Run: `cargo clippy -p mkrew-core -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings. (`theme.rs` reads only `settings.terminal.font_family` and `font_size`, so it compiles unchanged.)

- [ ] **Step 6: Write the ADR, SPEC and HUMAN_CHECKS**

Create `docs/decisions/0099-macos-terminal-input.md`:

```markdown
# 0099 - macOS terminal input, selection, scroll and IME

- Date: 2026-11-22
- Status: accepted
- Milestone: macOS audit 2026-11-22

## Context

An audit of the terminal pane on macOS found these problems.
It was done by reading the code and GPUI's macOS backend, not by running Mkrew on a Mac.

Cmd+C and Cmd+V did nothing in a terminal.
Plain Ctrl+C copied and plain Ctrl+V pasted (ADR 0022, ADR 0026).
That is a Windows Terminal habit: on a Mac Ctrl+C is the interrupt key and Ctrl+V is "quoted insert".

GPUI fills `key_char` with the Option-composed character (Option+B is "∫").
Option+key therefore sent `ESC ∫`, which is neither Meta nor the character.

GPUI gives a key that has a `key_char` to the view first, and to the input method only when the view leaves it alone.
The terminal sent the accent of a dead key at once, so Option+E then E never composed "é".

A double click or a triple click selected no more than a single click.

A trackpad sends pixel deltas with momentum.
One swipe sent dozens of arrow keys to `less` or an agent TUI.
The part of a line left over from the old direction delayed the first line after a change of direction.

The input handler ignored the range of `replace_text_in_range` and returned nothing from `text_for_range`.
With an IME input source active the first printable key reached the program as ASCII, because the handler never asked for printable keys.

The cell size is 13 px x 1.75 = 22.75 px.
GPUI snaps every quad to device pixels on its own (`Window::paint_quad`), so at 2x a row is 45 or 46 device pixels high depending on its index.

## Options

Option key:

1. Option always composes (the Terminal.app and iTerm2 default), no setting.
2. Option is always Meta, no setting.
3. `[terminal] option_as_meta`, default off.

Copy and paste:

1. On macOS Cmd+C / Cmd+V are the keymap's Copy and Paste commands, and plain Ctrl+C / Ctrl+V are never copy and paste.
2. Keep plain Ctrl+C / Ctrl+V everywhere.

Dead keys:

1. Let Option keys that compose go to the input method (what Zed's terminal does).
2. Detect dead keys in the terminal. GPUI gives no signal for them.

Trackpad:

1. Scale pixel deltas that go to a program, and drop the carried fraction when the direction changes.
2. Cap the lines per event. This needs a second constant and still lets a slow swipe run away.

## Decision

Option key 3, copy and paste 1, dead keys 1, trackpad 1.

With `option_as_meta` off (the default), a macOS Option+key press sends nothing from the key handler.
GPUI then hands the key to the input method, which types the composed character through `ime::Typing`.
With it on, Option+key sends `ESC` plus the key without the composition (Shift on a letter gives the capital).
Keys that are not text (arrows, Backspace, Enter, function keys) keep their xterm encoding with Option as the Alt modifier.
The setting has no effect on Windows and Linux.

`terminal::input::KeyEnv { os, option_as_meta }` carries both facts.
It is a GPUI global set at start and by `apply_settings_file`, so a change in `settings.toml` or the Settings dialog applies to open panes at once.

On macOS `is_copy_key` and `is_paste_key` are false: Ctrl+C is always ^C and Ctrl+V is always ^V.
Cmd+C is `Command::Copy`: it copies a selection and does nothing without one, and the keystroke is spent, so it never becomes ^C.
Cmd+V is `Command::Paste`.
Other Cmd keys are not sent to the program.

A double click selects the word under the pointer (alacritty's semantic selection, so a bracket selects through its partner), a triple click selects the line.
A drag after either extends by words or lines.
`Emulator::selection_text` never ends with a line break, so a copied line pasted into a shell does not run.
This applies on every OS.

A pixel (precise) scroll delta counts `PIXEL_WHEEL_SCALE` = 0.5 lines per line of travel when it goes to a program (wheel reports or arrow keys), and 1.0 when it scrolls the history.
The carried fraction is dropped when the direction changes.
This applies on every OS.

`ime::Typing` keeps the composing text (`ime::Marked`) as its document.
`text_for_range` returns a slice of it, and `replace_text_in_range` with a range erases back to the start of the range and types the replacement and what followed it.
Ranges count UTF-16 units.
Ranges are honored on macOS only: Windows Telex was written and tested with every range meaning "all composing text", so there a range stays that.
`prefers_ime_for_printable_keys` is true, so with a CJK or other IME source active printable keys reach the input method first (only GPUI's macOS backend asks for it).

`terminal::view::grid_cell_size` rounds the cell width and the line height to whole device pixels with the window's scale factor (`layout::snap_to_device`).
The Files editor keeps `cell_size`, because its text is laid out by the font's own advance.
This applies on every OS.

## Consequences

SPEC §13 says that the ADR 0022 and ADR 0026 exceptions are for Windows and Linux, and adds the macOS rules and the Option setting.
SPEC §10 gets the selection, scroll and cell rules.
SPEC §20.0 lists `[terminal] option_as_meta`.
The Settings dialog gets a "Option as Meta" switch on the General page (shown on every OS, like "Close to tray").
The settings type `FontSettings` is renamed `TerminalSettings`.

The Mac-only parts are written as runtime branches on `Os`, so Windows CI tests them.
Nothing in this ADR could be run on a Mac: `docs/HUMAN_CHECKS.md` lists what to try.

Ctrl+Space (^@) is taken by macOS's input source switch when two or more input sources are enabled.
That is a macOS setting and is not changed here.

## Not changed

- **"SF Mono" in `TERMINAL_FONTS`.**
  `all_font_names` returns the CoreText family names of all fonts registered with the system (`create_for_all_families`), and `pick_family` skips a name that is not in it.
  So "SF Mono" is used when it is installed and skipped otherwise, and Menlo is always there.
  The entry is not dead.
- **`DIAGRAM_FONT`.**
  On macOS the list resolves to Helvetica Neue, a system font.
  `mermaid-rs-renderer` maps `-apple-system` and `system-ui` to the generic `sans-serif`, which `fontdb` resolves to Arial, not to San Francisco, so adding them would make diagrams worse.
- **Option+Left / Option+Right in a shell.**
  They keep the xterm encoding `CSI 1;3D` / `CSI 1;3C` that agent TUIs read.
  Terminal.app sends `ESC b` / `ESC f` instead, and zsh's defaults bind only those.
  Changing it is a policy choice for the user.
- **Cmd+K (clear), Cmd+A (select all), Cmd+Left / Cmd+Backspace line editing in a terminal.**
  They need new commands in the keymap (Plan A's file), palette entries and SPEC rows on every OS.
- **Unmodified dead keys on non-US layouts (a French "^", a Spanish "´").**
  GPUI gives no dead key signal, and sending every printable key through the input method would make held keys show the accent picker instead of repeating.
- **Horizontal trackpad scrolling in a terminal.**
  No program the terminal hosts uses it, and `ScrollWheelEvent.delta.x` stays ignored.
- **Middle click closes a file tab.**
  It stays for mouse users.
  A trackpad has the visible x on each tab (always on the active tab, on hover for the others), the right-click or two-finger-click menu with Close / Close others / Close to the right, and the Close file tab key.
  A Cmd+click or Option+click alias would be a second hidden gesture next to three visible ways.
- **Row heights of code and diff lists (`DIFF_FONT_SIZE` 12.5, `code_line_height`).**
  GPUI's list layout owns them, and nothing shows they alternate in height the way the terminal grid does.
  Look again after the Mac visual check.
- **The default font size 13.**
  A larger default for macOS would change SPEC §14 for one OS without a failing behavior.
- **The IME candidate window position.**
  It already opens at the terminal's cursor cell (`TerminalElement::paint` passes the cursor cell as `Typing.caret`).
  It opens a few cells right of the start of the composing text, because the text is typed in place and the cursor sits after it.
  Changing that needs the echo's timing; check on a Mac first.
```

In `docs/SPEC.md` §20.0, in the `[terminal]` block (lines 2088-2090), add a line after `font_size`:

```toml
option_as_meta = false      # macOS: true = Option sends Esc + the key (Meta); false = Option types the layout's characters (ADR 0099)
```

In `docs/HUMAN_CHECKS.md`, add this entry above `### To verify - 2026-11-14 Git commit box footer on two rows`:

```markdown
### To verify - 2026-11-22 macOS terminal input (ADR 0099, plan docs/superpowers/plans/2026-11-22-macos-2-terminal.md)

- Needs a Mac: the pure parts are unit-tested on Windows, none of this has run on macOS.
- [ ] **Setting (dark + light):** Settings, General, has a switch "Option as Meta" under "Board shows all workspaces".
  Its help text fits the row.
  Turning it on writes `option_as_meta = true` under `[terminal]` and keeps the comments.
  Restart the daemon after installing: an older daemon rejects the new key as unknown.
```

- [ ] **Step 7: Commit**

```bash
git add docs/decisions/0099-macos-terminal-input.md crates/mkrew-core/src/settings.rs crates/mkrew-core/src/settings_fields.rs crates/mkrew-ui/src/settings_dialog.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(core): [terminal] option_as_meta setting and ADR 0099"
```

---

### Task 2: Terminal keys on macOS: Cmd+C / Cmd+V, plain Ctrl, Option

**Files:**
- Modify: `crates/mkrew-ui/src/terminal/input.rs` (lines 6, 17-91, test module 259-481)
- Modify: `crates/mkrew-ui/src/terminal/view.rs` (lines 38-40, 351-386, 746-748)
- Modify: `crates/mkrew-ui/src/keymap.rs` (test module only: line 1086 and line 1348)
- Modify: `crates/mkrew-ui/src/ime.rs` (test module only: line 199 and line 255)
- Modify: `crates/mkrew-ui/src/main.rs` (one line after line 243)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (`apply_settings_file`, line 1017)
- Modify: `docs/SPEC.md` (§13, lines 1433-1436), `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: `crate::keymap::Os` (`Os::Windows | Mac | Linux`, `Os::CURRENT`) from Plan A; `settings.terminal.option_as_meta` from Task 1.
- Produces, in `crate::terminal::input`:
  - `pub struct KeyEnv { pub os: Os, pub option_as_meta: bool }` (`Clone, Copy, Debug, PartialEq, Eq`, implements `gpui::Global`), `KeyEnv::current(option_as_meta: bool) -> KeyEnv` (const fn, this computer's `Os`), `KeyEnv::of(cx: &App) -> KeyEnv` (the global, else `current(false)`).
  - `pub fn key_to_bytes(keystroke: &Keystroke, mode: InputMode, env: KeyEnv) -> Option<Vec<u8>>`
  - `pub fn is_copy_key(keystroke: &Keystroke, os: Os) -> bool`, `pub fn is_paste_key(keystroke: &Keystroke, os: Os) -> bool`
  - `pub fn key_action(keystroke: &Keystroke, selected: bool, exited: bool, mode: InputMode, env: KeyEnv) -> KeyAction`

Behavior table (Mac = `env.os == Os::Mac`):

| Key | Windows / Linux | macOS, `option_as_meta` off | macOS, on |
| --- | --- | --- | --- |
| Ctrl+C with selection | copy | `^C` | `^C` |
| Ctrl+V | paste (`^V` if the clipboard is empty) | `^V` | `^V` |
| Cmd+C / Cmd+V | not sent | not sent (the keymap's Copy / Paste handle them) | same |
| Option+B (key_char "∫" on macOS, "b" elsewhere) | Alt prefixes ESC: `ESC b` | nothing from the key handler, the input method types "∫" | `ESC b` |
| Option+Shift+B | `ESC B` | input method | `ESC B` |
| Option+Left, Option+Backspace | `CSI 1;3D`, `ESC DEL` | same | same |
| Ctrl+Option+C | `ESC ^C` | same | same |

The command `Run(Command::Copy)` is already a no-op without a selection: `TerminalPane::copy_selection` returns `false`, `run` does not call `cx.propagate()`, so the keystroke is spent and nothing reaches the PTY.
This task pins the Ctrl side of it and adds a comment at the call site.

- [ ] **Step 1: Write the failing tests**

In `crates/mkrew-ui/src/terminal/input.rs`, in `mod tests`:

Replace the imports and constants at the top (lines 261-271) with:

```rust
    use super::*;
    use gpui::Modifiers;

    const NORMAL: InputMode = InputMode {
        app_cursor: false,
        bracketed_paste: false,
    };
    const APP: InputMode = InputMode {
        app_cursor: true,
        bracketed_paste: true,
    };
    const WIN: KeyEnv = KeyEnv {
        os: Os::Windows,
        option_as_meta: false,
    };
    const MAC: KeyEnv = KeyEnv {
        os: Os::Mac,
        option_as_meta: false,
    };
    const MAC_META: KeyEnv = KeyEnv {
        os: Os::Mac,
        option_as_meta: true,
    };
```

Replace the `bytes` helper (lines 293-295):

```rust
    fn bytes(spec: &str, key_char: Option<&str>, mode: InputMode) -> Option<Vec<u8>> {
        key_to_bytes(&key(spec, key_char), mode, WIN)
    }
```

The existing tests keep their Windows meaning: append `, WIN` as the last argument of every `key_action(` call in the module (lines 322, 323, 325, 328, 330, 332, 340, 342, 343, 347), and `, Os::Windows` as the last argument of every `is_copy_key(` call (lines 307, 309, 312, 313, 314).

Add these tests at the end of the module (before the closing `}`):

```rust
    #[test]
    fn mac_ctrl_c_and_ctrl_v_are_never_copy_or_paste() {
        // ADR 0099: on a Mac Ctrl+C interrupts even over a selection, Ctrl+V is ^V.
        // Cmd+C / Cmd+V are the keymap's Copy and Paste.
        let ctrl_c = key("ctrl-c", None);
        assert!(!is_copy_key(&ctrl_c, Os::Mac));
        assert_eq!(
            key_action(&ctrl_c, true, false, NORMAL, MAC),
            KeyAction::Send(vec![0x03])
        );
        let ctrl_v = key("ctrl-v", None);
        assert!(!is_paste_key(&ctrl_v, Os::Mac));
        assert_eq!(
            key_action(&ctrl_v, false, false, NORMAL, MAC),
            KeyAction::Send(vec![0x16])
        );
        // Windows and Linux keep ADR 0022 and ADR 0026.
        for os in [Os::Windows, Os::Linux] {
            assert!(is_copy_key(&ctrl_c, os), "{os:?}");
            assert!(is_paste_key(&ctrl_v, os), "{os:?}");
        }
    }

    #[test]
    fn mac_cmd_keys_are_not_for_the_program() {
        // A bound Cmd key never gets here (the keymap spends it); an unbound one is ignored.
        for spec in ["win-c", "win-v", "win-k", "win-a"] {
            let keystroke = key(spec, None);
            assert_eq!(
                key_action(&keystroke, true, false, NORMAL, MAC),
                KeyAction::Ignore,
                "{spec}"
            );
        }
    }

    #[test]
    fn mac_option_composes_by_default() {
        // Option+B is "∫", Option+2 "€", Option+E the dead accent "´": the key press sends
        // nothing, so GPUI hands it to the input method, which types the result (ime.rs).
        for (spec, composed) in [("alt-b", "∫"), ("alt-2", "€"), ("alt-e", "´")] {
            let keystroke = key(spec, Some(composed));
            assert_eq!(key_to_bytes(&keystroke, NORMAL, MAC), None, "{spec}");
            assert_eq!(
                key_action(&keystroke, false, false, NORMAL, MAC),
                KeyAction::Ignore,
                "{spec}"
            );
        }
        // Keys that are not text keep their encoding.
        assert_eq!(
            key_to_bytes(&key("alt-left", None), NORMAL, MAC),
            Some(b"\x1b[1;3D".to_vec())
        );
        assert_eq!(
            key_to_bytes(&key("alt-backspace", None), NORMAL, MAC),
            Some(b"\x1b\x7f".to_vec())
        );
        assert_eq!(
            key_to_bytes(&key("alt-enter", None), NORMAL, MAC),
            Some(b"\x1b\r".to_vec())
        );
        // Windows is unchanged: Alt prefixes ESC.
        assert_eq!(
            key_to_bytes(&key("alt-b", Some("b")), NORMAL, WIN),
            Some(b"\x1bb".to_vec())
        );
    }

    #[test]
    fn mac_option_as_meta_sends_escape_and_the_plain_key() {
        let cases: [(&str, &str, &[u8]); 6] = [
            ("alt-b", "∫", b"\x1bb"),
            ("alt-f", "ƒ", b"\x1bf"),
            // GPUI reports Shift+letter as shift plus the lowercase letter.
            ("alt-shift-b", "ı", b"\x1bB"),
            ("alt-1", "¡", b"\x1b1"),
            // Shift on a non-letter arrives as the shifted char with shift off.
            ("alt-!", "⁄", b"\x1b!"),
            ("alt-space", "\u{a0}", b"\x1b "),
        ];
        for (spec, composed, expected) in cases {
            assert_eq!(
                key_to_bytes(&key(spec, Some(composed)), NORMAL, MAC_META),
                Some(expected.to_vec()),
                "{spec}"
            );
        }
        // Ctrl+Option keeps its C0 form.
        assert_eq!(
            key_to_bytes(&key("ctrl-alt-c", None), NORMAL, MAC_META),
            Some(vec![0x1b, 0x03])
        );
        // The setting does nothing on other systems.
        let win_meta = KeyEnv {
            os: Os::Windows,
            option_as_meta: true,
        };
        assert_eq!(
            key_to_bytes(&key("ctrl-alt-q", Some("@")), NORMAL, win_meta),
            Some(b"@".to_vec()),
            "AltGr still types"
        );
        assert_eq!(
            key_to_bytes(&key("alt-b", Some("b")), NORMAL, win_meta),
            Some(b"\x1bb".to_vec())
        );
    }
```

In `crates/mkrew-ui/src/keymap.rs` (test module only), line 1086 and line 1348:

```rust
    use crate::terminal::input::{InputMode, KeyEnv, key_to_bytes};
```

```rust
            assert_eq!(
                key_to_bytes(&keystroke, mode, KeyEnv::current(false)),
                Some(bytes),
                "{keys}"
            );
```

In `crates/mkrew-ui/src/ime.rs` (test module only), line 199 and line 255:

```rust
    use crate::terminal::input::{InputMode, KeyEnv, key_to_bytes};
```

```rust
            let bytes = key_to_bytes(&keystroke, InputMode::default(), KeyEnv::current(false))
                .expect("typed key");
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mkrew-ui --lib terminal::input`
Expected: compile errors `cannot find type KeyEnv in this scope` and `cannot find type Os`.

- [ ] **Step 3: Implement `KeyEnv` and the macOS key rules in `input.rs`**

Imports (line 6):

```rust
use gpui::{App, Keystroke};

use crate::keymap::Os;
```

Replace `key_to_bytes`, `is_copy_key`, `is_paste_key`, `KeyAction` docs and `key_action` (lines 17-91) with:

```rust
/// What the system and `[terminal] option_as_meta` change about key encoding (ADR 0099). A GPUI
/// global, set at start and whenever `settings.toml` changes, read by every pane on each key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEnv {
    pub os: Os,
    /// macOS: Option sends ESC plus the key (Meta) instead of the character it composes.
    pub option_as_meta: bool,
}

impl gpui::Global for KeyEnv {}

impl KeyEnv {
    /// This computer, with the setting's value.
    pub const fn current(option_as_meta: bool) -> Self {
        Self {
            os: Os::CURRENT,
            option_as_meta,
        }
    }

    /// The global; this computer's default until it is set.
    pub fn of(cx: &App) -> Self {
        cx.try_global::<Self>()
            .copied()
            .unwrap_or(Self::current(false))
    }
}

/// Bytes a key press sends to the PTY, or `None` if the terminal should ignore it.
///
/// On macOS an Option+key press is not ours unless `env.option_as_meta`: GPUI puts the composed
/// character in `key_char` (Option+B is "∫"), and the key must reach the input method so that
/// dead keys compose (Option+E, then E, is "é"). The input method types the result through
/// `ime::Typing`.
pub fn key_to_bytes(keystroke: &Keystroke, mode: InputMode, env: KeyEnv) -> Option<Vec<u8>> {
    let m = keystroke.modifiers;
    if m.platform {
        return None;
    }
    // xterm modifier parameter: 1 + shift + 2·alt + 4·ctrl.
    let modifier = 1 + u8::from(m.shift) + 2 * u8::from(m.alt) + 4 * u8::from(m.control);

    if let Some(bytes) = special_key(&keystroke.key, m.control, m.shift, modifier, mode) {
        // Shift+Enter already starts with ESC: Alt adds nothing.
        let shift_enter = m.shift && keystroke.key == "enter";
        return Some(with_alt(
            bytes,
            m.alt && !is_csi(&keystroke.key) && !shift_enter,
        ));
    }
    if env.os == Os::Mac && m.alt && !m.control {
        return option_bytes(keystroke, env.option_as_meta);
    }
    // AltGr arrives as Ctrl+Alt with the composed char in `key_char`: send the char as is.
    let altgr = m.control && m.alt;
    let ctrl_only = m.control && !m.alt;
    if let Some(text) = keystroke.key_char.as_deref()
        && !ctrl_only
    {
        return Some(with_alt(text.as_bytes().to_vec(), m.alt && !altgr));
    }
    if m.control {
        return control_byte(&keystroke.key).map(|byte| with_alt(vec![byte], m.alt));
    }
    None
}

/// Option+key on macOS (not Ctrl, not a special key): `None` = the input method composes it;
/// with `option_as_meta`, ESC plus the key as typed without Option (GPUI reports Shift+letter as
/// shift plus the lowercase letter, and Shift on any other key as the shifted char).
fn option_bytes(keystroke: &Keystroke, option_as_meta: bool) -> Option<Vec<u8>> {
    if !option_as_meta {
        return None;
    }
    let key = if keystroke.key == "space" {
        " "
    } else {
        keystroke.key.as_str()
    };
    let mut chars = key.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    let c = if keystroke.modifiers.shift {
        c.to_ascii_uppercase()
    } else {
        c
    };
    let mut bytes = vec![0x1b];
    bytes.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
    Some(bytes)
}

/// Plain Ctrl+C on Windows and Linux: copies the selection when there is one, else it is ^C for
/// the program (like Windows Terminal, ADR 0022). Never on macOS, where Ctrl+C is always ^C and
/// Cmd+C is the keymap's Copy (ADR 0099).
pub fn is_copy_key(keystroke: &Keystroke, os: Os) -> bool {
    let m = keystroke.modifiers;
    os != Os::Mac && keystroke.key == "c" && m.control && !m.shift && !m.alt && !m.platform
}

/// Plain Ctrl+V on Windows and Linux: the pane pastes the clipboard; ^V goes to the program only
/// when there is nothing to paste (ADR 0026). Never on macOS: Ctrl+V is ^V, Cmd+V pastes.
pub fn is_paste_key(keystroke: &Keystroke, os: Os) -> bool {
    let m = keystroke.modifiers;
    os != Os::Mac && keystroke.key == "v" && m.control && !m.shift && !m.alt && !m.platform
}

/// What a key press in a terminal pane does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyAction {
    /// Copy the selection (plain Ctrl+C while text is selected, also in an exited pane; not on
    /// macOS).
    Copy,
    /// Paste the clipboard (plain Ctrl+V in a live pane, ADR 0026; not on macOS).
    Paste,
    Send(Vec<u8>),
    Ignore,
}

/// The pane's answer to `keystroke`: `selected` = text is selected, `exited` = the session
/// ended (nothing more is sent).
pub fn key_action(
    keystroke: &Keystroke,
    selected: bool,
    exited: bool,
    mode: InputMode,
    env: KeyEnv,
) -> KeyAction {
    if selected && is_copy_key(keystroke, env.os) {
        return KeyAction::Copy;
    }
    if !exited && is_paste_key(keystroke, env.os) {
        return KeyAction::Paste;
    }
    match key_to_bytes(keystroke, mode, env) {
        Some(bytes) if !exited => KeyAction::Send(bytes),
        _ => KeyAction::Ignore,
    }
}
```

Also update the module doc (lines 3-4): `ESC prefix for Alt` stays; add a line `//! macOS Option composes unless `[terminal] option_as_meta` (ADR 0099).`

- [ ] **Step 4: Update the callers**

`crates/mkrew-ui/src/terminal/view.rs`, imports (lines 38-40):

```rust
use crate::terminal::input::{
    KeyAction, KeyEnv, is_copy_key, key_action, paste_bytes, wheel_bytes, wheel_report_bytes,
};
```

`key_down` (lines 351-386). Replace the doc comment and the lines that compute `selected` and call `key_action`:

```rust
    /// Keys go to the session. Keystrokes bound in the keymap (SPEC §13) never get here: their
    /// action handler stops them first. On Windows and Linux plain Ctrl+C copies a selection
    /// instead, also in an exited pane (ADR 0022), and plain Ctrl+V pastes (ADR 0026). On macOS
    /// Option+key without `option_as_meta` is left to the input method (ADR 0099). While copy
    /// mode is on every key is the UI's.
```

```rust
        // Only plain Ctrl+C reads the selection: building its text per keystroke costs O(selection).
        let env = KeyEnv::of(cx);
        let selected =
            is_copy_key(&event.keystroke, env.os) && self.emulator.selection_text().is_some();
        let mode = self.emulator.input_mode();
        match key_action(&event.keystroke, selected, self.exited.is_some(), mode, env) {
```

`run` (lines 746-748), add the comment:

```rust
            // Never a ^C: without a selection nothing happens and the keystroke is spent here
            // (macOS Cmd+C is this command, ADR 0099).
            Command::Copy => {
                self.copy_selection(cx);
            }
```

`crates/mkrew-ui/src/main.rs`, directly after `cx.set_global(theme::ThemeNames::from_settings(&settings));` (line 243):

```rust
            cx.set_global(mkrew_ui::terminal::input::KeyEnv::current(
                settings.terminal.option_as_meta,
            ));
```

`crates/mkrew-ui/src/shell/connection.rs`, in `apply_settings_file`, inside `if let Ok(settings) = mkrew_core::settings::load(&self.settings_file) {` directly after `self.close_to_tray = settings.ui.close_to_tray;`:

```rust
                    // Every pane reads it on its next key press: no restart.
                    cx.set_global(mkrew_ui::terminal::input::KeyEnv::current(
                        settings.terminal.option_as_meta,
                    ));
```

Also update the doc comment above `apply_settings_file` ("...fonts, theme, diff layout and wrap, tray, taskbar flash, Jira, reading width, features") by adding `Option as Meta`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p mkrew-ui --lib terminal::input`
Expected: PASS (the four Mac tests `mac_ctrl_c_and_ctrl_v_are_never_copy_or_paste`, `mac_cmd_keys_are_not_for_the_program`, `mac_option_composes_by_default`, `mac_option_as_meta_sends_escape_and_the_plain_key`, and every older test with `WIN`).

Run: `cargo test -p mkrew-ui --lib keymap` and `cargo test -p mkrew-ui --lib ime`
Expected: PASS.

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.

(These tests are plain Rust, so they also run in Plan A's macOS CI job; `KeyEnv::current` there gives `Os::Mac`, and no test depends on it except the keymap one, whose keys are Ctrl+C, Ctrl+R and Escape on every OS.)

- [ ] **Step 6: SPEC and HUMAN_CHECKS**

`docs/SPEC.md` §13, replace lines 1433-1436 ("One exception: plain Ctrl+C copies ... (ADR 0026).") with:

```text
One exception on Windows and Linux: plain Ctrl+C copies while terminal text is selected (ADR 0022); without a selection
it reaches the agent as ^C. Anything sent to the agent clears the selection.
Plain Ctrl+V pastes in a terminal pane; ^V reaches the program only when the clipboard holds no
text, image or files (ADR 0026).
On macOS neither exception applies (ADR 0099): Ctrl+C is always ^C and Ctrl+V is always ^V.
Cmd+C is the Copy command: it copies the selection and does nothing without one (never ^C).
Cmd+V is the Paste command. Other Cmd keys are not sent to the program.
On macOS Option+key types the layout's character by default (Option+2 is €, Option+E then E is é, German Option+L is @).
With `[terminal] option_as_meta = true` Option sends ESC plus the key (Option+B is `ESC b`, Option+Shift+B is `ESC B`), so a shell's word moves work.
Keys that are not text keep their xterm form with Option as Alt (Option+←, Option+Backspace).
The setting has no effect on Windows and Linux (ADR 0099).
```

`docs/HUMAN_CHECKS.md`, add to the 2026-11-22 entry:

```markdown
- [ ] **Copy and paste:** select text in a shell pane and press Cmd+C, the selection clears and Cmd+V in another app pastes it.
  Cmd+C with nothing selected prints nothing (no `^C` in the shell).
  Ctrl+C with text selected interrupts a running `sleep 30` (the selection is dropped, nothing is copied).
  Ctrl+V in a shell prints `^V`.
  Cmd+V pastes text, and an image (path typed) as before.
- [ ] **Option, default (US layout):** Option+2 types €, Option+B types ∫, Option+E then E types é in zsh.
  German layout: Option+L types @, Option+5 types [, Option+7 types |.
- [ ] **Option as Meta:** turn the switch on in Settings without restarting.
  Option+B / Option+F move by word in zsh, Option+Backspace deletes a word, and Option+D deletes the next word.
  Turn it off again: Option+B types ∫ without restarting.
```

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-ui/src/terminal/input.rs crates/mkrew-ui/src/terminal/view.rs crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/src/ime.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): macOS terminal keys - Cmd+C/V, plain Ctrl is ^C/^V, Option composes or is Meta"
```

---

### Task 3: IME ranges, `text_for_range`, IME-first printable keys, dead keys

**Files:**
- Modify: `crates/mkrew-ui/src/ime.rs` (whole file above the test module, and the test module)
- Modify: `crates/mkrew-ui/src/shell/voice_input.rs` (lines 150-151)
- Modify: `crates/mkrew-ui/src/terminal/view.rs` (lines 972-973)
- Modify: `docs/SPEC.md` (§13, one sentence), `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: `KeyEnv`, `key_to_bytes(.., env)` from Task 2 (tests only).
- Produces, in `crate::ime`:
  - `pub struct Marked(RefCell<String>)` (`Debug, Default`) with `len_utf16(&self) -> usize`, `clear(&self)`, `slice(&self, range: Range<usize>) -> (String, Range<usize>)`.
  - `Composing<K>::marked_for(&self, field: &K) -> &Marked` (was `&Cell<usize>`).
  - `pub fn keys(marked: &Marked, range: Option<Range<usize>>, text: &str, mark: bool) -> Vec<Keystroke>`.
  - `Typing::prefers_ime_for_printable_keys` returns `true`.

The input handler's "document" is the composing text only (the fields and the PTY only append and erase at the end).
`replace_text_in_range(Some(range))` replaces `range` of that document: it erases back to `range.start`, then types the replacement and the part that followed `range.end`.
`None` keeps its meaning: replace all composing text.
Ranges are honored only on macOS (`Os::CURRENT == Os::Mac`, at the call site in `Typing::send`): on Windows and Linux a range is read as `None`, so Windows Telex and Unikey behave exactly as before.
GPUI 0.3.7 has `InputHandler::prefers_ime_for_printable_keys(&mut self, &mut Window, &mut App) -> bool` (default `false`); the name `query_prefers_ime_for_printable_keys` is only the platform wrapper's.
Returning `true` matters only while an IME input source (Japanese, Chinese, Korean, Telex) is active: macOS then offers printable keys to the IME first (`gpui-pre-macos` `window.rs` `handle_key_event`), and the IME calls back through `replace_text_in_range`.
With a plain layout (ABC, German) nothing changes.

- [ ] **Step 1: Write the failing tests**

Replace the whole test module of `crates/mkrew-ui/src/ime.rs` (from `#[cfg(test)]`, line 193, to the end) with:

```rust
#[cfg(test)]
mod tests {
    use gpui::{Keystroke, Modifiers};

    use super::{Composing, Marked, keys};
    use crate::keymap::Os;
    use crate::task_dialog::{DialogKey, dialog_key};
    use crate::terminal::input::{InputMode, KeyEnv, key_to_bytes};
    use crate::text_box::{Input, TextBox};

    const MAC: KeyEnv = KeyEnv {
        os: Os::Mac,
        option_as_meta: false,
    };
    const WIN: KeyEnv = KeyEnv {
        os: Os::Windows,
        option_as_meta: false,
    };

    /// Windows Vietnamese Telex composing "việt": each step replaces the composing text,
    /// revising chars typed before; the last step commits.
    fn telex(marked: &Marked) -> Vec<Keystroke> {
        let mut presses = Vec::new();
        for composing in ["v", "vi", "vie", "viê", "việ"] {
            presses.extend(keys(marked, None, composing, true));
        }
        presses.extend(keys(marked, None, "việt", false));
        presses
    }

    /// What a shell line editor does with the bytes the terminal sends: DEL (0x7f) erases one char.
    fn shell_line(presses: &[Keystroke], env: KeyEnv) -> String {
        let mut line = String::new();
        for keystroke in presses {
            let bytes = key_to_bytes(keystroke, InputMode::default(), env).expect("typed key");
            if bytes == [0x7f] {
                line.pop();
            } else {
                line.push_str(std::str::from_utf8(&bytes).expect("utf-8"));
            }
        }
        line
    }

    #[test]
    fn composition_steps_end_as_the_committed_text_in_a_text_box() {
        let marked = Marked::default();
        let mut text = TextBox::default();
        text.insert("tôi ");
        for keystroke in telex(&marked) {
            if let Some(key) = dialog_key(&keystroke) {
                text.key(key);
            }
        }
        assert_eq!(text.text(), "tôi việt", "earlier text is never erased");
        assert_eq!(marked.len_utf16(), 0);
    }

    /// Ctrl+A, then Telex: the first composing char replaces the selection, the re-typing
    /// Backspaces only erase composing chars (plain Backspace stays one char).
    #[test]
    fn composition_after_select_all_replaces_the_selection_once() {
        let marked = Marked::default();
        let mut input = Input::from("tôi cũ");
        input.select_all();
        for keystroke in telex(&marked) {
            match dialog_key(&keystroke) {
                Some(DialogKey::Insert(text)) => {
                    input.insert(&text);
                }
                Some(key) => {
                    input.edit(&key);
                }
                None => {}
            }
        }
        assert_eq!(input, "việt");
        assert_eq!(marked.len_utf16(), 0);
    }

    #[test]
    fn composition_steps_reach_the_terminal_as_the_committed_text() {
        for env in [WIN, MAC] {
            assert_eq!(shell_line(&telex(&Marked::default()), env), "việt");
        }
    }

    #[test]
    fn committed_text_types_each_char_and_drops_control_chars() {
        let typed: Vec<String> = keys(&Marked::default(), None, "ệ\u{8}a\r", false)
            .into_iter()
            .map(|k| k.key_char.unwrap_or(k.key))
            .collect();
        assert_eq!(typed, ["ệ", "a"]);
    }

    /// Windows Telex keeps composing when the click moves to another field: the next step
    /// must not erase that field's own text with the first field's composing text.
    #[test]
    fn composition_moved_to_another_field_never_erases_its_text() {
        let composing = Composing::<u8>::default();
        assert_eq!(keys(composing.marked_for(&1), None, "vi", true).len(), 2);
        let moved = keys(composing.marked_for(&2), None, "việ", true);
        assert!(moved.iter().all(|k| k.key != "backspace"), "{moved:?}");
        assert_eq!(moved.len(), 3);
        let next = keys(composing.marked_for(&2), None, "việt", false);
        assert_eq!(next.iter().filter(|k| k.key == "backspace").count(), 3);
    }

    #[test]
    fn a_range_inside_the_composing_text_erases_back_to_it() {
        let marked = Marked::default();
        let mut presses = keys(&marked, None, "viet", true);
        // The input method revises "et" (UTF-16 units 2..4) to "êt".
        presses.extend(keys(&marked, Some(2..4), "êt", true));
        assert_eq!(shell_line(&presses, MAC), "viêt");
        assert_eq!(marked.len_utf16(), 4);
    }

    #[test]
    fn a_range_in_the_middle_retypes_what_followed_it() {
        let marked = Marked::default();
        let mut presses = keys(&marked, None, "abc", true);
        presses.extend(keys(&marked, Some(1..2), "X", true));
        assert_eq!(shell_line(&presses, MAC), "aXc");
        assert_eq!(marked.slice(0..9).0, "aXc", "still composing");
    }

    #[test]
    fn a_range_past_the_composing_text_is_cut_to_it() {
        let marked = Marked::default();
        let mut presses = keys(&marked, None, "ab", true);
        presses.extend(keys(&marked, Some(5..9), "c", true));
        assert_eq!(shell_line(&presses, MAC), "abc");
        // With nothing composing a range erases nothing.
        let presses = keys(&Marked::default(), Some(0..1), "x", false);
        assert_eq!(shell_line(&presses, MAC), "x");
    }

    #[test]
    fn text_for_range_is_the_composing_text_cut_to_the_range() {
        let marked = Marked::default();
        keys(&marked, None, "việt", true);
        assert_eq!(marked.slice(1..3), ("iệ".to_owned(), 1..3));
        assert_eq!(marked.slice(2..99), ("ệt".to_owned(), 2..4));
        assert_eq!(marked.slice(9..12), (String::new(), 4..4));
        // Ranges count UTF-16 units: the emoji is two.
        let emoji = Marked::default();
        keys(&emoji, None, "a😀b", true);
        assert_eq!(emoji.len_utf16(), 4);
        assert_eq!(emoji.slice(1..3), ("😀".to_owned(), 1..3));
        assert_eq!(emoji.slice(1..2), ("😀".to_owned(), 1..3), "never half a pair");
    }

    /// macOS Option+E then E: the first key is not typed by the terminal (it goes to the input
    /// method, which marks the accent and then replaces it), and Option+2 types its character.
    #[test]
    fn option_dead_key_goes_to_the_input_method_which_types_the_accent() {
        let option = |key: &str, composed: &str| Keystroke {
            modifiers: Modifiers {
                alt: true,
                ..Modifiers::default()
            },
            key: key.into(),
            key_char: Some(composed.into()),
        };
        assert_eq!(
            key_to_bytes(&option("e", "´"), InputMode::default(), MAC),
            None,
            "left to the input method"
        );
        // AppKit then calls setMarkedText("´"), and the next key insertText("é").
        let marked = Marked::default();
        let mut presses = keys(&marked, None, "´", true);
        presses.extend(keys(&marked, None, "é", false));
        assert_eq!(shell_line(&presses, MAC), "é");
        // Option+2: no marking, the input method inserts "€".
        assert_eq!(
            key_to_bytes(&option("2", "€"), InputMode::default(), MAC),
            None
        );
        let presses = keys(&Marked::default(), None, "€", false);
        assert_eq!(shell_line(&presses, MAC), "€");
    }
}
```

`slice(1..2)` on "a😀b": `byte_at(1)` is the start of the emoji, `byte_at(2)` falls inside the pair and rounds to the next char (`b`), so the slice is the whole emoji and the range reported is `1..3`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mkrew-ui --lib ime`
Expected: compile errors `unresolved import super::Marked` and `this function takes 3 arguments but 4 arguments were supplied`.

- [ ] **Step 3: Implement `Marked`, ranges, `text_for_range` and IME-first keys**

In `crates/mkrew-ui/src/ime.rs`, replace the module doc (lines 1-4):

```rust
//! Platform text input for the hand-rolled fields and the terminal (SPEC §10, §13): text from
//! an IME (Windows Vietnamese Telex, CJK), a macOS dead key or Option character, or sent as
//! Unicode input (Unikey, EVKey) arrives here instead of as key presses. It becomes key presses
//! for the field's own key handler: chars are typed, and composing text is typed in place and
//! erased with Backspace when it changes.
```

Replace `Composing` and its impls (lines 15-42) with:

```rust
/// The composing text typed so far: the document the input method sees. It reads it
/// (`text_for_range`) and replaces parts of it (`replace_text_in_range` with a range); ranges
/// count UTF-16 units from its start. Text that is not composing any more belongs to the
/// program or the field: it is never offered, so it is never erased.
#[derive(Debug, Default)]
pub struct Marked(RefCell<String>);

impl Marked {
    /// Length in UTF-16 units.
    pub fn len_utf16(&self) -> usize {
        self.0.borrow().encode_utf16().count()
    }

    /// Nothing is composing any more.
    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }

    /// The text of `range` (cut to what is composing) and the range it covers.
    pub fn slice(&self, range: Range<usize>) -> (String, Range<usize>) {
        let text = self.0.borrow();
        let from = byte_at(&text, range.start);
        let to = byte_at(&text, range.end).max(from);
        let units = |end: usize| text[..end].encode_utf16().count();
        (text[from..to].to_owned(), units(from)..units(to))
    }
}

/// Byte offset of UTF-16 unit `unit` in `text`: the end when `unit` is past it, the next char
/// when it falls inside a surrogate pair.
fn byte_at(text: &str, unit: usize) -> usize {
    let mut seen = 0;
    for (byte, c) in text.char_indices() {
        if seen >= unit {
            return byte;
        }
        seen += c.len_utf16();
    }
    text.len()
}

/// The composing text typed so far and the field (`K`, a focus handle) it was typed into.
/// Telex keeps composing across a click into another field; that field never typed it.
#[derive(Debug)]
pub struct Composing<K> {
    field: RefCell<Option<K>>,
    marked: Marked,
}

impl<K> Default for Composing<K> {
    fn default() -> Self {
        Self {
            field: RefCell::new(None),
            marked: Marked::default(),
        }
    }
}

impl<K: PartialEq + Clone> Composing<K> {
    /// The composing text of `field`: empty when it was typed into another one.
    pub fn marked_for(&self, field: &K) -> &Marked {
        let mut owner = self.field.borrow_mut();
        if owner.as_ref() != Some(field) {
            *owner = Some(field.clone());
            self.marked.clear();
        }
        &self.marked
    }
}
```

Replace `keys` (lines 44-67) with:

```rust
/// Key presses that replace `range` of the `marked` composing text (`None` = all of it) with
/// `text`; the result is composing again when `mark`. Control chars of `text` are dropped:
/// Enter, Tab and Backspace come as real key presses. Only the end of a line can be edited, so
/// the presses erase back to the start of `range`, then type `text` and what followed `range`.
pub fn keys(
    marked: &Marked,
    range: Option<Range<usize>>,
    text: &str,
    mark: bool,
) -> Vec<Keystroke> {
    let old = marked.0.take();
    let (from, to) = match range {
        Some(range) => {
            let from = byte_at(&old, range.start);
            (from, byte_at(&old, range.end).max(from))
        }
        None => (0, old.len()),
    };
    let typed: String = text.chars().filter(|c| !c.is_control()).collect();
    let (head, tail) = (&old[..from], &old[to..]);
    let backspace = Keystroke {
        key: "backspace".into(),
        ..Keystroke::default()
    };
    let mut presses: Vec<Keystroke> = old[from..].chars().map(|_| backspace.clone()).collect();
    presses.extend(typed.chars().chain(tail.chars()).map(|c| {
        let s = c.to_string();
        Keystroke {
            key: s.clone(),
            key_char: Some(s),
            ..Keystroke::default()
        }
    }));
    if mark {
        *marked.0.borrow_mut() = format!("{head}{typed}{tail}");
    }
    presses
}
```

`Typing::send` (lines 104-115), and add `use crate::keymap::Os;` to the imports of `ime.rs`:

```rust
impl<V: 'static> Typing<V> {
    fn send(
        &self,
        range: Option<Range<usize>>,
        text: &str,
        mark: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Only macOS's input methods are known to send ranges: elsewhere a range stays "all of
        // the composing text" as before (ADR 0099).
        let range = range.filter(|_| Os::CURRENT == Os::Mac);
        for keystroke in keys(self.marked.marked_for(&self.focus), range, text, mark) {
            let event = KeyDownEvent {
                keystroke,
                is_held: false,
                prefer_character_input: false,
            };
            let on_key = self.on_key;
            self.view
                .update(cx, |view, cx| on_key(view, &event, window, cx));
        }
    }
}
```

Replace the comment and the methods of `impl InputHandler for Typing<V>` that change (lines 118-172):

```rust
// The fields and the PTY only append and erase at the end, so the "document" is just the
// composing text: selection = end of it, and ranges are UTF-16 units into it.
impl<V: 'static> InputHandler for Typing<V> {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<UTF16Selection> {
        let end = self.marked.marked_for(&self.focus).len_utf16();
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }

    fn marked_text_range(&mut self, _window: &mut Window, _cx: &mut App) -> Option<Range<usize>> {
        let len = self.marked.marked_for(&self.focus).len_utf16();
        (len > 0).then_some(0..len)
    }

    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<String> {
        let (text, actual) = self.marked.marked_for(&self.focus).slice(range);
        *adjusted = Some(actual);
        (!text.is_empty()).then_some(text)
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.send(range, text, false, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.send(range, text, true, window, cx);
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut App) {
        self.marked.marked_for(&self.focus).clear();
    }

    /// With an IME input source active (Japanese, Chinese, Korean, Telex) printable keys reach
    /// the input method first, as they must to compose; it types through `replace_text_in_range`.
    /// With a plain layout this changes nothing.
    fn prefers_ime_for_printable_keys(&mut self, _window: &mut Window, _cx: &mut App) -> bool {
        true
    }
```

(`bounds_for_range` and `character_index_for_point` stay as they are. `bounds_for_range` keeps returning `self.caret`, which the terminal sets to its cursor cell at `view.rs` `TerminalElement::paint`.)

Callers of the old API:

`crates/mkrew-ui/src/shell/voice_input.rs`, lines 150-151:

```rust
            self.ime_marked.marked_for(&focus).clear();
            for keystroke in ime::keys(&ime::Marked::default(), None, text, false) {
```

`crates/mkrew-ui/src/terminal/view.rs`, lines 972-973:

```rust
        self.ime_marked.marked_for(&self.find_focus).clear();
        for keystroke in crate::ime::keys(&crate::ime::Marked::default(), None, text, false) {
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p mkrew-ui --lib ime`
Expected: PASS, ten tests.

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.
(`prefers_ime_for_printable_keys` compiles on every OS: it is a default method of GPUI's shared `InputHandler`. It has no unit test because GPUI's `Window` cannot be built without a display; `docs/HUMAN_CHECKS.md` carries the manual check.)

- [ ] **Step 5: SPEC and HUMAN_CHECKS**

`docs/SPEC.md` §13, after the sentence about the Option setting added by Task 2, add:

```text
The input method (macOS dead keys and Option characters, Windows Telex, CJK) types into a terminal or field as key presses: composing text is typed in place and erased and retyped when the input method revises it, by range.
With an input source that is an IME, printable keys reach it first.
```

`docs/HUMAN_CHECKS.md`, add to the 2026-11-22 entry:

```markdown
- [ ] **Vietnamese (macOS built-in Telex, then VNI):** type "việt nam" in a zsh pane, in the composer and in the Git commit box.
  The word is right when you press Space, with no doubled letters ("vieetj" would be a bug) and nothing left of the older word.
  [INFERENCE] The built-in input method may revise text it already committed, which Mkrew cannot reach (see ADR 0099, Not changed): if so, say so.
- [ ] **Japanese or Chinese IME:** with Hiragana or Pinyin active, type "nihon" in a shell pane: the first key composes (no ASCII `n` printed before the candidate window opens).
  The candidate window opens next to the cursor.
- [ ] **Dead keys:** US layout, in a shell pane, Option+E then E types é; Option+` then A types à; Option+E then Space types ´.
  (Text fields do not compose Option dead keys yet; see Plan A "Not fixed".)
- [ ] **Windows (regression):** Telex or Unikey types "việt" in a shell pane, the composer and the find bar exactly as before.
```

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/ime.rs crates/mkrew-ui/src/shell/voice_input.rs crates/mkrew-ui/src/terminal/view.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): IME ranges, text_for_range and IME-first printable keys"
```

---

### Task 4: Double-click selects a word, triple-click a line

**Files:**
- Modify: `crates/mkrew-ui/src/terminal/emulator.rs` (lines 418-424 `start_selection`, 441-447 `selection_text`, 493-500 `CopyKey::Copy`, tests)
- Modify: `crates/mkrew-ui/src/terminal/layout.rs` (test module: lines 373, 577, 622)
- Modify: `crates/mkrew-ui/src/terminal/view.rs` (line 35 import, lines 640-645 `mouse_down`)
- Modify: `docs/SPEC.md` (§10, line 1037), `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces, in `crate::terminal::emulator`:
  - `pub enum Pick { Cell, Word, Line }` (`Clone, Copy, Debug, PartialEq, Eq`) with `Pick::from_clicks(clicks: usize) -> Pick` (0 or 1 = `Cell`, 2 = `Word`, 3 or more = `Line`).
  - `Emulator::start_selection(&mut self, row: usize, col: usize, side: Side, pick: Pick)` (one more parameter).
  - `Emulator::selection_text` never ends with a line break.

alacritty's `SelectionType::Semantic` expands a click to the word between the characters of `SEMANTIC_ESCAPE_CHARS` (`,│`|:"' ()[]{}<>\t`), so a path such as `/usr/local/bin` or `src/a-b.rs` is one word, and a click on a bracket selects through its partner.
`SelectionType::Lines` selects whole lines and its text ends with `\n`, which `selection_text` now removes.
`Emulator::grid` already builds the selected cells from `renderable_content().selection`, which is the expanded range, and `update_selection` while dragging expands by word or line.

- [ ] **Step 1: Write the failing tests**

In `crates/mkrew-ui/src/terminal/emulator.rs`, in `mod tests`, after `click_without_drag_selects_nothing` (line 780), add:

```rust
    #[test]
    fn pick_follows_the_click_count() {
        assert_eq!(Pick::from_clicks(0), Pick::Cell);
        assert_eq!(Pick::from_clicks(1), Pick::Cell);
        assert_eq!(Pick::from_clicks(2), Pick::Word);
        assert_eq!(Pick::from_clicks(3), Pick::Line);
        assert_eq!(Pick::from_clicks(4), Pick::Line);
    }

    #[test]
    fn a_double_click_selects_the_word_and_a_drag_extends_by_words() {
        let mut emu = Emulator::new(40, 3);
        emu.advance(b"cd /usr/local/bin && ls -la\r\nsecond line");
        // Column 7 is the "/" between "usr" and "local": the word runs to the blanks around it.
        emu.start_selection(0, 7, Side::Left, Pick::Word);
        assert_eq!(emu.selection_text().as_deref(), Some("/usr/local/bin"));
        assert!(emu.grid().rows[0][3].selected && emu.grid().rows[0][16].selected);
        assert!(!emu.grid().rows[0][2].selected && !emu.grid().rows[0][17].selected);
        // Dragging into "&&" (columns 18 and 19) takes that whole word too.
        emu.update_selection(0, 19, Side::Right);
        assert_eq!(emu.selection_text().as_deref(), Some("/usr/local/bin &&"));
    }

    #[test]
    fn a_double_click_on_a_bracket_selects_through_its_partner_and_on_blanks_nothing() {
        let mut emu = Emulator::new(40, 2);
        emu.advance(b"f(a, b) x");
        emu.start_selection(0, 1, Side::Left, Pick::Word);
        assert_eq!(emu.selection_text().as_deref(), Some("(a, b)"));
        emu.start_selection(0, 30, Side::Left, Pick::Word);
        assert_eq!(emu.selection_text(), None, "blank cells copy nothing");
    }

    #[test]
    fn a_triple_click_selects_the_line_and_copies_it_without_the_line_break() {
        let mut emu = Emulator::new(20, 3);
        emu.advance(b"first line\r\nsecond");
        emu.start_selection(0, 4, Side::Left, Pick::Line);
        assert_eq!(emu.selection_text().as_deref(), Some("first line"));
        assert!(emu.grid().rows[0][0].selected && emu.grid().rows[0][19].selected);
        assert!(!emu.grid().rows[1][0].selected);
        emu.update_selection(1, 2, Side::Right);
        assert_eq!(emu.selection_text().as_deref(), Some("first line\nsecond"));
    }
```

In the existing `copy_v_starts_switches_and_drops_the_selection` (lines 1174-1176) the line selection no longer ends with a break:

```rust
        copy_keys(&mut emu, [CopyKey::Select(Select::Lines)]);
        // A line selection ends with a line break in alacritty; selection_text drops it.
        assert_eq!(emu.selection_text().as_deref(), Some("second"));
```

Update the old call sites for the new parameter (`, Pick::Cell` as the last argument):
- `emulator.rs` lines 745, 763, 777, 804, 1300, 1310 (`emu.start_selection(...)`).
- `layout.rs` lines 577 and 622; in the `layout.rs` test module change line 373 to `use crate::terminal::emulator::{Emulator, Pick};`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mkrew-ui --lib terminal::emulator`
Expected: compile error `cannot find type Pick in this scope`.

- [ ] **Step 3: Implement**

In `crates/mkrew-ui/src/terminal/emulator.rs`, add near the other small public types (after the `Wheel` enum, line 142 area):

```rust
/// What a mouse press selects: the clicked cell (single click), the word under it (double
/// click) or its line (triple click). A drag that follows extends by cells, words or lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Cell,
    Word,
    Line,
}

impl Pick {
    /// The pick of a press with `clicks` clicks in a row (GPUI's `click_count`).
    pub fn from_clicks(clicks: usize) -> Self {
        match clicks {
            0 | 1 => Pick::Cell,
            2 => Pick::Word,
            _ => Pick::Line,
        }
    }
}
```

Replace `start_selection` (lines 418-424):

```rust
    /// Starts a selection at a visible cell (row 0 = top of the view). `side` = which half of
    /// the cell was hit, so a click between cells selects nothing. A `Word` pick uses
    /// alacritty's semantic selection (a bracket selects through its partner), a `Line` pick
    /// whole lines.
    pub fn start_selection(&mut self, row: usize, col: usize, side: Side, pick: Pick) {
        let point = self.view_point(row, col);
        let ty = match pick {
            Pick::Cell => SelectionType::Simple,
            Pick::Word => SelectionType::Semantic,
            Pick::Line => SelectionType::Lines,
        };
        self.term.selection = Some(Selection::new(ty, point, side));
        self.changed();
    }
```

Replace `selection_text` (lines 441-447):

```rust
    /// Selected text (lines joined with `\n`, trailing blanks trimmed, no line break at the end:
    /// alacritty ends a line selection with one, and pasted into a shell it would run); `None`
    /// if nothing is selected.
    pub fn selection_text(&self) -> Option<String> {
        self.term
            .selection_to_string()
            .map(|text| text.trim_end_matches('\n').to_owned())
            .filter(|text| !text.is_empty())
    }
```

In `copy_key`, `CopyKey::Copy` (lines 493-500), the trimming now lives in `selection_text`:

```rust
            CopyKey::Copy => {
                let text = self.selection_text();
                self.leave_copy_mode();
                return CopyEnd::Leave(text);
            }
```

`crates/mkrew-ui/src/terminal/view.rs`, import (line 35):

```rust
use crate::terminal::emulator::{CopyEnd, Emulator, Grid, Pick, Wheel};
```

`mouse_down`, lines 640-645:

```rust
        window.focus(&self.focus_handle, cx);
        if let Some((row, col, side)) = self.cell_at(event.position) {
            let pick = Pick::from_clicks(event.click_count);
            self.emulator.start_selection(row, col, side, pick);
            self.selecting = true;
            cx.notify();
        }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p mkrew-ui --lib terminal`
Expected: PASS (the four new tests, `copy_line_selection_takes_whole_lines`, `copy_v_starts_switches_and_drops_the_selection`, the layout tests).
If a drag assertion differs, read `Selection::range_semantic` in `alacritty_terminal-0.26.0/src/selection.rs:298`: it expands both ends with `semantic_search_left` / `semantic_search_right`, which is what the assertions describe.

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: SPEC and HUMAN_CHECKS**

`docs/SPEC.md` §10, line 1037-1038: change "selection & copy, paste (bracketed)," to:

```text
  selection (a drag; a double-click takes the word, a triple-click the line, and a drag after either extends by words or lines; a bracket selects through its partner) & copy (the copied text never ends with a line break), paste (bracketed),
```

`docs/HUMAN_CHECKS.md`, add to the 2026-11-22 entry:

```markdown
- [ ] **Double and triple click (dark + light):** double-click a path in `ls` output: the whole path highlights in the selection color, Cmd+C copies it.
  Triple-click a command line: the whole row highlights; paste into another shell pane: it is not run (no Enter).
  Double-click on a `(`: the text through the matching `)` highlights.
```

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/terminal/emulator.rs crates/mkrew-ui/src/terminal/layout.rs crates/mkrew-ui/src/terminal/view.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): double-click selects a word, triple-click a line in the terminal"
```

---

### Task 5: Trackpad scrolling: gentler for programs, no stale fraction

**Files:**
- Modify: `crates/mkrew-ui/src/terminal/input.rs` (new function after `wheel_report_bytes`, imports, tests)
- Modify: `crates/mkrew-ui/src/terminal/view.rs` (import lines 38-40, field doc line 66, `scroll_wheel` lines 689-701)
- Modify: `docs/SPEC.md` (§10, lines 1040-1042), `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: nothing from earlier tasks (it edits the same `input.rs` import line as Task 2: keep both `App` and the new items).
- Produces, in `crate::terminal::input`:
  - `pub const PIXEL_WHEEL_SCALE: f32 = 0.5;`
  - `pub fn wheel_lines(delta: ScrollDelta, line_height: Pixels, to_program: bool, carry: f32) -> (i32, f32)`: whole lines of one wheel event (positive = up / into the history) and the fraction to carry to the next event.

GPUI on macOS sends `ScrollDelta::Pixels` for a trackpad or Magic Mouse and `ScrollDelta::Lines` for a notched wheel (`gpui-pre-macos` `events.rs`); the values already follow the system's natural-scrolling setting.
`to_program` is true when the lines become wheel reports or arrow keys (a program has mouse tracking or alternate scroll), false when they scroll the UI-side history.
A notched wheel (`Lines`) is never scaled: one notch is the lines it says.

- [ ] **Step 1: Write the failing tests**

In `crates/mkrew-ui/src/terminal/input.rs`, in `mod tests`, change the import (`use gpui::Modifiers;`, the one from Task 2) to:

```rust
    use gpui::{Modifiers, ScrollDelta, point, px};
```

and add at the end of the module:

```rust
    fn pixels(y: f32) -> ScrollDelta {
        ScrollDelta::Pixels(point(px(0.0), px(y)))
    }

    #[test]
    fn a_notched_wheel_is_whole_lines_for_the_history_and_for_programs() {
        let lines = ScrollDelta::Lines(point(0.0, 3.0));
        assert_eq!(wheel_lines(lines, px(20.0), false, 0.0), (3, 0.0));
        assert_eq!(wheel_lines(lines, px(20.0), true, 0.0), (3, 0.0));
    }

    #[test]
    fn trackpad_pixels_move_a_program_half_as_far_as_the_history() {
        // 46 px of a 23 px line: two lines of history, one wheel report or arrow key.
        assert_eq!(wheel_lines(pixels(46.0), px(23.0), false, 0.0), (2, 0.0));
        assert_eq!(wheel_lines(pixels(46.0), px(23.0), true, 0.0), (1, 0.0));
        assert_eq!(wheel_lines(pixels(-46.0), px(23.0), true, 0.0), (-1, 0.0));
        assert_eq!(PIXEL_WHEEL_SCALE, 0.5);
    }

    #[test]
    fn a_fraction_carries_in_the_same_direction() {
        assert_eq!(wheel_lines(pixels(10.0), px(20.0), false, 0.0), (0, 0.5));
        assert_eq!(wheel_lines(pixels(15.0), px(20.0), false, 0.5), (1, 0.25));
        // No movement keeps what was carried.
        assert_eq!(wheel_lines(pixels(0.0), px(20.0), false, 0.5), (0, 0.5));
    }

    #[test]
    fn a_change_of_direction_drops_the_carried_fraction() {
        // Half a line up was carried; the first move down starts from zero, not from +0.5.
        assert_eq!(wheel_lines(pixels(-10.0), px(20.0), false, 0.5), (0, -0.5));
        assert_eq!(wheel_lines(pixels(10.0), px(20.0), false, -0.5), (0, 0.5));
        // 1.25 lines down after 0.5 up: one line, not zero.
        assert_eq!(wheel_lines(pixels(-25.0), px(20.0), false, 0.5), (-1, -0.25));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mkrew-ui --lib terminal::input`
Expected: compile error `cannot find function wheel_lines in this scope`.

- [ ] **Step 3: Implement**

In `crates/mkrew-ui/src/terminal/input.rs`, imports:

```rust
use gpui::{App, Keystroke, Pixels, ScrollDelta};
```

(keep `use crate::keymap::Os;`). Add before `wheel_bytes`:

```rust
/// Lines a trackpad's pixel scrolling counts per line of travel when they go to a program as
/// arrow keys or wheel reports: one swipe with momentum is dozens of lines, and each one would
/// be a key press for `less` or an agent TUI. Scrolling the history stays 1 to 1.
// ponytail: a fixed 0.5 tuned by reading, not by feel; make it `[terminal]` setting if a Mac
// user finds it too slow or too fast (docs/HUMAN_CHECKS.md).
pub const PIXEL_WHEEL_SCALE: f32 = 0.5;

/// Whole wheel lines of one scroll event (positive = up, into the history) and the fraction to
/// carry to the next one. `to_program`: the lines become wheel reports or arrow keys, not
/// history scrolling. A notched wheel gives lines as they are; precise (pixel) deltas are
/// scaled by [`PIXEL_WHEEL_SCALE`] when `to_program`. The carry of the other direction is
/// dropped: it is not part of this swipe.
pub fn wheel_lines(
    delta: ScrollDelta,
    line_height: Pixels,
    to_program: bool,
    carry: f32,
) -> (i32, f32) {
    let lines = match delta {
        ScrollDelta::Lines(lines) => lines.y,
        ScrollDelta::Pixels(pixels) => {
            let lines = f32::from(pixels.y) / f32::from(line_height);
            if to_program {
                lines * PIXEL_WHEEL_SCALE
            } else {
                lines
            }
        }
    };
    let carry = if carry * lines < 0.0 { 0.0 } else { carry };
    let total = lines + carry;
    // Whole lines of one wheel event: far inside i32.
    (total.trunc() as i32, total.fract())
}
```

`crates/mkrew-ui/src/terminal/view.rs`, import (lines 38-40, with Task 2's `KeyEnv`):

```rust
use crate::terminal::input::{
    KeyAction, KeyEnv, is_copy_key, key_action, paste_bytes, wheel_bytes, wheel_lines,
    wheel_report_bytes,
};
```

Field doc (line 66):

```rust
    /// Wheel movement below one line, carried to the next event (dropped when the direction changes).
    scroll_remainder: f32,
```

In `scroll_wheel`, replace lines 689-702 (from `let lines = f32::from(...)` through the `let wheel = ...;` statement) with:

```rust
        let wheel = if self.emulator.copy_mode() {
            Wheel::History
        } else {
            self.emulator.wheel()
        };
        let (whole, carry) = wheel_lines(
            event.delta,
            geometry.line_height,
            wheel != Wheel::History,
            self.scroll_remainder,
        );
        self.scroll_remainder = carry;
        if whole == 0 {
            return;
        }
```

The rest of the function (`if wheel != Wheel::History { ... } else { ... }`) is unchanged.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p mkrew-ui --lib terminal::input`
Expected: PASS (the four wheel tests and the older `wheel_lines_become_arrow_presses`, `wheel_lines_become_mouse_wheel_reports`).

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: SPEC and HUMAN_CHECKS**

`docs/SPEC.md` §10, after "alternate scroll gets Up/Down; copy mode always scrolls the history)," (line 1042) extend the wheel sentence:

```text
  a trackpad's pixel scrolling counts half a line per line of travel when it goes to a program (wheel reports or arrow keys) and a full line when it scrolls the history, and a change of direction drops the left-over fraction (ADR 0099),
```

`docs/HUMAN_CHECKS.md`, add to the 2026-11-22 entry:

```markdown
- [ ] **Trackpad (dark + light):** in `less` on a long file, a short swipe moves a few lines and a hard flick does not run to the end of the file.
  In an agent's full-screen view the scroll feels the same size as in a plain pane.
  Scroll the history of a shell pane with a swipe, then reverse in the middle: the first line after the reversal moves at once.
  Tune `PIXEL_WHEEL_SCALE` in `terminal/input.rs` if 0.5 is too slow or too fast.
```

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/terminal/input.rs crates/mkrew-ui/src/terminal/view.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): gentler trackpad scrolling for terminal programs, drop stale fraction"
```

---

### Task 6: Terminal cell size in whole device pixels

**Files:**
- Modify: `crates/mkrew-ui/src/terminal/layout.rs` (new function after `cell_at`, line 287; tests)
- Modify: `crates/mkrew-ui/src/terminal/view.rs` (import line 41, `prepaint` line 1131, new function after `cell_size`, line 1368)
- Modify: `docs/SPEC.md` (§10, line 1051), `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `crate::terminal::layout::snap_to_device(value: f32, scale: f32) -> f32` (logical px rounded to a whole number of device pixels, at least one), `crate::terminal::view::grid_cell_size(window: &Window, theme: &Theme) -> (Pixels, Pixels)` (private; the grid's cell width and line height).

Why: GPUI snaps each quad (cell backgrounds, selection, cursor) to device pixels on its own (`Window::paint_quad` calls `snap_bounds`), and rounds each glyph origin the same way.
A 22.75 px line is 45.5 device pixels at 2x, so row tops land on 45 or 46 pixel steps depending on the row index: uneven stripes in backgrounds and baselines that jump by a pixel between rows.
With a line height and cell width that are whole device pixels every row and column has the same size.
[INFERENCE] The visual effect needs a Retina display to confirm; the mechanism is in GPUI's source.
`cell_size` itself stays as it is: the Files editor lays its text out by the font's own advance and uses `cell_size(..).0` to map columns, so it must not be rounded.

- [ ] **Step 1: Write the failing tests**

In `crates/mkrew-ui/src/terminal/layout.rs`, in `mod tests`, add after `grid_size_fits_cells_inside_padding`:

```rust
    #[test]
    fn snap_to_device_rounds_to_whole_device_pixels() {
        // 13 px x 1.75 = 22.75 px: 46 device pixels at 2x, 23 at 1x.
        assert_eq!(snap_to_device(22.75, 2.0), 23.0);
        assert_eq!(snap_to_device(22.75, 1.0), 23.0);
        // A Menlo 13 px advance of 7.83 px is 15.66 device pixels at 2x: 16.
        assert_eq!(snap_to_device(7.83, 2.0), 8.0);
        // Fractional scales: 28.4375 device pixels at 1.25x round to 28, i.e. 22.4 px.
        assert!((snap_to_device(22.75, 1.25) - 22.4).abs() < 1e-4);
        // Already whole: unchanged.
        assert_eq!(snap_to_device(13.0, 2.0), 13.0);
    }

    #[test]
    fn snap_to_device_never_gives_a_zero_cell() {
        assert_eq!(snap_to_device(0.1, 1.0), 1.0);
        assert_eq!(snap_to_device(0.1, 2.0), 0.5);
        assert_eq!(snap_to_device(0.0, 2.0), 0.5);
        // A scale below 1 does not exist; a bad one never divides by zero.
        assert_eq!(snap_to_device(10.0, 0.0), 10.0);
        assert_eq!(snap_to_device(f32::NAN, 2.0), 0.5);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mkrew-ui --lib terminal::layout`
Expected: compile error `cannot find function snap_to_device in this scope`.

- [ ] **Step 3: Implement**

In `crates/mkrew-ui/src/terminal/layout.rs`, after `cell_at` (line 287):

```rust
/// `value` logical pixels rounded to a whole number of device pixels (at least one), for a
/// window with `scale` device pixels per logical pixel. A scale below 1 is taken as 1.
pub fn snap_to_device(value: f32, scale: f32) -> f32 {
    let scale = scale.max(1.0);
    (value * scale).round().max(1.0) / scale
}
```

`crates/mkrew-ui/src/terminal/view.rs`, import (line 41):

```rust
use crate::terminal::layout::{
    Background, CellStyle, RowCache, cell_at, grid_size, layout_row, snap_to_device,
};
```

`prepaint` (line 1131):

```rust
        let (cell_width, line_height) = grid_cell_size(window, &self.theme);
```

After `cell_size` (line 1368), add, and extend `cell_size`'s doc comment with "The Files editor uses this one."

```rust
/// [`cell_size`] with both sides a whole number of device pixels, for the terminal grid: GPUI
/// snaps every quad and glyph to device pixels on its own, so a 22.75 px line (45.5 device
/// pixels at 2x) would make rows alternate between 45 and 46 (ADR 0099). Not for the Files
/// editor, which lays its text out by the font's own advance.
fn grid_cell_size(window: &Window, theme: &Theme) -> (Pixels, Pixels) {
    let (cell_width, line_height) = cell_size(window, theme);
    let scale = window.scale_factor();
    (
        px(snap_to_device(f32::from(cell_width), scale)),
        px(snap_to_device(f32::from(line_height), scale)),
    )
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p mkrew-ui --lib terminal::layout`
Expected: PASS.

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.

Then run the whole gate once for the branch: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt, clippy and tests pass. If it runs past 600 s, run `cargo test --workspace` on its own.

- [ ] **Step 5: SPEC and HUMAN_CHECKS**

`docs/SPEC.md` §10, line 1051: "Use a monospace font with a precomputed cell size; no per-cell measurement." becomes:

```text
    - Use a monospace font with a precomputed cell size (width and line height are whole device pixels, ADR 0099); no per-cell measurement.
```

`docs/HUMAN_CHECKS.md`, add to the 2026-11-22 entry:

```markdown
- [ ] **Check the terminal visually on macOS (dark/light), Retina and a non-Retina display:** select several lines and run `ls --color` or a program with colored backgrounds: every row has the same height, no 1 px stripes or gaps between rows, text sits on the same baseline in every row.
  The window grid is a column or two different from before (the cell is slightly wider or taller): resizing the window still fits whole columns.
  On Windows at 100 % and 150 %: the same, and the text looks as before.
  The Files editor text and caret still line up when you click in the middle of a long line.
```

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/terminal/layout.rs crates/mkrew-ui/src/terminal/view.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "fix(ui): terminal cell width and line height in whole device pixels"
```

---

## Coordination with the other plans

- **Plan A, text fields.**
  `task_dialog::dialog_key` and `editor::edit_key` insert `key_char` for Option+key, so Option dead keys do not compose in fields.
  Returning `None` there is not enough: every field key listener calls `cx.stop_propagation()` first (`shell/composer.rs:41`), and gpui-pre-macos only hands a key to the input method when no listener stopped it (`window.rs:2737-2739`).
  Plan A records this as "Not fixed" (a guard in 26 field listeners) and keeps typing the composed Option char in fields.
  The terminal is different: `TerminalView::key_down` leaves an `Ignore`d key alone (`terminal/view.rs:384`), and the window root stops only `/` (`main.rs:1007-1026`), so this plan's Option-to-input-method rule works there.
- **Plan A, `keymap.rs`.**
  Task 2 changes one import and one call in the test module of `keymap.rs` (`agent_keys_are_unbound_and_reach_the_pty`).
  Plan A's `Command::Copy` / `Command::Paste` macOS defaults (`cmd-c`, `cmd-v`) are what make Cmd+C / Cmd+V work: this plan relies on them and does not edit defaults.
  Plan A's SPEC §13 table row "Terminal | Copy / paste" is theirs to extend with the Cmd keys; Task 2 only edits the prose paragraph above the table.
- **Plan C, `main.rs`.**
  Task 2 adds one `cx.set_global(KeyEnv::current(..))` statement after the `ThemeNames` line in `main()`; if Plan C moves that block, keep the statement next to the other `set_global` calls that read `settings`.
- **`feat/polish` (ADR 0097).**
  It touches `crates/mkrew-ui` render code; the files here are mostly non-render (`terminal/input.rs`, `ime.rs`, `emulator.rs`, `layout.rs`), `terminal/view.rs` changes are small, and conflicts should be confined to import lines.

## Not fixed

Every finding of this slice has a task above or a line here.

| Finding | Decision and reason |
| --- | --- |
| Cmd+K (clear) and Cmd+A (select all) in the terminal (MacKeyboard 1) | Not done. Each needs a new `Command` variant, palette entry, SPEC row and emulator function on every OS, and the keymap defaults are Plan A's file. Ask the user. |
| Cmd+Left / Cmd+Backspace line editing in the terminal (not in the audit's fix list) | Not done. Terminal.app sends nothing for them by default, so any encoding is a policy choice. |
| Option+Left / Option+Right as `ESC b` / `ESC f` (follows from MacKeyboard 4) | Not done: they keep the xterm form `CSI 1;3D` that agent TUIs read; zsh's defaults bind only `ESC b` / `ESC f`. Ask the user. |
| Dead keys without Option on non-US layouts (MacKeyboard 8, French "^" or Spanish "´") | Not done. GPUI gives no dead-key signal, and sending every printable key through the input method would make a held key show the accent picker instead of repeating (`apple_press_and_hold_enabled` would need to be false first). Option dead keys (every US dead key) are done in Tasks 2 and 3. Check on a Mac. |
| "SF Mono" is dead in `TERMINAL_FONTS` (MacTextAndMouse 7) | No change: `all_font_names` is CoreText's list of all registered families (`gpui-pre-macos` `text_system.rs:102-134`), so "SF Mono" is used when installed and skipped when not (`pick_family`), and Menlo is always installed. [INFERENCE] Whether a stock macOS lists "SF Mono" was not run on a Mac. Recorded in ADR 0099. |
| `DIAGRAM_FONT` mac fonts (MacTextAndMouse 8) | No change: Helvetica Neue is a system font and is what the list resolves to on macOS. The audit's `-apple-system` would map to `sans-serif`, which `fontdb` 0.23 resolves to Arial (`mermaid-rs-renderer` `text_metrics.rs:95-97`, `fontdb` `lib.rs:173-182`), a worse result. |
| Default font size 14 for macOS (MacTextAndMouse 10) | Not done. It is a taste change to SPEC §14 for one OS, with no failing behavior. |
| Rounding of `DIFF_FONT_SIZE` and `code_line_height` (MacTextAndMouse 11) | Not done. Those rows are laid out by GPUI's list and flex layout, and nothing shows they alternate in height. Look again after the Mac visual check of Task 6. |
| Horizontal trackpad scroll in a terminal (MacTextAndMouse 16c) | Not done. No hosted program uses it. |
| Middle click closes a file tab (MacTextAndMouse 14) | Decided: keep as it is. A trackpad has three visible ways: the x on each tab (always on the active tab, on hover for the others), the right-click or two-finger-click menu (Close, Close others, Close to the right) and the Close file tab key. An extra Cmd+click alias would be a second hidden gesture. |
| Unmodified-key text the IME already committed (the "vieetj" case, MacTextAndMouse 21) | Partly done: ranges inside the composing text work (Task 3). Text the IME committed earlier belongs to the shell's line editor, and the PTY has no text model to offer it back. [INFERENCE] macOS's built-in Telex uses marked text, so it may not need it. A HUMAN_CHECKS line asks for a test with the real input methods. |
| IME candidate window position in the terminal body (MacTextAndMouse 21) | Verified, no change: `Typing.caret` is the cursor cell (`TerminalElement::paint`). The window opens a few cells right of the start of the composing text because that text is typed in place. Check on a Mac. |
| `Ctrl+Space` in the terminal is the macOS input source switch (MacKeyboard 13) | Not changed, only documented in ADR 0099: it is a macOS setting. |
| Ctrl+click as a right click (MacKeyboard 14) | Wrong finding (the lead verified GPUI maps it, `window.rs:2813-2831`). No plan. |

## Open questions for the user

1. Do you want Cmd+K (clear) and Cmd+A (select all) in the terminal?
   They are new commands on every OS, so they need Plan A to add the `Command` variants and defaults first.
2. Should Option+Left / Option+Right send `ESC b` / `ESC f` on macOS (what Terminal.app does)?
   Today they send `CSI 1;3D` / `CSI 1;3C` with the Option+key rule above.
3. Task 4, 5 and 6 change Windows and Linux too: double-click and triple-click selection, gentler pixel scrolling for programs, and a whole-device-pixel cell (the Windows grid cell changes by less than one device pixel in each direction).
   Say so if you want any of them macOS only; each is one branch on `Os::CURRENT` at one call site.
