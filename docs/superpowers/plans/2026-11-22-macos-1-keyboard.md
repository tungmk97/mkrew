# macOS keyboard (Plan A of 3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every shortcut, shortcut label and text-field key behave the way a MacBook user expects, without changing anything on Windows or Linux.

**Architecture:** One `Os` enum in `crates/mkrew-ui/src/keymap.rs` (Windows, Mac, Linux; `Os::CURRENT` from `cfg!`) drives three pure, unit-tested choices: the default keystroke of each `Command` (`default_keys_for`), how a keystroke is written in the UI (`keys_label_for`), and what a key means in a text field or the Editor (`dialog_key_for`, `edit_key_for`).
Every function takes the `Os` explicitly, so the macOS behavior is tested on the Windows dev machine; the existing functions (`default_keys`, `keys_label`, `dialog_key`, `edit_key`) keep their names and call the `_for` variant with `Os::CURRENT`, so no caller changes.
Fixed UI text that names a key uses one `os_text!(pc, mac)` macro, which picks a `&'static str` at compile time.

**Tech Stack:** Rust 2024, GPUI (`gpui-pre 0.3.7`, `Keystroke`, `Modifiers`), `cargo test`, GitHub Actions.

**Spec:** `docs/SPEC.md` §13 (keyboard shortcuts, text-field keys), §10 "Open in File Explorer"; this plan writes `docs/decisions/0098-macos-keyboard.md` (Task 3) and amends §13 in the same commits as the code.
Plans B (`2026-11-22-macos-2-terminal.md`) and C (`2026-11-22-macos-3-app.md`) build on `Os` from Task 2.

## Findings this plan fixes

|Finding (macOS audit 2026-11-21)|Task|
|---|---|
|cfg(macos) code is never compiled or linted by CI|1|
|Labels say "Ctrl", "Alt", "Win" instead of ⌃ ⌥ ⇧ ⌘|2|
|Defaults use Ctrl/Alt only, never Cmd (SPEC §13 already says "On macOS, Ctrl becomes Cmd")|3|
|13 defaults with Shift + digit / symbol never match on macOS (`ctrl-shift-1..7`, `alt-shift-1..5`, `alt-shift-0`, `alt-shift-=`, `alt-shift--`)|3|
|`alt-1..9` / `alt-0` take Option+digit, so `[ ] { } \| ~` cannot be typed on German/French layouts|3|
|`alt-arrows` take Option+←/→ (the macOS word jump) from terminals and fields|3|
|F11 = Show Desktop; F2, F8, Shift+F10, Shift+F4, Ctrl+F5, PgUp / PgDn need Fn on a MacBook|3|
|`Ctrl+Shift+Z` (Open in Zed) is Redo on a Mac|3|
|Text fields: Cmd+←/→ jumps words, Option+←/→ does nothing, Option+Backspace deletes one char, Cmd+Backspace deletes a word, Cmd+↑/↓ and Cmd+Shift+Z do nothing, Emacs Ctrl+A/E/K do nothing|4|
|Hard-coded "Ctrl+Enter", "Ctrl+O" (really Cmd+O), "Alt+Y", "Ctrl+N", "Ctrl+S", "Ctrl+Alt+Enter", "(Ctrl+,)" in hints and tooltips|5|
|Recorder error says "Use Ctrl, Alt or Win"|5|
|"Open in File Explorer" on a Mac (it opens Finder)|5|
|Replace all in a file is Ctrl+Option+Enter on a Mac|5|

Not in this plan: terminal Cmd / Option input (Plan B), the app menu with Cmd+Q / Cmd+H / Cmd+M (Plan C).
The MacKeyboard audit item "Ctrl+click is a left click" is wrong: `gpui-pre-macos-0.3.7/src/window.rs:2813-2831` already turns Ctrl+left-click into a right click; nothing to do.
Not fixed: Option dead keys in text fields (Option+E, then E, gives "´e", not "é").
GPUI hands a key to the macOS input method only when no key listener stopped it (`gpui-pre-macos-0.3.7/src/window.rs:2737-2739`), and every field's key listener calls `cx.stop_propagation()` first (for example `shell/composer.rs:41`).
Fixing it means a guard for Option keys at the top of the 26 field key listeners (`composer_key_down`, `prompt_key_down`, `settings_key_down`, …); ask the user before taking that on.
Option characters that are not dead keys (∫, €, German `[` / `@`) are typed, as today.

## Global Constraints

- Windows and Linux behavior and defaults stay byte-for-byte the same; every existing test passes unchanged on Windows.
- Every macOS default is unique among commands that can be live at once (`Command::shares_context`), like the PC defaults.
- No macOS default needs Shift with a digit or symbol, an F-key, PgUp / PgDn / Home / End / Insert, or Option alone with a character key.
- No global macOS default takes a key that a text field or the Editor uses (`dialog_key_for` / `edit_key_for` give it no meaning).
- macOS labels: modifier symbols in Apple order ⌃ ⌥ ⇧ ⌘, then the key, no `+` (`⇧⌘K`, `⌥⌘↩`); Enter ↩, Backspace ⌫, Delete ⌦, Tab ⇥, arrows ← → ↑ ↓.
- Prefer runtime `Os` branches in pure functions over `#[cfg(target_os = "macos")]`: the dev machine is Windows and cannot compile macOS-only code.
- No literal colors or pixel values outside `theme.rs`; no em dash in code comments or docs.
- Commits: Conventional Commits, `git -c user.email=tungnv975@gmail.com commit`, stage only the files the task lists.
- Verify before each commit: `cargo test -p mkrew-ui <filter>`; before the last commit of the plan: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`.

## Review Focus

1. A user whose synced `keymap.toml` holds PC keys starts Mkrew on a Mac: their entries still apply, and an entry that now clashes with a macOS default is reported and falls back to that default (test in Task 3).
2. Every macOS default becomes a GPUI key binding (none is dropped by `KeyBinding::load`) (test in Task 3).
3. Option+letter in a text field types the character macOS composed (`∫`, German Option+5 `[`), it is not swallowed (test in Task 4).
4. Cmd+Shift+←/→ and Option+Shift+←/→ extend the selection instead of moving the caret (test in Task 4).
5. A global macOS shortcut such as Cmd+Shift+Enter or Cmd+Option+Enter does not also act as Save or a new line in the focused field (test in Task 4).

---

### Task 1: Compile and lint macOS code in CI

**Files:**
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: nothing.
- Produces: a `macos` job that fails when `cfg(target_os = "macos")` code does not compile or has a clippy warning. Plans B and C rely on it to check their macOS-only code.

The PC test suite pins PC keys (`Keymap::defaults()` with `ctrl-` expectations), so the macOS job lints and compiles every target but does not run the tests; macOS behavior is tested on Windows through the `_for(Os::Mac)` functions.

- [ ] **Step 1: Add the job**

Append to `.github/workflows/ci.yml`, after the `verify` job (same indentation as `verify:`):

```yaml
  # macOS: compile and lint every target, so `cfg(target_os = "macos")` code is checked.
  # The tests pin the PC defaults; macOS key behavior is tested on Windows through the
  # `*_for(Os::Mac)` functions (ADR 0098).
  macos:
    name: clippy (macos-latest)
    runs-on: macos-latest
    timeout-minutes: 60
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --workspace --all-targets -- -D warnings
```

Also change the first comment line of the file from `# SPEC §17: fmt check, clippy -D warnings and all tests on Windows + Linux.` to:

```yaml
# SPEC §17: fmt check, clippy -D warnings and all tests on Windows + Linux; clippy on macOS.
```

- [ ] **Step 2: Check the YAML parses**

Run: `git diff --stat .github/workflows/ci.yml` and read the diff once.
Expected: only the comment line and the new `macos` job; two-space indentation like `verify`.
The job itself runs when the human pushes; if it reports clippy warnings in macOS-only code, fix them in a follow-up commit `fix(ui): clippy on macOS` before Plan B starts.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git -c user.email=tungnv975@gmail.com commit -m "ci: clippy on macos-latest"
```

---

### Task 2: `Os` and macOS shortcut labels

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs` (new `Os` near the top; `keys_label` at :803-806; `Keymap::label` at :1013-1020; `fn label` at :1051-1081; tests module)

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub enum Os { Windows, Mac, Linux }` with `pub const CURRENT: Os`.
  - `pub fn keys_label_for(keys: &str, os: Os) -> String`.
  - `pub fn keys_label(keys: &str) -> String` (unchanged signature, now `keys_label_for(keys, Os::CURRENT)`).
  - private `fn label(keystroke: &Keystroke, os: Os) -> String` (Task 3 tests call it).

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `keymap.rs`:

```rust
    #[test]
    fn mac_labels_use_the_modifier_symbols_in_apple_order() {
        let mac = |keys| keys_label_for(keys, Os::Mac);
        assert_eq!(mac("cmd-shift-k"), "⇧⌘K");
        assert_eq!(mac("ctrl-alt-shift-cmd-left"), "⌃⌥⇧⌘←");
        assert_eq!(mac("cmd-alt-enter"), "⌥⌘↩");
        assert_eq!(mac("cmd-enter"), "⌘↩");
        assert_eq!(mac("cmd-backspace"), "⌘⌫");
        assert_eq!(mac("alt-delete"), "⌥⌦");
        assert_eq!(mac("ctrl-tab"), "⌃⇥");
        assert_eq!(mac("cmd-,"), "⌘,");
        assert_eq!(mac("alt-y"), "⌥Y");
        assert_eq!(mac("escape"), "Escape");
        assert_eq!(mac("nonsense key"), "nonsense key");
    }

    #[test]
    fn pc_labels_name_the_platform_key_per_os() {
        assert_eq!(keys_label_for("ctrl-shift-k", Os::Windows), "Ctrl+Shift+K");
        assert_eq!(keys_label_for("ctrl-enter", Os::Windows), "Ctrl+Enter");
        assert_eq!(keys_label_for("win-k", Os::Windows), "Win+K");
        assert_eq!(keys_label_for("super-k", Os::Linux), "Super+K");
        assert_eq!(keys_label_for("alt-shift-pageup", Os::Linux), "Alt+Shift+PgUp");
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p mkrew-ui --lib keymap::tests::mac_labels`
Expected: compile error `cannot find function keys_label_for` / `cannot find type Os`.

- [ ] **Step 3: Add `Os`**

In `keymap.rs`, right after the `use` lines (before `/// Something a shortcut does.`), add:

```rust
/// The OS whose conventions apply: default shortcuts, shortcut text and text-field keys
/// (SPEC §13, ADR 0098). Functions take it explicitly so the macOS rules are tested on any OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    Mac,
    Linux,
}

impl Os {
    /// The OS this build runs on.
    pub const CURRENT: Os = if cfg!(target_os = "macos") {
        Os::Mac
    } else if cfg!(windows) {
        Os::Windows
    } else {
        Os::Linux
    };
}
```

- [ ] **Step 4: Make the labels take the OS**

Replace `keys_label` (:803-806) with:

```rust
/// `keys` as this OS's shortcut text ("Ctrl+Shift+K", "⇧⌘K" on macOS); the raw text when it is
/// not a keystroke.
pub fn keys_label(keys: &str) -> String {
    keys_label_for(keys, Os::CURRENT)
}

/// [`keys_label`] as `os` writes it.
pub fn keys_label_for(keys: &str, os: Os) -> String {
    parse_keys(keys).map_or_else(|| keys.to_owned(), |keystroke| label(&keystroke, os))
}
```

In `Keymap::label` (:1013-1020) change the doc to `/// Shortcut text for tooltips and menus, e.g. "Ctrl+Shift+X" ("⇧⌘X" on macOS); empty if unbound.` and the map line to:

```rust
            .map(|(_, keystroke)| label(keystroke, Os::CURRENT))
```

Replace `fn label` (:1051-1081) with:

```rust
/// Shortcut text: "Ctrl+Alt+Shift+Win+K" (Super on Linux); on macOS the modifier symbols in
/// Apple's order and no separator ("⌃⌥⇧⌘K").
fn label(keystroke: &Keystroke, os: Os) -> String {
    let m = keystroke.modifiers;
    let key = key_name(&keystroke.key, os);
    if os == Os::Mac {
        let mut text: String = [
            (m.control, '⌃'),
            (m.alt, '⌥'),
            (m.shift, '⇧'),
            (m.platform, '⌘'),
        ]
        .into_iter()
        .filter_map(|(on, symbol)| on.then_some(symbol))
        .collect();
        text.push_str(&key);
        return text;
    }
    let platform = if os == Os::Linux { "Super" } else { "Win" };
    let mut parts: Vec<String> = [
        (m.control, "Ctrl"),
        (m.alt, "Alt"),
        (m.shift, "Shift"),
        (m.platform, platform),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then(|| name.to_owned()))
    .collect();
    parts.push(key);
    parts.join("+")
}

/// A key in shortcut text: arrows as ← → ↑ ↓, PgUp / PgDn, macOS's ↩ ⌫ ⌦ ⇥, else the key
/// with its first letter capitalized.
fn key_name(key: &str, os: Os) -> String {
    let mac = os == Os::Mac;
    let name = match key {
        "left" => "←",
        "right" => "→",
        "up" => "↑",
        "down" => "↓",
        "pageup" => "PgUp",
        "pagedown" => "PgDn",
        "enter" if mac => "↩",
        "backspace" if mac => "⌫",
        "delete" if mac => "⌦",
        "tab" if mac => "⇥",
        key => {
            let mut chars = key.chars();
            return chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default();
        }
    };
    name.to_owned()
}
```

- [ ] **Step 5: Run the keymap tests**

Run: `cargo test -p mkrew-ui --lib keymap::`
Expected: PASS, including the old `keys_label_matches_the_shortcut_text` and every label assertion on PC keys.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/keymap.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): macOS shortcut labels with modifier symbols"
```

---

### Task 3: macOS default shortcuts (+ ADR 0098, SPEC §13, README)

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs` (`Command::default_keys` :478-604; `Keymap::defaults` :847-854; `Keymap::parse` :873-923; `template` :663-681; `notice` :808-817; tests)
- Create: `docs/decisions/0098-macos-keyboard.md`
- Modify: `docs/SPEC.md` §13 (line 1448 and after the table ending at line 1535)
- Modify: `README.md` (usage list around lines 160-190)
- Modify: `docs/HUMAN_CHECKS.md` ("To verify")

**Interfaces:**
- Consumes: `Os`, `label(&Keystroke, Os)` from Task 2.
- Produces:
  - `pub fn default_keys_for(self, os: Os) -> String` on `Command` (`default_keys()` = `default_keys_for(Os::CURRENT)`).
  - `pub fn defaults_for(os: Os) -> Keymap` and `pub fn parse_for(text: &str, os: Os) -> (Keymap, Vec<KeymapError>)` on `Keymap` (`defaults()` / `parse()` call them with `Os::CURRENT`).
  - Plan C's app menu reuses these macOS defaults: OpenSettings `cmd-,`, ToggleFullScreen `ctrl-cmd-f`, Copy `cmd-c`, Paste `cmd-v`. Nothing here binds `cmd-q`, `cmd-h`, `alt-cmd-h`, `cmd-m` or `` cmd-` ``.

The macOS table (only the differences; every other command keeps its PC default on macOS):

|Command|PC default|macOS default|
|---|---|---|
|AddWorkspace|ctrl-shift-k|cmd-shift-k|
|Workspace(n)|alt-n|cmd-n|
|NewShellTab|ctrl-shift-t|cmd-t|
|NewAgentTab|ctrl-shift-a|cmd-shift-a|
|RenameTab|ctrl-shift-r|cmd-shift-r|
|CloseTab|ctrl-shift-w|cmd-shift-w|
|SplitRight / SplitDown|alt-shift-= / alt-shift--|cmd-d / cmd-shift-d|
|SplitAgent / SplitShell|alt-shift-a / alt-shift-s|cmd-alt-a / cmd-alt-s|
|Layout 1..5 / EqualizePanes|alt-shift-1..5 / alt-shift-0|cmd-alt-1..5 / cmd-alt-0|
|Focus ← → ↑ ↓|alt-arrows|cmd-alt-arrows|
|Resize ← → ↑ ↓|alt-shift-arrows|cmd-alt-shift-arrows|
|ZoomPane / ClosePane|ctrl-shift-m / ctrl-shift-x|cmd-shift-m / cmd-shift-x|
|RenamePane|f2|cmd-alt-r|
|Copy / Paste (terminal)|ctrl-shift-c / ctrl-shift-v|cmd-c / cmd-v|
|PageUp / PageDown (terminal)|shift-pageup / shift-pagedown|cmd-up / cmd-down|
|Find|ctrl-shift-f|cmd-f|
|OpenInZed|ctrl-shift-z|cmd-alt-e|
|NextInbox|ctrl-shift-i|cmd-shift-i|
|Peek|ctrl-shift-space|cmd-shift-space|
|OpenComposer|ctrl-shift-enter|cmd-shift-enter|
|SendSelection|ctrl-shift-l|cmd-shift-l|
|CopyMode / LinkHints|alt-shift-c / alt-shift-o|cmd-alt-c / cmd-alt-o|
|ShowTerminals, Board, FileTree, GitPanel, Docs, Editor, Diff|ctrl-shift-1..7|ctrl-cmd-1..7|
|ToggleDocSource|ctrl-shift-e|cmd-shift-e|
|SendComments|ctrl-shift-s|cmd-shift-s|
|ToggleIgnored|ctrl-shift-h|cmd-shift-h|
|SaveFile / FindInFile / ReplaceInFile / CloseFileTab|ctrl-s / ctrl-f / ctrl-h / ctrl-w|cmd-s / cmd-shift-f / cmd-alt-f / cmd-w|
|ToggleFilePreview|ctrl-shift-v|cmd-shift-v|
|QuickOpen / NewTask / RunTask|ctrl-shift-o / ctrl-shift-n / ctrl-enter|cmd-p / cmd-n / cmd-enter|
|RunStart / RunRestart / RunStop|ctrl-f5 / ctrl-shift-f5 / ctrl-alt-f5|cmd-r / ctrl-cmd-r / cmd-.|
|CommandPalette / OpenSettings|ctrl-shift-p / ctrl-,|cmd-shift-p / cmd-,|
|ToggleTheme|ctrl-shift-j|cmd-shift-j|
|ToggleFullScreen|f11|ctrl-cmd-f|
|ToggleSidebar / ToggleInspector|ctrl-shift-b / ctrl-shift-y|cmd-b / cmd-shift-y|
|VoiceInput / AskAi|ctrl-shift-d / ctrl-shift-g|ctrl-cmd-v / cmd-shift-g|
|ContextMenu|shift-f10|cmd-alt-enter|
|FocusSidebar|alt-0|cmd-0|
|NextComment / PreviousComment|f8 / shift-f8|cmd-] / cmd-[|
|MoveTabLeft / MoveTabRight|ctrl-shift-pageup / ctrl-shift-pagedown|ctrl-cmd-left / ctrl-cmd-right|
|MoveTabToNewWindow|shift-f4|cmd-shift-n|

Unchanged on macOS: NextWorkspace / PreviousWorkspace (`ctrl-alt-down` / `ctrl-alt-up`), NextTab / PreviousTab (`ctrl-tab` / `ctrl-shift-tab`), Tab(n) (`ctrl-alt-n`), PreviousInbox (`ctrl-alt-i`), AddComment (`ctrl-alt-m`; Cmd+Option+M minimizes all windows), ReplyComment (`ctrl-alt-r`), ResolveComment (`ctrl-alt-enter`), SelectTheme (`ctrl-alt-j`), ToggleCompactSidebar (`ctrl-alt-b`), ToggleFileSplit (`ctrl-alt-v`), PickDown / PickUp, every Git single key, every Card key, ApproveCard, GitToggleWrap, OpenKept (`alt-enter`, file tree only; Enter types nothing).
Ctrl+Option keys type nothing on a Mac and are not system shortcuts, so they stay.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `keymap.rs`:

```rust
    #[test]
    fn every_mac_default_parses_and_is_unique() {
        let keymap = Keymap::defaults_for(Os::Mac);
        assert_eq!(keymap.bindings().len(), Command::all().len());
        for (command, keys) in keymap.bindings() {
            for (other, bound) in keymap.bindings() {
                assert!(
                    other == command || !same_keys(keys, bound) || !command.shares_context(*other),
                    "{command:?} and {other:?} share {keys:?}"
                );
            }
        }
    }

    /// macOS reports Shift + digit / symbol as the shifted char (gpui-pre-macos events.rs), so
    /// such a binding never matches; F-keys and PgUp / PgDn / Home / End need Fn on a MacBook;
    /// Option + a character key types a character on a Mac.
    #[test]
    fn mac_defaults_are_keys_a_macbook_presses_without_typing() {
        for (command, keys) in Keymap::defaults_for(Os::Mac).bindings() {
            let m = keys.modifiers;
            let one_char = keys.key.chars().count() == 1;
            let letter = keys.key.chars().all(|c| c.is_ascii_alphabetic());
            assert!(
                !(m.shift && one_char && !letter),
                "{command:?}: Shift with {:?} never matches on macOS",
                keys.key
            );
            let function = (keys.key.strip_prefix('f'))
                .is_some_and(|n| n.parse::<u8>().is_ok());
            assert!(
                !function
                    && !matches!(
                        keys.key.as_str(),
                        "pageup" | "pagedown" | "home" | "end" | "insert"
                    ),
                "{command:?}: {:?} needs Fn on a MacBook",
                keys.key
            );
            assert!(
                !(m.alt && !m.control && !m.platform && one_char),
                "{command:?}: Option+{:?} types a character on a Mac",
                keys.key
            );
        }
    }

    #[test]
    fn every_mac_binding_becomes_a_gpui_key_binding() {
        let keymap = Keymap::defaults_for(Os::Mac);
        let bindings = keymap.key_bindings(&gpui::DummyKeyboardMapper);
        assert_eq!(
            bindings.len(),
            Command::all().len() + keymap.menu_shielded().count()
        );
    }

    #[test]
    fn a_pc_keymap_file_on_a_mac_keeps_its_keys_and_reports_clashes() {
        let (keymap, errors) = Keymap::parse_for(
            "close_pane = \"ctrl-shift-q\"\nzoom_pane = \"cmd-shift-k\"\n",
            Os::Mac,
        );
        let bound = |command| keymap.keystroke(command).unwrap();
        assert!(same_keys(bound(Command::ClosePane), &keystroke("ctrl-shift-q")));
        assert!(
            same_keys(bound(Command::ZoomPane), &keystroke("cmd-shift-m")),
            "the clash keeps the macOS default"
        );
        assert_eq!(
            errors,
            [KeymapError::Duplicate {
                command: "zoom_pane".into(),
                keys: "cmd-shift-k".into(),
                other: "add_workspace".into(),
            }]
        );
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p mkrew-ui --lib keymap::tests::every_mac_default`
Expected: compile error `no function or associated item named defaults_for`.

- [ ] **Step 3: Split the defaults by OS**

In `impl Command`, replace the head of `default_keys` (:478-480, `/// SPEC §13 default, in GPUI keystroke syntax.` through `match self {`) so the existing big `match` becomes `pc_keys`:

```rust
    /// SPEC §13 default for this OS, in GPUI keystroke syntax.
    pub fn default_keys(self) -> String {
        self.default_keys_for(Os::CURRENT)
    }

    /// SPEC §13 default on `os`: the macOS table where it differs, else the PC key.
    pub fn default_keys_for(self, os: Os) -> String {
        match os {
            Os::Mac => self.mac_keys().unwrap_or_else(|| self.pc_keys()),
            Os::Windows | Os::Linux => self.pc_keys(),
        }
    }

    /// The Windows / Linux default.
    fn pc_keys(self) -> String {
        match self {
```

(The arms from `Command::AddWorkspace => "ctrl-shift-k".into(),` to `Command::OpenKept => "alt-enter".into(),` and the closing braces stay exactly as they are.)

Right after `pc_keys`, add:

```rust
    /// macOS defaults where they differ from [`Command::pc_keys`] (SPEC §13 "macOS defaults",
    /// ADR 0098): Cmd for Ctrl; never Option + a character key (it types characters), Shift +
    /// a digit or symbol (macOS reports the shifted char, so the binding never matches), an
    /// F-key or PgUp / PgDn (Fn on a MacBook). `None` = the PC key.
    fn mac_keys(self) -> Option<String> {
        let keys = match self {
            Command::AddWorkspace => "cmd-shift-k",
            Command::Workspace(n) => return Some(format!("cmd-{n}")),
            Command::NewShellTab => "cmd-t",
            Command::NewAgentTab => "cmd-shift-a",
            Command::RenameTab => "cmd-shift-r",
            Command::CloseTab => "cmd-shift-w",
            Command::SplitRight => "cmd-d",
            Command::SplitDown => "cmd-shift-d",
            Command::SplitAgent => "cmd-alt-a",
            Command::SplitShell => "cmd-alt-s",
            Command::Layout(preset) => match preset {
                Preset::Columns2 => "cmd-alt-1",
                Preset::Rows2 => "cmd-alt-2",
                Preset::Columns3 => "cmd-alt-3",
                Preset::MainAndStack => "cmd-alt-4",
                Preset::Grid => "cmd-alt-5",
            },
            Command::EqualizePanes => "cmd-alt-0",
            Command::Focus(Direction::Left) => "cmd-alt-left",
            Command::Focus(Direction::Right) => "cmd-alt-right",
            Command::Focus(Direction::Up) => "cmd-alt-up",
            Command::Focus(Direction::Down) => "cmd-alt-down",
            Command::Resize(Direction::Left) => "cmd-alt-shift-left",
            Command::Resize(Direction::Right) => "cmd-alt-shift-right",
            Command::Resize(Direction::Up) => "cmd-alt-shift-up",
            Command::Resize(Direction::Down) => "cmd-alt-shift-down",
            Command::ZoomPane => "cmd-shift-m",
            Command::ClosePane => "cmd-shift-x",
            Command::RenamePane => "cmd-alt-r",
            Command::Copy => "cmd-c",
            Command::Paste => "cmd-v",
            Command::PageUp => "cmd-up",
            Command::PageDown => "cmd-down",
            Command::Find => "cmd-f",
            Command::OpenInZed => "cmd-alt-e",
            Command::NextInbox => "cmd-shift-i",
            Command::Peek => "cmd-shift-space",
            Command::OpenComposer => "cmd-shift-enter",
            Command::SendSelection => "cmd-shift-l",
            Command::CopyMode => "cmd-alt-c",
            Command::LinkHints => "cmd-alt-o",
            Command::ShowTerminals => "ctrl-cmd-1",
            Command::ShowBoard => "ctrl-cmd-2",
            Command::ShowFileTree => "ctrl-cmd-3",
            Command::ShowGitPanel => "ctrl-cmd-4",
            Command::ShowDocs => "ctrl-cmd-5",
            Command::ShowEditor => "ctrl-cmd-6",
            Command::ShowDiff => "ctrl-cmd-7",
            Command::ToggleDocSource => "cmd-shift-e",
            Command::SendComments => "cmd-shift-s",
            Command::ToggleIgnored => "cmd-shift-h",
            Command::SaveFile => "cmd-s",
            Command::FindInFile => "cmd-shift-f",
            Command::ReplaceInFile => "cmd-alt-f",
            Command::CloseFileTab => "cmd-w",
            Command::ToggleFilePreview => "cmd-shift-v",
            Command::QuickOpen => "cmd-p",
            Command::NewTask => "cmd-n",
            Command::RunTask => "cmd-enter",
            Command::RunStart => "cmd-r",
            Command::RunRestart => "ctrl-cmd-r",
            Command::RunStop => "cmd-.",
            Command::CommandPalette => "cmd-shift-p",
            Command::OpenSettings => "cmd-,",
            Command::ToggleTheme => "cmd-shift-j",
            Command::ToggleFullScreen => "ctrl-cmd-f",
            Command::ToggleSidebar => "cmd-b",
            Command::ToggleInspector => "cmd-shift-y",
            Command::VoiceInput => "ctrl-cmd-v",
            Command::AskAi => "cmd-shift-g",
            Command::ContextMenu => "cmd-alt-enter",
            Command::FocusSidebar => "cmd-0",
            Command::NextComment => "cmd-]",
            Command::PreviousComment => "cmd-[",
            Command::MoveTabLeft => "ctrl-cmd-left",
            Command::MoveTabRight => "ctrl-cmd-right",
            Command::MoveTabToNewWindow => "cmd-shift-n",
            _ => return None,
        };
        Some(keys.into())
    }
```

- [ ] **Step 4: Thread the OS through `defaults` and `parse`**

Replace `Keymap::defaults` (:847-854) with:

```rust
    /// The SPEC §13 defaults of this OS.
    pub fn defaults() -> Keymap {
        Keymap::defaults_for(Os::CURRENT)
    }

    /// The SPEC §13 defaults of `os`.
    pub fn defaults_for(os: Os) -> Keymap {
        let bindings = Command::all()
            .into_iter()
            .filter_map(|command| {
                Some((command, Keystroke::parse(&command.default_keys_for(os)).ok()?))
            })
            .collect();
        Keymap { bindings }
    }
```

Replace the head of `Keymap::parse` (:873-875):

```rust
    /// This OS's defaults overridden by the entries of a `keymap.toml`.
    pub fn parse(text: &str) -> (Keymap, Vec<KeymapError>) {
        Keymap::parse_for(text, Os::CURRENT)
    }

    /// [`Keymap::parse`] over `os`'s defaults.
    pub fn parse_for(text: &str, os: Os) -> (Keymap, Vec<KeymapError>) {
        let mut keymap = Keymap::defaults_for(os);
```

and inside the duplicate loop of the same function change `Keystroke::parse(&command.default_keys())` to `Keystroke::parse(&command.default_keys_for(os))`.

- [ ] **Step 5: Name this OS's keys in `template` and `notice`**

Replace the `let mut text = String::from(` … `);` block of `template` (:666-672) with:

```rust
    let settings = keys_label(&Command::OpenSettings.default_keys());
    let palette = keys_label(&Command::CommandPalette.default_keys());
    let mut text = format!(
        "# Mkrew keyboard shortcuts. The Settings dialog ({settings}) writes this file; a hand edit\n\
         # applies after a restart. Uncomment a line and change its keys.\n\
         # Keys use GPUI syntax: modifiers ctrl, alt, shift and cmd (macOS) joined by `-`, e.g. \"ctrl-shift-k\".\n\
         # A keystroke bound twice, an unknown command or an invalid keystroke is listed in the\n\
         # command palette ({palette}) and that entry keeps its default.\n\n"
    );
```

Replace the body of `notice` (:809-817) with:

```rust
pub fn notice(errors: &[KeymapError]) -> Option<String> {
    let settings = keys_label(&Command::OpenSettings.default_keys());
    errors.first().map(|first| match errors.len() {
        1 => format!("{first}; using the default. Fix it in Settings ({settings})."),
        n => format!(
            "{first} (+{} more); using the defaults. Fix them in Settings ({settings}).",
            n - 1
        ),
    })
}
```

- [ ] **Step 6: Run the keymap tests**

Run: `cargo test -p mkrew-ui --lib keymap::`
Expected: PASS (new macOS tests and every existing PC test).
If `every_mac_default_parses_and_is_unique` names two commands, the table above has a clash: fix the macOS key of the newer command and update the table in this plan, the SPEC and the ADR to match.

- [ ] **Step 7: Write ADR 0098**

Create `docs/decisions/0098-macos-keyboard.md`:

```markdown
# 0098 - macOS keyboard: Cmd defaults, symbol labels, Mac text keys

- Date: 2026-11-22
- Status: accepted (amends SPEC §13)
- Milestone: none (human request 2026-11-22, macOS audit 2026-11-21)

## Context

SPEC §13 said "On macOS, `Ctrl` becomes `Cmd`", but the code had one set of defaults, all Ctrl and Alt.
On a Mac, Option types characters, so `alt-1..9` and `alt-arrows` took `[ ] { } | ~` and the word jump away from every field and terminal.
macOS reports Shift + digit or symbol as the shifted character and gpui-pre-macos does not fold it back, so 13 defaults such as `ctrl-shift-2` never matched.
A MacBook needs Fn for F-keys and PgUp / PgDn, F11 is Show Desktop, and Ctrl + arrows switch Spaces.
Labels said "Ctrl", "Alt" and "Win", and text fields used Windows caret keys.

## Decision

- `keymap::Os` (Windows, Mac, Linux) is passed explicitly to the pure functions that choose defaults, labels and text keys, so the macOS rules are unit-tested on Windows.
- macOS defaults: Cmd takes Ctrl's place; no default uses Option + a character key, Shift + a digit or symbol, an F-key or PgUp / PgDn / Home / End; Ctrl+Option keys stay because they type nothing and are not system shortcuts. The table is in SPEC §13 "macOS defaults".
- macOS shortcut text uses ⌃ ⌥ ⇧ ⌘ in Apple order with no separator, and ↩ ⌫ ⌦ ⇥ for those keys.
- macOS text fields and the Editor: Option moves and deletes by word, Cmd goes to the line's or text's ends and Cmd+Backspace / Cmd+Delete delete to them, Cmd+Shift+Z redoes, and the Emacs keys Ctrl+A E B F P N D H K work.
- On macOS an Option + character key types the character macOS composed (Option+B is "∫", German Option+5 is "["), as today.
- CI compiles and lints every target on macOS; it does not run the tests there, because they pin the PC defaults.

## Alternatives

- One set of defaults with `secondary-` (Ctrl on PC, Cmd on Mac): fixes Ctrl but keeps the Option, Shift + digit and Fn problems.
- Fold Shift + digit into the shifted char on macOS like gpui-pre-windows does: depends on the layout and shows "⌘@" instead of a digit.
- `#[cfg(target_os = "macos")]` defaults: cannot be compiled or tested on the Windows dev machine.

## Consequences

A `keymap.toml` written on Windows keeps its keys on a Mac; an entry that clashes with a macOS default is reported and that entry keeps its default.
Existing macOS users get new defaults at the next start; their uncommented entries still win.
```

- [ ] **Step 8: Amend SPEC §13**

Replace line 1448 of `docs/SPEC.md` (`On macOS, \`Ctrl\` becomes \`Cmd\` for global and tab commands.`) with:

```markdown
On macOS the defaults use Cmd where the table says Ctrl, and never need Option with a character key (Option types characters on a Mac), Shift with a digit or symbol (macOS reports the shifted character, so such a binding never matches), an F-key or PgUp / PgDn (Fn on a MacBook); "macOS defaults" below lists every difference (ADR 0098).
Shortcut text on macOS uses ⌃ ⌥ ⇧ ⌘ in Apple's order with no separator (⇧⌘P).
```

After the table (after its last row, `| Task / Workspace | Merge back …`, and before `In the Editor, and while the Git list …`), insert a blank line and:

```markdown
**macOS defaults** (ADR 0098); a command not listed keeps the key of the table above:

| Action | Default on macOS |
| --- | --- |
| Add workspace / switch to workspace 1-9 / focus the sidebar | ⇧⌘K / ⌘1-9 / ⌘0 |
| New shell tab / new agent tab / rename tab / close tab | ⌘T / ⇧⌘A / ⇧⌘R / ⇧⌘W |
| Split right / split down / split with agent / split with shell | ⌘D / ⇧⌘D / ⌥⌘A / ⌥⌘S |
| Layouts 1-5 / equalize panes | ⌥⌘1-5 / ⌥⌘0 |
| Focus pane ← → ↑ ↓ / resize pane | ⌥⌘ arrows / ⌥⇧⌘ arrows |
| Zoom pane / close pane / rename pane | ⇧⌘M / ⇧⌘X / ⌥⌘R |
| Terminal copy / paste / page up / page down / copy mode / link hints | ⌘C / ⌘V / ⌘↑ / ⌘↓ / ⌥⌘C / ⌥⌘O |
| Find / Open in Zed / next Inbox item / peek | ⌘F / ⌥⌘E / ⇧⌘I / ⇧⌘Space |
| Composer / send selection / Ask AI / voice input | ⇧⌘↩ / ⇧⌘L / ⇧⌘G / ⌃⌘V |
| Terminals, Board, File tree, Git, Docs, Editor, Diff | ⌃⌘1 … ⌃⌘7 |
| Docs source / send comments / next / previous comment | ⇧⌘E / ⇧⌘S / ⌘] / ⌘[ |
| File tree: show ignored | ⇧⌘H |
| Editor: save / find / replace / close tab / preview | ⌘S / ⇧⌘F / ⌥⌘F / ⌘W / ⇧⌘V |
| Quick open / new task / run task (Board) | ⌘P / ⌘N / ⌘↩ |
| Run start / restart / stop | ⌘R / ⌃⌘R / ⌘. |
| Command palette / settings / toggle theme / full screen | ⇧⌘P / ⌘, / ⇧⌘J / ⌃⌘F |
| Sidebar / right panel / context menu | ⌘B / ⇧⌘Y / ⌥⌘↩ |
| Move tab left / right / to a new window | ⌃⌘← / ⌃⌘→ / ⇧⌘N |
```

- [ ] **Step 9: README note**

In `README.md`, after the usage bullet that starts `- **Command palette** \`Ctrl+Shift+P\``, add:

```markdown
- **macOS**: the shortcuts use Cmd (⇧⌘P for the palette, ⌘, for settings); Settings → Keyboard shows every key, and SPEC §13 "macOS defaults" lists them.
```

- [ ] **Step 10: HUMAN_CHECKS**

In `docs/HUMAN_CHECKS.md`, under `## To verify (non-blocking)` and its italic intro, add:

```markdown
### To verify - 2026-11-22 macOS keyboard (ADR 0098)

- [ ] **On a Mac (US layout):** ⇧⌘P opens the palette, ⌃⌘2 shows the Board, ⌥⌘← moves pane focus, ⌘D splits, ⌘1 switches workspace; Settings → Keyboard shows ⌘ symbols.
- [ ] **On a Mac (German layout):** Option+5 / Option+6 type `[` / `]` in a terminal and in the composer; ⌘] / ⌘[ step through comments.
- [ ] **Look (dark + light) on a Mac:** tooltips and the palette show ⇧⌘P style shortcuts, never "Ctrl".
```

- [ ] **Step 11: Commit**

```bash
git add crates/mkrew-ui/src/keymap.rs docs/decisions/0098-macos-keyboard.md docs/SPEC.md README.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): macOS default shortcuts use Cmd and avoid Option, Shift+digit and Fn keys"
```

---

### Task 4: macOS text-field and Editor keys

**Files:**
- Modify: `crates/mkrew-ui/src/editor.rs` (`EditKey` :44-67; `Editor::key` :311-339; new `delete_line` after `delete` :363-376; `edit_key` :603-646; tests :906-945)
- Modify: `crates/mkrew-ui/src/task_dialog.rs` (`DialogKey` :32-67; `dialog_key` :69-111; tests :459-465, :528-559)
- Modify: `crates/mkrew-ui/src/text_box.rs` (new `Input::delete_line` after `delete_word_after` :224-231; `Input::edit` :371-390; tests)
- Modify: `crates/mkrew-ui/src/keymap.rs` (tests only)
- Modify: `docs/SPEC.md` §13 text-field paragraph (lines 1437-1445)

**Interfaces:**
- Consumes: `Os` (Task 2), `Keymap::defaults_for` (Task 3).
- Produces:
  - `pub fn dialog_key_for(keystroke: &Keystroke, os: Os) -> Option<DialogKey>` (`dialog_key` = `dialog_key_for(k, Os::CURRENT)`).
  - `pub fn edit_key_for(keystroke: &Keystroke, os: Os) -> Option<EditKey>` (`edit_key` = `edit_key_for(k, Os::CURRENT)`).
  - `DialogKey::DeleteLine { forward: bool }`, `EditKey::DeleteLine { forward: bool }`: delete the selection, else from the caret to its line's start (`false`) or end (`true`).
  - `pub fn Input::delete_line(&mut self, forward: bool) -> bool`.
  - `pub(crate) fn editor::typed_text(keystroke: &Keystroke) -> Option<String>`.

- [ ] **Step 1: Write the failing tests**

In `task_dialog.rs` `mod tests`, add (next to `keys_map_to_dialog_actions`):

```rust
    /// A macOS key as gpui-pre-macos reports it: no char with Cmd or Ctrl, the composed char
    /// with Option.
    fn mac(spec: &str, key_char: Option<&str>) -> Option<DialogKey> {
        let mut keystroke = Keystroke::parse(spec).unwrap();
        keystroke.key_char = key_char.map(str::to_owned);
        dialog_key_for(&keystroke, Os::Mac)
    }

    #[test]
    fn mac_fields_use_option_for_words_cmd_for_ends_and_the_emacs_keys() {
        let moved = |motion, extend| Some(DialogKey::Move { motion, extend });
        assert_eq!(mac("cmd-left", None), moved(Motion::Home, false));
        assert_eq!(mac("cmd-shift-right", None), moved(Motion::End, true));
        assert_eq!(mac("alt-left", None), moved(Motion::WordLeft, false));
        assert_eq!(mac("alt-shift-right", None), moved(Motion::WordRight, true));
        assert_eq!(mac("cmd-up", None), moved(Motion::DocStart, false));
        assert_eq!(mac("cmd-shift-down", None), moved(Motion::DocEnd, true));
        assert_eq!(mac("ctrl-left", None), None, "Ctrl+arrows switch Spaces");
        assert_eq!(mac("alt-backspace", None), Some(DialogKey::DeleteWord));
        assert_eq!(
            mac("cmd-backspace", None),
            Some(DialogKey::DeleteLine { forward: false })
        );
        assert_eq!(mac("alt-delete", None), Some(DialogKey::Delete { word: true }));
        assert_eq!(
            mac("cmd-delete", None),
            Some(DialogKey::DeleteLine { forward: true })
        );
        assert_eq!(mac("cmd-z", None), Some(DialogKey::Undo));
        assert_eq!(mac("cmd-shift-z", None), Some(DialogKey::Redo));
        assert_eq!(mac("cmd-a", None), Some(DialogKey::SelectAll));
        assert_eq!(mac("cmd-v", None), Some(DialogKey::Paste));
        assert_eq!(mac("cmd-enter", None), Some(DialogKey::Save));
        assert_eq!(mac("shift-enter", Some("\n")), Some(DialogKey::Enter));
        assert_eq!(mac("cmd-shift-enter", None), None, "the composer key");
        assert_eq!(mac("cmd-alt-enter", None), None, "the context-menu key");
        assert_eq!(mac("ctrl-a", None), moved(Motion::Home, false));
        assert_eq!(mac("ctrl-e", None), moved(Motion::End, false));
        assert_eq!(mac("ctrl-n", None), moved(Motion::Down, false));
        assert_eq!(mac("ctrl-d", None), Some(DialogKey::Delete { word: false }));
        assert_eq!(mac("ctrl-h", None), Some(DialogKey::Backspace));
        assert_eq!(
            mac("ctrl-k", None),
            Some(DialogKey::DeleteLine { forward: true })
        );
        assert_eq!(mac("ctrl-tab", None), None, "the tab switch");
        assert_eq!(mac("cmd-b", None), None);
        assert_eq!(
            mac("alt-b", Some("∫")),
            Some(DialogKey::Insert("∫".into())),
            "Option types the char macOS composed"
        );
        assert_eq!(mac("shift-a", Some("A")), Some(DialogKey::Insert("A".into())));
    }
```

In `editor.rs` `mod tests`, add (next to `keys_follow_vs_code_but_leave_the_app_shortcuts_alone`):

```rust
    #[test]
    fn mac_editor_keys_follow_text_edit() {
        let key = |keys: &str| edit_key_for(&Keystroke::parse(keys).unwrap(), Os::Mac);
        let moved = |motion, extend| Some(EditKey::Move { motion, extend });
        assert_eq!(key("cmd-left"), moved(Motion::Home, false));
        assert_eq!(key("cmd-shift-right"), moved(Motion::End, true));
        assert_eq!(key("alt-shift-left"), moved(Motion::WordLeft, true));
        assert_eq!(key("cmd-up"), moved(Motion::DocStart, false));
        assert_eq!(key("cmd-down"), moved(Motion::DocEnd, false));
        assert_eq!(key("alt-backspace"), Some(EditKey::Backspace { word: true }));
        assert_eq!(
            key("cmd-backspace"),
            Some(EditKey::DeleteLine { forward: false })
        );
        assert_eq!(key("ctrl-k"), Some(EditKey::DeleteLine { forward: true }));
        assert_eq!(key("cmd-shift-z"), Some(EditKey::Redo));
        assert_eq!(key("cmd-c"), Some(EditKey::Copy));
        assert_eq!(key("shift-enter"), Some(EditKey::Enter));
        // Window shortcuts go on to the keymap.
        for keys in ["cmd-s", "cmd-alt-left", "cmd-shift-v", "ctrl-left", "cmd-alt-f"] {
            assert_eq!(key(keys), None, "{keys}");
        }
        let mut option_b = Keystroke::parse("alt-b").unwrap();
        option_b.key_char = Some("∫".into());
        assert_eq!(
            edit_key_for(&option_b, Os::Mac),
            Some(EditKey::Insert("∫".into()))
        );
    }

    #[test]
    fn delete_line_removes_to_the_line_start_or_end() {
        let mut editor = Editor::new("one two\nthree four");
        editor.set_cursor(Pos::new(1, 3), false);
        editor.key(EditKey::DeleteLine { forward: false }, 10);
        assert_eq!(
            (editor.text().as_str(), editor.cursor()),
            ("one two\nee four", Pos::new(1, 0))
        );
        assert!(
            !editor.key(EditKey::DeleteLine { forward: false }, 10),
            "nothing before the caret on its line"
        );
        editor.set_cursor(Pos::new(0, 3), false);
        editor.key(EditKey::DeleteLine { forward: true }, 10);
        assert_eq!(editor.text(), "one\nee four");
    }
```

In `text_box.rs` `mod tests`, add:

```rust
    #[test]
    fn delete_line_removes_to_the_line_start_or_end() {
        let mut input = Input::from("one two\nthree four");
        input.click(11, false, 1);
        assert!(input.delete_line(false));
        assert_eq!((input.as_str(), input.cursor()), ("one two\nee four", 8));
        assert!(!input.delete_line(false), "nothing before the caret on its line");
        assert!(input.delete_line(true));
        assert_eq!(input.as_str(), "one two\n");
        let mut input = Input::from("keep this");
        input.click(5, false, 1);
        input.click(9, true, 1);
        assert!(input.delete_line(false), "a selection goes first");
        assert_eq!(input.as_str(), "keep ");
    }
```

In `keymap.rs` `mod tests`, extend the imports (`use crate::terminal::input::{InputMode, key_to_bytes};`) with:

```rust
    use crate::editor::edit_key_for;
    use crate::task_dialog::dialog_key_for;
```

and add:

```rust
    /// GPUI runs key bindings before a field sees the key, so a window-wide macOS default on a
    /// text key would take it from every field (and an Editor default from the Editor).
    #[test]
    fn mac_window_shortcuts_leave_the_text_keys_alone() {
        for (command, keys) in Keymap::defaults_for(Os::Mac).bindings() {
            let context = command.context();
            if context.is_none() {
                assert_eq!(
                    dialog_key_for(keys, Os::Mac),
                    None,
                    "{command:?} takes a text-field key"
                );
            }
            if context.is_none() || context == Some(EDITOR_CONTEXT) {
                assert_eq!(
                    edit_key_for(keys, Os::Mac),
                    None,
                    "{command:?} takes an Editor key"
                );
            }
        }
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p mkrew-ui --lib mac_`
Expected: compile errors `cannot find function dialog_key_for`, `edit_key_for`, `no variant DeleteLine`, `no method delete_line`.

- [ ] **Step 3: Add the `DeleteLine` variants and their edits**

In `editor.rs` `EditKey`, after `Delete { word: bool },` add:

```rust
    /// Cmd+Backspace / Cmd+Delete / Ctrl+K (macOS): the selection, else from the caret to its
    /// line's start (`forward: false`) or end.
    DeleteLine {
        forward: bool,
    },
```

In `Editor::key`, after `EditKey::Delete { word } => self.delete(word, true),` add:

```rust
            EditKey::DeleteLine { forward } => self.delete_line(forward),
```

After `fn delete` add:

```rust
    fn delete_line(&mut self, forward: bool) {
        let at = self.cursor;
        let range = self.selection().unwrap_or_else(|| {
            let to = Pos::new(at.line, if forward { self.lines[at.line].len() } else { 0 });
            at.min(to)..at.max(to)
        });
        if range.start != range.end {
            self.replace(range, "");
        }
    }
```

In `task_dialog.rs` `DialogKey`, after `Delete { word: bool },` add:

```rust
    /// Cmd+Backspace / Cmd+Delete / Ctrl+K (macOS): the selection, else from the caret to its
    /// line's start (`forward: false`) or end.
    DeleteLine {
        forward: bool,
    },
```

In `text_box.rs`, after `delete_word_after` add:

```rust
    /// Cmd+Backspace / Cmd+Delete / Ctrl+K (macOS): the selection, else from the caret to its
    /// line's start (`forward: false`) or end. `true` when the text changed.
    pub fn delete_line(&mut self, forward: bool) -> bool {
        if let Some(range) = self.selection() {
            return self.splice(range, "", false);
        }
        let to = self.target(if forward { Motion::End } else { Motion::Home });
        self.splice(self.cursor.min(to)..self.cursor.max(to), "", false)
    }
```

and in `Input::edit`, after `DialogKey::Delete { word: true } => self.delete_word_after(),` add:

```rust
            DialogKey::DeleteLine { forward } => self.delete_line(*forward),
```

- [ ] **Step 4: Split `edit_key` by OS**

In `editor.rs` add `use crate::keymap::Os;` to the imports. Replace `edit_key` (:603-646) with:

```rust
/// The editor meaning of `keystroke` on this OS. `None` means the key is not the editor's:
/// window shortcuts like Ctrl+Shift+Z, Alt+arrows and Ctrl+S go on to the keymap.
pub fn edit_key(keystroke: &Keystroke) -> Option<EditKey> {
    edit_key_for(keystroke, Os::CURRENT)
}

/// [`edit_key`] with `os`'s keys: VS Code's on Windows / Linux; on macOS Option by word, Cmd
/// to the line's / file's ends, Cmd+Shift+Z redo and the Emacs Ctrl keys (ADR 0098).
pub fn edit_key_for(keystroke: &Keystroke, os: Os) -> Option<EditKey> {
    let key = match os {
        Os::Mac => mac_edit_key(keystroke),
        Os::Windows | Os::Linux => pc_edit_key(keystroke),
    };
    key.or_else(|| typed_text(keystroke).map(EditKey::Insert))
}

/// The text `keystroke` types: its char, unless Cmd or a lone Ctrl is held. AltGr (Ctrl+Alt on
/// Windows) and macOS Option arrive with the composed char, which is typed.
pub(crate) fn typed_text(keystroke: &Keystroke) -> Option<String> {
    let m = keystroke.modifiers;
    if m.platform || (m.control && !m.alt) {
        return None;
    }
    keystroke
        .key_char
        .as_deref()
        .filter(|text| !text.is_empty() && !text.chars().any(char::is_control))
        .map(str::to_owned)
}

fn pc_edit_key(keystroke: &Keystroke) -> Option<EditKey> {
    let m = keystroke.modifiers;
    let ctrl = m.control || m.platform;
    let plain_ctrl = ctrl && !m.alt && !m.shift;
    let caret = |plain, word| {
        let motion = if ctrl { word } else { plain };
        (!m.alt).then_some(EditKey::Move {
            motion,
            extend: m.shift,
        })
    };
    match keystroke.key.as_str() {
        "left" => caret(Motion::Left, Motion::WordLeft),
        "right" => caret(Motion::Right, Motion::WordRight),
        "up" if !ctrl => caret(Motion::Up, Motion::Up),
        "down" if !ctrl => caret(Motion::Down, Motion::Down),
        "home" => caret(Motion::Home, Motion::DocStart),
        "end" => caret(Motion::End, Motion::DocEnd),
        "pageup" if !ctrl => caret(Motion::PageUp, Motion::PageUp),
        "pagedown" if !ctrl => caret(Motion::PageDown, Motion::PageDown),
        "backspace" if !m.alt => Some(EditKey::Backspace { word: ctrl }),
        "delete" if !m.alt => Some(EditKey::Delete { word: ctrl }),
        "enter" if !ctrl && !m.alt => Some(EditKey::Enter),
        "tab" if !ctrl && !m.alt && !m.shift => Some(EditKey::Tab),
        "a" if plain_ctrl => Some(EditKey::SelectAll),
        "z" if plain_ctrl => Some(EditKey::Undo),
        "y" if plain_ctrl => Some(EditKey::Redo),
        "c" if plain_ctrl => Some(EditKey::Copy),
        "x" if plain_ctrl => Some(EditKey::Cut),
        "v" if plain_ctrl => Some(EditKey::Paste),
        _ => None,
    }
}

/// macOS: one modifier means one thing (Shift extends a selection); mixes go to the keymap.
fn mac_edit_key(keystroke: &Keystroke) -> Option<EditKey> {
    let m = keystroke.modifiers;
    let cmd = m.platform && !m.control && !m.alt;
    let opt = m.alt && !m.control && !m.platform;
    let plain = !m.platform && !m.control && !m.alt;
    let emacs = m.control && !m.alt && !m.platform && !m.shift;
    let plain_cmd = cmd && !m.shift;
    let to = |motion| {
        Some(EditKey::Move {
            motion,
            extend: m.shift,
        })
    };
    match keystroke.key.as_str() {
        "left" if cmd => to(Motion::Home),
        "left" if opt => to(Motion::WordLeft),
        "left" if plain => to(Motion::Left),
        "right" if cmd => to(Motion::End),
        "right" if opt => to(Motion::WordRight),
        "right" if plain => to(Motion::Right),
        "up" if cmd => to(Motion::DocStart),
        "up" if plain => to(Motion::Up),
        "down" if cmd => to(Motion::DocEnd),
        "down" if plain => to(Motion::Down),
        "home" if cmd => to(Motion::DocStart),
        "home" if plain => to(Motion::Home),
        "end" if cmd => to(Motion::DocEnd),
        "end" if plain => to(Motion::End),
        "pageup" if plain => to(Motion::PageUp),
        "pagedown" if plain => to(Motion::PageDown),
        "backspace" if cmd => Some(EditKey::DeleteLine { forward: false }),
        "backspace" if opt => Some(EditKey::Backspace { word: true }),
        "backspace" if plain => Some(EditKey::Backspace { word: false }),
        "delete" if cmd => Some(EditKey::DeleteLine { forward: true }),
        "delete" if opt => Some(EditKey::Delete { word: true }),
        "delete" if plain => Some(EditKey::Delete { word: false }),
        "enter" if plain => Some(EditKey::Enter),
        "tab" if plain && !m.shift => Some(EditKey::Tab),
        "a" if plain_cmd => Some(EditKey::SelectAll),
        "z" if plain_cmd => Some(EditKey::Undo),
        "z" if cmd => Some(EditKey::Redo),
        "y" if plain_cmd => Some(EditKey::Redo),
        "c" if plain_cmd => Some(EditKey::Copy),
        "x" if plain_cmd => Some(EditKey::Cut),
        "v" if plain_cmd => Some(EditKey::Paste),
        "a" if emacs => to(Motion::Home),
        "e" if emacs => to(Motion::End),
        "b" if emacs => to(Motion::Left),
        "f" if emacs => to(Motion::Right),
        "p" if emacs => to(Motion::Up),
        "n" if emacs => to(Motion::Down),
        "d" if emacs => Some(EditKey::Delete { word: false }),
        "h" if emacs => Some(EditKey::Backspace { word: false }),
        "k" if emacs => Some(EditKey::DeleteLine { forward: true }),
        _ => None,
    }
}
```

The PC arms are the old ones with `return` dropped; a key whose arm does not match falls to `typed_text`, as before.

- [ ] **Step 5: Split `dialog_key` by OS**

In `task_dialog.rs` add `use crate::editor::typed_text;` (next to `use crate::editor::Motion;`) and `use crate::keymap::Os;`. Replace `dialog_key` (:69-111) with:

```rust
/// The dialog meaning of `keystroke` on this OS; `None` = ignored.
pub fn dialog_key(keystroke: &Keystroke) -> Option<DialogKey> {
    dialog_key_for(keystroke, Os::CURRENT)
}

/// [`dialog_key`] with `os`'s text keys (SPEC §13): on macOS Option moves and deletes by word,
/// Cmd goes to the line's / text's ends, Cmd+Shift+Z redoes and the Emacs Ctrl keys work.
pub fn dialog_key_for(keystroke: &Keystroke, os: Os) -> Option<DialogKey> {
    let key = match os {
        Os::Mac => mac_dialog_key(keystroke),
        Os::Windows | Os::Linux => pc_dialog_key(keystroke),
    };
    key.or_else(|| typed_text(keystroke).map(DialogKey::Insert))
}

fn pc_dialog_key(keystroke: &Keystroke) -> Option<DialogKey> {
    let m = keystroke.modifiers;
    let command = m.platform || m.control;
    let plain_command = command && !m.alt && !m.shift;
    let caret = |plain, word| {
        (!m.alt).then_some(DialogKey::Move {
            motion: if command { word } else { plain },
            extend: m.shift,
        })
    };
    match keystroke.key.as_str() {
        "escape" => Some(DialogKey::Cancel),
        "enter" if command => Some(DialogKey::Save),
        "enter" => Some(DialogKey::Enter),
        "tab" => Some(DialogKey::SwitchField),
        "backspace" if command && !m.alt => Some(DialogKey::DeleteWord),
        "backspace" => Some(DialogKey::Backspace),
        "delete" if !m.alt => Some(DialogKey::Delete { word: command }),
        "left" => caret(Motion::Left, Motion::WordLeft),
        "right" => caret(Motion::Right, Motion::WordRight),
        "up" if !command => caret(Motion::Up, Motion::Up),
        "down" if !command => caret(Motion::Down, Motion::Down),
        "home" => caret(Motion::Home, Motion::DocStart),
        "end" => caret(Motion::End, Motion::DocEnd),
        "c" if plain_command => Some(DialogKey::Copy),
        "x" if plain_command => Some(DialogKey::Cut),
        "v" if plain_command => Some(DialogKey::Paste),
        "a" if plain_command => Some(DialogKey::SelectAll),
        "z" if plain_command => Some(DialogKey::Undo),
        "y" if plain_command => Some(DialogKey::Redo),
        _ => None,
    }
}

/// macOS: one modifier means one thing (Shift extends a selection); mixes go to the keymap.
fn mac_dialog_key(keystroke: &Keystroke) -> Option<DialogKey> {
    let m = keystroke.modifiers;
    let cmd = m.platform && !m.control && !m.alt;
    let opt = m.alt && !m.control && !m.platform;
    let plain = !m.platform && !m.control && !m.alt;
    let emacs = m.control && !m.alt && !m.platform && !m.shift;
    let plain_cmd = cmd && !m.shift;
    let to = |motion| {
        Some(DialogKey::Move {
            motion,
            extend: m.shift,
        })
    };
    match keystroke.key.as_str() {
        "escape" => Some(DialogKey::Cancel),
        "enter" if plain_cmd || emacs => Some(DialogKey::Save),
        "enter" if plain => Some(DialogKey::Enter),
        "tab" if plain => Some(DialogKey::SwitchField),
        "backspace" if cmd => Some(DialogKey::DeleteLine { forward: false }),
        "backspace" if opt => Some(DialogKey::DeleteWord),
        "backspace" if plain => Some(DialogKey::Backspace),
        "delete" if cmd => Some(DialogKey::DeleteLine { forward: true }),
        "delete" if opt => Some(DialogKey::Delete { word: true }),
        "delete" if plain => Some(DialogKey::Delete { word: false }),
        "left" if cmd => to(Motion::Home),
        "left" if opt => to(Motion::WordLeft),
        "left" if plain => to(Motion::Left),
        "right" if cmd => to(Motion::End),
        "right" if opt => to(Motion::WordRight),
        "right" if plain => to(Motion::Right),
        "up" if cmd => to(Motion::DocStart),
        "up" if plain => to(Motion::Up),
        "down" if cmd => to(Motion::DocEnd),
        "down" if plain => to(Motion::Down),
        "home" if cmd => to(Motion::DocStart),
        "home" if plain => to(Motion::Home),
        "end" if cmd => to(Motion::DocEnd),
        "end" if plain => to(Motion::End),
        "c" if plain_cmd => Some(DialogKey::Copy),
        "x" if plain_cmd => Some(DialogKey::Cut),
        "v" if plain_cmd => Some(DialogKey::Paste),
        "a" if plain_cmd => Some(DialogKey::SelectAll),
        "z" if plain_cmd => Some(DialogKey::Undo),
        "z" if cmd => Some(DialogKey::Redo),
        "y" if plain_cmd => Some(DialogKey::Redo),
        "a" if emacs => to(Motion::Home),
        "e" if emacs => to(Motion::End),
        "b" if emacs => to(Motion::Left),
        "f" if emacs => to(Motion::Right),
        "p" if emacs => to(Motion::Up),
        "n" if emacs => to(Motion::Down),
        "d" if emacs => Some(DialogKey::Delete { word: false }),
        "h" if emacs => Some(DialogKey::Backspace),
        "k" if emacs => Some(DialogKey::DeleteLine { forward: true }),
        _ => None,
    }
}
```

Update the `DialogKey` docs that name keys: `Save` → `/// Ctrl+Enter (Cmd+Enter on macOS) anywhere.` stays; `DeleteWord` → `/// Ctrl+Backspace (Option+Backspace on macOS): delete the word before the caret.`; `Delete` → `/// Delete; with Ctrl (Option on macOS), the word after the caret.`; `Undo`/`Redo` → `/// Ctrl+Z / Ctrl+Y (Cmd+Z / Cmd+Shift+Z on macOS): undo / redo the field's last edit.`

In the old test `keys_map_to_dialog_actions`, the assertion `assert_eq!(key("cmd-backspace"), Some(DialogKey::DeleteWord));` and its neighbors run on Windows through `dialog_key` (= PC keys), so they stay; only its message `"Alt+Backspace is not a Windows field key"` stays true for PC.

- [ ] **Step 6: Build the crate and route any non-exhaustive match**

Run: `cargo check -p mkrew-ui --all-targets`
Expected: compiles. If the compiler reports a non-exhaustive `match` over `DialogKey` or `EditKey`, add `DeleteLine { .. }` to the arm that handles `DeleteWord` (or `Backspace { .. }` for `EditKey`) in that match.

- [ ] **Step 7: Run the tests**

Run: `cargo test -p mkrew-ui --lib -- mac_ delete_line keys_map_to_dialog_actions keys_follow_vs_code`
Expected: PASS.
Then run: `cargo test -p mkrew-ui`
Expected: PASS (every PC text-field test unchanged).

- [ ] **Step 8: SPEC text-field paragraph**

In `docs/SPEC.md`, after line 1445 (`… Where a field sits over a list, ↑/↓ move in the list.`), add:

```markdown
On macOS (ADR 0098) fields and the Editor follow the Mac text keys instead: Option+←/→ by word, Cmd+←/→ to the line's start / end, Cmd+↑/↓ to the text's start / end, Option+Backspace / Option+Delete the word before / after the caret, Cmd+Backspace / Cmd+Delete to the line's start / end, Cmd+A / C / X / V / Z, Cmd+Shift+Z (and Cmd+Y) redo, Cmd+Enter or Ctrl+Enter save or send, and the Emacs keys Ctrl+A / E (line start / end), Ctrl+B / F / P / N (← → ↑ ↓), Ctrl+D (Delete), Ctrl+H (Backspace) and Ctrl+K (delete to the line's end); Ctrl+←/→ are left to macOS (Spaces).
An Option + character key types the character macOS composed (Option+B types "∫").
```

Add to the HUMAN_CHECKS block from Task 3:

```markdown
- [ ] **On a Mac:** in the composer, Option+← / Cmd+← / Option+Backspace / Cmd+Backspace / Ctrl+K / Cmd+Shift+Z do what TextEdit does; Option+B types "∫"; the same in the Editor.
```

- [ ] **Step 9: Commit**

```bash
git add crates/mkrew-ui/src/editor.rs crates/mkrew-ui/src/task_dialog.rs crates/mkrew-ui/src/text_box.rs crates/mkrew-ui/src/keymap.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): macOS text keys in fields and the Editor"
```

(If Step 6 touched other files for a non-exhaustive match, add them to `git add` by name.)

---

### Task 5: macOS key names in fixed UI text, Finder wording, replace-all key

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs` (new `os_text!` macro)
- Modify: `crates/mkrew-ui/src/git_actions.rs:51-52`
- Modify: `crates/mkrew-ui/src/settings_dialog.rs:72-75, 507-511`
- Modify: `crates/mkrew-ui/src/palette.rs:218`
- Modify: `crates/mkrew-ui/src/shell/ask_dialog.rs:270, 289`
- Modify: `crates/mkrew-ui/src/shell/cards.rs:817`
- Modify: `crates/mkrew-ui/src/shell/composer.rs:789`
- Modify: `crates/mkrew-ui/src/shell/files_preview.rs:358`
- Modify: `crates/mkrew-ui/src/shell/files_render.rs:644`
- Modify: `crates/mkrew-ui/src/shell/files.rs:324, 422`
- Modify: `crates/mkrew-ui/src/shell/git_render.rs:861, 984`
- Modify: `crates/mkrew-ui/src/shell/menus.rs:36, 104, 156`
- Modify: `crates/mkrew-ui/src/shell/peek.rs:519, 565, 567`
- Modify: `crates/mkrew-ui/src/shell/prompts_render.rs:73, 269`
- Modify: `crates/mkrew-ui/src/shell/run.rs:1061`
- Modify: `crates/mkrew-ui/src/shell/settings.rs:10, 334`
- Modify: `crates/mkrew-ui/src/shell/task_dialog.rs:570, 624`
- Modify: `crates/mkrew-ui/src/shell/workspace_actions.rs:410, 449`
- Modify: `crates/mkrew-ui/src/main.rs:382`
- Modify: `docs/SPEC.md` (§10 "Open in File Explorer", §12A file menu, §13 Peek row and Task / Workspace row)

**Interfaces:**
- Consumes: nothing new (the macOS spellings follow Task 2's labels: ⌘↩, ⌥⌘↩, ⌥Y, ⌘O, ⌘N, ⌘S).
- Produces: `#[macro_export] macro_rules! os_text` (`os_text!(pc, mac)` is `$mac` when built for macOS, else `$pc`; both are `&'static str`). Use as `crate::os_text!` in the library and `mkrew_ui::os_text!` in `shell/*.rs` and `main.rs`.

These strings are fixed keys that are not in `keymap.toml` (the text box Save key, the Peek popover keys, the Prompts page keys, Replace all), so a compile-time pair is enough; keys from the keymap keep using `self.keymap.label(..)`.

No new test: the macro is a compile-time `if`, and the macOS spellings it carries are the ones Task 2's `mac_labels_use_the_modifier_symbols_in_apple_order` pins.

- [ ] **Step 1: Add the macro**

In `keymap.rs`, right after `impl Os { … }` (Task 2), add:

```rust
/// A fixed UI text that names keys: `$mac` in a macOS build ("⌘↩ to send"), else `$pc`
/// ("Ctrl+Enter to send"). Both are `&'static str`. For keys of `keymap.toml` use
/// [`Keymap::label`] instead, which follows the user's bindings.
#[macro_export]
macro_rules! os_text {
    ($pc:expr, $mac:expr $(,)?) => {
        if cfg!(target_os = "macos") { $mac } else { $pc }
    };
}
```

- [ ] **Step 2: Check it builds**

Run: `cargo check -p mkrew-ui`
Expected: compiles (the macro is unused until Step 3).

- [ ] **Step 3: Replace each hard-coded key text**

Make exactly these replacements (left: current code, right: new code):

`git_actions.rs:51-52`:
```rust
pub const STAGE_ALL_OFFER: &str = crate::os_text!(
    "Nothing staged. Ctrl+Enter again stages every change and commits.",
    "Nothing staged. ⌘↩ again stages every change and commits.",
);
```

`shell/ask_dialog.rs:270`: `.child(muted("Enter new line · Ctrl+Enter ask · Esc close".into()))` →
```rust
                    .child(muted(
                        mkrew_ui::os_text!(
                            "Enter new line · Ctrl+Enter ask · Esc close",
                            "Enter new line · ⌘↩ ask · Esc close",
                        )
                        .into(),
                    )),
```
`shell/ask_dialog.rs:289`: `"Ctrl+Enter"` → `mkrew_ui::os_text!("Ctrl+Enter", "⌘↩")`.

`shell/cards.rs:817`: `"Follow up… (Ctrl+Enter sends)",` →
```rust
        mkrew_ui::os_text!("Follow up… (Ctrl+Enter sends)", "Follow up… (⌘↩ sends)"),
```

`shell/composer.rs:789`: `"Ctrl+Enter to send · @ for a file · Esc to hide".to_owned(),` →
```rust
                    mkrew_ui::os_text!(
                        "Ctrl+Enter to send · @ for a file · Esc to hide",
                        "⌘↩ to send · @ for a file · Esc to hide",
                    )
                    .to_owned(),
```

`shell/files_preview.rs:358`: `.child("Ctrl+Enter apply · Esc cancel"),` →
```rust
                .child(mkrew_ui::os_text!("Ctrl+Enter apply · Esc cancel", "⌘↩ apply · Esc cancel")),
```

`shell/files_render.rs:644`: `"Ctrl+Alt+Enter"` → `mkrew_ui::os_text!("Ctrl+Alt+Enter", "⌥⌘↩")`.

`shell/files.rs:324` and `:422`: `"enter" if m.control && m.alt => {` → `"enter" if m.secondary() && m.alt => {` (Ctrl+Alt+Enter on PC as before, Cmd+Option+Enter on macOS like VS Code).

`shell/git_render.rs:861`: `"Stage every change, then commit (Ctrl+Enter twice)"` →
```rust
            mkrew_ui::os_text!(
                "Stage every change, then commit (Ctrl+Enter twice)",
                "Stage every change, then commit (⌘↩ twice)",
            )
```
`shell/git_render.rs:984`: `"Ctrl+Enter"` → `mkrew_ui::os_text!("Ctrl+Enter", "⌘↩")`.

`shell/peek.rs:519`: `if letter == 'y' { "Alt+Y" } else { "Alt+N" },` →
```rust
                        if letter == 'y' {
                            mkrew_ui::os_text!("Alt+Y", "⌥Y")
                        } else {
                            mkrew_ui::os_text!("Alt+N", "⌥N")
                        },
```
`shell/peek.rs:565` and `:567` (the open key is `modifiers.secondary()`, so Cmd+O on macOS):
```rust
            mkrew_ui::os_text!(
                "Enter sends · Alt+Y / Alt+N answer · Ctrl+O opens · ↑ ↓ others · Esc closes",
                "Enter sends · ⌥Y / ⌥N answer · ⌘O opens · ↑ ↓ others · Esc closes",
            )
            .to_owned()
```
```rust
            mkrew_ui::os_text!(
                "Enter sends · Ctrl+O opens the pane · ↑ ↓ others that need you · Esc closes",
                "Enter sends · ⌘O opens the pane · ↑ ↓ others that need you · Esc closes",
            )
            .to_owned()
```

`shell/prompts_render.rs:73`: `"Ctrl+N"` → `mkrew_ui::os_text!("Ctrl+N", "⌘N")`; `:269`: `"Ctrl+S"` → `mkrew_ui::os_text!("Ctrl+S", "⌘S")`.

`shell/run.rs:1061`: `"Ctrl+Enter"` → `mkrew_ui::os_text!("Ctrl+Enter", "⌘↩")`.

`shell/settings.rs:334`: `"Ctrl+N new · Ctrl+S saves · Esc closes"` → `mkrew_ui::os_text!("Ctrl+N new · Ctrl+S saves · Esc closes", "⌘N new · ⌘S saves · Esc closes")`; doc comment `:10` `/// Ctrl+, and the palette's …` → `/// Ctrl+, (⌘, on macOS) and the palette's …`.

`shell/task_dialog.rs:570`: the `else` string →
```rust
                                mkrew_ui::os_text!(
                                    "@ for a file · paste an image or drop a file · Ctrl+Enter to save",
                                    "@ for a file · paste an image or drop a file · ⌘↩ to save",
                                )
```
`shell/task_dialog.rs:624`: `"Ctrl+Enter"` → `mkrew_ui::os_text!("Ctrl+Enter", "⌘↩")`.

- [ ] **Step 4: Finder wording**

`shell/menus.rs:36` and `:156`: `"Open in File Explorer",` → `mkrew_ui::os_text!("Open in File Explorer", "Reveal in Finder"),`.
`shell/menus.rs:104` doc: `/// (rows only), Open in File Explorer, Open in Zed.` → `/// (rows only), Open in File Explorer (Reveal in Finder on macOS), Open in Zed.`
`palette.rs:218`: `Self::WorkspaceExplorer => "Workspace: Open in File Explorer",` →
```rust
            Self::WorkspaceExplorer => crate::os_text!(
                "Workspace: Open in File Explorer",
                "Workspace: Reveal in Finder",
            ),
```
`shell/workspace_actions.rs:410` and `:449`, `main.rs:382`: in the doc comments, write `"Open in File Explorer" ("Reveal in Finder" on macOS)` where they say `"Open in File Explorer"` / `Open in File Explorer`.

- [ ] **Step 5: Recorder message**

In `settings_dialog.rs`, replace the error text at :510 with:

```rust
                    crate::os_text!(
                        "Use Ctrl, Alt or Win with the key (or a function key)",
                        "Use ⌃, ⌥ or ⌘ with the key (or a function key)",
                    )
                    .to_owned(),
```

and in the `bindable` doc (:72-75) change `/// Ctrl, Alt or Win, or a function key;` to `/// Ctrl, Alt or Win (⌃ ⌥ ⌘ on macOS), or a function key;`.

- [ ] **Step 6: SPEC wording**

In `docs/SPEC.md`:
- line 1054: `- **Open in File Explorer:** in the workspace menu …` → `- **Open in File Explorer** ("Reveal in Finder" on macOS): in the workspace menu …`
- line 1325: `"Open in File Explorer" and "Open in Zed" (§10)` → `"Open in File Explorer" ("Reveal in Finder" on macOS) and "Open in Zed" (§10)`
- the Task / Workspace row of the §13 table (`| Task / Workspace | Merge back …`): `pin, open in File Explorer,` → `pin, open in File Explorer (Reveal in Finder on macOS),`
- the Peek row of the §13 table (`| Agent (v2) | Peek the first session …`): `Alt+Y / Alt+N answer, Ctrl+O open the pane` → `Alt+Y / Alt+N answer, Ctrl+O (⌘O on macOS) open the pane`

- [ ] **Step 7: Check nothing PC-only is left**

Run: `grep -rn --include=*.rs -E '"[^"]*(Ctrl\+|Alt\+|Win\+|File Explorer)' crates/mkrew-ui/src | grep -v -E 'tests\.rs:|assert|os_text|keys_label|label_for'`
Expected: only doc-comment lines (starting with `///` or `//!`) and the PC halves inside `os_text!` calls split over lines; no bare UI string.

- [ ] **Step 8: Run the tests and the full verify**

Run: `cargo test -p mkrew-ui`
Expected: PASS (the `STAGE_ALL_OFFER` test compares the constant, unchanged on PC).
Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt check, clippy `-D warnings` and every test pass. If it exceeds 600 s, run `cargo test --workspace` separately with no timeout.

- [ ] **Step 9: HUMAN_CHECKS and commit**

Add to the HUMAN_CHECKS block from Task 3:

```markdown
- [ ] **Look (dark + light) on a Mac:** composer, task dialog, Ask AI, Peek, Run setup, Prompts page, commit box and Replace all show ⌘↩ / ⌘O / ⌥Y / ⌥⌘↩; the workspace menu and the file tree menu say "Reveal in Finder".
```

```bash
git add crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/src/git_actions.rs crates/mkrew-ui/src/settings_dialog.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/ask_dialog.rs crates/mkrew-ui/src/shell/cards.rs crates/mkrew-ui/src/shell/composer.rs crates/mkrew-ui/src/shell/files_preview.rs crates/mkrew-ui/src/shell/files_render.rs crates/mkrew-ui/src/shell/files.rs crates/mkrew-ui/src/shell/git_render.rs crates/mkrew-ui/src/shell/menus.rs crates/mkrew-ui/src/shell/peek.rs crates/mkrew-ui/src/shell/prompts_render.rs crates/mkrew-ui/src/shell/run.rs crates/mkrew-ui/src/shell/settings.rs crates/mkrew-ui/src/shell/task_dialog.rs crates/mkrew-ui/src/shell/workspace_actions.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "fix(ui): macOS key names in hints and tooltips, Reveal in Finder"
```
