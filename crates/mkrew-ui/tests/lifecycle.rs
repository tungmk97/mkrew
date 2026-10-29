//! Pane lifecycle against the real daemon (SPEC §10): new agent panes use the workspace's
//! default agent, a hidden pane's session keeps running after the UI closes and stays in the
//! sidebar, killing it ends it, and closing a tab removes its sessions from the sidebar. Runs
//! the real `mkrew-daemon` and fake agent in a temp dir.
//!
//! Needs `target/<profile>/mkrew-daemon` and `mkrew-fakeagent` built (`cargo test --workspace`).

// Helpers outside `#[test]` fns are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures_lite::future::block_on;
use mkrew_core::Paths;
use mkrew_protocol::{
    Project, ProjectAddParams, ProjectUpdateParams, Request, Session, SessionKillParams,
    SessionKind, SessionListParams, SessionSpawnResult, SessionState, TabCreateParams,
    TabListParams, TabRef, TerminalTab,
};
use mkrew_ui::client::Connection;
use mkrew_ui::connect::{DaemonLauncher, POLL, TIMEOUT, connect_or_spawn};
use mkrew_ui::lifecycle::{self, CloseAction};
use mkrew_ui::sidebar::{Row, Sidebar};
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
    let mut launcher =
        DaemonLauncher::new(paths.clone(), sibling_exe("mkrew-daemon")).without_notifications();
    connect_or_spawn(&mut launcher, TIMEOUT, POLL).unwrap()
}

/// Stops the spawned daemon even when an assertion fails.
struct StopOnDrop(Paths);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        let endpoint = mkrew_ui::client::Endpoint::Socket(self.0.socket.clone());
        if let Ok(Ok(connection)) = Connection::open(&endpoint) {
            let _ = block_on(connection.request(Request::DaemonStop));
        }
    }
}

/// What the UI's "+ Agent" button sends.
fn new_agent_pane(connection: &Connection, project_id: Ulid, tab_id: Ulid) -> Ulid {
    let params = lifecycle::spawn_params(project_id, Some(tab_id), SessionKind::Agent, None);
    let spawned: SessionSpawnResult =
        block_on(connection.call(Request::SessionSpawn(params))).unwrap();
    spawned.session_id
}

fn list_tabs(connection: &Connection, project_id: Ulid) -> Tabs {
    let listed: Vec<TerminalTab> =
        block_on(connection.call(Request::TabList(TabListParams { project_id }))).unwrap();
    let mut tabs = Tabs::new(project_id);
    tabs.load(listed);
    tabs
}

fn sessions(connection: &Connection) -> Vec<Session> {
    block_on(connection.call(Request::SessionList(SessionListParams::default()))).unwrap()
}

fn session(connection: &Connection, id: Ulid) -> Session {
    sessions(connection)
        .into_iter()
        .find(|s| s.id == id)
        .unwrap()
}

#[test]
fn hidden_agent_survives_the_ui_and_can_be_killed_later() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path());
    write_fake_profile(&paths);
    let _stop = StopOnDrop(paths.clone());
    let project_dir = dir.path().join("proj");
    std::fs::create_dir(&project_dir).unwrap();

    let first = connect(&paths);
    let project: Project = block_on(first.call(Request::ProjectAdd(ProjectAddParams {
        path: project_dir.to_string_lossy().into_owned(),
        name: None,
    })))
    .unwrap();
    let _: Project = block_on(first.call(Request::ProjectUpdate(ProjectUpdateParams {
        project_id: project.id,
        jira_keys: None,
        position: None,
        name: None,
        default_agent: Some(Some("fake".into())),
        default_shell: None,
        run_commands: None,
        run_autostart: None,
        color: None,
        pinned: None,
    })))
    .unwrap();
    let tab: TerminalTab = block_on(first.call(Request::TabCreate(TabCreateParams {
        project_id: project.id,
        title: "Terminal".into(),
        position: None,
    })))
    .unwrap();

    // The first new pane fills the empty tab (placed by the daemon), the second splits it.
    let a = new_agent_pane(&first, project.id, tab.id);
    let b = new_agent_pane(&first, project.id, tab.id);
    let mut tabs = list_tabs(&first, project.id);
    assert_eq!(tabs.visible(), vec![a]);
    let write = tabs.split_focused(Axis::Vertical, b).unwrap();
    block_on(first.call::<()>(Request::TabSetLayout(write))).unwrap();
    let agent = session(&first, b);
    assert_eq!(agent.agent_profile, "fake", "the workspace's default agent");
    assert_eq!(agent.kind, SessionKind::Agent);

    // Closing the running agent's pane asks; "Hide pane" only changes the layout.
    assert_eq!(lifecycle::close_action(Some(&agent)), CloseAction::Ask);
    let write = tabs.close_pane(b).unwrap();
    block_on(first.call::<()>(Request::TabSetLayout(write))).unwrap();
    drop(first);

    // Reopened UI: the hidden session is still running and listed in the sidebar.
    let second = connect(&paths);
    assert_eq!(list_tabs(&second, project.id).visible(), vec![a]);
    let projects: Vec<Project> = block_on(second.call(Request::ProjectList)).unwrap();
    let mut sidebar = Sidebar::default();
    sidebar.load(projects, sessions(&second));
    assert_ne!(session(&second, b).state, SessionState::Exited);
    let listed = sidebar
        .rows()
        .into_iter()
        .any(|row| matches!(row, Row::Session(row) if row.session_id == b));
    assert!(listed, "the hidden session is not in the sidebar");

    // "Kill session" ends it; its pane would then just go.
    block_on(second.call::<()>(Request::SessionKill(SessionKillParams {
        session_id: b,
        force: false,
    })))
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let exited = loop {
        let state = session(&second, b);
        if state.state == SessionState::Exited {
            break state;
        }
        assert!(Instant::now() < deadline, "the killed session did not exit");
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(lifecycle::close_action(Some(&exited)), CloseAction::Remove);
    assert_ne!(session(&second, a).state, SessionState::Exited);

    // Close tab (Ctrl+Shift+W): the tab goes and its sessions are removed with it, so they
    // leave the sidebar (ADR 0041).
    block_on(second.call::<()>(Request::TabClose(TabRef { tab_id: tab.id }))).unwrap();
    assert!(list_tabs(&second, project.id).is_empty());
    let mut sidebar = Sidebar::default();
    let projects: Vec<Project> = block_on(second.call(Request::ProjectList)).unwrap();
    sidebar.load(projects, sessions(&second));
    let listed = sidebar
        .rows()
        .into_iter()
        .any(|row| matches!(row, Row::Session(row) if row.session_id == a));
    assert!(!listed, "the closed tab's session is still in the sidebar");
}
