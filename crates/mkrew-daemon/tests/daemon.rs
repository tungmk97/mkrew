//! Runs the real `mkrew-daemon` binary in a temp dir and talks to it over the local socket.

// Helpers outside `#[test]` fns are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use std::collections::{BTreeSet, HashMap};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use interprocess::local_socket::{Stream, prelude::*};
use mkrew_daemon::EXIT_ALREADY_RUNNING;
use mkrew_protocol::{
    AgentProfilesResult, ClientKind, CommentCreateParams, CommentId, CommentListParams, CommentRef,
    CommentReplyParams, CommentSendParams, CommentStatus, Commit, CommitDetail, DaemonStatusResult,
    DocChangedEvent, DocComment, DocContent, DocFile, DocListParams, DocReadParams, ErrorCode,
    Event, FileDiff, Frame, FrameDecoder, GitChangedEvent, GitCommitParams, GitCommitResult,
    GitDiffParams, GitDiscardParams, GitLogParams, GitMode, GitSetBaseParams, GitShowParams,
    GitStageParams, GitStatus, GitStatusParams, HelloParams, HelloResult, HookReportParams,
    HunkTarget, Message, Outcome, PROTOCOL_VERSION, Project, ProjectAddParams, ProjectRef,
    ProjectUpdateParams, Request, ResponseError, ScreenSnapshot, Session, SessionAttachResult,
    SessionExitedEvent, SessionInputParams, SessionKillParams, SessionKind, SessionListParams,
    SessionRef, SessionResizeParams, SessionSendParams, SessionSpawnParams, SessionSpawnResult,
    SessionState, SessionWaitParams, SessionWaitResult, StateSource, TabCreateParams,
    TabListParams, TabMoveParams, TabRef, TabRenameParams, TabSetLayoutParams, Task,
    TaskCreateParams, TaskDeletedEvent, TaskListParams, TaskMoveParams, TaskRef, TaskRunParams,
    TaskStatus, TaskUndoParams, TaskUpdateParams, TerminalTab, UndoPreview, encode,
};
use mkrew_store::Store;
use serde_json::{Value, json};
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

    /// The daemon writes notifications here (JSON lines) instead of showing toasts.
    fn notifications(&self) -> PathBuf {
        self.dir.path().join("notifications.jsonl")
    }

    /// Waits until a notification for `session_id` was written; returns it.
    fn wait_notification(&self, session_id: Ulid) -> Value {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let text = std::fs::read_to_string(self.notifications()).unwrap_or_default();
            let found = text
                .lines()
                .map(|l| serde_json::from_str::<Value>(l).unwrap())
                .find(|n| n["session_id"] == session_id.to_string());
            if let Some(notice) = found {
                return notice;
            }
            assert!(Instant::now() < deadline, "no notification: {text}");
            std::thread::sleep(Duration::from_millis(20));
        }
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
        let fake = built_bin("mkrew-fakeagent");
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

    /// Like [`Env::write_fake_profile`], saved as `<id>.toml` with `prompt_mode = <mode>`.
    fn write_fake_profile_with_prompt(&self, id: &str, mode: &str) {
        self.write_fake_profile_as(id, "[]");
        let path = self.dir.path().join(format!("config/agents/{id}.toml"));
        let text = std::fs::read_to_string(&path).unwrap();
        let changed = text.replace(
            "prompt_mode = \"none\"",
            &format!("prompt_mode = \"{mode}\""),
        );
        assert_ne!(changed, text);
        std::fs::write(path, changed).unwrap();
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
            .env(mkrew_daemon::ENV_NOTIFY, self.notifications())
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
        self.wait_event_where(|e| e == event);
    }

    /// Makes calls until a pushed event matching `wanted` has arrived; returns it.
    fn wait_event_where(&mut self, wanted: impl Fn(&Event) -> bool) -> Event {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(event) = self.events.iter().find(|e| wanted(e)) {
                return event.clone();
            }
            assert!(
                Instant::now() < deadline,
                "no matching event in {:?}",
                self.events
            );
            std::thread::sleep(Duration::from_millis(20));
            self.call(Request::DaemonStatus);
        }
    }

    fn hook(&mut self, session_id: Ulid, agent: &str, event: &str, payload: Value) -> Outcome {
        self.call(Request::HookReport(HookReportParams {
            mkrew_session_id: session_id,
            agent: agent.into(),
            event: event.into(),
            payload,
        }))
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

/// A workspace binary next to `mkrew-daemon` (`cargo test --workspace` builds them all).
fn built_bin(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_BIN_EXE_mkrew-daemon"))
        .with_file_name(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    assert!(
        path.is_file(),
        "{} is missing; run `cargo build --workspace`",
        path.display()
    );
    path
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

/// Waits for the `comment.updated` of `id` with `status`; returns it.
fn wait_comment(client: &mut Client, id: CommentId, status: CommentStatus) -> DocComment {
    let event = client.wait_event_where(
        |e| matches!(e, Event::CommentUpdated(c) if c.id == id && c.status == status),
    );
    match event {
        Event::CommentUpdated(c) => *c,
        _ => unreachable!(),
    }
}

/// Threads: create, reply, resolve, reopen; an agent reply (via its session) addresses the
/// thread, a user reply on an addressed thread reopens it. Every change is broadcast.
#[test]
fn comment_threads_reply_resolve_reopen_and_agent_replies_address_them() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let session: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();

    let create = |file: &str, start_line, end_line, body: &str| {
        Request::CommentCreate(CommentCreateParams {
            project_id: project.id,
            file_path: file.into(),
            start_line,
            end_line,
            quote: "Use Redis".into(),
            body: body.into(),
        })
    };
    for bad in [
        create("docs/plan.md", 0, 1, "why?"),
        create("docs/plan.md", 3, 2, "why?"),
        create("docs/plan.md", 1, 1, " "),
        create(" ", 1, 1, "why?"),
    ] {
        assert_eq!(error_code(client.call(bad)), ErrorCode::InvalidRequest);
    }
    let root: DocComment = client
        .call(create(r"docs\plan.md", 12, 14, "Why not in-memory?"))
        .into_result()
        .unwrap();
    assert_eq!(
        (root.file_path.as_str(), root.status, root.author.as_str()),
        ("docs/plan.md", CommentStatus::Open, DocComment::USER)
    );
    wait_comment(&mut client, root.id, CommentStatus::Open);
    let other: DocComment = client
        .call(create("README.md", 1, 1, "Typo"))
        .into_result()
        .unwrap();

    let reply = |id, body: &str, session| {
        Request::CommentReply(CommentReplyParams {
            comment_id: id,
            body: body.into(),
            mkrew_session_id: session,
        })
    };
    let user: DocComment = client
        .call(reply(root.id, "Also check TTLs", None))
        .into_result()
        .unwrap();
    assert_eq!(user.parent_id, Some(root.id));
    assert_eq!(user.author, DocComment::USER);

    // The agent answers (naming the user reply joins the same thread): addressed.
    let agent: DocComment = client
        .call(reply(
            user.id,
            "Switched to in-memory",
            Some(session.session_id),
        ))
        .into_result()
        .unwrap();
    assert_eq!(agent.parent_id, Some(root.id));
    assert_eq!(agent.author, "fake");
    wait_comment(&mut client, root.id, CommentStatus::Addressed);

    let list = |client: &mut Client, file: Option<&str>| -> Vec<DocComment> {
        client
            .call(Request::CommentList(CommentListParams {
                project_id: project.id,
                file_path: file.map(Into::into),
            }))
            .into_result()
            .unwrap()
    };
    // The user answers back: open again.
    client
        .call(reply(root.id, "Still too big", None))
        .into_result::<DocComment>()
        .unwrap();
    assert_eq!(list(&mut client, None)[0].status, CommentStatus::Open);

    // Resolve / reopen act on the thread root, also when given a reply id.
    let resolved: DocComment = client
        .call(Request::CommentResolve(CommentRef {
            comment_id: user.id,
        }))
        .into_result()
        .unwrap();
    assert_eq!(
        (resolved.id, resolved.status),
        (root.id, CommentStatus::Resolved)
    );
    let reopened: DocComment = client
        .call(Request::CommentReopen(CommentRef {
            comment_id: root.id,
        }))
        .into_result()
        .unwrap();
    assert_eq!(reopened.status, CommentStatus::Open);

    let thread = list(&mut client, Some(r"docs\plan.md"));
    let bodies: Vec<_> = thread.iter().map(|c| c.body.as_str()).collect();
    assert_eq!(
        bodies,
        [
            "Why not in-memory?",
            "Also check TTLs",
            "Switched to in-memory",
            "Still too big"
        ]
    );
    assert_eq!(thread[0].status, CommentStatus::Open);
    assert_eq!(list(&mut client, None).len(), 5);
    assert!(
        list(&mut client, Some("README.md"))
            .iter()
            .all(|c| c.id == other.id)
    );

    let missing = CommentId::new();
    assert_eq!(
        error_code(client.call(reply(missing, "hi", None))),
        ErrorCode::NotFound
    );
    assert_eq!(
        error_code(client.call(reply(root.id, "hi", Some(Ulid::new())))),
        ErrorCode::NotFound
    );
    assert_eq!(
        error_code(client.call(reply(root.id, "", None))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(Request::CommentResolve(CommentRef {
            comment_id: missing
        }))),
        ErrorCode::NotFound
    );
    client.stop();
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
                state_source: StateSource::Screen,
                state_changed_at: 1,
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
    // Startup also writes today's DB backup (SPEC §8).
    let backups = env.data_dir().join("backups");
    let deadline = Instant::now() + Duration::from_secs(10);
    let backup = loop {
        let found = std::fs::read_dir(&backups).ok().and_then(|entries| {
            entries
                .map(|e| e.unwrap().path())
                .find(|p| p.extension().is_some_and(|x| x == "db"))
        });
        if let Some(path) = found {
            break path;
        }
        assert!(
            Instant::now() < deadline,
            "no backup in {}",
            backups.display()
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    client
        .call(Request::DaemonStop)
        .into_result::<()>()
        .unwrap();
    assert!(daemon.wait_exit().success());

    let sessions = Store::open(&db).unwrap().list_sessions(None).unwrap();
    assert_eq!(sessions[0].id, session_id);
    assert_eq!(sessions[0].state, SessionState::Exited);
    assert!(env.data_dir().join("logs").read_dir().unwrap().count() > 0);
    let backed_up = Store::open(&backup).unwrap().list_sessions(None).unwrap();
    assert_eq!(backed_up[0].id, session_id);
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
    let event = client.wait_event_where(|e| {
        matches!(e, Event::SessionState(s) if s.session_id == id && s.state == SessionState::NeedsInput)
    });
    let Event::SessionState(event) = event else {
        unreachable!()
    };
    // Found by the screen pattern; the event and the stored row agree.
    assert_eq!(event.source, StateSource::Screen);
    assert_eq!(session.state_source, StateSource::Screen);
    assert_eq!(event.changed_at, session.state_changed_at);
    assert!(event.changed_at >= session.started_at);
    // Entering needs_input notifies (SPEC §8).
    let notice = env.wait_notification(id);
    assert_eq!(notice["project_id"], project.id.to_string());
    assert_eq!(notice["body"], "Needs input");
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
fn hook_reports_drive_the_state_of_live_sessions_only() {
    let env = Env::new();
    env.write_fake_profile();
    // The fake profile with hooks enabled.
    let path = env
        .dir
        .path()
        .join("config")
        .join("agents")
        .join("fake.toml");
    let fake = std::fs::read_to_string(&path).unwrap();
    let hooked = fake.replace("mode = \"screen\"", "mode = \"hook+screen\"");
    assert_ne!(hooked, fake);
    env.write_profile("hooked", &hooked);
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "hooked").into_result().unwrap();
    let id = spawned.session_id;
    let started = client.session(id);
    assert_eq!(started.state_source, StateSource::Daemon);
    assert_eq!(started.state_changed_at, started.started_at);

    // Only a live session may report, and only for a known agent.
    let unknown = client.hook(Ulid::new(), "fake", "busy", Value::Null);
    assert_eq!(error_code(unknown), ErrorCode::NotFound);
    let omp = client.hook(id, "omp", "busy", Value::Null);
    assert_eq!(error_code(omp), ErrorCode::InvalidRequest);

    // needs_input first: the fake agent's first output may already have made it `working`.
    let native = json!({"session_id": "native-1"});
    client
        .hook(id, "fake", "needs_input", native)
        .into_result::<()>()
        .unwrap();
    let session = client.wait_state(id, SessionState::NeedsInput);
    assert_eq!(session.state_source, StateSource::Hook);
    assert_eq!(session.native_session_id.as_deref(), Some("native-1"));
    client.wait_event_where(|e| {
        matches!(e, Event::SessionState(s)
            if s.session_id == id && s.state == SessionState::NeedsInput
                && s.source == StateSource::Hook && s.changed_at == session.state_changed_at)
    });
    client
        .hook(id, "fake", "busy", Value::Null)
        .into_result::<()>()
        .unwrap();
    let session = client.wait_state(id, SessionState::Working);
    assert_eq!(session.state_source, StateSource::Hook);
    // A known agent's unknown event is recorded but changes nothing.
    client
        .hook(id, "fake", "thinking", Value::Null)
        .into_result::<()>()
        .unwrap();
    client
        .hook(id, "fake", "done", Value::Null)
        .into_result::<()>()
        .unwrap();
    let session = client.wait_state(id, SessionState::Done);
    assert_eq!(session.state_source, StateSource::Hook);
    // Hook events take priority: the fake agent's own (y/n) prompt does not override `done`.
    std::thread::sleep(Duration::from_millis(1_500));
    assert_eq!(client.session(id).state, SessionState::Done);

    let db = env.data_dir().join("mkrew.db");
    let events = Store::open(&db)
        .unwrap()
        .session_events(id, "hook")
        .unwrap();
    let names: Vec<&str> = events
        .iter()
        .map(|e| e.payload["event"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["needs_input", "busy", "thinking", "done"]);
    assert_eq!(events[0].payload["agent"], "fake");
    assert_eq!(events[0].payload["payload"]["session_id"], "native-1");

    // Exited sessions are rejected; the exit itself comes from the daemon.
    let kill = Request::SessionKill(SessionKillParams {
        session_id: id,
        force: true,
    });
    client.call(kill).into_result::<()>().unwrap();
    let session = client.wait_state(id, SessionState::Exited);
    assert_eq!(session.state_source, StateSource::Daemon);
    assert_eq!(session.native_session_id.as_deref(), Some("native-1"));
    let late = client.hook(id, "fake", "busy", Value::Null);
    assert_eq!(error_code(late), ErrorCode::InvalidState);
    client.stop();
}

#[test]
fn screen_only_profiles_record_hooks_but_ignore_them() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    client
        .hook(id, "fake", "done", json!({"session_id": "n"}))
        .into_result::<()>()
        .unwrap();
    // The screen still decides: the fake agent's prompt makes it needs_input, not done.
    let session = client.wait_state(id, SessionState::NeedsInput);
    assert_eq!(session.state_source, StateSource::Screen);
    assert_eq!(session.native_session_id.as_deref(), Some("n"));
    let db = env.data_dir().join("mkrew.db");
    let events = Store::open(&db)
        .unwrap()
        .session_events(id, "hook")
        .unwrap();
    assert_eq!(events.len(), 1);
    client.stop();
}

/// Screen mode: the fake agent works, asks, works again, then goes quiet.
const SCREEN_SCENARIO: &str = r#"
[[step]]
action = "print"
text = "Reading files"
[[step]]
action = "spinner"
text = "Planning"
ms = 600
[[step]]
action = "ask"
[[step]]
action = "spinner"
text = "Applying"
ms = 600
[[step]]
action = "print"
text = "one"
[[step]]
action = "print"
text = "two"
[[step]]
action = "print"
text = "three"
[[step]]
action = "print"
text = "four"
[[step]]
action = "sleep"
ms = 3500
"#;

/// Hook+screen mode: `mk hook fake …` drives needs_input/working/done; idle comes from the
/// screen once the 10 s hook-priority window has passed (SPEC §6).
const HOOK_SCENARIO: &str = r#"
[[step]]
action = "print"
text = "Reading files"
[[step]]
action = "sleep"
ms = 300
[[step]]
action = "hook"
event = "needs_input"
[[step]]
action = "ask"
text = "Continue?"
[[step]]
action = "hook"
event = "busy"
[[step]]
action = "sleep"
ms = 12000
[[step]]
action = "hook"
event = "done"
[[step]]
action = "sleep"
ms = 300
"#;

/// Records every `session.state` event of `ids` until all have exited, typing `y⏎` into a
/// session the first time it needs input.
fn record_states(
    client: &mut Client,
    ids: &[Ulid],
) -> HashMap<Ulid, Vec<(SessionState, StateSource)>> {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut states: HashMap<Ulid, Vec<_>> = ids.iter().map(|id| (*id, Vec::new())).collect();
    let mut answered = Vec::new();
    let mut seen = 0;
    loop {
        let new = client.events[seen..].to_vec();
        seen = client.events.len();
        for event in new {
            let Event::SessionState(s) = event else {
                continue;
            };
            let Some(list) = states.get_mut(&s.session_id) else {
                continue;
            };
            list.push((s.state, s.source));
            if s.state == SessionState::NeedsInput && !answered.contains(&s.session_id) {
                answered.push(s.session_id);
                client
                    .input(s.session_id, b"y\r")
                    .into_result::<()>()
                    .unwrap();
            }
        }
        let exited = |list: &Vec<(SessionState, StateSource)>| {
            list.last().is_some_and(|(s, _)| *s == SessionState::Exited)
        };
        if states.values().all(exited) {
            return states;
        }
        assert!(Instant::now() < deadline, "sessions still live: {states:?}");
        std::thread::sleep(Duration::from_millis(20));
        client.call(Request::DaemonStatus);
    }
}

/// The M3 e2e: every state transition, as `session.state` events, for a screen-mode agent,
/// a hooked agent and an omp-like agent, all running at once in one daemon.
#[test]
fn fake_agents_go_through_every_state_transition() {
    use SessionState::*;
    use StateSource::{Daemon as D, Hook as H, Screen as S};

    let env = Env::new();
    // `mk` next to the fake agent: its `hook` steps run `mk hook fake <event>`.
    built_bin("mk");
    for (name, scenario) in [("screen", SCREEN_SCENARIO), ("hooks", HOOK_SCENARIO)] {
        let path = env.dir.path().join(format!("{name}.toml"));
        std::fs::write(&path, scenario).unwrap();
        env.write_fake_profile_as(name, &format!("['--scenario', '{}']", path.display()));
    }
    let profiles = env.dir.path().join("config").join("agents");
    // Agent start-up under a loaded test run (and a first `mk hook` run that the virus scanner
    // holds up for seconds) must not look like a quiet (idle) period.
    let idle_after = |text: String, ms: &str| {
        let changed = text.replace("idle_after_ms = 1000", &format!("idle_after_ms = {ms}"));
        assert_ne!(changed, text);
        changed
    };
    let screen = std::fs::read_to_string(profiles.join("screen.toml")).unwrap();
    env.write_profile("screen", &idle_after(screen, "2000"));
    // Idle only shows after the 10 s hook window anyway, so a long quiet period costs nothing.
    let hooks = std::fs::read_to_string(profiles.join("hooks.toml")).unwrap();
    let hooks = idle_after(hooks, "8000").replace("mode = \"screen\"", "mode = \"hook+screen\"");
    assert!(hooks.contains("hook+screen"), "{hooks}");
    env.write_profile("hooks", &hooks);
    // The shipped omp profile, running the fake agent's `omp` scenario.
    let omp = include_str!("../../../agents/omp.toml").replace(
        "command = \"omp\"",
        &format!(
            "command = '{}'\nargs = ['--scenario', 'omp']",
            built_bin("mkrew-fakeagent").display()
        ),
    );
    assert!(omp.contains("--scenario"), "{omp}");
    env.write_profile("omp", &omp);

    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let ids: Vec<Ulid> = ["screen", "hooks", "omp"]
        .iter()
        .map(|profile| {
            let spawned: SessionSpawnResult =
                client.spawn(&project, profile).into_result().unwrap();
            spawned.session_id
        })
        .collect();
    // Sessions are created `starting` (no event); `client.events` holds every change since
    // the spawn, so the first recorded event is the transition out of `starting`.
    let states = record_states(&mut client, &ids);

    // Screen mode never reports done: only a hook can say a turn finished.
    assert_eq!(
        states[&ids[0]],
        [
            (Working, S),
            (NeedsInput, S),
            (Working, S),
            (Idle, S),
            (Exited, D)
        ]
    );
    assert_eq!(
        states[&ids[1]],
        [
            (Working, S),
            (NeedsInput, H),
            (Working, H),
            (Idle, S),
            (Done, H),
            (Exited, D)
        ]
    );
    // omp: spinner redraws are working, each quiet gap is idle; no patterns, no hooks.
    assert_eq!(
        states[&ids[2]],
        [
            (Working, S),
            (Idle, S),
            (Working, S),
            (Idle, S),
            (Exited, D)
        ]
    );
    for id in &ids {
        assert_eq!(client.session(*id).exit_code, Some(0));
    }
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

/// Every path recorded so far in the session's `files_changed` events.
fn changed_files(env: &Env, id: Ulid) -> BTreeSet<String> {
    let db = env.data_dir().join("mkrew.db");
    let events = Store::open(&db)
        .unwrap()
        .session_events(id, "files_changed")
        .unwrap();
    events
        .iter()
        .flat_map(|e| e.payload["paths"].as_array().unwrap().clone())
        .map(|p| p.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn file_watcher_records_changed_files_of_live_sessions_except_ignored_ones() {
    let env = Env::new();
    let repo = env.repo();
    std::fs::write(repo.join(".gitignore"), "*.log\n").unwrap();
    for dir in ["src", "target", "node_modules"] {
        std::fs::create_dir_all(repo.join(dir)).unwrap();
    }
    // Ignored writes first: once `src/lib.rs` is recorded, their batch has been handled too.
    let scenario = env.dir.path().join("writer.toml");
    let mut steps = String::new();
    for path in ["target/a.o", "node_modules/b.js", "debug.log", "src/lib.rs"] {
        steps +=
            &format!("[[step]]\naction = \"write_file\"\npath = \"{path}\"\ncontent = \"x\"\n\n");
    }
    std::fs::write(&scenario, steps + "[[step]]\naction = \"ask\"\n").unwrap();
    env.write_fake_profile_as(
        "writer",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let spawned: SessionSpawnResult = client.spawn(&project, "writer").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::NeedsInput);

    let deadline = Instant::now() + Duration::from_secs(10);
    while !changed_files(&env, id).contains("src/lib.rs") {
        assert!(Instant::now() < deadline, "src/lib.rs never recorded");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        changed_files(&env, id),
        BTreeSet::from(["src/lib.rs".to_owned()])
    );
    client.stop();
}

fn doc_list(client: &mut Client, project: &Project) -> Vec<DocFile> {
    let params = DocListParams {
        project_id: project.id,
    };
    client.call(Request::DocList(params)).into_result().unwrap()
}

fn doc_read(project: &Project, file: &str) -> Request {
    Request::DocRead(DocReadParams {
        project_id: project.id,
        file_path: file.into(),
    })
}

/// Waits for the `doc.changed` of `file` with `hash`.
fn wait_doc_changed(client: &mut Client, project: &Project, file: &str, hash: Option<String>) {
    client.wait_event(&Event::DocChanged(DocChangedEvent {
        project_id: project.id,
        file_path: file.into(),
        hash,
    }));
}

/// `doc.list` = review globs + Markdown an agent just wrote (with its session) + open-comment
/// counts; `doc.read`; after the session exited the docs stay watched, so an external edit
/// sends `doc.changed` and re-anchors the comments (moved / outdated), and so does deleting.
#[test]
fn docs_are_listed_read_and_external_edits_reanchor_comments() {
    let env = Env::new();
    let repo = env.repo();
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    for dir in ["src", "notes", ".claude/plans"] {
        std::fs::create_dir_all(repo.join(dir)).unwrap();
    }
    std::fs::write(repo.join(".gitignore"), ".claude/\n").unwrap();
    std::fs::write(repo.join(".claude/plans/p.md"), "# P\n").unwrap();
    std::fs::write(repo.join("README.md"), "# Readme\n").unwrap();
    std::fs::write(repo.join("src/inner.md"), "not reviewed\n").unwrap();
    let plan = "# Plan\n\nUse Redis for caching.\n\nM3: deploy\n";
    std::fs::write(repo.join("docs/plan.md"), plan).unwrap();
    let scenario = env.dir.path().join("writer.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"write_file\"\npath = \"notes/todo.md\"\ncontent = \"- [ ] a\"\n\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    env.write_fake_profile_as(
        "writer",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);

    let files = doc_list(&mut client, &project);
    let paths: Vec<&str> = files.iter().map(|f| f.file_path.as_str()).collect();
    assert_eq!(paths, [".claude/plans/p.md", "README.md", "docs/plan.md"]);
    let read: DocContent = client
        .call(doc_read(&project, r"docs\plan.md"))
        .into_result()
        .unwrap();
    assert_eq!(
        (read.file_path.as_str(), read.content.as_str()),
        ("docs/plan.md", plan)
    );
    let missing = client.call(doc_read(&project, "docs/nope.md"));
    assert_eq!(error_code(missing), ErrorCode::NotFound);
    for bad in ["../outside.md", "", "/abs.md"] {
        let outcome = client.call(doc_read(&project, bad));
        assert_eq!(error_code(outcome), ErrorCode::InvalidRequest, "{bad}");
    }

    let mut comment = |start_line, quote: &str| {
        let params = CommentCreateParams {
            project_id: project.id,
            file_path: "docs/plan.md".into(),
            start_line,
            end_line: start_line,
            quote: quote.into(),
            body: "why?".into(),
        };
        let created: DocComment = client
            .call(Request::CommentCreate(params))
            .into_result()
            .unwrap();
        created
    };
    let redis = comment(3, "Use Redis");
    let deploy = comment(5, "M3: deploy");

    // An agent writes a Markdown file outside the globs: listed with its session.
    let spawned: SessionSpawnResult = client.spawn(&project, "writer").into_result().unwrap();
    let session = spawned.session_id;
    client.wait_state(session, SessionState::NeedsInput);
    let deadline = Instant::now() + TIMEOUT;
    let todo = loop {
        let files = doc_list(&mut client, &project);
        if let Some(todo) = files.iter().find(|f| f.file_path == "notes/todo.md") {
            break todo.clone();
        }
        assert!(
            Instant::now() < deadline,
            "notes/todo.md never listed: {files:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(todo.last_session_id, Some(session));
    assert!(todo.last_changed_at.is_some());
    let files = doc_list(&mut client, &project);
    let plan_file = files
        .iter()
        .find(|f| f.file_path == "docs/plan.md")
        .unwrap();
    assert_eq!(plan_file.open_comments, 2);
    let kill = SessionKillParams {
        session_id: session,
        force: true,
    };
    client.call(Request::SessionKill(kill));
    client.wait_state(session, SessionState::Exited);

    // An external edit (no session live): the passage moved, the other one is gone.
    client.events.clear();
    let edited = "# Plan\n\nIntro.\n\nUse Redis for caching.\n";
    std::fs::write(repo.join("docs/plan.md"), edited).unwrap();
    let moved = client.wait_event_where(
        |e| matches!(e, Event::CommentUpdated(c) if c.id == redis.id && c.start_line == Some(5)),
    );
    assert!(matches!(moved, Event::CommentUpdated(c) if c.status == CommentStatus::Open));
    let gone = wait_comment(&mut client, deploy.id, CommentStatus::Outdated);
    assert_eq!(gone.start_line, Some(5));
    // The watcher announced the final content (a write may show up in steps first).
    let read: DocContent = client
        .call(doc_read(&project, "docs/plan.md"))
        .into_result()
        .unwrap();
    let last_hash = client.events.iter().rev().find_map(|e| match e {
        Event::DocChanged(c) if c.file_path == "docs/plan.md" => Some(c.hash.clone()),
        _ => None,
    });
    assert_eq!(last_hash, Some(Some(read.hash)));
    // A git-ignored plan is listed and followed too.
    std::fs::write(repo.join(".claude/plans/p.md"), "# P\n\nedited\n").unwrap();
    client.wait_event_where(
        |e| matches!(e, Event::DocChanged(c) if c.file_path == ".claude/plans/p.md"),
    );

    // Deleting the file outdates the rest; nothing is deleted.
    client.events.clear();
    std::fs::remove_file(repo.join("docs/plan.md")).unwrap();
    wait_doc_changed(&mut client, &project, "docs/plan.md", None);
    wait_comment(&mut client, redis.id, CommentStatus::Outdated);
    let comments: Vec<DocComment> = client
        .call(Request::CommentList(CommentListParams {
            project_id: project.id,
            file_path: None,
        }))
        .into_result()
        .unwrap();
    assert_eq!(comments.len(), 2);
    client.stop();
}

#[test]
fn two_sessions_editing_one_file_raise_a_conflict() {
    let env = Env::new();
    let repo = env.repo();
    // The first spinner lets the project watcher start (it starts after the first spawn); the
    // second keeps the session `working` until the batch landed (≤ `watch::MAX_DELAY` = 2 s).
    let scenario = env.dir.path().join("writer.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"spinner\"\ntext = \"reading\"\nms = 500\n\n\
         [[step]]\naction = \"write_file\"\npath = \"shared.txt\"\ncontent = \"x\"\n\n\
         [[step]]\naction = \"spinner\"\ntext = \"thinking\"\nms = 3000\n\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    env.write_fake_profile_as(
        "writer",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let run = |client: &mut Client, title: &str| {
        let task: Task = client
            .call(create_task(&project, title, Some("writer")))
            .into_result()
            .unwrap();
        let spawned: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
        client.wait_state(spawned.session_id, SessionState::NeedsInput);
        (task.id, spawned.session_id)
    };
    // One after the other: each change has exactly one `working` session to blame.
    let (task_a, a) = run(&mut client, "A");
    let (task_b, b) = run(&mut client, "B");

    let conflict = |path: &'static str| move |e: &Event| matches!(e, Event::ConflictDetected(c) if c.path == path);
    let Event::ConflictDetected(found) = client.wait_event_where(conflict("shared.txt")) else {
        unreachable!()
    };
    let mut sessions = vec![a, b];
    sessions.sort();
    let mut tasks = vec![task_a, task_b];
    tasks.sort();
    assert_eq!(found.project_id, project.id);
    assert_eq!(found.session_ids, sessions);
    assert_eq!(found.task_ids, tasks);

    // Paths named in hook payloads count too (absolute or relative to the project).
    let edit = |path: String| json!({"tool_input": {"file_path": path}});
    let absolute = repo.join("src").join("hooked.rs").display().to_string();
    let reported = client.hook(a, "fake", "PostToolUse", edit(absolute));
    reported.into_result::<()>().unwrap();
    let reported = client.hook(b, "fake", "PostToolUse", edit("src/hooked.rs".into()));
    reported.into_result::<()>().unwrap();
    let Event::ConflictDetected(found) = client.wait_event_where(conflict("src/hooked.rs")) else {
        unreachable!()
    };
    assert_eq!(found.session_ids, sessions);
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

fn send_comments(comment_ids: &[CommentId], session_id: Ulid) -> Request {
    Request::CommentSendToAgent(CommentSendParams {
        comment_ids: comment_ids.to_vec(),
        session_id,
    })
}

/// Comments sent while the agent works are marked `sent` at once (snapshot = file content)
/// and typed only once it asks; reply ids stand for their thread; bad targets are refused.
#[test]
fn comments_sent_to_a_working_agent_are_typed_once_it_asks() {
    let env = Env::new();
    env.write_fake_profile();
    let repo = env.repo();
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    let plan = "# Plan\n\nUse Redis for caching.\n\nM3: deploy\n";
    std::fs::write(repo.join("docs/plan.md"), plan).unwrap();
    std::fs::write(repo.join("README.md"), "# Readme\n").unwrap();
    let other_repo = env.dir.path().join("other");
    std::fs::create_dir_all(&other_repo).unwrap();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let other_project = client.add_project(&other_repo);

    let mut create = |file: &str, line, quote: &str, body: &str| {
        let params = CommentCreateParams {
            project_id: project.id,
            file_path: file.into(),
            start_line: line,
            end_line: line,
            quote: quote.into(),
            body: body.into(),
        };
        let created: DocComment = client
            .call(Request::CommentCreate(params))
            .into_result()
            .unwrap();
        created
    };
    let redis = create("docs/plan.md", 3, "Use Redis", "Why not in-memory?");
    let deploy = create("docs/plan.md", 5, "M3: deploy", "Add a rollback step.");
    let readme = create("README.md", 1, "# Readme", "Typo");
    let reply: DocComment = client
        .call(Request::CommentReply(CommentReplyParams {
            comment_id: redis.id,
            body: "Also check TTLs".into(),
            mkrew_session_id: None,
        }))
        .into_result()
        .unwrap();

    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    let other: SessionSpawnResult = client.spawn(&other_project, "fake").into_result().unwrap();
    for bad in [
        send_comments(&[], id),
        send_comments(&[redis.id, readme.id], id),
        send_comments(&[redis.id], other.session_id),
    ] {
        assert_eq!(error_code(client.call(bad)), ErrorCode::InvalidRequest);
    }
    let unknown = client.call(send_comments(&[CommentId(Ulid::new())], id));
    assert_eq!(error_code(unknown), ErrorCode::NotFound);

    let working: SessionWaitResult = client
        .call(wait(id, &[SessionState::Working], Some(15_000)))
        .into_result()
        .unwrap();
    assert_eq!(working.state, SessionState::Working);
    let sent: Vec<DocComment> = client
        .call(send_comments(&[reply.id, deploy.id, redis.id], id))
        .into_result()
        .unwrap();
    assert_eq!(
        sent.iter().map(|c| c.id).collect::<Vec<_>>(),
        [redis.id, deploy.id]
    );
    for comment in &sent {
        assert_eq!(comment.status, CommentStatus::Sent);
        assert_eq!(comment.sent_to, Some(id));
        assert_eq!(comment.sent_snapshot.as_deref(), Some(plan));
    }
    wait_comment(&mut client, deploy.id, CommentStatus::Sent);

    // Queued while working: the console echoes the prompt's first line after the agent asks.
    let session = client.wait_state(id, SessionState::Exited);
    assert_eq!(session.exit_code, Some(0));
    let log = session_log(&env, id);
    let asked = log.find("(y/n)").unwrap();
    let typed = log.find("Update docs/plan.md").unwrap();
    assert!(asked < typed, "{log}");

    let exited = client.call(send_comments(&[deploy.id], id));
    assert_eq!(error_code(exited), ErrorCode::InvalidState);
    client.stop();
}

/// The review round trip (M6 E2E): the fake writes a plan, two comments are sent to it, it
/// edits the file for each and answers each with `mk comment reply` → both `addressed`.
#[test]
fn fake_agent_addresses_sent_comments_and_replies_to_each() {
    let env = Env::new();
    let repo = env.repo();
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    let plan = "# Plan\n\nUse Redis for caching.\n\nM3: deploy\n";
    // The spinner keeps the fake working while the comments are sent, so the prompt is
    // usually queued until it asks (either way it must arrive whole).
    let scenario = env.dir.path().join("reviewer.toml");
    std::fs::write(
        &scenario,
        format!(
            "[[step]]\naction = \"write_file\"\npath = \"docs/plan.md\"\ncontent = '''{plan}'''\n\n\
             [[step]]\naction = \"spinner\"\ntext = \"planning\"\nms = 600\n\n\
             [[step]]\naction = \"reply_comments\"\ncomments = 2\n\n\
             [[step]]\naction = \"print\"\ntext = \"Edited docs/plan.md\"\n\n\
             [[step]]\naction = \"print\"\ntext = \"Replied to 2 comments\"\n\n\
             [[step]]\naction = \"print\"\ntext = \"All done\"\n\n\
             [[step]]\naction = \"print\"\ntext = \"Done.\"\n"
        ),
    )
    .unwrap();
    env.write_fake_profile_as(
        "reviewer",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let spawned: SessionSpawnResult = client.spawn(&project, "reviewer").into_result().unwrap();
    let id = spawned.session_id;
    let deadline = Instant::now() + TIMEOUT;
    while std::fs::read_to_string(repo.join("docs/plan.md"))
        .ok()
        .as_deref()
        != Some(plan)
    {
        assert!(Instant::now() < deadline, "the fake never wrote the plan");
        std::thread::sleep(Duration::from_millis(20));
    }

    let mut create = |line, quote: &str, body: &str| {
        let params = CommentCreateParams {
            project_id: project.id,
            file_path: "docs/plan.md".into(),
            start_line: line,
            end_line: line,
            quote: quote.into(),
            body: body.into(),
        };
        let created: DocComment = client
            .call(Request::CommentCreate(params))
            .into_result()
            .unwrap();
        created
    };
    let redis = create(3, "Use Redis", "Why not in-memory?");
    let deploy = create(5, "M3: deploy", "Add a rollback step.");
    let sent: Vec<DocComment> = client
        .call(send_comments(&[redis.id, deploy.id], id))
        .into_result()
        .unwrap();
    assert_eq!(sent.len(), 2);

    for comment in [&redis, &deploy] {
        wait_comment(&mut client, comment.id, CommentStatus::Addressed);
    }
    let session = client.wait_state(id, SessionState::Exited);
    assert_eq!(session.exit_code, Some(0));
    assert_eq!(
        std::fs::read_to_string(repo.join("docs/plan.md")).unwrap(),
        format!(
            "{plan}Addressed {}: Why not in-memory?\nAddressed {}: Add a rollback step.\n",
            redis.id, deploy.id
        )
    );

    let comments: Vec<DocComment> = client
        .call(Request::CommentList(CommentListParams {
            project_id: project.id,
            file_path: Some("docs/plan.md".into()),
        }))
        .into_result()
        .unwrap();
    for (root, body) in [
        (&redis, "Addressed: Why not in-memory?"),
        (&deploy, "Addressed: Add a rollback step."),
    ] {
        let stored = comments.iter().find(|c| c.id == root.id).unwrap();
        assert_eq!(stored.status, CommentStatus::Addressed);
        let replies: Vec<_> = comments
            .iter()
            .filter(|c| c.parent_id == Some(root.id))
            .map(|c| (c.author.as_str(), c.body.as_str()))
            .collect();
        assert_eq!(replies, [("reviewer", body)]);
    }
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

fn create_task(project: &Project, title: &str, agent_profile: Option<&str>) -> Request {
    Request::TaskCreate(TaskCreateParams {
        project_id: project.id,
        title: title.into(),
        prompt: format!("prompt for {title}"),
        agent_profile: agent_profile.map(Into::into),
        position: None,
    })
}

fn list_tasks(client: &mut Client, project: &Project) -> Vec<Task> {
    client
        .call(Request::TaskList(TaskListParams {
            project_id: Some(project.id),
            status: None,
        }))
        .into_result()
        .unwrap()
}

#[test]
fn tasks_crud_and_order_persist_across_daemon_restart() {
    let env = Env::new();
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());

    let first: Task = client
        .call(create_task(&project, "one", None))
        .into_result()
        .unwrap();
    let second: Task = client
        .call(create_task(&project, "two", Some("claude")))
        .into_result()
        .unwrap();
    let doomed: Task = client
        .call(create_task(&project, "doomed", None))
        .into_result()
        .unwrap();
    assert_eq!(first.status, TaskStatus::Todo);
    assert_eq!(first.prompt, "prompt for one");
    assert_eq!(second.agent_profile.as_deref(), Some("claude"));
    assert!(first.position < second.position && second.position < doomed.position);
    client.wait_event(&Event::TaskUpdated(first.clone()));

    let edited: Task = client
        .call(Request::TaskUpdate(TaskUpdateParams {
            task_id: second.id,
            title: Some("deux".into()),
            prompt: None,
            agent_profile: Some(None),
        }))
        .into_result()
        .unwrap();
    assert_eq!(
        (edited.title.as_str(), edited.agent_profile.as_deref()),
        ("deux", None)
    );
    assert_eq!(edited.prompt, "prompt for two");
    client.wait_event(&Event::TaskUpdated(edited.clone()));

    // Drop "one" into Review, then "deux" in front of it.
    let moved: Task = client
        .call(Request::TaskMove(TaskMoveParams {
            task_id: first.id,
            status: TaskStatus::Review,
            position: 1.0,
        }))
        .into_result()
        .unwrap();
    assert_eq!(moved.status, TaskStatus::Review);
    client.wait_event(&Event::TaskUpdated(moved.clone()));
    client
        .call(Request::TaskMove(TaskMoveParams {
            task_id: second.id,
            status: TaskStatus::Review,
            position: 0.5,
        }))
        .into_result::<Task>()
        .unwrap();

    client
        .call(Request::TaskDelete(TaskRef { task_id: doomed.id }))
        .into_result::<()>()
        .unwrap();
    client.wait_event(&Event::TaskDeleted(TaskDeletedEvent {
        task_id: doomed.id,
        project_id: project.id,
    }));
    let review: Vec<Task> = client
        .call(Request::TaskList(TaskListParams {
            project_id: Some(project.id),
            status: Some(TaskStatus::Review),
        }))
        .into_result()
        .unwrap();
    let titles: Vec<&str> = review.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, ["deux", "one"]);
    let before = list_tasks(&mut client, &project);
    assert_eq!(before.len(), 2);

    let unknown = || TaskRef {
        task_id: Ulid::new(),
    };
    assert_eq!(
        error_code(client.call(Request::TaskDelete(unknown()))),
        ErrorCode::NotFound
    );
    let missing_move = Request::TaskMove(TaskMoveParams {
        task_id: unknown().task_id,
        status: TaskStatus::Done,
        position: 0.0,
    });
    assert_eq!(error_code(client.call(missing_move)), ErrorCode::NotFound);
    let orphan = create_task(
        &Project {
            id: Ulid::new(),
            ..project.clone()
        },
        "x",
        None,
    );
    assert_eq!(error_code(client.call(orphan)), ErrorCode::NotFound);
    assert_eq!(
        error_code(client.call(create_task(&project, "  ", None))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(create_task(&project, "x", Some("shell")))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(create_task(&project, "x", Some("nope")))),
        ErrorCode::NotFound
    );
    let bad_edit = Request::TaskUpdate(TaskUpdateParams {
        task_id: first.id,
        title: None,
        prompt: None,
        agent_profile: Some(Some("shell".into())),
    });
    assert_eq!(error_code(client.call(bad_edit)), ErrorCode::InvalidRequest);

    client.stop();
    assert!(daemon.wait_exit().success());
    let (_daemon, mut client) = env.start();
    assert_eq!(list_tasks(&mut client, &project), before);
    client.stop();
}

#[test]
fn git_set_base_is_announced_persists_and_null_means_auto() {
    let env = Env::new();
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    assert_eq!(project.base_ref, None);
    let set_base = |base: Option<&str>| {
        Request::GitSetBase(GitSetBaseParams {
            project_id: project.id,
            base_ref: base.map(str::to_owned),
        })
    };

    // Returns `null`; the trimmed base comes back through `project.updated`.
    let _: () = client
        .call(set_base(Some(" develop ")))
        .into_result()
        .unwrap();
    let with_base = Project {
        base_ref: Some("develop".into()),
        ..project.clone()
    };
    client.wait_event(&Event::ProjectUpdated(with_base.clone()));
    for bad in ["--force", "a b"] {
        assert_eq!(
            error_code(client.call(set_base(Some(bad)))),
            ErrorCode::InvalidRequest
        );
    }
    let missing = Request::GitSetBase(GitSetBaseParams {
        project_id: Ulid::new(),
        base_ref: None,
    });
    assert_eq!(error_code(client.call(missing)), ErrorCode::NotFound);

    client.stop();
    assert!(daemon.wait_exit().success());
    let (_daemon, mut client) = env.start();
    let listed: Vec<Project> = client.call(Request::ProjectList).into_result().unwrap();
    assert_eq!(listed, vec![with_base]);
    // Blank is the same as `null`: back to auto-detect.
    let _: () = client.call(set_base(Some("  "))).into_result().unwrap();
    client.wait_event(&Event::ProjectUpdated(project.clone()));
    client.stop();
}

#[test]
fn git_status_and_diff_follow_the_project_base_and_reject_bad_input() {
    let env = Env::new();
    let repo = env.repo();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let fork = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["checkout", "-q", "-b", "feature"]);
    std::fs::write(repo.join("a.txt"), "one\ntwo\n").unwrap();
    let plain = env.dir.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let outside = client.add_project(&plain);
    let status = |project: &Project, mode| {
        Request::GitStatus(GitStatusParams {
            project_id: project.id,
            mode,
        })
    };
    let diff = |path: &str, mode, commit: Option<&str>| {
        Request::GitDiff(GitDiffParams {
            project_id: project.id,
            path: path.into(),
            mode,
            staged: false,
            commit: commit.map(str::to_owned),
        })
    };

    let working: GitStatus = client
        .call(status(&project, GitMode::Working))
        .into_result()
        .unwrap();
    assert_eq!(working.branch.as_deref(), Some("feature"));
    assert_eq!(working.base_ref.as_deref(), Some("main"));
    let unstaged: Vec<_> = working
        .unstaged
        .iter()
        .map(|f| (f.path.as_str(), f.added, f.removed))
        .collect();
    assert_eq!(unstaged, [("a.txt", Some(1), Some(0))]);
    let against: GitStatus = client
        .call(status(&project, GitMode::Base))
        .into_result()
        .unwrap();
    assert_eq!(against.merge_base, Some(fork));
    assert_eq!(against.against_base.len(), 1);
    let shown: FileDiff = client
        .call(diff("a.txt", GitMode::Base, None))
        .into_result()
        .unwrap();
    assert_eq!(
        (shown.path.as_str(), shown.added, shown.hunks.len()),
        ("a.txt", 1, 1)
    );

    // A configured base that names nothing: no merge-base, so no base diff.
    let set_base = Request::GitSetBase(GitSetBaseParams {
        project_id: project.id,
        base_ref: Some("develop".into()),
    });
    let _: () = client.call(set_base).into_result().unwrap();
    let against: GitStatus = client
        .call(status(&project, GitMode::Base))
        .into_result()
        .unwrap();
    assert_eq!(
        (
            against.base_ref.as_deref(),
            against.base_auto,
            against.merge_base
        ),
        (Some("develop"), false, None)
    );
    assert_eq!(
        error_code(client.call(diff("a.txt", GitMode::Base, None))),
        ErrorCode::InvalidState
    );
    assert_eq!(
        error_code(client.call(status(&outside, GitMode::Working))),
        ErrorCode::InvalidState
    );
    for bad in [
        diff("../a.txt", GitMode::Working, None),
        diff("a.txt", GitMode::Working, Some("--output=x")),
    ] {
        assert_eq!(error_code(client.call(bad)), ErrorCode::InvalidRequest);
    }
    let missing = Request::GitStatus(GitStatusParams {
        project_id: Ulid::new(),
        mode: GitMode::Working,
    });
    assert_eq!(error_code(client.call(missing)), ErrorCode::NotFound);
    client.stop();
}

/// T7.4: stage/unstage by path and by hunk, discard only when confirmed, commit; the watcher
/// started by `git.status` reports index changes as `git.changed`.
#[test]
fn git_stage_unstage_discard_and_commit_with_git_changed_events() {
    let env = Env::new();
    let repo = env.repo();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["config", "user.name", "t"]);
    git(&repo, &["config", "user.email", "t@t"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    let lines: Vec<String> = (1..=20).map(|n| format!("line {n}\n")).collect();
    std::fs::write(repo.join("a.txt"), lines.concat()).unwrap();
    std::fs::write(repo.join("b.txt"), "b\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let mut changed = lines.clone();
    changed[1] = "line two\n".into();
    changed[18] = "line nineteen\n".into();
    std::fs::write(repo.join("a.txt"), changed.concat()).unwrap();
    std::fs::write(repo.join("b.txt"), "b changed\n").unwrap();
    std::fs::write(repo.join("new.txt"), "new\n").unwrap();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let id = project.id;
    let status = |client: &mut Client| -> GitStatus {
        let request = Request::GitStatus(GitStatusParams {
            project_id: id,
            mode: GitMode::Working,
        });
        client.call(request).into_result().unwrap()
    };
    let names = |files: &[mkrew_protocol::ChangedFile]| -> Vec<String> {
        files.iter().map(|f| f.path.clone()).collect()
    };
    let by_path = |paths: &[&str]| GitStageParams {
        project_id: id,
        paths: paths.iter().map(|&p| p.to_owned()).collect(),
        hunk: None,
    };
    let by_hunk = |hunk: &mkrew_protocol::Hunk| GitStageParams {
        project_id: id,
        paths: vec![],
        hunk: Some(HunkTarget {
            path: "a.txt".into(),
            hunk: hunk.clone(),
        }),
    };
    let diff = |client: &mut Client, staged| -> FileDiff {
        let request = Request::GitDiff(GitDiffParams {
            project_id: id,
            path: "a.txt".into(),
            mode: GitMode::Working,
            staged,
            commit: None,
        });
        client.call(request).into_result().unwrap()
    };
    let changed_event = Event::GitChanged(GitChangedEvent { project_id: id });

    // Changes made through the daemon are reported at once.
    assert_eq!(names(&status(&mut client).unstaged), ["a.txt", "b.txt"]);
    let _: () = client
        .call(Request::GitStage(by_path(&["b.txt", "new.txt"])))
        .into_result()
        .unwrap();
    client.wait_event(&changed_event);
    let now = status(&mut client);
    assert_eq!(names(&now.staged), ["b.txt", "new.txt"]);
    assert!(now.untracked.is_empty());
    let _: () = client
        .call(Request::GitUnstage(by_path(&["new.txt"])))
        .into_result()
        .unwrap();
    assert_eq!(names(&status(&mut client).untracked), ["new.txt"]);

    // Stage the second hunk of a.txt only, then unstage it from the staged diff.
    let unstaged = diff(&mut client, false).hunks;
    assert_eq!(unstaged.len(), 2);
    let _: () = client
        .call(Request::GitStage(by_hunk(&unstaged[1])))
        .into_result()
        .unwrap();
    let staged = diff(&mut client, true).hunks;
    assert_eq!(staged.len(), 1);
    assert_eq!(staged[0].lines, unstaged[1].lines);
    assert_eq!(diff(&mut client, false).hunks, [unstaged[0].clone()]);
    let _: () = client
        .call(Request::GitUnstage(by_hunk(&staged[0])))
        .into_result()
        .unwrap();
    assert_eq!(diff(&mut client, true).hunks, []);
    // The same hunk again no longer applies.
    assert_eq!(
        error_code(client.call(Request::GitUnstage(by_hunk(&staged[0])))),
        ErrorCode::Internal
    );

    // Discard only with confirm: a.txt back to the index, new.txt deleted.
    let discard = |confirm| {
        Request::GitDiscard(GitDiscardParams {
            project_id: id,
            paths: vec!["a.txt".into(), "new.txt".into()],
            confirm,
        })
    };
    assert_eq!(
        error_code(client.call(discard(false))),
        ErrorCode::InvalidRequest
    );
    assert!(repo.join("new.txt").exists());
    let _: () = client.call(discard(true)).into_result().unwrap();
    assert_eq!(
        std::fs::read_to_string(repo.join("a.txt")).unwrap(),
        lines.concat()
    );
    assert!(!repo.join("new.txt").exists());

    // Commit the staged b.txt.
    let commit = |message: &str| {
        Request::GitCommit(GitCommitParams {
            project_id: id,
            message: message.into(),
        })
    };
    let committed: GitCommitResult = client.call(commit("two")).into_result().unwrap();
    assert_eq!(committed.commit, git(&repo, &["rev-parse", "HEAD"]));
    assert_eq!(git(&repo, &["log", "-1", "--format=%s"]), "two");
    let clean = status(&mut client);
    assert!(clean.staged.is_empty() && clean.unstaged.is_empty() && clean.untracked.is_empty());

    // `git.status` started the watcher: a commit made outside Mkrew (only `.git` changes) is
    // reported too.
    std::thread::sleep(Duration::from_millis(500));
    client.call(Request::DaemonStatus);
    client.events.clear();
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "outside"]);
    client.wait_event(&changed_event);

    for bad in [
        commit("  \n"),
        Request::GitStage(by_path(&[])),
        Request::GitStage(by_path(&["../x"])),
        Request::GitStage(GitStageParams {
            hunk: by_hunk(&unstaged[0]).hunk,
            ..by_path(&["a.txt"])
        }),
        Request::GitDiscard(GitDiscardParams {
            project_id: id,
            paths: vec![],
            confirm: true,
        }),
    ] {
        assert_eq!(error_code(client.call(bad)), ErrorCode::InvalidRequest);
    }
    client.stop();
}

/// T7.5: history and one commit's files; push to the upstream and fetch from it, against a
/// local bare remote (no network). Both report `git.changed`.
#[test]
fn git_log_show_push_and_fetch_with_a_local_remote() {
    let env = Env::new();
    let repo = env.repo();
    let bare = env.dir.path().join("remote.git");
    git(
        env.dir.path(),
        &["init", "-q", "--bare", "-b", "main", "remote.git"],
    );
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["remote", "add", "origin", bare.to_str().unwrap()]);
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    std::fs::write(repo.join("b.txt"), "b\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "two\n\nbody"]);
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;
    let log = |path: Option<&str>| {
        Request::GitLog(GitLogParams {
            project_id: id,
            limit: GitLogParams::DEFAULT_LIMIT,
            path: path.map(str::to_owned),
        })
    };
    let show = |commit: &str| {
        Request::GitShow(GitShowParams {
            project_id: id,
            commit: commit.into(),
        })
    };
    let changed_event = Event::GitChanged(GitChangedEvent { project_id: id });

    let commits: Vec<Commit> = client.call(log(None)).into_result().unwrap();
    let summaries: Vec<&str> = commits.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(summaries, ["two", "one"]);
    assert_eq!(commits[0].body, "body");
    let only_a: Vec<Commit> = client.call(log(Some("a.txt"))).into_result().unwrap();
    assert_eq!(only_a, [commits[1].clone()]);
    let detail: CommitDetail = client.call(show(&commits[0].hash)).into_result().unwrap();
    assert_eq!(detail.commit, commits[0]);
    let files: Vec<&str> = detail.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(files, ["b.txt"]);
    assert_eq!(error_code(client.call(show("nope"))), ErrorCode::NotFound);
    for bad in [show("--all"), log(Some("../x"))] {
        assert_eq!(error_code(client.call(bad)), ErrorCode::InvalidRequest);
    }

    // Push needs an upstream; then it updates the remote branch.
    let push = Request::GitPush(ProjectRef { project_id: id });
    assert_eq!(
        error_code(client.call(push.clone())),
        ErrorCode::InvalidState
    );
    git(&repo, &["push", "-q", "-u", "origin", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "three"]);
    client.events.clear();
    let _: () = client.call(push).into_result().unwrap();
    client.wait_event(&changed_event);
    assert_eq!(
        git(&bare, &["rev-parse", "main"]),
        git(&repo, &["rev-parse", "HEAD"])
    );

    // Fetch brings in someone else's commit: the branch is behind by one.
    let other = env.dir.path().join("other");
    git(
        env.dir.path(),
        &["clone", "-q", bare.to_str().unwrap(), "other"],
    );
    git(&other, &["commit", "-q", "--allow-empty", "-m", "theirs"]);
    git(&other, &["push", "-q"]);
    client.events.clear();
    let fetch = Request::GitFetch(ProjectRef { project_id: id });
    let _: () = client.call(fetch).into_result().unwrap();
    client.wait_event(&changed_event);
    let status: GitStatus = client
        .call(Request::GitStatus(GitStatusParams {
            project_id: id,
            mode: GitMode::Working,
        }))
        .into_result()
        .unwrap();
    assert_eq!(status.upstream.as_deref(), Some("origin/main"));
    assert_eq!((status.ahead, status.behind), (0, 1));
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

fn run_task(task_id: Ulid) -> Request {
    Request::TaskRun(TaskRunParams {
        task_id,
        cols: 80,
        rows: 24,
    })
}

/// Makes calls until a `task.updated` for `id` matches `wanted`; returns that task.
fn wait_task(client: &mut Client, id: Ulid, wanted: impl Fn(&Task) -> bool) -> Task {
    let event =
        client.wait_event_where(|e| matches!(e, Event::TaskUpdated(t) if t.id == id && wanted(t)));
    let Event::TaskUpdated(task) = event else {
        unreachable!()
    };
    task
}

#[test]
fn task_run_delivers_the_prompt_per_mode_and_the_card_follows_the_session() {
    let env = Env::new();
    env.write_fake_profile();
    env.write_fake_profile_with_prompt("fake-arg", "arg");
    env.write_fake_profile_with_prompt("fake-type", "type");
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let task = |client: &mut Client, title: &str, agent: Option<&str>| -> Task {
        client
            .call(create_task(&project, title, agent))
            .into_result()
            .unwrap()
    };
    let by_default = task(&mut client, "arg", None);
    let typed = task(&mut client, "type", Some("fake-type"));
    let silent = task(&mut client, "none", Some("fake"));

    // No task agent and no project default: nothing to run.
    assert_eq!(
        error_code(client.call(run_task(by_default.id))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(run_task(Ulid::new()))),
        ErrorCode::NotFound
    );
    let zero = Request::TaskRun(TaskRunParams {
        task_id: silent.id,
        cols: 0,
        rows: 24,
    });
    assert_eq!(error_code(client.call(zero)), ErrorCode::InvalidRequest);
    client
        .call(set_default_agent(&project, Some("fake-arg")))
        .into_result::<Project>()
        .unwrap();

    // `arg`: the default agent gets the prompt as its last argument; the card follows the
    // session through needs_input back to running and into review on exit 0.
    let run: SessionSpawnResult = client.call(run_task(by_default.id)).into_result().unwrap();
    let session = client.session(run.session_id);
    assert_eq!(session.agent_profile, "fake-arg");
    assert_eq!(session.task_id, Some(by_default.id));
    assert_eq!(session.project_id, project.id);
    assert_eq!(session.title.as_deref(), Some("arg"));
    wait_task(&mut client, by_default.id, |t| {
        t.status == TaskStatus::Running
    });
    client.wait_state(run.session_id, SessionState::NeedsInput);
    wait_task(&mut client, by_default.id, |t| {
        t.status == TaskStatus::NeedsInput
    });
    client
        .call(send(run.session_id, "y"))
        .into_result::<()>()
        .unwrap();
    let done = wait_task(&mut client, by_default.id, |t| {
        t.status == TaskStatus::Review
    });
    assert!(!done.error_flag);
    assert_eq!(client.session(run.session_id).exit_code, Some(0));
    let log = session_log(&env, run.session_id);
    assert!(log.contains("Task: prompt for arg"), "{log}");

    // `type`: the prompt is typed once the agent first waits for input (here it answers the
    // fake's question), so the run finishes on its own.
    let run: SessionSpawnResult = client.call(run_task(typed.id)).into_result().unwrap();
    assert_eq!(client.session(run.session_id).agent_profile, "fake-type");
    wait_task(&mut client, typed.id, |t| t.status == TaskStatus::Review);
    let log = session_log(&env, run.session_id);
    assert!(log.contains("prompt for type"), "{log}");
    assert!(!log.contains("Task:"), "{log}");

    // `none`: no prompt; a second run or a delete is refused while the session lives.
    let run: SessionSpawnResult = client.call(run_task(silent.id)).into_result().unwrap();
    wait_task(&mut client, silent.id, |t| {
        t.status == TaskStatus::NeedsInput
    });
    assert!(!session_log(&env, run.session_id).contains("prompt for none"));
    assert_eq!(
        error_code(client.call(run_task(silent.id))),
        ErrorCode::InvalidState
    );
    assert_eq!(
        error_code(client.call(Request::TaskDelete(TaskRef { task_id: silent.id }))),
        ErrorCode::InvalidState
    );
    // Killed (non-zero exit): needs_input with the error flag; running again clears it.
    let kill = |session_id| {
        Request::SessionKill(SessionKillParams {
            session_id,
            force: true,
        })
    };
    client
        .call(kill(run.session_id))
        .into_result::<()>()
        .unwrap();
    let failed = wait_task(&mut client, silent.id, |t| t.error_flag);
    assert_eq!(failed.status, TaskStatus::NeedsInput);
    client.events.clear();
    let rerun: SessionSpawnResult = client.call(run_task(silent.id)).into_result().unwrap();
    let running = wait_task(&mut client, silent.id, |t| t.status == TaskStatus::Running);
    assert!(!running.error_flag);

    // Stopping the daemon ends the session but is no failure: the card stays running for
    // the startup resume (SPEC §8), which cannot resume a profile without `resume` and asks
    // the user to run it again.
    client.stop();
    assert!(daemon.wait_exit().success());
    let (_daemon, mut client) = env.start();
    assert_eq!(client.session(rerun.session_id).state, SessionState::Exited);
    let after = list_tasks(&mut client, &project);
    let after = after.iter().find(|t| t.id == silent.id).unwrap();
    assert_eq!(
        (after.status, after.error_flag),
        (TaskStatus::NeedsInput, true)
    );
    client.stop();
}

/// `git` in `dir` with a throwaway identity; returns trimmed stdout.
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// M5 checkpoint: each run in a git repo stores HEAD in `task.checkpoint`; outside a repo the
/// task runs without one.
#[test]
fn task_run_stores_head_as_checkpoint() {
    let env = Env::new();
    env.write_fake_profile();
    let repo = env.repo();
    git(&repo, &["init", "-q"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "one"]);
    let first = git(&repo, &["rev-parse", "HEAD"]);
    let plain = env.dir.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let (mut daemon, mut client) = env.start();
    let in_repo = client.add_project(&repo);
    let outside = client.add_project(&plain);
    let task: Task = client
        .call(create_task(&in_repo, "t", Some("fake")))
        .into_result()
        .unwrap();
    assert_eq!(task.checkpoint, None);
    let kill = |session_id| {
        Request::SessionKill(SessionKillParams {
            session_id,
            force: true,
        })
    };

    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    wait_task(&mut client, task.id, |t| {
        t.checkpoint.as_deref() == Some(first.as_str())
    });
    client
        .call(kill(run.session_id))
        .into_result::<()>()
        .unwrap();
    wait_task(&mut client, task.id, |t| t.error_flag);

    // A later run takes a fresh checkpoint.
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "two"]);
    let second = git(&repo, &["rev-parse", "HEAD"]);
    client.events.clear();
    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    wait_task(&mut client, task.id, |t| {
        t.checkpoint.as_deref() == Some(second.as_str())
    });
    client
        .call(kill(run.session_id))
        .into_result::<()>()
        .unwrap();

    let other: Task = client
        .call(create_task(&outside, "t", Some("fake")))
        .into_result()
        .unwrap();
    client.events.clear();
    client
        .call(run_task(other.id))
        .into_result::<SessionSpawnResult>()
        .unwrap();
    let running = wait_task(&mut client, other.id, |t| t.status == TaskStatus::Running);
    assert_eq!(running.checkpoint, None);
    let listed = list_tasks(&mut client, &outside);
    assert_eq!(listed[0].checkpoint, None);
    client.stop();
    assert!(daemon.wait_exit().success());
}

/// M5 undo (ADR 0005): the preview lists what the run changed and created; `task.undo` needs
/// `confirm`, a stopped task and a checkpoint, then restores tracked files and deletes only
/// the untracked files created since the checkpoint.
#[test]
fn task_undo_previews_then_discards_changes_since_the_checkpoint() {
    let env = Env::new();
    env.write_fake_profile();
    let repo = env.repo();
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["init", "-q"]);
    // Byte-exact restores whatever the machine's global autocrlf says.
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let old = repo.join("old.txt");
    std::fs::write(&old, "mine\n").unwrap();
    let file = std::fs::File::options().write(true).open(&old).unwrap();
    file.set_modified(std::time::SystemTime::now() - Duration::from_secs(3600))
        .unwrap();
    drop(file);
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let task: Task = client
        .call(create_task(&project, "t", Some("fake")))
        .into_result()
        .unwrap();
    let preview = |task_id| Request::TaskUndoPreview(TaskRef { task_id });
    let undo = |task_id, confirm| {
        Request::TaskUndo(TaskUndoParams {
            task_id,
            confirm,
            created: Vec::new(),
            fingerprint: String::new(),
        })
    };

    assert_eq!(
        error_code(client.call(preview(task.id))),
        ErrorCode::InvalidState
    );
    assert_eq!(
        error_code(client.call(undo(task.id, true))),
        ErrorCode::InvalidState
    );
    assert_eq!(
        error_code(client.call(preview(Ulid::new()))),
        ErrorCode::NotFound
    );

    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    wait_task(&mut client, task.id, |t| t.checkpoint.is_some());
    std::fs::write(repo.join("a.txt"), "two\n").unwrap();
    std::fs::create_dir(repo.join("new")).unwrap();
    std::fs::write(repo.join("new").join("b.txt"), "b\n").unwrap();
    // Staged by the run (e.g. an agent's `git add -A`): not in the checkpoint, so undo
    // deletes it whatever its mtime, and the preview lists it as `added`.
    let notes = repo.join("notes.md");
    std::fs::write(&notes, "notes\n").unwrap();
    git(&repo, &["add", "notes.md"]);

    // Refused while the task's session is live, and without confirm.
    assert_eq!(
        error_code(client.call(undo(task.id, true))),
        ErrorCode::InvalidState
    );
    client
        .call(Request::SessionKill(SessionKillParams {
            session_id: run.session_id,
            force: true,
        }))
        .into_result::<()>()
        .unwrap();
    wait_task(&mut client, task.id, |t| t.error_flag);
    assert_eq!(
        error_code(client.call(undo(task.id, false))),
        ErrorCode::InvalidRequest
    );

    let shown: UndoPreview = client.call(preview(task.id)).into_result().unwrap();
    assert_eq!(shown.changed, ["a.txt"]);
    assert_eq!(shown.added, ["notes.md"]);
    assert_eq!(shown.created, ["new/b.txt"]);
    assert!(shown.diff.contains("-one\n+two"), "{}", shown.diff);
    assert!(!shown.diff_truncated);
    // Nothing is touched by the preview or the refusals.
    assert_eq!(
        std::fs::read_to_string(repo.join("a.txt")).unwrap(),
        "two\n"
    );

    // Staged after the preview was shown: the tracked changes no longer match it, so the undo
    // is refused and touches nothing.
    let staged = repo.join("staged.txt");
    std::fs::write(&staged, "staged\n").unwrap();
    git(&repo, &["add", "staged.txt"]);
    let confirmed = |shown: &UndoPreview| {
        Request::TaskUndo(TaskUndoParams {
            task_id: task.id,
            confirm: true,
            created: shown.created.clone(),
            fingerprint: shown.fingerprint.clone(),
        })
    };
    assert_eq!(
        error_code(client.call(confirmed(&shown))),
        ErrorCode::InvalidState
    );
    assert!(staged.exists());
    assert_eq!(
        std::fs::read_to_string(repo.join("a.txt")).unwrap(),
        "two\n"
    );
    git(&repo, &["rm", "-q", "--cached", "staged.txt"]);
    std::fs::remove_file(&staged).unwrap();

    // Created after the preview was shown: untracked, not confirmed, so kept.
    let late = repo.join("late.txt");
    std::fs::write(&late, "late\n").unwrap();
    client.call(confirmed(&shown)).into_result::<()>().unwrap();
    assert_eq!(
        std::fs::read_to_string(repo.join("a.txt")).unwrap(),
        "one\n"
    );
    assert!(!repo.join("new").join("b.txt").exists());
    assert!(!notes.exists());
    assert_eq!(std::fs::read_to_string(&old).unwrap(), "mine\n");
    assert!(late.exists());
    let after: UndoPreview = client.call(preview(task.id)).into_result().unwrap();
    assert!(
        after.changed.is_empty() && after.added.is_empty() && after.created == ["late.txt"],
        "{after:?}"
    );
    client.stop();
    assert!(daemon.wait_exit().success());
}

/// SPEC §8 resume: after a daemon restart a task whose agent reported its own session id runs
/// the profile's `resume` command with that id, in the old session's pane.
#[test]
fn restart_resumes_running_tasks_with_their_native_session_id() {
    let env = Env::new();
    let scenario = env.dir.path().join("native.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"hook\"\nevent = \"busy\"\nsession_id = \"native-42\"\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    let fake = built_bin("mkrew-fakeagent");
    let args = format!("args = ['--scenario', '{}']", scenario.display());
    env.write_fake_profile_as("resumable", &args["args = ".len()..]);
    let path = env.dir.path().join("config/agents/resumable.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    // TOML literal string: the quoted word keeps backslashes and spaces. The resumed fake
    // runs the default scenario (no hook), so its native id can only be carried over.
    let resume = format!(
        "resume = '\"{}\" --resume {{session_id}}'\n",
        fake.display()
    );
    let text = text.replace("[state]", &format!("{resume}\n[state]"));
    std::fs::write(&path, &text).unwrap();
    // Resumable profile, but the agent never reports its id: nothing to resume.
    env.write_profile("resumable-silent", &text.replace(&args, "args = []"));

    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let task: Task = client
        .call(create_task(&project, "resume me", Some("resumable")))
        .into_result()
        .unwrap();
    let silent: Task = client
        .call(create_task(&project, "no id", Some("resumable-silent")))
        .into_result()
        .unwrap();
    let silent_run: SessionSpawnResult = client.call(run_task(silent.id)).into_result().unwrap();
    client.wait_state(silent_run.session_id, SessionState::NeedsInput);
    let tab: TerminalTab = client
        .call(create_tab(&project, "agents"))
        .into_result()
        .unwrap();
    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    let layout = json!({"split": "vertical", "ratio": 0.5, "children": [
        {"session": Ulid::new().to_string()}, {"session": run.session_id.to_string()}
    ]});
    client
        .call(Request::TabSetLayout(TabSetLayoutParams {
            tab_id: tab.id,
            layout: layout.clone(),
        }))
        .into_result::<()>()
        .unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while client.session(run.session_id).native_session_id.is_none() {
        assert!(Instant::now() < deadline, "no native session id reported");
        std::thread::sleep(Duration::from_millis(50));
    }
    client.wait_state(run.session_id, SessionState::NeedsInput);

    client.stop();
    assert!(daemon.wait_exit().success());
    let (_daemon, mut client) = env.start();
    let sessions: Vec<Session> = client
        .call(Request::SessionList(SessionListParams::default()))
        .into_result()
        .unwrap();
    let resumed = sessions
        .iter()
        .find(|s| s.task_id == Some(task.id) && s.id != run.session_id)
        .expect("the task got a new session")
        .clone();
    assert_eq!(
        (resumed.agent_profile.as_str(), resumed.title.as_deref()),
        ("resumable", Some("resume me"))
    );
    // Carried over: a second restart before the agent's next hook resumes it again.
    assert_eq!(resumed.native_session_id.as_deref(), Some("native-42"));
    assert!(
        !sessions
            .iter()
            .any(|s| s.task_id == Some(silent.id) && s.id != silent_run.session_id)
    );
    let flagged = list_tasks(&mut client, &project);
    let flagged = flagged.iter().find(|t| t.id == silent.id).unwrap();
    assert_eq!(
        (flagged.status, flagged.error_flag),
        (TaskStatus::NeedsInput, true)
    );
    client.wait_state(resumed.id, SessionState::NeedsInput);
    let attached: SessionAttachResult = client.attach(resumed.id).into_result().unwrap();
    let screen = screen_text(&attached.snapshot);
    assert!(screen.contains("Resumed: native-42"), "{screen}");
    // The card follows the resumed session like a fresh run.
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let tasks = list_tasks(&mut client, &project);
        let after = tasks.iter().find(|t| t.id == task.id).unwrap();
        assert!(!after.error_flag);
        if after.status == TaskStatus::NeedsInput {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "task stuck in {:?}",
            after.status
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let tabs = list_tabs(&mut client, &project);
    let expected = layout
        .to_string()
        .replace(&run.session_id.to_string(), &resumed.id.to_string());
    assert_eq!(
        tabs[0].layout,
        serde_json::from_str::<Value>(&expected).unwrap()
    );
    client.stop();
}

/// npm installs `claude`/`codex` as `.cmd` shims; an `arg` prompt full of cmd.exe
/// metacharacters must still reach the agent verbatim (HUMAN_CHECKS T1.8). The `.js` →
/// node shim form needs a real node, so it is covered by `launch.rs` unit tests (argv) only.
#[cfg(windows)]
#[test]
fn task_run_passes_metacharacter_prompts_through_npm_cmd_shims() {
    let env = Env::new();
    let bin = env.dir.path().join("npm");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::copy(built_bin("mkrew-fakeagent"), bin.join("fake-cli.exe")).unwrap();
    let shim = bin.join("fake-cli.cmd");
    std::fs::write(&shim, "@ECHO off\r\n\"%dp0%\\fake-cli.exe\"   %*\r\n").unwrap();
    let sample = include_str!("../../../agents/fake.toml");
    let profile = sample
        .replace(
            "command = \"mkrew-fakeagent\"",
            &format!("command = '{}'", shim.display()),
        )
        .replace("prompt_mode = \"none\"", "prompt_mode = \"arg\"");
    env.write_profile("shimmed", &profile);
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let prompt = "fix \"a&b\" at 100% ^!|<x>";
    let task: Task = client
        .call(Request::TaskCreate(TaskCreateParams {
            project_id: project.id,
            title: "shim".into(),
            prompt: prompt.into(),
            agent_profile: Some("shimmed".into()),
            position: None,
        }))
        .into_result()
        .unwrap();

    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    client.wait_state(run.session_id, SessionState::NeedsInput);
    client
        .call(send(run.session_id, "y"))
        .into_result::<()>()
        .unwrap();
    wait_task(&mut client, task.id, |t| t.status == TaskStatus::Review);
    let log = session_log(&env, run.session_id);
    assert!(log.contains(&format!("Task: {prompt}")), "{log:?}");
    client.stop();
}

fn session_log(env: &Env, id: Ulid) -> String {
    let path = env.data_dir().join("sessions").join(format!("{id}.log"));
    String::from_utf8_lossy(&std::fs::read(path).unwrap()).into_owned()
}
