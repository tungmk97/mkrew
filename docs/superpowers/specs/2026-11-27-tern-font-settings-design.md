# Tern-style font settings and bundled Ioskeley Mono Term

Date: 2026-11-27.
ADR: 0120 (to be written with the implementation).

## Intent

The human wants Mkrew's Appearance fonts to work like Tern's (screenshot in the request):
a bundled default terminal font, fallback lists, separate bold / italic / bold italic families,
a terminal font weight, a code font for monospace text outside the grid, a UI font list,
size steppers beside the Font and UI font fields, and Ctrl+Shift+= / Ctrl+Shift+- to zoom both sizes.

Success: a fresh install draws the terminal in Ioskeley Mono Term with Medium bold on every OS,
every setting below applies live from `settings.toml` and the Settings dialog,
and existing `settings.toml` files keep loading unchanged.

## Bundled font

- Add `font/ioskeley-mono/` with four faces of Ioskeley Mono v2.2.0 (`IoskeleyMono-Term.zip`, `Normal/Hinted/`):
  `IoskeleyMonoTerm-Regular.ttf`, `-Italic.ttf`, `-Medium.ttf`, `-MediumItalic.ttf` (about 2.1 MB each, 8.6 MB total).
- Add the OFL-1.1 text as `font/ioskeley-mono/LICENSE` (from github.com/ahatem/IoskeleyMono).
- Append the four faces to `theme::BUNDLED_FONTS`; they register with gpui before the first window like Geist (ADR 0116).
- Family names in the files (name ID 1 / ID 16):
  Regular and Italic are `Ioskeley Mono Term`;
  Medium and MediumItalic are legacy family `Ioskeley Mono Term Medium`, typographic family `Ioskeley Mono Term` weight 500.
- `TERMINAL_FONTS` becomes `["Ioskeley Mono Term"]` on every platform; the per-platform lists go.

## Settings schema

All new keys default to empty / 400, so old files load unchanged (`deny_unknown_fields` stays).

|Key|Type|Default|Meaning|
|---|---|---|---|
|`[terminal] font_family`|string, comma list|`""`|Grid family; the first installed entry wins; empty or none installed: `Ioskeley Mono Term`.|
|`[terminal] bold_font_family`|string, comma list|`""`|Bold cells; empty: `Ioskeley Mono Term Medium` when Font is empty, else Font's family.|
|`[terminal] italic_font_family`|string, comma list|`""`|Italic cells; empty: Font's family.|
|`[terminal] bold_italic_font_family`|string, comma list|`""`|Bold italic cells; empty: same rule as bold.|
|`[terminal] font_weight`|integer 100-900|`400`|Regular cell weight; bold cells draw `max(weight + 300, 700)`, capped at 900.|
|`[terminal] font_size`|8-32|`13`|Unchanged: grid, and code in editors, diffs and chats.|
|`[ui] font_family`|string, comma list|`""`|UI family; first installed wins; empty or none installed: Geist, else the OS UI font.|
|`[ui] code_font_family`|string, comma list|`""`|Monospace text outside the grid; empty: the Files editor follows `[terminal] font_family`, everything else uses `Ioskeley Mono Term`.|
|`[ui] font_size`|8-32|`13`|Unchanged.|

A comma list is split on `,`, each entry trimmed, empty entries dropped.
Resolution of every list is one shared function in `theme.rs`: first entry found in `all_font_names()`, else the default.
An entry that is not installed is skipped silently (today's rule for `[terminal] font_family`).

Open check for the implementation: whether gpui's DirectWrite backend resolves the legacy family name `Ioskeley Mono Term Medium`.
If it does not, the bold default becomes family `Ioskeley Mono Term` at weight 500 for the default case, and the placeholder text stays the same.

## Rendering

- `theme::Fonts` gains `terminal_bold`, `terminal_italic`, `terminal_bold_italic` (resolved families), `terminal_weight: f32` and `code_family`; `Theme::with_fonts` copies them.
- `terminal/view.rs::text_run` picks the family by the cell's bold / italic flags, sets weight to `terminal_weight` or the bold weight, and keeps `FontStyle::Italic` for italic cells.
  Every variant keeps `GLYPH_FALLBACKS`.
- Cell metrics stay measured from the regular font (ADR 0099); bold and italic faces are drawn into the same cells (`force_width` already does this).
- TSP (`shell/tsp_view.rs`) keeps the grid font and uses the same bold rule.
- Code font consumers: the files listed by the scout (preview, chat_view, composer, dialogs, git_diff, git_render, git_rows, inspector, jira, peek, run, send_dialog, sidebar, slash, thread_panel, widgets, worktrees, tsp_view/picker, terminal/view overlays) switch from `terminal_font_family` to `code_font_family`.
  `file_pane.rs` (the editor) uses `editor_font_family`: the code font when set, else the grid font.
  Sizes keep using `terminal_font_size`.

## Settings dialog

- `Kind::Font` rows become free-text rows with a placeholder showing the effective default (as in the screenshot); the installed-font dropdown and its list code are removed.
- Appearance > Fonts order and labels follow the screenshot:
  Font (with size stepper), Bold font, Italic font, Bold italic font, Font weight (stepper), Code font, UI font (with size stepper).
- New stepper widget (`−  N  +`) in `shell/widgets.rs`, theme tokens only; it serves both size rows and Font weight (step 1 for sizes, 100 for weight).
- A row can carry its size stepper beside the text field: the size settings render inside the Font and UI font rows instead of as their own rows.
- Help texts are written for Mkrew (no Tern wording copied blindly), short, one sentence per idea.

## Zoom keys

- New commands `FontSizeUp` / `FontSizeDown` bound to Ctrl+Shift+= and Ctrl+Shift+- (Cmd on macOS), listed in the keymap and the app menu's View group.
- Each press adds or removes 1 px from both `[terminal] font_size` and `[ui] font_size` (each clamped 8-32) and writes them with the existing `write_ui_value`-style writer, so the live reload applies them.

## Testing

- Unit tests in `theme.rs`: list splitting and first-installed resolution; bold / italic defaults with empty and non-empty Font; bold weight formula (400 -> 700, 500 -> 800, 700 -> 900 cap).
- `mkrew-core` settings tests: new keys parse, defaults, an old file without them still loads, the commented template lists them.
- Settings dialog tests: Font rows are text rows; stepper changes the value within bounds.
- Keymap test: both zoom keys bound, each write clamps.
- Theme snapshots updated (`cargo insta`).
- UI proof: `scripts/smoke.ps1` screenshots of a terminal with bold / italic output and of the Appearance section, dark and light; `ui-reviewer` subagent pass.

## Docs

- ADR 0120: bundled Ioskeley Mono Term, font lists, weight rule, code font split, removal of the font dropdown.
- SPEC §14 (fonts) and §20.0 (`settings.toml` template and dialog rows) updated in the same commit as the code.

## Not in scope

- Remaining list entries as per-glyph fallbacks (only the first installed entry is used).
- SemiCondensed width, ligature settings, other Ioskeley weights.
