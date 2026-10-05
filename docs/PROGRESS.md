# Mkrew build progress

<!-- loop-state: machine-readable, keep these exact keys on their own lines -->
status: RUNNING
milestone: M2
<!-- /loop-state -->

Status values: `RUNNING` (loop continues) · `WAITING_HUMAN` (see docs/HUMAN_CHECKS.md, then set back to RUNNING) · `BLOCKED` (no task can progress) · `DONE`.
Task markers: `[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (see note).

## Current milestone: M2 — UI: workspaces & terminals (SPEC §18)

### Acceptance criteria
- [ ] UI spawns the daemon if needed.
- [ ] Workspace sidebar; add workspace via folder picker.
- [ ] Many terminal tabs per workspace; split panes vertically/horizontally; agent or shell panes.
- [ ] Tabs and split layout persist across UI restarts and daemon restarts.
- [ ] Workspace/tab/pane/copy/paste/search shortcuts from §13 work; unbound keys reach the agent
      (verify Ctrl+C, Ctrl+R, Esc inside Claude Code).
- [ ] Close and reopen UI: sessions still alive and re-attached.
- [ ] "Open in Zed" works.
- [ ] Design tokens in `theme.rs`; design check from §14 done.
- [ ] UI idle with 3 workspaces, 6 sessions: < 150 MB RAM.
- [ ] Terminal rendering follows the §10 performance rules; stress test (fake agent `flood` in
      6 panes) meets §15 throughput.
- [ ] Default agent per workspace used for new agent panes.

### Task queue
- [x] T2.1 Daemon + store: `tab.list/create/rename/close/move/set_layout`, `project.update` (`default_agent`), `session.spawn` with `kind` shell/agent + `tab_id` (agent profile defaults to the project's `default_agent`); integration tests; covers: layout persists (daemon side), default agent, shell panes. (spec: §5, §7, §6) — feat(daemon,store): terminal tabs, project.update, spawn defaults
- [ ] T2.2 New crate `mkrew-ui` (bin `mkrew`): `theme.rs` design tokens, empty window shell, background IPC client thread (reuse protocol framing), spawn `mkrew-daemon` detached if `hello` fails; test the connect-or-spawn logic headlessly; covers: UI spawns daemon, design tokens. (spec: §4, §14, §8)
- [ ] T2.3 Terminal element in `mkrew-ui`: port spike emulator/renderer, feed from `session.attach` snapshot + binary output frames, input → `session.input`, resize → `session.resize`; batched style runs per row, dirty-row cache, ≤ 1 present per frame; snapshot test of grid → runs; covers: §10 perf rules. (spec: §10, §7)
- [ ] T2.4 Workspace sidebar: project list, expand → sessions with status icon+text (§11), add workspace via folder picker (`prompt_for_paths`) → `project.add`; pure view-model tests; covers: sidebar, folder picker. (spec: §10, §11, §14)
- [ ] T2.5 Pure split-tree model (split h/v, close, focus neighbour, JSON round-trip matching §5 layout) + tab bar; restore tabs/layout from `tab.list` on start, save via `tab.set_layout`; panes not visible are detached; covers: tabs, splits, persistence, re-attach on reopen. (spec: §5, §10)
- [ ] T2.6 Pane lifecycle: new agent pane (workspace default agent) or shell pane in project folder; close pane with running agent → kill or hide dialog; hidden sessions reachable from sidebar; warn > 12 running; covers: agent/shell panes, default agent, sessions alive after UI close. (spec: §10, §6)
- [ ] T2.7 Keyboard: §13 keymap (workspace/tab/pane actions), selection + copy, bracketed paste, scrollback (10k lines) + search; unbound keys → bytes to PTY (unit-test key→bytes for Ctrl+C, Ctrl+R, Esc); covers: shortcuts. (spec: §13, §10)
- [ ] T2.8 "Open in Zed" in workspace menu (`zed <path>`, PATHEXT lookup, clear error + install hint if missing; testable command builder) + workspace settings for default agent; covers: Open in Zed, default agent UI. (spec: §10, §17)
- [ ] T2.9 Stress + footprint: integration test running fake agent `flood` in 6 attached sessions through the UI client path, assert §15 throughput; RAM measurement procedure; add visual/manual checks (§14 design check, real Claude keys, RAM < 150 MB, reopen UI) to HUMAN_CHECKS; covers: stress, RAM, design check. (spec: §15, §14, §16)

### Done (last 10)

_M0 and M1 tasks moved to `docs/progress-archive.md` (M1 tagged `m1-done`; its two unticked criteria — real-terminal `mk attach`, CI run green — are in HUMAN_CHECKS)._

## Notes for the next session

- Protocol: sync `FrameDecoder` + `encode()`; responses are raw JSON (`Outcome::ok/error`, `into_result::<T>()`); `PROTOCOL_VERSION = 1`. Core: `load_dir/parse_profile` → `ProfileSet`; `state::{StateMachine, SendQueue}`; pattern tail = 4 lines. Store: `ulid = "1"` (not 3.x). Fake agent: default built-in `ask` scenario (≈1.2 s, then "(y/n)" prompt, 4 lines after the answer).
- Daemon (`crates/mkrew-daemon/src`): `session.rs` = `Hub` (store, `events` broadcast, live map id → `mpsc::Sender<Command>`, `hub.sender(id)` for awaited sends) + one `Actor` per session owning `Emulator`, `StateMachine`, `SessionLog`, PTY writer and `attachments`. Commands: Kill/Attach/Resnapshot/Resize/Send/SendWritten. Input goes to a `pty-write-<id>` OS thread: `hub.input(id)` → `Arc<tokio Mutex<Sender>>`, server locks it per request and sends 16 KiB chunks (queue 16 chunks); DSR replies `try_send` (dropped if full). `session.send` → actor `SendQueue` → per-session `write_sends` task (FIFO, same input lock) → `SendWritten`; only output after that clears in-flight. `session.wait` (server.rs) = subscribe events, re-read store on each state event, ends on `until` or exit. T2.1: store `create/list/get/rename/move/set_tab_layout/close_tab`, `get/update_project`; `TerminalTab.layout` is opaque `serde_json::Value` (`null` for a new tab; UI owns the tree); `tab.updated` on create/rename/move/set_layout (none on close); `SessionSpawnParams.agent_profile: Option` (None → `default_agent` / `shell`), spawn into a `null`-layout tab sets `{"session": id}`.
- Attach: `attach.rs` = `Attachment` (4 MB queue, overflow → `session.snapshot` event) + `flush()` task per (connection, session), one output frame in flight (`Outgoing.written` oneshot). `server.rs`: async `handle(&mut Conn)`, flusher spawned after the attach reply; dropping `Attached` detaches. Snapshot = `emulator.snapshot()` (protocol `ScreenSnapshot`, shape in SPEC §7). `mk attach`: paint `ScreenSnapshot` runs → ANSI, then raw 0x02 bytes; handle `session.snapshot` by repainting. Every greeted connection gets ALL events → clients loop over every buffered frame (`while let`). The M2 UI client can reuse this shape.
- Daemon tests point `fake.toml` at `target/debug/mkrew-fakeagent(.exe)` (`Env::write_fake_profile`; needs the bin built — `cargo test --workspace` does). Profiles re-read per spawn; samples seeded per missing file. `launch.rs` refuses cmd-metachar args for `.cmd`/`.bat` (HUMAN_CHECKS T1.8: M2 prompt args need a real fix).
- CLI (`crates/mkrew-cli`): sync `client::Client` (`call::<T>()` skips non-matching frames; `next_frame()` streams). `mk session list/spawn <project> --agent P`, `mk attach` (`attach.rs`, `paint.rs`), `mk send/wait` in `session.rs`. CLI tests have `Env::write_fake_profile` + `Env::attach` and need `target/debug/mkrew-daemon` built. Detached daemons survive a killed test: stop with `cargo run -p mkrew-cli --bin mk --config 'env.MKREW_SOCKET="mkrew-test-<ulid>"' -- daemon stop`. Bash loops/`$VAR`/`sed -i` in headless commands need approval → one command at a time, edit files with Edit/Write.
- Snapshots (no cargo-insta): read the `.snap.new` and Write the `.snap` (drop `assertion_line`). Running built binaries directly needs approval → test via integration tests. CI: `.github/workflows/ci.yml` runs `bash scripts/verify.sh` on windows-latest + ubuntu-latest (never run yet). Linux cross-check here: `cargo clippy -p mkrew-core -p mkrew-protocol -p mkrew-fakeagent -p spike-term --all-targets --target x86_64-unknown-linux-gnu` (SQLite crates need a Linux C compiler).
- gpui deps: `gpui = { package = "gpui-pre" }` + `gpui_platform = { package = "gpui-pre-platform" }` (=0.3.7); Linux CI builds them with no x11/wayland features — a real Linux UI (later) needs those features + apt packages (HUMAN_CHECKS T1.12). GUI smoke: `timeout 6 ./target/debug/spike-term.exe` → exit 124 = OK.
- Windows-native; edition 2024, `unsafe_code` denied, `unwrap/expect` denied outside `#[test]` fns (integration-test helpers: file-level `#![allow(clippy::unwrap_used)]`). Headless: no `pwsh`/`python`, use `bash scripts/verify.sh`.
