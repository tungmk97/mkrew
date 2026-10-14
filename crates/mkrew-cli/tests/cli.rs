//! Runs the real `mk` binary against a daemon in a temp dir.
//!
//! `mk daemon start` spawns the `mkrew-daemon` binary next to `mk`, so it must be built first
//! (`cargo test --workspace` does; `cargo test -p mkrew-cli` alone may not).

// Helpers outside `#[test]` fns are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ulid::Ulid;

struct Env {
    dir: tempfile::TempDir,
    socket: String,
}

impl Env {
    fn new() -> Self {
        let daemon = Path::new(env!("CARGO_BIN_EXE_mk"))
            .with_file_name(format!("mkrew-daemon{}", std::env::consts::EXE_SUFFIX));
        assert!(
            daemon.is_file(),
            "{} is missing; run `cargo build -p mkrew-daemon`",
            daemon.display()
        );
        let dir = tempfile::tempdir().unwrap();
        let socket = if cfg!(windows) {
            format!(r"\\.\pipe\mkrew-test-{}", Ulid::new())
        } else {
            dir.path().join("mkrew.sock").to_string_lossy().into_owned()
        };
        Self { dir, socket }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_mk"));
        command
            .args(args)
            .env("MKREW_DATA_DIR", self.dir.path().join("data"))
            .env("MKREW_CONFIG_DIR", self.dir.path().join("config"))
            .env("MKREW_SOCKET", &self.socket)
            .env_remove("MKREW_MK")
            // Inherited by the daemon `mk` starts: no real toasts from tests.
            .env(mkrew_daemon::ENV_NOTIFY, "off");
        command
    }

    fn mk(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }

    /// Runs `mk`, asserts success and returns stdout.
    fn ok(&self, args: &[&str]) -> String {
        let output = self.mk(args);
        assert!(
            output.status.success(),
            "mk {args:?} failed: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }
}

impl Env {
    /// `agents/fake.toml` running the built `mkrew-fakeagent` (default `ask` scenario).
    fn write_fake_profile(&self) {
        self.write_profile("fake", &fake_profile());
    }

    fn write_profile(&self, name: &str, profile: &str) {
        let dir = self.dir.path().join("config").join("agents");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{name}.toml")), profile).unwrap();
    }

    /// Runs `mk hook …` as an agent inside session `id` would, with `stdin` piped in.
    fn hook(&self, id: &str, args: &[&str], stdin: &str) {
        let mut args = args.to_vec();
        args.insert(0, "hook");
        let mut child = self
            .command(&args)
            .env("MKREW_SESSION_ID", id)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "mk {args:?}: {output:?}");
    }
}

/// The sample `agents/fake.toml` pointed at the built `mkrew-fakeagent`.
fn fake_profile() -> String {
    let fake = Path::new(env!("CARGO_BIN_EXE_mk"))
        .with_file_name(format!("mkrew-fakeagent{}", std::env::consts::EXE_SUFFIX));
    assert!(
        fake.is_file(),
        "{} is missing; run `cargo build -p mkrew-fakeagent`",
        fake.display()
    );
    let sample = include_str!("../../../agents/fake.toml");
    let profile = sample.replace(
        "command = \"mkrew-fakeagent\"",
        &format!("command = '{}'", fake.display()),
    );
    assert_ne!(profile, sample);
    profile
}

impl Env {
    /// Starts `mk attach <id>` with piped stdio (no terminal: raw mode is skipped).
    fn attach(&self, id: &str) -> Attach {
        let mut child = self
            .command(&["attach", id])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let output = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&output);
        let reader = std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n @ 1..) = stdout.read(&mut buf) {
                sink.lock().unwrap().extend_from_slice(&buf[..n]);
            }
        });
        Attach {
            child,
            output,
            reader,
        }
    }
}

struct Attach {
    child: Child,
    output: Arc<Mutex<Vec<u8>>>,
    reader: JoinHandle<()>,
}

impl Attach {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.output.lock().unwrap()).into_owned()
    }

    /// Waits until stdout contains `needle`.
    fn wait_for(&self, needle: &str) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !self.text().contains(needle) {
            assert!(
                Instant::now() < deadline,
                "no {needle:?} in attach output:\n{}",
                self.text()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn type_bytes(&mut self, bytes: &[u8]) {
        let stdin = self.child.stdin.as_mut().unwrap();
        stdin.write_all(bytes).unwrap();
        stdin.flush().unwrap();
    }

    /// Waits for `mk attach` to exit successfully and returns all of its stdout.
    fn finish(mut self) -> String {
        let deadline = Instant::now() + Duration::from_secs(20);
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < deadline, "mk attach did not exit");
            std::thread::sleep(Duration::from_millis(50));
        };
        let mut stderr = String::new();
        self.child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut stderr)
            .unwrap();
        assert!(status.success(), "mk attach failed: {status}\n{stderr}");
        // The pipe closes with the process, so the reader has seen everything once it ends.
        self.reader.join().unwrap();
        String::from_utf8_lossy(&self.output.lock().unwrap()).into_owned()
    }
}

/// The daemon is detached, so stop it even when an assertion fails.
impl Drop for Env {
    fn drop(&mut self) {
        let _ = self.mk(&["daemon", "stop"]);
    }
}

/// The pid from `daemon started (pid N)` / `daemon already running (pid N)`.
fn pid(stdout: &str) -> u32 {
    let start = stdout.find("(pid ").unwrap() + "(pid ".len();
    let end = start + stdout[start..].find(')').unwrap();
    stdout[start..end].parse().unwrap()
}

/// The installer's stop (`--only-from {app}\bin`) leaves a daemon of another folder alone and
/// stops one that runs from the folder (any spelling of it).
#[test]
fn daemon_stop_only_from_spares_a_daemon_of_another_folder() {
    let env = Env::new();
    env.ok(&["daemon", "start"]);
    let elsewhere = tempfile::tempdir().unwrap();
    let out = env.ok(&[
        "daemon",
        "stop",
        "--only-from",
        &elsewhere.path().to_string_lossy(),
    ]);
    assert!(out.contains("left running"), "{out}");
    assert!(env.ok(&["daemon", "status"]).contains("daemon running"));

    // The test daemon runs from `target/<profile>/`, next to `mk`.
    let bin = std::path::Path::new(env!("CARGO_BIN_EXE_mk"))
        .parent()
        .unwrap();
    let mut spelled = bin.to_string_lossy().into_owned();
    if cfg!(windows) {
        spelled = spelled.to_uppercase();
    }
    spelled.push(std::path::MAIN_SEPARATOR);
    let out = env.ok(&["daemon", "stop", "--only-from", &spelled]);
    assert!(out.contains("daemon stopped"), "{out}");
    assert_eq!(env.mk(&["daemon", "status"]).status.code(), Some(1));
}

#[test]
fn daemon_start_status_stop() {
    let env = Env::new();

    let status = env.mk(&["daemon", "status"]);
    assert_eq!(status.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&status.stdout).contains("not running"));
    assert!(env.ok(&["daemon", "stop"]).contains("not running"));

    // Two concurrent starts end up talking to one daemon.
    let racing: Vec<_> = (0..2)
        .map(|_| {
            env.command(&["daemon", "start"])
                .stdout(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    let racing: Vec<_> = racing
        .into_iter()
        .map(|child| child.wait_with_output().unwrap())
        .collect();
    let racing_output = |i: usize| String::from_utf8(racing[i].stdout.clone()).unwrap();
    assert!(racing.iter().all(|o| o.status.success()));
    let daemon_pid = pid(&racing_output(0));
    assert_eq!(pid(&racing_output(1)), daemon_pid);

    let again = env.ok(&["daemon", "start"]);
    assert!(again.contains("already running"), "{again}");
    assert_eq!(pid(&again), daemon_pid);

    let status = env.ok(&["daemon", "status"]);
    assert!(
        status.contains(&format!("pid:       {daemon_pid}")),
        "{status}"
    );

    assert!(env.ok(&["daemon", "stop"]).contains("daemon stopped"));
    assert_eq!(env.mk(&["daemon", "status"]).status.code(), Some(1));
}

#[test]
fn projects_are_added_listed_and_persisted() {
    let env = Env::new();
    let repo = env.dir.path().join("repo");
    let other = env.dir.path().join("other");
    std::fs::create_dir(&repo).unwrap();
    std::fs::create_dir(&other).unwrap();

    let refused = env.mk(&["project", "list"]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("mk daemon start"));

    env.ok(&["daemon", "start"]);
    assert!(env.ok(&["project", "list"]).contains("no projects"));
    let added = env.ok(&["project", "add", repo.to_str().unwrap(), "--name", "demo"]);
    assert!(added.contains("added project demo"), "{added}");

    // Relative paths resolve against the caller's directory, not the daemon's.
    let output = env
        .command(&["project", "add", "."])
        .current_dir(&other)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");

    let duplicate = env.mk(&["project", "add", repo.to_str().unwrap()]);
    assert!(!duplicate.status.success());

    // Projects survive a daemon restart (SQLite).
    env.ok(&["daemon", "stop"]);
    env.ok(&["daemon", "start"]);
    let list = env.ok(&["project", "list"]);
    let rows: Vec<_> = list.lines().skip(1).collect();
    assert_eq!(rows.len(), 2, "{list}");
    assert!(
        rows[0].contains("demo") || rows[1].contains("demo"),
        "{list}"
    );
    assert!(list.contains("other"), "{list}");
}

#[test]
fn project_update_sets_and_clears_the_git_base() {
    let env = Env::new();
    let repo = env.dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    env.ok(&["daemon", "start"]);
    env.ok(&["project", "add", repo.to_str().unwrap(), "--name", "demo"]);

    let set = env.ok(&["project", "update", "demo", "--base", "develop"]);
    assert_eq!(set.trim(), "base of demo: develop");
    let auto = env.ok(&["project", "update", "demo", "--auto-base"]);
    assert_eq!(auto.trim(), "base of demo: auto-detect");

    // One change is required, and the two conflict.
    assert!(!env.mk(&["project", "update", "demo"]).status.success());
    let both = env.mk(&["project", "update", "demo", "--base", "x", "--auto-base"]);
    assert!(!both.status.success());
    // The daemon refuses refs git would read as options.
    let bad = env.mk(&["project", "update", "demo", "--base=--force"]);
    assert!(!bad.status.success());
    assert!(
        String::from_utf8_lossy(&bad.stderr).contains("invalid base ref"),
        "{bad:?}"
    );
    let unknown = env.mk(&["project", "update", "nope", "--base", "main"]);
    assert!(!unknown.status.success());
    env.ok(&["daemon", "stop"]);
}

#[test]
fn project_run_autostart_is_switched_on_update() {
    let env = Env::new();
    let repo = env.dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    env.ok(&["daemon", "start"]);
    env.ok(&["project", "add", repo.to_str().unwrap(), "--name", "demo"]);
    let on = env.ok(&["project", "update", "demo", "--run-autostart", "on"]);
    assert_eq!(on.trim(), "run auto-start of demo: on");
    let both = env.ok(&[
        "project",
        "update",
        "demo",
        "--auto-base",
        "--run-autostart",
        "off",
    ]);
    let lines: Vec<_> = both.lines().collect();
    assert_eq!(
        lines,
        ["base of demo: auto-detect", "run auto-start of demo: off"]
    );
    let bad = env.mk(&["project", "update", "demo", "--run-autostart", "maybe"]);
    assert!(!bad.status.success());
    env.ok(&["daemon", "stop"]);
}

#[test]
fn attach_detach_and_reattach_restores_the_screen() {
    let env = Env::new();
    env.write_fake_profile();
    let repo = env.dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    env.ok(&["daemon", "start"]);
    env.ok(&["project", "add", repo.to_str().unwrap(), "--name", "demo"]);
    assert!(env.ok(&["session", "list"]).contains("no sessions"));

    let id = env.ok(&["session", "spawn", "demo", "--agent", "fake"]);
    let id = id.trim();
    assert!(id.parse::<Ulid>().is_ok(), "{id}");

    // First attach: watch the agent work until it asks, then Ctrl+] detaches.
    let mut first = env.attach(id);
    first.wait_for("(y/n)");
    first.type_bytes(&[0x1d]);
    let first = first.finish();
    assert!(first.contains("detached from session"), "{first}");

    // The agent keeps running in the daemon.
    let list = env.ok(&["session", "list", "--project", "demo"]);
    let row = list.lines().find(|l| l.starts_with(id)).unwrap();
    assert!(row.contains("fake") && row.contains("demo"), "{list}");
    assert!(!row.contains("exited"), "{list}");

    // Re-attach: the snapshot alone (no new output while the agent waits) shows the screen.
    let mut second = env.attach(id);
    second.wait_for("Do you want to proceed? (y/n)");
    assert!(second.text().contains("Plan: edit src/main.rs"));
    // Stdin is forwarded; the agent finishes and attach ends with it.
    second.type_bytes(b"y\r");
    let second = second.finish();
    assert!(second.contains("Done."), "{second}");
    assert!(second.contains("exited with code 0"), "{second}");
}

/// The M1 E2E: a scripted answer reaches the fake agent and `mk wait` sees it finish.
#[test]
fn send_answers_the_agent_and_wait_returns_when_it_is_done() {
    let env = Env::new();
    env.write_fake_profile();
    let repo = env.dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    env.ok(&["daemon", "start"]);
    env.ok(&["project", "add", repo.to_str().unwrap(), "--name", "demo"]);
    let id = env.ok(&["session", "spawn", "demo", "--agent", "fake"]);
    let id = id.trim();

    // Sent before the agent asks: the daemon holds it until the prompt shows.
    env.ok(&["send", id, "y"]);
    let waited = env.ok(&["wait", id, "--until", "idle,done", "--timeout", "30000"]);
    // The fake agent exits right after its work, which also ends the wait.
    assert!(
        waited.trim() == "idle" || waited.trim() == "exited (exit code 0)",
        "{waited}"
    );
    let waited = env.ok(&["wait", id, "--until", "exited", "--timeout", "30000"]);
    assert_eq!(waited.trim(), "exited (exit code 0)");

    // Timeouts and unknown states fail with a message.
    let other = env.ok(&["session", "spawn", "demo", "--agent", "fake"]);
    let other = other.trim();
    env.ok(&[
        "wait",
        other,
        "--until",
        "needs_input",
        "--timeout",
        "30000",
    ]);
    let timed_out = env.mk(&["wait", other, "--until", "done", "--timeout", "200"]);
    assert!(!timed_out.status.success());
    assert!(
        String::from_utf8_lossy(&timed_out.stderr).contains("within 200 ms"),
        "{timed_out:?}"
    );
    let bad = env.mk(&["wait", other, "--until", "sleeping"]);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("unknown state"));
    // `--no-submit` types without Enter; the Enter then answers the prompt. The second send
    // waits for output after the first one: here the console's echo of the "y".
    env.ok(&["send", other, "y", "--no-submit"]);
    env.ok(&["send", other, ""]);
    let waited = env.ok(&["wait", other, "--until", "exited", "--timeout", "30000"]);
    assert_eq!(waited.trim(), "exited (exit code 0)");
}

/// Finished sessions — and one still running when the daemon stops — stay listed per project
/// after a restart, and `mk session history` prints their output logs.
#[test]
fn session_history_survives_a_daemon_restart() {
    let env = Env::new();
    env.write_fake_profile();
    let repo = env.dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    env.ok(&["daemon", "start"]);
    env.ok(&["project", "add", repo.to_str().unwrap(), "--name", "demo"]);
    let done = env.ok(&["session", "spawn", "demo", "--agent", "fake"]);
    let done = done.trim();
    env.ok(&["send", done, "y"]);
    env.ok(&["wait", done, "--until", "exited", "--timeout", "30000"]);
    let running = env.ok(&["session", "spawn", "demo", "--agent", "fake"]);
    let running = running.trim();
    let asked = ["--until", "needs_input", "--timeout", "30000"];
    env.ok(&[&["wait", running][..], &asked].concat());

    env.ok(&["daemon", "stop"]);
    env.ok(&["daemon", "start"]);
    let list = env.ok(&["session", "list", "--project", "demo"]);
    for id in [done, running] {
        let row = list.lines().find(|l| l.starts_with(id));
        assert!(row.is_some_and(|r| r.contains("exited")), "{list}");
        let history = env.ok(&["session", "history", id]);
        assert!(
            history.contains("Do you want to proceed? (y/n)"),
            "{history}"
        );
    }

    let unknown = env.mk(&["session", "history", &Ulid::new().to_string()]);
    assert!(!unknown.status.success());
    let stderr = String::from_utf8_lossy(&unknown.stderr);
    assert!(stderr.contains("no session with id"), "{stderr}");
}

/// `mk task add → list → run`: the fake agent gets the prompt as an argument, the card follows
/// its session to Review.
#[test]
fn task_add_list_run_reaches_review() {
    let env = Env::new();
    let arg_profile = fake_profile().replace("prompt_mode = \"none\"", "prompt_mode = \"arg\"");
    assert_ne!(arg_profile, fake_profile());
    env.write_profile("fake-arg", &arg_profile);
    let repo = env.dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    for args in [
        &["init", "-q"][..],
        // Byte-exact restores whatever the machine's global autocrlf says.
        &["config", "core.autocrlf", "false"],
        &["add", "."],
        &["commit", "-q", "-m", "one"],
    ] {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }
    env.ok(&["daemon", "start"]);
    env.ok(&["project", "add", repo.to_str().unwrap(), "--name", "demo"]);
    assert!(env.ok(&["task", "list"]).contains("no tasks"));

    let id = env.ok(&[
        "task",
        "add",
        "demo",
        "Fix the login bug",
        "--prompt",
        "fix login",
        "--agent",
        "fake-arg",
    ]);
    let id = id.trim();
    assert!(id.parse::<Ulid>().is_ok(), "{id}");
    let other = env.ok(&["task", "add", "demo", "Write docs"]);
    let other = other.trim();

    let list = env.ok(&["task", "list", "--project", "demo"]);
    let row = |list: &str, id: &str| list.lines().find(|l| l.starts_with(id)).unwrap().to_owned();
    let added = row(&list, id);
    assert!(
        added.contains("todo") && added.contains("fake-arg") && added.contains("Fix the login bug"),
        "{list}"
    );
    assert!(row(&list, other).contains("(default)"), "{list}");

    // No agent and no project default: the daemon refuses, the card stays in Todo.
    let refused = env.mk(&["task", "run", other]);
    assert!(!refused.status.success(), "{refused:?}");

    let session = env.ok(&["task", "run", id]);
    let session = session.trim();
    assert!(session.parse::<Ulid>().is_ok(), "{session}");
    let running = env.ok(&["task", "list", "--status", "running"]);
    assert!(
        running.contains(id) && !running.contains(other),
        "{running}"
    );

    let attach = env.attach(session);
    attach.wait_for("Task: fix login");
    attach.wait_for("(y/n)");
    env.ok(&["send", session, "y"]);
    attach.finish();

    // The card leaves Running once the session's exit is processed.
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let review = env.ok(&["task", "list", "--status", "review"]);
        if review.contains(id) {
            assert!(!row(&review, id).contains("review!"), "{review}");
            break;
        }
        assert!(Instant::now() < deadline, "task never reached review");
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        env.ok(&["task", "list", "--status", "todo"])
            .contains(other),
        "the refused task stays in todo"
    );
    let bad = env.mk(&["task", "list", "--status", "doing"]);
    assert!(String::from_utf8_lossy(&bad.stderr).contains("unknown status"));

    // Undo (ADR 0005): without `--yes` only the preview; with it the run's changes go away.
    std::fs::write(repo.join("a.txt"), "two\n").unwrap();
    std::fs::write(repo.join("made.txt"), "x\n").unwrap();
    let shown = env.ok(&["task", "undo", id]);
    assert!(
        shown.contains("restore  a.txt")
            && shown.contains("delete   made.txt")
            && shown.contains("+two")
            && shown.contains("--yes"),
        "{shown}"
    );
    assert!(repo.join("made.txt").exists());
    let done = env.ok(&["task", "undo", id, "--yes"]);
    assert!(done.contains("undone"), "{done}");
    assert_eq!(
        std::fs::read_to_string(repo.join("a.txt")).unwrap(),
        "one\n"
    );
    assert!(!repo.join("made.txt").exists());
    assert!(env.ok(&["task", "undo", id]).contains("no changes"));
    // A task that never ran has no checkpoint.
    assert!(!env.mk(&["task", "undo", other]).status.success());
}

/// Outside Mkrew (no `MKREW_SESSION_ID`) a hook does nothing, even without a daemon.
#[test]
fn hook_outside_mkrew_exits_silently() {
    let env = Env::new();
    for args in [&["hook", "claude", "Stop"][..], &["hook", "codex", "{"]] {
        let output = env
            .command(args)
            .env_remove("MKREW_SESSION_ID")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(
            output.stdout.is_empty() && output.stderr.is_empty(),
            "{output:?}"
        );
    }
}

/// A daemon of another protocol version: answers the first request (`hello`) with
/// `version_mismatch`, like `server.rs` `hello` does.
fn daemon_of_another_protocol(socket: &str) -> std::thread::JoinHandle<()> {
    use interprocess::local_socket::{ListenerOptions, traits::Listener as _};
    use mkrew_protocol::{ErrorCode, Frame, FrameDecoder, Message, Outcome, encode};
    let listener = ListenerOptions::new()
        .name(mkrew_daemon::ipc::name(socket).unwrap())
        .create_sync()
        .unwrap();
    std::thread::spawn(move || {
        let mut conn = listener.accept().unwrap();
        let mut decoder = FrameDecoder::default();
        let mut buf = [0u8; 4096];
        let id = loop {
            let n = conn.read(&mut buf).unwrap();
            decoder.push(&buf[..n]);
            if let Some(Frame::Message(Message::Request { id, .. })) = decoder.next_frame().unwrap()
            {
                break id;
            }
        };
        let message = "client speaks protocol 41, daemon speaks 40; restart the daemon";
        let reply = Message::Response {
            id,
            outcome: Outcome::error(ErrorCode::VersionMismatch, message),
        };
        conn.write_all(&encode(&Frame::Message(reply)).unwrap())
            .unwrap();
    })
}

fn hook_log(env: &Env) -> String {
    std::fs::read_to_string(env.dir.path().join("data").join("logs").join("hook.log"))
        .unwrap_or_default()
}

/// A hook that meets a daemon of another version fails (exit 1, never blocks) and says why
/// in `<data>/logs/hook.log`.
#[test]
fn hook_against_another_protocol_leaves_a_log_line() {
    let env = Env::new();
    let daemon = daemon_of_another_protocol(&env.socket);
    let output = env
        .command(&["hook", "omp", "busy", "{}"])
        .env("MKREW_SESSION_ID", Ulid::new().to_string())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    daemon.join().unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let log = hook_log(&env);
    assert!(
        log.contains("hook omp: client speaks protocol 41, daemon speaks 40"),
        "{log}"
    );
}

/// Inside a pane, `mk hook` runs the daemon's `mk` (`MKREW_MK`) when that is another file.
#[test]
fn hook_forwards_to_the_daemons_mk_and_logs_failures() {
    let env = Env::new();
    let copy = env
        .dir
        .path()
        .join("other bin")
        .join(format!("mk{}", std::env::consts::EXE_SUFFIX));
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_mk"), &copy).unwrap();
    // No daemon runs: the report fails, and the one line names the `mk` that tried.
    let output = env
        .command(&["hook", "omp", "busy", "{}"])
        .env("MKREW_SESSION_ID", Ulid::new().to_string())
        .env("MKREW_MK", &copy)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let log = hook_log(&env);
    let copy = dunce::canonicalize(&copy).unwrap();
    assert_eq!(log.lines().count(), 1, "{log}");
    assert!(log.contains(&copy.display().to_string()), "{log}");
}

/// `mk integration` on temp agent homes: install backs the file up once, repeating is a no-op,
/// uninstall restores the original content, status follows along.
#[test]
fn integration_install_status_uninstall() {
    let env = Env::new();
    let claude = env.dir.path().join("claude");
    let codex = env.dir.path().join("codex");
    let integration = |args: &[&str]| {
        env.command(args)
            .env("CLAUDE_CONFIG_DIR", &claude)
            .env("CODEX_HOME", &codex)
            .output()
            .unwrap()
    };
    let backups = |dir: &Path| {
        std::fs::read_dir(dir)
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".bak")
            })
            .count()
    };
    std::fs::create_dir_all(&claude).unwrap();
    let settings = claude.join("settings.json");
    let original = "{\n  \"model\": \"opus\"\n}\n";
    std::fs::write(&settings, original).unwrap();

    for agent in ["claude", "codex"] {
        let status = integration(&["integration", "status", agent]);
        assert_eq!(status.status.code(), Some(1), "{status:?}");
        assert!(String::from_utf8_lossy(&status.stdout).contains("not installed"));
        for _ in 0..2 {
            let install = integration(&["integration", "install", agent]);
            assert!(install.status.success(), "{install:?}");
        }
        let status = integration(&["integration", "status", agent]);
        assert!(status.status.success(), "{status:?}");
    }
    assert_eq!(backups(&claude), 1, "one backup, of the first change only");
    assert_eq!(backups(&codex), 0, "a new file has nothing to back up");
    let installed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    let hook = &installed["hooks"]["Notification"][0]["hooks"][0];
    assert_eq!(
        hook["args"],
        serde_json::json!(["hook", "claude", "Notification"])
    );
    let mk = hook["command"].as_str().unwrap();
    assert!(Path::new(mk).is_absolute(), "{hook}");
    let config: toml_edit::DocumentMut = std::fs::read_to_string(codex.join("config.toml"))
        .unwrap()
        .parse()
        .unwrap();
    let notify: Vec<_> = config["notify"].as_array().unwrap().iter().collect();
    assert_eq!(notify.len(), 3, "{config}");
    assert_eq!(notify[0].as_str(), Some(mk));
    assert_eq!(
        notify[1..]
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>(),
        ["hook", "codex"]
    );

    for agent in ["claude", "codex"] {
        let uninstall = integration(&["integration", "uninstall", agent]);
        assert!(uninstall.status.success(), "{uninstall:?}");
        let status = integration(&["integration", "status", agent]);
        assert_eq!(status.status.code(), Some(1), "{status:?}");
    }
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), original);
    assert_eq!(
        std::fs::read_to_string(codex.join("config.toml")).unwrap(),
        ""
    );

    // Invalid JSON is reported and left untouched.
    std::fs::write(&settings, "{").unwrap();
    let install = integration(&["integration", "install", "claude"]);
    assert!(!install.status.success());
    assert!(String::from_utf8_lossy(&install.stderr).contains("settings.json"));
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), "{");
}

/// The fake agent's `hook` steps drive the session (screen detection would not see the prompt),
/// and `mk hook` understands Claude's stdin and Codex's argument JSON.
#[test]
fn fake_agent_hooks_drive_the_session_state() {
    let env = Env::new();
    let scenario = env.dir.path().join("hooks.toml");
    std::fs::write(
        &scenario,
        r#"
        [[step]]
        action = "print"
        text = "Reading files"
        [[step]]
        action = "hook"
        event = "busy"
        [[step]]
        action = "hook"
        event = "needs_input"
        [[step]]
        action = "ask"
        text = "Continue?"
        [[step]]
        action = "hook"
        event = "busy"
        [[step]]
        action = "sleep"
        ms = 3000
        [[step]]
        action = "hook"
        event = "done"
        [[step]]
        action = "sleep"
        ms = 30000
        "#,
    )
    .unwrap();
    let profile = fake_profile()
        .replace("mode = \"screen\"", "mode = \"hook+screen\"")
        .replace(
            "args = []",
            &format!("args = ['--scenario', '{}']", scenario.display()),
        );
    assert!(profile.contains("hook+screen"), "{profile}");
    assert!(profile.contains("hooks.toml"), "{profile}");
    env.write_profile("hooked", &profile);
    let repo = env.dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    env.ok(&["daemon", "start"]);
    env.ok(&["project", "add", repo.to_str().unwrap(), "--name", "demo"]);

    // Agent start-up time is not part of the 2 s budget (measured from the answer on).
    let id = env.ok(&["session", "spawn", "demo", "--agent", "hooked"]);
    let id = id.trim();
    let waited = env.ok(&["wait", id, "--until", "needs_input", "--timeout", "10000"]);
    assert_eq!(waited.trim(), "needs_input");

    // The answer makes the fake agent run `mk hook fake busy`.
    let answered = Instant::now();
    env.ok(&["send", id, "y"]);
    let waited = env.ok(&["wait", id, "--until", "working", "--timeout", "2000"]);
    assert_eq!(waited.trim(), "working");
    assert!(answered.elapsed() < Duration::from_secs(2));
    let waited = env.ok(&["wait", id, "--until", "done", "--timeout", "10000"]);
    assert_eq!(waited.trim(), "done");

    // The agent now sleeps; report as Claude Code and Codex would.
    let reported = Instant::now();
    env.hook(
        id,
        &["claude", "Notification"],
        r#"{"session_id": "claude-1", "notification_type": "permission_prompt"}"#,
    );
    let waited = env.ok(&["wait", id, "--until", "needs_input", "--timeout", "2000"]);
    assert_eq!(waited.trim(), "needs_input");
    assert!(reported.elapsed() < Duration::from_secs(2));
    env.hook(id, &["claude", "UserPromptSubmit"], "{}");
    let waited = env.ok(&["wait", id, "--until", "working", "--timeout", "5000"]);
    assert_eq!(waited.trim(), "working");
    env.hook(
        id,
        &[
            "codex",
            r#"{"type": "agent-turn-complete", "thread-id": "t-1"}"#,
        ],
        "",
    );
    let waited = env.ok(&["wait", id, "--until", "done", "--timeout", "5000"]);
    assert_eq!(waited.trim(), "done");
}

impl Env {
    /// Creates a review comment as the UI would (there is no `mk` command for it); returns
    /// its id.
    fn create_comment(&self, project: &str, file: &str, lines: (u32, u32), body: &str) -> String {
        use interprocess::local_socket::prelude::*;
        use mkrew_protocol::{
            ClientKind, CommentCreateParams, DocComment, Frame, FrameDecoder, HelloParams, Message,
            Outcome, PROTOCOL_VERSION, Project, Request, encode,
        };
        let name = mkrew_daemon::ipc::name(&self.socket).unwrap();
        let mut stream = interprocess::local_socket::Stream::connect(name).unwrap();
        let mut decoder = FrameDecoder::new();
        let mut call = |id: u64, request: Request| -> Outcome {
            let frame = Frame::Message(Message::Request { id, request });
            stream.write_all(&encode(&frame).unwrap()).unwrap();
            let mut buf = [0u8; 4096];
            loop {
                while let Some(frame) = decoder.next_frame().unwrap() {
                    if let Frame::Message(Message::Response { id: got, outcome }) = frame
                        && got == id
                    {
                        return outcome;
                    }
                }
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0, "daemon closed the connection");
                decoder.push(&buf[..n]);
            }
        };
        let hello = HelloParams {
            client: ClientKind::Cli,
            protocol_version: PROTOCOL_VERSION,
        };
        call(1, Request::Hello(hello))
            .into_result::<serde_json::Value>()
            .unwrap();
        let projects: Vec<Project> = call(2, Request::ProjectList).into_result().unwrap();
        let project = projects.iter().find(|p| p.name == project).unwrap();
        let comment: DocComment = call(
            3,
            Request::CommentCreate(CommentCreateParams {
                project_id: project.id,
                file_path: file.into(),
                start_line: lines.0,
                end_line: lines.1,
                quote: "Use Redis".into(),
                body: body.into(),
                context: Default::default(),
            }),
        )
        .into_result()
        .unwrap();
        comment.id.to_string()
    }
}

/// An agent inside a session lists its project's review comments and answers them: the reply
/// is authored by the session's profile and the comment becomes `addressed`.
#[test]
fn comment_list_and_reply_from_inside_a_session() {
    let env = Env::new();
    env.write_fake_profile();
    let repo = env.dir.path().join("repo");
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    env.ok(&["daemon", "start"]);
    env.ok(&["project", "add", repo.to_str().unwrap(), "--name", "demo"]);
    let session = env.ok(&["session", "spawn", "demo", "--agent", "fake"]);
    let session = session.trim();
    let first = env.create_comment("demo", "docs/plan.md", (12, 14), "Why not in-memory?");
    let second = env.create_comment("demo", "docs/plan.md", (40, 40), "Add a rollback step.");
    env.create_comment("demo", "README.md", (1, 1), "Typo");
    assert!(first.starts_with("c_"), "{first}");

    // Inside the session: its project, file relative to the current directory.
    let in_session = |args: &[&str], cwd: &Path| {
        let output = env
            .command(args)
            .env("MKREW_SESSION_ID", session)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(output.status.success(), "mk {args:?}: {output:?}");
        String::from_utf8(output.stdout).unwrap()
    };
    let listed = in_session(
        &["comment", "list", "--file", "plan.md"],
        &repo.join("docs"),
    );
    assert_eq!(
        listed,
        format!(
            "[{first}] open docs/plan.md, lines 12–14, passage \"Use Redis\"\n\
             \x20 user: Why not in-memory?\n\
             [{second}] open docs/plan.md, line 40, passage \"Use Redis\"\n\
             \x20 user: Add a rollback step.\n"
        )
    );

    let replied = in_session(
        &["comment", "reply", &first, "Switched to in-memory"],
        &repo,
    );
    assert_eq!(replied.trim(), format!("replied to {first}"));
    // Without the `c_` prefix too.
    let bare = second.trim_start_matches("c_");
    in_session(&["comment", "reply", bare, "Added step 5"], &repo);

    // Outside any session: the project of the current directory; all of its files.
    let outside = |args: &[&str]| {
        let output = env
            .command(args)
            .env_remove("MKREW_SESSION_ID")
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(output.status.success(), "mk {args:?}: {output:?}");
        String::from_utf8(output.stdout).unwrap()
    };
    let listed = outside(&["comment", "list"]);
    for expected in [
        format!("[{first}] addressed docs/plan.md"),
        "  fake: Switched to in-memory".to_owned(),
        format!("[{second}] addressed docs/plan.md"),
        "  fake: Added step 5".to_owned(),
        "open README.md, line 1".to_owned(),
    ] {
        assert!(listed.contains(&expected), "no {expected:?} in:\n{listed}");
    }

    // A user reply (no session) reopens the addressed thread.
    outside(&["comment", "reply", &first, "Still too big"]);
    let listed = outside(&[
        "comment",
        "list",
        "--project",
        "demo",
        "--file",
        "docs/plan.md",
    ]);
    assert!(listed.contains(&format!("[{first}] open")), "{listed}");
    assert!(listed.contains("  user: Still too big"), "{listed}");

    let unknown = env.mk(&["comment", "reply", &format!("c_{}", Ulid::new()), "hi"]);
    assert!(!unknown.status.success());
    let stderr = String::from_utf8_lossy(&unknown.stderr);
    assert!(stderr.contains("not found"), "{stderr}");
}

#[test]
fn project_run_restart_and_stop_reach_the_daemon() {
    let env = Env::new();
    let repo = env.dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    env.ok(&["daemon", "start"]);
    env.ok(&["project", "add", repo.to_str().unwrap(), "--name", "demo"]);
    // Commands are set up in the UI; without any, run and restart fail and say why.
    for verb in ["run", "restart"] {
        let refused = env.mk(&["project", verb, "demo"]);
        assert!(!refused.status.success());
        assert!(
            String::from_utf8_lossy(&refused.stderr).contains("no run commands"),
            "{verb}"
        );
    }
    let unknown = env.mk(&["project", "stop", "demo", "BE"]);
    assert!(!unknown.status.success());
    let stopped = env.ok(&["project", "stop", "demo"]);
    assert_eq!(stopped.trim(), "stopped the run commands of demo");
    env.ok(&["daemon", "stop"]);
}

/// `mk web` on a free port, with its address and token read from stdout; killed on drop.
struct Web {
    child: Child,
    port: u16,
    token: String,
}

impl Env {
    /// Starts `mk web` (which starts the daemon when none runs). `--host` keeps `tailscale` out.
    fn web(&self) -> Web {
        let mut child = self
            .command(&["web", "--listen", "127.0.0.1:0", "--host", "test.invalid"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let (mut port, mut token) = (None, None);
        while port.is_none() || token.is_none() {
            let line = lines
                .next()
                .unwrap_or_else(|| panic!("mk web ended before printing its address"))
                .unwrap();
            if let Some(addr) = line.strip_prefix("Mkrew web is listening on http://") {
                port = Some(addr.rsplit(':').next().unwrap().parse().unwrap());
            }
            if let Some((_, t)) = line.split_once("#t=") {
                token = Some(t.trim().to_owned());
            }
        }
        // Keep reading so a later line never blocks `mk web` on a full pipe.
        std::thread::spawn(move || lines.for_each(drop));
        Web {
            child,
            port: port.unwrap(),
            token: token.unwrap(),
        }
    }
}

impl Drop for Web {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

type WebSocket = tungstenite::WebSocket<TcpStream>;

impl Web {
    /// `GET target` → (status, body).
    fn get(&self, target: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        write!(stream, "GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        let text = String::from_utf8_lossy(&response).into_owned();
        let status = text[9..12].parse().unwrap();
        let body = text
            .split_once("\r\n\r\n")
            .map(|(_, body)| body.to_owned())
            .unwrap_or_default();
        (status, body)
    }

    /// The error is boxed: `tungstenite::Error` is large (clippy `result_large_err`).
    fn socket(&self, token: &str) -> Result<WebSocket, Box<tungstenite::Error>> {
        let stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(20)))
            .unwrap();
        let url = format!("ws://127.0.0.1:{}/ws?t={token}", self.port);
        match tungstenite::client(url, stream) {
            Ok((ws, _)) => Ok(ws),
            Err(tungstenite::HandshakeError::Failure(e)) => Err(Box::new(e)),
            Err(tungstenite::HandshakeError::Interrupted(_)) => unreachable!("blocking stream"),
        }
    }
}

/// Reads text messages until one matches.
fn ws_until(
    ws: &mut WebSocket,
    mut wanted: impl FnMut(&serde_json::Value) -> bool,
) -> serde_json::Value {
    loop {
        if let tungstenite::Message::Text(text) = ws.read().unwrap() {
            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
            if wanted(&value) {
                return value;
            }
        }
    }
}

fn ws_request(ws: &mut WebSocket, id: u64, request: mkrew_protocol::Request) -> serde_json::Value {
    let text = serde_json::to_string(&mkrew_protocol::Message::Request { id, request }).unwrap();
    ws.send(tungstenite::Message::text(text)).unwrap();
    ws_until(ws, |v| v["type"] == "response" && v["id"] == id)
}

#[test]
fn web_serves_the_page_and_relays_a_session() {
    use mkrew_protocol::{
        Request, SessionInputParams, SessionListParams, SessionRef, SessionResizeParams,
    };
    let env = Env::new();
    env.write_fake_profile();
    let repo = env.dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    // No `mk daemon start`: `mk web` starts the daemon itself.
    let web = env.web();
    env.ok(&["project", "add", repo.to_str().unwrap(), "--name", "demo"]);
    let id = env
        .ok(&["session", "spawn", "demo", "--agent", "fake"])
        .trim()
        .to_owned();
    let session_id: Ulid = id.parse().unwrap();

    let (status, page) = web.get("/");
    assert_eq!(status, 200);
    assert!(page.contains("<title>Mkrew</title>"), "{page}");
    for asset in [
        "/app.mjs",
        "/model.mjs",
        "/app.css",
        "/vendor/xterm.mjs",
        "/vendor/xterm.css",
        "/apple-touch-icon.png",
    ] {
        assert_eq!(web.get(asset).0, 200, "{asset}");
    }
    assert_eq!(web.get("/nope").0, 404);
    // Without the token nothing reaches the daemon.
    let wrong = "0".repeat(64);
    assert_eq!(web.get("/check").0, 401);
    assert_eq!(web.get(&format!("/check?t={wrong}")).0, 401);
    assert_eq!(web.get(&format!("/check?t={}", web.token)).0, 204);
    match web.socket(&wrong).map(drop).map_err(|e| *e) {
        Err(tungstenite::Error::Http(response)) => assert_eq!(response.status(), 401),
        other => panic!("{other:?}"),
    }

    let mut ws = web.socket(&web.token).unwrap();
    // A request without params and one with params.
    let projects = ws_request(&mut ws, 1, Request::ProjectList);
    assert_eq!(projects["result"][0]["name"], "demo", "{projects}");
    let sessions = ws_request(
        &mut ws,
        2,
        Request::SessionList(SessionListParams::default()),
    );
    assert!(
        sessions["result"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == id.as_str()),
        "{sessions}"
    );
    // The phone never resizes a session.
    let resize = ws_request(
        &mut ws,
        3,
        Request::SessionResize(SessionResizeParams {
            session_id,
            cols: 40,
            rows: 20,
        }),
    );
    assert_eq!(resize["error"]["code"], "invalid_request", "{resize}");
    // A bad message gets an answer and the connection keeps working.
    ws.send(tungstenite::Message::text(
        r#"{"type":"request","id":4,"method":"no.such"}"#,
    ))
    .unwrap();
    let broken = ws_until(&mut ws, |v| v["id"] == 4);
    assert_eq!(broken["error"]["code"], "invalid_request", "{broken}");

    // Attach: the reply carries the painted screen; output comes behind the session id.
    let attach = mkrew_protocol::Message::Request {
        id: 5,
        request: Request::SessionAttach(SessionRef { session_id }),
    };
    ws.send(tungstenite::Message::text(
        serde_json::to_string(&attach).unwrap(),
    ))
    .unwrap();
    let mut screen = String::new();
    let deadline = Instant::now() + Duration::from_secs(20);
    while !screen.contains("(y/n)") {
        assert!(Instant::now() < deadline, "no question on screen: {screen}");
        match ws.read().unwrap() {
            tungstenite::Message::Text(text) => {
                let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                if value["id"] == 5 {
                    let ansi = value["result"]["ansi"]
                        .as_str()
                        .unwrap_or_else(|| panic!("attach reply has no ansi: {value}"));
                    screen.push_str(ansi);
                }
            }
            tungstenite::Message::Binary(bytes) => {
                assert_eq!(&bytes[..26], id.as_bytes(), "output of another session");
                screen.push_str(&String::from_utf8_lossy(&bytes[26..]));
            }
            _ => {}
        }
    }
    // Answer as the `y` and Enter quick keys do; the exit reaches the page.
    let input = ws_request(
        &mut ws,
        6,
        Request::SessionInput(SessionInputParams {
            session_id,
            data: b"y\r".to_vec(),
        }),
    );
    assert!(input.get("error").is_none(), "{input}");
    let exited = ws_until(&mut ws, |v| {
        v["event"] == "session.exited" && v["data"]["session_id"] == id.as_str()
    });
    assert_eq!(exited["data"]["exit_code"], 0, "{exited}");

    // An ended session: its log comes back as output, before the `session.history` reply.
    let history = mkrew_protocol::Message::Request {
        id: 7,
        request: Request::SessionHistory(SessionRef { session_id }),
    };
    ws.send(tungstenite::Message::text(
        serde_json::to_string(&history).unwrap(),
    ))
    .unwrap();
    let mut replayed = Vec::new();
    loop {
        match ws.read().unwrap() {
            tungstenite::Message::Binary(bytes) => {
                assert_eq!(&bytes[..26], id.as_bytes(), "output of another session");
                replayed.extend_from_slice(&bytes[26..]);
            }
            tungstenite::Message::Text(text) => {
                let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                if value["id"] == 7 {
                    assert!(value.get("error").is_none(), "{value}");
                    break;
                }
            }
            _ => {}
        }
    }
    let replayed = String::from_utf8_lossy(&replayed);
    assert!(
        replayed.contains("(y/n)"),
        "the log tail misses the question: {replayed}"
    );
}

#[test]
fn web_answers_503_with_the_reason_when_the_daemon_is_gone() {
    let env = Env::new();
    let web = env.web();
    env.ok(&["daemon", "stop"]);
    // No WebSocket opens, so the page keeps its "Can't reach Mkrew" banner and backs off.
    match web.socket(&web.token).map(drop).map_err(|e| *e) {
        Err(tungstenite::Error::Http(response)) => {
            assert_eq!(response.status(), 503);
            let body = String::from_utf8_lossy(response.body().as_deref().unwrap_or_default())
                .into_owned();
            assert!(body.contains("not running"), "{body}");
        }
        other => panic!("{other:?}"),
    }
}
