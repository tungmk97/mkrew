# 0028 — The Files view edits files

- Date: 2026-10-09
- Status: accepted
- Milestone: inbox 2026-10-09

## Context

Inbox items 28 and 29: the user expects to edit the opened file in the Files view, and to find
and replace text with VS Code keys. SPEC §1 said "Not a code editor … editing happens in Zed",
and §12A said "Viewer: read-only". Switching to Zed for a one-word fix in a plan or a config
breaks the flow that the Files view is for.

## Options

1. Keep the viewer read-only and only improve "Open in Zed". No work, but it does not answer
   the inbox items.
2. A plain editor in the viewer: caret, selection, undo and redo, clipboard, find and replace,
   Ctrl+S. Saves go over IPC with a conflict check. No LSP, completion, multi-cursor or folding.
3. Embed Zed's editor crate. It is not published for gpui-pre, it is very large, and it pulls
   in the LSP stack.

## Decision

Option 2.

- **Buffer and screen.** The buffer (`mkrew-ui/src/editor.rs`) is pure and tested. The screen
  reuses the viewer's rows and draws a caret and a selection on the monospace grid.
- **Saving.** Saving uses `fs.write` (protocol 27). It sends the whole text and the `hash`
  that `fs.read` returned (`docs::hash` of the bytes). The daemon writes only if the file still
  hashes to that value. If not, it answers `saved: false` with the disk's hash. The editor then
  shows "Changed on disk — Reload · Keep mine". "Keep mine" saves over that hash.
- **Unsaved text.** A re-read never replaces unsaved text. Leaving it asks "Save / Don't save /
  Cancel" (§2.5). That happens when the user opens another file, picks another worktree or
  closes the window.
- **The write.** The file is written in place (`std::fs::write`). This keeps permissions,
  symlinks and hard links. The write is not crash-atomic, and a writer between the check and the
  write is overwritten.
- **Limits.**
  - Existing files only.
  - At most 2 MB of UTF-8 text.
  - Line endings are kept: the first line's ending is used for the whole file.
  - The final newline is kept.
- **Keys.** The editor handles its own editing keys (arrows, Ctrl+Z/Y/A/C/X/V, …). They are not
  in the keymap, like Ctrl+C in terminals (ADR 0022).
  - Save, Find and Replace are keymap commands: `ctrl-s`, `ctrl-f`, `ctrl-h`. They work only in
    the Files context. No terminal has focus there, so plain Ctrl+letter is safe (§13).
  - Ctrl+Shift+Z stays "Open in Zed", so redo is Ctrl+Y only.
  - VS Code's Ctrl+Shift+1 (replace one) is "Board" here. Replace one is Enter in the replace
    field.

## Consequences

SPEC §1, §7, §12A, §13 and M7 change. A save is a file change like any other. The daemon's
watcher attributes it to the single working agent of the workspace (`files_changed`), exactly
as it does for an edit made in Zed today.

Not done: autosave, creating or renaming files, a crash-safe write.

Known limits:
- AltGr characters on Ctrl+Alt+digit cannot be typed, because those keys jump between tabs (the
  same as in terminals).
- The caret is placed on a monospace grid, so wide CJK glyphs move it out of place.
