# 0030 — Attach snapshots carry the daemon's scrollback

- Date: 2026-10-09
- Status: accepted
- Milestone: inbox 2026-10-09

## Context

Sometimes a terminal pane could not scroll back more than one page. Panes that are not visible
(other tab or workspace, Board, minimized, zoomed away) are detached and dropped (SPEC §10). A
re-attach rebuilds the pane from the daemon's snapshot, and that snapshot was the visible screen
only (SPEC §7). An output-queue overflow resnapshot did the same. So after any tab switch the
pane's history started empty again. The daemon emulator keeps 200 lines of history, but it never
sent them.

## Options

1. Send the daemon's history (≤ 200 lines) in the snapshot; `paint` scrolls it into the
   receiving terminal's scrollback.
2. Keep hidden panes' UI emulators (10,000 lines each) instead of dropping them.
3. Replay the tail of the session log file on attach.

## Decision

Option 1. It fixes both triggers (re-attach and overflow) in one place, it works for `mk attach`
too, and it stays inside the §8 memory budget because the daemon already holds those 200 lines.
Option 2 keeps 10k-line grids for every hidden pane (against §10's "not visible = no cost") and
still leaves a gap for output that scrolled while the pane was hidden. Option 3 races the buffered
log writer and adds file I/O to every attach.

## Consequences

`ScreenSnapshot` gets `history` (oldest first, main screen only), and the protocol version goes up
by one. SPEC §7 (snapshot definition), §8 and §10 are updated. After a re-attach a pane scrolls back
at most 200 lines (the daemon scrollback); older output is in the History dialog (session log).
Wrapped history lines come back as separate lines. Raising the daemon scrollback is a separate
memory-budget decision (§8, §15).
