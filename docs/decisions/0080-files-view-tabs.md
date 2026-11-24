# 0080 — Files view tabs

- Date: 2026-10-26
- Status: superseded by 0108
- Milestone: none (user request: view and close several files at once, like Zed / VS Code)

## Context

The Files view showed one file; opening another replaced it and asked "Save / Don't save /
Cancel" when it had unsaved text (SPEC §12A).

## Options

1. Per-file state in a `FileTab`, a tab list in `FilesView`; answers routed by path. (Chosen.)
2. Keep one shown file and swap parked copies in and out: an `fs.read` / `fs.write` answer that
   arrives after a switch lands in the wrong file.
3. Keep only unsaved buffers and re-read on switch: caret, undo and scroll are lost.

## Decision

- VS Code's transient tab: a single click opens an italic tab that the next single click
  replaces while it is clean; a double click, an edit, Pin or "New file…" keeps it.
- Close (✕, middle click, Ctrl+W, Ctrl+Shift+W, tab menu: Close / others / to the right / all)
  asks about each unsaved tab; Cancel closes nothing. Bulk closes skip pinned tabs.
- Pin and drag-to-reorder reuse the terminal tab strip's look and `shell/reorder.rs`.
- Ctrl+Tab, Ctrl+Shift+Tab, Ctrl+Alt+1–9 (9 = last) act on file tabs while Files is shown.
- One find bar per view; `git.changed` reads the active tab and marks the others stale.
- Tabs are in memory only: a restart or another worktree starts with none.

## Consequences

- SPEC §12A and §13 change; no protocol, storage or daemon change.
- Opening another file never asks about unsaved text any more; leaving a worktree or closing
  the window asks once per unsaved tab.
- New key `ctrl-w` (`close_file_tab`) in the Files context only (ADR 0028 allows plain
  Ctrl+letter there).
- A Markdown preview starts at the top when its tab is shown again (one shared page).

## Superseded by 0108

ADR 0108 removes the Editor view, the file tab strip, its keys and `FilesView.tabs`.
An open file is a leaf of a terminal tab's split tree (a tab or a pane next to the terminals), its text lives in the per-workspace `Documents` store, and open files are saved in the tab layout, so they come back after a restart.
What stays from this ADR: a single click opens an italic preview tab that the next single click replaces while it is clean (the leaf's `preview` flag); closing asks "Save / Don't save / Cancel" for each unsaved file; the find bar belongs to the shown file; Ctrl+W (`close_file_tab`) now closes the focused file pane.
What goes: the strip, pinned file tabs, file-tab cycling under Ctrl+Tab and Ctrl+Alt+1-9, and "tabs are in memory only".
