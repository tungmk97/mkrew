# Tern panes - Design

- Date: 2026-11-23
- Status: design approved in chat by the user (2026-11-23), section by section; this file waits for the user's review.
- Branch and worktree: `feat/tern-panes` in `.worktrees/tern-panes` (the user asked to keep other running branches untouched).
- ADR: `docs/decisions/0108-tern-panes.md`.
  0106 is the newest on `master`; `feat/omp-tsp` (unmerged) reserves 0107.
- Protocol: 58 -> 60.
  `master` is at 58; `feat/omp-tsp` reserves 59.
  Re-check both numbers when this branch merges; take the next free ones if they moved.
- Migration: none (the tab layout is a JSON column).
- Supersedes ADR 0080 (Files view tabs), the Editor / Diff chips and the "Alternatives" paragraph of ADR 0106 that rejected Editor and Diff as real tabs, and amends ADR 0041 (`tab.close` removes the layout's sessions).

## 1. Goal

The user wants the main area to work like Tern (Stencil): panes that look like Tern's, files and diffs that open as tabs or panes next to the terminals, and a `+` that offers the common new things in one row.

Success means:

- Panes have no border, sit on a frame of their own color with an 8 px gap, and the focused pane is marked by a 2 px stripe on its top edge.
- There is no Editor view and no Diff view.
  A file or the diff opens as a tab in the same tab strip as the terminals, or as a pane inside the tab in front.
- A file, the diff, a tab, a pane and every item of the `+` row can be dragged onto any of the four edges of a pane to split it there.
- File and diff panes keep every feature the Editor and Diff views have today.
- Tabs that hold files and the diff come back after a restart, in the same place.
- Hovering `+` shows a row of icons on the same line: default agent, default terminal, new note, and `▾` for the full menu.

## 2. What the user said, and what was assumed

Said by the user (request and brainstorm answers):

- Panes: split, spacing and highlight like Tern.
- Remove the Editor module; clicking a file opens a new tab next to the terminal tabs by default, or the user drags it onto the current view to make a pane, split according to the current layout, like Tern.
- `+` on the tab strip: on hover, the options show in a row on the same line; picking one opens a new tab by default, or the user drags it like above.
- A file pane keeps the whole current Editor (editing, save, find, git gutter, Markdown preview and split, selection and copy, block edit).
- The Diff view becomes a tab / pane too.
- File and diff leaves are saved in the layout and restored.
- Approach A: typed leaves in the split tree plus a UI-side Documents store.
- Drop zones: the four edges; the middle of a pane has no zone of its own, the nearest edge wins.
- Dragging a Files tree row onto the composer inserts the path, as today; onto the pane it opens the file.
- The `+` row holds, as icons: the default agent, the workspace's default terminal, then New note; keep `▾` at the end for the full menu.
- New note is a temporary text note file, saved in a folder the user picks; the default is the current workspace root.
- Work happens in a git worktree so the running branches are not touched.

Assumed (stated in chat, not contradicted):

- A file or the diff is open in at most one pane per workspace; opening it again focuses that pane.
- Single click in the Files tree opens a preview tab (title in italics) that the next single click replaces; editing, double click, pin or a drag keeps it.
- A new tab opens right after the active tab.
- A note is `note-YYYYMMDD-HHMMSS.md` (local time), opened in source mode.
- One Diff leaf per workspace, following the Git panel's selection as the Diff view does today.
- `[notes] folder` is one global setting; the daemon accepts absolute paths only inside it, and OS files outside the workspace and that folder do not open (found after the chat: the daemon's `fs.*` requests accept only root-relative paths today).

## 3. Pane look

### 3.1 Tokens (`crates/mkrew-ui/src/theme.rs`)

| Token | Today | New |
|---|---|---|
| `PANE_HALF_GAP` | `space::XS / 2.0` (2) | `space::XS` (4): 8 px between panes and 8 px from the frame edge. |
| `pane_gap` (new color) | - | The frame color behind and between panes, and the title bar background. |
| `PANE_ACCENT_WIDTH` | 2.0 (session color stripe) | 2.0, now the one top stripe (3.2). |
| `pane_focus_border` | Focus border of split panes | Renamed `pane_focus_stripe`; same values; its other callers (cards, comments, composer, slash, send dialog, run rows, jira, Files tree cursor, git diff, terminal view, preview) keep using it under the new name. |

`pane_gap` rule, enforced by a unit test over every built-in and gallery theme:

- Dark themes: darker than `terminal_bg` when `terminal_bg` leaves room (the pane looks raised, as in Tern); else lighter.
- Light themes: a step darker than `terminal_bg`.
- The contrast ratio between `pane_gap` and `terminal_bg` is at least 1.06:1 (Tern measures about 1.07:1).
- Generated themes (`Theme::from_palette`) derive it with `mix` from `terminal_bg` toward black or `fg`, so imported themes get it too.

The title bar uses `pane_gap` instead of `chrome`, so the bar and the frame read as one surface like Tern.
The sidebar and the right panel keep their colors.

### 3.2 Pane frame (`shell/pane_render.rs::pane_frame`, `render_tab`)

- No border on any pane: the layer that draws `status::pane_border` goes.
- Corners stay `radius::MEDIUM` (8).
- One top stripe, `PANE_ACCENT_WIDTH` high, inset by `radius::MEDIUM` on both sides (as the session color stripe is today).
  Its color, first match wins:

| Pane state | Stripe |
|---|---|
| Session needs input | `status_needs_input` (the badge still shows icon + text, rule 3 of the UI skill). |
| Session has a color | That color. |
| Focused, and the tab has two or more panes | `pane_focus_stripe`. |
| Otherwise | None. |

- `status::pane_border` becomes `status::pane_stripe(theme, state, color, several, focused) -> Option<u32>` with the table above; its tests move with it.
- Unfocused panes keep the 5% dim (`unfocused_pane_dim`).
- The divider stays invisible until hovered (`grip_hover` inside the 8 px gap); drag behavior unchanged.
- The drop overlay and the drag strip (`pane_edge`) stay above the stripe as today.

## 4. Data model

### 4.1 Leaves (`crates/mkrew-ui/src/split.rs`)

`Node::Pane { session, chat }` becomes `Node::Leaf(Leaf)`; the JSON stays `#[serde(untagged)]`, so every stored layout still reads.

| Leaf | JSON | Key (`PaneKey`) |
|---|---|---|
| `Leaf::Session { session, chat }` | `{"session": "<ulid>", "chat": true}` (`chat` optional, unchanged) | `PaneKey::Session(Ulid)` |
| `Leaf::File { file, md, preview }` | `{"file": "src/a.rs", "md": "split", "preview": true}` (`md`, `preview` optional) | `PaneKey::File(String)` |
| `Leaf::Diff` | `{"diff": true}` | `PaneKey::Diff` |

- `file` is relative to the workspace root with `/` separators; a note in an absolute notes folder outside the root (section 7.2) stores its absolute path.
  No other path outside the root can be a leaf: the daemon reads only inside the root and the notes folder (section 4.5).
  `PaneKey::File` holds the stored string, so one file has one key.
- `md` is the pane's own Markdown mode (`source`, `preview`, `split`); absent means `[ui] md_mode`.
- `preview` marks the workspace's preview tab (section 6.1); absent means kept.
- `Tab.focused` becomes `Option<PaneKey>`; `Tab.zoomed` likewise.
- `validate` rejects the same key twice in one tab (today: the same session twice).
- A leaf that does not parse is dropped with a `warn!`; the rest of the layout loads.
- Every existing tree function (`rects`, `dividers`, `equalize`, `set_ratio`, `remove`, `move_pane`, `merge_tab`, `arrange`, `Preset::build`) works on leaves of any kind.
  `chat_of` / `set_chat` only touch session leaves.

### 4.2 One pane per file per workspace

- A file key and the Diff key appear at most once per workspace, across all its tabs (main window and detached windows).
- `Tabs::find(key) -> Option<(tab_id, PaneKey)>` finds it.
- Opening something already open activates its tab and focuses its pane; dragging it somewhere moves that pane.
- So each path has one buffer and nothing counts references.

### 4.3 Documents store (`crates/mkrew-ui/src/documents.rs`, new; pure, no gpui)

- `FilesView.tabs` and the file tab strip (ADR 0080) go.
  What each `FileTab` held today (editor state, preview page and selection, block edit, dirty flag, disk stamp, load error, scroll, cursor) moves into `Doc`, keyed by the stored path, in one `Documents` per workspace (`Shell.documents: HashMap<Ulid, Documents>`).
- `FilesView` keeps the tree, its filter, its selection and the right panel; it no longer knows about open files.
- View state that is one per workspace on `Shell` today (the editor focus handle, scroll handles, the find bar, the preview list state) holds gpui types, so it does not go into the pure store: it moves into `shell::doc_view::DocView`, one per open file, in `Shell.doc_views: HashMap<(Ulid, String), DocView>` keyed like the store, so two file panes shown at once never share it.
- A leaf whose `Doc` is missing (after a restart, or in a window that never drew it) loads it with `fs.read` the first time it is drawn, through the same path as today's Retry.
- A `Doc` is dropped when its leaf leaves every layout and it is not dirty.
- Detached windows draw file leaves from the same store, so file tabs tear off like terminal tabs; "Plan 2 (Editor file tabs)" of the detached windows work is no longer needed.

### 4.4 Diff leaf

- One `Leaf::Diff` per workspace shows the diff of the Git panel's selected file, as the Diff view does today; ↑ / ↓ in the Git list still move it.
- `GitView.open` goes: the diff is open when a Diff leaf is in a layout of the workspace.
  `open_diff` places a leaf (section 6.1), `close_diff` removes it.
- When everything is committed, the diff leaf shows the Diff empty state instead of closing itself (a leaf the user placed does not vanish).
- The diff list's scroll and layout state (`git_diff_list` and the other `git_diff_*` fields, one set on `Shell` today) moves into a per-workspace map, so a diff pane in a detached window and one in the main window never share it.
- Comment context, picked lines and the comment palette commands that test `View::Diff` today test "the focused pane is the Diff leaf" instead.

### 4.5 Daemon and protocol

- `crates/mkrew-daemon/src/tabs.rs`: `cut()` and `layout_sessions()` only look at leaves with a `session` key and skip the others.
  `cut()` collapses a split whose child is removed, for any leaf kind.
- A tab whose last session ends but still holds a file or diff leaf stays.
  A tab left with no leaf behaves as today.
- `tab.close` still removes the sessions found in the layout, and only those (ADR 0041 amended).
- `crates/mkrew-core/src/run.rs::layout` (Run tab) is unchanged.
- `PROTOCOL_VERSION` 58 -> 60: an older UI must not read layouts with new leaves.
- SPEC §5: `terminal_tab.layout` is "JSON split tree; leaves are sessions, files or the diff".

### 4.6 Unsaved edits

- Closing a pane, a tab, or a window that holds a dirty file goes through today's save question (`shell/files.rs`, the unsaved-file prompt: Save, Don't save, Cancel).
- Quitting the app asks as today.
- A file changed on disk while clean follows the disk; while dirty, today's conflict handling stays.

## 5. Panes that hold files and the diff

- File and diff panes use the same `pane_frame` as terminal panes: stripe, dim, badges, drag strip.
- A file pane shows today's editor header (breadcrumb, Markdown mode buttons, `Ln, Col`), then the editor, the preview or the split.
- A diff pane shows today's diff toolbar (file, Unified / Split, Wrap, Viewed, Open in Zed, Discard, Stage file, `…`, ✕), then the diff.
- `EDITOR_CONTEXT` is set on the file pane's own element (today it sits on the window root while the Editor view shows); the `"Git"` diff bindings are set on the diff pane's element.
- Tab chip: the focused pane decides the title and icon (as today for sessions).
  A file shows its file-type icon and file name, a dot when dirty, and italics while it is the preview tab; the diff shows the diff icon and `Diff`.
- Find, save (Ctrl+S) and every editor command act on the focused file pane.

## 6. Opening and dragging

### 6.1 One way to open

Every place that opens a file calls `Shell::open_document(target, place)`; the diff calls `Shell::open_diff(place)`.

`place` is:

| `Place` | Used by | Result |
|---|---|---|
| `Preview` | Single click on a file in the Files tree | Replace the file of the workspace's preview tab; with no preview tab, open a new tab with `preview: true`. |
| `NewTab` | Every other open | A new tab right after the active tab: the UI computes the position (midpoint between the active tab and the next one in its pin group, as `Tabs::move_tab` does) and passes it in `tab.create`. |
| `Split(PaneKey, Edge)` | A drop on a pane | Split that pane at that edge (section 6.2). |

If the target is already open, any `place` only activates its tab and focuses its pane (section 4.2); `path:line:col` still jumps to the line.

Callers moved to `open_document` (the entry points of today):

1. Files tree click (`Preview`) and double click (`NewTab`, kept) via `click_file`.
2. Enter in the tree.
3. Keyboard open from the sidebar / sessions list.
4. Retry after a read error (reloads in place, no open).
5. The save question showing its file (focuses the leaf).
6. Ctrl+click on a path in the terminal, and link hints (`open_terminal_link` -> `open_file_at`).
7. The palette's Files scope and Search hits (`open_palette_file`).
8. Links in the Markdown preview and in Docs (`reveal_file`, `open_file_at`).
9. Path pills in Chat messages (`open_chat_path`).
10. A file created in the tree (`show_created_entry`).
11. `Command::ShowDiff` and every "open diff" action call `open_diff(NewTab)` (focuses it when open).

`show_view(View::Editor)` and `show_view(View::Diff)` are gone; `reveal_file` reveals in the tree only.

### 6.2 Four-edge drop zones

- `split::Edge { Left, Right, Top, Bottom }` replaces `drop_axis`.
- `Edge::nearest(x, y)` takes the pointer as fractions of the pane (0..1) and returns the edge with the smallest of `x`, `1-x`, `y`, `1-y`; ties go Right, then Bottom.
- The overlay tints the half of the pane that the new leaf will take, with `tint(accent)` as today.
- `Node::insert_at(target: &PaneKey, edge: Edge, leaf: Leaf)` splits the target 50 / 50, the new leaf on the edge's side.
- A drop of a pane on itself, or of a tab on the tab that holds only that pane, does nothing.
- No drop while a dialog is open (`modal_open`), as today.

### 6.3 What can be dragged

| Dragged | On a pane (4 edges) | On empty space in the tab strip | On a composer |
|---|---|---|---|
| Files tree file row (`DraggedPath`) | File leaf at that edge | New tab | Insert the reference (`composer.file_ref` / relative path), as today |
| Files tree folder row | Not accepted | Not accepted | Insert the path |
| `+` row item (`DraggedNew`, new) | Start the agent / shell / note in a leaf at that edge | New tab | Not accepted |
| Tab (`DraggedItem`) | Merge its panes at that edge (today: right / bottom only) | Reorder, as today | Not accepted |
| Pane (`DraggedPane`, now `{ key: PaneKey, title }`) | Move it to that edge, any leaf kind | Move it out to its own new tab (new) | Not accepted |
| OS files (`ExternalPaths`) | Terminal pane: type the paths, as today; file or diff pane: open each file inside the root or the notes folder as a new tab | Same as on a file pane | Type the paths |

- Dropping a Files tree row onto a terminal no longer types its path: that moves to the composer (`drop_tree_path` becomes the composer's drop handler).
  A composer that is hidden takes no drop.
- OS files outside the root and the notes folder do not open: one `toast_error` says "Only files inside the workspace or the notes folder open in Mkrew"; folders are skipped.
- Tear-off of a tab out of every window is unchanged and carries file and diff leaves.

## 7. The `+` row

### 7.1 Look and behavior

`+ ▾` (`render_new_tab_button`, `shell/chrome.rs`) becomes `+`.
On hover or keyboard focus it grows to the right, on the same line, into:

`[+] [default agent] [default terminal] [new note] [▾]`

| Item | Icon | Tooltip | Click | Drag |
|---|---|---|---|---|
| Default agent | The default profile's brand icon (`pane_agents()`, the profile today's menu marks as default) | `New <agent> tab` + `NewAgentTab` shortcut | New tab with that agent | Leaf at the drop edge |
| Default terminal | `>_` | `New <shell> tab` (`project.default_shell`) + `NewShellTab` shortcut | New tab with that shell | Leaf at the drop edge |
| New note | Note icon (Lucide `notebook-pen`, or the nearest icon in the shipped set) | `New note` + `NewNote` shortcut | Create the note (section 7.2) and open it in a new tab | Create the note, leaf at the drop edge |
| `▾` | Chevron | `More…` | Opens today's menu (`new_menu_rows`: every profile, Shell, Empty tab) | Not draggable |

- Grows in 150 ms ease-out; with reduce-motion it shows at once.
- Collapses 150 ms after the pointer leaves both `+` and the row, so the pointer can travel to the items.
- Keyboard: Tab reaches `+`; ← / → move between items; Enter picks; Esc collapses.
- While a pane starts (`starting_pane`), `loading_dots` replace the row, as today.
- Items are `pressable(Look::Icon, ..)` (UI skill rule 11) with a visible focus ring.
- With no agent profile, the default agent item is hidden; `▾` still lists "Agent" as today.
- Detached windows show the same row (the note goes to the window's workspace).

### 7.2 New note

- Setting `[notes] folder` (string, default empty), in the Settings dialog under General, as a text field like the other path settings (the dialog has no folder picker kind).
  Empty means the workspace root; a relative path is relative to the root; an absolute path is used as is, for every workspace.
  It lives in `settings.toml`, which the daemon reads (`SettingsHolder`); a remote machine uses its own `settings.toml`.
- New request `fs.new_note { project_id, worktree_task_id }` returns `FsCreated { path }`.
  The daemon resolves the folder from its own settings, makes the name `note-YYYYMMDD-HHMMSS.md` in local time, adds `-2`, `-3`, ... while the name exists, creates missing folders, and writes an empty file (`file_ops::create`).
  `path` is relative to the root when the note is inside it, else absolute.
- `fs.read` and `fs.write` accept only root-relative paths today (`docs::relative_path`).
  They gain one exception: an absolute path inside the daemon's absolute notes folder (normalized, no `..`) is accepted; any other absolute path is still refused with today's error.
  The protocol bump to 60 covers the new request and the new meaning.
- The note opens kept (not preview), in source mode, with the cursor in the editor.
- Errors (no write access, disk full, bad folder) show `toast_error` with the reason; nothing opens.
- `Command::NewNote` in the palette ("New note"); its default key is chosen in the plan after checking the keymap for conflicts.

## 8. Removed

Clean cutover, no aliases:

- `View::Editor`, `View::Diff` (`board.rs`), `view_chip`, `AreaItem::{Editor, Diff}` and the `area_items` arguments for them, the `render_editor` / `render_diff_view` branches of `render_main`.
- `shell/file_tabs.rs`, `shell/file_tabs_render.rs`, `ReorderList::FileTabs`, `follow_file_tabs`, `has_file_tabs`, `Open::{Transient, Kept}` (the preview flag lives in the leaf).
- `GitView.open` and `diff_open` (replaced by "a Diff leaf exists").
- `split::drop_axis`.
- File tab commands (cycle / nth file tab, close file tabs): tab cycling and closing become the general tab commands, which now include file tabs.
- Tests that pin the removed strip, chips or the right / bottom-only drop.

## 9. Errors and edge cases

| Case | Behavior |
|---|---|
| A leaf's file was deleted on disk | The pane shows "This file was removed." with a Close button. |
| The notes folder setting changed while a note is open from the old folder | The note's read is refused; the pane shows today's error state with the reason and Close. |
| Read error (too large, not UTF-8, I/O) | Today's error state with Retry, inside the pane. |
| A stored leaf does not parse | Dropped with `warn!`; the rest loads. |
| The workspace of a tab is closed | Its tabs, file tabs included, stay stored and come back with it. |
| Two windows show tabs of one workspace | One store; one pane per file across both (section 4.2). |
| The note folder cannot be created | `toast_error` with the daemon's reason, nothing opens. |
| A layout holds an absolute file leaf and the notes folder setting changed | The leaf's read is refused; the pane shows today's error state with the reason and Close. |
| Drag while a dialog is open | Ignored. |

## 10. Testing

Pure logic, with unit tests next to it (no gpui):

- `split.rs`: JSON round trip of the three leaf kinds and of a layout written by protocol 58; `validate` rejects duplicate keys; `Edge::nearest` for each edge and the ties; `insert_at` for the four edges; `move_pane` and `remove` with file and diff leaves; an unparsable leaf is dropped.
- `tabs.rs`: `find` across tabs; preview tab reuse and keep; a session ending keeps a tab that still holds a file.
- `documents.rs`: load on first draw, drop when unused and clean, keep when dirty.
- `status.rs`: `pane_stripe` precedence table.
- `theme.rs`: `pane_gap` contrast rule over every theme.
- `mkrew-core` settings: `[notes] folder` parses, defaults to empty, and appears in the settings table and template.
- Daemon: `cut` and `layout_sessions` skip file and diff leaves; `tab.close` of a tab with only file leaves removes no session (e2e in `crates/mkrew-daemon/tests/daemon`).
- Daemon: `fs.new_note` makes the name, adds `-2` on a clash, creates missing folders, answers a relative path inside the root and an absolute one outside it; `fs.read` / `fs.write` accept an absolute path inside the notes folder and refuse one outside it, `..` included.

Smoke on the real app (PrintWindow screenshots, dark and light), saved to `docs/screenshots/tern-panes-<view>-<theme>.png`:

- Four panes: agent, shell, a `.rs` file and a Markdown split, one focused and one needing input.
- A Files tree row dragged to each of the four edges (overlay visible).
- The `+` row expanded.
- The diff pane following ↑ / ↓ in the Git panel.
- Restart: the file and diff leaves come back in place.
- A file tab torn off into a detached window.

## 11. Docs

- ADR 0108 "Tern panes": typed leaves, the Documents store, one pane per file per workspace, the four-edge drops, the `+` row, notes (`fs.new_note`, the notes folder exception of `fs.read` / `fs.write`), protocol 60; supersedes ADR 0080 and the chip and "Alternatives" parts of ADR 0106; amends ADR 0041.
- SPEC: §5 (`terminal_tab.layout`), §7 (protocol 60; `fs.new_note`; `fs.read` / `fs.write` accept the notes folder), §10 (Views, Title bar, Terminals: file and diff tabs, drops, `+` row), §12A (Editor becomes the file pane; no file tab strip), §12B (Diff becomes the diff pane), §13 (removed file tab keys, `NewNote`), §14 (`pane_gap`, `pane_focus_stripe`, the stripe table, gap 8, the `+` row), §20.0 (`[notes] folder`), §20B.5 (drag and drop).
- `docs/HUMAN_CHECKS.md`: "2026-11-23 Tern panes" with the smoke list above.
- `docs/PROGRESS.md`: one note line when the work lands.

## 12. Order of work

1. Leaves, keys and the four-edge model in `split.rs` / `tabs.rs`; daemon `cut` / `layout_sessions`; protocol 60; ADR 0108 draft.
2. Pane look: `pane_gap`, gap 8, stripe, title bar color.
3. Documents store; file leaf rendering; `open_document`; remove the Editor view and the file tab strip.
4. Diff leaf; remove the Diff view.
5. Four-edge drops for every draggable; composer drop for tree rows.
6. The `+` row; `[notes] folder`; `NewNote`.
7. SPEC, HUMAN_CHECKS, screenshots.

## 13. Out of scope

- Tabs inside a pane (Tern's per-pane tab bars).
- The same file in two panes at once.
- Board and Docs as tabs.
- Picture-in-picture panes.
- Restoring unsaved edits after a restart.
