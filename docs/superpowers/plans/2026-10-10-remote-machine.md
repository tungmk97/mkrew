# Remote machine: one window, a daemon on another computer — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Mkrew window connects to the daemon of **one** other computer (Windows, Linux or macOS) through SSH, runs agents there like it does locally, and switches between "This computer" and the added machines from a title bar chip.

**Architecture:**
- On the other computer, a new `mk proxy` joins its stdin/stdout to the local daemon socket (it starts the daemon first if needed). The window runs `ssh <host> mk proxy` as a child process and speaks the unchanged §7 protocol over the child's pipes. The daemon gets no TCP port, and its owner-only socket stays as it is (Task 1).
- The protocol gains `session.log_tail`, because the UI read log files from its own disk. It also gains the `notification.show` event, because the daemon's toast appears on the computer that runs the daemon (Task 2).
- The UI client accepts a program endpoint besides the socket. The machine list lives in `<config>/mkrew/machines.toml`, and only the UI uses it (Task 3).
- A title bar chip picks the machine. A switch saves it and rebuilds the whole `Shell` with `Window::replace_root`, so nothing from the old daemon survives (Task 4).
- When the window is connected to another computer, the features that need this computer's disk are turned off or replaced by a typed path (Task 5).

**Tech Stack:** Rust 2024, GPUI (`gpui-pre` 0.3.7), tokio (current-thread runtime on the IPC thread, adds the `process` feature), `interprocess` 2.4.4 (sync split in `mk`), `toml` 1.x, OpenSSH client and server (installed by the user; not a crate).

**Spec:** `docs/SPEC.md` §3 (architecture), §7 (protocol), §8 (daemon: logs, notifications), §9 (CLI), §10 (title bar), §19 (backlog: "remote workspaces over SSH"). The decision record is `docs/decisions/0045-remote-machine.md`, which Task 1 writes. Read both before you start.

**Order:** Tasks 1 → 5, strictly. Tasks 3–5 use the names produced by earlier tasks.

## Global Constraints

- Windows native is first-class: ConPTY, named pipes, `PATHEXT`, `dunce` paths (AGENTS.md, SPEC §2.4). The user's main case is Windows → Windows.
- "An agent is just a terminal command. No per-agent code in the core." (SPEC §2.1)
- Every color, spacing, radius and font size comes from `crates/mkrew-ui/src/theme.rs` (SPEC §14). Grep your diff for hex colors and raw pixel numbers outside `theme.rs`.
- No `unwrap`/`expect` outside tests unless provably infallible, with a comment saying why.
- `thiserror` in libraries, `anyhow` with context in binaries, `tracing` for logs.
- No test may call a real agent or the network. Tests never run `ssh`; they use `mk proxy` as the program endpoint.
- Deviating from the spec requires an ADR plus a SPEC update in the same change. This plan's ADR is **0045**. `0041` exists, and `0042`–`0044` are reserved by `docs/superpowers/plans/2026-10-10-inbox-perf.md`. List `docs/decisions/` first; if `0045` is taken, use the next free number and rename every `0045` in this plan's changes.
- **Protocol version:** only Task 2 changes the wire format. It bumps `PROTOCOL_VERSION` (`crates/mkrew-protocol/src/lib.rs:16`) from the value it reads there (37 today; the perf plan's Task 8 may already have made it 38) to that value + 1.
- **Overlap with the perf plan** (`2026-10-10-inbox-perf.md`, not executed when this was written): its Task 15 edits `read_tail`/`TAIL_BYTES` in `crates/mkrew-ui/src/history.rs`. Task 2 here moves `read_tail` into the daemon. Move whatever `history.rs` holds at that moment: `TAIL_BYTES` with its current value, and `replay_start` plus its tests if they exist. Then the perf plan's Task 15 applies to `crates/mkrew-daemon/src/session_log.rs`.
- Migrations: none.
- Few dependencies: no new crates. The only manifest change is tokio's `process` feature in `crates/mkrew-ui/Cargo.toml`.
- GPUI rule: set `.hover()` at most once per element (`button()`, `menu_item()` already set it). `group_hover` is a separate slot and is safe.
- Commit (user rule 2026-10-10): when a task passes verification, commit it with a Conventional Commit, staging **only that task's files** (never `git add -A`/`.`). Never push, reset, checkout, stash or clean. Never stage the user's unrelated edits: `docs/PERF_REVIEW.md`, other plans, `docs/superpowers/specs/`, `.github/workflows/*`, `packaging/`, `docs/BUILD_VI.md`, `Cargo.lock` changes not caused by this task, and the x11/wayland lines in `crates/mkrew-ui/Cargo.toml`.
- Never write files outside this repo. The user's own `<config>/mkrew/machines.toml` and the SSH setup are **user steps** listed in HUMAN_CHECKS.
- **Inbox:** Task 5's last step changes the line this plan added to `docs/INBOX.md` from `[~]` to `[x]`.
- **HUMAN_CHECKS:** Task 1 creates the section `### To verify — 2026-10-10 remote machine (plan docs/superpowers/plans/2026-10-10-remote-machine.md)` directly above the first existing `### To verify —` heading. Later tasks append their bullets to the end of that section.
- Verify in the omp shell with `sh scripts/verify.sh` (`bash` there is WSL: never use it). If rustc crashes on mkrew-ui test builds (STATUS_STACK_BUFFER_OVERRUN), run these three in order: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`.
- The C: drive is nearly full: no `cargo clean`, no extra target dirs. Check free space before a full build.
- An `.exe` that is "Access is denied" (a running daemon holds it): rename it to `*.locked-old.exe`. Never kill the user's daemon or sessions.
- `crates/mkrew-ui/src/main.rs` is ~20.7k lines and the user edits it too: read ranges, and find each function by name (`grep -n "fn render_title_bar"`). Line numbers below are from 2026-10-10 and drift.
- GUI cannot be tested headless: pure logic gets tests, and every visible change gets a HUMAN_CHECKS line.
- Nothing in this plan was compiled while it was written. Where the code below names something in the codebase, check it against the code (the name, the field type) and adjust. Do not invent a second convention.

## Review Focus

1. **ssh fails before `hello`** (no key, `mk` not on the remote PATH, host down, host key changed). Expected: the window says "Cannot reach `<machine>`.", followed by ssh's own reason and one line on what to do. Not just "closed during hello". Pinned by Task 3 `a_program_that_fails_before_hello_explains_why`, `a_failed_start_names_the_programs_last_stderr_lines` and `hints_for_the_usual_failures`.
2. **The window closes or switches away while agents run there.** Expected: `mk proxy` exits and the remote daemon and its sessions keep running, so switching back finds them. Pinned by Task 1 `proxy_starts_the_daemon_and_carries_frames_until_stdin_closes`. That ssh itself is killed on a switch is a HUMAN_CHECKS line (Task 4).
3. **A typed machine that is really an ssh option** (`-oProxyCommand=…`, or text with spaces). Expected: refused in the dialog and never passed to ssh. Pinned by Task 3 `add_trims_and_refuses_empty_options_spaces_and_duplicates`.
4. **A hand-edited `machines.toml` with a mistake.** Expected: the window uses this computer and says why, and no later save overwrites the file. Pinned by Task 3 `save_writes_load_reads_and_never_overwrites_a_broken_file`. The "says why" part is a HUMAN_CHECKS line (Task 4).
5. **History of a session with a large log, opened over the network.** Expected: the newest 2 MiB (or whatever `TAIL_BYTES` is) arrive in one frame, under the 16 MiB frame cap even after base64, and are byte-identical to the log on disk. Pinned by Task 2 `session_log_tail_streams_the_log_files_of_a_session`, `log_tail_bytes_are_base64`, and the `read_tail` tests that move into `session_log.rs`.

Also pinned inside the tasks: a stale `current` naming a removed machine (Task 3), Reconnect being listed only while disconnected (Task 4), and the ssh command line never prompting (Task 3). These cannot be unit-tested and are HUMAN_CHECKS lines: a real Windows→Windows and Windows→Ubuntu connection, unsaved Files changes on a switch, notifications shown on this computer, Zed refused on another computer.

---

### Task 1: `mk proxy` + ADR 0045

Spec: §3 (SPEC.md:108-146), §9 (SPEC.md:653-684), §19 backlog (SPEC.md:1488-1491), §17 named pipe owner-only (SPEC.md:1294).

**Current state (investigated):**
- `crates/mkrew-cli/src/daemon.rs:30-80`, `start(paths)`: it connects with `try_connect`, otherwise spawns `daemon_exe()` with `detach()` and polls `try_connect` every 50 ms for up to 10 s. It handles `EXIT_ALREADY_RUNNING` (lost race) and `println!`s the result.
- `crates/mkrew-daemon/src/spawn.rs:23-40`, `detach(command)`: on Windows `creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)` plus `disinherit_stdio()`; on Unix `process_group(0)`. `creation_flags` replaces the earlier value, it does not OR into it.
- `crates/mkrew-cli/src/main.rs:28-88`: the `Command` enum. `run()` at :242 dispatches. Hook and Integration return before `Paths::from_env()`.
- `mk` uses the sync `interprocess::local_socket::Stream` (`crates/mkrew-cli/src/client.rs`). `Stream: StreamCommon` has `split(self) -> (RecvHalf, SendHalf)` (interprocess 2.4.4 `src/local_socket/stream/enum.rs:103`), and the halves implement `Read`/`Write`.
- `std::io::stdout()` is line-buffered, so binary frames sit in its buffer until a `\n`. The proxy must flush after every chunk.
- On Windows, OpenSSH puts a session's processes in a job object. A daemon started inside it can die when the SSH session closes `[INFERENCE]`. `CREATE_BREAKAWAY_FROM_JOB` takes it out, unless the job forbids that (`ERROR_ACCESS_DENIED` → `io::ErrorKind::PermissionDenied`).
- CLI integration tests: `crates/mkrew-cli/tests/cli.rs`. `Env::new()` gives a temp data/config dir and a unique socket; `env.command(&[..])` runs the built `mk` with `MKREW_*` set and `MKREW_NOTIFY=off`.

**Files:**
- Create: `crates/mkrew-cli/src/proxy.rs`
- Create: `docs/decisions/0045-remote-machine.md`
- Modify: `crates/mkrew-cli/src/main.rs` (`mod proxy;`, `Command::Proxy`, dispatch)
- Modify: `crates/mkrew-cli/src/daemon.rs` (`ensure_running`, `Running`, `spawn_daemon`; `start` uses them)
- Modify: `crates/mkrew-daemon/src/spawn.rs` (`detach_from_job`, shared flag constants)
- Modify: `docs/SPEC.md` §3, §9, §19
- Modify: `docs/HUMAN_CHECKS.md` (new section)
- Test: `crates/mkrew-cli/tests/cli.rs`

**Interfaces:**
- Produces: `mk proxy`, a CLI subcommand that is silent on stdout except for protocol bytes; exit 0 when stdin closes.
- Produces: `pub fn ensure_running(paths: &Paths) -> anyhow::Result<Running>` and `pub enum Running { Already(u32), Started(u32) }` in `crates/mkrew-cli/src/daemon.rs` (pid = `DaemonStatusResult::pid: u32`).
- Produces: `pub fn detach_from_job(command: &mut std::process::Command)` in `crates/mkrew-daemon/src/spawn.rs`.

- [ ] **Step 1: Write the failing test**

Append to `crates/mkrew-cli/tests/cli.rs`. Add `ChildStdin`/`ChildStdout` to the `std::process` import at the top: `use std::process::{Child, ChildStdin, ChildStdout, Command, Output, Stdio};`.

```rust
/// Sends `request` as frame `id` through a running `mk proxy` and returns its response
/// (events in between are skipped).
fn proxy_call(
    stdin: &mut ChildStdin,
    stdout: &mut ChildStdout,
    decoder: &mut mkrew_protocol::FrameDecoder,
    id: u64,
    request: mkrew_protocol::Request,
) -> mkrew_protocol::Outcome {
    use mkrew_protocol::{Frame, Message, encode};
    let frame = encode(&Frame::Message(Message::Request { id, request })).unwrap();
    stdin.write_all(&frame).unwrap();
    stdin.flush().unwrap();
    let mut buf = [0u8; 4096];
    loop {
        while let Some(frame) = decoder.next_frame().unwrap() {
            if let Frame::Message(Message::Response { id: got, outcome }) = frame
                && got == id
            {
                return outcome;
            }
        }
        let n = stdout.read(&mut buf).unwrap();
        assert!(n > 0, "mk proxy closed its stdout");
        decoder.push(&buf[..n]);
    }
}

#[test]
fn proxy_starts_the_daemon_and_carries_frames_until_stdin_closes() {
    use mkrew_protocol::{
        ClientKind, DaemonStatusResult, FrameDecoder, HelloParams, HelloResult, PROTOCOL_VERSION,
        Request,
    };
    let env = Env::new();
    // No daemon runs yet: the proxy starts it.
    let mut proxy = env
        .command(&["proxy"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = proxy.stdin.take().unwrap();
    let mut stdout = proxy.stdout.take().unwrap();
    let mut decoder = FrameDecoder::new();
    let hello = Request::Hello(HelloParams {
        client: ClientKind::Ui,
        protocol_version: PROTOCOL_VERSION,
    });
    // Anything else on stdout (a "daemon started" line) would break this decode.
    let hello: HelloResult = proxy_call(&mut stdin, &mut stdout, &mut decoder, 1, hello)
        .into_result()
        .unwrap();
    assert_eq!(hello.protocol_version, PROTOCOL_VERSION);
    let status: DaemonStatusResult =
        proxy_call(&mut stdin, &mut stdout, &mut decoder, 2, Request::DaemonStatus)
            .into_result()
            .unwrap();

    // The window went away (ssh closes stdin): the proxy ends, the daemon stays.
    drop(stdin);
    let deadline = Instant::now() + Duration::from_secs(10);
    let exit = loop {
        if let Some(exit) = proxy.try_wait().unwrap() {
            break exit;
        }
        assert!(Instant::now() < deadline, "mk proxy did not exit");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(exit.success(), "{exit}");
    let after = env.ok(&["daemon", "status"]);
    assert!(after.contains(&format!("pid:       {}", status.pid)), "{after}");
    env.ok(&["daemon", "stop"]);
}
```

- [ ] **Step 2: Run it to make sure it fails**

Run: `cargo build -p mkrew-daemon && cargo test -p mkrew-cli --test cli proxy_starts_the_daemon -j 4`
Expected: FAIL. `mk` exits with `error: unrecognized subcommand 'proxy'`, so `proxy_call` panics on "mk proxy closed its stdout".

- [ ] **Step 3: `detach_from_job` in `crates/mkrew-daemon/src/spawn.rs`**

Replace the Windows block of `detach` and add the new function plus shared constants:

```rust
#[cfg(windows)]
const DETACHED_PROCESS: u32 = 0x0000_0008;
#[cfg(windows)]
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// No console, own process group: Ctrl+C in the shell that ran the client does not reach the
/// daemon.
#[cfg(windows)]
const DETACH_FLAGS: u32 = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW;

/// No console window and no controlling terminal: the daemon outlives the shell (SPEC §8).
/// The UI also uses it for the short-lived `zed` CLI.
pub fn detach(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        disinherit_stdio();
        command.creation_flags(DETACH_FLAGS);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Own process group: terminal job-control signals (Ctrl+C, hangup) skip the daemon.
        command.process_group(0);
    }
}

/// [`detach`], and on Windows also out of the caller's job object: Windows OpenSSH runs a
/// session in a job that ends with the session, which would stop a daemon `mk proxy` started
/// there (ADR 0045). Spawning fails with `PermissionDenied` when the job forbids breaking
/// away; callers then fall back to [`detach`].
pub fn detach_from_job(command: &mut Command) {
    detach(command);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        command.creation_flags(DETACH_FLAGS | CREATE_BREAKAWAY_FROM_JOB);
    }
}
```

- [ ] **Step 4: `ensure_running` in `crates/mkrew-cli/src/daemon.rs`**

Replace `start` (lines 30-80) with the three items below. Add `use std::path::Path;` and `use std::process::Child;`, and add `detach_from_job` to the `mkrew_daemon::spawn` import.

```rust
/// What [`ensure_running`] found: the daemon's pid.
pub enum Running {
    Already(u32),
    Started(u32),
}

/// Starts the daemon unless one answers `hello`, and waits until it does. Prints nothing: `mk
/// proxy` uses it, and its stdout carries protocol frames only.
pub fn ensure_running(paths: &Paths) -> anyhow::Result<Running> {
    if let Some(mut client) = try_connect(paths)? {
        let status: DaemonStatusResult = client.call(Request::DaemonStatus)?;
        return Ok(Running::Already(status.pid));
    }
    let exe = daemon_exe()?;
    let mut child = spawn_daemon(&exe)?;
    let deadline = Instant::now() + TIMEOUT;
    let mut lost_race = false;
    loop {
        if let Some(mut client) = try_connect(paths)? {
            let status: DaemonStatusResult = client.call(Request::DaemonStatus)?;
            return Ok(if lost_race {
                Running::Already(status.pid)
            } else {
                Running::Started(status.pid)
            });
        }
        if !lost_race && let Some(exit) = child.try_wait().context("check the daemon process")? {
            // Another client won the lock: keep waiting for that daemon instead.
            if exit.code() == Some(i32::from(EXIT_ALREADY_RUNNING)) {
                lost_race = true;
            } else {
                bail!(
                    "the daemon exited during startup ({exit}); see the logs in {}",
                    paths.logs_dir().display()
                );
            }
        }
        if Instant::now() >= deadline {
            bail!(
                "the daemon did not answer within {}s; see the logs in {}",
                TIMEOUT.as_secs(),
                paths.logs_dir().display()
            );
        }
        std::thread::sleep(POLL);
    }
}

/// The daemon, detached, and on Windows outside the caller's job object when the job allows it.
fn spawn_daemon(exe: &Path) -> anyhow::Result<Child> {
    let command = || {
        let mut command = Command::new(exe);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command
    };
    let mut free = command();
    detach_from_job(&mut free);
    let spawned = match free.spawn() {
        // A job without JOB_OBJECT_LIMIT_BREAKAWAY_OK refuses with ERROR_ACCESS_DENIED.
        Err(e) if e.kind() == ErrorKind::PermissionDenied => {
            let mut plain = command();
            detach(&mut plain);
            plain.spawn()
        }
        spawned => spawned,
    };
    spawned.with_context(|| format!("start {}", exe.display()))
}

pub fn start(paths: &Paths) -> anyhow::Result<ExitCode> {
    match ensure_running(paths)? {
        Running::Already(pid) => println!("daemon already running (pid {pid})"),
        Running::Started(pid) => println!("daemon started (pid {pid})"),
    }
    Ok(ExitCode::SUCCESS)
}
```

- [ ] **Step 5: `crates/mkrew-cli/src/proxy.rs`**

```rust
//! `mk proxy` (ADR 0045): joins stdin/stdout to this computer's daemon socket, starting the
//! daemon first if none runs. A Mkrew window on another computer runs `ssh <host> mk proxy`
//! and speaks the normal protocol (SPEC §7) through it; bytes pass through unchanged.

use std::io::{self, Read, Write};
use std::process::ExitCode;

use anyhow::Context;
use interprocess::local_socket::{Stream, prelude::*};
use mkrew_core::Paths;

use crate::daemon;

const BUFFER: usize = 64 * 1024;

pub fn run(paths: &Paths) -> anyhow::Result<ExitCode> {
    daemon::ensure_running(paths)?;
    let stream = Stream::connect(mkrew_daemon::ipc::name(&paths.socket)?)
        .with_context(|| format!("connect to {}", paths.socket))?;
    let (mut from_daemon, mut to_daemon) = stream.split();
    // The process ends as soon as either side closes, so a closed SSH session never leaves a
    // proxy holding a daemon connection (and the daemon never keeps a dead client attached).
    std::thread::spawn(move || {
        let code = match pump(&mut from_daemon, &mut io::stdout().lock()) {
            Ok(()) => 0,
            Err(_) => 1,
        };
        std::process::exit(code);
    });
    pump(&mut io::stdin().lock(), &mut to_daemon)?;
    Ok(ExitCode::SUCCESS)
}

/// Copies until `from` ends, flushing every chunk: stdout is line-buffered and frames are
/// binary.
fn pump(from: &mut impl Read, to: &mut impl Write) -> io::Result<()> {
    let mut buf = vec![0u8; BUFFER];
    loop {
        let n = match from.read(&mut buf) {
            Ok(0) => return Ok(()),
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        to.write_all(&buf[..n])?;
        to.flush()?;
    }
}
```

In `crates/mkrew-cli/src/main.rs`: add `mod proxy;` (alphabetical, after `mod project;`). Add the variant after `Hook` in `enum Command`:

```rust
    /// Join stdin/stdout to the daemon, starting it if needed: a Mkrew window on another
    /// computer runs `ssh <this computer> mk proxy` (not for typing).
    Proxy,
```

Add the arm after `Command::Attach { session_id } => attach::attach(&paths, session_id),` in `run()`:

```rust
        Command::Proxy => proxy::run(&paths),
```

- [ ] **Step 6: Run the test to make sure it passes**

Run: `cargo build -p mkrew-daemon && cargo test -p mkrew-cli --test cli -j 4`
Expected: PASS, including the existing `daemon_start_status_stop` (it pins `start`'s output text).

- [ ] **Step 7: ADR 0045**

`docs/decisions/0045-remote-machine.md` (check the number first, see Global Constraints):

```markdown
# 0045 — Connect the window to a daemon on another computer

- Date: 2026-10-10
- Status: accepted
- Milestone: none (outside the milestone loop; user request: run agents on another Windows
  computer and drive them from this window, switching between computers)

## Context

- The daemon owns PTYs, SQLite, git and files; the window is a client of the §7 protocol on a
  local socket (§3). Running agents elsewhere only needs the frames to reach that computer's
  daemon.
- The named pipe / Unix socket is owner-only (§17) and stays so; the daemon has no
  authentication of its own.
- §19 lists "remote workspaces over SSH" in the backlog. Asked for: one window, one other
  computer at a time, easy to set up, and a way to switch back.

## Options

1. A TCP listener in the daemon with a token: a new network surface, TLS and key handling.
2. `ssh -L` forwarding of the socket: Windows OpenSSH does not document forwarding named pipes;
   two processes to manage.
3. `ssh <host> mk proxy`: `mk proxy` joins its stdin/stdout to the local socket (starting the
   daemon if needed); the window speaks the protocol over the ssh child's pipes. (Chosen.)

## Decision

- `mk proxy` (§9): makes sure the daemon runs (like `mk daemon start`, silently; on Windows
  outside the SSH session's job object when the job allows it), connects, copies bytes both
  ways, exits when either side closes.
- The window keeps its machines in `<config>/mkrew/machines.toml` (its own file; `settings.toml`
  stays per computer): `[[machine]] ssh = "user@host"` (optional `mk = "<remote mk command>"`)
  and `current` = the machine in use (absent = this computer).
- ssh runs as `ssh -T -o BatchMode=yes -o StrictHostKeyChecking=accept-new -o
  ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=3 -o LogLevel=ERROR <ssh>
  <mk> proxy`: keys only and never a prompt (there is no terminal to answer it), a first-seen
  host key is accepted and a changed one refused, warnings stay out of stderr so a failure shows
  ssh's own reason. A typed machine starting with `-` or holding whitespace is refused (it would
  reach ssh as an option).
- One machine at a time, picked from a title bar chip. A switch saves `current` and rebuilds the
  window's shell (`Window::replace_root`): no state of the previous daemon survives. Unsaved
  Files changes are asked about first. Reconnect rebuilds the same way.
- Session logs come over the protocol (`session.log_tail`). Every notification the daemon
  decides on is also broadcast (`notification.show`); a window connected to another computer
  shows it on this one.
- Connected to another computer, what needs this computer's disk is off or typed instead: Zed
  and the OS file manager say they only open this computer's files; terminal file links are
  off (URLs still open); Add workspace and Link plan ask for a path on that computer instead of
  the OS file picker.

## Consequences

- SPEC §3, §7, §8, §9, §10 and §19 change; the protocol version goes up by one.
- Both computers need the same Mkrew version: `hello` refuses others and the window says to
  update.
- A daemon `mk proxy` starts gets the SSH login's environment: on Linux/macOS the PATH of a
  non-interactive shell, on Windows a key logon that cannot use Credential Manager. The README
  recommends starting the daemon at desktop logon on that computer.
- Not done: several computers at once (would need a new ADR: one connection per machine, merged
  sidebar), Zed remote, files dropped or pasted from this computer into a remote terminal (they
  type this computer's paths), copying files between computers, automatic reconnect.
```

- [ ] **Step 8: SPEC §3, §9, §19**

`docs/SPEC.md` §3: after the **Lifecycle** bullet list (it ends with "Exactly one daemon per user, enforced by a lock file in the data dir."), add:

```markdown
**Another computer (ADR 0045).** The UI may instead talk to the daemon of one other computer:
it runs `ssh <host> mk proxy` and speaks the same protocol over that child's stdin/stdout;
`mk proxy` joins them to the daemon's local socket there (starting the daemon if needed). The
daemon never listens on the network. The machine in use is chosen in the title bar (§10).
```

§9, in the command block after the `mk hook` line:

```
mk proxy                               # stdin/stdout ↔ the daemon (for `ssh <host> mk proxy`, ADR 0045)
```

§19 backlog: change `remote workspaces over SSH;` to `several remote computers in one window (one is done, ADR 0045);`.

- [ ] **Step 9: HUMAN_CHECKS section**

In `docs/HUMAN_CHECKS.md`, directly above the first existing `### To verify —` heading, add:

```markdown
### To verify — 2026-10-10 remote machine (plan docs/superpowers/plans/2026-10-10-remote-machine.md)

- [ ] **`mk proxy` survives on Windows:** on the other Windows computer, with no daemon
  running, from this computer run `ssh user@host mk proxy` and close it with Ctrl+C. Then
  `ssh user@host mk daemon status` → still "daemon running" (it left the SSH job object).
```

- [ ] **Step 10: Verify and commit**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p mkrew-cli -j 4`
Expected: clean, all pass.

```bash
git add crates/mkrew-cli/src/proxy.rs crates/mkrew-cli/src/main.rs crates/mkrew-cli/src/daemon.rs crates/mkrew-daemon/src/spawn.rs crates/mkrew-cli/tests/cli.rs docs/decisions/0045-remote-machine.md docs/SPEC.md docs/HUMAN_CHECKS.md
git commit -m "feat(cli): mk proxy joins stdin/stdout to the daemon (ADR 0045)"
```

---

### Task 2: Protocol: `session.log_tail` and `notification.show`

Spec: §7 (SPEC.md:488-608), §8 session logs + notifications (SPEC.md:609-652), ADR 0045.

**Current state (investigated):**
- `Request::SessionHistory(SessionRef)` (`crates/mkrew-protocol/src/message.rs:151-153`) returns `SessionHistory { files: Vec<PathBuf> }` (:1098-1105). The daemon handler is at `crates/mkrew-daemon/src/server.rs:340-346`. The UI (`main.rs` `fn load_history_log`, ~:6145) then reads those paths with `history::read_tail` (`crates/mkrew-ui/src/history.rs:167-205`). That read fails when the daemon runs on another computer. `mk session history` (the CLI) runs on the daemon's computer and stays on `session.history`.
- Bytes in JSON use `#[serde(with = "base64_bytes")]` (message.rs:1132-1134, module at :1634). `MAX_FRAME_LEN` is 16 MiB (`frame.rs:16`); 2 MiB in base64 ≈ 2.7 MiB.
- Notifications: `crates/mkrew-daemon/src/notify.rs` `start()` (:58-94) decides per `session.state` and calls `notifier.show(notice, …)`. `Notice` (:31-38) and `trait Notifier` are `pub`, `struct OsNotifier` (:168) is private, and `mod notify` is private in `crates/mkrew-daemon/src/lib.rs:18` (only `ENV_NOTIFY` is re-exported). Daemon tests set `MKREW_NOTIFY` to a JSON-lines file and wait with `env.wait_notification(id)` (`crates/mkrew-daemon/tests/daemon.rs:72`). The test `fake_agent_session_reaches_needs_input_and_is_killed` (:797) checks one.
- `Event` (message.rs:336) is matched exhaustively in `crates/mkrew-ui/src/sidebar.rs` (~:398 and ~:432). Add the new variant to both `Event::NotificationClicked(_)` lists.
- `PROTOCOL_VERSION` is at `crates/mkrew-protocol/src/lib.rs:16`.

**Files:**
- Modify: `crates/mkrew-protocol/src/message.rs` (request, result, event, tests)
- Modify: `crates/mkrew-protocol/src/lib.rs` (`PROTOCOL_VERSION`, re-exports if the crate lists them)
- Modify: `crates/mkrew-daemon/src/session_log.rs` (`TAIL_BYTES`, `read_tail`, tests moved here)
- Modify: `crates/mkrew-daemon/src/server.rs` (handler)
- Modify: `crates/mkrew-daemon/src/notify.rs` (broadcast; `pub struct OsNotifier`)
- Modify: `crates/mkrew-daemon/src/lib.rs` (`pub mod notify;`)
- Modify: `crates/mkrew-ui/src/history.rs` (remove `Tail`, `read_tail`, `TAIL_BYTES` and their tests)
- Modify: `crates/mkrew-ui/src/main.rs` (`fn load_history_log`)
- Modify: `crates/mkrew-ui/src/sidebar.rs` (new event in the ignore lists)
- Modify: `docs/SPEC.md` §7, §8
- Test: `crates/mkrew-daemon/tests/daemon.rs`, `crates/mkrew-protocol/src/message.rs` tests

**Interfaces:**
- Produces: `Request::SessionLogTail(SessionRef)` ↔ wire `session.log_tail` → `SessionLogTail { bytes: Vec<u8> /* base64 */, cut: bool }`.
- Produces: `Event::NotificationShow(NotificationShowEvent { session_id: Ulid, project_id: Ulid, title: String, body: String, sound: bool })` ↔ wire `notification.show`.
- Produces: `pub mod notify` in `mkrew-daemon` with `pub struct OsNotifier;` (implements `pub trait Notifier { fn show(&self, notice: Notice, clicked: Clicked); }`, where `Clicked = tokio::sync::mpsc::UnboundedSender<Notice>`).
- Produces: `pub const TAIL_BYTES: u64` and `pub fn read_tail(files: &[PathBuf], limit: u64) -> io::Result<SessionLogTail>` in `crates/mkrew-daemon/src/session_log.rs` (crate-private module; `pub` within it).

- [ ] **Step 1: Failing protocol tests**

In `crates/mkrew-protocol/src/message.rs` `mod tests`:
- In the list of requests that round-trip (~:1863, next to `Request::SessionHistory(SessionRef { session_id: sid() })`), add `Request::SessionLogTail(SessionRef { session_id: sid() }),`.
- Next to the `NotificationClicked` round trip (~:2336), add:

```rust
        let shown = round_trip(&Message::Event(Event::NotificationShow(NotificationShowEvent {
            session_id: sid(),
            project_id: sid(),
            title: "api · claude".into(),
            body: "Needs input".into(),
            sound: true,
        })));
        assert_eq!(shown["event"], "notification.show");
```

(Match the surrounding code: if `round_trip` returns the JSON `Value`, keep the assert; otherwise drop it.)

Add after `input_data_is_base64` (~:2005):

```rust
    #[test]
    fn log_tail_bytes_are_base64() {
        let tail = SessionLogTail {
            bytes: b"\x1b[1mhi".to_vec(),
            cut: true,
        };
        let json = serde_json::to_value(&tail).unwrap();
        assert_eq!(json["bytes"], "G1sxbWhp");
        assert_eq!(serde_json::from_value::<SessionLogTail>(json).unwrap(), tail);
    }
```

Run: `cargo test -p mkrew-protocol -j 4`
Expected: FAIL to compile (`SessionLogTail`, `NotificationShow` unknown).

- [ ] **Step 2: Protocol types**

`message.rs`, in `enum Request` after `SessionHistory`:

```rust
    /// Returns [`SessionLogTail`]: the newest output of the session's log, read by the daemon
    /// (the window may run on another computer, ADR 0045). Works for exited sessions too.
    #[serde(rename = "session.log_tail")]
    SessionLogTail(SessionRef),
```

After `struct SessionHistory`:

```rust
/// `session.log_tail` result: the last bytes of the session's output log (SPEC §8) as one
/// stream, at most the daemon's tail limit; a cut stream starts after its first line break.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLogTail {
    /// Raw PTY bytes, base64 on the wire.
    #[serde(with = "base64_bytes")]
    pub bytes: Vec<u8>,
    /// Older output was left out.
    pub cut: bool,
}
```

In `enum Event` after `NotificationClicked`:

```rust
    /// The daemon showed (or tried to show) an OS notification for a session (SPEC §8). Sent to
    /// every client, so a window on another computer shows it there (ADR 0045).
    #[serde(rename = "notification.show")]
    NotificationShow(NotificationShowEvent),
```

After `struct NotificationClickedEvent`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationShowEvent {
    pub session_id: Ulid,
    pub project_id: Ulid,
    pub title: String,
    pub body: String,
    /// Play the OS default sound (`notify.sound`).
    pub sound: bool,
}
```

If `crates/mkrew-protocol/src/lib.rs` re-exports message types by name, add `SessionLogTail` and `NotificationShowEvent` there. Bump `PROTOCOL_VERSION` to the current value + 1 and keep its comment.

Run: `cargo test -p mkrew-protocol -j 4`
Expected: PASS.

- [ ] **Step 3: Move `read_tail` into the daemon**

Cut `TAIL_BYTES`, `struct Tail` and `fn read_tail` (plus `replay_start` if the perf plan added it) out of `crates/mkrew-ui/src/history.rs`, and move their tests along. Today these are `read_tail_joins_the_parts_and_cuts_at_a_line_break` and the read part of `a_replayed_tail_shows_the_sessions_last_screen`. That second test also replays into a `TerminalPane`/emulator: keep the replay half in the UI, feeding it a literal byte string instead of a file. Then delete the now-unused `File`/`Read`/`Seek` imports.

Paste into `crates/mkrew-daemon/src/session_log.rs`, returning the protocol type. Add `use std::io::{Read, Seek, SeekFrom};` to the existing `std::io` import and `use mkrew_protocol::SessionLogTail;`.

```rust
/// Most log bytes `session.log_tail` sends (the newest ones); `mk session history <id>` prints
/// all of it.
pub const TAIL_BYTES: u64 = 2 * 1024 * 1024;

/// Reads the last `limit` bytes of `files` (oldest first, as [`files`] lists them) as one
/// stream. A cut stream starts after its first line break, so a replay does not begin inside an
/// escape sequence. A file that is gone (rotated away meanwhile) is skipped.
pub fn read_tail(files: &[PathBuf], limit: u64) -> io::Result<SessionLogTail> {
    // body exactly as it was in history.rs, ending in:
    // Ok(SessionLogTail { bytes, cut })
}
```

Keep `TAIL_BYTES` at whatever value `history.rs` had. In the moved tests, replace `Tail` with `SessionLogTail`.

Run: `cargo test -p mkrew-daemon --lib session_log -j 4`
Expected: PASS (moved tests).

- [ ] **Step 4: Failing daemon test**

In `crates/mkrew-daemon/tests/daemon.rs`, add `SessionLogTail` and `NotificationShowEvent` (if the test matches on it) to the `mkrew_protocol` import. Then add:

```rust
#[test]
fn session_log_tail_streams_the_log_files_of_a_session() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::NeedsInput);
    let kill = Request::SessionKill(SessionKillParams {
        session_id: id,
        force: true,
    });
    client.call(kill).into_result::<()>().unwrap();
    client.wait_state(id, SessionState::Exited);

    let tail: SessionLogTail = client
        .call(Request::SessionLogTail(SessionRef { session_id: id }))
        .into_result()
        .unwrap();
    let history: SessionHistory = client
        .call(Request::SessionHistory(SessionRef { session_id: id }))
        .into_result()
        .unwrap();
    let on_disk: Vec<u8> = (history.files.iter())
        .flat_map(|file| std::fs::read(file).unwrap())
        .collect();
    assert!(!tail.cut);
    assert!(!tail.bytes.is_empty());
    // Read first, so a late flush can only add to the files.
    assert!(on_disk.starts_with(&tail.bytes), "the tail is the log, oldest part first");
    assert_eq!(
        error_code(client.call(Request::SessionLogTail(SessionRef {
            session_id: Ulid::new()
        }))),
        ErrorCode::NotFound
    );
    client.stop();
}
```

In `fake_agent_session_reaches_needs_input_and_is_killed`, after `assert_eq!(notice["body"], "Needs input");`, add:

```rust
    // The same notice goes to every client (a window on another computer shows it, ADR 0045).
    client.wait_event_where(|e| {
        matches!(e, Event::NotificationShow(n)
            if n.session_id == id && n.project_id == project.id && n.body == "Needs input")
    });
```

Run: `cargo build -p mkrew-fakeagent && cargo test -p mkrew-daemon --test daemon session_log_tail fake_agent_session_reaches -j 4`
Expected: FAIL. The daemon answers `session.log_tail` with an unknown-method error, and no `notification.show` event arrives.

- [ ] **Step 5: Daemon handler and broadcast**

`crates/mkrew-daemon/src/server.rs`: next to the `Request::SessionHistory` arm, add

```rust
            Request::SessionLogTail(params) => {
                (self.session_log_tail(params.session_id).await, After::Continue)
            }
```

Then add, among the other `async fn` request handlers of the same `impl`:

```rust
    /// `session.log_tail`: up to `TAIL_BYTES` read off the runtime (ADR 0045).
    async fn session_log_tail(&self, session_id: Ulid) -> Outcome {
        let session = match self.store().get_session(session_id) {
            Ok(session) => session,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        let files = session_log::files(&session_log::path(&self.paths.sessions_dir(), session.id));
        let read = tokio::task::spawn_blocking(move || {
            session_log::read_tail(&files, session_log::TAIL_BYTES)
        })
        .await;
        match read {
            Ok(Ok(tail)) => ok(&tail),
            Ok(Err(e)) => Outcome::error(ErrorCode::Internal, format!("read the session log: {e}")),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("read the session log: {e}")),
        }
    }
```

`crates/mkrew-daemon/src/notify.rs`: import `NotificationShowEvent` with the other `mkrew_protocol` names, make `struct OsNotifier` `pub`, and replace the `if let Some(notice) = notice(&hub, e.session_id, alert)` block in `start()` with:

```rust
                        if let Some(notice) = notice(&hub, e.session_id, alert) {
                            // Also to every client: a window on another computer shows it
                            // there (ADR 0045).
                            let _ = hub.events.send(Event::NotificationShow(NotificationShowEvent {
                                session_id: notice.session_id,
                                project_id: notice.project_id,
                                title: notice.title.clone(),
                                body: notice.body.clone(),
                                sound: notice.sound,
                            }));
                            notifier.show(notice, clicked.clone());
                        }
```

`crates/mkrew-daemon/src/lib.rs`: `mod notify;` → `pub mod notify;`. Keep `pub use notify::ENV_NOTIFY;`.

`crates/mkrew-ui/src/sidebar.rs`: add `| Event::NotificationShow(_)` next to each `| Event::NotificationClicked(_)` (two places). Run `cargo check --workspace` to find any other exhaustive `match` on `Event`.

Run: `cargo test -p mkrew-daemon -j 4`
Expected: PASS.

- [ ] **Step 6: UI history reads over the protocol**

`crates/mkrew-ui/src/main.rs` `fn load_history_log`: replace the `files` request and the `tail` computation (from `let files = connection.call::<SessionHistory>(` through the end of `let tail = match files.await { … };`) with:

```rust
        let tail = connection.call::<SessionLogTail>(Request::SessionLogTail(SessionRef {
            session_id: session,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let tail = tail
                .await
                .map_err(|error| format!("Cannot read the log: {error:#}"));
```

The rest of the function (`tail.bytes`, `tail.cut`) is unchanged. Remove `let background = …` and the now-unused `SessionHistory` import, and add `SessionLogTail` to the `mkrew_protocol` import.

- [ ] **Step 7: SPEC §7, §8**

§7 requests table, after the `session.history` row:

```markdown
| `session.log_tail` | `session_id` | `{bytes (base64), cut}` — the newest 2 MiB of the session's output log as one stream (a cut stream starts after its first line break); the History dialog uses it, so it works when the window runs on another computer (ADR 0045); exited sessions too |
```

§7 events table, after `notification.clicked`:

```markdown
| `notification.show` | the daemon notified about a session (`session_id`, `project_id`, `title`, `body`, `sound`); a window connected to another computer shows it on its own computer (ADR 0045) |
```

§8 (the notification sentence; grep `notification` in §8): append `Each notice is also broadcast as notification.show (§7).`

- [ ] **Step 8: Verify and commit**

Run: `sh scripts/verify.sh` (or the three-step fallback).
Expected: green.

```bash
git add crates/mkrew-protocol/src/message.rs crates/mkrew-protocol/src/lib.rs crates/mkrew-daemon/src/session_log.rs crates/mkrew-daemon/src/server.rs crates/mkrew-daemon/src/notify.rs crates/mkrew-daemon/src/lib.rs crates/mkrew-daemon/tests/daemon.rs crates/mkrew-ui/src/history.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/sidebar.rs docs/SPEC.md
git commit -m "feat(protocol): session.log_tail and notification.show for a remote window (ADR 0045)"
```

---

### Task 3: UI client over a program; `machines.toml`

Spec: §3 lifecycle, ADR 0045 (Decision bullets 2–3).

**Current state (investigated):**
- `crates/mkrew-ui/src/client.rs`: `Connection::open(socket: &str) -> Opened` (:142) spawns the thread `mkrew-ipc`, which runs `run(socket, ready)` (:204) on a current-thread tokio runtime with `enable_io()`. `run` connects an `interprocess` tokio `Stream`, `split()`s it into `RecvHalf`/`SendHalf`, `greet` (:260), then `select!{ read_loop, write_loop }`. `greet`/`read_loop`/`write_loop` take the concrete `RecvHalf`/`SendHalf` but only use `AsyncReadExt::read` / `AsyncWriteExt::write_all`.
- `crates/mkrew-ui/src/connect.rs`: `DaemonLauncher::connect` (:96) calls `Connection::open(&self.paths.socket)`. `pub fn connect(paths)` (:136) is the startup path.
- `crates/mkrew-ui/Cargo.toml:46`: tokio features `["rt", "net", "io-util", "sync", "macros"]`. There is no `process`.
- `crates/mkrew-ui/tests/connect.rs` runs the real daemon binary found next to `CARGO_BIN_EXE_mkrew`. `mk` is built to the same folder by `cargo test --workspace`.
- `crates/mkrew-core/src/paths.rs`: `Paths { data_dir, config_dir, socket }`, with file helpers such as `settings_file()` (:99).
- `toml` 1.x is a UI dependency (`Cargo.toml:48`), and `serde` derive is available.

**Files:**
- Create: `crates/mkrew-ui/src/machines.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod machines;`)
- Modify: `crates/mkrew-ui/src/client.rs` (`Endpoint`, program transport, generic loops)
- Modify: `crates/mkrew-ui/src/connect.rs` (`Endpoint::Socket`, `connect_remote`)
- Modify: `crates/mkrew-ui/Cargo.toml` (tokio `process`)
- Modify: `crates/mkrew-core/src/paths.rs` (`machines_file()`)
- Test: `crates/mkrew-ui/src/machines.rs` tests, `crates/mkrew-ui/src/client.rs` tests, `crates/mkrew-ui/tests/connect.rs`

**Interfaces:**
- Produces in `crates/mkrew-ui/src/client.rs`: `pub enum Endpoint { Socket(String), Program(std::process::Command) }` and `pub fn Connection::open(endpoint: Endpoint) -> io::Result<anyhow::Result<Connection>>`.
- Produces in `crates/mkrew-ui/src/connect.rs`: `pub fn connect_remote(machine: &Machine) -> anyhow::Result<Connection>`.
- Produces in `crates/mkrew-ui/src/machines.rs`:
  - `pub const LOCAL_MACHINE: &str = "This computer";`
  - `pub struct Machines { pub current: Option<String>, pub list: Vec<Machine> }` (TOML key `machine` for `list`)
  - `pub struct Machine { pub ssh: String, pub mk: String }`
  - `pub enum AddError { Empty, Invalid, Duplicate(String) }` (`thiserror`, user-facing `Display`)
  - `impl Machines { pub fn current(&self) -> Option<&Machine>; pub fn add(&mut self, ssh: &str) -> Result<Machine, AddError>; pub fn remove(&mut self, ssh: &str) }`
  - `pub fn load(path: &Path) -> Result<Machines, String>`, `pub fn save(path: &Path, machines: &Machines) -> Result<(), String>`
  - `pub fn ssh_command(machine: &Machine) -> std::process::Command`, `pub fn hint(error: &str) -> Option<&'static str>`
- Produces in `mkrew-core`: `Paths::machines_file(&self) -> PathBuf` (`<config>/machines.toml`).

- [ ] **Step 1: `machines.rs` with failing tests**

Create `crates/mkrew-ui/src/machines.rs`:

```rust
//! Other computers this window can connect to (ADR 0045): `<config>/mkrew/machines.toml`,
//! read and written by the UI only. Pure apart from [`load`] / [`save`]; tested headlessly.
//!
//! ```toml
//! current = "tung@build-box"   # absent: this computer
//!
//! [[machine]]
//! ssh = "tung@build-box"       # user@host, ssh://user@host:port or a ~/.ssh/config name
//! # mk = "mk"                  # the remote `mk` command (default: `mk` on its PATH)
//! ```

use std::io;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

/// The name of the window's own computer in the machine chip and menu.
pub const LOCAL_MACHINE: &str = "This computer";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Machines {
    /// `ssh` of the machine the window connects to; `None` = this computer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    #[serde(rename = "machine", skip_serializing_if = "Vec::is_empty")]
    pub list: Vec<Machine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Machine {
    /// What `ssh` connects to; also the machine's name in the window.
    pub ssh: String,
    /// The `mk` command on that machine, as its shell reads it (a path with spaces needs its
    /// own quotes).
    #[serde(default = "default_mk", skip_serializing_if = "is_default_mk")]
    pub mk: String,
}

fn default_mk() -> String {
    "mk".to_owned()
}

fn is_default_mk(mk: &str) -> bool {
    mk == "mk"
}

/// Why a typed machine is refused (shown in the dialog).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AddError {
    #[error("Type the computer as user@host, ssh://user@host:port or a name from ~/.ssh/config.")]
    Empty,
    #[error("A machine has no spaces and does not start with '-'.")]
    Invalid,
    #[error("{0} is already in the list.")]
    Duplicate(String),
}

impl Machines {
    /// The machine the window connects to; a name no longer in the list means this computer.
    pub fn current(&self) -> Option<&Machine> {
        let ssh = self.current.as_deref()?;
        self.list.iter().find(|machine| machine.ssh == ssh)
    }

    /// Adds `ssh` (trimmed). A leading `-` would reach ssh as an option, whitespace as a
    /// second argument: both are refused.
    pub fn add(&mut self, ssh: &str) -> Result<Machine, AddError> {
        let ssh = ssh.trim();
        if ssh.is_empty() {
            return Err(AddError::Empty);
        }
        if ssh.starts_with('-') || ssh.chars().any(char::is_whitespace) {
            return Err(AddError::Invalid);
        }
        if self.list.iter().any(|machine| machine.ssh == ssh) {
            return Err(AddError::Duplicate(ssh.to_owned()));
        }
        let machine = Machine {
            ssh: ssh.to_owned(),
            mk: default_mk(),
        };
        self.list.push(machine.clone());
        Ok(machine)
    }

    /// Forgets `ssh`; when it was the current machine, the window's next start uses this
    /// computer.
    pub fn remove(&mut self, ssh: &str) {
        self.list.retain(|machine| machine.ssh != ssh);
        if self.current.as_deref() == Some(ssh) {
            self.current = None;
        }
    }
}

/// Reads `path`; a missing file means no machines (this computer).
pub fn load(path: &Path) -> Result<Machines, String> {
    match std::fs::read_to_string(path) {
        Ok(src) => toml::from_str(&src).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Machines::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Writes `machines` to `path`. A file that does not parse is left alone (it may hold the
/// user's hand edits): the error says to fix it first.
pub fn save(path: &Path, machines: &Machines) -> Result<(), String> {
    load(path).map_err(|e| format!("fix it first: {e}"))?;
    let text = toml::to_string(machines).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// `ssh` to `machine`, running its `mk proxy` (ADR 0045). Never prompts (there is no terminal
/// to answer): keys only; a first-seen host key is accepted, a changed one refused; warnings
/// stay out of stderr, so a failure shows ssh's own reason.
pub fn ssh_command(machine: &Machine) -> Command {
    let mut command = Command::new("ssh");
    command.args([
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        "StrictHostKeyChecking=accept-new",
        "-o",
        "ConnectTimeout=10",
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=3",
        "-o",
        "LogLevel=ERROR",
    ]);
    command.arg(&machine.ssh).arg(&machine.mk).arg("proxy");
    command
}

/// One line on what to do about a failed connect, for the failures people meet first.
pub fn hint(error: &str) -> Option<&'static str> {
    let error = error.to_ascii_lowercase();
    let has = |needle: &str| error.contains(needle);
    if has("permission denied") {
        Some("Set up an SSH key: run ssh-keygen here and add the .pub key to that computer (README: Use another computer).")
    } else if has("not recognized") || has("command not found") || has("mk: not found") {
        Some("Install Mkrew on that computer with mk on its PATH, or set mk = \"…\" for it in machines.toml.")
    } else if has("host key verification failed") || has("identification has changed") {
        Some("That computer's SSH host key changed: check why, then run ssh-keygen -R <host>.")
    } else if has("could not resolve")
        || has("timed out")
        || has("connection refused")
        || has("no route")
    {
        Some("Check the host name, that the computer is on, and that its SSH server runs.")
    } else if has("speaks protocol") {
        Some("Install the same Mkrew version on both computers.")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_leaves_the_default_mk_out() {
        let mut machines = Machines::default();
        machines.add("tung@box").unwrap();
        machines.current = Some("tung@box".into());
        let text = toml::to_string(&machines).unwrap();
        assert!(!text.contains("mk ="), "{text}");
        assert_eq!(toml::from_str::<Machines>(&text).unwrap(), machines);
    }

    #[test]
    fn current_is_this_computer_unless_it_is_listed() {
        let mut machines: Machines = toml::from_str(
            "current = \"gone@box\"\n[[machine]]\nssh = \"a@b\"\nmk = \"/opt/mkrew/bin/mk\"\n",
        )
        .unwrap();
        assert_eq!(machines.current(), None);
        machines.current = Some("a@b".into());
        assert_eq!(
            machines.current().map(|m| m.mk.as_str()),
            Some("/opt/mkrew/bin/mk")
        );
    }

    #[test]
    fn add_trims_and_refuses_empty_options_spaces_and_duplicates() {
        let mut machines = Machines::default();
        assert_eq!(machines.add("  tung@box ").unwrap().ssh, "tung@box");
        assert_eq!(machines.add(" "), Err(AddError::Empty));
        assert_eq!(machines.add("-oProxyCommand=calc"), Err(AddError::Invalid));
        assert_eq!(machines.add("tung@box -p 22"), Err(AddError::Invalid));
        assert_eq!(
            machines.add("tung@box"),
            Err(AddError::Duplicate("tung@box".into()))
        );
        assert_eq!(machines.list.len(), 1);
    }

    #[test]
    fn removing_the_current_machine_goes_back_to_this_computer() {
        let mut machines = Machines::default();
        machines.add("a@b").unwrap();
        machines.add("c@d").unwrap();
        machines.current = Some("a@b".into());
        machines.remove("c@d");
        assert_eq!(machines.current.as_deref(), Some("a@b"));
        machines.remove("a@b");
        assert_eq!(machines, Machines::default());
    }

    #[test]
    fn save_writes_load_reads_and_never_overwrites_a_broken_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mkrew").join("machines.toml");
        assert_eq!(load(&path).unwrap(), Machines::default());
        let mut machines = Machines::default();
        machines.add("a@b").unwrap();
        save(&path, &machines).unwrap();
        assert_eq!(load(&path).unwrap(), machines);
        std::fs::write(&path, "current = [").unwrap();
        assert!(load(&path).is_err());
        assert!(save(&path, &Machines::default()).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "current = [");
    }

    #[test]
    fn ssh_never_prompts_and_runs_the_remote_mk_proxy() {
        let machine = Machine {
            ssh: "ssh://tung@box:2222".into(),
            mk: "mk".into(),
        };
        let command = ssh_command(&machine);
        assert_eq!(command.get_program(), "ssh");
        let args: Vec<String> = (command.get_args())
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(args.iter().any(|arg| arg == "BatchMode=yes"), "{args:?}");
        assert_eq!(args[args.len() - 3..], ["ssh://tung@box:2222", "mk", "proxy"]);
    }

    #[test]
    fn hints_for_the_usual_failures() {
        let says = |error: &str, part: &str| {
            assert!(hint(error).is_some_and(|h| h.contains(part)), "{error}");
        };
        says("tung@box: Permission denied (publickey).", "ssh-keygen");
        says("'mk' is not recognized as an internal or external command", "PATH");
        says("bash: line 1: mk: command not found", "PATH");
        says("ssh: Could not resolve hostname box: No such host is known.", "host name");
        says("Host key verification failed.", "ssh-keygen -R");
        says(
            "client speaks protocol 39, daemon speaks 38; restart the daemon",
            "same Mkrew version",
        );
        assert_eq!(hint("something else"), None);
    }
}
```

Add `pub mod machines;` to `crates/mkrew-ui/src/lib.rs` (alphabetical, after `pub mod lifecycle;`).

`crates/mkrew-core/src/paths.rs`, after `prompts_dir`:

```rust
    /// Other computers the window can connect to, and the one it uses (ADR 0045); the UI's
    /// own file.
    pub fn machines_file(&self) -> PathBuf {
        self.config_dir.join("machines.toml")
    }
```

Run: `cargo test -p mkrew-ui --lib machines -j 4`
Expected: PASS. The module is pure and new, so its tests pass as soon as it compiles. If one fails, fix the code, not the test.

- [ ] **Step 2: Failing transport tests**

Append to `crates/mkrew-ui/tests/connect.rs`. Add `Endpoint` to the `mkrew_ui::client` import, and add `use mkrew_core::paths::{ENV_CONFIG_DIR, ENV_DATA_DIR, ENV_SOCKET};`.

```rust
/// `mk` built next to `mkrew` (`cargo test --workspace` builds it).
fn mk_exe() -> PathBuf {
    let exe = Path::new(env!("CARGO_BIN_EXE_mkrew"))
        .with_file_name(format!("mk{}", std::env::consts::EXE_SUFFIX));
    assert!(exe.is_file(), "{} is missing; run `cargo build -p mkrew-cli`", exe.display());
    exe
}

/// `mk proxy` for the daemon of `paths`: what `ssh <host> mk proxy` runs on the other computer.
fn proxy(paths: &Paths) -> std::process::Command {
    let mut command = std::process::Command::new(mk_exe());
    command
        .arg("proxy")
        .env(ENV_DATA_DIR, &paths.data_dir)
        .env(ENV_CONFIG_DIR, &paths.config_dir)
        .env(ENV_SOCKET, &paths.socket)
        .env(mkrew_daemon::ENV_NOTIFY, "off");
    command
}

#[test]
fn a_program_endpoint_carries_the_protocol_like_the_socket() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path());
    let _stop = StopOnDrop(paths.clone());
    let remote = Connection::open(Endpoint::Program(proxy(&paths)))
        .unwrap()
        .unwrap();
    let status: DaemonStatusResult = block_on(remote.call(Request::DaemonStatus)).unwrap();
    assert_eq!(status.protocol_version, mkrew_protocol::PROTOCOL_VERSION);
    // The daemon `mk proxy` started is the one the socket reaches.
    let local = connect(&paths);
    let again: DaemonStatusResult = block_on(local.call(Request::DaemonStatus)).unwrap();
    assert_eq!(again.pid, status.pid);
}

#[test]
fn a_program_that_fails_before_hello_explains_why() {
    let mut command = std::process::Command::new(mk_exe());
    command.arg("no-such-command");
    let error = Connection::open(Endpoint::Program(command))
        .unwrap()
        .err()
        .unwrap();
    let text = format!("{error:#}");
    assert!(text.contains("no-such-command"), "{text}");
}
```

Run: `cargo build --workspace && cargo test -p mkrew-ui --test connect -j 4`
Expected: FAIL to compile (`Endpoint` unknown).

- [ ] **Step 3: Program transport in `client.rs`**

`crates/mkrew-ui/Cargo.toml`: tokio features → `["rt", "net", "io-util", "sync", "macros", "process"]`. Update the comment: `# The IPC thread runs a single-threaded runtime (SPEC §7 client side); "process": ssh to another computer (ADR 0045).`

`client.rs` imports: change `use interprocess::local_socket::tokio::{RecvHalf, SendHalf, Stream, prelude::*};` to `use interprocess::local_socket::tokio::{Stream, prelude::*};`. Change `use tokio::io::{AsyncReadExt, AsyncWriteExt};` to `use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};`. Add `use std::process::Stdio;` and `use tokio::process::{Child, ChildStderr};`.

Add after `const HELLO_ID`:

```rust
/// Most of a transport program's stderr kept for its error message.
const STDERR_KEEP: usize = 4 * 1024;

/// Where a [`Connection`] goes.
#[derive(Debug)]
pub enum Endpoint {
    /// The daemon's local socket ([`Paths::socket`](mkrew_core::Paths)).
    Socket(String),
    /// A program that carries the protocol on its stdin/stdout: `ssh <host> mk proxy` for the
    /// daemon of another computer (ADR 0045). Its stderr explains a failed start.
    Program(std::process::Command),
}
```

Replace `Connection::open`:

```rust
    /// Connects and performs `hello` (blocking). `Err` from the outer `io::Result` means the
    /// endpoint could not be reached (no daemon listening, the program does not exist); the
    /// inner error is a failed handshake (version mismatch, or the program ended first, with
    /// what it printed to stderr).
    pub fn open(endpoint: Endpoint) -> Opened {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("mkrew-ipc".into())
            .spawn(move || run(endpoint, ready_tx))?;
        ready_rx
            .recv()
            .map_err(|_| io::Error::other("the IPC thread stopped during connect"))?
    }
```

Replace `fn run` (:203-254) with:

```rust
/// Body of the IPC thread: connect, greet, report to [`Connection::open`], then serve.
fn run(endpoint: Endpoint, ready: std::sync::mpsc::Sender<Opened>) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    runtime.block_on(async move {
        match endpoint {
            Endpoint::Socket(socket) => {
                let stream = match connect(&socket).await {
                    Ok(stream) => stream,
                    Err(e) => {
                        let _ = ready.send(Err(e));
                        return;
                    }
                };
                let (mut rx, mut tx) = stream.split();
                let mut decoder = FrameDecoder::new();
                match greet(&mut rx, &mut tx, &mut decoder).await {
                    Ok(hello) => serve(rx, tx, decoder, hello, &ready).await,
                    Err(e) => {
                        let _ = ready.send(Ok(Err(e)));
                    }
                }
            }
            Endpoint::Program(command) => run_program(command, &ready).await,
        }
    });
}

/// [`Endpoint::Program`]: the protocol over the child's stdin/stdout. The child is killed when
/// the connection ends (the window switched machine or closed).
async fn run_program(command: std::process::Command, ready: &std::sync::mpsc::Sender<Opened>) {
    let mut child = match spawn(command) {
        Ok(child) => child,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let (Some(mut rx), Some(mut tx), Some(stderr)) =
        (child.stdout.take(), child.stdin.take(), child.stderr.take())
    else {
        let _ = ready.send(Err(io::Error::other("the program has no piped stdio")));
        return;
    };
    let stderr = tokio::spawn(stderr_tail(stderr));
    let mut decoder = FrameDecoder::new();
    match greet(&mut rx, &mut tx, &mut decoder).await {
        Ok(hello) => serve(rx, tx, decoder, hello, ready).await,
        Err(error) => {
            drop((rx, tx));
            let _ = child.start_kill();
            let said = stderr.await.unwrap_or_default();
            let _ = ready.send(Ok(Err(startup_error(error, &said))));
            return;
        }
    }
    // Ended after a good start (network gone, daemon stopped): log what the program said.
    let _ = child.start_kill();
    let said = stderr.await.unwrap_or_default();
    if !said.trim().is_empty() {
        tracing::warn!("connection program: {}", said.trim());
    }
}

fn spawn(mut command: std::process::Command) -> io::Result<Child> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The window has no console: without this each connect flashes one for ssh.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    tokio::process::Command::from(command)
        .kill_on_drop(true)
        .spawn()
}

/// The last [`STDERR_KEEP`] bytes the program writes to stderr, read until it closes.
async fn stderr_tail(mut stderr: ChildStderr) -> String {
    let mut kept = Vec::new();
    let mut buf = [0u8; 1024];
    while let Ok(n) = stderr.read(&mut buf).await {
        if n == 0 {
            break;
        }
        kept.extend_from_slice(&buf[..n]);
        if kept.len() > STDERR_KEEP {
            kept.drain(..kept.len() - STDERR_KEEP);
        }
    }
    String::from_utf8_lossy(&kept).into_owned()
}

/// A failed `hello` over a program: its last stderr lines first (ssh's or the remote `mk`'s
/// own reason), then what the hello saw.
fn startup_error(error: anyhow::Error, stderr: &str) -> anyhow::Error {
    let lines: Vec<&str> = (stderr.lines())
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return error;
    }
    error.context(lines[lines.len().saturating_sub(3)..].join(" "))
}

/// Reports the greeted connection to `ready`, then serves it until either side closes.
async fn serve<R, W>(
    rx: R,
    tx: W,
    decoder: FrameDecoder,
    hello: HelloResult,
    ready: &std::sync::mpsc::Sender<Opened>,
) where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let (requests, mut outbox) = mpsc::unbounded_channel();
    let (incoming_tx, incoming) = inbox(INCOMING_FRAMES, INCOMING_BYTES);
    let connection = Connection {
        requests,
        incoming,
        next_id: AtomicU64::new(HELLO_ID + 1),
        hello,
    };
    if ready.send(Ok(Ok(connection))).is_err() {
        return;
    }
    let pending = RefCell::new(HashMap::new());
    // Each loop runs until its end closes; neither is cancelled and then resumed (racing
    // single pipe reads in `select!` loses data on Windows). Whichever ends first ends the
    // connection: the daemon went away, or the UI dropped the `Connection`.
    tokio::select! {
        _ = read_loop(rx, decoder, &pending, incoming_tx) => {}
        _ = write_loop(tx, &mut outbox, &pending) => {}
    }
    // Dropping `pending` fails every request still waiting for a reply.
}
```

Make the three loops generic. Only the signatures change; the bodies stay as they are:

```rust
async fn greet(
    rx: &mut (impl AsyncRead + Unpin),
    tx: &mut (impl AsyncWrite + Unpin),
    decoder: &mut FrameDecoder,
) -> anyhow::Result<HelloResult> {

async fn read_loop(
    mut rx: impl AsyncRead + Unpin,
    mut decoder: FrameDecoder,
    pending: &RefCell<HashMap<u64, oneshot::Sender<Outcome>>>,
    incoming: Outbox,
) -> anyhow::Result<()> {

async fn write_loop(
    mut tx: impl AsyncWrite + Unpin,
    outbox: &mut mpsc::UnboundedReceiver<Pending>,
    pending: &RefCell<HashMap<u64, oneshot::Sender<Outcome>>>,
) -> io::Result<()> {
```

Add to `client.rs` `mod tests`:

```rust
    #[test]
    fn a_failed_start_names_the_programs_last_stderr_lines() {
        let error = startup_error(
            anyhow!("the daemon closed the connection during hello"),
            "a\n\n b \r\nc\nPermission denied (publickey).\n",
        );
        assert_eq!(
            format!("{error:#}"),
            "b c Permission denied (publickey).: the daemon closed the connection during hello"
        );
        let quiet = startup_error(anyhow!("client speaks protocol 39"), " \n");
        assert_eq!(format!("{quiet:#}"), "client speaks protocol 39");
    }
```

- [ ] **Step 4: `connect.rs`**

`DaemonLauncher::connect`: `Connection::open(&self.paths.socket)` → `Connection::open(Endpoint::Socket(self.paths.socket.clone()))`. Import `crate::client::{Connection, Endpoint}` and `crate::machines::{self, Machine}`. Add after `pub fn connect`:

```rust
/// Connects to the daemon of `machine` through `ssh … mk proxy` (ADR 0045); `mk proxy` starts
/// that daemon when none runs there.
pub fn connect_remote(machine: &Machine) -> anyhow::Result<Connection> {
    match Connection::open(Endpoint::Program(machines::ssh_command(machine))) {
        Ok(handshake) => handshake.with_context(|| format!("connect to {}", machine.ssh)),
        Err(e) if e.kind() == ErrorKind::NotFound => {
            Err(e).context("ssh was not found: install the OpenSSH client")
        }
        Err(e) => Err(e).with_context(|| format!("start ssh for {}", machine.ssh)),
    }
}
```

Update every other `Connection::open(` caller (`grep -rn "Connection::open(" crates`) to `Endpoint::Socket(...)`.

- [ ] **Step 5: Run the tests**

Run: `cargo build --workspace && cargo test -p mkrew-ui --test connect -j 4 && cargo test -p mkrew-ui --lib client machines -j 4`
Expected: PASS. For `a_program_that_fails_before_hello_explains_why`, clap's `error: unrecognized subcommand 'no-such-command'` is part of the message.

- [ ] **Step 6: Verify and commit**

Run: `sh scripts/verify.sh`
Expected: green.

```bash
git add crates/mkrew-ui/src/machines.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/client.rs crates/mkrew-ui/src/connect.rs crates/mkrew-ui/Cargo.toml crates/mkrew-core/src/paths.rs crates/mkrew-ui/tests/connect.rs Cargo.lock
git commit -m "feat(ui): connect over ssh to mk proxy; machines.toml (ADR 0045)"
```

(Stage `Cargo.lock` only if the tokio feature changed it; check `git diff Cargo.lock` first and stage nothing else in it.)

---

### Task 4: Machine chip, Add machine, switch, Reconnect, notifications

Spec: §10 title bar (SPEC.md:741-757), §13 palette, §14 design, ADR 0045. UI: read `skill://mkrew-ui-design` first.

**Current state (investigated):**
- `main.rs` `Shell::new` (~:493) takes `(launched: Instant, paths: Paths, keymap: Keymap, keymap_errors: &[KeymapError], window, cx)`. At ~:602-700 it spawns the connect task: it calls `mkrew_ui::connect::connect(&paths)` on the background executor, stores `Rc<Connection>`, calls `refresh`, and drains `incoming`. When the inbox closes it sets `Status::Disconnected` and clears per-connection fields (~:654-698). `Shell` keeps `keymap`, `keymap_errors: Vec<KeymapError>` and `launched: Option<Instant>`, but not `paths`.
- `enum Status { Connecting, Connected, Disconnected, Failed(String), ListFailed(String) }` (~:197). `fn status_line` (~:8560) words them: Disconnected says "Restart Mkrew to reconnect.". `fn render_main` (~:9816) shows `message(theme, Some("Mkrew"), sentence, detail)` while there is no workspace or connection.
- Title bar: `fn render_title_bar` (~:9875); `left` = `logo(theme)`, then the workspace switcher etc. (only with a workspace). The pattern to copy is `fn render_workspace_switcher` (~:10131): a `div().id(..)` button with `.hover`/`.active`/`.tooltip`/`.on_click` setting `workspace_switcher = true`, a `menu_panel(..).on_mouse_down_out(..)` of `menu_item(..)` rows, a divider, "Add workspace…", wrapped in `deferred(anchored().snap_to_window_with_margin(px(space::S)).child(div().pt(px(space::XS)).child(panel)))`. `fn close_menus` (~:4502) resets `workspace_switcher`.
- The one-line modal pattern is the Git base menu: `base_dialog.rs` (`Key`, `key()`, `BaseDialog`), and in `main.rs`: `git_base: Option<BaseDialog>` (~:434), `open_git_base`/`close_git_base`/`git_base_key_down`/`set_git_base` (~:3366-3463), `render_git_base` (~:14860; `track_focus(&self.dialog_focus)`, `self.typing(..)` for IME, `text_field(..)`, error row, buttons, `Self::modal_shade`). It is rendered in the root (~:20658), counted in `fn modal_open` (~:4484), and has `dialog_serial` for late replies.
- Unsaved Files changes: `fn window_should_close` (~:2325) asks per file through `leave_file(project_id, then: AfterSave, ..)` (~:2281). `type AfterSave = Box<dyn FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>)>` (:208).
- GPUI 0.3.7: `Window::replace_root<E: Render>(&mut self, cx: &mut App, build: impl FnOnce(&mut Window, &mut Context<E>) -> E) -> Entity<E>` (window.rs:2242). `Context::defer_in(window, f)` (context.rs:301). `on_window_should_close` replaces the platform callback.
- Palette: `crates/mkrew-ui/src/palette.rs`. `Context` (:21; tests build it with `fn context(view)` at ~:590), `enum Extra` + `Extra::ALL: [Self; 15]` (:50-85), `title()`, `available()`. Dispatch is in `main.rs` ~:4382: `Extra::OpenSettings` is handled before the workspace-only match, which repeats it under "Handled above". The Shell builds `palette::Context { … }` at ~:4287. The test helper `extras()` (~:862) filters always-available extras.
- Notifications: `Event::NotificationClicked` → `fn notification_clicked(session, project_id, window, cx)` (~:1479). The dispatch site is ~:8500.
- The existing "hover reveals a ✕" pattern is the sidebar's "Remove from the sidebar" (~:19220: `.opacity(opacity::HIDDEN).group_hover(row_id, |s| s.opacity(opacity::FULL))` inside a row with `.group(row_id)`).

**Files:**
- Create: `crates/mkrew-ui/src/line_dialog.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod line_dialog;`)
- Modify: `crates/mkrew-ui/src/main.rs`
- Modify: `crates/mkrew-ui/src/palette.rs`
- Modify: `docs/SPEC.md` §10, §13
- Modify: `README.md` (new section)
- Modify: `docs/HUMAN_CHECKS.md` (append)
- Test: `crates/mkrew-ui/src/line_dialog.rs`, `crates/mkrew-ui/src/palette.rs` tests

**Interfaces:**
- Consumes: `machines::{Machines, Machine, LOCAL_MACHINE, load, save, hint}`, `connect::connect_remote` (Task 3); `mkrew_daemon::notify::{Notice, Notifier, OsNotifier}` and `Event::NotificationShow` (Task 2).
- Produces in `crates/mkrew-ui/src/line_dialog.rs`: `pub enum Purpose { AddMachine }` (Task 5 adds `AddWorkspace`, `LinkPlan(Ulid)`), with `title()`, `sentence()`, `placeholder()`, `action()` returning `&'static str`; `pub struct LineDialog { pub purpose: Purpose, pub text: Input, pub busy: bool, pub error: Option<String> }` with `new(purpose)`, `key(&base_dialog::Key) -> bool`, `insert(&str) -> bool`, `begin() -> Option<String>`, `failed(String)`.
- Produces in `Shell` (used by Task 5): fields `paths: Paths`, `machines: Machines`, `machine: Option<Machine>`, `machine_switcher: bool`, `line_dialog: Option<LineDialog>`, `notice_clicks: tokio::sync::mpsc::UnboundedSender<Notice>`; methods `open_line_dialog(Purpose, window, cx)`, `close_line_dialog(window, cx)`, `switch_machine(Option<String>, window, cx)`, `rebuild(window, cx)`, `settle_files(AfterSave, window, cx)`.
- Produces in palette: `Extra::SwitchMachine`, `Extra::AddMachine`, `Extra::Reconnect`, and `Context::connected: bool`.

- [ ] **Step 1: `line_dialog.rs` with failing tests**

```rust
//! A one-line dialog (ADR 0045): add a machine to connect to. Task 5 adds paths typed on the
//! connected machine. Pure and tested headlessly; `main.rs` renders it.

use crate::base_dialog::Key;
use crate::text_box::Input;

/// What the typed line is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// Another computer to connect to (`machines.toml`).
    AddMachine,
}

impl Purpose {
    pub fn title(self) -> &'static str {
        match self {
            Self::AddMachine => "Add a machine",
        }
    }

    pub fn sentence(self) -> &'static str {
        match self {
            Self::AddMachine => {
                "Mkrew connects with ssh and runs mk proxy there: that computer needs Mkrew and your SSH key."
            }
        }
    }

    pub fn placeholder(self) -> &'static str {
        match self {
            Self::AddMachine => "user@host, ssh://user@host:2222 or a ~/.ssh/config name",
        }
    }

    pub fn action(self) -> &'static str {
        match self {
            Self::AddMachine => "Connect",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineDialog {
    pub purpose: Purpose,
    pub text: Input,
    pub busy: bool,
    pub error: Option<String>,
}

impl LineDialog {
    pub fn new(purpose: Purpose) -> Self {
        Self {
            purpose,
            text: Input::default(),
            busy: false,
            error: None,
        }
    }

    /// Edits the line; `true` when it changed.
    pub fn key(&mut self, key: &Key) -> bool {
        match key {
            Key::Backspace => self.text.backspace(),
            Key::DeleteWord => self.text.delete_word(),
            Key::SelectAll => {
                self.text.select_all();
                false
            }
            Key::Insert(text) => self.insert(text),
            Key::Close | Key::Save | Key::Paste => false,
        }
    }

    /// Appends typed or pasted text, line breaks dropped. `true` when it changed.
    pub fn insert(&mut self, text: &str) -> bool {
        let line: String = text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
        self.text.insert(&line)
    }

    /// The trimmed line to act on; `None` when nothing is typed or one is in flight.
    pub fn begin(&mut self) -> Option<String> {
        let typed = self.text.trim().to_owned();
        if self.busy || typed.is_empty() {
            return None;
        }
        self.busy = true;
        self.error = None;
        Some(typed)
    }

    /// The action failed; the dialog stays open with the reason.
    pub fn failed(&mut self, error: String) {
        self.busy = false;
        self.error = Some(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_sends_the_trimmed_line_once_and_a_failure_lets_it_retry() {
        let mut dialog = LineDialog::new(Purpose::AddMachine);
        assert_eq!(dialog.begin(), None, "nothing typed");
        assert!(dialog.insert("  tung@box \r\n"));
        assert_eq!(dialog.begin().as_deref(), Some("tung@box"));
        assert_eq!(dialog.begin(), None, "one at a time");
        dialog.failed("Permission denied".into());
        assert!(!dialog.busy);
        assert_eq!(dialog.error.as_deref(), Some("Permission denied"));
        assert_eq!(dialog.begin().as_deref(), Some("tung@box"));
        assert_eq!(dialog.error, None);
    }

    #[test]
    fn the_line_takes_ctrl_backspace_and_select_all() {
        let mut dialog = LineDialog::new(Purpose::AddMachine);
        dialog.insert("tung@build-box");
        assert!(dialog.key(&Key::DeleteWord));
        dialog.key(&Key::SelectAll);
        assert!(dialog.insert("a@b"));
        assert_eq!(dialog.text, "a@b");
    }
}
```

Add `pub mod line_dialog;` to `lib.rs` (after `pub mod lifecycle;`).

Run: `cargo test -p mkrew-ui --lib line_dialog -j 4`
Expected: PASS. If `DeleteWord` on `tung@build-box` stops at a different boundary, keep the assertion that it changed and nothing else.

- [ ] **Step 2: Failing palette test**

`palette.rs`: add `connected: true,` to the test helper `fn context` and to any other full `Context { … }` literal (`grep -n "Context {" src/palette.rs`). In `fn extras`, extend the filter: `!matches!(extra, Extra::RunSetup | Extra::OpenSettings | Extra::SwitchMachine | Extra::AddMachine)`. Add:

```rust
    #[test]
    fn machine_actions_are_always_listed_and_reconnect_only_while_disconnected() {
        let listed = |context: &Context| -> Vec<Extra> {
            entries(&Keymap::defaults(), context, &[], &[], &[])
                .into_iter()
                .filter_map(|entry| match entry.action {
                    Action::Extra(
                        extra @ (Extra::SwitchMachine | Extra::AddMachine | Extra::Reconnect),
                    ) => Some(extra),
                    _ => None,
                })
                .collect()
        };
        let online = listed(&context(View::Board));
        assert!(online.contains(&Extra::SwitchMachine) && online.contains(&Extra::AddMachine));
        assert!(!online.contains(&Extra::Reconnect));
        let offline = Context {
            connected: false,
            workspace: false,
            workspaces: 0,
            tabs: 0,
            ..context(View::Board)
        };
        assert!(listed(&offline).contains(&Extra::Reconnect));
    }
```

Run: `cargo test -p mkrew-ui --lib palette -j 4`
Expected: FAIL to compile (`connected`, the new extras).

- [ ] **Step 3: Palette extras**

`palette.rs`: in `Context`, add after `run_live`:

```rust
    /// The window has a daemon connection (Reconnect is offered when not).
    pub connected: bool,
```

In `enum Extra`, add `SwitchMachine`, `AddMachine`, `Reconnect`; `ALL: [Self; 18]` with the three appended. `title()`:

```rust
            Self::SwitchMachine => "Machine: Switch machine…",
            Self::AddMachine => "Machine: Add machine…",
            Self::Reconnect => "Machine: Reconnect",
```

`available()`:

```rust
            Self::SwitchMachine | Self::AddMachine => true,
            Self::Reconnect => !context.connected,
```

`main.rs` at the `palette::Context { … }` build (~:4287): add `connected: self.connection.is_some(),`. In the palette dispatch (~:4395), next to `palette::Action::Extra(Extra::OpenSettings) => self.open_settings(cx),`, add:

```rust
            palette::Action::Extra(Extra::SwitchMachine) => {
                self.machine_switcher = true;
                cx.notify();
            }
            palette::Action::Extra(Extra::AddMachine) => {
                self.open_line_dialog(Purpose::AddMachine, window, cx);
            }
            palette::Action::Extra(Extra::Reconnect) => self.rebuild(window, cx),
```

In the inner workspace-only match, extend the "Handled above" arm: `Extra::OpenSettings | Extra::SwitchMachine | Extra::AddMachine | Extra::Reconnect => {}`. The `OpenSettings` call there becomes unreachable; keep the existing behavior by leaving `OpenSettings` handled above only.

Check whether the palette can open while disconnected (`grep -n "fn open_palette"`). If it returns early without a connection, let it open: Reconnect must be reachable.

Run: `cargo test -p mkrew-ui --lib palette -j 4`
Expected: PASS.

- [ ] **Step 4: Shell state, connect target, rebuild**

`main.rs` imports: `use mkrew_ui::line_dialog::{LineDialog, Purpose};`, `use mkrew_ui::machines::{self, LOCAL_MACHINE, Machine, Machines};`, `use mkrew_daemon::notify::{Notice, Notifier, OsNotifier};`, and `Instant` if not imported.

`struct Shell`, next to `workspace_switcher`:

```rust
    /// This computer's paths: the local socket, and where `machines.toml` lives.
    paths: Paths,
    /// `machines.toml` as read at start; the machine chip edits and saves it (ADR 0045).
    machines: Machines,
    /// The computer whose daemon this window talks to; `None` = this computer.
    machine: Option<Machine>,
    /// The title bar's machine dropdown is open.
    machine_switcher: bool,
    /// The open one-line dialog (modal).
    line_dialog: Option<LineDialog>,
    /// Clicks on notifications this window showed for another computer's daemon.
    notice_clicks: tokio::sync::mpsc::UnboundedSender<Notice>,
```

`Shell::new`, before the connect task:

```rust
        let (machines, machines_error) = match machines::load(&paths.machines_file()) {
            Ok(machines) => (machines, None),
            Err(error) => (
                Machines::default(),
                Some(format!("machines.toml has a mistake, using this computer: {error}")),
            ),
        };
        let machine = machines.current().cloned();
        let (notice_clicks, mut clicks) = tokio::sync::mpsc::unbounded_channel::<Notice>();
        cx.spawn_in(window, async move |this, cx| {
            while let Some(notice) = clicks.recv().await {
                let shown = this.update_in(cx, |shell, window, cx| {
                    shell.notification_clicked(notice.session_id, notice.project_id, window, cx);
                });
                if shown.is_err() {
                    return;
                }
            }
        })
        .detach();
```

In the connect task, replace `.spawn(async move { mkrew_ui::connect::connect(&paths) })` with:

```rust
                .spawn(async move {
                    match &target {
                        None => mkrew_ui::connect::connect(&connect_paths),
                        Some(machine) => mkrew_ui::connect::connect_remote(machine),
                    }
                })
```

with `let target = machine.clone(); let connect_paths = paths.clone();` before `cx.spawn_in`. Initialise the new fields in `Self { … }`: `paths`, `machines`, `machine`, `machine_switcher: false`, `line_dialog: None`, `notice_clicks`. Set `sidebar_error: machines_error` instead of `None`. If `paths` is used after the move, reorder or clone.

In `fn close_menus`, add `self.machine_switcher = false;`. In `fn modal_open`, add `|| self.line_dialog.is_some()`.

Replace `fn window_should_close` with:

```rust
    /// The window's close button (§2.5): unsaved Files changes ask first; the window closes
    /// once none are left.
    fn window_should_close(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> bool {
        if self.files.values().all(|view| view.unsaved().is_none()) {
            return true;
        }
        self.settle_files(Box::new(|_, window, _| window.remove_window()), window, cx);
        false
    }

    /// Asks about unsaved Files changes one file at a time, showing the workspace that holds
    /// them first so a refused save is visible; `then` runs once none are left. "Cancel" on any
    /// file stops (§2.5).
    fn settle_files(&mut self, then: AfterSave, window: &mut Window, cx: &mut Context<'_, Self>) {
        let unsaved = (self.files.values())
            .find(|view| view.unsaved().is_some())
            .map(|view| view.project_id);
        let Some(project_id) = unsaved else {
            return then(self, window, cx);
        };
        if self.workspace != Some(project_id) {
            self.activate_workspace(project_id, window, cx);
        }
        self.show_view(View::Files, window, cx);
        let next: AfterSave = Box::new(move |shell, window, cx| shell.settle_files(then, window, cx));
        self.leave_file(project_id, next, window, cx);
    }
```

Add:

```rust
    /// A fresh shell in this window, connecting to `machines.toml`'s current machine (a switch,
    /// or Reconnect; ADR 0045). The old shell goes with its connection, panes, ssh and every
    /// state of the old daemon.
    fn rebuild(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let paths = self.paths.clone();
        let keymap = self.keymap.clone();
        let errors = self.keymap_errors.clone();
        cx.defer_in(window, move |_, window, cx| {
            window.replace_root(cx, |window, cx| {
                Shell::new(Instant::now(), paths, keymap, &errors, window, cx)
            });
        });
    }

    /// Connects the window to `ssh` (`None` = this computer): unsaved Files changes are asked
    /// about first, `current` is saved in machines.toml, then the shell is rebuilt.
    fn switch_machine(
        &mut self,
        ssh: Option<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.machine_switcher = false;
        let same = self.machine.as_ref().map(|m| &m.ssh) == ssh.as_ref();
        if same && self.connection.is_some() {
            return cx.notify();
        }
        if self.files.values().any(|view| view.unsaved().is_some()) {
            let then: AfterSave =
                Box::new(move |shell, window, cx| shell.switch_machine(ssh, window, cx));
            return self.settle_files(then, window, cx);
        }
        let mut machines = self.machines.clone();
        machines.current = ssh;
        if let Err(error) = machines::save(&self.paths.machines_file(), &machines) {
            self.sidebar_error = Some(format!("Cannot save the machine list: {error}"));
            return cx.notify();
        }
        self.machines = machines;
        self.rebuild(window, cx);
    }

    /// "✕" on a machine row (not the connected one): forgets it.
    fn forget_machine(&mut self, ssh: &str, cx: &mut Context<'_, Self>) {
        let mut machines = self.machines.clone();
        machines.remove(ssh);
        match machines::save(&self.paths.machines_file(), &machines) {
            Ok(()) => self.machines = machines,
            Err(error) => self.sidebar_error = Some(format!("Cannot save the machine list: {error}")),
        }
        cx.notify();
    }
```

If `Shell` has no `keymap_errors` with that exact name or type, use the field `Shell::new` fills from `keymap_errors.to_vec()` (~:733).

- [ ] **Step 5: Status lines and Reconnect button**

`fn status_line`:

```rust
    fn status_line(&self) -> (SharedString, Option<SharedString>) {
        let target = self
            .machine
            .as_ref()
            .map_or_else(|| "the daemon".to_owned(), |machine| machine.ssh.clone());
        match &self.status {
            Status::Connecting => (format!("Connecting to {target}…").into(), None),
            Status::Connected if self.sidebar.is_empty() => (
                "No workspaces yet.".into(),
                Some("Add a project folder from the sidebar to get started.".into()),
            ),
            Status::Connected => ("Opening the workspace…".into(), None),
            Status::Disconnected => (
                format!("The connection to {target} closed.").into(),
                Some("Its agents keep running; reconnect when it is back.".into()),
            ),
            Status::Failed(error) => (
                format!("Cannot reach {target}.").into(),
                Some(match machines::hint(error) {
                    Some(hint) => format!("{error} — {hint}").into(),
                    None => error.clone().into(),
                }),
            ),
            Status::ListFailed(error) => (
                "Cannot load the workspace.".into(),
                Some(error.clone().into()),
            ),
        }
    }
```

`fn render_main`, in the no-workspace branch:

```rust
        let Some(project_id) = self.workspace.filter(|_| self.connection.is_some()) else {
            let (sentence, detail) = self.status_line();
            let reconnect = matches!(self.status, Status::Disconnected | Status::Failed(_))
                .then(|| {
                    button("reconnect", "Reconnect", theme, true)
                        .tooltip(tooltip(theme, "Connect again", ""))
                        .on_click(cx.listener(|shell, _, window, cx| shell.rebuild(window, cx)))
                });
            return message(theme, Some("Mkrew"), sentence, detail)
                .children(reconnect)
                .flex_1()
                .h_full()
                .into_any_element();
        };
```

- [ ] **Step 6: Machine chip and menu**

`fn render_title_bar`: change `.child(div().flex_none().child(logo(theme)));` to

```rust
            .child(div().flex_none().child(logo(theme)))
            .child(self.render_machine_switcher(theme, cx));
```

Add after `fn render_workspace_switcher`:

```rust
    /// The machine chip (ADR 0045): "This computer", or the connected computer with an accent
    /// ⇄, and its dropdown: this computer and each added machine (click connects; ✕ on hover
    /// forgets one that is not connected), then "Add machine…". Always shown, so a window whose
    /// machine is unreachable can still switch away.
    fn render_machine_switcher(&self, theme: &Theme, cx: &mut Context<'_, Self>) -> gpui::AnyElement {
        let hover = theme.hover;
        let (glyph, glyph_color, label) = match &self.machine {
            Some(machine) => ("⇄", theme.accent_text, machine.ssh.clone()),
            None => ("⌂", theme.text_muted, LOCAL_MACHINE.to_owned()),
        };
        let button = div()
            .id("machine-switch")
            .max_w(theme::rems_px(theme::TITLE_CHIP_MAX_WIDTH))
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(radius::SMALL))
            .cursor_pointer()
            .text_size(font_size::META)
            .text_color(rgb(theme.text_muted))
            .hover(move |style| style.bg(rgb(hover)))
            .active(|style| style.opacity(opacity::ACTIVE))
            .tooltip(tooltip(theme, "Switch machine", ""))
            .on_click(cx.listener(|shell, _, _, cx| {
                shell.machine_switcher = true;
                cx.notify();
            }))
            .child(div().flex_none().text_color(rgb(glyph_color)).child(glyph))
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(label),
            )
            .child(div().flex_none().child("▾"));
        let menu = (self.machine_switcher && !self.modal_open()).then(|| {
            let connected = self.machine.as_ref().map(|machine| machine.ssh.clone());
            let check = |on: bool| {
                on.then(|| div().text_color(rgb(theme.accent_text)).child("✓"))
            };
            let local = menu_item("machine-local".into(), theme)
                .child(div().flex_1().text_color(rgb(theme.text)).child(LOCAL_MACHINE))
                .children(check(connected.is_none()))
                .on_click(cx.listener(|shell, _, window, cx| {
                    cx.stop_propagation();
                    shell.switch_machine(None, window, cx);
                }));
            let rows = self.machines.list.iter().map(|machine| {
                let ssh = machine.ssh.clone();
                let is_connected = connected.as_deref() == Some(ssh.as_str());
                let row_id = SharedString::from(format!("machine-{ssh}"));
                let pick = ssh.clone();
                let forget = ssh.clone();
                menu_item(row_id.clone(), theme)
                    .group(row_id.clone())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(rgb(theme.text))
                            .child(ssh.clone()),
                    )
                    .children(check(is_connected))
                    .when(!is_connected, |el| {
                        el.child(
                            div()
                                .id(SharedString::from(format!("machine-forget-{ssh}")))
                                .flex_none()
                                .px(px(space::XS))
                                .rounded(px(radius::SMALL))
                                .text_color(rgb(theme.text_muted))
                                .opacity(opacity::HIDDEN)
                                .group_hover(row_id.clone(), |style| style.opacity(opacity::FULL))
                                .hover(move |style| style.bg(rgb(hover)))
                                .tooltip(tooltip(theme, "Forget this machine", ""))
                                .on_click(cx.listener(move |shell, _, _, cx| {
                                    cx.stop_propagation();
                                    shell.forget_machine(&forget, cx);
                                }))
                                .child("✕"),
                        )
                    })
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        cx.stop_propagation();
                        shell.switch_machine(Some(pick.clone()), window, cx);
                    }))
            });
            let panel = menu_panel("machine-switch-panel", theme)
                .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                    shell.machine_switcher = false;
                    cx.notify();
                }))
                .child(local)
                .children(rows)
                .child(
                    div()
                        .h(px(theme::BORDER_WIDTH))
                        .my(px(space::XS))
                        .bg(rgb(theme.border)),
                )
                .child(
                    menu_item("machine-add".into(), theme)
                        .child(div().flex_1().text_color(rgb(theme.text)).child("Add machine…"))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            cx.stop_propagation();
                            shell.machine_switcher = false;
                            shell.open_line_dialog(Purpose::AddMachine, window, cx);
                        })),
                );
            deferred(
                anchored()
                    .snap_to_window_with_margin(px(space::S))
                    .child(div().pt(px(space::XS)).child(panel)),
            )
        });
        div().child(button).children(menu).into_any_element()
    }
```

If `menu_item` sets `.hover` on the row, that does not conflict: the ✕ is its own element. If `.group()` is not available on `menu_item`'s return type, copy how the sidebar row at ~:19220 sets its group.

- [ ] **Step 7: The line dialog in the Shell**

Add next to the Git base menu methods, copying their shape:

```rust
    fn open_line_dialog(&mut self, purpose: Purpose, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.close_menus();
        self.line_dialog = Some(LineDialog::new(purpose));
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    fn close_line_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.line_dialog = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    /// Typing edits the line, Enter acts on it, Esc closes.
    fn line_dialog_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = base_dialog::key(&event.keystroke) else {
            return;
        };
        let paste = (key == base_dialog::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(dialog) = self.line_dialog.as_mut() else {
            return;
        };
        match key {
            base_dialog::Key::Close => return self.close_line_dialog(window, cx),
            base_dialog::Key::Save => return self.submit_line_dialog(window, cx),
            base_dialog::Key::Paste => {
                if let Some(text) = paste {
                    dialog.insert(&text);
                }
            }
            key => {
                dialog.key(&key);
            }
        }
        cx.notify();
    }

    /// Enter / the action button: acts on the typed line by purpose.
    fn submit_line_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(dialog) = self.line_dialog.as_mut() else {
            return;
        };
        let purpose = dialog.purpose;
        let Some(text) = dialog.begin() else {
            return;
        };
        cx.notify();
        match purpose {
            Purpose::AddMachine => {
                let mut machines = self.machines.clone();
                let saved = (machines.add(&text))
                    .map_err(|error| error.to_string())
                    .and_then(|machine| {
                        machines::save(&self.paths.machines_file(), &machines)
                            .map(|()| machine.ssh)
                    });
                match saved {
                    Ok(ssh) => {
                        self.machines = machines;
                        self.line_dialog = None;
                        self.switch_machine(Some(ssh), window, cx);
                    }
                    Err(error) => {
                        if let Some(dialog) = self.line_dialog.as_mut() {
                            dialog.failed(error);
                        }
                    }
                }
            }
        }
    }
```

Extract the error row closure from `render_git_base` into a free fn and use it in both dialogs:

```rust
/// The ✕ + message row under a dialog field.
fn dialog_error(theme: &Theme, error: String) -> gpui::Div {
    div()
        .flex()
        .gap(px(space::XS))
        .text_size(font_size::META)
        .child(div().text_color(rgb(theme.status_exited)).child("✕"))
        .child(div().text_color(rgb(theme.text)).child(error))
}
```

In `render_git_base`, replace `let error_row = |error: String| { … };` and `.children(dialog.error.clone().map(error_row))` with `.children(dialog.error.clone().map(|error| dialog_error(theme, error)))`. Then add after `render_git_base`:

```rust
    /// The one-line dialog: title, one sentence, the field, then Cancel and the action (Enter).
    fn render_line_dialog(
        &self,
        dialog: &LineDialog,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let purpose = dialog.purpose;
        let action = if dialog.busy { "Working…" } else { purpose.action() };
        let panel = div()
            .id("line-dialog")
            .w(px(theme::DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::line_dialog_key_down))
            .relative()
            .child(self.typing(&self.dialog_focus, Self::line_dialog_key_down, cx))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(purpose.title()),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child(purpose.sentence()),
            )
            .child(text_field(
                "line-dialog-text",
                &dialog.text,
                purpose.placeholder(),
                true,
                dialog.text.all_selected(),
                theme,
            ))
            .children(dialog.error.clone().map(|error| dialog_error(theme, error)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(space::S))
                    .pt(px(space::S))
                    .child(
                        button("line-dialog-cancel", "Cancel", theme, false)
                            .tooltip(tooltip(theme, "Close", "Esc"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.close_line_dialog(window, cx);
                            })),
                    )
                    .child(if dialog.busy {
                        disabled_button("line-dialog-go", action, theme, true)
                    } else {
                        button("line-dialog-go", action, theme, true)
                            .tooltip(tooltip(theme, purpose.action(), "Enter"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.submit_line_dialog(window, cx);
                            }))
                    }),
            );
        Self::modal_shade("line-dialog-shade", panel, cx)
    }
```

In the root render, next to `.children(self.git_base.as_ref().map(|dialog| self.render_git_base(dialog, &theme, cx)))`, add the same for `line_dialog` / `render_line_dialog`.

- [ ] **Step 8: Notifications from another computer**

In `fn dispatch`, next to `if let Event::NotificationClicked(clicked) = event { … }` (~:8500), add:

```rust
                // This computer's daemon shows its own; another computer's toast would appear
                // over there (ADR 0045).
                if let Event::NotificationShow(shown) = event
                    && self.machine.is_some()
                {
                    let notice = Notice {
                        session_id: shown.session_id,
                        project_id: shown.project_id,
                        title: shown.title.clone(),
                        body: shown.body.clone(),
                        sound: shown.sound,
                    };
                    OsNotifier.show(notice, self.notice_clicks.clone());
                }
```

- [ ] **Step 9: Build and smoke run**

Run: `cargo build -p mkrew-ui && cargo test -p mkrew-ui --lib -j 4`
Expected: builds, tests pass.

Smoke (this computer only; needs no SSH server). Start the app with a throwaway config dir, so the user's real `machines.toml` is not touched:

```bash
MKREW_CONFIG_DIR="$PWD/target/smoke-config" MKREW_DATA_DIR="$PWD/target/smoke-data" MKREW_SOCKET='\\.\pipe\mkrew-smoke' cargo run -p mkrew-ui
```

Check: the chip shows `⌂ This computer ▾`. Add machine… → type `-x` → "A machine has no spaces…". Type `nobody@127.0.0.1` → Connect. The window rebuilds and shows "Cannot reach nobody@127.0.0.1." with ssh's reason (connection refused, or `ssh` not found) and the hint, plus a Reconnect button. The chip shows `⇄ nobody@127.0.0.1`. Chip → This computer → the local workspaces come back. Close the app, then delete `target/smoke-config`, `target/smoke-data` and the smoke daemon (`mk daemon stop` with the same env).

- [ ] **Step 10: SPEC, README, HUMAN_CHECKS**

`docs/SPEC.md` §10, in the **Title bar** bullet: after "left to right: the Mkrew logo;", insert `the machine chip (ADR 0045: ⌂ This computer, or ⇄ <machine> in the accent color; a dropdown of this computer and the added machines — click connects, ✕ on hover forgets one that is not connected — and "Add machine…"; shown even without a workspace);`. Change "Without a workspace only the logo, the drag area and the window controls show." to "Without a workspace only the logo, the machine chip, the drag area and the window controls show.". After the bullet, add:

```markdown
- **Another computer (ADR 0045).** "Add machine…" asks for `user@host`, `ssh://user@host:port`
  or a `~/.ssh/config` name, saves it in `<config>/mkrew/machines.toml` and connects. A switch
  (or Reconnect, offered with the "Cannot reach …" / "The connection to … closed." message)
  saves the machine as `current` and rebuilds the window; unsaved Files changes are asked about
  first. A failed connect shows ssh's own reason and one hint (key, `mk` on PATH, host, version).
  While connected to another computer, its notifications show on this one.
```

§13 (the palette part; grep `palette-only`): add "Machine: Switch machine…", "Machine: Add machine…", "Machine: Reconnect" (while disconnected) to the list of palette-only actions.

`README.md`: add after the `## Daily use` section (before `## Uninstall`):

```markdown
## Use another computer

Mkrew can run its agents on another computer (Windows, Linux or macOS) while you use the window
on this one. The window talks to that computer's daemon through SSH; nothing listens on the
network except the SSH server.

On the other computer:

1. Install Mkrew (the same version as here) so `mk` is on its `PATH`, and the agents you want,
   logged in.
2. Turn on its SSH server. Windows: Settings → System → Optional features → add "OpenSSH
   Server", then in an administrator PowerShell `Start-Service sshd; Set-Service sshd
   -StartupType Automatic`. Ubuntu: `sudo apt install openssh-server`. macOS: System Settings →
   General → Sharing → Remote Login.
3. Recommended: start the daemon when you log in there (`mk daemon start` from the Startup
   folder / a login item). Otherwise the first connection starts it inside the SSH login, which
   on Windows cannot use Credential Manager (git over HTTPS) and on Linux/macOS only gets the
   `PATH` of a non-interactive shell (put `PATH` lines at the top of `~/.bashrc`, or in
   `~/.zshenv`).

On this computer:

1. Make an SSH key once: `ssh-keygen -t ed25519` (Enter for the defaults).
2. Put `~/.ssh/id_ed25519.pub` on the other computer: Linux/macOS `ssh-copy-id user@host`;
   Windows: append it to `C:\Users\<user>\.ssh\authorized_keys` (an administrator account uses
   `C:\ProgramData\ssh\administrators_authorized_keys`).
3. Check: `ssh user@host mk --version` prints the version without asking for a password.
4. In Mkrew, click the machine chip in the title bar (`⌂ This computer ▾`) → Add machine… →
   `user@host` (or `ssh://user@host:2222`, or a name from `~/.ssh/config`) → Connect.

The chip switches between this computer and the machines you added. If `mk` is not on the
other computer's `PATH`, set it in `<config>/mkrew/machines.toml` (`mk = "/opt/mkrew/bin/mk"`
under that `[[machine]]`).
```

`docs/HUMAN_CHECKS.md`, append to the remote machine section:

```markdown
- [ ] **Windows → Windows, real:** follow README "Use another computer" with a second Windows
  PC. Add machine → the window connects, the sidebar shows that PC's workspaces, + Agent starts
  an agent there (its terminal works, typing is not laggy on LAN), History opens a finished
  session's log.
- [ ] **Windows → Ubuntu, real:** same with an Ubuntu box; `omp`/`claude` there show as
  installed in the agent picker (if not: the README PATH note).
- [ ] **Switch kills ssh, agents stay:** connected to the other PC with an agent working →
  chip → This computer → Task Manager shows no `ssh.exe` left; switch back → the agent is still
  running and its pane re-attaches.
- [ ] **Unsaved Files changes on a switch:** edit a file in Files without saving → chip → another
  machine → asked Save / Don't save / Cancel; Cancel keeps the window and the edit.
- [ ] **Failures read well:** a machine with no key → "Cannot reach …" with "Permission denied
  (publickey)." and the ssh-keygen hint; a host that is off → the host hint; Reconnect retries.
- [ ] **Notifications here:** connected to the other PC, an agent there needs input → the toast
  shows on this computer; clicking it opens that pane.
- [ ] **Broken machines.toml:** write `current = [` into `<config>/mkrew/machines.toml` → start
  → this computer is used and the sidebar says machines.toml has a mistake; Add machine →
  the dialog says to fix the file first, and the file is unchanged.
- [ ] **Look:** the machine chip and its menu, and the Add machine dialog, in dark and light.
```

- [ ] **Step 11: Verify and commit**

Run: `sh scripts/verify.sh`
Expected: green. Grep the diff for hex colors and raw pixel numbers outside `theme.rs`.

```bash
git add crates/mkrew-ui/src/line_dialog.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/palette.rs docs/SPEC.md README.md docs/HUMAN_CHECKS.md
git commit -m "feat(ui): machine chip, add machine, switch and reconnect (ADR 0045)"
```

---

### Task 5: Connected to another computer: typed paths; no local-only actions

Spec: §10 (Add workspace, Files/Docs/Git "Open in Zed", terminal links), §12 plan link (ADR 0039), ADR 0045 (last Decision bullet).

**Current state (investigated):**
- `fn add_workspace` (~:4654) opens `cx.prompt_for_paths` (the OS folder picker of **this** computer), then sends `project.add { path }`; on success it does `sidebar.add_project` + `activate_workspace`. `fn pick_plan` (~:4707) picks a file the same way and calls `link_plan(session_id, Some(path))` (~:4743; a failure goes to `plan_error`). The daemon accepts an absolute or project-relative `.md` path (SPEC §7 `session.link_plan`).
- These calls open daemon-side paths on **this** computer: `zed::open` in `open_doc_in_zed` (~:1769), `open_file_in_zed` (~:2490), `open_git_file_in_zed` (~:3066), `open_in_zed` (~:4807), `open_path_in_zed` (~:4875), `open_transcript` (~:16465); `follow_wait` (~:5896) after `toggle_follow` (~:5884); and the OS file manager in `open_workspace_in_explorer` (~:4823) and `open_path_in_explorer` (~:4854). The `zed::open` calls for `keymap.toml`, `settings.toml` and the prompts folder (~:4900, :4916, :5017) are this computer's own files and stay as they are.
- Terminal links: `TerminalPane::set_links(base: Option<PathBuf>)` (`terminal/view.rs:181`); `None` = URLs only. `main.rs` ~:4624 passes `shell.link_scope(session).map(|(_, _, dir)| dir)`, and `links::resolve` canonicalizes on **this** disk.
- On a disconnect the reset block in `Shell::new`'s connect task (~:654-698) closes dialogs.

**Files:**
- Modify: `crates/mkrew-ui/src/line_dialog.rs` (`Purpose::AddWorkspace`, `Purpose::LinkPlan(Ulid)`)
- Modify: `crates/mkrew-ui/src/main.rs`
- Modify: `docs/SPEC.md` §10
- Modify: `docs/HUMAN_CHECKS.md` (append)
- Modify: `docs/INBOX.md` (tick)
- Test: `crates/mkrew-ui/src/line_dialog.rs`

**Interfaces:**
- Consumes: `Shell::{machine, line_dialog, open_line_dialog, close_line_dialog, dialog_serial}` (Task 4).
- Produces: `Purpose::AddWorkspace`, `Purpose::LinkPlan(Ulid)`; free fn `fn local_files_only(machine: Option<&Machine>) -> Result<(), String>` in `main.rs`; `Shell::workspace_added(project, window, cx)`.

- [ ] **Step 1: Purposes, with a failing test**

`line_dialog.rs`: add `use ulid::Ulid;`, update the module doc's first line to "add a machine, or — connected to another computer, where the OS file picker would browse this one — type a folder to add as a workspace or a plan file to link", and add the variants:

```rust
    /// A folder on the connected computer (`project.add`).
    AddWorkspace,
    /// A Markdown file on the connected computer, linked as this session's plan
    /// (`session.link_plan`).
    LinkPlan(Ulid),
```

Add their arms:
- `title`: `"Add workspace"`, `"Link plan"`.
- `sentence`: `"A folder on the connected computer."`, `"A Markdown file on the connected computer, absolute or relative to the workspace."`.
- `placeholder`: `"e.g. /home/you/src/api or C:\\src\\api"`, `"e.g. docs/plan.md"`.
- `action`: `"Add"`, `"Link"`.

Test:

```rust
    #[test]
    fn a_link_plan_dialog_keeps_its_session() {
        let session = Ulid::new();
        let mut dialog = LineDialog::new(Purpose::LinkPlan(session));
        dialog.insert("docs/plan.md");
        assert_eq!(dialog.begin().as_deref(), Some("docs/plan.md"));
        assert_eq!(dialog.purpose, Purpose::LinkPlan(session));
    }
```

Run: `cargo test -p mkrew-ui --lib line_dialog -j 4`
Expected: FAIL to compile first (the variants), then PASS after the arms are added. `main.rs`'s `match purpose` fails to compile until Step 2.

- [ ] **Step 2: Typed paths in the Shell**

`fn add_workspace`: right after `let Some(connection) = …` add

```rust
        if self.machine.is_some() {
            // The OS picker would browse this computer, not the connected one (ADR 0045).
            return self.open_line_dialog(Purpose::AddWorkspace, window, cx);
        }
```

In its success arm, replace the three lines (`let project_id = …; shell.sidebar_error = None; shell.sidebar.add_project(project); shell.activate_workspace(project_id, window, cx);`) with `shell.workspace_added(project, window, cx);`. Add:

```rust
    /// A workspace the daemon just added: listed and shown.
    fn workspace_added(&mut self, project: Project, window: &mut Window, cx: &mut Context<'_, Self>) {
        let project_id = project.id;
        self.sidebar_error = None;
        self.sidebar.add_project(project);
        self.activate_workspace(project_id, window, cx);
    }

    /// `project.add` with a folder typed in the line dialog; a refusal stays in the dialog.
    fn add_workspace_path(&mut self, path: String, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let serial = self.dialog_serial;
        let added = connection.call::<Project>(Request::ProjectAdd(ProjectAddParams {
            path,
            name: None,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let added = added.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let open = shell.dialog_serial == serial && shell.line_dialog.is_some();
                match added {
                    Ok(project) => {
                        if open {
                            shell.line_dialog = None;
                        }
                        shell.workspace_added(project, window, cx);
                    }
                    Err(error) => match shell.line_dialog.as_mut().filter(|_| open) {
                        Some(dialog) => dialog.failed(format!("{error:#}")),
                        None => {
                            shell.sidebar_error =
                                Some(format!("Cannot add the workspace: {error:#}"));
                        }
                    },
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// `session.link_plan` with a path typed in the line dialog; a refusal stays in the dialog.
    fn link_plan_path(
        &mut self,
        session_id: Ulid,
        path: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let serial = self.dialog_serial;
        let linked = connection.call::<Session>(Request::SessionLinkPlan(SessionLinkPlanParams {
            session_id,
            path: Some(path),
        }));
        cx.spawn_in(window, async move |this, cx| {
            let linked = linked.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let open = shell.dialog_serial == serial && shell.line_dialog.is_some();
                match linked {
                    Ok(_) if open => shell.close_line_dialog(window, cx),
                    Ok(_) => {}
                    Err(error) => match shell.line_dialog.as_mut().filter(|_| open) {
                        Some(dialog) => dialog.failed(format!("{error:#}")),
                        None => shell.plan_error = Some((session_id, format!("{error:#}"))),
                    },
                }
                cx.notify();
            });
        })
        .detach();
    }
```

`fn pick_plan`: at the top, after `self.plan_error = None;`:

```rust
        if self.machine.is_some() {
            return self.open_line_dialog(Purpose::LinkPlan(session_id), window, cx);
        }
```

`fn submit_line_dialog`: add the arms

```rust
            Purpose::AddWorkspace => self.add_workspace_path(text, window, cx),
            Purpose::LinkPlan(session_id) => self.link_plan_path(session_id, text, window, cx),
```

In the disconnect reset block (~:654-698), add `shell.line_dialog.take_if(|dialog| dialog.purpose != Purpose::AddMachine);`. Add Machine stays open: it does not need the connection.

- [ ] **Step 3: Local-only actions**

Add a free fn near `fn message`:

```rust
/// Zed and the OS file manager only see this computer's files (ADR 0045).
fn local_files_only(machine: Option<&Machine>) -> Result<(), String> {
    match machine {
        Some(machine) => Err(format!(
            "{} is another computer: Zed and the file manager only open this computer's files. Use the Files view.",
            machine.ssh
        )),
        None => Ok(()),
    }
}
```

At each daemon-path `zed::open` site, check first. The error type at these sites is `String` after `.to_string()`, so the pattern is:

```rust
let opened = local_files_only(self.machine.as_ref())
    .and_then(|()| zed::open(&full, line).map_err(|error| error.to_string()));
```

Then keep each site's own handling of `Err(error)` (the warn log, and setting `docs.zed_error` / `zed_error` / `sidebar_error`), now receiving a `String`. The sites:
- `open_doc_in_zed`: compute `let local = local_files_only(self.machine.as_ref());` **before** the `docs` borrow, then `docs.zed_error = match local.and_then(|()| zed::open(&path, None).map_err(|e| e.to_string())) { Ok(()) => None, Err(error) => { warn; Some(error) } };`.
- `open_file_in_zed`, `open_git_file_in_zed`, `open_in_zed`, `open_path_in_zed`, `open_transcript`: same pattern.
- `open_workspace_in_explorer`, `open_path_in_explorer`: at the top, `if let Err(error) = local_files_only(self.machine.as_ref()) { self.workspace_menu = None; /* or file_menu */ self.sidebar_error = Some(error); return cx.notify(); }`.
- `toggle_follow`: at the top, `if let Err(error) = local_files_only(self.machine.as_ref()) { self.sidebar_error = Some(error); return cx.notify(); }`, so following never turns on.

Terminal links, at `pane.set_links(base)` (~:4624-4628):

```rust
                // Paths resolve on this computer's disk; on another computer only URLs link.
                let base = if shell.machine.is_some() {
                    None
                } else {
                    shell.link_scope(session).map(|(_, _, dir)| dir)
                };
```

- [ ] **Step 4: Build and smoke**

Run: `cargo build -p mkrew-ui && cargo test -p mkrew-ui --lib -j 4`
Expected: builds, tests pass.

Smoke with the Task 4 command (throwaway config). Without a second computer, the remote branch cannot be reached for real. Check that this computer still behaves as before: Add workspace opens the OS folder picker, Open in Zed works, Ctrl+Click on a file path in a terminal opens it in Files. Then edit `target/smoke-config/machines.toml` to `current = "nobody@127.0.0.1"` with that `[[machine]]`, start, and confirm the Failed screen with Reconnect. The remote-only behaviors are HUMAN_CHECKS.

- [ ] **Step 5: SPEC, HUMAN_CHECKS, INBOX**

`docs/SPEC.md` §10, at the end of the "Another computer (ADR 0045)" bullet added in Task 4, append:

```markdown
  Connected to another computer: Add workspace and Link plan ask for a path on that computer
  (one-line dialog) instead of the OS picker; Open in Zed, Open in File Explorer and Follow in
  Zed answer that they only open this computer's files; terminal file links are off (URLs still
  open). Files dropped or pasted from this computer still type this computer's paths.
```

`docs/HUMAN_CHECKS.md`, append to the section:

```markdown
- [ ] **On the other computer, local-only actions:** connected to another PC → Add workspace
  asks for a path (type a folder there → it appears and opens; a wrong path → the daemon's error
  in the dialog); Link plan asks for a path; Open in Zed / Open in File Explorer / Follow in Zed
  show "… is another computer …" in the sidebar; Ctrl+Click on a URL in a terminal opens the
  browser, a file path is not underlined. Back on This computer everything works as before.
```

`docs/INBOX.md`: change this plan's line from `- [~]` to `- [x]` (keep the suffix).

- [ ] **Step 6: Verify and commit**

Run: `sh scripts/verify.sh`
Expected: green.

```bash
git add crates/mkrew-ui/src/line_dialog.rs crates/mkrew-ui/src/main.rs docs/SPEC.md docs/HUMAN_CHECKS.md docs/INBOX.md
git commit -m "feat(ui): typed paths and no local-only actions on another computer (ADR 0045)"
```

---

## Open questions (defaults used)

| Question | Default in this plan | Alternative |
| --- | --- | --- |
| Where the machine list lives | `<config>/mkrew/machines.toml`, the UI's own file | A `[[machines]]` table in `settings.toml`. Rejected: `settings.toml` is also read by the daemon with `deny_unknown_fields`, and it would sync awkwardly between computers |
| Host key policy | `StrictHostKeyChecking=accept-new` (first key trusted, a changed one refused) | `yes` (the user runs `ssh` once by hand first): safer, one more setup step |
| Zed on another computer | Refused with a message | `zed ssh://user@host/path` (Zed remote). Zed's remote server for a Windows host is not confirmed `[INFERENCE]` |
| Zed buttons while remote | Stay visible; clicking explains | Hidden or disabled at each render site (more code paths) |
| Files dropped/pasted into a remote terminal | Type this computer's paths (unchanged) | Upload through `attachment.save`, like a pasted image |
| Automatic reconnect | None; Reconnect button and palette | Retry with backoff while the message shows |
