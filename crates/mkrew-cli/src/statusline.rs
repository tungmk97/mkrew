//! `mk statusline` (spec 2026-11-22 section 5.3): Claude Code's status line command, set up by
//! `mk integration install claude`. Claude pipes a JSON document to it on every update. It sends
//! the account's rate limits to the daemon (`limits.report`, a failure is silent), then runs
//! the user's own status line, if they had one, with the same stdin and prints its output.
//! Nothing here may fail or slow Claude down: every error means "nothing", the exit code is 0.

use std::io::{Read, Write};
#[cfg(any(windows, test))]
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mkrew_core::Paths;
use mkrew_protocol::{AgentLimits, Cli, LimitWindow, Request};
use serde_json::Value;

/// Claude's document is a few KB; anything past this is not read.
const STDIN_CAP: u64 = 1024 * 1024;
/// Connect and send, together.
const REPORT_BUDGET: Duration = Duration::from_millis(300);
/// The user's own status line gets this long.
const CHAIN_TIMEOUT: Duration = Duration::from_secs(5);

pub fn run() -> ExitCode {
    let mut input = Vec::new();
    // A read error keeps what came before it.
    let _ = std::io::stdin().take(STDIN_CAP).read_to_end(&mut input);
    if let Some(limits) = limits_of(&input, now_secs()) {
        report(limits);
    }
    let saved = crate::integration::saved_statusline_file()
        .and_then(|file| std::fs::read_to_string(file).ok());
    if let Some(command) = saved.as_deref().and_then(saved_command)
        && let Some(output) = chain(&command, input, CHAIN_TIMEOUT)
    {
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(&output);
        let _ = stdout.flush();
    }
    ExitCode::SUCCESS
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// The limits in Claude's status line document, `None` when it has none (API-key users and
/// older Claude Code send no `rate_limits`), is not JSON, or has no usable window.
fn limits_of(input: &[u8], now: i64) -> Option<AgentLimits> {
    let json: Value = serde_json::from_slice(input).ok()?;
    let rate = json.get("rate_limits")?;
    let five_hour = window(&rate["five_hour"]);
    let weekly = window(&rate["seven_day"]);
    (five_hour.is_some() || weekly.is_some()).then_some(AgentLimits {
        agent: Cli::Claude,
        five_hour,
        weekly,
        observed_at: now,
    })
}

fn window(value: &Value) -> Option<LimitWindow> {
    let used = value["used_percentage"]
        .as_f64()
        .filter(|u| u.is_finite())?;
    Some(LimitWindow {
        used_percent: used as f32,
        resets_at: value["resets_at"].as_f64().map(|at| at as i64),
    })
}

/// Sends `limits` to the daemon the way `mk hook` reaches it (the local socket), within
/// [`REPORT_BUDGET`]; without a daemon, or too slowly, the report is dropped.
fn report(limits: AgentLimits) {
    let Ok(paths) = Paths::from_env() else {
        return;
    };
    let (done, wait) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let sent = crate::project::connect(&paths)
            .and_then(|mut client| client.call::<()>(Request::LimitsReport(limits)));
        let _ = done.send(sent.is_ok());
    });
    let _ = wait.recv_timeout(REPORT_BUDGET);
}

/// The `command` of the user's saved `statusLine`; `None` for anything that is not a command
/// status line (an unreadable saved file is "no chained command", spec section 8).
fn saved_command(text: &str) -> Option<String> {
    let line: Value = serde_json::from_str(text).ok()?;
    if line["type"] != "command" {
        return None;
    }
    let command = line["command"].as_str()?.trim();
    (!command.is_empty()).then(|| command.to_owned())
}

/// Git for Windows' `bash.exe`, the shell Claude Code runs status lines in: the file
/// `CLAUDE_CODE_GIT_BASH_PATH` names, else `<git root>/bin/bash.exe` for a `git.exe` in
/// `<git root>/cmd` or `<git root>/bin` (never `System32\bash.exe`, which is WSL). `None`
/// means PowerShell, which Claude uses when Git Bash is absent.
#[cfg(any(windows, test))]
fn git_bash(
    env: Option<&str>,
    git: Option<&Path>,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let set = env.map(Path::new).filter(|path| exists(path));
    if let Some(path) = set {
        return Some(path.to_owned());
    }
    let dir = git?.parent()?;
    let name = dir.file_name()?.to_str()?;
    if !name.eq_ignore_ascii_case("cmd") && !name.eq_ignore_ascii_case("bin") {
        return None;
    }
    let bash = dir.parent()?.join("bin").join("bash.exe");
    exists(&bash).then_some(bash)
}

#[cfg(windows)]
fn shell(command: &str) -> Command {
    let env = std::env::var("CLAUDE_CODE_GIT_BASH_PATH").ok();
    let git = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join("git.exe"))
            .find(|file| file.is_file())
    });
    match git_bash(env.as_deref(), git.as_deref(), Path::is_file) {
        Some(bash) => {
            let mut shell = Command::new(bash);
            shell.arg("-c").arg(command);
            shell
        }
        None => {
            let mut shell = Command::new("powershell");
            shell.args(["-NoProfile", "-Command"]).arg(command);
            shell
        }
    }
}

#[cfg(not(windows))]
fn shell(command: &str) -> Command {
    use std::os::unix::process::CommandExt;
    let mut shell = Command::new("sh");
    // Its own group, so a timeout can end the commands it started too.
    shell.arg("-c").arg(command).process_group(0);
    shell
}

/// Everything the status line command started, so that a timeout can end all of it.
#[cfg(not(windows))]
struct Tree(u32);

#[cfg(not(windows))]
impl Tree {
    fn new(child: &std::process::Child) -> Self {
        Self(child.id())
    }

    fn kill(self) {
        use rustix::process::{Pid, Signal, kill_process_group};
        if let Some(pid) = i32::try_from(self.0).ok().and_then(Pid::from_raw) {
            let _ = kill_process_group(pid, Signal::KILL);
        }
    }
}

/// A kill-on-close job object holding the command: closing it ends the whole tree.
#[cfg(windows)]
#[allow(unsafe_code)]
mod tree {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    /// `None` when Windows refused the job: then only the shell itself can be killed.
    pub struct Tree(Option<HANDLE>);

    impl Tree {
        pub fn new(child: &Child) -> Self {
            // SAFETY: plain Win32 calls with valid arguments; `info` outlives the call and the
            // child's handle stays open while `child` lives.
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if job.is_null() {
                    return Self(None);
                }
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let size = std::mem::size_of_val(&info) as u32;
                let info_ptr = std::ptr::addr_of!(info).cast();
                let set =
                    SetInformationJobObject(job, JobObjectExtendedLimitInformation, info_ptr, size);
                if set == 0 || AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE) == 0 {
                    CloseHandle(job);
                    return Self(None);
                }
                Self(Some(job))
            }
        }

        pub fn kill(self) {}
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            if let Some(job) = self.0 {
                // SAFETY: the handle is ours and closed once.
                unsafe { CloseHandle(job) };
            }
        }
    }
}

#[cfg(windows)]
use tree::Tree;

/// Runs `command` through the shell with `input` on stdin and returns its stdout; `None` when
/// it cannot start, exits with an error, or is not done within `timeout` (it and what it
/// started are killed then).
fn chain(command: &str, input: Vec<u8>, timeout: Duration) -> Option<Vec<u8>> {
    let deadline = Instant::now() + timeout;
    let mut child = shell(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let tree = Tree::new(&child);
    let mut stdin = child.stdin.take()?;
    let mut stdout = child.stdout.take()?;
    // A command that never reads its stdin closes the pipe under the write: not an error.
    std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let (done, output) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.read_to_end(&mut out);
        let _ = done.send(out);
    });
    if let Ok(out) = output.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        while Instant::now() < deadline {
            match child.try_wait() {
                Ok(Some(status)) => return status.success().then_some(out),
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(_) => break,
            }
        }
    }
    tree.kill();
    let _ = child.kill();
    let _ = child.wait();
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A command that copies its stdin to stdout, in `bash`, `powershell` and `sh`.
    #[cfg(windows)]
    const ECHO_STDIN: &str = r#"findstr "^""#;
    #[cfg(not(windows))]
    const ECHO_STDIN: &str = "cat";
    /// Runs far longer than any test waits, in `bash`, `powershell` and `sh`.
    #[cfg(windows)]
    const SLEEP_LONG: &str = "ping -n 31 127.0.0.1";
    #[cfg(not(windows))]
    const SLEEP_LONG: &str = "sleep 30";
    /// For tests of what `chain` reads and writes, not of how fast: Git Bash needs more than
    /// five seconds to start when the machine is saturated (measured up to 16 s), so a test
    /// that waited for the production timeout would fail on a busy machine.
    const SLOW_START: Duration = Duration::from_secs(60);

    const FULL: &str = r#"{"session_id":"s1","rate_limits":{"five_hour":{"used_percentage":42.5,"resets_at":1778533631},"seven_day":{"used_percentage":18,"resets_at":1778571719}}}"#;

    fn window(percent: f32, at: i64) -> Option<LimitWindow> {
        Some(LimitWindow {
            used_percent: percent,
            resets_at: Some(at),
        })
    }

    #[test]
    fn both_windows_are_read() {
        let limits = limits_of(FULL.as_bytes(), 77).unwrap();
        assert_eq!(
            limits,
            AgentLimits {
                agent: Cli::Claude,
                five_hour: window(42.5, 1_778_533_631),
                weekly: window(18.0, 1_778_571_719),
                observed_at: 77,
            }
        );
    }

    #[test]
    fn one_window_is_enough_and_a_missing_reset_is_kept_open() {
        let json = br#"{"rate_limits":{"seven_day":{"used_percentage":3.5}}}"#;
        let limits = limits_of(json, 1).unwrap();
        assert_eq!(limits.five_hour, None);
        assert_eq!(
            limits.weekly,
            Some(LimitWindow {
                used_percent: 3.5,
                resets_at: None
            })
        );
    }

    #[test]
    fn a_payload_without_limits_sends_nothing() {
        for input in [
            &b""[..],
            b"not json",
            b"{}",
            br#"{"session_id":"s1"}"#,
            br#"{"rate_limits":{}}"#,
            br#"{"rate_limits":{"five_hour":{"used_percentage":"high"}}}"#,
            br#"{"rate_limits":null}"#,
            b"[1,2]",
        ] {
            assert_eq!(
                limits_of(input, 1),
                None,
                "{}",
                String::from_utf8_lossy(input)
            );
        }
    }

    #[test]
    fn the_saved_command_is_the_users_status_line_command() {
        let line = serde_json::json!({"type": "command", "command": "line.sh", "padding": 0});
        assert_eq!(saved_command(&line.to_string()), Some("line.sh".to_owned()));
        for other in [
            r#"{"type":"text","command":"x"}"#,
            r#"{"command":""}"#,
            r#"{"type":"command"}"#,
            r#""line.sh""#,
            "{",
            "",
        ] {
            assert_eq!(saved_command(other), None, "{other}");
        }
    }

    #[test]
    fn the_chained_command_gets_the_same_stdin_and_its_output_is_kept() {
        let out = chain(ECHO_STDIN, FULL.as_bytes().to_vec(), SLOW_START).unwrap();
        assert_eq!(String::from_utf8_lossy(&out).trim(), FULL);
    }

    #[test]
    fn a_command_that_ignores_its_stdin_still_answers() {
        let big = vec![b'x'; 1024 * 1024];
        let out = chain("echo hi", big, SLOW_START).unwrap();
        assert_eq!(String::from_utf8_lossy(&out).trim(), "hi");
    }

    #[test]
    fn a_failing_or_missing_command_prints_nothing() {
        assert_eq!(
            chain("exit 3", b"{}".to_vec(), Duration::from_secs(5)),
            None
        );
        assert_eq!(
            chain(
                "mkrew-no-such-program-xyz",
                b"{}".to_vec(),
                Duration::from_secs(5)
            ),
            None
        );
    }

    #[test]
    fn a_command_past_its_timeout_is_stopped() {
        let started = std::time::Instant::now();
        assert_eq!(
            chain(SLEEP_LONG, b"{}".to_vec(), Duration::from_millis(300)),
            None
        );
        // Stopped at the timeout, not left to run its 30 s: the margin is for a busy machine.
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "{:?}",
            started.elapsed()
        );
    }

    fn bash_for(env: Option<&str>, git: Option<&str>, files: &[&str]) -> Option<PathBuf> {
        git_bash(env, git.map(Path::new), |path| {
            files.iter().any(|file| Path::new(file) == path)
        })
    }

    #[test]
    fn the_shell_is_the_configured_git_bash() {
        let bash = bash_for(
            Some("D:/tools/bash.exe"),
            Some("C:/Git/cmd/git.exe"),
            &["D:/tools/bash.exe", "C:/Git/bin/bash.exe"],
        );
        assert_eq!(bash, Some(PathBuf::from("D:/tools/bash.exe")));
    }

    #[test]
    fn a_configured_bash_that_is_missing_falls_back_to_git() {
        let bash = bash_for(
            Some("D:/gone/bash.exe"),
            Some("C:/Git/cmd/git.exe"),
            &["C:/Git/bin/bash.exe"],
        );
        assert_eq!(bash, Some(PathBuf::from("C:/Git/bin/bash.exe")));
    }

    #[test]
    fn the_shell_is_found_from_git_in_cmd_or_bin() {
        let files = ["C:/Git/bin/bash.exe"];
        for git in [
            "C:/Git/cmd/git.exe",
            "C:/Git/bin/git.exe",
            "C:/Git/CMD/git.exe",
        ] {
            assert_eq!(
                bash_for(None, Some(git), &files),
                Some(PathBuf::from("C:/Git/bin/bash.exe")),
                "{git}"
            );
        }
    }

    #[test]
    fn without_git_bash_the_shell_is_powershell() {
        let files = ["C:/Git/bin/bash.exe"];
        // No git, a git outside `cmd`/`bin`, and a git root without bash.
        assert_eq!(bash_for(None, None, &files), None);
        assert_eq!(bash_for(None, Some("C:/Git/mingw64/git.exe"), &files), None);
        assert_eq!(bash_for(None, Some("C:/Other/cmd/git.exe"), &files), None);
        assert_eq!(bash_for(Some(""), Some("git.exe"), &files), None);
    }
}
