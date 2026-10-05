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

- [ ] (T7.4) **Build machine:** a leftover `mkrew-fakeagent.exe` process (probably from a test
      daemon that survived a killed run) locks `target\debug\mkrew-fakeagent.exe`, so
      `scripts/verify.sh` fails to relink it ("Access is denied"). Please end stray
      `mkrew-fakeagent.exe` / `mkrew-daemon.exe` processes. Also, drive C: has only ~7 GB free,
      and a second target dir filled it up during T7.4 (LNK1318).

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

- [ ] (T4.7) **Task dialog.** Restart the daemon and app, open a workspace's Board
  (Ctrl+Shift+1). Empty board → "New task" button; Ctrl+Shift+N opens the dialog from anywhere
  (also from Terminals, switching to the Board). Type a title, Tab to the prompt, type several
  lines past the field height (it should scroll with the caret), paste with Ctrl+V, pick an
  agent chip, Enter in the title or Ctrl+Enter saves → card appears in Todo. Hover a card →
  "Edit" appears; change fields → Save updates the card; `mk task list` shows the change.
  Delete → native confirm → card disappears; delete a task whose session still runs → error
  in the dialog. Esc cancels. Check AltGr characters (e.g. `{ [ ] }` on a German layout) type
  into the fields. Dark + light theme screenshots for the §14 design check.

- [ ] (T4.8) **Board drag & drop.** With 3+ cards in Todo: drag one between two others → the
  accent line shows in the gap where it will land; drop → order changes. Drag to the end of
  Todo, into an empty column (free space under "No tasks") and to Review/Done → the card moves
  and `mk task list` shows the new status. Restart the app → order and columns persist. Check
  the drag preview follows the pointer smoothly, Edit still opens on click, and nothing moves
  when dropping a card back on its own place. Dark + light screenshots for §14.

- [ ] (T4.9) **Card actions.** On the Board: Run a Todo card (button, and click + Ctrl+Enter)
  → it moves to Running with the right agent; click it → the Terminals view shows its pane.
  Back on the Board, click its follow-up field, type a line, Enter → the agent receives it
  (check in its pane); Esc empties the field. A needs-input card takes a follow-up too. When
  it reaches Review, Approve → Done. Check that dragging a card does not also open its
  terminal, that Ctrl+Enter inside a terminal still reaches the agent, and AltGr characters
  type into the follow-up field. Dark + light screenshots for §14.

- [ ] (T4.10) **M4 wrap-up — the unticked M4 criteria.** `mk task` ↔ UI sharing is covered by
  `crates/mkrew-ui/tests/board_shared.rs`; the rest needs eyes or real agents:
  - **Create/edit/delete, drag, order persists** (criterion 1): the T4.7 + T4.8 checks above.
  - **Real agents with `arg` prompts** (criterion 2): give `agents/claude.toml` and
    `agents/codex.toml` `prompt_mode = "arg"` (or a copy), set each as a task's agent, prompt
    `fix "a&b" 100%`, Run from the Board → the session starts in the project folder (check
    `pwd`/the agent's cwd) and the agent receives the prompt verbatim (npm `.cmd` shims on
    Windows: T4.4). Also run a task with no agent → the workspace's default agent is used, and
    one with `prompt_mode = "type"` → the prompt is typed once the agent is ready.
  - **Card opens its terminal / follow-up** (criteria 4, 5): the T4.9 check above, with real
    Claude (follow-up while it works, answer at a permission prompt from the card).
  - **Board drag feel + §14 design check**: drag preview smoothness, drop line, card density,
    dark + light screenshots of a board with cards in every column (incl. an error row).

- [ ] (T5.4) **Undo task dialog.** In a git repo workspace run a task that edits a tracked file,
  adds a new file and `git add`s another; once it is in Review (close its terminal), hover the
  card → "Undo" → the dialog lists the files (M restored / A deleted / ? deleted) and a colored,
  scrollable diff; with the terminal still open the confirm is disabled with "Close the task's
  terminal first."; "Discard changes" restores the folder and closes the dialog. Edit a file
  after opening the dialog, then confirm → refused ("preview again"), the error shows and the
  preview reloads. A task whose run changed nothing → one sentence + "Close". Check a long diff
  (> 2000 lines → "cut short" note) scrolls smoothly, long paths ellipsize, Esc closes, and
  dark + light screenshots for §14 (destructive button legibility, hover).

- [ ] (T5.7) **Conflict badges.** Run two tasks in one workspace whose agents edit the same file
  within 60 s (e.g. with Claude hooks installed, or one working at a time): both cards on the
  Board and both pane headers show "⚠ Conflict: <path>", the tooltip names the file(s); hover
  the badge → "Dismiss" clears it on both cards and panes; without dismissing, approving/moving
  one card to Review clears that card only. Long paths ellipsize; the ⚠ renders as a monochrome
  amber glyph (not a color emoji); hovering Dismiss shows its own tooltip; dark + light
  screenshots §14.

- [ ] (T5.9) **History viewer.** Run a fake/real agent session to exit, `mk daemon stop`, start
  the app: Terminals tab bar → "History" opens the dialog listing the workspace's finished
  sessions newest first (title/agent, "exited (code) · ran … · … ago"); the newest log replays
  in the right pane (colors, last screen, mouse-wheel scrollback, Ctrl+Shift+F find,
  Ctrl+Shift+C copy of a selection); ↑ / ↓ and clicks pick other sessions; Esc / Close returns
  the keyboard to the focused pane. A log > 2 MB shows the "Older output is left out" note.
  Empty workspace → "No finished sessions in this workspace yet." Dark + light screenshots §14.

- [ ] (T5.11) **M5 wrap-up — the unticked M5 criteria.** Watcher ignores and daily backups are
  ticked (automated: `file_watcher_records_changed_files_of_live_sessions_except_ignored_ones`,
  backup unit tests + `startup_marks_stale_sessions_exited`). Restart the daemon first
  (protocol v8), with `mk` on PATH and `mk integration install claude` / `codex` done (the
  native session id comes from hooks).
  - **Real Claude/Codex resume across a daemon restart** (criterion 1): run a Board task with
    `claude` and one with `codex` (`agents/claude.toml` `resume = "claude --resume {session_id}"`,
    `agents/codex.toml` `resume = "codex resume {session_id}"`); after the first turn, check a
    hook arrived (the pane header status hover says "from hook"; `mk session list` does not
    print the native id). `mk daemon stop`
    while both are running/needs_input, start the app → each task gets a new session in the
    same tab slot, and the agent shows the SAME conversation (scroll back: the earlier prompt
    and answer). Send a follow-up → it continues with that context. A task whose agent has no
    `resume` (e.g. `shell`) → needs input + error row. Also check on Windows that the npm
    `.cmd` shim launch passes the resume argv (session id) through unchanged, and note if Codex's `resume` subcommand
    syntax differs in the installed version.
  - **Undo shows the diff and needs confirmation** (criterion 2): the T5.4 check above (backend
    already covered by daemon tests + `mk task undo` without `--yes` only previewing).
  - **UI warns on both cards** (criterion 3): the T5.7 check above, with two real Claude tasks
    (hooks installed) editing one file within 60 s; also note false positives from the
    "single working session" watcher heuristic (T5.6 note below).
  - **History viewable after a restart** (criterion 5): the T5.9 check above, with a real
    Claude session's log (alternate screen, colors, wide glyphs).
  - **§14 design check**: dark + light screenshots of the Undo dialog, a conflict badge on a
    card and a pane header, and the History dialog.
- [ ] (T6.8) **Docs view** (logic covered by `docs.rs` unit tests + `tests/docs_view.rs`
  against the real daemon; rendering never seen): add a workspace with a `PLAN.md` using
  headings, nested lists, `- [ ]`/`- [x]` items, a table with `:-:`/`--:` columns, a fenced
  code block, a quote, a link, inline `code`, **bold**/_italic_/~~strike~~ and a rule.
  Ctrl+Shift+3 (or the "Docs" tab) → files listed (root `*.md`, `docs/**`), folder shown
  under the name, "N open" badge after creating a comment (`mk comment` has no create — use
  T6.9 or raw IPC). Check every block renders legibly in dark + light; Ctrl+Shift+E and the
  Preview/Source toggle switch to numbered source lines (long lines are not wrapped — check
  they stay readable; scroll a 5,000-line file smoothly). Edit the file in Zed → preview
  updates within ~1 s; delete it → "This file was deleted."; "Open in Zed" / Ctrl+Shift+Z
  opens the file (without `zed` installed → the install hint shows under the header). Empty
  workspace → "No Markdown files to review yet." + "Show terminals". Screenshots for §14.
- [ ] (T6.9) **Docs comments** (logic covered by `comments.rs` unit tests + `tests/docs_view.rs`;
  the margin was never seen): in Docs, click a paragraph (highlighted), Shift+click a later
  block → both picked; "Comment" / Ctrl+Alt+M → draft in the margin with "Lines a–b"; type,
  Enter → thread appears, the file's "N open" badge goes up. Same in Source view by clicking
  line numbers/lines. Hover a thread → Reply / Resolve; reply with Enter; Resolve → "✓
  Resolved" (dimmed), Reopen brings it back. Esc drops a draft. Edit the passage away in Zed →
  "⚠ Outdated"; `mk comment reply` from a session → reply shows live with "↩ Addressed".
  On a layout where Ctrl+Alt+M is AltGr+M (e.g. German "µ"), typing in a draft still inserts
  the character. Dark + light screenshots for §14.
- [ ] (T6.10) **Docs "Send to agent" / "View changes"** (logic covered by `doc_dialog.rs` unit
  tests; daemon delivery by the T6.5/T6.6 e2e; the dialogs were never seen): with 2 open
  comments on a plan an agent wrote, the header's "Send to agent" tooltip says "Send 2 open
  comments"; Ctrl+Shift+S opens the picker with that agent preselected ("edited this file"),
  ↑ / ↓ move, Enter sends → dialog closes, threads show "↑ Sent", the prompt is typed into
  the session (queued while it works). Select one thread's lines first → only that one goes.
  No live agent → "No agent is running…" + Close. After the agent edits the file, hover the
  thread → "View changes" shows the diff (−/+ rows, hunk band); unchanged file → "Nothing
  changed…". Dark + light screenshots for §14.
- [ ] (T6.11) **M6 wrap-up — the unticked M6 criteria.** The fake-agent E2E criterion is ticked
  (automated: `fake_agent_addresses_sent_comments_and_replies_to_each`). The backend of every
  other criterion is covered by daemon/CLI e2e (`docs_are_listed_read_and_external_edits_reanchor_comments`,
  `comment_threads_reply_resolve_reopen_and_agent_replies_address_them`,
  `comments_sent_to_a_working_agent_are_typed_once_it_asks`, `comment_list_and_reply_from_inside_a_session`)
  and `crates/mkrew-ui/tests/docs_view.rs`; what is left needs eyes or real agents. Restart the
  daemon first (protocol v11), with `mk` on PATH.
  - **Docs list + preview** (criterion 1): the T6.8 check above; also have an agent create a
    `.md` outside the review globs (e.g. `notes/x.md`) → it is listed (recently changed).
  - **Comment on a selection, threads** (criterion 2): the T6.9 check above; restart the daemon
    and the app → threads and statuses are still there.
  - **External edits in Zed** (criterion 3): edit the commented passage in Zed (save) → preview
    refreshes, a comment above a newly inserted paragraph moves down with its passage, a
    deleted passage → "⚠ Outdated"; undo in Zed → back to open.
  - **Real Claude/Codex review round trip** (criteria 4, 5): Claude writes `PLAN.md` from a
    Board task; add 2 comments; "Send to agent" while Claude is still working → the prompt is
    typed only once it stops, arrives as ONE pasted message with its line breaks (T6.5/T6.6
    notes below: bracketed paste through ConPTY), Claude edits the plan and runs
    `mk comment reply c_… "…"` per comment → replies appear live, threads show "↩ Addressed".
    Repeat with Codex. If the agent does not know `mk`, note what the §12 prompt is missing.
  - **View changes** (criterion 6): the T6.10 check above, on the threads Claude just addressed.
  - **§14 design check**: dark + light screenshots of the Docs view (preview with margin
    threads, source view), the Send-to-agent picker and the View-changes dialog.

## Questions with assumed defaults

_Decisions the agent made on its own. Override them here if you disagree._

- (T4.1, ADR 0004) Tasks without an agent store `agent_profile = ''` = workspace default at run
  time; deleting a task is refused while one of its sessions is live, exited sessions are kept
  as free terminals (`task_id` = NULL); new `task.deleted` event. `task.move` accepts any status
  (the user can drag cards anywhere, SPEC §5).

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
  shim and run `node <script.js>` directly (**resolved in T4.4**: npm shims are unwrapped, the
  refusal stays for other batch files). Other T1.8 defaults: the daemon seeds the repo's
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
- (T4.2) Task status rules (`mkrew_core::task`): session events move a card only while it is
  `running`/`needs_input`. A card the user dragged to todo/review/done (or one already in review)
  stays put on later events, so a follow-up typed into a review card's terminal does not move it
  back to running. A session start (Run) always sets running and clears `error_flag`. An exit
  with no code (killed) counts as non-zero. Only review cards can be approved. Session `idle`
  does not move a card: screen-only agents such as omp never reach `done`, so their cards stay
  running until the session exits or the user drags them.
- (T4.3) `task.run`: refused (`invalid_state`) while the task has a non-exited session (checked
  again under the spawn lock); an empty prompt sends nothing in every mode; the session title is
  the task title. `type` writes the prompt + Enter through the send queue at the first
  idle/needs_input/done, unchanged, so a multi-line prompt presses Enter per line (like
  `session.send`; bracketed paste = backlog). A card moved by the run lifecycle keeps its
  `position` (it may land mid-column). Sessions ended by daemon shutdown do not move their card
  (it stays running/needs_input for the §8 startup resume instead of getting `error_flag`).
  `arg` prompts with cmd metacharacters for `.cmd` shims: see T4.4.
- (T4.4) `.cmd` prompt args: chose **npm-shim unwrapping** over vendoring portable-pty's
  `cmdline()` (no fork; SPEC only asks to honor PATHEXT, so no ADR). A `.cmd` whose `%*` line is
  exactly `<launcher> "%dp0%\<target>" %*` (or `%~dp0`; launcher = nothing, `&`, `"%_prog%"`,
  `node`, `…\node.exe"`) runs its target directly: `.exe` as is, `.js/.cjs/.mjs` via the shim
  dir's `node.exe`, else `node.exe` on PATH (a `node.cmd`/`.ps1` wrapper is refused). The
  shim's `SET`s are skipped (e.g. pnpm's `NODE_PATH`) — fine for claude/codex. Shebang flags,
  extra args, missing targets, other batch files → old rule (metachar args refused). Ctrl+C no
  longer shows cmd's "Terminate batch job". Check: real `claude.cmd`/`codex.cmd` task with an
  `arg` prompt like `fix "a&b" 100%` (listed with the T4.10 manual checks).
- (T4.6) Each workspace opens on **Terminals**; the Board / Terminals choice is kept per
  workspace for the app run (not persisted). On the Board, tab/pane-creating shortcuts (new
  tab, next/previous/nth tab, split) switch to Terminals first; shortcuts that act on hidden
  panes (rename/close tab, focus/resize/zoom/close pane) do nothing. A card's session = the
  task's newest session; run time = its start to its exit, or to its `done` state change while
  the agent waits at its prompt, else now. "Last run failed" shows only when the card's status
  is not already `exited (code)`. The Board/Terminals switcher has no keyboard focus or focus
  ring yet (deferred with the tab/button focus rings above). Ctrl+Shift+1 / 2 (§13) are
  bound as `ctrl-shift-1/2`; whether Windows reports shifted digits that way is part of the
  T4.10 manual board check. The empty board names `mk task add` until T4.7 adds the New task
  action.
- (T4.7) Task dialog: modal; clicking the shade does **not** cancel (a stray click never throws
  a draft away; Esc / Cancel do), and shortcuts are ignored while it is open. Enter saves from
  the title and adds a new line in the prompt; Ctrl/Cmd+Enter saves anywhere; Tab toggles
  title ↔ prompt. Title and prompt are trimmed; a blank title is refused in the dialog. Edit
  sends only changed fields, diffed against the board's current task. Edit and delete live in
  the dialog, opened from a hover-revealed "Edit" on the card (no keyboard path until T4.9
  adds a selected card; agent chips are mouse-only for now). Text fields only append/backspace
  at the end (no caret movement, selection or IME composition yet — backlog). A daemon
  disconnect drops an open draft (the whole UI resets). Delete confirm text says past sessions
  stay in the sidebar (ADR 0004 detaches them).
- (T4.8) Drag & drop: a card lands **before** the card under the pointer (the 2 px accent line
  sits in the gap above it), or at the end when dropped in the free space under the last card.
  Dropping where the card already is sends nothing. Any status can be dragged to any column
  (the user may override §5 transitions; a later session event still moves it per §5). No
  motion yet: the indicator and the drop snap without the §14 120–180 ms ease-out, and the
  original card stays visible (not dimmed) while its copy is dragged — polish backlog. No
  keyboard way to move a card (§13 lists none; backlog with the command palette). Two drops
  in quick succession during a column renumber may compute from stale positions (rare;
  `task.list` on reconnect fixes the order). A failed move shows a notice on that workspace's
  board until the next drop.
- (T4.9) Card actions: clicking a card selects it (accent border); a non-Todo card with a
  session also opens its terminal (Todo cards only select — a card dragged back to Todo is
  about to re-run). Ctrl+Enter (`run_task`) runs the selected Todo card; it is bound in a
  `Board` key context set on the window root only while a board is shown, so terminals still
  get Ctrl+Enter. Run keeps the user on the board (the card follows its session; click to
  watch). Approve = move to the end of Done. Follow-up is a single line (pasted line breaks
  become spaces), Enter sends text + Enter via `session.send`, Esc drops it; while it has the
  keyboard every shortcut goes to the field (like the task dialog). A typed draft survives
  losing focus; an empty one disappears; a draft whose session was replaced (re-run) or
  ended is dropped. Run shows "Starting…" (disabled) until `task.run` replies. Failed
  Run/Approve show the board notice. Run is quiet (secondary) on every card and accent only on
  the selected one, whose tooltip shows Ctrl+Enter. Gaps (backlog): cards can only be
  selected with the mouse (no arrow-key selection / focus ring yet, so Ctrl+Enter needs a
  click first); Approve has no shortcut/`Command`; a double-click on Approve sends two
  (harmless) moves; pressing in the follow-up field and moving the mouse may start a card
  drag (stopping mouse-down propagation there may also swallow the field's own click —
  untested without a display).
- (T5.1) Startup resume: Task has no note column, so "session stopped — run again?" is
  `needs_input` + `error_flag` (the card's error row) plus a daemon log line, not a stored
  note. "Last session" = the task's newest session by `started_at`; its `agent_profile` (not
  the task's current one) picks the `resume` template. The resume template is split on
  whitespace with `"…"`/`'…'` grouping and no escapes (SPEC §6), then `{session_id}` is
  substituted per word. The resumed session keeps the old title/size, carries the native id
  over (a second restart before the next hook still resumes), gets no prompt, and replaces
  the old session id in every tab layout of the project. A resume spawn failure (e.g.
  command not found) falls back to the error flag.
- (T5.2) Checkpoint: every `task.run` overwrites `task.checkpoint` with HEAD (`git rev-parse
  --verify HEAD^{commit}` in the project folder, so a subfolder of a repo uses that repo's
  HEAD); outside a repo, before the first commit, or when git cannot run, it is cleared and the
  run still starts (warning in the daemon log). Resume after a restart is not a new run and
  keeps the old checkpoint. HEAD does not capture uncommitted changes made before the run —
  T5.3's undo must account for that (preview shows everything that differs from HEAD).
- (T5.3, ADR 0005) Undo: tracked files go back to the checkpoint commit, so uncommitted edits
  the user had BEFORE the run are discarded too (they appear in the preview diff; there is no
  record of them). "Created since" untracked files = untracked, non-ignored files with mtime ≥
  `task.checkpoint_at` (taken just before HEAD is read in `task.run`); a pre-existing untracked
  file the run edited is deleted, an untouched one is kept, ignored files never. HEAD/commits
  are left alone (agent commits stay, their changes are reverted in the working tree). Empty
  directories left by deleted files stay. Files tracked since the checkpoint (e.g. an agent's
  `git add -A` of the user's old untracked notes) are deleted by the restore; the preview
  lists them under `added` ("delete"). Undo only deletes created files the client confirmed
  (previewed `created` list) and refuses when the tracked changes differ from the previewed
  `fingerprint` (std `DefaultHasher`, fixed keys: stable per daemon build; a daemon upgrade
  between preview and confirm only refuses). Review SHOULDs left open: `added` files are
  deleted whatever their age (could unstage old ones instead), `git diff` output is read
  whole before the 1 MiB cut, nested repos show as `sub/` in `created` but `git clean -f`
  keeps them. The preview is allowed while the task runs; undo is not — but
  only the task's OWN sessions block it: another agent live in the same folder gets its edits
  reverted, and a `task.run` racing an in-flight undo is not locked out. `mk task undo` without `--yes` prints the preview and exits 0. Tests pin
  `core.autocrlf=false`; with a global `autocrlf=true` a restore rewrites LF files with CRLF
  (normal git behaviour).
- (T5.4) Undo UI: "Undo" is a hover-revealed secondary button on Review/Done cards with a
  checkpoint (no keyboard path yet — like card Edit; a "Undo selected task" command could act on
  the selected card). The destructive confirm uses the existing `diff-del-bg`/`diff-del-text`
  tokens (no danger token in §14; status colors stay for agent states). The UI disables confirm
  while the preview is empty or any session of the task is live (same rule as the daemon). The
  diff renders at most 2000 lines as plain divs (rows parsed once per preview); a virtualized
  `uniform_list` and per-row backgrounds wider than the box on horizontal scroll are left for
  the M7 Git view. An undo that fails after Esc closed the dialog is only logged (no toasts yet).
  Open ui-review SHOULDs (left for a later polish task): no keyboard path to Undo / Tab to
  "Discard changes" (§14.7); buttons have no focus ring; hover-only Undo still reserves a row on
  Done cards; the diff builds up to 2000 row divs per render (`uniform_list` later). "Discard
  changes" stays enabled when the diff is cut short (the dialog points at `mk task undo`).
- (T5.5) File watcher: one recursive `notify` watcher per project while it has a live session,
  on the project folder (the session cwd). Only the ROOT `.gitignore` is read, once when the
  watch starts (nested `.gitignore`, `.git/info/exclude`, the global excludes file and edits to
  `.gitignore` while sessions run are not applied); matching is case-sensitive even on Windows.
  `.git`, `node_modules`, `target` are always ignored at any depth. Batches = 300 ms quiet, at
  most 2 s; existing directories are dropped, removed paths are reported (a removed directory
  looks like a file). Each batch becomes a `files_changed` session event (`{"paths": [...]}`,
  `/`-separated, relative) on EVERY live session of the project (attribution is T5.6). Review
  SHOULD left open: on Linux/macOS a recursive inotify/kqueue watch walks the whole tree
  (including `node_modules`/`target`) synchronously in `Hub::spawn` and can hit
  `fs.inotify.max_user_watches` on big repos (Windows ReadDirectoryChangesW is cheap); Windows
  drops events silently on buffer overflow (no rescan). Unwatching after the last session
  exits is not asserted by a test.
- (T5.6) Conflict attribution heuristic — who "edited" a file: (1) a watcher batch is blamed on
  the project's session in `working` when the batch lands, but ONLY when exactly one session is
  working; with two or more working at once the editor is unknown and the batch is not
  attributed (default chosen to avoid flagging every file while two agents run together; the
  alternative "blame every working session" would catch more real conflicts but warn on every
  concurrent edit — your call). Sessions in idle/needs_input/done are never blamed (an agent
  waiting for input does not write). Your own edits (e.g. in Zed) while exactly one agent is
  working are blamed on that agent. (2) Hook payloads with `tool_input.file_path` /
  `tool_input.notebook_path` (Claude `PreToolUse`/`PostToolUse` shape) are attributed to the
  reporting session exactly, whatever is working (a `tool_name` other than Edit/Write/MultiEdit/
  NotebookEdit is skipped; `PreToolUse` of a denied edit still counts); Codex `notify` payloads
  name no files. `hook_edits` does a store read + `metadata()` inside the actor task (cheap, once
  per hook).
  `mk integration install claude` does NOT install `PostToolUse` yet (it would run `mk hook`
  after every tool call) — decide whether to add it with an `Edit|Write|MultiEdit|NotebookEdit`
  matcher. Hook paths go through the watcher filter (ignored dirs, root `.gitignore`); absolute
  paths must start with the project path as stored (case-sensitive prefix, so `c:\` vs `C:\`
  misses). `conflict.detected` fires when a file gets a second (or further) editor within 60 s;
  repeat edits by the same editors stay quiet; trackers are dropped when a project's last
  session exits. The event is not persisted: a UI that connects later does not see it.
- (T5.7) Conflict warnings in the UI: each card / pane stops warning on its own — a task when it
  leaves Running/Needs input (re-running it later does not bring the warning back), a session
  when it exits or its task stops warning; the other party keeps its warning until it leaves
  too. "Dismiss" (hover-revealed, on the badge) clears that conflict on every card and pane, not
  only the clicked one. Several files → "Conflict: <first> +N", tooltip lists all. Warnings live
  in UI memory only (lost on UI restart, like the event itself). Badge color = the
  `status-needs-input` amber (no dedicated warning token in §14; add a `warning` token if you
  want status colors reserved for agent states). Dismiss is mouse-only for now (no focus ring /
  command), like the card's Edit and Undo — backlog with keyboard card selection. A session
  whose task is missing from a loaded board stops warning (covers deleted tasks; a task whose
  first `task.updated` has not arrived yet would lose its pane warning — unlikely race, accepted).
- (T5.8) `session.history` returns `files[]` (rotated `.log.1` then `.log`, existing only)
  instead of SPEC's single "path", so the rotated part is not lost; SPEC §7 updated (a
  refinement, no ADR). Clients read the files directly (daemon and clients share a machine).
  For a live session it is best effort (unflushed buffer, rotation race). `mk session history`
  prints raw PTY bytes including ANSI escapes (no stripping).
- (T5.9) History viewer = a modal dialog opened by a "History" button in the Terminals tab bar
  (the task allowed sidebar or Terminals menu; a dialog needs no layout slot and never touches
  `terminal_tab`). No keyboard shortcut (SPEC §13 has none; add one there if wanted). The log is
  replayed into a fresh UI emulator at the session's recorded cols/rows (no resize, no input),
  newest 2 MiB only, starting after the first line break past the cut; a session that ended on
  the alternate screen (full-screen TUI) shows that last screen with little scrollback. The list
  is read once on open (sessions exiting meanwhile appear on the next open). Open review
  SHOULDs (backlog): parse the tail off the UI thread (a 2 MiB log may stall one frame), a
  Retry button on list/log failures, focusable rows with a focus ring (↑ / ↓ work), a primary
  action in the empty state, a clean exit's ✕ is muted (only non-zero codes use the exited color).
- (T5.10) Daily backups are named by the **UTC** date (no time-zone crate; SPEC does not say
  which), so the "day" flips at local UTC midnight. The daemon checks at startup and then every
  hour (not every 24 h: monotonic timers pause during suspend) and writes `VACUUM INTO`
  `mkrew-<date>.db.partial` → rename, then keeps the newest 7 `mkrew-YYYY-MM-DD.db` (other files
  untouched). Only while the daemon runs: a day it never ran has no backup. The copy runs under
  the store mutex (requests wait while it runs; fine at today's DB size — review SHOULD: lock
  only around `backup_to` if the DB grows). No restore command (copy the file over `mkrew.db`
  with the daemon stopped).
- (T6.3) Review prompt (`mkrew_core::review::prompt`): besides the §12 template, a resent
  thread carries its replies as `  <author> replied: <body>` lines under the comment (later
  reply lines indented by four), so the agent sees the follow-up; quotes are not escaped
  (a `"` inside stays as is); a multi-line quote is collapsed to one line (whitespace runs →
  one space), a body keeps its lines (indented by two), no trailing newline. Re-anchoring
  ignores whitespace differences, picks the match whose first line is nearest the old start
  (tie → earlier) and does not strip Markdown markup — the UI (T6.9) must store the quote as
  source text, not rendered text. A comment without a quote keeps its lines while they exist.
  A quote may match inside a word (plain substring). A comment without lines is labelled
  `whole file` in the prompt (unreachable today: `comment.create` requires lines).
  Review globs: `*`/`?` never cross `/` (`*.md` = root files only), `**` spans directories;
  matching is case-sensitive (`PLAN.MD` does not match `*.md`, also on Windows).
- (T6.4) **How md files are watched with no live session:** `doc.list` or `doc.read` on a
  project starts (or marks) its recursive file watcher as "docs", and it then stays until the
  daemon stops — even after the last session exits (one watcher per project ever opened in
  the Docs view; no unwatch on client disconnect / archive). `doc.changed` fires for any `.md`
  file (any case) the watcher reports (`.md` files are reported even when `.gitignore`d, so
  `files_changed`/conflicts now see ignored Markdown too) or `doc.read` returns, whose content
  hash differs from the last one this daemon saw — so the first sight after a daemon start
  fires (and catches up edits made while it was down); a deleted file sends `hash: null`.
  Files > 2 MiB or not UTF-8 are skipped; they are read on the watcher's tokio task (review
  SHOULD: move to `spawn_blocking` if large docs ever stall it). Re-anchor
  rules: a passage that is gone → `outdated` (lines kept); an `outdated` comment whose passage
  is back → `open` (its earlier status is not remembered); `resolved` comments only follow
  their passage, never change status. `doc.list`: walks the project for glob matches
  **including** git-ignored files (plans often live in ignored folders) but never enters `.git`,
  `node_modules`, `target`; "recently changed" = `files_changed` events of the last **24 h**;
  files with unresolved comments are listed too; `open_comments` counts every unresolved
  thread root (open/sent/addressed/outdated). `last_session_id` = the newest event naming the
  file — a batch is recorded on every live session of the project, so ties go to the most
  recently started session (not necessarily the editor). Hash = std `DefaultHasher` (stable
  per daemon build only).
- (T6.5) **How `comment.send_to_agent` types the prompt:** the §12 prompt + Enter is queued
  like `session.send` (written once the session is idle/done/needs_input/unknown). It has
  newlines, so when the agent enabled bracketed paste (mode 2004, as Claude Code/Codex do) the
  daemon wraps it in `ESC[200~ … ESC[201~` then sends `\r` — this applies to every multi-line
  `session.send` now. Without mode 2004 the text is typed as is (each `\n` may submit early).
  To verify with real agents: does ConPTY pass mode 2004 through so the wrap happens, and does
  Claude/Codex accept the pasted prompt as one message? Other choices: any thread status can be
  sent (resolved/outdated → `sent`; the UI preselects open ones); all ids must be on one file
  of the session's project; comments are marked `sent` (snapshot = file content at send time)
  when queued, not when typed; reply ids stand for their thread; the reply is the sent roots.
- (T6.6) **Observed: without mode 2004, ConPTY drops the `\n`s of a typed multi-line text.**
  The fake agent (cooked console, no bracketed paste) receives the whole review prompt as ONE
  line (lines glued, only the final `\r` submits). The fake parses that form; the e2e
  `fake_agent_addresses_sent_comments_and_replies_to_each` passes. For real agents that enable
  mode 2004 this should not matter, but check during the T6.5 real-agent round trip that
  Claude/Codex see the prompt with its line breaks. If not, a fix task could type `\r\n` or
  Shift+Enter-style newlines instead.
- (T6.8) **Docs view choices:** a workspace opens Docs on its first listed file (sorted by
  path); the list is re-read when the view is shown, on `doc.changed` and on `comment.updated`
  — only for the workspace on screen (hidden ones catch up when shown again). The empty
  state's primary action is "Show terminals" (agents write the docs). Ctrl+Shift+E is bound
  only while Docs are shown (`Docs` key context), so terminals still get it; Ctrl+Shift+Z in
  Docs opens the shown file (no line yet; T6.9 may pass the selected line). Code spans keep
  the UI font on a raised background (gpui highlights cannot switch fonts); code blocks use
  the terminal font. The whole preview is laid out each frame (no virtualization; the source
  view is a `uniform_list`) — fine for plans, revisit if 2 MiB docs feel slow. Links are
  styled, not clickable yet. Open review SHOULDs: no keyboard navigation / focus ring in the
  file list; source lines neither wrap nor scroll sideways (long lines are cut); links look
  interactive but do nothing; the selected row / active Preview-Source segment use
  `surface_raised` (same as hover) instead of an accent tint; `docs` entries of removed
  workspaces are never pruned.
- (T6.9) **Comment selection = lines, not characters:** GPUI text has no built-in selection,
  so "select text" picks whole lines: a click on a top-level preview block (a whole list or
  table counts as one block) or on a source line; Shift+click extends to cover both. The quote
  is the selected source lines, trimmed (Markdown syntax included — the daemon re-anchors on
  the raw file). Use Source view for finer anchors. Threads are listed in the right margin in
  anchor order (not pixel-aligned next to their anchor); clicking a thread's "Lines a–b"
  selects those lines. Enter saves a draft (no multi-line typing; pasted line breaks are
  kept). Resolve is offered on every unresolved thread (incl. outdated), Reopen only on
  resolved ones; resolved threads hide Reply. Selection highlight reuses the
  `terminal_selection_bg` token. See ADR 0006 (SPEC §12 amended). The quote is taken when
  the draft opens; the margin stays while the doc is re-read after an edit. Open review
  items: Reply/Resolve are hover-revealed (also shown on a picked thread / while replying)
  and not keyboard-reachable; nothing clears a selection (no Esc / empty-space click); a
  click on a (future clickable) link also picks its block; agent replies show the raw
  profile id as author; the 300 px margin never collapses on narrow windows; A→B→A file
  switches can let a stale `comment.list` answer drop an update in between (narrow window);
  a double click on Resolve sends it twice (harmless).
- (T6.10) **"Send to agent" picks:** "the selected comments" = open threads whose anchor
  overlaps the current line selection (there is no per-thread selection); if none overlap,
  all open threads of the file go. Only `open` roots are sent (sent / addressed / outdated /
  resolved are not; a user reply on an addressed thread reopens it, so it can be resent).
  The picker lists the workspace's live agent sessions (shells excluded) newest first; the
  default is the file's `last_session_id` from `doc.list` when it is still live, else the
  newest. "View changes" is a per-thread action (shown when the root has a `sent_snapshot`)
  diffing that snapshot against the doc as currently shown (line endings ignored, 3 lines
  of context, capped at the undo dialog's 2000 lines); the dialog does not refresh if the
  file changes while it is open. Open review items: the error row is copy-pasted per dialog
  and uses `status_exited`; send errors show the whole error chain; titles ellipsize long
  paths at the end; quiet buttons inside thread cards have no visible hover; "Send to agent"
  looks enabled while disconnected (does nothing); a dialog left open survives a workspace
  switch via a notification click (as the other dialogs do).
- (T7.1) **Git base setting:** `git.set_base` trims the ref, treats blank like `null`
  (auto-detect) and refuses refs starting with `-` or containing whitespace; it does NOT
  check that the ref exists (it may only appear after a fetch — T7.3's status reports a
  missing base). Returns `null` and broadcasts `project.updated` (Project gained `base_ref`).
  CLI: `mk project update <p> --base <ref>` or `--auto-base` (one is required); `--dev` from
  SPEC §9 is still not implemented (no task asks for it). Protocol shapes chosen for
  T7.3–T7.6: `GitStatus` has separate `staged/unstaged/untracked/against_base` lists
  (conflicts are `unstaged` with kind `conflicted`), `base_auto`, `merge_base` (base mode
  only); `git.diff` has `staged` (working mode) and `commit?`; stage/unstage send back the
  whole `Hunk` as shown; `git.log` limit defaults to 50; `fs.find` limit defaults to 50.
- (T7.2) **Git parsers:** `DiffLine` gained `crlf` (SPEC §7 row updated, no version bump —
  no client uses `git.diff` yet) so a hunk patch reproduces CRLF lines byte-exactly instead
  of leaking `\r` into `text`. `parse_diff` assumes `a/`/`b/` prefixes (daemon must pass
  `--src-prefix=a/ --dst-prefix=b/`), is lenient (combined `diff --cc` hunks are skipped),
  and a deleted file's `path` is its old path. `hunk_patch` names only the new path (no
  rename headers) and never writes `new file mode`/`deleted file mode`: per-hunk staging of an
  untracked file fails, of a deleted file leaves it empty in the index, and unstaging a new
  file's hunk empties it — T7.4 stages/unstages those by path instead. Status `R`/`C` on
  either side carries `old_path`; ignored entries dropped.
- (T7.3) **git.status / git.diff:** ADR 0007 — a refresh runs status ∥ base lookup ∥
  staged/unstaged numstat (not ONE process; SPEC §12B updated). Base is resolved in both
  modes (header); `merge_base` only in base mode. Base mode also lists untracked files
  (kind `untracked`, no counts) and clears the staged/unstaged/untracked lists. Untracked files
  never have +/− counts. Not a repo / no merge-base → `invalid_state`; git failure →
  `internal`. `git.diff` of an untracked file = `git diff --no-index /dev/null <path>`; a
  per-file diff does not detect renames (pathspec is the one path). Diff cache key = target +
  HEAD + file mtime/size + index mtime (staging invalidates it), 256 entries then cleared;
  commit diffs = `git show --diff-merges=first-parent` (needs git ≥ 2.31). Status uses
  `--untracked-files=all` and `GIT_OPTIONAL_LOCKS=0`.
- (T7.4) **git.stage/unstage/discard/commit + git.changed:** stage = `git add -A`, unstage =
  `git reset -q` (also on an unborn branch), paths on stdin (`--pathspec-from-file`, literal;
  needs git ≥ 2.26); a hunk = `git apply --cached [-R]` with `--directory=<repo prefix>` (the
  client must stage new/deleted/untracked files by path, see T7.2). **Discard** = lose the
  *unstaged* changes only: tracked files go back to their index version, untracked ones are
  deleted (`git clean`, ignored files never); staged changes are kept. **Commit** = the whole
  index (also changes staged outside a subfolder project), hooks run, no amend. A stale hunk or
  "nothing to commit" comes back as `internal` with git's message (not `invalid_state`).
  `git.changed`: every watcher batch (working files or root `.git/{HEAD,index,packed-refs,refs/…}`)
  + right after each successful daemon mutation, so two events may come < 300 ms apart.
  `git.status` keeps the project watched until the daemon stops (like docs). Outside changes to a
  `.git` dir outside the project folder (repo subfolder, worktree) only show via working-file
  changes. Open review items: discard runs one `ls-files` per path; paths are `trim()`med
  (`docs::relative_path`).

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
