# Mkrew build progress

<!-- loop-state: machine-readable, keep these exact keys on their own lines -->
status: RUNNING
milestone: M2
<!-- /loop-state -->

Status values: `RUNNING` (loop continues) · `WAITING_HUMAN` (see docs/HUMAN_CHECKS.md, then set back to RUNNING) · `BLOCKED` (no task can progress) · `DONE`.
Task markers: `[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (see note).

## Current milestone: M2 — UI: workspaces & terminals (SPEC §18)

_No task queue yet — the next session plans M2 with the `mkrew-plan-milestone` skill._

### Done (last 10)

_M0 and M1 tasks moved to `docs/progress-archive.md` (M1 tagged `m1-done`; its two unticked criteria — real-terminal `mk attach`, CI run green — are in HUMAN_CHECKS)._

## Notes for the next session

- Protocol: sync `FrameDecoder` + `encode()`; responses are raw JSON (`Outcome::ok/error`, `into_result::<T>()`); `PROTOCOL_VERSION = 1`. Core: `load_dir/parse_profile` → `ProfileSet`; `state::{StateMachine, SendQueue}`; pattern tail = 4 lines. Store: `ulid = "1"` (not 3.x). Fake agent: default built-in `ask` scenario (≈1.2 s, then "(y/n)" prompt, 4 lines after the answer).
- Daemon (`crates/mkrew-daemon/src`): `session.rs` = `Hub` (store, `events` broadcast, live map id → `mpsc::Sender<Command>`, `hub.sender(id)` for awaited sends) + one `Actor` per session owning `Emulator`, `StateMachine`, `SessionLog`, PTY writer and `attachments`. Commands: Kill/Attach/Resnapshot/Resize/Send/SendWritten. Input goes to a `pty-write-<id>` OS thread: `hub.input(id)` → `Arc<tokio Mutex<Sender>>`, server locks it per request and sends 16 KiB chunks (queue 16 chunks); DSR replies `try_send` (dropped if full). `session.send` → actor `SendQueue` → per-session `write_sends` task (FIFO, same input lock) → `SendWritten`; only output after that clears in-flight. `session.wait` (server.rs) = subscribe events, re-read store on each state event, ends on `until` or exit.
- Attach: `attach.rs` = `Attachment` (4 MB queue, overflow → `session.snapshot` event) + `flush()` task per (connection, session), one output frame in flight (`Outgoing.written` oneshot). `server.rs`: async `handle(&mut Conn)`, flusher spawned after the attach reply; dropping `Attached` detaches. Snapshot = `emulator.snapshot()` (protocol `ScreenSnapshot`, shape in SPEC §7). `mk attach`: paint `ScreenSnapshot` runs → ANSI, then raw 0x02 bytes; handle `session.snapshot` by repainting. Every greeted connection gets ALL events → clients loop over every buffered frame (`while let`). The M2 UI client can reuse this shape.
- Daemon tests point `fake.toml` at `target/debug/mkrew-fakeagent(.exe)` (`Env::write_fake_profile`; needs the bin built — `cargo test --workspace` does). Profiles re-read per spawn; samples seeded per missing file. `launch.rs` refuses cmd-metachar args for `.cmd`/`.bat` (HUMAN_CHECKS T1.8: M2 prompt args need a real fix).
- CLI (`crates/mkrew-cli`): sync `client::Client` (`call::<T>()` skips non-matching frames; `next_frame()` streams). `mk session list/spawn <project> --agent P`, `mk attach` (`attach.rs`, `paint.rs`), `mk send/wait` in `session.rs`. CLI tests have `Env::write_fake_profile` + `Env::attach` and need `target/debug/mkrew-daemon` built. Detached daemons survive a killed test: stop with `cargo run -p mkrew-cli --bin mk --config 'env.MKREW_SOCKET="mkrew-test-<ulid>"' -- daemon stop`. Bash loops/`$VAR`/`sed -i` in headless commands need approval → one command at a time, edit files with Edit/Write.
- Snapshots (no cargo-insta): read the `.snap.new` and Write the `.snap` (drop `assertion_line`). Running built binaries directly needs approval → test via integration tests. CI: `.github/workflows/ci.yml` runs `bash scripts/verify.sh` on windows-latest + ubuntu-latest (never run yet). Linux cross-check here: `cargo clippy -p mkrew-core -p mkrew-protocol -p mkrew-fakeagent -p spike-term --all-targets --target x86_64-unknown-linux-gnu` (SQLite crates need a Linux C compiler).
- gpui deps: `gpui = { package = "gpui-pre" }` + `gpui_platform = { package = "gpui-pre-platform" }` (=0.3.7); Linux CI builds them with no x11/wayland features — a real Linux UI (later) needs those features + apt packages (HUMAN_CHECKS T1.12). GUI smoke: `timeout 6 ./target/debug/spike-term.exe` → exit 124 = OK.
- Windows-native; edition 2024, `unsafe_code` denied, `unwrap/expect` denied outside `#[test]` fns (integration-test helpers: file-level `#![allow(clippy::unwrap_used)]`). Headless: no `pwsh`/`python`, use `bash scripts/verify.sh`.
