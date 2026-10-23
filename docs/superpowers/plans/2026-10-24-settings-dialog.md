# Settings dialog Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A modal Settings dialog (Ctrl+,) that edits every `settings.toml` key and every keyboard shortcut, applying each change at once.

**Architecture:** `mkrew-core` gets a table of settings (`settings_fields::SETTINGS`) that reads current values through `Serialize` and writes through the existing `write_value`. `mkrew-ui` gets `keymap::write_binding`, a pure dialog model (`settings_dialog.rs`) and the shell glue (`shell/settings.rs`) that renders it, writes files, re-applies settings and rebinds keys live.

**Tech Stack:** Rust, GPUI, serde + toml 1.x, existing `text_box::Input`, `task_dialog::dialog_key`, widgets in `shell/widgets.rs`.

**Spec:** `docs/superpowers/specs/2026-10-24-settings-dialog-design.md`

## Global Constraints

- Work only in the worktree `C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/settings-ui`, branch `feat/settings-ui`.
- Before the first `cargo` command: ask the user whether to share `CARGO_TARGET_DIR` with the main checkout or build into the worktree's own `target/` (C: is nearly full). No `cargo clean`.
- No protocol change, no migration, no daemon change.
- Every color, spacing, radius, font size from `crates/mkrew-ui/src/theme.rs`; new sizes are added there as named constants. Dark and light themes.
- No `unwrap`/`expect` outside tests unless provably infallible (comment why).
- Errors with a place of their own show in the dialog, never as a toast (SPEC §14).
- Every commit builds with `cargo clippy --workspace --all-targets -- -D warnings` clean (no dead code between tasks).
- Commits: Conventional Commits, `git -c user.email=tungnv975@gmail.com commit`, stage only own paths, never `git add -A`, never commit `Cargo.lock`, `.github/workflows`, `packaging/`, `docs/BUILD_VI.md`, `docs/REDEPLOY_VI.md`.
- Verify with `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL). If rustc crashes on mkrew-ui test builds, run fmt check + clippy, then `cargo test --workspace -j 4`.
- Never kill the daemon, Mkrew or omp. A locked exe: rename it to `*.locked-old.exe`.
- ADR number: 0072 unless `docs/decisions/` (here and on master) already has it; re-check before writing.
- Code in this plan names real helpers (`text_field`, `caret_field`, `kbd`, `menu_item`, `meta_line`, `modal_shade`, `typing`, theme fields). If a signature differs from what is written here, follow the real signature; do not add new helpers for it.

## Review Focus

1. **Windows Shift folding when capturing a key:** Ctrl+Shift+2 arrives as `ctrl-@` on a US layout. A captured binding must still fire after it is written and rebound. Task 5 Step 7 smoke checks it; HUMAN_CHECKS lists it.
2. **A keymap.toml that already has an unrelated problem** (an unknown command left from the rename `open_keymap_settings`): rebinding another command must still work. Task 2 has `an_old_problem_does_not_block_a_new_binding`.
3. **Text with quotes, backslashes or Vietnamese** (`voice.prompt`, `composer.file_ref`): written escaped, read back the same. Task 1 has `text_is_written_as_an_escaped_toml_string`.
4. **A number typed with letters** (`font_size = 1a`): refused with serde's message, file unchanged. Task 1 has `a_number_with_letters_is_refused`.
5. **A hand edit while a draft is open:** the draft stays, the other rows show the new values. Task 3 has `reload_keeps_the_open_draft`.

---

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `crates/mkrew-core/src/settings.rs` | modify | `Serialize` derives; `write_value` → `pub(crate)` |
| `crates/mkrew-core/src/settings_fields.rs` | create | `Page`, `Kind`, `Setting`, `SETTINGS`, `current`, `literal`, `write` |
| `crates/mkrew-core/src/lib.rs` | modify | `pub mod settings_fields;` |
| `crates/mkrew-ui/src/keymap.rs` | modify | rename `OpenKeymapSettings` → `OpenSettings`; `write_binding`, `set_binding`, `Keymap::changed`, `keys_label`, `notice`; template text |
| `crates/mkrew-ui/src/palette.rs`, `palette/tests.rs` | modify | rename; Task 5 drops `Extra::OpenSettings` |
| `crates/mkrew-ui/src/settings_dialog.rs` | create | pure dialog model |
| `crates/mkrew-ui/src/lib.rs` | modify | `pub mod settings_dialog;` |
| `crates/mkrew-ui/src/theme.rs` | modify | dialog + switch size tokens |
| `crates/mkrew-ui/src/main.rs` | modify | `ActiveKeymap` global, layout rebind reads it, `Shell` fields, render child |
| `crates/mkrew-ui/src/shell/settings.rs` | create | open/close, keys, effects, writes, rebind, render |
| `crates/mkrew-ui/src/shell.rs` | modify | `mod settings;` |
| `crates/mkrew-ui/src/shell/connection.rs` | modify | extract `apply_settings_file`; `keymap::notice` |
| `crates/mkrew-ui/src/shell/focus.rs` | modify | `modal_open` includes the dialog |
| `crates/mkrew-ui/src/shell/sessions.rs`, `navigation.rs`, `workspace_actions.rs` | modify | call sites |
| `docs/decisions/0072-settings-dialog.md`, `docs/SPEC.md`, `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md` | create/modify | docs |

---

### Task 1: Settings table in mkrew-core

**Files:**
- Modify: `crates/mkrew-core/src/settings.rs` (derives at lines 14–304, `FontSize` at 390–405, `write_value` at 355)
- Create: `crates/mkrew-core/src/settings_fields.rs`
- Modify: `crates/mkrew-core/src/lib.rs:20`

**Interfaces:**
- Produces:
  - `mkrew_core::settings_fields::Page` (`General, Appearance, Notifications, Voice, Jira, Phone, Features`), `Page::ALL: [Page; 7]`, `Page::title(self) -> &'static str`
  - `Kind` (`Bool`, `Choice(&'static [&'static str])`, `Number`, `Text`, `Font`)
  - `Setting { page, section, key, kind, label, help, hidden }` (all `pub`), `Copy`
  - `pub const SETTINGS: &[Setting]`
  - `Setting::id(&self) -> String` (`"ui.theme"`), `Setting::current(&self, &Settings) -> String`, `Setting::literal(&self, text: &str) -> String`, `Setting::write(&self, path: &Path, text: &str) -> Result<(), SettingsError>`

- [ ] **Step 1: Add `Serialize` to every settings type**

In `settings.rs` change `use serde::Deserialize;` to `use serde::{Deserialize, Serialize};` and add `Serialize` to the derive list of `Settings`, `FeatureSettings`, `WebSettings`, `NotifySettings`, `AwakeSettings`, `ComposerSettings`, `ZedSettings`, `VoiceSettings`, `JiraSettings`, `FontSettings`, `ThemeMode`, `DiffMode`, `UiSettings`. The `#[serde(rename_all = "lowercase")]` on the enums applies to both directions. `FontSize`:

```rust
/// A font size in px, 8–32 (checked when parsed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct FontSize(pub u16);

impl From<FontSize> for u16 {
    fn from(size: FontSize) -> u16 {
        size.0
    }
}
```

Change `fn write_value(` to `pub(crate) fn write_value(`.

- [ ] **Step 2: Write the failing tests**

Create `settings_fields.rs` with only this tests module, and add `pub mod settings_fields;` after `pub mod settings;` in `lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Settings, load, template};
    use std::collections::BTreeSet;

    /// `section.key` of every key the template lists (commented or not).
    fn template_keys() -> BTreeSet<String> {
        let mut section = String::new();
        let mut keys = BTreeSet::new();
        for line in template().lines() {
            let line = line.trim_start_matches('#').trim();
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.to_owned();
            } else if let Some((key, _)) = line.split_once(" = ")
                && !key.contains(' ')
                && !section.is_empty()
            {
                keys.insert(format!("{section}.{key}"));
            }
        }
        keys
    }

    fn setting(id: &str) -> &'static Setting {
        SETTINGS.iter().find(|s| s.id() == id).unwrap()
    }

    #[test]
    fn every_template_key_has_exactly_one_setting() {
        let table: Vec<String> = SETTINGS.iter().map(Setting::id).collect();
        let unique: BTreeSet<String> = table.iter().cloned().collect();
        assert_eq!(unique.len(), table.len(), "a key is listed twice");
        assert_eq!(unique, template_keys());
    }

    #[test]
    fn writing_each_default_reads_it_back_and_keeps_comments() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let defaults = Settings::default();
        for setting in SETTINGS {
            let text = setting.current(&defaults);
            setting
                .write(&path, &text)
                .unwrap_or_else(|e| panic!("{}: {e}", setting.id()));
            let loaded = load(&path).unwrap();
            assert_eq!(setting.current(&loaded), text, "{}", setting.id());
        }
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.starts_with("# Mkrew settings"), "the template comments stay");
        assert_eq!(load(&path).unwrap(), defaults);
    }

    #[test]
    fn out_of_range_values_are_refused_and_the_file_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "[ui]\nfont_size = 14\n").unwrap();
        for (id, text, message) in [
            ("ui.font_size", "40", "font_size must be 8–32 px, got 40"),
            ("voice.max_seconds", "0", "max_seconds must be 1–600 seconds, got 0"),
        ] {
            let error = setting(id).write(&path, text).unwrap_err();
            assert!(error.to_string().contains(message), "{id}: {error}");
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[ui]\nfont_size = 14\n");
    }

    #[test]
    fn a_number_with_letters_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        assert!(setting("ui.font_size").write(&path, "1a").is_err());
        assert!(!path.exists(), "nothing written");
    }

    #[test]
    fn text_is_written_as_an_escaped_toml_string() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let text = r#"Sửa "lỗi" C:\repo"#;
        setting("voice.prompt").write(&path, text).unwrap();
        assert_eq!(load(&path).unwrap().voice.prompt, text);
    }

    #[test]
    fn values_show_as_their_toml_names() {
        let defaults = Settings::default();
        assert_eq!(setting("ui.theme").current(&defaults), "system");
        assert_eq!(setting("ui.diff_wrap").current(&defaults), "true");
        assert_eq!(setting("ui.font_size").current(&defaults), "13");
        assert_eq!(setting("composer.file_ref").current(&defaults), "@{path}");
    }
}
```

Check the exact `FontSize` error text in `settings.rs:395-405` before running and use it in `out_of_range_values_…` (the palette test at `palette/tests.rs:151` shows `font_size must be 8–32 px, got 4`).

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p mkrew-core settings_fields`
Expected: FAIL to compile (`SETTINGS`, `Setting` not found).

- [ ] **Step 4: Write the module** (above the tests)

```rust
//! The settings dialog's table (SPEC §20.0, ADR 0072): one [`Setting`] per `settings.toml`
//! key, so the UI builds its rows from data and a test keeps the table and the template in step.

use std::path::Path;

use crate::settings::{Settings, SettingsError, write_value};

/// A page of the dialog (Keyboard is the UI's own).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    General,
    Appearance,
    Notifications,
    Voice,
    Jira,
    Phone,
    Features,
}

impl Page {
    pub const ALL: [Page; 7] = [
        Page::General,
        Page::Appearance,
        Page::Notifications,
        Page::Voice,
        Page::Jira,
        Page::Phone,
        Page::Features,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Page::General => "General",
            Page::Appearance => "Appearance",
            Page::Notifications => "Notifications",
            Page::Voice => "Voice",
            Page::Jira => "Jira",
            Page::Phone => "Phone",
            Page::Features => "Features",
        }
    }
}

/// How a value is edited. Ranges are checked by serde when the file is parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Bool,
    /// One of these TOML strings.
    Choice(&'static [&'static str]),
    /// A whole number.
    Number,
    Text,
    /// An installed font family; empty = the built-in default.
    Font,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Setting {
    pub page: Page,
    pub section: &'static str,
    pub key: &'static str,
    pub kind: Kind,
    pub label: &'static str,
    pub help: &'static str,
    /// Set by another control (dragging a panel edge); listed for the template check only.
    pub hidden: bool,
}

const fn row(
    page: Page,
    section: &'static str,
    key: &'static str,
    kind: Kind,
    label: &'static str,
    help: &'static str,
) -> Setting {
    Setting { page, section, key, kind, label, help, hidden: false }
}

const fn hidden(section: &'static str, key: &'static str) -> Setting {
    Setting {
        page: Page::Appearance,
        section,
        key,
        kind: Kind::Number,
        label: "",
        help: "",
        hidden: true,
    }
}

use Kind::{Bool, Choice, Font, Number, Text};
use Page::{Appearance, Features, General, Jira, Notifications, Phone, Voice};

/// Every `settings.toml` key, in page order.
pub const SETTINGS: &[Setting] = &[
    row(General, "awake", "keep_awake", Bool, "Keep the PC awake", "While an agent works, the PC does not sleep."),
    row(General, "composer", "file_ref", Text, "File reference", "How a picked file is typed in the composer; {path} is the path."),
    row(General, "zed", "follow_debounce_ms", Number, "Follow in Zed wait", "Milliseconds after the agent's last edit before Zed opens the file."),
    row(General, "ui", "reading_width", Bool, "Reading width", "Agent panes start narrow (about 120–140 columns, centered)."),
    row(General, "ui", "close_to_tray", Bool, "Close to tray", "Windows: the window's ✕ hides Mkrew to the tray icon."),
    row(Appearance, "ui", "theme", Choice(&["system", "light", "dark"]), "Theme", "System follows the OS."),
    row(Appearance, "ui", "font_family", Font, "App font", "An installed font; Default is the system UI font."),
    row(Appearance, "ui", "font_size", Number, "App font size", "8–32 px."),
    row(Appearance, "terminal", "font_family", Font, "Code font", "Terminal, Files and Git; pick a monospace font."),
    row(Appearance, "terminal", "font_size", Number, "Code font size", "8–32 px."),
    row(Appearance, "ui", "diff_mode", Choice(&["unified", "split"]), "Diff layout", "Unified: one column. Split: old | new."),
    row(Appearance, "ui", "diff_wrap", Bool, "Wrap long diff lines", "Off: lines are cut at the pane edge."),
    hidden("ui", "inspector_width"),
    hidden("ui", "sidebar_width"),
    row(Notifications, "notify", "needs_input", Bool, "When an agent needs you", "An OS notification."),
    row(Notifications, "notify", "done", Bool, "When an agent is done", "An OS notification."),
    row(Notifications, "notify", "sound", Bool, "Sound", "Notifications play the OS sound."),
    row(Notifications, "notify", "flash_taskbar", Bool, "Flash the taskbar", "Windows: while an agent needs you and Mkrew is not focused."),
    row(Voice, "voice", "base_url", Text, "API URL", "An OpenAI-compatible speech-to-text API."),
    row(Voice, "voice", "model", Text, "Model", ""),
    row(Voice, "voice", "api_key_env", Text, "API key variable", "The environment variable holding the key; empty: no key. Restart Mkrew after setting it."),
    row(Voice, "voice", "language", Text, "Language", "Empty detects it per recording; vi or en forces one."),
    row(Voice, "voice", "prompt", Text, "Spelling hint", "Words the model should spell as written."),
    row(Voice, "voice", "max_seconds", Number, "Longest recording", "1–600 seconds."),
    row(Jira, "jira", "site", Text, "Site", "Like acme.atlassian.net; empty turns Jira off."),
    row(Jira, "jira", "email", Text, "Email", "Your Atlassian account email."),
    row(Jira, "jira", "api_token_env", Text, "API token variable", "The environment variable holding the token."),
    row(Phone, "web", "enabled", Bool, "Phone access", "The daemon runs mk web for phones on your tailnet."),
    row(Phone, "web", "push", Bool, "Phone notifications", "mk web notifies phones that opted in."),
    row(Features, "features", "docs", Bool, "Docs view", "Markdown docs with comments."),
    row(Features, "features", "turns", Bool, "Turn snapshots", "The agent's Turns… dialog; applies to sessions started after the change."),
];

impl Setting {
    /// `section.key`, e.g. `ui.theme`.
    pub fn id(&self) -> String {
        format!("{}.{}", self.section, self.key)
    }

    /// The value in `settings` as the dialog shows it: `true`, `dark`, `13`, or the raw text.
    pub fn current(&self, settings: &Settings) -> String {
        // Never fails in practice: every settings type is plain data with string keys.
        let Ok(toml::Value::Table(table)) = toml::Value::try_from(settings) else {
            return String::new();
        };
        match table.get(self.section).and_then(|section| section.get(self.key)) {
            Some(toml::Value::String(text)) => text.clone(),
            Some(value) => value.to_string(),
            None => String::new(),
        }
    }

    /// `text` as the TOML literal written for this setting. A number with other characters is
    /// written as a string, so the parse refuses it with serde's own message.
    pub fn literal(&self, text: &str) -> String {
        let text = if self.kind == Kind::Text { text } else { text.trim() };
        match self.kind {
            Kind::Bool => text.to_owned(),
            Kind::Number if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) => {
                text.to_owned()
            }
            _ => toml::Value::String(text.to_owned()).to_string(),
        }
    }

    /// Saves `text` (see [`Self::literal`]); nothing is written when the result does not parse.
    pub fn write(&self, path: &Path, text: &str) -> Result<(), SettingsError> {
        write_value(path, self.section, self.key, &self.literal(text))
    }
}
```

If `toml::Value::try_from` is not the toml 1.x name, use the crate's serializer into a `toml::Table` (`toml::Table::try_from(settings)`); look it up in the toml 1.x docs, do not guess.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p mkrew-core settings`
Expected: PASS, including the existing `settings::tests`.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-core/src/settings.rs crates/mkrew-core/src/settings_fields.rs crates/mkrew-core/src/lib.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(core): settings table for the settings dialog"
```

---

### Task 2: Keymap writes and the `OpenSettings` rename

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs` (enum line 122, `all()` 243, `name()` 321, `default_keys()` 411, template 452–467, new fns after `ensure_file` 471–492, `impl Keymap` 521–665, tests 799–824)
- Modify: `crates/mkrew-ui/src/palette.rs` (title 251, group arm 272, `keymap_errors` 342–344), `crates/mkrew-ui/src/palette/tests.rs` (132, 288)
- Modify: `crates/mkrew-ui/src/shell/sessions.rs:353`, `shell/workspace_actions.rs:364-394`, `shell/navigation.rs:482,554`, `shell/connection.rs:336-344`, `main.rs:363`

**Interfaces:**
- Produces:
  - `Command::OpenSettings` (name `open_settings`, default `ctrl-,`, palette title `Global: Open settings`)
  - `pub fn write_binding(path: &Path, command: Command, keys: Option<&str>) -> Result<(Keymap, Vec<KeymapError>), KeymapError>`
  - `pub fn keys_label(keys: &str) -> String` ("ctrl-shift-k" → "Ctrl+Shift+K")
  - `pub fn notice(errors: &[KeymapError]) -> Option<String>` (the sidebar line, moved from `connection.rs`)
  - `Keymap::changed(&self, command: Command) -> bool`
  - `Shell::edit_settings_file(&mut self, cx)`, `Shell::edit_keymap_file(&mut self, cx)` (renamed from `open_settings` / `open_keymap_settings`)

- [ ] **Step 1: Write the failing tests** (append to `keymap.rs` `mod tests`)

```rust
    fn temp_keymap(text: Option<&str>) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keymap.toml");
        if let Some(text) = text {
            std::fs::write(&path, text).unwrap();
        }
        (dir, path)
    }

    #[test]
    fn write_binding_sets_replaces_and_resets() {
        let (_dir, path) = temp_keymap(None);
        let (keymap, _) = write_binding(&path, Command::ClosePane, Some("ctrl-shift-q")).unwrap();
        assert_eq!(keymap.label(Command::ClosePane), "Ctrl+Shift+Q");
        assert!(keymap.changed(Command::ClosePane));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# Mkrew keyboard shortcuts"), "template written first");
        assert!(text.contains("\nclose_pane = \"ctrl-shift-q\"\n"), "replaces the commented line");
        assert!(!text.contains("# close_pane ="));
        write_binding(&path, Command::ClosePane, Some("ctrl-alt-shift-y")).unwrap();
        let (loaded, errors) = Keymap::load(&path);
        assert!(errors.is_empty());
        assert_eq!(loaded.label(Command::ClosePane), "Ctrl+Alt+Shift+Y");
        let (keymap, _) = write_binding(&path, Command::ClosePane, None).unwrap();
        assert!(!keymap.changed(Command::ClosePane));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), template(), "back to the template");
    }

    #[test]
    fn write_binding_keeps_comments_and_crlf() {
        let (_dir, path) = temp_keymap(Some("# mine\r\n"));
        write_binding(&path, Command::ClosePane, Some("ctrl-shift-q")).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# mine\r\nclose_pane = \"ctrl-shift-q\"\r\n"
        );
    }

    #[test]
    fn a_duplicate_or_invalid_binding_is_refused_and_nothing_written() {
        let (_dir, path) = temp_keymap(Some("# mine\n"));
        let taken = Command::AddWorkspace.default_keys();
        match write_binding(&path, Command::ClosePane, Some(&taken)) {
            Err(KeymapError::Duplicate { other, .. }) => assert_eq!(other, "add_workspace"),
            other => panic!("expected a duplicate, got {other:?}"),
        }
        assert!(matches!(
            write_binding(&path, Command::ClosePane, Some("ctrl-")),
            Err(KeymapError::InvalidKeys { .. })
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# mine\n");
    }

    #[test]
    fn an_old_problem_does_not_block_a_new_binding() {
        let (_dir, path) = temp_keymap(Some("open_keymap_settings = \"ctrl-,\"\n"));
        let (keymap, errors) =
            write_binding(&path, Command::ClosePane, Some("ctrl-shift-q")).unwrap();
        assert_eq!(keymap.label(Command::ClosePane), "Ctrl+Shift+Q");
        assert_eq!(errors, vec![KeymapError::UnknownCommand("open_keymap_settings".into())]);
        // A file that is not TOML at all is not edited.
        std::fs::write(&path, "close_pane = \n").unwrap();
        assert!(matches!(
            write_binding(&path, Command::Find, Some("ctrl-alt-shift-y")),
            Err(KeymapError::Syntax(_))
        ));
    }

    #[test]
    fn keys_label_matches_the_shortcut_text() {
        assert_eq!(keys_label("ctrl-shift-k"), "Ctrl+Shift+K");
        assert_eq!(keys_label("nonsense key"), "nonsense key");
    }
```

In the existing tests (lines 799–824) replace `Command::OpenKeymapSettings` with `Command::OpenSettings`; keep the asserted keys (`ctrl-,`, `Ctrl+,`).

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p mkrew-ui --lib keymap`
Expected: FAIL to compile (`write_binding`, `OpenSettings` not found).

- [ ] **Step 3: Implement**

Rename the variant in the four places of `keymap.rs`: doc comment `/// Open the Settings dialog (ADR 0072).`, `Command::OpenSettings`, name `"open_settings"`, default `"ctrl-,"`. The template's first line becomes two lines:

```rust
        "# Mkrew keyboard shortcuts. The Settings dialog (Ctrl+,) writes this file; a hand edit\n\
         # applies after a restart. Uncomment a line and change its keys.\n\
```

(keep the other template lines as they are). After `ensure_file` add:

```rust
/// `src` with the line of `name` replaced by `line`: its uncommented line, else its commented
/// template line, else appended. Everything else is kept byte for byte (CRLF too).
fn set_binding(src: &str, name: &str, line: &str) -> String {
    let eol = if src.contains("\r\n") { "\r\n" } else { "\n" };
    let is_entry = |l: &str| {
        l.trim_start()
            .strip_prefix(name)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    };
    let mut lines: Vec<&str> = src.lines().collect();
    let at = lines.iter().position(|l| is_entry(l)).or_else(|| {
        lines
            .iter()
            .position(|l| l.trim_start().strip_prefix('#').is_some_and(is_entry))
    });
    match at {
        Some(i) => lines[i] = line,
        None => lines.push(line),
    }
    let mut out = lines.join(eol);
    if src.is_empty() || src.ends_with('\n') || at.is_none() {
        out.push_str(eol);
    }
    out
}

/// Saves `command`'s keystroke (`None`: back to its default, the template's commented line)
/// in `path` (the template when missing). Writes nothing when the result has a problem the
/// file did not have already (a duplicate, an invalid keystroke), or the file is not TOML.
/// Returns the new keymap and the problems the file still has.
pub fn write_binding(
    path: &Path,
    command: Command,
    keys: Option<&str>,
) -> Result<(Keymap, Vec<KeymapError>), KeymapError> {
    let read_error = |error: std::io::Error| KeymapError::Read {
        path: path.display().to_string(),
        message: error.to_string(),
    };
    let src = match std::fs::read_to_string(path) {
        Ok(src) => src,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => template(),
        Err(error) => return Err(read_error(error)),
    };
    let (_, before) = Keymap::parse(&src);
    if let Some(syntax) = before.iter().find(|e| matches!(e, KeymapError::Syntax(_))) {
        return Err(syntax.clone());
    }
    let name = command.name();
    let line = match keys {
        Some(keys) => format!("{name} = {}", toml::Value::String(keys.to_owned())),
        None => format!("# {name} = \"{}\"", command.default_keys()),
    };
    let out = set_binding(&src, &name, &line);
    let (keymap, after) = Keymap::parse(&out);
    if let Some(new) = after.iter().find(|e| !before.contains(e)) {
        return Err(new.clone());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(read_error)?;
    }
    std::fs::write(path, out).map_err(read_error)?;
    Ok((keymap, after))
}

/// `keys` as shortcut text ("Ctrl+Shift+K"); the raw text when it is not a keystroke.
pub fn keys_label(keys: &str) -> String {
    parse_keys(keys).map_or_else(|| keys.to_owned(), |keystroke| label(&keystroke))
}

/// The sidebar's one-line summary of `keymap.toml` problems.
pub fn notice(errors: &[KeymapError]) -> Option<String> {
    errors.first().map(|first| match errors.len() {
        1 => format!("{first}; using the default. Fix it in Settings (Ctrl+,)."),
        n => format!(
            "{first} (+{} more); using the defaults. Fix them in Settings (Ctrl+,).",
            n - 1
        ),
    })
}
```

The reset case relies on `set_binding` putting the template's commented line back in place: `template()` ends with `\n`, so the text equals `template()` again.

In `impl Keymap` add:

```rust
    /// Whether `command`'s binding differs from its SPEC §13 default.
    pub fn changed(&self, command: Command) -> bool {
        let default = Keystroke::parse(&command.default_keys()).ok();
        match (self.keystroke(command), default) {
            (Some(bound), Some(default)) => !same_keys(bound, &default),
            (bound, default) => bound.is_some() != default.is_some(),
        }
    }
```

`palette.rs`: title arm `Command::OpenSettings => "Global: Open settings".into()`; the global group arm lists `Command::OpenSettings`; `keymap_errors` uses `Action::Run(Command::OpenSettings)` and `keymap.label(Command::OpenSettings)`. `Extra::OpenSettings` stays in this task (Task 5 removes it). `palette/tests.rs` lines 132 and 288: `Command::OpenSettings`.

Shell call sites:
- `workspace_actions.rs`: rename `open_keymap_settings` → `edit_keymap_file` (doc: `/// Settings dialog "Edit keymap.toml in Zed": creates the commented template if missing and opens it in Zed; a failure shows in the sidebar.`) and `open_settings` → `edit_settings_file` (doc: same, for `settings.toml`).
- `sessions.rs:353`: `Command::OpenSettings => self.edit_keymap_file(cx),` (today's behavior; Task 5 points it at the dialog).
- `navigation.rs:482, 554`: `self.edit_settings_file(cx)`.
- `connection.rs:336-344`: `keymap_error: keymap::notice(keymap_errors),` (add `use mkrew_ui::keymap;` if not in scope via `crate::*`).
- `main.rs:363` doc: `/// `<config>/mkrew/keymap.toml` (the Settings dialog writes it).`

- [ ] **Step 4: Run the tests and clippy**

Run: `cargo test -p mkrew-ui --lib keymap palette` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: PASS, clean.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette/tests.rs crates/mkrew-ui/src/shell/sessions.rs crates/mkrew-ui/src/shell/navigation.rs crates/mkrew-ui/src/shell/workspace_actions.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/main.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): keymap writes and the open_settings command"
```

---

### Task 3: Pure dialog model

**Files:**
- Create: `crates/mkrew-ui/src/settings_dialog.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod settings_dialog;` after `pub mod scrollbar;`)

**Interfaces:**
- Consumes: `mkrew_core::settings_fields::{Page, Kind, Setting, SETTINGS}`, `keymap::{Command, Keymap, KeymapError, keys_label}`, `palette::title`, `task_dialog::{dialog_key, DialogKey}`, `text_box::{Input, Motion}`.
- Produces (used by Task 5):

```rust
pub enum Tab { Settings(Page), Keyboard }                       // Tab::all(), Tab::title()
pub enum Row { Setting(&'static Setting), Binding(Command) }    // Row::id() -> String
pub enum Focus { Tabs, Search, Row(usize) }
pub struct FontPick { pub setting: &'static Setting, pub query: Input, pub selected: usize }
pub enum Effect { None, Close, Write { setting: &'static Setting, text: String },
                  Bind { command: Command, keys: Option<String> } }
pub struct SettingsDialog { pub tab, pub focus, pub settings: Result<Settings, String>,
    pub fonts: Vec<String>, pub draft: Option<(&'static Setting, Input)>,
    pub errors: HashMap<String, String>, pub search: Input, pub capture: Option<Command>,
    pub font_pick: Option<FontPick> }
impl SettingsDialog {
    pub fn new(settings: Result<Settings, String>, fonts: Vec<String>) -> Self;
    pub fn read_only(&self) -> bool;
    pub fn rows(&self, keymap: &Keymap) -> Vec<Row>;
    pub fn shown(&self, setting: &Setting) -> String;
    pub fn key(&mut self, keystroke: &Keystroke, paste: Option<&str>, keymap: &Keymap) -> Effect;
    pub fn click(&mut self, focus: Focus, keymap: &Keymap) -> Effect;
    pub fn set_tab(&mut self, tab: Tab) -> Effect;
    pub fn choose(&mut self, setting: &'static Setting, text: &str) -> Effect;
    pub fn reset(&mut self, command: Command) -> Effect;
    pub fn font_choices(&self) -> Vec<String>;
    pub fn reload(&mut self, settings: Result<Settings, String>);
    pub fn written(&mut self, setting: &Setting, result: Result<(), String>);
    pub fn bound(&mut self, command: Command, result: Result<(), String>);
}
pub fn binding_error(error: &KeymapError) -> String;
```

- [ ] **Step 1: Write the failing tests** (`settings_dialog.rs`, `mod tests` only)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_core::settings::Settings;

    fn key(keys: &str) -> Keystroke {
        Keystroke::parse(keys).unwrap()
    }

    /// A key that types `text` (Space types " ").
    fn typed(text: &str) -> Keystroke {
        let mut keystroke = key(if text == " " { "space" } else { text });
        keystroke.key_char = Some(text.into());
        keystroke
    }

    fn dialog() -> SettingsDialog {
        SettingsDialog::new(Ok(Settings::default()), vec!["Inter".into(), "JetBrains Mono".into()])
    }

    fn setting(id: &str) -> &'static Setting {
        SETTINGS.iter().find(|s| s.id() == id).unwrap()
    }

    /// Puts the keyboard on the row of `id` with Tab / arrows only (no click: a click on a
    /// switch would toggle it).
    fn focus_on(d: &mut SettingsDialog, id: &str, keymap: &Keymap) {
        d.set_tab(Tab::Settings(setting(id).page));
        let index = d
            .rows(keymap)
            .iter()
            .position(|row| matches!(row, Row::Setting(s) if s.id() == id))
            .unwrap();
        d.key(&key("tab"), None, keymap);
        for _ in 0..index {
            d.key(&key("down"), None, keymap);
        }
        assert_eq!(d.focus, Focus::Row(index));
    }

    #[test]
    fn esc_cancels_capture_then_draft_then_closes() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.set_tab(Tab::Keyboard);
        assert_eq!(d.click(Focus::Row(0), &keymap), Effect::None);
        assert_eq!(d.capture, Some(Command::AddWorkspace));
        assert_eq!(d.key(&key("escape"), None, &keymap), Effect::None);
        assert_eq!(d.capture, None);
        focus_on(&mut d, "composer.file_ref", &keymap);
        d.key(&typed("x"), None, &keymap);
        assert_eq!(d.shown(setting("composer.file_ref")), "@{path}x");
        assert_eq!(d.key(&key("escape"), None, &keymap), Effect::None);
        assert_eq!(d.shown(setting("composer.file_ref")), "@{path}");
        assert_eq!(d.key(&key("escape"), None, &keymap), Effect::Close);
    }

    #[test]
    fn a_captured_keystroke_becomes_a_binding() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.set_tab(Tab::Keyboard);
        d.click(Focus::Row(0), &keymap);
        assert_eq!(
            d.key(&key("ctrl-shift-y"), None, &keymap),
            Effect::Bind { command: Command::AddWorkspace, keys: Some("ctrl-shift-y".into()) }
        );
        assert_eq!(d.capture, None);
    }

    #[test]
    fn enter_writes_a_changed_draft_and_leaving_the_row_does_too() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        focus_on(&mut d, "ui.font_size", &keymap);
        d.key(&key("backspace"), None, &keymap);
        d.key(&typed("4"), None, &keymap);
        assert_eq!(
            d.key(&key("enter"), None, &keymap),
            Effect::Write { setting: setting("ui.font_size"), text: "14".into() }
        );
        d.key(&key("backspace"), None, &keymap);
        d.key(&typed("5"), None, &keymap);
        assert_eq!(
            d.key(&key("tab"), None, &keymap),
            Effect::Write { setting: setting("ui.font_size"), text: "15".into() }
        );
        // An unchanged draft writes nothing.
        focus_on(&mut d, "ui.font_size", &keymap);
        assert_eq!(d.key(&key("enter"), None, &keymap), Effect::None);
    }

    #[test]
    fn space_toggles_a_bool_and_enter_cycles_a_choice() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        focus_on(&mut d, "ui.diff_wrap", &keymap);
        assert_eq!(
            d.key(&typed(" "), None, &keymap),
            Effect::Write { setting: setting("ui.diff_wrap"), text: "false".into() }
        );
        focus_on(&mut d, "ui.theme", &keymap);
        assert_eq!(
            d.key(&key("enter"), None, &keymap),
            Effect::Write { setting: setting("ui.theme"), text: "light".into() }
        );
    }

    #[test]
    fn a_broken_file_makes_settings_read_only_but_not_keys() {
        let keymap = Keymap::defaults();
        let mut d = SettingsDialog::new(Err("settings.toml:2:1: oops".into()), Vec::new());
        assert!(d.read_only());
        focus_on(&mut d, "ui.diff_wrap", &keymap);
        assert_eq!(d.key(&typed(" "), None, &keymap), Effect::None);
        assert_eq!(d.choose(setting("ui.theme"), "dark"), Effect::None);
        assert_eq!(d.reset(Command::ClosePane), Effect::Bind { command: Command::ClosePane, keys: None });
    }

    #[test]
    fn reload_keeps_the_open_draft() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        focus_on(&mut d, "composer.file_ref", &keymap);
        d.key(&typed("x"), None, &keymap);
        let mut edited = Settings::default();
        edited.ui.diff_wrap = false;
        d.reload(Ok(edited));
        assert_eq!(d.shown(setting("composer.file_ref")), "@{path}x");
        assert_eq!(d.shown(setting("ui.diff_wrap")), "false");
    }

    #[test]
    fn keyboard_search_filters_by_title_and_by_shortcut() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.set_tab(Tab::Keyboard);
        d.click(Focus::Search, &keymap);
        for c in ["t", "h", "e", "m", "e"] {
            d.key(&typed(c), None, &keymap);
        }
        assert_eq!(d.rows(&keymap), vec![Row::Binding(Command::ToggleTheme)]);
        d.search = Input::from("ctrl+,");
        assert_eq!(d.rows(&keymap), vec![Row::Binding(Command::OpenSettings)]);
    }

    #[test]
    fn the_font_pick_lists_default_first_and_filters() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        focus_on(&mut d, "ui.font_family", &keymap);
        d.key(&key("enter"), None, &keymap);
        assert_eq!(d.font_choices(), vec!["", "Inter", "JetBrains Mono"]);
        for c in ["m", "o", "n"] {
            d.key(&typed(c), None, &keymap);
        }
        assert_eq!(d.font_choices(), vec!["JetBrains Mono"]);
        assert_eq!(
            d.key(&key("enter"), None, &keymap),
            Effect::Write { setting: setting("ui.font_family"), text: "JetBrains Mono".into() }
        );
        assert!(d.font_pick.is_none());
    }

    #[test]
    fn a_duplicate_names_the_other_command() {
        let error = KeymapError::Duplicate {
            command: "close_pane".into(),
            keys: "ctrl-shift-k".into(),
            other: "add_workspace".into(),
        };
        assert_eq!(
            binding_error(&error),
            format!("Ctrl+Shift+K is used by \"{}\"", palette::title(Command::AddWorkspace))
        );
    }
}
```

If more than one command title contains "theme", change the search word in `keyboard_search_…` to one only `ToggleTheme`'s title has (read `palette::title`).

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p mkrew-ui --lib settings_dialog`
Expected: FAIL to compile.

- [ ] **Step 3: Implement** (above the tests)

```rust
//! The Settings dialog (SPEC §20.0, §13, ADR 0072): pages of `settings.toml` rows built from
//! `mkrew_core::settings_fields::SETTINGS`, and a Keyboard page of every command. Pure: the
//! shell turns an [`Effect`] into a file write and feeds the result back.

use std::collections::HashMap;

use gpui::Keystroke;
use mkrew_core::settings::Settings;
use mkrew_core::settings_fields::{Kind, Page, SETTINGS, Setting};

use crate::keymap::{Command, Keymap, KeymapError, keys_label};
use crate::palette;
use crate::task_dialog::{DialogKey, dialog_key};
use crate::text_box::{Input, Motion};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Settings(Page),
    Keyboard,
}

impl Tab {
    pub fn all() -> Vec<Tab> {
        let mut all: Vec<Tab> = Page::ALL.into_iter().map(Tab::Settings).collect();
        all.push(Tab::Keyboard);
        all
    }

    pub fn title(self) -> &'static str {
        match self {
            Tab::Settings(page) => page.title(),
            Tab::Keyboard => "Keyboard",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Setting(&'static Setting),
    Binding(Command),
}

impl Row {
    /// Key of the row's error.
    pub fn id(&self) -> String {
        match self {
            Row::Setting(setting) => setting.id(),
            Row::Binding(command) => command.name(),
        }
    }

    fn is_text(&self) -> bool {
        matches!(self, Row::Setting(s) if matches!(s.kind, Kind::Text | Kind::Number))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The page list on the left.
    Tabs,
    /// The Keyboard page's search box.
    Search,
    Row(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FontPick {
    pub setting: &'static Setting,
    pub query: Input,
    pub selected: usize,
}

/// What the shell does after a key or click.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    None,
    Close,
    Write { setting: &'static Setting, text: String },
    /// `None` keys: back to the default.
    Bind { command: Command, keys: Option<String> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsDialog {
    pub tab: Tab,
    pub focus: Focus,
    /// `Err`: the file does not parse; the settings pages are read-only.
    pub settings: Result<Settings, String>,
    /// Installed font families, for the font pick.
    pub fonts: Vec<String>,
    /// The focused text or number row's text while it has the keyboard.
    pub draft: Option<(&'static Setting, Input)>,
    /// Per row id: why its last write failed.
    pub errors: HashMap<String, String>,
    pub search: Input,
    /// The command waiting for its new keystroke.
    pub capture: Option<Command>,
    pub font_pick: Option<FontPick>,
}

impl SettingsDialog {
    pub fn new(settings: Result<Settings, String>, fonts: Vec<String>) -> Self {
        Self {
            tab: Tab::Settings(Page::General),
            focus: Focus::Tabs,
            settings,
            fonts,
            draft: None,
            errors: HashMap::new(),
            search: Input::default(),
            capture: None,
            font_pick: None,
        }
    }

    pub fn read_only(&self) -> bool {
        self.settings.is_err()
    }

    pub fn rows(&self, keymap: &Keymap) -> Vec<Row> {
        match self.tab {
            Tab::Settings(page) => SETTINGS
                .iter()
                .filter(|s| s.page == page && !s.hidden)
                .map(Row::Setting)
                .collect(),
            Tab::Keyboard => {
                let query = self.search.trim().to_lowercase();
                Command::all()
                    .into_iter()
                    .filter(|&command| {
                        query.is_empty()
                            || palette::title(command).to_lowercase().contains(&query)
                            || keymap.label(command).to_lowercase().contains(&query)
                    })
                    .map(Row::Binding)
                    .collect()
            }
        }
    }

    /// The value the file holds (the defaults when the file is broken).
    fn current(&self, setting: &Setting) -> String {
        match &self.settings {
            Ok(settings) => setting.current(settings),
            Err(_) => setting.current(&Settings::default()),
        }
    }

    /// What the row shows: the draft while it is edited, else the file's value.
    pub fn shown(&self, setting: &Setting) -> String {
        match &self.draft {
            Some((edited, input)) if edited.id() == setting.id() => input.to_string(),
            _ => self.current(setting),
        }
    }

    fn write(&self, setting: &'static Setting, text: String) -> Effect {
        if self.read_only() {
            return Effect::None;
        }
        Effect::Write { setting, text }
    }

    /// Ends the draft; a changed one is written.
    fn commit_draft(&mut self) -> Effect {
        match self.draft.take() {
            Some((setting, input)) if *input != *self.current(setting) => {
                self.write(setting, input.to_string())
            }
            _ => Effect::None,
        }
    }

    /// Moves the keyboard to `focus`, writing a draft it leaves; a text row starts a draft.
    fn move_focus(&mut self, focus: Focus, keymap: &Keymap) -> Effect {
        let effect = self.commit_draft();
        self.capture = None;
        self.font_pick = None;
        self.focus = focus;
        if let Focus::Row(i) = focus
            && let Some(Row::Setting(setting)) = self.rows(keymap).get(i).copied()
            && matches!(setting.kind, Kind::Text | Kind::Number)
        {
            self.draft = Some((setting, Input::from(self.current(setting))));
        }
        effect
    }

    pub fn set_tab(&mut self, tab: Tab) -> Effect {
        let effect = self.commit_draft();
        self.tab = tab;
        self.focus = Focus::Tabs;
        self.capture = None;
        self.font_pick = None;
        effect
    }

    /// Enter / Space / a click on the focused row.
    fn activate(&mut self, keymap: &Keymap) -> Effect {
        let Focus::Row(i) = self.focus else {
            return Effect::None;
        };
        match self.rows(keymap).get(i).copied() {
            Some(Row::Binding(command)) => {
                self.capture = Some(command);
                Effect::None
            }
            Some(Row::Setting(setting)) => match setting.kind {
                Kind::Bool => {
                    let on = self.current(setting) == "true";
                    self.write(setting, (!on).to_string())
                }
                Kind::Choice(choices) => {
                    let now = self.current(setting);
                    let at = choices.iter().position(|c| *c == now).unwrap_or(0);
                    self.write(setting, choices[(at + 1) % choices.len()].to_owned())
                }
                Kind::Font if !self.read_only() => {
                    self.font_pick = Some(FontPick { setting, query: Input::default(), selected: 0 });
                    Effect::None
                }
                Kind::Font => Effect::None,
                // Enter in a text row: write it and keep editing.
                Kind::Text | Kind::Number => {
                    let effect = self.commit_draft();
                    let text = match &effect {
                        Effect::Write { text, .. } => text.clone(),
                        _ => self.current(setting),
                    };
                    self.draft = Some((setting, Input::from(text)));
                    effect
                }
            },
            None => Effect::None,
        }
    }

    /// A click: focus, then act like Enter (a text row only takes the keyboard). A click that
    /// leaves a changed draft writes that draft first; the next click acts.
    pub fn click(&mut self, focus: Focus, keymap: &Keymap) -> Effect {
        let left = self.move_focus(focus, keymap);
        if left != Effect::None {
            return left;
        }
        match focus {
            Focus::Row(i) if !self.rows(keymap).get(i).is_some_and(Row::is_text) => {
                self.activate(keymap)
            }
            _ => Effect::None,
        }
    }

    pub fn choose(&mut self, setting: &'static Setting, text: &str) -> Effect {
        self.font_pick = None;
        self.write(setting, text.to_owned())
    }

    pub fn reset(&mut self, command: Command) -> Effect {
        self.capture = None;
        Effect::Bind { command, keys: None }
    }

    /// `""` (Default) first, then the installed fonts matching the pick's query.
    pub fn font_choices(&self) -> Vec<String> {
        let query = self
            .font_pick
            .as_ref()
            .map(|pick| pick.query.trim().to_lowercase())
            .unwrap_or_default();
        let mut choices = Vec::new();
        if query.is_empty() {
            choices.push(String::new());
        }
        choices.extend(
            self.fonts
                .iter()
                .filter(|font| font.to_lowercase().contains(&query))
                .cloned(),
        );
        choices
    }

    fn step_focus(&mut self, back: bool, keymap: &Keymap) -> Effect {
        let mut order = vec![Focus::Tabs];
        if self.tab == Tab::Keyboard {
            order.push(Focus::Search);
        }
        order.extend((0..self.rows(keymap).len()).map(Focus::Row));
        let at = order.iter().position(|f| *f == self.focus).unwrap_or(0);
        let next = if back {
            at.checked_sub(1).unwrap_or(order.len() - 1)
        } else {
            (at + 1) % order.len()
        };
        self.move_focus(order[next], keymap)
    }

    fn font_key(&mut self, key: DialogKey) -> Effect {
        let choices = self.font_choices();
        let last = choices.len().saturating_sub(1);
        let Some(pick) = self.font_pick.as_mut() else {
            return Effect::None;
        };
        match key {
            DialogKey::Cancel => self.font_pick = None,
            DialogKey::Move { motion: Motion::Up, .. } => pick.selected = pick.selected.saturating_sub(1),
            DialogKey::Move { motion: Motion::Down, .. } => pick.selected = (pick.selected + 1).min(last),
            DialogKey::Enter | DialogKey::Save => {
                let setting = pick.setting;
                let choice = choices.get(pick.selected).cloned();
                self.font_pick = None;
                return choice.map_or(Effect::None, |font| self.write(setting, font));
            }
            DialogKey::Insert(text) => {
                pick.query.insert(&text);
                pick.selected = 0;
            }
            other => {
                if pick.query.edit(&other) {
                    pick.selected = 0;
                }
            }
        }
        Effect::None
    }

    pub fn key(&mut self, keystroke: &Keystroke, paste: Option<&str>, keymap: &Keymap) -> Effect {
        if let Some(command) = self.capture.take() {
            if keystroke.key == "escape" && keystroke.modifiers == Default::default() {
                return Effect::None;
            }
            return Effect::Bind { command, keys: Some(keystroke.unparse()) };
        }
        let Some(key) = dialog_key(keystroke) else {
            return Effect::None;
        };
        if self.font_pick.is_some() {
            return self.font_key(key);
        }
        match key {
            DialogKey::Cancel => {
                if let Some((setting, input)) = &self.draft
                    && **input != *self.current(setting)
                {
                    let setting = *setting;
                    self.draft = Some((setting, Input::from(self.current(setting))));
                    return Effect::None;
                }
                if self.focus == Focus::Search && !self.search.is_empty() {
                    self.search = Input::default();
                    return Effect::None;
                }
                Effect::Close
            }
            DialogKey::SwitchField => self.step_focus(keystroke.modifiers.shift, keymap),
            DialogKey::Move { motion: motion @ (Motion::Up | Motion::Down), extend: false } => {
                let up = motion == Motion::Up;
                match self.focus {
                    Focus::Tabs => {
                        let tabs = Tab::all();
                        let at = tabs.iter().position(|t| *t == self.tab).unwrap_or(0);
                        let next = if up { at.saturating_sub(1) } else { (at + 1).min(tabs.len() - 1) };
                        self.set_tab(tabs[next])
                    }
                    Focus::Row(i) => {
                        let last = self.rows(keymap).len().saturating_sub(1);
                        let next = if up { i.saturating_sub(1) } else { (i + 1).min(last) };
                        self.move_focus(Focus::Row(next), keymap)
                    }
                    Focus::Search => Effect::None,
                }
            }
            DialogKey::Enter | DialogKey::Save => self.activate(keymap),
            DialogKey::Insert(text)
                if text == " " && self.draft.is_none() && self.focus != Focus::Search =>
            {
                self.activate(keymap)
            }
            DialogKey::Paste => {
                if let Some(text) = paste {
                    self.edit_text(|input| {
                        input.insert(text);
                    });
                }
                Effect::None
            }
            DialogKey::Insert(text) => {
                self.edit_text(|input| {
                    input.insert(&text);
                });
                Effect::None
            }
            other => {
                self.edit_text(|input| {
                    input.edit(&other);
                });
                Effect::None
            }
        }
    }

    /// Applies `edit` to the text that has the keyboard: the search box or the draft.
    fn edit_text(&mut self, edit: impl FnOnce(&mut Input)) {
        if self.focus == Focus::Search {
            edit(&mut self.search);
        } else if let Some((_, input)) = &mut self.draft {
            edit(input);
        }
    }

    /// The file changed (a write, a hand edit): new values; the open draft stays.
    pub fn reload(&mut self, settings: Result<Settings, String>) {
        self.settings = settings;
    }

    pub fn written(&mut self, setting: &Setting, result: Result<(), String>) {
        match result {
            Ok(()) => self.errors.remove(&setting.id()),
            Err(error) => self.errors.insert(setting.id(), error),
        };
    }

    pub fn bound(&mut self, command: Command, result: Result<(), String>) {
        match result {
            Ok(()) => self.errors.remove(&command.name()),
            Err(error) => self.errors.insert(command.name(), error),
        };
    }
}

/// A refused binding, for its row.
pub fn binding_error(error: &KeymapError) -> String {
    match error {
        KeymapError::Duplicate { keys, other, .. } => match Command::from_name(other) {
            Some(other) => format!("{} is used by \"{}\"", keys_label(keys), palette::title(other)),
            None => error.to_string(),
        },
        KeymapError::InvalidKeys { keys, .. } => format!("{keys} cannot be a shortcut"),
        other => other.to_string(),
    }
}
```

Check before compiling: `text_box::Motion` is `pub` with `Up` / `Down` variants and `PartialEq` (`task_dialog.rs` uses `Motion::Up`); `Input: Default`; `Input::insert` returns `bool` (ignored on purpose). Fix borrow-checker complaints by copying into locals; keep the behavior.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-ui --lib settings_dialog`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/settings_dialog.rs crates/mkrew-ui/src/lib.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): settings dialog model"
```

---

### Task 4: Apply settings on demand, rebind from the current keymap

**Files:**
- Modify: `crates/mkrew-ui/src/shell/connection.rs:79-121`
- Modify: `crates/mkrew-ui/src/main.rs:174-185`, add `ActiveKeymap`

**Interfaces:**
- Produces:
  - `pub(crate) struct ActiveKeymap(pub Keymap); impl gpui::Global for ActiveKeymap {}` in `main.rs`
  - `Shell::apply_settings_file(&mut self, window: &mut Window, cx: &mut Context<'_, Self>)` — re-reads `settings.toml` and applies fonts, theme, diff, tray, Jira, reading width, features, `settings_error`.

- [ ] **Step 1: Extract `apply_settings_file`**

Move the body inside `if modified != shell.settings_modified { … }` (lines 84–120: from `let installed = …` to the closing `cx.notify();`) into a new method in `connection.rs`'s `impl Shell`, with `shell` renamed to `self`:

```rust
    /// Re-reads `settings.toml` and applies what the UI reads from it: fonts, theme, diff
    /// layout and wrap, tray, taskbar flash, Jira, reading width, features (§20.0). A file that
    /// does not parse keeps the last good settings; the palette lists why.
    pub(crate) fn apply_settings_file(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let installed = cx.text_system().all_font_names();
        let last = cx.global::<theme::Fonts>().clone();
        let mode = cx.theme().mode;
        match theme::reload(&self.settings_file, &installed, &last, mode) {
            // the existing `Ok(changed) => { … }` and `Err(error) => …` arms, unchanged
            // except `shell.` → `self.`
        }
        cx.notify();
    }
```

The tick becomes:

```rust
                    if modified != shell.settings_modified {
                        shell.settings_modified = modified;
                        shell.apply_settings_file(window, cx);
                    }
```

- [ ] **Step 2: The layout observer reads the current keymap**

`main.rs`, beside `struct Shell`:

```rust
/// The bindings in force; the keyboard layout observer rebinds from it, so keys set in the
/// Settings dialog survive a layout switch.
pub(crate) struct ActiveKeymap(pub Keymap);

impl gpui::Global for ActiveKeymap {}
```

Replace lines 175–185 with:

```rust
            let bindings = keymap.key_bindings(cx.keyboard_mapper().as_ref());
            cx.bind_keys(bindings);
            cx.set_global(ActiveKeymap(keymap.clone()));
            // Which char a shifted digit reports depends on the layout: bind again after a switch
            // (e.g. US ↔ Vietnamese), or Ctrl+Shift+1..5 stop working until a restart.
            cx.on_keyboard_layout_change(|cx| {
                let bindings = cx
                    .global::<ActiveKeymap>()
                    .0
                    .key_bindings(cx.keyboard_mapper().as_ref());
                cx.clear_key_bindings();
                cx.bind_keys(bindings);
            })
            .detach();
```

Run `grep -rn "bind_keys" crates/mkrew-ui/src`: `main.rs` must be the only place binding keys (`clear_key_bindings` drops all of them; the layout observer already relies on that).

- [ ] **Step 3: Build, test, clippy**

Run: `cargo test -p mkrew-ui --lib` and `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: PASS, clean.

- [ ] **Step 4: Commit**

```bash
git add crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/main.rs
git -c user.email=tungnv975@gmail.com commit -m "refactor(ui): apply settings on demand, rebind from the active keymap"
```

---

### Task 5: The dialog in the shell

**Files:**
- Create: `crates/mkrew-ui/src/shell/settings.rs`
- Modify: `crates/mkrew-ui/src/shell.rs` (`mod settings;` in the list), `shell/focus.rs:7-24`, `shell/sessions.rs:353`, `shell/connection.rs` (`Shell::new` fields, `apply_settings_file`), `shell/navigation.rs:391,482,554`, `shell/workspace_actions.rs` (`set_keymap`), `palette.rs` + `palette/tests.rs` (drop `Extra::OpenSettings`), `main.rs` (fields, import, render child), `theme.rs`

**Interfaces:**
- Consumes: Tasks 1–4.
- Produces: `Shell::open_settings_dialog(window, cx)`, `close_settings_dialog`, `settings_key_down`, `apply_settings_effect`, `set_keymap`, `render_settings_dialog(&self, dialog, viewport_height: Pixels, theme, cx) -> AnyElement`; `palette::settings_error(keymap: &Keymap, error: Option<&str>)`.

- [ ] **Step 1: Palette: one way to open settings**

`palette.rs`: remove `Extra::OpenSettings` (variant, `ALL` entry → `pub const ALL: [Self; 17]`, its title, its `available` arm). `settings_error` runs the command:

```rust
/// The `settings.toml` problem (SPEC §20.0), listed before the commands like keymap errors;
/// picking it opens the Settings dialog, which shows the problem and "Fix in Zed".
pub fn settings_error(keymap: &Keymap, error: Option<&str>) -> Vec<Entry> {
    error
        .map(|error| Entry {
            action: Action::Run(Command::OpenSettings),
            title: format!("Settings error: {error}"),
            shortcut: keymap.label(Command::OpenSettings),
        })
        .into_iter()
        .collect()
}
```

`palette/tests.rs`: `a_settings_error_opens_the_settings` calls `settings_error(&Keymap::defaults(), Some(error))` and asserts `Action::Run(Command::OpenSettings)`; the check at line 301 becomes `e.action == Action::Run(Command::OpenSettings)`; drop `Extra::OpenSettings` from the match at line 356. `navigation.rs:391`: `palette::settings_error(&self.keymap, self.settings_error.as_deref())`; delete the `Extra::OpenSettings` arms at 482 and 554.

- [ ] **Step 2: Theme tokens** (`theme.rs`, after `DIALOG_WIDTH`)

```rust
/// The Settings dialog (ADR 0072): its size, the page list width and the value column.
pub const SETTINGS_DIALOG_WIDTH: f32 = 760.0;
pub const SETTINGS_DIALOG_HEIGHT: f32 = 560.0;
pub const SETTINGS_NAV_WIDTH: f32 = 180.0;
pub const SETTINGS_CONTROL_WIDTH: f32 = 240.0;
/// Tallest the font pick's list grows (it scrolls past that).
pub const SETTINGS_FONT_LIST_HEIGHT: f32 = 200.0;
/// An on/off switch: track size and the knob's inset.
pub const SWITCH_WIDTH: f32 = 32.0;
pub const SWITCH_HEIGHT: f32 = 18.0;
pub const SWITCH_INSET: f32 = 2.0;
```

- [ ] **Step 3: Shell state and entry points**

`main.rs` `struct Shell`, after `phone_open`:

```rust
    /// The open Settings dialog (modal, ADR 0072).
    settings_dialog: Option<SettingsDialog>,
    /// Scrolls its rows.
    settings_scroll: ScrollHandle,
```

Add `use mkrew_ui::settings_dialog::{self, SettingsDialog};`. `connection.rs` `Shell::new`: `settings_dialog: None, settings_scroll: ScrollHandle::new(),`. `focus.rs::modal_open`: add `|| self.settings_dialog.is_some()`. `apply_settings_file`, before its `cx.notify()`:

```rust
        if let Some(dialog) = &mut self.settings_dialog {
            dialog.reload(
                mkrew_core::settings::load(&self.settings_file).map_err(|error| error.to_string()),
            );
        }
```

Render, beside the phone child in `main.rs`'s `render`:

```rust
                    .children(self.settings_dialog.as_ref().map(|dialog| {
                        self.render_settings_dialog(dialog, window.viewport_size().height, &theme, cx)
                    }))
```

`sessions.rs:353`: `Command::OpenSettings => self.open_settings_dialog(window, cx),`.

`workspace_actions.rs`, next to `edit_keymap_file`:

```rust
    /// Makes `keymap` the bound shortcuts at once (the Settings dialog wrote it): GPUI's key
    /// bindings, tooltips, the palette and the sidebar notice.
    pub(crate) fn set_keymap(
        &mut self,
        keymap: Keymap,
        errors: Vec<KeymapError>,
        cx: &mut Context<'_, Self>,
    ) {
        let bindings = keymap.key_bindings(cx.keyboard_mapper().as_ref());
        cx.clear_key_bindings();
        cx.bind_keys(bindings);
        cx.set_global(ActiveKeymap(keymap.clone()));
        self.keymap_error = keymap::notice(&errors);
        self.keymap_errors = errors;
        self.keymap = keymap;
        cx.notify();
    }
```

- [ ] **Step 4: Open, close, keys, effects** (`shell/settings.rs`)

```rust
//! The Settings dialog (ADR 0072): every `settings.toml` key and every shortcut, applied at
//! once. The model is `mkrew_ui::settings_dialog`; this file writes the files and draws it.

use crate::*;
use mkrew_core::settings_fields::{Kind, Setting};
use mkrew_ui::settings_dialog::{Effect, Focus, Row, Tab};

impl Shell {
    /// Ctrl+, and the palette's "Global: Open settings". Needs no daemon.
    pub(crate) fn open_settings_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.close_menus();
        let settings =
            mkrew_core::settings::load(&self.settings_file).map_err(|error| error.to_string());
        let mut fonts: Vec<String> = cx.text_system().all_font_names();
        fonts.sort_unstable_by_key(|font| font.to_lowercase());
        fonts.dedup();
        self.settings_dialog = Some(SettingsDialog::new(settings, fonts));
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    pub(crate) fn close_settings_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.settings_dialog = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    pub(crate) fn settings_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let paste = (task_dialog::dialog_key(&event.keystroke) == Some(DialogKey::Paste))
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let keymap = self.keymap.clone();
        let Some(dialog) = self.settings_dialog.as_mut() else {
            return;
        };
        let effect = dialog.key(&event.keystroke, paste.as_deref(), &keymap);
        self.apply_settings_effect(effect, window, cx);
    }

    /// Writes what the dialog asked for and feeds the result back to it.
    pub(crate) fn apply_settings_effect(
        &mut self,
        effect: Effect,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match effect {
            Effect::None => {}
            Effect::Close => return self.close_settings_dialog(window, cx),
            Effect::Write { setting, text } => {
                let result = setting
                    .write(&self.settings_file, &text)
                    .map_err(|error| error.to_string());
                let ok = result.is_ok();
                if let Some(dialog) = &mut self.settings_dialog {
                    dialog.written(setting, result);
                }
                if ok {
                    // Now, not at the next mtime poll (or window focus).
                    self.settings_modified = std::fs::metadata(&self.settings_file)
                        .and_then(|m| m.modified())
                        .ok();
                    self.apply_settings_file(window, cx);
                }
            }
            Effect::Bind { command, keys } => {
                let result = keymap::write_binding(&self.keymap_file, command, keys.as_deref());
                let reply = match result {
                    Ok((keymap, errors)) => {
                        self.set_keymap(keymap, errors, cx);
                        Ok(())
                    }
                    Err(error) => Err(settings_dialog::binding_error(&error)),
                };
                if let Some(dialog) = &mut self.settings_dialog {
                    dialog.bound(command, reply);
                }
            }
        }
        cx.notify();
    }

    fn settings_do(
        &mut self,
        act: impl FnOnce(&mut SettingsDialog, &Keymap) -> Effect,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.focus(&self.dialog_focus, cx);
        let keymap = self.keymap.clone();
        if let Some(effect) = self.settings_dialog.as_mut().map(|d| act(d, &keymap)) {
            self.apply_settings_effect(effect, window, cx);
        }
    }
}
```

- [ ] **Step 5: Render** (same file)

```rust
impl Shell {
    pub(crate) fn render_settings_dialog(
        &self,
        dialog: &SettingsDialog,
        viewport_height: Pixels,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let tabs = Tab::all().into_iter().map(|tab| {
            let active = tab == dialog.tab;
            let focused = active && dialog.focus == Focus::Tabs;
            div()
                .id(SharedString::from(format!("settings-tab-{}", tab.title())))
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .cursor_pointer()
                .text_size(font_size::BODY)
                .text_color(rgb(if active { theme.text } else { theme.text_muted }))
                .when(active, |el| el.bg(rgb(theme.surface_raised)))
                .when(focused, |el| {
                    el.border(px(theme::BORDER_WIDTH)).border_color(rgb(theme.selected_ring))
                })
                .hover(|style| style.bg(rgb(theme.surface_raised)))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    shell.settings_do(|d, _| d.set_tab(tab), window, cx);
                }))
                .child(tab.title())
        });
        let nav = div()
            .w(px(theme::SETTINGS_NAV_WIDTH))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .p(px(space::M))
            .border_r(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .children(tabs);

        let banner = match (&dialog.settings, dialog.tab) {
            (Err(error), Tab::Settings(_)) => Some(self.settings_banner(error.clone(), true, theme, cx)),
            (_, Tab::Keyboard) => keymap::notice(&self.keymap_errors)
                .map(|notice| self.settings_banner(notice, false, theme, cx)),
            _ => None,
        };
        let search = (dialog.tab == Tab::Keyboard).then(|| {
            text_field(
                "settings-search",
                dialog.search.as_str(),
                "Search shortcuts",
                dialog.focus == Focus::Search,
                false,
                theme,
            )
            .on_click(cx.listener(|shell, _, window, cx| {
                shell.settings_do(|d, keymap| d.click(Focus::Search, keymap), window, cx);
            }))
        });
        let rows = dialog.rows(&self.keymap);
        let body = div()
            .id("settings-rows")
            .flex_1()
            .min_w_0()
            .overflow_y_scroll()
            .track_scroll(&self.settings_scroll)
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .p(px(space::M))
            .children(banner)
            .children(search)
            .children(
                rows.iter()
                    .enumerate()
                    .map(|(i, row)| self.settings_row(dialog, i, *row, theme, cx)),
            );

        let footer = div()
            .flex()
            .items_center()
            .gap(px(space::L))
            .px(px(space::L))
            .py(px(space::M))
            .border_t(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .text_size(font_size::META)
            .child(self.settings_link("settings-edit-toml", "Edit settings.toml in Zed", theme, cx, |shell, cx| {
                shell.edit_settings_file(cx);
            }))
            .child(self.settings_link("settings-edit-keymap", "Edit keymap.toml in Zed", theme, cx, |shell, cx| {
                shell.edit_keymap_file(cx);
            }))
            .child(div().flex_1())
            .child(div().text_color(rgb(theme.text_muted)).child("Changes apply at once · Esc closes"));

        let max_height = (viewport_height - px(space::XXL * 2.0)).max(px(0.0));
        let panel = div()
            .id("settings-dialog")
            .w(px(theme::SETTINGS_DIALOG_WIDTH))
            .h(px(theme::SETTINGS_DIALOG_HEIGHT))
            .max_h(max_height)
            .flex()
            .flex_col()
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::settings_key_down))
            .relative()
            .child(self.typing(&self.dialog_focus, Self::settings_key_down, cx))
            .child(
                div()
                    .px(px(space::L))
                    .py(px(space::M))
                    .border_b(px(theme::BORDER_WIDTH))
                    .border_color(rgb(theme.divider))
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Settings"),
            )
            .child(div().flex().flex_1().min_h_0().child(nav).child(body))
            .child(footer);
        Self::modal_shade("settings-shade", panel, cx)
    }

    /// The broken-file / keymap-problem line above the rows, with "Fix in Zed".
    fn settings_banner(
        &self,
        text: String,
        settings_file: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .p(px(space::S))
            .rounded(px(radius::MEDIUM))
            .bg(rgb(theme.surface_raised))
            .text_size(font_size::META)
            .child(div().text_color(rgb(theme.status_exited)).child("✕"))
            .child(div().flex_1().min_w_0().text_color(rgb(theme.text)).child(text))
            .child(self.settings_link("settings-fix", "Fix in Zed", theme, cx, move |shell, cx| {
                if settings_file {
                    shell.edit_settings_file(cx);
                } else {
                    shell.edit_keymap_file(cx);
                }
            }))
    }

    fn settings_link(
        &self,
        id: &'static str,
        label: &'static str,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
        on_click: impl Fn(&mut Self, &mut Context<'_, Self>) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .cursor_pointer()
            .text_color(rgb(theme.accent_text))
            .hover(|style| style.underline())
            .on_click(cx.listener(move |shell, _, _, cx| on_click(shell, cx)))
            .child(label)
    }

    fn settings_row(
        &self,
        dialog: &SettingsDialog,
        index: usize,
        row: Row,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let focused = dialog.focus == Focus::Row(index);
        let error = dialog.errors.get(&row.id()).cloned();
        let (title, help, control): (SharedString, Option<String>, gpui::AnyElement) = match row {
            Row::Setting(setting) => (
                setting.label.into(),
                (!setting.help.is_empty()).then(|| setting.help.to_owned()),
                self.settings_control(dialog, setting, focused, theme, cx),
            ),
            Row::Binding(command) => (
                palette::title(command).into(),
                command.context().map(|context| format!("{} only", context_name(context))),
                self.binding_control(dialog, command, theme, cx),
            ),
        };
        let is_default = match row {
            Row::Setting(setting) => dialog.settings.as_ref().is_ok_and(|settings| {
                setting.current(settings) == setting.current(&Default::default())
            }),
            Row::Binding(_) => false,
        };
        div()
            .id(SharedString::from(format!("settings-row-{}", row.id())))
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .p(px(space::S))
            .rounded(px(radius::MEDIUM))
            .when(focused, |el| el.bg(rgb(theme.surface_raised)))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.settings_do(|d, keymap| d.click(Focus::Row(index), keymap), window, cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .gap(px(space::S))
                                    .text_size(font_size::BODY)
                                    .text_color(rgb(theme.text))
                                    .child(title)
                                    .when(is_default, |el| {
                                        el.child(
                                            div()
                                                .text_size(font_size::META)
                                                .text_color(rgb(theme.text_muted))
                                                .child("default"),
                                        )
                                    }),
                            )
                            .children(help.map(|help| meta_line(theme, help.into()))),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(theme::SETTINGS_CONTROL_WIDTH))
                            .flex()
                            .justify_end()
                            .child(control),
                    ),
            )
            .children(error.map(|error| {
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.status_exited))
                    .child(error)
            }))
            .children(self.font_list(dialog, row, theme, cx))
            .into_any_element()
    }

    fn settings_control(
        &self,
        dialog: &SettingsDialog,
        setting: &'static Setting,
        focused: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let shown = dialog.shown(setting);
        let read_only = dialog.read_only();
        let ring = if focused { theme.selected_ring } else { theme.border };
        match setting.kind {
            Kind::Bool => {
                let on = shown == "true";
                let knob = px(theme::SWITCH_HEIGHT - 2.0 * theme::SWITCH_INSET);
                div()
                    .id(SharedString::from(format!("switch-{}", setting.id())))
                    .flex_none()
                    .w(px(theme::SWITCH_WIDTH))
                    .h(px(theme::SWITCH_HEIGHT))
                    .p(px(theme::SWITCH_INSET))
                    .rounded_full()
                    .flex()
                    .when(on, |el| el.justify_end())
                    .bg(rgb(if on { theme.accent } else { theme.border }))
                    .when(focused, |el| el.border(px(theme::BORDER_WIDTH)).border_color(rgb(ring)))
                    .when(read_only, |el| el.opacity(opacity::DISABLED))
                    .when(!read_only, |el| el.cursor_pointer())
                    .child(div().size(knob).rounded_full().bg(rgb(theme.surface)))
                    .into_any_element()
            }
            Kind::Choice(choices) => div()
                .flex()
                .gap(px(space::XS))
                .p(px(space::XS))
                .rounded(px(radius::MEDIUM))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(ring))
                .when(read_only, |el| el.opacity(opacity::DISABLED))
                .children(choices.iter().map(|&choice| {
                    let active = shown == choice;
                    div()
                        .id(SharedString::from(format!("choice-{}-{choice}", setting.id())))
                        .px(px(space::S))
                        .rounded(px(radius::SMALL))
                        .text_size(font_size::META)
                        .text_color(rgb(if active { theme.text } else { theme.text_muted }))
                        .when(active, |el| el.bg(rgb(theme.surface_raised)))
                        .when(!active && !read_only, |el| {
                            el.cursor_pointer().on_click(cx.listener(move |shell, _, window, cx| {
                                cx.stop_propagation();
                                shell.settings_do(|d, _| d.choose(setting, choice), window, cx);
                            }))
                        })
                        .child(choice)
                }))
                .into_any_element(),
            Kind::Text | Kind::Number => {
                let editing = dialog.draft.as_ref().filter(|(s, _)| s.id() == setting.id());
                let field = match editing {
                    Some((_, input)) => caret_field("settings-value", input, "", true, theme, cx, |_, _| {}),
                    None => text_field("settings-value", &shown, "", false, false, theme),
                };
                field
                    .w_full()
                    .when(read_only, |el| el.opacity(opacity::DISABLED))
                    .into_any_element()
            }
            Kind::Font => div()
                .w_full()
                .flex()
                .justify_between()
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::MEDIUM))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(ring))
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text))
                .when(read_only, |el| el.opacity(opacity::DISABLED))
                .child(if shown.is_empty() { "Default".to_owned() } else { shown })
                .child(div().text_color(rgb(theme.text_muted)).child("▾"))
                .into_any_element(),
        }
    }

    /// The shortcut chip (or "Press keys…"), the changed dot and Reset.
    fn binding_control(
        &self,
        dialog: &SettingsDialog,
        command: Command,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let capturing = dialog.capture == Some(command);
        let changed = self.keymap.changed(command);
        div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .when(changed, |el| {
                el.child(div().size(px(space::S)).rounded_full().bg(rgb(theme.accent)))
                    .child(
                        div()
                            .id(SharedString::from(format!("reset-{}", command.name())))
                            .px(px(space::S))
                            .rounded(px(radius::SMALL))
                            .cursor_pointer()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .hover(|style| style.bg(rgb(theme.surface_raised)))
                            .tooltip(tooltip(
                                theme,
                                &format!("Back to {}", keymap::keys_label(&command.default_keys())),
                                "",
                            ))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                cx.stop_propagation();
                                shell.settings_do(|d, _| d.reset(command), window, cx);
                            }))
                            .child("Reset"),
                    )
            })
            .child(if capturing {
                kbd("Press keys…".into(), theme, true)
            } else {
                kbd(self.keymap.label(command), theme, false)
            })
            .into_any_element()
    }

    /// The font pick under its row: the query, then the matching fonts (Default first).
    fn font_list(
        &self,
        dialog: &SettingsDialog,
        row: Row,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        let pick = dialog.font_pick.as_ref()?;
        if row != Row::Setting(pick.setting) {
            return None;
        }
        let setting = pick.setting;
        let selected = pick.selected;
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .child(caret_field("settings-font-query", &pick.query, "Search fonts", true, theme, cx, |_, _| {}))
                .child(
                    div()
                        .id("settings-font-list")
                        .max_h(px(theme::SETTINGS_FONT_LIST_HEIGHT))
                        .overflow_y_scroll()
                        .children(dialog.font_choices().into_iter().enumerate().map(|(i, font)| {
                            let label = if font.is_empty() { "Default".to_owned() } else { font.clone() };
                            menu_item(SharedString::from(format!("font-choice-{i}")), theme)
                                .when(i == selected, |el| el.bg(rgb(theme.surface_raised)))
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    cx.stop_propagation();
                                    let font = font.clone();
                                    shell.settings_do(move |d, _| d.choose(setting, &font), window, cx);
                                }))
                                .child(label)
                        })),
                )
                .into_any_element(),
        )
    }
}

/// `"Terminal"` from a key context like `Terminal` or `Git && !Editing`, for "Terminal only".
fn context_name(context: &str) -> String {
    let word = context
        .split(|c: char| !c.is_alphanumeric())
        .find(|w| !w.is_empty())
        .unwrap_or(context);
    let mut chars = word.chars();
    chars
        .next()
        .map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}
```

The whole row is the click target (`Shell::settings_row` → `SettingsDialog::click`), so controls other than the choice chips and Reset have no click of their own. Check against the code: `opacity::DISABLED` (grep `pub mod opacity` in `theme.rs`; use what `disabled_button` uses), `theme.selected_ring`, `theme.accent_text`, `theme.divider`, `kbd(text, theme, on_accent)`, `menu_item(id, theme)`, `text_field`/`caret_field` return `Stateful<Div>`, `Pixels::max`. A mismatch is a compile error: follow the real API.

- [ ] **Step 6: Build, clippy, test**

Run: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p mkrew-ui --lib`
Expected: clean, PASS.

- [ ] **Step 7: Smoke run** (the real app, from the worktree build; the daemon keeps running)

1. Start the worktree's `mkrew-ui`. Press Ctrl+, → the dialog opens on General.
2. Appearance → click Dark → the app turns dark at once; `%APPDATA%\mkrew\settings.toml` has `theme = "dark"`; set System again.
3. App font size: type `40`, Enter → red line `…font_size must be 8–32 px, got 40`, file unchanged; Esc restores 13.
4. Notifications → toggle Sound twice → the file shows `sound = false`, then `sound = true`.
5. Keyboard → search `close pane` → click the row → "Press keys…" → Ctrl+Alt+Shift+Y → chip reads Ctrl+Alt+Shift+Y with an accent dot; close the dialog; with two panes open press it → a pane closes, no restart. Reset it.
6. Rebind Toggle sidebar to Ctrl+Shift+2, press it → the sidebar toggles (Review Focus 1). Reset it.
7. Rebind a row to Ctrl+Shift+K → `Ctrl+Shift+K is used by "…"` on the row; `keymap.toml` unchanged.

Report what you saw; what cannot be checked here goes to HUMAN_CHECKS in Task 6.

- [ ] **Step 8: Commit**

```bash
git add crates/mkrew-ui/src/shell/settings.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/shell/focus.rs crates/mkrew-ui/src/shell/sessions.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/navigation.rs crates/mkrew-ui/src/shell/workspace_actions.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette/tests.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/theme.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): settings dialog (Ctrl+,)"
```

---

### Task 6: ADR, SPEC, HUMAN_CHECKS, PROGRESS

**Files:**
- Create: `docs/decisions/0072-settings-dialog.md`
- Modify: `docs/SPEC.md` (§13 lines 1221, 1248, 1305; §14 from line 1312; §20.0 line 1718), `docs/HUMAN_CHECKS.md` ("To verify" at line 19; "Numbers and merge notes"), `docs/PROGRESS.md`

- [ ] **Step 1: ADR** (format of `docs/decisions/0071-in-app-update.md`)

```markdown
# 0072 — Settings dialog

- Date: 2026-10-24
- Status: accepted (SPEC §13, §14, §20.0)
- Milestone: none (user request 2026-10-24)

## Context

Everything in `settings.toml` and `keymap.toml` had to be changed by hand in Zed: the user had to
know TOML, the key names and the GPUI keystroke syntax, and a new shortcut needed a restart.

## Decision

- A modal Settings dialog: pages General, Appearance, Notifications, Voice, Jira, Phone, Features,
  Keyboard. Ctrl+, opens it; the command `open_keymap_settings` is renamed `open_settings` (a
  keymap.toml that still uses the old name reports it as an unknown command). The palette's
  separate "Open settings" entry is gone: the command is that entry.
- Rows come from one table, `mkrew_core::settings_fields::SETTINGS`; a test keeps it equal to the
  template's keys. The panel widths are in the table but hidden (dragging an edge sets them).
- Every change is written at once through the existing `write_value` (parse before write, comments
  kept). No validation of its own: serde's message shows under the row and the file is unchanged.
- A file that does not parse makes the settings pages read-only, with "Fix in Zed".
- Shortcuts are captured from the keyboard and written with `keymap::write_binding`, refused when
  they add a duplicate or an invalid keystroke, and applied at once (`clear_key_bindings` +
  `bind_keys`; the layout observer reads the `ActiveKeymap` global). A hand edit of keymap.toml
  still applies after a restart.
- No "unbind": an empty keystroke stays an error.

## Consequences

- A new `settings.toml` key needs a `Setting` row or the table test fails.
- Dialog writes apply without waiting for the settings mtime poll.
```

- [ ] **Step 2: SPEC**

- §13 line 1221: "All shortcuts live in `<config>/mkrew/keymap.toml`; below are defaults." → "All shortcuts live in `<config>/mkrew/keymap.toml`, which the Settings dialog's Keyboard page writes (applied at once, ADR 0072); below are defaults."
- §13 table line 1248: `| Global | Open settings | Ctrl+, |`
- §13 line 1304–1305: "rebind `split_agent` / `split_shell` / `rename_pane` in keymap.toml if needed" → "… in Settings → Keyboard if needed".
- §14: where dialogs are described (`grep -n "dialog" docs/SPEC.md` within §14), add: "**Settings** (Ctrl+,, ADR 0072): a modal dialog; page list on the left, rows on the right (label, help, control: switch, segmented choice, text field, font pick); shortcut rows show a chip, a changed dot and Reset; the footer opens the files in Zed."
- §20.0 after line 1719: "The Settings dialog (ADR 0072) writes this file one key at a time; the UI applies a dialog change at once."

- [ ] **Step 3: HUMAN_CHECKS** — under "## To verify (non-blocking)" add:

```markdown
### To verify — 2026-10-24 Settings dialog (ADR 0072, plan docs/superpowers/plans/2026-10-24-settings-dialog.md)

- [ ] **Look (dark + light, 100 / 150 / 200 % DPI):** Ctrl+, → page list left, rows right, footer links; focus ring on the focused row; switch, segments, text field and font pick readable; a small window shrinks the dialog and the rows scroll.
- [ ] **Every page:** change one value per page; the file gets the line, comments stay; fonts / theme / diff / tray apply at once.
- [ ] **Broken file:** delete a `]` in settings.toml → the dialog shows the error and "Fix in Zed", the controls do nothing; fix it → the rows become editable within a second.
- [ ] **Read-only file:** make settings.toml read-only → a change shows the error under its row, no toast.
- [ ] **Keyboard:** rebind, press the new key without restarting; a duplicate is refused with the other command's name; Reset brings the default back; switch the keyboard layout (US ↔ Vietnamese) → the new binding still works.
- [ ] **Shift + digit / punctuation:** rebind to Ctrl+Shift+2 and to Ctrl+Shift+/ → both work.
- [ ] **Old keymap:** a keymap.toml with `open_keymap_settings = "…"` lists "unknown command" in the palette; Ctrl+, still opens the dialog.
```

Under "Numbers and merge notes" add: "- 2026-10-24 `feat/settings-ui`: ADR 0072, no protocol/migration. Merging with master: every key master added to `settings.rs`'s `template()` since `7fe269e` (e.g. `[jira] mine`, perf `[features] turn_snapshots` / `conflict_detection`) needs a row in `mkrew_core::settings_fields::SETTINGS`, or `every_template_key_has_exactly_one_setting` fails."

- [ ] **Step 4: PROGRESS** — add at the top of the notes list:

```markdown
- 2026-10-24 settings dialog (branch `feat/settings-ui`, plan docs/superpowers/plans/2026-10-24-settings-dialog.md, ADR 0072, no protocol/migration): `mkrew_core::settings_fields` (table + `Serialize`), `keymap::write_binding` / `open_settings`, `settings_dialog.rs` (pure), `shell/settings.rs`, `apply_settings_file` / `set_keymap` / `ActiveKeymap`. Checks in HUMAN_CHECKS "2026-10-24 Settings dialog". Outside the milestone loop.
```

- [ ] **Step 5: Full verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt, clippy and tests clean.

- [ ] **Step 6: Commit**

```bash
git add docs/decisions/0072-settings-dialog.md docs/SPEC.md docs/HUMAN_CHECKS.md docs/PROGRESS.md
git -c user.email=tungnv975@gmail.com commit -m "docs(settings): ADR 0072, SPEC and checks for the settings dialog"
```
