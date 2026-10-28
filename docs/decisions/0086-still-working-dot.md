# 0086 - A working agent shows one still colored dot

- Date: 2026-10-28
- Status: accepted (the user ticked `Làm` for perf A1 on the 2026-10-14 plan tick, with the note below)
- Milestone: perf review (A1, N1)

## Context

ADR 0027 replaced the pulsing dot with three dots stepping on the 500 ms status tick, and ADR 0054 added a 12 fps pulse to the tab dot and the sidebar workspace pill.
Every animation frame renders the whole window (`theme.rs` said so itself).
While any agent works, the UI rendered 2 times a second for the dots and 12 times a second for the pulse, all day, against SPEC §15's idle CPU budget.
The Run pill already has a still dot for this reason (ADR 0018, ADR 0054).
The user asked that each status have its own distinct dot color instead of motion, as long as it costs nothing.

## Options

1. Keep both (before). Most lively; 14 renders per second while an agent works.
2. Drop the pulse, keep the stepping dots. 2 renders per second.
3. One still dot in the state's color; only the elapsed text moves, once per second for the first minute, then once per minute (as for idle time). Nothing animates.

## Decision

Option 3.
`widgets::status_dot` has no pulse; the `working` mark, the sidebar slot, the tab dot, the workspace pill and the voice recording dots are still dots.
Each state has its own token, always beside an icon or text (SPEC §11, §14): working `status_working` (filled dot), needs input `status_needs_input` (`?`), done `status_done` (`✓`), exited `status_exited` (`✕`), idle `status_idle` (`○`, a hollow ring on the tab).
No new token was needed.
`status::redraw_due` redraws when a working or idle session's time text changes.
The loading marks (a pane being started, a voice transcription, a Jira fetch) keep three stepping dots, renamed `loading_dots`: they last seconds and the status loop already redraws for them (`starting_pane`, `transcribing`, `jira_busy`).
The voice ring keeps `voice::pulse` (it redraws on the voice meter tick anyway).

## Consequences

- SPEC §10, §11, §14 and §20G say so; ADR 0054 and ADR 0027 point here.
- A still dot says "working" less loudly than motion; its color and the `working · 12s` text next to it are the live signal (never color alone, §11).
- gpui's missing reduce-motion query no longer matters for these marks.
- Reverting is local: `status_dot`, `status_mark`, `redraw_due`.
