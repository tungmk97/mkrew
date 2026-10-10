# 0037 — Pane colors (session.set_color)

- Date: 2026-10-10
- Status: accepted
- Milestone: inbox 2026-10-10

## Context

The redesign adds a right-click menu on panes with "Rename" and "Change color", the color from
the same palette as workspaces (ADR 0020). ADR 0020 kept workspace colors to workspaces
("no coloring of tabs or inbox rows yet") and SPEC §14 principle 2 allows no saturated color
besides the accent, the status colors and the workspace avatar. A pane is a session; its name
already lives in `session.title` and changes with `session.rename` (ADR 0031).

## Options

1. A `color` column on `session`, set with a new `session.set_color` request; shown as a tinted
   agent chip and a thin line under the pane header. A live session is edited through its task
   (like a rename), so the task's next save keeps it.
2. Store the color in the tab layout JSON: a second place that nothing outside the tab sees, lost
   when a pane moves to another tab or is resumed.
3. Add a color field to `session.rename`: its `null` already means "clear the title", so two
   optional fields would conflate two verbs.
4. Color the whole header or the pane border. The header fill fights the status pill; the border
   already carries state (needs input, focus).

## Decision

Option 1. `session.color` (migration `0015_session_color.sql`) holds one of
`WORKSPACE_COLORS` or `NULL`; the daemon refuses any other name and `null` clears it. Reply =
the Session, also `session.updated`. A live session goes through `Command::SetColor`, an exited
one through the store; both share one helper with `session.rename`. A resume copies the color.
Run sessions may be colored (only their title is fixed). The UI tints the agent chip in the
pane header with the color and draws a 2 px line under the header; the state-driven pane border,
the status pill and the sidebar rows are unchanged. The colors keep ADR 0020's ≥ 3:1 contrast on
`bg`.

## Consequences

Protocol: `Session.color` (optional on the wire), request `session.set_color`, protocol version
`34`. SPEC: §5 (column), §7 (request and `session.updated` text), §10 (color bullet), §11
(pane menu), §14 principle 2 (a second place where workspace colors show). ADR 0020's "never
used for state" still holds: a pane color is decoration chosen by the user. No CLI flag
(`mk session color` only if someone asks).
