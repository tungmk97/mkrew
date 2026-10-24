# 0072 — Select, copy and edit blocks in the Files Markdown preview

- Date: 2026-10-23
- Status: accepted (SPEC §12A, §12)
- Milestone: none (user request 2026-10-23)

## Context

SPEC §12A said the Files Markdown preview is read-only: editing only happens in Source. The user
wants to copy rendered text, and to fix one passage without leaving the preview.

## Decision

- Selection works on flattened pieces: the preview is turned into a list of text pieces in
  reading order, and a selection is a start and an end position in that list. This lets a drag
  cross blocks, tables and lists.
- Each piece's text layout is recorded from the paint callbacks, so hit-testing (click, drag,
  double-click word) uses the layout that was really drawn.
- The selection background is merged by hand per piece into that piece's highlights, because
  GPUI has no selection for static text.
- Copy writes the selected text as shown: blocks and lines on their own lines, table cells
  tab-separated, list markers `- ` / `N. ` / `[x] `, nothing for drawn diagrams. A drag or
  double-click adds no marker to the list item it starts in (even from its first byte); items
  after it keep their markers; Ctrl+A copies every item with its marker.
- Edit is per top-level block. A ✎ button sits inside each block at its top-right corner, on a
  rounded `surface_raised` background, shown on hover. It opens the block's Markdown in a caret
  field in the block's place (click to place the caret, drag to select, Ctrl+C/X/V; max height
  `PROMPT_FIELD_HEIGHT`; muted hint "Ctrl+Enter apply · Esc cancel"). Ctrl+Enter replaces the
  block's source lines in the editor as one undo step; Esc or a click outside cancels.
- Each edit remembers a stamp of the whole editor text (text id + version): an edit anywhere in
  the file since the field opened refuses the apply with a toast.
- Files view only. Docs keeps its block picking for comments.

## Consequences

- Docs, Ask AI and the thread panel are unchanged.
- Nested lists copy without indentation.
- No autoscroll while dragging past the edge of the preview.
- An emptied block leaves a blank line in the file.
- Ctrl+S while the field is focused saves without the pending field text (the field stays
  open). Ctrl+F switches to Source and closes the field (its text is dropped).
- A long block's field has no caret autoscroll.
- One more focus handle in the Files view, and a Lucide pencil icon.
