//! Accept loop and per-connection request handling (SPEC §7, §8).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use interprocess::local_socket::tokio::{Listener, Stream, prelude::*};
use mkrew_core::{HookAgent, Paths, PromptMode};
use mkrew_protocol::{
    AgentProfileInfo, AgentProfilesResult, CommentStatus, DaemonStatusResult, ErrorCode, Event,
    Frame, FrameDecoder, HelloParams, HelloResult, HookReportParams, Message, Outcome,
    PROTOCOL_VERSION, ProfileErrorInfo, Project, ProjectAddParams, ProjectUpdateParams, Request,
    Session, SessionHistory, SessionKillParams, SessionKind, SessionSpawnParams,
    SessionSpawnResult, SessionState, SessionWaitParams, SessionWaitResult, StateSource, Task,
    TaskCreateParams, TaskDeletedEvent, TaskRunParams, TaskUndoParams, TaskUpdateParams,
    TerminalTab, encode,
};
use mkrew_store::{Store, StoreError};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use ulid::Ulid;

use crate::attach::{self, Attachment};
use crate::git::{GitBackend, GitCli};
use crate::launch::LaunchError;
use crate::notify::{self, Notifier};
use crate::session::{Command, Hub, INPUT_CHUNK_BYTES, SpawnError, SpawnSpec};
use crate::{backup, session_log, undo};

mod comment;
mod doc;
mod resume;

const DAEMON_VERSION: &str = env!("CARGO_PKG_VERSION");

/// State shared by all connections.
pub struct Daemon {
    hub: Arc<Hub>,
    paths: Paths,
    git: Arc<dyn GitBackend>,
    started_at: i64,
    shutdown: watch::Sender<bool>,
}

/// Where and to what `task.undo` resets (ADR 0005).
struct UndoTarget {
    dir: std::path::PathBuf,
    checkpoint: String,
    /// Unix ms the checkpoint was taken.
    since: i64,
    /// A session of the task has not exited.
    live: bool,
}

/// What the connection does after replying.
#[derive(Debug, PartialEq)]
enum After {
    Continue,
    Close,
    Shutdown,
}

impl Daemon {
    pub fn new(store: Store, paths: Paths) -> Arc<Self> {
        Arc::new(Self {
            hub: Arc::new(Hub::new(store)),
            paths,
            git: Arc::new(GitCli),
            started_at: now_ms(),
            shutdown: watch::channel(false).0,
        })
    }

    /// Starts daily DB backups, resumes interrupted tasks, then serves `listener` until
    /// `daemon.stop` or Ctrl+C, then stops every session (SPEC §8). `notifier` = `None` turns
    /// OS notifications off.
    pub async fn serve(self: Arc<Self>, listener: Listener, notifier: Option<Arc<dyn Notifier>>) {
        let notifications = notifier.map(|n| notify::start(Arc::clone(&self.hub), n));
        let backups = backup::start(Arc::clone(&self.hub), self.paths.backups_dir());
        self.resume_tasks();
        Arc::clone(&self).accept(listener).await;
        backups.abort();
        if let Some(task) = notifications {
            task.abort();
        }
        self.hub.shutdown().await;
    }

    async fn accept(self: Arc<Self>, listener: Listener) {
        let mut shutdown = self.shutdown.subscribe();
        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);
        loop {
            tokio::select! {
                accepted = listener.accept() => match accepted {
                    Ok(stream) => {
                        let daemon = Arc::clone(&self);
                        tokio::spawn(async move {
                            if let Err(e) = daemon.connection(stream).await {
                                tracing::debug!("connection closed: {e}");
                            }
                        });
                    }
                    Err(e) => tracing::warn!("accept failed: {e}"),
                },
                _ = shutdown.changed() => {
                    tracing::info!("daemon.stop received, shutting down");
                    return;
                }
                _ = &mut ctrl_c => {
                    tracing::info!("Ctrl+C received, shutting down");
                    return;
                }
            }
        }
    }

    /// Requests are answered in order; the first one must be `hello`. Once greeted, the
    /// client also receives every event (`session.state`, `session.exited`, …).
    ///
    /// The read loop is never cancelled (racing a pipe read against events in `select!`
    /// loses requests on Windows); replies and events reach the socket through one writer task.
    async fn connection(&self, stream: Stream) -> anyhow::Result<()> {
        let (mut rx, mut tx) = stream.split();
        let (out, mut outbox) = mpsc::channel::<Outgoing>(OUTBOX_FRAMES);
        let writer = tokio::spawn(async move {
            while let Some(item) = outbox.recv().await {
                tx.write_all(&item.bytes).await?;
                drop(item.written);
            }
            tx.flush().await
        });
        let mut decoder = FrameDecoder::new();
        let mut buf = vec![0u8; 64 * 1024];
        let mut conn = Conn::default();
        let mut forwarder: Option<AbortOnDrop> = None;
        loop {
            while let Some(frame) = decoder.next_frame()? {
                let Frame::Message(Message::Request { id, request }) = frame else {
                    anyhow::bail!("client sent a non-request frame");
                };
                let (outcome, after) = self.handle(&mut conn, request).await;
                if conn.greeted && forwarder.is_none() {
                    // Subscribed before the hello reply goes out: no event is missed.
                    let events = self.hub.events.subscribe();
                    forwarder = Some(AbortOnDrop(tokio::spawn(forward(events, out.clone()))));
                }
                let reply = Frame::Message(Message::Response { id, outcome });
                let new_attachment = conn.new_attachment.take();
                let sent = async { anyhow::Ok(out.send(encode(&reply)?.into()).await?) }.await;
                if let Err(e) = sent {
                    // The session already holds it; closing stops it from queueing output.
                    if let Some(new) = new_attachment {
                        new.attachment.close();
                    }
                    return Err(e);
                }
                if let Some(new) = new_attachment {
                    // Started after the attach reply is queued, so output frames follow it.
                    let flusher = attach::flush(
                        new.session_id,
                        Arc::clone(&new.attachment),
                        new.session,
                        out.clone(),
                    );
                    let attached = Attached {
                        attachment: new.attachment,
                        _flusher: AbortOnDrop(tokio::spawn(flusher)),
                    };
                    conn.attachments.insert(new.session_id, attached);
                }
                if after == After::Continue {
                    continue;
                }
                drop((forwarder, out, conn));
                writer.await??;
                if after == After::Shutdown {
                    self.shutdown.send_replace(true);
                }
                return Ok(());
            }
            let n = rx.read(&mut buf).await?;
            if n == 0 {
                return Ok(());
            }
            decoder.push(&buf[..n]);
        }
    }

    async fn handle(&self, conn: &mut Conn, request: Request) -> (Outcome, After) {
        let request = match request {
            Request::Hello(params) => return self.hello(&mut conn.greeted, params),
            _ if !conn.greeted => {
                let outcome = Outcome::error(ErrorCode::HelloRequired, "send hello first");
                return (outcome, After::Close);
            }
            request => request,
        };
        match request {
            Request::DaemonStatus => (self.status(), After::Continue),
            Request::DaemonStop => (ok(&()), After::Shutdown),
            Request::ProjectAdd(params) => (self.project_add(params), After::Continue),
            Request::ProjectList => {
                let projects = self.store().list_projects();
                (store_outcome(projects), After::Continue)
            }
            Request::ProjectUpdate(params) => (self.project_update(params), After::Continue),
            Request::TabList(params) => {
                let tabs = self.store().list_tabs(params.project_id);
                (store_outcome(tabs), After::Continue)
            }
            Request::TabCreate(params) => {
                let tab =
                    self.store()
                        .create_tab(params.project_id, &params.title, params.position);
                (self.tab_updated(tab), After::Continue)
            }
            Request::TabRename(params) => {
                let tab = self.store().rename_tab(params.tab_id, &params.title);
                (self.tab_updated(tab), After::Continue)
            }
            Request::TabMove(params) => {
                let tab = self.store().move_tab(params.tab_id, params.position);
                (self.tab_updated(tab), After::Continue)
            }
            Request::TabSetLayout(params) => {
                let tab = self.store().set_tab_layout(params.tab_id, &params.layout);
                // SPEC §7: `ok`, the new tab goes out as `tab.updated`.
                let outcome = match self.tab_updated(tab) {
                    Outcome::Result(_) => ok(&()),
                    error => error,
                };
                (outcome, After::Continue)
            }
            Request::TabClose(params) => {
                // The tab's sessions keep running; the UI decides about them (SPEC §10).
                let closed = self.store().close_tab(params.tab_id);
                (store_outcome(closed), After::Continue)
            }
            Request::TaskList(params) => {
                let tasks = self.store().list_tasks(params.project_id, params.status);
                (store_outcome(tasks), After::Continue)
            }
            Request::TaskCreate(params) => (self.task_create(params), After::Continue),
            Request::TaskUpdate(params) => (self.task_update(params), After::Continue),
            Request::TaskMove(params) => {
                let task = self
                    .store()
                    .move_task(params.task_id, params.status, params.position);
                (self.task_updated(task), After::Continue)
            }
            Request::TaskDelete(params) => (self.task_delete(params.task_id), After::Continue),
            Request::TaskRun(params) => (self.task_run(params).await, After::Continue),
            Request::TaskUndoPreview(params) => (
                self.task_undo_preview(params.task_id).await,
                After::Continue,
            ),
            Request::TaskUndo(params) => (self.task_undo(params).await, After::Continue),
            Request::SessionSpawn(params) => (self.session_spawn(params), After::Continue),
            Request::SessionList(params) => {
                let sessions = self.store().list_sessions(params.project_id);
                (store_outcome(sessions), After::Continue)
            }
            Request::SessionKill(params) => (self.session_kill(params), After::Continue),
            Request::SessionHistory(params) => {
                let session = self.store().get_session(params.session_id);
                let history = session.map(|s| SessionHistory {
                    files: session_log::files(&session_log::path(&self.paths.sessions_dir(), s.id)),
                });
                (store_outcome(history), After::Continue)
            }
            Request::SessionAttach(params) => {
                let outcome = self.session_attach(conn, params.session_id).await;
                (outcome, After::Continue)
            }
            Request::SessionDetach(params) => {
                // Idempotent: detaching a session that is not attached is fine.
                conn.attachments.remove(&params.session_id);
                (ok(&()), After::Continue)
            }
            Request::SessionInput(params) => {
                let outcome = self.session_input(params.session_id, &params.data).await;
                (outcome, After::Continue)
            }
            Request::SessionResize(params) if params.cols == 0 || params.rows == 0 => (
                Outcome::error(ErrorCode::InvalidRequest, "cols and rows must be > 0"),
                After::Continue,
            ),
            Request::SessionResize(params) => {
                let command = Command::Resize {
                    cols: params.cols,
                    rows: params.rows,
                };
                let outcome = self.session_command(params.session_id, command).await;
                (outcome, After::Continue)
            }
            Request::SessionSend(params) => {
                let mut text = params.text;
                if params.submit {
                    text.push('\r');
                }
                let outcome = self
                    .session_command(params.session_id, Command::Send(text))
                    .await;
                (outcome, After::Continue)
            }
            Request::SessionWait(params) => (self.session_wait(params).await, After::Continue),
            Request::HookReport(params) => (self.hook_report(params).await, After::Continue),
            Request::AgentProfiles => (ok(&self.agent_profiles()), After::Continue),
            Request::DocList(params) => (self.doc_list(params).await, After::Continue),
            Request::DocRead(params) => (self.doc_read(params).await, After::Continue),
            Request::CommentList(params) => (self.comment_list(params), After::Continue),
            Request::CommentCreate(params) => (self.comment_create(params), After::Continue),
            Request::CommentReply(params) => (self.comment_reply(params), After::Continue),
            Request::CommentResolve(params) => (
                self.comment_set_status(params.comment_id, CommentStatus::Resolved),
                After::Continue,
            ),
            Request::CommentReopen(params) => (
                self.comment_set_status(params.comment_id, CommentStatus::Open),
                After::Continue,
            ),
            other => {
                let message = format!("{} is not implemented yet", method_name(&other));
                (
                    Outcome::error(ErrorCode::InvalidRequest, message),
                    After::Continue,
                )
            }
        }
    }

    fn hello(&self, greeted: &mut bool, params: HelloParams) -> (Outcome, After) {
        if params.protocol_version != PROTOCOL_VERSION {
            let message = format!(
                "client speaks protocol {}, daemon speaks {PROTOCOL_VERSION}; restart the daemon",
                params.protocol_version
            );
            return (
                Outcome::error(ErrorCode::VersionMismatch, message),
                After::Close,
            );
        }
        *greeted = true;
        tracing::debug!(client = ?params.client, "hello");
        let result = HelloResult {
            daemon_version: DAEMON_VERSION.into(),
            protocol_version: PROTOCOL_VERSION,
        };
        (ok(&result), After::Continue)
    }

    fn status(&self) -> Outcome {
        ok(&DaemonStatusResult {
            daemon_version: DAEMON_VERSION.into(),
            protocol_version: PROTOCOL_VERSION,
            pid: std::process::id(),
            started_at: self.started_at,
            sessions: u32::try_from(self.hub.live_count()).unwrap_or(u32::MAX),
        })
    }

    /// Profiles are re-read on every spawn, so edits apply without a daemon restart.
    fn session_spawn(&self, params: SessionSpawnParams) -> Outcome {
        if params.cols == 0 || params.rows == 0 {
            return Outcome::error(ErrorCode::InvalidRequest, "cols and rows must be > 0");
        }
        let project = match self.store().get_project(params.project_id) {
            Ok(project) => project,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        if let Some(tab_id) = params.tab_id {
            match self.store().get_tab(tab_id) {
                Ok(tab) if tab.project_id == project.id => {}
                Ok(_) => {
                    let message = format!("tab {tab_id} belongs to another project");
                    return Outcome::error(ErrorCode::InvalidRequest, message);
                }
                Err(e) => return store_outcome::<()>(Err(e)),
            }
        }
        let profile_id = match (params.agent_profile, params.kind) {
            (Some(id), _) => id,
            (None, SessionKind::Shell) => "shell".to_owned(),
            (None, SessionKind::Agent) => match project.default_agent.clone() {
                Some(id) => id,
                None => {
                    let message = format!(
                        "project {} has no default agent; name an agent profile",
                        project.name
                    );
                    return Outcome::error(ErrorCode::InvalidRequest, message);
                }
            },
        };
        let profile = match self.load_profile(&profile_id) {
            Ok(profile) => profile,
            Err(outcome) => return outcome,
        };
        let start = Start {
            project,
            profile,
            task_id: None,
            title: params.title,
            cols: params.cols,
            rows: params.rows,
            typed: None,
            native_session_id: None,
        };
        match self.start_session(start) {
            Ok(session_id) => {
                if let Some(tab_id) = params.tab_id {
                    self.place_in_empty_tab(tab_id, session_id);
                }
                ok(&SessionSpawnResult { session_id })
            }
            Err(outcome) => outcome,
        }
    }

    /// Runs the task (SPEC §5, §6): its agent, else the project's default, in the project
    /// folder, with the prompt per the profile's `prompt_mode`. Refused while the task already
    /// has a live session. In a git repo, HEAD before the run becomes `task.checkpoint`.
    async fn task_run(&self, params: TaskRunParams) -> Outcome {
        if params.cols == 0 || params.rows == 0 {
            return Outcome::error(ErrorCode::InvalidRequest, "cols and rows must be > 0");
        }
        let found = {
            let store = self.store();
            store.get_task(params.task_id).and_then(|task| {
                let project = store.get_project(task.project_id)?;
                let sessions = store.list_sessions(Some(project.id))?;
                let live = sessions
                    .iter()
                    .any(|s| s.task_id == Some(task.id) && s.state != SessionState::Exited);
                Ok((task, project, live))
            })
        };
        let (task, project, live) = match found {
            Ok(found) => found,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        if live {
            let message = format!("task {} is already running", task.id);
            return Outcome::error(ErrorCode::InvalidState, message);
        }
        let Some(profile_id) = task.agent_profile.or(project.default_agent.clone()) else {
            let message = format!(
                "task has no agent and project {} has no default agent",
                project.name
            );
            return Outcome::error(ErrorCode::InvalidRequest, message);
        };
        let mut profile = match self.load_profile(&profile_id) {
            Ok(profile) if profile.kind == mkrew_core::ProfileKind::Agent => profile,
            Ok(_) => {
                let message = format!("{profile_id} is a shell profile, not an agent");
                return Outcome::error(ErrorCode::InvalidRequest, message);
            }
            Err(outcome) => return outcome,
        };
        // An empty prompt starts the agent without one, whatever the mode.
        let mut typed = None;
        if !task.prompt.is_empty() {
            match profile.prompt_mode {
                PromptMode::Arg => profile.args.push(task.prompt),
                PromptMode::Type => typed = Some(format!("{}\r", task.prompt)),
                PromptMode::None => {}
            }
        }
        // Taken before HEAD is read, so every file the run creates is newer (undo).
        let checkpoint_at = now_ms();
        let checkpoint = self.head(&project.path).await;
        let start = Start {
            project,
            profile,
            task_id: Some(task.id),
            title: Some(task.title),
            cols: params.cols,
            rows: params.rows,
            typed,
            native_session_id: None,
        };
        match self.start_session(start) {
            Ok(session_id) => {
                let task =
                    self.store()
                        .set_task_checkpoint(task.id, checkpoint.as_deref(), checkpoint_at);
                self.task_updated(task);
                ok(&SessionSpawnResult { session_id })
            }
            Err(outcome) => outcome,
        }
    }

    /// What `task.undo` would discard (ADR 0005).
    async fn task_undo_preview(&self, task_id: Ulid) -> Outcome {
        let target = match self.undo_target(task_id) {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        let git = Arc::clone(&self.git);
        let previewed = tokio::task::spawn_blocking(move || {
            undo::preview(&*git, &target.dir, &target.checkpoint, target.since)
        })
        .await;
        match previewed {
            Ok(Ok(preview)) => ok(&preview),
            Ok(Err(e)) => Outcome::error(ErrorCode::Internal, e.to_string()),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("undo preview failed: {e}")),
        }
    }

    /// Discards everything since the task's checkpoint (ADR 0005): only with `confirm` and
    /// while no session of the task is live.
    async fn task_undo(&self, params: TaskUndoParams) -> Outcome {
        if !params.confirm {
            let message = "task.undo discards changes; it needs confirm: true";
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
        let target = match self.undo_target(params.task_id) {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        if target.live {
            let message = format!("task {} is running; stop it before undoing", params.task_id);
            return Outcome::error(ErrorCode::InvalidState, message);
        }
        let git = Arc::clone(&self.git);
        let undone = tokio::task::spawn_blocking(move || {
            let (dir, checkpoint) = (&target.dir, &target.checkpoint);
            let (previewed, created) = (&params.fingerprint, &params.created);
            undo::undo(&*git, dir, checkpoint, target.since, previewed, created)
        })
        .await;
        match undone {
            Ok(Ok(undo::Undone::Done)) => {
                tracing::info!(task = %params.task_id, "task undone");
                ok(&())
            }
            Ok(Ok(undo::Undone::Stale)) => {
                let message = "the changes differ from the preview; preview again";
                Outcome::error(ErrorCode::InvalidState, message)
            }
            Ok(Err(e)) => Outcome::error(ErrorCode::Internal, e.to_string()),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("undo failed: {e}")),
        }
    }

    /// The task's project folder and checkpoint; `invalid_state` without a checkpoint.
    fn undo_target(&self, task_id: Ulid) -> Result<UndoTarget, Outcome> {
        let found = {
            let store = self.store();
            store.get_task(task_id).and_then(|task| {
                let project = store.get_project(task.project_id)?;
                let live = store
                    .list_sessions(Some(project.id))?
                    .iter()
                    .any(|s| s.task_id == Some(task.id) && s.state != SessionState::Exited);
                let since = store.task_checkpoint_at(task.id)?;
                Ok((task, project, live, since))
            })
        };
        let (task, project, live, since) = found.map_err(|e| store_outcome::<()>(Err(e)))?;
        let Some(checkpoint) = task.checkpoint.filter(|c| undo::is_commit_id(c)) else {
            let message = format!("task {task_id} has no checkpoint; run it in a git repo first");
            return Err(Outcome::error(ErrorCode::InvalidState, message));
        };
        Ok(UndoTarget {
            dir: std::path::PathBuf::from(project.path),
            checkpoint,
            // A checkpoint from before `checkpoint_at` existed: delete no untracked file.
            since: since.unwrap_or(i64::MAX),
            live,
        })
    }

    /// HEAD of the repo holding `dir` (SPEC §12B: no network); `None` outside a repo or when
    /// git cannot run, which only costs the task its checkpoint.
    async fn head(&self, dir: &str) -> Option<String> {
        let git = Arc::clone(&self.git);
        let dir = std::path::PathBuf::from(dir);
        match tokio::task::spawn_blocking(move || git.head(&dir)).await {
            Ok(Ok(head)) => head,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "git rev-parse HEAD failed; no checkpoint");
                None
            }
            Err(e) => {
                tracing::warn!(error = %e, "git checkpoint task failed");
                None
            }
        }
    }

    /// Inserts the session row and starts its PTY; a task's card moves to running.
    fn start_session(&self, start: Start) -> Result<Ulid, Outcome> {
        let started_at = now_ms();
        let session = Session {
            id: Ulid::new(),
            task_id: start.task_id,
            project_id: start.project.id,
            agent_profile: start.profile.id.clone(),
            kind: session_kind(start.profile.kind),
            title: start.title,
            native_session_id: start.native_session_id,
            state: SessionState::Starting,
            state_source: StateSource::Daemon,
            state_changed_at: started_at,
            exit_code: None,
            cols: start.cols,
            rows: start.rows,
            started_at,
            ended_at: None,
        };
        let session_id = session.id;
        let spec = SpawnSpec {
            log_file: session_log::path(&self.paths.sessions_dir(), session_id),
            session,
            profile: start.profile,
            cwd: start.project.path.into(),
            socket: self.paths.socket.clone(),
            typed: start.typed,
        };
        match self.hub.spawn(spec) {
            Ok(()) => Ok(session_id),
            Err(SpawnError::Launch(e @ LaunchError::NotFound { .. })) => {
                Err(Outcome::error(ErrorCode::NotFound, e.to_string()))
            }
            Err(SpawnError::Launch(e)) => {
                Err(Outcome::error(ErrorCode::InvalidRequest, e.to_string()))
            }
            Err(e @ SpawnError::TaskRunning(_)) => {
                Err(Outcome::error(ErrorCode::InvalidState, e.to_string()))
            }
            Err(SpawnError::Other(e)) => {
                tracing::error!("spawn failed: {e:#}");
                Err(Outcome::error(ErrorCode::Internal, format!("{e:#}")))
            }
        }
    }

    /// Loads profile `id`, naming the parse error if its file is broken.
    fn load_profile(&self, id: &str) -> Result<mkrew_core::AgentProfile, Outcome> {
        let profiles = mkrew_core::load_dir(&self.paths.agents_dir());
        if let Some(profile) = profiles.get(id) {
            return Ok(profile.clone());
        }
        let wanted = format!("{id}.toml");
        let broken = profiles
            .errors
            .iter()
            .find(|e| e.file.file_name().is_some_and(|f| f == wanted.as_str()));
        let message = match broken {
            Some(e) => format!("agent profile {id} is broken: {e}"),
            None => format!(
                "agent profile {id} not found in {}",
                self.paths.agents_dir().display()
            ),
        };
        Err(Outcome::error(ErrorCode::NotFound, message))
    }

    /// Makes `session_id` the only pane of a tab that has no layout yet. A tab that already
    /// has panes is left to the client, which knows where the new pane goes.
    fn place_in_empty_tab(&self, tab_id: Ulid, session_id: Ulid) {
        let placed = {
            // One lock for check and set, so a concurrent `tab.set_layout` is not overwritten.
            let store = self.store();
            match store.get_tab(tab_id) {
                Ok(tab) if tab.layout.is_null() => {
                    let leaf = serde_json::json!({ "session": session_id.to_string() });
                    store.set_tab_layout(tab_id, &leaf).map(Some)
                }
                Ok(_) => Ok(None),
                Err(e) => Err(e),
            }
        };
        match placed {
            Ok(Some(tab)) => {
                let _ = self.hub.events.send(Event::TabUpdated(tab));
            }
            Ok(None) => {}
            // Closed meanwhile; the session still runs and shows in the sidebar.
            Err(e) => tracing::warn!(%tab_id, "cannot place session {session_id} in tab: {e}"),
        }
    }

    /// Changes the project; a new `default_agent` must name an agent profile.
    fn project_update(&self, params: ProjectUpdateParams) -> Outcome {
        if let Some(Some(id)) = &params.default_agent
            && let Err(outcome) = self.check_agent_profile(id)
        {
            return outcome;
        }
        let updated = self.store().update_project(
            params.project_id,
            params.name.as_deref(),
            params.default_agent.as_ref().map(Option::as_deref),
        );
        if let Ok(project) = &updated {
            let _ = self.hub.events.send(Event::ProjectUpdated(project.clone()));
        }
        store_outcome(updated)
    }

    /// `id` must name an agent profile (not a shell).
    fn check_agent_profile(&self, id: &str) -> Result<(), Outcome> {
        match self.load_profile(id)? {
            profile if profile.kind == mkrew_core::ProfileKind::Agent => Ok(()),
            _ => {
                let message = format!("{id} is a shell profile, not an agent");
                Err(Outcome::error(ErrorCode::InvalidRequest, message))
            }
        }
    }

    fn task_create(&self, params: TaskCreateParams) -> Outcome {
        if params.title.trim().is_empty() {
            return Outcome::error(ErrorCode::InvalidRequest, "task title is empty");
        }
        if let Some(id) = &params.agent_profile
            && let Err(outcome) = self.check_agent_profile(id)
        {
            return outcome;
        }
        let task = self.store().create_task(
            params.project_id,
            &params.title,
            &params.prompt,
            params.agent_profile.as_deref(),
            params.position,
        );
        self.task_updated(task)
    }

    fn task_update(&self, params: TaskUpdateParams) -> Outcome {
        if params.title.as_deref().is_some_and(|t| t.trim().is_empty()) {
            return Outcome::error(ErrorCode::InvalidRequest, "task title is empty");
        }
        if let Some(Some(id)) = &params.agent_profile
            && let Err(outcome) = self.check_agent_profile(id)
        {
            return outcome;
        }
        let task = self.store().update_task(
            params.task_id,
            params.title.as_deref(),
            params.prompt.as_deref(),
            params.agent_profile.as_ref().map(Option::as_deref),
        );
        self.task_updated(task)
    }

    /// Deletes the task (refused while it has a live session) and announces `task.deleted`.
    fn task_delete(&self, task_id: Ulid) -> Outcome {
        let deleted = self.store().delete_task(task_id);
        match deleted {
            Ok(task) => {
                let event = TaskDeletedEvent {
                    task_id,
                    project_id: task.project_id,
                };
                let _ = self.hub.events.send(Event::TaskDeleted(event));
                ok(&())
            }
            Err(e) => store_outcome::<()>(Err(e)),
        }
    }

    /// Replies with the task and announces it as `task.updated`.
    fn task_updated(&self, task: Result<Task, StoreError>) -> Outcome {
        if let Ok(task) = &task {
            let _ = self.hub.events.send(Event::TaskUpdated(task.clone()));
        }
        store_outcome(task)
    }

    /// Replies with the tab and announces it as `tab.updated`.
    fn tab_updated(&self, tab: Result<TerminalTab, StoreError>) -> Outcome {
        if let Ok(tab) = &tab {
            let _ = self.hub.events.send(Event::TabUpdated(tab.clone()));
        }
        store_outcome(tab)
    }

    /// Killing an already exited session succeeds (idempotent).
    fn session_kill(&self, params: SessionKillParams) -> Outcome {
        let command = Command::Kill {
            force: params.force,
        };
        if self.hub.command(params.session_id, command) {
            return ok(&());
        }
        let known = self
            .store()
            .list_sessions(None)
            .map(|all| all.iter().any(|s| s.id == params.session_id));
        match known {
            Ok(true) => ok(&()),
            Ok(false) => {
                let message = format!("session {} not found", params.session_id);
                Outcome::error(ErrorCode::NotFound, message)
            }
            Err(e) => store_outcome::<()>(Err(e)),
        }
    }

    /// Replies with the screen snapshot; the flusher is started once the reply is queued.
    async fn session_attach(&self, conn: &mut Conn, session_id: Ulid) -> Outcome {
        let Some(session) = self.hub.sender(session_id) else {
            return self.not_live(session_id);
        };
        let attachment = Arc::new(Attachment::default());
        let (reply, result) = oneshot::channel();
        let command = Command::Attach {
            attachment: Arc::clone(&attachment),
            reply,
        };
        if session.send(command).await.is_err() {
            return self.not_live(session_id);
        }
        // Dropped unanswered when the session exits first.
        let Ok(result) = result.await else {
            return self.not_live(session_id);
        };
        conn.new_attachment = Some(NewAttachment {
            session_id,
            attachment,
            session,
        });
        ok(&result)
    }

    /// Delivers `command` to a live session, waiting for room in its queue.
    async fn session_command(&self, session_id: Ulid, command: Command) -> Outcome {
        match self.hub.sender(session_id) {
            Some(session) if session.send(command).await.is_ok() => ok(&()),
            _ => self.not_live(session_id),
        }
    }

    /// Hands a hook report to its session. Only a live session may report (SPEC §15): an
    /// exited or unknown id is rejected like any other session request.
    async fn hook_report(&self, report: HookReportParams) -> Outcome {
        let Some(agent) = HookAgent::parse(&report.agent) else {
            let message = format!("unknown hook agent {:?}", report.agent);
            return Outcome::error(ErrorCode::InvalidRequest, message);
        };
        let session_id = report.mkrew_session_id;
        self.session_command(session_id, Command::Hook { agent, report })
            .await
    }

    /// Queues `data` for the session's PTY writer thread. A child that does not read its
    /// input makes this wait (in this connection only), never the session task.
    async fn session_input(&self, session_id: Ulid, data: &[u8]) -> Outcome {
        let Some(input) = self.hub.input(session_id) else {
            return self.not_live(session_id);
        };
        let input = input.lock().await;
        for chunk in data.chunks(INPUT_CHUNK_BYTES) {
            if input.send(chunk.to_vec()).await.is_err() {
                return self.not_live(session_id);
            }
        }
        ok(&())
    }

    /// Answers once the session is in one of `until` or has exited (nothing changes after
    /// that). Blocks this connection's later requests meanwhile, like any request.
    async fn session_wait(&self, params: SessionWaitParams) -> Outcome {
        if params.until.is_empty() {
            return Outcome::error(ErrorCode::InvalidRequest, "until must name a state");
        }
        let id = params.session_id;
        // Subscribed before reading the state, so no change in between is missed.
        let mut events = self.hub.events.subscribe();
        let wait = async {
            loop {
                let session = match self.store().list_sessions(None) {
                    Ok(all) => all.into_iter().find(|s| s.id == id),
                    Err(e) => return store_outcome::<()>(Err(e)),
                };
                let Some(session) = session else {
                    return Outcome::error(ErrorCode::NotFound, format!("session {id} not found"));
                };
                if params.until.contains(&session.state) || session.state == SessionState::Exited {
                    return ok(&SessionWaitResult {
                        state: session.state,
                        exit_code: session.exit_code,
                    });
                }
                // The session task saves the state before announcing it: re-read on change.
                loop {
                    match events.recv().await {
                        Ok(Event::SessionState(e)) if e.session_id == id => break,
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(_)) => break,
                        Err(broadcast::error::RecvError::Closed) => {
                            return Outcome::error(ErrorCode::Internal, "daemon is stopping");
                        }
                    }
                }
            }
        };
        let Some(timeout_ms) = params.timeout_ms else {
            return wait.await;
        };
        let limit = std::time::Duration::from_millis(timeout_ms);
        tokio::time::timeout(limit, wait).await.unwrap_or_else(|_| {
            let message = format!("session {id} did not reach the state within {timeout_ms} ms");
            Outcome::error(ErrorCode::Timeout, message)
        })
    }

    /// Error for a session that is not running: exited (`invalid_state`) or unknown.
    fn not_live(&self, session_id: Ulid) -> Outcome {
        let known = self
            .store()
            .list_sessions(None)
            .map(|all| all.iter().any(|s| s.id == session_id));
        match known {
            Ok(true) => {
                let message = format!("session {session_id} has exited");
                Outcome::error(ErrorCode::InvalidState, message)
            }
            Ok(false) => {
                let message = format!("session {session_id} not found");
                Outcome::error(ErrorCode::NotFound, message)
            }
            Err(e) => store_outcome::<()>(Err(e)),
        }
    }

    fn agent_profiles(&self) -> AgentProfilesResult {
        let set = mkrew_core::load_dir(&self.paths.agents_dir());
        let profiles = set
            .profiles
            .into_iter()
            .map(|p| AgentProfileInfo {
                kind: session_kind(p.kind),
                id: p.id,
                name: p.name,
                command: p.command,
            })
            .collect();
        let errors = set
            .errors
            .into_iter()
            .map(|e| ProfileErrorInfo {
                file: e.file.display().to_string(),
                line: e.line,
                column: e.column,
                message: e.message,
            })
            .collect();
        AgentProfilesResult { profiles, errors }
    }

    fn project_add(&self, params: ProjectAddParams) -> Outcome {
        let added = self
            .store()
            .add_project(std::path::Path::new(&params.path), params.name.as_deref());
        if let Ok(project) = &added {
            tracing::info!(id = %project.id, path = %project.path, "project added");
            let _ = self.hub.events.send(Event::ProjectUpdated(project.clone()));
        }
        store_outcome(added)
    }

    fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.hub.store()
    }
}

/// Frames queued for one client's writer task.
const OUTBOX_FRAMES: usize = 256;

/// One encoded frame for a connection's writer task.
pub struct Outgoing {
    pub bytes: Vec<u8>,
    /// Dropped once the frame is written, so an output flusher can wait for it.
    pub written: Option<oneshot::Sender<()>>,
}

impl From<Vec<u8>> for Outgoing {
    fn from(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            written: None,
        }
    }
}

/// What [`Daemon::start_session`] starts.
struct Start {
    project: Project,
    profile: mkrew_core::AgentProfile,
    task_id: Option<Ulid>,
    title: Option<String>,
    cols: u16,
    rows: u16,
    typed: Option<String>,
    /// Known up front only for a resumed conversation.
    native_session_id: Option<String>,
}

/// Per-connection state kept by the read loop.
#[derive(Default)]
struct Conn {
    greeted: bool,
    /// Sessions this client is attached to; dropping an entry detaches it.
    attachments: HashMap<Ulid, Attached>,
    /// Set by `session.attach`; its flusher starts after the reply is queued.
    new_attachment: Option<NewAttachment>,
}

struct NewAttachment {
    session_id: Ulid,
    attachment: Arc<Attachment>,
    session: mpsc::Sender<Command>,
}

struct Attached {
    attachment: Arc<Attachment>,
    _flusher: AbortOnDrop,
}

impl Drop for Attached {
    fn drop(&mut self) {
        self.attachment.close();
    }
}

/// Pushes every event to one client until it disconnects.
async fn forward(mut events: broadcast::Receiver<Event>, out: mpsc::Sender<Outgoing>) {
    loop {
        match events.recv().await {
            Ok(event) => match encode(&Frame::Message(Message::Event(event))) {
                Ok(bytes) => {
                    if out.send(bytes.into()).await.is_err() {
                        return;
                    }
                }
                Err(e) => tracing::error!("encode event: {e}"),
            },
            Err(broadcast::error::RecvError::Lagged(missed)) => {
                tracing::warn!(missed, "client too slow; events dropped");
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

/// Stops the event forwarder when the connection ends.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn ok<T: Serialize>(result: &T) -> Outcome {
    Outcome::ok(result)
        .unwrap_or_else(|e| Outcome::error(ErrorCode::Internal, format!("encode result: {e}")))
}

fn store_outcome<T: Serialize>(result: Result<T, StoreError>) -> Outcome {
    match result {
        Ok(value) => ok(&value),
        Err(e) => {
            let code = match e {
                StoreError::DuplicateProject(_) => ErrorCode::AlreadyExists,
                StoreError::Path { .. } | StoreError::NotADirectory(_) => ErrorCode::InvalidRequest,
                StoreError::SessionNotFound(_)
                | StoreError::ProjectNotFound(_)
                | StoreError::TabNotFound(_)
                | StoreError::TaskNotFound(_)
                | StoreError::CommentNotFound(_) => ErrorCode::NotFound,
                StoreError::TaskHasLiveSession(_) => ErrorCode::InvalidState,
                _ => {
                    tracing::error!("store error: {e}");
                    ErrorCode::Internal
                }
            };
            Outcome::error(code, e.to_string())
        }
    }
}

fn session_kind(kind: mkrew_core::ProfileKind) -> SessionKind {
    match kind {
        mkrew_core::ProfileKind::Agent => SessionKind::Agent,
        mkrew_core::ProfileKind::Shell => SessionKind::Shell,
    }
}

/// The wire method name, taken from the serialized request.
fn method_name(request: &Request) -> String {
    serde_json::to_value(request)
        .ok()
        .and_then(|v| v.get("method")?.as_str().map(str::to_owned))
        .unwrap_or_else(|| "this method".into())
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
