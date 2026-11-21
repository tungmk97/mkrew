# Review flow - Design

- Date: 2026-11-22
- Status: approved under the user's standing approval of the controller's recommendations (2026-11-22, "assume OK with your recommendations"); sub-project 4 of the Tern-inspired update (Polish 1, Theme gallery 2, Widget bar 3).
- ADR: `docs/decisions/0103-review-flow.md` (0098-0100 macOS plans, 0101 Theme gallery, 0102 Widget bar).
- Protocol: 55 -> 56 (55 is the Widget bar's; this branch merges after it and rebases the number if the order changes).

## 1. Goal

In Tern the user reviews an agent's changes in one pass: mark a file reviewed and the next one opens, step through hunks across files, throw away one bad hunk, and send any selected text to an agent.
A read-only survey of Mkrew (2026-11-22) found that it already has the large pieces: split and unified diffs, "Viewed N/M" with collapsed viewed files (ADR 0065), line comments on diffs sent to agents, per-hunk stage and unstage, Markdown Preview | Split | Source with editing (ADR 0083, 0092), and "Send selection" from the editor, the diff and the terminal.
This sub-project adds only the missing steps of the flow.

Success means:

- Marking a file viewed moves to the next file that is not viewed.
- `n` / `p` past the last / first hunk of a file go on to the next / previous file.
- One hunk of an unstaged change can be discarded, with the same confirm dialog as a file discard.
- "Send selection" (Ctrl+Shift+L) works from a Markdown preview selection and from the Docs view selection, with the file's path and line range.

## 2. Decisions (standing approval)

| Topic | Decision |
|---|---|
| Viewed then next | `v` on a file that is not viewed marks it and selects the next not-viewed file below it, wrapping to the top; when every file is viewed the selection stays. `v` on a viewed file only clears the mark. |
| Hunk stepping | `n` after the last hunk selects the next file in the list and its first hunk; `p` before the first hunk selects the previous file and its last hunk. Viewed files are skipped. No wrap at the list ends. |
| Hunk discard | A "Discard hunk" button next to "Stage hunk" on hunks of the Changes group (unstaged, modified files), and `x` with a hunk selected; the confirm dialog shows only that hunk. |
| Send from preview | The preview's character selection maps to its source blocks' line range; the sent text is the Markdown source of those lines, not the rendered text. |
| Send from Docs | The Docs view's comment selection (block or line range) is the selection. |
| `C` key for comments | Not changed: `c` stays "focus the commit box" and Ctrl+Alt+M stays "add comment". |
| History comments and viewed | Out of scope: they need anchors on a commit, not on the working file. |

## 3. Viewed then next

- `GitView::toggle_viewed` returns the request as today.
- When the toggle marks a file viewed, the view then calls a new `next_unviewed()` that searches `steppable` from the current index forward, wrapping, and selects the first file that is not viewed.
- The request for that file's diff goes out through the same `Requests` that `step_file` returns, so the diff loads as on an arrow key.
- The viewed state used for the search is the local optimistic one, so a fast second `v` does not jump back.

## 4. Hunk stepping across files

- `step_hunk(forward)` keeps its behavior inside a file.
- At the edge it calls `step_file(±1)` over `steppable` minus viewed files and sets a pending hunk target, "first" or "last".
- When the new file's diff arrives, the pending target selects that hunk and scrolls to it; a file with no hunks (binary, too large) keeps the pending target and the next `n` / `p` moves on again.
- The pending target clears on any other selection change.

## 5. Hunk discard

### 5.1 Protocol

- `GitDiscardParams` gains `#[serde(default)] pub hunk: Option<HunkTarget>`.
- With `hunk` set, `paths` must be empty; otherwise the request fails with `invalid_request`.
- `confirm` stays required.

### 5.2 Daemon

- The daemon builds the hunk patch the same way as for `git.stage` with a hunk, and runs `git apply -R` on the working tree (no `--cached`).
- A patch that no longer applies fails with the git error text, as a stale hunk stage does today.
- Only a hunk of an unstaged change of a modified file is accepted; anything else is `invalid_request`.

### 5.3 UI

- The hunk header gets a "Discard hunk" button (`pressable`, danger text) next to "Stage hunk", only in the Changes group.
- `x` with a hunk selected opens the confirm dialog for that hunk; with no hunk selected it is the file discard as today.
- The confirm dialog shows that hunk's lines and "Discard this hunk?".
- After success the diff reloads through the existing `git.status` / `git.diff` refresh.

## 6. Send selection from Markdown

- `picked_lines` (shell/composer.rs) gains two sources, tried after the editor's:
  - a Files tab in Preview or Split mode with a non-empty preview selection: the line range is the union of the source ranges of the blocks the selection touches;
  - the Docs view with a comment selection: its line range.
- The text sent is the file's source for that range, so the agent sees Markdown, with `path:a-b` as today.
- With no selection in either, Ctrl+Shift+L does what it does today.

## 7. Error handling

| Case | Behavior |
|---|---|
| Hunk discard on a stale hunk | Git's error in the existing error toast; the diff reloads. |
| Hunk discard with `paths` and `hunk` | `invalid_request` (programming error, never sent by the UI). |
| Daemon older than protocol 56 | Refused by the version check, as today. |
| Preview selection inside one block | The block's full line range. |
| Next unviewed when all are viewed | Selection stays; no request. |

## 8. Testing

- UI `git_view` tests: viewed-then-next with wrap and all-viewed; `n` / `p` across files, skipping viewed files and a binary file; the pending target cleared by a click.
- Protocol: `GitDiscardParams` with a hunk round-trips; the default is `None`.
- Daemon integration (real temp repo): discard one of two hunks leaves the other; a stale hunk fails; `paths` plus `hunk` is refused.
- UI: `picked_lines` from a preview selection over two blocks and from a Docs selection.
- `docs/HUMAN_CHECKS.md`: the review pass in dark and light (v, n/p across files, discard hunk dialog, send from preview).

## 9. Docs

- ADR 0103 (decisions above; why History stays out).
- SPEC §12A (send from preview), §12B (viewed then next, hunk stepping, hunk discard), §13 (`x` with a hunk), the protocol table (`git.discard` hunk).

## 10. Out of scope

- Comments and viewed marks in History.
- Rebinding `c` to comments.
- A continuous multi-file diff page.
- Discarding hunks of staged or untracked changes.

## 11. Settled while planning

- Hunk stepping across files uses its own `step_hunk_file`, which skips viewed files and never leaves the selected commit in History; `step_file` is unchanged.
- Viewed-then-next runs only when the marked file is the selected one; a checkbox on another row only marks it.
- A viewed file's collapsed diff is skipped by `n` / `p`, and `x` on it discards the file, not a hidden hunk.
- The daemon refuses a hunk discard on an untracked file (`invalid_request`); any other hunk goes to `git apply -R`, so a stale hunk fails with git's error as section 7 says.
- `paths` stays required on the wire: a hunk-only request sends `"paths": []`.
- Send selection picks by Files mode: Preview asks the page first; Split asks the editor's real selection, then the page; Source is unchanged; a page older than the editor text is not used.
- The hunk discard dialog disables its confirm button if a reload moved or removed the hunk.
- The palette offers Send selection in the Docs view.
