# 0082 - Daemon scrollback raised to 1,000 lines

- Date: 2026-10-27
- Status: accepted
- Milestone: inbox 2026-10-27

## Context

ADR 0030 sends the daemon's scrollback in attach snapshots, so a re-attached pane scrolls back as far as the daemon kept.
The daemon kept 200 lines.
A pane is re-attached after every tab or workspace switch, the Board, minimize, and every output-queue overflow, so its history often fell from the UI's 10,000 lines to 200.
One agent answer or diff is often longer than 200 lines, and the user could not scroll back to read it.

## Options

1. Raise the daemon scrollback to 1,000 lines.
2. Keep hidden panes' UI emulators (rejected in ADR 0030).
3. Replay the tail of the session log file on attach (rejected in ADR 0030).

## Decision

Option 1, as the user asked.
It is a one-constant change and keeps the ADR 0030 path.

## Consequences

- A full history costs about 1,000 rows x cols x 24 bytes (alacritty `Cell`): about 2.9 MB at 120 columns, against 0.6 MB before.
  Six sessions with full histories use about 17 MB (about 29 MB at 200 columns), inside the §15 daemon budget (< 50 MB).
  Alacritty allocates history rows only as output scrolls, so short sessions cost what they used.
- An attach snapshot carries up to 1,000 rows of JSON (at most a few MB for colorful output), far below the 16 MiB frame limit (§7).
- SPEC §7, §8 and §10 are updated.
  Older output is still in the History dialog (session log).
- Amended 2026-10-28 (ADR 0088): the UI scrollback of an attached pane is 1,000 lines too, so a re-attach no longer loses history.
