# Tern-style font settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mkrew ships Ioskeley Mono Term as the default terminal font and gets Tern's Appearance font settings: font lists, bold / italic / bold italic families, a grid weight, a separate code font, size steppers beside Font and UI font, and Ctrl+Shift+= / Ctrl+Shift+- zoom.

**Architecture:** `mkrew-core` gains the new `settings.toml` keys, a weight type and one zoom writer.
`theme::Fonts` resolves every list once (first installed entry, else a default) into plain families; `Theme::with_fonts` copies them, and views only read resolved families.
The grid builds its four faces once per paint (`CellFonts`), every other monospace text outside the terminal reads `code_font_family`, and the Settings dialog edits font rows as text with a stepper widget for sizes and weight.

**Tech Stack:** Rust 2024, gpui-pre 0.3.7 (`Font`, `FontWeight(f32)`, `FontFallbacks`), gpui-pre-platform (real text system in one test), serde / toml, insta (existing snapshots must stay unchanged).

**Spec:** `docs/superpowers/specs/2026-11-27-tern-font-settings-design.md` (commit 72009d73 on master). Executors read it and this plan; the spec wins on behavior, this plan wins on names and order. Two deliberate refinements of the spec are listed under Global Constraints and written back to the spec in Task 7.

## Global Constraints

- Work only in the worktree `C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/tern-fonts` (branch `feat/tern-fonts`, base `master` at 72009d73 or later). Use it as `cwd` for every command and give tool paths as absolute paths or paths starting with `.worktrees/tern-fonts/`. Never edit the main checkout or other worktrees.
- Commit with `git -c user.email=tungnv975@gmail.com commit -m "<Conventional Commit>"`, staging the task's files by explicit path. Never push, tag, rewrite history or add a co-author line.
- Cargo: `-j 4` on every command; set `CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target` (shared target; C: is often full, use the `mkrew-cleanup` skill if a build fails for space). A locked exe is renamed to `*.locked-old.exe`. Never kill the installed daemon or the user's sessions.
- Final verify: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL). Fallback if it runs past 600 s: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -j 4 -- -D warnings`, `cargo test --workspace -j 2`. Intermediate tasks run only their focused tests plus `cargo clippy -p <crate> --all-targets -j 4 -- -D warnings`.
- ADR number 0120 (master ends at 0119). No protocol change, no migration. Re-check the ADR number before the merge.
- Font: Ioskeley Mono v2.2.0, `IoskeleyMono-Term.zip`, faces `Normal/Hinted/IoskeleyMonoTerm-{Regular,Italic,Medium,MediumItalic}.ttf`, license OFL-1.1, stored in `font/ioskeley-mono/`.
- Settings keys, exact, all with defaults so old files load: `[terminal] font_family = ""`, `bold_font_family = ""`, `italic_font_family = ""`, `bold_italic_font_family = ""`, `font_weight = 400` (100-900), `font_size = 13`; `[ui] font_family = ""`, `code_font_family = ""`, `font_size = 13`. Lists are comma separated, entries trimmed, empty entries dropped, the first installed entry wins.
- Bold weight = `weight + 300`, clamped to 700-900.
- Commands, exact: `font_size_up` "Global: Increase font size" (PC `ctrl-shift-=`, macOS `cmd-=`), `font_size_down` "Global: Decrease font size" (PC `ctrl-shift--`, macOS `cmd--`). Each press moves both `[terminal]` and `[ui] font_size` by 1 px, each kept 8-32, in one write.
- Refinement 1 (robustness): `TERMINAL_FONTS` keeps the platform families after `"Ioskeley Mono Term"`, so a failed font registration never gives a proportional grid.
- Refinement 2 (consistency): the omp surface (`shell/tsp_view.rs`, `shell/tsp_view/picker.rs`) and the terminal find bar (`terminal/view.rs` overlays) keep the grid font; only views outside the terminal and omp move to the code font.
- UI rules (SPEC §14, skill `mkrew-ui-design`): colors, spacing, radius and font sizes only from `theme.rs`; hover and press only through `shell/widgets.rs` widgets or `.pressable(Look::..)` + `.on_press(..)`; dark and light themes.
- No `unwrap`/`expect` outside tests unless provably infallible (comment why). No test runs a real agent or touches the network; no test reads source text. Docs: no em dash, one sentence per line in long Markdown. Never edit CHANGELOG.md.
- Clean cutover: the font picker (`SettingsDialog::fonts`, the font branch of `list_choices`, "Search fonts") is deleted, not hidden.

## Review Focus

Inputs the spec implies but its feature list does not test, most likely first. Each has a test or smoke step in the named task.

- A font list with spaces, a trailing comma, or only missing families (`" , Nope ,"`). Expect: whitespace and empty entries ignored; only missing families gives the default, never a proportional grid. [Task 2 `a_font_list_uses_its_first_installed_family`]
- A user who sets Font to another family and leaves Bold empty. Expect: bold cells use that family at the bold weight, not Ioskeley Medium. [Task 2 `bold_and_italic_follow_font_unless_set`]
- Zoom at the bounds and on a missing or broken `settings.toml`. Expect: sizes stop at 8 / 32 independently; a missing file is created from the template; a broken file is left untouched. [Task 1 `zoom_moves_both_sizes_within_bounds`, `zoom_leaves_a_broken_file_alone`]
- The stepper pressed at a bound, or while the file is broken (read-only dialog). Expect: nothing is written. [Task 5 `the_stepper_keeps_its_bounds_and_the_read_only_file`]
- A grid weight of 500+ with the bundled font (only Regular / Medium faces exist). Expect: regular cells draw Medium, bold cells still draw a real face, all one cell wide. [Task 2 `the_bundled_terminal_faces_are_distinct_and_one_cell_wide`, Task 7 smoke step 4]

---

### Task 1: Settings schema and zoom writer (mkrew-core)

**Files:**
- Modify: `crates/mkrew-core/src/settings.rs` (`TerminalSettings` ~241-259, `UiSettings` ~499-583, `write_value` ~634-654, `FontSize` ~684-705, `template()` ~707-821, tests)

**Interfaces:**
- Produces:
  - `pub struct TextWeight(pub u16)` (serde `try_from = "u16"`, `into = "u16"`, 100-900; error `"font_weight must be 100–900, got {n}"`), `pub const DEFAULT_TEXT_WEIGHT: TextWeight = TextWeight(400)`.
  - `TerminalSettings { font_family, bold_font_family: String, italic_font_family: String, bold_italic_font_family: String, font_weight: TextWeight, font_size, option_as_meta }`.
  - `UiSettings.code_font_family: String` (right after `font_size`).
  - `pub fn write_font_zoom(path: &Path, delta: i16) -> Result<(), SettingsError>`.

- [ ] **Step 1: Write the failing tests** (in `settings.rs` `mod tests`, next to `font_size_outside_8_to_32_is_an_error`)

```rust
    #[test]
    fn new_font_keys_default_empty_and_old_files_still_load() {
        let settings = Settings::default();
        assert_eq!(settings.terminal.bold_font_family, "");
        assert_eq!(settings.terminal.italic_font_family, "");
        assert_eq!(settings.terminal.bold_italic_font_family, "");
        assert_eq!(settings.terminal.font_weight, TextWeight(400));
        assert_eq!(settings.ui.code_font_family, "");
        let old = parse_ok("[terminal]\nfont_family = \"Consolas\"\nfont_size = 14\n");
        assert_eq!(old.terminal.font_weight, TextWeight(400));
        let listed = parse_ok(
            "[terminal]\nfont_family = \"Berkeley Mono, Ioskeley Mono Term\"\nfont_weight = 500\n[ui]\ncode_font_family = \"JetBrains Mono\"\n",
        );
        assert_eq!(listed.terminal.font_family, "Berkeley Mono, Ioskeley Mono Term");
        assert_eq!(listed.terminal.font_weight, TextWeight(500));
        assert_eq!(listed.ui.code_font_family, "JetBrains Mono");
    }

    #[test]
    fn font_weight_outside_100_to_900_is_an_error() {
        for weight in [50, 950] {
            let message = parse_err(&format!("[terminal]\nfont_weight = {weight}\n"));
            assert!(
                message.contains(&format!("font_weight must be 100–900, got {weight}")),
                "{message}"
            );
        }
    }

    #[test]
    fn zoom_moves_both_sizes_within_bounds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "# mine\n[ui]\nfont_size = 13\n\n[terminal]\nfont_size = 32\n").unwrap();
        write_font_zoom(&path, 1).unwrap();
        let grown = load(&path).unwrap();
        assert_eq!((grown.ui.font_size, grown.terminal.font_size), (FontSize(14), FontSize(32)));
        assert!(std::fs::read_to_string(&path).unwrap().starts_with("# mine"), "comments stay");
        std::fs::write(&path, "[ui]\nfont_size = 9\n[terminal]\nfont_size = 8\n").unwrap();
        write_font_zoom(&path, -1).unwrap();
        let shrunk = load(&path).unwrap();
        assert_eq!((shrunk.ui.font_size, shrunk.terminal.font_size), (FontSize(8), FontSize(8)));
        let fresh = dir.path().join("new.toml");
        write_font_zoom(&fresh, 1).unwrap();
        let created = load(&fresh).unwrap();
        assert_eq!((created.ui.font_size, created.terminal.font_size), (FontSize(14), FontSize(14)));
        assert!(std::fs::read_to_string(&fresh).unwrap().starts_with("# Mkrew settings"));
    }

    #[test]
    fn zoom_leaves_a_broken_file_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "[ui]\nfont_size = 4\n").unwrap();
        assert!(write_font_zoom(&path, 1).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[ui]\nfont_size = 4\n");
    }
```

If the test module has no `parse_ok` helper, add one beside `parse_err` (~905) that calls the same parser `parse_err` uses and unwraps.

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p mkrew-core -j 4 settings::tests` - Expected: compile errors (`TextWeight`, `bold_font_family`, `write_font_zoom` unknown).

- [ ] **Step 3: Implement**

`TerminalSettings` (update its doc to "the grid font, its bold / italic faces and weight, and keys"):

```rust
pub struct TerminalSettings {
    /// Families separated by commas, most wanted first; empty: the bundled Ioskeley Mono Term.
    pub font_family: String,
    /// Bold cells' families; empty: Font's bold face (ADR 0120).
    pub bold_font_family: String,
    /// Italic cells' families; empty: Font.
    pub italic_font_family: String,
    /// Bold italic cells' families; empty: as bold.
    pub bold_italic_font_family: String,
    /// Weight of regular cells; bold cells draw 300 heavier, at least 700.
    pub font_weight: TextWeight,
    pub font_size: FontSize,
    // option_as_meta unchanged
}
```

Default: the three new strings `String::new()`, `font_weight: DEFAULT_TEXT_WEIGHT`.
`UiSettings`: after `font_size` add `/// `[ui] code_font_family` (ADR 0120): monospace text outside the terminal; empty: the editor follows the terminal font, the rest use Ioskeley Mono Term.` and `pub code_font_family: String,`, default `String::new()`.

Beside `FontSize`:

```rust
/// `[terminal] font_weight`: a CSS weight, 100–900 (checked when parsed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct TextWeight(pub u16);

/// `[terminal] font_weight` of a new install.
pub const DEFAULT_TEXT_WEIGHT: TextWeight = TextWeight(400);

impl From<TextWeight> for u16 {
    fn from(weight: TextWeight) -> u16 {
        weight.0
    }
}

impl TryFrom<u16> for TextWeight {
    type Error = String;

    fn try_from(weight: u16) -> Result<Self, String> {
        if (100..=900).contains(&weight) {
            Ok(Self(weight))
        } else {
            Err(format!("font_weight must be 100–900, got {weight}"))
        }
    }
}
```

Split `write_value` so the zoom writes both keys with one read and one write:

```rust
/// Rewrites `path` (the template when it is missing) with `edit` of its text and settings.
/// Writes nothing when the file does not parse or the edit would not parse, so a good file
/// never turns bad.
fn edit_file(
    path: &Path,
    edit: impl FnOnce(&str, &Settings) -> String,
) -> Result<(), SettingsError> {
    let io_error = |e: io::Error| SettingsError {
        file: path.to_path_buf(),
        line_col: None,
        message: e.to_string(),
    };
    let src = match std::fs::read_to_string(path) {
        Ok(src) => src,
        Err(e) if e.kind() == io::ErrorKind::NotFound => template().to_owned(),
        Err(e) => return Err(io_error(e)),
    };
    let settings = parse(path, &src)?;
    let out = edit(&src, &settings);
    parse(path, &out)?;
    std::fs::write(path, out).map_err(io_error)
}

pub(crate) fn write_value(
    path: &Path,
    section: &str,
    key: &str,
    value: &str,
) -> Result<(), SettingsError> {
    edit_file(path, |src, _| set_value(src, section, key, value))
}

/// Ctrl+Shift+= / Ctrl+Shift+- (ADR 0120): `[terminal]` and `[ui] font_size` move by `delta`
/// px, each kept 8–32, in one write; see [`write_value`].
pub fn write_font_zoom(path: &Path, delta: i16) -> Result<(), SettingsError> {
    let zoomed = |size: FontSize| size.0.saturating_add_signed(delta).clamp(8, 32).to_string();
    edit_file(path, |src, settings| {
        let src = set_value(src, "terminal", "font_size", &zoomed(settings.terminal.font_size));
        set_value(&src, "ui", "font_size", &zoomed(settings.ui.font_size))
    })
}
```

Keep `write_value`'s existing doc comment. If `parse` does not return `Result<Settings, SettingsError>`, adapt `edit_file` to what it returns.

Template: replace the `# App font: …` line (~733) with

```
     # UI font: one family or several separated by commas, the first installed is used\n\
     # (empty: the bundled Geist); size 8–32 px.\n\
     # Code font: monospace text outside the terminal (diffs, chats, labels), a family or several;\n\
     # empty: the Files editor follows the terminal font, the rest use Ioskeley Mono Term.\n\
```

add `     # code_font_family = \"\"\n\` after `# font_size = 13\n\` in `[ui]`; replace the terminal comment (~769-770) with

```
     # Terminal font: one family or several separated by commas, the first installed is used\n\
     # (empty: the bundled Ioskeley Mono Term); size 8–32 px, also code in editors, diffs, chats.\n\
     # Bold / italic / bold italic font: the same, for those cells (empty: Font's bold face, Font).\n\
     # Font weight: 100–900 for regular cells; bold draws 300 heavier, at least 700.\n\
```

and in `[terminal]` list `# bold_font_family = \"\"`, `# italic_font_family = \"\"`, `# bold_italic_font_family = \"\"`, `# font_weight = 400` after `# font_family = \"\"`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-core -j 4 settings` - Expected: PASS except possibly `settings_fields::tests::every_template_key_has_exactly_one_setting`.
If that one fails, add Task 5 Step 3's table rows now (only the `settings_fields.rs` `Kind` / rows / `size` / `literal` part) so this task ends green, and skip that part in Task 5.

- [ ] **Step 5: Clippy and commit**

Run: `cargo clippy -p mkrew-core --all-targets -j 4 -- -D warnings`.
Commit `feat(core): font lists, bold and italic families, weight, code font and font zoom in settings`.

---

### Task 2: Bundled Ioskeley Mono Term and font resolution (theme)

**Files:**
- Create: `font/ioskeley-mono/IoskeleyMonoTerm-Regular.ttf`, `-Italic.ttf`, `-Medium.ttf`, `-MediumItalic.ttf`, `font/ioskeley-mono/LICENSE`
- Modify: `crates/mkrew-ui/src/theme.rs` (`BUNDLED_FONTS` ~198-207, `TERMINAL_FONTS` ~914-925, `Fonts` ~954-991, `Theme` fields ~1240-1242, `with_fonts` ~1302-1307, defaults ~1733-1735, `golden` ~3770, font tests ~3795-3895)
- Modify: `crates/mkrew-ui/src/preview.rs` tests that build `theme::Fonts { .. }` (~2028-2047)

**Interfaces:**
- Consumes: Task 1's `TerminalSettings` / `UiSettings` fields.
- Produces (all `pub` in `theme`):
  - `const TERMINAL_FONT: &str = "Ioskeley Mono Term"`, `const TERMINAL_BOLD_FONT: &str` (value settled in Step 5).
  - `fn font_list(list: &str) -> impl Iterator<Item = &str>`, `fn bold_weight(regular: f32) -> f32`.
  - `Fonts { ui_family, ui_size, terminal_family, terminal_bold, terminal_italic, terminal_bold_italic, terminal_weight: f32, terminal_size, code_family, editor_family }` (families `SharedString`).
  - `Fonts::in_use(&self, id: &str) -> Option<SharedString>` (Settings placeholders).
  - `Theme` fields `terminal_bold_family`, `terminal_italic_family`, `terminal_bold_italic_family`, `terminal_font_weight: f32`, `code_font_family`, `editor_font_family`.

- [ ] **Step 1: Add the font files**

```bash
d=$(mktemp -d)
curl -sL -o "$d/t.zip" https://github.com/ahatem/IoskeleyMono/releases/download/v2.2.0/IoskeleyMono-Term.zip
mkdir -p font/ioskeley-mono
for f in Regular Italic Medium MediumItalic; do unzip -p "$d/t.zip" "Normal/Hinted/IoskeleyMonoTerm-$f.ttf" > "font/ioskeley-mono/IoskeleyMonoTerm-$f.ttf"; done
rm -rf "$d"; ls -l font/ioskeley-mono
curl -s https://api.github.com/repos/ahatem/IoskeleyMono/contents/ | jq -r '.[].name'
```

Expected: four files of about 2.1 MB.
Fetch the repo's OFL license file named in the listing (e.g. `LICENSE` or `LICENSE.md`) from `https://raw.githubusercontent.com/ahatem/IoskeleyMono/main/<name>` into `font/ioskeley-mono/LICENSE`, and check it starts with the copyright line and "SIL OPEN FONT LICENSE".

- [ ] **Step 2: Write the failing tests** (in `theme.rs` tests; replace `a_configured_family_is_used_only_when_installed` and `default_settings_give_the_spec_fonts`, keep the rest)

```rust
    fn installed(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn a_font_list_uses_its_first_installed_family() {
        let mut settings = Settings::default();
        settings.terminal.font_family = " , Nope , JetBrains Mono,Consolas,".into();
        settings.terminal.font_size = FontSize(18);
        settings.ui.font_family = "Nope, Inter".into();
        settings.ui.font_size = FontSize(15);
        let all = installed(&["Consolas", "JetBrains Mono", "Inter", TERMINAL_FONT]);
        let fonts = Fonts::from_settings(&settings, &all);
        assert_eq!(fonts.terminal_family.as_ref(), "JetBrains Mono", "list order, not install order");
        assert_eq!(fonts.ui_family.as_ref(), "Inter");
        assert_eq!((fonts.terminal_size, fonts.ui_size), (18.0, 15.0));
        let fonts = Fonts::from_settings(&settings, &installed(&[TERMINAL_FONT, PREFERRED_UI_FONT]));
        assert_eq!(fonts.terminal_family.as_ref(), TERMINAL_FONT, "a missing family never makes the grid proportional");
        assert_eq!(fonts.ui_family.as_ref(), PREFERRED_UI_FONT);
    }

    #[test]
    fn bold_and_italic_follow_font_unless_set() {
        let all = installed(&[TERMINAL_FONT, TERMINAL_BOLD_FONT, "JetBrains Mono", "Consolas"]);
        let styles = |fonts: &Fonts| {
            [&fonts.terminal_bold, &fonts.terminal_italic, &fonts.terminal_bold_italic]
                .map(|family| family.to_string())
        };
        let fonts = Fonts::from_settings(&Settings::default(), &all);
        assert_eq!(styles(&fonts), [TERMINAL_BOLD_FONT, TERMINAL_FONT, TERMINAL_BOLD_FONT]);
        let mut settings = Settings::default();
        settings.terminal.font_family = "JetBrains Mono".into();
        let fonts = Fonts::from_settings(&settings, &all);
        assert_eq!(styles(&fonts), ["JetBrains Mono"; 3], "another family's bold is its own bold face");
        settings.terminal.bold_font_family = "Nope, Consolas".into();
        let fonts = Fonts::from_settings(&settings, &all);
        assert_eq!(styles(&fonts), ["Consolas", "JetBrains Mono", "JetBrains Mono"]);
    }

    #[test]
    fn bold_draws_300_heavier_at_least_700() {
        assert_eq!([100.0, 400.0, 500.0, 700.0].map(bold_weight), [700.0, 700.0, 800.0, 900.0]);
        let mut settings = Settings::default();
        settings.terminal.font_weight = mkrew_core::settings::TextWeight(500);
        assert_eq!(Fonts::from_settings(&settings, &[]).terminal_weight, 500.0);
    }

    #[test]
    fn the_editor_follows_font_and_the_rest_use_the_code_font() {
        let all = installed(&[TERMINAL_FONT, "JetBrains Mono", "Consolas"]);
        let mut settings = Settings::default();
        settings.terminal.font_family = "JetBrains Mono".into();
        let fonts = Fonts::from_settings(&settings, &all);
        assert_eq!((fonts.editor_family.as_ref(), fonts.code_family.as_ref()), ("JetBrains Mono", TERMINAL_FONT));
        settings.ui.code_font_family = "Consolas".into();
        let fonts = Fonts::from_settings(&settings, &all);
        assert_eq!((fonts.editor_family.as_ref(), fonts.code_family.as_ref()), ("Consolas", "Consolas"));
    }

    #[test]
    fn default_settings_give_the_spec_fonts() {
        let fonts = Fonts::from_settings(&Settings::default(), &[]);
        assert_eq!(fonts.ui_family.as_ref(), UI_FONT);
        assert_eq!(fonts.terminal_family.as_ref(), TERMINAL_FONT);
        assert_eq!((fonts.ui_size, fonts.terminal_size, fonts.terminal_weight), (13.0, 13.0, 400.0));
        assert_eq!(fonts.in_use("terminal.bold_font_family").as_deref(), Some(TERMINAL_BOLD_FONT));
        assert_eq!(fonts.in_use("ui.code_font_family").as_deref(), Some(TERMINAL_FONT));
        assert_eq!(fonts.in_use("ui.theme"), None);
        assert_eq!(
            Theme::dark().with_fonts(&fonts),
            Theme::dark(),
            "the default theme already uses the default fonts"
        );
    }

    /// The bundled faces load, each family resolves to its own face, and all four are one grid
    /// cell wide, on this platform's real text system.
    #[test]
    fn the_bundled_terminal_faces_are_distinct_and_one_cell_wide() {
        let platform = gpui_platform::current_platform(true);
        let text = platform.text_system();
        text.add_fonts(BUNDLED_FONTS.iter().map(|f| std::borrow::Cow::Borrowed(*f)).collect())
            .unwrap();
        let fonts = Fonts::from_settings(&Settings::default(), &text.all_font_names());
        assert_eq!(fonts.terminal_family.as_ref(), TERMINAL_FONT, "registered and listed");
        let face = |family: &SharedString, weight: f32, italic: bool| {
            let mut font = gpui::font(family.clone());
            font.weight = gpui::FontWeight(weight);
            if italic {
                font.style = gpui::FontStyle::Italic;
            }
            text.font_id(&font).unwrap()
        };
        let bold = bold_weight(fonts.terminal_weight);
        let faces = [
            face(&fonts.terminal_family, fonts.terminal_weight, false),
            face(&fonts.terminal_bold, bold, false),
            face(&fonts.terminal_italic, fonts.terminal_weight, true),
            face(&fonts.terminal_bold_italic, bold, true),
        ];
        for (i, a) in faces.iter().enumerate() {
            for b in &faces[i + 1..] {
                assert_ne!(a, b, "{faces:?}");
            }
        }
        let width = |id| {
            let glyph = text.glyph_for_char(id, 'm').unwrap();
            text.advance(id, glyph).unwrap().width
        };
        for id in faces {
            assert_eq!(width(id), width(faces[0]), "every face is one cell wide");
        }
    }
```

- [ ] **Step 3: Run to see them fail**

Run: `cargo test -p mkrew-ui -j 4 theme::tests` - Expected: compile errors (new names).

- [ ] **Step 4: Implement**

`BUNDLED_FONTS`: append the four `include_bytes!("../../../font/ioskeley-mono/IoskeleyMonoTerm-<Face>.ttf")` and extend its doc comment with Ioskeley Mono Term (OFL, `font/ioskeley-mono/`, ADR 0120).

Replace the `TERMINAL_FONTS` block:

```rust
/// The bundled terminal font (Ioskeley Mono, OFL, `font/ioskeley-mono/`, ADR 0120): the grid
/// and the code font when settings name none.
pub const TERMINAL_FONT: &str = "Ioskeley Mono Term";
/// The bundled font's bold cells: its Medium face (Tern's look, ADR 0120).
pub const TERMINAL_BOLD_FONT: &str = "Ioskeley Mono Term Medium";

/// Monospace families by preference; the first installed one is used. The platform ones only
/// matter when the bundled font failed to register (SPEC §14).
#[cfg(windows)]
pub const TERMINAL_FONTS: &[&str] = &[
    TERMINAL_FONT,
    "JetBrains Mono",
    "Cascadia Code",
    "Cascadia Mono",
    "Consolas",
];
#[cfg(target_os = "macos")]
pub const TERMINAL_FONTS: &[&str] = &[TERMINAL_FONT, "JetBrains Mono", "SF Mono", "Menlo"];
#[cfg(not(any(windows, target_os = "macos")))]
pub const TERMINAL_FONTS: &[&str] = &[TERMINAL_FONT, "JetBrains Mono", "DejaVu Sans Mono"];
```

Replace `Fonts` and `from_settings`:

```rust
/// The families of a settings font list: comma separated, trimmed, empty entries dropped.
pub fn font_list(list: &str) -> impl Iterator<Item = &str> {
    list.split(',').map(str::trim).filter(|family| !family.is_empty())
}

/// The first family of `list` that is `installed`.
fn first_installed(list: &str, installed: &[String]) -> Option<SharedString> {
    font_list(list)
        .find(|family| installed.iter().any(|name| name == family))
        .map(|family| SharedString::from(family.to_owned()))
}

/// The weight of bold cells: 300 heavier than `regular`, at least 700, at most 900 (ADR 0120).
pub fn bold_weight(regular: f32) -> f32 {
    (regular + 300.0).clamp(700.0, 900.0)
}

/// The fonts of `settings.toml` (SPEC §20.0, ADR 0120), resolved to installed families, as a
/// GPUI global read by every [`Theme`].
#[derive(Clone, Debug, PartialEq)]
pub struct Fonts {
    pub ui_family: SharedString,
    pub ui_size: f32,
    /// The grid's regular cells.
    pub terminal_family: SharedString,
    pub terminal_bold: SharedString,
    pub terminal_italic: SharedString,
    pub terminal_bold_italic: SharedString,
    /// Weight of regular cells (`[terminal] font_weight`); bold cells use [`bold_weight`].
    pub terminal_weight: f32,
    pub terminal_size: f32,
    /// Monospace text outside the terminal and omp (diffs, chats, labels).
    pub code_family: SharedString,
    /// The Files editor: the code font when one is set, else the grid's.
    pub editor_family: SharedString,
}

impl gpui::Global for Fonts {}

impl Fonts {
    /// `installed` = `cx.text_system().all_font_names()`. Only installed families are used: a
    /// missing one would silently become a proportional font.
    pub fn from_settings(settings: &Settings, installed: &[String]) -> Self {
        let terminal = &settings.terminal;
        let mono = SharedString::from(pick_family(TERMINAL_FONTS, installed));
        let terminal_family =
            first_installed(&terminal.font_family, installed).unwrap_or_else(|| mono.clone());
        // The bundled family's bold is its Medium face; any other family's is its own bold.
        let bold_default = if terminal_family == TERMINAL_FONT {
            SharedString::from(TERMINAL_BOLD_FONT)
        } else {
            terminal_family.clone()
        };
        let code = first_installed(&settings.ui.code_font_family, installed);
        let ui_family = first_installed(&settings.ui.font_family, installed).unwrap_or_else(|| {
            if installed.iter().any(|name| name == PREFERRED_UI_FONT) {
                PREFERRED_UI_FONT.into()
            } else {
                UI_FONT.into()
            }
        });
        Self {
            ui_family,
            ui_size: settings.ui.font_size.0.into(),
            terminal_bold: first_installed(&terminal.bold_font_family, installed)
                .unwrap_or_else(|| bold_default.clone()),
            terminal_italic: first_installed(&terminal.italic_font_family, installed)
                .unwrap_or_else(|| terminal_family.clone()),
            terminal_bold_italic: first_installed(&terminal.bold_italic_font_family, installed)
                .unwrap_or(bold_default),
            terminal_weight: terminal.font_weight.0.into(),
            terminal_size: terminal.font_size.0.into(),
            editor_family: code.clone().unwrap_or_else(|| terminal_family.clone()),
            code_family: code.unwrap_or(mono),
            terminal_family,
        }
    }

    /// The family an empty font row of the Settings dialog uses now (its placeholder).
    pub fn in_use(&self, id: &str) -> Option<SharedString> {
        Some(match id {
            "terminal.font_family" => self.terminal_family.clone(),
            "terminal.bold_font_family" => self.terminal_bold.clone(),
            "terminal.italic_font_family" => self.terminal_italic.clone(),
            "terminal.bold_italic_font_family" => self.terminal_bold_italic.clone(),
            "ui.code_font_family" => self.code_family.clone(),
            "ui.font_family" if self.ui_family == UI_FONT => "System UI".into(),
            "ui.font_family" => self.ui_family.clone(),
            _ => return None,
        })
    }
}
```

`Theme`, after `terminal_font_family`:

```rust
    pub terminal_bold_family: SharedString,
    pub terminal_italic_family: SharedString,
    pub terminal_bold_italic_family: SharedString,
    /// `[terminal] font_weight`; bold cells draw [`bold_weight`] of it.
    pub terminal_font_weight: f32,
    /// Monospace text outside the terminal and omp (ADR 0120).
    pub code_font_family: SharedString,
    /// The Files editor's family.
    pub editor_font_family: SharedString,
```

`with_fonts` copies all of them (`terminal_bold_family = fonts.terminal_bold.clone()`, …, `terminal_font_weight = fonts.terminal_weight`, `code_font_family = fonts.code_family.clone()`, `editor_font_family = fonts.editor_family.clone()`).
Defaults (~1734): `terminal_font_family: TERMINAL_FONT.into()`, `terminal_bold_family: TERMINAL_BOLD_FONT.into()`, `terminal_italic_family: TERMINAL_FONT.into()`, `terminal_bold_italic_family: TERMINAL_BOLD_FONT.into()`, `terminal_font_weight: 400.0`, `code_font_family: TERMINAL_FONT.into()`, `editor_font_family: TERMINAL_FONT.into()`.
`golden`: filter every `*_family:` line and `terminal_font_weight:` so the four snapshots stay byte-identical:

```rust
            .filter(|line| {
                let field = line.trim_start().split(':').next().unwrap_or_default();
                !(field == "name" || field.ends_with("_family") || field == "terminal_font_weight")
            })
```

Update its doc comment ("minus fields that are not colors: `name`, font families, the grid weight").
`preview.rs` tests building `theme::Fonts { ... }`: build them as `theme::Fonts { ui_family: "Inter".into(), terminal_family: "JetBrains Mono".into(), ..theme::Fonts::from_settings(&Settings::default(), &[]) }` (import `mkrew_core::settings::Settings` in that test module if missing).
Fix any other compile error from the new `Fonts` fields the same way.

- [ ] **Step 5: Run the tests and settle `TERMINAL_BOLD_FONT`**

Run: `cargo test -p mkrew-ui -j 4 theme::tests`.
If only `the_bundled_terminal_faces_are_distinct_and_one_cell_wide` fails on the bold faces (equal ids, a different width, or `font_id` falling back to another family), DirectWrite does not know the legacy family `Ioskeley Mono Term Medium`.
Then set `TERMINAL_BOLD_FONT` to `"Ioskeley Mono Term"` with the doc "The bundled font's bold cells: the same family; with only Regular and Medium bundled, a bold weight picks the Medium face (Tern's look, ADR 0120).", and rerun.
If it still fails, stop and report the `font_id`s and widths; do not weaken the test.
Expected: PASS, and `git status` shows no `.snap` change.

- [ ] **Step 6: Clippy and commit**

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings` (fix only what is needed to build; consumers are Tasks 3-4).
Commit `feat(ui): bundle Ioskeley Mono Term and resolve font lists, bold and italic faces, weight and code font (ADR 0120)` with the font files.

---

### Task 3: Grid faces, omp surface and editor width

**Files:**
- Modify: `crates/mkrew-ui/src/terminal/view.rs` (`paint` ~1253-1295, hint chips ~1396-1414, `cell_size` doc ~1432, `terminal_font` ~1476-1482, `text_run` ~1484-1505)
- Modify: `crates/mkrew-ui/src/shell/tsp_view.rs` (`push_runs` ~1101-1150, import line 21)
- Modify: `crates/mkrew-ui/src/shell/file_pane.rs` (~653, ~753), `crates/mkrew-ui/src/shell/files.rs` (~908)

**Interfaces:**
- Consumes: Task 2's `Theme` fields and `theme::bold_weight`.
- Produces: `pub struct CellFonts` with `CellFonts::new(theme: &Theme) -> Self` and `pub fn get(&self, bold: bool, italic: bool) -> &Font`; `pub fn editor_cell_width(window: &Window, theme: &Theme) -> Pixels`.

- [ ] **Step 1: Write the failing test** (in `terminal/view.rs` tests; create `#[cfg(test)] mod tests` with `use super::*;` if none exists)

```rust
    #[test]
    fn each_cell_style_gets_its_face_and_weight() {
        let mut theme = Theme::dark();
        theme.terminal_font_family = "R".into();
        theme.terminal_bold_family = "B".into();
        theme.terminal_italic_family = "I".into();
        theme.terminal_bold_italic_family = "BI".into();
        theme.terminal_font_weight = 500.0;
        let fonts = CellFonts::new(&theme);
        let seen = |bold, italic| {
            let font = fonts.get(bold, italic);
            (
                font.family.to_string(),
                font.weight.0,
                font.style == FontStyle::Italic,
                font.fallbacks.is_some(),
            )
        };
        assert_eq!(seen(false, false), ("R".into(), 500.0, false, true));
        assert_eq!(seen(true, false), ("B".into(), 800.0, false, true));
        assert_eq!(seen(false, true), ("I".into(), 500.0, true, true));
        assert_eq!(seen(true, true), ("BI".into(), 800.0, true, true));
        assert_eq!(terminal_font(&theme), *fonts.get(false, false), "metrics use the regular face");
    }
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-ui -j 4 each_cell_style_gets_its_face_and_weight` - Expected: `CellFonts` not found.

- [ ] **Step 3: Implement**

Replace `terminal_font` and the font part of `text_run`:

```rust
/// One grid face: `family` with the glyph fallbacks at `weight`, upright or italic.
fn face(family: &SharedString, weight: f32, italic: bool) -> Font {
    let mut base = font(family.clone());
    base.fallbacks = Some(GLYPH_FALLBACKS.clone());
    base.weight = FontWeight(weight);
    if italic {
        base.style = FontStyle::Italic;
    }
    base
}

/// The grid's regular face with the glyph fallbacks: the cell metrics, and text drawn outside
/// the grid (TSP `mono`, `ansi` and `rows`).
pub fn terminal_font(theme: &Theme) -> Font {
    face(&theme.terminal_font_family, theme.terminal_font_weight, false)
}

/// The grid's four faces (regular, bold, italic, bold italic; ADR 0120), built once per paint.
pub struct CellFonts([Font; 4]);

impl CellFonts {
    pub fn new(theme: &Theme) -> Self {
        let regular = theme.terminal_font_weight;
        let bold = theme::bold_weight(regular);
        Self([
            face(&theme.terminal_font_family, regular, false),
            face(&theme.terminal_bold_family, bold, false),
            face(&theme.terminal_italic_family, regular, true),
            face(&theme.terminal_bold_italic_family, bold, true),
        ])
    }

    pub fn get(&self, bold: bool, italic: bool) -> &Font {
        &self.0[usize::from(bold) + 2 * usize::from(italic)]
    }
}

fn text_run(len: usize, style: CellStyle, fonts: &CellFonts, underline_thickness: f32) -> TextRun {
    let font = fonts.get(style.bold, style.italic).clone();
    // the rest of the body unchanged
```

In `paint`: `let base_font = terminal_font(theme);` becomes `let fonts = CellFonts::new(theme);` and `text_run(…, &base_font, …)` passes `&fonts`.
The hint chips' `text_run` call (~1396-1414) gets a `CellFonts` too (build one there if it is another function).
Change `cell_size`'s doc to "Cell width and line height of the terminal grid. Monospace: one precomputed cell size, no per-cell measurement (SPEC §10)." and add:

```rust
/// One character of the Files editor: the advance of `m` in the editor font (ADR 0120), which
/// may differ from the grid's.
pub fn editor_cell_width(window: &Window, theme: &Theme) -> Pixels {
    let text_system = window.text_system();
    let font_size = px(theme.terminal_font_size);
    text_system
        .advance(
            text_system.resolve_font(&font(theme.editor_font_family.clone())),
            font_size,
            'm',
        )
        .map(|advance| advance.width)
        .unwrap_or(font_size * CELL_WIDTH_ESTIMATE)
}
```

`file_pane.rs` ~653 and `files.rs` ~908: `mkrew_ui::terminal::view::cell_size(window, …).0` becomes `mkrew_ui::terminal::view::editor_cell_width(window, …)`.
`file_pane.rs` ~753 (the editor list): `.font_family(theme.editor_font_family.clone())`.

`tsp_view.rs` `push_runs` (import `CellFonts`; keep `terminal_font` in the import only if still used in the file):

```rust
    let ui_font = gpui::font(theme.ui_font_family.clone());
    let mono = CellFonts::new(theme);
    let icon_font = gpui::font(theme::ICON_FONT);
    // UI and icon text: `regular`, semibold when bold; mono text: the grid's own faces.
    let styled = |mut font: Font, style: &SpanStyle| {
        font.weight = FontWeight(regular);
        if style.bold {
            font.weight = FontWeight(weight::SEMIBOLD);
        }
        if style.italic {
            font.style = FontStyle::Italic;
        }
        font
    };
    for (part, style) in runs.iter().filter(|(part, _)| !part.is_empty()) {
        let font = if part.chars().all(tsp::is_icon_glyph) {
            styled(icon_font.clone(), style)
        } else if style.mono || all_mono {
            mono.get(style.bold, style.italic).clone()
        } else {
            styled(ui_font.clone(), style)
        };
        // the TextRun push stays as it was
```

`cell_width` in `tsp_view.rs` keeps its (family, size) cache key: the weights of one monospace family share the advance.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-ui -j 4 terminal::view` and `cargo test -p mkrew-ui -j 4 tsp` - Expected: PASS.

- [ ] **Step 5: Clippy and commit**

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`.
Commit `feat(ui): the grid draws bold and italic in their own faces and weight; the editor measures its own font`.

---

### Task 4: Code font outside the terminal

**Files (each `theme.terminal_font_family` listed becomes `theme.code_font_family`):**
- `crates/mkrew-ui/src/preview.rs` 942, 1145, 1199, 1305, 1326, 1525, 1530, 1592, test 1912
- `crates/mkrew-ui/src/shell/chat_view.rs` 767, 844
- `crates/mkrew-ui/src/shell/composer.rs` 1247, 1523
- `crates/mkrew-ui/src/shell/dialogs.rs` 312, 340
- `crates/mkrew-ui/src/shell/file_pane.rs` 187 (the `Ln, Col` label; its comment becomes "The code font: its digits are all as wide (tabular).")
- `crates/mkrew-ui/src/shell/git_diff.rs` 766 (the diff's width measure) and 911
- `crates/mkrew-ui/src/shell/git_render.rs` 976, `git_rows.rs` 79, 108, 129
- `crates/mkrew-ui/src/shell/inspector.rs` 707, `jira.rs` 1038, `peek.rs` 504, `run.rs` 394, 594, `send_dialog.rs` 543, `sidebar.rs` 83, 233, `slash.rs` 314, `thread_panel.rs` 322, `widgets.rs` 532, `worktrees.rs` 390
- Not touched (Refinement 2): `terminal/view.rs`, `shell/tsp_view.rs`, `shell/tsp_view/picker.rs`.

**Interfaces:**
- Consumes: `Theme::code_font_family` (Task 2).

- [ ] **Step 1: Make the edits**

Line numbers are from master at 72009d73; find each with `rg -n 'terminal_font_family' crates/mkrew-ui/src` and change exactly the listed files.
A comment next to a changed line that says "the terminal font" now says "the code font".
`theme.terminal_font_size` stays everywhere (one code size, per the spec).

- [ ] **Step 2: Check nothing else outside the terminal reads the grid family**

Run: `rg -n 'terminal_font_family' crates/mkrew-ui/src` - Expected: only `theme.rs`, `terminal/view.rs`, `shell/tsp_view.rs`, `shell/tsp_view/picker.rs`.

- [ ] **Step 3: Run the tests that cover the moved text**

Run: `cargo test -p mkrew-ui -j 4 preview` and `cargo test -p mkrew-ui -j 4 git_diff` - Expected: PASS (the preview icon test compares against `theme.code_font_family`).

- [ ] **Step 4: Clippy and commit**

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`.
Commit `feat(ui): monospace text outside the terminal uses the code font`.

---

### Task 5: Settings dialog rows, text font fields and steppers

**Files:**
- Modify: `crates/mkrew-core/src/settings_fields.rs` (`Kind` ~44-57, `Setting.hidden` doc ~70, constructors ~74-106, Appearance Fonts rows ~248-283, `literal` ~523-536, tests)
- Modify: `crates/mkrew-ui/src/settings_dialog.rs` (`click_focuses_only` ~135-140, `fonts` field ~358-359 and `new` ~392-397, `move_focus` ~729-734, `activate` ~785-803, `list_choices` ~842-866, new `step`, tests ~1451-1456, ~1746, ~1951-1977, ~2018, ~2065-2084)
- Modify: `crates/mkrew-ui/src/shell/settings.rs` (`open_settings_dialog` ~15-18, `settings_row` control width ~827-831, `settings_control` ~899-949, `list_pick_view` ~1031-1035)
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (`text_field` ~1081, `caret_field` ~1095, `caret_field_for` ~1116; new `stepper`)
- Modify: `crates/mkrew-ui/src/theme.rs` (tokens beside `SETTINGS_CONTROL_WIDTH` ~421)

**Interfaces:**
- Consumes: Task 1 keys, Task 2 `Fonts::in_use`, `Theme::code_font_family`.
- Produces: `Kind::Step { min: u16, max: u16, step: u16 }`; `Setting::size_beside(&self) -> Option<&'static Setting>`; `SettingsDialog::new(settings)` (one argument); `SettingsDialog::step(&mut self, setting: &'static Setting, up: bool) -> Effect`; `widgets::stepper(down, value, up, theme) -> gpui::Div`; tokens `SETTINGS_FONT_CONTROL_WIDTH: f32 = 400.0`, `STEPPER_VALUE_WIDTH: f32 = 40.0`.

- [ ] **Step 1: Write the failing tests**

In `settings_fields.rs` tests:

```rust
    #[test]
    fn the_font_rows_follow_tern_and_sizes_sit_beside_their_family() {
        let fonts: Vec<String> = SETTINGS
            .iter()
            .filter(|s| s.page == Page::Appearance && s.group == "Fonts" && !s.hidden)
            .map(Setting::id)
            .collect();
        assert_eq!(
            fonts,
            [
                "terminal.font_family",
                "terminal.bold_font_family",
                "terminal.italic_font_family",
                "terminal.bold_italic_font_family",
                "terminal.font_weight",
                "ui.code_font_family",
                "ui.font_family",
            ]
        );
        for section in ["terminal", "ui"] {
            let family = setting(&format!("{section}.font_family"));
            let size = family.size_beside().expect("a size beside");
            assert_eq!((size.id(), size.hidden), (format!("{section}.font_size"), true));
            assert_eq!(size.kind, Kind::Step { min: 8, max: 32, step: 1 });
        }
        assert_eq!(setting("ui.code_font_family").size_beside(), None);
        assert_eq!(
            setting("terminal.font_weight").kind,
            Kind::Step { min: 100, max: 900, step: 100 }
        );
    }

    #[test]
    fn a_font_list_is_written_as_a_string_and_a_step_as_a_number() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        setting("terminal.font_family")
            .write(&path, " Berkeley Mono, Ioskeley Mono Term ")
            .unwrap();
        setting("terminal.font_weight").write(&path, "500").unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.terminal.font_family, "Berkeley Mono, Ioskeley Mono Term");
        assert_eq!(loaded.terminal.font_weight.0, 500);
    }
```

In `values_show_as_their_toml_names`, add `assert_eq!(setting("terminal.font_weight").current(&defaults), "400");`.
`a_number_with_letters_is_refused` keeps `ui.font_size` (still a key, now `Step`).

In `settings_dialog.rs` tests: `dialog()` becomes `SettingsDialog::new(Ok(Settings::default()))`; the two `SettingsDialog::new(Err(..), Vec::new())` lose the second argument; delete `the_font_pick_lists_default_first_and_filters`; replace `enter_writes_a_changed_draft_and_leaving_the_row_does_too` and add two tests:

```rust
    /// Replaces a three-character draft with `text`.
    fn type_over(d: &mut SettingsDialog, text: &str, keymap: &Keymap) {
        for _ in 0..3 {
            d.key(&key("backspace"), None, keymap);
        }
        for c in text.chars() {
            d.key(&typed(&c.to_string()), None, keymap);
        }
    }

    #[test]
    fn enter_writes_a_changed_draft_and_leaving_the_row_does_too() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        focus_on(&mut d, "terminal.font_weight", &keymap);
        type_over(&mut d, "500", &keymap);
        assert_eq!(
            d.key(&key("enter"), None, &keymap),
            Effect::Write {
                setting: setting("terminal.font_weight"),
                text: "500".into()
            }
        );
        type_over(&mut d, "600", &keymap);
        assert_eq!(
            d.key(&key("tab"), None, &keymap),
            Effect::Write {
                setting: setting("terminal.font_weight"),
                text: "600".into()
            }
        );
        // An unchanged draft writes nothing.
        focus_on(&mut d, "terminal.font_weight", &keymap);
        assert_eq!(d.key(&key("enter"), None, &keymap), Effect::None);
    }

    #[test]
    fn a_font_row_is_typed_like_text() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        focus_on(&mut d, "terminal.bold_font_family", &keymap);
        for c in "Berkeley Mono, Consolas".chars() {
            d.key(&typed(&c.to_string()), None, &keymap);
        }
        assert_eq!(
            d.key(&key("enter"), None, &keymap),
            Effect::Write {
                setting: setting("terminal.bold_font_family"),
                text: "Berkeley Mono, Consolas".into()
            }
        );
        assert!(d.list_pick.is_none(), "no font list opens");
    }

    #[test]
    fn the_stepper_keeps_its_bounds_and_the_read_only_file() {
        let mut d = dialog();
        assert_eq!(
            d.step(setting("terminal.font_weight"), true),
            Effect::Write {
                setting: setting("terminal.font_weight"),
                text: "500".into()
            }
        );
        assert_eq!(
            d.step(setting("ui.font_size"), false),
            Effect::Write {
                setting: setting("ui.font_size"),
                text: "12".into()
            }
        );
        let mut settings = Settings::default();
        settings.ui.font_size = mkrew_core::settings::FontSize(32);
        settings.terminal.font_weight = mkrew_core::settings::TextWeight(100);
        let mut d = SettingsDialog::new(Ok(settings));
        assert_eq!(d.step(setting("ui.font_size"), true), Effect::None, "at the top");
        assert_eq!(d.step(setting("terminal.font_weight"), false), Effect::None, "at the bottom");
        let mut broken = SettingsDialog::new(Err("settings.toml:2:1: oops".into()));
        assert_eq!(broken.step(setting("ui.font_size"), true), Effect::None);
    }
```

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p mkrew-core -j 4 settings_fields` and `cargo test -p mkrew-ui -j 4 settings_dialog` - Expected: compile errors or failures.

- [ ] **Step 3: Implement the table** (`settings_fields.rs`)

`Kind`:

```rust
    /// Font families separated by commas, most wanted first; empty = the built-in default. A
    /// text field showing the family in use as its placeholder (ADR 0120).
    Font,
    /// A whole number in `min`–`max`: typed like [`Kind::Number`], or moved by `step` with
    /// − / + buttons.
    Step { min: u16, max: u16, step: u16 },
```

`hidden` field doc: "Set by another control (a panel edge, a size stepper beside its font row); listed for the template check only."
Beside `hidden()`:

```rust
/// `[section] font_size`: drawn as the stepper beside its `font_family` row, not as a row.
const fn size(section: &'static str) -> Setting {
    Setting {
        kind: Kind::Step {
            min: 8,
            max: 32,
            step: 1,
        },
        ..hidden(section, "font_size")
    }
}
```

Replace the four Appearance "Fonts" rows with these, then `size("terminal"), size("ui"),` (bring `Step` into the `use Kind::{…}` line; `cargo fmt` lays them out):

|section|key|kind|label|help|
|---|---|---|---|---|
|terminal|font_family|`Font`|Font|The terminal grid's family, or several separated by commas, most wanted first. Beside it, the size of the grid and of code in editors, diffs and chats (Ctrl+Shift+= and Ctrl+Shift+- change it and the UI font size together).|
|terminal|bold_font_family|`Font`|Bold font|Bold text's family, or several, most wanted first; empty uses Font's bold face.|
|terminal|italic_font_family|`Font`|Italic font|Italic text's family, or several, most wanted first; empty uses Font.|
|terminal|bold_italic_font_family|`Font`|Bold italic font|Bold italic text's family, or several, most wanted first; empty uses Font's bold face.|
|terminal|font_weight|`Step { min: 100, max: 900, step: 100 }`|Font weight|The terminal grid's text weight; bold draws 300 heavier, at least 700.|
|ui|code_font_family|`Font`|Code font|Monospace text outside the terminal: diffs, chats, blocks and labels. Empty: the editor follows Font, the rest use the built-in Ioskeley Mono Term.|
|ui|font_family|`Font`|UI font|The interface's family, or several, most wanted first; empty uses the built-in Geist. Beside it, the size of the rest of the window: tabs, menus, blocks and settings.|

Each row is `row(Appearance, "Fonts", <section>, <key>, <kind>, <label>, <help>)`.
The macOS shortcut wording is recorded in Task 7's HUMAN_CHECKS (help is one static string).

`Setting`:

```rust
    /// The size drawn beside this font row (`[terminal]` / `[ui] font_size`).
    pub fn size_beside(&self) -> Option<&'static Setting> {
        if self.kind != Kind::Font || self.key != "font_family" {
            return None;
        }
        SETTINGS
            .iter()
            .find(|s| s.section == self.section && s.key == "font_size")
    }
```

`literal`: the numeric arm becomes `Kind::Number | Kind::Step { .. } if !text.is_empty() && …`.

- [ ] **Step 4: Implement the dialog model** (`settings_dialog.rs`)

- `click_focuses_only`: `Kind::Text | Kind::Number | Kind::Font | Kind::Step { .. } | Kind::Choice(_)`.
- `move_focus`: drafts start for `Kind::Text | Kind::Number | Kind::Font | Kind::Step { .. }`.
- `activate`: the two pick arms become `Kind::List(_) if !self.read_only()` and `Kind::List(_)`; the text arm becomes `Kind::Text | Kind::Number | Kind::Font | Kind::Step { .. }`.
- Delete the `fonts` field and parameter: `pub fn new(settings: Result<Settings, String>) -> Self`.
- `list_choices` (doc: "The theme pick's names matching its query."):

```rust
    pub fn list_choices(&self) -> Vec<String> {
        let Some(pick) = self.list_pick.as_ref() else {
            return Vec::new();
        };
        let Kind::List(names) = pick.setting.kind else {
            return Vec::new();
        };
        let query = pick.query.trim().to_lowercase();
        // The theme slots offer the catalog (every built-in and the files of their own
        // appearance); any other list is its fixed table.
        let all: Vec<String> = match pick.setting.id().as_str() {
            "ui.dark_theme" => self.catalog.slot_names(true),
            "ui.light_theme" => self.catalog.slot_names(false),
            _ => names.iter().map(|name| (*name).to_owned()).collect(),
        };
        all.into_iter()
            .filter(|name| name.to_lowercase().contains(&query))
            .collect()
    }
```

- After `choose`:

```rust
    /// A stepper's − / + on `setting` (a [`Kind::Step`]): one step, kept in its bounds. Nothing
    /// is written at a bound or while the file does not parse; a draft of the row shows the
    /// new value.
    pub fn step(&mut self, setting: &'static Setting, up: bool) -> Effect {
        let Kind::Step { min, max, step } = setting.kind else {
            return Effect::None;
        };
        let Ok(now) = self.current(setting).parse::<u16>() else {
            return Effect::None;
        };
        let next = if up {
            now.saturating_add(step).min(max)
        } else {
            now.saturating_sub(step).max(min)
        };
        if next == now || self.read_only() {
            return Effect::None;
        }
        if self.draft.as_ref().is_some_and(|(s, _)| s.id() == setting.id()) {
            self.draft = Some((setting, Input::from(next.to_string())));
        }
        self.write(setting, next.to_string())
    }
```

- [ ] **Step 5: Implement the widgets and rendering**

`theme.rs`, beside `SETTINGS_CONTROL_WIDTH`:

```rust
/// Control column of a font row with its size stepper beside the field (ADR 0120).
pub const SETTINGS_FONT_CONTROL_WIDTH: f32 = 400.0;
/// The number between a stepper's − and + (font size, font weight).
pub const STEPPER_VALUE_WIDTH: f32 = 40.0;
```

`widgets.rs`: `text_field`, `caret_field` and `caret_field_for` take `placeholder: impl IntoElement` instead of `&'static str` (every caller passes a `&'static str`, which still fits; `caret_text` already takes `impl IntoElement`). Add:

```rust
/// `−  value  +` in one field frame (Settings' font sizes and font weight, ADR 0120).
pub(crate) fn stepper(
    down: impl IntoElement,
    value: impl IntoElement,
    up: impl IntoElement,
    theme: &Theme,
) -> gpui::Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(space::XS))
        .px(px(space::XS))
        .rounded(px(radius::MEDIUM))
        .border(px(theme::BORDER_WIDTH))
        .border_color(gpui::rgba(theme.layer[1]))
        .bg(gpui::rgba(theme.layer[0]))
        .child(down)
        .child(
            div()
                .w(theme::rems_px(theme::STEPPER_VALUE_WIDTH))
                .flex()
                .justify_center()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text))
                .child(value),
        )
        .child(up)
}
```

`shell/settings.rs`:
- `open_settings_dialog`: delete the three `fonts` lines; `SettingsDialog::new(settings)`.
- `settings_row`: the control column's width is `SETTINGS_FONT_CONTROL_WIDTH` when `matches!(row, Row::Setting(s) if s.size_beside().is_some())`, else `SETTINGS_CONTROL_WIDTH`.
- `settings_control`: the `Kind::Text | Kind::Number` arm becomes `Kind::Text | Kind::Number | Kind::Font | Kind::Step { .. }`:

```rust
                let placeholder: SharedString = match setting.kind {
                    Kind::Font => cx
                        .global::<theme::Fonts>()
                        .in_use(&setting.id())
                        .unwrap_or_default(),
                    _ => SharedString::default(),
                };
                // `editing` and the caret_field / text_field match as today, passing
                // `placeholder.clone()` where it passed "".
                let field = field
                    .when(setting.kind == Kind::Font, |el| {
                        el.font_family(theme.code_font_family.clone())
                    })
                    .when(read_only, |el| el.opacity(opacity::DISABLED));
                match (setting.kind, setting.size_beside()) {
                    // The number sits inside the stepper, without a frame of its own.
                    (Kind::Step { .. }, _) => self.setting_stepper(
                        dialog,
                        setting,
                        field
                            .w(theme::rems_px(theme::STEPPER_VALUE_WIDTH))
                            .border_0()
                            .bg(gpui::transparent_black())
                            .into_any_element(),
                        theme,
                        cx,
                    ),
                    (_, Some(size)) => div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .child(field.flex_1().min_w_0())
                        .child(self.setting_stepper(
                            dialog,
                            size,
                            dialog.shown(size).into_any_element(),
                            theme,
                            cx,
                        ))
                        .into_any_element(),
                    _ => field.w_full().into_any_element(),
                }
```

- The `Kind::Font | Kind::List(_)` arm becomes `Kind::List(_)`.
- `list_pick_view`: the query placeholder is always `"Search themes"`; drop the `matches!` branch; update its doc ("the matching theme names").
- New method:

```rust
    /// `−  value  +` for a [`Kind::Step`] setting; a button at its bound, or on a broken file,
    /// is faded and does nothing.
    fn setting_stepper(
        &self,
        dialog: &SettingsDialog,
        setting: &'static Setting,
        value: gpui::AnyElement,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let Kind::Step { min, max, .. } = setting.kind else {
            return value;
        };
        let now = dialog.shown(setting).trim().parse::<u16>().ok();
        let button = |up: bool| {
            let id = SharedString::from(format!(
                "step-{}-{}",
                setting.id(),
                if up { "up" } else { "down" }
            ));
            let glyph = if up { Icon::Plus } else { Icon::Minus };
            let can = !dialog.read_only() && now.is_some_and(|n| if up { n < max } else { n > min });
            if !can {
                return disabled_icon_button(id, glyph, theme.text_disabled, theme);
            }
            icon_button(id, glyph, theme).on_press(
                true,
                cx.listener(move |shell, _, window, cx| {
                    cx.stop_propagation();
                    shell.settings_do(move |d, _| d.step(setting, up), window, cx);
                }),
            )
        };
        stepper(button(false), value, button(true), theme).into_any_element()
    }
```

Fix compile errors in the order the compiler reports them; do not restyle unrelated code.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p mkrew-core -j 4 settings_fields` and `cargo test -p mkrew-ui -j 4 settings_dialog` - Expected: PASS, including `every_template_key_has_exactly_one_setting`, `every_setting_has_a_group_and_a_groups_rows_are_contiguous` and the Appearance groups test (`["Theme", "Fonts", "Diffs", "Markdown"]`).

- [ ] **Step 7: Clippy and commit**

Run: `cargo clippy -p mkrew-ui -p mkrew-core --all-targets -j 4 -- -D warnings`.
Commit `feat(ui): Tern-style font rows in Settings: text font lists, size steppers beside Font and UI font, a weight stepper`.

---

### Task 6: Font zoom keys

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs` (`Command` enum, `ALL` ~400, `name` ~522, `mac_keys` ~641, `pc_keys` ~776, tests)
- Modify: `crates/mkrew-ui/src/palette.rs` (`title` ~726, `available` ~754)
- Modify: `crates/mkrew-ui/src/app_menu.rs` (View menu ~63-66)
- Modify: `crates/mkrew-ui/src/shell/sessions.rs` (`run` after the `VoiceInput` check ~177-179, the main match ~538-539)
- Modify: `crates/mkrew-ui/src/shell/detached_actions.rs` (`run_detached` ~379)
- Modify: `crates/mkrew-ui/src/shell/settings.rs` (new `zoom_fonts`)

**Interfaces:**
- Consumes: `mkrew_core::settings::write_font_zoom` (Task 1).
- Produces: `Command::FontSizeUp`, `Command::FontSizeDown`; `Shell::zoom_fonts(&mut self, delta: i16, window: &mut Window, cx: &mut Context<'_, Self>)`.

- [ ] **Step 1: Write the failing test** (in `keymap.rs` tests, beside the test asserting `bound("ctrl-,") == Some(Command::OpenSettings)`; reuse that test's `bound` lookup exactly)

```rust
    #[test]
    fn font_zoom_keys_work_anywhere() {
        // `bound` as in the OpenSettings test above.
        assert_eq!(Command::FontSizeUp.name(), "font_size_up");
        assert_eq!(Command::FontSizeDown.name(), "font_size_down");
        assert_eq!((Command::FontSizeUp.context(), Command::FontSizeDown.context()), (None, None));
        assert_eq!(bound("ctrl-shift-="), Some(Command::FontSizeUp));
        assert_eq!(bound("ctrl-shift--"), Some(Command::FontSizeDown));
    }
```

`shifted_digit_bindings_are_folded_like_windows_reports_them` covers the Shift folding of both new defaults automatically.

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-ui -j 4 font_zoom_keys_work_anywhere` - Expected: no variant `FontSizeUp`.

- [ ] **Step 3: Implement**

- `Command`, after `OpenSettings`: `/// Both font sizes (terminal and UI) one px bigger (ADR 0120).` `FontSizeUp,` and `/// Both font sizes one px smaller.` `FontSizeDown,`.
- `ALL`: both after `Command::OpenSettings`.
- `name`: `"font_size_up".into()`, `"font_size_down".into()`.
- `mac_keys`: `"cmd-="`, `"cmd--"` (macOS never gets Shift + a symbol).
- `pc_keys`: `"ctrl-shift-=".into()`, `"ctrl-shift--".into()`.
- `palette::title`: `"Global: Increase font size".into()`, `"Global: Decrease font size".into()`; `palette::available`: add both to the always-`true` arm with `OpenSettings`.
- `app_menu.rs` View menu: after "Enter Full Screen", `MenuItem::action("Increase Font Size", Run(Command::FontSizeUp))` and `MenuItem::action("Decrease Font Size", Run(Command::FontSizeDown))`.
- `shell/settings.rs`:

```rust
    /// Ctrl+Shift+= / Ctrl+Shift+- (Cmd+= / Cmd+- on macOS): both font sizes one px up or down,
    /// written to `settings.toml` and applied now (ADR 0120). A file that does not parse is
    /// left alone; the palette already says why (§20.0).
    pub(crate) fn zoom_fonts(
        &mut self,
        delta: i16,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Err(error) = mkrew_core::settings::write_font_zoom(&self.settings_file, delta) {
            tracing::warn!("font zoom not written: {error}");
            return;
        }
        self.settings_modified = std::fs::metadata(&self.settings_file)
            .and_then(|m| m.modified())
            .ok();
        self.apply_settings_file(window, cx);
    }
```

- `sessions.rs` `run`, right after the `VoiceInput` early return (zoom acts from any focused field, dialog or pane):

```rust
        // Font zoom changes the app's settings, whatever has the keyboard (ADR 0120).
        if let Command::FontSizeUp | Command::FontSizeDown = *command {
            let delta = if *command == Command::FontSizeUp { 1 } else { -1 };
            return self.zoom_fonts(delta, window, cx);
        }
```

and in the main match `Command::FontSizeUp | Command::FontSizeDown => {}` beside `Command::VoiceInput => {}` under its "Handled at the top of `run`." comment.
- `detached_actions.rs` `run_detached`, before the `OpenSettings` check:

```rust
        if let Command::FontSizeUp | Command::FontSizeDown = command {
            // The settings are the app's: applied from the primary Space window.
            let delta = if command == Command::FontSizeUp { 1 } else { -1 };
            self.in_window(self.primary_host(), cx, |shell, window, cx| {
                shell.zoom_fonts(delta, window, cx);
            });
            return;
        }
```

Add the two variants to any other exhaustive `match` or command list the compiler or tests name (e.g. `palette/tests.rs`), following that list's own grouping.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-ui -j 4 keymap`, `cargo test -p mkrew-ui -j 4 palette` and `cargo test -p mkrew-ui -j 4 app_menu` - Expected: PASS.

- [ ] **Step 5: Clippy and commit**

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`.
Commit `feat(ui): Ctrl+Shift+= and Ctrl+Shift+- change the terminal and UI font size together`.

---

### Task 7: Docs, smoke proof and final verify

**Files:**
- Create: `docs/decisions/0120-tern-font-settings.md`
- Modify: `docs/SPEC.md` (§14 ~1956-1970 and ~2008, §20.0 template ~2504-2527), `docs/HUMAN_CHECKS.md`, `docs/superpowers/specs/2026-11-27-tern-font-settings-design.md`

- [ ] **Step 1: Write ADR 0120** in the style of `docs/decisions/0119-omp-picker-and-markdown-accent.md` (read it first for its headings), one sentence per line, no em dash:
  - Context: Tern's Appearance fonts; a Berkeley-like Ioskeley Mono by default and per-style families.
  - Decision: bundle Ioskeley Mono Term v2.2.0 Regular, Italic, Medium, MediumItalic (OFL, about 8.6 MB), registered like Geist (ADR 0116); comma lists resolved to the first installed family; bold / italic / bold italic families with defaults (the bundled font: its Medium face for bold; another family: itself); a grid weight with bold = weight + 300 clamped 700-900; the code font for monospace text outside the terminal and omp, the editor following Font when the code font is empty; Settings font rows are text fields with the family in use as placeholder, and the installed-font picker is removed; size steppers beside Font and UI font; `font_size_up` / `font_size_down` on Ctrl+Shift+= / Ctrl+Shift+- (Cmd+= / Cmd+- on macOS) move both sizes in one write.
  - The value of `TERMINAL_BOLD_FONT` that Task 2 settled, and why.
  - Consequences: the binary grows by about 8.6 MB; only the first installed list entry is used (the rest are not per-glyph fallbacks); the platform monospace list stays only as a fallback for a failed registration; the omp surface and the terminal find bar keep the grid font.

- [ ] **Step 2: Update SPEC**
  - §14 UI font bullet (~1956): Geist "unless `[ui] font_family` names an installed family (a comma list; the first installed wins)".
  - §14 terminal font bullet (~1963-1970): the bundled Ioskeley Mono Term (ADR 0120), the platform list only when it failed to register; `[terminal] font_family` and the bold / italic / bold italic families take comma lists, the first installed wins; `font_weight` 100-900, bold 300 heavier within 700-900; keep the Nerd Font icon sentence.
  - New §14 bullet after it: the code font (`[ui] code_font_family`, ADR 0120) for monospace text outside the terminal and omp; empty: the Files editor uses the terminal font, the rest Ioskeley Mono Term; Ctrl+Shift+= / Ctrl+Shift+- (Cmd+= / Cmd+- on macOS) change the terminal and UI font size together.
  - ~2008: controls are a switch, a segmented choice, `dropdown_trigger` (themes), text fields for Text, Number and font rows (placeholder: the family in use), and a `−  N  +` stepper for the font weight and for the sizes beside Font and UI font.
  - §20.0 template block: `code_font_family = ""` under `[ui]`; `bold_font_family`, `italic_font_family`, `bold_italic_font_family = ""` and `font_weight = 400` under `[terminal]`, with comments in the block's style; the two `font_family` comments describe lists.

- [ ] **Step 3: Update the design spec** with the two refinements from Global Constraints (one sentence each, under "Bundled font" and "Rendering"), and the settled `TERMINAL_BOLD_FONT` if it changed.

- [ ] **Step 4: HUMAN_CHECKS** (append in the file's own format):
  - macOS and Linux: bold cells in the bundled font draw the Medium face (not a synthetic bold) and stay one cell wide.
  - macOS: the Font row's help names Ctrl+Shift+= / Ctrl+Shift+-; the keys there are Cmd+= / Cmd+-.
  - The installer is about 8.6 MB larger (Ioskeley Mono Term faces).

- [ ] **Step 5: Smoke proof (Windows)** with `scripts/smoke.ps1` (read its header for usage) on the worktree's debug build:
  1. `start`; open a shell tab; run `Write-Output "$([char]27)[1mBold$([char]27)[0m $([char]27)[3mItalic$([char]27)[0m $([char]27)[1;3mBoldItalic$([char]27)[0m Regular 0Oo1lI"`; `shot` in dark and in light. Expect four distinct faces, bold visibly Medium, every glyph on the grid.
  2. Open Settings > Appearance; `shot` dark and light. Expect the rows Font (field + `− 13 +`), Bold font, Italic font, Bold italic font, Font weight (`− 400 +`), Code font, UI font (field + `− 13 +`), with placeholders `Ioskeley Mono Term`, the settled bold family, `Geist`.
  3. Press the Font stepper's + once and Ctrl+Shift+= once; `shot`. Expect the grid and UI to grow live and the sandbox `settings.toml` to hold the new sizes.
  4. Set Font weight to 500 with its stepper; `shot` the shell tab. Expect regular text in Medium and bold still distinct.
  5. Run the `ui-reviewer` subagent on the screenshots; fix what it finds within this feature's files.
  `stop` the sandbox afterwards.

- [ ] **Step 6: Full verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (fallback commands in Global Constraints) - Expected: fmt clean, clippy clean, all tests pass, no `.snap` changes.

- [ ] **Step 7: Commit**

Commit any smoke fixes first as `fix(ui): …`, then `docs: ADR 0120 Tern font settings, SPEC, human checks` with the docs files.
