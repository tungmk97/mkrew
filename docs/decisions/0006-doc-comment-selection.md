# 0006 — Doc comments anchor to picked lines, threads listed in a margin

- Date: 2026-10-06
- Status: accepted
- Milestone: M6

## Context

SPEC §12 says "select text → Add comment" and shows threads "in the right margin next to the
anchor". GPUI's `StyledText` (gpui-pre 0.3.7) has no built-in text selection, and the preview
is a column of blocks with no layout API to line up margin cards with block positions.
Comments are anchored by line range + quote anyway (SPEC §5, §12), and the daemon re-anchors
on the raw file.

## Options

1. Character-level selection: build a custom text element with hit-testing and selection
   painting over every block. Pros: matches the wording. Cons: a large custom element; the
   anchor is still whole lines.
2. Pick lines: a click picks a top-level preview block or a source line, and Shift+click
   extends the pick. The quote is the selected source lines. Pros: small, and it matches
   what the anchor stores. Cons: coarser in the preview, where a whole list or table is one
   block.
3. Pixel-aligned margin: measure each block's bounds and position the thread cards next to
   them. Cons: layout feedback across frames; overlapping threads need stacking.

## Decision

Option 2 for selection. The margin lists threads in anchor order (line order); clicking a
thread's "Lines a–b" picks those lines and highlights them. The Source view gives
line-precise anchors. Pixel alignment (option 3) and character selection (option 1) go to
the backlog.

## Consequences

- SPEC §12 "Comments" bullet now reads "pick lines (click a block / source line, Shift+click
  extends)", and says threads are listed in the right margin in anchor order.
- Backlog: character-level selection, margin cards aligned with their anchors.
