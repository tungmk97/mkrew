//! Stress test (SPEC §15 throughput, §16): six sessions of the fake agent's `flood` scenario,
//! all attached on one UI connection and fed through the UI's `Router` and `Emulator` like the
//! window does. While they flood, the daemon must keep answering that connection quickly, the
//! UI-side feed of one drained batch must fit a frame, memory must stay flat, the daemon must
//! use less than one core, and every pane must end on the final screen.
//!
//! Needs `target/<profile>/mkrew-daemon` and `mkrew-fakeagent` built (`cargo test --workspace`).

// Helpers outside `#[test]` fns are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_lite::future::block_on;
use mkrew_core::Paths;
use mkrew_protocol::{
    DaemonStatusResult, Event, Project, ProjectAddParams, Request, SessionAttachResult,
    SessionInputParams, SessionKind, SessionRef, SessionSpawnResult,
};
use mkrew_ui::client::{Connection, Incoming};
use mkrew_ui::connect::{DaemonLauncher, POLL, TIMEOUT, connect_or_spawn};
use mkrew_ui::lifecycle;
use mkrew_ui::router::{Delivery, Router};
use mkrew_ui::terminal::emulator::Emulator;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use ulid::Ulid;

const PANES: usize = 6;
/// Flood lines per session; a line is ~70 bytes, so ~14 MB each in release (the manual §15 run
/// uses 50 MB, see `docs/HUMAN_CHECKS.md`). Debug builds assert no timings: a shorter flood,
/// still longer than a full scrollback, keeps `verify` quick.
const FLOOD_LINES: u32 = if cfg!(debug_assertions) {
    30_000
} else {
    200_000
};
/// Same as the window's `DRAIN_BATCH`.
const DRAIN_BATCH: usize = 256;
const RUN_TIMEOUT: Duration = Duration::from_secs(300);
/// §15: input latency < 16 ms. The test asserts it on the median round trip; slow CI runners
/// get headroom on the tail.
const FRAME: Duration = Duration::from_millis(16);
const TAIL_LIMIT: Duration = Duration::from_millis(250);
/// A pane fed this many lines has a full scrollback (the UI emulator keeps 1,000) and screen.
const WARM_LINES: usize = 1_100;
/// Growth allowed after the first sample with every scrollback full ("RAM stays flat").
const MEMORY_GROWTH: u64 = 16 * 1024 * 1024;
/// The daemon's per-attachment queues (4 MB each, `CLIENT_QUEUE_BYTES`) may still fill later.
const DAEMON_MEMORY_GROWTH: u64 = MEMORY_GROWTH + PANES as u64 * 4 * 1024 * 1024;

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

/// `flood.toml`: the built fake agent running a flood of [`FLOOD_LINES`] after a line of input
/// (so every pane is attached before the floods start).
fn write_flood_profile(paths: &Paths, dir: &Path) {
    let scenario = dir.join("flood-scenario.toml");
    std::fs::write(
        &scenario,
        format!(
            "[[step]]\naction = \"ask\"\ntext = \"Flood?\"\n\n\
             [[step]]\naction = \"flood\"\nlines = {FLOOD_LINES}\n\n\
             [[step]]\naction = \"print\"\ntext = \"Flood done.\"\n"
        ),
    )
    .unwrap();
    let fake = sibling_exe("mkrew-fakeagent");
    let sample = include_str!("../../../agents/fake.toml");
    let profile = sample
        .replace(
            "command = \"mkrew-fakeagent\"",
            &format!("command = '{}'", fake.display()),
        )
        .replace(
            "args = []",
            &format!("args = ['--scenario', '{}']", scenario.display()),
        );
    assert!(profile.contains("--scenario"));
    let agents = paths.config_dir.join("agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(agents.join("flood.toml"), profile).unwrap();
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

/// The window's side of the connection: router, one emulator per attached pane.
#[derive(Default)]
struct Ui {
    router: Router,
    panes: HashMap<Ulid, Emulator>,
    exited: HashMap<Ulid, Option<i32>>,
    /// Lines fed to each pane since it was built.
    lines: HashMap<Ulid, usize>,
    output_bytes: usize,
    snapshots: usize,
    /// Time to route and feed each drained batch (one frame's worth of work in the window).
    batch_times: Vec<Duration>,
}

impl Ui {
    fn deliver(&mut self, delivery: Delivery) {
        match delivery {
            Delivery::Output { session_id, data } => {
                self.output_bytes += data.len();
                if let Some(pane) = self.panes.get_mut(&session_id) {
                    pane.advance(&data);
                    *self.lines.entry(session_id).or_default() +=
                        data.iter().filter(|&&b| b == b'\n').count();
                }
            }
            Delivery::Snapshot {
                session_id,
                snapshot,
            } => {
                self.snapshots += 1;
                self.panes
                    .insert(session_id, Emulator::from_snapshot(&snapshot));
                self.lines.insert(session_id, 0);
            }
            Delivery::Exited {
                session_id,
                exit_code,
            } => {
                self.exited.insert(session_id, exit_code);
            }
        }
    }

    /// Every pane's scrollback is full, so its memory has reached its bound.
    fn warm(&self) -> bool {
        self.panes.len() == PANES
            && self
                .panes
                .keys()
                .all(|id| self.lines.get(id).is_some_and(|&lines| lines >= WARM_LINES))
    }

    fn screen(&self, session: Ulid) -> String {
        self.panes
            .get(&session)
            .map(|pane| pane.grid().to_text())
            .unwrap_or_default()
    }
}

/// Drains the connection like the window's frame loop until it closes. `exits` (count of
/// `session.exited` events) and `warm` ([`Ui::warm`] was reached) are readable without the `Ui`
/// lock, which a debug-build batch can hold for seconds.
fn drain(
    connection: &Connection,
    ui: Arc<Mutex<Ui>>,
    exits: Arc<AtomicUsize>,
    warm: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    let incoming = connection.incoming();
    std::thread::spawn(move || {
        while let Ok(first) = block_on(incoming.recv()) {
            let mut batch = vec![first];
            while batch.len() < DRAIN_BATCH
                && let Ok(next) = incoming.try_recv()
            {
                batch.push(next);
            }
            let exited = batch
                .iter()
                .filter(|incoming| matches!(incoming, Incoming::Event(Event::SessionExited(_))))
                .count();
            exits.fetch_add(exited, Ordering::Relaxed);
            let started = Instant::now();
            let mut ui = ui.lock().unwrap();
            for incoming in batch {
                if let Some(delivery) = ui.router.route(incoming) {
                    ui.deliver(delivery);
                }
            }
            ui.batch_times.push(started.elapsed());
            if ui.warm() {
                warm.store(true, Ordering::Relaxed);
            }
        }
    })
}

/// Attaches like `Shell::attach`: buffer in the router, then build the pane from the reply.
fn attach(connection: &Connection, ui: &Mutex<Ui>, session: Ulid) {
    let generation = ui.lock().unwrap().router.begin_attach(session).unwrap();
    let attached: SessionAttachResult =
        block_on(connection.call(Request::SessionAttach(SessionRef {
            session_id: session,
        })))
        .unwrap();
    let mut ui = ui.lock().unwrap();
    let buffered = ui.router.finish_attach(session, generation).unwrap();
    let snapshot = buffered.snapshot.unwrap_or(attached.snapshot);
    let mut pane = Emulator::from_snapshot(&snapshot);
    pane.advance(&buffered.output);
    ui.panes.insert(session, pane);
    if let Some(exit_code) = buffered.exited {
        ui.exited.insert(session, exit_code);
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

fn percentile(sorted: &[Duration], at: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let index = ((sorted.len() - 1) as f64 * at).round() as usize;
    sorted[index]
}

fn mib(bytes: u64) -> u64 {
    bytes / (1024 * 1024)
}

#[test]
fn six_attached_floods_keep_the_ui_connection_responsive() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path());
    write_flood_profile(&paths, dir.path());
    let _stop = StopOnDrop(paths.clone());
    let project_dir = dir.path().join("proj");
    std::fs::create_dir(&project_dir).unwrap();

    let mut launcher =
        DaemonLauncher::new(paths.clone(), sibling_exe("mkrew-daemon")).without_notifications();
    let connection = connect_or_spawn(&mut launcher, TIMEOUT, POLL).unwrap();
    let ui = Arc::new(Mutex::new(Ui::default()));
    let exits = Arc::new(AtomicUsize::new(0));
    let warm = Arc::new(AtomicBool::new(false));
    let drainer = drain(&connection, ui.clone(), exits.clone(), warm.clone());
    let status: DaemonStatusResult = block_on(connection.call(Request::DaemonStatus)).unwrap();
    let mut daemon = Probe::new(status.pid);
    let mut client = Probe::new(std::process::id());
    let project: Project = block_on(connection.call(Request::ProjectAdd(ProjectAddParams {
        path: project_dir.to_string_lossy().into_owned(),
        name: None,
    })))
    .unwrap();

    let mut sessions = Vec::new();
    for _ in 0..PANES {
        let mut params = lifecycle::spawn_params(project.id, None, SessionKind::Agent, None);
        params.agent_profile = Some("flood".into());
        let spawned: SessionSpawnResult =
            block_on(connection.call(Request::SessionSpawn(params))).unwrap();
        attach(&connection, &ui, spawned.session_id);
        sessions.push(spawned.session_id);
    }
    let started = Instant::now();
    for &session_id in &sessions {
        block_on(
            connection.call::<()>(Request::SessionInput(SessionInputParams {
                session_id,
                data: b"y\r".to_vec(),
            })),
        )
        .unwrap();
    }

    // While flooding: round trips on the UI connection, daemon CPU, and memory (daemon,
    // client) once every scrollback is full.
    let mut round_trips = Vec::new();
    let mut memory = Vec::new();
    let mut daemon_cpu = Vec::new();
    let mut last_sample = Instant::now();
    // Only these sessions exist, and every connection gets every event.
    while exits.load(Ordering::Relaxed) < PANES {
        assert!(started.elapsed() < RUN_TIMEOUT, "the floods did not finish");
        let sent = Instant::now();
        let _: DaemonStatusResult = block_on(connection.call(Request::DaemonStatus)).unwrap();
        round_trips.push(sent.elapsed());
        if last_sample.elapsed() >= Duration::from_millis(250) {
            last_sample = Instant::now();
            let (daemon_rss, cpu) = daemon.sample();
            let (client_rss, _) = client.sample();
            if warm.load(Ordering::Relaxed) {
                memory.push((daemon_rss, client_rss));
            }
            daemon_cpu.push(cpu);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let flood_time = started.elapsed();

    // Every pane ends on the final screen (the exit event may overtake the last output).
    let deadline = Instant::now() + Duration::from_secs(60);
    for &id in &sessions {
        loop {
            let ui = ui.lock().unwrap();
            if ui.screen(id).contains("Flood done.") && ui.exited.contains_key(&id) {
                assert_eq!(ui.exited[&id], Some(0));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "pane {id} did not end on the final screen:\n{}",
                ui.screen(id)
            );
            drop(ui);
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    round_trips.sort();
    let mut batch_times = ui.lock().unwrap().batch_times.clone();
    batch_times.sort();
    let (output_bytes, snapshots) = {
        let ui = ui.lock().unwrap();
        (ui.output_bytes, ui.snapshots)
    };
    let mean_cpu = daemon_cpu.iter().sum::<f32>() / daemon_cpu.len().max(1) as f32;
    let first = memory.first().copied().unwrap_or_default();
    let peak = memory
        .iter()
        .fold((0, 0), |(d, c), &(dm, cm)| (d.max(dm), c.max(cm)));
    eprintln!(
        "stress: {PANES} panes in {flood_time:.1?}, {} MiB output + {snapshots} snapshots; \
         round trip p50={:?} p95={:?} max={:?} ({}); batch p50={:?} p95={:?} max={:?} ({}); \
         daemon cpu mean={mean_cpu:.0}% of a core; with full scrollbacks ({} samples): \
         daemon rss first={} peak={} MiB, client rss first={} peak={} MiB",
        mib(output_bytes as u64),
        percentile(&round_trips, 0.5),
        percentile(&round_trips, 0.95),
        percentile(&round_trips, 1.0),
        round_trips.len(),
        percentile(&batch_times, 0.5),
        percentile(&batch_times, 0.95),
        percentile(&batch_times, 1.0),
        batch_times.len(),
        memory.len(),
        mib(first.0),
        mib(peak.0),
        mib(first.1),
        mib(peak.1),
    );

    assert!(
        peak.0 < first.0 + DAEMON_MEMORY_GROWTH,
        "daemon memory grew while flooding"
    );
    // Unoptimized emulators parse a flood many times slower than a frame (`verify` runs debug
    // builds), so overflow snapshots may keep the scrollbacks from filling, and debug CPU says
    // nothing about release. So the §15 timings, the client's memory and the daemon's CPU are
    // asserted on `cargo test --release`.
    if !cfg!(debug_assertions) {
        assert!(
            mean_cpu < 100.0,
            "the daemon used {mean_cpu:.0}% of a core while flooding"
        );
        assert!(
            memory.len() >= 2,
            "the flood ended before memory was sampled with full scrollbacks"
        );
        assert!(
            peak.1 < first.1 + MEMORY_GROWTH,
            "client memory grew after the scrollbacks were full"
        );
        assert!(
            percentile(&round_trips, 0.5) < FRAME,
            "median round trip on the UI connection is a frame or more"
        );
        assert!(percentile(&round_trips, 0.95) < TAIL_LIMIT);
        assert!(
            percentile(&batch_times, 0.5) < FRAME,
            "feeding a drained batch takes a frame or more"
        );
    }

    drop(connection);
    drainer.join().unwrap();
}
