//! The UI client over a program's pipes, with the built `mk proxy` as the program (what
//! `ssh … mk proxy` is on a real remote machine).
//!
//! Needs `target/<profile>/mk` and `mkrew-daemon` built (`cargo build --workspace`).

// Helpers outside `#[test]` fns are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures_lite::future::block_on;
use mkrew_core::Paths;
use mkrew_protocol::{Event, PROTOCOL_VERSION, Project, ProjectAddParams, Request};
use mkrew_ui::client::{Connection, Endpoint, Incoming, OpenError};
use ulid::Ulid;

fn sibling_exe(name: &str) -> PathBuf {
    let exe = Path::new(env!("CARGO_BIN_EXE_mkrew"))
        .with_file_name(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    assert!(
        exe.is_file(),
        "{} is missing; run `cargo build --workspace`",
        exe.display()
    );
    exe
}

fn paths(dir: &Path) -> Paths {
    let socket = if cfg!(windows) {
        format!(r"\\.\pipe\mkrew-test-{}", Ulid::new())
    } else {
        dir.join("mkrew.sock").to_string_lossy().into_owned()
    };
    Paths {
        data_dir: dir.join("data"),
        config_dir: dir.join("config"),
        socket,
    }
}

/// Stops the daemon `mk proxy` started, even when an assertion fails.
struct StopOnDrop(Paths);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        if let Ok(Ok(connection)) = Connection::open(&Endpoint::Socket(self.0.socket.clone())) {
            let _ = block_on(connection.request(Request::DaemonStop));
        }
    }
}

/// `mk proxy` against a test daemon, as `ssh` would run it on the other computer.
fn proxy_endpoint(paths: &Paths) -> Endpoint {
    Endpoint::Program {
        program: sibling_exe("mk").to_string_lossy().into_owned(),
        args: vec!["proxy".into()],
        env: vec![
            (
                "MKREW_DATA_DIR".into(),
                paths.data_dir.to_string_lossy().into_owned(),
            ),
            (
                "MKREW_CONFIG_DIR".into(),
                paths.config_dir.to_string_lossy().into_owned(),
            ),
            ("MKREW_SOCKET".into(), paths.socket.clone()),
            ("MKREW_NOTIFY".into(), "off".into()),
        ],
    }
}

/// The handshake error of an endpoint that must not open (`Connection` has no `Debug`).
fn open_error(endpoint: &Endpoint) -> anyhow::Error {
    match Connection::open(endpoint).unwrap() {
        Ok(_) => panic!("the endpoint opened"),
        Err(error) => error,
    }
}

/// A timer without a runtime: a thread that sleeps, then wakes the future.
async fn sleep(duration: Duration) {
    let (tx, rx) = async_channel::bounded::<()>(1);
    std::thread::spawn(move || {
        std::thread::sleep(duration);
        let _ = tx.send_blocking(());
    });
    let _ = rx.recv().await;
}

#[test]
fn a_program_endpoint_greets_answers_and_streams_events() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path());
    let _stop = StopOnDrop(paths.clone());
    let connection = Connection::open(&proxy_endpoint(&paths)).unwrap().unwrap();
    assert_eq!(connection.hello().protocol_version, PROTOCOL_VERSION);
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let inbox = connection.incoming();
    let project: Project = block_on(connection.call(Request::ProjectAdd(ProjectAddParams {
        path: repo.to_string_lossy().into_owned(),
        name: None,
    })))
    .unwrap();
    // `project.updated` arrives through the pipes.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(Incoming::Event(Event::ProjectUpdated(updated))) = inbox.try_recv()
            && updated.id == project.id
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "no event through the program endpoint"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn stopping_the_daemon_closes_the_inbox() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path());
    let _stop = StopOnDrop(paths.clone());
    let connection = Connection::open(&proxy_endpoint(&paths)).unwrap().unwrap();
    let inbox = connection.incoming();
    // The daemon stops: the proxy's socket closes, the proxy exits, the pipes close.
    let _ = block_on(connection.request(Request::DaemonStop));
    let closed = block_on(futures_lite::future::or(
        async {
            while inbox.recv().await.is_ok() {}
            true
        },
        async {
            sleep(Duration::from_secs(10)).await;
            false
        },
    ));
    assert!(closed, "the inbox did not close after daemon.stop");
}

#[test]
fn a_program_that_exits_before_hello_is_unreachable_with_its_stderr() {
    // `mk` with an unknown subcommand prints an error on stderr and exits 2: like `ssh` that
    // cannot log in, or a remote shell without `mk`.
    let endpoint = Endpoint::Program {
        program: sibling_exe("mk").to_string_lossy().into_owned(),
        args: vec!["no-such-command".into()],
        env: Vec::new(),
    };
    let error = open_error(&endpoint);
    match error.downcast_ref::<OpenError>() {
        Some(OpenError::Ended { stderr }) => {
            assert!(stderr.contains("no-such-command"), "{stderr}");
        }
        other => panic!("{other:?} / {error:#}"),
    }
}

#[test]
fn a_missing_program_is_a_spawn_error() {
    let endpoint = Endpoint::Program {
        program: "mkrew-no-such-program".into(),
        args: Vec::new(),
        env: Vec::new(),
    };
    let error = open_error(&endpoint);
    assert!(
        matches!(
            error.downcast_ref::<OpenError>(),
            Some(OpenError::Spawn { .. })
        ),
        "{error:#}"
    );
}
