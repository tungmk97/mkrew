//! Runs the real `mkrew-fakeagent` binary with piped stdio (no PTY).
#![allow(clippy::unwrap_used)]

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn fakeagent(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mkrew-fakeagent"))
        .args(args)
        .env_remove("MKREW_FAKE_SCENARIO")
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
    child.wait_with_output().unwrap()
}

#[test]
fn default_ask_scenario_runs_to_done() {
    let out = fakeagent(&[], "y\n");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("Do you want to proceed? (y/n) "),
        "{stdout}"
    );
    assert!(stdout.contains("Done."), "{stdout}");
}

#[test]
fn broken_scenario_reports_file_and_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.toml");
    std::fs::write(
        &path,
        "[[step]]\naction = \"print\"\ntext = \"a\"\nbogus = 1\n",
    )
    .unwrap();
    let out = fakeagent(&["--scenario", path.to_str().unwrap()], "");
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("bad.toml"), "{stderr}");
    assert!(stderr.contains("line "), "{stderr}");
    assert!(stderr.contains("bogus"), "{stderr}");
}
