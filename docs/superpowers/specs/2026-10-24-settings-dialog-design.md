# Settings dialog — design

Date: 2026-10-24 · Branch: `feat/settings-ui` (worktree `.worktrees/settings-ui`) · ADR 0072 (new)

## Goal

Everything a user can set in `settings.toml` (SPEC §20.0) and `keymap.toml` (SPEC §13) can be
changed from the UI, without knowing TOML. Hand editing still works and stays in sync.

Success:
- Every `settings.toml` key in `template()` is editable in the dialog, except the two panel widths
  (dragging a panel edge sets them).
- Every `Command` can be rebound; a new binding works at once, without a restart.
- A bad value or a duplicate keystroke is refused with a message on that row; the file is not changed.

Non-goals: agent profiles, prompt templates, unbinding a command (an empty keystroke is an error
in today's keymap format), a Save/Cancel flow, any daemon or protocol change.

## Decisions

| Decision | Choice |
|---|---|
| Scope | `settings.toml` + `keymap.toml` |
| Surface | One large modal dialog, page list on the left |
| Saving | Applies at once; no Save button |
| Approach | One table of field descriptions in `mkrew-core`; the dialog builds its rows from it |
| Ctrl+, | Opens the dialog (was: open `keymap.toml` in Zed) |
| Keymap | Applied live with `cx.clear_key_bindings()` + `bind_keys` |

No protocol change, no migration. The daemon already re-reads `settings.toml` when it changes;
the UI already re-applies it on its mtime poll (`shell/connection.rs`).

## Components

| Unit | File | Does |
|---|---|---|
| Field table | `crates/mkrew-core/src/settings_fields.rs` (new module beside `settings.rs`) | `pub const FIELDS: &[Field]`. `Field { page, section, key, kind, label, help, hidden }`. `Kind`: `Bool`, `Choice(&[&str])`, `Number { min, max }`, `Text`, `Font`. `pub fn value(&Settings, &Field) -> String` gives the current value as a TOML literal. |
| Settings write | `crates/mkrew-core/src/settings.rs` | `write_value` becomes `pub`. Unchanged behavior: parses before writing, keeps comments and CRLF. |
| Keymap write | `crates/mkrew-ui/src/keymap.rs` | `pub fn write_binding(path, command, keys: Option<&str>) -> Result<(), KeymapError>`. `Some` sets `<name> = "<keys>"`, `None` removes the line (back to the default). Builds the new text, runs `Keymap::parse`, writes only when it reports no error. Template created first when the file is missing. |
| Dialog model | `crates/mkrew-ui/src/settings_dialog.rs` (new, pure) | Open page, focused row, text drafts, per-row error, keyboard search, capture state, Esc order. |
| Render + events | `crates/mkrew-ui/src/shell/settings.rs` (new) | Draws the dialog from theme tokens, writes on change, rebinds keys. |

`Command::OpenKeymapSettings` is renamed `OpenSettings` (keymap name `open_settings`, default
`ctrl-,`) and opens the dialog. The palette entry "Global: Open settings" opens the dialog too.
The dialog footer has "Edit settings.toml in Zed" and "Edit keymap.toml in Zed", which call the
existing `open_settings` / `open_keymap_settings` functions (renamed to `edit_settings_file` /
`edit_keymap_file`). A keymap with the old name `open_keymap_settings` reports it as an unknown
command, like any other unknown name; the ADR notes the rename.

### Pages

| Page | Keys |
|---|---|
| General | `awake.keep_awake`, `composer.file_ref`, `zed.follow_debounce_ms`, `ui.reading_width`, `ui.close_to_tray` |
| Appearance | `ui.theme`, `ui.font_family`, `ui.font_size`, `terminal.font_family`, `terminal.font_size`, `ui.diff_mode`, `ui.diff_wrap` |
| Notifications | `notify.needs_input`, `notify.done`, `notify.sound`, `notify.flash_taskbar` |
| Voice | `voice.base_url`, `voice.model`, `voice.api_key_env`, `voice.language`, `voice.prompt`, `voice.max_seconds` |
| Jira | `jira.site`, `jira.email`, `jira.api_token_env` |
| Phone | `web.enabled`, `web.push` |
| Features | `features.docs`, `features.turns` |
| Keyboard | every `Command` |

Hidden (in the table, not shown): `ui.inspector_width`, `ui.sidebar_width`.

## Data flow

### settings.toml

- Open: `settings::load`. A key missing from the file shows its default with a muted "default" tag.
- `Bool` / `Choice`: a click writes at once.
- `Number` / `Text`: a draft; written on Enter or when focus leaves the row; Esc drops the draft.
  No validation of its own: `write_value` parses the result, so serde's message
  (e.g. `font_size must be 8–32 px, got 40`) shows under the row and the file is unchanged.
- `Font`: a searchable dropdown over `cx.text_system().all_font_names()`, "Default" first (empty
  family). Monospace is not filtered: GPUI does not say which fonts are monospace.
- After a write the dialog loads the file again and shows what it now holds.
- A file that does not parse: a banner at the top with the error and "Fix in Zed"; every row is
  read-only (`write_value` refuses to write such a file, so the dialog never overwrites a file the
  user is editing by hand).
- A hand edit while the dialog is open: the existing mtime poll reloads the values; open drafts stay.
- A failed write (read-only file, disk full): the error shows on that row, not as a toast (SPEC §14).

### keymap.toml

- A click on a row (or Enter) starts capture: the chip reads "Press keys…". The first keystroke
  that is not only modifiers becomes the binding; Esc cancels. While capturing, the keystroke
  runs no command.
- `write_binding` refuses a duplicate or invalid keystroke; the row shows
  `Ctrl+Shift+K is used by "Add workspace"` (the palette title of the other command).
- On success: `Keymap::load`, `cx.clear_key_bindings()`, `cx.bind_keys(keymap.key_bindings(..))`,
  and `Shell.keymap` / `keymap_errors` are replaced, so tooltips and the palette show the new key.
  The keyboard-layout observer in `main.rs` rebinds from the current keymap, not the one loaded at
  start, so a layout switch does not bring old keys back.
- "Reset" (shown on hover/focus) removes the line: the default applies again.
- A row whose binding differs from the default has an accent dot. The search box filters by
  title or by shortcut label.
- The keymap template's "then restart Mkrew" line changes to "changes apply at once".

## UI

- Size about 760×560, never larger than the window minus its margin; the right side scrolls.
- Widgets: switch (`Bool`), segmented control (`Choice`), `text_box::Input` (`Number`, `Text`),
  dropdown with search (`Font`).
- Keyboard page row: palette title (`palette::title`), shortcut chip, accent dot when
  changed, Reset on hover/focus; context ("Terminal only", "Git only", "Files only", "Docs only",
  "Board only") in muted text under the title.
- Keys: Tab / Shift+Tab move between rows; Enter / Space toggle or open; ↑ / ↓ change the page
  while the page list has focus. Esc: cancel capture → drop draft → close. The dialog is modal:
  global shortcuts do not run while it is open.
- Every color, spacing, radius and font size from `theme.rs`; dark and light. Every control has
  hover, focus ring, active and disabled states.

## Testing

Permanent tests:

| Test | Guards |
|---|---|
| `FIELDS` matches `template()` | every template key has exactly one `Field`, and the reverse |
| write each `Field` | writing its default to a temp file and loading gives that value back; comments stay |
| bad values refused | `ui.font_size = 40`, `voice.max_seconds = 0`: error returned, file bytes unchanged |
| `write_binding` | set, replace, remove; comments and CRLF kept; template created when missing |
| duplicate refused | binding a used keystroke: error names the other command, file unchanged |
| dialog model | Esc order; drafts kept on reload; keyboard search by title and by shortcut |
| rename | `ctrl-,` is bound to `OpenSettings`; existing keymap/palette tests updated, not re-pinned |

No render or wiring tests. Smoke run in the worktree: Ctrl+, opens the dialog; change the theme,
a font size (also a bad value), a switch; rebind a key and press it without restarting; check the
lines in `settings.toml` and `keymap.toml`.

## Docs

- ADR 0072 "Settings dialog": Ctrl+, opens the dialog, the command rename, live keymap, no unbind,
  read-only on a broken file. Re-check the next free ADR number before writing it.
- SPEC §13: the Ctrl+, row ("Open settings"), live keymap changes. §14: the dialog. §20.0: the
  dialog writes this file.
- HUMAN_CHECKS: dark/light, 100/150/200 % DPI, broken file, read-only file, live rebinding, a
  keyboard layout switch after rebinding.
- PROGRESS: one line.

## Merge with master

Master has an uncommitted `[jira] mine` key (with `write_jira_mine`). When merged it needs a
`Field` (`Bool`, page Jira), or the `FIELDS` matches `template()` test fails. Note this in
HUMAN_CHECKS "Numbers and merge notes".

## Build note

The worktree builds into its own `target/` unless `CARGO_TARGET_DIR` is shared; C: is nearly full.
Ask before the first build.
