# 0068 — One highlight in the sidebar

- Date: 2026-10-20
- Status: accepted (extends ADR 0062; SPEC §10)
- Milestone: none (user request 2026-10-20)

## Context

The shown workspace's header row had the `selected` fill, and its selected session row right
under it had the same fill plus the `selected_ring`. Two stacked blocks of one color, one
ringed and one not, said the same thing twice and looked broken.

## Decision

- The shown workspace's header gets the `selected` fill only while none of its listed rows is
  the focused session: collapsed, filtered out, or no session focused (e.g. its Files view).
- Same rule in the compact rail.
- No new tokens; the color stripe and card border are unchanged.

## Consequences

- Exactly one highlighted row marks where you are.
- Known gap: a focused terminal inside a folded `TERMINAL` group still counts as listed, so
  neither the row nor the header is filled; fold-aware if that turns out to matter.
