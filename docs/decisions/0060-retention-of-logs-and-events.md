# 0060 — Session logs and noisy events are deleted 7 days after a session ended

- Date: 2026-10-14
- Status: accepted (amends ADR 0019 "history stays"; SPEC §5, §7, §8)
- Milestone: none (bug fix, user report 2026-10-14)

## Context

In 7 days the data dir grew to 570 MB of session logs (255 files) and a 23 MB database, 14 MB
of it `files_changed` events: each batch was stored once per live session, and the watcher
reported nested-gitignored files, `.git/info/exclude` matches and `.worktrees/`. Nothing deleted
old logs; ADR 0019 kept hidden sessions' logs and events forever.

## Decision

- Retention window: 7 days after `ended_at`, the attachments' window (`attachment::KEEP_MS`).
- At daemon startup (before the IPC listener) and in the hourly backup tick: delete `<id>.log`
  and `<id>.log.1` of every session that ended before the window and that no tab shows; delete
  `files_changed`, `hook` and `edited` events older than the window.
- Kept: session rows (resume, `get_session`), `native_session_id`, turns, `usage` events (usage
  totals), comments, tabs. `session.history` of a purged session answers `files: []`.
- `files_changed` is stored once per batch, on the project's newest live session.
- The watcher also honours nested `.gitignore` files and `.git/info/exclude`, and skips
  `.worktrees/`. Precedence is git's: `.gitignore` files apply top-down and the deepest level
  with a match decides each path prefix; nothing inside an ignored folder is reported or read.
  Markdown stays reported (doc review).
- Startup runs `VACUUM` when free pages exceed a quarter of the database.

## Alternatives

- `auto_vacuum = INCREMENTAL`: needs a full `VACUUM` on existing databases first and periodic
  `incremental_vacuum`; freed pages are reused anyway. Rejected.
- A `[storage]` setting or a total-size cap: not needed yet (`ponytail:` in `retention.rs`).
- A project-level `files_changed` table: a migration for the same `doc.list` result. Rejected.

## Consequences

- History of a session older than 7 days is gone unless its pane is still in a tab.
- Logs are deleted 7 days after their session ended, so the data dir settles at about 7 days of logs (60–140 MB/day observed). The first startup frees only what is already older than that: on a copy of real data (274 files, 617 MB) it deleted about 8 MB of logs and pruned 2,898 of 15,699 events; the database went from 23.2 to 22.5 MB (the VACUUM threshold was not reached).
