# 0012 — Peek shows plain screen lines

- Date: 2026-10-07
- Status: accepted
- Milestone: M9

## Context

SPEC §20A.1 asked the peek popover to show the last 8 screen lines "monospace, colors kept",
but the same section defines `session.read` (the only allowed data source, never attaching) as
returning `lines[]` of plain text. Colors would need styled spans in the protocol.

## Options

1. Add styled spans (fg/bg/bold runs) to `session.read` — a protocol change, emulator-side
   span extraction and a UI renderer, for a glance-sized popover.
2. Show the plain lines in the terminal font.

## Decision

Option 2. The popover is for reading a question and answering it; the text carries that. Lines
use the terminal font and the normal text color, clipped with an ellipsis.

## Consequences

SPEC §20A.1 updated ("colors kept" dropped). Colored peek lines can come later as an additive
`session.read` field. Implemented in `crates/mkrew-ui/src/peek.rs` + `Shell::render_peek` (T9.9).
