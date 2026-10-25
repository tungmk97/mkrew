# Theme picker — design

- Date: 2026-10-25
- Status: approved in chat, awaiting spec review
- ADR to write with the implementation: `docs/decisions/0075-theme-picker.md`

## Goal

Pick a named color theme the way VS Code / Zed do: a list of built-in themes, live preview
while moving through it, Enter saves, Esc restores. System mode keeps following the OS, with one
chosen theme for light and one for dark (Zed's model).

## Hard invariant: the default look does not change

`Mkrew Dark` and `Mkrew Light` are the current `Theme::dark()` and `Theme::light()`, token for
token (UI, diff, syntax, `terminal_bg`, ANSI 0–15, status, workspace colors, `classic`). A user
with no new keys in `settings.toml` sees exactly what they see today, in Mkrew and inside every
terminal (omp, Claude Code, shells). Enforced by:

- `dark()` / `light()` are not edited and do not go through `from_palette`.
- An `insta` snapshot of `Theme::dark()` and `Theme::light()` (Debug output) is taken **before**
  any refactor and must stay byte-identical.
- `resolve` with missing, unknown or default theme names equals `Theme::dark()` / `Theme::light()`
  (`PartialEq`, mode aside).

## Scope

In:
- 12 built-in themes (below), selected per appearance slot.
- `[ui] light_theme`, `[ui] dark_theme` keys.
- Command palette picker with live preview.
- Settings → Appearance rows for both keys.
- Title bar tooltip shows the theme name.

Out (YAGNI; may come later):
- User theme files, importing VS Code / Zed themes.
- Answering OSC 10/11 color queries (so omp / Claude Code could detect a light background):
  the PTY is in the daemon, the theme in the UI. Recorded as an item in `docs/INBOX.md`.
- Per-theme status colors (`status_*` stay Mkrew's, they carry meaning).

## Built-in themes

| Dark | Light |
| --- | --- |
| Mkrew Dark (default) | Mkrew Light (default) |
| One Dark | One Light |
| Dracula | GitHub Light |
| Catppuccin Mocha | Catppuccin Latte |
| Tokyo Night | Solarized Light |
| Gruvbox Dark | Gruvbox Light |

Palettes come from each theme's official published colors (all MIT or similar). UI text colors
are made readable by `readable()` (see Theme construction); the terminal `ansi` stays as published.

## What a theme changes in the terminal

Recorded so the behavior is not mistaken for a bug (`crates/mkrew-ui/src/terminal/layout.rs`):

| A program writes | Follows the theme |
| --- | --- |
| Default fg / bg | yes (`text`, `terminal_bg`) |
| ANSI 0–15 (incl. dim) | yes (`ansi[i]`) |
| 256-color 16–255 | no (fixed xterm cube / grays) |
| 24-bit RGB | no (drawn as sent) |

Claude Code's and omp's own themes use RGB, so they keep their colors; Claude Code's
"ANSI colors only" themes follow the Mkrew theme.

## Settings (`mkrew-core`)

```toml
[ui]
theme = "system"             # unchanged: "system" | "light" | "dark"
light_theme = "Mkrew Light"  # used in Light mode, and in System mode while the OS is light
dark_theme = "Mkrew Dark"    # used in Dark mode, and in System mode while the OS is dark
```

- `UiSettings` gains `light_theme: String`, `dark_theme: String` (serde default = the Mkrew
  names). Any string parses; an unknown name is not an error, the UI falls back to the Mkrew
  theme of that slot (like a font that is not installed).
- `pub const THEME_NAMES: &[&str]` in `settings.rs`: the 12 names in table order (dark column then
  light column). The UI's theme list is tested against it.
- Written with the existing `write_ui_value` (rest of the file kept byte for byte). The keys are
  written only when the user picks a theme.
- `settings_fields.rs`: two rows on `Appearance` right after `theme`:
  `light_theme` "Light mode theme" ("Used in Light mode and when the OS is light."),
  `dark_theme` "Dark mode theme" ("Used in Dark mode and when the OS is dark."), kind
  `Kind::List(THEME_NAMES)` (new, see Settings dialog).
- Settings template (`settings.rs` template string) and SPEC §20.0 list both keys commented at
  their defaults.

## Theme construction (`crates/mkrew-ui/src/theme.rs`)

```rust
pub struct Palette {
    pub dark: bool,
    pub bg: u32,        // main area, terminal_bg
    pub chrome: u32,    // sidebar, title bar, tab strip
    pub surface: u32,   // panels, popovers, inputs, cards
    pub fg: u32,        // text
    pub muted: u32,     // text_muted / comments
    pub accent: u32,
    // Readable hues for UI text roles (diff, syntax, git letters, status-like text).
    pub red: u32, pub green: u32, pub yellow: u32, pub blue: u32, pub magenta: u32, pub cyan: u32,
    pub ansi: [u32; 16], // the terminal palette, as published by the theme
}

pub struct ThemeEntry { pub name: &'static str, pub dark: bool, palette: Option<Palette> }
pub const THEMES: &[ThemeEntry];   // order = THEME_NAMES
impl ThemeEntry { pub fn build(&self) -> Theme }
impl Theme { pub fn from_palette(name: &'static str, p: &Palette) -> Theme }
pub fn find_theme(name: &str) -> Option<&'static ThemeEntry>
```

- The two Mkrew entries have `palette: None`; `build` returns `Theme::dark()` / `Theme::light()`.
- `from_palette` starts from `Theme::dark()` or `Theme::light()` (by `p.dark`) and overwrites the
  color tokens; everything not listed below stays the base's value (`status_*`,
  `workspace_colors`, `qr_*`, `tint_alpha`, `scrim`, `classic`, fonts, sizes). `mix(a, b, t)` is
  a linear RGB blend of `a` toward `b` by `t`.
- **Readable by construction:** every token used as text (marked *R*) goes through
  `readable(color, toward = fg, grounds, 4.5)`: blended toward `fg` in 5 % steps until it reads
  ≥ 4.5:1 on every ground (`bg`, `chrome`, `surface`, `surface_raised`, `hover`, every diff fill).
  Fills that carry `fg` text (selection, find matches) lower their mix in 0.05 steps until `fg`
  reads ≥ 4.5:1 on them. So a published color may come out slightly lighter/darker; the ADR says so.

| Tokens | From |
| --- | --- |
| `bg`, `terminal_bg` | `bg` |
| `sidebar`, `chrome` | `chrome` |
| `surface`, `card` | `surface` |
| `surface_raised`, `selected` | `mix(surface, fg, 0.10)` |
| `hover`, `row_hover`, `well`, `diff_hunk_header` | `mix(bg, fg, 0.06)` |
| `border`, `control_border` | `mix(bg, fg, 0.14)` |
| `divider` | `mix(bg, fg, 0.09)` |
| `selected_ring` | `mix(accent, bg, 0.50)` |
| `text` | `fg` |
| `text_dim` *R* | `mix(fg, bg, 0.20)` |
| `text_muted`, `text_subtle`, `text_disabled` *R* | `muted` |
| `subtle` | `mix(muted, bg, 0.35)` |
| `accent` | `accent` |
| `accent_text` *R* | `accent` |
| `info`, `info_soft` / `info_text` *R* | `blue` |
| `accent_bg` | `mix(bg, accent, 0.14)` |
| `pane_focus_border` | `mix(accent, bg, 0.50)` |
| `on_accent` | `bg` or `fg`, whichever has the higher contrast on `accent` |
| `terminal_selection_bg` | `mix(bg, accent, ≤ 0.30)` (fg-readable) |
| `terminal_match_bg` / `_current_bg` | `mix(bg, yellow, ≤ 0.20)` / `mix(bg, yellow, ≤ 0.50)` (fg-readable) |
| `warning_bg` / `success_bg` / `danger_bg` | `mix(bg, yellow / green / red, 0.14)` |
| `run_pill_bg` / `_border` / `_text` *R* | `mix(bg, green, 0.12)` / `mix(bg, green, 0.35)` / `green` |
| `diff_add_bg` / `diff_del_bg` / `diff_mod_bg` | `mix(bg, green / red / yellow, 0.14)` |
| `diff_*_gutter` | same hue at 0.24 |
| `diff_*_word_bg` | same hue at 0.30 |
| `diff_add_text`, `success_text` / `diff_del_text`, `danger_text` / `diff_mod_text` *R* | `green` / `red` / `yellow` |
| `git_modified`, `git_untracked`, `syntax_number` *R* | `yellow` |
| `syntax_keyword` / `string` / `type` / `function` *R* | `magenta` / `green` / `cyan` / `blue` |
| `teal` / `folder` *R* | `cyan` / `blue` |
| `ansi` | `ansi` |
| `dark` | `p.dark` |

- A fixed token (`status_*`, workspace colors) that fails a contrast test on a new theme's
  grounds is fixed by adjusting that theme's `Palette` grounds, never the fixed token.
- Names: `ThemeNames { light: String, dark: String }`, a GPUI global like `Fonts`, built from
  `[ui] light_theme` / `dark_theme`. `slot_is_dark(mode, os_dark)`: `System` → `os_dark`,
  `Light` → false, `Dark` → true.
- `Theme::resolve(mode, os_dark)` becomes `Theme::resolve(mode, os_dark, &ThemeNames)`: look up
  the slot's name with `find_theme`, fall back to `Theme::light()` / `Theme::dark()` of that slot,
  set `mode`. A slot may hold a theme of the other appearance (Zed allows it); `Theme.dark`
  follows the theme, so Mermaid images pick the right variant.
- `theme::refresh(mode, window, cx)` keeps its signature and reads the `ThemeNames` global.
  `theme::reload` takes and returns `ThemeNames` too (its "changed?" test includes them).
  Startup (`main.rs`) sets the global; `apply_settings_file` updates it.
- `Theme` gains `name: &'static str` (for the ✓ in the picker and the tooltip).

## Picker (command palette)

- New `Command::SelectTheme`: name `select_theme`, default key `ctrl-alt-j` (free; the keymap has
  no chords, so VS Code's `Ctrl+K Ctrl+T` is not possible), palette title "Theme: Select theme…".
- Reuses `palette::Palette` (fuzzy filter, Up/Down, click). New `palette::Action::Theme(usize)`
  (index into `THEMES`). Entries: 12 rows "`<name>`" with a meta label `Dark` / `Light`; the
  current theme is the initial pick and shows ✓.
- The shell keeps the `Theme` shown when the picker opened (`theme_pick: Option<Theme>`).
- Moving the pick (Up/Down, typing that changes the best match): apply that theme to the `Theme`
  global (no file write) and refresh windows.
- Enter, or a click on a row: write the picked name to the slot shown now
  (`slot_is_dark(mode, os_dark)` → `dark_theme`, else `light_theme`) via `write_ui_value`, set the
  `ThemeNames` global, close. A write error keeps the theme for this session and shows the
  existing "not saved" toast (`settings_write_failed`), like the title bar toggle.
- Esc / click outside: restore the `Theme` from when it opened; close.
- The title reads "Select theme", the query placeholder "Type a theme name".
- The settings reload after the write is a no-op (names already applied).

## Settings dialog

- New `Kind::List(&'static [&'static str])`: edited by the same searchable pick list as
  `Kind::Font` (`FontPick` generalised to a list pick: the item source is the installed fonts for
  `Font`, the static list for `List`). Picking writes the name. No live preview here (the picker
  is the preview path).
- `Kind::Choice` rendering is unchanged.

## Title bar

- Toggle and `Ctrl+Shift+J` unchanged (System → Light → Dark).
- Tooltip: `Theme: System · One Dark` (mode, then the resolved theme name).

## Tests

`crates/mkrew-ui/src/theme.rs`:
- Golden snapshot of `Theme::dark()` and `Theme::light()` (taken first, unchanged after).
- `resolve` with default / missing / unknown names == `Theme::dark()` / `Theme::light()`.
- Over every `THEMES` entry: the existing contrast checks (body text and `syntax_*` ≥ 4.5:1 on
  `bg`, `surface`, `terminal_bg`; `text_muted` ≥ 4.5:1 on `bg`; `on_accent` ≥ 3:1 on `accent`;
  workspace colors ≥ 3:1 per ADR 0020), `entry.dark == build().dark`, names unique and equal to
  `THEME_NAMES` in order.
- A light theme in the dark slot resolves with `dark == false`.

`crates/mkrew-core`:
- `light_theme` / `dark_theme` parse, default to the Mkrew names, accept unknown strings.
- `write_ui_value` writes them keeping comments and other lines.
- `settings_fields` template check covers the two rows.

`crates/mkrew-ui` picker logic (pure, no GPUI): open → pick is current theme; move → preview
theme changes; Esc → original; Enter → write to the slot shown (dark and light cases).

Keymap tests: `select_theme` has a default, no duplicate keystroke.

## Visual check

Screenshots of Terminals and Git in Mkrew Dark and Mkrew Light before and after must match.
New themes: screenshots if automatic capture works, else "check the 10 new themes visually"
in `docs/HUMAN_CHECKS.md`.

## Docs

- ADR 0075-theme-picker (this design, contrast nudges).
- SPEC §13 (Select theme, `Ctrl+Alt+J`), §14 (theme list, slot rule, what follows the theme in the
  terminal), §20.0 (`light_theme`, `dark_theme`).
- `docs/INBOX.md`: OSC 10/11 answers (out of scope here).
