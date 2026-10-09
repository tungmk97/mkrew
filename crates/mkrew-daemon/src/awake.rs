//! Keep awake (SPEC §20A.5): while at least one agent session (not shell) is `working` and
//! `awake.keep_awake`, the daemon holds a sleep inhibitor; it releases it [`LINGER`] after the
//! last one leaves `working`. Display sleep and screen lock stay allowed.
//!
//! [`start`] applies the policy to hub events and is tested with a fake [`Inhibitor`] on a
//! paused clock. `keep_awake` is read on every state event, so a settings change applies at
//! the next one. [`os`] is the real backend.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use mkrew_protocol::{Event, SessionKind, SessionState};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio::time::Instant;
use ulid::Ulid;

use crate::session::Hub;
use crate::settings::SettingsHolder;

/// How long the inhibitor stays held after the last agent stops working.
pub const LINGER: Duration = Duration::from_secs(5);

/// A system sleep inhibitor. Calls alternate: `acquire`, `release`, `acquire`, …
pub trait Inhibitor: Send + 'static {
    fn acquire(&mut self);
    fn release(&mut self);
}

/// Watches `hub` events and holds `inhibitor` per the policy; runs until aborted. The
/// inhibitor is released when the task ends or is aborted.
pub fn start(
    hub: Arc<Hub>,
    settings: Arc<SettingsHolder>,
    inhibitor: Box<dyn Inhibitor>,
) -> JoinHandle<()> {
    let mut events = hub.events.subscribe();
    tokio::spawn(async move {
        let mut hold = Hold {
            inhibitor,
            held: false,
        };
        let mut kinds: HashMap<Ulid, SessionKind> = HashMap::new();
        let mut working = working_agents(&hub);
        let mut release_at: Option<Instant> = None;
        loop {
            let want = !working.is_empty() && settings.current().awake.keep_awake;
            if want {
                release_at = None;
                hold.set(true);
            } else if hold.held && release_at.is_none() {
                release_at = Some(Instant::now() + LINGER);
            }
            tokio::select! {
                event = events.recv() => match event {
                    Ok(Event::SessionState(e)) => {
                        if e.state == SessionState::Working {
                            if is_agent(&hub, &mut kinds, e.session_id) {
                                working.insert(e.session_id);
                            }
                        } else {
                            working.remove(&e.session_id);
                        }
                        if e.state == SessionState::Exited {
                            kinds.remove(&e.session_id);
                        }
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(n, "keep awake missed events, re-reading sessions");
                        working = working_agents(&hub);
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                },
                _ = sleep_until(release_at), if release_at.is_some() => {
                    release_at = None;
                    hold.set(false);
                }
            }
        }
    })
}

/// The inhibitor and whether it is held; dropping releases it.
struct Hold {
    inhibitor: Box<dyn Inhibitor>,
    held: bool,
}

impl Hold {
    fn set(&mut self, held: bool) {
        if held == self.held {
            return;
        }
        self.held = held;
        if held {
            tracing::debug!("keep awake: acquire");
            self.inhibitor.acquire();
        } else {
            tracing::debug!("keep awake: release");
            self.inhibitor.release();
        }
    }
}

impl Drop for Hold {
    fn drop(&mut self) {
        self.set(false);
    }
}

async fn sleep_until(at: Option<Instant>) {
    if let Some(at) = at {
        tokio::time::sleep_until(at).await;
    }
}

/// Agent sessions the store has as working (start-up and after missed events).
fn working_agents(hub: &Hub) -> HashSet<Ulid> {
    let sessions = hub.store().list_sessions(None);
    match sessions {
        Ok(sessions) => sessions
            .into_iter()
            .filter(|s| s.kind == SessionKind::Agent && s.state == SessionState::Working)
            .map(|s| s.id)
            .collect(),
        Err(e) => {
            tracing::error!("keep awake: listing sessions failed: {e}");
            HashSet::new()
        }
    }
}

/// Whether `session_id` is an agent session; looked up once per session.
fn is_agent(hub: &Hub, kinds: &mut HashMap<Ulid, SessionKind>, session_id: Ulid) -> bool {
    if let Some(kind) = kinds.get(&session_id) {
        return *kind == SessionKind::Agent;
    }
    let session = hub.store().get_session(session_id);
    match session {
        Ok(session) => {
            kinds.insert(session_id, session.kind);
            session.kind == SessionKind::Agent
        }
        Err(e) => {
            tracing::warn!(%session_id, "keep awake: session lookup failed: {e}");
            false
        }
    }
}

/// The OS backends: Windows `SetThreadExecutionState` on a dedicated thread, macOS
/// `caffeinate`, Linux `systemd-inhibit`.
pub mod os {
    use super::Inhibitor;

    /// The inhibitor for this OS.
    pub fn inhibitor() -> Box<dyn Inhibitor> {
        Box::new(imp::OsInhibitor::default())
    }

    #[cfg(windows)]
    mod imp {
        use std::sync::mpsc;

        use windows_sys::Win32::System::Power::{
            ES_CONTINUOUS, ES_SYSTEM_REQUIRED, EXECUTION_STATE, SetThreadExecutionState,
        };

        use super::Inhibitor;

        /// The execution state flag belongs to the thread that set it, so one thread owns it
        /// for the daemon's lifetime and applies each request; it ends when this is dropped.
        pub struct OsInhibitor {
            requests: Option<mpsc::Sender<bool>>,
        }

        impl Default for OsInhibitor {
            fn default() -> Self {
                let (tx, rx) = mpsc::channel::<bool>();
                let spawned =
                    std::thread::Builder::new()
                        .name("keep-awake".into())
                        .spawn(move || {
                            for hold in rx {
                                let flags = if hold {
                                    ES_CONTINUOUS | ES_SYSTEM_REQUIRED
                                } else {
                                    ES_CONTINUOUS
                                };
                                if !set_execution_state(flags) {
                                    tracing::warn!("SetThreadExecutionState({flags:#x}) failed");
                                }
                            }
                        });
                let requests = match spawned {
                    Ok(_) => Some(tx),
                    Err(e) => {
                        tracing::warn!("keep awake is off: starting its thread failed: {e}");
                        None
                    }
                };
                Self { requests }
            }
        }

        /// Sets this thread's execution state; `false` = the call failed.
        #[allow(unsafe_code)]
        fn set_execution_state(flags: EXECUTION_STATE) -> bool {
            // SAFETY: plain FFI call taking flags by value; no pointers involved.
            unsafe { SetThreadExecutionState(flags) != 0 }
        }

        impl OsInhibitor {
            fn send(&self, hold: bool) {
                if let Some(requests) = &self.requests {
                    let _ = requests.send(hold);
                }
            }
        }

        impl Inhibitor for OsInhibitor {
            fn acquire(&mut self) {
                self.send(true);
            }

            fn release(&mut self) {
                self.send(false);
            }
        }
    }

    #[cfg(unix)]
    mod imp {
        use std::process::{Child, Command, Stdio};

        use super::Inhibitor;

        /// A child process that holds the inhibition while it runs.
        #[derive(Default)]
        pub struct OsInhibitor {
            child: Option<Child>,
            /// The tool is missing: logged once, then acquiring does nothing.
            missing: bool,
        }

        #[cfg(target_os = "macos")]
        fn command() -> Command {
            let mut command = Command::new("caffeinate");
            // `-w`: caffeinate also ends when the daemon dies.
            command.args(["-i", "-w", &std::process::id().to_string()]);
            command
        }

        #[cfg(not(target_os = "macos"))]
        fn command() -> Command {
            use std::os::unix::process::CommandExt as _;

            let mut command = Command::new("systemd-inhibit");
            command.args([
                "--what=idle:sleep",
                "--why=Mkrew",
                "--mode=block",
                "sleep",
                "infinity",
            ]);
            // Own group, so releasing ends `sleep` too.
            command.process_group(0);
            command
        }

        impl Inhibitor for OsInhibitor {
            fn acquire(&mut self) {
                if self.missing || self.child.is_some() {
                    return;
                }
                let spawned = command()
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn();
                match spawned {
                    Ok(child) => self.child = Some(child),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        self.missing = true;
                        tracing::warn!(
                            "keep awake is off: {:?} not found",
                            command().get_program()
                        );
                    }
                    Err(e) => tracing::warn!("keep awake: starting the inhibitor failed: {e}"),
                }
            }

            fn release(&mut self) {
                let Some(mut child) = self.child.take() else {
                    return;
                };
                #[cfg(not(target_os = "macos"))]
                {
                    use rustix::process::{Pid, Signal, kill_process_group};
                    let pid = i32::try_from(child.id()).ok().and_then(Pid::from_raw);
                    if let Some(pid) = pid {
                        let _ = kill_process_group(pid, Signal::TERM);
                    }
                }
                let _ = child.kill();
                let _ = child.wait();
            }
        }

        impl Drop for OsInhibitor {
            fn drop(&mut self) {
                self.release();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use mkrew_protocol::{Session, SessionStateEvent, StateSource};
    use mkrew_store::Store;

    use super::*;

    /// Records `true` per acquire and `false` per release, with the paused clock's time.
    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<Vec<(bool, Instant)>>>);

    impl Inhibitor for Fake {
        fn acquire(&mut self) {
            self.0.lock().unwrap().push((true, Instant::now()));
        }

        fn release(&mut self) {
            self.0.lock().unwrap().push((false, Instant::now()));
        }
    }

    impl Fake {
        fn calls(&self) -> Vec<bool> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .map(|(held, _)| *held)
                .collect()
        }
    }

    struct Setup {
        _dir: tempfile::TempDir,
        hub: Arc<Hub>,
        settings: Arc<SettingsHolder>,
        settings_file: std::path::PathBuf,
        project_id: Ulid,
    }

    fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("mkrew.db")).unwrap();
        let project_id = store.add_project(dir.path(), Some("repo")).unwrap().id;
        let settings_file = dir.path().join("settings.toml");
        Setup {
            hub: Arc::new(Hub::new(store)),
            settings: Arc::new(SettingsHolder::load(settings_file.clone())),
            settings_file,
            project_id,
            _dir: dir,
        }
    }

    impl Setup {
        fn add(&self, kind: SessionKind, state: SessionState) -> Ulid {
            let id = Ulid::new();
            self.hub
                .store()
                .insert_session(&Session {
                    id,
                    task_id: None,
                    project_id: self.project_id,
                    agent_profile: "claude".into(),
                    kind,
                    title: None,
                    terminal_title: None,
                    native_session_id: None,
                    state,
                    state_source: StateSource::Daemon,
                    state_changed_at: 0,
                    exit_code: None,
                    cols: 80,
                    rows: 24,
                    started_at: 0,
                    ended_at: None,
                })
                .unwrap();
            id
        }

        fn state(&self, session_id: Ulid, state: SessionState) {
            self.hub
                .events
                .send(Event::SessionState(SessionStateEvent {
                    session_id,
                    state,
                    source: StateSource::Hook,
                    changed_at: 0,
                }))
                .unwrap();
        }

        fn start(&self, fake: &Fake) -> JoinHandle<()> {
            start(
                Arc::clone(&self.hub),
                Arc::clone(&self.settings),
                Box::new(fake.clone()),
            )
        }
    }

    /// Lets the spawned task handle everything sent so far without moving the clock.
    async fn settle() {
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn held_while_an_agent_works_and_released_five_seconds_after_the_last_stops() {
        let s = setup();
        let a = s.add(SessionKind::Agent, SessionState::Idle);
        let b = s.add(SessionKind::Agent, SessionState::Idle);
        let fake = Fake::default();
        let task = s.start(&fake);
        settle().await;
        assert_eq!(fake.calls(), [] as [bool; 0]);

        s.state(a, SessionState::Working);
        s.state(b, SessionState::Working);
        settle().await;
        assert_eq!(fake.calls(), [true]);

        // One still works: held.
        s.state(a, SessionState::Done);
        settle().await;
        tokio::time::sleep(LINGER * 2).await;
        assert_eq!(fake.calls(), [true]);

        // The last one stops: released LINGER later, not before.
        s.state(b, SessionState::NeedsInput);
        settle().await;
        let stopped = Instant::now();
        tokio::time::sleep(LINGER - Duration::from_millis(1)).await;
        assert_eq!(fake.calls(), [true]);
        tokio::time::sleep(Duration::from_millis(1)).await;
        settle().await;
        assert_eq!(fake.calls(), [true, false]);
        assert_eq!(fake.0.lock().unwrap()[1].1, stopped + LINGER);
        task.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn working_again_within_the_linger_keeps_it_held() {
        let s = setup();
        let a = s.add(SessionKind::Agent, SessionState::Idle);
        let fake = Fake::default();
        let task = s.start(&fake);

        s.state(a, SessionState::Working);
        s.state(a, SessionState::Idle);
        settle().await;
        tokio::time::sleep(LINGER - Duration::from_secs(1)).await;
        s.state(a, SessionState::Working);
        settle().await;
        tokio::time::sleep(LINGER * 2).await;
        assert_eq!(fake.calls(), [true]);

        s.state(a, SessionState::Exited);
        settle().await;
        tokio::time::sleep(LINGER).await;
        settle().await;
        assert_eq!(fake.calls(), [true, false]);
        task.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn shells_never_hold_it() {
        let s = setup();
        let shell = s.add(SessionKind::Shell, SessionState::Idle);
        let fake = Fake::default();
        let task = s.start(&fake);

        s.state(shell, SessionState::Working);
        settle().await;
        tokio::time::sleep(LINGER).await;
        assert_eq!(fake.calls(), [] as [bool; 0]);
        task.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn the_keep_awake_switch_applies_at_the_next_state_event() {
        let s = setup();
        std::fs::write(&s.settings_file, "[awake]\nkeep_awake = false\n").unwrap();
        s.settings.reload();
        let a = s.add(SessionKind::Agent, SessionState::Idle);
        let b = s.add(SessionKind::Agent, SessionState::Idle);
        let fake = Fake::default();
        let task = s.start(&fake);

        s.state(a, SessionState::Working);
        settle().await;
        assert_eq!(fake.calls(), [] as [bool; 0]);

        std::fs::write(&s.settings_file, "").unwrap();
        s.settings.reload();
        s.state(b, SessionState::Working);
        settle().await;
        assert_eq!(fake.calls(), [true]);

        std::fs::write(&s.settings_file, "[awake]\nkeep_awake = false\n").unwrap();
        s.settings.reload();
        s.state(b, SessionState::Idle);
        settle().await;
        tokio::time::sleep(LINGER).await;
        settle().await;
        assert_eq!(fake.calls(), [true, false]);
        task.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn agents_already_working_at_start_hold_it_and_aborting_releases() {
        let s = setup();
        s.add(SessionKind::Agent, SessionState::Working);
        s.add(SessionKind::Shell, SessionState::Working);
        let fake = Fake::default();
        let task = s.start(&fake);
        settle().await;
        assert_eq!(fake.calls(), [true]);

        task.abort();
        let _ = task.await;
        assert_eq!(fake.calls(), [true, false]);
    }

    #[cfg(windows)]
    #[test]
    fn the_windows_backend_acquires_and_releases() {
        let mut inhibitor = os::inhibitor();
        inhibitor.acquire();
        inhibitor.release();
        drop(inhibitor);
    }
}
