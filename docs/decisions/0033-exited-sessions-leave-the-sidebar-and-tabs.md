# 0033 — Exited sessions leave the sidebar and their tabs

- Date: 2026-10-09
- Status: accepted
- Milestone: inbox 2026-10-09

## Context

A shell or agent that exited stayed in the sidebar as `✕ exited` (not clickable, only a hover ✕ to
remove it) and its pane stayed in its tab as "Session exited". A stopped run command did the same
in the `run` tab. The row had no use, and ✕ = `session.remove` hides the session for good (ADR
0019), which also drops it from History. The daemon cannot just hide on exit for the same reason.
A daemon stop also exits every session; those panes must stay in their tabs, because the next
start resumes them from there (ADR 0017).

## Options

1. Filter exited sessions out of the sidebar rows only; the daemon removes the pane of a session
   from its tabs when it exits while the daemon runs (not while it stops).
2. Hide the session on exit (`session.remove`).
3. Make exited rows clickable (open a replay) and keep them.
4. A setting to keep or drop them.

## Decision

Option 1. The session stays in `session.list`, so History keeps its exit code, log and replay
(option 2 would delete that, option 3 keeps the clutter the user wants gone, option 4 is a knob
nobody asked for). The daemon does it in `Actor::finish`, where it knows whether it is stopping;
a UI-side prune on `session.exited` would also fire for the exits of a daemon stop and erase the
panes that must resume. A restarted run command is a new session in the old pane's place; the old
one is no longer in the sidebar.

`project.archive` kills the workspace's live sessions while the daemon runs, so the hook also
removes their panes from the archived workspace's tabs. Accepted: archiving ends those sessions
for good (they are not marked `interrupted`, so a free agent pane is never resumed from them, with
or without the pane), and "resume when added again" (`archived_workspaces_are_not_resumed_until_added_again`)
runs from the task's status, which the hook does not touch. The only loss is the pane position: a
task agent resumed after adding the folder again is a new session that opens from the sidebar, not
in the old tab slot. A flag to skip the hook for archive kills was not worth that.

## Consequences

SPEC §8, §10 and §11 change; no protocol change. After a daemon stop the exited panes of
non-resumable sessions (shells, agents without an id) stay in their tab as a placeholder until the
user closes them, as before (a startup sweep would also remove the panes of archived workspaces).
Panes that exited before this version stay too. Closing a live shell with ✕ now also removes its
sidebar row.
