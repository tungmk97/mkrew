# 0019 — Archive workspaces, hide sessions

- Date: 2026-10-07
- Status: accepted; amended by ADR 0060 (logs and events expire)
- Milestone: none (outside the milestone loop)

## Context

- The user could not remove a workspace or an agent row.
- SPEC §7 named `project.archive`, but it was never built. Nothing wrote `project.archived = 1`.
- A hard delete fails on foreign keys: `PRAGMA foreign_keys` is on, and `task`, `session`,
  `terminal_tab`, `doc_comment`, `turn` and `session_event` reference `project` or `session`
  with no `ON DELETE`. It would also lose the history that ADR 0004 keeps.

## Options

1. Archive and hide flags: `project.archived` (exists) and a new `session.hidden` column. Small,
   keeps all history, reversible for workspaces. (Chosen.)
2. Cascade delete through a table-rebuild migration. Loses logs, turns and comments, and needs a
   big migration.
3. Hide in the UI only. The rows come back on every restart and on other clients.

## Decision

`project.archive` stops the project's live sessions (run commands too: they are shell sessions
of the project), sets `archived = 1` and announces `project.updated`. `project.add` of an
archived folder un-archives the same project (same id, settings, tasks and history).
`session.remove` sets `session.hidden = 1` (migration `0012_session_hidden.sql`) and asks a live
session to stop; `Store::list_sessions` leaves hidden sessions out.

## Consequences

- Archived projects stay in `project.list`; clients filter them out (the sidebar, palette, inbox
  and run autostart already do). Startup resume skips their tasks and panes without changing
  them, so adding the folder again resumes the tasks on the next daemon start.
- Hidden sessions stay in the DB and on disk (`session.history`, `get_session`, turns, comments
  still work). The existing log and attachment cleanup still applies.
- Hidden sessions disappear from every `list_sessions` caller, not only `session.list`: e.g. the
  `session.kill` "known session" check, `session.wait`, `session.attach`, ports, resume, fan-out,
  run commands and worktree checks. So `session.kill`/`wait`/`attach` on a removed id answer
  `not_found`. A removed session must not come back anywhere. Removal hides the row first and
  then asks a live session to stop gracefully (`Kill { force: false }`, up to the kill grace).
  During that window the still-running session is not seen by the `list_sessions` busy checks,
  so a quick re-run of the same task or run command could start a duplicate. Accepted: the window
  is short and needs a deliberate re-run. A removed live run session is no longer seen by
  `project.run_stop`, but `session.remove` already asked it to stop.
  Store `delete_task` uses its own query and is unaffected.
- Protocol v26. There is no CLI command yet.
