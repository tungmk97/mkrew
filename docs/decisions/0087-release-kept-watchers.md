# 0087 - Release kept file watchers, 5 s batch cap while an agent works

- Date: 2026-10-28
- Status: accepted
- Milestone: none (perf review, B5)

## Context

`doc.*`, `fs.*` and `git.*` requests keep the project's recursive file watcher running until the daemon stops (SPEC §7).
A project looked at once keeps its watcher for the daemon's lifetime, and every batch of changes makes each UI client run `git status`.
An agent that writes files all the time cut a batch every 2 s.
Nothing tells the daemon that a view closed, and `ProjectRef` cannot name a task worktree.
Files and Git are tabs of the right panel (ADR 0081, 0084), so a view closes by hiding the panel, changing its tab, leaving the Diff view or switching workspace.

## Options

1. Keep watchers until the daemon stops (today) - pros: nothing to build / cons: idle handles and `git status` runs for projects nobody looks at.
2. Daemon-side timeout after the last request - pros: no protocol change / cons: a view that stays open and idle loses its refresh; needs a heartbeat.
3. New request `watch.release` sent by the UI when it stops showing what needed the watcher - pros: one small request, the UI already knows its workspace, panel tab and worktree target / cons: protocol bump, one more thing to keep in step with the views.

## Decision

Option 3: `watch.release { project_id, worktree_task_id? }` (protocol 49).
The daemon clears the `kept` flag of the project watcher and stops it unless a session of the project is live (its exit then drops it as before); a worktree watcher stops at once.
The UI keeps the shown workspace's project watcher (the title bar summary follows `git.changed`) and a worktree watcher only while its Files or Git view is on screen.
There is no reference count: the app has one UI window.
While an agent session of the project is `working`, a batch may wait up to 5 s after its first change instead of 2 s (300 ms of quiet still ends it).
Shell and run sessions do not count.
`git.changed` was already sent only for a batch with a reported file or a git-state change; that does not change.

## Consequences

- SPEC §7: new `watch.release` row; `doc.*` rows say "until released or the daemon stops"; `git.changed` debounce text; §12B refresh line. Protocol 48 to 49.
- A watcher the UI never asked about through its shown view is not released: the Quick open `fs.find` on a worktree, the fan-out columns' `git.status`, the worktree dialog's `git.status` / `git.log`.
  They stay until the worktree is removed or the daemon stops, as before.
- Watchers kept by an earlier UI run of the same daemon are not released either; the new UI only releases what it used itself.
- A second UI window that still shows a released workspace would lose `git.changed` for it until its next request re-keeps it; revisit with a reference count if multiple windows arrive.
- An old UI against the new daemon gets `version_mismatch` and asks to restart the daemon (SPEC §7).
