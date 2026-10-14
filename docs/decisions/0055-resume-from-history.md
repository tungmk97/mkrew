# 0055 — Resume a finished agent session on request

- Date: 2026-10-14
- Status: accepted (extends ADR 0017, which resumes only at daemon startup)
- Milestone: none (outside the milestone loop; user request: History is view-only, resuming is
  what makes it useful)

## Context

Resume exists only at daemon startup (SPEC §8, ADR 0017): stopped tasks and agent panes still in a
tab. A session that exits while the daemon runs leaves its tab (ADR 0033), and the History dialog
only replays its log. To continue that conversation the user had to type `claude --resume <id>` in
a shell pane, which also loses the agent's status, turns and hooks.

## Options

1. A "Resume" in History that spawns the profile's `resume` command in a shell: no daemon change,
   but the pane is a shell, not an agent session.
2. A `session.resume` request that reuses the startup resume path (`Daemon::resume`): the new
   session is a real agent session with the old one's title, color, pin, plan, task and worktree.
   (Chosen.)

## Decision

- `session.resume { session_id, tab_id? }` → `SessionSpawnResult`; `PROTOCOL_VERSION` 41 → 42.
- Refused (`invalid_state`) while the session is live, for a shell, without a `native_session_id`
  or a profile `resume` line, in an archived workspace, and while a live session has the same
  `native_session_id` or the same task, so one conversation never runs twice.
- Placement: the old pane's place if a tab still shows it, else `tab_id` when that tab is empty;
  otherwise the client splits the focused pane, as for `session.spawn`.
- UI: History gets a primary **Resume** button (Enter), disabled with the reason as its tooltip for
  a shell or a session without an agent id. On success the dialog closes, the Terminals view shows
  the new pane and it gets the keyboard. A refusal is an error row in the dialog.
- CLI: `mk session resume <id>` prints the new session id.

## Consequences

- The old session stays in History; each resume adds a new session row.
- The log of the old session is not carried over; the agent shows its own conversation.
