# 0110 - Tern look for the Files and Git panel, and a pane drag handle

- Date: 2026-11-24
- Status: accepted (amends the Git tab layout of SPEC section 12B, the Files tree header of section 12A, the right panel tab row of section 10 and the pane drag strip of ADR 0106 and SPEC section 14)
- Milestone: none (human request 2026-11-24, design approved in chat with a mockup; Tern 0.9 running on the same machine as the reference)

## Context

The user found the right panel's Files and Git tabs crowded next to Tern.
Two segmented controls sat on top of each other (`Files | Session | Git`, then `Working | Vs. base | History`), the branch card touched the tab row, every element had a frame (card, fields, a dashed "Nothing staged" box, the commit box), and group headers were upper case.
Tern's panel uses a row of icon tabs, a flat filter field, sentence-case group titles with a muted count, dense rows without frames and the branch at the bottom.
The user also asked for Tern's pane drag handle: a small bar at the top of the pane while the pointer is over it, and a six-dot grip on the bar that drags the pane.
Only the look changes; no feature, data or behavior changes.

## Decision

- **Panel tab row.**
  Icon tabs (`files`, `bot`, `git-branch`) on the left of the 44 px row, 28 px high, the shown one with the `selected` fill; the title and shortcut are in the tooltip; the Git count chip sits in the Git tab.
  The row has no fill of its own, only the divider under it.
- **Files.**
  The "Search files…" entry is 30 px with the faint `divider` border; "Ignored" is an eye toggle (`eye` accent while ignored files show, `eye-off` muted otherwise).
  Tree rows are 24 px (`TREE_ROW_HEIGHT`).
- **Git.**
  Working / Vs. base / History are underlined text tabs (`underline_tab`, shared with the repository tabs) on a 36 px row with a divider.
  `Viewed N/M` moves onto the filter row; the filter field gets a `list-filter` mark and the faint border.
  Group headers are sentence case (META, medium, `text_muted`) with the count in `text_subtle`.
  The empty Staged hint is a muted line under the header's title, without the dashed box.
  File rows are 28 px; the change letter is colored without the tinted fill.
  The branch, its upstream, Fetch and Push move from a card at the top to a footer under the commit box with a divider above; a Fetch or Push error shows above that line.
  The commit box and its field use the faint `divider` border.
- **Pane drag handle.**
  The pane frame is a hover group; while the pointer is over a pane of a split tab, a 32 × 4 muted bar shows at the centre of its top edge.
  Over the 40 × 20 handle it becomes a `surface_raised` chip with the `grip-horizontal` icon; dragging the handle moves the pane as before (ADR 0108).
  Only the handle has a hitbox, so the rest of the pane's top edge goes to the terminal again.
- New Lucide icons: `files`, `eye-off`, `list-filter`, `grip-horizontal`.

## Alternatives

- **Move the panel to the left like Tern.**
  The left side is the session sidebar; the user asked for a look change only.
  Rejected.
- **Keep the full-width top strip as the drag area and only redraw the grip.**
  The strip blocked mouse selection on the terminal's first row across the whole width.
  The centred handle is what Tern does.
  Rejected.

## Consequences

- The Git tab shows more files in the same height (rows 34 to 28 px, no card on top).
- The panel's tab titles are only in tooltips; the shortcuts (`Ctrl+Shift+3`, `Ctrl+Shift+4`) and the palette still name them.
- Panes are dragged only from the centred handle, not from any point of the top edge.
