# 0108 - Tern panes: files and the diff as tabs and panes, four-edge drops, a `+` row

- Date: 2026-11-23
- Status: accepted (supersedes ADR 0080 and the Editor and Diff chips, the `+ ▾` button and the "Alternatives" line about Editor and Diff of ADR 0106; amends ADR 0041, ADR 0087 and SPEC sections 5, 7, 10, 11, 12A, 12B, 13, 14, 20.0 and 20B.5)
- Milestone: none (human request 2026-11-23, spec `docs/superpowers/specs/2026-11-23-tern-panes-design.md`, plan `docs/superpowers/plans/2026-11-23-tern-panes.md`)

## Context

ADR 0106 gave Mkrew Tern's top bar, but the main area still had two worlds.
Terminals were tabs with split panes.
Files lived in an Editor view with its own tab strip (ADR 0080), the diff in a Diff view, and both showed as chips in the title bar.
Panes had a thin border, a 4 px gap and drops only to the right or the bottom half.
The user wants the area to work like Tern: panes with a frame and a gap, a stripe on the focused one, files and the diff that open as tabs or as panes next to the terminals, a drop zone on every edge, and a `+` that offers the common new things in one row.
A new text note should be one click.

## Decision

- **Typed leaves.**
  A leaf of the split tree is a session, a file or the diff: `Leaf::Session`, `Leaf::File { file, task, md, preview }`, `Leaf::Diff`.
  A leaf is identified by a `PaneKey` (`Session(Ulid)`, `File(FileKey)`, `Diff`), and every tree and tab operation takes a key.
  A `FileKey` is a path and the folder it is of: `task` is a task worktree, absent = the project folder.
  The layout stays untagged JSON in the `terminal_tab.layout` column: `{"session": ...}` as before, `{"file": "src/a.rs"}` or `{"file": "src/a.rs", "task": "<ulid>"}`, `{"diff": true}`.
  A file leaf without `task` (every layout of protocol 58 and 60) is a project file, so no migration and no protocol bump are needed for the field.
  A leaf that does not parse is dropped with a warning and the rest of the tab loads.
  There is no migration.
- **Documents store and `DocView`.**
  What `FilesView.tabs` held (text, caret, undo, dirty flag, disk stamp, preview page) moves into a pure `Documents` store, one per workspace, keyed by the `FileKey` (`documents.rs`, no gpui).
  The gpui state of a shown file (focus, scroll handles, find bar, preview list) is a `DocView` per open file.
  A leaf whose document is missing (after a restart) loads it with `fs.read` the first time it is drawn.
  A document is dropped when its leaf leaves every layout and it is not dirty.
  The file tab strip, the Editor view and the Diff view are deleted.
- **A file knows its folder.**
  The folder a file is of (the project, or a task worktree) lives with the open file: in its leaf and its `Doc`, never in the Files tree's state.
  Its `fs.read`, Reload and `fs.write` use that folder, so a restart or a lost connection cannot send an edit to the other copy of the same relative path.
  A tree click or drag takes the folder the tree shows at that moment, a terminal link or chat path pill the worktree of its agent, a new note and an OS drop the project's.
  Choosing another worktree in the tree changes only the tree and closes nothing.
  A restored file whose worktree is gone shows the load-error state of the pane, not the project's file.
  The Diff leaf needs no such field: it draws the workspace's Git view, and every stage, discard or commit acts on the target that view shows.
- **One pane per file per workspace.**
  A file key (path and folder) and the Diff key appear at most once across all tabs and windows of a workspace; the same path in the project and in a worktree are two files.
  Opening what is open focuses its pane (its tab and window come to the front) and jumps to the line.
  So each path has one buffer and nothing counts references.
- **One way to open.**
  `Shell::open_document(path, place)` and `Shell::open_diff(place)` with `Place::{Preview, NewTab, Split(key, edge)}`.
  A single click in the Files tree (or Enter on a row) opens the workspace's preview tab (italic title), which the next single click replaces; an edit, a double click, a pin or a drag (any move of the pane) keeps it.
  Every other open is a new tab right after the active one.
- **Four-edge drops.**
  `Edge::{Left, Right, Top, Bottom}` replaces `drop_axis`; the middle of a pane has no zone, the nearest edge wins (ties: right, then bottom).
  Files, tabs, panes and the `+` row items drop on any edge and split there 50 / 50; a pane or tab dropped on the tab strip's empty space becomes its own tab.
  A Files tree row dropped on a composer still inserts the path; dropped on a terminal pane it no longer types the path.
- **Pane look.**
  No pane border; panes sit on a frame of the new color `pane_gap` with 8 px between them and from the edge (`PANE_HALF_GAP = space::XS`), and the title bar uses the same color.
  One 2 px stripe at the pane's top edge: needs input first, then the session color, then the focus color when the tab has two or more panes.
  `pane_focus_border` is renamed `pane_focus_stripe`.
  A rule ("pane gap") in `theme::problems()` needs `pane_gap` and `terminal_bg` to differ by at least 1.06:1; a theme file that breaks it gets the "Low contrast" badge.
- **The `+` row.**
  `+ ▾` becomes `+`, which grows on hover or keyboard focus into `[+] [default agent] [default terminal] [new note] [▾]`.
  Each item is a click (new tab) or a drag (leaf at the drop edge); `▾` opens today's menu.
- **Notes.**
  A new setting `[notes] folder` (empty = the workspace root, relative = under the root, absolute = as is) and a new request `fs.new_note { project_id, worktree_task_id }`.
  The daemon resolves the folder from its own `settings.toml`, names the file `note-YYYYMMDD-HHMMSS.md` in local time (`-2`, `-3`, ... on a clash), creates missing folders and answers `FsCreated { path }` (relative to the root inside it, else absolute).
  `fs.read` and `fs.write` accept one more kind of path: an absolute path inside the daemon's absolute notes folder (normalized, no `..`).
  Every other absolute path is refused as before.
  `Command::NewNote` has `ctrl-alt-n` (`cmd-alt-n` on macOS).
- **Daemon.**
  `cut()` and `layout_sessions()` look only at session leaves and skip the others; a tab whose last session ends but that still holds a file or the diff stays.
  `PROTOCOL_VERSION` 58 to 60 (59 belongs to `feat/omp-tsp`): an older UI must not read layouts with new leaves.
- **`CloseFileTab` stays as the file pane's close key.**
  `Command::ShowEditor` is deleted (Ctrl+Shift+6 is unbound).
  `Command::CloseFileTab` (`close_file_tab` in `keymap.toml`, Ctrl+W, ⌘W on macOS, `EDITOR_CONTEXT`) is kept and now closes the focused file pane, asking about unsaved text first.
  Renaming it to `CloseFilePane` would change the `keymap.toml` name that users may have bound, for no gain, so the name stays and only its doc comment and the SPEC row say "file pane".
  The palette label is "File: Close file".

## Alternatives

- **B: typed tabs.**
  A tab would be a session tab, a file tab or a diff tab, and panes would stay sessions.
  A file could not sit beside a terminal in one tab, which is the point of the request, and drag and drop would need two models.
  Rejected.
- **C: documents in the daemon.**
  The daemon would keep open files so that every window and machine shares buffers.
  It would need a protocol for edits, undo and conflicts, and the UI already has all of that.
  Rejected; the daemon only learns the notes folder.
- **Keeping the Editor and Diff chips.**
  The chips were ADR 0106's answer ("Board, Editor and Diff as real tabs in the tab model: more work and a model change").
  With typed leaves the model change is small and the chips would be a second way to reach the same file.
  Rejected, and that Alternatives line of ADR 0106 no longer holds.
- **The same file in two panes.**
  Needs reference counting or two buffers for one path.
  Out of scope; one pane per file.
- **A notes folder per workspace, or a folder picker.**
  One global setting and a text field are enough for the first version (the Settings dialog has no folder picker kind).

## Consequences

- Protocol 60: a UI from before it cannot connect to the new daemon (the version check blocks it), so it never reads a layout it cannot parse.
  After the update the user restarts the daemon.
- A layout written by protocol 58 or 59 loads unchanged.
  A layout with file or diff leaves, read by an older daemon, would lose them in `cut()`; the version check prevents that pairing.
- `fs.read` and `fs.write` now know one path outside the root; the daemon never opens any other absolute path, so the rule "the daemon reads only inside the root and the notes folder" holds.
  A remote machine uses its own `settings.toml`, so its notes folder is that machine's.
- The daemon now also refuses a relative path that holds a drive part in the middle (for example `a/C:/x` or `a/C:x`), which Windows resolves to another drive.
  This closes an escape from the root that existed before this branch.
- Closing a pane, a tab, a window, a workspace or the app asks about unsaved files as before (Save, Don't save, Cancel); moving a file leaf (drag, tear-off, merge) never asks.
  An edited preview tab is kept and not replaced.
- ADR 0080 is superseded: the strip, the file tab cycling and the Editor chip are gone, and `Command::ShowEditor` is deleted, so Ctrl+Shift+6 is free.
  Ctrl+W stays bound to `CloseFileTab`, which closes the focused file pane (see Decision).
  What carries over: the preview (italic) tab, the save question, one find bar per shown file.
- Detached windows draw file and diff leaves from the same store, so file tabs tear off like terminal tabs; "Plan 2 (Editor file tabs)" of ADR 0096's follow-up is not needed.
- `Command::ShowDiff` (Ctrl+Shift+7) opens the Diff tab or focuses it; it is no longer limited to "while a diff is open".
- OS files dropped on a file or diff pane open when they are inside the workspace root or the notes folder; others show one error toast.
- Typed leaves make the key the only identity: a file renamed on disk is a new key (its old pane shows "This file was removed.").
- Tabs inside a pane, Board and Docs as tabs, and restoring unsaved edits after a restart stay out of scope.
- Follow-ups known and left open:
  - The daemon's `.git` guard can be bypassed on Windows with a trailing dot (`.git.`) or the 8.3 short name (`GIT~1`).
    This is older than this branch and is listed in `docs/INBOX.md`.
