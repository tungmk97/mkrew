# Theme gallery - Design

- Date: 2026-11-22
- Status: approved under the user's standing approval of the controller's recommendations (2026-11-22, "assume OK with your recommendations"); sub-project 2 of the Tern-inspired update (Polish was sub-project 1).
- ADR: `docs/decisions/0101-theme-gallery-and-theme-files.md` (written by the plan; amends ADR 0078, which rejected theme files as not asked for).
- Inspiration: Tern's Settings > Appearance, a gallery of 20+ themes as cards with color swatches; hovering a card previews it live, a click keeps it, and themes are plain files an agent can edit, applied on save.
- Numbers checked against every branch: ADR 0098-0100 are taken by the macOS plans on master, so this is 0101; `PROTOCOL_VERSION` does not change (no daemon work).

## 1. Goal

The user picks a theme by seeing it, not by reading names in a list.
Mkrew ships more themes, and a user (or an agent) can add one by saving a small file.

Done means:

- Settings > Appearance opens with a gallery of every theme, grouped Dark and Light.
- Hovering a card (or focusing it with the keyboard) shows the whole app in that theme; leaving the gallery or closing Settings puts the kept theme back.
- A click (or Enter) keeps the theme for its slot.
- At least 22 built-in themes.
- A valid TOML file saved in `<config>/themes/` shows up in the gallery, the theme picker and the slot lists within a second; an invalid file shows its error on a card and never breaks the app.

## 2. What the user decided (standing approval) and what the controller chose

| Topic | Decision |
| --- | --- |
| Place | The top of Settings > Appearance, above the existing Theme card; the Mode segmented control (System / Light / Dark) and the "Light mode theme" / "Dark mode theme" rows stay. |
| Card | A small picture of a window in that theme, its name, and badges. |
| Groups | Two overlines: "Dark" and "Light"; built-ins in table order, then file themes by name. |
| Hover | Preview after `THEME_PREVIEW_DELAY` (120 ms) so moving across cards does not flash; leaving the gallery, closing Settings or pressing Esc restores. |
| Click | Keeps the theme in its own slot (`dark_theme` for a dark theme, `light_theme` for a light one); the mode does not change. |
| New built-ins | 10 more: Nord, Solarized Dark, GitHub Dark, Rosé Pine, Kanagawa, Everforest Dark (dark); Rosé Pine Dawn, Everforest Light, Ayu Light, Tokyo Night Day (light). |
| Theme files | `<config>/themes/*.toml`, the same fields as the built-in `Palette`. |
| Bad contrast in a file | Loaded, with a warning badge on its card; the user's file is the user's call. |
| Parse error | Not loaded; an error card shows the file name and the message. |
| Name taken | A file whose name equals a built-in or an earlier file is skipped with an error card. |
| Actions | "New theme from current" and "Open themes folder" under the gallery. |

## 3. The gallery

### 3.1 Layout

The gallery is a section of the Appearance page, drawn before the settings cards, in the same way the Prompts and Machines pages draw custom content (`shell/settings.rs`, `Tab::Settings(Page::Appearance)`, only while no search query is active).
It has the overline "Themes", then a "Dark" group and a "Light" group.
Each group is a wrapping grid of cards, `THEME_CARD_WIDTH` wide (token), with `space::M` gaps; at the default Settings width that is three per row.

A card is one `card_press` element (Polish P3) with:

- the picture: a rounded box (`radius::MEDIUM`) filled with the theme's `bg`, a `chrome` strip on top, a `sidebar` strip on the left, two short text bars in `text` and `text_muted`, an `accent` pill, and a row of six dots (red, green, yellow, blue, magenta, cyan from its ANSI colors);
- the name in `font_size::BODY`;
- badges in `font_size::META`: "Dark mode" / "Light mode" when it is the kept theme of that slot, "File" for a file theme, "Low contrast" (warning color) when the file theme has problems.

The picture is drawn from the card's own theme, so every card shows its own colors whatever theme the app has.

An error card has no picture: the file name, the error in `danger_text`, and no press look (it is not clickable).

Under the groups, a muted line with the themes folder path and two quiet buttons: "New theme from current" and "Open themes folder".

### 3.2 Preview and keep

- Pointer over a card for `THEME_PREVIEW_DELAY`: the app shows that theme (the GPUI theme global is replaced, as the theme picker does today; nothing is written).
- Pointer leaves the gallery, or Settings closes, or Esc: the kept theme comes back.
- Keyboard: Tab reaches the gallery; arrow keys move between cards in reading order; the focused card previews (no delay); Enter keeps; Esc restores and then works as today.
- Click or Enter: writes the slot key (`dark_theme` or `light_theme`) with `write_ui_value`, as the picker does; the preview becomes the kept theme only when the shown appearance uses that slot; otherwise the kept theme of the shown slot comes back and the card gets its slot badge.
- A preview never survives a restart and never writes `settings.toml`.

### 3.3 "New theme from current"

Writes `<config>/themes/<name> copy.toml` (numbered `copy 2`, `copy 3` when taken) with the palette of the theme shown now and `name = "<name> copy"`, then opens it in Zed through the existing "open in Zed" path.
For Mkrew Dark / Light, which are written token by token, the file takes `bg`, `chrome`, `surface`, `text`, `text_muted`, `accent` and the ANSI colors from the theme.

"Open themes folder" creates the folder when missing and opens it in the OS file manager, like "Open in File Explorer".

## 4. Theme files

### 4.1 Format

```toml
name = "My Theme"          # shown in the gallery and stored in [ui] dark_theme / light_theme
dark = true                # which slot and group
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
```

- Every key is required; colors are `#RRGGBB` (case-insensitive).
- Unknown keys are an error (a typo should not pass silently).
- The theme is built by `Theme::from_palette`, the same derivation as the built-ins, so derived tokens (hover, borders, glass, readable text) follow the same rules.
- Status, workspace, QR and classic-sidebar colors stay Mkrew's, as for the built-in derived themes (ADR 0078).
- Diagrams keep their Mkrew colors (ADR 0021), as they do for every theme today.

### 4.2 Loading and reload

- At start the UI reads `<config>/themes/*.toml` (`MKREW_CONFIG_DIR` respected; the folder is not created until an action needs it).
- The existing 500 ms settings poll (`shell/connection.rs`, `ponytail:` comment) also checks the folder: the list of `.toml` files and their modified times.
  When anything changed, the catalog is rebuilt; if the kept theme of the shown slot came from a changed file, it is applied again at once.
- A file that disappears falls back the same way an unknown name does today (Mkrew Dark / Light for the slot).
- The poll stays a poll; a `ponytail:` comment names the ceiling (a folder with hundreds of files) and the upgrade path (`notify`).

### 4.3 Problems

`theme::problems(&Theme) -> Vec<Problem>` checks the contrast rules the theme tests check today (text on glass, selected text, on-accent, code, toast, highlighted diff lines, change bars, diff colors).
The tests call it for every built-in and assert it is empty; the loader calls it for a file theme and shows "Low contrast" with the first problem in a tooltip.

## 5. The theme catalog

- `ThemeCatalog` (a GPUI global, `crates/mkrew-ui/src/theme.rs` or a new `theme_catalog.rs`) holds the built-ins followed by the file themes, plus the error entries.
- Everything that lists or finds themes uses it: `Theme::resolve`, the theme picker (`palette::theme_entries`), the Settings slot lists, the gallery.
- `Theme::name` becomes an owned `SharedString` (file names are not `'static`).
- `THEMES` stays the compiled table; `mkrew_core::settings::THEME_NAMES` stays the built-in list for the settings template and its test; the Settings "Light mode theme" / "Dark mode theme" rows read their options from the catalog: every built-in (a slot may hold the other appearance, as today) plus the file themes of that slot's own appearance.
- Core changes only by `Paths::themes_dir()` and the longer `THEME_NAMES`: `[ui] light_theme` / `dark_theme` already accept any string, and an unknown name falls back in the UI.

## 6. New built-in themes

Each palette comes from the theme project's own published colors; the plan cites the source URL in a comment next to each new entry (the existing entries cite none).
Each one must pass `problems()` with no entry; where a published UI color fails, the derivation's `readable` blending applies as for the existing themes, and ADR 0101 lists any palette change made for contrast (as ADR 0078 does).
`THEME_NAMES` and `THEMES` grow to 22 in the same order, dark themes first, then light ones (the six new dark themes after Gruvbox Dark, so Mkrew Light moves to index 12; tests that pin index 6 follow).
When a published color fails a rule, the smallest change that passes is made and listed in ADR 0101.

## 7. Error handling

- Unreadable folder or file: one error card naming the path and the OS error; the rest of the gallery works.
- Parse error: error card with the TOML error (line and message).
- Name clash: error card "Name taken by <built-in or file>".
- Writing a slot fails: the existing toast path for `write_ui_value` errors.
- "New theme from current" fails to write: a toast with the error; nothing opens.
- Preview while Settings closes because of an outside event (window close, palette): the existing restore path (`restore_theme_pick`) also restores a gallery preview.

## 8. Testing

Model and pure tests only; no test reads source text; no network.

| Area | Behaviour tests |
| --- | --- |
| Theme file parse | A valid file gives the palette; a missing key, an unknown key, a bad color, a 15-color `ansi` each give an error naming the key. |
| Catalog | Built-ins first in table order, then files by name; a file named like a built-in or an earlier file becomes an error entry; a removed file leaves the catalog; dark / light filtering for the slot lists. |
| Problems | Empty for every built-in (replaces the per-rule loops where they duplicate); a deliberately bad palette reports the rule it breaks. |
| Gallery model | Hover after the delay previews, leaving restores, Enter keeps and writes the right slot key, Esc restores; arrow keys move in reading order across the two groups; an error card is skipped by the keyboard and not clickable. |
| New theme from current | The file name numbering (`copy`, `copy 2`); the written file parses back to the same palette. |
| Snapshots | `mkrew_dark` / `mkrew_light` unchanged. |

Visual checks (dark and light) go to `docs/HUMAN_CHECKS.md`: the gallery layout, a card picture against its theme, hover preview without flashing, a broken file's card, live reload after saving a file in Zed.

## 9. Docs

- ADR 0101 (decision, options, consequences; the new built-ins and any palette change for contrast).
- SPEC §14 theme bullet (gallery, preview, files), §20.0 `[ui]` notes (themes folder), §13 unchanged (no new keys).
- README: one line on theme files.
- `docs/HUMAN_CHECKS.md`: the visual checks above.

## 9a. Details settled while planning

- `problems()` covers every contrast rule the theme tests check today, so a file theme is held to the same bar as a built-in.
- An editor that saves in two steps can be caught half-written for one poll: the card shows an error and the kept theme falls back to the slot's Mkrew theme until the next poll; no debounce.
- Arrow keys: Left / Up move to the previous card, Right / Down to the next, in reading order across both groups; no column-aware movement.
- A broken `settings.toml`: previews still work; a click writes nothing and shows the existing settings-error notice.
- Safety: a theme name is non-empty and has no control characters; a `*.toml` over 64 KB is an error card; a slot value is written as an escaped TOML string; "New theme from current" creates its file with `create_new`, so it never overwrites.

## 10. Out of scope

- Importing VS Code / Zed / Tern theme files.
- Editing a theme inside Mkrew (the file opens in Zed).
- Per-workspace themes.
- Answering OSC 10/11 color queries (still in INBOX).
