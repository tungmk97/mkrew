# Files tree in the right panel - design

- Date: 2026-10-27
- Status: approved in chat, awaiting spec review
- ADR to write with the implementation: `docs/decisions/0081-files-tree-in-the-right-panel.md`
- Follow-up (sub-project 2, not in this spec): moving Git into the right panel, see `GIT_PANEL_PROMPT.md`.

## Goal

Remove the top-level Files view.
The file tree moves into the right panel, next to the terminals, so files can be opened and dragged into agents without leaving the Terminals view.
Clicking a file shows it in the center area with the editor that exists today (tabs, find, Markdown preview, change gutter).
A file or folder dragged from the tree onto an agent pane types its reference, the way `@` in the composer does.

## Scope

In:
- The right panel (today's session panel, ADR 0038) gets two tabs: `Files | Session`.
- The panel shows in the Terminals and Editor views, with one toggle and one width.
- The panel's open state and tab are remembered across restarts.
- `View::Files` becomes `View::Editor`, listed in the view bar only while the workspace has open file tabs.
- Drag a tree row (file or folder) onto a pane: an agent pane gets `composer.file_ref`, a shell pane gets the plain relative path.

Out (YAGNI; may come later):
- Git in the right panel (sub-project 2).
- Dragging several tree rows at once (the tree has no multi-select).
- Dragging tree rows anywhere but a pane (composer of a Board card, Docs, other workspaces).
- The right panel in the Board, Docs and Git views.

## Layout and navigation

```
┌ title bar: [Board│Terminals│Docs│Git│Editor*]  … ◨ ┐
├ sidebar ┬ center ──────────────────────┬ right panel ───┤
│         │ Terminals: tab strip + panes │ [Files│Session]│
│         │ or Editor: file tabs +       │  file tree     │
│         │   header + editor / preview  │  (draggable)   │
└─────────┴──────────────────────────────┴────────────────┘
* Editor is listed only while ≥1 file tab is open
```

- **View bar:** `Board │ Terminals │ Docs │ Git`, then `Editor` while the shown workspace has at least one open file tab.
- **Right panel:** one column right of the center area in the Terminals and Editor views.
  It has a tab row `Files │ Session` at its top; a click switches the tab.
  `Session` is today's session panel content, unchanged.
  `Files` is today's tree with its header (search, `Ignored` toggle), context menu, git markers and keyboard cursor, unchanged.
- **Toggle:** the title bar's `◨`, `Ctrl+Shift+Y` (`ToggleInspector`) and the palette show or hide the panel; the tab is kept.
  From the Board, Docs or Git view the toggle shows Terminals with the panel open, as today.
  The palette label becomes "Toggle right panel".
- **Remembered:** `[ui] right_panel = "files" | "session" | "hidden"`, default `"hidden"` (a first start looks like today).
  The toggle and the tab row write it; it is read at start.
  Like `[ui] sidebar`, it is a hidden key in the Settings dialog (`settings_fields.rs`): the toggle is the way to change it.
- **Width:** `[ui] inspector_width` (240-560 px, dragging the left edge), shared by both tabs.
  `theme::FILE_TREE_WIDTH` is removed.
- **Ctrl+Shift+3 (`ShowFileTree`, was `ShowFiles`):** opens the panel on the Files tab and gives the tree the keyboard (↑/↓/Enter, `Ctrl+Shift+H` for ignored files).
  From the Board, Docs or Git view it shows Terminals first.
- **Opening a file:** a tree click, Enter in the tree, quick open (`Ctrl+Shift+O`) and a terminal link show the Editor view and open the file in a tab (transient or kept, ADR 0080).
- **ShowEditor (`Ctrl+Shift+6`):** shows the Editor view while a file tab is open (every keymap command has a default key; 6 follows Docs' 5).
  `keymap.toml` users with a `show_files` entry get the usual unknown-name error and the defaults (clean rename, no alias).
- **Closing the last file tab** removes `Editor` from the view bar and shows Terminals.
- **Unchanged:** the editor, find / replace, Markdown preview, change gutter, "Changed on disk" banner, unsaved-changes prompts (closing a tab, switching worktree, closing the window) and the worktree target of the tree (§20E.1).
  Switching view keeps unsaved text, as today.

## Drag from the tree into an agent

- **Source:** every tree row (file or folder) is draggable with gpui `on_drag`, carrying
  `DraggedPath { project_id, path, is_dir }` (`path` relative to the tree's root, `/`-separated).
  The drag preview is the row's name on the shared drag chip (the same look as dragged tabs and cards).
- **Target:** the whole pane (header, terminal, composer), as for OS drops (§20B.5).
  The pane uses plain `on_drop::<DraggedPath>`: an in-app drag is mouse-driven, so the OLE keyboard-modality bug that `drop_catcher` works around does not apply.
  While dragged over, the pane shows an accent ring (`drag_over`, theme tokens only).
- **Typed text:**
  - Agent pane: `composer::file_ref(template, path)` with the `[composer] file_ref` setting (default `@{path}`), read as the `@` picker reads it: `@src/main.rs`; a folder ends with `/`: `@src/`.
  - Shell pane: `composer::file_ref("{path}", path)`: `src/main.rs`, `src/`.
  - A path with spaces is quoted with `"` (as `file_ref` does today).
  - One space follows the reference, so two drops in a row stay two references (`@a.rs @b.rs `).
- **Relative to:** the folder the tree shows (the project folder, or the task worktree, §20E.1).
  A worktree mirrors the repository layout, so the path is right for an agent running in it.
- **Delivery:** the shared branch of today's `drop_paths` is extracted as `type_into_pane(session, text)`:
  into the composer draft while the pane's composer is shown, else `type_text` into the terminal; never an Enter; the pane gets focus.
  OS drops and tree drops both call it.
  A modal dialog open: nothing is typed.
- **Drop elsewhere:** nothing happens.
- **OS drops** are unchanged: absolute paths.

## Code changes (`crates/mkrew-ui`, plus settings)

|File|Change|
|---|---|
|`board.rs`|`View::Files` → `View::Editor`; the view bar lists `Editor` only while the shown workspace has file tabs.|
|`keymap.rs`|`ShowFiles` → `ShowFileTree` (`show_file_tree`, `ctrl-shift-3`); new `ShowEditor` (`show_editor`, `ctrl-shift-6`). `FILES_CONTEXT` → `EDITOR_CONTEXT` (save, find, file tab keys); new `FILE_TREE_CONTEXT` (`ToggleIgnored`).|
|`shell/files_render.rs`|`render_files` splits into `render_editor` (center) and `render_file_tree` (panel tab); tree rows get `on_drag(DraggedPath)`.|
|`shell/inspector.rs`, `inspector.rs`|The panel gets the `Files │ Session` tab row; `inspector_open: bool` → `right_panel: RightPanel { open, tab }`, read from and written to `[ui] right_panel`; the toggle hides / shows and keeps the tab. The Session tab follows the active tab's focused pane in the Editor view too.|
|`shell/chrome.rs`|`render_main` adds the panel to the Terminals and Editor views.|
|`shell/navigation.rs`, `shell/pane_render.rs`|`type_into_pane` extracted from `drop_paths`; panes get `on_drop::<DraggedPath>` and `drag_over`.|
|`shell/workspace.rs`, `shell/sessions.rs`, `palette.rs`, `shell/send_dialog.rs`, `shell/composer.rs`, `shell/file_tabs.rs`, `shell/files.rs`|Every `View::Files` use moves to `View::Editor` or the tree; closing the last file tab shows Terminals.|
|`theme.rs`|`FILE_TREE_WIDTH` removed; a token for the panel's tab row only if no existing token fits.|
|`mkrew-core` settings, `settings_fields.rs`|`ui.right_panel` (`RightPanelMode`: `files` / `session` / `hidden`, default `"hidden"`), a hidden key like `sidebar`.|

The `FilesView` model (`files_view.rs`) is unchanged: one per workspace, holding the tree, the file tabs and the editor, now drawn in two places.

## Docs

- ADR 0081 "Files tree in the right panel": supersedes in part ADR 0050 (tree right of the editor) and ADR 0038 (the session panel becomes a tab).
- SPEC §10: layout sketch, view bar, the right panel and its tabs, `[ui] right_panel`.
- SPEC §12A: Tree in the right panel, Editor in the center, the Editor view's visibility.
- SPEC §13: `Ctrl+Shift+3` = Show file tree, Show editor, Toggle right panel.
- SPEC §20B.5: drops from the tree.
- SPEC config sample: `right_panel`.
- `docs/HUMAN_CHECKS.md`: one entry for the visual check.

## Testing

Unit tests (behavior only):
- The view bar lists `Editor` exactly while the shown workspace has file tabs.
- `ui.right_panel` parses `files` / `session` / `hidden`, defaults to `hidden`, rejects other values.
- `ctrl-shift-3` is bound to `ShowFileTree`, `ctrl-shift-6` to `ShowEditor`.
- The text typed for a `DraggedPath`: agent `@src/main.rs`, agent folder `@src/`, shell `src/main.rs`, a path with spaces quoted.
- The existing `files_view` tests stay as they are.
- `insta` snapshots of the palette and keymap are updated.

Smoke run on the real app:
- Open the panel, click a file: the Editor view shows it; the Files tab stays in the panel.
- Close every file tab: `Editor` leaves the view bar and Terminals shows.
- Drag a file and a folder onto a Claude pane: `@...` is typed, no Enter; onto a shell pane: the plain path.
- With the composer open, the drop goes into the draft.
- Restart: the panel comes back hidden, or on the tab it was left on.
