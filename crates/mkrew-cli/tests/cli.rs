//! Runs the real `mk` binary against a daemon in a temp dir.
//!
//! `mk daemon start` spawns the `mkrew-daemon` binary next to `mk`, so it must be built first
//! (`cargo test --workspace` does; `cargo test -p mkrew-cli` alone may not).

// Helpers outside `#[test]` fns are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use std::io::{Read, Write};
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
    let installed = std::fs::read_to_string(&settings).unwrap();
    assert!(
        installed.contains("mk hook claude Notification"),
        "{installed}"
    );
    assert_eq!(
        std::fs::read_to_string(codex.join("config.toml")).unwrap(),
        "notify = [\"mk\", \"hook\", \"codex\"]\n"
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
