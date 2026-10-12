# 0050 — The Files tree sits on the right; search and Ignored are redrawn

- Date: 2026-10-12
- Status: accepted

## Context

The Files tree stood left of the file, beside the app sidebar: two lists side by side and the
file pushed to the far right. Its "Show ignored" / "Hide ignored" text button changed width and
read as an action rather than a state. The human asked for the tree on the right and a cleaner
search and ignored control.

## Decision

- Tree column right of the file panel; same width, keys, menus.
- The search row is one field-looking button (`⌕`, placeholder, Quick open shortcut) that opens
  Quick open, as before.
- Ignored is a fixed-label toggle chip with an on (pressed) and off look; the tooltip says what a
  click does.

## Alternatives rejected

- A setting for the tree side: a choice nobody asked for.
- An icon-only ignored toggle: no glyph says "git-ignored" without reading the tooltip.

## Consequences

Layout only; no protocol, store or key change.
