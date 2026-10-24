# 0073 — Settings dialog

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
- A shifted digit or punctuation key is captured the way Windows reports it (`ctrl-@` for
  Ctrl+Shift+2 on a US layout), so the duplicate check does not see it against a default written
  with `shift` (`ctrl-shift-2`): the new binding wins in GPUI and the default is shadowed without
  a message.
- This ADR is 0073: master took 0072 (Files preview) while the branch was open.
