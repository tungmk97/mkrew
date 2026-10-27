# 0083 - Files preview: Ctrl+Shift+V toggle, 960 px page, expandable diagrams

- Date: 2026-10-27
- Status: accepted
- Milestone: inbox 2026-10-27

## Context

The user asked for three changes to the Files view's Markdown preview.
Ctrl+Shift+V should switch between Preview and Source, as in VS Code.
The page should be wider, because 720 px left tables and diagrams cramped.
A Mermaid diagram should have an expand button to see it large.

Ctrl+Shift+V is already the terminal's Paste (SPEC §13).
The keymap refused any key bound twice, wherever the two commands act.

## Options

1. Bind Ctrl+Shift+V in the Editor context only, and let the keymap accept one key in the Terminal and Editor contexts.
2. Pick another key (the Docs view uses Ctrl+Shift+E).

For the diagram: a dialog that shows the diagram large (a), or zoom and pan inside the page (b).

## Decision

Option 1: the user asked for that key, and no terminal has focus in the Editor view.
`Command::shares_context` says that a Terminal command and an Editor command never act together.
The duplicate checks (keymap.toml parse, Settings capture) skip such pairs; every other pair stays unique.
The page max width is 960 px, the agent pane's reading width.
Diagrams: option (a), a modal dialog of 90% of the window.
It shows the diagram as wide as fits, at most 2x its natural size, because gpui rasterizes SVG at 2x.
The card's "Expand" dispatches a gpui action with the diagram, so Docs and Files share it.

## Consequences

- A user can bind another command to a key that a command of the other context already uses (for example `ctrl-shift-c` to an Editor command).
- The button is at the card's bottom-right, because the Files ✎ is at a block's top-right.
- SPEC §12A, §13 and §14 are updated.
