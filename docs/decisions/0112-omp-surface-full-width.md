# 0112 - The omp surface's transcript spans the pane among several panes

- Date: 2026-11-25
- Status: accepted (amends ADR 0107 and SPEC section 7.2: the layout of `main`)
- Milestone: none (human request 2026-11-25)

## Context

An omp pane drew its transcript (`main`) in the Chat page's centered column, at most 760 px wide.
Next to other panes, a pane is rarely much wider than that, and the side padding plus the cap left long lines and tables wrapping early.
Tern draws the same transcript across the whole block, and the user wants the same when panes are split.
A pane that is alone in its tab keeps the centered reading column, which the user wants as a focus view.

## Decision

- `tsp::column_max(several)`: alone in the tab (or zoomed), `main` (and a `screen` surface) stays in the Chat column (`CHAT_COLUMN_WIDTH`, centered); with other panes shown beside it, it is as wide as the pane with `space::XL` at both sides (`page_column(el, None, ..)` in `shell/chat_view.rs`).
- The PTY is sized to the same width (`tsp::column_px`), so what omp wraps itself fits the column, and a split or an unsplit resizes it.
- The dock (the composer card) stays in the Chat column in both cases, like Tern's narrower composer.
- The Chat view of Claude and Codex panes keeps its 760 px column (ADR 0105).

## Consequences

- Splitting or closing a neighbour changes the column width, so omp gets a `resize` and rewraps its own rows.
- The reading width toggle still does not apply to a surface.
- `overlay` widths are unchanged.
