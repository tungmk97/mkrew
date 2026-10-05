//! The UI's connect-or-spawn path against the real `mkrew-daemon` binary in a temp dir.
//!
//! Needs `target/<profile>/mkrew-daemon` built (`cargo test --workspace` does).

// Helpers outside `#[test]` fns are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_lite::future::block_on;
use mkrew_core::Paths;
use mkrew_protocol::{
    DaemonStatusResult, Event, Project, ProjectAddParams, ProjectUpdateParams, Request,
};
use mkrew_ui::client::{Connection, Incoming};
use mkrew_ui::connect::{DaemonLauncher, POLL, TIMEOUT, connect_or_spawn};
use ulid::Ulid;

fn daemon_exe() -> PathBuf {
    let exe = Path::new(env!("CARGO_BIN_EXE_mkrew"))
        .with_file_name(format!("mkrew-daemon{}", std::env::consts::EXE_SUFFIX));
    assert!(
        exe.is_file(),
        "{} is missing; run `cargo build -p mkrew-daemon`",
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

fn connect(paths: &Paths) -> Connection {
    let mut launcher = DaemonLauncher::new(paths.clone(), daemon_exe()).without_notifications();
    connect_or_spawn(&mut launcher, TIMEOUT, POLL).unwrap()
}

/// Stops the spawned daemon even when an assertion fails.
struct StopOnDrop(Paths);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        if let Ok(Ok(connection)) = Connection::open(&self.0.socket) {
            let _ = block_on(connection.request(Request::DaemonStop));
        }
    }
}

#[test]
fn spawns_the_daemon_once_then_reuses_it() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path());
    let _stop = StopOnDrop(paths.clone());
    assert!(
        Connection::open(&paths.socket).is_err(),
        "nothing listens yet"
    );

    let first = connect(&paths);
    assert_eq!(
        first.hello().protocol_version,
        mkrew_protocol::PROTOCOL_VERSION
    );
    let status: DaemonStatusResult = block_on(first.call(Request::DaemonStatus)).unwrap();
    // The daemon got the paths through the environment.
    assert!(paths.lock_file().is_file());

    let second = connect(&paths);
    let again: DaemonStatusResult = block_on(second.call(Request::DaemonStatus)).unwrap();
    assert_eq!(again.pid, status.pid, "the running daemon is reused");

    // Requests can be in flight together; events reach every connection.
    let project = dir.path().join("proj");
    std::fs::create_dir(&project).unwrap();
    let added: Project = block_on(first.call(Request::ProjectAdd(ProjectAddParams {
        path: project.to_string_lossy().into_owned(),
        name: None,
    })))
    .unwrap();
    let renamed = first.call::<Project>(Request::ProjectUpdate(ProjectUpdateParams {
        project_id: added.id,
        name: Some("renamed".into()),
        default_agent: None,
    }));
    let listed = first.call::<Vec<Project>>(Request::ProjectList);
    assert_eq!(block_on(renamed).unwrap().name, "renamed");
    assert_eq!(block_on(listed).unwrap().len(), 1);
    let incoming = second.incoming();
    // The add is announced, then the rename.
    for name in ["proj", "renamed"] {
        let event = block_on(futures_lite::future::or(
            async { incoming.recv().await.ok() },
            async {
                sleep(Duration::from_secs(5)).await;
                None
            },
        ));
        match event {
            Some(Incoming::Event(Event::ProjectUpdated(project))) => {
                assert_eq!(project.name, name);
            }
            other => panic!("expected project.updated, got {other:?}"),
        }
    }

    // After `daemon.stop` the connection reports it is gone.
    block_on(first.request(Request::DaemonStop)).unwrap();
    let incoming = first.incoming();
    let closed = block_on(futures_lite::future::or(
        async {
            while incoming.recv().await.is_ok() {}
            true
        },
        async {
            sleep(Duration::from_secs(10)).await;
            false
        },
    ));
    assert!(closed, "the connection did not close after daemon.stop");
    assert!(block_on(first.request(Request::DaemonStatus)).is_err());
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
