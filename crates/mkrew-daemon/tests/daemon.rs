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
    AgentProfilesResult, ClientKind, DaemonStatusResult, ErrorCode, Event, Frame, FrameDecoder,
    HelloParams, HelloResult, Message, Outcome, PROTOCOL_VERSION, Project, ProjectAddParams,
    Request, ResponseError, Session, SessionExitedEvent, SessionKillParams, SessionKind,
    SessionListParams, SessionSpawnParams, SessionSpawnResult, SessionState, SessionStateEvent,
    encode,
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

    fn write_profile(&self, id: &str, toml: &str) {
        let dir = self.dir.path().join("config").join("agents");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{id}.toml")), toml).unwrap();
    }

    /// `agents/fake.toml` with the built `mkrew-fakeagent` instead of a PATH lookup.
    fn write_fake_profile(&self) {
        let fake = Path::new(env!("CARGO_BIN_EXE_mkrew-daemon"))
            .with_file_name(format!("mkrew-fakeagent{}", std::env::consts::EXE_SUFFIX));
        assert!(
            fake.is_file(),
            "{} is missing; run `cargo build -p mkrew-fakeagent`",
            fake.display()
        );
        let sample = include_str!("../../../agents/fake.toml");
        let profile = sample.replace(
            "command = \"mkrew-fakeagent\"",
            &format!("command = '{}'", fake.display()),
        );
        assert_ne!(profile, sample);
        self.write_profile("fake", &profile);
    }

    fn repo(&self) -> PathBuf {
        let repo = self.dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        repo
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
    /// Events pushed by the daemon, in arrival order.
    events: Vec<Event>,
}

impl Client {
    fn connect(socket: &str) -> std::io::Result<Self> {
        Ok(Self {
            stream: Stream::connect(mkrew_daemon::ipc::name(socket)?)?,
            decoder: FrameDecoder::new(),
            next_id: 1,
            events: Vec::new(),
        })
    }

    fn call(&mut self, request: Request) -> Outcome {
        let id = self.next_id;
        self.next_id += 1;
        let frame = Frame::Message(Message::Request { id, request });
        self.stream.write_all(&encode(&frame).unwrap()).unwrap();
        let mut buf = [0u8; 4096];
        loop {
            // One read can carry several frames (events before the response).
            while let Some(frame) = self.decoder.next_frame().unwrap() {
                match frame {
                    Frame::Message(Message::Response { id: got, outcome }) => {
                        assert_eq!(got, id);
                        return outcome;
                    }
                    Frame::Message(Message::Event(event)) => self.events.push(event),
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

    fn add_project(&mut self, path: &Path) -> Project {
        self.call(Request::ProjectAdd(ProjectAddParams {
            path: path.to_string_lossy().into_owned(),
            name: None,
        }))
        .into_result()
        .unwrap()
    }

    fn spawn(&mut self, project: &Project, profile: &str) -> Outcome {
        self.call(Request::SessionSpawn(SessionSpawnParams {
            project_id: project.id,
            agent_profile: profile.into(),
            kind: SessionKind::Agent,
            tab_id: None,
            title: None,
            cols: 80,
            rows: 24,
        }))
    }

    fn session(&mut self, id: Ulid) -> Session {
        let all: Vec<Session> = self
            .call(Request::SessionList(SessionListParams::default()))
            .into_result()
            .unwrap();
        all.into_iter().find(|s| s.id == id).unwrap()
    }

    /// Polls `session.list` until the session is in `state` (events arrive meanwhile).
    fn wait_state(&mut self, id: Ulid, state: SessionState) -> Session {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let session = self.session(id);
            if session.state == state {
                return session;
            }
            assert!(
                Instant::now() < deadline,
                "session stuck in {:?}, wanted {state:?}",
                session.state
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Makes calls until a pushed event equal to `event` has arrived.
    fn wait_event(&mut self, event: &Event) {
        let deadline = Instant::now() + TIMEOUT;
        while !self.events.contains(event) {
            assert!(
                Instant::now() < deadline,
                "no {event:?} in {:?}",
                self.events
            );
            std::thread::sleep(Duration::from_millis(20));
            self.call(Request::DaemonStatus);
        }
    }

    fn stop(&mut self) {
        self.call(Request::DaemonStop).into_result::<()>().unwrap();
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

#[test]
fn agent_profiles_lists_samples_and_broken_files() {
    let env = Env::new();
    env.write_profile("broken", "name = \"Broken\"\ncommand = \"x\"\nbogus = 1\n");
    let (_daemon, mut client) = env.start();

    let result: AgentProfilesResult = client.call(Request::AgentProfiles).into_result().unwrap();
    let ids: Vec<&str> = result.profiles.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["claude", "codex", "fake", "omp", "shell"]);
    assert_eq!(result.errors.len(), 1, "{:?}", result.errors);
    let error = &result.errors[0];
    assert!(error.file.ends_with("broken.toml"), "{error:?}");
    assert_eq!(error.line, Some(3));

    // The daemon keeps serving, and spawning the broken profile names the problem.
    let project = client.add_project(&env.repo());
    let outcome = client.spawn(&project, "broken");
    let Err(ResponseError::Daemon(body)) = outcome.into_result::<SessionSpawnResult>() else {
        panic!("spawning a broken profile must fail");
    };
    assert_eq!(body.code, ErrorCode::NotFound);
    assert!(body.message.contains("broken.toml:3"), "{}", body.message);
    assert_eq!(
        error_code(client.spawn(&project, "nope")),
        ErrorCode::NotFound
    );
    client.stop();
}

#[test]
fn fake_agent_session_reaches_needs_input_and_is_killed() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());

    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    // The prompt is only visible on the emulated screen: the PTY works end to end.
    let session = client.wait_state(id, SessionState::NeedsInput);
    assert_eq!((session.cols, session.rows), (80, 24));
    client.wait_event(&Event::SessionState(SessionStateEvent {
        session_id: id,
        state: SessionState::NeedsInput,
    }));
    let status: DaemonStatusResult = client.call(Request::DaemonStatus).into_result().unwrap();
    assert_eq!(status.sessions, 1);

    let kill = |session_id, force| Request::SessionKill(SessionKillParams { session_id, force });
    client.call(kill(id, true)).into_result::<()>().unwrap();
    let session = client.wait_state(id, SessionState::Exited);
    assert!(session.ended_at.is_some());
    client.wait_event(&Event::SessionExited(SessionExitedEvent {
        session_id: id,
        exit_code: session.exit_code,
    }));
    // Killing an exited session is fine; an unknown one is not found.
    client.call(kill(id, false)).into_result::<()>().unwrap();
    assert_eq!(
        error_code(client.call(kill(Ulid::new(), false))),
        ErrorCode::NotFound
    );

    let log = session_log(&env, id);
    assert!(log.contains("Do you want to proceed? (y/n)"), "{log}");
    client.stop();
}

#[test]
fn session_env_cwd_and_exit_code_reach_the_store() {
    let env = Env::new();
    let script = if cfg!(windows) {
        "command = 'cmd.exe'\nargs = ['/c', 'echo', 'id=%MKREW_SESSION_ID%', 'term=%TERM%', '&', 'cd', '&', 'exit', '3']"
    } else {
        "command = 'sh'\nargs = ['-c', 'echo id=$MKREW_SESSION_ID term=$TERM; pwd; exit 3']"
    };
    env.write_profile("probe", &format!("name = \"Probe\"\n{script}\n"));
    let (_daemon, mut client) = env.start();
    let repo = env.repo();
    let project = client.add_project(&repo);

    let spawned: SessionSpawnResult = client.spawn(&project, "probe").into_result().unwrap();
    let id = spawned.session_id;
    let session = client.wait_state(id, SessionState::Exited);
    assert_eq!(session.exit_code, Some(3));

    let log = session_log(&env, id);
    assert!(log.contains(&format!("id={id}")), "{log}");
    assert!(log.contains("term=xterm-256color"), "{log}");
    let repo_name = repo.file_name().unwrap().to_string_lossy().into_owned();
    assert!(log.contains(&repo_name), "cwd missing: {log}");
    client.stop();
}

#[test]
fn daemon_stop_terminates_live_sessions() {
    let env = Env::new();
    env.write_fake_profile();
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    client.wait_state(spawned.session_id, SessionState::NeedsInput);

    let started = Instant::now();
    client.stop();
    assert!(daemon.wait_exit().success());
    // Under the daemon's 5 s kill grace: the graceful close (ConPTY close / SIGHUP) worked.
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "{:?}",
        started.elapsed()
    );

    let sessions = Store::open(&env.data_dir().join("mkrew.db"))
        .unwrap()
        .list_sessions(None)
        .unwrap();
    assert_eq!(sessions[0].state, SessionState::Exited);
}

fn session_log(env: &Env, id: Ulid) -> String {
    let path = env.data_dir().join("sessions").join(format!("{id}.log"));
    String::from_utf8_lossy(&std::fs::read(path).unwrap()).into_owned()
}
