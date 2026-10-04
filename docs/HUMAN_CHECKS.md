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
- [x] (T0.2) Before T0.8 (measurements, release build): install the Windows SDK so `fxc.exe`
  is available, or set `GPUI_FXC_PATH`. Release gpui builds panic without it. — Resolved:
  `fxc.exe` is on PATH (`~/msvc/Windows Kits/10/bin/10.0.28000.0/x64`); T0.8 built release.
- [ ] (T0.4) `crates/spike-term/tests/fixtures/tui_frame.ans` is a hand-made Claude-Code-like
  frame. Record a real `claude` session (e.g. via `script`/a PTY logger), convert it to the
  escaped fixture format (`tests/fixtures/README.md`), add it as a fixture and accept its
  snapshot. Until then the M0 "recorded agent output" criterion stays unticked.
- [ ] (T0.5) Run `cargo run -p spike-term` and check the terminal window visually in dark and
  light OS themes: shell prompt readable, colors (e.g. `dir`/`ls --color`, a TUI like
  `claude`), accent cursor block, wide chars/emoji aligned on the grid. Optionally save
  screenshots to `docs/screenshots/m0-terminal-{dark,light}.png` (headless session cannot
  capture them; it only confirmed the window starts without panicking).
- [ ] (T0.6) In `cargo run -p spike-term`: type in the shell (letters, AltGr chars on your
  layout), Enter, Backspace, arrows/history, Tab completion, Ctrl+C on a running command,
  Esc; Ctrl+Shift+V of multi-line text (shell and a bracketed-paste TUI); resize the window
  and check the prompt/TUI reflows to the new size. Headless session only unit-tested these.
- [ ] (T0.7) Run `cargo run -p spike-term -- claude` (and `-- codex`, `-- omp`) on Windows:
  the npm `.cmd`/`.ps1` shim is found via `PATHEXT` and the agent TUI starts in the window.
  Tests only ran fake `.cmd`/`.ps1` shims (no real agents allowed); the Unix exec-bit branch of
  `launch::is_runnable` was not compiled on this Windows host.
- [ ] (T0.8) Real key-to-glyph latency: the automated number (ADR 0001 "Measurements") uses
  synthetic keys written to the PTY and stops at paint end, so it misses OS input dispatch and
  GPU present. Measure once with a high-speed camera / `Typometer`-style tool, or at least
  type in `cargo run --release -p spike-term -- --stats` and read the `key_p50` lines.

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
- (T0.3) `spike-term` has a lib target but uses `anyhow` (not `thiserror`, SPEC §17) because it
  is throwaway spike code and `portable-pty` itself returns `anyhow::Error`. Real crates
  (`mkrew-daemon` etc.) will follow §17.
- (T0.4) Terminal fixtures are stored as escaped text (`\e`, `\r`, `\xNN`; literal line breaks
  ignored) instead of raw bytes: readable, diffable, immune to git CRLF conversion, and the
  agent's file tools cannot write raw control bytes. Raw recordings must be converted first.
- (T0.5) Spike design tokens live in `crates/spike-term/src/theme.rs` (no `mkrew-ui` crate
  yet). The ANSI palette is the agent's own choice harmonized with §14; light-theme "white"
  ANSI colors are darkened (`#8A8C94`, `#55575F`) so they stay readable on `#FAFAF9`. Cursor is
  always an accent block (shape/unfocused style later).
- (T0.6) Spike input has no IME/`InputHandler`: IME composition (Vietnamese Telex, CJK) and
  dead keys may not reach the PTY. Default: out of scope for M0; the real terminal view
  registers an input handler. Paste is Ctrl+Shift+V only (§13); Shift+Insert/right-click later.
- (T0.6) PTY writes (keys, paste) are blocking writes on the UI thread; a huge paste into a
  child that is not reading could stall the window. Default: acceptable for the spike; the
  daemon (M1) owns PTY writes on its own thread.
- (T0.7) `spike-term -- <cmd>` resolves the program itself (PATH + `PATHEXT`, then `.ps1`;
  never the extensionless npm sh shim) and runs `.ps1` via `powershell.exe -File`. `.cmd`/`.bat`
  go straight to CreateProcessW, so cmd.exe re-parses args: quoted args with a space in the
  shim path, or `& | % ^` in args, can break (BatBadBut). Default: fine for the spike; the
  daemon's command lookup (M1) must build `cmd.exe /d /s /c "..."` with cmd-aware escaping.
  `-- <cmd>` starts in the current directory.
- (T0.8) SPEC §15 "Idle CPU < 1 %" is read as % of one core (stricter); measured ~3 % of one
  core (~0.25 % of a 16-thread machine). Stats use `sysinfo` 0.31 (already in the tree via gpui)
  because `unsafe` is denied and `powershell`/`tasklist` are not available to headless sessions.
  Added T0.8a (wake-on-output + reader coalescing, re-measure) before the T0.9 wrap-up, since
  idle CPU, key latency and the flood frame gap miss §15 for likely fixable reasons.

## Done

