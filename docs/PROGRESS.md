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
- [ ] Measured on Windows (in the ADR): idle RAM with one terminal, key-to-glyph latency, CPU during an output flood.
- [ ] ADR `docs/decisions/0001-ui-framework.md` + HUMAN_CHECKS entries; status set to WAITING_HUMAN.

### Task queue

- [ ] T0.7 Launch options: `spike-term -- <command> [args]` to run `claude`/`codex`/`omp` instead of the shell; Windows `PATHEXT` lookup. (spec: §8, §17)
- [ ] T0.8 Measurements: add a `--flood` mode (prints high-volume colored output) and a small harness/instructions to measure idle RAM, key latency and flood CPU; record numbers in the ADR (automated where possible, otherwise as HUMAN_CHECKS items). (spec: §15)
- [ ] T0.9 Wrap-up: finalize ADR 0001 with recommendation (GPUI vs Tauri) and evidence; add manual checks (real agents, Windows, resize, paste) to HUMAN_CHECKS as "Waiting (blocking)"; run Step 8 of BUILD_LOOP. (spec: §18, §19)

### Done (last 10)

- [x] T0.6 Input & resize: key → bytes mapping (printables, Enter, Backspace, arrows, Ctrl+letter, Esc, Tab), bracketed paste, resize propagates to PTY and grid. Unit-test the key mapping. (spec: §10, §13) — feat(spike): keyboard input, paste and resize
- [x] T0.5 GPUI window: render the grid (colors, cursor) using tokens from a minimal `theme.rs`, following the §10 performance rules from the start (styled text runs per row, dirty rows only, one present per frame). (spec: §10, §14) — feat(spike): render terminal grid in a GPUI window
- [x] T0.4 Snapshot tests: ANSI fixtures in `crates/spike-term/tests/fixtures/` (colors, cursor moves, alternate screen, wide chars/emoji, a recorded TUI frame); `insta` snapshots of the dumped grid. (spec: §16) — test(spike): grid-dump snapshot tests from ANSI fixtures
- [x] T0.3 Headless terminal core in `spike-term`: spawn a shell via `portable-pty`, feed bytes into `alacritty_terminal`, expose a `Grid` snapshot (chars + fg/bg + cursor); `--dump` flag prints the grid as text. (spec: §8, §10) — feat(spike): headless terminal core with grid dump
- [x] T0.2 Research spike: use the researcher to confirm current versions/APIs of `gpui` (standalone usage outside Zed, Windows support), `gpui-component`, `portable-pty`, `alacritty_terminal`; write findings to `docs/decisions/0001-ui-framework.md` (draft, "Context" section). (spec: §4, §19) — docs(adr): draft ADR 0001 context from research spike
- [x] T0.1 Cargo workspace skeleton: root `Cargo.toml` (workspace, resolver 2, shared lints), `rust-toolchain.toml` already present, crate `crates/spike-term` (bin) that prints "hello"; `scripts/verify.*` green. (spec: §4, §17) — chore: cargo workspace skeleton with spike-term

## Notes for the next session

- Next: T0.7. `spike-term` (no args) opens the GPUI window. `view.rs`: TerminalView drains the PTY every 16 ms; focus + `on_key_down` → `input::key_to_bytes`; Ctrl+Shift+V → `input::paste_bytes`; `fit_to_window` (in `render`) resizes emulator + PTY from the viewport. `input.rs` = pure xterm key encoding (unit-tested); `layout.rs` = row layout, `RowCache`, `grid_size`. `main.rs` args are a slice match — extend for `-- <cmd>`.
- gpui deps: `gpui = { package = "gpui-pre" }` + `gpui_platform = { package = "gpui-pre-platform" }` (=0.3.7); entry `gpui_platform::application().run(..)`. `window.focus(&h, cx)`. Windows: `Keystroke.key_char` is None for control keys; AltGr = ctrl+alt with `key_char`.
- `Emulator::advance` reply bytes MUST be written to the PTY (ConPTY waits for the DSR reply); `drain`/`pump` do it. OSC 10/11, CSI 14t not answered yet.
- Known skews for T0.8: the 16 ms poll timer wakes the UI even when idle (idle CPU) and adds ≤16 ms key latency → switch to wake-on-output if numbers suffer. Dirty rows found by comparing cells, not alacritty damage. Row cache doesn't key on cell width (clear it on font change).
- Snapshot tests: `crates/spike-term/tests/grid_snapshots.rs`; accept new snapshots by `mv *.snap.new *.snap` (no cargo-insta).
- T0.8 needs a release build → `fxc.exe` (Windows SDK) or `GPUI_FXC_PATH`; check HUMAN_CHECKS first. GUI smoke: `timeout 6 ./target/debug/spike-term.exe` → exit 124 = ran without panic.
- Windows-native target; edition 2024, `[lints] workspace = true`. `unwrap/expect` denied outside `#[test]` fns. Headless: no `pwsh`, use `bash scripts/verify.sh`.
