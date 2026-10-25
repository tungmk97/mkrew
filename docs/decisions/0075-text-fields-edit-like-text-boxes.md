# 0075 — Every text field edits like a normal text box, with undo

- Date: 2026-10-25
- Status: accepted (SPEC §13; supersedes the "other fields edit at the end" rule of ADR 0048 / 0063)
- Milestone: none (user request 2026-10-25)

## Context

A review of every input showed that only the task dialog, Run setup and the preview block edit
moved the caret and copied. The composer, follow-up, commit box and every filter / search box only
typed and erased at the end: no arrows, no click to place the caret, Ctrl+C did nothing after
Ctrl+A. Ctrl+Z worked only in the Files editor. In Settings, Ctrl+X removed the selected text
without copying it (lost text), and the shortcut search moved a caret it did not draw.

## Decision

- `Input` keeps an undo history: Ctrl+Z / Ctrl+Y (Cmd on macOS; Ctrl+Shift+Z stays Open in Zed).
  Chars typed or erased one at a time join one step until a blank is typed, so a word, or an IME
  (Telex, Unikey) erasing and retyping a composing letter, is one step; a paste, a cut, a deletion
  by word or a selection replaced is its own step. 100 steps per field; clearing a field (after
  send) drops its history.
- Every field forwards the editing and caret keys to `Input::edit` (`DialogKey`, and the
  `Edit(DialogKey)` variant of the quick open, base menu and find-bar keys) and draws its caret and
  selection where they are (`field::caret_text`); a click puts the caret under the pointer, a drag
  or Shift+click selects, a double / triple click selects the word / line (`field::caret_mouse`).
- The shell puts the selection on the clipboard before Ctrl+C / Ctrl+X (`field::copy_selection`);
  the model then removes it on Cut.
- ↑ / ↓ keep their list meaning where a field has a list (quick open, palette, peek, broadcast,
  settings rows).

## Consequences

- One behavior for all fields; the terminal find bar uses the same drawing (it lives in the lib,
  so the drawing moved from `shell/widgets.rs` to `field.rs`).
- A caret moved above the visible lines of a scrolling multi-line field is not scrolled into view
  (only a caret at the end keeps the field scrolled to the bottom).
- A single-line field does not scroll sideways to follow the caret (unchanged).
