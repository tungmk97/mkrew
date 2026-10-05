//! Runs the real `mkrew-daemon` binary in a temp dir and talks to it over the local socket.

// Helpers outside `#[test]` fns are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use interprocess::local_socket::{Stream, prelude::*};
use mkrew_daemon::EXIT_ALREADY_RUNNING;
use mkrew_protocol::{
    AgentProfilesResult, ClientKind, DaemonStatusResult, ErrorCode, Event, Frame, FrameDecoder,
    HelloParams, HelloResult, Message, Outcome, PROTOCOL_VERSION, Project, ProjectAddParams,
    ProjectUpdateParams, Request, ResponseError, ScreenSnapshot, Session, SessionAttachResult,
    SessionExitedEvent, SessionInputParams, SessionKillParams, SessionKind, SessionListParams,
    SessionRef, SessionResizeParams, SessionSendParams, SessionSpawnParams, SessionSpawnResult,
    SessionState, SessionStateEvent, SessionWaitParams, SessionWaitResult, TabCreateParams,
    TabListParams, TabMoveParams, TabRef, TabRenameParams, TabSetLayoutParams, TerminalTab, encode,
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
        self.write_fake_profile_as("fake", "[]");
    }

    /// Like [`Env::write_fake_profile`], saved as `<id>.toml` with fake-agent `args`.
    fn write_fake_profile_as(&self, id: &str, args: &str) {
        let fake = Path::new(env!("CARGO_BIN_EXE_mkrew-daemon"))
            .with_file_name(format!("mkrew-fakeagent{}", std::env::consts::EXE_SUFFIX));
        assert!(
            fake.is_file(),
            "{} is missing; run `cargo build -p mkrew-fakeagent`",
            fake.display()
        );
        let sample = include_str!("../../../agents/fake.toml");
        let profile = sample
            .replace(
                "command = \"mkrew-fakeagent\"",
                &format!("command = '{}'", fake.display()),
            )
            .replace("args = []", &format!("args = {args}"));
        assert_ne!(profile, sample);
        self.write_profile(id, &profile);
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
    /// Bytes of binary output frames per session.
    output: HashMap<Ulid, Vec<u8>>,
}

impl Client {
    fn connect(socket: &str) -> std::io::Result<Self> {
        Ok(Self {
            stream: Stream::connect(mkrew_daemon::ipc::name(socket)?)?,
            decoder: FrameDecoder::new(),
            next_id: 1,
            events: Vec::new(),
            output: HashMap::new(),
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
                    Frame::Output { session_id, data } => {
                        self.output.entry(session_id).or_default().extend(data);
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
            agent_profile: Some(profile.into()),
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

    fn attach(&mut self, session_id: Ulid) -> Outcome {
        self.call(Request::SessionAttach(SessionRef { session_id }))
    }

    fn input(&mut self, session_id: Ulid, data: &[u8]) -> Outcome {
        self.call(Request::SessionInput(SessionInputParams {
            session_id,
            data: data.to_vec(),
        }))
    }

    /// Makes calls until the output frames of `id` contain `text`.
    fn wait_output(&mut self, id: Ulid, text: &str) {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let output = self.output.get(&id).map(|o| String::from_utf8_lossy(o));
            if output.as_ref().is_some_and(|o| o.contains(text)) {
                return;
            }
            assert!(Instant::now() < deadline, "no {text:?} in {output:?}");
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

fn screen_text(snapshot: &ScreenSnapshot) -> String {
    snapshot
        .lines
        .iter()
        .map(|runs| runs.iter().map(|r| r.text.as_str()).collect::<String>() + "\n")
        .collect()
}

#[test]
fn attach_streams_output_and_reattach_restores_the_screen() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut first) = env.start();
    let project = first.add_project(&env.repo());
    let spawned: SessionSpawnResult = first.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    first.wait_state(id, SessionState::NeedsInput);

    let attached: SessionAttachResult = first.attach(id).into_result().unwrap();
    assert_eq!(attached.state, SessionState::NeedsInput);
    assert_eq!((attached.snapshot.cols, attached.snapshot.rows), (80, 24));
    let screen = screen_text(&attached.snapshot);
    assert!(screen.contains("Plan: edit src/main.rs"), "{screen}");
    assert!(screen.contains("Do you want to proceed? (y/n)"), "{screen}");

    let resize = |cols, rows| {
        Request::SessionResize(SessionResizeParams {
            session_id: id,
            cols,
            rows,
        })
    };
    first.call(resize(100, 30)).into_result::<()>().unwrap();
    assert_eq!(
        error_code(first.call(resize(0, 30))),
        ErrorCode::InvalidRequest
    );
    // Applied by the session task right after the reply.
    let deadline = Instant::now() + TIMEOUT;
    while (first.session(id).cols, first.session(id).rows) != (100, 30) {
        assert!(Instant::now() < deadline, "resize not saved");
        std::thread::sleep(Duration::from_millis(20));
    }
    first
        .call(Request::SessionDetach(SessionRef { session_id: id }))
        .into_result::<()>()
        .unwrap();
    // Output queued before the detach reply (e.g. ConPTY repainting after the resize) is fine.
    let before_detach = first.output.get(&id).map_or(0, Vec::len);

    // Another client re-attaches: the screen comes from the daemon's emulator, at the new size.
    let mut second = Client::connect(&env.socket).unwrap();
    second.hello().unwrap();
    let again: SessionAttachResult = second.attach(id).into_result().unwrap();
    assert_eq!((again.snapshot.cols, again.snapshot.rows), (100, 30));
    let screen = screen_text(&again.snapshot);
    assert!(screen.contains("Do you want to proceed? (y/n)"), "{screen}");

    // Input reaches the agent; its output streams back as binary frames.
    second.input(id, b"y\r").into_result::<()>().unwrap();
    second.wait_output(id, "Edited src/main.rs");
    second.wait_state(id, SessionState::Exited);
    first.call(Request::DaemonStatus);
    assert_eq!(
        first.output.get(&id).map_or(0, Vec::len),
        before_detach,
        "detached client got output"
    );

    assert_eq!(error_code(second.attach(id)), ErrorCode::InvalidState);
    assert_eq!(error_code(second.input(id, b"x")), ErrorCode::InvalidState);
    assert_eq!(error_code(second.attach(Ulid::new())), ErrorCode::NotFound);
    second.stop();
}

#[test]
fn slow_attached_client_gets_a_snapshot_instead_of_stalling_the_session() {
    let env = Env::new();
    env.write_fake_profile_as("flood", "['--scenario', 'flood']");
    let (_daemon, mut slow) = env.start();
    let project = slow.add_project(&env.repo());
    let spawned: SessionSpawnResult = slow.spawn(&project, "flood").into_result().unwrap();
    let id = spawned.session_id;
    slow.attach(id)
        .into_result::<SessionAttachResult>()
        .unwrap();

    // `slow` stops reading. The session must keep running and the daemon keep answering.
    let mut other = Client::connect(&env.socket).unwrap();
    other.hello().unwrap();
    let session = other.wait_state(id, SessionState::Exited);
    assert_eq!(session.exit_code, Some(0));

    // Its queue overflowed: the dropped output was replaced by a fresh snapshot.
    let deadline = Instant::now() + TIMEOUT;
    let snapshot = loop {
        let found = slow.events.iter().find_map(|e| match e {
            Event::SessionSnapshot(s) if s.session_id == id => Some(s.snapshot.clone()),
            _ => None,
        });
        if let Some(snapshot) = found {
            break snapshot;
        }
        assert!(Instant::now() < deadline, "no session.snapshot event");
        slow.call(Request::DaemonStatus);
    };
    assert_eq!((snapshot.cols, snapshot.rows), (80, 24));
    let received = slow.output.get(&id).map_or(0, Vec::len);
    let log = std::fs::metadata(env.data_dir().join("sessions").join(format!("{id}.log")))
        .unwrap()
        .len();
    assert!(
        (received as u64) < log,
        "received {received} of {log} bytes: nothing was dropped"
    );
    slow.stop();
}

#[test]
fn big_paste_to_a_child_that_does_not_read_leaves_the_daemon_responsive() {
    let env = Env::new();
    // The sleeper prints a line and then never reads its input.
    let scenario = env.dir.path().join("sleeper.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"print\"\ntext = \"Sleeper ready.\"\n\n\
         [[step]]\naction = \"sleep\"\nms = 120000\n",
    )
    .unwrap();
    env.write_fake_profile_as(
        "sleeper",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let sleeper: SessionSpawnResult = client.spawn(&project, "sleeper").into_result().unwrap();
    let sleeper = sleeper.session_id;
    client
        .attach(sleeper)
        .into_result::<SessionAttachResult>()
        .unwrap();
    client.wait_output(sleeper, "Sleeper ready.");

    // A 4 MiB paste on its own connection; it may wait until the session is killed.
    let socket = env.socket.clone();
    let (done_tx, done) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut paster = Client::connect(&socket).unwrap();
        paster.hello().unwrap();
        let outcome = paster.input(sleeper, &vec![b'a'; 4 * 1024 * 1024]);
        let _ = done_tx.send(outcome);
    });
    std::thread::sleep(Duration::from_millis(500));

    // Other sessions and the daemon keep working while the paste is pending.
    let fake: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let fake = fake.session_id;
    client.wait_state(fake, SessionState::NeedsInput);
    client.input(fake, b"y\r").into_result::<()>().unwrap();
    let session = client.wait_state(fake, SessionState::Exited);
    assert_eq!(session.exit_code, Some(0));
    let status: DaemonStatusResult = client.call(Request::DaemonStatus).into_result().unwrap();
    assert_eq!(status.sessions, 1);
    // Otherwise the OS absorbed the paste and this test proves nothing.
    assert!(done.try_recv().is_err(), "the paste is not backed up");

    // The sleeper's own task still handles commands: a kill ends it and the paste.
    let kill = SessionKillParams {
        session_id: sleeper,
        force: true,
    };
    client
        .call(Request::SessionKill(kill))
        .into_result::<()>()
        .unwrap();
    client.wait_state(sleeper, SessionState::Exited);
    let outcome = done
        .recv_timeout(TIMEOUT)
        .expect("the paste never returned");
    // Normally cut short by the exit; `ok` if the last chunks were queued just before.
    if outcome.clone().into_result::<()>().is_err() {
        assert_eq!(error_code(outcome), ErrorCode::InvalidState);
    }
    client.stop();
}

fn wait(session_id: Ulid, until: &[SessionState], timeout_ms: Option<u64>) -> Request {
    Request::SessionWait(SessionWaitParams {
        session_id,
        until: until.to_vec(),
        timeout_ms,
    })
}

fn send(session_id: Ulid, text: &str) -> Request {
    Request::SessionSend(SessionSendParams {
        session_id,
        text: text.into(),
        submit: true,
    })
}

#[test]
fn send_while_working_is_queued_until_the_agent_asks_and_wait_sees_it_finish() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;

    let working: SessionWaitResult = client
        .call(wait(id, &[SessionState::Working], Some(15_000)))
        .into_result()
        .unwrap();
    assert_eq!(working.state, SessionState::Working);
    // Queued while working, delivered once the prompt shows; the agent then finishes.
    client.call(send(id, "y")).into_result::<()>().unwrap();
    let done: SessionWaitResult = client
        .call(wait(
            id,
            &[SessionState::Idle, SessionState::Done],
            Some(30_000),
        ))
        .into_result()
        .unwrap();
    // The agent exits right after answering: exit ends every wait.
    assert!(
        matches!(done.state, SessionState::Idle | SessionState::Exited),
        "{done:?}"
    );
    let session = client.wait_state(id, SessionState::Exited);
    assert_eq!(session.exit_code, Some(0));
    assert!(session_log(&env, id).contains("Done."));

    // An exited session answers at once; sending to it is refused.
    let exited: SessionWaitResult = client
        .call(wait(id, &[SessionState::NeedsInput], None))
        .into_result()
        .unwrap();
    assert_eq!(
        exited,
        SessionWaitResult {
            state: SessionState::Exited,
            exit_code: Some(0)
        }
    );
    assert_eq!(
        error_code(client.call(send(id, "y"))),
        ErrorCode::InvalidState
    );
    client.stop();
}

#[test]
fn wait_times_out_and_rejects_bad_requests() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::NeedsInput);

    // Nothing answers the prompt, so the session never gets done.
    let started = Instant::now();
    let outcome = client.call(wait(id, &[SessionState::Done], Some(300)));
    assert_eq!(error_code(outcome), ErrorCode::Timeout);
    assert!(started.elapsed() < Duration::from_secs(5));
    // Already there: answers at once.
    let now: SessionWaitResult = client
        .call(wait(id, &[SessionState::NeedsInput], Some(300)))
        .into_result()
        .unwrap();
    assert_eq!(now.state, SessionState::NeedsInput);

    assert_eq!(
        error_code(client.call(wait(id, &[], None))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(wait(Ulid::new(), &[SessionState::Idle], None))),
        ErrorCode::NotFound
    );
    assert_eq!(
        error_code(client.call(send(Ulid::new(), "y"))),
        ErrorCode::NotFound
    );
    client.stop();
}

fn spawn_params(project: &Project, kind: SessionKind, tab_id: Option<Ulid>) -> Request {
    Request::SessionSpawn(SessionSpawnParams {
        project_id: project.id,
        agent_profile: None,
        kind,
        tab_id,
        title: None,
        cols: 80,
        rows: 24,
    })
}

fn set_default_agent(project: &Project, agent: Option<&str>) -> Request {
    Request::ProjectUpdate(ProjectUpdateParams {
        project_id: project.id,
        name: None,
        default_agent: Some(agent.map(str::to_owned)),
    })
}

fn create_tab(project: &Project, title: &str) -> Request {
    Request::TabCreate(TabCreateParams {
        project_id: project.id,
        title: title.into(),
        position: None,
    })
}

fn list_tabs(client: &mut Client, project: &Project) -> Vec<TerminalTab> {
    client
        .call(Request::TabList(TabListParams {
            project_id: project.id,
        }))
        .into_result()
        .unwrap()
}

#[test]
fn tabs_and_layout_persist_across_daemon_restart() {
    let env = Env::new();
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());

    let first: TerminalTab = client
        .call(create_tab(&project, "one"))
        .into_result()
        .unwrap();
    let second: TerminalTab = client
        .call(create_tab(&project, "two"))
        .into_result()
        .unwrap();
    let doomed: TerminalTab = client
        .call(create_tab(&project, "doomed"))
        .into_result()
        .unwrap();
    assert_eq!(first.layout, serde_json::Value::Null);
    assert!(first.position < second.position && second.position < doomed.position);
    client.wait_event(&Event::TabUpdated(first.clone()));

    let renamed: TerminalTab = client
        .call(Request::TabRename(TabRenameParams {
            tab_id: first.id,
            title: "uno".into(),
        }))
        .into_result()
        .unwrap();
    client
        .call(Request::TabMove(TabMoveParams {
            tab_id: renamed.id,
            position: second.position + 0.5,
        }))
        .into_result::<TerminalTab>()
        .unwrap();
    let layout = serde_json::json!({"split": "vertical", "ratio": 0.5, "children": [
        {"session": Ulid::new().to_string()},
        {"split": "horizontal", "ratio": 0.6, "children": [
            {"session": Ulid::new().to_string()}, {"session": Ulid::new().to_string()}
        ]}
    ]});
    client
        .call(Request::TabSetLayout(TabSetLayoutParams {
            tab_id: second.id,
            layout: layout.clone(),
        }))
        .into_result::<()>()
        .unwrap();
    client
        .call(Request::TabClose(TabRef { tab_id: doomed.id }))
        .into_result::<()>()
        .unwrap();
    let before = list_tabs(&mut client, &project);
    let titles: Vec<&str> = before.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, ["two", "uno"]);
    assert_eq!(before[0].layout, layout);
    client.wait_event(&Event::TabUpdated(before[0].clone()));

    let unknown = TabRef {
        tab_id: Ulid::new(),
    };
    assert_eq!(
        error_code(client.call(Request::TabClose(unknown))),
        ErrorCode::NotFound
    );
    let orphan = Request::TabCreate(TabCreateParams {
        project_id: Ulid::new(),
        title: "x".into(),
        position: None,
    });
    assert_eq!(error_code(client.call(orphan)), ErrorCode::NotFound);

    client.stop();
    assert!(daemon.wait_exit().success());
    let (_daemon, mut client) = env.start();
    assert_eq!(list_tabs(&mut client, &project), before);
    client.stop();
}

#[test]
fn spawn_defaults_to_the_project_agent_or_the_shell_and_fills_an_empty_tab() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    // Adding is announced too, so every UI shows the new workspace.
    client.wait_event(&Event::ProjectUpdated(project.clone()));

    // No default agent yet: the client must name one.
    assert_eq!(
        error_code(client.call(spawn_params(&project, SessionKind::Agent, None))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(set_default_agent(&project, Some("shell")))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(set_default_agent(&project, Some("nope")))),
        ErrorCode::NotFound
    );
    let updated: Project = client
        .call(set_default_agent(&project, Some("fake")))
        .into_result()
        .unwrap();
    assert_eq!(updated.default_agent.as_deref(), Some("fake"));
    client.wait_event(&Event::ProjectUpdated(updated.clone()));
    let listed: Vec<Project> = client.call(Request::ProjectList).into_result().unwrap();
    assert_eq!(listed, vec![updated]);

    // The first pane of a new tab becomes its layout; later panes are the client's job.
    let tab: TerminalTab = client
        .call(create_tab(&project, "agents"))
        .into_result()
        .unwrap();
    let agent: SessionSpawnResult = client
        .call(spawn_params(&project, SessionKind::Agent, Some(tab.id)))
        .into_result()
        .unwrap();
    let session = client.session(agent.session_id);
    assert_eq!(session.agent_profile, "fake");
    assert_eq!(session.kind, SessionKind::Agent);
    let leaf = serde_json::json!({"session": agent.session_id.to_string()});
    let placed = TerminalTab {
        layout: leaf.clone(),
        ..tab.clone()
    };
    client.wait_event(&Event::TabUpdated(placed));

    let shell: SessionSpawnResult = client
        .call(spawn_params(&project, SessionKind::Shell, Some(tab.id)))
        .into_result()
        .unwrap();
    let session = client.session(shell.session_id);
    assert_eq!(session.agent_profile, "shell");
    assert_eq!(session.kind, SessionKind::Shell);
    assert_eq!(list_tabs(&mut client, &project)[0].layout, leaf);

    // A tab of another workspace is refused; clearing the default works.
    let other_repo = env.dir.path().join("other");
    std::fs::create_dir(&other_repo).unwrap();
    let other = client.add_project(&other_repo);
    assert_eq!(
        error_code(client.call(spawn_params(&other, SessionKind::Shell, Some(tab.id)))),
        ErrorCode::InvalidRequest
    );
    let cleared: Project = client
        .call(set_default_agent(&project, None))
        .into_result()
        .unwrap();
    assert_eq!(cleared.default_agent, None);

    for session_id in [agent.session_id, shell.session_id] {
        let kill = SessionKillParams {
            session_id,
            force: true,
        };
        client
            .call(Request::SessionKill(kill))
            .into_result::<()>()
            .unwrap();
        client.wait_state(session_id, SessionState::Exited);
    }
    client.stop();
}

fn session_log(env: &Env, id: Ulid) -> String {
    let path = env.data_dir().join("sessions").join(format!("{id}.log"));
    String::from_utf8_lossy(&std::fs::read(path).unwrap()).into_owned()
}
