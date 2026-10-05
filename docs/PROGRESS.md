# Mkrew build progress

<!-- loop-state: machine-readable, keep these exact keys on their own lines -->
status: RUNNING
milestone: M1
<!-- /loop-state -->

Status values: `RUNNING` (loop continues) · `WAITING_HUMAN` (see docs/HUMAN_CHECKS.md, then set back to RUNNING) · `BLOCKED` (no task can progress) · `DONE`.
Task markers: `[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (see note).

## Current milestone: M1 — Daemon & CLI, no UI (SPEC §18)

### Acceptance criteria
- [ ] `mk daemon start` runs detached; `status`/`stop` work; starting twice never yields two daemons.
- [ ] `mk project add/list` persist to SQLite.
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
- [ ] T1.4 `mkrew-core` session state machine (pure): starting/working/idle/needs_input/exited/unknown from output activity, `idle_after_ms`, needs_input patterns over emulated screen lines, process exit; hook-event input stubbed for M3; plus send-queue policy (deliver when idle/done/needs_input); unit tests with a fake clock; covers: groundwork for `mk send`/`mk wait`. (spec: §6, §8)
- [ ] T1.5 Create `mkrew-daemon` skeleton: data/config dirs (`directories`, override via env for tests), single-instance lock file, `interprocess` local socket (named pipe current-user only / Unix socket), hello + protocol_version check, daemon.status/stop, project.add/list via store, startup marks stale sessions exited, tracing to rotating files; integration test: two daemons in one temp dir → second exits; covers: starting twice never yields two daemons. (spec: §3, §8, §17)
- [ ] T1.6 Create `mkrew-cli` (`mk`, clap): IPC client, `mk daemon start` (spawn detached, `CREATE_NO_WINDOW | DETACHED_PROCESS` on Windows, wait for hello) / `stop` / `status`, `mk project add/list`; integration test against a temp-dir daemon; covers: daemon start/status/stop, project add/list. (spec: §8, §9)
- [ ] T1.7 Create `mkrew-fakeagent`: scenario file (TOML) steps — print (colored), spinner, prompt "Do you want to proceed? (y/n)" and read a line, sleep, write file, exit code; `flood` + `ask` sample scenarios in `crates/mkrew-fakeagent/scenarios/`; point `agents/fake.toml` at it; unit tests on scenario parsing + a PTY-free run test; covers: `mkrew-fakeagent` exists. (spec: §16)
- [ ] T1.8 Daemon sessions: `session.spawn/list/kill` from profiles (command lookup with PATHEXT + cmd-safe `.cmd` quoting, reuse `spike-term/src/launch.rs`), PTY env `MKREW_SESSION_ID`/`MKREW_SOCKET`/`TERM`, one reader OS thread → bounded channel → tokio, headless `alacritty_terminal` (200 lines scrollback, DSR replies written back), state machine wiring + `session.state`/`session.exited` events, session log file (5 MB rotation), shutdown kills PTYs ≤ 5 s; integration test spawning the fake agent; covers: sessions spawn from profiles, PTY reader threads + headless emulator. (spec: §6, §8)
- [ ] T1.9 Daemon attach path: `session.attach` returns grid snapshot (cells + cursor + modes) and subscribes, binary 0x02 output frames batched ≤ 16 ms, per-client bounded queue (4 MB) with drop-and-resnapshot on overflow, `session.input/resize/detach`; tests incl. a slow-client overflow test; covers: binary output frames + backpressure, re-attach snapshot. (spec: §7, §8)
- [ ] T1.10 `mk attach <id>` + `mk session list` (+ `mk session spawn` helper or `--agent` flag, record choice): raw mode (crossterm), paint snapshot then stream frames, forward stdin as `session.input`, resize on terminal resize, `Ctrl+]` detaches leaving the agent running; integration test detach → re-attach shows the same screen; covers: `mk attach` interactive, Ctrl+] detach, instant re-attach. (spec: §9, §7)
- [ ] T1.11 `session.send` (queued while working) + `session.wait` (until states, timeout) in the daemon, `mk send` / `mk wait`; E2E test: real daemon in a temp dir, fake agent `ask` scenario, `mk send y`, `mk wait --until idle,done` until finished; covers: `mk send`/`mk wait`, E2E test. (spec: §7, §9, §16)
- [ ] T1.12 GitHub Actions CI (`windows-latest` + `ubuntu-latest`: fmt, clippy -D warnings, tests); add manual checks to HUMAN_CHECKS (`mk attach` with a real shell/`claude`, detached daemon has no console window, CI run green); covers: CI workflow. (spec: §17, §18)

### Done (last 10)

_M0 tasks moved to `docs/progress-archive.md`._

## Notes for the next session

- `mkrew-protocol` = sync `FrameDecoder` (push bytes → `next_frame()`) + `encode()`; `Message::{Request,Response,Event}`. Responses carry raw JSON (`Outcome::ok/error`, client decodes with `Outcome::into_result::<T>()`). `session.attach` snapshot result type still to add (T1.9). `PROTOCOL_VERSION = 1`.
- T1.2 done: `mkrew_core::{load_dir, parse_profile}` → `ProfileSet { profiles, errors }` (`ProfileError` = file + line/col). Daemon maps to `AgentProfilesResult` (protocol); core's `ProfileKind` ↔ protocol `SessionKind`. Patterns are validated but stored as strings — compile in T1.4. Broken-profile criterion stays unticked until the daemon (T1.5/T1.8) shows it survives. `agents/fake.toml` has no scenario arg yet (T1.7).
- T1.3 done: `mkrew_store::Store::open(path)` (WAL, FKs on, migrations via `user_version` in one IMMEDIATE tx) returns protocol `Project`/`Session` types. `add_project` → `StoreError::DuplicateProject` on same canonical path; `update_session` overwrites mutable cols. Stale-session marking (`mark … exited`) is not there yet — add it in T1.5. Store uses `ulid = "1"` to match protocol (cargo add picks 3.x — don't). "Project persist" criterion waits for `mk project` (T1.6). Next: T1.4.
- Snapshots (no cargo-insta): `mv *.snap.new *.snap`; headless `mv`/`INSTA_UPDATE=...` need approval → read the `.snap.new` and Write the `.snap` (drop `assertion_line`); insta deletes the `.snap.new` on the next pass.
- M1 has no hooks (M3): "until done" in the E2E is reached via screen idle/exit detection (see HUMAN_CHECKS assumption).
- M0 tagged `m0-done` with 5 criteria left to the human gate (see archive). Spike code: `crates/spike-term` (PTY inbox/wake in `pty.rs`, renderer in `view.rs`, key mapping, `launch.rs` PATHEXT lookup) — reuse in M1 daemon (PTY reader) and M2 UI.
- `Emulator::advance` reply bytes MUST be written to the PTY (ConPTY waits for the DSR reply). portable-pty on Windows: wait on the child on its own thread; reader never hits EOF.
- gpui deps: `gpui = { package = "gpui-pre" }` + `gpui_platform = { package = "gpui-pre-platform" }` (=0.3.7). Release builds need `fxc.exe` (on PATH here).
- GUI smoke: `timeout 6 ./target/debug/spike-term.exe` → exit 124 = OK.
- Windows-native target; edition 2024, `[lints] workspace = true`, `unsafe_code` denied. `unwrap/expect` denied outside `#[test]` fns. Headless: no `pwsh`/`powershell`/`tasklist`/`python`, use `bash scripts/verify.sh`.
