# 0003 — Persist the session state's source and change time

- Date: 2026-10-05
- Status: accepted
- Milestone: M3

## Context

SPEC §11 wants the status hover to show where the state came from (hook / screen / daemon)
and when it last changed; T3.2 adds both to `Session` and `session.state`. The UI loads
sessions with `session.list` (from SQLite) and then follows `session.state` events, so the
values must survive in the `session` row, not only in the daemon's memory. SPEC §5 had no
columns for them and reserved `0003` for the M6 doc-review migration.

## Options

1. Keep them in daemon memory only — `session.list` after a UI restart would lose them.
2. Derive them from the latest `state_changed` row in `session_event` — needs an extra event
   per state change (screen mode flips often) and a per-session subquery on every list.
3. Two columns on `session` in a new migration.

## Decision

Option 3: `migrations/0003_session_state.sql` adds `state_source` (`hook|screen|daemon`,
default `daemon`) and `state_changed_at` (unix ms, backfilled from `ended_at`/`started_at`),
plus an index on `session_event(session_id, id)` for the `hook` events stored from now on. The
later migrations in SPEC §5 shift by one (doc review = `0004`, files/git = `0005`).
`PROTOCOL_VERSION` goes to 2 because `Session` and `session.state` gain required fields.

## Consequences

- Restarting the daemon marks live sessions `exited` with source `daemon`.
- An old UI talking to the new daemon gets `version_mismatch` instead of failing to decode.
