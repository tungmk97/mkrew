# 0015 - Only visible screen changes count as activity

- Date: 2026-10-07
- Status: accepted
- Milestone: post-M13 fixes

## Context

SPEC §6 said a session is `working` while it is "producing output" and `idle` after
`idle_after_ms` (3 s) with no output. Oh My Pi and Claude Code send bytes at an idle prompt at
least every 3 s, and those bytes change nothing the user can see: cursor show/hide, OSC title
and progress sequences, synchronized-update markers, repaints of identical text. The user's
database had three omp sessions in `working` (source `screen`) for 882 s while they sat at a
static prompt, and the UI's elapsed counter kept going from the first output after spawn.

## Options

1. Count only output that changes the visible screen text (a hash of the visible cells'
   characters after each batch). Cheap, deterministic, needs no profile change.
2. alacritty damage tracking. Also flags cursor moves and mode changes, so the same false
   activity remains.
3. Per-agent ignore patterns for the invisible sequences. Breaks §2.1 (no per-agent code) and
   needs tuning for every agent release.

## Decision

Option 1. `Emulator::text_fingerprint` hashes the characters of the visible screen (no colors,
no cursor); `Actor::on_output` calls the state machine only when the fingerprint changed since
the last batch. The first batch always counts, so hook-only profiles still leave `starting`.
The send queue still sees every byte.

## Consequences

- SPEC §6 "Session states": `working` / `idle` are defined by visible text changes.
- Typing at a prompt echoes, so a session shows `working` for up to `idle_after_ms`.
- A frozen screen during a long silent tool call shows `idle`, as true silence already did.
- A status-line clock that ticks once a minute causes a short `working` blip.
