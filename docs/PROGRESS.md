# Mkrew build progress

<!-- loop-state: machine-readable, keep these exact keys on their own lines -->
status: RUNNING
milestone: M0
<!-- /loop-state -->

Status values: `RUNNING` (loop continues) · `WAITING_HUMAN` (see docs/HUMAN_CHECKS.md, then set back to RUNNING) · `BLOCKED` (no task can progress) · `DONE`.
Task markers: `[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (see note).

## Current milestone: M0 — Terminal-in-GPUI spike (SPEC §18)

### Acceptance criteria

- [ ] Runs `claude` (or `codex`) in the window; TUI renders with correct colors, no broken frames.
- [ ] Typing, arrows, Ctrl+C, multi-line paste work.
- [ ] Window resize redraws the TUI correctly.
- [ ] Works on native Windows.
- [ ] Automated: grid-dump snapshot test fed with recorded agent output.
- [x] Measured on Windows (in the ADR): idle RAM with one terminal, key-to-glyph latency, CPU during an output flood.
- [ ] ADR `docs/decisions/0001-ui-framework.md` + HUMAN_CHECKS entries; status set to WAITING_HUMAN.

### Task queue

- [ ] T0.8a Wake-on-output: replace the 16 ms poll in `view.rs` with a wake from the PTY reader thread (reader coalesces chunks; UI drains once per frame when woken), then re-run `scripts/measure-spike.sh` and update the ADR "Measurements" table (targets: idle CPU < 1 % of a core, key p95 < 16 ms, flood frame gap < 16 ms with throughput near the headless `flood_pty` rate). (spec: §10, §15)
- [ ] T0.9 Wrap-up: finalize ADR 0001 with recommendation (GPUI vs Tauri) and evidence; add manual checks (real agents, Windows, resize, paste) to HUMAN_CHECKS as "Waiting (blocking)"; run Step 8 of BUILD_LOOP. (spec: §18, §19)

### Done (last 10)

- [x] T0.8 Measurements: add a `--flood` mode (prints high-volume colored output) and a small harness/instructions to measure idle RAM, key latency and flood CPU; record numbers in the ADR (automated where possible, otherwise as HUMAN_CHECKS items). (spec: §15) — feat(spike): flood mode, --stats and measurement harness

- [x] T0.7 Launch options: `spike-term -- <command> [args]` to run `claude`/`codex`/`omp` instead of the shell; Windows `PATHEXT` lookup. (spec: §8, §17) — feat(spike): launch a command via PATH/PATHEXT lookup

- [x] T0.6 Input & resize: key → bytes mapping (printables, Enter, Backspace, arrows, Ctrl+letter, Esc, Tab), bracketed paste, resize propagates to PTY and grid. Unit-test the key mapping. (spec: §10, §13) — feat(spike): keyboard input, paste and resize
- [x] T0.5 GPUI window: render the grid (colors, cursor) using tokens from a minimal `theme.rs`, following the §10 performance rules from the start (styled text runs per row, dirty rows only, one present per frame). (spec: §10, §14) — feat(spike): render terminal grid in a GPUI window
- [x] T0.4 Snapshot tests: ANSI fixtures in `crates/spike-term/tests/fixtures/` (colors, cursor moves, alternate screen, wide chars/emoji, a recorded TUI frame); `insta` snapshots of the dumped grid. (spec: §16) — test(spike): grid-dump snapshot tests from ANSI fixtures
- [x] T0.3 Headless terminal core in `spike-term`: spawn a shell via `portable-pty`, feed bytes into `alacritty_terminal`, expose a `Grid` snapshot (chars + fg/bg + cursor); `--dump` flag prints the grid as text. (spec: §8, §10) — feat(spike): headless terminal core with grid dump
- [x] T0.2 Research spike: use the researcher to confirm current versions/APIs of `gpui` (standalone usage outside Zed, Windows support), `gpui-component`, `portable-pty`, `alacritty_terminal`; write findings to `docs/decisions/0001-ui-framework.md` (draft, "Context" section). (spec: §4, §19) — docs(adr): draft ADR 0001 context from research spike
- [x] T0.1 Cargo workspace skeleton: root `Cargo.toml` (workspace, resolver 2, shared lints), `rust-toolchain.toml` already present, crate `crates/spike-term` (bin) that prints "hello"; `scripts/verify.*` green. (spec: §4, §17) — chore: cargo workspace skeleton with spike-term

## Notes for the next session

- Next: T0.8a. CLI: `spike-term [--dump | --stats [--probe-keys]] [-- <cmd>]` / `--flood <MiB>` (`launch::parse_args` → `Mode`). `view.rs`: `spawn_every` timers (16 ms drain, 1 s stats, 500 ms probe); `TerminalView::send` = all PTY key writes; `stats.rs` = `Stats` + `ProcessProbe` (sysinfo). In Git Bash, `/C` args get MSYS-mangled — use `//C`.
- T0.8 numbers (ADR 0001 "Measurements"): idle ~3.5 % of a core, 65–101 MB; key p50 22 / p95 37 ms; flood 5.8 MB/s UI vs 7.7 MB/s headless (`tests/flood_pty.rs`), ~34 fps, frame gap ≤ 53 ms. Suspects: 16 ms poll tick, 1 MiB drain budget, 64-chunk channel (`pty.rs`).
- Measuring: release build works (`fxc.exe` on PATH). Headless sessions can't run `bash scripts/measure-spike.sh` directly (permission) — run its `timeout N ./target/release/spike-term.exe --stats ... 2>target/measure/X.log` lines one by one; `Read` logs, don't cat them (flood log is long).
- gpui deps: `gpui = { package = "gpui-pre" }` + `gpui_platform = { package = "gpui-pre-platform" }` (=0.3.7); entry `gpui_platform::application().run(..)`. Windows: `Keystroke.key_char` is None for control keys; AltGr = ctrl+alt with `key_char`.
- `Emulator::advance` reply bytes MUST be written to the PTY (ConPTY waits for the DSR reply); `drain`/`pump` do it. Dirty rows found by comparing cells; row cache doesn't key on cell width.
- Snapshot tests: `crates/spike-term/tests/grid_snapshots.rs`; accept new snapshots by `mv *.snap.new *.snap` (no cargo-insta). GUI smoke: `timeout 6 ./target/debug/spike-term.exe` → exit 124 = OK.
- Windows-native target; edition 2024, `[lints] workspace = true`, `unsafe_code` denied. `unwrap/expect` denied outside `#[test]` fns. Headless: no `pwsh`/`powershell`/`tasklist`, use `bash scripts/verify.sh`.
