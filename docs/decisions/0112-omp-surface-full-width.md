# 0112 - The omp surface's transcript is as wide as the pane

- Date: 2026-11-25
- Status: accepted (amends ADR 0107 and SPEC section 7.2: the layout of `main`)
- Milestone: none (human request 2026-11-25)

## Context

An omp pane drew its transcript (`main`) in the Chat page's centered column, at most 760 px wide.
On a wide pane most of the space stayed empty, and long lines and tables wrapped early.
Tern draws the same transcript across the whole block, and the user wants the same.

## Decision

- `main` (and a `screen` surface) is as wide as the pane, with `space::XL` at both sides (`page_column(el, None, ..)` in `shell/chat_view.rs`).
- The PTY is sized to that width (`tsp::column_px`), so what omp wraps itself fits the wider column.
- The dock (the composer card) stays in the Chat column (`CHAT_COLUMN_WIDTH`), like Tern's narrower composer.
- The Chat view of Claude and Codex panes keeps its 760 px column (ADR 0105).

## Consequences

- On a very wide pane, lines get long; the reading width toggle still does not apply to a surface.
- `overlay` widths are unchanged.
