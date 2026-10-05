//! Runs the real `mkrew-daemon` binary in a temp dir and talks to it over the local socket.

// Helpers outside `#[test]` fns are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use interprocess::local_socket::{Stream, prelude::*};
use mkrew_daemon::EXIT_ALREADY_RUNNING;
use mkrew_protocol::{
    ClientKind, DaemonStatusResult, ErrorCode, Frame, FrameDecoder, HelloParams, HelloResult,
    Message, Outcome, PROTOCOL_VERSION, Project, ProjectAddParams, Request, ResponseError, Session,
    SessionKind, SessionState, encode,
};
use mkrew_store::Store;
use ulid::Ulid;

const TIMEOUT: Duration = Duration::from_secs(15);

struct Env {
    dir: tempfile::TempDir,
    socket: String,
}

impl Env {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let socket = if cfg!(windows) {
            format!(r"\\.\pipe\mkrew-test-{}", Ulid::new())
        } else {
            dir.path().join("mkrew.sock").to_string_lossy().into_owned()
        };
        Self { dir, socket }
    }

    fn data_dir(&self) -> PathBuf {
        self.dir.path().join("data")
    }

    fn spawn(&self) -> Daemon {
        let child = Command::new(env!("CARGO_BIN_EXE_mkrew-daemon"))
            .env("MKREW_DATA_DIR", self.data_dir())
            .env("MKREW_CONFIG_DIR", self.dir.path().join("config"))
            .env("MKREW_SOCKET", &self.socket)
            .env("MKREW_LOG", "debug")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        Daemon(child)
    }

    /// Spawns a daemon and waits until it answers `hello`.
    fn start(&self) -> (Daemon, Client) {
        let mut daemon = self.spawn();
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Ok(mut client) = Client::connect(&self.socket) {
                client.hello().unwrap();
                return (daemon, client);
            }
            if let Some(status) = daemon.0.try_wait().unwrap() {
                panic!("daemon exited during startup: {status}");
            }
            assert!(Instant::now() < deadline, "daemon did not start");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// Kills the daemon if a test fails before stopping it.
struct Daemon(Child);

impl Daemon {
    fn wait_exit(&mut self) -> ExitStatus {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "daemon did not exit");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Client {
    stream: Stream,
    decoder: FrameDecoder,
    next_id: u64,
}

impl Client {
    fn connect(socket: &str) -> std::io::Result<Self> {
        Ok(Self {
            stream: Stream::connect(mkrew_daemon::ipc::name(socket)?)?,
            decoder: FrameDecoder::new(),
            next_id: 1,
        })
    }

    fn call(&mut self, request: Request) -> Outcome {
        let id = self.next_id;
        self.next_id += 1;
        let frame = Frame::Message(Message::Request { id, request });
        self.stream.write_all(&encode(&frame).unwrap()).unwrap();
        let mut buf = [0u8; 4096];
        loop {
            if let Some(frame) = self.decoder.next_frame().unwrap() {
                match frame {
                    Frame::Message(Message::Response { id: got, outcome }) => {
                        assert_eq!(got, id);
                        return outcome;
                    }
                    other => panic!("unexpected frame {other:?}"),
                }
            }
            let n = self.stream.read(&mut buf).unwrap();
            assert!(n > 0, "daemon closed the connection");
            self.decoder.push(&buf[..n]);
        }
    }

    fn hello(&mut self) -> Result<HelloResult, ResponseError> {
        self.call(Request::Hello(HelloParams {
            client: ClientKind::Cli,
            protocol_version: PROTOCOL_VERSION,
        }))
        .into_result()
    }
}

fn error_code(outcome: Outcome) -> ErrorCode {
    match outcome.into_result::<serde_json::Value>() {
        Err(ResponseError::Daemon(body)) => body.code,
        other => panic!("expected a daemon error, got {other:?}"),
    }
}

#[test]
fn starting_twice_never_yields_two_daemons() {
    let env = Env::new();
    let (mut first, mut client) = env.start();

    let mut second = env.spawn();
    assert_eq!(
        second.wait_exit().code(),
        Some(i32::from(EXIT_ALREADY_RUNNING))
    );

    // The first daemon is unaffected.
    let status: DaemonStatusResult = client.call(Request::DaemonStatus).into_result().unwrap();
    assert_eq!(status.pid, first.0.id());
    assert_eq!(status.protocol_version, PROTOCOL_VERSION);

    // daemon.stop exits cleanly and releases the lock, so a new daemon can start.
    client
        .call(Request::DaemonStop)
        .into_result::<()>()
        .unwrap();
    assert!(first.wait_exit().success());
    let (_third, mut client) = env.start();
    client
        .call(Request::DaemonStop)
        .into_result::<()>()
        .unwrap();
}

#[test]
fn hello_is_required_and_version_checked() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();

    let mut fresh = Client::connect(&env.socket).unwrap();
    assert_eq!(
        error_code(fresh.call(Request::ProjectList)),
        ErrorCode::HelloRequired
    );

    let mut old = Client::connect(&env.socket).unwrap();
    let outcome = old.call(Request::Hello(HelloParams {
        client: ClientKind::Ui,
        protocol_version: PROTOCOL_VERSION + 1,
    }));
    assert_eq!(error_code(outcome), ErrorCode::VersionMismatch);

    client
        .call(Request::DaemonStop)
        .into_result::<()>()
        .unwrap();
}

#[test]
fn projects_are_added_and_listed() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    let repo = env.dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();

    let add = |client: &mut Client, path: &Path| {
        client.call(Request::ProjectAdd(ProjectAddParams {
            path: path.to_string_lossy().into_owned(),
            name: Some("demo".into()),
        }))
    };
    let project: Project = add(&mut client, &repo).into_result().unwrap();
    assert_eq!(project.name, "demo");
    assert_eq!(
        error_code(add(&mut client, &repo)),
        ErrorCode::AlreadyExists
    );
    assert_eq!(
        error_code(add(&mut client, &repo.join("missing"))),
        ErrorCode::InvalidRequest
    );

    let listed: Vec<Project> = client.call(Request::ProjectList).into_result().unwrap();
    assert_eq!(listed, vec![project]);
    client
        .call(Request::DaemonStop)
        .into_result::<()>()
        .unwrap();
}

#[test]
fn startup_marks_stale_sessions_exited() {
    let env = Env::new();
    std::fs::create_dir_all(env.data_dir()).unwrap();
    let db = env.data_dir().join("mkrew.db");
    let session_id = Ulid::new();
    {
        let store = Store::open(&db).unwrap();
        let project = store.add_project(env.dir.path(), None).unwrap();
        store
            .insert_session(&Session {
                id: session_id,
                task_id: None,
                project_id: project.id,
                agent_profile: "fake".into(),
                kind: SessionKind::Agent,
                title: None,
                native_session_id: None,
                state: SessionState::Working,
                exit_code: None,
                cols: 80,
                rows: 24,
                started_at: 1,
                ended_at: None,
            })
            .unwrap();
    }

    let (mut daemon, mut client) = env.start();
    let status: DaemonStatusResult = client.call(Request::DaemonStatus).into_result().unwrap();
    assert_eq!(status.sessions, 0);
    client
        .call(Request::DaemonStop)
        .into_result::<()>()
        .unwrap();
    assert!(daemon.wait_exit().success());

    let sessions = Store::open(&db).unwrap().list_sessions(None).unwrap();
    assert_eq!(sessions[0].id, session_id);
    assert_eq!(sessions[0].state, SessionState::Exited);
    assert!(env.data_dir().join("logs").read_dir().unwrap().count() > 0);
}
