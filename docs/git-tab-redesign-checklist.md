# Git tab redesign: feature inventory checklist

Audit of the Git view before the redesign (2026-10-11), ticked after the redesign. Each row is a
feature that existed before. "Before" is where it lived; "Now" is where it lives after the
redesign. A tick means: the code path is kept and wired, `cargo clippy -D warnings` and the
`mkrew-ui` / `mkrew-core` tests pass. The visual click-through is listed in
`docs/HUMAN_CHECKS.md` ("To verify — 2026-10-11 Git tab redesign").

Code map after the redesign:

| File | What it holds |
|---|---|
| `crates/mkrew-ui/src/shell/git_render.rs` | side panel layout, branch header, Fetch/Push, view tabs, base line, target chooser, commit box, empty states |
| `crates/mkrew-ui/src/shell/git_menus.rs` | repo strip + Repositories menu, Filter files field, "By: ▾" menu, Git menu open/close |
| `crates/mkrew-ui/src/shell/git_rows.rs` | file list (`uniform_list`, 30 px rows), group headers, file row, commit row, `change_counts`, `row_action` |
| `crates/mkrew-ui/src/shell/git_diff.rs` | diff toolbar, "⋯" menu, diff rows in a `list()` (wrapped or not), hunk headers, Split hatch, selection bar |
| `crates/mkrew-ui/src/shell/git_dialogs.rs` | Discard dialog (preview via `git_diff_preview_row`), Base dialog |
| `crates/mkrew-ui/src/shell/git.rs` | actions: load, select, keys (Esc clears picked lines), stage, commit, generate, fetch/push, Zed |
| `crates/mkrew-ui/src/git_view.rs` | view model: groups, filter, folded groups, file position, stage-all request, tracking line |
| `crates/mkrew-ui/src/theme.rs` | tokens (dark and light) |
| `crates/mkrew-core/src/settings.rs` | `[ui] diff_wrap` (default `true`), next to `diff_mode` |

## 1. Repo, target and branch

| ✓ | Feature | Before | Now |
|---|---|---|---|
| [x] | Repo chooser for multi-repo workspaces; hidden with < 2 repos and while a task worktree is shown | chip row in header | 38 px repo strip: tabs (accent underline on the shown one), amber dot on repos with changes, ▾ "Repositories" menu (search, name, branch, Clean / N changes). Only the shown tab when the tabs don't fit |
| [x] | Target chooser "Showing: Project \| <task worktree>" | chip row in header | same chip row, above the repo strip |
| [x] | Switching repo/target keeps the commit draft, resets scroll | `retarget_git` | unchanged |
| [x] | Branch name / "Detached HEAD" / "Git" while loading | header title | branch header, bold (plain text: no branch UI exists) |
| [x] | Upstream line | `meta_line` under branch | "Tracking origin/x · 2 commits ahead" / "· up to date" / "No upstream" (`git_view::tracking`) |
| [x] | Fetch: "Fetching…", faded while Push runs, tooltip | text action | icon button with tooltip, same states |
| [x] | Push: "Pushing…", disabled without branch/upstream or while Fetch runs, tooltip says why | text action | "Push N" button; muted at 0 ahead but clickable; disabled cases unchanged |
| [x] | Fetch/Push error, wrapped | under header | under the branch header |
| [x] | Refresh error | end of header | end of header |
| [x] | Palette: Git: Fetch / Push / Discard… | `palette.rs` | unchanged |
| [x] | Title bar branch chip, `Git N` badge, inspector "Review diff" | `chrome.rs`, `inspector.rs` | unchanged |

## 2. View tabs and base

| ✓ | Feature | Before | Now |
|---|---|---|---|
| [x] | Working / Against base / History with tooltips and keys (`b`, `h`) | header segment | three equal segments "Working N" / "Vs. base" / "History", same tooltips + keys |
| [x] | `b` / `h` toggles | keymap `GIT_CONTEXT` | unchanged |
| [x] | Base line + "Change…" / "Set base…" | header | only on the Vs. base tab |
| [x] | Base dialog | `render_git_base` | unchanged |
| [x] | Task/turn changes label with "Close" | header base line | same slot (base line) while Since is shown |

## 3. "Changed by" filter (SPEC §20C.4)

| ✓ | Feature | Before | Now |
|---|---|---|---|
| [x] | Chips All / per session / named / ended | wrapping chip row | "By: All ▾" in the filter row; menu: All sessions (✓ when active), AGENTS, OTHER SESSIONS (truncated); same `filter_git_changed_by` |
| [x] | Hidden while no session changed files and in History | `changers()` empty | same |
| [x] | Tooltip "Only the files X changed in the last 24 h" | chip | menu row |
| [x] | Empty filtered list "No changes here by X." + "Show all" | `render_git` | unchanged |
| [x] | Commit box shown only for kept files | `commit_box_shown` | unchanged |

## 4. File list

| ✓ | Feature | Before | Now |
|---|---|---|---|
| [x] | Groups with counts | header rows | uppercase headers with chevron, click folds (↑ ↓ skip folded groups); "Unstaged" reads "Changes"; Staged always listed, empty hint "Nothing staged. Stage a file or a hunk." |
| [x] | Row: kind letter + tooltip, name, folder, +/− | `git_file_row` | 30 px row; M/R/C/T in `git_modified`, ? in `git_untracked`; name truncates, folder `flex_none` |
| [x] | Selected row raised, focus ring | `git_file_row` | `selected` fill + 1 px accent ring (stronger while focused) |
| [x] | Per-row Stage / Unstage on hover or selected | `row_action` | unchanged |
| [x] | Click selects and loads the diff | `select_git_file` | unchanged |
| [x] | ↑ ↓ / j k move, diff scrolls to top | `git_key_down` | unchanged, plus ‹ › in the diff toolbar and ↑ ↓ from Filter files |
| [x] | `s` / `u` | `stage_key` | unchanged |
| [x] | History commit rows, expanded commit's files, notes | `git_commit_row`, `ListRow::Note` | unchanged |
| [x] | History empty / failed / loading states | `render_git` | unchanged |
| [x] | Status failed + Retry; Since failed | `render_git` | unchanged |
| new | Filter files (client-side fuzzy on the path; Esc clears, ↑ ↓ move) | — | filter row, `GitView::filter` |
| new | Stage all / Unstage all on group headers (hover) | — | one existing `git.stage` / `git.unstage` with every path of the group |

## 5. Commit box

| ✓ | Feature | Before | Now |
|---|---|---|---|
| [x] | Multi-line message, Enter / Ctrl+Enter / Esc / paste, `c` focuses | `git_commit_box` | pinned bottom, placeholder "Summary, then a blank line and details", "N / 72" summary counter |
| [x] | "Nothing staged" / "N staged files" | footer | footer status text |
| [x] | ✦ generate, "✦ Writing…", tooltip | footer | "✦ Generate", same action |
| [x] | Commit disabled when nothing staged / busy / generating; tooltip Ctrl+Enter | footer | same; disabled style readable |
| [x] | Commit/hook error | under field | unchanged |
| [x] | Box only in Working with files or a draft | `commit_box_shown` | same; hidden in the clean state |
| [x] | No amend | — | no dropdown next to Commit |

## 6. Empty states

| ✓ | Feature | Before | Now |
|---|---|---|---|
| [x] | "No uncommitted changes." + "Against base" | `render_git` | ✓ tile, "Working tree clean", "No uncommitted changes in <repo>.", "Review against <base>" (same toggle, `b`) + "Open history"; card "<repo> has uncommitted changes · Switch"; filter row and commit box hidden |
| [x] | Base empty/failed states | `render_git` | unchanged text |
| [x] | Since empty states | `render_git` | unchanged |
| [x] | Diff panel without a selection, commit detail loading/failed + Retry | `render_git_diff` | unchanged |
| [x] | Loading states | various | unchanged |

## 7. Diff toolbar

| ✓ | Feature | Before | Now |
|---|---|---|---|
| [x] | File path; "from <old>" | header | status badge + `folder /` muted + **name**; rename label kept |
| [x] | +/− counts | header | toolbar |
| [x] | "Hunk N of M" / "M hunks", tooltip `n` / `p` | header | toolbar (wraps when narrow) |
| [x] | Unified / Split, `d`, `[ui] diff_mode` | header | segmented control, same persistence |
| [x] | Stage / Unstage file | ghost button | primary filled button |
| [x] | Discard… (asks first) | ghost button | red-text button, same dialog |
| [x] | Comment (Ctrl+Alt+M) | toolbar button | selection bar, "⋯" menu, shortcut |
| [x] | Send to agent (Ctrl+Shift+S) | toolbar button | selection bar, "⋯" menu (count in label), shortcut |
| [x] | No Comment/Send in History | `can_comment` | "⋯" and selection bar hidden in History |
| [x] | Open in Zed + error | toolbar | toolbar, error under it |
| [x] | Staging error | notice | unchanged |
| new | ‹ › previous / next file with "1 / 6" | — | toolbar start (same as ↑ ↓) |
| new | Wrap toggle, persisted `[ui] diff_wrap` (default on) | — | toolbar, next to Unified / Split |

## 8. Diff body

| ✓ | Feature | Before | Now |
|---|---|---|---|
| [x] | Collapsed gap "⋯ N unchanged lines" | Gap row | full-width subtle band, display only (not expandable: see below) |
| [x] | Hunk header, current ▸, `n`/`p` scroll to it | Hunk row | `diff_hunk_header` band, `@@ … @@` in the accent, section muted + truncated, ▸ kept; `n`/`p` scroll the list |
| [x] | Stage / Unstage hunk | `hunk_stage_button` | right end of the hunk header |
| [x] | Numbers, sign, add/del/modified colors, syntax colors | `git_diff_row` | gutter (old, new, sign) with its own stronger fill; text 12.5 px / 20 px; soft wrap by default |
| [x] | "(no newline at end of file)" | after the text | small pill at the end of the line |
| [x] | Binary / too large / mode-only / failed + Retry | `render_git_diff` | unchanged |
| [x] | Split rows | `git_split_row` | one row per pair (halves stay aligned when wrapped); empty side hatched |
| [x] | Click picks, Shift+click extends, hover preview, focus to list | `pick_on_click` | unchanged; picked gutter in the accent |
| [x] | Comment margin (draft, threads, Reply, Resolve/Reopen, errors) | `render_comments` | unchanged; it opens for a draft or threads. Picked lines alone get the selection bar instead of the margin hint (Docs keeps its hint) |
| [x] | Discard dialog preview | `git_diff_row` | `git_diff_preview_row` (Unified rows, unwrapped) |
| new | Selection bar "Line N selected · Comment Ctrl+Alt+M · Send to agent · Clear (Esc)" | — | under the last picked line; Esc in the Git list clears it |

## 9. Shortcuts

| ✓ | Key | Action |
|---|---|---|
| [x] | ↑ ↓ / j k | previous / next file |
| [x] | n / p | next / previous hunk |
| [x] | b / h | toggle mode / History |
| [x] | s / u | stage / unstage |
| [x] | c | focus commit box |
| [x] | d | toggle Unified / Split |
| [x] | Ctrl+Alt+M / Ctrl+Shift+S | comment / send comments |
| [x] | Ctrl+Shift+Z | open in Zed |
| [x] | Ctrl+Shift+5 | show Git view |
| [x] | Ctrl+Enter / Esc | commit / leave commit box |
| new | Esc (Git list) | close an open Git menu, else clear the picked lines |

## Skipped or changed because it is not purely UI

1. **Wrap persistence.** Added `[ui] diff_wrap` to `mkrew-core` settings (approved): one optional
   key, default `true`, old files still parse.
2. **Branch click.** No branch UI exists, so the branch name is plain text.
3. **Discard hunk.** Does not exist; not added.
4. **Amend.** Does not exist; no dropdown.
5. **"Show N unchanged lines".** The diff does not contain those lines; expanding needs daemon
   data. The band stays display-only.
6. **Wrap in the palette.** No command or key for Wrap (toolbar only).
7. **Theme values.** Mapped onto existing tokens (dark and light) instead of the spec's dark-only
   hex values.
