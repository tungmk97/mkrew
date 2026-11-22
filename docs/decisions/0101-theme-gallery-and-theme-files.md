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
  A name clash ignores ASCII case: a file named `nord` or `NORD` clashes with the built-in Nord.
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

  That is six themes changed: Solarized Dark, GitHub Dark, Everforest Dark, Everforest Light, Ayu Light and Tokyo Night Day.
  Nord, Rosé Pine, Kanagawa and Rosé Pine Dawn use the published values; Nord, Solarized Dark and Rosé Pine have a `bg` equal to or very near their `surface` as their projects publish a flat window.
