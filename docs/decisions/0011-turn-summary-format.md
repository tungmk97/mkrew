# 0011 — Turn summary format

- Date: 2026-10-07
- Status: accepted
- Milestone: M9

## Context

SPEC §20A.3 showed the turn summary as `✓ done · 3 files · 4 min`. Every other duration in the
UI (status text, hover, history) uses `status::format_duration`: `12s`, `4m`, `1h 5m`.

## Options

1. `4 min` as written — needs a second duration format (`12 s`? `1 h 5 min`?) used only here.
2. Reuse `format_duration` — `done · 3 files · 4m`, consistent with the status next to it.

## Decision

Option 2. The summary sits right next to `working · 4m` style statuses; one format reads
better and keeps one tested function. A turn without files shows only the time
(`done · 4m`); one file is `1 file`.

## Consequences

SPEC §20A.3 example updated. Implemented in `crates/mkrew-ui/src/turn.rs` (T9.8).
