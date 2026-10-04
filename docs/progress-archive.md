# Progress archive

Done tasks moved out of PROGRESS.md to keep it short. Newest first.

## M0 — Terminal-in-GPUI spike (tag `m0-done`)

Acceptance criteria at completion (unticked ones are in HUMAN_CHECKS "Waiting (blocking)" / T0.4):

- [ ] Runs `claude` (or `codex`) in the window; TUI renders with correct colors, no broken frames.
- [ ] Typing, arrows, Ctrl+C, multi-line paste work.
- [ ] Window resize redraws the TUI correctly.
- [ ] Works on native Windows.
- [ ] Automated: grid-dump snapshot test fed with recorded agent output.
- [x] Measured on Windows (in the ADR): idle RAM with one terminal, key-to-glyph latency, CPU during an output flood.
- [x] ADR `docs/decisions/0001-ui-framework.md` + HUMAN_CHECKS entries; status set to WAITING_HUMAN.

Tasks:

- [x] T0.9 Wrap-up: finalize ADR 0001 with recommendation (GPUI vs Tauri) and evidence; add manual checks (real agents, Windows, resize, paste) to HUMAN_CHECKS as "Waiting (blocking)"; run Step 8 of BUILD_LOOP. (spec: §18, §19) — docs(adr): recommend GPUI for the M0 gate
- [x] T0.8a Wake-on-output: replace the 16 ms poll in `view.rs` with a wake from the PTY reader thread, re-measure, update the ADR. (spec: §10, §15) — perf(spike): wake the UI on PTY output instead of polling
- [x] T0.8 Measurements: `--flood` mode + harness for idle RAM, key latency, flood CPU; numbers in the ADR. (spec: §15) — feat(spike): flood mode, --stats and measurement harness
- [x] T0.7 Launch options: `spike-term -- <command> [args]`; Windows `PATHEXT` lookup. (spec: §8, §17) — feat(spike): launch a command via PATH/PATHEXT lookup
- [x] T0.6 Input & resize: key → bytes mapping, bracketed paste, resize to PTY and grid. (spec: §10, §13) — feat(spike): keyboard input, paste and resize
- [x] T0.5 GPUI window: render the grid with tokens from a minimal `theme.rs`. (spec: §10, §14) — feat(spike): render terminal grid in a GPUI window
- [x] T0.4 Snapshot tests: ANSI fixtures + `insta` snapshots of the dumped grid. (spec: §16) — test(spike): grid-dump snapshot tests from ANSI fixtures
- [x] T0.3 Headless terminal core: `portable-pty` + `alacritty_terminal`, `Grid` snapshot, `--dump`. (spec: §8, §10) — feat(spike): headless terminal core with grid dump
- [x] T0.2 Research spike: gpui, gpui-component, portable-pty, alacritty_terminal → ADR 0001 Context. (spec: §4, §19) — docs(adr): draft ADR 0001 context from research spike
- [x] T0.1 Cargo workspace skeleton with `spike-term`; verify green. (spec: §4, §17) — chore: cargo workspace skeleton with spike-term
