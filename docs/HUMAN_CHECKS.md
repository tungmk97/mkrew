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
- [ ] (T2.2) Run `cargo run -p mkrew-ui` with no daemon running: a "Mkrew" window opens
  ("Connecting to the daemon…" → "Connected." + version), a detached `mkrew-daemon` starts
  (no console window), and it keeps running after the window closes; a second launch reuses
  it. Check the shell in dark and light OS themes (§14 design check; running GUI binaries
  needs approval in headless sessions, so not done automatically).
  (T2.3: with no live session the line now reads "No running sessions."; the version is logged.)
- [ ] (T2.3) Start a session (`mk session spawn <project> --agent shell`), then `cargo run -p
  mkrew-ui`: the window shows it in a terminal pane (colors, cursor block, wide chars); typing,
  Ctrl+C, arrows and Ctrl+Shift+V paste reach the shell; resizing the window reflows and
  `mk session list` shows the new cols/rows; minimizing does not shrink the session; `exit`
  shows "✕ Session exited (code 0)." under the pane; a `flood` fake-agent session stays
  responsive. Check dark and light themes.
- [ ] (T2.4) `cargo run -p mkrew-ui`: the sidebar lists workspaces; with none, "No workspaces
  yet." + "Add workspace Ctrl+Shift+K". The button, the header "+" and Ctrl+Shift+K (also while
  a terminal has focus) open the OS folder picker; the picked folder appears expanded ("No
  sessions yet."); adding the same folder again shows an error under the header. Expand /
  collapse a workspace; sessions show icon + status text; the workspace row shows the working
  count and `?` when one needs input (fake agent `ask`). Clicking another session switches the
  pane. Hover tooltips (path, shortcut). Check dark and light themes.
- [ ] (T2.5b) `cargo run -p mkrew-ui` with two live sessions in one workspace: a tab bar shows
  "Terminal"; clicking the second session in the sidebar splits the pane side by side (4 px
  gaps, rounded frames, focused pane has a faint accent border, the other is slightly dimmed);
  clicking a pane moves focus and keyboard input. "+" adds "Terminal 2" (empty state), clicking
  tabs switches panes (hidden ones stop updating). Close the window and reopen: same tabs and
  split, both panes live. Restart the daemon (`mk daemon stop`): panes show "✕ Session
  exited", layout kept. Check dark and light themes.
- [ ] (T2.6) `cargo run -p mkrew-ui` with a workspace whose default agent is set (`mk` /
  `project.update`): "+ Agent" and "+ Shell" open panes in the project folder next to the focused
  one (shell = cmd/pwsh, `cd` shows the project path); an empty tab offers "New agent" / "New
  shell". Ctrl+Shift+X (also while the terminal has focus — must not reach the PTY) on a shell
  closes it; on a running agent the dialog offers Kill / Hide / Cancel; hidden sessions stay in
  the sidebar and clicking them brings the pane back; close and reopen the UI: still alive.
  With 13+ live sessions the tab bar shows the "⚠ … sessions running" notice.
- [ ] (T2.7a) `cargo run -p mkrew-ui`, focus a terminal: Alt+1…9 / Ctrl+Alt+↑↓ switch
  workspaces, Ctrl+Shift+T / Ctrl+Shift+A open a tab with a shell / the default agent,
  Ctrl+Tab / Ctrl+Shift+Tab / Ctrl+Alt+1…9 switch tabs, Ctrl+Shift+W closes the tab (its sessions
  stay in the sidebar), **Alt+Shift+= / Alt+Shift+-** split right/down (check that GPUI on
  Windows reports these as `alt-shift-=` / `alt-shift--` on your layout; rebind in keymap.toml
  if not; also that Ctrl+Tab is not eaten by the OS), Alt+arrows move focus, Ctrl+Shift+V pastes (bracketed in Claude Code). None of these
  reach the PTY; **Ctrl+C, Ctrl+R, Esc inside Claude Code** still work. Put
  `close_pane = "ctrl-shift-q"` and `bogus = "x"` in `%APPDATA%\mkrew\keymap.toml`: Ctrl+Shift+Q
  closes panes, the sidebar shows "✕ unknown command `bogus` …", the tooltip says Ctrl+Shift+Q.
  AltGr layouts: Ctrl+Alt+digit is AltGr+digit (e.g. `{` on German) and is intercepted by the
  tab-jump defaults — rebind if you type those characters.
- [ ] (T2.7b) `cargo run -p mkrew-ui`, in a shell pane run a command with long output (`dir /s`):
  mouse-drag selects text (accent-tinted, readable, I-beam pointer), Ctrl+Shift+C copies it
  (paste elsewhere matches; the highlight clears), Shift+PgUp/PgDn and the wheel page through
  the history, typing returns to the live screen. In Claude Code (full-screen): the wheel
  scrolls its view (arrow keys), selection + copy still work. Ctrl+Shift+C is not sent to the PTY.
- [ ] (T2.7c) In a pane with long output press Ctrl+Shift+F: the find bar appears top-right
  (light + dark), typing highlights matches (amber, readable) and jumps to the nearest one,
  Enter / Shift+Enter walk older / newer matches and scroll the view, "No matches" / "✕ Invalid
  pattern" show, Esc or ✕ closes it, clears the highlights and typing goes to the terminal
  again. Clicking the bar of an unfocused split focuses that pane (border moves) and the bar.
- [ ] (T2.7d) In a tab with splits: Alt+Shift+←/→/↑/↓ moves the divider next to the focused
  pane and the sizes survive a UI restart; Ctrl+Shift+M fills the tab with the focused pane
  ("Pane zoomed" notice), again shows all panes; Ctrl+Shift+R (or double-click a tab) edits its
  title inline — Enter / clicking elsewhere saves, Esc cancels — and the new title survives a
  restart.
- [ ] (T2.8) Click "⋯" on the active workspace (or right-click a workspace row): the menu
  drops down below the row. "Open in Zed" (or Ctrl+Shift+Z) opens the folder in Zed; with
  `zed` off PATH the sidebar shows the "not found … Install Zed" hint instead. Pick another
  default agent: ✓ moves, and "+ Agent" / Ctrl+Shift+A (new agent tab) now starts that agent; survives a
  restart.
- [ ] (T2.9) **§15 stress, automated part**: `cargo build --release -p mkrew-daemon -p
  mkrew-fakeagent` then `cargo test --release -p mkrew-ui --test stress -- --nocapture` (6
  attached floods of ~14 MB each through the UI's `Router` + `Emulator`). Here it passed 3/3:
  round trip on the UI connection p50 ≈ 0.23 ms, p95 ≈ 1.3 ms; feeding one drained batch p50
  ≈ 3.7 ms, p95 ≈ 17 ms, max ≈ 23 ms; daemon RSS 33→37 MiB, test-process RSS 128→130 MiB (flat
  once the six 10k-line scrollbacks are full); daemon CPU was ≈ 1.5 cores, after T2.9a's flood
  pacing (ADR 0002) 66–76 % of a core, now asserted < 100 % in release. `verify` runs the same
  test in debug, where only correctness and daemon memory are asserted.
- [ ] (T2.9) **§15 stress, in the real UI**: open 6 panes (3 workspaces) with the fake agent
  flood profile (`agents/fake.toml` copy with `args = ["--scenario", "<file>"]`, the file a
  `flood` step with `lines = 750000` ≈ 50 MB), run them at once in a release build
  (`cargo run --release -p mkrew-ui`): typing in a 7th shell pane stays instant, the window
  repaints smoothly, every pane ends on "Flood done.", and memory (below) stays flat.
- [ ] (T2.9) **RAM < 150 MB** (§15, M2 criterion), procedure: release build, the daemon started
  by the UI, 3 workspaces, 6 live sessions (shell + fake agent; no flood), all panes shown once,
  then idle 1 min. Read "Memory (private working set)" of `mkrew.exe` and `mkrew-daemon.exe` in
  Task Manager → Details (or `Get-Process mkrew,mkrew-daemon | Select Name,WS,PM`). Expected:
  UI < 150 MB, daemon < 50 MB. Then repeat after the 6-pane flood above: the UI keeps a 10k-line
  scrollback per attached pane (≈ 100 MB for six full ones in the headless test process alone),
  so the UI may exceed 150 MB after floods — note the numbers here if it does.
- [ ] (T2.9) **§14 design check for M2** (no automatic screenshot capture): in light and dark OS
  theme take screenshots into `docs/screenshots/` of: empty window (no workspaces), sidebar with
  workspaces + sessions in every status, a tab bar with 2 tabs, a 3-pane split (focused border,
  dimmed others), an empty tab, the find bar with matches, the inline tab rename, the workspace
  menu, the kill/hide dialog, the "⚠ … sessions running" notice, an exited pane. Self-review per
  §14: no hard-coded colors/spacing (all in `theme.rs`), hover/focus/disabled states, empty
  states with one action + shortcut, body text contrast ≥ 4.5:1, status never by color alone.
- [ ] (T2.9) **Real Claude Code** in a UI pane (`agents/claude.toml`, your keys/login): Ctrl+C,
  Ctrl+R, Esc, arrows, Shift+Tab and bracketed paste reach it; full-screen redraws are clean;
  its status in the sidebar follows working → needs input → idle.
- [ ] (T2.9) **Reopen UI**: with 6 live sessions in split tabs, close the window and start the
  UI again: same workspaces, tabs, splits and sizes; every visible pane re-attached showing the
  current screen; hidden sessions still in the sidebar.

- [ ] (M2 wrap-up) M2 tagged `m2-done` with 8 of 11 criteria unticked. All of them wait on the
  manual T2.4–T2.9 checks above (sidebar/picker, tabs/splits, persistence, §13 shortcuts +
  Ctrl+C/Ctrl+R/Esc in Claude Code, Open in Zed, §14 design check, RAM < 150 MB, stress in the
  real UI). One §10 rule is still **not implemented**: detaching panes while the window is
  minimized. It is carried over to M3 as the first task. If a manual check fails, add a fix task
  to the M3 queue.

- [ ] (T3.1) **Minimize detaches**: with a busy agent pane visible, minimize the window
  (`mk session list` / daemon log: the UI's attachment is gone), let output continue, restore:
  every visible pane is back with the current screen and the focused pane has keyboard focus.

- [ ] (T3.4) **Integration install on the real home**: `mk integration install claude` and
  `codex`, then `status` (exit 0, no PATH warning); check `~/.claude/settings.json` keeps your
  settings + a `.bak` exists; start `claude` in a Mkrew pane, submit a prompt → working, finish →
  done (Claude's `/hooks` lists the 4 `mk hook claude` entries). `uninstall` both → your files
  as before.

- [ ] (T3.5) **OS notification + click**: restart the daemon (protocol v3), spawn the fake
  agent → a toast "<project> · fake / Needs input" appears (Windows: attributed to "Windows
  PowerShell" — no own AppUserModelID until packaging, M8); a second needs_input/done of the
  same session within 30 s shows none. Click the toast (also once from Action Center) → `mk`
  or the UI receives `notification.clicked` (the UI handles it from T3.7). Linux: same with
  your notification server.

- [ ] (T3.6) **Per-session status, dark and light** (§11, §14): spawn the fake agent and a
  shell in split panes. Each pane header shows icon, agent, title and `working · Ns` with a
  slowly turning spinner, then `needs input` with an amber pane border; the tab label shows the
  most urgent state's icon (needs input over done over working); the sidebar row text matches;
  the workspace row shows the working count and `?`. Hover the status in a pane header and in
  the sidebar → "<state> · from hook|screen|daemon · Ns ago". Idle shows `idle · Nm`; an
  exited session shows `exited (code)`. Check contrast and that the header does not crowd the
  terminal.

- [ ] (T3.7) **Inbox and notification click** (§10, §11): with two workspaces, let the fake
  agent ask in workspace A while B is shown → the Inbox lists `A: fake` with `?  needs input`;
  click it → A is shown with that pane focused. Answer and let it finish while B is shown →
  `✓ done` entry stays until you focus that pane, then leaves. With nothing pending the Inbox
  reads "Nothing needs you right now.". Minimize Mkrew, trigger needs_input, click the OS
  notification → the window is restored, in front, on that pane. Windows: check it really comes
  to the front and not only flashes in the taskbar (the click reaches the daemon, a background
  process; if it only flashes, the daemon should call `AllowSetForegroundWindow` for the UI
  process before sending `notification.clicked`).

- [ ] (T3.9) **M3 criteria that need real agents and eyes.** Do these after the T3.4–T3.7 checks
  above; tick the M3 criterion in `docs/PROGRESS.md` (or §18) once its check passes, else add a fix
  task. Restart the daemon first (protocol v3) with `mk` on PATH and `mk integration install
  claude` done.
  - **Claude approval → needs_input in ≤ 2 s** (criterion 2): in a Mkrew pane run `claude` and ask
    for something that needs a tool approval (e.g. "create a file hello.txt"). From the moment
    the approval prompt appears: within 2 s the pane header shows `? needs input` (hover: "from
    hook" or "from screen" — the screen pattern may win the race; note which, both pass), the
    sidebar Inbox lists the session, and one OS toast appears. Approve → `working` (from screen:
    no hook fires on approval), turn ends → `done` (from hook). If it takes longer, note the time and whether the first `mk hook` run
    was the slow part (first run can take seconds).
  - **Notification click focuses the pane** (criterion 3): the T3.7 click check, but with a real
    Claude approval toast and Mkrew minimized behind another app.
  - **Status on header, tab and sidebar** (criterion 4): the T3.6 check with real Claude in one
    pane and a shell in another, dark and light OS theme (§11, §14 design check); also
    `Ctrl+Shift+I` cycles through Inbox entries (T3.7a).
  - **Real omp working ↔ idle** (criterion 5): run `omp` in a Mkrew pane (`agents/omp.toml`,
    screen mode), send a prompt → `working` (hover: "from screen") while it streams, `idle`
    ≈ 3 s after output stops; send another prompt → `working` again. If it flips to idle while
    omp is still thinking or never leaves working (e.g. a blinking cursor/clock redraws), note
    what the screen shows; the next session tunes `idle_after_ms` / patterns (see the T3.8 note under "Questions with assumed defaults").

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
- (T2.2) `mkrew-ui` depends on the `mkrew-daemon` library (like `mk`) for `ipc::name`,
  `EXIT_ALREADY_RUNNING` and the detached-spawn helpers (moved from the CLI to
  `mkrew_daemon::spawn`); a small shared client crate is possible later. The UI passes its
  resolved paths to the daemon it spawns via `MKREW_DATA_DIR/CONFIG_DIR/SOCKET`. A protocol
  version mismatch is shown as an error; restarting the daemon automatically is left for later.
- (T2.2) Contrast: §14 light `status-idle` #9A9CA6 is ~2.6:1 on `bg` and the light ANSI bright
  yellow #D69E2E ~2.3:1 — below 3:1 for icons/terminal text. Kept the spec values; proposal:
  `status-idle` ≈ #8A8C94, bright yellow ≈ #B8860B (spec change + ADR if you agree).
- (T2.3) The UI emulator is rebuilt from a daemon snapshot by replaying it as ANSI (the
  `paint` module moved from `mk` to `mkrew_protocol::paint`, shared by CLI and UI). A
  `session.snapshot` after an overflow therefore resets the pane's UI-side scrollback (the
  dropped output left a gap anyway). Until the sidebar/tabs land (T2.4–T2.6) the window shows
  the most recently started live session full-window.
- (T2.4) Sidebar lists every session of a workspace, exited ones included (✕ + exit code),
  newest first; pruning/hiding old exited sessions is left for later. The first load expands
  workspaces with a live session; the app still opens the newest live session on start.
  `project.add` now emits `project.updated` (SPEC §7 "entity changed") so every UI shows a
  workspace added from `mk`. Working uses a static `◐` glyph (animated spinner, the
  elapsed/idle times and the status-source hover of §11 come later); exited rows are inert
  until "Run again"/"Resume" exist; sidebar keyboard navigation + focus rings ship with
  the keymap (T2.7). Ctrl+Shift+K is bound on every OS (§13: only global/tab commands use Cmd).
- (T2.5b) Tabs/panes UI: tabs are loaded per workspace when it is first shown (start = the
  workspace of the newest live session); a workspace with no tabs gets a "Terminal" tab. Panes
  of exited or unknown sessions keep their slot in the layout (placeholder "✕ Session exited"
  / "Cannot attach") so layouts survive daemon restarts — nothing is pruned automatically.
  Clicking a session in the sidebar that is in no tab splits the focused pane **vertically**
  (side by side); an empty active tab is filled with the workspace's newest live session not
  in any tab (keeps the old "opens the newest session" behaviour, also for `mk` spawns).
  Clicking a workspace row shows it; clicking the shown one expands/collapses it. Deferred:
  close tab / split / new pane actions (T2.6/T2.7), tab status labels + pane headers (§11),
  focus ring on tabs, a primary action in the empty-tab state, detaching panes while the
  window is minimized (§10), not stealing focus back on every layout event.
- (T2.6) Pane lifecycle: closing a **live shell** pane kills the shell without asking (§10 only
  asks for a running agent) — change `lifecycle::close_action` if shells running a dev server
  should ask too. A session the UI has not listed yet (just spawned) asks. The close dialog is
  the native `window.prompt` (Kill session / Hide pane / Cancel; dismiss = cancel). New panes
  split the focused pane **vertically** (Alt+Shift+= / Alt+Shift+- split right/down arrive with
  T2.7); "+ Agent" fails with the daemon's "no default agent" error until the workspace has one
  (settings UI = T2.8). Close pane is bound to Ctrl+Shift+X now; "New agent/shell" shortcut
  hints wait for the T2.7 keymap. The >12 running warning is a tab-bar notice, not a toast.
  Deferred: disabled/in-flight state on the new-pane buttons (a double click spawns two),
  focus rings on buttons, a per-pane close icon on hover.
- (T2.7a) Keymap: `keymap.toml` is a flat table `command_name = "gpui-keystroke"` (names in
  `keymap::Command::name`, e.g. `close_pane`, `workspace_3`, `focus_left`); single keystrokes
  only, no unbinding yet; key names are not validated beyond GPUI's parser. A bad entry (unknown
  command, not a string, unparsable, already bound to another command) keeps its default and
  the first problem is shown in the sidebar (all are logged). Loaded once at start (no live
  reload). Split right/down starts a session of the **focused pane's kind** (agent → the
  workspace default agent, shell → shell; nothing focused → shell). Ctrl+Shift+A "pick
  profile" uses the default agent for now (profile picker later). Close tab does **not** ask
  and does not kill: its sessions keep running and stay in the sidebar; closing the last tab
  leaves a fresh empty tab. macOS Cmd mapping not done (Windows first). Only implemented
  commands are bound; palette, quick open, views, inbox, task, Docs, Zed, Files, Git shortcuts
  come with their features; copy, find, Shift+PgUp/PgDn, pane resize/zoom, rename tab = T2.7b.
- (T2.7b) Selection/scrollback: plain drag = character selection only (no double-click word,
  triple-click line, block selection or drag-past-edge auto-scroll; the drag stops at the pane
  edge because GPUI only reports moves over the pane). Copy clears the highlight (like Windows
  Terminal). Selected cells use `text` on `terminal-selection-bg` (program colors dropped for
  readability). Typing/paste scrolls back to the live screen; the selection stays. Wheel = UI
  history; in the alternate screen with alternate-scroll on and no mouse mode it sends Up/Down
  arrows; programs that enable mouse reporting get **no** mouse events yet (selection wins) —
  forwarding mouse/wheel to such programs is backlog. No "scrolled back" indicator yet.
- (T2.7c) Find bar: Enter = next match towards **older** output (up the history, the usual
  direction when the newest output is at the bottom), Shift+Enter = towards newer; both wrap
  around the 10k-line history. Pattern = regex (alacritty `RegexSearch`): case-insensitive
  unless it contains an uppercase letter; an invalid regex shows "✕ Invalid pattern". Each
  query edit jumps to the nearest match up to 1,000 lines above the view's bottom (Enter then
  searches everything). Minimal hand-rolled input: type, Backspace, Ctrl+Shift+V pastes the
  first line; no caret movement, selection or IME, and a query longer than the field is
  clipped at its end. No "n of m" count. Clicking the terminal keeps the bar open (Esc then
  goes to the agent); close with ✕ or Ctrl+Shift+F → Esc. Matches wrapped across 3+ rows
  starting above the view are not highlighted on its top rows. A match scrolled to the top row
  can sit under the bar.
- (T2.7d) Resize moves the innermost divider around the focused pane on the arrow's axis
  (→/↓ = divider right/down, whichever side the pane is on) by 5 % of that split, clamped so
  each side keeps ≥ 10 %; ratios rounded to 4 decimals. Zoom is UI-only and not persisted;
  panes hidden by a zoom are **detached** like other invisible panes (§10), so unzooming
  re-attaches them from the daemon snapshot and their UI-side scrollback restarts. Moving
  focus, splitting or closing the zoomed pane ends the zoom; resize is ignored while zoomed.
  Rename: same minimal input as the find bar (no caret movement/IME); blank or unchanged
  titles are not sent; the title changes when the daemon's `tab.updated` arrives.
- (T2.8) Workspace menu = "⋯" on the active workspace row or right-click any row; it holds
  "Open in Zed" (also Ctrl+Shift+Z, opens the workspace folder) and the default-agent list
  (agent profiles only, from `agent.profiles`). Deferred: keyboard navigation of the menu (it
  takes no focus; Esc does not close it, click outside does), no "None" choice to clear the
  default agent, no menu-hover token in `theme.rs` (uses `border` as hover fill), "⋯" does not
  toggle the menu shut. `zed` is looked up and spawned on the UI thread (fast; a slow network
  drive on PATH would stall a frame). PATH lookup moved to `mkrew_core::lookup` (shared with
  the daemon). `.ps1` zed shims run via `powershell.exe -File`.
- (T2.9) The automated §15 stress test (`crates/mkrew-ui/tests/stress.rs`) stands in for the
  window. "Input latency" there means the `daemon.status` round trip on the UI's own
  connection (behind six attached floods), plus the time to feed one drained batch (≤ 256
  frames, as the window does per frame). Gpui paint and real key dispatch are not covered.
  Floods are ~14 MB per pane in release and ~2 MB in debug (not 50 MB), so the test stays
  quick. Timings and client memory
  are asserted only in release builds (`verify` is debug). "RAM stays flat" means at most
  16 MiB growth after every scrollback is full. The daemon now merges queued PTY chunks
  (≤ 256 KiB) before its per-output work, which halved its CPU per flood byte. §15 "daemon
  CPU < 1 core" is still missed (~1.5 cores, because the floods are unthrottled) → T2.9a.
- (T2.9a) Flood pacing (ADR 0002): a session over 64 KiB of output per 100 ms is "flooding";
  flood batches of all sessions run one at a time and flood work is held to ≤ 2/3 of a core
  (floods ~20 % slower). Sessions under that rate never wait, so typing in a 7th pane during
  six floods should stay instant — not measured automatically (covered by the "§15 stress, in
  the real UI" check above). UI incoming channel: bounded to 16 MiB of output (+ 256 frames).
- (T3.1) "Minimized" = gpui `Window::is_visible()` is false. On Windows that is minimized or
  hidden (`IsIconic`/`!IsWindowVisible`); on macOS gpui may also report a fully covered window
  as hidden, which then detaches too (still §10 "not visible"). Restoring re-attaches from
  daemon snapshots; panes lose UI scrollback and an open find bar, like a tab switch.
- (T3.2) Hook mapping (`mkrew_core::hook`, SPEC §6): Claude `Notification` → needs_input only
  for `permission_prompt`/`elicitation_*`/`agent_needs_input` or a missing `notification_type`
  (assumed older builds); `idle_prompt` (~60 s after `Stop`) is recorded only. `SessionStart`
  only records the native id. Codex `native_session_id` = notify `thread-id` (docs call it the
  session id; the Codex source has a separate session id — check `codex resume <thread-id>`
  works for real). A hook that repeats the current state leaves `source` unchanged (hover
  may say "screen" for a hook-confirmed state). Unknown events of a known agent: stored, ok;
  unknown agent: `invalid_request`. Schema change → migration 0003 + ADR 0003, protocol v2
  (restart an old daemon after updating).
- (T3.4) `mk integration`: config dirs follow the agents' own overrides (`CLAUDE_CONFIG_DIR`,
  `CODEX_HOME`, else `~/.claude`, `~/.codex`). Hooks run bare `mk` as SPEC §6 says, so `mk` must
  be on PATH (`status` warns if not); an absolute path would survive a missing PATH but break on
  moves. Backup = `<file>.<unix ms>.bak`, only when the file changes. `settings.json` is
  re-printed (2-space, LF; key order kept), so uninstall restores content, not bytes. Codex
  allows one `notify`: install refuses to replace a foreign one (status says "not installed").
  Not used: Codex's newer `[hooks]` system (needs the user to trust hooks via `/hooks`, no
  Notification event) — revisit if `notify` is deprecated. SPEC §19 "status reports hook
  version" is not implemented (no version marker in the hooks).
- (T3.5) Notifications: `MKREW_NOTIFY` on the daemon (unset = OS via `notify-rust`, `off` =
  none, anything else = JSON-lines file path; tests use `off` or a file, so `cargo test` shows
  no toasts). Throttle = 30 s per session across needs_input and done together. Each toast
  keeps one daemon thread waiting for its click/dismissal (a Linux server that never closes a
  notification keeps it until the daemon exits). macOS: no click reporting (the daemon has no
  main run loop). A click with no UI connected is dropped (the UI is not launched). New
  `notification.clicked` event → protocol v3 (restart an old daemon after updating).
- (T3.6) Status display: a 500 ms check (`theme::STATUS_TICK`) redraws only while the window
  is visible and something shown changed: every tick while a session works (4-frame spinner),
  for idle only when its time text changes (each second, then each minute). Hover ages are
  computed when the tooltip opens. gpui has no
  reduce-motion query, so the spinner always turns (slow enough to pass; revisit if gpui adds
  one). Time = since the last state change (working → elapsed, idle → idle time), `12s` / `4m` /
  `1h 5m`. Tab label ranks starting/unknown below exited. Pane header = icon, agent profile id,
  title, status text; it takes one row from the terminal. "Run again" / "Resume" on exited
  sessions are not in T3.6 (still backlog; not yet in a task).
- (T3.7) Inbox: "viewed" = the done session is the focused pane of the shown tab while the
  Mkrew window is active (checked on every render; coming back to the window counts). Viewed
  state is in memory only (a restarted UI lists done sessions again) and keyed by the state
  change time, so a later `done` re-lists it. needs_input stays listed while looked at. Sessions
  of archived workspaces are left out. The Inbox sits under the workspace list (scrolls with
  it). Clicking an entry or a notification = same as clicking the sidebar row (focus its pane, or
  split the focused pane to show it). A notification click for an unlisted session only shows its
  workspace. `Ctrl+Shift+I` (jump to next Inbox item, §13) is not in T3.7 (backlog).
- (T3.8) `agents/omp.toml` is unchanged (screen, idle 3000 ms, no patterns): there is no real
  omp recording to refine it from. The fake `omp` scenario (spinner turns + quiet gaps) is only
  modelled on how omp is assumed to look. If real omp has an approval prompt or a quiet "thinking"
  phase > 3 s, add a pattern / raise `idle_after_ms` and record its output as a fixture. Screen-only
  profiles never reach `done` (by design, §6): a finished omp turn shows as `idle`.

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
