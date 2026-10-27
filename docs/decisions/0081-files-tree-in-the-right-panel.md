# 0081 - Files tree in the right panel

- Date: 2026-10-27
- Status: accepted; superseded in part by 0084
- Milestone: M7

## Context

The Files view took the whole main area: to open a file or hand one to an agent, the user left the terminals.
The session panel (ADR 0038) already sat right of the terminals with a resizable width.

## Options

1. Keep the Files view - pros: no change / cons: files and agents never on screen together.
2. Draw the existing `FilesView` in two places: the tree in the right panel, the editor in the center - pros: no new state, the tree stays next to the agents / cons: the panel holds two tabs.
3. A separate tree entity synced with the editor - pros: separate code / cons: two-way sync of cursor, tabs and worktree target.
4. A drawer over the panes - pros: less layout / cons: cannot stay open, covers the drop target.

## Decision

Option 2.
The right panel has `Files │ Session` tabs, one toggle (`◨`, Ctrl+Shift+Y) and one width; it shows next to Terminals and the Editor and is remembered as `[ui] right_panel`.
The center `Editor` view exists only while files are open.
Tree rows drag onto panes and type `composer.file_ref` (agents) or the bare path (shells).

## Consequences

Supersedes in part ADR 0050 (tree right of the editor) and ADR 0038 (the session panel is a tab, and no longer closed at every start).
SPEC §10, §12A, §13, §20B.5 and the config sample change in the same commit.
Moving Git into the panel followed in ADR 0084.
