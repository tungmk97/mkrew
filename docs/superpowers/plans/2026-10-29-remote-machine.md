# Remote machine: one window per computer over SSH - Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** From any computer, Mkrew opens one window per added machine (Windows, macOS, Ubuntu) that drives that machine's daemon over SSH, while the daemon there keeps running whatever happens to this computer.

**Architecture:**
- On the other computer `mk proxy` joins its stdin/stdout to the local daemon socket; the window runs `ssh … <host> mk proxy` as a child process and speaks the unchanged SPEC §7 frames over its pipes.
- `mkrew-ui --machine <name>` is a machine window (its own process); this computer's `mkrew-ui` spawns one per machine at startup, owns the only tray icon and the Settings → Machines page.
- Four new requests (`session.log_tail`, `session.transcript`, `fs.create`, `fs.ignore`) move the UI's last direct reads and writes of project and session files into the daemon, for every window.
- A dropped connection keeps the window's screen and re-attaches after reconnecting (both kinds of window).

**Tech Stack:** Rust 2024, GPUI (`gpui-pre`), tokio (UI IPC thread: current-thread runtime, gains the `process` feature), `interprocess` 2.4 (sync in `mk`), `rustix` 1 (Unix `setsid`), `windows-sys` 0.61, `toml` 1.x, OpenSSH client/server (user-installed; not a crate).

**Spec:** `docs/superpowers/specs/2026-10-29-remote-machine-design.md`. Read it before starting any task; section numbers below (`spec §4.3`) refer to it. `docs/SPEC.md` sections are written `SPEC §7`.

**Order:** Tasks 1 → 10 strictly. Later tasks use names produced by earlier ones (see each task's Interfaces).

## Global Constraints

- Work in the worktree `C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/remote-machine` (branch `feat/remote-machine`). Never touch the main checkout.
- Windows native is first-class: ConPTY, named pipes, `PATHEXT`, `dunce` paths (AGENTS.md).
- Every color, spacing, radius and font size in UI code comes from `crates/mkrew-ui/src/theme.rs` (SPEC §14). Grep your diff for hex colors and raw pixel numbers outside `theme.rs`.
- No `unwrap`/`expect` outside tests unless provably infallible, with a comment saying why.
- `thiserror` in libraries, `anyhow` with context in binaries, `tracing` for logs.
- No test may call a real agent, `ssh`, or the network. Program-endpoint tests use the built `mk` (`mk proxy`) found next to the test's `CARGO_BIN_EXE_*` binary, as the existing `sibling_exe` helpers do.
- Protocol: only Task 2 changes the wire format. It bumps `PROTOCOL_VERSION` in `crates/mkrew-protocol/src/lib.rs` from 49 to 50.
- The ADR is `docs/decisions/0089-remote-machine.md` (Task 1). If 0089 is taken when you start, use the next free number and rename every `0089` in this plan's changes.
- No new crates. Manifest changes allowed: tokio's `process` feature (and `time` if missing) in `crates/mkrew-ui/Cargo.toml`; `rustix` `process` feature already present in `mkrew-daemon`.
- GPUI rule: `.hover()` at most once per element (`button()`, `menu_item()` already set it).
- Commit after every task that passes verification: Conventional Commits, `git -c user.email=tungnv975@gmail.com commit`, stage only that task's files (never `git add -A`/`.`). Never push, reset, checkout, stash, rebase or clean.
- Never write outside the repo. The user's `machines.toml` and SSH setup are user steps listed in HUMAN_CHECKS.
- HUMAN_CHECKS: Task 1 creates the section `### To verify - 2026-10-29 remote machine (plan docs/superpowers/plans/2026-10-29-remote-machine.md)` directly above the first existing `### To verify` heading in `docs/HUMAN_CHECKS.md`. Later tasks append bullets to the end of that section.
- Verify with `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` from the worktree root (the omp `bash` is WSL: never use it for cargo). If rustc crashes on mkrew-ui test builds (STATUS_STACK_BUFFER_OVERRUN), run in order: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`.
- Fast loops: `cargo test -p <crate> <filter> -j 4`. UI integration tests need the bins built: `cargo build -p mkrew-daemon -p mkrew-cli -p mkrew-fakeagent` first.
- C: is nearly full (about 29 GB free): no `cargo clean`, no extra target dirs (`CARGO_TARGET_DIR` stays default).
- An `.exe` that is "Access is denied": rename it to `*.locked-old.exe`. Never kill the user's daemon, UI or sessions.
- `crates/mkrew-ui/src/main.rs` and `crates/mkrew-ui/src/shell/*.rs` are large: read ranges and find functions by name with grep. Line numbers in this plan are from 2026-10-29 and drift.
- GUI cannot be tested headless: pure logic gets unit tests; every visible change gets a HUMAN_CHECKS bullet.
- Long Markdown: one sentence per physical line. Never use the em dash character; use `-`.
- Nothing in this plan was compiled while it was written. Where code names something in the codebase, check it against the code (name, field type) and adjust; do not invent a second convention.

## Review Focus

1. **`mk proxy` writes anything but frames to stdout** (a `println!` from the shared daemon start, a tracing line). Expected: the window's `hello` decodes. Pinned by Task 1 `proxy_starts_the_daemon_and_carries_frames_until_stdin_closes` (it decodes every byte of stdout as frames).
2. **ssh fails before `hello`** (no key, `mk` not on the remote `PATH`, host offline, host key changed). Expected: "Cannot reach <name>." plus ssh's last stderr lines plus one hint, never a bare "connection closed". Pinned by Task 5 `a_program_that_exits_before_hello_is_unreachable_with_its_stderr` and Task 4 `hints_match_the_usual_ssh_failures`.
3. **A typed host or mk path that is really an ssh option** (`-oProxyCommand=…`, text with spaces). Expected: refused in the Add form and never passed to ssh. Pinned by Task 4 `validation_refuses_options_spaces_duplicates_and_the_reserved_name` and `ssh_args_put_the_double_dash_before_the_host`.
4. **The connection drops while panes are shown.** Expected: the screen stays, input is blocked, the same workspace comes back and panes re-attach. Pinned by Task 7 `reconnect_keeps_the_shown_workspace` (pure choice function) and Task 5 `killing_the_program_closes_the_inbox`; the visual part is a HUMAN_CHECKS bullet.
5. **Large history or transcript read over the wire.** Expected: at most 4 MiB arrive, byte-identical to the newest bytes on disk, under the 16 MiB frame cap after base64, with `cut` / `start` telling the reader what was skipped. Pinned by Task 2 `session_log_tail_returns_the_newest_bytes_across_rotated_files` and `session_transcript_reads_the_window_then_appends_and_resets_on_rewrite`.

Also pinned inside tasks: a hand-edited broken `machines.toml` is never overwritten (Task 4), old `mkrew://` links without `machine` still parse (Task 4), a machine window launched with an unknown name (Task 6), Quit kills every machine window (Task 8, HUMAN_CHECKS).

---

### Task 1: `mk proxy`, daemon start that survives SSH, ADR 0089

Spec: §4.1, §7, §8 (daemon env), §10.

**Current state (investigated):**
- `crates/mkrew-cli/src/daemon.rs`: `try_connect(paths) -> anyhow::Result<Option<Client>>` (lines ~20-29; the `Client` has already sent `hello` as `ClientKind::Cli`); `start(paths) -> anyhow::Result<ExitCode>` (lines ~31-82) connects, else spawns `daemon_exe()` with null stdio and `detach`, polls every `POLL` (50 ms) up to `TIMEOUT` (10 s), handles `EXIT_ALREADY_RUNNING` (lost race), and `println!`s "daemon already running (pid N)" / "daemon started (pid N)".
- `mk web` calls `if try_connect(paths)?.is_none() { crate::daemon::start(paths)?; }` (`web.rs:34-36`) and would print to stdout.
- `crates/mkrew-daemon/src/spawn.rs:39-56` `detach`: Windows `disinherit_stdio()` + `creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)`; Unix `process_group(0)`. `creation_flags` replaces, it does not OR.
- `crates/mkrew-daemon/src/ipc.rs:9` `pub fn name(socket: &str) -> io::Result<Name<'_>>`.
- `crates/mkrew-cli/src/main.rs`: `enum Command` (lines ~29-252), `run()` dispatch after `let paths = Paths::from_env()?;` (line ~279).
- `crates/mkrew-cli/tests/cli.rs`: `Env::new()`, `env.command(&[..]) -> Command` with `MKREW_*` set and `MKREW_NOTIFY=off`, `env.ok(&[..]) -> String`, `Drop` stops the daemon.
- `mkrew-cli` has the sync `interprocess::local_socket::Stream` (`client.rs:6`, `use interprocess::local_socket::{Stream, prelude::*};`).

**Files:**
- Create: `crates/mkrew-cli/src/proxy.rs`
- Create: `docs/decisions/0089-remote-machine.md`
- Modify: `crates/mkrew-cli/src/main.rs` (`mod proxy;`, `Command::Proxy`, dispatch)
- Modify: `crates/mkrew-cli/src/daemon.rs` (`Running`, `ensure_running`, `spawn_daemon`; `start` uses them)
- Modify: `crates/mkrew-cli/src/web.rs` (use `ensure_running`, no stdout lines)
- Modify: `crates/mkrew-daemon/src/spawn.rs` (`setsid` on Unix, `detach_from_job`)
- Modify: `docs/SPEC.md` §3, §9, §15, §19
- Modify: `README.md` (new section "Work on another computer")
- Modify: `docs/HUMAN_CHECKS.md` (new section)
- Test: `crates/mkrew-cli/tests/cli.rs`

**Interfaces:**
- Produces: `mk proxy` (stdout carries protocol bytes only; exit 0 when stdin or the daemon closes).
- Produces: `pub enum Running { Already(u32), Started(u32) }` and `pub fn ensure_running(paths: &Paths) -> anyhow::Result<Running>` in `crates/mkrew-cli/src/daemon.rs`.
- Produces: `pub fn detach_from_job(command: &mut std::process::Command)` in `crates/mkrew-daemon/src/spawn.rs`.

- [ ] **Step 1: Write the failing test**

Append to `crates/mkrew-cli/tests/cli.rs` (extend the `std::process` import with `ChildStdin, ChildStdout` if missing; `Read`/`Write` from `std::io`):

```rust
/// Sends `request` as frame `id` through a running `mk proxy` and returns its response.
/// Every byte the proxy writes must decode as a frame: anything else on stdout fails here.
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
    // Nothing but frames was written: the decoder holds no partial garbage.
    let mut rest = Vec::new();
    stdout.read_to_end(&mut rest).unwrap();
    decoder.push(&rest);
    while decoder.next_frame().unwrap().is_some() {}
    assert_eq!(decoder.buffered(), 0, "stray bytes on stdout");
    let after = env.ok(&["daemon", "status"]);
    assert!(after.contains(&status.pid.to_string()), "{after}");
}

#[test]
fn proxy_uses_a_running_daemon() {
    use mkrew_protocol::{ClientKind, FrameDecoder, HelloParams, HelloResult, PROTOCOL_VERSION, Request};
    let env = Env::new();
    env.ok(&["daemon", "start"]);
    let mut proxy = env
        .command(&["proxy"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = proxy.stdin.take().unwrap();
    let mut stdout = proxy.stdout.take().unwrap();
    let mut decoder = FrameDecoder::new();
    let hello = Request::Hello(HelloParams {
        client: ClientKind::Ui,
        protocol_version: PROTOCOL_VERSION,
    });
    let _: HelloResult = proxy_call(&mut stdin, &mut stdout, &mut decoder, 1, hello)
        .into_result()
        .unwrap();
    drop(stdin);
    assert!(proxy.wait().unwrap().success());
}
```

Check `FrameDecoder::buffered()` exists (`frame.rs`); if its name differs, use the existing accessor.

- [ ] **Step 2: Run it to make sure it fails**

Run: `cargo build -p mkrew-daemon && cargo test -p mkrew-cli --test cli proxy_ -j 4`
Expected: FAIL (`unrecognized subcommand 'proxy'`, so `proxy_call` panics "mk proxy closed its stdout").

- [ ] **Step 3: `detach` and `detach_from_job` in `crates/mkrew-daemon/src/spawn.rs`**

Replace `detach` with the version below and add `detach_from_job`. `rustix` is a Unix dependency of `mkrew-daemon` with the `process` feature; check `rustix::process::setsid` exists in the version in `Cargo.lock` (1.x has it).

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
        // A new session (and so its own process group, no controlling terminal): the hangup
        // of an SSH session or a closed terminal never reaches it (ADR 0089).
        // SAFETY: `setsid` is async-signal-safe and touches no memory of the parent.
        unsafe {
            command.pre_exec(|| rustix::process::setsid().map(drop).map_err(Into::into));
        }
    }
}

/// [`detach`], and on Windows also out of the caller's job object: Windows OpenSSH runs a
/// session in a job that ends with the session, which would stop a daemon `mk proxy` started
/// there (ADR 0089). Spawning fails with `PermissionDenied` when the job forbids breaking
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

If `pre_exec` + `setsid` returns `EPERM` because the child is already a group leader, that cannot happen after `fork` (a fresh child is never a group leader); no special case needed.
The UI's `connect.rs` daemon spawn uses `detach` too and gains `setsid` on Unix: intended.

- [ ] **Step 4: `ensure_running` in `crates/mkrew-cli/src/daemon.rs`**

Replace `start` with the items below. Imports: `std::path::Path`, `std::process::Child`, and `detach_from_job` from `mkrew_daemon::spawn`.

```rust
/// What [`ensure_running`] found: the daemon's pid.
pub enum Running {
    Already(u32),
    Started(u32),
}

/// Starts the daemon unless one answers `hello`, and waits until it does. Prints nothing:
/// `mk proxy` uses it, and its stdout carries protocol frames only.
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

Keep whatever the old `start` did that this drops (for example an env var passed to the child): compare before deleting.
In `web.rs`, replace `if try_connect(paths)?.is_none() { crate::daemon::start(paths)?; }` with `crate::daemon::ensure_running(paths)?;` (the start line went to `mk web`'s stdout before; it is not needed).

- [ ] **Step 5: `crates/mkrew-cli/src/proxy.rs`**

```rust
//! `mk proxy` (ADR 0089): joins stdin/stdout to the daemon socket, starting the daemon first
//! when needed. A machine window runs it on the other computer through `ssh`; the window sends
//! `hello` itself, so the proxy only moves bytes. Nothing but daemon bytes goes to stdout.

use std::io::{self, Read, Write};
use std::process::ExitCode;

use anyhow::Context;
use interprocess::local_socket::{Stream, prelude::*};
use mkrew_core::Paths;

const CHUNK: usize = 64 * 1024;

pub fn run(paths: &Paths) -> anyhow::Result<ExitCode> {
    crate::daemon::ensure_running(paths)?;
    let stream = Stream::connect(mkrew_daemon::ipc::name(&paths.socket)?)
        .with_context(|| format!("connect to {}", paths.socket))?;
    let (mut from_daemon, mut to_daemon) = stream.split();
    // stdin → daemon on its own thread; daemon → stdout here. Either side ending ends the
    // proxy: the process exit closes the other half.
    std::thread::Builder::new()
        .name("mk-proxy-in".into())
        .spawn(move || {
            let mut stdin = io::stdin().lock();
            let mut buf = vec![0u8; CHUNK];
            loop {
                match stdin.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if to_daemon.write_all(&buf[..n]).is_err() {
                            break;
                        }
                    }
                }
            }
            // The window (or ssh) went away.
            std::process::exit(0);
        })
        .context("start the stdin thread")?;
    let mut stdout = io::stdout().lock();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = from_daemon.read(&mut buf).context("read from the daemon")?;
        if n == 0 {
            return Ok(ExitCode::SUCCESS);
        }
        stdout.write_all(&buf[..n]).context("write to stdout")?;
        // Line-buffered stdout would hold binary frames until a newline.
        stdout.flush().context("write to stdout")?;
    }
}
```

Verify against interprocess 2.4.x: the sync `Stream` must offer `split()` returning halves that implement `Read`/`Write` (the async relay in `web/relay.rs` uses `stream.split()` on the tokio stream). If the sync `split` is missing, use `&Stream` (which implements `Read` and `Write`) shared through `Arc<Stream>` between the two threads. Do not race reads with `select!` (SPEC comment in `server.rs`: racing pipe reads loses data on Windows); two blocking threads are correct.
`std::process::exit(0)` from the stdin thread is deliberate: the main thread is blocked in a pipe read that nothing else can interrupt portably. Comment says why.

In `main.rs`: `mod proxy;`, a variant documented for `--help`:

```rust
    /// Join stdin/stdout to the daemon (starting it if needed); a machine window runs it
    /// through ssh (ADR 0089). Not for interactive use.
    #[command(hide = true)]
    Proxy,
```

and in `run()` after `Paths::from_env()`: `Command::Proxy => proxy::run(&paths),`.
`mk` must not install a tracing subscriber that writes to stdout for this command: check `main()`; if logs go to stdout anywhere, route them to stderr for `Proxy`.

- [ ] **Step 6: Run the tests**

Run: `cargo build -p mkrew-daemon && cargo test -p mkrew-cli --test cli -j 4`
Expected: PASS, including the existing `daemon start` race and `web` tests.

- [ ] **Step 7: ADR, SPEC, README, HUMAN_CHECKS**

`docs/decisions/0089-remote-machine.md` (follow `docs/decisions/0000-template.md` headings):
- Context: the user works from another computer on the home computer's workspaces (INBOX 15); the daemon must keep running when the other computer leaves.
- Options: (1) TCP listener in the daemon (rejected: network surface in the process that owns every agent, ADR 0052); (2) `mk web` relay over Tailscale (rejected by the user: the desktop UI is the client); (3) one window switching between machines (old plan 2026-10-10; rejected: the user wants every machine ready, no switching); (4) one window per machine, its own process, SSH + `mk proxy` (chosen).
- Decision: spec §2 table, one line per row.
- Consequences: `mk proxy`; Unix daemon start uses `setsid`; Windows start breaks away from the SSH job; protocol 50 with four requests used by every window; keep-screen reconnect for every window; notifications of a machine window come from that window; a daemon started by `mk proxy` inherits the SSH session's environment (keep the home daemon started by the home login).

`docs/SPEC.md`:
- §3 after the diagram paragraph: "**Another computer (ADR 0089):** a machine window (`mkrew-ui --machine <name>`) runs `ssh <host> mk proxy` and speaks §7 over its pipes; the other computer's daemon keeps its local owner-only socket. This computer's window spawns one machine window per entry of `<config>/mkrew/machines.toml` at startup."
- §9 code block: add `mk proxy                               # stdin/stdout ↔ daemon socket, for machine windows over ssh (ADR 0089)`.
- §15 security row: append "; other computers reach it only through SSH and `mk proxy` (ADR 0089)".
- §19 backlog: remove "remote workspaces over SSH;".

`README.md`: add a section "### Work on another computer" after the install sections (one sentence per line):
the other computer needs Mkrew installed, an SSH server (Windows: Settings → System → Optional features → OpenSSH Server; macOS: Remote Login; Ubuntu: `sudo apt install openssh-server`) and this computer's public key in its `authorized_keys` (`ssh-copy-id`, or append by hand; Tailscale SSH also works);
`ssh <host> mk --version` must work from this computer without a password prompt;
Linux: `~/.bashrc` often skips non-interactive shells, so set the machine's *mk path* (e.g. `~/.local/opt/mkrew/bin/mk`) in Settings → Machines;
keep Mkrew (or `mk daemon start`) running at login on the other computer so its daemon has your full `PATH` for agents;
then Settings → Machines → Add machine.

`docs/HUMAN_CHECKS.md`: create the section heading (Global Constraints) and add:

```markdown
- [ ] **mk proxy over real ssh:** from this computer run `ssh <host> mk proxy` with no daemon running there; the terminal shows binary noise, no "daemon started" text; Ctrl+C ends it; on the other computer `mk daemon status` still shows the daemon running (Windows host and Ubuntu host).
- [ ] **Daemon outlives the SSH session:** after the step above, log out of every SSH session to the other computer; `mk daemon status` there still runs (Windows: the daemon left the sshd job; Ubuntu/macOS: `setsid`).
```

- [ ] **Step 8: Verify and commit**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: green.

```bash
git add crates/mkrew-cli/src/proxy.rs crates/mkrew-cli/src/main.rs crates/mkrew-cli/src/daemon.rs crates/mkrew-cli/src/web.rs crates/mkrew-daemon/src/spawn.rs crates/mkrew-cli/tests/cli.rs docs/decisions/0089-remote-machine.md docs/SPEC.md README.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(cli): mk proxy joins stdio to the daemon for machine windows (ADR 0089)"
```

---

### Task 2: Protocol 50 - `session.log_tail`, `session.transcript`, `fs.create`, `fs.ignore`

Spec: §5.

**Current state (investigated):**
- Requests: `crates/mkrew-protocol/src/message.rs` (`#[serde(tag = "method", content = "params")] pub enum Request`, variants `#[serde(rename = "x.y")] Xy(Params)`); fs types in `crates/mkrew-protocol/src/files.rs` (`FsReadParams`, `FsWriteParams`, `FsWritten`; check where `FsRead` variants are declared and follow it). Bytes on the wire use `#[serde(with = "base64_bytes")]` (`message.rs:1218`).
- Round-trip tests: `crates/mkrew-protocol/src/message/tests.rs` (`round_trip`, `every_m1_request_round_trips` list); fs tests in `files.rs` (`Request::GitFetch(..)` list near line 688).
- Daemon: dispatch in `crates/mkrew-daemon/src/server.rs` `handle` (e.g. `Request::FsWrite(params) => (self.fs_write(params).await, After::Continue)`); `session.history` handler at ~353 uses `session_log::files(&session_log::path(&self.paths.sessions_dir(), s.id))`; fs handlers in `crates/mkrew-daemon/src/server/fs.rs` (`file_target`, `watched_target`, `outside`, `in_git_dir`, `ok`, `Outcome::error`).
- `mkrew_core::log_tail::read_tail(files: &[PathBuf], limit: u64) -> io::Result<Tail { bytes, cut }>`.
- Transcript lookup: `mkrew_daemon::transcript_path(format: &UsageFormat, native: &str) -> Option<PathBuf>` (`usage.rs:85`); the session row has `agent_profile` and the native session id (check the field name on `Session`, e.g. `native_session_id`); profiles: find how the daemon gets an `AgentProfile` by id (grep `load_dir` / `profiles` in `crates/mkrew-daemon/src`), and its `usage: Option<UsageConfig { format, .. }>`.
- `mkrew_core::UsageFormat` (`profile.rs:54`) has a private `fn parse(name)`; names `claude_jsonl`, `codex_jsonl`, `omp_jsonl`, else `Unknown(name)`.
- UI `crates/mkrew-ui/src/file_ops.rs` holds `create(root, dir, name, folder) -> Result<String, String>` and `ignore(root, rel, is_dir) -> Result<(), String>` with tests; it uses `crate::worktree_target::join_relative`.
- Daemon integration tests: `crates/mkrew-daemon/tests/daemon/` (`main.rs` has `Env`, a client, helpers; see `git.rs` there for a request test pattern).

**Files:**
- Modify: `crates/mkrew-protocol/src/lib.rs` (`PROTOCOL_VERSION = 50`)
- Modify: `crates/mkrew-protocol/src/message.rs` (two session requests + types + consts)
- Modify: `crates/mkrew-protocol/src/files.rs` (two fs requests + types), wherever fs variants live
- Modify: `crates/mkrew-protocol/src/message/tests.rs`
- Modify: `crates/mkrew-core/src/profile.rs` (`UsageFormat::parse` pub, `UsageFormat::name`)
- Create: `crates/mkrew-daemon/src/file_ops.rs` (moved from the UI with its tests; uses its own `join_relative` or the daemon's path helper)
- Modify: `crates/mkrew-daemon/src/lib.rs` (`mod file_ops;`)
- Modify: `crates/mkrew-daemon/src/server.rs` (dispatch; `session_log_tail`, `session_transcript`)
- Modify: `crates/mkrew-daemon/src/server/fs.rs` (`fs_create`, `fs_ignore`)
- Create: `crates/mkrew-daemon/tests/daemon/remote_reads.rs` (and `mod remote_reads;` in `tests/daemon/main.rs`)
- Modify: `docs/SPEC.md` §7

**Interfaces:**
- Produces (protocol):

```rust
// message.rs
#[serde(rename = "session.log_tail")]
SessionLogTail(SessionLogTailParams),
#[serde(rename = "session.transcript")]
SessionTranscript(SessionTranscriptParams),

/// Most bytes `session.log_tail` and `session.transcript` return in one reply: 4 MiB is
/// about 5.4 MiB of base64, well under the 16 MiB frame cap.
pub const READ_WINDOW: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLogTailParams {
    pub session_id: Ulid,
    /// Capped at [`READ_WINDOW`].
    pub max_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLogTail {
    /// The newest log bytes (`<id>.log.1` then `<id>.log`), base64 on the wire.
    #[serde(with = "base64_bytes")]
    pub data: Vec<u8>,
    /// Older output was left out.
    pub cut: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTranscriptParams {
    pub session_id: Ulid,
    /// Where the reader stands (0 on the first read).
    pub offset: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTranscript {
    /// A transcript file was found (profile with `[usage]`, native id known, file exists).
    pub found: bool,
    /// The profile's `[usage] format` as written in its TOML (`claude_jsonl`, …).
    pub format: String,
    /// Where `data` starts in the file.
    pub start: u64,
    /// The file's length now.
    pub len: u64,
    #[serde(with = "base64_bytes")]
    pub data: Vec<u8>,
}

// files.rs
#[serde(rename = "fs.create")]
FsCreate(FsCreateParams),
#[serde(rename = "fs.ignore")]
FsIgnore(FsIgnoreParams),

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsCreateParams {
    pub project_id: Ulid,
    #[serde(default)]
    pub worktree_task_id: Option<Ulid>,
    /// `/`-separated, relative to the root; `""` = the root.
    #[serde(default)]
    pub dir: String,
    /// May hold `/` (or `\`): missing folders on the way are created.
    pub name: String,
    pub folder: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsCreated {
    /// The new entry, `/`-separated, relative to the root.
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsIgnoreParams {
    pub project_id: Ulid,
    #[serde(default)]
    pub worktree_task_id: Option<Ulid>,
    /// `/`-separated, relative to the root.
    pub path: String,
    pub is_dir: bool,
}
```

`fs.ignore` answers `ok(&())` (JSON `null`); clients decode `()`.
- Produces (core): `pub fn UsageFormat::parse(name: &str) -> Self` (made `pub`) and `pub fn name(&self) -> &str` (inverse of `parse`; `Unknown(s)` returns `s`).

- [ ] **Step 1: Protocol round-trip tests (failing)**

In `message/tests.rs` append the four requests to the every-request list (`every_m1_request_round_trips` or the list that test keeps):

```rust
Request::SessionLogTail(SessionLogTailParams { session_id: Ulid::nil(), max_bytes: 1024 }),
Request::SessionTranscript(SessionTranscriptParams { session_id: Ulid::nil(), offset: 7 }),
Request::FsCreate(FsCreateParams {
    project_id: Ulid::nil(),
    worktree_task_id: None,
    dir: "src".into(),
    name: "a.rs".into(),
    folder: false,
}),
Request::FsIgnore(FsIgnoreParams {
    project_id: Ulid::nil(),
    worktree_task_id: None,
    path: "target".into(),
    is_dir: true,
}),
```

and one wire-shape test:

```rust
#[test]
fn log_tail_and_transcript_bytes_are_base64() {
    let tail = serde_json::to_value(SessionLogTail { data: b"hi".to_vec(), cut: true }).unwrap();
    assert_eq!(tail, json!({"data": "aGk=", "cut": true}));
    let read = SessionTranscript {
        found: true,
        format: "claude_jsonl".into(),
        start: 3,
        len: 5,
        data: b"hi".to_vec(),
    };
    let value = serde_json::to_value(&read).unwrap();
    assert_eq!(
        value,
        json!({"found": true, "format": "claude_jsonl", "start": 3, "len": 5, "data": "aGk="})
    );
    assert_eq!(serde_json::from_value::<SessionTranscript>(value).unwrap(), read);
    let wire = round_trip(&Message::Request {
        id: 3,
        request: Request::SessionLogTail(SessionLogTailParams {
            session_id: Ulid::nil(),
            max_bytes: 10,
        }),
    });
    assert_eq!(wire["method"], "session.log_tail");
}
```

Run: `cargo test -p mkrew-protocol -j 4` → FAIL (types missing).

- [ ] **Step 2: Add the types and bump the version**

Add the variants and structs above; export them as the crate already does (`pub use message::*;`, and the `files` module's re-export). `PROTOCOL_VERSION` 49 → 50.
Run: `cargo test -p mkrew-protocol -j 4` → PASS. (Other crates do not compile until their `match`es are exhaustive; that is Step 4.)

- [ ] **Step 3: Copy `file_ops` into the daemon**

Copy `crates/mkrew-ui/src/file_ops.rs` to `crates/mkrew-daemon/src/file_ops.rs` (Task 3 deletes the UI copy once the UI uses the requests; until then both exist and the workspace keeps compiling). Change the daemon copy's module doc to: "`fs.create` and `fs.ignore` (spec 2026-10-29 §5): New file, New folder and Add to .gitignore of the Files tree, done by the daemon so every window (local or another computer's) works the same."
Replace `crate::worktree_target::join_relative` with the daemon's equivalent: grep `fn join_relative` in `crates/`; if it only exists in the UI, copy its body into `file_ops.rs` as a private fn with its doc (it is a few lines) and delete it from `worktree_target.rs` in Task 3 if unused there.
Keep the existing tests; they run under `cargo test -p mkrew-daemon file_ops`.
`mod file_ops;` in `crates/mkrew-daemon/src/lib.rs` (private, `pub(crate)` fns).
The UI keeps its `mod file_ops;` until Task 3.

- [ ] **Step 4: Daemon handlers**

`server.rs` `handle`:

```rust
Request::SessionLogTail(params) => (self.session_log_tail(params).await, After::Continue),
Request::SessionTranscript(params) => (self.session_transcript(params).await, After::Continue),
Request::FsCreate(params) => (self.fs_create(params).await, After::Continue),
Request::FsIgnore(params) => (self.fs_ignore(params).await, After::Continue),
```

`session_log_tail` (put it next to the `session.history` arm, in `server.rs` or a `server/` submodule the session handlers use):

```rust
/// `session.log_tail` (spec 2026-10-29 §5): the newest log bytes, read here so a window on
/// another computer can replay them.
async fn session_log_tail(&self, params: SessionLogTailParams) -> Outcome {
    let session = match self.store().get_session(params.session_id) {
        Ok(session) => session,
        Err(e) => return Outcome::error(ErrorCode::NotFound, format!("session {}: {e}", params.session_id)),
    };
    let files = session_log::files(&session_log::path(&self.paths.sessions_dir(), session.id));
    let limit = params.max_bytes.min(READ_WINDOW);
    match tokio::task::spawn_blocking(move || mkrew_core::log_tail::read_tail(&files, limit)).await {
        Ok(Ok(tail)) => ok(&SessionLogTail { data: tail.bytes, cut: tail.cut }),
        Ok(Err(e)) => Outcome::error(ErrorCode::Internal, format!("read the log: {e}")),
        Err(e) => Outcome::error(ErrorCode::Internal, format!("session.log_tail failed: {e}")),
    }
}
```

Match how `session.history` maps `get_session` errors (it may return `Option` or a store error; use the same `NotFound` handling it uses, and the same "hidden sessions" rule from ADR 0019).

`session_transcript`: look up the session (as above), its profile's `usage.format` and its native id; locate with `crate::usage::transcript_path`; then read the window:

```rust
/// The part of a transcript `session.transcript` returns: from `offset` when the file still
/// reaches it, else from the start; the last [`READ_WINDOW`] bytes when starting from 0.
fn transcript_window(len: u64, offset: u64) -> (u64, u64) {
    let start = if offset > len { 0 } else { offset };
    let start = if start == 0 { len.saturating_sub(READ_WINDOW) } else { start };
    let end = (start + READ_WINDOW).min(len);
    (start, end)
}
```

Read `[start, end)` with `File::open` + `seek` + `read_exact` on a blocking thread.
No profile, no `[usage]`, no native id, or no file: `ok(&SessionTranscript { found: false, format, start: 0, len: 0, data: Vec::new() })` (`format` empty when unknown).
`format` is `usage.format.name()` (new `UsageFormat::name`).

`fs_create` / `fs_ignore` in `server/fs.rs`, following `fs_write`:

```rust
/// `fs.create`: New file / New folder of the Files tree, inside the watched target.
pub(super) async fn fs_create(&self, params: FsCreateParams) -> Outcome {
    let dir = if params.dir.trim().is_empty() {
        String::new()
    } else {
        match docs::relative_path(&params.dir) {
            Some((dir, _)) if !in_git_dir(&dir) => dir.trim_end_matches('/').to_owned(),
            _ => return outside("dir", &params.dir),
        }
    };
    let target = match self.watched_target(params.project_id, params.worktree_task_id, None).await {
        Ok(target) => target,
        Err(outcome) => return outcome,
    };
    let made = tokio::task::spawn_blocking(move || {
        file_ops::create(&target.dir, &dir, &params.name, params.folder)
    })
    .await;
    match made {
        Ok(Ok(path)) if in_git_dir(&path) => outside("name", &path),
        Ok(Ok(path)) => ok(&FsCreated { path }),
        Ok(Err(message)) => Outcome::error(ErrorCode::InvalidRequest, message),
        Err(e) => Outcome::error(ErrorCode::Internal, format!("fs.create failed: {e}")),
    }
}
```

The `in_git_dir` check must happen **before** creating: check `in_git_dir(&format!("{dir}/{}", params.name))` (normalize `\` to `/`) up front and return `outside` instead of the post-check shown. An existing entry is `ErrorCode::AlreadyExists` with the message from `file_ops::create` (match on `io::ErrorKind::AlreadyExists` inside `file_ops::create`: change its error type to `(ErrorKind, String)` or a small enum so the handler can map it; keep messages identical).

`fs_ignore`: `docs::relative_path(&params.path)` (else `outside`), not inside `.git`, `watched_target`, then `file_ops::ignore(&target.dir, &path, params.is_dir)` on a blocking thread; `Ok(())` → `ok(&())`; `Err(message)` → `ErrorCode::Internal`.

- [ ] **Step 5: Daemon integration tests (failing first)**

Create `crates/mkrew-daemon/tests/daemon/remote_reads.rs` using the helpers in `tests/daemon/main.rs` (read it: `Env::new()`, a connected client, `env.repo()`, project add, spawning a fake session). Tests:

```rust
//! Reads and writes a window on another computer needs from the daemon (spec 2026-10-29 §5).

use super::*;

/// `.log.1` + `.log` are joined oldest first; `max_bytes` keeps the newest; `cut` tells.
#[test]
fn session_log_tail_returns_the_newest_bytes_across_rotated_files() {
    let env = Env::new();
    // Spawn a short fake session so a session row and its log path exist (use the helper the
    // other tests use), wait until it exited, then overwrite its log files with known bytes.
    let session_id = env.spawn_exited_fake_session();
    let log = env.session_log_path(session_id); // `<data>/sessions/<id>.log`
    std::fs::write(log.with_extension("log.1"), b"old-part|").unwrap();
    std::fs::write(&log, b"new-part").unwrap();
    let all: SessionLogTail = env.call(Request::SessionLogTail(SessionLogTailParams {
        session_id,
        max_bytes: 1024,
    }));
    assert_eq!(all.data, b"old-part|new-part");
    assert!(!all.cut);
    let tail: SessionLogTail = env.call(Request::SessionLogTail(SessionLogTailParams {
        session_id,
        max_bytes: 4,
    }));
    assert_eq!(tail.data, b"part");
    assert!(tail.cut);
    let huge: SessionLogTail = env.call(Request::SessionLogTail(SessionLogTailParams {
        session_id,
        max_bytes: u64::MAX,
    }));
    assert_eq!(huge.data, b"old-part|new-part", "capped at READ_WINDOW, not an error");
}

#[test]
fn fs_create_makes_files_and_folders_and_refuses_existing_and_outside() {
    let env = Env::new();
    let (project_id, repo) = env.project_with_repo();
    let made: FsCreated = env.call(Request::FsCreate(FsCreateParams {
        project_id,
        worktree_task_id: None,
        dir: "src".into(),
        name: "a/b.rs".into(),
        folder: false,
    }));
    assert_eq!(made.path, "src/a/b.rs");
    assert!(repo.join("src/a/b.rs").is_file());
    let again = env.request(Request::FsCreate(FsCreateParams {
        project_id,
        worktree_task_id: None,
        dir: "src".into(),
        name: "a/b.rs".into(),
        folder: false,
    }));
    assert_eq!(again.error_code(), Some(ErrorCode::AlreadyExists));
    for (dir, name) in [("..", "x"), ("", "../x"), (".git", "x"), ("", ".git/hooks/x")] {
        let refused = env.request(Request::FsCreate(FsCreateParams {
            project_id,
            worktree_task_id: None,
            dir: dir.into(),
            name: name.into(),
            folder: true,
        }));
        assert_eq!(refused.error_code(), Some(ErrorCode::InvalidRequest), "{dir} {name}");
    }
    assert!(!repo.parent().unwrap().join("x").exists());
}

#[test]
fn fs_ignore_appends_once_and_keeps_line_endings() {
    let env = Env::new();
    let (project_id, repo) = env.project_with_repo();
    std::fs::write(repo.join(".gitignore"), "target\r\n").unwrap();
    for _ in 0..2 {
        let () = env.call(Request::FsIgnore(FsIgnoreParams {
            project_id,
            worktree_task_id: None,
            path: "build".into(),
            is_dir: true,
        }));
    }
    assert_eq!(
        std::fs::read_to_string(repo.join(".gitignore")).unwrap(),
        "target\r\n/build/\r\n"
    );
}
```

Replace the helper names (`spawn_exited_fake_session`, `session_log_path`, `project_with_repo`, `call`, `request`, `error_code`) with what `tests/daemon/main.rs` really offers; add small helpers there only when nothing equivalent exists.
`mod remote_reads;` in `tests/daemon/main.rs`.

- [ ] **Step 6: Transcript tests**

Unit test next to `transcript_window` (daemon crate):

```rust
#[test]
fn transcript_window_reads_the_tail_first_then_from_the_offset_and_restarts_after_a_shrink() {
    let w = READ_WINDOW;
    assert_eq!(transcript_window(10, 0), (0, 10));
    assert_eq!(transcript_window(w + 5, 0), (5, w + 5), "first look: the newest window");
    assert_eq!(transcript_window(w + 5, w + 5), (w + 5, w + 5), "nothing new");
    assert_eq!(transcript_window(3 * w, w), (w, 2 * w), "one window per reply");
    assert_eq!(transcript_window(4, 9), (0, 4), "rewritten shorter: from the start");
}
```

Integration test in `remote_reads.rs`: write a profile with `[usage] format = "claude_jsonl"` whose transcript root points into the temp dir (the daemon reads `CLAUDE_CONFIG_DIR` for Claude; set it on the daemon's environment through the test `Env` if it lets you pass env vars; otherwise test `session_transcript`'s found/not-found branches only with `found: false` for a session without a native id, and rely on the unit test for the window). Assert: a session with no native id answers `found: false` and empty data.

- [ ] **Step 7: Run**

Run: `cargo test -p mkrew-protocol -p mkrew-core -p mkrew-daemon -j 4`
Expected: PASS.

- [ ] **Step 8: SPEC §7**

Add four rows to the §7 request table (near `session.history` and the `fs.*` rows), matching the Interfaces above in the table's style; mention `READ_WINDOW` (4 MiB). Note under `session.history`: "Windows read logs with `session.log_tail`; `session.history` serves `mk web` and the CLI on the daemon's computer."

- [ ] **Step 9: Commit**

```bash
git add crates/mkrew-protocol crates/mkrew-core/src/profile.rs crates/mkrew-daemon/src/file_ops.rs crates/mkrew-daemon/src/lib.rs crates/mkrew-daemon/src/server.rs crates/mkrew-daemon/src/server/fs.rs crates/mkrew-daemon/tests/daemon docs/SPEC.md
git -c user.email=tungnv975@gmail.com commit -m "feat(daemon): session.log_tail, session.transcript, fs.create, fs.ignore (protocol 50)"
```

---

### Task 3: Windows read logs and transcripts and create files through the daemon

Spec: §5, §6.4 rows History/Thread/Files.

**Current state (investigated):**
- History: `crates/mkrew-ui/src/shell/history.rs` `load_history_log` (~317-398) calls `SessionHistory` then `mkrew_core::log_tail::read_tail(&history.files, history::TAIL_BYTES)` on the background executor, then replays into `Emulator::for_replay`. `history::TAIL_BYTES = 1 MiB` (`crates/mkrew-ui/src/history.rs:12`).
- Thread view: `crates/mkrew-ui/src/shell/thread_panel.rs` `poll_thread` (~60-116) loads profiles from `self.config_dir.join("agents")`, builds `ThreadFile::new(&format, locate)` with `mkrew_daemon::transcript_path`, and polls `file.read()` every 1 s. `crates/mkrew-ui/src/thread_view.rs` `ThreadFile { format, locate, path, reader: ThreadReader }`, `read()` → `read_appended(path)` (reset on shrink, `start_mid_file(len - WINDOW)` on a first read of a long file, then `feed` chunks), `page()`.
- Files: `crates/mkrew-ui/src/shell/workspace_actions.rs` `create_entry` (~450-489) and `ignore_file_path` (~510-535) call `file_ops::create/ignore` on `self.target_file(project_id, target, "")`.

**Files:**
- Modify: `crates/mkrew-ui/src/shell/history.rs`
- Modify: `crates/mkrew-ui/src/thread_view.rs`
- Modify: `crates/mkrew-ui/src/shell/thread_panel.rs`
- Modify: `crates/mkrew-ui/src/shell/workspace_actions.rs`
- Modify: `crates/mkrew-ui/src/worktree_target.rs` (delete what became unused, e.g. `join_relative`, if nothing else uses it)
- Delete: `crates/mkrew-ui/src/file_ops.rs` (the daemon has its copy since Task 2) and its `mod file_ops;` in `crates/mkrew-ui/src/main.rs` or `lib.rs`

**Interfaces:**
- Consumes: Task 2's `SessionLogTail(Params)`, `SessionTranscript(Params)`, `FsCreate(Params)`, `FsCreated`, `FsIgnore(Params)`, `READ_WINDOW`, `UsageFormat::parse`.
- Produces: `ThreadFile::new(format: &UsageFormat) -> Result<Self, String>`, `ThreadFile::offset(&self) -> u64`, `ThreadFile::apply(&mut self, start: u64, data: &[u8]) -> bool`, `ThreadFile::page(&self) -> Page` (unchanged).

- [ ] **Step 1: Failing unit test for `ThreadFile::apply`**

Rewrite the existing file-based tests in `thread_view.rs` (they write files and call `read()`) as byte-fed tests; keep what they assert (prompts appear, nothing new = false, rewritten shorter = start over, long file shows the tail with `truncated`):

```rust
#[test]
fn apply_feeds_contiguous_chunks_and_restarts_on_a_gap() {
    let mut file = ThreadFile::new(&UsageFormat::OmpJsonl).unwrap();
    let one = prompt("one");
    assert!(file.apply(0, one.as_bytes()));
    assert!(!file.apply(file.offset(), b""), "nothing new");
    let two = prompt("two");
    assert!(file.apply(file.offset(), two.as_bytes()));
    assert_eq!(prompts(&file), ["one", "two"]);
    // The daemon restarted from 0 (the file was rewritten shorter): start over.
    let fresh = prompt("fresh");
    assert!(file.apply(0, fresh.as_bytes()));
    assert_eq!(prompts(&file), ["fresh"]);
}

#[test]
fn a_first_chunk_from_the_middle_shows_the_tail_as_truncated() {
    let mut file = ThreadFile::new(&UsageFormat::OmpJsonl).unwrap();
    let text = prompt("new");
    assert!(file.apply(1234, text.as_bytes()));
    assert!(file.page().truncated);
}
```

Use the existing test helpers (`prompt`, `prompts`) and the format the current tests use.
Run: `cargo test -p mkrew-ui --lib thread_view -j 4` → FAIL.

- [ ] **Step 2: `ThreadFile` fed by bytes**

```rust
/// The reader of one shown session's transcript, fed with what `session.transcript` returns.
pub struct ThreadFile {
    format: UsageFormat,
    reader: ThreadReader,
}

impl ThreadFile {
    pub fn new(format: &UsageFormat) -> Result<Self, String> {
        Ok(Self { format: format.clone(), reader: ThreadReader::new(format)? })
    }

    /// Where the next `session.transcript` read starts.
    pub fn offset(&self) -> u64 {
        self.reader.offset()
    }

    /// Feeds `data`, which starts at `start` in the file; true if the turns changed. A chunk
    /// that does not continue where the reader stands (the file was rewritten, or the first
    /// look starts mid-file) starts the reader over at `start`.
    pub fn apply(&mut self, start: u64, data: &[u8]) -> bool {
        let mut changed = false;
        if start != self.reader.offset() {
            // The format was accepted in `new`.
            if let Ok(reader) = ThreadReader::new(&self.format) {
                self.reader = reader;
                changed = true;
            }
            if start > 0 {
                self.reader.start_mid_file(start);
            }
        }
        for chunk in data.chunks(CHUNK) {
            changed |= self.reader.feed(chunk);
        }
        changed
    }
}
```

Delete `WINDOW`, `locate`, `path`, `read`, `read_appended` and the `std::fs`/`Seek` imports. Check `ThreadReader::start_mid_file` sets the offset to `start` (read it in `mkrew-core/src/thread.rs`); if it does not, adapt the condition so a mid-file first chunk does not reset on every poll.
Run the Step 1 tests → PASS.

- [ ] **Step 3: `poll_thread` uses `session.transcript`**

In `thread_panel.rs`, `poll_thread` no longer reads `config_dir` or calls `mkrew_daemon::transcript_path`. Loop:

```rust
// One `session.transcript` read a second while shown; the daemon finds and reads the file, so
// this works the same for a window on another computer (spec 2026-10-29 §5).
let mut file: Option<ThreadFile> = None;
loop {
    let offset = file.as_ref().map_or(0, ThreadFile::offset);
    let read = connection.call::<SessionTranscript>(Request::SessionTranscript(
        SessionTranscriptParams { session_id: key.0, offset },
    ));
    let page = match read.await {
        Ok(read) if read.found => {
            if file.is_none() {
                file = ThreadFile::new(&UsageFormat::parse(&read.format)).ok();
            }
            match file.take() {
                Some(mut f) => {
                    let (back, page) = cx
                        .background_executor()
                        .spawn(async move {
                            let changed = f.apply(read.start, &read.data);
                            let page = changed.then(|| f.page());
                            (f, page)
                        })
                        .await;
                    file = Some(back);
                    page
                }
                None => None,
            }
        }
        // Not found yet (no native id, file not written): keep polling.
        Ok(_) => None,
        Err(error) => {
            tracing::debug!("session.transcript failed: {error:#}");
            None
        }
    };
    // … the existing `this.update(...)` block, unchanged, using `page` …
    // A reply that did not reach the end of the file: read on at once, no 1 s wait.
}
```

Take `connection` from `self.connection.clone()` at the start (return an empty task when `None`). Keep the existing "one last read after the exit" rule. When `start + data.len() < len`, skip the 1 s timer for the next iteration (track `more = read.start + read.data.len() as u64 < read.len`).
`sync_thread` no longer needs `profile`; drop the parameter if unused.
Also check `inspector.rs` `wanted_thread` still decides when to show the section; no change expected.

- [ ] **Step 4: History uses `session.log_tail`**

In `load_history_log` replace the `SessionHistory` call and the `read_tail` on the background executor with:

```rust
let tail = connection.call::<SessionLogTail>(Request::SessionLogTail(SessionLogTailParams {
    session_id: session,
    max_bytes: history::TAIL_BYTES,
}));
```

and in the spawned task replay `tail.data` (same emulator code), using `tail.cut`. The error text stays "Cannot read the log: …". Remove now-unused imports (`SessionHistory`, `SessionRef` if unused).

- [ ] **Step 5: Files New / Add to .gitignore use the daemon**

`create_entry`:

```rust
let Some(connection) = self.connection.clone() else { return };
let made = connection.call::<FsCreated>(Request::FsCreate(FsCreateParams {
    project_id,
    worktree_task_id: target,
    dir,
    name: name.to_owned(),
    folder,
}));
cx.spawn_in(window, async move |this, cx| {
    let made = made.await;
    let _ = this.update_in(cx, |shell, window, cx| match made {
        Ok(FsCreated { path }) => shell.show_created_entry(project_id, target, path, folder, window, cx),
        Err(error) => {
            tracing::warn!(%project_id, "new file / folder failed: {error:#}");
            shell.toast_error(format!("{error:#}"), cx);
        }
    });
})
.detach();
```

where `show_created_entry` holds the old tail of `create_entry` (the `files_at` check, `refresh_files`, `reveal_dir` / `reveal_file`). Check `target` is the worktree task id the Files view uses (`Option<Ulid>`), the same value `target_file` took.
`ignore_file_path`: the same pattern with `Request::FsIgnore(FsIgnoreParams { project_id, worktree_task_id: target, path: path.to_owned(), is_dir })`, `call::<()>`, then `refresh_files` on success and the toast on error.
Delete `crates/mkrew-ui/src/file_ops.rs`, its `mod` line and its `use`s. If `worktree_target::join_relative` (or other helpers) became unused, delete them and their tests; `target_file` stays (Copy path, Zed).

- [ ] **Step 6: Run**

Run: `cargo build -p mkrew-daemon -p mkrew-cli -p mkrew-fakeagent && cargo test -p mkrew-ui -j 4`
Expected: PASS. Then `cargo clippy --workspace --all-targets -- -D warnings` (dead code shows here).

- [ ] **Step 7: HUMAN_CHECKS and commit**

Append:

```markdown
- [ ] **History, Thread, New file through the daemon (this computer's window):** History of an ended session replays as before (cut notice on a long one); the session panel's Thread follows a running Claude Code session; Files → New file, New folder and Add to .gitignore work in a project and in a worktree task.
```

```bash
git add crates/mkrew-ui/src docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "refactor(ui): history, thread and new files go through the daemon"
```

---

### Task 4: Machine list, ssh arguments, hints, status lines, machine links

Spec: §4.2, §4.3, §4.4 hints, §4.5 status lines, §6.5 links.

**Files:**
- Create: `crates/mkrew-ui/src/machines.rs` (pure: no GPUI)
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod machines;`; check how modules are exported: `lib.rs` vs `main.rs` `mod`; tests in `tests/` need `lib.rs`)
- Modify: `crates/mkrew-core/src/link.rs` (machine parameter)
- Modify: callers of `session_link` / `parse_session_link` (grep: `wintoast.rs`, `main.rs`, `desktop`)

**Interfaces:**
- Produces (`mkrew_ui::machines`):

```rust
pub const MACHINES_FILE: &str = "machines.toml";
pub const THIS_COMPUTER: &str = "This computer";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Machine {
    pub name: String,
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mk: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Machines { pub list: Vec<Machine> }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field { Name, Host, Mk }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalid { pub field: Field, pub message: String }

impl Machines {
    /// `Ok(empty)` when the file is missing; `Err(text with file and line)` when broken.
    pub fn load(config_dir: &Path) -> Result<Self, String>;
    pub fn save(&self, config_dir: &Path) -> std::io::Result<()>;
    /// Trims, validates against the others, appends.
    pub fn add(&mut self, machine: Machine) -> Result<(), Invalid>;
    pub fn remove(&mut self, name: &str) -> bool;
    pub fn get(&self, name: &str) -> Option<&Machine>;
}

/// `ssh` and its arguments for `machine` (spec §4.3).
pub fn ssh_command(machine: &Machine) -> (String, Vec<String>);
/// One line of advice for ssh's stderr, if a known failure.
pub fn hint(stderr: &str, machine: &Machine) -> Option<String>;

/// What a machine window tells the launcher on its stdout, one per line (spec §4.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MachineState {
    Connecting,
    Connected,
    Reconnecting,
    Unreachable(String),
    Version(String),
    /// Set by the launcher only, when the child exited.
    Closed,
}
impl MachineState {
    pub fn line(&self) -> String;
    pub fn parse(line: &str) -> Option<Self>;
    /// "Connected", "Not reachable: …", … for the Machines page and the tray.
    pub fn label(&self) -> String;
}
```

- Produces (`mkrew_core::link`):

```rust
pub struct SessionLink { pub session: Ulid, pub project: Ulid, pub machine: Option<String> }
pub fn session_link(session: Ulid, project: Ulid, machine: Option<&str>) -> String;
pub fn parse_session_link(link: &str) -> Option<SessionLink>;
```

- [ ] **Step 1: Failing tests in `machines.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn machine(name: &str, host: &str) -> Machine {
        Machine { name: name.into(), host: host.into(), mk: None }
    }

    #[test]
    fn validation_refuses_options_spaces_duplicates_and_the_reserved_name() {
        let mut machines = Machines::default();
        machines.add(machine(" Home PC ", " home-pc ")).unwrap();
        assert_eq!(machines.list[0], machine("Home PC", "home-pc"), "trimmed");
        let refused = |m: Machine| machines.clone().add(m).unwrap_err().field;
        assert_eq!(refused(machine("", "x")), Field::Name);
        assert_eq!(refused(machine(&"n".repeat(41), "x")), Field::Name);
        assert_eq!(refused(machine("home pc", "other")), Field::Name, "case-insensitive duplicate");
        assert_eq!(refused(machine("This computer", "x")), Field::Name);
        assert_eq!(refused(machine("A", "")), Field::Host);
        assert_eq!(refused(machine("A", "-oProxyCommand=calc")), Field::Host);
        assert_eq!(refused(machine("A", "user@host evil")), Field::Host);
        let mut bad_mk = machine("A", "h");
        bad_mk.mk = Some("-x".into());
        assert_eq!(refused(bad_mk), Field::Mk);
        let mut blank_mk = machine("A", "h");
        blank_mk.mk = Some("  ".into());
        machines.add(blank_mk).unwrap();
        assert_eq!(machines.get("a").unwrap().mk, None, "blank mk = default");
    }

    #[test]
    fn save_writes_load_reads_and_a_missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Machines::load(dir.path()).unwrap(), Machines::default());
        let mut machines = Machines::default();
        machines.add(machine("Home PC", "home-pc")).unwrap();
        let mut ubuntu = machine("Ubuntu", "me@box");
        ubuntu.mk = Some("~/.local/opt/mkrew/bin/mk".into());
        machines.add(ubuntu).unwrap();
        machines.save(dir.path()).unwrap();
        assert_eq!(Machines::load(dir.path()).unwrap(), machines);
        assert!(machines.remove("home pc"));
        assert!(!machines.remove("nope"));
    }

    #[test]
    fn a_broken_file_names_the_file_and_line_and_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(MACHINES_FILE);
        for broken in [
            "[[machine]]\nname = \"A\"\nhost = \n",
            "[[machine]]\nname = \"A\"\nhost = \"-oX\"\n",
            "[[machine]]\nname = \"A\"\nhost = \"h\"\n[[machine]]\nname = \"a\"\nhost = \"h2\"\n",
        ] {
            std::fs::write(&file, broken).unwrap();
            let error = Machines::load(dir.path()).unwrap_err();
            assert!(error.contains(MACHINES_FILE), "{error}");
            assert_eq!(std::fs::read_to_string(&file).unwrap(), broken);
        }
    }

    #[test]
    fn ssh_args_put_the_double_dash_before_the_host() {
        let (program, args) = ssh_command(&machine("Home PC", "me@home-pc"));
        assert_eq!(program, "ssh");
        assert_eq!(
            args,
            [
                "-T", "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=accept-new",
                "-o", "ServerAliveInterval=15", "-o", "ServerAliveCountMax=3",
                "--", "me@home-pc", "mk", "proxy",
            ]
        );
        let mut custom = machine("U", "box");
        custom.mk = Some("/opt/mkrew/bin/mk".into());
        let (_, args) = ssh_command(&custom);
        assert_eq!(args[args.len() - 2..], ["/opt/mkrew/bin/mk", "proxy"]);
    }

    #[test]
    fn hints_match_the_usual_ssh_failures() {
        let m = machine("Home PC", "home-pc");
        let hint = |text: &str| hint(text, &m).unwrap_or_default();
        assert!(hint("me@home-pc: Permission denied (publickey).").contains("SSH key"));
        assert!(hint("Host key verification failed.").contains("ssh-keygen -R home-pc"));
        assert!(hint("@@@ WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED! @@@").contains("ssh-keygen -R"));
        assert!(hint("bash: line 1: mk: command not found").contains("mk path"));
        assert!(hint("'mk' is not recognized as an internal or external command").contains("mk path"));
        assert!(hint("ssh: Could not resolve hostname home-pc: Name or service not known").contains("offline"));
        assert!(hint("ssh: connect to host home-pc port 22: Connection timed out").contains("offline"));
        assert!(hint("ssh: connect to host home-pc port 22: Connection refused").contains("SSH server"));
        assert_eq!(hint("something else"), "");
    }

    #[test]
    fn state_lines_round_trip() {
        for state in [
            MachineState::Connecting,
            MachineState::Connected,
            MachineState::Reconnecting,
            MachineState::Unreachable("Permission denied (publickey).".into()),
            MachineState::Version("Home PC speaks protocol 51, this computer 50.".into()),
        ] {
            let line = state.line();
            assert!(!line.contains('\n'), "{line}");
            assert_eq!(MachineState::parse(&line), Some(state));
        }
        assert_eq!(
            MachineState::Unreachable("a\nb".into()).line(),
            MachineState::Unreachable("a b".into()).line(),
            "newlines folded"
        );
        assert_eq!(MachineState::parse("garbage"), None);
    }
}
```

Run: `cargo test -p mkrew-ui --lib machines -j 4` → FAIL.

- [ ] **Step 2: Implement `machines.rs`**

```rust
//! The machines a window can open (spec 2026-10-29 §4): `<config>/mkrew/machines.toml`,
//! read and written by the UI only, the ssh command for one machine, advice for ssh's usual
//! failures, and the state lines a machine window sends to the window that started it.

use std::path::Path;

use serde::{Deserialize, Serialize};

pub const MACHINES_FILE: &str = "machines.toml";
/// The name of this computer's window; no machine may take it.
pub const THIS_COMPUTER: &str = "This computer";
const MAX_NAME: usize = 40;

// (types from Interfaces)

#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default, rename = "machine")]
    machine: Vec<Machine>,
}

impl Machines {
    pub fn load(config_dir: &Path) -> Result<Self, String> {
        let path = config_dir.join(MACHINES_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        // `toml`'s error names the line and column.
        let file: File = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut machines = Self::default();
        for machine in file.machine {
            let name = machine.name.clone();
            machines
                .add(machine)
                .map_err(|e| format!("{}: machine {name:?}: {}", path.display(), e.message))?;
        }
        Ok(machines)
    }

    pub fn save(&self, config_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(config_dir)?;
        let text = toml::to_string(&File { machine: self.list.clone() })
            .map_err(std::io::Error::other)?;
        std::fs::write(config_dir.join(MACHINES_FILE), text)
    }

    pub fn add(&mut self, machine: Machine) -> Result<(), Invalid> {
        let name = machine.name.trim().to_owned();
        let host = machine.host.trim().to_owned();
        let mk = machine.mk.map(|mk| mk.trim().to_owned()).filter(|mk| !mk.is_empty());
        let invalid = |field, message: &str| Err(Invalid { field, message: message.into() });
        if name.is_empty() {
            return invalid(Field::Name, "Enter a name.");
        }
        if name.chars().count() > MAX_NAME {
            return invalid(Field::Name, "Use at most 40 characters.");
        }
        if name.eq_ignore_ascii_case(THIS_COMPUTER) || self.get(&name).is_some() {
            return invalid(Field::Name, "That name is already used.");
        }
        if host.is_empty() {
            return invalid(Field::Host, "Enter the SSH host (name, user@host or a ~/.ssh/config alias).");
        }
        if host.starts_with('-') || host.chars().any(char::is_whitespace) {
            return invalid(Field::Host, "A host has no spaces and does not start with \"-\".");
        }
        if mk.as_deref().is_some_and(|mk| mk.starts_with('-')) {
            return invalid(Field::Mk, "A path does not start with \"-\".");
        }
        self.list.push(Machine { name, host, mk });
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.list.len();
        self.list.retain(|m| !m.name.eq_ignore_ascii_case(name));
        self.list.len() != before
    }

    pub fn get(&self, name: &str) -> Option<&Machine> {
        self.list.iter().find(|m| m.name.eq_ignore_ascii_case(name))
    }
}

pub fn ssh_command(machine: &Machine) -> (String, Vec<String>) {
    let mut args: Vec<String> = [
        "-T",
        "-o", "BatchMode=yes",
        "-o", "StrictHostKeyChecking=accept-new",
        "-o", "ServerAliveInterval=15",
        "-o", "ServerAliveCountMax=3",
        "--",
    ]
    .map(String::from)
    .into();
    args.push(machine.host.clone());
    args.push(machine.mk.clone().unwrap_or_else(|| "mk".into()));
    args.push("proxy".into());
    ("ssh".into(), args)
}

pub fn hint(stderr: &str, machine: &Machine) -> Option<String> {
    let text = stderr.to_lowercase();
    let has = |needle: &str| text.contains(needle);
    let name = &machine.name;
    if has("permission denied") {
        Some(format!("Add this computer's SSH key to {name} (ssh-copy-id, or its authorized_keys)."))
    } else if has("host key verification failed") || has("remote host identification has changed") {
        Some(format!(
            "{name}'s host key changed. If that is expected, remove the old key with `ssh-keygen -R {}`.",
            machine.host
        ))
    } else if has("mk") && (has("not found") || has("not recognized") || has("no such file")) {
        Some(format!("Mkrew's `mk` is not on {name}'s PATH for SSH. Set the mk path for this machine in Settings → Machines."))
    } else if has("connection refused") {
        Some(format!("{name} answers but runs no SSH server. Start it there (OpenSSH Server, Remote Login, openssh-server)."))
    } else if has("could not resolve hostname") || has("timed out") || has("no route to host") {
        Some(format!("{name} is offline or not reachable from here (Tailscale up? Same network?)."))
    } else {
        None
    }
}
```

`MachineState::line`: `connecting`, `connected`, `reconnecting`, `unreachable <text>`, `version <text>` (text with `\r`/`\n` replaced by spaces); `Closed` has no line (the launcher sets it) - `line()` returns `"closed"` and `parse` accepts it for completeness. `label`: `Connecting…`, `Connected`, `Reconnecting…`, `Not reachable: <text>`, `Version differs: <text>`, `Window closed`.
Run Step 1 tests → PASS. Format the arrays with `cargo fmt` (it will put one per line; the assertion in the test compares values, not layout).

- [ ] **Step 3: Machine links (failing test first)**

In `link.rs` tests add:

```rust
#[test]
fn a_machine_link_round_trips_and_old_links_have_no_machine() {
    let (session, project) = (Ulid::new(), Ulid::new());
    let link = session_link(session, project, Some("Home PC & co"));
    assert_eq!(
        link,
        format!("mkrew://session/{session}?project={project}&machine=Home%20PC%20%26%20co")
    );
    let parsed = parse_session_link(&link).unwrap();
    assert_eq!((parsed.session, parsed.project), (session, project));
    assert_eq!(parsed.machine.as_deref(), Some("Home PC & co"));
    let old = parse_session_link(&format!("mkrew://session/{session}?project={project}")).unwrap();
    assert_eq!(old.machine, None);
    assert!(parse_session_link(&format!("mkrew://session/{session}?project={project}&machine=")).is_none());
    assert!(parse_session_link(&format!("mkrew://session/{session}?project={project}&machine=%zz")).is_none());
}
```

Update the old tests to the new return type (`.map(|l| (l.session, l.project))`); keep the rejected-forms list, except `…&x=1` stays rejected (only `machine` is allowed after `project`).
Implement percent-encoding of every byte outside `A-Za-z0-9-._~` as `%XX` (uppercase hex) and strict decoding (invalid `%` → `None`, decoded bytes must be UTF-8, empty → `None`). No new crate.
Update every caller (`grep -rn "session_link\|parse_session_link" crates`): the daemon's toast passes `None` (Task 9 passes the machine for machine windows); `main.rs` uses `link.session`/`link.project` (Task 6 uses `link.machine`).
Run: `cargo test -p mkrew-core link -j 4` and `cargo check --workspace` → PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/mkrew-ui/src/machines.rs crates/mkrew-ui/src/lib.rs crates/mkrew-core/src/link.rs <every caller you changed>
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): machine list, ssh command, failure hints and machine links"
```

---

### Task 5: The UI client speaks over a program's pipes

Spec: §4.3, §4.4.

**Current state (investigated):** `crates/mkrew-ui/src/client.rs`: `Connection::open(socket: &str) -> Opened` where `type Opened = io::Result<anyhow::Result<Connection>>`; `run(socket, ready)` builds a current-thread runtime (`enable_io`), `connect(socket)` → `Stream::connect(…)`, `stream.split()` → `(RecvHalf, SendHalf)`, `greet(&mut rx, &mut tx, &mut decoder)`, then `select! { read_loop(rx, …), write_loop(tx, …) }`. `greet`, `read_loop`, `write_loop` take the interprocess halves. `connect.rs` `DaemonLauncher::connect` calls `Connection::open(&self.paths.socket)`; integration tests in `crates/mkrew-ui/tests/connect.rs` use `sibling_exe` + `DaemonLauncher`.

**Files:**
- Modify: `crates/mkrew-ui/Cargo.toml` (tokio features `process`, `time`)
- Modify: `crates/mkrew-ui/src/client.rs`
- Modify: `crates/mkrew-ui/src/connect.rs` (`connect_machine`)
- Create: `crates/mkrew-ui/tests/remote.rs`

**Interfaces:**
- Consumes: Task 1 `mk proxy`; Task 4 `Machine`, `ssh_command`, `hint`.
- Produces:

```rust
// client.rs
pub enum Endpoint {
    Socket(String),
    /// A program whose stdin/stdout carry the frames (`ssh … mk proxy`).
    Program { program: String, args: Vec<String>, env: Vec<(String, String)> },
}

/// Why opening a program endpoint failed before `hello` was answered.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// The program could not be started.
    #[error("cannot run {program}: {source}")]
    Spawn { program: String, source: std::io::Error },
    /// It ended (or closed its stdout) before answering `hello`; `stderr` = its last lines.
    #[error("the connection ended before it was ready")]
    Ended { stderr: String },
    /// The daemon answered `hello` with `version_mismatch`.
    #[error("{message}")]
    Version { message: String },
}

impl Connection {
    pub fn open(endpoint: &Endpoint) -> Opened; // was `open(socket: &str)`
}

// connect.rs
/// Connects to `machine`'s daemon through `ssh … mk proxy`; never spawns anything else.
pub fn connect_machine(machine: &Machine) -> Result<Connection, MachineError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MachineError {
    /// Show the reason (ssh's last stderr lines or the spawn error) and the hint; retry later.
    Unreachable { reason: String, hint: Option<String> },
    /// Show the message; do not retry by itself.
    Version(String),
}
```

`thiserror` may not be a UI dependency: if not, implement `Display` + `std::error::Error` by hand (the UI is a binary crate; do not add a crate).
Callers of `Connection::open(&str)` become `Connection::open(&Endpoint::Socket(…))` (grep `Connection::open(` in `crates/mkrew-ui`, including `tests/`).

- [ ] **Step 1: Failing integration tests `crates/mkrew-ui/tests/remote.rs`**

Copy the helper block of `tests/connect.rs` (`sibling_exe`, `paths`, `StopOnDrop`, the `allow` attribute). Then:

```rust
/// `mk proxy` against a test daemon, as `ssh` would run it on the other computer.
fn proxy_endpoint(paths: &Paths) -> Endpoint {
    Endpoint::Program {
        program: sibling_exe("mk").to_string_lossy().into_owned(),
        args: vec!["proxy".into()],
        env: vec![
            ("MKREW_DATA_DIR".into(), paths.data_dir.to_string_lossy().into_owned()),
            ("MKREW_CONFIG_DIR".into(), paths.config_dir.to_string_lossy().into_owned()),
            ("MKREW_SOCKET".into(), paths.socket.clone()),
            ("MKREW_NOTIFY".into(), "off".into()),
        ],
    }
}

#[test]
fn a_program_endpoint_greets_answers_and_streams_events() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path());
    let _stop = StopOnDrop(paths.clone());
    let connection = Connection::open(&proxy_endpoint(&paths)).unwrap().unwrap();
    assert_eq!(connection.hello().protocol_version, PROTOCOL_VERSION);
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let inbox = connection.incoming();
    let project: Project = block_on(connection.call(Request::ProjectAdd(ProjectAddParams {
        path: repo.to_string_lossy().into_owned(),
        name: None,
        // other fields as the struct requires
    })))
    .unwrap();
    // `project.updated` (or the event `project.add` broadcasts) arrives through the pipes.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(Incoming::Event(event)) = inbox.try_recv() {
            if format!("{event:?}").contains(&project.id.to_string()) {
                break;
            }
        }
        assert!(Instant::now() < deadline, "no event through the program endpoint");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn killing_the_program_closes_the_inbox() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path());
    let _stop = StopOnDrop(paths.clone());
    let connection = Connection::open(&proxy_endpoint(&paths)).unwrap().unwrap();
    let inbox = connection.incoming();
    // Stop the daemon: the proxy's socket closes, the proxy exits, the pipes close.
    let _ = block_on(connection.request(Request::DaemonStop));
    assert!(block_on(inbox.recv()).is_err() || {
        // Drain whatever events were queued first.
        while block_on(inbox.recv()).is_ok() {}
        true
    });
}

#[test]
fn a_program_that_exits_before_hello_is_unreachable_with_its_stderr() {
    // `mk` with an unknown subcommand prints an error on stderr and exits 2: like `ssh` that
    // cannot log in, or a remote shell without `mk`.
    let endpoint = Endpoint::Program {
        program: sibling_exe("mk").to_string_lossy().into_owned(),
        args: vec!["no-such-command".into()],
        env: Vec::new(),
    };
    let error = Connection::open(&endpoint).unwrap().unwrap_err();
    match error.downcast_ref::<OpenError>() {
        Some(OpenError::Ended { stderr }) => assert!(stderr.contains("no-such-command"), "{stderr}"),
        other => panic!("{other:?} / {error:#}"),
    }
}

#[test]
fn a_missing_program_is_a_spawn_error() {
    let endpoint = Endpoint::Program {
        program: "mkrew-no-such-program".into(),
        args: Vec::new(),
        env: Vec::new(),
    };
    let error = Connection::open(&endpoint).unwrap().unwrap_err();
    assert!(matches!(error.downcast_ref::<OpenError>(), Some(OpenError::Spawn { .. })), "{error:#}");
}
```

Fix the struct fields (`ProjectAddParams`, the event that `project.add` emits) against the protocol; the point is "an event crosses the program endpoint".
Run: `cargo build -p mkrew-daemon -p mkrew-cli && cargo test -p mkrew-ui --test remote -j 4` → FAIL (no `Endpoint`).

- [ ] **Step 2: Transport-generic client**

Make `greet`, `read_loop`, `write_loop` generic:

```rust
async fn greet<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(rx: &mut R, tx: &mut W, decoder: &mut FrameDecoder) -> anyhow::Result<HelloResult>
async fn read_loop<R: AsyncRead + Unpin>(rx: R, …)
async fn write_loop<W: AsyncWrite + Unpin>(tx: W, …)
```

(`tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt}`; the interprocess tokio halves implement tokio's traits - check; if they implement `futures` traits instead, use `tokio_util::compat`... no new crates: in that case wrap the program side to the interprocess traits' family instead. Read the imports at the top of `client.rs` first.)
`greet` keeps returning a typed error for `version_mismatch`: inspect the hello `Outcome` before `into_result`; if its error code is `ErrorCode::VersionMismatch`, return `anyhow::Error::new(OpenError::Version { message })`.
`run(endpoint, ready)`:

```rust
match endpoint {
    Endpoint::Socket(socket) => {
        let stream = match connect(&socket).await { … as today … };
        let (rx, tx) = stream.split();
        serve(rx, tx, None, ready).await;
    }
    Endpoint::Program { program, args, env } => {
        let mut command = tokio::process::Command::new(&program);
        command
            .args(&args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW); // 0x0800_0000: no console window for ssh
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(source) => {
                let _ = ready.send(Ok(Err(OpenError::Spawn { program, source }.into())));
                return;
            }
        };
        // stdout/stdin/stderr are `Some` because they were piped above.
        let (Some(rx), Some(tx), Some(stderr)) = (child.stdout.take(), child.stdin.take(), child.stderr.take()) else { … };
        let tail = StderrTail::spawn(stderr); // keeps the last 20 lines (spawned task)
        serve(rx, tx, Some((child, tail)), ready).await;
    }
}
```

`serve` is the old body after `split()`: greet (on failure: if a program, wait up to 2 s for the child to exit and for the stderr task to finish, then send `OpenError::Ended { stderr: tail.text() }` unless the error is `OpenError::Version`), then the `select!`. The `Child` lives inside `serve` until the loops end, so dropping the `Connection` (which ends `write_loop`) drops the child and kills it (`kill_on_drop`).
`StderrTail`: an `Rc<RefCell<VecDeque<String>>>` filled by a `tokio::task::spawn_local` reading lines (`BufReader::lines`), or simpler: a `std::thread` reading the `ChildStderr` converted with `into_owned_fd`/handle is not portable; use a tokio task on the same runtime (`tokio::spawn` needs `Send`: `Arc<Mutex<VecDeque<String>>>`). Lines are trimmed; at most 20 kept; `text()` joins with `\n`.
`CREATE_NO_WINDOW` via `std::os::windows::process::CommandExt` works on `tokio::process::Command` (it implements `creation_flags` itself; check).
`PATHEXT`: `tokio::process::Command::new("ssh")` finds `ssh.exe` on `PATH` on Windows (Rust's `Command` appends `.exe`); no `PATHEXT` walk needed for an `.exe`.

- [ ] **Step 3: `connect_machine`**

```rust
pub fn connect_machine(machine: &Machine) -> Result<Connection, MachineError> {
    let (program, args) = crate::machines::ssh_command(machine);
    let opened = Connection::open(&Endpoint::Program { program, args, env: Vec::new() });
    let error = match opened {
        Ok(Ok(connection)) => return Ok(connection),
        Ok(Err(error)) => error,
        Err(error) => anyhow::Error::new(error),
    };
    Err(match error.downcast_ref::<OpenError>() {
        Some(OpenError::Version { message }) => MachineError::Version(message.clone()),
        Some(OpenError::Ended { stderr }) => MachineError::Unreachable {
            reason: last_lines(stderr, 3).unwrap_or_else(|| "The connection ended before it was ready.".into()),
            hint: crate::machines::hint(stderr, machine),
        },
        Some(OpenError::Spawn { source, .. }) => MachineError::Unreachable {
            reason: format!("Cannot run ssh: {source}."),
            hint: Some("Install the OpenSSH client (Windows: Settings → System → Optional features).".into()),
        },
        None => MachineError::Unreachable { reason: format!("{error:#}"), hint: None },
    })
}
```

`last_lines` keeps the last 3 non-empty lines joined with a space; `None` when empty.

- [ ] **Step 4: Run**

Run: `cargo build -p mkrew-daemon -p mkrew-cli && cargo test -p mkrew-ui --test remote --test connect -j 4`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/Cargo.toml Cargo.lock crates/mkrew-ui/src/client.rs crates/mkrew-ui/src/connect.rs crates/mkrew-ui/tests/remote.rs <callers of Connection::open>
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): connections over a program's pipes (ssh … mk proxy)"
```

(`Cargo.lock` only if the feature change touched it.)

---

### Task 6: Machine window mode

Spec: §4.5, §6.1, §6.3 (first connect, not reachable, version), §6.4 rows Phone/update/close to tray.

**Current state (investigated):**
- `crates/mkrew-ui/src/main.rs` `main()` (~155-253): `Paths::from_env()`, keymap, jump link from `std::env::args().nth(1)` (`parse_session_link`), `desktop::raise_running_instance(&paths.socket, jump)`, `cx.open_window` with `TitlebarOptions { title: Some("Mkrew".into()), … }` (~219), `Shell::new(launched, paths, keymap, &keymap_errors, jump, window, cx)`.
- `Shell` fields: `connection: Option<Rc<Connection>>`, `status: Status { Connecting, Connected, Disconnected, Failed(String), ListFailed(String) }`, `reconnect_in`, `connect_gen`, `reconnect_attempts`, `phone`, `update`.
- `shell/connection.rs`: `Shell::new` creates `Desktop::new(window, &paths.socket)` (tray), the settings/redraw ticks, the phone poll (~117-131), the update loop (~132-152), then `connect`. `start_connect` runs `connect::connect(&paths)` on the background executor; on error `Status::Failed` + `schedule_reconnect`; `status_line()` (~986) and `status_action` (~1049) render the status screen; `chrome.rs:117` renders the message screen when there is no workspace/connection.
- `desktop.rs`: `instance_key(exe, socket)`, `current_key(socket)`, `raise_running_instance(socket, jump)`; `win32.rs` `Os::new(window, socket)` starts `Tray::start(&current_key(socket))` whose hidden window class `MkrewTray.<key>` is the single-instance target; `set_tray` shows the icon (called from `shell/attention.rs` `sync_attention`).
- Title bar: `shell/chrome.rs` `render_title_bar` (logo, sidebar toggle, workspace switcher, …, `render_update_button`, `render_phone_button`).
- Close to tray: ADR 0064; grep `close_to_tray` in `crates/mkrew-ui/src`.

**Files:**
- Create: `crates/mkrew-ui/src/launch.rs` (pure argument parsing)
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod launch;`)
- Modify: `crates/mkrew-ui/src/main.rs` (parse, raise, title, `Shell::new` gets the target)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (target-aware connect, status lines, no phone/update loops in a machine window)
- Modify: `crates/mkrew-ui/src/shell/chrome.rs` (machine chip; status screen texts)
- Modify: `crates/mkrew-ui/src/desktop.rs`, `desktop/win32.rs`, `desktop/none.rs` (key from a string; tray icon optional)
- Modify: `crates/mkrew-ui/src/shell/attention.rs` (no tray icon in a machine window)
- Modify: `crates/mkrew-ui/src/theme.rs` (chip tokens if new values are needed)
- Modify: the close-to-tray code path (machine window closes)

**Interfaces:**
- Consumes: Task 4 `Machines`, `Machine`, `MachineState`, `SessionLink`; Task 5 `connect_machine`, `MachineError`.
- Produces:

```rust
// launch.rs
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launch {
    /// This computer's window, maybe jumping to a session.
    Local { jump: Option<(Ulid, Ulid)> },
    /// The window of machine `name`.
    Machine { name: String, jump: Option<(Ulid, Ulid)> },
}
/// `args` without the program name: `[]`, `[<mkrew:// link>]`, `["--machine", name]`,
/// `["--machine", name, <link>]`; a link with `machine=` means that machine.
pub fn parse(args: &[String]) -> Launch;
/// The single-instance key input for a launch (`machine:<name>` or the socket).
pub fn instance_id(launch: &Launch, socket: &str) -> String;

// shell (main.rs / connection.rs)
pub(crate) enum Target { Local, Machine(Machine) }
// Shell gains `target: Target` and `machine_state: MachineState` (machine windows).
```

- [ ] **Step 1: Failing tests for `launch.rs`**

```rust
#[test]
fn arguments_pick_the_window() {
    let s = |v: &[&str]| v.iter().map(|a| a.to_string()).collect::<Vec<_>>();
    let (session, project) = (Ulid::new(), Ulid::new());
    assert_eq!(parse(&s(&[])), Launch::Local { jump: None });
    let local_link = mkrew_core::link::session_link(session, project, None);
    assert_eq!(parse(&s(&[&local_link])), Launch::Local { jump: Some((session, project)) });
    assert_eq!(
        parse(&s(&["--machine", "Home PC"])),
        Launch::Machine { name: "Home PC".into(), jump: None }
    );
    let machine_link = mkrew_core::link::session_link(session, project, Some("Home PC"));
    assert_eq!(
        parse(&s(&[&machine_link])),
        Launch::Machine { name: "Home PC".into(), jump: Some((session, project)) }
    );
    assert_eq!(
        parse(&s(&["--machine", "Home PC", &local_link])),
        Launch::Machine { name: "Home PC".into(), jump: Some((session, project)) }
    );
    // Garbage is ignored as before (the old code ignored a non-link argument).
    assert_eq!(parse(&s(&["whatever"])), Launch::Local { jump: None });
    assert_eq!(parse(&s(&["--machine"])), Launch::Local { jump: None });
}

#[test]
fn instance_ids_differ_per_machine_and_keep_the_socket_for_this_computer() {
    let local = Launch::Local { jump: None };
    let home = Launch::Machine { name: "Home PC".into(), jump: None };
    assert_eq!(instance_id(&local, r"\\.\pipe\mkrew-me"), r"\\.\pipe\mkrew-me");
    assert_eq!(instance_id(&home, "x"), "machine:home pc");
}
```

Run: `cargo test -p mkrew-ui --lib launch -j 4` → FAIL; implement; → PASS.

- [ ] **Step 2: Desktop key and optional tray icon**

- `raise_running_instance(id: &str, jump)` and `Os::new(window, id: &str, tray_icon: bool)` take the instance id string from `launch::instance_id` instead of the socket (rename the parameters; `instance_key(exe, id)` unchanged).
- `tray_icon: false` (machine window): the hidden tray window still starts (it is the raise target), but `set_tray` never shows an icon. Implement as a flag in `Os`/`Tray` that makes `set_tray` a no-op and `has_tray()` return `false`. `none.rs` takes the same parameters.
- Machine windows never close to tray: where the close request checks close-to-tray (grep `close_to_tray`), treat a machine window as off.

- [ ] **Step 3: `main.rs`**

```rust
let args: Vec<String> = std::env::args().skip(1).collect();
let launch = mkrew_ui::launch::parse(&args);
let id = mkrew_ui::launch::instance_id(&launch, &paths.socket);
let jump = match &launch { Launch::Local { jump } | Launch::Machine { jump, .. } => *jump };
if mkrew_ui::desktop::raise_running_instance(&id, jump) {
    return;
}
let target = match &launch {
    Launch::Local { .. } => Ok(Target::Local),
    Launch::Machine { name, .. } => match Machines::load(&paths.config_dir) {
        Ok(machines) => machines.get(name).cloned().map(Target::Machine).ok_or_else(|| {
            format!("{name} is no longer in the machine list.")
        }),
        Err(error) => Err(error),
    },
};
let title = match &launch {
    Launch::Local { .. } => "Mkrew".to_owned(),
    Launch::Machine { name, .. } => format!("Mkrew - {name}"),
};
```

`TitlebarOptions { title: Some(title.into()), … }`; `Shell::new(…, target, id, …)`.
An `Err(text)` target opens the window showing only that text (reuse the status screen with a `Status::Failed(text)` and no reconnect; add a `Status::NoMachine(String)` variant if `Failed` triggers retries).
A link that names a machine while this is the local launch: `parse` already returns `Launch::Machine`, so the raise targets that machine's window; if none runs, this process becomes that machine's window. Good enough (spec §6.5).

- [ ] **Step 4: Connect by target**

In `start_connect` choose by `self.target`:
- `Target::Local` → today's `connect::connect(&paths)`.
- `Target::Machine(m)` → `connect::connect_machine(&m)` on the background executor; on `MachineError::Unreachable { reason, hint }` → `Status::Unreachable { reason, hint }` + `schedule_reconnect`; on `MachineError::Version(msg)` → `Status::Version(msg)` and **no** reconnect.
Add the two `Status` variants; `status_line()` texts: Unreachable → sentence "Cannot reach <name>.", detail "<reason>\n<hint>" (hint line only when present), countdown "Retrying in N s…" as today; Version → sentence "<name> runs another version of Mkrew.", detail "<msg> Update Mkrew on the older computer.", action **Try now**. Rename the Failed/Disconnected action label to **Try now** in machine windows (keep today's labels in this computer's window).
Every state change in a machine window writes `MachineState::line()` to stdout (one line, flush; ignore write errors - nobody may be reading). Do this in one helper `fn report_machine_state(&mut self, state: MachineState)` that also stores it in `self.machine_state` for the chip and only writes when it changed.
Machine windows skip the phone poll loop and the update loop in `Shell::new`, and `render_phone_button` / `render_update_button` return nothing there. The palette's `Extra::PhoneAccess` and `Extra::CheckUpdates` are unavailable in machine windows (`palette.rs` `available()`).

- [ ] **Step 5: Machine chip in the title bar**

In `render_title_bar`, right after `app_logo` (before the sidebar toggle), for `Target::Machine(m)`: a non-interactive row with the Lucide `server` icon (check `crates/mkrew-ui/src/icons` or how `smartphone` is drawn by `render_phone_button`; add the `server` icon the same way if missing), the name, and a status dot; tooltip = `self.machine_state.label()`.
Tokens: add to `theme.rs` only what is missing, e.g. `MACHINE_CHIP_GAP`, `MACHINE_CHIP_PAD_X`, `MACHINE_CHIP_RADIUS`, using existing color tokens (`success` for connected, `warning` for connecting/reconnecting, `error` for unreachable/version; `muted` text). Follow the `mkrew-ui-design` skill (`.claude/skills/mkrew-ui-design`) rules; read it before this step.
The chip keeps showing while disconnected (the title bar otherwise drops the workspace switcher when `connection` is `None`).

- [ ] **Step 6: Run and check visually**

Run: `cargo test -p mkrew-ui -j 4`, `cargo clippy -p mkrew-ui --all-targets -- -D warnings`.
Manual smoke (allowed: it uses no network): create a temp config dir with a `machines.toml` whose host is `127.0.0.1` on a port nothing listens on is still a network call - instead use host `mkrew-invalid.invalid` with env `MKREW_CONFIG_DIR=<temp>` and run `target/debug/mkrew.exe --machine Test`: the window titled "Mkrew - Test" shows "Cannot reach Test." with ssh's resolve error and the offline hint, and retries. Take a screenshot if you can; otherwise note it for HUMAN_CHECKS.

- [ ] **Step 7: HUMAN_CHECKS and commit**

```markdown
- [ ] **Machine window:** `mkrew --machine "<name>"` opens "Mkrew - <name>" with the server chip (dot + tooltip state); with the other computer reachable it shows its workspaces and agents run there; with it offline: "Cannot reach <name>." + ssh's reason + hint + countdown + Try now; with an older Mkrew there: the version message and no retry; no tray icon for it; closing it ends it (no close to tray) and the agents keep running there; Phone access and update are absent.
```

```bash
git add crates/mkrew-ui/src docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): machine windows (mkrew --machine <name>) over ssh"
```

---

### Task 7: Keep the screen while reconnecting

Spec: §6.3 last row.

**Current state (investigated):** `shell/connection.rs` (~476-547): when `incoming.recv()` fails, it sets `status = Disconnected`, `connection = None`, `workspace = None`, clears `tabs`, `loading_tabs`, `early_tab_events`, `router`, `panes`, `attach_errors`, menus, `run_setup`, `ports`, `plans`, `docs`, `files`, `git`, `git_summary`, closes many dialogs (keeps `task_dialog`, `add_workspace`), focuses, then `schedule_reconnect`. On reconnect `refresh()` picks `initial_workspace()`. `chrome.rs:117` shows the message screen when `workspace` or `connection` is `None`; `render_title_bar` hides the workspace switcher without a connection.

**Files:**
- Modify: `crates/mkrew-ui/src/shell/connection.rs`
- Modify: `crates/mkrew-ui/src/shell/chrome.rs` (banner; render the kept screen while reconnecting)
- Modify: the input entry points that must ignore input while reconnecting (pane key/mouse handlers in `shell/pane_render.rs` / `terminal/view.rs`, sidebar clicks): one guard `self.reconnecting()` checked where the shell routes input, not in every handler
- Modify: `crates/mkrew-ui/src/theme.rs` (banner tokens if needed)

**Interfaces:**
- Produces: `fn reconnect_workspace(shown: Option<Ulid>, listed: &[Project]) -> Option<Ulid>` (pure, in `connection.rs` or a small module) choosing the workspace to show after reconnecting.

- [ ] **Step 1: Failing unit test**

```rust
#[test]
fn reconnect_keeps_the_shown_workspace() {
    let (a, b) = (project("a"), project("b"));
    assert_eq!(reconnect_workspace(Some(b.id), &[a.clone(), b.clone()]), Some(b.id));
    // Removed meanwhile (on the other computer): fall back to today's initial choice.
    assert_eq!(reconnect_workspace(Some(Ulid::new()), &[a.clone()]), None);
    assert_eq!(reconnect_workspace(None, &[a]), None);
}
```

`None` means "use `initial_workspace()` as today". Use whatever `Project` constructor the shell tests use.

- [ ] **Step 2: Move the clearing to the reconnect**

- On disconnect: set `status = Disconnected`, `connection = None`, remember `self.kept_workspace = self.workspace` (new field), **do not** clear `workspace`, `tabs`, `panes` or the views. Close only transient popups that would act on the daemon (menus, palette, quick open, peek, prompt picker); keep dialogs as today's code keeps them.
- Panes: each `TerminalPane` holds the dead `Rc<Connection>`; its own requests fail with "the daemon connection is closed" - that is fine while input is blocked.
- Background polls that call the daemon while `connection` is `None` already return early (they use `self.connection.clone()`); polls that captured a connection fail quietly (check: they must not toast every second; if one does, make it skip when `!self.status.is_connected()`).
- On the next successful `hello` (in `start_connect` success, before `refresh()`): run today's clearing code (moved into `fn reset_connection_state(&mut self, …)`), then `refresh()` choosing `reconnect_workspace(self.kept_workspace.take(), &projects)` before falling back to `initial_workspace()`. The visible panes re-attach through the normal path (they were cleared; the render path attaches visible panes on demand - confirm by reading how panes are created after `refresh`).
- First connect (no kept workspace) behaves exactly as today.

- [ ] **Step 3: Render the kept screen with a banner**

- `chrome.rs:117`: show the message screen only when there is no workspace to show (first connect, unreachable, version); while `Disconnected`/`Connecting` with a kept workspace, render the normal layout plus a banner at the top of the main area: "Connection to <name> lost. Reconnecting in N s…" (this computer's window: "Connection to the daemon lost. Reconnecting in N s…") and **Try now** (same action as the status screen's).
- The title bar keeps the workspace switcher visible but inert while reconnecting.
- Input: keyboard and mouse to panes, sidebar and title-bar actions are ignored while `self.reconnecting()` (status is not `Connected` and a workspace is kept). Implement with one transparent overlay element over the main area and sidebar that captures mouse events (`occlude()` in GPUI; check the API used elsewhere for modal backdrops) and an early return in the shell's key dispatch (`on_key_down`/action handlers routed through the shell). Dialogs keep working (their buttons fail with the request error).
- Tokens: banner background/border/text from existing semantic tokens (`warning` family); spacing from `theme.rs`.

- [ ] **Step 4: Run**

Run: `cargo test -p mkrew-ui -j 4` and clippy.
Manual smoke (local daemon, no network): run the built `mkrew` against a test config/data dir, open a workspace with a shell pane, run `mk daemon stop` for that socket: the screen stays with the banner and input is ignored; `mk daemon start` (or wait for the window's own respawn) brings the same workspace back with the pane re-attached. Screenshot if possible.

- [ ] **Step 5: HUMAN_CHECKS and commit**

```markdown
- [ ] **Keep the screen while reconnecting:** with panes open, stop the daemon (this computer) or pull the network (machine window): the layout and last frames stay, a banner counts down with Try now, typing and clicks do nothing; when the connection is back the same workspace shows and panes re-attach with their history; an open task dialog keeps its text.
```

```bash
git add crates/mkrew-ui/src docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): keep the screen and re-attach after a lost connection"
```

---

### Task 8: Launcher, Settings → Machines, palette and tray

Spec: §4.5 launcher, §6.2.

**Current state (investigated):** Settings tabs: `crates/mkrew-ui/src/settings_dialog.rs:18` `pub enum Tab { Settings(Page), Keyboard }`, `Tab::all()`; rendering in `shell/settings.rs`. Palette: `crates/mkrew-ui/src/palette.rs` `enum Extra` (`ALL`, `title()`, `available()`), dispatch in `shell/navigation.rs` (~659 and ~840). Tray: `attention.rs` `TrayMenu { needs_you, working, items }`, `TrayEvent` in `desktop.rs` (`Open`, `Session(Ulid)`, `Notification{..}`, `Quit`, `TaskbarRestarted`), win32 menu built from `TrayMenu`; `shell/attention.rs` `tray_event` handles events and `quit_ui`.

**Files:**
- Create: `crates/mkrew-ui/src/launcher.rs` (child processes; no GPUI types)
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod launcher;`)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (`Shell::new`: start the launcher in this computer's window; poll its states)
- Modify: `crates/mkrew-ui/src/settings_dialog.rs`, `crates/mkrew-ui/src/shell/settings.rs` (Machines tab and Add form)
- Modify: `crates/mkrew-ui/src/palette.rs`, `crates/mkrew-ui/src/shell/navigation.rs`
- Modify: `crates/mkrew-ui/src/attention.rs`, `crates/mkrew-ui/src/desktop.rs`, `crates/mkrew-ui/src/desktop/win32.rs`, `crates/mkrew-ui/src/shell/attention.rs` (tray machine items, Quit kills children)
- Modify: the update install path (kills children before installing; grep `quit_ui` / installer start in `shell/update.rs`)

**Interfaces:**
- Consumes: Task 4 `Machines`, `MachineState`; Task 6 `--machine`.
- Produces:

```rust
// launcher.rs
pub struct Launcher { /* exe, config_dir, children: Vec<Child> by name */ }
impl Launcher {
    pub fn new(exe: PathBuf) -> (Self, async_channel::Receiver<(String, MachineState)>);
    /// Raises the machine's window if its child runs (Windows: through the instance key),
    /// else starts `exe --machine <name>` with piped stdout read on a thread.
    pub fn open(&mut self, name: &str);
    /// Ends that machine's window (Remove).
    pub fn close(&mut self, name: &str);
    /// Kills every child (Quit, update install).
    pub fn close_all(&mut self);
    /// Running children (for "Window closed").
    pub fn is_running(&mut self, name: &str) -> bool;
}
// TrayEvent gains `Machine(u8)` (index into the tray menu's machine list; `TrayEvent` is `Copy`).
// TrayMenu gains `machines: Vec<(String, String)>` (name, state label).
// settings_dialog::Tab gains `Machines`.
// palette::Extra gains `AddMachine` and `OpenMachine(u8)` (or whatever shape lets the palette list one entry per machine; follow how the palette lists dynamic entries such as sessions or workspaces).
```

- [ ] **Step 1: Failing unit test for the launcher's line reader**

The process parts are not unit-testable headless; test the reader that turns a child's stdout into states:

```rust
#[test]
fn a_childs_stdout_lines_become_states_and_eof_means_closed() {
    let text = b"connecting\nunreachable Permission denied\nnoise\nconnected\n";
    let states: Vec<MachineState> = read_states(&text[..]).collect();
    assert_eq!(
        states,
        [
            MachineState::Connecting,
            MachineState::Unreachable("Permission denied".into()),
            MachineState::Connected,
            MachineState::Closed,
        ]
    );
}
```

`fn read_states(reader: impl BufRead) -> impl Iterator<Item = MachineState>` parses lines with `MachineState::parse`, skips unknown lines, and ends with `Closed` at EOF.
Run → FAIL; implement; → PASS.

- [ ] **Step 2: `Launcher`**

- Spawn: `Command::new(exe).arg("--machine").arg(name).stdin(null).stdout(piped).stderr(null)`; Windows: no extra flags (a GUI app has no console). The child inherits this process's environment (`MKREW_CONFIG_DIR` etc. reach it, which keeps tests and dev setups consistent).
- One reader thread per child sends `(name, state)` for each `read_states` item to the channel.
- `open(name)`: if `is_running(name)` → on Windows call `desktop::raise_running_instance(&launch::instance_id(&Launch::Machine{..}), None)`; else spawn.
- `close_all`: `child.kill()` then `wait()` for each.
- Before spawning at startup, skip a machine whose window already runs: `raise_running_instance` would raise it - use a non-raising probe instead (add `desktop::instance_running(id) -> bool` that only does the `FindWindowW` part on Windows, `false` elsewhere).

- [ ] **Step 3: Wire into this computer's window**

In `Shell::new` when `target` is `Local`: load `Machines` (keep the result: `Ok` list or `Err(text)` for the Machines tab), create the `Launcher`, call `open` for every machine (after the window is created; a spawned GUI child steals no focus because it is started before this window is activated? - check; if focus jumps to the child, open children after the first paint with a short timer), and poll the state channel on the existing status tick (no new timer), storing `machine_states: HashMap<String, MachineState>` and re-syncing the tray menu.
Quit (`TrayEvent::Quit` / `quit_ui`) and the update install call `launcher.close_all()` first.

- [ ] **Step 4: Settings → Machines tab**

`Tab::Machines` after `Keyboard`, title "Machines". In this computer's window:
- Error state (broken file): the error text (file + line), no buttons except closing the dialog.
- List rows: name (primary text), host and `mk` (muted), state label (`MachineState::label`, icon + text), buttons **Open**, **Remove** (Remove → confirmation using the app's existing confirm pattern; on confirm `launcher.close(name)`, `machines.remove`, `save`).
- **Add machine…** button reveals three text fields (*Name*, *Host*, *mk path (optional)*) and **Save** / **Cancel**; Save calls `machines.add` and shows `Invalid.message` under `Invalid.field`'s input; success saves the file and calls `launcher.open(name)`.
- Help line under the list (spec §6.2).
- In a machine window the tab shows only "Machines are managed in This computer's window."
Use the existing text input component the Settings dialog uses (grep the Keyboard page or the task dialog for `Input`), existing button helpers, theme tokens. Read `.claude/skills/mkrew-ui-design` first.

- [ ] **Step 5: Palette and tray**

- Palette (this computer's window only): "Machine: Add…" opens Settings on the Machines tab with the Add form open; "Machine: Open <name>" per machine calls `launcher.open`.
- Tray menu (Windows): a section above Quit with one item per machine "<name> - <state label>"; clicking sends `TrayEvent::Machine(index)` → `launcher.open(name)`. Keep the tooltip as today.

- [ ] **Step 6: Run and smoke**

Run: `cargo test -p mkrew-ui -j 4`, clippy.
Smoke (no network): temp config dir with a `machines.toml` holding one machine with host `mkrew-invalid.invalid`; start `target/debug/mkrew.exe` with that `MKREW_CONFIG_DIR` (and test data dir/socket): a second window "Mkrew - <name>" opens showing "Cannot reach…"; Settings → Machines shows "Not reachable: …"; closing the machine window shows "Window closed"; Open brings it back; tray Quit closes both. Screenshot if possible.

- [ ] **Step 7: HUMAN_CHECKS and commit**

```markdown
- [ ] **Launcher and Machines page:** with two machines in Settings → Machines, starting Mkrew opens this computer's window and both machine windows; the page shows each state (Connected / Reconnecting… / Not reachable: … / Window closed); Add machine refuses a host starting with "-" or holding a space and an existing name, with the message under the field; Save opens the new window; Remove asks, closes that window and deletes the line; a broken machines.toml shows its error and line and the file is not rewritten; one tray icon only, listing the machines; tray Quit closes every window.
```

```bash
git add crates/mkrew-ui/src docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): open machine windows at startup; Settings → Machines; tray and palette entries"
```

---

### Task 9: Machine windows refuse this computer's disk; their notifications show here

Spec: §6.4, §6.5.

**Current state (investigated):** see the spec §6.4 table; call sites:
- Folder pickers: `shell/workspace_actions.rs` `cx.prompt_for_paths` at ~86 (Add workspace, Clone into) and ~231 (Link plan).
- Zed / Explorer: `workspace_actions.rs` ~349, ~364, ~405-424; `files.rs` ~829; `git.rs` ~714; `docs.rs` ~192; `shell/history.rs` Follow in Zed ~8-58; `inspector.rs` Open transcript ~600-610; `navigation.rs` ~737; `menus.rs` entries.
- Terminal links: `shell/focus.rs` ~285 `set_links(...)` from `link_scope`; `terminal/view.rs` ~455-461.
- File drop / pasted file lists: `shell/pane_render.rs` ~499-503 → `navigation.rs` ~83-97 `drop_paths`; `terminal/view.rs` ~369-377.
- Settings dialog header: `shell/settings.rs`.
- Daemon notifications: `crates/mkrew-daemon/src/notify.rs` (`decide`, `Throttle`, `Notice`, `Notifier`, `from_env`, `Alert` with private fields), `wintoast.rs` builds the toast link with `session_link`.

**Files:**
- Modify: the call sites above
- Create: `crates/mkrew-ui/src/shell/path_field.rs` only if no existing one-line text dialog can be reused (grep for a rename/line input dialog first; `Renaming`/`Input` exist in `workspace_actions.rs`)
- Modify: `crates/mkrew-daemon/src/notify.rs` (`Alert` fields `pub`; `Notice` gains `machine: Option<String>`)
- Modify: `crates/mkrew-daemon/src/wintoast.rs` (link with `notice.machine`)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (machine window: notifier + throttle on `SessionState`)
- Modify: `crates/mkrew-daemon/src/lib.rs` (export what the UI needs: `notify::{decide, Throttle, Notice, Notifier, from_env}` if not already public)

**Interfaces:**
- Consumes: `Target`, `Machine` (Task 6), `session_link(…, Some(name))` (Task 4).
- Produces: `fn local_only(&self, cx) -> bool` on `Shell`: `true` and a toast "Opens files on this computer only; <name> is another computer." when `target` is a machine (callers return early when it is `true`).

- [ ] **Step 1: Failing test for notices with a machine**

In `notify.rs` tests (or wintoast's), assert the toast XML/link of a notice with `machine: Some("Home PC")` contains `machine=Home%20PC`, and `None` keeps today's link. Run → FAIL; implement (`Notice.machine`, `toast_xml` uses `session_link(notice.session_id, notice.project_id, notice.machine.as_deref())`; every `Notice { … }` literal gets `machine: None`) → PASS.

- [ ] **Step 2: Typed paths in machine windows**

- Add workspace / Clone into: in a machine window the picker button becomes a text field "Folder on <name>" (prefilled empty) with **Add**/**Clone**; submitting sends the same `ProjectAdd`/`ProjectClone` request with the typed path; the daemon's error shows under the field (the dialog already shows request errors; reuse that).
- Link plan: the same with "Plan file on <name>" and `SessionLinkPlan`.
- This computer's window keeps the OS pickers.

- [ ] **Step 3: Refusals**

Every Zed/Explorer/Reveal/Open transcript/Follow entry point starts with `if self.local_only(cx) { return; }`. Find each with grep (`zed::open`, `open_with_system`, `reveal_path`, `follow`); list the ones you changed in the commit body.
Terminal links: in a machine window `set_links(None)` (URLs only). Confirm URL links still open (`cx.open_url`).
File drop and pasted file lists: in a machine window, toast "These files are on this computer; <name> cannot read them." and type nothing. Pasted images unchanged.
Settings dialog: in a machine window a muted line at the top of the Settings pages (not Keyboard/Machines): "Daemon settings (notifications, retention, features) are <name>'s own; change them on <name>."

- [ ] **Step 4: Notifications in machine windows**

In a machine window only: keep `notifier: Option<Arc<dyn Notifier>>` (`mkrew_daemon::notify::from_env()`, created once in `Shell::new`) and a `Throttle`. On `Event::SessionState(e)` in `dispatch`, call `decide(e.session_id, e.state, &local_settings.notify, &mut throttle, Instant::now())` with this computer's `settings.toml` `[notify]` (the shell already reads settings; find where `flash_taskbar` comes from); on `Some(alert)` build a `Notice { session_id, project_id, title: "<project> · <name of session>", body: alert.body, sound: alert.sound, machine: Some(machine.name) }` from the shell's session and project lists (same title rule as `notify::notice`: user title, else terminal title, else profile), and `notifier.show(notice, clicked_tx)`. Clicks (non-Windows) arrive on `clicked_rx`: poll it on the status tick and call `notification_clicked(session, project)`. On Windows the toast's `mkrew://` link starts `mkrew` with a machine link, which `launch::parse` routes to this window (Task 6).
This computer's window changes nothing: its daemon still notifies.

- [ ] **Step 5: Run and smoke**

Run: `cargo test -p mkrew-daemon -p mkrew-ui -j 4`, clippy, and grep the diff for hex colors and px numbers outside `theme.rs`.

- [ ] **Step 6: HUMAN_CHECKS and commit**

```markdown
- [ ] **Machine window, local-only actions:** Add workspace and Link plan ask for a path on the other computer (a wrong path shows the daemon's error); Open in Zed / Explorer / Follow / Open transcript toast "Opens files on this computer only; <name> is another computer."; Ctrl+Click on a URL opens the browser, file paths are not links; dropping files toasts and types nothing; a pasted screenshot is typed as a path on the other computer; Settings shows the "daemon settings are <name>'s own" line.
- [ ] **Machine window notifications:** an agent on the other computer that needs input shows a notification on this computer (once per 30 s per session, per this computer's [notify] settings); clicking it brings that machine's window to the session; the other computer still shows its own notification.
```

```bash
git add crates/mkrew-ui/src crates/mkrew-daemon/src docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): machine windows type remote paths, refuse local-only actions and notify here"
```

---

### Task 10: Docs close-out

Spec: §10.

**Files:**
- Modify: `docs/SPEC.md` §10 (title bar machine chip; Settings → Machines; connection states incl. keep-screen banner), §14 (chip and banner tokens with their names)
- Modify: `docs/INBOX.md` (item 15)
- Delete: `docs/superpowers/plans/2026-10-10-remote-machine.md`
- Modify: `docs/decisions/0089-remote-machine.md` (consequences: anything that changed during implementation)

- [ ] **Step 1: SPEC**

§10 Title bar bullet: "In a machine window (ADR 0089) the logo is followed by a chip: Lucide `server`, the machine name and a status dot whose tooltip names the state." Connection paragraph: the keep-screen banner and blocked input (both windows), Unreachable/Version screens in machine windows. Settings dialog: the Machines tab. §14: the new tokens with values.

- [ ] **Step 2: INBOX and old plan**

`docs/INBOX.md` item 15: replace the line and its sub-bullets with
`- [x] **15. Remote machine: one window per computer over SSH** [feature] → plan: docs/superpowers/plans/2026-10-29-remote-machine.md (spec docs/superpowers/specs/2026-10-29-remote-machine-design.md, ADR 0089)`.
`git rm docs/superpowers/plans/2026-10-10-remote-machine.md`.

- [ ] **Step 3: Full verify and commit**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` → green.

```bash
git add docs/SPEC.md docs/INBOX.md docs/decisions/0089-remote-machine.md
git rm -q docs/superpowers/plans/2026-10-10-remote-machine.md
git -c user.email=tungnv975@gmail.com commit -m "docs: remote machine in SPEC, inbox 15 done, old plan removed"
```
