# 0040 — Compact sidebar rail

- Date: 2026-10-10
- Status: accepted
- Milestone: none (outside the milestone loop; user request "compact mode for the workspace sidebar")

## Context

- SPEC §10 has two sidebar states: full width (280 px) and hidden (Toggle sidebar, Ctrl+Shift+B).
  Hidden loses every status at a glance; full width takes room from the terminals.
- The user asked for a third, narrow mode that still shows each row's main information, with the
  full title on hover, as an added feature with its own shortcut and icon (not replacing hide).
- §20A.1 opens the peek after hovering a session row 300 ms. On a narrow rail a hover tooltip
  with the full title is what the user asked for, and the two would fight for the same hover.

## Options

1. Replace hidden with compact. Rejected by the user ("instead of change total, add 1 more").
2. A draggable sidebar width. More state and a resize handle; no designed narrow layout.
3. A separate compact mode, toggled by its own key and status-bar icon. (Chosen.)

## Decision

- `Toggle compact sidebar` (Ctrl+Alt+B, a `⇤` / `⇥` status-bar button, palette "Global: Toggle
  compact sidebar") switches full width ↔ a rail of `COMPACT_SIDEBAR_WIDTH` (56 px at the
  default font size, scaled with `[ui] font_size`). On a hidden sidebar it shows the rail.
  Toggle sidebar keeps hiding and shows again at the last width. In memory only.
- Rail rows: a workspace is its avatar plus `?` (needs input) or its working count; a session is
  its status mark plus agent chip. Clicks, right-click menus and expand/collapse are the full
  sidebar's. Each row's tooltip has the full text: workspace name / summary / folder; session
  `agent · title` / status in words.
- The rail draws no filter box or chips: entering it clears them, so no row stays hidden unseen.
- In the rail, hovering a session row shows the tooltip, not the peek. The keyboard peek
  (Ctrl+Shift+Space) still opens next to the row.

## Consequences

- SPEC §10, §13 and §20A.1 are updated in the same change.
- Pin marks, times, the run `▶` and the hover `✕` / `⋯` actions are not on the rail. The
  right-click menus still reach Pin and the workspace actions; removing a session from the
  sidebar needs the full sidebar's hover `✕`.
- Ctrl+Alt+B is AltGr+B on Windows, so layouts that type a character with AltGr+B lose it
  (as with Ctrl+Alt+digit, ADR 0028); rebind `toggle_compact_sidebar` in `keymap.toml`.
- Without workspaces the rail shows only `+` (with its tooltip and shortcut); the full sidebar
  keeps the empty-state sentence.
