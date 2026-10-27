# 0084 - Git in the right panel

- Date: 2026-10-27
- Status: accepted
- Milestone: M7

## Context

The Git view took the whole main area: reviewing what an agent changed meant leaving its terminal.
ADR 0081 moved the file tree into a right panel beside the terminals; the Git view's side column (header, list, commit box) has the same shape.

## Options

1. Keep the Git view - pros: no change / cons: changes and agents never on screen together.
2. Draw the existing `GitView` in two places: the list in a `Git` tab of the right panel, the diff in a center `Diff` view shown only while a diff is open - pros: no new model, like VS Code's Source Control / cons: the list must fit 240 px.
3. Diffs as tabs of the Editor view - pros: one center tab strip / cons: `FileTab` assumes a file (save, unsaved prompts, change gutter), so every tab path needs a tab kind.

## Decision

Option 2.
The right panel has `Files │ Session │ Git` tabs; `[ui] right_panel` accepts `"git"`.
A click or Enter on a file opens the `Diff` view; `×`, `Esc` or an emptied list close it.
History stays in the Git tab.
`Ctrl+Shift+4` shows the Git tab (`show_git_panel`), `Ctrl+Shift+7` the diff (`show_diff`).
Git refreshes while its tab or the Diff view is on screen.

## Consequences

Supersedes in part ADR 0081 (two panel tabs).
SPEC §10, §12A, §12B, §13 and the config sample change in the same commit.
`keymap.toml` entries named `show_git` are reported as unknown (no alias).
