# Progress archive

Done tasks moved out of PROGRESS.md to keep it short. Newest first.

## M1 — Daemon & CLI, no UI (tag `m1-done`)

Acceptance criteria at completion (unticked ones are in HUMAN_CHECKS "To verify": T1.10 real
terminal `mk attach`, T1.12 CI run green):

- [x] `mk daemon start` runs detached; `status`/`stop` work; starting twice never yields two daemons.
- [x] `mk project add/list` persist to SQLite.
- [ ] Sessions spawn from TOML profiles; `mk attach` is fully interactive; `Ctrl+]` detaches, agent keeps running.
- [x] Re-attaching restores the screen instantly from the daemon snapshot.
- [x] Binary output frames + backpressure (§7); PTY reader threads + headless emulator (§8).
- [x] Broken profile TOML → clear error with file and line; daemon does not crash.
- [x] `mkrew-fakeagent` + `agents/fake.toml` exist; `mk send` and `mk wait` work with it.
- [x] E2E test: real daemon in a temp dir, fake agent, `mk send` an answer, `mk wait` until done.
- [ ] CI workflow for Windows + Linux.

Tasks:

- [x] T1.1 Create `mkrew-protocol`: frame codec (u32 BE len + kind byte; 0x01 JSON, 0x02 = 16-byte ULID + raw bytes), Request/Response/Event enums for the M1 methods (hello, daemon.*, project.add/list, session.spawn/list/attach/detach/input/send/wait/resize/kill, agent.profiles), error codes incl. `version_mismatch`; round-trip + partial-read unit tests; covers: binary output frames. (spec: §4, §7) — feat(protocol): frame codec and M1 message types
- [x] T1.2 Create `mkrew-core` profiles: AgentProfile TOML parsing (prompt_mode, env, resume, [state]), directory loader returning profiles + per-file errors with file and line/column, sample `agents/{claude,codex,omp,shell,fake}.toml`; insta snapshots of parse errors; covers: broken profile TOML, `agents/fake.toml`. (spec: §6, §16) — feat(core): agent profile parsing and sample profiles
- [x] T1.3 Create `mkrew-store`: rusqlite bundled, WAL, numbered migrations runner + `0001_init.sql`, project add/list (dunce-normalized unique path) and session insert/update/list; tempdir tests; covers: project persist. (spec: §5, §17) — feat(store): SQLite store with migrations, projects and sessions
- [x] T1.4 `mkrew-core` session state machine (pure): starting/working/idle/needs_input/exited/unknown from output activity, `idle_after_ms`, needs_input patterns over emulated screen lines, process exit; hook-event input stubbed for M3; plus send-queue policy (deliver when idle/done/needs_input); unit tests with a fake clock; covers: groundwork for `mk send`/`mk wait`. (spec: §6, §8) — feat(core): session state machine and send queue
- [x] T1.5 Create `mkrew-daemon` skeleton: data/config dirs (`directories`, override via env for tests), single-instance lock file, `interprocess` local socket (named pipe current-user only / Unix socket), hello + protocol_version check, daemon.status/stop, project.add/list via store, startup marks stale sessions exited, tracing to rotating files; integration test: two daemons in one temp dir → second exits; covers: starting twice never yields two daemons. (spec: §3, §8, §17) — feat(daemon): daemon skeleton with lock, local socket and projects
- [x] T1.6 Create `mkrew-cli` (`mk`, clap): IPC client, `mk daemon start` (spawn detached, `CREATE_NO_WINDOW | DETACHED_PROCESS` on Windows, wait for hello) / `stop` / `status`, `mk project add/list`; integration test against a temp-dir daemon; covers: daemon start/status/stop, project add/list. (spec: §8, §9) — feat(cli): mk daemon start/stop/status and project add/list
- [x] T1.7 Create `mkrew-fakeagent`: scenario file (TOML) steps — print (colored), spinner, prompt "Do you want to proceed? (y/n)" and read a line, sleep, write file, exit code; `flood` + `ask` sample scenarios in `crates/mkrew-fakeagent/scenarios/`; point `agents/fake.toml` at it; unit tests on scenario parsing + a PTY-free run test; covers: `mkrew-fakeagent` exists. (spec: §16) — feat(fakeagent): scriptable fake agent with ask and flood scenarios
- [x] T1.8 Daemon sessions: `session.spawn/list/kill` from profiles (command lookup with PATHEXT + cmd-safe `.cmd` quoting, reuse `spike-term/src/launch.rs`), PTY env `MKREW_SESSION_ID`/`MKREW_SOCKET`/`TERM`, one reader OS thread → bounded channel → tokio, headless `alacritty_terminal` (200 lines scrollback, DSR replies written back), state machine wiring + `session.state`/`session.exited` events, session log file (5 MB rotation), shutdown kills PTYs ≤ 5 s; integration test spawning the fake agent; covers: sessions spawn from profiles, PTY reader threads + headless emulator. (spec: §6, §8) — feat(daemon): spawn, list and kill PTY sessions from agent profiles
- [x] T1.9 Daemon attach path: `session.attach` returns grid snapshot (cells + cursor + modes) and subscribes, binary 0x02 output frames batched ≤ 16 ms, per-client bounded queue (4 MB) with drop-and-resnapshot on overflow, `session.input/resize/detach`; tests incl. a slow-client overflow test; covers: binary output frames + backpressure, re-attach snapshot. (spec: §7, §8) — feat(daemon): attach with screen snapshots, batched output frames and backpressure
- [x] T1.9a Dedicated PTY writer OS thread per session (bounded channel; `session.input` awaits room on the connection side, never in the session task) so a big paste while the child's stdout is backed up cannot deadlock the session or block a tokio worker; DSR replies go through it too; test: a multi-MB `session.input` to a child that does not read stdin leaves the daemon and other sessions responsive. (spec: §8) — feat(daemon): dedicated PTY writer thread with bounded input queue
- [x] T1.10 `mk attach <id>` + `mk session list` (+ `mk session spawn` helper or `--agent` flag, record choice): raw mode (crossterm), paint snapshot then stream frames, forward stdin as `session.input`, resize on terminal resize, `Ctrl+]` detaches leaving the agent running; integration test detach → re-attach shows the same screen; covers: `mk attach` interactive, Ctrl+] detach, instant re-attach. (spec: §9, §7) — feat(cli): mk attach, session list and session spawn
- [x] T1.11 `session.send` (queued while working) + `session.wait` (until states, timeout) in the daemon, `mk send` / `mk wait`; E2E test: real daemon in a temp dir, fake agent `ask` scenario, `mk send y`, `mk wait --until idle,done` until finished; covers: `mk send`/`mk wait`, E2E test. (spec: §7, §9, §16) — feat(daemon,cli): session.send queue, session.wait, mk send and mk wait
- [x] T1.12 GitHub Actions CI (`windows-latest` + `ubuntu-latest`: fmt, clippy -D warnings, tests); add manual checks to HUMAN_CHECKS (`mk attach` with a real shell/`claude`, detached daemon has no console window, CI run green); covers: CI workflow. (spec: §17, §18) — ci: GitHub Actions verify on Windows and Linux

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
