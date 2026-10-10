# 0036 — Sidebar filter and chips replace the Inbox card

- Date: 2026-10-10
- Status: accepted
- Milestone: none (outside the milestone loop; inbox item "Workspace sidebar redesign + cleanup")

## Context

- SPEC §10/§11 put an **Inbox card** at the bottom of the sidebar: every needs_input or
  freshly-done session, newest first, with its last screen line (§20A.4) and turn summary
  (§20A.3), `Place::Inbox` in the peek.
- The redesign adds a status bar with an Inbox state (`Inbox clear` / `N need you`, click = next),
  and a filter box with chips `All` / `Working` / `Needs you` at the top of the sidebar. The
  card now shows the same sessions a third time and takes up to half of the sidebar's height.
- The user asked to remove the card (inbox line 61): "the chips + status bar replace it; peek on
  hover stays".

## Options

1. Keep the card next to the chips. Triple display, less room for workspaces.
2. Remove the card; keep the Inbox *logic* (`inbox::entries`, `needs_you`, `next`) for the chip,
   the status bar, Ctrl+Shift+I and the keyboard peek. (Chosen.)
3. Remove the Inbox concept completely. Loses "jump to the next session that needs me".

## Decision

- The Inbox card, its rows, the last-line read per entry (`session.read lines=1`) and the
  `Place::Inbox` peek variant are deleted. `inbox::entries` / `needs_you` / `next` stay: the
  `Needs you` chip, the status bar and `NextInbox` (Ctrl+Shift+I) use them, so "needs you" has
  one definition.
- The peek popover stays on sidebar session rows (hover) and on `Ctrl+Shift+Space`, which peeks
  the first needs-you session and reveals its row (opens the workspace, clears filter and chip).
  ↑ / ↓ still cycle the needs-you sessions.
- The workspace row shows a summary (`1 working`, `2 idle`) instead of the token total; usage
  lives in the status bar for the focused agent. Session rows show only the time on the right.
- The filter and the chip are client-side and in memory only; nothing new on the wire.

## Consequences

- SPEC §10, §11, §13, §20A.1, §20A.3, §20A.4 and §20E.3 and three acceptance lines are updated
  in the same change (the last-line feature is gone; the pane header keeps the turn summary).
- `Usages::workspace_total` is deleted (task cards keep `task_total`).
- Risk: a `done` session the user did not look at is now visible only as a chip count and in the
  status bar until they open it; the sidebar row still shows its ✓.
