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
use mkrew_protocol::Project;
use ulid::Ulid;

use crate::client::{Connection, Endpoint, OpenError};
use crate::machines::Machine;

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
    notifications: bool,
}

impl DaemonLauncher {
    pub fn new(paths: Paths, exe: PathBuf) -> Self {
        Self {
            paths,
            exe,
            child: None,
            notifications: true,
        }
    }

    /// A spawned daemon shows no OS notifications (tests).
    pub fn without_notifications(mut self) -> Self {
        self.notifications = false;
        self
    }
}

impl Launcher for DaemonLauncher {
    type Conn = Connection;

    fn connect(&mut self) -> anyhow::Result<Option<Connection>> {
        match Connection::open(&Endpoint::Socket(self.paths.socket.clone())) {
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
        if !self.notifications {
            command.env(mkrew_daemon::ENV_NOTIFY, "off");
        }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MachineError {
    /// Show the reason (ssh's last stderr lines or the spawn error) and the hint; retry later.
    Unreachable {
        reason: String,
        hint: Option<String>,
    },
    /// Show the message; do not retry by itself.
    Version(String),
}

/// Connects to `machine`'s daemon through `ssh … mk proxy`; never spawns anything else.
pub fn connect_machine(machine: &Machine) -> Result<Connection, MachineError> {
    let (program, args) = crate::machines::ssh_command(machine);
    let endpoint = Endpoint::Program {
        program,
        args,
        env: Vec::new(),
    };
    let error = match Connection::open(&endpoint) {
        Ok(Ok(connection)) => return Ok(connection),
        Ok(Err(error)) => error,
        Err(error) => anyhow::Error::new(error),
    };
    Err(match error.downcast_ref::<OpenError>() {
        Some(OpenError::Version { message }) => MachineError::Version(message.clone()),
        Some(OpenError::Ended { stderr }) => MachineError::Unreachable {
            reason: last_lines(stderr, 3)
                .unwrap_or_else(|| "The connection ended before it was ready.".into()),
            hint: crate::machines::hint(stderr, machine),
        },
        Some(OpenError::Spawn { source, .. }) => MachineError::Unreachable {
            reason: format!("Cannot run ssh: {source}."),
            hint: Some(
                "Install the OpenSSH client (Windows: Settings → System → Optional features)."
                    .into(),
            ),
        },
        None => MachineError::Unreachable {
            reason: format!("{error:#}"),
            hint: None,
        },
    })
}

/// The last `count` non-empty lines joined with a space; `None` when there are none.
fn last_lines(text: &str, count: usize) -> Option<String> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let from = lines.len().saturating_sub(count);
    (!lines.is_empty()).then(|| lines[from..].join(" "))
}

/// How long to wait before automatic retry number `attempt` (0-based) after a failed or lost
/// connection: 1 s, 2 s, 5 s, 10 s, then every 30 s.
pub fn backoff(attempt: u32) -> Duration {
    const STEPS: [u64; 4] = [1, 2, 5, 10];
    Duration::from_secs(STEPS.get(attempt as usize).copied().unwrap_or(30))
}

/// The workspace to show after a lost connection came back: the one that was shown, if the
/// daemon still lists it. `None` = no choice made here (use the usual first choice).
pub fn reconnect_workspace(shown: Option<Ulid>, listed: &[Project]) -> Option<Ulid> {
    shown.filter(|id| listed.iter().any(|project| project.id == *id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_lines_keeps_the_last_non_empty_lines() {
        assert_eq!(last_lines("a\n\n b \nc\nd\n", 3).as_deref(), Some("b c d"));
        assert_eq!(last_lines(" \n", 3), None);
    }

    #[test]
    fn backoff_grows_then_holds_at_30_seconds() {
        let secs: Vec<u64> = (0..7).map(|n| backoff(n).as_secs()).collect();
        assert_eq!(secs, [1, 2, 5, 10, 30, 30, 30]);
        assert_eq!(backoff(u32::MAX).as_secs(), 30);
    }

    fn project(name: &str) -> Project {
        Project {
            id: Ulid::new(),
            name: name.to_owned(),
            path: format!("C:\\src\\{name}"),
            run_commands: Vec::new(),
            run_autostart: false,
            default_agent: None,
            default_shell: None,
            review_globs: None,
            created_at: 0,
            archived: false,
            base_ref: None,
            color: None,
            pinned: false,
            jira_keys: Vec::new(),
            position: 0,
        }
    }

    #[test]
    fn reconnect_keeps_the_shown_workspace() {
        let (a, b) = (project("a"), project("b"));
        assert_eq!(
            reconnect_workspace(Some(b.id), &[a.clone(), b.clone()]),
            Some(b.id)
        );
        // Removed meanwhile (on the other computer): fall back to the usual first choice.
        assert_eq!(
            reconnect_workspace(Some(Ulid::new()), std::slice::from_ref(&a)),
            None
        );
        assert_eq!(reconnect_workspace(None, &[a]), None);
    }

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
