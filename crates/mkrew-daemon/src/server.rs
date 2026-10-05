//! Accept loop and per-connection request handling (SPEC §7, §8).

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use interprocess::local_socket::tokio::{Listener, Stream, prelude::*};
use mkrew_protocol::{
    DaemonStatusResult, ErrorCode, Frame, FrameDecoder, HelloParams, HelloResult, Message, Outcome,
    PROTOCOL_VERSION, ProjectAddParams, Request, encode,
};
use mkrew_store::{Store, StoreError};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::watch;

const DAEMON_VERSION: &str = env!("CARGO_PKG_VERSION");

/// State shared by all connections.
pub struct Daemon {
    store: Mutex<Store>,
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
    pub fn new(store: Store) -> Arc<Self> {
        Arc::new(Self {
            store: Mutex::new(store),
            started_at: now_ms(),
            shutdown: watch::channel(false).0,
        })
    }

    /// Serves `listener` until `daemon.stop` or Ctrl+C.
    pub async fn serve(self: Arc<Self>, listener: Listener) {
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

    /// Requests are answered in order; the first one must be `hello`.
    async fn connection(&self, stream: Stream) -> anyhow::Result<()> {
        let (mut rx, mut tx) = stream.split();
        let mut decoder = FrameDecoder::new();
        let mut buf = vec![0u8; 64 * 1024];
        let mut greeted = false;
        loop {
            while let Some(frame) = decoder.next_frame()? {
                let Frame::Message(Message::Request { id, request }) = frame else {
                    anyhow::bail!("client sent a non-request frame");
                };
                let (outcome, after) = self.handle(&mut greeted, request);
                let reply = Frame::Message(Message::Response { id, outcome });
                tx.write_all(&encode(&reply)?).await?;
                if after == After::Continue {
                    continue;
                }
                tx.flush().await?;
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

    fn handle(&self, greeted: &mut bool, request: Request) -> (Outcome, After) {
        let request = match request {
            Request::Hello(params) => return self.hello(greeted, params),
            _ if !*greeted => {
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
        // No PTYs yet (T1.8): every session left after startup marking is exited.
        let sessions = self.store().list_sessions(None).map(|all| {
            let live = all
                .iter()
                .filter(|s| s.state != mkrew_protocol::SessionState::Exited)
                .count();
            DaemonStatusResult {
                daemon_version: DAEMON_VERSION.into(),
                protocol_version: PROTOCOL_VERSION,
                pid: std::process::id(),
                started_at: self.started_at,
                sessions: u32::try_from(live).unwrap_or(u32::MAX),
            }
        });
        store_outcome(sessions)
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
        // A panic mid-query leaves the connection usable (SQLite rolls back), so ignore poison.
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
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

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
