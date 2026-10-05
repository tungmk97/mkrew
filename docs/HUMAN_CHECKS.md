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
- [ ] (T0.5–T0.7) Visual, input, resize and real-agent checks moved to "Waiting (blocking)"
  (T0.9). Also: the Unix exec-bit branch of `launch::is_runnable` was never compiled here.
- [ ] (T0.8) Real key-to-glyph latency: the automated number (ADR 0001 "Measurements") uses
  synthetic keys written to the PTY and stops at paint end, so it misses OS input dispatch and
  GPU present. Optionally measure with a high-speed camera / `Typometer`-style tool.
- [ ] (T1.5) The Unix branch of `mkrew-daemon/src/ipc.rs` (`try_overwrite`, Linux `mode`)
  was never compiled here; the CI Linux job (T1.12) covers it.
- [ ] (T1.6) `mk daemon start` from a normal terminal: no console window appears, the daemon
  survives closing that terminal, `mk daemon status`/`stop` work afterwards. The Unix
  `process_group` branch was never compiled here.
- [ ] (T1.8) The Unix branches of `mkrew-daemon/src/session.rs` (SIGHUP via portable-pty,
  `rustix` SIGKILL) were never compiled here; the CI Linux job (T1.12) covers it.
- [ ] (T1.10) `mk attach` in a real terminal (Windows Terminal + a Unix terminal/SSH): raw
  keys incl. arrows, Ctrl+C and paste reach the agent, resizing the window resizes the
  session, `Ctrl+]` detaches and leaves the terminal sane (cursor, no mouse/bracketed-paste
  leftovers), and re-attach repaints the same screen. Tests only cover the piped (non-tty) path.
- [ ] (T1.12) Push the repo to GitHub and check that `.github/workflows/ci.yml` runs green on
  both `windows-latest` and `ubuntu-latest`. Never run here (no push allowed). Not compiled
  locally for Linux: anything depending on bundled SQLite (`mkrew-store`, `mkrew-daemon`,
  `mkrew-cli` incl. the Unix branches from T1.5/T1.6/T1.8) — no Linux C compiler on this machine.
  `mkrew-core/protocol/fakeagent` and `spike-term` pass `cargo clippy --target
  x86_64-unknown-linux-gnu -D warnings`. The Linux job installs no apt packages (gpui-pre's
  `x11`/`wayland` features are off, so no xkbcommon/wayland/fontconfig; the cross-check never
  linked, so a link error would show only in the test step); if linking fails, add
  `sudo apt-get install -y libxkbcommon-dev libwayland-dev libfontconfig-dev libfreetype-dev`.
  Watch for timing-sensitive daemon/CLI tests on slower runners.
- [ ] (T1.12) Native Windows: `mk daemon start` from Windows Terminal opens **no** console
  window, and `mk attach` to a session of `agents/shell.toml` and `agents/claude.toml` works
  (see the T1.6 and T1.10 items above for the details to check).

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
- (T0.8a) Output drains are paced by painted frames (at most one per display refresh, §10), not
  capped at 60 Hz. On this 165 Hz display a 50 MiB flood costs ~0.8 core of UI CPU (frames
  track refresh, gap max 18–24 ms); a 60 Hz cap would cut CPU but raise the gap. Idle CPU stays
  ~3.5 % of a core because GPUI's Windows vsync thread invalidates every window each vsync —
  not fixable without patching gpui. Default: accept both for the spike, weigh them in T0.9.
  The spike still quits on child exit and may drop output ConPTY delivers after the exit; the
  real pane (M2) must keep a finished session's final screen.
- (T0.9) Tauri was not prototyped; its ADR column is qualitative. SPEC §4 UI row now names
  `gpui-pre` + `gpui-kit` (matches the recommendation; revert if you choose Tauri). M0 is
  tagged `m0-done` with five criteria unticked (real agents, input, resize, native Windows,
  recorded agent fixture); they are covered by the checks above and the T0.4 item.
- (T0.9) SPEC §15 "Idle CPU < 1 %" is read as % of the **machine** from M2 on (GPUI's Windows
  vsync floor is ~3.5 % of one core, ~0.25 % of 16 threads); T0.8 had read it per core.
  Override here if you want the stricter reading (would require patching gpui).
- (M1 plan) Agent hooks (`mk hook`, `hook.report`) belong to M3, so in M1 the E2E "`mk wait`
  until done" is satisfied by screen detection (`idle` after `idle_after_ms`) or process exit
  of the fake agent, not by a `done` hook event. `mk attach` uses `crossterm` for raw mode
  (not listed in SPEC §4; small, cross-platform, Windows-native console API).
- (T1.2) Profile schema details not fixed by SPEC §6, now written into §6: optional
  `kind = "agent"|"shell"`; shell may omit `command` (daemon picks `%COMSPEC%`/`$SHELL`);
  unknown keys are errors (catches typos); `prompt_mode` defaults to `none`, `[state]` to
  `mode = "screen"`, `idle_after_ms = 3000`; name defaults to the file stem. A missing
  `command` is reported with the file but no line (no span for an absent key).
- (T1.4) State machine details SPEC §6/§8 leave open: a `needs_input_patterns` match on the
  last 4 non-blank screen lines (up to the cursor) gives `needs_input` at once, no idle wait;
  only `working` decays to `idle`; `done` is never downgraded by the screen; hook-only
  profiles go `starting` → `unknown` on first output. The send queue also delivers in
  `unknown` (otherwise a session we cannot read never gets input), one message at a time
  (the next one waits for PTY output). Tail size needs tuning with recorded agent output.
- (T1.5) Daemon details: env overrides `MKREW_DATA_DIR` / `MKREW_CONFIG_DIR` (the `mkrew` dir
  itself) and `MKREW_SOCKET` (endpoint; same var the PTYs get). A second daemon exits with code
  2. Startup marks **every** non-`exited` session exited (incl. starting/done/unknown — no PTY
  survives a restart), wider than SPEC §8's "working/needs_input/idle". Methods not built yet
  answer `invalid_request`. Windows pipe DACL is `D:P(A;;GA;;;SY)(A;;GA;;;OW)` (SYSTEM + pipe
  owner): a daemon started from an **elevated** shell is owned by Administrators, so a
  non-elevated `mk`/UI of the same user may be refused — use the user SID if that matters.
  Unix socket mode 0600 is set on Linux only (macOS rejects it before bind; relies on the
  0700 runtime dir / data dir there).
- (T1.6) `mk` details: sync (non-tokio) client; `mk daemon start` runs `mkrew-daemon` from
  next to `mk`, else `PATH`; it is a no-op if a daemon already answers, and `stop` returns
  once the daemon released its lock. `mk daemon status` exits 1 when not running. Other
  commands never auto-start the daemon. `mk project add --dev` is not built yet
  (`ProjectAddParams`/store have no dev field) — added with `mk project update` later.
  Windows: `CreateProcess` hands every inheritable handle to the child, so a detached daemon
  kept `mk`'s caller's stdout pipe open (`mk daemon start | cat` hung). `mk` clears the
  inherit flag on its own stdio with a scoped `#[allow(unsafe_code)]` `SetHandleInformation`
  (`windows-sys`) — the only `unsafe` in the workspace. Also adds `CREATE_NEW_PROCESS_GROUP`
  (Ctrl+C in the shell does not reach the daemon); Unix uses its own process group, not
  `setsid`.
- (T1.7) `mkrew-fakeagent`: TOML scenarios only (no JSON yet); `[[step]]` tables tagged by
  `action` (`print`/`spinner`/`ask`/`sleep`/`flood`/`write_file`/`exit`), unknown keys are
  errors. Scenario = `--scenario <file|name>`, else `MKREW_FAKE_SCENARIO`, else built-in `ask`
  (an existing file wins over a built-in name; written into SPEC §16). `ask`'s answer is not
  checked (`n` behaves like `y`). `write_file` overwrites. The `mk hook fake <event>` and
  `mk comment reply` steps of §16 come with those commands (M3 / comments milestone).
  Colors use `anstyle-query` to enable Windows VT processing (no own `unsafe`).
- (T1.8) `.cmd`/`.bat` programs: portable-pty 0.9 cannot pass a raw Windows command line, so
  the planned `cmd.exe /d /s /c "..."` escaping is impossible without vendoring it. Default:
  args containing `" % & | < > ^ !` or CR/LF/NUL are **refused** for `.cmd`/`.bat` programs
  (`invalid_request` naming the arg); `.exe`/`.ps1` get any args. This blocks `prompt_mode =
  "arg"` prompts with such characters for npm shims like `claude.cmd` — M2 (`task.run`) must
  choose: vendor portable-pty's `cmdline()` with std's BatBadBut escaping, or parse the npm
  shim and run `node <script.js>` directly. Other T1.8 defaults: the daemon seeds the repo's
  sample profiles into `<config>/agents` per missing file (never overwrites) and re-reads
  profiles on every spawn; `session.kill` on an exited session is ok (idempotent); graceful
  kill = close the pseudo console (Windows) / SIGHUP (Unix), forced after 5 s
  (`TerminateProcess` / SIGKILL); every greeted connection receives all events (no per-event
  subscription yet); session state is persisted on every change.
- (T1.9) Attach details SPEC §7 leaves open: the snapshot is the **visible screen only** (no
  scrollback) as styled runs per row (trailing default blanks trimmed, wide chars in their own
  `WIDE` run, ANSI 0–15 + dim variants → indexed, theme colors → `default`) + cursor + input
  modes; `session.attach` returns `{state, snapshot}`. Overflow of the 4 MB per-client queue
  sends a new per-client event `session.snapshot` (added to the §7 table), produced by the
  session task when the client has drained; an exiting session hands its final screen to
  overflowed clients. Output frames: first byte sent at once, then at most one frame per 16 ms
  and one frame in flight per client. `session.input/resize/attach` on an exited session →
  `invalid_state`, unknown → `not_found`; `session.detach` is idempotent; resize replies once
  queued (applied right after, in order with later commands); other attached clients are not
  told about a resize. PTY input writes are still blocking writes in the session task: a big
  paste while the child's stdout is backed up can deadlock that session — queued as T1.9a
  (dedicated PTY writer thread).
- (T1.10) Sessions are started from the CLI with a separate helper `mk session spawn <project>
  --agent <profile>` (prints only the id; size = the terminal's, else 120x40), not an
  `--agent` flag on `mk attach` (added to SPEC §9). Projects are named by id or name. `mk
  attach` uses two connections (streaming + control for input/resize), polls the terminal size
  every 250 ms (no resize signal on Windows), and detaches by closing the stream connection.
  Painting a snapshot clears the screen (and toggles `?1049` to match `alt_screen`); after
  detach the agent's last screen stays visible (like `tmux detach`). Mouse mode is restored as
  `?1002` (the snapshot only says "some mode"). Windows: raw mode + `ENABLE_VIRTUAL_TERMINAL_
  INPUT/PROCESSING` via a second scoped `unsafe` (`Get/SetConsoleMode`); std's console reader
  swallows a lone **Ctrl+Z** (0-byte read), so Ctrl+Z does not reach the agent. When stdin is
  not a terminal, raw mode/resizing are skipped and stdin EOF detaches. The session's exit ends
  attach with `[session … exited with code N]`.
- (T1.11) `session.send` with `submit` appends `\r` to the text in ONE write (an agent that
  treats fast input as a paste might not submit; check with `claude` in M2). Queued texts go out
  one at a time: the next waits for PTY output that arrives after the previous one was written
  (echo counts). `session.wait` also returns when the session **exits**, whatever `until` says
  (the state can never change again); `mk wait` prints the state (`exited (exit code N)`) and
  exits 0, a timeout exits 1 — scripts tell "reached" from "exited" by the printed state. A
  `session.wait` without a timeout blocks its connection; if the client vanishes, the daemon
  keeps that wait until the session reaches the state or exits (no default timeout, as SPEC §9).
- (T1.12) CI = one job per OS running `bash scripts/verify.sh` (one source of truth with local
  sessions), on pushes to `main`/`master` and on pull requests, `fail-fast: false`, cached with
  `Swatinem/rust-cache`. Windows checkout forces `core.autocrlf false` (repo is LF; fixtures and
  snapshots depend on it). Added the `x86_64-unknown-linux-gnu` rustup target on this machine
  for cross-checking (only the Rust std component, no system install).
- (T2.1) Tabs/defaults (written into SPEC §7): the daemon keeps `terminal_tab.layout` as
  **opaque JSON** (the UI owns the split-tree model, T2.5); new tabs have layout `null`, and
  `session.spawn` with `tab_id` into a `null`-layout tab makes the session its only pane
  (`tab.updated`); otherwise the client places panes. `tab.close` deletes the row, leaves its
  sessions running and sends **no event** (one UI per user; add `tab.closed` if a second
  client ever needs it). `session.spawn` without `agent_profile` uses the project's
  `default_agent` (agent; `invalid_request` if unset) or `shell` (shell); a named profile sets
  the session kind (a conflicting `kind` is ignored). `project.update` supports `name` and
  `default_agent` only (absent = keep, `null` = clear; must be an agent-kind profile).

## Done

- [x] (T0.9) **M0 gate — UI framework.** Read `docs/decisions/0001-ui-framework.md`
  (recommendation: GPUI; switch criteria listed under "Decision"). Do the manual checks below,
  then answer here: "GPUI" (ADR → accepted, tick SPEC §19 open question) or "Tauri" (the next
  session writes ADR 0002 and replans). Then set `status: RUNNING`; the next session plans M1.
  Note: the automated "grid-dump snapshot fed with recorded agent output" criterion is still
  open (only a hand-made TUI frame exists, see T0.4 below); answering GPUI accepts M0 without it
  — or record a real session first.
  **Answer (2026-10-05): GPUI straight away — follow the recommendation; all assumed defaults
  accepted.** ADR 0001 → accepted, SPEC §19 question ticked, `status: RUNNING`.
- [x] (T0.9 / T0.7) Real agents: `cargo run -p spike-term -- claude` (and `-- codex`, `-- omp`)
  on native Windows. The npm `.cmd`/`.ps1` shim is found via `PATHEXT`, the TUI renders with
  correct colors and no broken frames. Tests only used fake shims.
- [x] (T0.9 / T0.6) Input: type in the shell and in `claude` (letters, AltGr chars on your
  layout, dead keys / IME if you use one — expected to fail, no `InputHandler` yet), Enter,
  Backspace, arrows/history, Tab, Ctrl+C on a running command, Esc;
  Ctrl+Shift+V of multi-line text (shell and a bracketed-paste TUI). — ok
- [x] (T0.9 / T0.6) Resize: drag the window edges with a shell and with `claude` running; the
  prompt/TUI reflows to the new size without leftovers. — ok
- [x] (T0.9 / T0.5) Visuals on native Windows, dark and light OS themes: prompt readable, `dir`
  / TUI colors, accent cursor block, wide chars/emoji aligned. — ok
- [x] (T0.9 / T0.8) Perceived latency and idle cost: type in
  `cargo run --release -p spike-term -- --stats` (read `key_p50` lines) and judge the idle CPU
  (~3.5 % of one core, GPUI vsync floor) and flood CPU (~0.8 core at 165 Hz) in the ADR. — ok
