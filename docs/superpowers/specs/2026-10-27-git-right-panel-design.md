# Git in the right panel - design

- Date: 2026-10-27
- Status: approved in chat, awaiting spec review
- ADR to write with the implementation: `docs/decisions/0084-git-in-the-right-panel.md`
- Follows sub-project 1: `docs/superpowers/specs/2026-10-27-files-right-panel-design.md` (ADR 0081).

## Goal

Remove the top-level Git view.
Git moves into the right panel as a third tab, the way VS Code's Source Control sits in its side bar.
Clicking a changed file shows its diff in the center area, the way a tree click shows the editor.

## Scope

In:
- The right panel gets a third tab: `Files │ Session │ Git`.
- The Git tab holds today's whole Git side column: header (worktree chooser, repo strip, branch card with Fetch / Push, `Working / Vs. base / History`, base line, `Viewed N/M`), filter row with the "Changed by" session filter, the change list (or History with its lane graph), and the commit box.
- `View::Git` becomes `View::Diff`: the diff card (toolbar, unified / split, hunks, comment margin, Send to agent) in the center, listed in the view bar only while a diff is open.
- `ShowGit` becomes `ShowGitPanel`; new `ShowDiff`.
- The refresh rule of SPEC §12B follows the new places.

Out (YAGNI; may come later):
- Diffs as tabs of the Editor view, or several diffs open at once.
- A center History view (History stays in the panel tab).
- Moving the "Changed by" filter or any list control out of the panel.

## Layout and navigation

```
┌ title bar: [Terminals│Board│Docs│Editor*│Diff*]  … ◨ ┐
├ sidebar ┬ center ─────────────────────────┬ right panel ──────────┤
│         │ Terminals / Editor / Diff:      │ [Files│Session│Git 3] │
│         │  Diff = toolbar + unified/split │  Git: header, branch, │
│         │  + hunk actions + comments      │  Working/Vs.base/     │
│         │  margin + Send to agent         │  History, filter+By,  │
│         │                                 │  list, commit box     │
└─────────┴─────────────────────────────────┴───────────────────────┘
* Editor: while ≥1 file tab is open.  Diff: while a diff is open.
```

- **View bar:** `Terminals │ Board │ Docs`, then `Editor` while the shown workspace has a file tab, then `Diff` while it has an open diff.
  The `Git` entry is gone.
  The changed-file count chip moves from the view bar to the panel's `Git` tab label (`tinted_chip`, shown when the count is above 0).
- **Right panel:** tabs `Files │ Session │ Git`; a click switches the tab.
  The panel shows beside the Terminals, Editor and Diff views.
  One toggle (`◨`, `Ctrl+Shift+Y`) and one width (`[ui] inspector_width`, 240-560 px) for all three tabs.
  From the Board or Docs view the toggle shows Terminals with the panel open, as today.
- **Remembered:** `[ui] right_panel` accepts `"files" | "session" | "git" | "hidden"`, default `"hidden"`.
- **Git tab at narrow widths:** the column takes the panel's width (`w_full`) instead of the fixed 320 px.
  The filter row wraps: the filter field on one line, the "By:" button below it when both do not fit.
  The repo strip measures its room from the panel's width (`inspector_width`), not a constant.
  File names and branch names truncate, as today.
  `theme::GIT_FILE_LIST_WIDTH` is removed.
- **Opening a diff:** a click or Enter on a file row (Working, Vs. base, or a file of an expanded History commit) opens the diff and shows the Diff view.
  The keyboard stays in the Git list, the way a tree click keeps it in the tree.
- **Closing a diff:** the `×` button at the end of the diff toolbar, or `Esc` in the Git list after the existing Esc steps (close an open Git menu, then clear picked diff lines).
  Closing removes `Diff` from the view bar and shows Terminals.
  A diff also closes when the list has no file left to show (a commit, a discard of the last file, a clean worktree).
- **Kept while open:** when the selection moves by itself (a staged file moves to Staged, `reselect` picks the next file), the diff stays open on the new selection.
- **Ctrl+Shift+4 (`ShowGitPanel`, was `ShowGit`):** opens the panel on the Git tab and gives the list the keyboard.
  From the Board or Docs view it shows Terminals first; from the Editor or Diff view it keeps that view.
- **Ctrl+Shift+7 (`ShowDiff`):** shows the Diff view while a diff is open (every keymap command has a default key; 7 follows Editor's 6).
  `keymap.toml` users with a `show_git` entry get the usual unknown-name error and the defaults (clean rename, no alias).
- **Entry points:** the Changes card's "Review diff", a task's "View changes", a turn's "Diff since", the worktree list and the title-bar branch chip all open the panel on the Git tab with the right repo / section and open the first file's diff in the Diff view.
  They share one function, `open_git(project_id, section, window, cx)`.
- **Unchanged:** staging, discard (always confirmed), commit box (`Ctrl+Enter`, ✦, Stage all & commit), Fetch / Push, History (last 50, lane graph, at most 8 lanes), Viewed boxes, Unified / Split (`d`, `[ui] diff_mode`), hunk navigation (`n` / `p`), diffs over 1 MB with "Open in Zed", diff comments and Send to agent, dialogs.

## Keyboard

- `GIT_CONTEXT` stays on the Git list (`git_focus`), now inside the panel tab.
- The Diff view takes no focus of its own, as today: picking lines or clicking in the diff returns the keyboard to `git_focus`.
  So every Git single key (`s`, `u`, `b`, `h`, `n`, `p`, `d`, …) and the comment keys work while a diff is shown.
- The header, filter and commit box stay outside `GIT_CONTEXT`: typing never stages.

## Refresh (SPEC §12B)

- "The Git view is open" becomes "the Git panel tab is on screen, or the Diff view is shown" (`git_shown()`).
- `git.changed` refreshes the workspace's Git state only while `git_shown()`.
- Showing the Git tab or the Diff view refreshes it when neither was shown before.
- The cheap status for sidebar markers and the title-bar summary is unchanged.

## Code changes (`crates/mkrew-ui`, plus settings)

|File|Change|
|---|---|
|`board.rs`|`View::Git` → `View::Diff` (title "Diff", `ShowDiff`), placed after `Editor` in `ALL`; `bar(docs, editor, diff)` lists `Diff` only while a diff is open.|
|`git_view.rs`|`GitView.open: bool` ("a diff is open"): set by a file click / Enter and the entry points; cleared by `×`, `Esc` and an empty list; `reselect` keeps it.|
|`shell/workspace.rs`|`Shell::view()` maps a stored `Editor` without file tabs and a stored `Diff` without an open diff to Terminals, in one `match`.|
|`inspector.rs`, `mkrew-core` settings|`PanelTab::Git`, `RightPanelMode::Git` (`"git"`); `panel_shown()` includes the Diff view.|
|`shell/inspector.rs`|Tab row `Files │ Session │ Git` with the count chip; the Git body calls `render_git_panel`.|
|`shell/git_render.rs`|`render_git` splits into `render_git_panel` (the old column, `w_full`, wrapping filter row) and `render_diff_view` (the diff card in the center).|
|`shell/git_diff.rs`|`×` close button at the end of the diff toolbar.|
|`shell/git_menus.rs`|The repo strip's room comes from `inspector_width`.|
|`shell/git.rs`|`git_shown()`; the `git.changed` guard and `git_commit_shown` use it; `Esc` closes the diff as the last step; `open_git` replaces `review_diff` and the other entry points' code.|
|`shell/chrome.rs`|`render_main` draws the Diff view with the panel; the branch chip calls `open_git`; the view bar's Git chip moves to the panel tab.|
|`shell/board.rs`, `shell/worktrees.rs`, `shell/files.rs`|Entry points and refresh guards use `open_git` / `git_shown()`.|
|`shell/comments.rs`, `shell/navigation.rs`|Focus returns to `git_focus` while `git_shown()`; comment context `Diff` in `View::Diff`.|
|`shell/composer.rs`, `shell/send_dialog.rs`|Picked diff lines come from `View::Diff`.|
|`keymap.rs`|`ShowGit` → `ShowGitPanel` (`show_git_panel`, `ctrl-shift-4`); new `ShowDiff` (`show_diff`, `ctrl-shift-7`).|
|`palette.rs`|"View: Git panel" (`ShowGitPanel`, needs a workspace), "View: Diff" (`ShowDiff`, needs an open diff); `Git*` commands while `git_shown()`; `AddComment` / `SendComments` / `SendSelection` for diffs in `View::Diff`.|
|`theme.rs`|`GIT_FILE_LIST_WIDTH` removed.|
|`settings_fields.rs`|`right_panel` stays a hidden key; `"git"` is a valid value.|

`GitView` stays one per workspace, now drawn in two places (panel tab and Diff view), as `FilesView` is.

## Docs

- ADR 0084 "Git in the right panel": supersedes in part ADR 0081 (two-tab panel) and the ADRs that placed the Git side column right of the diff.
- SPEC §10: layout sketch, view bar (`Diff`), the Git tab, `[ui] right_panel = "git"`, the Changes card's "Review diff".
- SPEC §12B: the Git tab, the Diff view, opening / closing a diff, the refresh rule.
- SPEC §12A: "what the Git view calls unstaged" → "what the Git tab calls unstaged".
- SPEC §13: `Ctrl+Shift+4` = Show Git panel, `Ctrl+Shift+7` = Show diff.
- SPEC config sample: `right_panel`.
- `docs/HUMAN_CHECKS.md`: one entry for the visual check.

## Testing

Unit tests (behavior only):
- The view bar lists `Diff` exactly while a diff is open, after `Editor`.
- `GitView`: a file click / Enter opens; a `reselect` after staging keeps it open on the new file; an empty list closes it; `×` / `Esc` close it.
- `RightPanel` and `ui.right_panel` round-trip `"git"`; other values are rejected.
- `ctrl-shift-4` is bound to `ShowGitPanel`, `ctrl-shift-7` to `ShowDiff`.
- Palette: `ShowDiff` only with an open diff; `Git*` commands while the Git tab or Diff view is shown.
- The existing `git_view` tests stay; `board`, `palette`, `keymap` tests move to the new names; `insta` snapshots of the palette and keymap are updated.

Smoke run on the real app:
- `Ctrl+Shift+4`: the panel opens on the Git tab, the list has the keyboard.
- Click a file: the Diff view shows it and `Diff` is in the view bar.
- `s`, `n`, `d` work while the diff is shown.
- Pick lines, comment, Send to agent.
- Drag the panel to 240 px: the filter row wraps, the repo strip does not overflow.
- `×` on the diff: `Diff` leaves the view bar and Terminals shows.
- History: click a commit, its first file's diff shows.
- "Review diff" on the Changes card opens the right repo.
- Restart: the panel comes back on the Git tab.
