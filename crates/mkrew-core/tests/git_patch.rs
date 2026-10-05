//! Hunk patches built by `mkrew_core::git` applied by real `git` on a temp repo.

// Helpers outside `#[test]` fns are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use mkrew_core::git::{hunk_patch, parse_diff, parse_status};
use mkrew_protocol::ChangeKind;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn git(dir: &Path, args: &[&str]) -> String {
    git_with(dir, args, None)
}

fn git_with(dir: &Path, args: &[&str], stdin: Option<&str>) -> String {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        // Ignore the developer's git config (signing, hooks, whitespace rules, autocrlf).
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join("..").join("empty.gitconfig"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.unwrap_or("").as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn diff(dir: &Path, cached: bool) -> String {
    let mut args = vec!["diff", "--src-prefix=a/", "--dst-prefix=b/"];
    if cached {
        args.push("--cached");
    }
    git(dir, &args)
}

#[test]
fn one_hunk_stages_and_unstages_with_crlf_and_no_final_newline() {
    let tmp = tempfile::tempdir().unwrap();
    // The repo is a subfolder so the (missing) `../empty.gitconfig` stays inside `tmp`.
    let dir = &tmp.path().join("repo");
    std::fs::create_dir(dir).unwrap();
    git(dir, &["init", "-q"]);
    git(dir, &["config", "core.autocrlf", "false"]);
    let lines: Vec<String> = (1..=20).map(|n| format!("line {n}")).collect();
    let original = lines.join("\r\n");
    std::fs::write(dir.join("a b.txt"), &original).unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "init"]);

    // Two separate hunks: one near the top, one changing the last (newline-less) line.
    let mut changed = lines.clone();
    changed[1] = "line two".into();
    changed[19] = "line twenty".into();
    std::fs::write(dir.join("a b.txt"), changed.join("\r\n")).unwrap();

    let status = parse_status(git(dir, &["status", "--porcelain=v2", "--branch", "-z"]).as_bytes());
    assert_eq!(status.unstaged.len(), 1);
    assert_eq!(status.unstaged[0].kind, ChangeKind::Modified);
    assert!(status.head.is_some() && status.staged.is_empty());

    let files = parse_diff(&diff(dir, false));
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "a b.txt");
    assert_eq!(files[0].hunks.len(), 2);
    let last = &files[0].hunks[1];
    assert!(last.lines.iter().any(|l| l.no_newline));
    assert!(last.lines.iter().all(|l| l.crlf || l.no_newline));

    // Stage only the second hunk.
    git_with(
        dir,
        &["apply", "--cached", "-"],
        Some(&hunk_patch("a b.txt", last)),
    );
    let staged = parse_diff(&diff(dir, true));
    assert_eq!(staged[0].hunks.len(), 1);
    assert_eq!(staged[0].hunks[0].lines, last.lines);
    let unstaged = parse_diff(&diff(dir, false));
    assert_eq!(unstaged[0].hunks, vec![files[0].hunks[0].clone()]);

    // Unstage it again: the staged diff's hunk applied in reverse.
    git_with(
        dir,
        &["apply", "--cached", "-R", "-"],
        Some(&hunk_patch("a b.txt", &staged[0].hunks[0])),
    );
    assert_eq!(diff(dir, true), "");
    assert_eq!(parse_diff(&diff(dir, false)), files);
}
