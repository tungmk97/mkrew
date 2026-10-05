//! Runs the real `mk` binary against a daemon in a temp dir.
//!
//! `mk daemon start` spawns the `mkrew-daemon` binary next to `mk`, so it must be built first
//! (`cargo test --workspace` does; `cargo test -p mkrew-cli` alone may not).

// Helpers outside `#[test]` fns are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::{Command, Output, Stdio};

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
            .env("MKREW_SOCKET", &self.socket);
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
