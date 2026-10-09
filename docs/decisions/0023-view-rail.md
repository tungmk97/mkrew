# 0023 — View rail on the right

- Date: 2026-10-08
- Status: superseded by 0027
- Milestone: inbox 2026-10-08

## Context

The user wants more height for the main area (inbox 2026-10-08). The view switcher was a bar
across the top of every view (about 37 px), holding the five views, `+ New task` and the Run
button. The Terminals view adds its own tab bar below it.

## Options

- A: a rail right of the main area — views stacked vertically; costs width, not height.
- B: the views in the left sidebar — keeps one column, but mixes per-workspace views with the
  workspace list.
- C: a compact strip inside the Terminals tab bar — no extra column, but the other views have no
  tab bar to host it.

## Decision

A. About 37 px of height is regained in every view; shortcuts (Ctrl+Shift+1..5) and the palette
are unchanged, and the rail tooltips show the keys.

## Consequences

- SPEC §10: diagram, a "View rail" bullet, Run placement, New task placement.
- Run is full-size in the Terminals tab bar (before History / + Agent) and compact (state icon +
  ▾, full label in the tooltip, ↻ ■ in the menu) at the rail foot in the other views.
- `+ New task` on a non-empty board moves to the Todo column header.
- The rail costs about 76 px of width (about 9 terminal columns at 13 px).
