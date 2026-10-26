# Files view tabs — design

- Date: 2026-10-26
- Status: approved in chat, awaiting spec review
- ADR to write with the implementation: `docs/decisions/0080-files-view-tabs.md`

## Goal

Open several files at once in the Files view (SPEC §12A) and switch, close, pin and reorder
them like VS Code / Zed. Today a click on another file replaces the shown one (and asks
"Save / Don't save / Cancel" first when it has unsaved text).

## Scope

In:
- A tab strip over the file pane: one tab per opened file.
- VS Code preview tab: a single click opens a temporary (italic) tab that the next single click
  replaces.
- Close: ✕, middle click, `Ctrl+W` and `Ctrl+Shift+W`, with a prompt for unsaved text.
- Right-click menu: Close, Close others, Close to the right, Close all, Pin / Unpin, Copy path.
- Pin: pinned tabs come first, show a pin icon and are skipped by every bulk close.
- Drag to reorder (within the pinned group or within the unpinned group).
- Switch tabs with `Ctrl+Tab`, `Ctrl+Shift+Tab` and `Ctrl+Alt+1–9`.

Out (YAGNI; may come later):
- Restoring tabs after a restart: tabs live in memory only. No protocol, storage or migration
  change.
- Revealing the active tab's file in the tree on switch (the tree cursor stays where it is).
- One find bar per tab.
- Split editors / several tab groups.

## Model (`crates/mkrew-ui/src/files_view.rs`)

The per-file fields of `FilesView` move into a new struct; the tree stays in `FilesView`.

```rust
pub struct FileTab {
    pub path: String,
    pub content: Content,
    pub mode: Mode,           // .md: Preview | Source
    pub preview: bool,        // VS Code preview tab (italic), replaced by the next single click
    pub pinned: bool,
    pub opening: u64,
    text_id: u64,
    md_preview: Option<MdPreview>,
    preview_asked: Option<Stamp>,
    pub preview_selection: Option<Selection>,
    pub block_edit: Option<BlockEdit>,
    saving: bool,
    reading: bool,
    read_again: bool,
    stale: bool,              // git.changed arrived while it was not the active tab
    pending_line: Option<u32>,
    pub scroll_top: usize,    // first visible line, restored when the tab is shown again
}

pub struct FilesView {
    // tree fields unchanged: project_id, worktree_task_id, show_ignored, dirs, expanded,
    // rows, cursor, listing, list_again, generation, sent_in
    pub tabs: Vec<FileTab>,   // pinned tabs first
    pub active: Option<usize>,
    pub find: Option<TextFind>,   // shared: searched again on the active tab's text
    pub zed_error: Option<String>,
}
```

Invariants:
- At most one tab per path.
- At most one tab has `preview == true`.
- Every pinned tab comes before every unpinned tab.
- `active` is `None` exactly when `tabs` is empty.
- A pinned tab is never a preview tab (pinning makes it permanent).

The current accessors keep their meaning for the active tab, so most callers stay as they are:
`file()` (was the `file` field), `shown()`, `content()`, `mode`, `find_and_editor()`, and the
editor key handlers. Answers are routed by path, not by "the opened file":
- `read(path, result)` lands in the tab with that `path`. When no tab has it (closed
  meanwhile), it is dropped.
- `saved(path, point, written)` does the same. When the tab is gone, `files_view::unshown_save`
  produces the message, as now.

## Behavior

| Action | Result |
|---|---|
| Single click on a file in the tree, Enter on the tree cursor, Quick open, Ctrl+Click on a terminal path, "Open at a line" | Already in a tab → that tab becomes active. Otherwise a preview tab opens. It replaces the current preview tab when that tab has no unsaved text; else the old one becomes permanent and the new one is added. New tabs go right of the active tab (or after the pinned group when the active tab is pinned). |
| Double click on a tree file, double click on a tab, the first edit in a tab, "New file…" | Tab becomes permanent (`preview = false`). |
| Click on a tab | Becomes active. |
| ✕ (on hover and on the active tab), middle click, `Ctrl+W`, `Ctrl+Shift+W` (`CloseTab` while Files is shown) | Closes that tab (the active one for keys). Unsaved → "Save / Don't save / Cancel" (existing `leave_file` dialog). The active tab closed → the right neighbour becomes active, else the left one. |
| Right-click a tab | Menu: Close · Close others · Close to the right · Close all · Pin / Unpin · Copy path (absolute). Bulk closes skip pinned tabs and ask once for each unsaved tab, in strip order. Cancel stops the remaining closes. |
| Pin / Unpin | Pin moves the tab to the end of the pinned group and makes it permanent. Unpin moves it to the start of the unpinned group. |
| Drag a tab | Drop position is limited to its group (pinned or unpinned). |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous tab, wrapping around. |
| `Ctrl+Alt+1–9` | Tab N (9 = last, like VS Code), ignored when there is no tab N. |
| Unsaved text | `●` before the tab title (replaces ✕ until hover, like VS Code). |
| Switch worktree (chooser, link into another worktree), close the window, Quit UI | One prompt per unsaved tab before leaving (as today for the single file). A new worktree starts with no tabs. |
| Switch workspace or view | Tabs and unsaved text are kept (as today). |

Removed from SPEC §12A: "Opening another file … with unsaved changes asks Save / Don't save /
Cancel". Opening another file no longer leaves the current one.

## Sync

- `git.changed`: the active tab is read again (current path). Every other tab gets
  `stale = true` and is read again when it becomes active. A tab with unsaved text gets the
  existing "Changed on disk — Reload · Keep mine" banner when its read finds a new hash.
- A file deleted on disk: its tab stays, its read fails and shows the existing failed state.
- Find bar: one per view. Switching tabs runs `search()` on the new active text. Find on a
  `.md` tab switches that tab to Source (as now).
- Markdown preview pages: `md_preview` is per tab; the shell's single `file_page`
  (`preview::Page`) is rebuilt for the active tab on switch (another tab starts at the stored
  scroll, a re-read keeps it, as today).

## UI (`crates/mkrew-ui/src/shell/files_render.rs`)

- Tab strip on top of the file pane, above the existing header (path, Save, Find, Preview |
  Source, Open in Zed). Horizontal scroll when tabs overflow. The active tab is scrolled into
  view on switch.
- Tab: file-type icon (as in the tree), file name; the parent folder in muted text only when
  two tabs share a name; italic title for the preview tab; pin icon for pinned tabs
  (`icon/lucide/pin.svg`); `●` / ✕ on the right.
- The active tab: `selected` background and a 2 px accent line along its bottom, like the
  terminal tab strip (ADR 0054). Hover: `hover` background, ✕ shown.
- Tooltip: the relative path.
- Empty state (no tabs): the existing "no file opened" state.
- Every color, spacing, radius and font size comes from `theme.rs`. New tokens only if no
  existing one fits (expected: none; the terminal tab strip tokens are reused).

## Keys (`crates/mkrew-ui/src/keymap.rs`, SPEC §13)

- New command `CloseFileTab` = `ctrl-w`, context `FILES_CONTEXT` (allowed by the ADR 0028 rule
  that plain Ctrl+letter is used only in Files). Configurable like every command, listed in the
  command palette as "Files: Close tab".
- `NextTab`, `PreviousTab`, `Tab(n)` and `CloseTab` act on file tabs while the Files view is
  shown, and on terminal tabs otherwise. No new default for them.
- Palette: "Files: Close other tabs", "Files: Close all tabs", "Files: Pin tab" (no keys).

## Shell changes (`crates/mkrew-ui/src/shell/`)

- `files.rs`: `read_file` / read answers and `save_file` / save answers carry the path to the
  tab. `files_event` marks inactive tabs stale. `leave_file` becomes a check over a list of tabs
  (one tab for close, several for bulk close and leaving the worktree).
- `navigation.rs`: `open_file_at` / `reveal_file` / `open_quick_pick` open a preview tab instead
  of calling `leave_file` first. `open_file_at` into another worktree still asks first.
- `files_render.rs`: the tab strip and its menu. Drag reuses `shell/reorder.rs` (the same chip,
  dimmed source and accent gap line as terminal tabs): a new `ReorderList::FileTabs(Ulid, bool)`
  (workspace, pinned group), so a tab never leaves its group.
- `sessions.rs`: route `NextTab` / `PreviousTab` / `Tab(n)` / `CloseTab` / `CloseFileTab` to the
  Files view when `View::Files` is shown.
- `composer.rs` `prompt_vars` / `picked_lines`: use the active tab (no behavior change).

## Testing

Headless tests in `files_view.rs` (no GPUI):
- A single click opens a preview tab; a second single click replaces it; with unsaved text the
  first tab stays and becomes permanent.
- Opening a path that already has a tab activates it and adds nothing.
- An edit makes the preview tab permanent.
- Closing the active tab activates the right neighbour, else the left one; closing the last tab
  leaves `active == None`.
- Close others / to the right / all skip pinned tabs.
- Pin moves a tab to the end of the pinned group; unpin to the start of the unpinned group;
  reorder cannot move a tab across groups.
- A read answer for a tab that is not active lands in that tab; one for a closed tab is
  dropped; a save answer for a closed tab goes to `unshown_save`.
- `git.changed` re-reads the active tab only and marks the others stale; activating a stale
  tab asks for its read.
- `Ctrl+Alt+9` selects the last tab; next/previous wrap around.

Keymap test: `ctrl-w` maps to `CloseFileTab` in `FILES_CONTEXT` and does not break
`agent_keys_are_unbound_and_reach_the_pty`.

Manual: screenshots of the strip (dark and light) into `docs/screenshots/`, or a line in
`docs/HUMAN_CHECKS.md` "To verify".

## Docs

- ADR `docs/decisions/0080-files-view-tabs.md` (re-check the next free number before writing).
- SPEC §12A: new "Tabs" bullet; remove the "opening another file asks" sentence; worktree switch
  and window close ask for every unsaved tab.
- SPEC §13: `Ctrl+W` in Files; tab keys act on file tabs in Files.
