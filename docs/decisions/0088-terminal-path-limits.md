# 0088 - Terminal path limits: screen scan rate, UI scrollback, history replay

- Date: 2026-10-28
- Status: accepted
- Milestone: perf 2026-10-28 (plan docs/superpowers/plans/2026-10-28-perf-redraw-terminal.md)

## Context

The daemon hashes every cell of a session's screen after each output batch (up to 60 a second per session, background ones too).
When the hash changed it built a `String` for every row up to the cursor and ran the state patterns, although the state machine reads only the last `PATTERN_TAIL_LINES` = 4 rows (ADR 0015 added the fingerprint, ADR 0003 the state source).
It did so also while the machine ignored the screen: for 10 s after every hook event of `hook+screen` profiles, and for `hook` profiles always.
The UI scrollback and the History replay costs are decided in the sections appended by their own tasks.

## Decision: screen scan

- The state machine gets only the last 4 non-blank rows up to the cursor (`Emulator::tail_to_cursor`).
  A run session still builds every row for its port scan (SPEC §10).
- A session's screen is scanned at most every 250 ms (`scan::ScanRate`).
  Output inside the gap is scanned by a trailing scan when the gap ends, so a prompt that is the last thing printed is never missed.
  A session with a queued `session.send`, and a run session (its port line can scroll off within one gap), scan every batch.
- When the machine ignores the screen (`StateMachine::screen_active` is false) and the session is not a run session, the scan does no fingerprint and builds no rows; the machine still sees that output happened.
  The first scan after such a stretch only sets the fingerprint baseline (the machine still reads the tail without counting it as activity, so a prompt printed first after a hook window is classified), so a `done` session does not turn `working` because its screen moved while nobody looked.
- The fingerprint (ADR 0015) is compared between two scans, not two batches.
  A change that is undone inside one gap is not seen.

## Consequences

- `working` and `needs_input` can show up to 250 ms late; `idle` (3 s quiet) can be up to 250 ms late too (the tick runs the owed scan first).
- During a hook window, invisible output (cursor, title) now moves `last_output`, which the fingerprint gate used to filter: a TUI that keeps blinking through the end of the window goes `idle` up to `idle_after_ms` after the window ends, not after its last visible change.
- SPEC §6 and §8 say so.
