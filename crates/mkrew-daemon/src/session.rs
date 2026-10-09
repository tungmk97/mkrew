//! Live sessions (SPEC §8): one PTY per session, one blocking reader OS thread feeding a
//! bounded channel, one blocking writer OS thread fed by a bounded input channel, and one
//! tokio task owning the headless emulator, the state machine and the session log.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use mkrew_core::turn::{TurnEvent, TurnTracker};
use mkrew_core::{AgentProfile, HookAgent, SendQueue, StateMachine, conflict};
use mkrew_protocol::{
    ConflictDetectedEvent, DocChangedEvent, DocComment, Event, GitChangedEvent, HookReportParams,
    QueuedSend, Session, SessionAttachResult, SessionEditedEvent, SessionExitedEvent, SessionKind,
    SessionPortsEvent, SessionQueueEvent, SessionReadResult, SessionState, SessionStateEvent,
    SessionTerminalTitleEvent, StateSource,
};
use mkrew_store::Store;
use portable_pty::{ChildKiller, MasterPty, PtySize, native_pty_system};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinSet;
use ulid::Ulid;

use crate::attach::Attachment;
use crate::docs;
use crate::emulator::Emulator;
use crate::files::FileIndex;
use crate::git::{GitBackend, GitCli};
use crate::launch;
use crate::session_log::SessionLog;
use crate::snapshot::Snapshots;
use crate::turns;
use crate::usage;
use crate::watch::{self, Watch};

const READ_BUFFER_BYTES: usize = 64 * 1024;
/// Chunks in flight between a reader thread and its session task. When full the reader
/// blocks, which back-pressures the child instead of buffering without bound.
const OUTPUT_CHANNEL_CHUNKS: usize = 64;
/// The session task merges queued output chunks up to this size before handling them. State
/// patterns then see the screen once per batch, so a prompt drawn and scrolled away within
/// one batch can be missed (only under heavy output).
const COALESCE_BYTES: usize = 256 * 1024;
/// A session that handled more than [`FLOOD_BYTES`] of output within one [`FLOOD_WINDOW`] is
/// flooding: its batches wait for [`Hub::flood_turn`], so floods are handled one at a time,
/// with [`Pacer`] pauses so that work (mostly the emulator) takes at most 2/3 of a core. Six
/// unthrottled floods then keep the whole daemon under one core (SPEC §15, measured by
/// `mkrew-ui/tests/stress.rs`). Sessions under that rate (typing, prompts, redraws) never wait
/// for the turn, and
/// a waiting session still serves its commands; its PTY reader blocks meanwhile, which
/// back-pressures the child.
const FLOOD_WINDOW: Duration = Duration::from_millis(100);
const FLOOD_BYTES: usize = 64 * 1024;
/// Pause ⇒ 1/2 of the time spent handling flood batches.
const PACE_IDLE_DIVISOR: u32 = 2;
/// Pauses shorter than this are carried over: Windows timers are coarse (~15 ms).
const PACE_MIN_PAUSE: Duration = Duration::from_millis(16);
/// Input is queued for the writer thread in chunks of at most this size, so the channel bound
/// below caps the bytes buffered per session (≈ 256 KiB) whatever the size of one request.
pub const INPUT_CHUNK_BYTES: usize = 16 * 1024;
const INPUT_CHANNEL_CHUNKS: usize = 16;
/// Graceful kill: after closing the PTY, wait this long for the exit before killing.
pub const KILL_GRACE: Duration = Duration::from_secs(5);
/// ConPTY can deliver output after the exit; keep reading while it trickles in.
const AFTER_EXIT_IDLE: Duration = Duration::from_millis(200);
const AFTER_EXIT_MAX: Duration = Duration::from_secs(1);

/// State shared by the server and every session task.
pub struct Hub {
    pub store: Mutex<Store>,
    pub events: broadcast::Sender<Event>,
    live: Mutex<HashMap<Ulid, Live>>,
    /// One file watcher per project with live sessions or read docs, and one per task worktree
    /// a Files or Git view opened (keyed by the task id). Lock order: `watches`, then `live`.
    watches: Mutex<HashMap<Ulid, ProjectWatch>>,
    /// Last content hash (`None` = deleted) of each Markdown file the watchers reported, by
    /// project and relative path. Never held with another lock.
    doc_hashes: Mutex<HashMap<(Ulid, String), Option<String>>>,
    /// Recent file editors per project (SPEC §7 `conflict.detected`). Lock order: `watches`,
    /// then `conflicts`; never held with `live` or `store`.
    conflicts: Mutex<HashMap<Ulid, conflict::Tracker<Ulid>>>,
    /// `fs.find`'s file lists; a watcher batch with changed files invalidates its project's.
    pub file_index: FileIndex,
    tasks: Mutex<JoinSet<()>>,
    /// Set by [`Hub::shutdown`]; refuses new sessions.
    stopping: AtomicBool,
    /// Held by a flooding session task while it handles one output batch; see [`take_turn`].
    flood_turn: tokio::sync::Mutex<Pacer>,
    pub git: Arc<dyn GitBackend>,
    /// Turn snapshots (SPEC §20C.1), one at a time per project.
    pub snapshots: Snapshots,
    /// Profiles whose unknown usage format was already logged.
    usage_warned: Mutex<HashSet<String>>,
    /// Ports printed by live run sessions (SPEC §10); memory only.
    pub ports: Mutex<HashMap<Ulid, Vec<u16>>>,
    /// Cleaned titles agent sessions gave their terminal (ADR 0031); memory only, kept until
    /// the daemon stops, so exited sessions keep their name in `session.list`.
    // ponytail: one ≤ 60-char string per titled agent session; prune on exit if it grows.
    pub terminal_titles: Mutex<HashMap<Ulid, String>>,
}

/// Idle time owed by flood handling: 1/[`PACE_IDLE_DIVISOR`] of the time spent, minus the idle
/// time since the previous batch. Once it reaches [`PACE_MIN_PAUSE`] no flood batch starts
/// before `resume_at`.
#[derive(Debug, Default)]
struct Pacer {
    debt: Duration,
    last_end: Option<Instant>,
    resume_at: Option<Instant>,
}

impl Pacer {
    /// Records one batch handled from `started` to `ended`.
    fn after_batch(&mut self, started: Instant, ended: Instant) {
        let idle = self.last_end.map_or(Duration::MAX, |last| {
            started.saturating_duration_since(last)
        });
        self.debt = self.debt.saturating_sub(idle)
            + ended.saturating_duration_since(started) / PACE_IDLE_DIVISOR;
        self.last_end = Some(ended);
        if self.debt >= PACE_MIN_PAUSE {
            let resume_at = ended + std::mem::take(&mut self.debt);
            self.resume_at = Some(resume_at);
            // The pause is not idle time that pays off later debt.
            self.last_end = Some(resume_at);
        }
    }
}

/// Waits until no other flood batch is being handled and the pacer's pause is over.
async fn take_turn(turn: &tokio::sync::Mutex<Pacer>) -> tokio::sync::MutexGuard<'_, Pacer> {
    loop {
        let pacer = turn.lock().await;
        match pacer.resume_at {
            Some(at) if at > Instant::now() => {
                drop(pacer);
                tokio::time::sleep_until(tokio::time::Instant::from_std(at)).await;
            }
            _ => return pacer,
        }
    }
}

/// Output handled by one session in the current [`FLOOD_WINDOW`].
#[derive(Debug, Default)]
struct OutputRate {
    window_start: Option<Instant>,
    bytes: usize,
}

impl OutputRate {
    /// Counts `bytes` handled at `now`; `true` once the session is flooding.
    fn add(&mut self, now: Instant, bytes: usize) -> bool {
        if self
            .window_start
            .is_none_or(|start| now.saturating_duration_since(start) >= FLOOD_WINDOW)
        {
            self.window_start = Some(now);
            self.bytes = 0;
        }
        self.bytes += bytes;
        self.bytes > FLOOD_BYTES
    }
}

/// Appends output chunks already queued, up to [`COALESCE_BYTES`], so the per-output work
/// (screen lines, state patterns) runs once per batch during a flood (SPEC §15). Returns the
/// exit if it was queued behind them.
fn coalesce(bytes: &mut Vec<u8>, output: &mut mpsc::Receiver<PtyEvent>) -> Option<Option<i32>> {
    while bytes.len() < COALESCE_BYTES {
        match output.try_recv() {
            Ok(PtyEvent::Output(more)) => bytes.extend_from_slice(&more),
            Ok(PtyEvent::Exited(code)) => return Some(code),
            Err(_) => break,
        }
    }
    None
}

/// A project's file watcher. `kept`: a client listed or read its docs or opened its Git view,
/// so it stays watched after its last session exits, until the daemon stops (HUMAN_CHECKS
/// T6.4).
struct ProjectWatch {
    _watch: Watch,
    kept: bool,
}

/// The channels of one live session.
struct Live {
    project_id: Ulid,
    commands: mpsc::Sender<Command>,
    /// To the PTY writer thread; chunks of at most [`INPUT_CHUNK_BYTES`]. Held locked while
    /// one request is sent so two clients' input never interleaves.
    input: Arc<tokio::sync::Mutex<mpsc::Sender<Vec<u8>>>>,
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
    /// `session.read`: the last `lines` non-blank screen rows and the state.
    Read {
        lines: usize,
        reply: oneshot::Sender<SessionReadResult>,
    },
    /// The attachment overflowed: give it a fresh snapshot.
    Resnapshot(Arc<Attachment>),
    Resize {
        cols: u16,
        rows: u16,
    },
    /// `session.send`: queued until the state allows delivery (SPEC §8). `reply` gets whether
    /// the text still waits in the queue after the delivery check.
    Send {
        id: Ulid,
        text: String,
        reply: Option<oneshot::Sender<bool>>,
    },
    /// `session.unqueue`: `reply` gets whether the text was still waiting (SPEC §20B.2).
    Unqueue {
        id: Ulid,
        reply: oneshot::Sender<bool>,
    },
    /// The send-writer task has queued a delivered text for the PTY.
    SendWritten,
    /// `hook.report` from inside the session (SPEC §6); `agent` is `report.agent` parsed.
    /// `reply` is dropped once handled; for a hook that started a turn, after its start
    /// snapshot (SPEC §20C.1).
    Hook {
        agent: HookAgent,
        report: HookReportParams,
        reply: oneshot::Sender<()>,
    },
    /// A watcher batch attributed to this session (relative paths): goes to its open turn.
    TurnFiles(Vec<String>),
    /// `session.rename`: the actor owns the row (its next save would undo a store-only
    /// rename). Replies with the renamed session, also announced as `session.updated`.
    Rename {
        title: Option<String>,
        reply: oneshot::Sender<Session>,
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
    /// The project folder, which the project's file watcher covers (`cwd` may be a task
    /// worktree instead, SPEC §20E.1).
    pub project_dir: PathBuf,
    /// `MKREW_SOCKET` inside the PTY.
    pub socket: String,
    pub log_file: PathBuf,
    /// Text queued like a `session.send` before the session starts (`prompt_mode = "type"`):
    /// written once the agent is first ready.
    pub typed: Option<String>,
    /// A line of mkrew's own shown before the program's output (and kept in its log).
    pub banner: Option<String>,
    /// Extra environment (run commands, SPEC §10); after the profile's, before mkrew's own.
    pub env: Vec<(String, String)>,
}

#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    #[error(transparent)]
    Launch(launch::LaunchError),
    #[error("task {0} is already running")]
    TaskRunning(Ulid),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl Hub {
    pub fn new(store: Store) -> Self {
        Self {
            store: Mutex::new(store),
            events: broadcast::channel(256).0,
            live: Mutex::default(),
            watches: Mutex::default(),
            doc_hashes: Mutex::default(),
            conflicts: Mutex::default(),
            file_index: FileIndex::default(),
            tasks: Mutex::default(),
            stopping: AtomicBool::new(false),
            flood_turn: tokio::sync::Mutex::default(),
            git: Arc::new(GitCli),
            snapshots: Snapshots::default(),
            usage_warned: Mutex::default(),
            ports: Mutex::default(),
            terminal_titles: Mutex::default(),
        }
    }

    pub fn store(&self) -> MutexGuard<'_, Store> {
        // A panic mid-query leaves the connection usable (SQLite rolls back), so ignore poison.
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The ports `id` printed, if it is a live run session that printed some.
    pub fn session_ports(&self, id: Ulid) -> Option<Vec<u16>> {
        lock(&self.ports).get(&id).cloned()
    }

    /// The cleaned terminal title of agent session `id`, if its program set one.
    pub fn terminal_title(&self, id: Ulid) -> Option<String> {
        lock(&self.terminal_titles).get(&id).cloned()
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
            project_dir,
            socket,
            log_file,
            typed,
            banner,
            env,
        } = spec;
        let mut cmd = launch::command(&profile).map_err(SpawnError::Launch)?;
        cmd.cwd(&cwd);
        for (key, value) in &env {
            cmd.env(key, value);
        }
        cmd.env("MKREW_SESSION_ID", session.id.to_string());
        cmd.env(mkrew_core::paths::ENV_SOCKET, &socket);
        cmd.env("TERM", "xterm-256color");
        // A Mkrew PTY is never omp's own shell; an inherited OMPCODE disables the omp extension.
        cmd.env_remove("OMPCODE");
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
        let mut writer = pair.master.take_writer().context("take pty writer")?;
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
        // Writes block while the child does not read its input, so they get their own thread
        // too: a big paste then only waits in the connection that sent it.
        let (input, mut input_rx) = mpsc::channel::<Vec<u8>>(INPUT_CHANNEL_CHUNKS);
        let writer_id = session.id;
        thread::Builder::new()
            .name(format!("pty-write-{}", session.id))
            .spawn(move || {
                while let Some(bytes) = input_rx.blocking_recv() {
                    if let Err(e) = writer.write_all(&bytes).and_then(|()| writer.flush()) {
                        tracing::debug!(id = %writer_id, "pty write failed: {e}");
                        break;
                    }
                }
            })
            .context("start pty writer thread")
            .map_err(&mut kill_on_error)?;

        let (commands_tx, commands) = mpsc::channel(8);
        let shared_input = Arc::new(tokio::sync::Mutex::new(input.clone()));
        {
            // Checked under the `live` lock that `shutdown` drains, so no session slips past it.
            let mut live = lock(&self.live);
            if self.stopping.load(Ordering::SeqCst) {
                return Err(kill_on_error(anyhow::anyhow!("the daemon is stopping")));
            }
            {
                let store = self.store();
                // Checked again here: two clients may run the same task at once.
                if let Some(task_id) = session.task_id {
                    let sessions = store
                        .list_sessions(Some(session.project_id))
                        .context("list sessions")
                        .map_err(&mut kill_on_error)?;
                    let busy = sessions
                        .iter()
                        .any(|s| s.task_id == Some(task_id) && s.state != SessionState::Exited);
                    if busy {
                        // Only for the kill; the caller gets the precise error.
                        let _ = kill_on_error(anyhow::anyhow!("task already running"));
                        return Err(SpawnError::TaskRunning(task_id));
                    }
                }
                store
                    .insert_session(&session)
                    .context("insert session")
                    .map_err(&mut kill_on_error)?;
            }
            let channels = Live {
                project_id: session.project_id,
                commands: commands_tx.clone(),
                input: Arc::clone(&shared_input),
            };
            live.insert(session.id, channels);
            // Before the session task runs, so its own task updates come after this one.
            if let Some(task_id) = session.task_id {
                self.follow_task(task_id, SessionState::Starting, None);
            }
        }
        tracing::info!(id = %session.id, profile = %profile.id, cwd = %cwd.display(), "session spawned");
        self.watch_project(session.project_id, &project_dir, false);
        let (sends, sends_rx) = mpsc::unbounded_channel();
        tokio::spawn(write_sends(session.id, sends_rx, shared_input, commands_tx));
        let (turn_jobs, jobs) = mpsc::unbounded_channel();
        // In `tasks`, so shutdown waits for the end snapshot of turns cut off by the exit.
        self.spawn_task(turns::record(
            Arc::clone(self),
            session.id,
            session.project_id,
            cwd,
            jobs,
        ));
        let usage = self.track_usage(&session, &profile);
        let mut queue = SendQueue::default();
        if let Some(text) = typed {
            queue.push(Ulid::new(), text);
        }
        let run_ports = mkrew_core::run::command_name(&session)
            .is_some()
            .then(Vec::new);
        let mut actor = Actor {
            hub: std::sync::Arc::clone(self),
            session,
            master: Some(pair.master),
            input,
            sends,
            send_writing: false,
            killer,
            pid,
            emulator: None,
            machine,
            screen_print: None,
            queue,
            log,
            attachments: Vec::new(),
            turns: TurnTracker::default(),
            turn_jobs,
            hook_reply: None,
            usage,
            ports: run_ports,
        };
        if let Some(banner) = banner {
            actor.on_banner(&banner);
        }
        self.spawn_task(actor.run(rx, commands));
        Ok(())
    }

    /// Spawns `task` into `tasks`, which shutdown waits for. Finished tasks are reaped first:
    /// a `JoinSet` keeps each one's allocation until it is joined, which would otherwise grow
    /// with every session the daemon ever ran.
    fn spawn_task(&self, task: impl Future<Output = ()> + Send + 'static) {
        let mut tasks = lock(&self.tasks);
        while tasks.try_join_next().is_some() {}
        tasks.spawn(task);
    }

    /// Starts the usage task of an agent session whose profile has `[usage]` (SPEC §20E.3);
    /// an unknown format is logged once per profile.
    fn track_usage(
        self: &std::sync::Arc<Self>,
        session: &Session,
        profile: &AgentProfile,
    ) -> Option<mpsc::UnboundedSender<String>> {
        let config = profile.usage.as_ref()?;
        if session.kind != SessionKind::Agent {
            return None;
        }
        match usage::track(Arc::clone(self), session.id, config) {
            Ok((ends, task)) => {
                // In `tasks`, so shutdown waits for the read of the last turn's end.
                self.spawn_task(task);
                Some(ends)
            }
            Err(warning) => {
                if lock(&self.usage_warned).insert(profile.id.clone()) {
                    tracing::warn!(profile = %profile.id, "{warning}");
                }
                None
            }
        }
    }

    /// Moves task `task_id` after its session went to `state` (SPEC §5 rules) and announces
    /// the change. Read and write share one store lock, so a concurrent drag is not lost.
    /// Exits caused by daemon shutdown are not failures: the card keeps its status for the
    /// startup resume (SPEC §8).
    pub fn follow_task(&self, task_id: Ulid, state: SessionState, exit_code: Option<i32>) {
        if self.stopping.load(Ordering::SeqCst) {
            return;
        }
        let updated = {
            let store = self.store();
            store.get_task(task_id).and_then(|task| {
                match mkrew_core::task::on_session_state(task.status, state, exit_code) {
                    Some((status, error_flag)) => {
                        store.set_task_status(task_id, status, error_flag).map(Some)
                    }
                    None => Ok(None),
                }
            })
        };
        match updated {
            Ok(Some(task)) => {
                tracing::debug!(%task_id, status = ?task.status, "task follows its session");
                let _ = self.events.send(Event::TaskUpdated(task));
            }
            Ok(None) => {}
            Err(e) => tracing::warn!(%task_id, "task status not updated: {e}"),
        }
    }

    /// Keeps project `project_id` (folder `root`) watched for doc and git changes until the
    /// daemon stops, whether or not it has live sessions.
    pub fn keep_watched(self: &Arc<Self>, project_id: Ulid, root: &Path) {
        self.watch_project(project_id, root, true);
    }

    /// Keeps the worktree of task `task_id` (project `project_id`; `root` = the project's folder
    /// inside it) watched for the Files and Git views until the daemon stops: each batch
    /// broadcasts `git.changed` with `worktree_task_id` and invalidates `fs.find`'s list
    /// (keyed by the task id). Changes there are not attributed to sessions or turns.
    pub fn keep_worktree_watched(self: &Arc<Self>, project_id: Ulid, task_id: Ulid, root: &Path) {
        self.watch(task_id, project_id, Some(task_id), root, true);
    }

    /// Stops watching the worktree of task `task_id` and forgets its `fs.find` list: an open
    /// watcher handle can make `git worktree remove` fail on Windows.
    pub fn unwatch_worktree(&self, task_id: Ulid) {
        lock(&self.watches).remove(&task_id);
        self.file_index.invalidate(task_id);
    }

    /// Starts watching `root` for project `project_id` unless it is watched already; `kept`
    /// keeps the watch after the project's last session exits. Every batch also broadcasts
    /// `git.changed`: a reported file or the git state changed.
    fn watch_project(self: &Arc<Self>, project_id: Ulid, root: &Path, kept: bool) {
        self.watch(project_id, project_id, None, root, kept);
    }

    /// The watch of [`Hub::watch_project`] (`worktree_task_id` = `None`, `key` = the project)
    /// or [`Hub::keep_worktree_watched`] (`key` = the task).
    fn watch(
        self: &Arc<Self>,
        key: Ulid,
        project_id: Ulid,
        worktree_task_id: Option<Ulid>,
        root: &Path,
        kept: bool,
    ) {
        let mut watches = lock(&self.watches);
        if let Some(watch) = watches.get_mut(&key) {
            watch.kept |= kept;
            return;
        }
        let hub = Arc::downgrade(self);
        let started = watch::watch(root, move |batch| {
            if let Some(hub) = hub.upgrade() {
                if !batch.files.is_empty() {
                    hub.file_index.invalidate(key);
                    if worktree_task_id.is_none() {
                        hub.files_changed(project_id, &batch.files);
                    }
                }
                let _ = hub.events.send(Event::GitChanged(GitChangedEvent {
                    project_id,
                    worktree_task_id,
                }));
            }
        });
        match started {
            Ok(watch) => {
                tracing::debug!(%project_id, ?worktree_task_id, root = %root.display(), "watching files");
                let watch = ProjectWatch {
                    _watch: watch,
                    kept,
                };
                watches.insert(key, watch);
            }
            Err(e) => tracing::warn!(%project_id, root = %root.display(), "cannot watch: {e}"),
        }
    }

    /// Stops watching project `project_id` once none of its sessions is live, unless it is
    /// kept watched. Its conflict tracker goes either way.
    fn unwatch_idle_project(&self, project_id: Ulid) {
        let mut watches = lock(&self.watches);
        let live = lock(&self.live)
            .values()
            .any(|live| live.project_id == project_id);
        if live {
            return;
        }
        if watches.get(&project_id).is_some_and(|w| !w.kept) {
            watches.remove(&project_id);
            tracing::debug!(%project_id, "stopped watching project files");
        }
        lock(&self.conflicts).remove(&project_id);
    }

    /// Records a debounced batch of changed files on every live session of the project
    /// (session event `files_changed`, payload `{"paths": [...]}`), and attributes the changes
    /// for conflict detection to the project's session in `working`. With several sessions
    /// working the editor is unknown, so the batch is not attributed (HUMAN_CHECKS T5.6).
    fn files_changed(&self, project_id: Ulid, paths: &[String]) {
        self.docs_changed(project_id, paths);
        let sessions: Vec<Ulid> = lock(&self.live)
            .iter()
            .filter(|(_, live)| live.project_id == project_id)
            .map(|(id, _)| *id)
            .collect();
        // A batch flushed after the last session exited.
        if sessions.is_empty() {
            return;
        }
        let payload = serde_json::json!({ "paths": paths });
        let at = crate::server::now_ms();
        let working: Vec<Ulid> = {
            let store = self.store();
            for &id in &sessions {
                if let Err(e) = store.insert_session_event(id, "files_changed", &payload, at) {
                    tracing::warn!(%id, "files_changed not recorded: {e}");
                }
            }
            match store.list_sessions(Some(project_id)) {
                Ok(all) => all
                    .iter()
                    .filter(|s| s.state == SessionState::Working && sessions.contains(&s.id))
                    .map(|s| s.id)
                    .collect(),
                Err(e) => {
                    tracing::warn!(%project_id, "cannot attribute changed files: {e}");
                    Vec::new()
                }
            }
        };
        if let [editor] = working[..] {
            self.record_edits(project_id, paths, editor, at);
            // A batch flushed after the turn ended is dropped by the tracker (HUMAN_CHECKS T9.5).
            if !self.command(editor, Command::TurnFiles(paths.to_vec())) {
                tracing::debug!(%editor, "changed files not added to the turn");
            }
        }
    }

    /// [`Hub::doc_seen`] for each changed Markdown file. Files too large or not UTF-8 are
    /// skipped. The files are small, so they are read right here on the watcher's task.
    fn docs_changed(&self, project_id: Ulid, paths: &[String]) {
        let mut docs = paths.iter().filter(|p| docs::is_markdown(p)).peekable();
        if docs.peek().is_none() {
            return;
        }
        let root = match self.store().get_project(project_id) {
            Ok(project) => PathBuf::from(project.path),
            Err(e) => {
                tracing::warn!(%project_id, "changed docs not checked: {e}");
                return;
            }
        };
        for path in docs {
            let content = match docs::read(&root.join(path)) {
                Ok(content) => Some(content),
                Err(docs::ReadError::NotFound) => None,
                Err(e) => {
                    tracing::debug!(%project_id, %path, "changed doc not read: {e}");
                    continue;
                }
            };
            self.doc_seen(project_id, path, content.as_deref());
        }
    }

    /// Markdown file `path` of the project now holds `content` (`None` = deleted). When its
    /// hash differs from the last one seen (or it was never seen), announces `doc.changed` and
    /// re-anchors its comments (`comment.updated` for each one that moved or changed status).
    /// `doc.read` calls this too, so edits made while the daemon was down are caught up.
    pub fn doc_seen(&self, project_id: Ulid, path: &str, content: Option<&str>) {
        let hash = content.map(|c| docs::hash(c.as_bytes()));
        let key = (project_id, path.to_owned());
        if lock(&self.doc_hashes).insert(key, hash.clone()) == Some(hash.clone()) {
            return;
        }
        tracing::debug!(%project_id, %path, "doc changed");
        let _ = self.events.send(Event::DocChanged(DocChangedEvent {
            project_id,
            file_path: path.to_owned(),
            hash,
        }));
        self.reanchor_comments(project_id, path, content);
    }

    fn reanchor_comments(&self, project_id: Ulid, path: &str, content: Option<&str>) {
        let updated: Vec<DocComment> = {
            let store = self.store();
            let comments = match store.list_comments(project_id, Some(path)) {
                Ok(comments) => comments,
                Err(e) => {
                    tracing::warn!(%project_id, %path, "comments not re-anchored: {e}");
                    return;
                }
            };
            docs::reanchored(&comments, content)
                .into_iter()
                .filter_map(|(id, start, end, status)| {
                    store
                        .set_comment_anchor(id, start, end, status)
                        .inspect_err(|e| tracing::warn!(%id, "comment not re-anchored: {e}"))
                        .ok()
                })
                .collect()
        };
        for comment in updated {
            let _ = self.events.send(Event::CommentUpdated(Box::new(comment)));
        }
    }

    /// A hook of session `session_id` named files its agent edited (`hook::edited_paths`):
    /// attributes the ones inside the project and not ignored by the watcher, records them
    /// (session event `edited`, payload `{"paths": [...]}`, for `session.files`), announces
    /// each with `session.edited` and returns them relative to the project. The project path
    /// is the watch root too: sessions run with `cwd` = project path (SPEC §8).
    fn hook_edits(
        &self,
        project_id: Ulid,
        session_id: Ulid,
        reported: &[String],
        line: Option<u32>,
    ) -> Vec<String> {
        let root = match self.store().get_project(project_id) {
            Ok(project) => PathBuf::from(project.path),
            Err(e) => {
                tracing::warn!(%project_id, "hook edits not attributed: {e}");
                return Vec::new();
            }
        };
        let filter = watch::Filter::new(&root);
        let paths: Vec<String> = reported
            .iter()
            .filter_map(|path| {
                let path = root.join(path);
                let is_dir = path.metadata().ok().map(|m| m.is_dir());
                filter.relative(&path, is_dir)
            })
            .collect();
        if paths.is_empty() {
            return paths;
        }
        let at = crate::server::now_ms();
        let payload = serde_json::json!({ "paths": paths });
        if let Err(e) = self
            .store()
            .insert_session_event(session_id, "edited", &payload, at)
        {
            tracing::warn!(%session_id, "hook edits not recorded: {e}");
        }
        self.record_edits(project_id, &paths, session_id, at);
        for path in &paths {
            let _ = self.events.send(Event::SessionEdited(SessionEditedEvent {
                session_id,
                path: path.clone(),
                line,
            }));
        }
        paths
    }

    /// Records that `editor` changed `paths` at `at` and announces each new conflict.
    fn record_edits(&self, project_id: Ulid, paths: &[String], editor: Ulid, at: i64) {
        let found: Vec<(String, Vec<Ulid>)> = {
            let mut conflicts = lock(&self.conflicts);
            let tracker = conflicts.entry(project_id).or_default();
            tracker.prune(at);
            paths
                .iter()
                .filter_map(|path| Some((path.clone(), tracker.edit(path, &[editor], at)?)))
                .collect()
        };
        if found.is_empty() {
            return;
        }
        let sessions = match self.store().list_sessions(Some(project_id)) {
            Ok(sessions) => sessions,
            Err(e) => {
                tracing::warn!(%project_id, "conflict tasks unknown: {e}");
                Vec::new()
            }
        };
        for (path, session_ids) in found {
            let mut task_ids: Vec<Ulid> = sessions
                .iter()
                .filter(|s| session_ids.contains(&s.id))
                .filter_map(|s| s.task_id)
                .collect();
            task_ids.sort();
            task_ids.dedup();
            tracing::info!(%project_id, %path, sessions = ?session_ids, "conflict detected");
            let _ = self
                .events
                .send(Event::ConflictDetected(ConflictDetectedEvent {
                    project_id,
                    path,
                    session_ids,
                    task_ids,
                }));
        }
    }

    /// Sends `command` to a live session; `false` if it is not running. A full command queue
    /// (a busy session) does not lose it: it is sent from a task once there is room. Must run
    /// inside the tokio runtime.
    pub fn command(&self, id: Ulid, command: Command) -> bool {
        self.sender(id).is_some_and(|s| send_soon(s, command))
    }

    /// The command channel of a live session (waits for room instead of failing when full).
    pub fn sender(&self, id: Ulid) -> Option<mpsc::Sender<Command>> {
        lock(&self.live).get(&id).map(|live| live.commands.clone())
    }

    /// The input channel of a live session's PTY writer; lock it for a whole request and send
    /// chunks of at most [`INPUT_CHUNK_BYTES`].
    pub fn input(&self, id: Ulid) -> Option<Arc<tokio::sync::Mutex<mpsc::Sender<Vec<u8>>>>> {
        lock(&self.live)
            .get(&id)
            .map(|live| Arc::clone(&live.input))
    }

    /// Daemon shutdown (SPEC §8): asks every session to terminate and waits for all of them
    /// (each one kills its child after [`KILL_GRACE`]).
    pub async fn shutdown(&self) {
        let live: Vec<_> = {
            let mut live = lock(&self.live);
            self.stopping.store(true, Ordering::SeqCst);
            live.drain().collect()
        };
        for (_, live) in live {
            send_soon(live.commands, Command::Kill { force: false });
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

/// Sends `command` now, or from a task once the queue has room; `false` if the session task
/// is gone. Must run inside the tokio runtime.
fn send_soon(commands: mpsc::Sender<Command>, command: Command) -> bool {
    match commands.try_send(command) {
        Ok(()) => true,
        Err(mpsc::error::TrySendError::Full(command)) => {
            tokio::spawn(async move {
                let _ = commands.send(command).await;
            });
            true
        }
        Err(mpsc::error::TrySendError::Closed(_)) => false,
    }
}

/// One session task: the only owner of the session's emulator, state machine and log.
struct Actor {
    hub: std::sync::Arc<Hub>,
    session: Session,
    /// `None` once closed (graceful kill or exit).
    master: Option<Box<dyn MasterPty + Send>>,
    /// To the PTY writer thread (DSR replies).
    input: mpsc::Sender<Vec<u8>>,
    /// Delivered `session.send` texts, written in order by [`write_sends`].
    sends: mpsc::UnboundedSender<String>,
    /// A delivered text is not written yet: output until then is not the agent reacting.
    send_writing: bool,
    killer: Box<dyn ChildKiller + Send + Sync>,
    /// For `SIGKILL` (Unix only; Windows kills through `killer`).
    #[cfg_attr(not(unix), allow(dead_code))]
    pid: Option<u32>,
    /// Created lazily so a session that never prints costs no grid memory.
    emulator: Option<Emulator>,
    machine: StateMachine,
    /// [`Emulator::text_fingerprint`] after the last output; output that leaves it unchanged
    /// is not activity (ADR 0015).
    screen_print: Option<u64>,
    /// `session.send` texts waiting for the session to be ready.
    queue: SendQueue,
    log: SessionLog,
    /// Attached clients; closed ones are dropped on the next output.
    attachments: Vec<Arc<Attachment>>,
    /// Agent sessions only; each ended turn is broadcast as `session.turn` (SPEC §20A.3).
    turns: TurnTracker,
    /// To the session's [`turns::record`] task.
    turn_jobs: mpsc::UnboundedSender<turns::Job>,
    /// The reply of the hook being handled; a turn it starts keeps it until the snapshot.
    hook_reply: Option<oneshot::Sender<()>>,
    /// To the session's [`usage::track`] task: the native session id at each turn end.
    usage: Option<mpsc::UnboundedSender<String>>,
    /// `Some` for a run session: the ports it printed so far.
    ports: Option<Vec<u16>>,
}

impl Actor {
    async fn run(
        mut self,
        mut output: mpsc::Receiver<PtyEvent>,
        mut commands: mpsc::Receiver<Command>,
    ) {
        let mut commands_open = true;
        let mut kill_at: Option<tokio::time::Instant> = None;
        let hub = Arc::clone(&self.hub);
        let mut rate = OutputRate::default();
        // A flooding session's batch waiting for its turn (more output queues up meanwhile).
        let mut waiting: Option<Vec<u8>> = None;
        let exit_code = loop {
            let tick_at = self
                .machine
                .next_deadline()
                .map(tokio::time::Instant::from_std);
            tokio::select! {
                event = output.recv(), if waiting.is_none() => match event {
                    Some(PtyEvent::Output(mut bytes)) => {
                        let exited = coalesce(&mut bytes, &mut output);
                        let flooding = rate.add(Instant::now(), bytes.len());
                        if flooding && exited.is_none() {
                            waiting = Some(bytes);
                            continue;
                        }
                        self.on_output(&bytes);
                        if let Some(code) = exited {
                            drain_after_exit(&mut self, &mut output).await;
                            break code;
                        }
                    }
                    Some(PtyEvent::Exited(code)) => {
                        drain_after_exit(&mut self, &mut output).await;
                        break code;
                    }
                    None => break None,
                },
                // Rebuilt each pass, so the turn is only roughly FIFO; the tick branch below may
                // see the screen before a waiting batch (waits are tens of ms, `idle_after` s).
                mut pacer = take_turn(&hub.flood_turn), if waiting.is_some() => {
                    let mut bytes = waiting.take().unwrap_or_default();
                    let queued = bytes.len();
                    let exited = coalesce(&mut bytes, &mut output);
                    let started = Instant::now();
                    rate.add(started, bytes.len() - queued);
                    self.on_output(&bytes);
                    pacer.after_batch(started, Instant::now());
                    drop(pacer);
                    if let Some(code) = exited {
                        drain_after_exit(&mut self, &mut output).await;
                        break code;
                    }
                }
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
                    Some(Command::Read { lines, reply }) => {
                        // The caller may have gone away; nothing to do then.
                        let _ = reply.send(SessionReadResult {
                            lines: self.emulator().last_lines(lines),
                            state: self.session.state,
                            state_changed_at: self.session.state_changed_at,
                        });
                    }
                    Some(Command::Resnapshot(attachment)) => {
                        attachment.resnapshot(self.emulator().snapshot());
                    }
                    Some(Command::Resize { cols, rows }) => self.resize(cols, rows),
                    Some(Command::Send { id, text, reply }) => {
                        self.queue.push(id, text);
                        let delivered = self.deliver();
                        let queued = self.queue.contains(id);
                        // A delivery already announced the queue; else the push changed it.
                        if !delivered && queued {
                            self.announce_queue();
                        }
                        if let Some(reply) = reply {
                            // The caller may have gone away; the text stays queued.
                            let _ = reply.send(queued);
                        }
                    }
                    Some(Command::Unqueue { id, reply }) => {
                        let removed = self.queue.remove(id);
                        if removed {
                            self.announce_queue();
                        }
                        let _ = reply.send(removed);
                    }
                    Some(Command::SendWritten) => self.send_writing = false,
                    Some(Command::Hook { agent, report, reply }) => {
                        self.hook_reply = Some(reply);
                        self.on_hook(agent, report);
                        self.hook_reply = None;
                    }
                    Some(Command::TurnFiles(paths)) => self.turns.add_files(paths),
                    Some(Command::Rename { title, reply }) => {
                        self.session.title = title;
                        self.save();
                        let _ = self.hub.events.send(Event::SessionUpdated(self.session.clone()));
                        // The caller may have gone away; the rename stands.
                        let _ = reply.send(self.session.clone());
                    }
                    None => commands_open = false,
                },
                () = sleep_until(tick_at), if tick_at.is_some() => {
                    let lines = self.screen_lines();
                    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
                    if let Some(state) = self.machine.tick(Instant::now(), &lines) {
                        self.set_state(state, StateSource::Screen);
                    }
                    self.deliver();
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

    /// Shows mkrew's own `line` before any program output: logged and on the screen, but not
    /// the program's, so the state machine and the send queue do not see it.
    fn on_banner(&mut self, line: &str) {
        let bytes = format!("{line}\r\n").into_bytes();
        if let Err(e) = self.log.write(&bytes) {
            tracing::warn!(id = %self.session.id, "session log write failed: {e}");
        }
        self.emulator().advance(&bytes);
    }

    fn on_output(&mut self, bytes: &[u8]) {
        if let Err(e) = self.log.write(bytes) {
            tracing::warn!(id = %self.session.id, "session log write failed: {e}");
        }
        let reply = self.emulator().advance(bytes);
        // Never wait for the writer here: a full input queue means the child is not reading
        // its input anyway, so dropping this answer is the lesser evil. (It can land between
        // two chunks of a paste; DSR during a paste is rare enough to accept that.)
        if !reply.is_empty() && self.input.try_send(reply).is_err() {
            tracing::debug!(id = %self.session.id, "input queue full or closed; DSR reply dropped");
        }
        let title = self.emulator().take_title();
        if let Some(title) = title {
            self.on_terminal_title(title.as_deref());
        }
        self.attachments.retain(|a| !a.is_closed());
        for attachment in &self.attachments {
            attachment.push(bytes);
        }
        // Only visible changes are work: idle TUIs keep sending cursor/title/progress bytes,
        // which would otherwise hold the session in `working` forever (ADR 0015). The first
        // batch always counts, so hook-only profiles still leave `starting`.
        let print = self.emulator().text_fingerprint();
        if self.screen_print.replace(print) != Some(print) {
            let lines = self.screen_lines();
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            self.scan_ports(&lines);
            if let Some(state) = self.machine.on_output(Instant::now(), &lines) {
                self.set_state(state, StateSource::Screen);
            }
        }
        if !self.send_writing {
            self.queue.on_output();
        }
        self.deliver();
    }

    /// Records the report as a `hook` session event, takes the agent's session id and lets
    /// the state machine decide (it ignores hooks for screen-only profiles).
    fn on_hook(&mut self, agent: HookAgent, report: HookReportParams) {
        let at = crate::server::now_ms();
        let native = agent.native_session_id(&report.payload);
        let event = agent.event(&report.event, &report.payload);
        tracing::debug!(id = %self.session.id, agent = %report.agent, event = %report.event, "hook");
        let record = serde_json::json!({
            "agent": report.agent,
            "event": report.event,
            "payload": report.payload,
        });
        if let Err(e) = self
            .hub
            .store()
            .insert_session_event(self.session.id, "hook", &record, at)
        {
            tracing::error!(id = %self.session.id, "saving hook event failed: {e}");
        }
        let edited = mkrew_core::hook::edited_paths(&report.payload);
        if !edited.is_empty() {
            let line = mkrew_core::hook::edited_line(&report.payload);
            let paths =
                self.hub
                    .hook_edits(self.session.project_id, self.session.id, &edited, line);
            self.turns.add_files(paths);
        }
        let native_changed = native.is_some() && native != self.session.native_session_id;
        if native_changed {
            self.session.native_session_id = native;
        }
        match event.and_then(|e| self.machine.on_hook(Instant::now(), e)) {
            Some(state) => self.set_state(state, StateSource::Hook),
            None if native_changed => self.save(),
            None => {}
        }
        self.deliver();
    }

    /// Hands the next queued `session.send` text to [`write_sends`] if the state allows it;
    /// the session task never waits for the PTY writer. `true` when a text left the queue
    /// (then `session.queue` was announced).
    fn deliver(&mut self) -> bool {
        let Some(text) = self.queue.pop(self.session.state) else {
            return false;
        };
        tracing::debug!(id = %self.session.id, pending = self.queue.len(), "delivering a queued send");
        let text = pasted(text, self.emulator().bracketed_paste());
        if self.sends.send(text).is_ok() {
            self.send_writing = true;
        }
        self.announce_queue();
        true
    }

    /// Broadcasts the whole send queue as `session.queue` (SPEC §20B.2).
    fn announce_queue(&self) {
        let items = self
            .queue
            .items()
            .map(|(send_id, text)| QueuedSend {
                send_id,
                // Shown as sent: the submit Enter is not part of the text.
                text: text.strip_suffix('\r').unwrap_or(text).to_owned(),
            })
            .collect();
        let _ = self.hub.events.send(Event::SessionQueue(SessionQueueEvent {
            session_id: self.session.id,
            items,
        }));
    }

    fn emulator(&mut self) -> &mut Emulator {
        let (cols, rows) = (self.session.cols, self.session.rows);
        self.emulator
            .get_or_insert_with(|| Emulator::new(cols, rows))
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

    fn set_state(&mut self, state: SessionState, source: StateSource) {
        tracing::debug!(id = %self.session.id, ?state, ?source, "session state");
        self.session.state = state;
        self.session.state_source = source;
        self.session.state_changed_at = crate::server::now_ms();
        self.save();
        let _ = self.hub.events.send(Event::SessionState(SessionStateEvent {
            session_id: self.session.id,
            state,
            source,
            changed_at: self.session.state_changed_at,
        }));
        if let Some(task_id) = self.session.task_id {
            self.hub.follow_task(task_id, state, self.session.exit_code);
        }
        self.track_turn(state);
    }

    /// Feeds the turn tracker and hands starts and ends to the turn recorder, which snapshots,
    /// persists and broadcasts `session.turn` when a turn ends.
    fn track_turn(&mut self, state: SessionState) {
        if self.session.kind != SessionKind::Agent {
            return;
        }
        let at = self.session.state_changed_at;
        let job = match self.turns.on_state(state, at) {
            Some(TurnEvent::Started(n)) => turns::Job::Start {
                n,
                at,
                reply: self.hook_reply.take(),
            },
            Some(TurnEvent::Ended(turn)) => {
                if let (Some(usage), Some(native)) = (&self.usage, &self.session.native_session_id)
                {
                    let _ = usage.send(native.clone());
                }
                turns::Job::End {
                    turn,
                    snapshot: state != SessionState::NeedsInput,
                }
            }
            Some(TurnEvent::Resumed(_)) | None => return,
        };
        // The recorder lives until this sender is dropped.
        let _ = self.turn_jobs.send(job);
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

    /// Adds the ports a run session's screen shows (SPEC §10); announces a grown list.
    fn scan_ports(&mut self, lines: &[&str]) {
        let Some(ports) = self.ports.as_mut() else {
            return;
        };
        let before = ports.len();
        for port in lines
            .iter()
            .flat_map(|line| mkrew_core::run::scan_ports(line))
        {
            if ports.len() < mkrew_core::run::MAX_PORTS && !ports.contains(&port) {
                ports.push(port);
            }
        }
        if ports.len() == before {
            return;
        }
        let ports = ports.clone();
        lock(&self.hub.ports).insert(self.session.id, ports.clone());
        let _ = self.hub.events.send(Event::SessionPorts(SessionPortsEvent {
            session_id: self.session.id,
            ports,
        }));
    }

    /// Keeps and announces an agent's new terminal title (ADR 0031). Shell titles (paths,
    /// running commands) are ignored; so are unusable titles (a path, only glyphs), which
    /// keep the current name. A reset or empty title clears it. Spinner frames clean to the
    /// same name: announced once.
    fn on_terminal_title(&mut self, raw: Option<&str>) {
        if self.session.kind != SessionKind::Agent {
            return;
        }
        let title = match raw.map(str::trim) {
            None | Some("") => None,
            Some(raw) => match crate::emulator::clean_title(raw) {
                Some(title) => Some(title),
                None => return,
            },
        };
        if title == self.session.terminal_title {
            return;
        }
        self.session.terminal_title.clone_from(&title);
        {
            let mut titles = lock(&self.hub.terminal_titles);
            match &title {
                Some(title) => titles.insert(self.session.id, title.clone()),
                None => titles.remove(&self.session.id),
            };
        }
        let _ = self
            .hub
            .events
            .send(Event::SessionTerminalTitle(SessionTerminalTitleEvent {
                session_id: self.session.id,
                title,
            }));
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
        lock(&self.hub.ports).remove(&self.session.id);
        self.close_master();
        lock(&self.hub.live).remove(&self.session.id);
        self.hub.unwatch_idle_project(self.session.project_id);
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
        // Waiting texts are never written now: announce them gone.
        if !self.queue.is_empty() {
            self.queue = SendQueue::default();
            self.announce_queue();
        }
        // Cut off by the daemon stop, not ended by the user or the agent: resume it at the
        // next start (SPEC §8). A reboot leaves the row live instead; startup marks those.
        let stopping = self.hub.stopping.load(Ordering::SeqCst);
        if stopping && let Err(e) = self.hub.store().mark_interrupted(self.session.id) {
            tracing::warn!(id = %self.session.id, "not marked interrupted: {e}");
        }
        self.session.exit_code = exit_code;
        self.session.ended_at = Some(crate::server::now_ms());
        self.set_state(SessionState::Exited, StateSource::Daemon);
        let _ = self
            .hub
            .events
            .send(Event::SessionExited(SessionExitedEvent {
                session_id: self.session.id,
                exit_code,
            }));
        // A session that exited while the daemon runs has nothing to show: its pane leaves
        // the tab (ADR 0033; its output stays in History). After `Exited` is stored, so
        // `show_run_tab`, which lists live sessions only, cannot write a run pane back. Never
        // while the daemon stops: the pane must stay for the resume at the next start
        // (SPEC §8, ADR 0017).
        if !stopping {
            self.hub
                .leave_tabs(self.session.project_id, self.session.id);
        }
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

/// A multi-line send typed as is would submit at its first newline. When the program
/// enabled bracketed paste, wrap the text (without its submitting `\r`) as one paste; an end
/// marker inside the text is dropped so it cannot end the paste early.
fn pasted(text: String, bracketed: bool) -> String {
    let (body, submit) = match text.strip_suffix('\r') {
        Some(body) => (body, "\r"),
        None => (text.as_str(), ""),
    };
    if !bracketed || !body.contains('\n') {
        return text;
    }
    let body = body.replace("\x1b[201~", "");
    format!("\x1b[200~{body}\x1b[201~{submit}")
}

/// Writes delivered `session.send` texts in order through the shared input lock, so a send
/// never interleaves with another client's `session.input`. Ends with the session task.
async fn write_sends(
    id: Ulid,
    mut sends: mpsc::UnboundedReceiver<String>,
    input: Arc<tokio::sync::Mutex<mpsc::Sender<Vec<u8>>>>,
    commands: mpsc::Sender<Command>,
) {
    while let Some(text) = sends.recv().await {
        let input = input.lock().await;
        for chunk in text.as_bytes().chunks(INPUT_CHUNK_BYTES) {
            if input.send(chunk.to_vec()).await.is_err() {
                tracing::debug!(%id, "session ended before a send was written");
                return;
            }
        }
        drop(input);
        if commands.send(Command::SendWritten).await.is_err() {
            return;
        }
    }
}

async fn sleep_until(at: Option<tokio::time::Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_line_sends_are_pasted_when_the_program_asks_for_it() {
        assert_eq!(pasted("a\nb\r".into(), true), "\x1b[200~a\nb\x1b[201~\r");
        assert_eq!(pasted("a\nb".into(), true), "\x1b[200~a\nb\x1b[201~");
        assert_eq!(
            pasted("a\n\x1b[201~b\r".into(), true),
            "\x1b[200~a\nb\x1b[201~\r"
        );
        assert_eq!(pasted("a\nb\r".into(), false), "a\nb\r");
        assert_eq!(pasted("yes\r".into(), true), "yes\r");
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn back_to_back_batches_pause_half_their_time() {
        let mut pacer = Pacer::default();
        let t0 = Instant::now();
        pacer.after_batch(t0, t0 + ms(10));
        pacer.after_batch(t0 + ms(10), t0 + ms(20));
        assert_eq!(pacer.resume_at, None);
        pacer.after_batch(t0 + ms(20), t0 + ms(32));
        assert_eq!(pacer.resume_at, Some(t0 + ms(48)));
        // The pause itself does not pay off the next batch.
        pacer.after_batch(t0 + ms(48), t0 + ms(58));
        assert_eq!(pacer.debt, ms(5));
        assert_eq!(pacer.resume_at, Some(t0 + ms(48)));
    }

    #[test]
    fn idle_time_pays_off_the_debt() {
        let mut pacer = Pacer::default();
        let t0 = Instant::now();
        pacer.after_batch(t0, t0 + ms(30));
        assert_eq!(pacer.debt, ms(15));
        pacer.after_batch(t0 + ms(40), t0 + ms(44));
        assert_eq!(pacer.debt, ms(7));
        // Sporadic flood batches never pause.
        for i in 1..100 {
            let start = t0 + ms(44 + i * 10);
            pacer.after_batch(start, start + ms(2));
        }
        assert_eq!(pacer.resume_at, None);
    }

    #[tokio::test]
    async fn the_turn_waits_for_the_pause_and_for_the_holder() {
        let turn = tokio::sync::Mutex::new(Pacer::default());
        let resume_at = Instant::now() + ms(40);
        turn.lock().await.resume_at = Some(resume_at);
        let first = take_turn(&turn).await;
        assert!(Instant::now() >= resume_at);
        let mut second = Box::pin(take_turn(&turn));
        assert!(futures_poll_once(&mut second).await.is_none());
        drop(first);
        drop(second.await);
    }

    /// Polls `future` once.
    async fn futures_poll_once<F: std::future::Future + Unpin>(
        future: &mut F,
    ) -> Option<F::Output> {
        tokio::select! {
            biased;
            output = future => Some(output),
            () = std::future::ready(()) => None,
        }
    }

    #[test]
    fn a_session_floods_past_the_bytes_of_one_window() {
        let mut rate = OutputRate::default();
        let t0 = Instant::now();
        assert!(!rate.add(t0, FLOOD_BYTES / 2));
        assert!(!rate.add(t0 + ms(50), FLOOD_BYTES / 2));
        assert!(rate.add(t0 + ms(99), 1));
        // A new window starts from zero.
        assert!(!rate.add(t0 + ms(100), FLOOD_BYTES));
        assert!(rate.add(t0 + ms(150), 1));
        // Typing never floods.
        for i in 1..1000 {
            assert!(!rate.add(t0 + ms(200 + i), 64));
        }
    }
}
