//! Tabs and splits survive closing the UI: the layout saved through `tab.set_layout` is restored
//! from `tab.list` by a new connection, which re-attaches the visible sessions (SPEC §5, §10).
//! Runs the real `mkrew-daemon` and fake agent in a temp dir.
//!
//! Needs `target/<profile>/mkrew-daemon` and `mkrew-fakeagent` built (`cargo test --workspace`).

// Helpers outside `#[test]` fns are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_lite::future::block_on;
use mkrew_core::Paths;
use mkrew_protocol::{
    Project, ProjectAddParams, Request, SessionAttachResult, SessionInputParams, SessionKind,
    SessionRef, SessionSpawnParams, SessionSpawnResult, TabCreateParams, TabListParams,
    TerminalTab,
};
use mkrew_ui::client::Connection;
use mkrew_ui::connect::{DaemonLauncher, POLL, TIMEOUT, connect_or_spawn};
use mkrew_ui::router::{Delivery, Router};
use mkrew_ui::split::Axis;
use mkrew_ui::tabs::Tabs;
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

/// `fake.toml` running the built fake agent (default `ask` scenario).
fn write_fake_profile(paths: &Paths) {
    let fake = sibling_exe("mkrew-fakeagent");
    let sample = include_str!("../../../agents/fake.toml");
    let profile = sample.replace(
        "command = \"mkrew-fakeagent\"",
        &format!("command = '{}'", fake.display()),
    );
    assert_ne!(profile, sample);
    let dir = paths.config_dir.join("agents");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("fake.toml"), profile).unwrap();
}

fn connect(paths: &Paths) -> Connection {
    let mut launcher = DaemonLauncher::new(paths.clone(), sibling_exe("mkrew-daemon"));
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

fn spawn(connection: &Connection, project_id: Ulid, tab_id: Option<Ulid>) -> Ulid {
    let spawned: SessionSpawnResult =
        block_on(connection.call(Request::SessionSpawn(SessionSpawnParams {
            project_id,
            agent_profile: Some("fake".into()),
            kind: SessionKind::Agent,
            tab_id,
            title: None,
            cols: 80,
            rows: 24,
        })))
        .unwrap();
    spawned.session_id
}

fn list_tabs(connection: &Connection, project_id: Ulid) -> Tabs {
    let listed: Vec<TerminalTab> =
        block_on(connection.call(Request::TabList(TabListParams { project_id }))).unwrap();
    let mut tabs = Tabs::new(project_id);
    tabs.load(listed);
    tabs
}

#[test]
fn layout_is_restored_and_visible_panes_reattach_after_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path());
    write_fake_profile(&paths);
    let _stop = StopOnDrop(paths.clone());
    let project_dir = dir.path().join("proj");
    std::fs::create_dir(&project_dir).unwrap();

    // First UI run: one tab, the first session placed by the daemon, the second split in.
    let first = connect(&paths);
    let project: Project = block_on(first.call(Request::ProjectAdd(ProjectAddParams {
        path: project_dir.to_string_lossy().into_owned(),
        name: None,
    })))
    .unwrap();
    let tab: TerminalTab = block_on(first.call(Request::TabCreate(TabCreateParams {
        project_id: project.id,
        title: "Terminal".into(),
        position: None,
    })))
    .unwrap();
    let a = spawn(&first, project.id, Some(tab.id));
    let b = spawn(&first, project.id, None);
    let mut tabs = list_tabs(&first, project.id);
    assert_eq!(tabs.visible(), vec![a]);
    let write = tabs.split_focused(Axis::Vertical, b).unwrap();
    block_on(first.call::<()>(Request::TabSetLayout(write))).unwrap();
    drop(first);

    // Reopened UI: the same tree comes back and both panes attach.
    let second = connect(&paths);
    let tabs = list_tabs(&second, project.id);
    assert_eq!(tabs.visible(), vec![a, b]);
    let mut router = Router::default();
    for session in tabs.visible() {
        let generation = router.begin_attach(session).unwrap();
        let _: SessionAttachResult = block_on(second.call(Request::SessionAttach(SessionRef {
            session_id: session,
        })))
        .unwrap();
        assert!(router.finish_attach(session, generation).is_some());
        // Answers the fake agent's prompt, so output follows the attach.
        block_on(second.call::<()>(Request::SessionInput(SessionInputParams {
            session_id: session,
            data: b"y\r".to_vec(),
        })))
        .unwrap();
    }

    // The fake agent's output reaches the panes through the router.
    let incoming = second.incoming();
    let mut fed = Vec::new();
    let delivered = block_on(futures_lite::future::or(
        async {
            while fed.len() < 2 {
                let Ok(frame) = incoming.recv().await else {
                    return false;
                };
                if let Some(Delivery::Output { session_id, .. }) = router.route(frame)
                    && !fed.contains(&session_id)
                {
                    fed.push(session_id);
                }
            }
            true
        },
        async {
            sleep(Duration::from_secs(15)).await;
            false
        },
    ));
    assert!(delivered, "no output from both re-attached sessions");

    // Hiding a pane detaches it.
    assert!(router.forget(a));
    block_on(second.call::<()>(Request::SessionDetach(SessionRef { session_id: a }))).unwrap();
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
