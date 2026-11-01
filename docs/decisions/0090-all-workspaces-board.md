# 0090 - All workspaces board

- Date: 2026-11-02
- Status: accepted
- Milestone: none (human request 2026-11-02: "a global board that shows every workspace")

## Context

The Board (SPEC §10) showed only the active workspace's tasks.
The human wants one board that shows the tasks of every workspace Mkrew knows.
The UI already lists every workspace's tasks (`task.list` without a project) and keeps them live, so no daemon or protocol change is needed.

## Decision

- A `This workspace | All workspaces` switch heads the Tasks side of the Board, next to the `Tasks | Jira` switch.
  It is one choice for every workspace, saved as `[ui] board_all_workspaces` (default `false`, also a switch on Settings → General), and the palette toggles it.
  It is not a separate view: the Board stays one of the workspace views, so its key (Ctrl+Shift+2), its card keys and its menus keep working unchanged.
- All workspaces shows the tasks of every listed workspace; archived workspaces are not listed, so their tasks stay hidden.
- One set of five status columns, not one row (swimlane) per workspace.
  Inside a column the cards go by workspace in sidebar order, then by position, so one workspace's cards stay together.
  Each card names its workspace (avatar and name) above its title.
- Positions stay per workspace (`task.move` is unchanged).
  A drag or Shift+←/→ moves a card among its own workspace's cards; dropped before another workspace's card, it lands after its own workspace's last card in that column.
- A card of another workspace opens in its own workspace: a click and Enter show its session there, and Changes switches to that workspace's Git tab.
  Dialogs (edit, undo, merge back, compare, run with) already work from the task, whatever workspace is active.
- New task adds the task to the active workspace; on the All workspaces board the button's tooltip says which.

## Rejected

- Swimlanes (one row of columns per workspace): with many workspaces most rows are empty, and the board scrolls both ways.
- A separate global view outside any workspace: it needs its own place in the title bar and its own key, and every card action would need a workspace switch first.
- A workspace picker in the New task dialog: the active workspace is the one the user is in; picking another is one sidebar click.

## Consequences

- `Board` queries take a `board::Scope` (one workspace, or every listed one) instead of a project id.
- While the All workspaces board is on screen, task events and usage events of every workspace redraw the window (`redraw::Shown::all_board`).
- SPEC §10 and the `[ui]` settings list say so.
