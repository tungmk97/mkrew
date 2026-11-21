# Theme gallery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Settings > Appearance opens with a gallery of every theme (22 built-in plus the valid files of `<config>/themes/`), hovering a card previews the whole app, a click keeps the theme for its slot, and a user or an agent adds a theme by saving a small TOML file.

**Architecture:** `Theme::name` becomes an owned `SharedString`; a pure `theme_file` module parses and writes theme files; `theme::problems` is the one place that knows the contrast rules (the tests and the file loader both call it); a `ThemeCatalog` GPUI global (built-ins, then valid files, plus error entries) replaces every direct read of `THEMES`.
The existing 500 ms settings poll also stamps the themes folder and rebuilds the catalog.
The gallery is a pure model inside `settings_dialog.rs` (cards, hover with a serial for the delay, keyboard, badges, `Effect::KeepTheme`) and `shell/theme_gallery.rs` only draws it and applies the preview.

**Tech Stack:** Rust 2024, gpui-pre 0.3.7, `toml` 1.1 and `serde` (already dependencies of `mkrew-ui`), `tempfile` (dev), insta snapshots (unchanged).

**Spec:** `docs/superpowers/specs/2026-11-22-theme-gallery-design.md` (sections 3 to 9).

## Global Constraints

- Work in the worktree `C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/theme-gallery` on branch `feat/theme-gallery`; always use absolute paths under it (relative paths hit the main checkout).
- Commit with `git -c user.email=tungnv975@gmail.com commit`, Conventional Commits, only the files of the task, never push.
- Shell: `bash` in the omp tool is WSL; use `sh`, or `"C:/Program Files/Git/bin/bash.exe"` for `scripts/verify.sh`.
  Prefix cargo with `CARGO_BUILD_JOBS=4`.
- Gate after every task (the workspace must compile and be clippy clean): `CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-core -p mkrew-ui --all-targets -- -D warnings` and `cargo fmt --all -- --check` (run `cargo fmt --all` first).
- Every color, spacing, radius, font size and duration comes from `crates/mkrew-ui/src/theme.rs` tokens; spacing only 4 / 8 / 12 / 16 / 24 / 32; no hex colors or raw px numbers outside `theme.rs` (palette literals of built-in themes are in `theme.rs`).
- Only `shell/widgets.rs` and `pressable.rs` call `.hover(`, `.active(` or `.cursor_pointer()`; the gallery card is `card_press` plus `.on_press`; `.on_hover(listener)` is allowed (peek and the sidebar use it).
- No `unwrap` / `expect` outside tests.
  Tests test behavior, never read source text, never use the network.
- `PROTOCOL_VERSION` stays 54 (no daemon work).
  ADR number is 0101 (`docs/decisions/0101-theme-gallery-and-theme-files.md`, amends ADR 0078).
- Never the em dash character in code, docs or UI text; use `-`.
  In long Markdown put each sentence on its own line.
- Spec values used verbatim: at least 22 built-in themes; `THEME_PREVIEW_DELAY` 120 ms; `THEME_CARD_WIDTH` (token) wide cards with `space::M` gaps; theme files are `<config>/themes/*.toml` with the fields of `Palette`; the 500 ms poll stays a poll with a `ponytail:` comment naming the ceiling and `notify`; an invalid file shows its error on a card and never breaks the app; a name taken by a built-in or an earlier file is skipped with an error card; "Low contrast" badge with the first problem in a tooltip; "New theme from current" writes `<name> copy.toml` (`copy 2`, `copy 3`) and opens it in Zed; "Open themes folder" creates the folder and opens it in the OS file manager.
- Base commit for every line number below: `59b0906` (`docs(spec): theme gallery ...`).
  Line numbers drift as tasks land; the function or test name is the real anchor.

## Review Focus

The spec is silent on these inputs; each line names the test that pins it.

1. A theme name that holds `"`, `\` or a non-ASCII letter (`Rosé Pine`) is saved to `[ui] dark_theme` / `light_theme` as a valid TOML string and a copy's file name never holds `\ / : * ? " < > |` (Task 2 `file_name`, Task 5 `toml_string`).
2. A `*.toml` entry that is not UTF-8, is larger than 64 KB or is a directory is an error card or ignored, and never hangs or breaks the page (Task 4 `read_folder` test).
3. The kept theme's file is removed or renamed: the slot falls back to the slot's Mkrew theme like an unknown name (Task 5 `a_file_theme_resolves_until_its_file_goes`).
4. `settings.toml` does not parse (the dialog is read-only): previews still work, a click on a card writes nothing (Task 8 test).
5. The catalog is replaced while a preview or the keyboard sits on a card that no longer exists: the preview ends and the keyboard moves to a card that exists (Task 8 tests).

## File structure

| File | Responsibility |
| --- | --- |
| `crates/mkrew-core/src/paths.rs` | `Paths::themes_dir()` |
| `crates/mkrew-core/src/settings.rs` | `THEME_NAMES` grows to 22 (Task 6) |
| `crates/mkrew-ui/src/theme.rs` | owned `Theme::name`, `problems()`, `Palette::from_theme`, `toml_string`, `Theme::resolve` with a catalog, 10 new palettes, gallery tokens |
| `crates/mkrew-ui/src/theme_file.rs` (new) | parse / render a theme file, copy and file names |
| `crates/mkrew-ui/src/theme_catalog.rs` (new) | `ThemeCatalog`, folder reading, `ThemesStamp` |
| `crates/mkrew-ui/src/settings_dialog.rs` | catalog snapshot, slot lists, gallery model |
| `crates/mkrew-ui/src/palette.rs` | the theme picker lists the catalog |
| `crates/mkrew-ui/src/shell/theme_gallery.rs` (new) | catalog reload, preview sync, gallery drawing, New theme / Open folder |
| `crates/mkrew-ui/src/shell/{navigation,settings,connection}.rs`, `main.rs` | wiring |
| `docs/decisions/0101-...md`, `docs/SPEC.md`, `README.md`, `docs/HUMAN_CHECKS.md` | docs (Task 11) |

## Tasks

1. Owned theme name
2. Theme files: `Paths::themes_dir`, parse, render, names
3. `problems()` replaces the per-rule contrast loops
4. `ThemeCatalog` and folder reading (pure)
5. Catalog wired: global, resolve, picker, slot lists, `keep_theme`
6. Ten new built-in palettes
7. Poll reload of the themes folder
8. Gallery model in `settings_dialog`
9. Gallery drawing and preview sync
10. New theme from current, Open themes folder
11. Docs (ADR 0101, SPEC, README, HUMAN_CHECKS) and full verify

---

### Task 1: Owned theme name

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (struct field line 842, `dark()` 1003, `light()` 1096, `shared()` 1334, `from_palette` 1475 and 1506, tests 2059 and 2332-2346)
- Modify: `crates/mkrew-ui/src/shell/navigation.rs` (lines 531-532)

**Interfaces:**
- Consumes: nothing.
- Produces: `Theme::name: SharedString`; `Theme::from_palette(name: impl Into<SharedString>, p: &Palette) -> Theme`.

- [ ] **Step 1: Write the failing test**

In `theme.rs` `mod tests`, after `the_theme_table_matches_the_settings_names`, add:

```rust
    #[test]
    fn a_theme_may_carry_a_name_that_is_not_static() {
        let palette = Palette {
            dark: true,
            bg: 0x1E2030,
            chrome: 0x191A2A,
            surface: 0x222436,
            fg: 0xC8D3F5,
            muted: 0x828BB8,
            accent: 0x82AAFF,
            red: 0xFF757F,
            green: 0xC3E88D,
            yellow: 0xFFC777,
            blue: 0x82AAFF,
            magenta: 0xC099FF,
            cyan: 0x86E1FC,
            ansi: [
                0x1B1D2B, 0xFF757F, 0xC3E88D, 0xFFC777, 0x82AAFF, 0xC099FF, 0x86E1FC, 0x828BB8,
                0x444A73, 0xFF8D94, 0xC7FB6D, 0xFFD8AB, 0x9AB8FF, 0xCAABFF, 0xB2EBFF, 0xC8D3F5,
            ],
        };
        let theme = Theme::from_palette(String::from("My Theme"), &palette);
        assert_eq!(theme.name.as_ref(), "My Theme");
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib theme::tests::a_theme_may_carry_a_name_that_is_not_static`
Expected: compile error `E0308 mismatched types` (expected `&'static str`, found `String`) at `Theme::from_palette`.

- [ ] **Step 3: Implement**

`theme.rs`:

1. Lines 841-842, replace
   ```rust
       /// The built-in theme this is (ADR 0078), e.g. "Mkrew Dark".
       pub name: &'static str,
   ```
   with
   ```rust
       /// The theme this is, e.g. "Mkrew Dark" (ADR 0078); owned because the name of a theme
       /// file is not `'static` (ADR 0101).
       pub name: SharedString,
   ```
2. Line 1003 `name: "Mkrew Dark",` becomes `name: "Mkrew Dark".into(),`.
3. Line 1096 `name: "Mkrew Light",` becomes `name: "Mkrew Light".into(),`.
4. Line 1334 `name: "",` becomes `name: "".into(),`.
5. Line 1475 `pub fn from_palette(name: &'static str, p: &Palette) -> Self {` becomes `pub fn from_palette(name: impl Into<SharedString>, p: &Palette) -> Self {` and line 1506 `name,` becomes `name: name.into(),`.
6. Test at line 2059: `assert_eq!(theme.name, entry.name);` becomes `assert_eq!(theme.name.as_ref(), entry.name);`.
7. Replace the whole body of `each_slot_picks_its_name` (lines 2331-2339) with:
   ```rust
       #[test]
       fn each_slot_picks_its_name() {
           let n = names("Solarized Light", "Dracula");
           assert_eq!(
               Theme::resolve(ThemeMode::Dark, false, &n).name.as_ref(),
               "Dracula"
           );
           assert_eq!(
               Theme::resolve(ThemeMode::Light, true, &n).name.as_ref(),
               "Solarized Light"
           );
           assert_eq!(
               Theme::resolve(ThemeMode::System, true, &n).name.as_ref(),
               "Dracula"
           );
       }
   ```
8. In `a_slot_may_hold_the_other_appearance` (line 2345) replace the assertion with:
   ```rust
           assert_eq!((theme.name.as_ref(), theme.dark), ("GitHub Light", false));
   ```

`shell/navigation.rs` (inside `open_theme_picker`):
- Line 531 `palette::theme_entries(current.name)` becomes `palette::theme_entries(&current.name)`.
- Line 532 `theme::THEMES.iter().position(|e| e.name == current.name)` becomes `theme::THEMES.iter().position(|e| e.name == current.name.as_ref())`.

If the compiler reports another use of `Theme::name` as `&str`, use `.as_ref()` (comparison) or `&theme.name` (argument); `format!` and `assert!` messages need nothing.

- [ ] **Step 4: Run the tests**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib theme::`
Expected: all `theme::tests` PASS, including `a_theme_may_carry_a_name_that_is_not_static` and `the_mkrew_themes_never_change` (snapshots unchanged).
Run: `CARGO_BUILD_JOBS=4 cargo check -p mkrew-ui --all-targets`
Expected: finishes without errors.

- [ ] **Step 5: Gate and commit**

```bash
cargo fmt --all
CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-core -p mkrew-ui --all-targets -- -D warnings
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/shell/navigation.rs
git -c user.email=tungnv975@gmail.com commit -m "refactor(ui): theme name is an owned SharedString"
```

---

### Task 2: Theme files: `Paths::themes_dir`, parse, render, names

**Files:**
- Modify: `crates/mkrew-core/src/paths.rs` (after `prompts_dir` line 106; test `overrides_win` line 152)
- Create: `crates/mkrew-ui/src/theme_file.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (add `pub mod theme_file;` after `pub mod theme;` line 97)

**Interfaces:**
- Consumes: `theme::Palette` (public fields).
- Produces:
  - `Paths::themes_dir(&self) -> PathBuf`
  - `theme_file::ThemeFile { name: String, palette: Palette }`
  - `theme_file::parse(src: &str) -> Result<ThemeFile, String>`
  - `theme_file::render(name: &str, palette: &Palette) -> String`
  - `theme_file::file_name(theme_name: &str) -> String`
  - `theme_file::copy_name(name: &str, taken: impl Fn(&str) -> bool) -> String`

- [ ] **Step 1: Write the failing tests**

`paths.rs`, in `overrides_win` after the `prompts_dir` assertion add:

```rust
        assert_eq!(paths.themes_dir(), Path::new("/tmp/c").join("themes"));
```

Create `crates/mkrew-ui/src/theme_file.rs` with the tests only first (the code of Step 3 goes above them):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"
name = "My Theme"
dark = true
bg = "#1e2030"
chrome = "#191a2a"
surface = "#222436"
fg = "#c8d3f5"
muted = "#828bb8"
accent = "#82aaff"
red = "#ff757f"
green = "#c3e88d"
yellow = "#ffc777"
blue = "#82aaff"
magenta = "#c099ff"
cyan = "#86e1fc"
ansi = ["#1b1d2b", "#ff757f", "#c3e88d", "#ffc777", "#82aaff", "#c099ff", "#86e1fc", "#828bb8",
        "#444a73", "#ff8d94", "#c7fb6d", "#ffd8ab", "#9ab8ff", "#caabff", "#b2ebff", "#c8d3f5"]
"##;

    #[test]
    fn a_valid_file_gives_the_palette() {
        let file = parse(SAMPLE).unwrap();
        assert_eq!(file.name, "My Theme");
        assert!(file.palette.dark);
        assert_eq!(file.palette.bg, 0x1E2030);
        assert_eq!(file.palette.cyan, 0x86E1FC);
        assert_eq!(file.palette.ansi[0], 0x1B1D2B);
        assert_eq!(file.palette.ansi[15], 0xC8D3F5);
    }

    #[test]
    fn colors_are_case_insensitive() {
        let upper = SAMPLE.replace("#1e2030", "#1E2030");
        assert_eq!(parse(&upper).unwrap(), parse(SAMPLE).unwrap());
    }

    #[test]
    fn a_missing_key_is_an_error_naming_it() {
        let text = SAMPLE.replace("cyan = \"#86e1fc\"\n", "");
        let error = parse(&text).unwrap_err();
        assert!(error.contains("cyan"), "{error}");
    }

    #[test]
    fn an_unknown_key_is_an_error_naming_it() {
        let text = format!("{SAMPLE}bgg = \"#000000\"\n");
        let error = parse(&text).unwrap_err();
        assert!(error.contains("bgg"), "{error}");
    }

    #[test]
    fn a_bad_color_names_its_key_and_value() {
        for bad in ["#12345", "#gggggg", "123456", "#1234567"] {
            let text = SAMPLE.replace("\"#1e2030\"", &format!("\"{bad}\""));
            let error = parse(&text).unwrap_err();
            assert!(error.contains("bg:") && error.contains(bad), "{error}");
        }
        let text = SAMPLE.replace("\"#ff757f\", \"#c3e88d\"", "\"#ff757f\", \"nope\"");
        assert!(parse(&text).unwrap_err().contains("ansi[2]"));
    }

    #[test]
    fn a_short_ansi_list_is_an_error_naming_ansi() {
        let text = SAMPLE.replace(", \"#c8d3f5\"]", "]");
        let error = parse(&text).unwrap_err();
        assert!(error.contains("ansi") && error.contains("16"), "{error}");
    }

    #[test]
    fn a_syntax_error_names_its_line() {
        let error = parse("name = \"x\"\ndark = maybe\n").unwrap_err();
        assert!(error.contains("line 2"), "{error}");
    }

    #[test]
    fn a_name_must_be_text_a_person_can_read() {
        for bad in ["", "   ", "a\\nb"] {
            let text = SAMPLE.replace("My Theme", bad);
            let error = parse(&text).unwrap_err();
            assert!(error.contains("name"), "{bad:?}: {error}");
        }
        let padded = SAMPLE.replace("My Theme", "  Padded  ");
        assert_eq!(parse(&padded).unwrap().name, "Padded");
    }

    #[test]
    fn a_rendered_file_parses_back_to_the_same_palette() {
        let original = parse(SAMPLE).unwrap();
        for name in ["My Theme copy", "Rosé Pine", "He said \"hi\"", "back\\slash"] {
            let text = render(name, &original.palette);
            let back = parse(&text).unwrap();
            assert_eq!(back.name, name);
            assert_eq!(back.palette, original.palette);
        }
    }

    #[test]
    fn file_names_never_hold_what_a_path_cannot() {
        assert_eq!(file_name("Nord copy"), "Nord copy.toml");
        assert_eq!(file_name("a/b\\c:d*e?f\"g<h>i|j"), "a_b_c_d_e_f_g_h_i_j.toml");
        assert_eq!(file_name("Rosé Pine copy"), "Rosé Pine copy.toml");
        assert_eq!(file_name("trailing. "), "trailing.toml");
        assert_eq!(file_name("..."), "theme.toml");
    }

    #[test]
    fn a_copy_takes_the_first_free_number() {
        let taken = |names: &'static [&'static str]| move |n: &str| names.contains(&n);
        assert_eq!(copy_name("Nord", taken(&[])), "Nord copy");
        assert_eq!(copy_name("Nord", taken(&["Nord copy"])), "Nord copy 2");
        assert_eq!(
            copy_name("Nord", taken(&["Nord copy", "Nord copy 2"])),
            "Nord copy 3"
        );
        assert_eq!(
            copy_name("Nord copy", taken(&["Nord copy copy"])),
            "Nord copy copy 2"
        );
    }
}
```

Add `pub mod theme_file;` to `lib.rs` after `pub mod theme;` so the file is compiled.

- [ ] **Step 2: Run to see failures**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-core paths::`
Expected: FAIL to compile, `no method named themes_dir`.
Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib theme_file`
Expected: FAIL to compile, `cannot find function parse` (and the others).

- [ ] **Step 3: Implement**

`paths.rs`, after `prompts_dir`:

```rust
    /// Theme files, one `*.toml` per theme (ADR 0101).
    pub fn themes_dir(&self) -> PathBuf {
        self.config_dir.join("themes")
    }
```

`theme_file.rs`, above the tests:

```rust
//! Theme files (ADR 0101): `<config>/themes/*.toml`, a name plus the fields of
//! [`Palette`]. Pure: the catalog reads the folder, this module only turns text into a
//! palette and back.

use serde::Deserialize;

use crate::theme::Palette;

/// A parsed theme file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeFile {
    pub name: String,
    pub palette: Palette,
}

/// The keys of a theme file, all required; an unknown key is an error (a typo must not pass
/// silently).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    name: String,
    dark: bool,
    bg: String,
    chrome: String,
    surface: String,
    fg: String,
    muted: String,
    accent: String,
    red: String,
    green: String,
    yellow: String,
    blue: String,
    magenta: String,
    cyan: String,
    ansi: Vec<String>,
}

/// `#RRGGBB` (any case) as `0xRRGGBB`; the error names `key`.
fn color(key: &str, text: &str) -> Result<u32, String> {
    text.strip_prefix('#')
        .filter(|hex| hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
        .ok_or_else(|| format!("{key}: \"{text}\" is not a color like #RRGGBB"))
}

/// Reads a theme file. The error is one line a person can act on: the TOML line and message,
/// or the key that is missing, unknown or wrong.
pub fn parse(src: &str) -> Result<ThemeFile, String> {
    let raw: Raw = toml::from_str(src).map_err(|error| {
        let line = error.span().map_or(String::new(), |span| {
            let before = src.get(..span.start).unwrap_or_default();
            format!("line {}: ", before.matches('\n').count() + 1)
        });
        format!("{line}{}", error.message().trim_end())
    })?;
    let name = raw.name.trim().to_owned();
    if name.is_empty() {
        return Err("name: must not be empty".to_owned());
    }
    if name.chars().any(char::is_control) {
        return Err("name: must not contain control characters".to_owned());
    }
    let ansi = raw
        .ansi
        .iter()
        .enumerate()
        .map(|(i, text)| color(&format!("ansi[{i}]"), text))
        .collect::<Result<Vec<_>, _>>()?;
    let ansi = <[u32; 16]>::try_from(ansi)
        .map_err(|colors| format!("ansi: needs 16 colors, found {}", colors.len()))?;
    let palette = Palette {
        dark: raw.dark,
        bg: color("bg", &raw.bg)?,
        chrome: color("chrome", &raw.chrome)?,
        surface: color("surface", &raw.surface)?,
        fg: color("fg", &raw.fg)?,
        muted: color("muted", &raw.muted)?,
        accent: color("accent", &raw.accent)?,
        red: color("red", &raw.red)?,
        green: color("green", &raw.green)?,
        yellow: color("yellow", &raw.yellow)?,
        blue: color("blue", &raw.blue)?,
        magenta: color("magenta", &raw.magenta)?,
        cyan: color("cyan", &raw.cyan)?,
        ansi,
    };
    Ok(ThemeFile { name, palette })
}

/// The text of a theme file for `palette` named `name` ("New theme from current").
pub fn render(name: &str, palette: &Palette) -> String {
    let color = |value: u32| format!("\"#{value:06x}\"");
    let row = |colors: &[u32]| {
        colors
            .iter()
            .map(|c| color(*c))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut text = String::from("# A Mkrew theme. Every key is required; colors are #RRGGBB.\n");
    let mut line = |key: &str, value: String| {
        text.push_str(key);
        text.push_str(" = ");
        text.push_str(&value);
        text.push('\n');
    };
    line("name", toml::Value::String(name.to_owned()).to_string());
    line("dark", palette.dark.to_string());
    for (key, value) in [
        ("bg", palette.bg),
        ("chrome", palette.chrome),
        ("surface", palette.surface),
        ("fg", palette.fg),
        ("muted", palette.muted),
        ("accent", palette.accent),
        ("red", palette.red),
        ("green", palette.green),
        ("yellow", palette.yellow),
        ("blue", palette.blue),
        ("magenta", palette.magenta),
        ("cyan", palette.cyan),
    ] {
        line(key, color(value));
    }
    line(
        "ansi",
        format!(
            "[\n  {},\n  {},\n]",
            row(&palette.ansi[..8]),
            row(&palette.ansi[8..])
        ),
    );
    text
}

/// `<name>.toml` with every character a file name cannot hold replaced by `_`.
pub fn file_name(theme_name: &str) -> String {
    let stem: String = theme_name
        .chars()
        .map(|c| {
            if c.is_control() || "\\/:*?\"<>|".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let stem = stem.trim_end_matches(['.', ' ']);
    format!("{}.toml", if stem.is_empty() { "theme" } else { stem })
}

/// The name of a copy of `name`: "<name> copy", then "<name> copy 2", "<name> copy 3", ...
/// the first one `taken` does not know.
pub fn copy_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut number = 1;
    loop {
        let candidate = if number == 1 {
            format!("{name} copy")
        } else {
            format!("{name} copy {number}")
        };
        if !taken(&candidate) {
            return candidate;
        }
        number += 1;
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-core paths::`
Expected: PASS (`overrides_win`).
Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib theme_file`
Expected: 11 tests PASS.

- [ ] **Step 5: Gate and commit**

```bash
cargo fmt --all
CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-core -p mkrew-ui --all-targets -- -D warnings
git add crates/mkrew-core/src/paths.rs crates/mkrew-ui/src/theme_file.rs crates/mkrew-ui/src/lib.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): theme file parse, render and names"
```

---

### Task 3: `problems()` replaces the per-rule contrast loops

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (add after `contrast`, line 1951; `ThemeEntry` impl line 1586; tests)

**Interfaces:**
- Consumes: `contrast`, `glass_over`, `Theme` tokens.
- Produces:
  - `theme::Problem { rule: &'static str, detail: String }` (`Clone, Debug, PartialEq, Eq`, `Display` as `rule: detail`)
  - `theme::problems(theme: &Theme) -> Vec<Problem>`
  - `ThemeEntry::palette(&self) -> Option<&Palette>`

The rules are the ones the theme tests check today: body text, text on glass, on accent, window close, selected text, code, toast, highlighted diff lines, change bars, diff lines, classic sidebar text, workspace colors, avatar letters, chevron.

- [ ] **Step 1: Write the failing tests**

In `theme.rs` `mod tests` add:

```rust
    #[test]
    fn every_built_in_theme_meets_every_contrast_rule() {
        for theme in all_themes() {
            let found = problems(&theme);
            assert!(
                found.is_empty(),
                "{}: {}",
                theme.name,
                found
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }
    }

    #[test]
    fn a_palette_with_no_contrast_reports_the_rules_it_breaks() {
        let mut palette = *THEMES[1].palette().expect("One Dark has a palette");
        palette.fg = palette.bg;
        palette.accent = palette.bg;
        let found = problems(&Theme::from_palette("Bad", &palette));
        let rules: Vec<&str> = found.iter().map(|problem| problem.rule).collect();
        for rule in ["body text", "on accent", "code"] {
            assert!(rules.contains(&rule), "{rule} missing from {rules:?}");
        }
        let first = found[0].to_string();
        assert!(first.starts_with(found[0].rule), "{first}");
        assert!(first.contains(" on "), "the colors are named: {first}");
    }
```

- [ ] **Step 2: Run to see them fail**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib theme::tests::every_built_in`
Expected: compile error `cannot find function problems` and `no method named palette`.

- [ ] **Step 3: Implement**

`ThemeEntry` (after `build`, line ~1593):

```rust
    /// The palette the theme is derived from; `None` for the two Mkrew themes, which are
    /// written token by token.
    pub fn palette(&self) -> Option<&Palette> {
        self.palette.as_ref()
    }
```

After `contrast` (line 1951):

```rust
/// A contrast rule a theme does not meet (ADR 0101): which rule, and the colors that fail it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub rule: &'static str,
    pub detail: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.rule, self.detail)
    }
}

/// The contrast rules `theme` does not meet, each failing pair once; empty for every built-in
/// theme (a test), and the reason for the "Low contrast" badge of a theme file.
pub fn problems(theme: &Theme) -> Vec<Problem> {
    let t = theme;
    let mut found: Vec<Problem> = Vec::new();
    let mut check = |rule: &'static str, fg: u32, bg: u32, min: f32| {
        let ratio = contrast(fg, bg);
        if ratio < min {
            let problem = Problem {
                rule,
                detail: format!("{fg:06X} on {bg:06X} reads {ratio:.2}:1, needs {min}:1"),
            };
            if !found.contains(&problem) {
                found.push(problem);
            }
        }
    };
    let grounds = [
        t.bg,
        t.surface,
        t.surface_raised,
        t.sidebar,
        t.hover,
        t.selected,
        t.card,
    ];
    for bg in grounds {
        for fg in [t.text, t.text_muted] {
            check("body text", fg, bg, 4.5);
        }
    }
    // Decoded exactly as `with_glass` quantized them; the page behind is a light or a dark one.
    let alpha = (t.glass & 0xFF) as f32 / 255.0;
    let tint = (t.glass_tint & 0xFF) as f32 / 255.0;
    for backdrop in [t.bg, t.text] {
        let fill = glass_over(t.surface, t.accent, alpha, tint, backdrop);
        check("text on glass", t.text, fill, 4.5);
    }
    check("on accent", t.on_accent, t.accent, 3.0);
    // The close button's red hover under its cross.
    check("window close", t.on_accent, t.status_exited, 3.0);
    for bg in [
        t.terminal_selection_bg,
        t.terminal_match_bg,
        t.terminal_match_current_bg,
    ] {
        check("selected text", t.text, bg, 4.5);
    }
    let code = [
        t.syntax_keyword,
        t.syntax_string,
        t.syntax_number,
        t.syntax_type,
        t.syntax_function,
    ];
    for fg in code.into_iter().chain([t.accent]) {
        for bg in [t.bg, t.surface, t.surface_raised] {
            check("code", fg, bg, 4.5);
        }
    }
    check("toast", t.text, t.surface_raised, 4.5);
    for mark in [t.accent_text, t.status_exited, t.text_muted] {
        check("toast mark", mark, t.surface_raised, 3.0);
    }
    for fg in code.into_iter().chain([t.text, t.text_muted]) {
        for bg in [t.diff_add_bg, t.diff_del_bg, t.diff_mod_bg] {
            check("highlighted diff", fg, bg, 4.5);
        }
    }
    // WCAG 1.4.11: the Files editor's change bars are graphical objects.
    for fg in [t.diff_add_text, t.diff_mod_text, t.diff_del_text] {
        for bg in [t.bg, t.surface, t.surface_raised] {
            check("change bars", fg, bg, 3.0);
        }
    }
    for (fg, bg) in [
        (t.diff_add_text, t.diff_add_bg),
        (t.diff_del_text, t.diff_del_bg),
        (t.diff_mod_text, t.diff_mod_bg),
        (t.text_dim, t.diff_add_gutter),
        (t.text_dim, t.diff_del_gutter),
        (t.text_dim, t.diff_mod_gutter),
        (t.git_modified, t.bg),
        (t.git_untracked, t.bg),
        (t.git_modified, t.selected),
        (t.git_untracked, t.selected),
    ] {
        check("diff lines", fg, bg, 4.5);
    }
    // The sidebar's session list keeps Mkrew's classic palette on this theme's sidebar.
    let classic = t.classic();
    for fg in [classic.text, classic.text_muted] {
        check("sidebar text", fg, t.sidebar, 4.5);
    }
    for color in t.workspace_colors {
        for bg in grounds {
            check("workspace colors", color, bg, 3.0);
        }
    }
    for bg in [t.sidebar, t.hover, t.selected] {
        for color in t.workspace_colors {
            let (letter, fill) = t.avatar_colors(color, bg);
            check("avatar letters", letter, fill, 4.5);
        }
        check("chevron", t.chevron_color(), bg, 3.0);
    }
    found
}
```

In `mod tests`, delete these tests (whole `#[test] fn` with its doc comment); the new `every_built_in_theme_meets_every_contrast_rule` replaces them (line numbers at the base commit):
- `text_on_glass_reads_at_4_5_over_a_light_or_dark_page` (1993-2009)
- `the_classic_sidebar_text_reads_on_every_sidebar` (2080-2089)
- `body_text_contrast_is_at_least_4_5` (2240-2262)
- `window_close_hover_is_readable` with its doc comment (2358-2365)
- `on_accent_text_is_readable_on_the_accent` (2367-2377)
- `selected_text_stays_readable` (2379-2391)
- `code_stays_readable` (2393-2417)
- `toast_text_and_marks_read_on_the_card` with its doc comment (2418-2433)
- `highlighted_diff_lines_stay_readable` (2435-2460)
- `change_bars_stand_out_from_the_editor` with its doc comment (2461-2482)
- `diff_line_colors_read_on_their_backgrounds` with its doc comment (2483-2509)
- `avatar_letters_and_chevrons_stay_readable` (2611-2634)

Keep `glass_is_derived_from_surface_and_accent_in_every_theme` (it pins the derivation, not a contrast rule).

Replace `workspace_colors_stand_out_from_every_surface_and_from_each_other` (2556-2599) with the hue and names part only (its contrast loop now lives in `problems`):

```rust
    #[test]
    fn workspace_colors_are_distinct_and_every_name_has_one() {
        for theme in all_themes() {
            // Clearly distinct: neighbouring hues (round the color wheel) differ by ≥ 12°.
            let mut hues: Vec<f32> = theme.workspace_colors.iter().map(|c| hue(*c)).collect();
            hues.sort_by(f32::total_cmp);
            for pair in hues.windows(2) {
                assert!(pair[1] - pair[0] >= 12.0, "hues too close: {hues:?}");
            }
            let wrap = hues[0] + 360.0 - hues[hues.len() - 1];
            assert!(wrap >= 12.0, "hues too close round the wheel: {hues:?}");
            // Every name has a color; a hex value or unknown name has none.
            for name in WORKSPACE_COLORS {
                assert!(theme.workspace_color(name).is_some(), "{name}");
            }
            let teal = WORKSPACE_COLORS.iter().position(|n| *n == "teal").unwrap();
            assert_eq!(
                theme.workspace_color("teal"),
                Some(theme.workspace_colors[teal])
            );
            assert_eq!(theme.workspace_color("#ff0000"), None);
        }
    }
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib theme::`
Expected: PASS, including both new tests; the remaining tests compile (the `hue` helper is still used).

- [ ] **Step 5: Gate and commit**

```bash
cargo fmt --all
CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-core -p mkrew-ui --all-targets -- -D warnings
git add crates/mkrew-ui/src/theme.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): theme::problems, one place for the contrast rules"
```

---

### Task 4: `ThemeCatalog` and folder reading (pure)

**Files:**
- Create: `crates/mkrew-ui/src/theme_catalog.rs`
- Modify: `crates/mkrew-ui/src/theme.rs` (add `Palette::from_theme` after the `Palette` struct, line 1378)
- Modify: `crates/mkrew-ui/src/lib.rs` (add `pub mod theme_catalog;` after `pub mod theme_file;`)

**Interfaces:**
- Consumes: `theme::{THEMES, Palette, Problem, Theme, problems}`, `theme_file::{parse, ThemeFile}`.
- Produces:
  - `Palette::from_theme(theme: &Theme) -> Palette`
  - `Origin { BuiltIn, File(String) }`
  - `CatalogTheme { name: SharedString, dark: bool, origin: Origin, palette: Palette, problems: Vec<Problem>, theme: Theme }`
  - `FileError { file: String, message: String }`
  - `ThemeCatalog` (a `gpui::Global`, `Clone + Debug + PartialEq`): `builtin()`, `load(dir: &Path)`, `from_files(Vec<(String, Result<String, String>)>)`, `themes() -> &[CatalogTheme]`, `errors() -> &[FileError]`, `find(&str) -> Option<&CatalogTheme>`, `index_of(&str) -> Option<usize>`, `group(dark: bool) -> impl Iterator<Item = &CatalogTheme>`, `slot_names(dark: bool) -> Vec<String>`
  - `read_folder(dir: &Path) -> Vec<(String, Result<String, String>)>`
  - `ThemesStamp` (`Clone + Debug + Default + PartialEq + Eq`): `read(dir: &Path) -> ThemesStamp`
  - `MAX_FILE_BYTES: u64 = 65_536`

- [ ] **Step 1: Write the failing tests**

Create `theme_catalog.rs` with the tests first (code of Step 3 goes above):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme_file;

    /// A theme file for a copy of Mkrew Dark (`dark`) or Mkrew Light named `name`.
    fn file(name: &str, dark: bool) -> (String, Result<String, String>) {
        let base = if dark { Theme::dark() } else { Theme::light() };
        (
            format!("{name}.toml"),
            Ok(theme_file::render(name, &Palette::from_theme(&base))),
        )
    }

    fn names(catalog: &ThemeCatalog) -> Vec<&str> {
        catalog.themes().iter().map(|t| &*t.name).collect()
    }

    #[test]
    fn built_ins_come_first_in_table_order() {
        let catalog = ThemeCatalog::builtin();
        let table: Vec<&str> = THEMES.iter().map(|e| e.name).collect();
        assert_eq!(names(&catalog), table);
        assert!(catalog.themes().iter().all(|t| t.origin == Origin::BuiltIn));
        assert!(catalog.themes().iter().all(|t| t.problems.is_empty()));
        assert!(catalog.errors().is_empty());
        assert_eq!(catalog.index_of("Mkrew Dark"), Some(0));
        assert_eq!(catalog.find("Mkrew Light").map(|t| t.dark), Some(false));
        assert!(catalog.find("mkrew dark").is_none(), "exact names only");
    }

    #[test]
    fn file_themes_follow_the_built_ins_sorted_by_name() {
        let catalog = ThemeCatalog::from_files(vec![file("zed", true), file("Aurora", false)]);
        let all = names(&catalog);
        let built_in = THEMES.len();
        assert_eq!(&all[built_in..], ["Aurora", "zed"]);
        assert_eq!(catalog.find("Aurora").unwrap().origin, Origin::File("Aurora.toml".into()));
        assert!(catalog.errors().is_empty());
    }

    #[test]
    fn a_name_taken_by_a_built_in_or_an_earlier_file_is_an_error_card() {
        let same = (
            "b.toml".to_owned(),
            Ok(theme_file::render("a", &Palette::from_theme(&Theme::dark()))),
        );
        let catalog = ThemeCatalog::from_files(vec![file("Dracula", true), file("a", true), same]);
        let taken: Vec<_> = catalog.errors().iter().map(|e| e.file.as_str()).collect();
        assert_eq!(taken, ["Dracula.toml", "b.toml"]);
        assert!(catalog.errors()[0].message.contains("built-in"), "{:?}", catalog.errors());
        assert!(catalog.errors()[1].message.contains("a.toml"), "{:?}", catalog.errors());
        assert_eq!(names(&catalog).iter().filter(|n| **n == "a").count(), 1);
        assert_eq!(names(&catalog).iter().filter(|n| **n == "Dracula").count(), 1);
    }

    #[test]
    fn a_broken_or_unreadable_file_is_an_error_card_and_the_rest_loads() {
        let catalog = ThemeCatalog::from_files(vec![
            ("bad.toml".into(), Ok("oops".into())),
            ("gone.toml".into(), Err("cannot read the file: denied".into())),
            file("Good", true),
        ]);
        assert!(catalog.find("Good").is_some());
        let files: Vec<_> = catalog.errors().iter().map(|e| e.file.as_str()).collect();
        assert_eq!(files, ["bad.toml", "gone.toml"]);
        assert_eq!(catalog.errors()[1].message, "cannot read the file: denied");
    }

    #[test]
    fn a_removed_file_leaves_the_catalog() {
        let with = ThemeCatalog::from_files(vec![file("Aurora", true)]);
        assert!(with.find("Aurora").is_some());
        assert_eq!(ThemeCatalog::from_files(Vec::new()), ThemeCatalog::builtin());
    }

    #[test]
    fn a_low_contrast_file_loads_with_its_problems() {
        let mut palette = Palette::from_theme(&Theme::dark());
        palette.fg = palette.bg;
        let text = theme_file::render("Dim", &palette);
        let catalog = ThemeCatalog::from_files(vec![("dim.toml".into(), Ok(text))]);
        let dim = catalog.find("Dim").expect("loaded, the user's file is the user's call");
        assert!(!dim.problems.is_empty());
        assert!(catalog.errors().is_empty());
    }

    #[test]
    fn the_slot_lists_hold_every_built_in_and_the_files_of_their_appearance() {
        let catalog = ThemeCatalog::from_files(vec![file("Aurora", true), file("Dawn", false)]);
        let dark = catalog.slot_names(true);
        let light = catalog.slot_names(false);
        for list in [&dark, &light] {
            assert!(list.contains(&"Dracula".to_owned()) && list.contains(&"Mkrew Light".to_owned()));
        }
        assert!(dark.contains(&"Aurora".to_owned()) && !dark.contains(&"Dawn".to_owned()));
        assert!(light.contains(&"Dawn".to_owned()) && !light.contains(&"Aurora".to_owned()));
        let groups: Vec<_> = catalog.group(true).map(|t| t.dark).collect();
        assert!(groups.iter().all(|dark| *dark));
        assert_eq!(catalog.group(true).count() + catalog.group(false).count(), catalog.themes().len());
    }

    #[test]
    fn the_folder_is_read_for_toml_files_only_and_odd_ones_become_errors() {
        let dir = tempfile::tempdir().unwrap();
        let moon = theme_file::render("Moon", &Palette::from_theme(&Theme::dark()));
        std::fs::write(dir.path().join("moon.TOML"), moon).unwrap();
        std::fs::write(dir.path().join("notes.txt"), "x").unwrap();
        std::fs::create_dir(dir.path().join("sub.toml")).unwrap();
        std::fs::write(dir.path().join("binary.toml"), [0xFF, 0xFE, 0x00]).unwrap();
        std::fs::write(
            dir.path().join("huge.toml"),
            vec![b'#'; MAX_FILE_BYTES as usize + 1],
        )
        .unwrap();
        let catalog = ThemeCatalog::load(dir.path());
        assert!(catalog.find("Moon").is_some(), "the extension is case-insensitive");
        let bad: Vec<_> = catalog.errors().iter().map(|e| e.file.as_str()).collect();
        assert_eq!(bad, ["binary.toml", "huge.toml"]);
        assert!(catalog.errors()[1].message.contains("64 KB"), "{:?}", catalog.errors());
    }

    #[test]
    fn a_missing_folder_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = ThemeCatalog::load(&dir.path().join("themes"));
        assert_eq!(catalog, ThemeCatalog::builtin());
    }

    #[test]
    fn the_stamp_changes_with_every_edit_add_and_removal() {
        let dir = tempfile::tempdir().unwrap();
        let empty = ThemesStamp::read(dir.path());
        assert_eq!(empty, ThemesStamp::read(&dir.path().join("missing")));
        let file = dir.path().join("a.toml");
        std::fs::write(&file, "x").unwrap();
        let one = ThemesStamp::read(dir.path());
        assert_ne!(one, empty);
        // The length differs even when the clock did not tick.
        std::fs::write(&file, "xy").unwrap();
        let two = ThemesStamp::read(dir.path());
        assert_ne!(two, one);
        std::fs::write(dir.path().join("notes.txt"), "z").unwrap();
        assert_eq!(ThemesStamp::read(dir.path()), two, "only .toml files count");
        std::fs::remove_file(&file).unwrap();
        assert_eq!(ThemesStamp::read(dir.path()), empty);
    }
}
```

Add `pub mod theme_catalog;` to `lib.rs` after `pub mod theme_file;`.

- [ ] **Step 2: Run to see failures**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib theme_catalog`
Expected: compile errors (`ThemeCatalog`, `Origin`, `Palette::from_theme` not found).

- [ ] **Step 3: Implement**

`theme.rs`, directly after the `Palette` struct (after line 1378):

```rust
impl Palette {
    /// The palette a theme can be written back as (the two Mkrew themes have none of their
    /// own): its own background, chrome, surface, text, muted text, accent and ANSI colors,
    /// with the six hues taken from ANSI red to cyan.
    pub fn from_theme(theme: &Theme) -> Self {
        let a = theme.ansi;
        Self {
            dark: theme.dark,
            bg: theme.bg,
            chrome: theme.chrome,
            surface: theme.surface,
            fg: theme.text,
            muted: theme.text_muted,
            accent: theme.accent,
            red: a[1],
            green: a[2],
            yellow: a[3],
            blue: a[4],
            magenta: a[5],
            cyan: a[6],
            ansi: a,
        }
    }
}
```

`theme_catalog.rs`, above the tests:

```rust
//! The themes the UI can show (ADR 0101): the compiled built-ins in table order, then the
//! valid files of `<config>/themes/` by name, plus one error entry per file that is not
//! loaded. A GPUI global; everything that lists or finds a theme reads it.

use std::path::Path;
use std::time::SystemTime;

use gpui::SharedString;

use crate::theme::{self, Palette, Problem, THEMES, Theme};
use crate::theme_file;

/// The largest theme file read; a theme is a couple of kilobytes, so a bigger `*.toml` is a
/// mistake and must not stall the 500 ms poll.
pub const MAX_FILE_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    BuiltIn,
    /// The file name inside the themes folder.
    File(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CatalogTheme {
    pub name: SharedString,
    pub dark: bool,
    pub origin: Origin,
    /// What "New theme from current" writes for this theme.
    pub palette: Palette,
    /// The contrast rules the theme breaks; empty for every built-in.
    pub problems: Vec<Problem>,
    pub theme: Theme,
}

/// A file (or the folder) that is not loaded, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileError {
    pub file: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThemeCatalog {
    themes: Vec<CatalogTheme>,
    errors: Vec<FileError>,
}

impl gpui::Global for ThemeCatalog {}

impl ThemeCatalog {
    /// Only the built-in themes.
    pub fn builtin() -> Self {
        let themes = THEMES
            .iter()
            .map(|entry| {
                let theme = entry.build();
                CatalogTheme {
                    name: theme.name.clone(),
                    dark: entry.dark,
                    origin: Origin::BuiltIn,
                    palette: entry
                        .palette()
                        .copied()
                        .unwrap_or_else(|| Palette::from_theme(&theme)),
                    problems: theme::problems(&theme),
                    theme,
                }
            })
            .collect();
        Self {
            themes,
            errors: Vec::new(),
        }
    }

    /// The built-ins and the valid files of `dir` (a missing folder holds none).
    pub fn load(dir: &Path) -> Self {
        Self::from_files(read_folder(dir))
    }

    /// The built-ins and these files, `(file name, its text or why it could not be read)`.
    /// Files are taken in file-name order, so "an earlier file" has a meaning; the loaded
    /// ones are then listed by theme name.
    pub fn from_files(mut files: Vec<(String, Result<String, String>)>) -> Self {
        let mut catalog = Self::builtin();
        files.sort_by(|a, b| a.0.cmp(&b.0));
        let mut loaded: Vec<CatalogTheme> = Vec::new();
        for (file, text) in files {
            let parsed = match text.and_then(|text| theme_file::parse(&text)) {
                Ok(parsed) => parsed,
                Err(message) => {
                    catalog.errors.push(FileError { file, message });
                    continue;
                }
            };
            let taken = (catalog.themes.iter().chain(&loaded)).find(|t| *t.name == *parsed.name);
            if let Some(other) = taken {
                let by = match &other.origin {
                    Origin::BuiltIn => "the built-in theme".to_owned(),
                    Origin::File(other_file) => other_file.clone(),
                };
                catalog.errors.push(FileError {
                    file,
                    message: format!("Name taken by {by}"),
                });
                continue;
            }
            let theme = Theme::from_palette(parsed.name.clone(), &parsed.palette);
            loaded.push(CatalogTheme {
                name: parsed.name.into(),
                dark: parsed.palette.dark,
                origin: Origin::File(file),
                palette: parsed.palette,
                problems: theme::problems(&theme),
                theme,
            });
        }
        // Stable: two names that differ only in case keep their file order.
        loaded.sort_by_key(|t| t.name.to_lowercase());
        catalog.themes.extend(loaded);
        catalog
    }

    pub fn themes(&self) -> &[CatalogTheme] {
        &self.themes
    }

    pub fn errors(&self) -> &[FileError] {
        &self.errors
    }

    pub fn find(&self, name: &str) -> Option<&CatalogTheme> {
        self.themes.iter().find(|t| &*t.name == name)
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.themes.iter().position(|t| &*t.name == name)
    }

    /// The themes of one appearance, in catalog order (a gallery group).
    pub fn group(&self, dark: bool) -> impl Iterator<Item = &CatalogTheme> {
        self.themes.iter().filter(move |t| t.dark == dark)
    }

    /// The names a slot's Settings list offers: every built-in (a slot may hold the other
    /// appearance, ADR 0078) and the valid files of the slot's own appearance.
    pub fn slot_names(&self, dark: bool) -> Vec<String> {
        self.themes
            .iter()
            .filter(|t| t.origin == Origin::BuiltIn || t.dark == dark)
            .map(|t| t.name.to_string())
            .collect()
    }
}

fn is_theme_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        && path.is_file()
}

fn read_file(path: &Path) -> Result<String, String> {
    let size = std::fs::metadata(path)
        .map_err(|error| format!("cannot read the file: {error}"))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(format!(
            "the file is larger than {} KB",
            MAX_FILE_BYTES / 1024
        ));
    }
    std::fs::read_to_string(path).map_err(|error| format!("cannot read the file: {error}"))
}

/// The `*.toml` files of `dir` as `(file name, text or error)`. A missing folder holds none;
/// a folder that cannot be read is one entry named after it, so the gallery shows one error
/// card with the path and the OS error.
pub fn read_folder(dir: &Path) -> Vec<(String, Result<String, String>)> {
    let folder_error =
        |error: std::io::Error| (dir.display().to_string(), Err(format!("cannot read the folder: {error}")));
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => return vec![folder_error(error)],
    };
    let mut files = Vec::new();
    for entry in entries {
        match entry {
            Err(error) => files.push(folder_error(error)),
            Ok(entry) => {
                let path = entry.path();
                if is_theme_file(&path) {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    files.push((name, read_file(&path)));
                }
            }
        }
    }
    files
}

/// The names, times and sizes of the `*.toml` files of a folder: the poll compares two of
/// them to know the catalog is stale. The size is there because two saves inside one clock
/// tick keep the same time.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemesStamp(Vec<(String, Option<SystemTime>, u64)>);

impl ThemesStamp {
    pub fn read(dir: &Path) -> Self {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Self::default();
        };
        let mut files: Vec<(String, Option<SystemTime>, u64)> = entries
            .flatten()
            .filter(|entry| is_theme_file(&entry.path()))
            .map(|entry| {
                let meta = entry.metadata().ok();
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    meta.as_ref().and_then(|m| m.modified().ok()),
                    meta.map_or(0, |m| m.len()),
                )
            })
            .collect();
        files.sort();
        Self(files)
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib theme_catalog`
Expected: 10 tests PASS.
Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib theme::`
Expected: still PASS.

- [ ] **Step 5: Gate and commit**

```bash
cargo fmt --all
CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-core -p mkrew-ui --all-targets -- -D warnings
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/theme_catalog.rs crates/mkrew-ui/src/lib.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): theme catalog with theme files, errors and a folder stamp"
```

---

### Task 5: Catalog wired: global, resolve, picker, slot lists, `keep_theme`

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (`resolve` 870-877, `find_theme` 1596-1599, `refresh` 1865-1869, tests 2053-2070 and 2296-2346; add `toml_string`)
- Modify: `crates/mkrew-ui/src/palette.rs` (`theme_entries` 815-833, doc of `Action::Theme` 442) and `crates/mkrew-ui/src/palette/tests.rs` (1203-1222)
- Modify: `crates/mkrew-ui/src/settings_dialog.rs` (imports line 9 and after 19; struct 272-296; `new` 299-315; `list_choices` 562-581; tests module)
- Modify: `crates/mkrew-ui/src/main.rs` (import near line 122; `set_global` near line 244)
- Modify: `crates/mkrew-ui/src/shell/navigation.rs` (527-598)
- Modify: `crates/mkrew-ui/src/shell/settings.rs` (`open_settings_dialog` 8-26)

**Interfaces:**
- Consumes: Task 4 `ThemeCatalog`.
- Produces:
  - `Theme::resolve(mode, os_dark, names: &ThemeNames, catalog: &ThemeCatalog) -> Theme`
  - `theme::toml_string(text: &str) -> String`
  - `palette::theme_entries(catalog: &ThemeCatalog, current: &str) -> Vec<Entry>`; `Action::Theme(i)` indexes `catalog.themes()`
  - `SettingsDialog::catalog: ThemeCatalog` (pub) and `SettingsDialog::set_catalog(&mut self, ThemeCatalog)`
  - `Shell::keep_theme(&mut self, name: &str, dark: bool, window, cx)`
  - `find_theme` is deleted (every caller migrates).

- [ ] **Step 1: Write the failing tests**

`theme.rs` tests: add near `names`:

```rust
    fn catalog() -> ThemeCatalog {
        ThemeCatalog::builtin()
    }
```

Replace the five resolve tests (lines 2303-2346) with the same tests passing the catalog:

```rust
    #[test]
    fn mode_resolves_against_the_os() {
        let n = ThemeNames::from_settings(&Settings::default());
        assert!(Theme::resolve(ThemeMode::System, true, &n, &catalog()).dark);
        assert!(!Theme::resolve(ThemeMode::System, false, &n, &catalog()).dark);
        assert!(!Theme::resolve(ThemeMode::Light, true, &n, &catalog()).dark);
        assert!(Theme::resolve(ThemeMode::Dark, false, &n, &catalog()).dark);
        assert_eq!(
            Theme::resolve(ThemeMode::Dark, false, &n, &catalog()).mode,
            ThemeMode::Dark
        );
    }

    #[test]
    fn default_names_resolve_to_exactly_today() {
        let n = ThemeNames::from_settings(&Settings::default());
        assert_eq!(
            Theme::resolve(ThemeMode::System, true, &n, &catalog()),
            Theme::dark()
        );
        assert_eq!(
            Theme::resolve(ThemeMode::System, false, &n, &catalog()),
            Theme::light()
        );
    }

    #[test]
    fn unknown_names_fall_back_to_mkrew() {
        let n = names("nope", "dracula");
        assert_eq!(
            Theme::resolve(ThemeMode::System, true, &n, &catalog()),
            Theme::dark()
        );
        assert_eq!(
            Theme::resolve(ThemeMode::System, false, &n, &catalog()),
            Theme::light()
        );
    }

    #[test]
    fn each_slot_picks_its_name() {
        let n = names("Solarized Light", "Dracula");
        let name = |mode, os_dark| {
            Theme::resolve(mode, os_dark, &n, &catalog())
                .name
                .to_string()
        };
        assert_eq!(name(ThemeMode::Dark, false), "Dracula");
        assert_eq!(name(ThemeMode::Light, true), "Solarized Light");
        assert_eq!(name(ThemeMode::System, true), "Dracula");
    }

    #[test]
    fn a_slot_may_hold_the_other_appearance() {
        let n = names("Mkrew Light", "GitHub Light");
        let theme = Theme::resolve(ThemeMode::Dark, false, &n, &catalog());
        assert_eq!((theme.name.as_ref(), theme.dark), ("GitHub Light", false));
    }

    #[test]
    fn a_file_theme_resolves_until_its_file_goes() {
        let file = (
            "aurora.toml".to_owned(),
            Ok(crate::theme_file::render(
                "Aurora",
                &Palette::from_theme(&Theme::dark()),
            )),
        );
        let with = ThemeCatalog::from_files(vec![file]);
        let n = names("Mkrew Light", "Aurora");
        let shown = Theme::resolve(ThemeMode::System, true, &n, &with);
        assert_eq!(shown.name.as_ref(), "Aurora");
        assert_eq!(
            Theme::resolve(ThemeMode::System, true, &n, &catalog()),
            Theme::dark(),
            "the file is gone: the slot's Mkrew theme, like an unknown name"
        );
    }

    #[test]
    fn a_theme_name_is_saved_as_a_valid_toml_string() {
        for name in ["Plain", "Rosé Pine", "He said \"hi\"", "back\\slash"] {
            let text = format!("theme = {}", toml_string(name));
            let table: toml::Table = text.parse().unwrap();
            assert_eq!(table["theme"].as_str(), Some(name), "{text}");
        }
    }
```

In `the_theme_table_matches_the_settings_names` replace the `find_theme` lines (2061-2070) with:

```rust
        let catalog = catalog();
        assert_eq!(
            catalog.find("Mkrew Dark").map(|t| t.theme.clone()),
            Some(Theme::dark())
        );
        assert_eq!(
            catalog.find("Mkrew Light").map(|t| t.theme.clone()),
            Some(Theme::light())
        );
        assert!(catalog.find("dracula").is_none(), "exact names only");
```

`palette/tests.rs` lines 1205 and 1217: `theme_entries("Dracula")` becomes `theme_entries(&crate::theme_catalog::ThemeCatalog::builtin(), "Dracula")`.

`settings_dialog.rs` `mod tests`: add the imports and helper, and the slot test:

```rust
    use crate::theme::{Palette, Theme};
    use crate::theme_catalog::ThemeCatalog;
```

```rust
    /// A theme file for a copy of Mkrew Dark (`dark`) or Mkrew Light named `name`.
    fn file_theme(name: &str, dark: bool) -> (String, Result<String, String>) {
        let base = if dark { Theme::dark() } else { Theme::light() };
        (
            format!("{name}.toml"),
            Ok(crate::theme_file::render(name, &Palette::from_theme(&base))),
        )
    }

    #[test]
    fn the_theme_slot_lists_add_the_files_of_their_appearance() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.set_catalog(ThemeCatalog::from_files(vec![
            file_theme("Aurora", true),
            file_theme("Dawn", false),
        ]));
        focus_on(&mut d, "ui.dark_theme", &keymap);
        d.key(&key("enter"), None, &keymap);
        let dark = d.list_choices();
        assert!(dark.contains(&"Aurora".to_owned()) && !dark.contains(&"Dawn".to_owned()));
        assert!(dark.contains(&"Dracula".to_owned()), "every built-in stays");
        d.key(&key("escape"), None, &keymap);
        focus_on(&mut d, "ui.light_theme", &keymap);
        d.key(&key("enter"), None, &keymap);
        let light = d.list_choices();
        assert!(light.contains(&"Dawn".to_owned()) && !light.contains(&"Aurora".to_owned()));
        assert!(light.contains(&"Dracula".to_owned()));
    }
```

- [ ] **Step 2: Run to see failures**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib theme::tests::a_file_theme`
Expected: compile errors (`resolve` takes 3 arguments, `toml_string` missing, `set_catalog` missing).

- [ ] **Step 3: Implement**

`theme.rs`:

1. `resolve` (lines 868-877):
   ```rust
       /// The theme of `mode`'s slot (SPEC §14, ADR 0078, ADR 0101); a name the catalog does
       /// not know (a removed file) is that slot's Mkrew theme.
       pub fn resolve(
           mode: ThemeMode,
           os_dark: bool,
           names: &ThemeNames,
           catalog: &ThemeCatalog,
       ) -> Self {
           let dark = slot_is_dark(mode, os_dark);
           let theme = catalog.find(names.slot(dark)).map_or_else(
               || if dark { Self::dark() } else { Self::light() },
               |listed| listed.theme.clone(),
           );
           Self { mode, ..theme }
       }
   ```
2. Delete `find_theme` (lines 1596-1599, with its doc comment).
3. `refresh` (lines 1865-1869):
   ```rust
   pub fn refresh(mode: ThemeMode, window: &Window, cx: &mut App) {
       let fonts = cx.global::<Fonts>().clone();
       let names = cx.global::<ThemeNames>().clone();
       let theme = Theme::resolve(mode, os_dark(window), &names, cx.global::<ThemeCatalog>());
       cx.set_global(theme.with_fonts(&fonts));
   }
   ```
   and add `Fonts`/catalog doc: "Called at startup, on OS appearance changes, on a settings or themes folder reload and by the toggle."
4. After `slot_key` (line 669) add:
   ```rust
   /// `text` as a TOML string literal (quoted and escaped): the value `write_ui_value` takes,
   /// so a theme name with a quote or a backslash cannot break `settings.toml`.
   pub fn toml_string(text: &str) -> String {
       toml::Value::String(text.to_owned()).to_string()
   }
   ```
5. Add `use crate::theme_catalog::ThemeCatalog;` next to the other `use` lines at the top of `theme.rs`.

`palette.rs` (lines 813-833):

```rust
/// The theme picker's rows (ADR 0078, ADR 0101): every theme of the catalog, its appearance
/// on the right, the shown one marked. `Action::Theme(i)` is the index into the catalog.
pub fn theme_entries(catalog: &crate::theme_catalog::ThemeCatalog, current: &str) -> Vec<Entry> {
    catalog
        .themes()
        .iter()
        .enumerate()
        .map(|(index, listed)| {
            let look = if listed.dark { "Dark" } else { "Light" };
            Entry::new(
                Action::Theme(index),
                listed.name.to_string(),
                if &*listed.name == current {
                    format!("✓ {look}")
                } else {
                    look.into()
                },
            )
        })
        .collect()
}
```

and the doc of `Action::Theme` (line 442) becomes `/// The theme picker's row for the theme at this index of the theme catalog (ADR 0101).`

`settings_dialog.rs`:

- Line 9 stays `use gpui::Keystroke;` for now. Add after `use crate::task_dialog::{DialogKey, dialog_key};`:
  ```rust
  use crate::theme_catalog::ThemeCatalog;
  ```
- In `SettingsDialog` (after `pub prompts: PromptsPage,`):
  ```rust
      /// The themes the slot lists pick from (ADR 0101); the shell hands over every new
      /// catalog with [`SettingsDialog::set_catalog`].
      pub catalog: ThemeCatalog,
  ```
  and in `new()` after `prompts: PromptsPage::default(),`: `catalog: ThemeCatalog::builtin(),`.
- After `read_only()`:
  ```rust
      /// A rebuilt catalog (the themes folder changed).
      pub fn set_catalog(&mut self, catalog: ThemeCatalog) {
          self.catalog = catalog;
      }
  ```
- `list_choices` (lines 562-581): replace the `if let Kind::List(names) = pick.setting.kind { ... }` block with:
  ```rust
          if let Kind::List(names) = pick.setting.kind {
              // The theme slots offer the catalog (every built-in and the files of their own
              // appearance); any other list is its fixed table.
              let all: Vec<String> = match pick.setting.id().as_str() {
                  "ui.dark_theme" => self.catalog.slot_names(true),
                  "ui.light_theme" => self.catalog.slot_names(false),
                  _ => names.iter().map(|name| (*name).to_owned()).collect(),
              };
              return all.into_iter().filter(|name| matches(name)).collect();
          }
  ```

`main.rs`: add the import after line 122 `use mkrew_ui::theme::{...};`:
```rust
use mkrew_ui::theme_catalog::ThemeCatalog;
```
and after line 244 `cx.set_global(theme::ThemeNames::from_settings(&settings));`:
```rust
            // Theme files are read once here (the status tick reads them again when they change).
            cx.set_global(ThemeCatalog::load(&paths.themes_dir()));
```

`shell/navigation.rs` (lines 527-598): replace `open_theme_picker`'s opening, `preview_theme_pick` and `choose_theme`; `restore_theme_pick` stays.

```rust
    /// The theme picker (ADR 0078): the palette listing the theme catalog, on the current one.
    pub(crate) fn open_theme_picker(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.restore_theme_pick(window, cx);
        let current = cx.theme().clone();
        let catalog = cx.global::<ThemeCatalog>();
        let mut palette = Palette::new(palette::theme_entries(catalog, &current.name));
        if let Some(index) = catalog.index_of(&current.name) {
            palette.pick_entry(index);
        }
        // ... the rest of the function is unchanged (from `self.workspace_menu = None;` on).
```
(keep every line after the `if let Some(index)` block as it is).

```rust
    /// Shows the picked theme without saving it.
    fn preview_theme_pick(&mut self, cx: &mut Context<'_, Self>) {
        let Some(mode) = self.theme_pick else {
            return;
        };
        let Some(palette::Action::Theme(index)) = self.palette.as_ref().and_then(Palette::picked)
        else {
            return;
        };
        let fonts = cx.global::<theme::Fonts>().clone();
        let Some(listed) = cx.global::<ThemeCatalog>().themes().get(index) else {
            return;
        };
        let shown = Theme {
            mode,
            ..listed.theme.clone()
        };
        cx.set_global(shown.with_fonts(&fonts));
        cx.refresh_windows();
    }

    /// Saves theme `index` to the slot shown and shows it (Enter or a click in the picker).
    fn choose_theme(&mut self, index: usize, window: &mut Window, cx: &mut Context<'_, Self>) {
        // The catalog may have been rebuilt since the picker listed it.
        let Some(name) = (cx.global::<ThemeCatalog>().themes().get(index)).map(|t| t.name.clone())
        else {
            return;
        };
        let dark = theme::slot_is_dark(cx.theme().mode, theme::os_dark(window));
        self.keep_theme(&name, dark, window, cx);
    }

    /// Saves `name` to the slot of `dark` and shows the kept theme of the slot that is shown
    /// (a theme of the other appearance changes the other slot only). A failed write keeps
    /// the choice for this session and says so, like the title bar toggle.
    pub(crate) fn keep_theme(
        &mut self,
        name: &str,
        dark: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let mode = cx.theme().mode;
        let mut names = cx.global::<theme::ThemeNames>().clone();
        if dark {
            names.dark = name.into();
        } else {
            names.light = name.into();
        }
        cx.set_global(names);
        theme::refresh(mode, window, cx);
        cx.refresh_windows();
        let value = theme::toml_string(name);
        if let Err(error) =
            mkrew_core::settings::write_ui_value(&self.settings_file, theme::slot_key(dark), &value)
        {
            self.settings_write_failed("the theme", error, cx);
        }
    }
```

`shell/settings.rs` `open_settings_dialog` (line 18): replace `self.settings_dialog = Some(SettingsDialog::new(settings, fonts));` with:

```rust
        let mut dialog = SettingsDialog::new(settings, fonts);
        dialog.set_catalog(cx.global::<ThemeCatalog>().clone());
        self.settings_dialog = Some(dialog);
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib`
Expected: PASS (theme resolve tests, `a_file_theme_resolves_until_its_file_goes`, `a_theme_name_is_saved_as_a_valid_toml_string`, palette picker tests, `the_theme_slot_lists_add_the_files_of_their_appearance`, `the_theme_pick_lists_the_names_and_filters`).
Run: `CARGO_BUILD_JOBS=4 cargo check -p mkrew-ui --all-targets`
Expected: no errors (the bin compiles with the catalog global).

- [ ] **Step 5: Gate and commit**

```bash
cargo fmt --all
CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-core -p mkrew-ui --all-targets -- -D warnings
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette/tests.rs crates/mkrew-ui/src/settings_dialog.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/navigation.rs crates/mkrew-ui/src/shell/settings.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): the theme catalog drives resolve, the picker and the slot lists"
```

---

### Task 6: Ten new built-in palettes

**Files:**
- Modify: `crates/mkrew-core/src/settings.rs` (`THEME_NAMES` 250-264; test lines 736-737)
- Modify: `crates/mkrew-ui/src/theme.rs` (`THEMES`: six dark entries before the `Mkrew Light` entry at line 1734, four light entries before `];` at line 1850; tests)
- Modify: `crates/mkrew-ui/src/palette/tests.rs` (1203-1222)

**Interfaces:**
- Consumes: Task 3 `problems` test, Task 5 catalog.
- Produces: 22 built-in themes in order Dark: Mkrew Dark, One Dark, Dracula, Catppuccin Mocha, Tokyo Night, Gruvbox Dark, Nord, Solarized Dark, GitHub Dark, Rosé Pine, Kanagawa, Everforest Dark; Light: Mkrew Light, One Light, GitHub Light, Catppuccin Latte, Solarized Light, Gruvbox Light, Rosé Pine Dawn, Everforest Light, Ayu Light, Tokyo Night Day.
  The picker order stays "dark ones, then light ones", so `THEME_NAMES[12]` is `DEFAULT_LIGHT_THEME`.

Every palette below comes from the project's own published colors (URLs in the comment above each entry).
Where the existing contrast rules (`problems()`, which includes the Mkrew workspace colors drawn on every sidebar, hover and selected fill) reject a published color, one field was changed by the least amount that passes; the change is in the comment and in the ADR table (Task 11).
The values were checked with an independent script that repeats `from_palette`, `with_glass` and the rules of `problems()`; the smallest margin is 0.04:1.

- [ ] **Step 1: Write the failing tests**

`settings.rs` test (line 736-737): replace
```rust
        assert_eq!(THEME_NAMES[0], DEFAULT_DARK_THEME);
        assert_eq!(THEME_NAMES[6], DEFAULT_LIGHT_THEME);
```
with
```rust
        assert_eq!(THEME_NAMES[0], DEFAULT_DARK_THEME);
        assert_eq!(
            THEME_NAMES[12], DEFAULT_LIGHT_THEME,
            "the light ones follow the 12 dark ones"
        );
```

`theme.rs` tests, add:

```rust
    #[test]
    fn there_are_22_built_in_themes_grouped_dark_then_light() {
        assert_eq!(THEMES.len(), 22);
        assert_eq!(THEMES.iter().filter(|e| e.dark).count(), 12);
        assert!(THEMES.iter().take(12).all(|e| e.dark), "picker order: dark first");
        assert!(THEMES.iter().skip(12).all(|e| !e.dark));
        let mut names: Vec<&str> = THEMES.iter().map(|e| e.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 22, "names are unique");
    }
```

`palette/tests.rs` (lines 1203-1222): replace the two tests with:

```rust
#[test]
fn theme_entries_list_every_theme_with_its_appearance_and_mark_the_current() {
    let catalog = crate::theme_catalog::ThemeCatalog::builtin();
    let entries = theme_entries(&catalog, "Dracula");
    assert_eq!(entries.len(), crate::theme::THEMES.len());
    assert_eq!(entries[0].action, Action::Theme(0));
    assert_eq!(entries[0].title, "Mkrew Dark");
    assert_eq!(entries[0].shortcut, "Dark");
    assert_eq!(entries[2].title, "Dracula");
    assert_eq!(entries[2].shortcut, "✓ Dark");
    assert_eq!(entries[12].title, "Mkrew Light");
    assert_eq!(entries[13].shortcut, "Light");
}

#[test]
fn the_picker_starts_on_the_current_theme_and_moves_from_there() {
    let catalog = crate::theme_catalog::ThemeCatalog::builtin();
    let mut palette = Palette::new(theme_entries(&catalog, "Dracula"));
    palette.pick_entry(2);
    assert_eq!(palette.picked(), Some(Action::Theme(2)));
    palette.step(1);
    assert_eq!(palette.picked(), Some(Action::Theme(3)));
    palette.insert("solarized light");
    let solarized_light = catalog.index_of("Solarized Light").unwrap();
    assert_eq!(palette.picked(), Some(Action::Theme(solarized_light)));
}
```

- [ ] **Step 2: Run to see failures**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib theme::tests::there_are_22`
Expected: FAIL `assertion left == right failed: left: 12, right: 22`.

- [ ] **Step 3: Implement**

`settings.rs` (lines 250-264), the whole constant:

```rust
/// Built-in theme names (ADR 0078, ADR 0101), in picker order: dark ones, then light ones.
/// The UI's `theme::THEMES` lists the same names (tested there).
pub const THEME_NAMES: &[&str] = &[
    "Mkrew Dark",
    "One Dark",
    "Dracula",
    "Catppuccin Mocha",
    "Tokyo Night",
    "Gruvbox Dark",
    "Nord",
    "Solarized Dark",
    "GitHub Dark",
    "Rosé Pine",
    "Kanagawa",
    "Everforest Dark",
    "Mkrew Light",
    "One Light",
    "GitHub Light",
    "Catppuccin Latte",
    "Solarized Light",
    "Gruvbox Light",
    "Rosé Pine Dawn",
    "Everforest Light",
    "Ayu Light",
    "Tokyo Night Day",
];
```

`theme.rs` `THEMES`: insert the six dark entries after the `Gruvbox Dark` entry (just before `ThemeEntry { name: "Mkrew Light", ..`), in this order:

```rust
    // Nord. Source: palette https://www.nordtheme.com/docs/colors-and-palettes (nord0 to
    // nord15); ANSI and the muted text (dim_foreground) from
    // https://github.com/nordtheme/alacritty/blob/main/src/nord.yaml; the flat nord0 surfaces
    // as in https://github.com/arcticicestudio/nord-visual-studio-code/blob/develop/themes/nord-color-theme.json
    dark_entry(
        "Nord",
        Palette {
            dark: true,
            bg: 0x2E3440,
            chrome: 0x2E3440,
            surface: 0x2E3440,
            fg: 0xD8DEE9,
            muted: 0xA5ABB6,
            accent: 0x88C0D0,
            red: 0xBF616A,
            green: 0xA3BE8C,
            yellow: 0xEBCB8B,
            blue: 0x81A1C1,
            magenta: 0xB48EAD,
            cyan: 0x88C0D0,
            ansi: [
                0x3B4252, 0xBF616A, 0xA3BE8C, 0xEBCB8B, 0x81A1C1, 0xB48EAD, 0x88C0D0, 0xE5E9F0,
                0x4C566A, 0xBF616A, 0xA3BE8C, 0xEBCB8B, 0x81A1C1, 0xB48EAD, 0x8FBCBB, 0xECEFF4,
            ],
        },
    ),
    // Solarized Dark. Source: https://ethanschoonover.com/solarized/ (base03 to base3 and the
    // eight accents) and the terminal mapping of
    // https://github.com/altercation/solarized/blob/master/xresources/solarized
    // Changed for the contrast rules (ADR 0101): fg 839496 (base0) -> EEE8D5 (base2), accent
    // 268BD2 -> 62ABDE; muted is base1.
    dark_entry(
        "Solarized Dark",
        Palette {
            dark: true,
            bg: 0x002B36,
            chrome: 0x002B36,
            surface: 0x002B36,
            fg: 0xEEE8D5,
            muted: 0x93A1A1,
            accent: 0x62ABDE,
            red: 0xDC322F,
            green: 0x859900,
            yellow: 0xB58900,
            blue: 0x268BD2,
            magenta: 0xD33682,
            cyan: 0x2AA198,
            ansi: [
                0x073642, 0xDC322F, 0x859900, 0xB58900, 0x268BD2, 0xD33682, 0x2AA198, 0xEEE8D5,
                0x002B36, 0xCB4B16, 0x586E75, 0x657B83, 0x839496, 0x6C71C4, 0x93A1A1, 0xFDF6E3,
            ],
        },
    ),
    // GitHub Dark (GitHub Dark Default). Source: https://github.com/primer/github-vscode-theme
    // (src/colors.js: fg e6edf3, muted 7d8590) with the primitives of
    // https://github.com/primer/primitives/blob/main/src/tokens/base/color/dark/dark.json5
    // (canvas 0d1117, inset 010409, overlay 151b23, blue 58a6ff); ANSI as exported in
    // https://gist.github.com/stef-k/aa85769be6b0a080a97cb2b623665137 (its terminal.ansi* keys).
    // Changed for the contrast rules (ADR 0101): accent 2F81F7 (accent.fg) -> 58A6FF.
    dark_entry(
        "GitHub Dark",
        Palette {
            dark: true,
            bg: 0x0D1117,
            chrome: 0x010409,
            surface: 0x151B23,
            fg: 0xE6EDF3,
            muted: 0x7D8590,
            accent: 0x58A6FF,
            red: 0xFF7B72,
            green: 0x3FB950,
            yellow: 0xD29922,
            blue: 0x58A6FF,
            magenta: 0xBC8CFF,
            cyan: 0x39C5CF,
            ansi: [
                0x484F58, 0xFF7B72, 0x3FB950, 0xD29922, 0x58A6FF, 0xBC8CFF, 0x39C5CF, 0xB1BAC4,
                0x6E7681, 0xFFA198, 0x56D364, 0xE3B341, 0x79C0FF, 0xD2A8FF, 0x56D4DD, 0xFFFFFF,
            ],
        },
    ),
    // Rosé Pine. Source: https://github.com/rose-pine/alacritty/blob/main/dist/rose-pine.toml
    // (base 191724, surface 1f1d2e, text e0def4, subtle 908caa, love, gold, rose, pine, foam,
    // iris and the ANSI table) and https://rosepinetheme.com/palette/
    dark_entry(
        "Rosé Pine",
        Palette {
            dark: true,
            bg: 0x191724,
            chrome: 0x191724,
            surface: 0x1F1D2E,
            fg: 0xE0DEF4,
            muted: 0x908CAA,
            accent: 0xC4A7E7,
            red: 0xEB6F92,
            green: 0x31748F,
            yellow: 0xF6C177,
            blue: 0x9CCFD8,
            magenta: 0xC4A7E7,
            cyan: 0xEBBCBA,
            ansi: [
                0x26233A, 0xEB6F92, 0x31748F, 0xF6C177, 0x9CCFD8, 0xC4A7E7, 0xEBBCBA, 0xE0DEF4,
                0x6E6A86, 0xEB6F92, 0x31748F, 0xF6C177, 0x9CCFD8, 0xC4A7E7, 0xEBBCBA, 0xE0DEF4,
            ],
        },
    ),
    // Kanagawa (wave). Source: https://github.com/rebelot/kanagawa.nvim/blob/master/lua/kanagawa/colors.lua
    // (palette) and .../lua/kanagawa/themes.lua (the wave `ui`, `syn` and `term` tables):
    // sumiInk3 bg, sumiInk0 chrome, sumiInk2 surface, fujiWhite fg, oldWhite muted, crystalBlue.
    dark_entry(
        "Kanagawa",
        Palette {
            dark: true,
            bg: 0x1F1F28,
            chrome: 0x16161D,
            surface: 0x1A1A22,
            fg: 0xDCD7BA,
            muted: 0xC8C093,
            accent: 0x7E9CD8,
            red: 0xE46876,
            green: 0x98BB6C,
            yellow: 0xE6C384,
            blue: 0x7E9CD8,
            magenta: 0x957FB8,
            cyan: 0x7AA89F,
            ansi: [
                0x16161D, 0xC34043, 0x76946A, 0xC0A36E, 0x7E9CD8, 0x957FB8, 0x6A9589, 0xC8C093,
                0x727169, 0xE82424, 0x98BB6C, 0xE6C384, 0x7FB4CA, 0x938AA9, 0x7AA89F, 0xDCD7BA,
            ],
        },
    ),
    // Everforest Dark (medium). Source: https://github.com/sainnhe/everforest/blob/master/palette.md
    // (bg0 2d353b, bg_dim 232a2e, grey1 859289 and the accents) and the terminal table of
    // https://github.com/sainnhe/everforest/blob/master/colors/everforest.vim
    // Changed for the contrast rules (ADR 0101): fg D3C6AA -> D5C9AE (the ANSI white stays D3C6AA).
    dark_entry(
        "Everforest Dark",
        Palette {
            dark: true,
            bg: 0x2D353B,
            chrome: 0x232A2E,
            surface: 0x232A2E,
            fg: 0xD5C9AE,
            muted: 0x859289,
            accent: 0xA7C080,
            red: 0xE67E80,
            green: 0xA7C080,
            yellow: 0xDBBC7F,
            blue: 0x7FBBB3,
            magenta: 0xD699B6,
            cyan: 0x83C092,
            ansi: [
                0x475258, 0xE67E80, 0xA7C080, 0xDBBC7F, 0x7FBBB3, 0xD699B6, 0x83C092, 0xD3C6AA,
                0x475258, 0xE67E80, 0xA7C080, 0xDBBC7F, 0x7FBBB3, 0xD699B6, 0x83C092, 0xD3C6AA,
            ],
        },
    ),
```

Insert the four light entries after the `Gruvbox Light` entry (just before the closing `];` of `THEMES`), in this order:

```rust
    // Rosé Pine Dawn. Source: https://github.com/rose-pine/alacritty/blob/main/dist/rose-pine-dawn.toml
    // (base faf4ed, surface fffaf3, overlay f2e9e1, text 575279, subtle 797593, pine 286983,
    // and the ANSI table) and https://rosepinetheme.com/palette/
    light_entry(
        "Rosé Pine Dawn",
        Palette {
            dark: false,
            bg: 0xFAF4ED,
            chrome: 0xF2E9E1,
            surface: 0xFFFAF3,
            fg: 0x575279,
            muted: 0x797593,
            accent: 0x286983,
            red: 0xB4637A,
            green: 0x286983,
            yellow: 0xEA9D34,
            blue: 0x56949F,
            magenta: 0x907AA9,
            cyan: 0xD7827E,
            ansi: [
                0xF2E9E1, 0xB4637A, 0x286983, 0xEA9D34, 0x56949F, 0x907AA9, 0xD7827E, 0x575279,
                0x9893A5, 0xB4637A, 0x286983, 0xEA9D34, 0x56949F, 0x907AA9, 0xD7827E, 0x575279,
            ],
        },
    ),
    // Everforest Light. Source: https://github.com/sainnhe/everforest/blob/master/palette.md
    // (light: bg0 fdf6e3, bg_dim efebd4, hard bg0 fffbef, grey2 829181 and the accents) and the
    // terminal table of .../colors/everforest.vim (black is fg, white is bg3 in the light scheme).
    // Changed for the contrast rules (ADR 0101): fg 5C6A72 -> 525F66, accent 3A94C5 -> 2A6F94,
    // surface F4F0D9 (bg1) -> FFFBEF (the hard-contrast bg0).
    light_entry(
        "Everforest Light",
        Palette {
            dark: false,
            bg: 0xFDF6E3,
            chrome: 0xEFEBD4,
            surface: 0xFFFBEF,
            fg: 0x525F66,
            muted: 0x829181,
            accent: 0x2A6F94,
            red: 0xF85552,
            green: 0x8DA101,
            yellow: 0xDFA000,
            blue: 0x3A94C5,
            magenta: 0xDF69BA,
            cyan: 0x35A77C,
            ansi: [
                0x5C6A72, 0xF85552, 0x8DA101, 0xDFA000, 0x3A94C5, 0xDF69BA, 0x35A77C, 0xE6E2CC,
                0x5C6A72, 0xF85552, 0x8DA101, 0xDFA000, 0x3A94C5, 0xDF69BA, 0x35A77C, 0xE6E2CC,
            ],
        },
    ),
    // Ayu Light. Source: https://github.com/ayu-theme/ayu-colors/blob/master/themes/light.yaml
    // (surface lift fcfcfc, base f8f9fa, over ffffff, accent f29718, palette) and the terminal
    // table of https://github.com/ayu-theme/vscode-ayu/blob/master/ayu-light.json; fg is the
    // editor text 020a12 at 64 % over fcfcfc (5C6166), muted the ui text 021a3c at 48 % over
    // f8f9fa (828E9F).
    // Changed for the contrast rules (ADR 0101): accent F29718 -> 975E0F.
    light_entry(
        "Ayu Light",
        Palette {
            dark: false,
            bg: 0xFCFCFC,
            chrome: 0xF8F9FA,
            surface: 0xFFFFFF,
            fg: 0x5C6166,
            muted: 0x828E9F,
            accent: 0x975E0F,
            red: 0xF07171,
            green: 0x86B300,
            yellow: 0xEBA400,
            blue: 0x22A4E6,
            magenta: 0xA37ACC,
            cyan: 0x4CBF99,
            ansi: [
                0x000000, 0xF06B6C, 0x6CBF43, 0xE7A100, 0x21A1E2, 0xA176CB, 0x4ABC96, 0xC7C7C7,
                0x686868, 0xF07171, 0x86B300, 0xEBA400, 0x22A4E6, 0xA37ACC, 0x4CBF99, 0xD1D1D1,
            ],
        },
    ),
    // Tokyo Night Day. Source: https://github.com/folke/tokyonight.nvim/blob/main/extras/alacritty/tokyonight_day.toml
    // (bg e1e2e7, fg 3760bf, fg_dark 6172b0, the ANSI table).
    // Changed for the contrast rules (ADR 0101): the Mkrew workspace colors need 3:1 on the
    // hover and selected fills, which a bg this dark cannot give, so bg and surface are
    // E1E2E7 lightened to F1F2F5 and the published bg is the chrome; fg 3760BF -> 2F52A2 and
    // accent 2E7DE9 -> 2461B5 (text and code need 4.5:1).
    light_entry(
        "Tokyo Night Day",
        Palette {
            dark: false,
            bg: 0xF1F2F5,
            chrome: 0xE1E2E7,
            surface: 0xF1F2F5,
            fg: 0x2F52A2,
            muted: 0x6172B0,
            accent: 0x2461B5,
            red: 0xF52A65,
            green: 0x587539,
            yellow: 0x8C6C3E,
            blue: 0x2E7DE9,
            magenta: 0x9854F1,
            cyan: 0x007197,
            ansi: [
                0xB4B5B9, 0xF52A65, 0x587539, 0x8C6C3E, 0x2E7DE9, 0x9854F1, 0x007197, 0x6172B0,
                0xA1A6C5, 0xFF4774, 0x5C8524, 0xA27629, 0x358AFF, 0xA463FF, 0x007EA8, 0x3760BF,
            ],
        },
    ),
```

Also update the doc comment above `THEMES` (line 1617) to `/// Every built-in theme, in [`mkrew_core::settings::THEME_NAMES`] order: dark ones, then light ones.`

- [ ] **Step 4: Run the tests**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-core settings::`
Expected: PASS.
Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib theme:: pressable palette::`
Expected: PASS: `there_are_22_built_in_themes_grouped_dark_then_light`, `the_theme_table_matches_the_settings_names`, `every_built_in_theme_meets_every_contrast_rule` (22 themes), `workspace_colors_are_distinct_and_every_name_has_one`, `derived_themes_keep_the_fixed_tokens`, `glass_is_derived_from_surface_and_accent_in_every_theme`, `every_hover_fill_differs_from_the_rest_fill_and_from_selected_in_every_theme`, `the_mkrew_themes_never_change`.
If `every_built_in_theme_meets_every_contrast_rule` names a theme (the f32 derivation can differ from the checking script by one color level), change only the field the comment above that entry names, by 2.5 % of the way to black (light themes) or white (dark themes), and write the new value in the ADR table.

- [ ] **Step 5: Gate and commit**

```bash
cargo fmt --all
CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-core -p mkrew-ui --all-targets -- -D warnings
git add crates/mkrew-core/src/settings.rs crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/palette/tests.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): ten more built-in themes (22 in all)"
```

---

### Task 7: Poll reload of the themes folder

**Files:**
- Create: `crates/mkrew-ui/src/shell/theme_gallery.rs`
- Modify: `crates/mkrew-ui/src/shell.rs` (add `mod theme_gallery;` after `mod task_dialog;`, line 65)
- Modify: `crates/mkrew-ui/src/main.rs` (import near line 122; fields after `settings_error`, line 523)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (poll at lines 137-145; `prompts_dir` line 52; struct literal near line 321)
- Modify: `crates/mkrew-ui/src/theme_catalog.rs` (one more test)

**Interfaces:**
- Consumes: `ThemeCatalog::load`, `ThemesStamp::read`, `Paths::themes_dir`, `theme::refresh`.
- Produces: `Shell::apply_themes_dir(&mut self, window: &mut Window, cx)`, fields `themes_dir: PathBuf`, `themes_stamp: ThemesStamp`.

- [ ] **Step 1: Write the failing test**

In `theme_catalog.rs` `mod tests` add (it pins what the poll relies on: every edit, add and removal is seen by the next `load`):

```rust
    #[test]
    fn the_catalog_follows_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("moon.toml");
        let mut palette = Palette::from_theme(&Theme::dark());
        std::fs::write(&path, theme_file::render("Moon", &palette)).unwrap();
        let stamp = ThemesStamp::read(dir.path());
        let first = ThemeCatalog::load(dir.path());
        assert_eq!(first.find("Moon").unwrap().theme.bg, palette.bg);

        palette.bg = 0x101820;
        std::fs::write(&path, theme_file::render("Moon", &palette)).unwrap();
        assert_ne!(ThemesStamp::read(dir.path()), stamp, "the poll sees the edit");
        let second = ThemeCatalog::load(dir.path());
        assert_eq!(second.find("Moon").unwrap().theme.bg, 0x101820);
        assert_ne!(first, second);

        std::fs::remove_file(&path).unwrap();
        assert!(ThemeCatalog::load(dir.path()).find("Moon").is_none());
    }
```

- [ ] **Step 2: Run it**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib theme_catalog::tests::the_catalog_follows_the_folder`
Expected: PASS already (it pins Task 4's behavior for the poll); it is the safety net for the wiring below.

- [ ] **Step 3: Implement**

`main.rs`:
- Change the import added in Task 5 to `use mkrew_ui::theme_catalog::{ThemeCatalog, ThemesStamp};`.
- After the `settings_error` field (line 523) add:
  ```rust
      /// `<config>/mkrew/themes`: theme files (ADR 0101), polled with `settings.toml`.
      themes_dir: PathBuf,
      /// Their names, times and sizes at the last poll; a difference rebuilds the catalog.
      themes_stamp: ThemesStamp,
  ```

`shell.rs`: add `mod theme_gallery;` between `mod task_dialog;` and `pub(crate) mod thread_panel;`.

`connection.rs`:
- After line 52 `let prompts_dir = paths.prompts_dir();` add `let themes_dir = paths.themes_dir();`.
- In the struct literal after `settings_modified: None,` (line 321) add:
  ```rust
              themes_stamp: ThemesStamp::read(&themes_dir),
              themes_dir,
  ```
- Poll (lines 137-145): replace
  ```rust
                      // ponytail: mtime poll on the 500 ms tick; a file watcher if it ever shows
                      // in a profile.
  ```
  with
  ```rust
                      // ponytail: mtime poll on the 500 ms tick; a file watcher if it ever shows
                      // in a profile. The themes folder rides the same tick (the names, times
                      // and sizes of its `*.toml` files): a folder with hundreds of files costs
                      // one stat each per tick; the upgrade path is `notify`.
  ```
  and after the `if modified != shell.settings_modified { ... }` block add:
  ```rust
                      let themes = ThemesStamp::read(&shell.themes_dir);
                      if themes != shell.themes_stamp {
                          shell.themes_stamp = themes;
                          shell.apply_themes_dir(window, cx);
                      }
  ```

Create `shell/theme_gallery.rs`:

```rust
//! The theme catalog's reload (ADR 0101), the gallery's preview and its drawing on
//! Settings → Appearance. The model is `mkrew_ui::settings_dialog`; the catalog is
//! `mkrew_ui::theme_catalog`.

use crate::*;

impl Shell {
    /// `<config>/themes/` changed: the catalog is read again and the theme shown is rebuilt
    /// from it, so a kept theme whose file was edited shows its new colors at once and one
    /// whose file went falls back like an unknown name. The theme picker lists the old
    /// catalog, so it closes (restoring its preview) rather than pick a shifted index.
    pub(crate) fn apply_themes_dir(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let catalog = ThemeCatalog::load(&self.themes_dir);
        if let Some(dialog) = &mut self.settings_dialog {
            dialog.set_catalog(catalog.clone());
        }
        cx.set_global(catalog);
        if self.theme_pick.is_some() {
            self.close_palette(window, cx);
        }
        let mode = cx.theme().mode;
        theme::refresh(mode, window, cx);
        cx.refresh_windows();
        cx.notify();
    }
}
```

- [ ] **Step 4: Run the checks**

Run: `CARGO_BUILD_JOBS=4 cargo check -p mkrew-ui --all-targets`
Expected: no errors and no `dead_code` warning (`themes_dir`, `themes_stamp` and `apply_themes_dir` are all read).
Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib theme_catalog`
Expected: 11 tests PASS.

- [ ] **Step 5: Gate and commit**

```bash
cargo fmt --all
CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-core -p mkrew-ui --all-targets -- -D warnings
git add crates/mkrew-ui/src/shell/theme_gallery.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/theme_catalog.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): the settings poll also reloads the themes folder"
```

---

### Task 8: Gallery model in `settings_dialog`

**Files:**
- Modify: `crates/mkrew-ui/src/settings_dialog.rs` (imports 9 and the theme import; `Focus` 118-129; `Effect` 139-157; struct 272-296; `new` 299-315; `set_catalog`; `move_focus` 449-463; `set_tab` 465-478; `activate` 481-526; `step_focus` 583-609; `chain_edge` 611-633; `key` 689-829; tests)
- Modify: `crates/mkrew-ui/src/shell/settings.rs` (`apply_settings_effect` match, lines 128-182)

**Interfaces:**
- Consumes: `ThemeCatalog`, `CatalogTheme`, `Origin`.
- Produces (all on `SettingsDialog` unless noted):
  - `Focus::Gallery(usize)`; `Effect::KeepTheme { name: SharedString, dark: bool }`
  - `Badge { DarkMode, LightMode, File, LowContrast }` with `label() -> &'static str`
  - `gallery_shown() -> bool`, `gallery_cards() -> Vec<&CatalogTheme>`, `focused_card() -> Option<&CatalogTheme>`, `kept_names() -> (String, String)` (dark, light), `card_badges(&CatalogTheme) -> Vec<Badge>`
  - `gallery_hover(&mut self, name: &str) -> u64`, `gallery_hover_due(&mut self, serial: u64)`, `gallery_leave(&mut self)`, `end_preview(&mut self)`, `previewing() -> Option<&SharedString>`
  - `gallery_click(&mut self, name: &str, keymap: &Keymap) -> Effect`
  - `set_catalog` now also ends a vanished preview and clamps the keyboard.

Behavior in words: the pointer resting on a card for the delay (the shell waits `THEME_PREVIEW_DELAY`, then calls `gallery_hover_due` with the serial it got) sets `previewing`; a newer hover makes the older serial stale; leaving the gallery clears it; the keyboard on a card previews it at once; Enter, Space or a click returns `KeepTheme` for the card's own slot and clears the preview; Esc clears the preview and then does what Esc does today; arrows walk the cards in reading order across both groups.

- [ ] **Step 1: Write the failing tests**

In `settings_dialog.rs` `mod tests`, first make `focus_on` skip the gallery stop (replace its two `tab` presses, lines 1120-1121):

```rust
        d.key(&key("tab"), None, keymap);
        d.key(&key("tab"), None, keymap);
        if matches!(d.focus, Focus::Gallery(_)) {
            d.key(&key("tab"), None, keymap);
        }
```

Then add (the `file_theme` helper and imports come from Task 5):

```rust
    /// Appearance with the built-ins, a dark file theme "Aurora", a light one "Dawn" and an
    /// error card.
    fn gallery_dialog() -> SettingsDialog {
        let mut d = dialog();
        d.set_catalog(ThemeCatalog::from_files(vec![
            file_theme("Aurora", true),
            file_theme("Dawn", false),
            ("bad.toml".to_owned(), Ok("oops".to_owned())),
        ]));
        d.set_tab(Tab::Settings(Page::Appearance));
        d
    }

    fn card_names(d: &SettingsDialog) -> Vec<String> {
        (d.gallery_cards().iter())
            .map(|card| card.name.to_string())
            .collect()
    }

    fn previewing(d: &SettingsDialog) -> Option<String> {
        d.previewing().map(ToString::to_string)
    }

    /// Tab, Tab: the page list, the search box, then the gallery.
    fn to_gallery(d: &mut SettingsDialog, keymap: &Keymap) {
        d.key(&key("tab"), None, keymap);
        d.key(&key("tab"), None, keymap);
        assert_eq!(d.focus, Focus::Gallery(0));
    }

    #[test]
    fn the_gallery_lists_dark_then_light_without_the_error_card() {
        let d = gallery_dialog();
        let names = card_names(&d);
        let dark = crate::theme::THEMES.iter().filter(|e| e.dark).count();
        assert_eq!(names[0], "Mkrew Dark");
        assert_eq!(names[dark - 1], "Everforest Dark", "built-in darks in table order");
        assert_eq!(names[dark], "Aurora", "then the dark files");
        assert_eq!(names[dark + 1], "Mkrew Light", "then the light group");
        assert_eq!(names.last().map(String::as_str), Some("Dawn"));
        assert!(!names.iter().any(|name| name == "bad"));
        assert_eq!(names.len(), crate::theme::THEMES.len() + 2);
    }

    #[test]
    fn the_gallery_shows_on_appearance_with_no_query_only() {
        let mut d = gallery_dialog();
        assert!(d.gallery_shown());
        d.find = Input::from("theme");
        assert!(!d.gallery_shown());
        d.find = Input::default();
        d.set_tab(Tab::Settings(Page::General));
        assert!(!d.gallery_shown());
    }

    #[test]
    fn a_hover_previews_after_the_delay_and_a_newer_hover_wins() {
        let mut d = gallery_dialog();
        let first = d.gallery_hover("Dracula");
        assert_eq!(previewing(&d), None, "nothing before the delay");
        let second = d.gallery_hover("One Dark");
        d.gallery_hover_due(first);
        assert_eq!(previewing(&d), None, "the first timer is stale");
        d.gallery_hover_due(second);
        assert_eq!(previewing(&d).as_deref(), Some("One Dark"));
    }

    #[test]
    fn leaving_the_gallery_restores_and_a_late_timer_does_nothing() {
        let mut d = gallery_dialog();
        let serial = d.gallery_hover("Dracula");
        d.gallery_hover_due(serial);
        assert!(d.previewing().is_some());
        d.gallery_leave();
        assert_eq!(previewing(&d), None);
        d.gallery_hover_due(serial);
        assert_eq!(previewing(&d), None, "the pointer is gone");
        assert_eq!(d.gallery_hover("No such theme"), serial + 2);
        d.gallery_hover_due(serial + 2);
        assert_eq!(previewing(&d), None, "an unknown card never previews");
    }

    #[test]
    fn a_click_keeps_the_theme_in_its_own_slot_and_ends_the_preview() {
        let keymap = Keymap::defaults();
        let mut d = gallery_dialog();
        let serial = d.gallery_hover("Dawn");
        d.gallery_hover_due(serial);
        assert_eq!(
            d.gallery_click("Dawn", &keymap),
            Effect::KeepTheme {
                name: "Dawn".into(),
                dark: false
            }
        );
        assert_eq!(previewing(&d), None);
        let at = card_names(&d).iter().position(|n| n == "Aurora").unwrap();
        assert_eq!(
            d.gallery_click("Aurora", &keymap),
            Effect::KeepTheme {
                name: "Aurora".into(),
                dark: true
            }
        );
        assert_eq!(d.focus, Focus::Gallery(at), "the keyboard follows the click");
    }

    #[test]
    fn an_error_card_or_an_unknown_name_is_not_clickable() {
        let keymap = Keymap::defaults();
        let mut d = gallery_dialog();
        let focus = d.focus;
        assert_eq!(d.gallery_click("bad", &keymap), Effect::None);
        assert_eq!(d.gallery_click("bad.toml", &keymap), Effect::None);
        assert_eq!(d.focus, focus);
    }

    #[test]
    fn the_keyboard_previews_a_card_and_enter_keeps_it() {
        let keymap = Keymap::defaults();
        let mut d = gallery_dialog();
        to_gallery(&mut d, &keymap);
        assert_eq!(previewing(&d).as_deref(), Some("Mkrew Dark"), "no delay");
        d.key(&key("right"), None, &keymap);
        assert_eq!(previewing(&d).as_deref(), Some("One Dark"));
        assert_eq!(
            d.key(&key("enter"), None, &keymap),
            Effect::KeepTheme {
                name: "One Dark".into(),
                dark: true
            }
        );
        assert_eq!(previewing(&d), None);
        d.key(&key("right"), None, &keymap);
        assert_eq!(
            d.key(&typed(" "), None, &keymap),
            Effect::KeepTheme {
                name: "Dracula".into(),
                dark: true
            },
            "Space acts like Enter"
        );
    }

    #[test]
    fn esc_restores_the_preview_and_then_closes_as_today() {
        let keymap = Keymap::defaults();
        let mut d = gallery_dialog();
        let serial = d.gallery_hover("Dracula");
        d.gallery_hover_due(serial);
        assert_eq!(d.key(&key("escape"), None, &keymap), Effect::Close);
        assert_eq!(previewing(&d), None);
    }

    #[test]
    fn arrows_walk_the_cards_in_reading_order_across_both_groups() {
        let keymap = Keymap::defaults();
        let mut d = gallery_dialog();
        let names = card_names(&d);
        to_gallery(&mut d, &keymap);
        let mut seen = vec![previewing(&d).unwrap()];
        for _ in 1..names.len() {
            d.key(&key("right"), None, &keymap);
            seen.push(previewing(&d).unwrap());
        }
        assert_eq!(seen, names, "dark cards, then light ones; the error card is skipped");
        let last = names.len() - 1;
        d.key(&key("right"), None, &keymap);
        assert_eq!(d.focus, Focus::Gallery(last), "no wrap");
        d.key(&key("left"), None, &keymap);
        assert_eq!(d.focus, Focus::Gallery(last - 1));
        d.key(&key("down"), None, &keymap);
        d.key(&key("down"), None, &keymap);
        assert_eq!(d.focus, Focus::Row(0), "Down from the last card enters the rows");
        assert_eq!(previewing(&d), None, "the preview ends with the focus");
        d.key(&key("tab"), None, &keymap);
        d.key(&key("tab"), None, &keymap);
        d.focus = Focus::Gallery(0);
        d.key(&key("up"), None, &keymap);
        assert_eq!(d.focus, Focus::Find, "Up from the first card goes back");
    }

    #[test]
    fn tab_reaches_the_gallery_once_and_returns_to_the_same_card() {
        let keymap = Keymap::defaults();
        let mut d = gallery_dialog();
        to_gallery(&mut d, &keymap);
        d.key(&key("right"), None, &keymap);
        d.key(&key("tab"), None, &keymap);
        assert_eq!(d.focus, Focus::Row(0), "one stop for the whole gallery");
        d.key(&key("shift-tab"), None, &keymap);
        assert_eq!(d.focus, Focus::Gallery(1), "back on the card it left");
    }

    #[test]
    fn a_broken_settings_file_lets_cards_preview_but_never_writes() {
        let keymap = Keymap::defaults();
        let mut d = SettingsDialog::new(Err("settings.toml:2:1: oops".into()), Vec::new());
        d.set_tab(Tab::Settings(Page::Appearance));
        let serial = d.gallery_hover("Dracula");
        d.gallery_hover_due(serial);
        assert_eq!(previewing(&d).as_deref(), Some("Dracula"));
        assert_eq!(d.gallery_click("Dracula", &keymap), Effect::None);
        assert_eq!(previewing(&d), None);
    }

    #[test]
    fn a_new_catalog_ends_the_preview_of_a_card_that_is_gone() {
        let mut d = gallery_dialog();
        let serial = d.gallery_hover("Aurora");
        d.gallery_hover_due(serial);
        assert!(d.previewing().is_some());
        d.set_catalog(ThemeCatalog::builtin());
        assert_eq!(previewing(&d), None);
        let mut d = gallery_dialog();
        let serial = d.gallery_hover("Dracula");
        d.gallery_hover_due(serial);
        d.set_catalog(ThemeCatalog::builtin());
        assert_eq!(previewing(&d).as_deref(), Some("Dracula"), "a card that stays keeps its preview");
    }

    #[test]
    fn the_keyboard_moves_to_a_card_that_exists_when_the_list_shrinks() {
        let keymap = Keymap::defaults();
        let mut d = gallery_dialog();
        to_gallery(&mut d, &keymap);
        for _ in 0..card_names(&d).len() {
            d.key(&key("right"), None, &keymap);
        }
        let built_in = crate::theme::THEMES.len();
        d.set_catalog(ThemeCatalog::builtin());
        assert_eq!(d.focus, Focus::Gallery(built_in - 1));
        d.set_catalog(ThemeCatalog::from_files(vec![file_theme("Aurora", true)]));
        d.set_tab(Tab::Settings(Page::General));
        d.set_catalog(ThemeCatalog::builtin());
        assert_eq!(d.focus, Focus::Tabs, "off the page the focus is the page list");
    }

    #[test]
    fn badges_name_the_slots_the_origin_and_low_contrast() {
        let mut d = dialog();
        let mut dim = Palette::from_theme(&Theme::dark());
        dim.fg = dim.bg;
        d.set_catalog(ThemeCatalog::from_files(vec![(
            "dim.toml".into(),
            Ok(crate::theme_file::render("Dim", &dim)),
        )]));
        let badges = |d: &SettingsDialog, name: &str| d.card_badges(d.catalog.find(name).unwrap());
        assert_eq!(badges(&d, "Mkrew Dark"), [Badge::DarkMode]);
        assert_eq!(badges(&d, "Mkrew Light"), [Badge::LightMode]);
        assert_eq!(badges(&d, "Dracula"), []);
        assert_eq!(badges(&d, "Dim"), [Badge::File, Badge::LowContrast]);
        let mut settings = Settings::default();
        settings.ui.dark_theme = "Dim".into();
        d.reload(Ok(settings));
        assert_eq!(
            badges(&d, "Dim"),
            [Badge::DarkMode, Badge::File, Badge::LowContrast]
        );
        assert_eq!(badges(&d, "Mkrew Dark"), []);
        assert_eq!(Badge::LowContrast.label(), "Low contrast");
        assert_eq!(d.kept_names(), ("Dim".to_owned(), "Mkrew Light".to_owned()));
    }
```

- [ ] **Step 2: Run to see failures**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib settings_dialog`
Expected: compile errors (`Focus::Gallery`, `gallery_hover`, `Badge`, `Effect::KeepTheme` do not exist).

- [ ] **Step 3: Implement**

Imports: line 9 becomes `use gpui::{Keystroke, SharedString};`; the Task 5 theme import becomes `use crate::theme_catalog::{CatalogTheme, Origin, ThemeCatalog};`.

`Focus` (after `Prompts(Spot),`):
```rust
    /// A card of the theme gallery (Appearance); the whole gallery is one Tab stop and the
    /// arrow keys move between its cards.
    Gallery(usize),
```

`Effect` (after `Prompts(PromptAction),`):
```rust
    /// A gallery card was kept: the shell saves `name` to the slot of its own appearance
    /// (`dark_theme` or `light_theme`) and shows the kept theme of the slot shown.
    KeepTheme { name: SharedString, dark: bool },
```

Before `pub enum Effect` add:
```rust
/// What a gallery card says besides its name (spec 3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Badge {
    /// The kept theme of the dark slot.
    DarkMode,
    /// The kept theme of the light slot.
    LightMode,
    /// A theme file.
    File,
    /// A theme file that breaks a contrast rule.
    LowContrast,
}

impl Badge {
    pub fn label(self) -> &'static str {
        match self {
            Badge::DarkMode => "Dark mode",
            Badge::LightMode => "Light mode",
            Badge::File => "File",
            Badge::LowContrast => "Low contrast",
        }
    }
}
```

`SettingsDialog` fields (after `pub catalog: ThemeCatalog,`):
```rust
    /// The gallery card the keyboard was last on (its Tab stop returns there).
    gallery_at: usize,
    /// The card under the pointer, and the counter that tells a late timer its hover is
    /// stale.
    hovered: Option<SharedString>,
    hover_serial: u64,
    /// The theme the whole app shows now instead of the kept one (`None`: the kept one): set
    /// by a hover that lasted the preview delay or by the keyboard on a card. The shell draws
    /// it; nothing is written.
    previewing: Option<SharedString>,
```
and in `new()` after `catalog: ThemeCatalog::builtin(),`: `gallery_at: 0, hovered: None, hover_serial: 0, previewing: None,`.

Replace `set_catalog` (from Task 5):
```rust
    /// A rebuilt catalog (the themes folder changed): a hover or a preview of a card that is
    /// gone ends, and the keyboard stays on a card that exists.
    pub fn set_catalog(&mut self, catalog: ThemeCatalog) {
        self.catalog = catalog;
        let cards = self.gallery_cards().len();
        self.gallery_at = self.gallery_at.min(cards.saturating_sub(1));
        if let Focus::Gallery(at) = self.focus {
            self.focus = if cards == 0 {
                Focus::Find
            } else {
                Focus::Gallery(at.min(cards - 1))
            };
        }
        let gone = |name: &Option<SharedString>| {
            name.as_ref()
                .is_some_and(|name| self.catalog.find(name).is_none())
        };
        if gone(&self.hovered) {
            self.hovered = None;
        }
        if gone(&self.previewing) {
            self.previewing = None;
        }
    }
```
(If the borrow checker rejects the closure borrowing `self.catalog` while assigning, compute `let (hover_gone, preview_gone) = (gone(&self.hovered), gone(&self.previewing));` first and assign after.)

Gallery methods, in a new `impl SettingsDialog` block after `read_only`/`set_catalog`:

```rust
    /// The Appearance page with no query draws the gallery above its cards.
    pub fn gallery_shown(&self) -> bool {
        self.tab == Tab::Settings(Page::Appearance) && !self.searching()
    }

    /// The cards the keyboard visits, in reading order: the dark group, then the light group
    /// (an error card is not one of them).
    pub fn gallery_cards(&self) -> Vec<&CatalogTheme> {
        (self.catalog.group(true))
            .chain(self.catalog.group(false))
            .collect()
    }

    /// The card the keyboard is on.
    pub fn focused_card(&self) -> Option<&CatalogTheme> {
        let Focus::Gallery(at) = self.focus else {
            return None;
        };
        self.gallery_cards().get(at).copied()
    }

    /// The names `[ui] dark_theme` and `light_theme` hold now (the defaults while the file does
    /// not parse).
    pub fn kept_names(&self) -> (String, String) {
        let defaults;
        let settings = match &self.settings {
            Ok(settings) => settings,
            Err(_) => {
                defaults = Settings::default();
                &defaults
            }
        };
        (
            settings.ui.dark_theme.clone(),
            settings.ui.light_theme.clone(),
        )
    }

    /// The badges of `card`: its slots first, then its origin and its contrast.
    pub fn card_badges(&self, card: &CatalogTheme) -> Vec<Badge> {
        let (dark, light) = self.kept_names();
        let mut badges = Vec::new();
        if *card.name == *dark {
            badges.push(Badge::DarkMode);
        }
        if *card.name == *light {
            badges.push(Badge::LightMode);
        }
        if matches!(card.origin, Origin::File(_)) {
            badges.push(Badge::File);
        }
        if !card.problems.is_empty() {
            badges.push(Badge::LowContrast);
        }
        badges
    }

    /// The theme the app should show now instead of the kept one.
    pub fn previewing(&self) -> Option<&SharedString> {
        self.previewing.as_ref()
    }

    /// The pointer entered card `name`; the shell waits the preview delay and then calls
    /// [`Self::gallery_hover_due`] with the returned serial. An unknown name (an error card)
    /// is never previewed.
    pub fn gallery_hover(&mut self, name: &str) -> u64 {
        self.hover_serial += 1;
        self.hovered = self.catalog.find(name).map(|theme| theme.name.clone());
        self.hover_serial
    }

    /// The delay of hover `serial` is over: it previews when it is still the latest hover.
    pub fn gallery_hover_due(&mut self, serial: u64) {
        if serial == self.hover_serial && self.hovered.is_some() {
            self.previewing.clone_from(&self.hovered);
        }
    }

    /// The pointer left the gallery: the kept theme comes back.
    pub fn gallery_leave(&mut self) {
        self.end_preview();
    }

    /// No hover, no preview (the pointer left, Esc, a page change, Settings closing).
    pub fn end_preview(&mut self) {
        self.hover_serial += 1;
        self.hovered = None;
        self.previewing = None;
    }

    /// A click on card `name`: the keyboard moves to it and the theme is kept for its slot.
    /// An unknown name (an error card, a card that just went) does nothing.
    pub fn gallery_click(&mut self, name: &str, keymap: &Keymap) -> Effect {
        let Some(at) = (self.gallery_cards().iter()).position(|card| &*card.name == name) else {
            return Effect::None;
        };
        let left = self.move_focus(Focus::Gallery(at), keymap);
        if left != Effect::None {
            return left;
        }
        self.keep_card(at)
    }

    /// Enter, Space or a click on card `at`.
    fn keep_card(&mut self, at: usize) -> Effect {
        let Some((name, dark)) = (self.gallery_cards().get(at)).map(|c| (c.name.clone(), c.dark))
        else {
            return Effect::None;
        };
        self.end_preview();
        if self.read_only() {
            return Effect::None;
        }
        Effect::KeepTheme { name, dark }
    }

    /// What the keyboard previews: the card it is on.
    fn focus_preview(&self) -> Option<SharedString> {
        self.focused_card().map(|card| card.name.clone())
    }

    /// Where an arrow key takes the keyboard from card `at`: Left / Up and Right / Down step
    /// through the cards in reading order, across the two groups; Up from the first card goes
    /// to the search box and Down from the last one to the first row.
    fn gallery_move(&self, at: usize, motion: Motion, keymap: &Keymap) -> Option<Focus> {
        let last = self.gallery_cards().len().checked_sub(1)?;
        match motion {
            Motion::Left => Some(Focus::Gallery(at.saturating_sub(1))),
            Motion::Right => Some(Focus::Gallery((at + 1).min(last))),
            Motion::Up if at == 0 => Some(Focus::Find),
            Motion::Up => Some(Focus::Gallery(at - 1)),
            Motion::Down if at >= last => (!self.rows(keymap).is_empty())
                .then_some(Focus::Row(0))
                .or(Some(Focus::Gallery(last))),
            Motion::Down => Some(Focus::Gallery(at + 1)),
            _ => None,
        }
    }

    /// Where Down from the search box goes: the gallery when it is drawn, else the first row.
    fn after_find(&self, keymap: &Keymap) -> Option<Focus> {
        if self.gallery_shown() && !self.gallery_cards().is_empty() {
            return Some(Focus::Gallery(self.gallery_at));
        }
        (!self.rows(keymap).is_empty()).then_some(Focus::Row(0))
    }
```

`move_focus`: before the final `effect` line add
```rust
        if let Focus::Gallery(at) = focus {
            self.gallery_at = at;
        }
        // The keyboard on a card previews it (no delay); anywhere else ends that preview.
        self.previewing = self.focus_preview();
```

`set_tab`: after `self.list_pick = None;` add `self.end_preview();`.

`activate`: first lines of the function:
```rust
        if let Focus::Gallery(at) = self.focus {
            return self.keep_card(at);
        }
```

`step_focus`: after `let mut order = vec![Focus::Tabs, Focus::Find];` add
```rust
        if self.gallery_shown() && !self.gallery_cards().is_empty() {
            order.push(Focus::Gallery(self.gallery_at));
        }
```

`chain_edge`: `Focus::Tabs => false,` becomes `Focus::Tabs | Focus::Gallery(_) => false,`.

`key`: after the `if self.tab == Tab::Prompts && let Focus::Prompts(spot) = self.focus && ... { return effect; }` block and before `match key {` add:
```rust
        if let Focus::Gallery(at) = self.focus
            && let DialogKey::Move {
                motion,
                extend: false,
            } = &key
            && let Some(next) = self.gallery_move(at, *motion, keymap)
        {
            return self.move_focus(next, keymap);
        }
```
In `DialogKey::Cancel => {` add `self.end_preview();` as the first statement.
In the Up/Down inner `match self.focus`, replace
```rust
                    Focus::Find if !up && !self.rows(keymap).is_empty() => {
                        self.move_focus(Focus::Row(0), keymap)
                    }
                    Focus::Find | Focus::Search | Focus::Add(_) | Focus::Prompts(_) => Effect::None,
```
with
```rust
                    Focus::Find if !up => match self.after_find(keymap) {
                        Some(focus) => self.move_focus(focus, keymap),
                        None => Effect::None,
                    },
                    Focus::Find
                    | Focus::Search
                    | Focus::Gallery(_)
                    | Focus::Add(_)
                    | Focus::Prompts(_) => Effect::None,
```

`shell/settings.rs` `apply_settings_effect`, add an arm before `Effect::AddMachine`:
```rust
            Effect::KeepTheme { name, dark } => self.keep_theme(&name, dark, window, cx),
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib settings_dialog`
Expected: PASS, the 14 new gallery tests and every existing settings dialog test (`focus_on` now skips the gallery stop).
Run: `CARGO_BUILD_JOBS=4 cargo check -p mkrew-ui --all-targets`
Expected: no errors (the match on `Effect` in the shell is exhaustive again).

- [ ] **Step 5: Gate and commit**

```bash
cargo fmt --all
CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-core -p mkrew-ui --all-targets -- -D warnings
git add crates/mkrew-ui/src/settings_dialog.rs crates/mkrew-ui/src/shell/settings.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): theme gallery model (cards, hover, keyboard, badges)"
```

---

### Task 9: Gallery drawing and preview sync

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (tokens after `SETTINGS_PROMPT_BODY_HEIGHT`, line 324)
- Modify: `crates/mkrew-ui/src/shell/theme_gallery.rs` (preview sync, hover handlers, drawing)
- Modify: `crates/mkrew-ui/src/main.rs` (field after `theme_pick`, line 853)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (init near line 480)
- Modify: `crates/mkrew-ui/src/shell/navigation.rs` (`restore_theme_pick` 553-558; `keep_theme`)
- Modify: `crates/mkrew-ui/src/shell/settings.rs` (`close_settings_dialog`, key scroll, `apply_settings_effect` tail, body children)

**Interfaces:**
- Consumes: Task 8 model; `card_press`, `overline`, `meta_line`, `tooltip`, `Pressable::on_press`.
- Produces: `Shell::sync_theme_preview`, `gallery_hover`, `gallery_hover_due`, `gallery_leave`, `render_theme_gallery`, field `gallery_preview: Option<(ThemeMode, SharedString)>`; tokens `THEME_CARD_WIDTH`, `THEME_PICTURE_*`, `THEME_BAR_*`, `THEME_PILL_*`, `THEME_DOT`, `THEME_PREVIEW_DELAY`.

The pure behavior is tested in Task 8 (hover delay, keyboard, keep, Esc, badges); this task is drawing and wiring only, so its check is a build plus the visual items of Task 11.

- [ ] **Step 1: Add the tokens**

`theme.rs`, after `SETTINGS_PROMPT_BODY_HEIGHT` (line 324):

```rust
/// The theme gallery of Settings → Appearance (ADR 0101): a card is this wide (three fit a
/// row at the default Settings width), the picture of a window in the card's own colors is
/// this tall, and its parts: the top strip, the left strip, the two text bars, the accent
/// pill and the six ANSI dots.
pub const THEME_CARD_WIDTH: f32 = 200.0;
pub const THEME_PICTURE_HEIGHT: f32 = 96.0;
pub const THEME_PICTURE_CHROME: f32 = 12.0;
pub const THEME_PICTURE_SIDEBAR: f32 = 36.0;
pub const THEME_BAR_HEIGHT: f32 = 6.0;
pub const THEME_BAR_LONG: f32 = 72.0;
pub const THEME_BAR_SHORT: f32 = 48.0;
pub const THEME_PILL_HEIGHT: f32 = 10.0;
pub const THEME_PILL_WIDTH: f32 = 28.0;
pub const THEME_DOT: f32 = 8.0;
/// How long the pointer rests on a gallery card before the app shows that theme, so moving
/// across the cards does not flash.
pub const THEME_PREVIEW_DELAY: Duration = Duration::from_millis(120);
```

- [ ] **Step 2: Preview state and restore**

`main.rs`, after the `theme_pick` field (line 853):
```rust
    /// The theme shown for a gallery card (mode it had, name), while one is previewed.
    gallery_preview: Option<(mkrew_core::settings::ThemeMode, SharedString)>,
```
`connection.rs`, after `theme_pick: None,` (line 480): `gallery_preview: None,`.

`navigation.rs`: replace `restore_theme_pick` (lines 551-558) with:
```rust
    /// Back to the current theme for the mode the picker or a gallery preview started on (Esc,
    /// or anything replacing it); rebuilt, so an OS appearance or settings change meanwhile
    /// is kept.
    pub(crate) fn restore_theme_pick(&mut self, window: &Window, cx: &mut Context<'_, Self>) {
        let picked = self.theme_pick.take();
        let previewed = self.gallery_preview.take();
        if let Some(dialog) = self.settings_dialog.as_mut() {
            dialog.end_preview();
        }
        if let Some(mode) = picked.or(previewed.map(|(mode, _)| mode)) {
            theme::refresh(mode, window, cx);
            cx.refresh_windows();
        }
    }
```
and in `keep_theme` add `self.gallery_preview = None;` right before `theme::refresh(mode, window, cx);`.
`theme_gallery.rs` `apply_themes_dir`: add `self.gallery_preview = None;` before `theme::refresh(...)`, and `self.sync_theme_preview(window, cx);` after `cx.refresh_windows();` (a preview the dialog still wants is shown again from the new catalog).

- [ ] **Step 3: Drawing and hover handlers**

Replace the imports of `shell/theme_gallery.rs` and add (below `apply_themes_dir`, same `impl Shell` block or a second one):

```rust
use crate::*;
use mkrew_ui::settings_dialog::{Badge, Focus};
use mkrew_ui::theme_catalog::{CatalogTheme, FileError};
```

```rust
impl Shell {
    /// Makes the theme shown follow what the dialog wants previewed (`previewing`): the
    /// card's theme in the mode the app has, or the kept theme again. Nothing is written.
    pub(crate) fn sync_theme_preview(&mut self, window: &Window, cx: &mut Context<'_, Self>) {
        let wanted = (self.settings_dialog.as_ref()).and_then(|d| d.previewing().cloned());
        if wanted.as_ref() == self.gallery_preview.as_ref().map(|(_, name)| name) {
            return;
        }
        let Some(name) = wanted else {
            if let Some((mode, _)) = self.gallery_preview.take() {
                theme::refresh(mode, window, cx);
                cx.refresh_windows();
            }
            return;
        };
        let mode = (self.gallery_preview.as_ref()).map_or_else(|| cx.theme().mode, |(m, _)| *m);
        let fonts = cx.global::<theme::Fonts>().clone();
        let Some(listed) = cx.global::<ThemeCatalog>().find(&name) else {
            return;
        };
        let shown = Theme {
            mode,
            ..listed.theme.clone()
        };
        cx.set_global(shown.with_fonts(&fonts));
        self.gallery_preview = Some((mode, name));
        cx.refresh_windows();
    }

    /// The pointer entered card `name`: after the preview delay it shows that theme.
    pub(crate) fn gallery_hover(
        &mut self,
        name: SharedString,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(dialog) = self.settings_dialog.as_mut() else {
            return;
        };
        let serial = dialog.gallery_hover(&name);
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(theme::THEME_PREVIEW_DELAY)
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Some(dialog) = shell.settings_dialog.as_mut() {
                    dialog.gallery_hover_due(serial);
                }
                shell.sync_theme_preview(window, cx);
            });
        })
        .detach();
    }

    /// The pointer left the gallery: the kept theme comes back.
    pub(crate) fn gallery_leave(&mut self, window: &Window, cx: &mut Context<'_, Self>) {
        if let Some(dialog) = self.settings_dialog.as_mut() {
            dialog.gallery_leave();
        }
        self.sync_theme_preview(window, cx);
    }

    /// The gallery: the "Themes" overline, the Dark and Light groups of cards, one card per
    /// file that is not loaded, and the folder line (spec 3.1).
    pub(crate) fn render_theme_gallery(
        &self,
        dialog: &SettingsDialog,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let mut cards = div()
            .id("theme-gallery")
            .flex()
            .flex_col()
            .gap(px(space::M))
            // Leaving the whole gallery (not one card) puts the kept theme back.
            .on_hover(cx.listener(|shell, hovered: &bool, window, cx| {
                if !*hovered {
                    shell.gallery_leave(window, cx);
                }
            }))
            .child(overline("Themes", theme).px(px(space::S)));
        for (title, dark) in [("Dark", true), ("Light", false)] {
            let listed: Vec<&CatalogTheme> = dialog.catalog.group(dark).collect();
            if listed.is_empty() {
                continue;
            }
            let row: Vec<gpui::AnyElement> = listed
                .into_iter()
                .map(|card| self.theme_card(dialog, card, theme, cx))
                .collect();
            cards = cards.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::S))
                    .child(overline(title, theme).px(px(space::S)))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(space::M))
                            .children(row),
                    ),
            );
        }
        let errors = dialog.catalog.errors();
        if !errors.is_empty() {
            cards = cards.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(space::M))
                    .children(errors.iter().map(|error| theme_error_card(error, theme))),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(space::M))
            .child(cards)
            .child(meta_line(
                theme,
                self.themes_dir.display().to_string().into(),
            ))
            .into_any_element()
    }

    /// One card: the picture, the name and the badges; one `card_press` element.
    fn theme_card(
        &self,
        dialog: &SettingsDialog,
        card: &CatalogTheme,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let name = card.name.clone();
        let badges = dialog.card_badges(card);
        let kept = badges
            .iter()
            .any(|badge| matches!(badge, Badge::DarkMode | Badge::LightMode));
        let focused = dialog.focused_card().is_some_and(|c| c.name == card.name);
        let ring = if focused {
            theme.selected_ring
        } else if kept {
            theme.pane_focus_border
        } else {
            theme.border
        };
        let hover_name = name.clone();
        let press_name = name.clone();
        let badge_row = badges.into_iter().map(|badge| {
            let color = match badge {
                Badge::DarkMode | Badge::LightMode => theme.accent_text,
                Badge::File => theme.text_muted,
                Badge::LowContrast => theme.diff_mod_text,
            };
            let text = div()
                .text_size(font_size::META)
                .text_color(rgb(color))
                .child(badge.label());
            match (badge, card.problems.first()) {
                (Badge::LowContrast, Some(problem)) => text
                    .id(SharedString::from(format!("theme-low-{name}")))
                    .tooltip(tooltip(theme, &problem.to_string(), ""))
                    .into_any_element(),
                _ => text.into_any_element(),
            }
        });
        div()
            .id(SharedString::from(format!("theme-card-{name}")))
            .flex_none()
            .w(px(theme::THEME_CARD_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::S))
            .p(px(space::S))
            .rounded(px(radius::MEDIUM))
            .bg(rgb(theme.surface))
            // Always bordered, so a ring shifts nothing.
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(ring))
            .map(|el| card_press(el, kept || focused, theme))
            .on_hover(cx.listener(move |shell, hovered: &bool, window, cx| {
                if *hovered {
                    shell.gallery_hover(hover_name.clone(), window, cx);
                }
            }))
            .on_press(
                true,
                cx.listener(move |shell, _, window, cx| {
                    let name = press_name.clone();
                    shell.settings_do(move |d, keymap| d.gallery_click(&name, keymap), window, cx);
                }),
            )
            .child(theme_picture(&card.theme))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child(card.name.clone()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .min_h(theme::rems_px(theme::CHIP_HEIGHT))
                    .children(badge_row),
            )
            .into_any_element()
    }
}

/// A small window drawn in `card`'s own colors, whatever theme the app has: the chrome strip,
/// the sidebar strip, two text bars, an accent pill and the six hues of its ANSI palette.
fn theme_picture(card: &Theme) -> gpui::Div {
    let bar = |color: u32, width: f32| {
        div()
            .h(px(theme::THEME_BAR_HEIGHT))
            .w(px(width))
            .rounded(px(radius::SMALL))
            .bg(rgb(color))
    };
    let dots = card.ansi[1..=6]
        .iter()
        .map(|color| div().size(px(theme::THEME_DOT)).rounded_full().bg(rgb(*color)));
    div()
        .relative()
        .w_full()
        .h(px(theme::THEME_PICTURE_HEIGHT))
        .rounded(px(radius::MEDIUM))
        .overflow_hidden()
        .bg(rgb(card.bg))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(card.border))
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(theme::THEME_PICTURE_CHROME))
                .bg(rgb(card.chrome)),
        )
        .child(
            div()
                .absolute()
                .top(px(theme::THEME_PICTURE_CHROME))
                .bottom_0()
                .left_0()
                .w(px(theme::THEME_PICTURE_SIDEBAR))
                .bg(rgb(card.sidebar)),
        )
        .child(
            div()
                .absolute()
                .top(px(theme::THEME_PICTURE_CHROME + space::S))
                .left(px(theme::THEME_PICTURE_SIDEBAR + space::S))
                .flex()
                .flex_col()
                .gap(px(space::S))
                .child(bar(card.text, theme::THEME_BAR_LONG))
                .child(bar(card.text_muted, theme::THEME_BAR_SHORT))
                .child(
                    div()
                        .h(px(theme::THEME_PILL_HEIGHT))
                        .w(px(theme::THEME_PILL_WIDTH))
                        .rounded_full()
                        .bg(rgb(card.accent)),
                )
                .child(div().flex().gap(px(space::XS)).children(dots)),
        )
}

/// A file that is not loaded: its name and why, in the danger color; not clickable, so no
/// press look.
fn theme_error_card(error: &FileError, theme: &Theme) -> gpui::AnyElement {
    div()
        .flex_none()
        .w(px(theme::THEME_CARD_WIDTH))
        .flex()
        .flex_col()
        .gap(px(space::XS))
        .p(px(space::S))
        .rounded(px(radius::MEDIUM))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.border))
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text))
                .child(error.file.clone()),
        )
        .child(
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.danger_text))
                .child(error.message.clone()),
        )
        .into_any_element()
}
```

`Focus` is imported for use by Task 10's key scroll only if used here; if the compiler reports `unused import: Focus`, remove it from the `use` line in this task and add it back in the file that needs it.

- [ ] **Step 4: Wire it into the dialog**

`shell/settings.rs`:

1. `close_settings_dialog`: first line becomes `self.restore_theme_pick(window, cx);` (before `self.settings_dialog = None;`).
2. `apply_settings_effect`, just before `// The Prompts page reads its folder when it opens and after every write.` add:
   ```rust
           // A hover, a keyboard move or Esc changed what the gallery previews.
           self.sync_theme_preview(window, cx);
   ```
3. In `render_settings_dialog`'s `body`, after `.children(search.map(|search| search.mb(px(below))))` add:
   ```rust
               .children(
                   dialog
                       .gallery_shown()
                       .then(|| self.render_theme_gallery(dialog, theme, cx)),
               )
   ```
4. In `settings_key_down`, replace the final scroll block (lines 111-120) with:
   ```rust
           // Keep the focused row on screen: the banner, the Keyboard search box and the theme
           // gallery come before the cards, and a card holds several rows.
           if let Some(dialog) = &self.settings_dialog {
               let banner = usize::from(self.settings_notice(dialog).is_some());
               match dialog.focus {
                   Focus::Row(index) => {
                       let search = usize::from(keyboard_search_shown(dialog));
                       let gallery = usize::from(dialog.gallery_shown());
                       let card = dialog.group_of(index, &self.keymap);
                       self.settings_scroll
                           .scroll_to_item(card + banner + search + gallery);
                   }
                   Focus::Gallery(_) => self.settings_scroll.scroll_to_item(banner),
                   _ => {}
               }
           }
   ```

- [ ] **Step 5: Build, gate and commit**

Run: `CARGO_BUILD_JOBS=4 cargo check -p mkrew-ui --all-targets`
Expected: no errors, no warnings.
Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib settings_dialog theme`
Expected: PASS (unchanged).

```bash
cargo fmt --all
CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-core -p mkrew-ui --all-targets -- -D warnings
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/shell/theme_gallery.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/navigation.rs crates/mkrew-ui/src/shell/settings.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): theme gallery cards with hover preview on Settings Appearance"
```

---

### Task 10: New theme from current, Open themes folder

**Files:**
- Modify: `crates/mkrew-ui/src/shell/theme_gallery.rs` (two actions; the footer buttons)
- Modify: `crates/mkrew-ui/src/theme_catalog.rs` (one more test)

**Interfaces:**
- Consumes: `theme_file::{copy_name, file_name, render}`, `ThemeCatalog::find`, `Palette::from_theme`, `zed::open`, `Shell::toast_error`.
- Produces: `Shell::new_theme_from_current(&mut self, cx)`, `Shell::open_themes_folder(&mut self, cx)`.

- [ ] **Step 1: Write the failing test**

In `theme_catalog.rs` `mod tests` add (the file "New theme from current" writes loads back to the palette it was written from):

```rust
    #[test]
    fn a_written_copy_of_every_theme_loads_back_to_the_same_palette() {
        let original = ThemeCatalog::builtin();
        for listed in original.themes() {
            let name = theme_file::copy_name(&listed.name, |taken| original.find(taken).is_some());
            let text = theme_file::render(&name, &listed.palette);
            let copy = ThemeCatalog::from_files(vec![(theme_file::file_name(&name), Ok(text))]);
            let loaded = copy.find(&name).unwrap_or_else(|| panic!("{name} did not load"));
            assert_eq!(loaded.palette, listed.palette, "{name}");
            assert_eq!(loaded.dark, listed.dark, "{name}");
            assert!(copy.errors().is_empty(), "{name}: {:?}", copy.errors());
        }
    }
```

- [ ] **Step 2: Run it**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib theme_catalog::tests::a_written_copy`
Expected: PASS for the 22 built-ins (it pins `render`, `parse`, `copy_name` and `file_name` together before the shell uses them).

- [ ] **Step 3: Implement**

`theme_gallery.rs`: add `use mkrew_ui::theme::Palette;` and `use mkrew_ui::theme_file;` to the imports, then in an `impl Shell` block:

```rust
    /// "New theme from current": writes `<themes>/<name> copy.toml` (`copy 2`, `copy 3` when
    /// taken) with the palette of the theme shown now and opens it in Zed (spec 3.3). A
    /// failure toasts it and opens nothing; the poll lists the new file within a second.
    pub(crate) fn new_theme_from_current(&mut self, cx: &mut Context<'_, Self>) {
        let shown = cx.theme().clone();
        let catalog = cx.global::<ThemeCatalog>();
        let palette = (catalog.find(&shown.name))
            .map_or_else(|| Palette::from_theme(&shown), |listed| listed.palette);
        let dir = self.themes_dir.clone();
        let name = theme_file::copy_name(&shown.name, |candidate| {
            catalog.find(candidate).is_some()
                || dir.join(theme_file::file_name(candidate)).exists()
        });
        let path = dir.join(theme_file::file_name(&name));
        let text = theme_file::render(&name, &palette);
        let written = std::fs::create_dir_all(&dir).and_then(|()| {
            // `create_new`: a file made since the name was chosen is never overwritten.
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            std::io::Write::write_all(&mut file, text.as_bytes())
        });
        match written {
            Err(error) => {
                tracing::warn!(path = %path.display(), "new theme failed: {error}");
                self.toast_error(format!("Cannot create {}: {error}", path.display()), cx);
            }
            Ok(()) => {
                if let Err(error) = zed::open(&path, None) {
                    tracing::warn!(path = %path.display(), "open theme file failed: {error}");
                    self.toast_error(error.to_string(), cx);
                }
            }
        }
    }

    /// "Open themes folder": makes the folder when it is missing and opens it in the OS file
    /// manager, like "Open in File Explorer".
    pub(crate) fn open_themes_folder(&mut self, cx: &mut Context<'_, Self>) {
        match std::fs::create_dir_all(&self.themes_dir) {
            Ok(()) => cx.open_with_system(&self.themes_dir),
            Err(error) => {
                self.toast_error(format!("Cannot create the themes folder: {error}"), cx);
            }
        }
    }
```

In `render_theme_gallery` replace the last `.child(meta_line(theme, self.themes_dir.display().to_string().into()))` with the folder line plus its two quiet buttons:

```rust
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .child(div().flex_1().min_w_0().child(meta_line(
                        theme,
                        self.themes_dir.display().to_string().into(),
                    )))
                    .child(
                        quiet_button("theme-new-from-current", "New theme from current", theme, theme.hover)
                            .on_press(
                                true,
                                cx.listener(|shell, _, _, cx| shell.new_theme_from_current(cx)),
                            ),
                    )
                    .child(
                        quiet_button("theme-open-folder", "Open themes folder", theme, theme.hover)
                            .on_press(
                                true,
                                cx.listener(|shell, _, _, cx| shell.open_themes_folder(cx)),
                            ),
                    ),
            )
```

- [ ] **Step 4: Build and test**

Run: `CARGO_BUILD_JOBS=4 cargo check -p mkrew-ui --all-targets`
Expected: no errors, no warnings (the borrow of `catalog` ends before `self.toast_error`; if the compiler objects, copy what is needed out of `catalog` first: `let taken = |candidate: &str| catalog.find(candidate).is_some();` is not enough, so compute `name` before calling any `&mut self` method, as above).
Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib theme_catalog theme_file`
Expected: PASS.

- [ ] **Step 5: Gate and commit**

```bash
cargo fmt --all
CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-core -p mkrew-ui --all-targets -- -D warnings
git add crates/mkrew-ui/src/shell/theme_gallery.rs crates/mkrew-ui/src/theme_catalog.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): New theme from current and Open themes folder"
```

---

### Task 11: Docs (ADR 0101, SPEC, README, HUMAN_CHECKS) and full verify

**Files:**
- Create: `docs/decisions/0101-theme-gallery-and-theme-files.md`
- Modify: `docs/SPEC.md` (§14 theme bullet lines 1795-1803; §20.0 lines 2133-2135)
- Modify: `README.md` (Daily use after the Shortcuts bullet, line 195; Files and folders table row, line 273)
- Modify: `docs/HUMAN_CHECKS.md` (new section before line 24)

**Interfaces:** none (docs).

- [ ] **Step 1: Write the ADR**

Create `docs/decisions/0101-theme-gallery-and-theme-files.md`:

```markdown
# 0101 - Theme gallery, ten more built-in themes and theme files

- Date: 2026-11-22
- Status: accepted (amends ADR 0078, which rejected theme files as not asked for; amends SPEC section 14)
- Milestone: none (human request 2026-11-22, spec `docs/superpowers/specs/2026-11-22-theme-gallery-design.md`)

## Context

ADR 0078 gave Mkrew 12 themes picked from a list of names.
The user liked Tern's Settings > Appearance: themes as cards with swatches, a live preview on hover, and themes as plain files an agent can edit.
`PROTOCOL_VERSION` does not change (no daemon work).

## Options

1. Keep the name list and add themes: no preview by sight, no way to add a theme without a release.
2. A gallery of the compiled themes only: no files.
3. A gallery plus theme files with the fields of the built-in `Palette`, and ten more built-in themes. (Chosen.)
4. Import VS Code, Zed or Tern theme files: out of scope.

## Decision

- Settings > Appearance starts with a gallery of every theme, grouped Dark and Light.
  A card is a small window drawn in the theme's own colors, its name and badges ("Dark mode" / "Light mode" for the kept theme of a slot, "File", "Low contrast").
- Hovering a card for `THEME_PREVIEW_DELAY` (120 ms), or focusing it with the keyboard, shows the whole app in that theme; leaving the gallery, Esc or closing Settings puts the kept theme back; a click or Enter keeps the theme for its own slot (`dark_theme` or `light_theme`) and never changes the mode.
  A preview never writes `settings.toml` and never survives a restart.
- A theme file is `<config>/themes/*.toml`: `name`, `dark` and the fields of `Palette` (all required, `#RRGGBB`, unknown keys are an error).
  It is built with `Theme::from_palette`, the same derivation as the built-ins.
  The 500 ms settings poll also stamps the folder (names, times, sizes of the `*.toml` files) and rebuilds the catalog on a change.
- `ThemeCatalog` (a GPUI global) holds the built-ins in table order, then the valid files by name, plus one error entry per file that is not loaded (unreadable, parse error, name taken by a built-in or an earlier file, larger than 64 KB).
  `Theme::resolve`, the theme picker, the Settings slot lists and the gallery all read it.
  The slot lists offer every built-in and the file themes of the slot's own appearance.
  `Theme::name` is an owned `SharedString`.
- `theme::problems(&Theme)` is the one place that knows the contrast rules the theme tests used to repeat: body text, text on glass, on accent, window close, selected text, code, toast, highlighted diff lines, change bars, diff lines, the classic sidebar text, workspace colors, avatar letters and the chevron.
  The tests assert it is empty for every built-in; a theme file that breaks a rule still loads, with a "Low contrast" badge and the first problem in a tooltip.
- "New theme from current" writes `<name> copy.toml` (`copy 2`, `copy 3` when taken, a name never holds a character a file name cannot) and opens it in Zed; "Open themes folder" creates the folder and opens it in the OS file manager.
- Ten more built-in themes, each from the project's own published colors (the source URLs are comments in `theme::THEMES`): Nord, Solarized Dark, GitHub Dark, Rosé Pine, Kanagawa, Everforest Dark (dark), Rosé Pine Dawn, Everforest Light, Ayu Light, Tokyo Night Day (light).
  `THEMES` and `THEME_NAMES` list the dark ones first (12), then the light ones (10), so the picker keeps its order and `THEME_NAMES[12]` is Mkrew Light.

## Consequences

- A person or an agent adds a theme by saving a small file; it shows up within a second.
  A file saved in two steps by an editor can show its error card (and, for the kept theme, the slot's Mkrew theme) for one poll.
- The poll stays a poll (`ponytail:` comment): a folder with hundreds of files costs one stat each per tick; the upgrade path is `notify`.
- Programs drawing 24-bit colors keep their colors; ANSI 0-15 and the default fg/bg follow the theme, as in ADR 0078.
- Diagrams keep their Mkrew colors (ADR 0021); status, workspace, QR and classic sidebar colors stay Mkrew's.
- Palette changes made so every built-in passes `problems()` (published value to value used; the Mkrew workspace colors are fixed, so a pale theme needs lighter hover and selected fills than a mid-tone one can give):

  | Theme | Field | Published -> used |
  |---|---|---|
  | Solarized Dark | fg | 839496 -> EEE8D5 |
  | Solarized Dark | accent | 268BD2 -> 62ABDE |
  | GitHub Dark | accent | 2F81F7 -> 58A6FF |
  | Everforest Dark | fg | D3C6AA -> D5C9AE |
  | Everforest Light | fg | 5C6A72 -> 525F66 |
  | Everforest Light | accent | 3A94C5 -> 2A6F94 |
  | Everforest Light | surface | F4F0D9 -> FFFBEF |
  | Ayu Light | accent | F29718 -> 975E0F |
  | Tokyo Night Day | bg and surface | E1E2E7 -> F1F2F5 (E1E2E7 is the chrome) |
  | Tokyo Night Day | fg | 3760BF -> 2F52A2 |
  | Tokyo Night Day | accent | 2E7DE9 -> 2461B5 |

  Solarized Dark, GitHub Dark, Rosé Pine, Kanagawa, Everforest Dark, Nord and Rosé Pine Dawn otherwise use the published values; Nord, Solarized Dark and Rosé Pine have a `bg` equal to or very near their `surface` as their projects publish a flat window.
```

- [ ] **Step 2: Update SPEC, README, HUMAN_CHECKS**

`docs/SPEC.md` §14: replace the Theme bullet (lines 1795-1803, from `- Theme: System (follows the OS live)` to `sends them.`) with:

```markdown
- Theme: System (follows the OS live), Light or Dark - `[ui] theme`, cycled by the title bar toggle
  (ADR 0027). Each slot shows a theme (ADR 0078, ADR 0101): `[ui] light_theme` (default Mkrew Light) and `[ui] dark_theme` (default Mkrew Dark).
  There are 22 built-in themes.
  Dark: Mkrew Dark, One Dark, Dracula, Catppuccin Mocha, Tokyo Night, Gruvbox Dark, Nord, Solarized Dark, GitHub Dark, Rosé Pine, Kanagawa, Everforest Dark.
  Light: Mkrew Light, One Light, GitHub Light, Catppuccin Latte, Solarized Light, Gruvbox Light, Rosé Pine Dawn, Everforest Light, Ayu Light, Tokyo Night Day.
  A file `<config>/mkrew/themes/*.toml` adds one (`name`, `dark` and the fields of the built-in palette, all required; the folder is polled with `settings.toml`, so a saved file shows up within a second).
  An invalid file shows its error on a card and never breaks the app; a file with poor contrast loads with a "Low contrast" badge.
  A name that is unknown, or whose file is gone, shows the slot's Mkrew theme.
  Settings → Appearance opens with the theme gallery above its cards: "Dark" and "Light" groups of cards (a small window in the theme's own colors, its name, badges), "New theme from current" and "Open themes folder" under them.
  Hovering a card for 120 ms, or focusing it with the keyboard (Tab reaches the gallery, arrows move between cards), shows the whole app in that theme; leaving the gallery, Esc or closing Settings restores; a click or Enter keeps the theme for its own slot.
  "Theme: Select theme…" (Ctrl+Alt+J) lists the same themes, previews while moving, Enter or a click on a row saves to the slot shown, Esc restores (an outside click does not close it).
  Settings → Appearance also has "Light mode theme" / "Dark mode theme" rows. In terminals the default
  fg/bg and ANSI 0–15 follow the theme; 256-color 16–255 and 24-bit colors are drawn as the program
  sends them.
```

`docs/SPEC.md` §20.0 (lines 2134-2135) become:

```
light_theme = "Mkrew Light" # §14; a built-in theme name or the name of a file in themes/, shown in Light mode / a light OS
dark_theme = "Mkrew Dark"   # §14; the same for Dark mode / a dark OS
```

`README.md`: after the "Shortcuts" bullet (after line 195 `Defaults: [SPEC §13](docs/SPEC.md#13-ui-keyboard-shortcuts).`) add:

```markdown
- **Themes**: Settings → Appearance shows every theme as a card; hover to preview, click to keep.
  To add your own, save a `.toml` file in the `themes/` folder of the config directory (the page has "New theme from current" and "Open themes folder"); it shows up within a second.
```

and in the "Files and folders" table, row 273: `| Config (`agents/`, `keymap.toml`) |` becomes `| Config (`agents/`, `keymap.toml`, `themes/`) |`.

`docs/HUMAN_CHECKS.md`: insert before the heading `### To verify - 2026-11-21 Pressable audit (P3)` (line 24):

```markdown
### To verify - 2026-11-22 Theme gallery (plan docs/superpowers/plans/2026-11-22-theme-gallery.md)

- [ ] **Gallery layout (dark + light):** open Settings > Appearance: "Themes" overline, a "Dark" group and a "Light" group of cards, three per row at the default size, then the themes folder line with "New theme from current" and "Open themes folder".
  Each card has its picture, name and the right badges ("Dark mode" / "Light mode" on the kept themes).
- [ ] **A card picture against its theme (dark + light):** open the theme picker (`Ctrl+Alt+J`) on a few themes and compare the app with that theme's card: the strip colors, the text bars, the accent pill and the six dots match.
  Check Nord, Solarized Dark, Tokyo Night Day and Ayu Light in particular (their colors were changed for contrast, ADR 0101).
- [ ] **Hover preview without flashing (dark + light):** sweep the pointer across several cards: nothing changes until it rests about a tenth of a second on one; the whole app then shows that theme; leaving the gallery, pressing Esc and closing Settings each bring the kept theme back.
  A click keeps the card for its own slot (the badge moves) and does not change the mode; with the other appearance shown, the app keeps its own theme.
- [ ] **Keyboard:** Tab reaches the gallery as one stop, arrows move between cards in reading order across Dark and Light, the focused card previews at once, Enter keeps it.
- [ ] **A broken file's card:** save `<config>/themes/broken.toml` with `name = "x"` only: an error card names the file and the missing key; the rest of the gallery works and the card cannot be clicked.
  A file named like a built-in shows "Name taken by the built-in theme".
- [ ] **Live reload in Zed:** "New theme from current" opens `<name> copy.toml` in Zed; change `bg`, save: the card (and the app, when that theme is kept) follows within a second; delete the file: the card goes and the slot shows its Mkrew theme.
  Make a file whose `fg` equals its `bg`: its card shows "Low contrast" with the reason in a tooltip.
```

- [ ] **Step 3: Full verify**

Run (Git Bash, not WSL): `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: `cargo fmt --check` clean, clippy `-D warnings` clean, `INSTA_UPDATE=no cargo test --workspace` all PASS, final line `verify: OK`.
If the run passes 600 s, rerun the test step alone: `CARGO_BUILD_JOBS=4 INSTA_UPDATE=no cargo test --workspace`.
Also check the repo rules on the new files: `git grep -nP "\xE2\x80\x94" -- docs/decisions/0101-theme-gallery-and-theme-files.md docs/superpowers/plans/2026-11-22-theme-gallery.md crates/mkrew-ui/src/theme_file.rs crates/mkrew-ui/src/theme_catalog.rs crates/mkrew-ui/src/shell/theme_gallery.rs` prints nothing.

- [ ] **Step 4: Commit**

```bash
git add docs/decisions/0101-theme-gallery-and-theme-files.md docs/SPEC.md README.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "docs: ADR 0101, SPEC, README and HUMAN_CHECKS for the theme gallery"
```

---

## Self-review

**Spec coverage (sections 3 to 9):**
- 3.1 layout, card, groups, error card, folder line and buttons: Tasks 9 and 10; badges are the Task 8 model (`card_badges`) drawn in Task 9.
- 3.2 hover delay, leave, Esc, keyboard, click writes the slot key, never writes on preview: Task 8 (model, tests) and Task 9 (`sync_theme_preview`, `keep_theme` from Task 5).
- 3.3 New theme from current (numbering, round trip, Zed) and Open themes folder: Tasks 2 (`copy_name`, `file_name`, `render`), 4 and 10.
- 4.1 format, required keys, unknown keys, `from_palette`: Task 2 (parse) and Task 4 (build).
- 4.2 start load, poll with `ponytail:` comment, disappearing file fallback: Tasks 5 (resolve fallback), 7.
- 4.3 `problems()` with tests replacing the per-rule loops and the "Low contrast" badge: Tasks 3, 4, 8, 9.
- 5 catalog, owned name, `THEMES` and `THEME_NAMES` stay, slot lists from the catalog: Tasks 1, 4, 5, 6.
- 6 ten palettes with cited sources, ADR lists changes, 22 in the name-table test: Task 6 and Task 11.
- 7 error handling (folder or file unreadable, parse error, name clash, write failure toast, restore path): Tasks 4, 5 (`settings_write_failed`), 9 (`restore_theme_pick`), 10.
- 8 testing table: parse (Task 2), catalog (Task 4), problems (Task 3), gallery model (Task 8), copy numbering and round trip (Tasks 2 and 10), snapshots unchanged (every theme task runs `the_mkrew_themes_never_change`).
- 9 docs: Task 11.

**Placeholder scan:** no TBD, TODO or "similar to" steps; every step carries its code, path, command and expected result.

**Type consistency:** `Theme::name: SharedString` (Task 1) is read with `.as_ref()`, `&*` or `to_string()` everywhere; `ThemeCatalog::{find, index_of, group, slot_names, themes, errors}` (Task 4) match their uses in Tasks 5 to 10; `Effect::KeepTheme { name: SharedString, dark: bool }` (Task 8) matches the arm in `apply_settings_effect` and `Shell::keep_theme(&str, bool, ..)` (Task 5); `SettingsDialog::{gallery_hover, gallery_hover_due, gallery_leave, end_preview, previewing, gallery_click, card_badges, kept_names, focused_card, gallery_shown, gallery_cards, set_catalog}` match Task 9's calls; `Palette::from_theme`, `ThemeEntry::palette`, `theme_file::{parse, render, file_name, copy_name}` are defined before use.

**Spec gaps found (spec not edited):**
1. Existing built-in entries cite no source URLs, so "as the existing entries do" has no precedent; the plan adds URLs for the ten new entries only.
2. "`THEME_NAMES` and `THEMES` grow to 22 in the same order" collides with `THEME_NAMES[6] == DEFAULT_LIGHT_THEME` and two picker tests; the plan keeps the picker's "dark ones, then light ones" order (six dark entries after Gruvbox Dark) and updates those tests.
3. Published colors fail the existing rules for 7 of the 10 themes (the Mkrew workspace colors are fixed, so a mid-tone light theme cannot satisfy their 3:1 rule on its hover fill); Tokyo Night Day needs its bg lightened from E1E2E7 to F1F2F5. The ADR table lists every change.
4. "Slot lists read built-ins plus valid files of that kind" is ambiguous; the plan offers every built-in (as today, a slot may hold the other appearance) plus the file themes of the slot's own appearance.
5. The spec says core needs no change, but `Paths::themes_dir()` and the `THEME_NAMES` growth are core changes.
6. `problems()` in the spec lists eight rules; today's tests also check body text, window close, the classic sidebar text, workspace colors, avatar letters and the chevron, so the plan includes all of them (a file theme gets the same bar as a built-in).
7. A file saved by an editor in two steps (truncate, then write) can be read half-written: its error card and, for the kept theme, the slot's Mkrew theme show for one poll. The spec is silent; the plan documents it in the ADR and does not debounce.
8. "Esc restores and then works as today" is implemented as: Esc ends the preview and then does what Esc does today (clear search, close Settings).
9. "Arrow keys move between cards in reading order" is implemented as Left / Up previous and Right / Down next, with no column-aware movement.
10. Spec is silent on a broken `settings.toml` (read-only dialog): previews work, a click on a card writes nothing.
11. Safety additions the spec does not name: theme names must be non-empty without control characters, a `*.toml` over 64 KB is an error card, and the slot value is written as an escaped TOML string.
