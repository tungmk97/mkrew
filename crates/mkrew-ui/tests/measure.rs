//! SPEC §15 measurements that need the real window: UI startup (launch → first frame, and →
//! first frame connected to a running daemon), idle CPU and memory of the UI and the daemon with
//! six fake sessions on screen (agents excluded). Reports only; `scripts/measure.*` runs it in
//! release next to the flood stress test and the 10,000-file git status test, and the results
//! are recorded in `docs/decisions/0009-non-functional-measurements.md`.
//!
//! Opens a real window, so it is `#[ignore]`d (needs a desktop session):
//! `cargo test --release -p mkrew-ui --test measure -- --ignored --nocapture`.

// Helpers outside `#[test]` fns are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use futures_lite::future::block_on;
use mkrew_core::Paths;
use mkrew_core::paths::{ENV_CONFIG_DIR, ENV_DATA_DIR, ENV_SOCKET};
use mkrew_protocol::{
    DaemonStatusResult, Project, ProjectAddParams, Request, SessionKind, SessionSpawnResult,
    TabCreateParams, TabSetLayoutParams, TerminalTab,
};
use mkrew_ui::client::Connection;
use mkrew_ui::connect::{DaemonLauncher, POLL, TIMEOUT, connect_or_spawn};
use mkrew_ui::lifecycle;
use serde_json::json;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use ulid::Ulid;

const PANES: usize = 6;
/// The window's startup is waited for this long at most.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
/// Time for the panes to attach and the fake agents to settle on their question.
const SETTLE: Duration = Duration::from_secs(5);
/// Idle CPU is averaged over this window.
const IDLE_WINDOW: Duration = Duration::from_secs(10);

fn sibling_exe(name: &str) -> PathBuf {
    let exe = Path::new(env!("CARGO_BIN_EXE_mkrew"))
        .with_file_name(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    assert!(
        exe.is_file(),
        "{} is missing; run `scripts/measure.*` (or `cargo build --release --workspace`)",
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

/// `fake.toml` running the built fake agent (default `ask` scenario: asks, then waits).
fn write_fake_profile(paths: &Paths) {
    let fake = sibling_exe("mkrew-fakeagent");
    let profile = include_str!("../../../agents/fake.toml").replace(
        "command = \"mkrew-fakeagent\"",
        &format!("command = '{}'", fake.display()),
    );
    let agents = paths.config_dir.join("agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(agents.join("fake.toml"), profile).unwrap();
}

/// Stops the spawned daemon and the window even when an assertion fails.
struct StopOnDrop(Paths, Option<Child>);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        if let Some(window) = &mut self.1 {
            let _ = window.kill();
            let _ = window.wait();
        }
        if let Ok(Ok(connection)) = Connection::open(&self.0.socket) {
            let _ = block_on(connection.request(Request::DaemonStop));
        }
    }
}

/// Resident memory (Windows: working set) and CPU % of one core since the last sample.
struct Probe {
    system: System,
    pid: Pid,
}

impl Probe {
    fn new(pid: u32) -> Self {
        let mut probe = Self {
            system: System::new(),
            pid: Pid::from_u32(pid),
        };
        // The first refresh is the CPU baseline.
        probe.sample();
        probe
    }

    fn sample(&mut self) -> (u64, f32) {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[self.pid]),
            ProcessRefreshKind::new().with_cpu().with_memory(),
        );
        self.system
            .process(self.pid)
            .map_or((0, 0.0), |process| (process.memory(), process.cpu_usage()))
    }
}

fn mib(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

/// Six panes in one tab: two rows of three.
fn six_pane_layout(sessions: &[Ulid]) -> serde_json::Value {
    let pane = |id: Ulid| json!({ "session": id.to_string() });
    let row = |ids: &[Ulid]| {
        json!({"split": "vertical", "ratio": 0.333, "children": [
            pane(ids[0]),
            {"split": "vertical", "ratio": 0.5, "children": [pane(ids[1]), pane(ids[2])]},
        ]})
    };
    json!({"split": "horizontal", "ratio": 0.5, "children": [row(&sessions[..3]), row(&sessions[3..])]})
}

/// Waits for a line containing `text` on the window's log.
fn wait_for(lines: &mpsc::Receiver<String>, text: &str, launched: Instant) -> Duration {
    loop {
        let left = STARTUP_TIMEOUT.saturating_sub(launched.elapsed());
        let line = lines
            .recv_timeout(left)
            .unwrap_or_else(|_| panic!("the window never logged {text:?}"));
        if line.contains(text) {
            return launched.elapsed();
        }
    }
}

#[test]
#[ignore = "opens a real window; run by scripts/measure.*"]
fn startup_idle_cpu_and_memory_with_six_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path());
    write_fake_profile(&paths);
    let mut stop = StopOnDrop(paths.clone(), None);
    let project_dir = dir.path().join("proj");
    std::fs::create_dir(&project_dir).unwrap();

    let mut launcher =
        DaemonLauncher::new(paths.clone(), sibling_exe("mkrew-daemon")).without_notifications();
    let connection = connect_or_spawn(&mut launcher, TIMEOUT, POLL).unwrap();
    let status: DaemonStatusResult = block_on(connection.call(Request::DaemonStatus)).unwrap();
    let project: Project = block_on(connection.call(Request::ProjectAdd(ProjectAddParams {
        path: project_dir.to_string_lossy().into_owned(),
        name: None,
        dev_command: None,
    })))
    .unwrap();
    let tab: TerminalTab = block_on(connection.call(Request::TabCreate(TabCreateParams {
        project_id: project.id,
        title: "Agents".into(),
        position: None,
    })))
    .unwrap();
    let mut sessions = Vec::new();
    for _ in 0..PANES {
        let mut params = lifecycle::spawn_params(project.id, Some(tab.id), SessionKind::Agent);
        params.agent_profile = Some("fake".into());
        let spawned: SessionSpawnResult =
            block_on(connection.call(Request::SessionSpawn(params))).unwrap();
        sessions.push(spawned.session_id);
    }
    block_on(
        connection.call::<serde_json::Value>(Request::TabSetLayout(TabSetLayoutParams {
            tab_id: tab.id,
            layout: six_pane_layout(&sessions),
        })),
    )
    .unwrap();

    // UI startup with the daemon running: launch → first frame / first connected frame.
    let launched = Instant::now();
    let mut window = Command::new(env!("CARGO_BIN_EXE_mkrew"))
        .env(ENV_DATA_DIR, &paths.data_dir)
        .env(ENV_CONFIG_DIR, &paths.config_dir)
        .env(ENV_SOCKET, &paths.socket)
        .env("MKREW_LOG", "info")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = window.stderr.take().unwrap();
    let window_pid = window.id();
    stop.1 = Some(window);
    let (tx, lines) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let first_frame = wait_for(&lines, "first frame", launched);
    let connected_frame = wait_for(&lines, "first connected frame", launched);

    // Idle: six attached panes, every fake agent waiting on its question, no output.
    std::thread::sleep(SETTLE);
    let mut ui = Probe::new(window_pid);
    let mut daemon = Probe::new(status.pid);
    std::thread::sleep(IDLE_WINDOW);
    let (ui_rss, ui_cpu) = ui.sample();
    let (daemon_rss, daemon_cpu) = daemon.sample();

    let verdict = |ok: bool| if ok { "ok" } else { "OVER BUDGET" };
    eprintln!(
        "measure: first frame {first_frame:?} ({}), first connected frame {connected_frame:?} \
         ({}); idle cpu over {IDLE_WINDOW:?} ui {ui_cpu:.2}% + daemon {daemon_cpu:.2}% of a \
         core ({}); memory with {PANES} sessions ui {} MiB ({}), daemon {} MiB ({})",
        verdict(first_frame < Duration::from_millis(500)),
        verdict(connected_frame < Duration::from_millis(500)),
        verdict(ui_cpu + daemon_cpu < 1.0),
        mib(ui_rss),
        verdict(ui_rss < 150 * 1024 * 1024),
        mib(daemon_rss),
        verdict(daemon_rss < 50 * 1024 * 1024),
    );
    assert!(ui_rss > 0 && daemon_rss > 0, "a process was not found");
}
