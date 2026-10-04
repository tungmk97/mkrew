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

- [ ] T0.1 Cargo workspace skeleton: root `Cargo.toml` (workspace, resolver 2, shared lints), `rust-toolchain.toml` already present, crate `crates/spike-term` (bin) that prints "hello"; `scripts/verify.*` green. (spec: §4, §17)
- [ ] T0.2 Research spike: use the researcher to confirm current versions/APIs of `gpui` (standalone usage outside Zed, Windows support), `gpui-component`, `portable-pty`, `alacritty_terminal`; write findings to `docs/decisions/0001-ui-framework.md` (draft, "Context" section). (spec: §4, §19)
- [ ] T0.3 Headless terminal core in `spike-term`: spawn a shell via `portable-pty`, feed bytes into `alacritty_terminal`, expose a `Grid` snapshot (chars + fg/bg + cursor); `--dump` flag prints the grid as text. (spec: §8, §10)
- [ ] T0.4 Snapshot tests: ANSI fixtures in `crates/spike-term/tests/fixtures/` (colors, cursor moves, alternate screen, wide chars/emoji, a recorded TUI frame); `insta` snapshots of the dumped grid. (spec: §16)
- [ ] T0.5 GPUI window: render the grid (colors, cursor) using tokens from a minimal `theme.rs`, following the §10 performance rules from the start (styled text runs per row, dirty rows only, one present per frame). (spec: §10, §14)
- [ ] T0.6 Input & resize: key → bytes mapping (printables, Enter, Backspace, arrows, Ctrl+letter, Esc, Tab), bracketed paste, resize propagates to PTY and grid. Unit-test the key mapping. (spec: §10, §13)
- [ ] T0.7 Launch options: `spike-term -- <command> [args]` to run `claude`/`codex`/`omp` instead of the shell; Windows `PATHEXT` lookup. (spec: §8, §17)
- [ ] T0.8 Measurements: add a `--flood` mode (prints high-volume colored output) and a small harness/instructions to measure idle RAM, key latency and flood CPU; record numbers in the ADR (automated where possible, otherwise as HUMAN_CHECKS items). (spec: §15)
- [ ] T0.9 Wrap-up: finalize ADR 0001 with recommendation (GPUI vs Tauri) and evidence; add manual checks (real agents, Windows, resize, paste) to HUMAN_CHECKS as "Waiting (blocking)"; run Step 8 of BUILD_LOOP. (spec: §18, §19)

### Done (last 10)

_(none yet)_

## Notes for the next session

- Fresh repo. Start with T0.1.
- Target OS for the user is native Windows; keep every path/command Windows-safe.
