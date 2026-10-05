//! Connect to the daemon, starting it first if nothing is listening (SPEC §3, §8).
//!
//! The decision logic ([`connect_or_spawn`]) is written against the [`Launcher`] trait so it is
//! tested without processes; [`DaemonLauncher`] is the real implementation.

use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use mkrew_core::Paths;
use mkrew_core::paths::{ENV_CONFIG_DIR, ENV_DATA_DIR, ENV_SOCKET};
use mkrew_daemon::EXIT_ALREADY_RUNNING;

use crate::client::Connection;

/// How long to wait for a freshly started daemon to answer `hello`.
pub const TIMEOUT: Duration = Duration::from_secs(10);
pub const POLL: Duration = Duration::from_millis(50);

/// What [`connect_or_spawn`] needs from the outside world.
pub trait Launcher {
    type Conn;
    /// `Ok(None)` = no daemon is listening.
    fn connect(&mut self) -> anyhow::Result<Option<Self::Conn>>;
    /// Starts a detached daemon.
    fn spawn(&mut self) -> anyhow::Result<()>;
    /// `Some(code)` once the spawned daemon exited (`code` is `None` when killed by a signal).
    fn exited(&mut self) -> anyhow::Result<Option<Option<i32>>>;
}

/// Connects to a running daemon, or spawns one and waits until it answers.
///
/// A spawned daemon that exits with [`EXIT_ALREADY_RUNNING`] lost the start race to another
/// client's daemon: keep waiting for that one. Any other early exit is an error.
pub fn connect_or_spawn<L: Launcher>(
    launcher: &mut L,
    timeout: Duration,
    poll: Duration,
) -> anyhow::Result<L::Conn> {
    if let Some(conn) = launcher.connect()? {
        return Ok(conn);
    }
    launcher.spawn()?;
    let deadline = Instant::now() + timeout;
    let mut lost_race = false;
    loop {
        if let Some(conn) = launcher.connect()? {
            return Ok(conn);
        }
        if !lost_race && let Some(code) = launcher.exited()? {
            if code == Some(i32::from(EXIT_ALREADY_RUNNING)) {
                lost_race = true;
            } else {
                let code = code.map_or_else(|| "a signal".to_owned(), |c| format!("code {c}"));
                bail!("the daemon exited during startup ({code})");
            }
        }
        if Instant::now() >= deadline {
            bail!("the daemon did not answer within {}s", timeout.as_secs());
        }
        std::thread::sleep(poll);
    }
}

/// Spawns `exe` detached with `paths` passed through the environment, so the daemon serves
/// exactly the endpoint and data dir this client resolved.
pub struct DaemonLauncher {
    paths: Paths,
    exe: PathBuf,
    child: Option<Child>,
}

impl DaemonLauncher {
    pub fn new(paths: Paths, exe: PathBuf) -> Self {
        Self {
            paths,
            exe,
            child: None,
        }
    }
}

impl Launcher for DaemonLauncher {
    type Conn = Connection;

    fn connect(&mut self) -> anyhow::Result<Option<Connection>> {
        match Connection::open(&self.paths.socket) {
            Ok(handshake) => Ok(Some(handshake?)),
            Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused) => {
                Ok(None)
            }
            Err(e) => Err(e).with_context(|| format!("connect to {}", self.paths.socket)),
        }
    }

    fn spawn(&mut self) -> anyhow::Result<()> {
        let mut command = Command::new(&self.exe);
        command
            .env(ENV_DATA_DIR, &self.paths.data_dir)
            .env(ENV_CONFIG_DIR, &self.paths.config_dir)
            .env(ENV_SOCKET, &self.paths.socket)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        mkrew_daemon::spawn::detach(&mut command);
        let child = command
            .spawn()
            .with_context(|| format!("start {}", self.exe.display()))?;
        self.child = Some(child);
        Ok(())
    }

    fn exited(&mut self) -> anyhow::Result<Option<Option<i32>>> {
        let Some(child) = &mut self.child else {
            return Ok(None);
        };
        let status = child.try_wait().context("check the daemon process")?;
        Ok(status.map(|status| status.code()))
    }
}

/// The app's startup path: the daemon next to `mkrew` (or on `PATH`), default timeouts.
pub fn connect(paths: &Paths) -> anyhow::Result<Connection> {
    let exe = mkrew_daemon::spawn::daemon_exe()?;
    let mut launcher = DaemonLauncher::new(paths.clone(), exe);
    let connected = connect_or_spawn(&mut launcher, TIMEOUT, POLL).with_context(|| {
        format!(
            "connect to the Mkrew daemon; see the logs in {}",
            paths.logs_dir().display()
        )
    });
    // The UI is long-lived: reap a daemon it started once that exits (no zombie on Unix).
    if let Some(mut child) = launcher.child.take() {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
    connected
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scripted outside world: `connect` fails until `up_after` polls after the spawn.
    #[derive(Default)]
    struct Fake {
        running: bool,
        up_after: Option<u32>,
        exit: Option<Option<i32>>,
        connects: u32,
        spawns: u32,
        handshake_error: bool,
    }

    impl Launcher for Fake {
        type Conn = u32;

        fn connect(&mut self) -> anyhow::Result<Option<u32>> {
            self.connects += 1;
            if self.handshake_error {
                bail!("version mismatch");
            }
            if let Some(left) = &mut self.up_after
                && self.spawns > 0
            {
                if *left == 0 {
                    self.running = true;
                } else {
                    *left -= 1;
                }
            }
            Ok(self.running.then_some(self.connects))
        }

        fn spawn(&mut self) -> anyhow::Result<()> {
            self.spawns += 1;
            Ok(())
        }

        fn exited(&mut self) -> anyhow::Result<Option<Option<i32>>> {
            Ok(self.exit)
        }
    }

    fn run(fake: &mut Fake) -> anyhow::Result<u32> {
        connect_or_spawn(fake, Duration::from_millis(200), Duration::ZERO)
    }

    #[test]
    fn running_daemon_is_used_without_spawning() {
        let mut fake = Fake {
            running: true,
            ..Fake::default()
        };
        assert_eq!(run(&mut fake).unwrap(), 1);
        assert_eq!(fake.spawns, 0);
    }

    #[test]
    fn missing_daemon_is_spawned_once_and_awaited() {
        let mut fake = Fake {
            up_after: Some(3),
            ..Fake::default()
        };
        run(&mut fake).unwrap();
        assert_eq!(fake.spawns, 1);
        assert_eq!(fake.connects, 5);
    }

    #[test]
    fn lost_start_race_waits_for_the_other_daemon() {
        let mut fake = Fake {
            up_after: Some(2),
            exit: Some(Some(i32::from(EXIT_ALREADY_RUNNING))),
            ..Fake::default()
        };
        run(&mut fake).unwrap();
        assert_eq!(fake.spawns, 1);
    }

    #[test]
    fn crashed_daemon_is_an_error() {
        let mut fake = Fake {
            exit: Some(Some(1)),
            ..Fake::default()
        };
        let error = run(&mut fake).unwrap_err().to_string();
        assert!(error.contains("exited during startup (code 1)"), "{error}");
    }

    #[test]
    fn silent_daemon_times_out() {
        let mut fake = Fake::default();
        let error = run(&mut fake).unwrap_err().to_string();
        assert!(error.contains("did not answer"), "{error}");
        assert_eq!(fake.spawns, 1);
    }

    #[test]
    fn handshake_error_is_not_retried() {
        let mut fake = Fake {
            handshake_error: true,
            ..Fake::default()
        };
        let error = run(&mut fake).unwrap_err().to_string();
        assert!(error.contains("version mismatch"), "{error}");
        assert_eq!((fake.connects, fake.spawns), (1, 0));
    }
}
