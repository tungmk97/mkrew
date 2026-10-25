# Theme Picker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pick one of 12 built-in named themes per light/dark slot, VS Code / Zed style, with live preview, without changing the default Mkrew look by a single token.

**Architecture:** `theme.rs` gains a `Palette` → `Theme` derivation (`from_palette`) and a `THEMES` table; the two Mkrew entries keep calling today's `Theme::dark()` / `Theme::light()`. `[ui] light_theme` / `dark_theme` (mkrew-core) feed a `ThemeNames` GPUI global that `theme::refresh` reads. The command palette is reused as the picker (`Action::Theme`), the Settings dialog gets a `Kind::List` pick.

**Tech Stack:** Rust, GPUI, serde/toml, insta.

**Spec:** `docs/superpowers/specs/2026-10-25-theme-picker-design.md`

## Global Constraints

- `Theme::dark()` / `Theme::light()` color values are never edited; the golden snapshot of Task 1 must stay byte-identical through every task.
- Missing / unknown theme names fall back to `Mkrew Light` / `Mkrew Dark` silently (no parse error).
- Theme names, in order: `Mkrew Dark`, `One Dark`, `Dracula`, `Catppuccin Mocha`, `Tokyo Night`, `Gruvbox Dark`, `Mkrew Light`, `One Light`, `GitHub Light`, `Catppuccin Latte`, `Solarized Light`, `Gruvbox Light`.
- `status_*`, `workspace_colors`, `qr_*`, `classic` are the Mkrew base's in every theme.
- Settings writes go through `write_ui_value` / `Setting::write` (rest of the file byte for byte).
- New command `select_theme`, default `ctrl-alt-j`, palette title `Theme: Select theme…`.
- No hard-coded color/spacing outside `theme.rs` (SPEC §14).
- Verify: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL). Commit with `git -c user.email=tungnv975@gmail.com commit`, stage only your own paths.

## Review Focus

1. A user with no `light_theme` / `dark_theme` keys sees exactly today's colors (Task 1 golden + Task 4 `default_names_resolve_to_exactly_today`).
2. A hand-edited, misspelled theme name (`dark_theme = "dracula"`) must not break the file or the UI: Mkrew Dark is used (Task 4 `unknown_names_fall_back_to_mkrew`).
3. Esc in the picker after browsing several themes restores the exact theme from before opening, including fonts (Task 5 restore uses the saved `Theme` clone; smoke step).
4. Picking a theme while in System mode with the OS dark writes `dark_theme`, never `light_theme` (Task 5 `the_pick_goes_to_the_slot_shown`).
5. A light theme placed in the dark slot renders with `dark == false` so Mermaid images pick the light variant (Task 4 `a_slot_may_hold_the_other_appearance`).

---

### Task 1: Golden snapshot of the default themes

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (tests module, ~line 1309+)
- Create (generated): `crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_dark.snap`, `…__mkrew_light.snap`

**Interfaces:**
- Produces: test helper `golden(&Theme) -> String` in `theme.rs` tests.

- [ ] **Step 1: Add the golden test** (inside `mod tests` of `theme.rs`)

```rust
    /// Debug dump of every token, minus fields that are not colors and may differ by platform or
    /// be added later (`name`, font families). Guards "the default look never changes" (ADR 0075).
    fn golden(theme: &Theme) -> String {
        format!("{theme:#?}")
            .lines()
            .filter(|line| {
                let line = line.trim_start();
                !(line.starts_with("name:")
                    || line.starts_with("ui_font_family:")
                    || line.starts_with("terminal_font_family:"))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_mkrew_themes_never_change() {
        insta::assert_snapshot!("mkrew_dark", golden(&Theme::dark()));
        insta::assert_snapshot!("mkrew_light", golden(&Theme::light()));
    }
```

- [ ] **Step 2: Run, accept the first snapshots**

Run: `cargo test -p mkrew-ui --lib theme::tests::the_mkrew_themes_never_change` → FAIL (new snapshots), then `cargo insta accept`, then run again → PASS. Open the two `.snap` files: they list color tokens (`bg: 1382171,` …) and `classic: Some(…)`.

- [ ] **Step 3: Commit**

```bash
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_dark.snap crates/mkrew-ui/src/snapshots/mkrew_ui__theme__tests__mkrew_light.snap
git -c user.email=tungnv975@gmail.com commit -m "test(ui): golden snapshot of the Mkrew themes"
```

---

### Task 2: `[ui] light_theme` / `dark_theme` and the Settings list pick

**Files:**
- Modify: `crates/mkrew-core/src/settings.rs` (`UiSettings` ~269-307, template ~447-463, test literal ~676-686)
- Modify: `crates/mkrew-core/src/settings_fields.rs` (`Kind` ~46-55, `use Kind::…` line 100, rows after `ui.theme` ~145-152)
- Modify: `crates/mkrew-ui/src/settings_dialog.rs` (`FontPick` → `ListPick`, `font_pick` → `list_pick`, `font_choices` → `list_choices`, `font_key` → `list_key`; activation ~251-259; tests ~689-707)
- Modify: `crates/mkrew-ui/src/shell/settings.rs` (`Kind::Font` control ~507-525, `font_list` ~581-638 and its caller)

**Interfaces:**
- Produces: `mkrew_core::settings::{THEME_NAMES, DEFAULT_DARK_THEME, DEFAULT_LIGHT_THEME}`; `UiSettings.light_theme: String`, `UiSettings.dark_theme: String`; `settings_fields::Kind::List(&'static [&'static str])`; `SettingsDialog::list_choices(&self) -> Vec<String>`, field `list_pick: Option<ListPick>`.

- [ ] **Step 1: Failing core tests** (append in `settings.rs` tests)

```rust
    #[test]
    fn theme_names_default_to_mkrew_and_accept_any_string() {
        let ui = |src: &str| parse(Path::new("s.toml"), src).unwrap().ui;
        assert_eq!(ui("").light_theme, DEFAULT_LIGHT_THEME);
        assert_eq!(ui("").dark_theme, DEFAULT_DARK_THEME);
        assert_eq!(ui("[ui]\ndark_theme = \"Dracula\"").dark_theme, "Dracula");
        // Unknown names parse; the UI falls back (ADR 0075).
        assert_eq!(ui("[ui]\nlight_theme = \"nope\"").light_theme, "nope");
        assert_eq!(THEME_NAMES[0], DEFAULT_DARK_THEME);
        assert_eq!(THEME_NAMES[6], DEFAULT_LIGHT_THEME);
    }

    #[test]
    fn writing_a_theme_name_keeps_the_rest() {
        let src = "# mine\n[ui]\ntheme = \"dark\"\n";
        let out = set_ui_value(src, "dark_theme", "\"One Dark\"");
        assert!(out.starts_with("# mine\n[ui]\n"), "{out}");
        assert!(out.contains("theme = \"dark\"\n"), "{out}");
        let ui = parse(Path::new("s.toml"), &out).unwrap().ui;
        assert_eq!((ui.dark_theme.as_str(), ui.theme), ("One Dark", ThemeMode::Dark));
    }
```

(`set_ui_value` is the private test helper / function used by `set_theme` at ~551; if the helper is named differently in the tests module, call the same function `set_theme` wraps.)

Run: `cargo test -p mkrew-core theme_name` → FAIL (no field / const).

- [ ] **Step 2: Implement in `settings.rs`**

Add near `ThemeMode`:

```rust
/// Built-in theme names (ADR 0075), in picker order: dark ones, then light ones. The UI's
/// `theme::THEMES` lists the same names (tested there).
pub const THEME_NAMES: &[&str] = &[
    "Mkrew Dark",
    "One Dark",
    "Dracula",
    "Catppuccin Mocha",
    "Tokyo Night",
    "Gruvbox Dark",
    "Mkrew Light",
    "One Light",
    "GitHub Light",
    "Catppuccin Latte",
    "Solarized Light",
    "Gruvbox Light",
];
pub const DEFAULT_DARK_THEME: &str = "Mkrew Dark";
pub const DEFAULT_LIGHT_THEME: &str = "Mkrew Light";
```

In `UiSettings` after `theme`:

```rust
    /// `[ui] light_theme` (ADR 0075): the theme in Light mode and in System mode while the OS is
    /// light. Any string; an unknown name shows Mkrew Light.
    pub light_theme: String,
    /// `[ui] dark_theme`: the same for Dark mode / a dark OS; unknown shows Mkrew Dark.
    pub dark_theme: String,
```

`Default`: `light_theme: DEFAULT_LIGHT_THEME.into(), dark_theme: DEFAULT_DARK_THEME.into(),`.
Template: after the `# Theme: system …` comment line add
`     # Light / dark theme: a built-in name (Settings or \"Theme: Select theme…\" lists them).\n\`
and after `     # theme = \"system\"\n\` add
`     # light_theme = \"Mkrew Light\"\n\` and `     # dark_theme = \"Mkrew Dark\"\n\`.
Test literal at ~676: add `light_theme: DEFAULT_LIGHT_THEME.into(), dark_theme: DEFAULT_DARK_THEME.into(),`.

- [ ] **Step 3: `Kind::List` and rows in `settings_fields.rs`**

In `enum Kind`:

```rust
    /// One of these names, picked from a searchable list (too many for `Choice`).
    List(&'static [&'static str]),
```

`use Kind::{Bool, Choice, Font, List, Number, Text};`, `use crate::settings::{Settings, SettingsError, THEME_NAMES, write_value};`. After the `ui.theme` row:

```rust
    row(
        Appearance,
        "ui",
        "light_theme",
        List(THEME_NAMES),
        "Light mode theme",
        "Used in Light mode and when the OS is light.",
    ),
    row(
        Appearance,
        "ui",
        "dark_theme",
        List(THEME_NAMES),
        "Dark mode theme",
        "Used in Dark mode and when the OS is dark.",
    ),
```

`Setting::literal` already writes non-Bool/Number kinds as TOML strings: no change.

- [ ] **Step 4: Run core tests**

Run: `cargo test -p mkrew-core` → PASS (including `every_template_key_has_exactly_one_setting`, `writing_each_default_reads_it_back_and_keeps_comments`).

- [ ] **Step 5: Failing dialog test** (in `settings_dialog.rs` tests; also rename `font_choices`/`font_pick` in `the_font_pick_lists_default_first_and_filters`)

```rust
    #[test]
    fn the_theme_pick_lists_the_names_and_filters() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        focus_on(&mut d, "ui.dark_theme", &keymap);
        d.key(&key("enter"), None, &keymap);
        assert_eq!(
            d.list_choices().len(),
            mkrew_core::settings::THEME_NAMES.len(),
            "every name, no Default row"
        );
        for c in ["d", "r", "a", "c"] {
            d.key(&typed(c), None, &keymap);
        }
        assert_eq!(d.list_choices(), vec!["Dracula"]);
        assert_eq!(
            d.key(&key("enter"), None, &keymap),
            Effect::Write {
                setting: setting("ui.dark_theme"),
                text: "Dracula".into()
            }
        );
        assert!(d.list_pick.is_none());
    }
```

Run: `cargo test -p mkrew-ui --lib settings_dialog` → FAIL.

- [ ] **Step 6: Generalise the pick in `settings_dialog.rs`**

Rename `FontPick` → `ListPick`, `font_pick` → `list_pick`, `font_key` → `list_key` (all uses). Activation:

```rust
                Kind::Font | Kind::List(_) if !self.read_only() => {
                    self.list_pick = Some(ListPick {
                        setting,
                        query: Input::default(),
                        selected: 0,
                    });
                    Effect::None
                }
                Kind::Font | Kind::List(_) => Effect::None,
```

Replace `font_choices`:

```rust
    /// The pick's choices matching its query: for a font `""` (Default) first, then the
    /// installed fonts; for a list its names.
    pub fn list_choices(&self) -> Vec<String> {
        let Some(pick) = self.list_pick.as_ref() else {
            return Vec::new();
        };
        let query = pick.query.trim().to_lowercase();
        let matches = |name: &str| name.to_lowercase().contains(&query);
        if let Kind::List(names) = pick.setting.kind {
            return names
                .iter()
                .filter(|name| matches(name))
                .map(|name| (*name).to_owned())
                .collect();
        }
        let mut choices = Vec::new();
        if query.is_empty() {
            choices.push(String::new());
        }
        choices.extend(self.fonts.iter().filter(|font| matches(font)).cloned());
        choices
    }
```

`list_key` calls `self.list_choices()`. Any other `match setting.kind` the compiler flags gets `| Kind::List(_)` on the `Kind::Font` arm.

- [ ] **Step 7: Render in `shell/settings.rs`**

Control arm: `Kind::Font | Kind::List(_) => div()…` (same box; "Default" only when `shown.is_empty()`, which a list never is). Rename `font_list` → `list_pick_view` (and its caller) and inside: `let what = if matches!(setting.kind, Kind::List(_)) { "Search themes" } else { "Search fonts" };` as the caret field placeholder; ids `settings-list-query`, `settings-list`, `list-choice-{i}`; iterate `dialog.list_choices()`; `dialog.list_pick`.

- [ ] **Step 8: Run**

Run: `cargo test -p mkrew-ui --lib settings_dialog` → PASS; `cargo build -p mkrew-ui` → OK.

- [ ] **Step 9: Commit**

```bash
git add crates/mkrew-core/src/settings.rs crates/mkrew-core/src/settings_fields.rs crates/mkrew-ui/src/settings_dialog.rs crates/mkrew-ui/src/shell/settings.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(core): light_theme / dark_theme settings and list pick"
```

---

### Task 3: `Palette`, `from_palette` and the 12 themes

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (struct `Theme` ~566-717, `dark()` ~772, `light()` ~864, `shared()` ~1097, new items after `impl Theme`, tests)

**Interfaces:**
- Consumes: `mkrew_core::settings::THEME_NAMES` (Task 2).
- Produces: `pub struct Palette`; `pub fn mix(a: u32, b: u32, t: f32) -> u32`; `Theme::from_palette(name: &'static str, p: &Palette) -> Theme`; `pub struct ThemeEntry { pub name: &'static str, pub dark: bool, palette: Option<Palette> }`; `ThemeEntry::build(&self) -> Theme`; `pub const THEMES: &[ThemeEntry]`; `pub fn find_theme(name: &str) -> Option<&'static ThemeEntry>`; field `Theme.name: &'static str`.

- [ ] **Step 1: Failing tests** (theme.rs tests)

```rust
    fn all_themes() -> Vec<Theme> {
        THEMES.iter().map(ThemeEntry::build).collect()
    }

    #[test]
    fn the_theme_table_matches_the_settings_names() {
        let names: Vec<&str> = THEMES.iter().map(|e| e.name).collect();
        assert_eq!(names, mkrew_core::settings::THEME_NAMES);
        for entry in THEMES {
            let theme = entry.build();
            assert_eq!(theme.dark, entry.dark, "{}", entry.name);
            assert_eq!(theme.name, entry.name);
        }
        assert_eq!(find_theme("Mkrew Dark").map(ThemeEntry::build), Some(Theme::dark()));
        assert_eq!(find_theme("Mkrew Light").map(ThemeEntry::build), Some(Theme::light()));
        assert!(find_theme("dracula").is_none(), "exact names only");
    }

    #[test]
    fn mix_blends_per_channel() {
        assert_eq!(mix(0x000000, 0xFFFFFF, 0.5), 0x808080);
        assert_eq!(mix(0x102030, 0x102030, 0.7), 0x102030);
        assert_eq!(mix(0xFF0000, 0x0000FF, 0.0), 0xFF0000);
        assert_eq!(mix(0xFF0000, 0x0000FF, 1.0), 0x0000FF);
    }

    #[test]
    fn derived_themes_keep_the_fixed_tokens() {
        for entry in THEMES.iter().filter(|e| e.palette.is_some()) {
            let theme = entry.build();
            let base = if entry.dark { Theme::dark() } else { Theme::light() };
            let fixed = |t: &Theme| {
                (t.status_working, t.status_needs_input, t.status_done, t.status_exited,
                 t.status_idle, t.workspace_colors, t.qr_dark, t.qr_light)
            };
            assert_eq!(fixed(&theme), fixed(&base), "{}", entry.name);
            assert_eq!(theme.classic(), base.classic(), "{}", entry.name);
        }
    }
```

Replace `[Theme::dark(), Theme::light()]` by `all_themes()` in every existing contrast test: `body_text_contrast_is_at_least_4_5`, `window_close_hover_is_readable`, `on_accent_text_is_readable_on_the_accent`, `selected_text_stays_readable`, `code_stays_readable`, `toast_text_and_marks_read_on_the_card`, `highlighted_diff_lines_stay_readable`, `change_bars_stand_out_from_the_editor`, `diff_line_colors_read_on_their_backgrounds`, `workspace_colors_stand_out_from_every_surface_and_from_each_other`, `avatar_letters_and_chevrons_stay_readable`. Prefix each assert message with the theme name (e.g. `"{}: {fg:06X} on {bg:06X}: {ratio:.2}", theme.name`).

Run: `cargo test -p mkrew-ui --lib theme` → FAIL (missing items).

- [ ] **Step 2: `name` field**

In `struct Theme` after `mode`:

```rust
    /// The built-in theme this is (ADR 0075), e.g. "Mkrew Dark".
    pub name: &'static str,
```

`shared()`: `name: "",`. In `dark()`'s struct literal: `name: "Mkrew Dark",`; in `light()`'s: `name: "Mkrew Light",` (no color changes). Run `cargo test -p mkrew-ui --lib the_mkrew_themes_never_change` → PASS (golden filters `name:`).

- [ ] **Step 3: Palette, helpers, derivation** (new code after the existing `impl Theme { … }`)

```rust
/// The colors a built-in theme is made of (ADR 0075); [`Theme::from_palette`] derives the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub dark: bool,
    /// The main area and the terminal.
    pub bg: u32,
    /// Sidebar, title bar, tab strip.
    pub chrome: u32,
    /// Panels, popovers, inputs, cards.
    pub surface: u32,
    pub fg: u32,
    pub muted: u32,
    pub accent: u32,
    /// Hues for UI text roles (diff, syntax, git letters); made readable by `readable`.
    pub red: u32,
    pub green: u32,
    pub yellow: u32,
    pub blue: u32,
    pub magenta: u32,
    pub cyan: u32,
    /// The terminal's ANSI 0–15 as the theme publishes them.
    pub ansi: [u32; 16],
}

/// `a` blended toward `b` by `t` (0 = `a`, 1 = `b`), per RGB channel.
pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    [16, 8, 0].into_iter().fold(0, |out, shift| {
        let (x, y) = (((a >> shift) & 0xFF) as f32, ((b >> shift) & 0xFF) as f32);
        out | (((x + (y - x) * t).round() as u32) << shift)
    })
}

/// `color` blended toward `toward` in 5 % steps until it reads at `min` on every ground.
fn readable(color: u32, toward: u32, grounds: &[u32], min: f32) -> u32 {
    (0..=20)
        .map(|step| mix(color, toward, step as f32 * 0.05))
        .find(|c| grounds.iter().all(|g| contrast(*c, *g) >= min))
        .unwrap_or(toward)
}

/// `bg` tinted with `hue` by at most `t`, less (0.05 steps) until `fg` reads ≥ 4.5:1 on it.
fn fill(bg: u32, hue: u32, t: f32, fg: u32) -> u32 {
    let steps = (t / 0.05).round() as i32;
    (0..=steps)
        .rev()
        .map(|step| mix(bg, hue, step as f32 * 0.05))
        .find(|c| contrast(fg, *c) >= 4.5)
        .unwrap_or(bg)
}

impl Theme {
    /// A built-in theme from its palette (ADR 0075; token table in the spec). Status, workspace
    /// and QR colors, the classic sidebar palette, fonts and sizes stay the Mkrew base's.
    pub fn from_palette(name: &'static str, p: &Palette) -> Self {
        let base = if p.dark { Self::dark() } else { Self::light() };
        let tint = |hue, t| mix(p.bg, hue, t);
        let surface_raised = mix(p.surface, p.fg, 0.10);
        let hover = mix(p.bg, p.fg, 0.06);
        let diff = |hue| (tint(hue, 0.14), tint(hue, 0.24), tint(hue, 0.30));
        let (add_bg, add_gutter, add_word) = diff(p.green);
        let (del_bg, del_gutter, del_word) = diff(p.red);
        let (mod_bg, mod_gutter, _) = diff(p.yellow);
        let grounds = [
            p.bg, p.chrome, p.surface, surface_raised, hover, add_bg, del_bg, mod_bg, add_gutter,
            del_gutter, mod_gutter, add_word, del_word,
        ];
        let text = |color| readable(color, p.fg, &grounds, 4.5);
        let on_accent = if contrast(p.bg, p.accent) >= contrast(p.fg, p.accent) {
            p.bg
        } else {
            p.fg
        };
        Self {
            name,
            bg: p.bg,
            terminal_bg: p.bg,
            sidebar: p.chrome,
            chrome: p.chrome,
            surface: p.surface,
            card: p.surface,
            surface_raised,
            selected: surface_raised,
            hover,
            row_hover: hover,
            well: hover,
            diff_hunk_header: hover,
            border: mix(p.bg, p.fg, 0.14),
            control_border: mix(p.bg, p.fg, 0.14),
            divider: mix(p.bg, p.fg, 0.09),
            selected_ring: mix(p.accent, p.bg, 0.50),
            text: p.fg,
            text_dim: text(mix(p.fg, p.bg, 0.20)),
            text_muted: text(p.muted),
            text_subtle: text(p.muted),
            text_disabled: text(p.muted),
            subtle: mix(p.muted, p.bg, 0.35),
            accent: p.accent,
            accent_text: text(p.accent),
            info: p.blue,
            info_soft: p.blue,
            info_text: text(p.blue),
            accent_bg: tint(p.accent, 0.14),
            pane_focus_border: mix(p.accent, p.bg, 0.50),
            on_accent,
            terminal_selection_bg: fill(p.bg, p.accent, 0.30, p.fg),
            terminal_match_bg: fill(p.bg, p.yellow, 0.20, p.fg),
            terminal_match_current_bg: fill(p.bg, p.yellow, 0.50, p.fg),
            warning_bg: tint(p.yellow, 0.14),
            success_bg: tint(p.green, 0.14),
            danger_bg: tint(p.red, 0.14),
            run_pill_bg: tint(p.green, 0.12),
            run_pill_border: tint(p.green, 0.35),
            run_pill_text: text(p.green),
            diff_add_bg: add_bg,
            diff_del_bg: del_bg,
            diff_mod_bg: mod_bg,
            diff_add_gutter: add_gutter,
            diff_del_gutter: del_gutter,
            diff_mod_gutter: mod_gutter,
            diff_add_word_bg: add_word,
            diff_del_word_bg: del_word,
            diff_add_text: text(p.green),
            success_text: text(p.green),
            diff_del_text: text(p.red),
            danger_text: text(p.red),
            diff_mod_text: text(p.yellow),
            git_modified: text(p.yellow),
            git_untracked: text(p.yellow),
            syntax_number: text(p.yellow),
            syntax_keyword: text(p.magenta),
            syntax_string: text(p.green),
            syntax_type: text(p.cyan),
            syntax_function: text(p.blue),
            teal: text(p.cyan),
            folder: text(p.blue),
            ansi: p.ansi,
            dark: p.dark,
            ..base
        }
    }
}

/// One built-in theme (ADR 0075). The Mkrew ones have no palette: they are `Theme::dark()` /
/// `Theme::light()` exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeEntry {
    pub name: &'static str,
    pub dark: bool,
    palette: Option<Palette>,
}

impl ThemeEntry {
    pub fn build(&self) -> Theme {
        match (&self.palette, self.dark) {
            (Some(palette), _) => Theme::from_palette(self.name, palette),
            (None, true) => Theme::dark(),
            (None, false) => Theme::light(),
        }
    }
}

/// The theme named `name` (exact match).
pub fn find_theme(name: &str) -> Option<&'static ThemeEntry> {
    THEMES.iter().find(|entry| entry.name == name)
}
```

(`as` casts mirror the existing `hue()` test helper; `cargo fmt` decides the layout.)

- [ ] **Step 4: The table** (values from each theme's published palette; `ansi` as published)

```rust
const fn dark_entry(name: &'static str, palette: Palette) -> ThemeEntry {
    ThemeEntry { name, dark: true, palette: Some(palette) }
}

const fn light_entry(name: &'static str, palette: Palette) -> ThemeEntry {
    ThemeEntry { name, dark: false, palette: Some(palette) }
}

/// Every built-in theme, in [`mkrew_core::settings::THEME_NAMES`] order.
pub const THEMES: &[ThemeEntry] = &[
    ThemeEntry { name: "Mkrew Dark", dark: true, palette: None },
    dark_entry("One Dark", Palette {
        dark: true, bg: 0x282C34, chrome: 0x21252B, surface: 0x2C313A, fg: 0xABB2BF,
        muted: 0x9DA5B4, accent: 0x61AFEF,
        red: 0xE06C75, green: 0x98C379, yellow: 0xE5C07B, blue: 0x61AFEF, magenta: 0xC678DD, cyan: 0x56B6C2,
        ansi: [0x3F4451, 0xE06C75, 0x98C379, 0xD19A66, 0x61AFEF, 0xC678DD, 0x56B6C2, 0xABB2BF,
               0x5C6370, 0xE06C75, 0x98C379, 0xE5C07B, 0x61AFEF, 0xC678DD, 0x56B6C2, 0xFFFFFF],
    }),
    dark_entry("Dracula", Palette {
        dark: true, bg: 0x282A36, chrome: 0x21222C, surface: 0x343746, fg: 0xF8F8F2,
        muted: 0xA0A8CD, accent: 0xBD93F9,
        red: 0xFF5555, green: 0x50FA7B, yellow: 0xF1FA8C, blue: 0xBD93F9, magenta: 0xFF79C6, cyan: 0x8BE9FD,
        ansi: [0x21222C, 0xFF5555, 0x50FA7B, 0xF1FA8C, 0xBD93F9, 0xFF79C6, 0x8BE9FD, 0xF8F8F2,
               0x6272A4, 0xFF6E6E, 0x69FF94, 0xFFFFA5, 0xD6ACFF, 0xFF92DF, 0xA4FFFF, 0xFFFFFF],
    }),
    dark_entry("Catppuccin Mocha", Palette {
        dark: true, bg: 0x1E1E2E, chrome: 0x181825, surface: 0x313244, fg: 0xCDD6F4,
        muted: 0xA6ADC8, accent: 0x89B4FA,
        red: 0xF38BA8, green: 0xA6E3A1, yellow: 0xF9E2AF, blue: 0x89B4FA, magenta: 0xCBA6F7, cyan: 0x94E2D5,
        ansi: [0x45475A, 0xF38BA8, 0xA6E3A1, 0xF9E2AF, 0x89B4FA, 0xF5C2E7, 0x94E2D5, 0xBAC2DE,
               0x585B70, 0xF38BA8, 0xA6E3A1, 0xF9E2AF, 0x89B4FA, 0xF5C2E7, 0x94E2D5, 0xA6ADC8],
    }),
    dark_entry("Tokyo Night", Palette {
        dark: true, bg: 0x1A1B26, chrome: 0x16161E, surface: 0x24283B, fg: 0xC0CAF5,
        muted: 0x9AA5CE, accent: 0x7AA2F7,
        red: 0xF7768E, green: 0x9ECE6A, yellow: 0xE0AF68, blue: 0x7AA2F7, magenta: 0xBB9AF7, cyan: 0x7DCFFF,
        ansi: [0x15161E, 0xF7768E, 0x9ECE6A, 0xE0AF68, 0x7AA2F7, 0xBB9AF7, 0x7DCFFF, 0xA9B1D6,
               0x414868, 0xF7768E, 0x9ECE6A, 0xE0AF68, 0x7AA2F7, 0xBB9AF7, 0x7DCFFF, 0xC0CAF5],
    }),
    dark_entry("Gruvbox Dark", Palette {
        dark: true, bg: 0x282828, chrome: 0x1D2021, surface: 0x32302F, fg: 0xEBDBB2,
        muted: 0xBDAE93, accent: 0x83A598,
        red: 0xFB4934, green: 0xB8BB26, yellow: 0xFABD2F, blue: 0x83A598, magenta: 0xD3869B, cyan: 0x8EC07C,
        ansi: [0x282828, 0xCC241D, 0x98971A, 0xD79921, 0x458588, 0xB16286, 0x689D6A, 0xA89984,
               0x928374, 0xFB4934, 0xB8BB26, 0xFABD2F, 0x83A598, 0xD3869B, 0x8EC07C, 0xEBDBB2],
    }),
    ThemeEntry { name: "Mkrew Light", dark: false, palette: None },
    light_entry("One Light", Palette {
        dark: false, bg: 0xFAFAFA, chrome: 0xEAEAEB, surface: 0xF2F2F2, fg: 0x383A42,
        muted: 0x696C77, accent: 0x4078F2,
        red: 0xE45649, green: 0x50A14F, yellow: 0xC18401, blue: 0x4078F2, magenta: 0xA626A4, cyan: 0x0184BC,
        ansi: [0x383A42, 0xE45649, 0x50A14F, 0xC18401, 0x4078F2, 0xA626A4, 0x0184BC, 0xA0A1A7,
               0x4F525E, 0xE06C75, 0x98C379, 0xE5C07B, 0x61AFEF, 0xC678DD, 0x56B6C2, 0xFFFFFF],
    }),
    light_entry("GitHub Light", Palette {
        dark: false, bg: 0xFFFFFF, chrome: 0xF6F8FA, surface: 0xF6F8FA, fg: 0x1F2328,
        muted: 0x59636E, accent: 0x0969DA,
        red: 0xCF222E, green: 0x1A7F37, yellow: 0x9A6700, blue: 0x0969DA, magenta: 0x8250DF, cyan: 0x1B7C83,
        ansi: [0x24292F, 0xCF222E, 0x116329, 0x4D2D00, 0x0969DA, 0x8250DF, 0x1B7C83, 0x6E7781,
               0x57606A, 0xA40E26, 0x1A7F37, 0x633C01, 0x218BFF, 0xA475F9, 0x3192AA, 0x8C959F],
    }),
    light_entry("Catppuccin Latte", Palette {
        dark: false, bg: 0xEFF1F5, chrome: 0xDCE0E8, surface: 0xE6E9EF, fg: 0x4C4F69,
        muted: 0x5C5F77, accent: 0x1E66F5,
        red: 0xD20F39, green: 0x40A02B, yellow: 0xDF8E1D, blue: 0x1E66F5, magenta: 0x8839EF, cyan: 0x179299,
        ansi: [0x5C5F77, 0xD20F39, 0x40A02B, 0xDF8E1D, 0x1E66F5, 0xEA76CB, 0x179299, 0xACB0BE,
               0x6C6F85, 0xD20F39, 0x40A02B, 0xDF8E1D, 0x1E66F5, 0xEA76CB, 0x179299, 0xBCC0CC],
    }),
    light_entry("Solarized Light", Palette {
        dark: false, bg: 0xFDF6E3, chrome: 0xEEE8D5, surface: 0xF5EFDC, fg: 0x586E75,
        muted: 0x657B83, accent: 0x268BD2,
        red: 0xDC322F, green: 0x859900, yellow: 0xB58900, blue: 0x268BD2, magenta: 0xD33682, cyan: 0x2AA198,
        ansi: [0x073642, 0xDC322F, 0x859900, 0xB58900, 0x268BD2, 0xD33682, 0x2AA198, 0xEEE8D5,
               0x002B36, 0xCB4B16, 0x586E75, 0x657B83, 0x839496, 0x6C71C4, 0x93A1A1, 0xFDF6E3],
    }),
    light_entry("Gruvbox Light", Palette {
        dark: false, bg: 0xFBF1C7, chrome: 0xEBDBB2, surface: 0xF2E5BC, fg: 0x3C3836,
        muted: 0x665C54, accent: 0x076678,
        red: 0x9D0006, green: 0x79740E, yellow: 0xB57614, blue: 0x076678, magenta: 0x8F3F71, cyan: 0x427B58,
        ansi: [0xFBF1C7, 0xCC241D, 0x98971A, 0xD79921, 0x458588, 0xB16286, 0x689D6A, 0x7C6F64,
               0x928374, 0x9D0006, 0x79740E, 0xB57614, 0x076678, 0x8F3F71, 0x427B58, 0x3C3836],
    }),
];
```

- [ ] **Step 5: Run; fix grounds only**

Run: `cargo test -p mkrew-ui --lib theme`.
Expected: PASS. If a test fails on a **fixed** token (status mark, workspace color, avatar) or on `text` itself for one theme, change only that theme's `chrome` / `surface` (closer to `bg`) or `fg` (further from `bg`) in the table and rerun; write each change down for the ADR (Task 6). Never edit `dark()` / `light()` colors; `the_mkrew_themes_never_change` stays green.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/theme.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): 12 built-in themes from palettes"
```

---

### Task 4: Resolve by slot names (`ThemeNames`)

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (`reload` ~548-561, `resolve` ~722-731, `refresh` ~1209-1218, tests ~1356-1385 and ~1455-1462)
- Modify: `crates/mkrew-ui/src/main.rs:189-194`
- Modify: `crates/mkrew-ui/src/shell/connection.rs:541-558`
- Modify: `crates/mkrew-ui/src/shell/chrome.rs:76-80,171-176`

**Interfaces:**
- Consumes: `find_theme`, `ThemeEntry::build` (Task 3); `UiSettings.light_theme/dark_theme` (Task 2).
- Produces: `pub struct ThemeNames { pub light: String, pub dark: String }` (GPUI global); `ThemeNames::from_settings(&Settings) -> Self`; `ThemeNames::slot(&self, dark: bool) -> &str`; `pub fn slot_is_dark(mode: ThemeMode, os_dark: bool) -> bool`; `pub fn os_dark(window: &Window) -> bool`; `Theme::resolve(mode: ThemeMode, os_dark: bool, names: &ThemeNames) -> Theme`; `reload(path, installed, last: &Fonts, mode: ThemeMode, names: &ThemeNames) -> Result<Option<(Fonts, ThemeMode, ThemeNames)>, String>`.

- [ ] **Step 1: Failing tests** (replace `mode_resolves_against_the_os`, add the rest)

```rust
    fn names(light: &str, dark: &str) -> ThemeNames {
        ThemeNames { light: light.into(), dark: dark.into() }
    }

    #[test]
    fn mode_resolves_against_the_os() {
        let n = ThemeNames::from_settings(&Settings::default());
        assert!(Theme::resolve(ThemeMode::System, true, &n).dark);
        assert!(!Theme::resolve(ThemeMode::System, false, &n).dark);
        assert!(!Theme::resolve(ThemeMode::Light, true, &n).dark);
        assert!(Theme::resolve(ThemeMode::Dark, false, &n).dark);
        assert_eq!(Theme::resolve(ThemeMode::Dark, false, &n).mode, ThemeMode::Dark);
    }

    #[test]
    fn default_names_resolve_to_exactly_today() {
        let n = ThemeNames::from_settings(&Settings::default());
        assert_eq!(Theme::resolve(ThemeMode::System, true, &n), Theme::dark());
        assert_eq!(Theme::resolve(ThemeMode::System, false, &n), Theme::light());
    }

    #[test]
    fn unknown_names_fall_back_to_mkrew() {
        let n = names("nope", "dracula");
        assert_eq!(Theme::resolve(ThemeMode::System, true, &n), Theme::dark());
        assert_eq!(Theme::resolve(ThemeMode::System, false, &n), Theme::light());
    }

    #[test]
    fn each_slot_picks_its_name() {
        let n = names("Solarized Light", "Dracula");
        assert_eq!(Theme::resolve(ThemeMode::Dark, false, &n).name, "Dracula");
        assert_eq!(Theme::resolve(ThemeMode::Light, true, &n).name, "Solarized Light");
        assert_eq!(Theme::resolve(ThemeMode::System, true, &n).name, "Dracula");
    }

    #[test]
    fn a_slot_may_hold_the_other_appearance() {
        let n = names("Mkrew Light", "GitHub Light");
        let theme = Theme::resolve(ThemeMode::Dark, false, &n);
        assert_eq!((theme.name, theme.dark), ("GitHub Light", false));
    }
```

Update `a_broken_settings_file_keeps_the_last_settings_and_says_why`: `let n = ThemeNames::from_settings(&Settings::default());`, pass `&n` to every `reload`, destructure `(grown, mode, _)`, expect `Ok(Some((grown.clone(), ThemeMode::Dark, n.clone())))` for the theme change, and append:

```rust
        // A new theme name alone is a change.
        std::fs::write(
            &path,
            "[ui]\ntheme = \"dark\"\ndark_theme = \"Dracula\"\n[terminal]\nfont_size = 18\n",
        )
        .unwrap();
        let dark = ThemeMode::Dark;
        assert_eq!(
            reload(&path, &[], &grown, dark, &n),
            Ok(Some((grown.clone(), dark, names("Mkrew Light", "Dracula"))))
        );
```

Run: `cargo test -p mkrew-ui --lib theme` → FAIL.

- [ ] **Step 2: Implement**

```rust
/// `[ui] light_theme` / `dark_theme` (ADR 0075), as a GPUI global read by [`refresh`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeNames {
    pub light: String,
    pub dark: String,
}

impl gpui::Global for ThemeNames {}

impl ThemeNames {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            light: settings.ui.light_theme.clone(),
            dark: settings.ui.dark_theme.clone(),
        }
    }

    /// The name in the dark (`true`) or light slot.
    pub fn slot(&self, dark: bool) -> &str {
        if dark { &self.dark } else { &self.light }
    }
}

/// Which slot `mode` shows: `System` follows the OS.
pub fn slot_is_dark(mode: ThemeMode, os_dark: bool) -> bool {
    match mode {
        ThemeMode::System => os_dark,
        ThemeMode::Light => false,
        ThemeMode::Dark => true,
    }
}

/// Whether the OS shows `window` dark.
pub fn os_dark(window: &Window) -> bool {
    matches!(
        window.appearance(),
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}
```

`Theme::resolve`:

```rust
    /// The theme of `mode`'s slot (SPEC §14, ADR 0075); an unknown name is that slot's Mkrew theme.
    pub fn resolve(mode: ThemeMode, os_dark: bool, names: &ThemeNames) -> Self {
        let dark = slot_is_dark(mode, os_dark);
        let theme = find_theme(names.slot(dark)).map_or_else(
            || if dark { Self::dark() } else { Self::light() },
            ThemeEntry::build,
        );
        Self { mode, ..theme }
    }
```

`refresh`:

```rust
pub fn refresh(mode: ThemeMode, window: &Window, cx: &mut App) {
    let fonts = cx.global::<Fonts>().clone();
    let names = cx.global::<ThemeNames>().clone();
    cx.set_global(Theme::resolve(mode, os_dark(window), &names).with_fonts(&fonts));
}
```

`reload` (doc: "The fonts, theme mode and theme names of `path` when any differs …"):

```rust
pub fn reload(
    path: &Path,
    installed: &[String],
    last: &Fonts,
    mode: ThemeMode,
    names: &ThemeNames,
) -> Result<Option<(Fonts, ThemeMode, ThemeNames)>, String> {
    let settings = mkrew_core::settings::load(path).map_err(|error| error.to_string())?;
    let fonts = Fonts::from_settings(&settings, installed);
    let new_names = ThemeNames::from_settings(&settings);
    let changed = fonts != *last || settings.ui.theme != mode || new_names != *names;
    Ok(changed.then_some((fonts, settings.ui.theme, new_names)))
}
```

- [ ] **Step 3: Callers**

`main.rs`, right after the `Fonts` global (line ~194): `cx.set_global(theme::ThemeNames::from_settings(&settings));`

`connection.rs` `apply_settings_file`:

```rust
        let mode = cx.theme().mode;
        let names = cx.global::<theme::ThemeNames>().clone();
        match theme::reload(&self.settings_file, &installed, &last, mode, &names) {
            Ok(changed) => {
                self.settings_error = None;
                if let Some((fonts, mode, names)) = changed {
                    cx.set_global(fonts);
                    cx.set_global(names);
                    theme::refresh(mode, window, cx);
                    cx.refresh_windows();
                }
```

and the comment "`theme::reload` only returns fonts and theme" → "fonts, theme mode and names".

`chrome.rs`:

```rust
        let mode = format!(
            "Theme: {} · {}",
            match theme.mode {
                theme::ThemeMode::System => "System",
                theme::ThemeMode::Light => "Light",
                theme::ThemeMode::Dark => "Dark",
            },
            theme.name
        );
```

and `tooltip(theme, &mode, …)`.

- [ ] **Step 4: Run**

Run: `cargo test -p mkrew-ui --lib theme` → PASS; `cargo build -p mkrew-ui` → OK.

- [ ] **Step 5: Smoke**

Run the UI. With no theme keys it looks unchanged. Add `dark_theme = "Dracula"` under `[ui]` of `<config>/mkrew/settings.toml` while in Dark mode (or System with a dark OS): the window switches on the next settings check. Remove the line: back to Mkrew Dark. The theme button tooltip reads `Theme: Dark · Dracula`, then `… · Mkrew Dark`.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/chrome.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): resolve the theme from light_theme / dark_theme"
```

---

### Task 5: "Theme: Select theme…" picker

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs` (`Command` ~124, `all()` ~244, `name()` ~322, `default_keys()` ~412, tests ~920)
- Modify: `crates/mkrew-ui/src/palette.rs` (`Action` ~159, `title()` ~249, `available()` ~270, new `theme_entries`, `Palette::pick_entry`)
- Modify: `crates/mkrew-ui/src/palette/tests.rs` (global command list ~289-294, action matches ~45-46 and ~450-452)
- Modify: `crates/mkrew-ui/src/settings_dialog.rs` tests (~676-686 keyboard search that expects only `ToggleTheme`)
- Modify: `crates/mkrew-ui/src/theme.rs` (`slot_key`, test)
- Modify: `crates/mkrew-ui/src/main.rs` (`struct Shell` field `theme_pick`, near `palette_return` ~627)
- Modify: `crates/mkrew-ui/src/shell/connection.rs:426-428` (init)
- Modify: `crates/mkrew-ui/src/shell/sessions.rs:352-354` (dispatch)
- Modify: `crates/mkrew-ui/src/shell/navigation.rs` (`open_theme_picker`, `preview_theme_pick`, `choose_theme`, `close_palette` ~411, `palette_key_down` ~457, `run_palette_pick` ~463, `render_palette` ~826-837)

**Interfaces:**
- Consumes: `THEMES`, `ThemeEntry::build`, `ThemeNames`, `slot_is_dark`, `os_dark`, `Fonts` (Tasks 3-4).
- Produces: `Command::SelectTheme`; `palette::Action::Theme(usize)`; `palette::theme_entries(current: &str) -> Vec<Entry>`; `Palette::pick_entry(&mut self, entry_index: usize)`; `theme::slot_key(dark: bool) -> &'static str`; `Shell.theme_pick: Option<Theme>`; `Shell::open_theme_picker`.

- [ ] **Step 1: Failing pure tests**

`palette/tests.rs`:

```rust
#[test]
fn theme_entries_list_every_theme_with_its_appearance_and_mark_the_current() {
    let entries = theme_entries("Dracula");
    assert_eq!(entries.len(), crate::theme::THEMES.len());
    assert_eq!(entries[0].action, Action::Theme(0));
    assert_eq!(entries[0].title, "Mkrew Dark");
    assert_eq!(entries[0].shortcut, "Dark");
    assert_eq!(entries[2].title, "Dracula");
    assert_eq!(entries[2].shortcut, "✓ Dark");
    assert_eq!(entries[7].shortcut, "Light");
}

#[test]
fn the_picker_starts_on_the_current_theme_and_moves_from_there() {
    let mut palette = Palette::new(theme_entries("Dracula"));
    palette.pick_entry(2);
    assert_eq!(palette.picked(), Some(Action::Theme(2)));
    palette.step(1);
    assert_eq!(palette.picked(), Some(Action::Theme(3)));
    palette.insert("solar");
    assert_eq!(palette.picked(), Some(Action::Theme(10)));
}
```

`theme.rs` tests:

```rust
    #[test]
    fn the_pick_goes_to_the_slot_shown() {
        assert_eq!(slot_key(slot_is_dark(ThemeMode::System, true)), "dark_theme");
        assert_eq!(slot_key(slot_is_dark(ThemeMode::System, false)), "light_theme");
        assert_eq!(slot_key(slot_is_dark(ThemeMode::Light, true)), "light_theme");
        assert_eq!(slot_key(slot_is_dark(ThemeMode::Dark, false)), "dark_theme");
    }
```

`keymap.rs` tests, next to `bound("ctrl-,")`: `assert_eq!(bound("ctrl-alt-j"), Some(Command::SelectTheme));` and `assert_eq!(Command::SelectTheme.context(), None);`

Run: `cargo test -p mkrew-ui --lib palette` (and `theme`, `keymap`) → FAIL.

- [ ] **Step 2: Command, palette items, slot key**

`keymap.rs`: in the enum after `ToggleTheme`:

```rust
    /// Pick a built-in theme for the slot shown (ADR 0075), saved as `[ui] dark_theme` /
    /// `light_theme`.
    SelectTheme,
```

`all()` after `Command::ToggleTheme,`: `Command::SelectTheme,`; `name()`: `Command::SelectTheme => "select_theme".into(),`; `default_keys()`: `Command::SelectTheme => "ctrl-alt-j".into(),`.

`palette.rs`: in `enum Action`:

```rust
    /// The theme picker's row for `theme::THEMES[i]` (ADR 0075).
    Theme(usize),
```

`title()`: `Command::SelectTheme => "Theme: Select theme…".into(),`; `available()`: add `| Command::SelectTheme` next to `| Command::ToggleTheme`. New function:

```rust
/// The theme picker's rows (ADR 0075): every built-in theme, its appearance on the right, the
/// shown one marked.
pub fn theme_entries(current: &str) -> Vec<Entry> {
    crate::theme::THEMES
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let look = if entry.dark { "Dark" } else { "Light" };
            Entry {
                action: Action::Theme(index),
                title: entry.name.into(),
                shortcut: if entry.name == current {
                    format!("✓ {look}")
                } else {
                    look.into()
                },
            }
        })
        .collect()
}
```

In `impl Palette`:

```rust
    /// Picks the row showing entry `entry_index` (the theme picker opens on the current theme).
    pub fn pick_entry(&mut self, entry_index: usize) {
        if let Some(row) = self.shown.iter().position(|(index, _)| *index == entry_index) {
            self.selected = row;
        }
    }
```

`theme.rs`:

```rust
/// The `[ui]` key of a theme slot.
pub fn slot_key(dark: bool) -> &'static str {
    if dark { "dark_theme" } else { "light_theme" }
}
```

Fix exhaustive matches in `palette/tests.rs` (`Action::Extra(_) | Action::GoTo(_) | Action::Theme(_) => None`; in the kinds map `Action::Theme(_) => 3`), add `Command::SelectTheme` after `Command::ToggleTheme` in the global command list (~289-294), and update the settings dialog keyboard-search test if it now also lists `Row::Binding(Command::SelectTheme)` (expect both, `Command::all()` order).

Run: `cargo test -p mkrew-ui --lib palette`, `… theme`, `… keymap`, `… settings_dialog` → PASS (the shell does not compile yet only if `run_palette_pick`'s `match` is exhaustive; add Step 3's arm before building).

- [ ] **Step 3: Shell glue**

`main.rs` `struct Shell`, after `palette_return`:

```rust
    /// The theme shown when the theme picker opened, restored on Esc (ADR 0075). `Some` while the
    /// palette is the theme picker.
    theme_pick: Option<Theme>,
```

`connection.rs` init after `palette_return: None,`: `theme_pick: None,`.

`sessions.rs` after the `ToggleTheme` arm: `Command::SelectTheme => self.open_theme_picker(window, cx),`.

`navigation.rs` — new methods in `impl Shell`:

```rust
    /// The theme picker (ADR 0075): the palette listing `theme::THEMES`, on the current one.
    pub(crate) fn open_theme_picker(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let current = cx.theme().clone();
        let mut palette = Palette::new(palette::theme_entries(current.name));
        if let Some(index) = theme::THEMES.iter().position(|e| e.name == current.name) {
            palette.pick_entry(index);
        }
        self.workspace_menu = None;
        self.palette_scroll.scroll_to_item(palette.selected);
        self.palette = Some(palette);
        self.palette_return = window.focused(cx).map(|focus| (focus, None));
        self.theme_pick = Some(current);
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    /// Shows the picked theme without saving it.
    fn preview_theme_pick(&mut self, cx: &mut Context<'_, Self>) {
        let Some(mode) = self.theme_pick.as_ref().map(|from| from.mode) else {
            return;
        };
        let Some(palette::Action::Theme(index)) = self.palette.as_ref().and_then(Palette::picked)
        else {
            return;
        };
        let fonts = cx.global::<theme::Fonts>().clone();
        let shown = Theme {
            mode,
            ..theme::THEMES[index].build()
        };
        cx.set_global(shown.with_fonts(&fonts));
        cx.refresh_windows();
    }

    /// Saves theme `index` to the slot shown and shows it (Enter or a click in the picker). A
    /// failed write keeps it for this session and says so, like the title bar toggle.
    fn choose_theme(&mut self, index: usize, window: &mut Window, cx: &mut Context<'_, Self>) {
        let name = theme::THEMES[index].name;
        let mode = cx.theme().mode;
        let dark = theme::slot_is_dark(mode, theme::os_dark(window));
        let mut names = cx.global::<theme::ThemeNames>().clone();
        if dark {
            names.dark = name.into();
        } else {
            names.light = name.into();
        }
        cx.set_global(names);
        theme::refresh(mode, window, cx);
        cx.refresh_windows();
        let value = format!("\"{name}\"");
        if let Err(error) =
            mkrew_core::settings::write_ui_value(&self.settings_file, theme::slot_key(dark), &value)
        {
            self.settings_write_failed("the theme", error, cx);
        }
    }
```

`close_palette`, first lines:

```rust
        // Esc / click outside the theme picker: back to the theme it opened on.
        if let Some(from) = self.theme_pick.take() {
            cx.set_global(from);
            cx.refresh_windows();
        }
```

`palette_key_down`: after `self.palette_scroll.scroll_to_item(palette.selected);` add `self.preview_theme_pick(cx);` (Enter / Esc returned earlier).

`run_palette_pick`, right after the `let Some(action) = … else { return; };`:

```rust
        if let palette::Action::Theme(index) = action {
            // Chosen: closing must not restore the old theme.
            self.theme_pick = None;
            self.close_palette(window, cx);
            self.choose_theme(index, window, cx);
            return;
        }
```

and in its `match action` add `palette::Action::Theme(_) => {}` (handled above).

`render_palette`: `let picking_theme = self.theme_pick.is_some();`; title child `.child(if picking_theme { "Select theme" } else { "Command palette" })`; placeholder `if picking_theme { "Type a theme name" } else { "Type a command, workspace, session or task" }`.

- [ ] **Step 4: Build, test, smoke**

Run: `cargo test -p mkrew-ui --lib` → PASS; `cargo build -p mkrew-ui` → OK.
Smoke (run the app):
1. Ctrl+Alt+J → "Select theme", 12 rows, ✓ and the pick on the current theme.
2. ↓ ↓ → window colors change live; Esc → exactly the previous colors (compare a screenshot of Terminals before/after).
3. Reopen, type `drac`, Enter → Dracula stays; `settings.toml` gained only `dark_theme = "Dracula"` (Dark / System with dark OS); nothing else in the file changed.
4. Title bar tooltip `Theme: … · Dracula`; Ctrl+Shift+J still cycles System → Light → Dark.
5. Ctrl+Shift+P → "Command palette", no theme rows; "Theme: Select theme…" is listed with `Ctrl+Alt+J`.
6. Settings (Ctrl+,) → Appearance → "Dark mode theme" shows Dracula; picking One Dark there switches the window.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette/tests.rs crates/mkrew-ui/src/settings_dialog.rs crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/sessions.rs crates/mkrew-ui/src/shell/navigation.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Select theme picker with live preview"
```

---

### Task 6: ADR, SPEC, inbox, full verify

**Files:**
- Create: `docs/decisions/0075-theme-picker.md`
- Modify: `docs/SPEC.md` (§13 table near line 1259; §14 "Theme:" bullet near line 1480; §20.0 `[ui]` block near lines 1787-1790)
- Modify: `docs/INBOX.md`, `docs/HUMAN_CHECKS.md`

- [ ] **Step 1: ADR** — create `docs/decisions/0075-theme-picker.md`:

```markdown
# 0075 — Theme picker and built-in themes

- Date: 2026-10-25
- Status: accepted
- Amends: 0027 (theme modes)

## Context

The user asked to change themes like VS Code / Zed. Mkrew had two palettes (dark, light) and a
System / Light / Dark mode.

## Options

1. Built-in themes only, each from ~20 palette colors, the rest derived. (Chosen.)
2. Every theme written token by token (~85 tokens × 10): long, and drifts when a token is added.
3. Theme files (TOML) or VS Code / Zed imports: not asked for (YAGNI).

## Decision

- 12 themes (`theme::THEMES`; names in `mkrew_core::settings::THEME_NAMES`). Mkrew Dark / Light
  are `Theme::dark()` / `Theme::light()` unchanged (golden snapshot test).
- `[ui] light_theme` / `dark_theme` name the theme of each slot (Zed's model); `[ui] theme` still
  picks the slot. Unknown names fall back to the slot's Mkrew theme.
- `Theme::from_palette` derives the tokens; UI text colors are blended toward the text color until
  they read ≥ 4.5:1 (`readable`), so some published colors come out lighter / darker. Terminal
  ANSI colors are as published. Status, workspace and QR colors and the classic sidebar palette
  stay Mkrew's.
- "Theme: Select theme…" (`Ctrl+Alt+J`; the keymap has no chords) reuses the command palette with
  live preview; Enter or a click saves to the slot shown, Esc restores.
- Settings → Appearance lists both slots (`Kind::List`, the font pick generalised).

## Consequences

- Programs drawing 24-bit colors (Claude Code's and omp's own themes) keep their colors; ANSI 0–15
  and the default fg/bg follow the theme.
- Not done: answering OSC 10/11 color queries (INBOX).
- Palette changes made for contrast tests: none.
```

If Task 3 Step 5 changed any palette value, replace "none" with the list (theme, field, old → new).

- [ ] **Step 2: SPEC**

§13 table, after the Toggle theme row: `| Global | Select theme | Ctrl+Alt+J |`.

§14, replace the "Theme:" bullet with:

```markdown
- Theme: System (follows the OS live), Light or Dark — `[ui] theme`, cycled by the title bar toggle
  (ADR 0027). Each slot shows a built-in theme (ADR 0075): `[ui] light_theme` (default Mkrew Light;
  One Light, GitHub Light, Catppuccin Latte, Solarized Light, Gruvbox Light) and `[ui] dark_theme`
  (default Mkrew Dark; One Dark, Dracula, Catppuccin Mocha, Tokyo Night, Gruvbox Dark); an unknown
  name shows the Mkrew theme. "Theme: Select theme…" (Ctrl+Alt+J) previews while moving, Enter saves
  to the slot shown, Esc restores. In terminals the default fg/bg and ANSI 0–15 follow the theme;
  256-color 16–255 and 24-bit colors are drawn as the program sends them.
```

§20.0 `[ui]` block, after the `theme = …` line:

```toml
light_theme = "Mkrew Light" # §14; a built-in theme name, shown in Light mode / a light OS
dark_theme = "Mkrew Dark"   # §14; shown in Dark mode / a dark OS
```

- [ ] **Step 3: INBOX / HUMAN_CHECKS**

`docs/INBOX.md`, new item in the file's existing item format:
`- [ ] **Answer OSC 10/11 color queries** [enh] — the daemon's emulator answers background / foreground queries with the UI theme's colors, so omp / Claude Code can detect a light background (the theme colors must reach the daemon; out of scope of ADR 0075).`

`docs/HUMAN_CHECKS.md` → "To verify":
`- Check the 10 new themes visually (Terminals, Git, Files, dialogs) via Ctrl+Alt+J; Mkrew Dark / Light must look unchanged (ADR 0075).`

- [ ] **Step 4: Full verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` → fmt check, clippy `-D warnings`, all tests PASS. On failure fix and rerun (`cargo fmt --all` for formatting).

- [ ] **Step 5: Commit**

```bash
git add docs/decisions/0075-theme-picker.md docs/SPEC.md docs/INBOX.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "docs: ADR 0075 theme picker, SPEC §13 §14 §20.0"
```
