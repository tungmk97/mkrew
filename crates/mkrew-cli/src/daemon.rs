//! `mk daemon start | stop | status` (SPEC §8, §9).

use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use mkrew_core::Paths;
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

pub fn start(paths: &Paths) -> anyhow::Result<ExitCode> {
    if let Some(mut client) = try_connect(paths)? {
        let status: DaemonStatusResult = client.call(Request::DaemonStatus)?;
        println!("daemon already running (pid {})", status.pid);
        return Ok(ExitCode::SUCCESS);
    }

    let exe = daemon_exe()?;
    let mut command = Command::new(&exe);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach(&mut command);
    let mut child = command
        .spawn()
        .with_context(|| format!("start {}", exe.display()))?;

    let deadline = Instant::now() + TIMEOUT;
    let mut lost_race = false;
    loop {
        if let Some(mut client) = try_connect(paths)? {
            let status: DaemonStatusResult = client.call(Request::DaemonStatus)?;
            if lost_race {
                println!("daemon already running (pid {})", status.pid);
            } else {
                println!("daemon started (pid {})", status.pid);
            }
            return Ok(ExitCode::SUCCESS);
        }
        if !lost_race && let Some(exit) = child.try_wait().context("check the daemon process")? {
            // Another `mk daemon start` won the lock: keep waiting for that daemon instead.
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

pub fn stop(paths: &Paths) -> anyhow::Result<ExitCode> {
    let Some(mut client) = try_connect(paths)? else {
        println!("daemon not running");
        return Ok(ExitCode::SUCCESS);
    };
    let status: DaemonStatusResult = client.call(Request::DaemonStatus)?;
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
    println!("  socket:    {}", paths.socket);
    println!("  data dir:  {}", paths.data_dir.display());
    Ok(ExitCode::SUCCESS)
}

/// `mkrew-daemon` installed next to `mk` (the release layout and `target/<profile>/`),
/// else whatever `PATH` finds.
fn daemon_exe() -> anyhow::Result<PathBuf> {
    let name = format!("mkrew-daemon{}", std::env::consts::EXE_SUFFIX);
    let exe = std::env::current_exe().context("locate the mk executable")?;
    let sibling = exe.with_file_name(&name);
    Ok(if sibling.is_file() {
        sibling
    } else {
        PathBuf::from(name)
    })
}

/// No console window and no controlling terminal: the daemon outlives the shell (SPEC §8).
fn detach(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        disinherit_stdio();
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // New process group: Ctrl+C in the shell that ran `mk` does not reach the daemon.
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Own process group: terminal job-control signals (Ctrl+C, hangup) skip the daemon.
        command.process_group(0);
    }
}

/// `CreateProcess` passes EVERY inheritable handle to the child, not just its stdio. mk's own
/// stdio are inheritable when mk runs with redirected output (`mk daemon start | cat`, a test
/// capturing output), and a daemon holding the write end of that pipe keeps the reader waiting
/// until the daemon exits. mk spawns nothing else, so clear the flag on all three.
#[cfg(windows)]
#[allow(unsafe_code)]
fn disinherit_stdio() {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};

    let handles = [
        std::io::stdin().as_raw_handle(),
        std::io::stdout().as_raw_handle(),
        std::io::stderr().as_raw_handle(),
    ];
    for handle in handles {
        // SAFETY: the handles are mk's live std handles (or null when absent, which the call
        // rejects harmlessly); only the inherit flag changes. Failure leaves the old behavior.
        unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) };
    }
}
