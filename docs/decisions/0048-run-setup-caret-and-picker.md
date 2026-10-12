# 0048 — Run setup fields get a caret; Folder and Env file get a workspace picker

- Date: 2026-10-12
- Status: accepted

## Context

SPEC §13 kept every small text field "edit at the end": fixing a typo in the middle of a long
command meant erasing back to it. The human asked for normal fields in Run setup, and for picking
the folder and the env file in the workspace instead of typing them.

## Decision

- `text_box::Input` keeps a caret (byte offset) and an optional selection anchor; typing,
  Backspace and Ctrl+Backspace act at the caret. `dialog_key` adds `Move` (arrows, Home/End,
  Ctrl for words / text ends, Shift extends), `Delete`, `Copy` and `Cut`. Only Run setup
  forwards them (`Input::edit` / `TextBox::edit`) and draws the caret (`caret_field`); every other
  field ignores them, so its caret stays at the end and it looks as before.
- `Browse…` beside Folder and Env file opens a picker inside the dialog fed by `fs.list` (with
  ignored files: `.env` usually is). Folder: from the workspace root, `Use this folder`. Env
  file: from the card's folder, never above it, so the result stays relative to it as
  `normalize` requires.

## Alternatives rejected

- The native file dialog (`prompt_for_paths`): gpui cannot start it in the workspace, and it
  lets the user leave the workspace, which `normalize` rejects.
- The Files editor (`editor::Editor`) as the field model: its Tab, Enter and undo keys clash with
  the dialog's.
- Caret keys in every field at once: each draws through `text_field` with its own key routing;
  left for when it is asked for.

## Consequences

No protocol change. Mouse clicks focus a field but do not place the caret (no text measuring in
`text_field`).
