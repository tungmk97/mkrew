# Human checks

The build loop never stops to ask you. Everything that needs your eyes, hands or decision
lands here. Skim it at the end of each milestone (or when `scripts/status.sh` says so).

How to answer: write your answer under the item, move it to "Done", and if the loop status is
`WAITING_HUMAN` set `status: RUNNING` in `docs/PROGRESS.md`. Then restart the loop.

## Waiting (blocking)

_Items that stop the loop until you answer._

## To verify (non-blocking)

_Things the agent could not verify automatically (visual checks, real agents, Windows-only behavior).
The loop keeps going; report problems by adding a task line to PROGRESS.md or writing them here._

- [ ] (T0.1) Run `pwsh scripts/verify.ps1` once. The headless session was not allowed to run
  `pwsh`; `bash scripts/verify.sh` (same fmt/clippy/test steps) passed on Windows.
- [ ] (T0.2) Before T0.8 (measurements, release build): install the Windows SDK so `fxc.exe`
  is available, or set `GPUI_FXC_PATH`. Release gpui builds panic without it.

## Questions with assumed defaults

_Decisions the agent made on its own. Override them here if you disagree._

- (T0.1) Crate license: no `license` field set yet (SPEC §19 leaves it open). Default: decide
  when the repo goes public.
- (T0.1) Rust edition 2024 for all crates; clippy `unwrap_used`/`expect_used` warn (denied by
  verify), allowed in tests via `clippy.toml`.
- (T0.2) gpui source: use crates.io `gpui-pre = "=0.3.7"` (renamed to `gpui`) instead of a git
  dep on zed `main`, because `gpui-component` 0.7 pins exactly that version. Note: `gpui-pre`
  is a third-party snapshot of Zed published by the gpui-component maintainers, not by Zed
  itself (supply-chain trust choice). See ADR 0001.
- (T0.2) Keep `portable-pty` 0.9 (SPEC §4) despite known ConPTY EOF quirks; alacritty's own
  `tty` module (what Zed uses) is the fallback if it blocks. Use crates.io
  `alacritty_terminal` 0.26, not Zed's fork.
- (T0.2) Release builds of gpui on Windows need `fxc.exe` (Windows SDK) — the spike is built in
  debug mode by verify, so no install is required now.

## Done

