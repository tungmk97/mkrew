//! Live sessions (SPEC §8): one PTY per session, one blocking reader OS thread feeding a
//! bounded channel, and one tokio task owning the headless emulator, the state machine and
//! the session log.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use mkrew_core::{AgentProfile, StateMachine};
use mkrew_protocol::{
    Event, Session, SessionAttachResult, SessionExitedEvent, SessionState, SessionStateEvent,
};
use mkrew_store::Store;
use portable_pty::{ChildKiller, MasterPty, PtySize, native_pty_system};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinSet;
use ulid::Ulid;

use crate::attach::Attachment;
use crate::emulator::Emulator;
use crate::launch;
use crate::session_log::SessionLog;

const READ_BUFFER_BYTES: usize = 64 * 1024;
/// Chunks in flight between a reader thread and its session task. When full the reader
/// blocks, which back-pressures the child instead of buffering without bound.
const OUTPUT_CHANNEL_CHUNKS: usize = 64;
/// Graceful kill: after closing the PTY, wait this long for the exit before killing.
pub const KILL_GRACE: Duration = Duration::from_secs(5);
/// ConPTY can deliver output after the exit; keep reading while it trickles in.
const AFTER_EXIT_IDLE: Duration = Duration::from_millis(200);
const AFTER_EXIT_MAX: Duration = Duration::from_secs(1);

/// State shared by the server and every session task.
pub struct Hub {
    pub store: Mutex<Store>,
    pub events: broadcast::Sender<Event>,
    live: Mutex<HashMap<Ulid, mpsc::Sender<Command>>>,
    tasks: Mutex<JoinSet<()>>,
    /// Set by [`Hub::shutdown`]; refuses new sessions.
    stopping: AtomicBool,
}

#[derive(Debug)]
pub enum Command {
    Kill {
        force: bool,
    },
    /// Replies with the current screen; output after it goes to `attachment`.
    Attach {
        attachment: Arc<Attachment>,
        reply: oneshot::Sender<SessionAttachResult>,
    },
    /// The attachment overflowed: give it a fresh snapshot.
    Resnapshot(Arc<Attachment>),
    Input(Vec<u8>),
    Resize {
        cols: u16,
        rows: u16,
    },
}

/// What a session task receives from its PTY threads.
enum PtyEvent {
    Output(Vec<u8>),
    Exited(Option<i32>),
}

/// Everything needed to start a session; `session` is the row to insert.
pub struct SpawnSpec {
    pub session: Session,
    pub profile: AgentProfile,
    pub cwd: PathBuf,
    /// `MKREW_SOCKET` inside the PTY.
    pub socket: String,
    pub log_file: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    #[error(transparent)]
    Launch(launch::LaunchError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl Hub {
    pub fn new(store: Store) -> Self {
        Self {
            store: Mutex::new(store),
            events: broadcast::channel(256).0,
            live: Mutex::default(),
            tasks: Mutex::default(),
            stopping: AtomicBool::new(false),
        }
    }

    pub fn store(&self) -> MutexGuard<'_, Store> {
        // A panic mid-query leaves the connection usable (SQLite rolls back), so ignore poison.
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn live_count(&self) -> usize {
        lock(&self.live).len()
    }

    /// Starts the PTY and its session task. Must run inside the tokio runtime.
    pub fn spawn(self: &std::sync::Arc<Self>, spec: SpawnSpec) -> Result<(), SpawnError> {
        let SpawnSpec {
            session,
            profile,
            cwd,
            socket,
            log_file,
        } = spec;
        let mut cmd = launch::command(&profile).map_err(SpawnError::Launch)?;
        cmd.cwd(&cwd);
        cmd.env("MKREW_SESSION_ID", session.id.to_string());
        cmd.env(mkrew_core::paths::ENV_SOCKET, &socket);
        cmd.env("TERM", "xterm-256color");
        let machine = StateMachine::new(&profile.state).context("compile state patterns")?;
        let log = SessionLog::create(&log_file)
            .with_context(|| format!("create {}", log_file.display()))?;

        let pair = native_pty_system()
            .openpty(PtySize {
                rows: session.rows,
                cols: session.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("open pty")?;
        let mut reader = pair.master.try_clone_reader().context("clone pty reader")?;
        let writer = pair.master.take_writer().context("take pty writer")?;
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .with_context(|| format!("spawn {}", profile.command))?;
        // Dropping the slave lets the Unix reader see EOF once the child exits.
        drop(pair.slave);
        let mut killer = child.clone_killer();
        let pid = child.process_id();
        // From here on a failure must not leave an orphaned child behind.
        let mut kill_on_error = |e: anyhow::Error| {
            let _ = killer.kill();
            SpawnError::Other(e)
        };

        let (tx, rx) = mpsc::channel(OUTPUT_CHANNEL_CHUNKS);
        // Blocking reads live on their own OS thread, never in async tasks (SPEC §8).
        let reader_tx = tx.clone();
        thread::Builder::new()
            .name(format!("pty-read-{}", session.id))
            .spawn(move || {
                let mut buf = vec![0u8; READ_BUFFER_BYTES];
                loop {
                    let n = match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    if reader_tx
                        .blocking_send(PtyEvent::Output(buf[..n].to_vec()))
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .context("start pty reader thread")
            .map_err(&mut kill_on_error)?;
        // ConPTY never signals EOF on child exit, so exit is detected by waiting on the child.
        thread::Builder::new()
            .name(format!("pty-wait-{}", session.id))
            .spawn(move || {
                let code = child.wait().ok().map(|status| {
                    // Windows exit codes are u32 (NTSTATUS values wrap to negative), Unix fit.
                    i32::from_ne_bytes(status.exit_code().to_ne_bytes())
                });
                let _ = tx.blocking_send(PtyEvent::Exited(code));
            })
            .context("start pty wait thread")
            .map_err(&mut kill_on_error)?;

        let (commands_tx, commands) = mpsc::channel(8);
        {
            // Checked under the `live` lock that `shutdown` drains, so no session slips past it.
            let mut live = lock(&self.live);
            if self.stopping.load(Ordering::SeqCst) {
                return Err(kill_on_error(anyhow::anyhow!("the daemon is stopping")));
            }
            self.store()
                .insert_session(&session)
                .context("insert session")
                .map_err(&mut kill_on_error)?;
            live.insert(session.id, commands_tx);
        }
        tracing::info!(id = %session.id, profile = %profile.id, cwd = %cwd.display(), "session spawned");
        let actor = Actor {
            hub: std::sync::Arc::clone(self),
            session,
            master: Some(pair.master),
            writer,
            killer,
            pid,
            emulator: None,
            machine,
            log,
            attachments: Vec::new(),
        };
        lock(&self.tasks).spawn(actor.run(rx, commands));
        Ok(())
    }

    /// Sends `command` to a live session; `false` if it is not running.
    pub fn command(&self, id: Ulid, command: Command) -> bool {
        self.sender(id).is_some_and(|s| s.try_send(command).is_ok())
    }

    /// The command channel of a live session (waits for room instead of failing when full).
    pub fn sender(&self, id: Ulid) -> Option<mpsc::Sender<Command>> {
        lock(&self.live).get(&id).cloned()
    }

    /// Daemon shutdown (SPEC §8): asks every session to terminate and waits for all of them
    /// (each one kills its child after [`KILL_GRACE`]).
    pub async fn shutdown(&self) {
        let live: Vec<_> = {
            let mut live = lock(&self.live);
            self.stopping.store(true, Ordering::SeqCst);
            live.drain().collect()
        };
        for (_, sender) in live {
            let _ = sender.try_send(Command::Kill { force: false });
        }
        let mut tasks = std::mem::take(&mut *lock(&self.tasks));
        let all_done = async { while tasks.join_next().await.is_some() {} };
        let limit = KILL_GRACE + AFTER_EXIT_MAX + Duration::from_secs(1);
        if tokio::time::timeout(limit, all_done).await.is_err() {
            tracing::warn!("sessions did not stop in time");
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Plain maps: still consistent if a holder panicked.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One session task: the only owner of the session's emulator, state machine and log.
struct Actor {
    hub: std::sync::Arc<Hub>,
    session: Session,
    /// `None` once closed (graceful kill or exit).
    master: Option<Box<dyn MasterPty + Send>>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    /// For `SIGKILL` (Unix only; Windows kills through `killer`).
    #[cfg_attr(not(unix), allow(dead_code))]
    pid: Option<u32>,
    /// Created lazily so a session that never prints costs no grid memory.
    emulator: Option<Emulator>,
    machine: StateMachine,
    log: SessionLog,
    /// Attached clients; closed ones are dropped on the next output.
    attachments: Vec<Arc<Attachment>>,
}

impl Actor {
    async fn run(
        mut self,
        mut output: mpsc::Receiver<PtyEvent>,
        mut commands: mpsc::Receiver<Command>,
    ) {
        let mut commands_open = true;
        let mut kill_at: Option<tokio::time::Instant> = None;
        let exit_code = loop {
            let tick_at = self
                .machine
                .next_deadline()
                .map(tokio::time::Instant::from_std);
            tokio::select! {
                event = output.recv() => match event {
                    Some(PtyEvent::Output(bytes)) => self.on_output(&bytes),
                    Some(PtyEvent::Exited(code)) => {
                        drain_after_exit(&mut self, &mut output).await;
                        break code;
                    }
                    None => break None,
                },
                command = commands.recv(), if commands_open => match command {
                    Some(Command::Kill { force: true }) => self.kill(),
                    Some(Command::Kill { force: false }) => {
                        tracing::info!(id = %self.session.id, "asking the session to terminate");
                        self.terminate();
                        kill_at.get_or_insert(tokio::time::Instant::now() + KILL_GRACE);
                    }
                    Some(Command::Attach { attachment, reply }) => {
                        let result = SessionAttachResult {
                            state: self.session.state,
                            snapshot: self.emulator().snapshot(),
                        };
                        if reply.send(result).is_ok() {
                            self.attachments.push(attachment);
                        }
                    }
                    Some(Command::Resnapshot(attachment)) => {
                        attachment.resnapshot(self.emulator().snapshot());
                    }
                    Some(Command::Input(bytes)) => self.write_pty(&bytes),
                    Some(Command::Resize { cols, rows }) => self.resize(cols, rows),
                    None => commands_open = false,
                },
                () = sleep_until(tick_at), if tick_at.is_some() => {
                    let lines = self.screen_lines();
                    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
                    if let Some(state) = self.machine.tick(Instant::now(), &lines) {
                        self.set_state(state);
                    }
                }
                () = sleep_until(kill_at), if kill_at.is_some() => {
                    tracing::warn!(id = %self.session.id, "no exit after closing the pty; killing");
                    kill_at = None;
                    self.kill();
                }
            }
        };
        self.finish(exit_code);
    }

    fn on_output(&mut self, bytes: &[u8]) {
        if let Err(e) = self.log.write(bytes) {
            tracing::warn!(id = %self.session.id, "session log write failed: {e}");
        }
        let reply = self.emulator().advance(bytes);
        // Small and rare (DSR answers), so a blocking write here is fine.
        if !reply.is_empty() {
            self.write_pty(&reply);
        }
        self.attachments.retain(|a| !a.is_closed());
        for attachment in &self.attachments {
            attachment.push(bytes);
        }
        let lines = self.screen_lines();
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        if let Some(state) = self.machine.on_output(Instant::now(), &lines) {
            self.set_state(state);
        }
    }

    fn emulator(&mut self) -> &mut Emulator {
        let (cols, rows) = (self.session.cols, self.session.rows);
        self.emulator
            .get_or_insert_with(|| Emulator::new(cols, rows))
    }

    /// Blocking, like any PTY write; input is small (keys, pastes) and the child reads it.
    fn write_pty(&mut self, bytes: &[u8]) {
        if let Err(e) = self
            .writer
            .write_all(bytes)
            .and_then(|()| self.writer.flush())
        {
            tracing::debug!(id = %self.session.id, "pty write failed: {e}");
        }
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        if (cols, rows) == (self.session.cols, self.session.rows) {
            return;
        }
        if let Some(master) = &self.master {
            let size = PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            };
            if let Err(e) = master.resize(size) {
                tracing::warn!(id = %self.session.id, "pty resize failed: {e}");
                return;
            }
        }
        if let Some(emulator) = &mut self.emulator {
            emulator.resize(cols, rows);
        }
        self.session.cols = cols;
        self.session.rows = rows;
        self.save();
    }

    fn screen_lines(&self) -> Vec<String> {
        self.emulator
            .as_ref()
            .map(Emulator::lines_to_cursor)
            .unwrap_or_default()
    }

    fn set_state(&mut self, state: SessionState) {
        tracing::debug!(id = %self.session.id, ?state, "session state");
        self.session.state = state;
        self.save();
        let _ = self.hub.events.send(Event::SessionState(SessionStateEvent {
            session_id: self.session.id,
            state,
        }));
    }

    fn save(&self) {
        if let Err(e) = self.hub.store().update_session(&self.session) {
            tracing::error!(id = %self.session.id, "saving session failed: {e}");
        }
    }

    /// Graceful: Windows closes the pseudo console (the child gets `CTRL_CLOSE_EVENT`); on
    /// Unix portable-pty's cloned killer sends `SIGHUP`.
    fn terminate(&mut self) {
        if cfg!(windows) {
            self.close_master();
        } else {
            let _ = self.killer.kill();
        }
    }

    /// Forced: `TerminateProcess` on Windows, `SIGKILL` on Unix.
    fn kill(&mut self) {
        #[cfg(windows)]
        {
            // portable-pty 0.9 inverts this result on Windows; the exit is observed anyway.
            let _ = self.killer.kill();
        }
        #[cfg(unix)]
        {
            use rustix::process::{Pid, Signal, kill_process};
            let pid = self.pid.and_then(|p| Pid::from_raw(i32::try_from(p).ok()?));
            if let Some(pid) = pid
                && let Err(e) = kill_process(pid, Signal::KILL)
            {
                tracing::debug!(id = %self.session.id, "SIGKILL failed (already exited?): {e}");
            }
        }
    }

    fn close_master(&mut self) {
        // Closing a pseudo console can block on Windows; keep it off the runtime.
        if let Some(master) = self.master.take() {
            thread::spawn(move || drop(master));
        }
    }

    fn finish(mut self, exit_code: Option<i32>) {
        if let Err(e) = self.log.flush() {
            tracing::warn!(id = %self.session.id, "session log flush failed: {e}");
        }
        self.close_master();
        lock(&self.hub.live).remove(&self.session.id);
        // No one is left to answer a later `Resnapshot`: hand out the final screen now.
        self.attachments
            .retain(|a| !a.is_closed() && a.is_overflowed());
        if !self.attachments.is_empty() {
            let snapshot = self.emulator().snapshot();
            for attachment in &self.attachments {
                attachment.resnapshot(snapshot.clone());
            }
        }
        self.machine.on_exit();
        self.session.exit_code = exit_code;
        self.session.ended_at = Some(crate::server::now_ms());
        self.set_state(SessionState::Exited);
        let _ = self
            .hub
            .events
            .send(Event::SessionExited(SessionExitedEvent {
                session_id: self.session.id,
                exit_code,
            }));
        tracing::info!(id = %self.session.id, ?exit_code, "session exited");
    }
}

/// Feeds output that arrives shortly after the exit (ConPTY flushes late).
async fn drain_after_exit(actor: &mut Actor, output: &mut mpsc::Receiver<PtyEvent>) {
    let end = tokio::time::Instant::now() + AFTER_EXIT_MAX;
    while let Ok(Some(PtyEvent::Output(bytes))) = tokio::time::timeout_at(
        end.min(tokio::time::Instant::now() + AFTER_EXIT_IDLE),
        output.recv(),
    )
    .await
    {
        actor.on_output(&bytes);
    }
}

async fn sleep_until(at: Option<tokio::time::Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}
