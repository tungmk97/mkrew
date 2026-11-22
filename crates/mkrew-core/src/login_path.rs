//! The login shell's `PATH` for an app started from Finder or the Dock (macOS, SPEC §20L,
//! ADR 0100). launchd gives such an app `/usr/bin:/bin:/usr/sbin:/sbin`, so `claude`, `codex`
//! and `zed` in `/opt/homebrew/bin` or `~/.local/bin` are not found. A terminal's `PATH` is
//! built by the login shell's `.zprofile` and `.zshrc`; asking that shell once gives the same
//! list.
//!
//! The process environment is never changed (`std::env::set_var` is `unsafe`): [`path`] hands
//! the merged list to the callers that look programs up ([`crate::lookup`]) or start agents.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::time::Duration;

const BEGIN: &str = "__MKREW_PATH_BEGIN__";
const END: &str = "__MKREW_PATH_END__";
/// Prints `$PATH` between the markers, so a banner or `motd` of the shell's rc files around it
/// is ignored. `printf` and `"$PATH"` work in zsh, bash and fish.
const SCRIPT: &str = "printf '%s%s%s' __MKREW_PATH_BEGIN__ \"$PATH\" __MKREW_PATH_END__";
/// Longest wait for the shell: a `.zshrc` with version managers takes 1 to 2 s.
const TIMEOUT: Duration = Duration::from_secs(5);
/// How long a shell that has answered gets to exit by itself before it is killed.
const EXIT_GRACE: Duration = Duration::from_millis(500);

/// The `PATH` to look programs up on and to give agents: on macOS the login shell's list in
/// front of this process's own; `None` elsewhere (the process's own is right) or when the
/// shell gave no answer in time. The first call waits for the shell; [`warm`] makes that happen
/// on a thread at start.
pub fn path() -> Option<&'static OsStr> {
    static PATH: LazyLock<Option<OsString>> = LazyLock::new(|| {
        if !cfg!(target_os = "macos") {
            return None;
        }
        let shell = std::env::var_os("SHELL");
        let current = std::env::var_os("PATH");
        probe(shell.as_deref(), current.as_deref(), |command| {
            run(command, TIMEOUT)
        })
    });
    PATH.as_deref()
}

/// Starts [`path`] on its own thread so the first lookup does not wait for the shell.
pub fn warm() {
    if cfg!(target_os = "macos") {
        let _ = std::thread::Builder::new()
            .name("mkrew-login-path".into())
            .spawn(|| {
                let _ = path();
            });
    }
}

/// Asks `shell` (zsh when unset: the macOS default since Catalina) through `run` and merges
/// its answer in front of `current`.
fn probe(
    shell: Option<&OsStr>,
    current: Option<&OsStr>,
    run: impl FnOnce(Command) -> Option<String>,
) -> Option<OsString> {
    let shell = shell
        .filter(|shell| !shell.is_empty())
        .unwrap_or(OsStr::new("/bin/zsh"));
    let stdout = run(shell_command(Path::new(shell)))?;
    Some(merge(parse(&stdout)?, current))
}

/// `shell -l -i -c <script>`: login and interactive, so both `.zprofile` and `.zshrc` run (VS
/// Code and Zed do the same). The variables keep a framework such as oh-my-zsh from asking
/// questions in a shell nobody sees.
fn shell_command(shell: &Path) -> Command {
    let mut command = Command::new(shell);
    command
        .args(["-l", "-i", "-c", SCRIPT])
        .env("MKREW_RESOLVING_ENVIRONMENT", "1")
        .env("DISABLE_AUTO_UPDATE", "true");
    command
}

/// The text between the markers; `None` for no markers, an unfinished answer or an empty `PATH`.
fn parse(stdout: &str) -> Option<&str> {
    let start = stdout.find(BEGIN)? + BEGIN.len();
    let len = stdout[start..].find(END)?;
    let path = stdout[start..start + len].trim();
    (!path.is_empty()).then_some(path)
}

/// `login` first, then the entries of `current` that it lacks.
fn merge(login: &str, current: Option<&OsStr>) -> OsString {
    let mut dirs: Vec<PathBuf> = std::env::split_paths(login).collect();
    for dir in current.map(std::env::split_paths).into_iter().flatten() {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    std::env::join_paths(dirs).unwrap_or_else(|_| OsString::from(login))
}

/// Runs `command` with no input and returns what it printed up to the end marker; `None` when it
/// does not start or says nothing within `timeout`. The shell is killed either way (it may still
/// be running the rest of its rc files). ponytail: a background job an rc file started may keep
/// the pipe's reader thread alive; it holds nothing else.
fn run(mut command: Command, timeout: Duration) -> Option<String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut out = child.stdout.take()?;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut seen, mut chunk) = (Vec::new(), [0u8; 4096]);
        while let Ok(read) = out.read(&mut chunk) {
            if read == 0 {
                break;
            }
            seen.extend_from_slice(&chunk[..read]);
            if String::from_utf8_lossy(&seen).contains(END) {
                break;
            }
        }
        let _ = sender.send(String::from_utf8_lossy(&seen).into_owned());
    });
    let text = receiver.recv_timeout(timeout).ok();
    // After a good answer the shell is about to exit by itself: let it restore the terminal it
    // may share; kill it only when it is still running (or on a timeout).
    let until = std::time::Instant::now() + EXIT_GRACE;
    while text.is_some()
        && matches!(child.try_wait(), Ok(None))
        && std::time::Instant::now() < until
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    if !matches!(child.try_wait(), Ok(Some(_))) {
        let _ = child.kill();
        let _ = child.wait();
    }
    text
}

#[cfg(test)]
mod tests {
    use std::ffi::{OsStr, OsString};
    use std::path::PathBuf;
    use std::process::Command;

    use super::*;

    fn joined(dirs: &[&str]) -> OsString {
        std::env::join_paths(dirs).unwrap()
    }

    fn dirs(path: &OsStr) -> Vec<PathBuf> {
        std::env::split_paths(path).collect()
    }

    #[test]
    fn the_text_between_the_markers_is_the_path_whatever_the_shell_prints() {
        let out =
            format!("Last login: today\nwelcome\n{BEGIN}/opt/homebrew/bin:/usr/bin{END}\nbye\n");
        assert_eq!(parse(&out), Some("/opt/homebrew/bin:/usr/bin"));
        assert_eq!(parse("no markers"), None);
        assert_eq!(
            parse(&format!("{BEGIN}{END}")),
            None,
            "an empty PATH is no answer"
        );
        assert_eq!(parse(&format!("{BEGIN}/a")), None, "cut off before the end");
        assert_eq!(
            parse(&format!("{END}{BEGIN}/a{END}")),
            Some("/a"),
            "an end marker before the begin is noise"
        );
    }

    #[test]
    fn the_login_list_comes_first_and_the_rest_is_kept_once() {
        let login = joined(&["/opt/homebrew/bin", "/usr/bin"]);
        let current = joined(&["/usr/bin", "/bin", "/opt/homebrew/bin"]);
        let merged = merge(login.to_str().unwrap(), Some(current.as_os_str()));
        assert_eq!(
            dirs(&merged),
            [
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin")
            ]
        );
        let alone = merge(login.to_str().unwrap(), None);
        assert_eq!(dirs(&alone).len(), 2);
    }

    #[test]
    fn the_probe_runs_the_users_shell_as_a_login_shell_and_merges_its_answer() {
        let login = joined(&["/opt/homebrew/bin"]);
        let current = joined(&["/usr/bin"]);
        let answer = format!("noise{BEGIN}{}{END}", login.to_str().unwrap());
        let found = probe(
            Some(OsStr::new("/bin/fish")),
            Some(current.as_os_str()),
            |command| {
                assert_eq!(command.get_program(), "/bin/fish");
                let args: Vec<_> = command.get_args().collect();
                assert_eq!(args[..3], ["-l", "-i", "-c"]);
                assert!(args[3].to_string_lossy().contains(BEGIN));
                Some(answer)
            },
        )
        .unwrap();
        assert_eq!(
            dirs(&found),
            [
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/bin")
            ]
        );
    }

    #[test]
    fn an_unset_or_empty_shell_is_zsh_and_a_silent_shell_is_no_answer() {
        for shell in [None, Some(OsStr::new(""))] {
            let program = std::cell::RefCell::new(OsString::new());
            let _ = probe(shell, None, |command: Command| {
                *program.borrow_mut() = command.get_program().to_owned();
                None
            });
            assert_eq!(*program.borrow(), OsString::from("/bin/zsh"));
        }
        assert_eq!(
            probe(None, None, |_| None),
            None,
            "no answer: use the process PATH"
        );
        assert_eq!(
            probe(None, None, |_| Some("only a banner".into())),
            None,
            "an answer without the markers"
        );
    }

    #[test]
    fn the_script_prints_both_markers() {
        assert!(SCRIPT.contains(BEGIN) && SCRIPT.contains(END));
    }

    #[cfg(unix)]
    #[test]
    fn a_shell_that_answers_gives_its_text_and_a_stuck_one_times_out() {
        use std::time::Instant;

        let mut quick = Command::new("sh");
        quick.args(["-c", &format!("echo banner; printf '%s' {BEGIN}/a:/b{END}")]);
        let text = run(quick, Duration::from_secs(10)).unwrap();
        assert_eq!(parse(&text), Some("/a:/b"));

        let mut stuck = Command::new("sh");
        stuck.args(["-c", "sleep 30"]);
        let began = Instant::now();
        assert_eq!(run(stuck, Duration::from_millis(300)), None);
        assert!(
            began.elapsed() < Duration::from_secs(10),
            "{:?}",
            began.elapsed()
        );
    }
}
