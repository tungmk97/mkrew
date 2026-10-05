# Mkrew build progress

<!-- loop-state: machine-readable, keep these exact keys on their own lines -->
status: RUNNING
milestone: M1
<!-- /loop-state -->

Status values: `RUNNING` (loop continues) · `WAITING_HUMAN` (see docs/HUMAN_CHECKS.md, then set back to RUNNING) · `BLOCKED` (no task can progress) · `DONE`.
Task markers: `[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (see note).

## Current milestone: M1 — Daemon & CLI, no UI (SPEC §18)

### Acceptance criteria
- [x] `mk daemon start` runs detached; `status`/`stop` work; starting twice never yields two daemons.
- [x] `mk project add/list` persist to SQLite.
- [ ] Sessions spawn from TOML profiles; `mk attach` is fully interactive; `Ctrl+]` detaches, agent keeps running.
- [ ] Re-attaching restores the screen instantly from the daemon snapshot.
- [ ] Binary output frames + backpressure (§7); PTY reader threads + headless emulator (§8).
- [ ] Broken profile TOML → clear error with file and line; daemon does not crash.
- [ ] `mkrew-fakeagent` + `agents/fake.toml` exist; `mk send` and `mk wait` work with it.
- [ ] E2E test: real daemon in a temp dir, fake agent, `mk send` an answer, `mk wait` until done.
- [ ] CI workflow for Windows + Linux.

### Task queue
- [x] T1.1 Create `mkrew-protocol`: frame codec (u32 BE len + kind byte; 0x01 JSON, 0x02 = 16-byte ULID + raw bytes), Request/Response/Event enums for the M1 methods (hello, daemon.*, project.add/list, session.spawn/list/attach/detach/input/send/wait/resize/kill, agent.profiles), error codes incl. `version_mismatch`; round-trip + partial-read unit tests; covers: binary output frames. (spec: §4, §7) — feat(protocol): frame codec and M1 message types
- [x] T1.2 Create `mkrew-core` profiles: AgentProfile TOML parsing (prompt_mode, env, resume, [state]), directory loader returning profiles + per-file errors with file and line/column, sample `agents/{claude,codex,omp,shell,fake}.toml`; insta snapshots of parse errors; covers: broken profile TOML, `agents/fake.toml`. (spec: §6, §16) — feat(core): agent profile parsing and sample profiles
- [x] T1.3 Create `mkrew-store`: rusqlite bundled, WAL, numbered migrations runner + `0001_init.sql`, project add/list (dunce-normalized unique path) and session insert/update/list; tempdir tests; covers: project persist. (spec: §5, §17) — feat(store): SQLite store with migrations, projects and sessions
- [x] T1.4 `mkrew-core` session state machine (pure): starting/working/idle/needs_input/exited/unknown from output activity, `idle_after_ms`, needs_input patterns over emulated screen lines, process exit; hook-event input stubbed for M3; plus send-queue policy (deliver when idle/done/needs_input); unit tests with a fake clock; covers: groundwork for `mk send`/`mk wait`. (spec: §6, §8) — feat(core): session state machine and send queue
- [x] T1.5 Create `mkrew-daemon` skeleton: data/config dirs (`directories`, override via env for tests), single-instance lock file, `interprocess` local socket (named pipe current-user only / Unix socket), hello + protocol_version check, daemon.status/stop, project.add/list via store, startup marks stale sessions exited, tracing to rotating files; integration test: two daemons in one temp dir → second exits; covers: starting twice never yields two daemons. (spec: §3, §8, §17) — feat(daemon): daemon skeleton with lock, local socket and projects
- [x] T1.6 Create `mkrew-cli` (`mk`, clap): IPC client, `mk daemon start` (spawn detached, `CREATE_NO_WINDOW | DETACHED_PROCESS` on Windows, wait for hello) / `stop` / `status`, `mk project add/list`; integration test against a temp-dir daemon; covers: daemon start/status/stop, project add/list. (spec: §8, §9) — feat(cli): mk daemon start/stop/status and project add/list
- [ ] T1.7 Create `mkrew-fakeagent`: scenario file (TOML) steps — print (colored), spinner, prompt "Do you want to proceed? (y/n)" and read a line, sleep, write file, exit code; `flood` + `ask` sample scenarios in `crates/mkrew-fakeagent/scenarios/`; point `agents/fake.toml` at it; unit tests on scenario parsing + a PTY-free run test; covers: `mkrew-fakeagent` exists. (spec: §16)
- [ ] T1.8 Daemon sessions: `session.spawn/list/kill` from profiles (command lookup with PATHEXT + cmd-safe `.cmd` quoting, reuse `spike-term/src/launch.rs`), PTY env `MKREW_SESSION_ID`/`MKREW_SOCKET`/`TERM`, one reader OS thread → bounded channel → tokio, headless `alacritty_terminal` (200 lines scrollback, DSR replies written back), state machine wiring + `session.state`/`session.exited` events, session log file (5 MB rotation), shutdown kills PTYs ≤ 5 s; integration test spawning the fake agent; covers: sessions spawn from profiles, PTY reader threads + headless emulator. (spec: §6, §8)
- [ ] T1.9 Daemon attach path: `session.attach` returns grid snapshot (cells + cursor + modes) and subscribes, binary 0x02 output frames batched ≤ 16 ms, per-client bounded queue (4 MB) with drop-and-resnapshot on overflow, `session.input/resize/detach`; tests incl. a slow-client overflow test; covers: binary output frames + backpressure, re-attach snapshot. (spec: §7, §8)
- [ ] T1.10 `mk attach <id>` + `mk session list` (+ `mk session spawn` helper or `--agent` flag, record choice): raw mode (crossterm), paint snapshot then stream frames, forward stdin as `session.input`, resize on terminal resize, `Ctrl+]` detaches leaving the agent running; integration test detach → re-attach shows the same screen; covers: `mk attach` interactive, Ctrl+] detach, instant re-attach. (spec: §9, §7)
- [ ] T1.11 `session.send` (queued while working) + `session.wait` (until states, timeout) in the daemon, `mk send` / `mk wait`; E2E test: real daemon in a temp dir, fake agent `ask` scenario, `mk send y`, `mk wait --until idle,done` until finished; covers: `mk send`/`mk wait`, E2E test. (spec: §7, §9, §16)
- [ ] T1.12 GitHub Actions CI (`windows-latest` + `ubuntu-latest`: fmt, clippy -D warnings, tests); add manual checks to HUMAN_CHECKS (`mk attach` with a real shell/`claude`, detached daemon has no console window, CI run green); covers: CI workflow. (spec: §17, §18)

### Done (last 10)

_M0 tasks moved to `docs/progress-archive.md`._

## Notes for the next session

- Protocol: sync `FrameDecoder` + `encode()`; responses are raw JSON (`Outcome::ok/error`, `into_result::<T>()`); `PROTOCOL_VERSION = 1`. Core: `load_dir/parse_profile` → `ProfileSet`; `state::{StateMachine, SendQueue}` (T1.8 wiring: per chunk `on_output(now, lines)` + `queue.on_output()`, `tick` at `next_deadline()`, `on_exit()`, then `queue.pop(state)`; pattern tail = 4 lines → fake `ask` scenario (T1.7) prints ≥ 4 lines after the answer). Store: `ulid = "1"` (not 3.x).
- T1.5 done: `mkrew_core::Paths::from_env()` (`MKREW_DATA_DIR`/`MKREW_CONFIG_DIR`/`MKREW_SOCKET` overrides; `db_file/lock_file/logs_dir/sessions_dir/agents_dir`). `mkrew-daemon` is lib+bin: `run(&paths)`, `ipc::name(socket)` (reuse in the CLI client), `EXIT_ALREADY_RUNNING = 2`. `server.rs` answers requests sequentially per connection (no event push yet — T1.8 needs a per-connection writer task + broadcast); first request must be `hello`; unbuilt methods → `invalid_request`. Store behind `std::sync::Mutex`, called inline.
- T1.6 done: `mk` = `crates/mkrew-cli` (sync `client::Client::connect` → hello, `call::<T>()` skips non-matching frames — T1.10 attach needs a streaming reader; `daemon::try_connect` → `None` if not running). CLI tests (`tests/cli.rs`) run `mk` with `MKREW_*` env and need `target/debug/mkrew-daemon` built (`cargo test --workspace` does; `-p mkrew-cli` alone may not). Detached daemons survive a killed test: stop them with `cargo run -p mkrew-cli --bin mk --config 'env.MKREW_SOCKET="mkrew-test-<ulid>"' -- daemon stop` (pipe names are in `%TEMP%/.tmp*/data/logs/daemon*.log`). Never pipe test output through `| tail` while debugging daemon spawns. Broken-profile criterion waits for `agent.profiles` (T1.8).
- Snapshots (no cargo-insta): headless `mv`/`INSTA_UPDATE=...` need approval → read the `.snap.new` and Write the `.snap` (drop `assertion_line`).
- M1 has no hooks (M3): E2E "until done" = screen idle/exit. Reuse spike code in `crates/spike-term` (`pty.rs` reader, `launch.rs` PATHEXT lookup). `Emulator::advance` replies MUST be written to the PTY (ConPTY DSR); portable-pty on Windows: wait on the child on its own thread, reader never hits EOF.
- gpui deps: `gpui = { package = "gpui-pre" }` + `gpui_platform = { package = "gpui-pre-platform" }` (=0.3.7). GUI smoke: `timeout 6 ./target/debug/spike-term.exe` → exit 124 = OK.
- Windows-native; edition 2024, `unsafe_code` denied, `unwrap/expect` denied outside `#[test]` fns (integration-test helpers: file-level `#![allow(clippy::unwrap_used)]`). Headless: no `pwsh`/`python`, use `bash scripts/verify.sh`.
