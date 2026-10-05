# 0002 — Pace flood output in the daemon

- Date: 2026-10-05
- Status: accepted
- Milestone: M2

## Context

SPEC §15: six panes each receiving a continuous output flood, daemon CPU < 1 core. T2.9's
stress test (`crates/mkrew-ui/tests/stress.rs`, release) measured 148–170 % of a core. Profiling
`Actor::on_output` (6 × 200k flood lines, ~83 MB): ~3.2 s CPU, of which the `alacritty_terminal`
emulator is ~2.2 s; log writes, attachment copies, screen lines and state patterns are the rest.
The emulator parses ~85 MB/s on an idle core but ~40 MB/s while the floods load the machine
(fake agents, conhost, the UI's emulators), so per-byte work is close to its floor. Every
session task ran its output in parallel, so CPU grew with the number of floods.

## Options

1. Cheaper per-byte work only — the emulator dominates and is needed for state detection and
   snapshots; cannot get 6 floods under a core.
2. Cap the tokio runtime's worker threads — also slows commands, attaches and the IPC server.
3. Serialize flood batches across sessions and pace them — bounds flood work to a share of one
   core; quiet sessions are unaffected.

## Decision

Option 3. A session that handled more than 64 KiB in a 100 ms window is flooding; its batches
wait for a daemon-wide FIFO turn (`Hub::flood_turn`), and a `Pacer` keeps flood work at ≤ 2/3 of
a core (pause = 1/2 of the time spent, paid in ≥ 16 ms chunks because Windows timers are coarse;
idle time pays the debt off). The wait is a `select!` branch, so the session still serves
commands (attach, kill, resize, send), and quiet sessions never take the turn. Its PTY reader
blocks meanwhile, back-pressuring the child, as before.

## Consequences

- Release stress: daemon 66–72 % of a core (was 148–170 %); the 6 floods take ~3.3 s instead of
  ~2.7 s. `stress.rs` asserts < 100 % in release.
- A flooding agent's state detection sees its screen up to one pause later (tens of ms).
- No SPEC change: §15 is met as written.
