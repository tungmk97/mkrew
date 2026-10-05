//! Accept loop and per-connection request handling (SPEC §7, §8).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use interprocess::local_socket::tokio::{Listener, Stream, prelude::*};
use mkrew_core::Paths;
use mkrew_protocol::{
    AgentProfileInfo, AgentProfilesResult, DaemonStatusResult, ErrorCode, Event, Frame,
    FrameDecoder, HelloParams, HelloResult, Message, Outcome, PROTOCOL_VERSION, ProfileErrorInfo,
    ProjectAddParams, Request, Session, SessionKillParams, SessionKind, SessionSpawnParams,
    SessionSpawnResult, SessionState, encode,
};
use mkrew_store::{Store, StoreError};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use ulid::Ulid;

use crate::attach::{self, Attachment};
use crate::launch::LaunchError;
use crate::session::{Command, Hub, SpawnError, SpawnSpec};

const DAEMON_VERSION: &str = env!("CARGO_PKG_VERSION");

/// State shared by all connections.
pub struct Daemon {
    hub: Arc<Hub>,
    paths: Paths,
    started_at: i64,
    shutdown: watch::Sender<bool>,
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
            started_at: now_ms(),
            shutdown: watch::channel(false).0,
        })
    }

    /// Serves `listener` until `daemon.stop` or Ctrl+C, then stops every session (SPEC §8).
    pub async fn serve(self: Arc<Self>, listener: Listener) {
        Arc::clone(&self).accept(listener).await;
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
            Request::SessionSpawn(params) => (self.session_spawn(params), After::Continue),
            Request::SessionList(params) => {
                let sessions = self.store().list_sessions(params.project_id);
                (store_outcome(sessions), After::Continue)
            }
            Request::SessionKill(params) => (self.session_kill(params), After::Continue),
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
                let command = Command::Input(params.data);
                let outcome = self.session_command(params.session_id, command).await;
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
            Request::AgentProfiles => (ok(&self.agent_profiles()), After::Continue),
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
        let project = match self.store().list_projects() {
            Ok(projects) => projects.into_iter().find(|p| p.id == params.project_id),
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        let Some(project) = project else {
            let message = format!("project {} not found", params.project_id);
            return Outcome::error(ErrorCode::NotFound, message);
        };
        let profiles = mkrew_core::load_dir(&self.paths.agents_dir());
        let Some(profile) = profiles.get(&params.agent_profile).cloned() else {
            let wanted = format!("{}.toml", params.agent_profile);
            let broken = profiles
                .errors
                .iter()
                .find(|e| e.file.file_name().is_some_and(|f| f == wanted.as_str()));
            let message = match broken {
                Some(e) => format!("agent profile {} is broken: {e}", params.agent_profile),
                None => format!(
                    "agent profile {} not found in {}",
                    params.agent_profile,
                    self.paths.agents_dir().display()
                ),
            };
            return Outcome::error(ErrorCode::NotFound, message);
        };
        let session = Session {
            id: Ulid::new(),
            task_id: None,
            project_id: project.id,
            agent_profile: profile.id.clone(),
            kind: params.kind,
            title: params.title,
            native_session_id: None,
            state: SessionState::Starting,
            exit_code: None,
            cols: params.cols,
            rows: params.rows,
            started_at: now_ms(),
            ended_at: None,
        };
        let session_id = session.id;
        let spec = SpawnSpec {
            log_file: self.paths.sessions_dir().join(format!("{session_id}.log")),
            session,
            profile,
            cwd: project.path.into(),
            socket: self.paths.socket.clone(),
        };
        match self.hub.spawn(spec) {
            Ok(()) => ok(&SessionSpawnResult { session_id }),
            Err(SpawnError::Launch(e @ LaunchError::NotFound { .. })) => {
                Outcome::error(ErrorCode::NotFound, e.to_string())
            }
            Err(SpawnError::Launch(e)) => Outcome::error(ErrorCode::InvalidRequest, e.to_string()),
            Err(SpawnError::Other(e)) => {
                tracing::error!("spawn failed: {e:#}");
                Outcome::error(ErrorCode::Internal, format!("{e:#}"))
            }
        }
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
                kind: match p.kind {
                    mkrew_core::ProfileKind::Agent => SessionKind::Agent,
                    mkrew_core::ProfileKind::Shell => SessionKind::Shell,
                },
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
                StoreError::SessionNotFound(_) => ErrorCode::NotFound,
                _ => {
                    tracing::error!("store error: {e}");
                    ErrorCode::Internal
                }
            };
            Outcome::error(code, e.to_string())
        }
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
