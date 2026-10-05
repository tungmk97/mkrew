# 0004 — Task CRUD details: default agent, deletion and its event

- Date: 2026-10-05
- Status: accepted
- Milestone: M4

## Context

T4.1 implements `task.list/create/update/move/delete`. SPEC §5 has `task.agent_profile TEXT NOT
NULL`, but M4 wants "the task's agent, else the workspace default" at run time. SPEC §7 lists
only `task.updated`, so clients cannot learn about a deleted task, and `session.task_id`
references `task(id)` (foreign keys are on), so deleting a task with sessions fails in SQLite.

## Options

1. Resolve the default agent at create time — later changes to the workspace default would not
   apply to existing Todo cards. / Store `''` for "default" — no migration, run time fallback.
2. Delete: cascade-delete sessions (loses history) / refuse always (old exited sessions would
   make tasks undeletable) / refuse only while a session is live, detach exited ones.
3. Deletion signal: none (clients re-list) / a `task.deleted` event.

## Decision

`agent_profile = ''` in SQLite means "project default", exposed as `Option<String>` (`None`) in
the protocol; a named profile must be an agent profile. `task.delete` returns `invalid_state`
while any session of the task has not exited; otherwise its sessions get `task_id = NULL` (the
history stays as free terminals) and the task row is deleted in one transaction. A new
`task.deleted {task_id, project_id}` event tells boards to drop the card.
`PROTOCOL_VERSION` goes to 4.

## Consequences

SPEC §7 request and event tables updated. T4.3 resolves `None` to `default_agent` when running.
