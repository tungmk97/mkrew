# 0020 — Workspace colors

- Date: 2026-10-07
- Status: accepted
- Milestone: none (outside the milestone loop)

## Context

- The user wants to tell workspaces apart at a glance.
- SPEC §14 allowed no saturated colors besides the accent and the status colors.

## Options

1. 8 named colors, stored as names, mapped to per-appearance theme tokens. (Chosen.)
2. Free hex values. Unreadable in one of the two themes, and they hard-code colors.
3. Coloring the whole sidebar row. It fights the selection fill.

## Decision

- `project.color` (migration `0013_project_color.sql`) holds one of `WORKSPACE_COLORS`: red,
  orange, yellow, green, teal, blue, purple, pink, or `NULL` for none. `project.update` sets it
  (`null` clears it); the daemon refuses any other name.
- The UI maps the name to a per-appearance token (`Theme.workspace_colors`), each ≥ 3:1 on
  `bg`, `surface` and `surface-raised` (WCAG 1.4.11).
- Shown as a small dot before the workspace name in the sidebar, and as swatches in the
  workspace menu.
- Never used for state: status keeps its own colors.

## Consequences

- §14 principle 2 gets one exception: a small dot per workspace.
- `Project.color` is optional on the wire; protocol stays v26.
- No CLI flag and no coloring of tabs or inbox rows yet.

## Amendment 2026-10-08 (ADR 0027)

The dot became the workspace's first letter on a tint of its color (the redesign's avatar).

## Amendment 2026-10-10 (inbox redesign, item 4)

The palette was too close in places (dark blue/purple, light orange/yellow) and had no lime. It
is now nine names: red, orange, yellow, **lime**, green, teal, blue, purple, pink. Every old name
stays valid (stored values and the daemon check are untouched; `lime` is the only new name); the
protocol stays as it is (`color` was already an optional string). The per-appearance tokens were
re-picked: neighbouring hues differ by at least 12° on the color wheel, and each color is ≥ 3:1
on `bg`, `surface`, `surface-raised`, `sidebar`, `card`, `hover` and `selected` in both themes
(tests in `theme.rs`). Values are in SPEC §14.
