//! `mk daemon start | stop | status` (SPEC §8, §9).

use std::io::ErrorKind;
use std::path::Path;
use std::process::{Child, Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use mkrew_core::Paths;
use mkrew_daemon::spawn::{daemon_exe, detach, detach_from_job};
use mkrew_daemon::{EXIT_ALREADY_RUNNING, InstanceLock, LockError};
use mkrew_protocol::{DaemonStatusResult, Request};

use crate::client::Client;

/// How long `start` waits for `hello` and `stop` waits for the endpoint to go away.
const TIMEOUT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(50);

/// `None` = no daemon is listening.
pub fn try_connect(paths: &Paths) -> anyhow::Result<Option<Client>> {
    match Client::connect(&paths.socket) {
        Ok(handshake) => Ok(Some(handshake?.0)),
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused) => {
            Ok(None)
        }
        Err(e) => Err(e).with_context(|| format!("connect to {}", paths.socket)),
    }
}

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

/// Stops the daemon. With `only_from`, only a daemon whose exe lives in that folder (the
/// installer passes `{app}\bin`): another install's daemon is left running (exit 0).
pub fn stop(paths: &Paths, only_from: Option<&Path>) -> anyhow::Result<ExitCode> {
    let Some(mut client) = try_connect(paths)? else {
        println!("daemon not running");
        return Ok(ExitCode::SUCCESS);
    };
    let status: DaemonStatusResult = client.call(Request::DaemonStatus)?;
    if let Some(dir) = only_from {
        let exe = process_exe(status.pid);
        if !exe.as_deref().is_ok_and(|exe| runs_from(exe, dir)) {
            let from = match &exe {
                Ok(exe) => exe.display().to_string(),
                Err(e) => format!("an unknown folder ({e})"),
            };
            println!(
                "daemon (pid {}) runs from {from}, not {}; left running",
                status.pid,
                dir.display()
            );
            return Ok(ExitCode::SUCCESS);
        }
    }
    client.call::<()>(Request::DaemonStop)?;
    drop(client);
    // Return only once the daemon released its instance lock (after closing the endpoint),
    // so `stop && start` never races the exiting daemon.
    let deadline = Instant::now() + TIMEOUT;
    while let Err(e) = InstanceLock::acquire(&paths.lock_file()) {
        if !matches!(e, LockError::AlreadyRunning(_)) {
            return Err(e.into());
        }
        if Instant::now() >= deadline {
            bail!("the daemon (pid {}) did not stop", status.pid);
        }
        std::thread::sleep(POLL);
    }
    println!("daemon stopped (pid {})", status.pid);
    Ok(ExitCode::SUCCESS)
}

/// Exit code 1 when no daemon is running, so scripts can test for it.
pub fn status(paths: &Paths) -> anyhow::Result<ExitCode> {
    let Some(mut client) = try_connect(paths)? else {
        println!("daemon not running");
        return Ok(ExitCode::FAILURE);
    };
    let status: DaemonStatusResult = client.call(Request::DaemonStatus)?;
    println!("daemon running");
    println!("  pid:       {}", status.pid);
    println!("  version:   {}", status.daemon_version);
    println!("  protocol:  {}", status.protocol_version);
    println!("  sessions:  {}", status.sessions);
    if let Some(error) = &status.settings_error {
        println!("  settings:  {error}");
    }
    println!("  socket:    {}", paths.socket);
    println!("  data dir:  {}", paths.data_dir.display());
    Ok(ExitCode::SUCCESS)
}

/// Whether `exe` lives directly in `dir`. Both are canonicalised, so case, `\\?\`, 8.3 short
/// names and a trailing separator do not matter; a path that cannot be resolved never matches.
pub fn runs_from(exe: &Path, dir: &Path) -> bool {
    match (dunce::canonicalize(exe), dunce::canonicalize(dir)) {
        (Ok(exe), Ok(dir)) => exe.parent() == Some(dir.as_path()),
        _ => false,
    }
}

/// The image path of process `pid`.
#[cfg(windows)]
fn process_exe(pid: u32) -> std::io::Result<std::path::PathBuf> {
    win::process_exe(pid)
}

/// The image path of process `pid` (Linux; elsewhere the call fails and the daemon is left).
#[cfg(not(windows))]
fn process_exe(pid: u32) -> std::io::Result<std::path::PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe"))
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod win {
    use std::ffi::OsString;
    use std::io;
    use std::os::windows::ffi::OsStringExt;
    use std::path::PathBuf;

    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };

    pub fn process_exe(pid: u32) -> io::Result<PathBuf> {
        // SAFETY: plain Win32 call; a null handle is checked below.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let mut buf = vec![0u16; 32_768];
        let mut len = u32::try_from(buf.len()).unwrap_or(u32::MAX);
        // SAFETY: `handle` is open; `buf` holds `len` u16s and outlives the call.
        let ok = unsafe {
            QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
        };
        let error = io::Error::last_os_error();
        // SAFETY: `handle` came from `OpenProcess` and is closed once.
        unsafe { CloseHandle(handle) };
        if ok == 0 {
            return Err(error);
        }
        Ok(OsString::from_wide(&buf[..len as usize]).into())
    }
}

#[cfg(test)]
mod tests {
    use super::runs_from;

    fn exe_in(dir: &std::path::Path) -> std::path::PathBuf {
        let exe = dir.join("mkrew-daemon.exe");
        std::fs::write(&exe, b"").unwrap();
        exe
    }

    #[test]
    fn runs_from_accepts_an_exe_directly_in_the_dir() {
        let bin = tempfile::tempdir().unwrap();
        assert!(runs_from(&exe_in(bin.path()), bin.path()));
    }

    #[test]
    fn runs_from_refuses_an_exe_outside_the_dir() {
        let (bin, other) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        assert!(
            !runs_from(&exe_in(other.path()), bin.path()),
            "another folder"
        );
        let sub = bin.path().join("old");
        std::fs::create_dir(&sub).unwrap();
        assert!(
            !runs_from(&exe_in(&sub), bin.path()),
            "a subfolder is not the dir"
        );
    }

    #[test]
    fn runs_from_refuses_missing_paths() {
        let bin = tempfile::tempdir().unwrap();
        let exe = exe_in(bin.path());
        assert!(!runs_from(&exe, &bin.path().join("gone")));
        assert!(!runs_from(&bin.path().join("gone.exe"), bin.path()));
    }

    #[test]
    fn runs_from_ignores_case_and_trailing_separator() {
        let bin = tempfile::tempdir().unwrap();
        let exe = exe_in(bin.path());
        let mut dir = bin.path().as_os_str().to_owned();
        dir.push(std::path::MAIN_SEPARATOR_STR);
        assert!(runs_from(&exe, std::path::Path::new(&dir)));
        if cfg!(windows) {
            let upper = bin.path().to_string_lossy().to_uppercase();
            assert!(runs_from(&exe, std::path::Path::new(&upper)));
        }
    }
}
