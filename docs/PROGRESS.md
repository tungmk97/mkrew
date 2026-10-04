# Mkrew build progress

<!-- loop-state: machine-readable, keep these exact keys on their own lines -->
status: WAITING_HUMAN
milestone: M1
<!-- /loop-state -->

Status values: `RUNNING` (loop continues) · `WAITING_HUMAN` (see docs/HUMAN_CHECKS.md, then set back to RUNNING) · `BLOCKED` (no task can progress) · `DONE`.
Task markers: `[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (see note).

## Current milestone: M1 — Daemon & CLI, no UI (SPEC §18)

### Acceptance criteria

_Copied from SPEC §18 when the milestone is planned._

### Task queue

_Empty — the next session plans M1 (`mkrew-plan-milestone`)._

### Done (last 10)

_M0 tasks moved to `docs/progress-archive.md`._

## Notes for the next session

- WAITING_HUMAN: M0 gate. Human answers the blocking items in `docs/HUMAN_CHECKS.md` (GPUI vs Tauri; ADR 0001 recommends GPUI) and sets `status: RUNNING`.
- If the answer is "Tauri": write ADR 0002, revert the SPEC §4 UI row, then plan M1 (daemon/CLI are unaffected).
- M0 tagged `m0-done` with 5 criteria left to the human gate (see archive). Spike code: `crates/spike-term` (PTY inbox/wake in `pty.rs`, renderer in `view.rs`, key mapping, `launch.rs` PATHEXT lookup) — reuse in M1 daemon (PTY reader) and M2 UI.
- `Emulator::advance` reply bytes MUST be written to the PTY (ConPTY waits for the DSR reply). portable-pty on Windows: wait on the child on its own thread; reader never hits EOF.
- gpui deps: `gpui = { package = "gpui-pre" }` + `gpui_platform = { package = "gpui-pre-platform" }` (=0.3.7). Release builds need `fxc.exe` (on PATH here).
- Snapshot tests: accept by `mv *.snap.new *.snap` (no cargo-insta). GUI smoke: `timeout 6 ./target/debug/spike-term.exe` → exit 124 = OK.
- Windows-native target; edition 2024, `[lints] workspace = true`, `unsafe_code` denied. `unwrap/expect` denied outside `#[test]` fns. Headless: no `pwsh`/`powershell`/`tasklist`/`python`, use `bash scripts/verify.sh`.
