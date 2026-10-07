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
    AgentProfilesResult, AiGenerateParams, AiGeneratedEvent, AiKind, AttachmentSaveParams,
    AttachmentSaveResult, ClientKind, CommentContext, CommentCreateParams, CommentId,
    CommentListParams, CommentRef, CommentReplyParams, CommentSendParams, CommentStatus, Commit,
    CommitDetail, DaemonStatusResult, DevServer, DevStartParams, DocChangedEvent, DocComment,
    DocContent, DocFile, DocListParams, DocReadParams, ErrorCode, Event, FileDiff, Frame,
    FrameDecoder, FsContent, FsEntry, FsFindParams, FsListParams, FsMatch, FsReadParams,
    GitChangedEvent, GitCommitParams, GitCommitResult, GitDiffParams, GitDiscardParams,
    GitLogParams, GitMarker, GitMode, GitRemoteParams, GitSetBaseParams, GitShowParams,
    GitStageParams, GitStatus, GitStatusParams, HelloParams, HelloResult, HookReportParams,
    HunkTarget, Message, Outcome, PROTOCOL_VERSION, Project, ProjectAddParams, ProjectRef,
    ProjectUpdateParams, QueuedSend, RemoveWorktreeResult, Request, ResponseError, ScreenSnapshot,
    Session, SessionAttachResult, SessionExitedEvent, SessionFiles, SessionFilesParams,
    SessionInputParams, SessionKillParams, SessionKind, SessionListParams, SessionQueueEvent,
    SessionReadParams, SessionReadResult, SessionRef, SessionResizeParams, SessionSendParams,
    SessionSendResult, SessionSpawnParams, SessionSpawnResult, SessionState, SessionUnqueueParams,
    SessionUsageEvent, SessionWaitParams, SessionWaitResult, StateSource, TabCreateParams,
    TabListParams, TabMoveParams, TabRef, TabRenameParams, TabSetLayoutParams, Task,
    TaskConfirmParams, TaskCreateParams, TaskDeletedEvent, TaskFanoutParams, TaskListParams,
    TaskMergeResult, TaskMoveParams, TaskRef, TaskRunParams, TaskStatus, TaskUndoParams,
    TaskUpdateParams, TerminalTab, Turn, TurnRef, TurnRestoreParams, UndoPreview, encode,
};
use mkrew_store::Store;
use serde_json::{Value, json};
use ulid::Ulid;

// Generous: the flood test alone takes ~13 s on Windows, longer under parallel test load.
const TIMEOUT: Duration = Duration::from_secs(30);

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

    /// `<id>.toml`: an agent whose `oneshot` runs the built fake agent with `scenario`.
    fn write_oneshot_profile(&self, id: &str, scenario: &Path) {
        let fake = built_bin("mkrew-fakeagent");
        let toml = format!(
            "command = '{fake}'\noneshot = '\"{fake}\" --scenario \"{scenario}\"'\n",
            fake = fake.display(),
            scenario = scenario.display(),
        );
        self.write_profile(id, &toml);
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
            // Transcripts for token usage (SPEC §20E.3): never the user's real ones.
            .env("CLAUDE_CONFIG_DIR", self.dir.path().join("claude"))
            .env("CODEX_HOME", self.dir.path().join("codex"))
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
            dev_command: None,
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
fn settings_are_reread_and_errors_reported_in_status() {
    // No config dir yet: the daemon creates it (seeding profiles) and watches it.
    let env = Env::new();
    let file = env.dir.path().join("config").join("settings.toml");
    let (_daemon, mut client) = env.start();
    assert_eq!(wait_error(&mut client, false), None);
    std::fs::write(&file, "[notify]\nsound = \"loud\"\n").unwrap();
    assert!(wait_error(&mut client, true).is_some());
    std::fs::write(&file, "[notify]\nsound = false\n").unwrap();

    /// Polls `daemon.status` until `settings_error` is (not) set; returns it.
    fn wait_error(client: &mut Client, wanted: bool) -> Option<String> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let status: DaemonStatusResult =
                client.call(Request::DaemonStatus).into_result().unwrap();
            if status.settings_error.is_some() == wanted {
                return status.settings_error;
            }
            assert!(
                Instant::now() < deadline,
                "settings_error stayed {:?}",
                status.settings_error
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    assert_eq!(wait_error(&mut client, false), None);

    std::fs::write(&file, "[notify]\nsound = \"loud\"\n").unwrap();
    let error = wait_error(&mut client, true).unwrap();
    assert!(error.contains("settings.toml:2:"), "{error}");

    std::fs::write(&file, "[notify]\nsound = true\n").unwrap();
    assert_eq!(wait_error(&mut client, false), None);
    client.call(Request::DaemonStop);
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
            dev_command: None,
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
            context: CommentContext::Doc,
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
                context: None,
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
    assert_eq!(ids, ["claude", "codex", "copilot", "fake", "omp", "shell"]);
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

fn save_attachment(session_id: Ulid, ext: &str, data: Vec<u8>) -> Request {
    Request::AttachmentSave(AttachmentSaveParams {
        session_id,
        ext: ext.into(),
        data,
    })
}

/// A pasted image lands in the session's attachments dir; bad params are rejected
/// (SPEC §20B.5).
#[test]
fn attachments_are_saved_per_session_and_bad_ones_rejected() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;

    let image = vec![0x89, b'P', b'N', b'G', 0, 1, 2];
    let saved: AttachmentSaveResult = client
        .call(save_attachment(id, "png", image.clone()))
        .into_result()
        .unwrap();
    let path = PathBuf::from(&saved.path);
    let dir = env.data_dir().join("attachments").join(id.to_string());
    assert_eq!(path.parent().unwrap(), dir);
    assert_eq!(path.extension().unwrap(), "png");
    assert_eq!(std::fs::read(&path).unwrap(), image);

    let too_big = vec![0; 10 * 1024 * 1024 + 1];
    for request in [
        save_attachment(id, "exe", vec![1]),
        save_attachment(id, "png", Vec::new()),
        save_attachment(id, "png", too_big),
    ] {
        assert_eq!(error_code(client.call(request)), ErrorCode::InvalidRequest);
    }
    assert_eq!(
        error_code(client.call(save_attachment(Ulid::new(), "png", vec![1]))),
        ErrorCode::NotFound
    );
    assert_eq!(dir.read_dir().unwrap().count(), 1);
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
    let unknown_agent = client.hook(id, "gemini", "busy", Value::Null);
    assert_eq!(error_code(unknown_agent), ErrorCode::InvalidRequest);

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
            context: CommentContext::Doc,
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
            context: None,
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
    client
        .call(send(id, "y"))
        .into_result::<SessionSendResult>()
        .unwrap();
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

fn unqueue(session_id: Ulid, send_id: Ulid) -> Request {
    Request::SessionUnqueue(SessionUnqueueParams {
        session_id,
        send_id,
    })
}

/// Texts sent while the agent works wait with a `send_id`; one removed from the queue is never
/// typed, the other is, and `session.queue` follows every change (SPEC §20B.2).
#[test]
fn a_queued_text_removed_while_working_is_never_typed() {
    let env = Env::new();
    let scenario = env.dir.path().join("twice.toml");
    let tail = "[[step]]\naction = \"print\"\ntext = \"step\"\n\n".repeat(4);
    std::fs::write(
        &scenario,
        format!(
            "[[step]]\naction = \"spinner\"\ntext = \"Working\"\nms = 4000\n\n\
             [[step]]\naction = \"ask\"\n\n{tail}\
             [[step]]\naction = \"ask\"\n\n{tail}\
             [[step]]\naction = \"print\"\ntext = \"Done.\"\n"
        ),
    )
    .unwrap();
    env.write_fake_profile_as(
        "twice",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "twice").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::Working);

    let kept: SessionSendResult = client.call(send(id, "kept-answer")).into_result().unwrap();
    let removed: SessionSendResult = client
        .call(send(id, "removed-answer"))
        .into_result()
        .unwrap();
    assert!(kept.queued && removed.queued, "{kept:?} {removed:?}");
    assert_ne!(kept.send_id, removed.send_id);
    let both = vec![
        QueuedSend {
            send_id: kept.send_id,
            text: "kept-answer".into(),
        },
        QueuedSend {
            send_id: removed.send_id,
            text: "removed-answer".into(),
        },
    ];
    client.wait_event(&Event::SessionQueue(SessionQueueEvent {
        session_id: id,
        items: both,
    }));
    client
        .call(unqueue(id, removed.send_id))
        .into_result::<()>()
        .unwrap();
    assert_eq!(
        error_code(client.call(unqueue(id, removed.send_id))),
        ErrorCode::NotFound
    );
    client.wait_event(&Event::SessionQueue(SessionQueueEvent {
        session_id: id,
        items: vec![QueuedSend {
            send_id: kept.send_id,
            text: "kept-answer".into(),
        }],
    }));

    // The first question takes the kept text; once written it can no longer be removed.
    client.wait_event(&Event::SessionQueue(SessionQueueEvent {
        session_id: id,
        items: Vec::new(),
    }));
    assert_eq!(
        error_code(client.call(unqueue(id, kept.send_id))),
        ErrorCode::NotFound
    );
    client.wait_state(id, SessionState::NeedsInput);
    client
        .call(send(id, "last-answer"))
        .into_result::<SessionSendResult>()
        .unwrap();
    let session = client.wait_state(id, SessionState::Exited);
    assert_eq!(session.exit_code, Some(0));
    let log = session_log(&env, id);
    assert!(log.contains("kept-answer"), "{log}");
    assert!(log.contains("last-answer"), "{log}");
    assert!(!log.contains("removed-answer"), "{log}");
    client.stop();
}

/// A session that exits with texts still waiting announces an empty queue.
#[test]
fn exiting_with_queued_texts_announces_an_empty_queue() {
    let env = Env::new();
    let scenario = env.dir.path().join("slow.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"spinner\"\ntext = \"Working\"\nms = 20000\n",
    )
    .unwrap();
    env.write_fake_profile_as("slow", &format!("['--scenario', '{}']", scenario.display()));
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "slow").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::Working);
    let sent: SessionSendResult = client.call(send(id, "later")).into_result().unwrap();
    assert!(sent.queued, "{sent:?}");
    let kill = Request::SessionKill(SessionKillParams {
        session_id: id,
        force: true,
    });
    client.call(kill).into_result::<()>().unwrap();
    client.wait_event(&Event::SessionQueue(SessionQueueEvent {
        session_id: id,
        items: Vec::new(),
    }));
    client.wait_state(id, SessionState::Exited);
    client.stop();
}

fn read(session_id: Ulid, lines: u16) -> Request {
    Request::SessionRead(SessionReadParams { session_id, lines })
}

#[test]
fn session_read_shows_the_prompt_line_and_the_answer_goes_through() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    let session = client.wait_state(id, SessionState::NeedsInput);

    // Read without attaching: the last non-blank line is the agent's question.
    let last: SessionReadResult = client.call(read(id, 1)).into_result().unwrap();
    assert_eq!(last.lines.len(), 1, "{last:?}");
    assert!(last.lines[0].contains("(y/n)"), "{last:?}");
    assert_eq!(last.state, SessionState::NeedsInput);
    assert_eq!(last.state_changed_at, session.state_changed_at);
    // More lines than the screen has: every non-blank row, top to bottom.
    let all: SessionReadResult = client.call(read(id, 200)).into_result().unwrap();
    assert!(all.lines.iter().all(|l| !l.trim().is_empty()), "{all:?}");
    assert_eq!(
        all.lines.first().map(String::as_str),
        Some("Fake agent ready.")
    );
    assert_eq!(all.lines.last(), last.lines.last());

    for lines in [0, 201] {
        assert_eq!(
            error_code(client.call(read(id, lines))),
            ErrorCode::InvalidRequest
        );
    }
    assert_eq!(
        error_code(client.call(read(Ulid::new(), 1))),
        ErrorCode::NotFound
    );

    // Quick reply: written at once (not queued), the agent finishes and exits.
    let sent: SessionSendResult = client.call(send(id, "y")).into_result().unwrap();
    assert!(!sent.queued, "{sent:?}");
    let session = client.wait_state(id, SessionState::Exited);
    assert_eq!(session.exit_code, Some(0));
    assert!(session_log(&env, id).contains("Done."));
    assert_eq!(
        error_code(client.call(read(id, 1))),
        ErrorCode::InvalidState
    );
    client.stop();
}

/// A fake agent edits two files (one twice, with hooks) and writes a third (no hook) in one
/// turn; at `done`, `session.turn` carries each path once, relative, sorted (SPEC §20A.3).
#[test]
fn a_turn_that_edits_two_files_reports_both() {
    let env = Env::new();
    let scenario = env.dir.path().join("editor.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"hook\"\nevent = \"busy\"\n\n\
         [[step]]\naction = \"write_file\"\npath = \"notes.txt\"\ncontent = \"x\"\n\n\
         [[step]]\naction = \"edit_files\"\npaths = [\"src/b.rs\", \"a.rs\", \"a.rs\"]\n\n\
         [[step]]\naction = \"sleep\"\nms = 1500\n\n\
         [[step]]\naction = \"hook\"\nevent = \"done\"\n\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    env.write_fake_profile_as(
        "editor",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    // Hooks decide the states: a slow `mk hook` run must not look like a quiet (idle) period.
    // Spawning `mk` prints nothing visible, so that wait no longer counts as work (ADR 0015).
    let path = env.dir.path().join("config/agents/editor.toml");
    let profile = std::fs::read_to_string(&path).unwrap();
    let hooks = profile
        .replace("mode = \"screen\"", "mode = \"hook+screen\"")
        .replace("idle_after_ms = 1000", "idle_after_ms = 8000");
    assert_ne!(hooks, profile);
    env.write_profile("editor", &hooks);
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "editor").into_result().unwrap();
    let id = spawned.session_id;

    let event =
        client.wait_event_where(|e| matches!(e, Event::SessionTurn(t) if t.session_id == id));
    let Event::SessionTurn(turn) = event else {
        unreachable!()
    };
    assert_eq!(turn.n, 1);
    assert!(turn.started_at <= turn.ended_at, "{turn:?}");
    // The turn ended when the `done` hook arrived.
    client.wait_event_where(|e| {
        matches!(e, Event::SessionState(s) if s.session_id == id
            && s.state == SessionState::Done && s.changed_at == turn.ended_at)
    });
    let paths: Vec<&str> = turn.files.iter().map(|f| f.path.as_str()).collect();
    // `notes.txt` was written without a hook: only the file watcher saw it.
    assert_eq!(paths, ["a.rs", "notes.txt", "src/b.rs"]);
    assert!(
        turn.files
            .iter()
            .all(|f| f.added.is_none() && f.removed.is_none())
    );
    client.stop();
}

/// A profile with `[usage]`: at the turn end the daemon finds the transcript of the agent's
/// native session id, announces `session.usage` and `session.usage` lists it (SPEC §20E.3).
#[test]
fn a_turn_end_reads_token_usage_from_the_agent_transcript() {
    let env = Env::new();
    let transcripts = env.dir.path().join("claude/projects/C--repo");
    std::fs::create_dir_all(&transcripts).unwrap();
    std::fs::write(
        transcripts.join("native-7.jsonl"),
        include_bytes!("../../mkrew-core/tests/fixtures/usage/claude.jsonl"),
    )
    .unwrap();
    let scenario = env.dir.path().join("usage.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"hook\"\nevent = \"busy\"\nsession_id = \"native-7\"\n\n\
         [[step]]\naction = \"sleep\"\nms = 300\n\n\
         [[step]]\naction = \"hook\"\nevent = \"done\"\nsession_id = \"native-7\"\n\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    env.write_fake_profile_as(
        "usage",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let path = env.dir.path().join("config/agents/usage.toml");
    let profile = std::fs::read_to_string(&path).unwrap();
    let hooks = profile.replace("mode = \"screen\"", "mode = \"hook+screen\"");
    assert_ne!(hooks, profile);
    env.write_profile(
        "usage",
        &format!("{hooks}\n[usage]\nformat = \"claude_jsonl\"\ncontext_window = 200000\n"),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "usage").into_result().unwrap();
    let id = spawned.session_id;

    let event =
        client.wait_event_where(|e| matches!(e, Event::SessionUsage(u) if u.session_id == id));
    let Event::SessionUsage(usage) = event else {
        unreachable!()
    };
    // The totals of `mkrew-core`'s Claude fixture.
    let expected = SessionUsageEvent {
        session_id: id,
        input: (2 + 688 + 48215) + (3 + 9000) + (1 + 1200 + 48903),
        output: 206 + 40 + 512,
        context: 1 + 1200 + 48903,
        context_window: Some(200_000),
    };
    assert_eq!(usage, expected);
    let listed: Vec<SessionUsageEvent> = client
        .call(Request::SessionUsage(ProjectRef {
            project_id: project.id,
        }))
        .into_result()
        .unwrap();
    assert_eq!(listed, [expected]);
    let unknown = client.call(Request::SessionUsage(ProjectRef {
        project_id: Ulid::new(),
    }));
    assert_eq!(error_code(unknown), ErrorCode::NotFound);
    client.stop();
}

/// In a git repo the `hook.report` that starts a turn is answered after the start snapshot:
/// the agent's first write (right after its hook returns) is not in it. The turn's files get
/// line counts from start to end snapshot; branch and index stay as they were (SPEC §20C.1).
#[test]
fn a_hook_turn_start_is_snapshotted_before_the_agent_edits() {
    let env = Env::new();
    let repo = env.repo();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let head = git(&repo, &["rev-parse", "HEAD"]);
    let scenario = env.dir.path().join("editor.toml");
    // The agent's start output is screen-detected as turn 1; the `done` hook ends it, so the
    // `busy` hook starts turn 2.
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"hook\"\nevent = \"done\"\n\n\
         [[step]]\naction = \"hook\"\nevent = \"busy\"\n\n\
         [[step]]\naction = \"write_file\"\npath = \"notes.txt\"\ncontent = \"x\\n\"\n\n\
         [[step]]\naction = \"edit_files\"\npaths = [\"a.txt\", \"a.txt\"]\n\n\
         [[step]]\naction = \"sleep\"\nms = 1500\n\n\
         [[step]]\naction = \"hook\"\nevent = \"done\"\n\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    env.write_fake_profile_as(
        "editor",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let path = env.dir.path().join("config/agents/editor.toml");
    let profile = std::fs::read_to_string(&path).unwrap();
    env.write_profile(
        "editor",
        &profile.replace("mode = \"screen\"", "mode = \"hook+screen\""),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let spawned: SessionSpawnResult = client.spawn(&project, "editor").into_result().unwrap();
    let id = spawned.session_id;

    let event = client
        .wait_event_where(|e| matches!(e, Event::SessionTurn(t) if t.session_id == id && t.n == 2));
    let Event::SessionTurn(turn) = event else {
        unreachable!()
    };
    let counts: Vec<_> = turn
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.added, f.removed))
        .collect();
    assert_eq!(
        counts,
        [("a.txt", Some(2), Some(0)), ("notes.txt", Some(1), Some(0))]
    );
    let start = format!("refs/mkrew/turns/{id}/2");
    assert_eq!(git(&repo, &["ls-tree", "--name-only", &start]), "a.txt");
    assert_eq!(git(&repo, &["show", &format!("{start}:a.txt")]), "one");
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        git(&repo, &["branch", "--format=%(refname)"]),
        "refs/heads/main"
    );
    assert_eq!(git(&repo, &["diff", "--cached", "--name-only"]), "");
    client.stop();
}

/// M11 restore (SPEC §20C.2) over 2 turns of the fake agent, driven by hooks (no screen
/// detection, so turn numbers do not depend on timing): `turn.list` returns both;
/// `turn.restore` is refused while the agent works, without `confirm` or with a stale
/// fingerprint; it puts back the files from before turn 2, then before turn 1, and leaves
/// HEAD and the index.
#[test]
fn turns_are_listed_previewed_and_restored() {
    let env = Env::new();
    let repo = env.repo();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let head = git(&repo, &["rev-parse", "HEAD"]);
    let scenario = env.dir.path().join("idle.toml");
    std::fs::write(&scenario, "[[step]]\naction = \"sleep\"\nms = 60000\n").unwrap();
    env.write_fake_profile_as(
        "editor",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let path = env.dir.path().join("config/agents/editor.toml");
    let profile = std::fs::read_to_string(&path).unwrap();
    env.write_profile(
        "editor",
        &profile.replace("mode = \"screen\"", "mode = \"hook\""),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let spawned: SessionSpawnResult = client.spawn(&project, "editor").into_result().unwrap();
    let id = spawned.session_id;
    let hook = |client: &mut Client, event: &str| {
        client
            .hook(id, "fake", event, Value::Null)
            .into_result::<()>()
            .unwrap();
    };
    let restore = |n, confirm, created: &[String], fingerprint: &str| {
        Request::TurnRestore(TurnRestoreParams {
            session_id: id,
            n,
            confirm,
            created: created.to_vec(),
            fingerprint: fingerprint.into(),
        })
    };
    let preview = |n| Request::TurnRestorePreview(TurnRef { session_id: id, n });

    let read = |path: &str| std::fs::read_to_string(repo.join(path)).unwrap();

    // Turn 1 edits a.txt. The `busy` reply comes after the start snapshot.
    hook(&mut client, "busy");
    std::fs::write(repo.join("a.txt"), "two\n").unwrap();
    hook(&mut client, "done");
    // Turn 2 creates notes.txt and edits a.txt again; restoring is refused meanwhile.
    hook(&mut client, "busy");
    std::fs::write(repo.join("a.txt"), "three\n").unwrap();
    std::fs::write(repo.join("notes.txt"), "x\n").unwrap();
    match client.call(restore(1, true, &[], "")).into_result::<()>() {
        Err(ResponseError::Daemon(body)) => {
            assert_eq!(body.code, ErrorCode::InvalidState);
            assert!(body.message.contains("working"), "{}", body.message);
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    hook(&mut client, "done");
    client
        .wait_event_where(|e| matches!(e, Event::SessionTurn(t) if t.session_id == id && t.n == 2));

    let turns: Vec<Turn> = client
        .call(Request::TurnList(SessionRef { session_id: id }))
        .into_result()
        .unwrap();
    assert_eq!(turns.iter().map(|t| t.n).collect::<Vec<_>>(), [1, 2]);
    let start = git(&repo, &["rev-parse", &format!("refs/mkrew/turns/{id}/2")]);
    assert_eq!(turns[1].start_tree.as_deref(), Some(start.as_str()));
    assert!(
        turns
            .iter()
            .all(|t| t.ended_at.is_some() && t.end_tree.is_some())
    );

    let shown: UndoPreview = client.call(preview(2)).into_result().unwrap();
    assert_eq!(shown.checkpoint, start);
    assert_eq!(shown.changed, ["a.txt"]);
    assert_eq!(shown.created, ["notes.txt"]);
    assert!(shown.added.is_empty());
    assert!(shown.diff.contains("-two") && shown.diff.contains("+three"));
    let unconfirmed = client.call(restore(2, false, &shown.created, &shown.fingerprint));
    assert_eq!(error_code(unconfirmed), ErrorCode::InvalidRequest);
    let stale = client.call(restore(2, true, &shown.created, "stale"));
    assert_eq!(error_code(stale), ErrorCode::InvalidState);
    assert_eq!(error_code(client.call(preview(9))), ErrorCode::NotFound);

    client
        .call(restore(2, true, &shown.created, &shown.fingerprint))
        .into_result::<()>()
        .unwrap();
    assert_eq!(read("a.txt"), "two\n");
    assert!(!repo.join("notes.txt").exists());

    let shown: UndoPreview = client.call(preview(1)).into_result().unwrap();
    assert_eq!(
        (shown.changed.as_slice(), shown.created.len()),
        (&["a.txt".to_owned()][..], 0)
    );
    client
        .call(restore(1, true, &shown.created, &shown.fingerprint))
        .into_result::<()>()
        .unwrap();
    assert_eq!(read("a.txt"), "one\n");
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&repo, &["status", "--porcelain"]), "");
    let unknown = client.call(Request::TurnList(SessionRef {
        session_id: Ulid::new(),
    }));
    assert_eq!(error_code(unknown), ErrorCode::NotFound);
    client.stop();
}

/// SPEC §20C.3–§20C.5: a hook-reported edit is announced with `session.edited` (line from the
/// payload) and attributed by `session.files`; `git.status`/`git.diff` with `since` = the
/// turn's start tree show everything changed after it, untracked files included.
#[test]
fn hook_edits_are_announced_attributed_and_diffed_since_a_turn() {
    let env = Env::new();
    let repo = env.repo();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let scenario = env.dir.path().join("idle.toml");
    std::fs::write(&scenario, "[[step]]\naction = \"sleep\"\nms = 60000\n").unwrap();
    env.write_fake_profile_as(
        "editor",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let path = env.dir.path().join("config/agents/editor.toml");
    let profile = std::fs::read_to_string(&path).unwrap();
    env.write_profile(
        "editor",
        &profile.replace("mode = \"screen\"", "mode = \"hook\""),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let spawned: SessionSpawnResult = client.spawn(&project, "editor").into_result().unwrap();
    let id = spawned.session_id;
    let files = |client: &mut Client, since_ms| {
        let files: SessionFiles = client
            .call(Request::SessionFiles(SessionFilesParams {
                project_id: project.id,
                since_ms,
            }))
            .into_result()
            .unwrap();
        files
    };

    client
        .hook(id, "fake", "busy", Value::Null)
        .into_result::<()>()
        .unwrap();
    std::fs::write(repo.join("a.txt"), "two\n").unwrap();
    let edit = json!({
        "tool_name": "Edit",
        "tool_input": {"file_path": "a.txt"},
        "tool_response": {"structuredPatch": [{"newStart": 1, "lines": ["-one", "+two"]}]}
    });
    client
        .hook(id, "fake", "PostToolUse", edit)
        .into_result::<()>()
        .unwrap();
    let Event::SessionEdited(edited) =
        client.wait_event_where(|e| matches!(e, Event::SessionEdited(e) if e.session_id == id))
    else {
        unreachable!()
    };
    assert_eq!((edited.path.as_str(), edited.line), ("a.txt", Some(1)));
    // The running turn's hook edits count already.
    assert_eq!(
        files(&mut client, 0),
        SessionFiles::from([(id, vec!["a.txt".into()])])
    );
    client
        .hook(id, "fake", "done", Value::Null)
        .into_result::<()>()
        .unwrap();
    client
        .wait_event_where(|e| matches!(e, Event::SessionTurn(t) if t.session_id == id && t.n == 1));
    assert_eq!(
        files(&mut client, 0),
        SessionFiles::from([(id, vec!["a.txt".into()])])
    );
    assert!(files(&mut client, i64::MAX).is_empty());
    let unknown = client.call(Request::SessionFiles(SessionFilesParams {
        project_id: Ulid::new(),
        since_ms: 0,
    }));
    assert_eq!(error_code(unknown), ErrorCode::NotFound);

    std::fs::write(repo.join("notes.txt"), "x\n").unwrap();
    let start = git(&repo, &["rev-parse", &format!("refs/mkrew/turns/{id}/1")]);
    let status: GitStatus = client
        .call(Request::GitStatus(GitStatusParams {
            project_id: project.id,
            worktree_task_id: None,
            mode: GitMode::Base,
            since: Some(start.clone()),
        }))
        .into_result()
        .unwrap();
    assert_eq!(status.merge_base.as_deref(), Some(start.as_str()));
    let paths: Vec<&str> = status
        .against_base
        .iter()
        .map(|f| f.path.as_str())
        .collect();
    assert_eq!(paths, ["a.txt", "notes.txt"]);
    let diff: FileDiff = client
        .call(Request::GitDiff(GitDiffParams {
            project_id: project.id,
            worktree_task_id: None,
            path: "a.txt".into(),
            mode: GitMode::Base,
            staged: false,
            commit: None,
            since: Some(start),
        }))
        .into_result()
        .unwrap();
    assert_eq!((diff.added, diff.removed), (1, 1));
    // The Git view's reads changed nothing.
    assert_eq!(
        git(&repo, &["status", "--porcelain"]),
        "M a.txt\n?? notes.txt"
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
            context: CommentContext::Doc,
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
            context: CommentContext::Doc,
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
            context: None,
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

/// Diff-line comments (M7 E2E): a comment on new-side lines of a code file is sent with the
/// "fix the code" prompt; the fake edits the file and answers with `mk comment reply`.
#[test]
fn fake_agent_fixes_code_for_diff_comments_and_replies() {
    let env = Env::new();
    let repo = env.repo();
    std::fs::create_dir_all(repo.join("src")).unwrap();
    let code = "pub fn add(a: i32, b: i32) -> i32 {\n    a - b\n}\n";
    let scenario = env.dir.path().join("fixer.toml");
    std::fs::write(
        &scenario,
        format!(
            "[[step]]\naction = \"write_file\"\npath = \"src/lib.rs\"\ncontent = '''{code}'''\n\n\
             [[step]]\naction = \"spinner\"\ntext = \"coding\"\nms = 600\n\n\
             [[step]]\naction = \"reply_comments\"\ncomments = 1\n\n\
             [[step]]\naction = \"print\"\ntext = \"Fixed src/lib.rs\"\n"
        ),
    )
    .unwrap();
    env.write_fake_profile_as(
        "fixer",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let spawned: SessionSpawnResult = client.spawn(&project, "fixer").into_result().unwrap();
    let id = spawned.session_id;
    let deadline = Instant::now() + TIMEOUT;
    while std::fs::read_to_string(repo.join("src/lib.rs"))
        .ok()
        .as_deref()
        != Some(code)
    {
        assert!(Instant::now() < deadline, "the fake never wrote the code");
        std::thread::sleep(Duration::from_millis(20));
    }

    let mut create = |file: &str, context| {
        let params = CommentCreateParams {
            project_id: project.id,
            file_path: file.into(),
            start_line: 2,
            end_line: 2,
            quote: "a - b".into(),
            body: "This subtracts.".into(),
            context,
        };
        let created: DocComment = client
            .call(Request::CommentCreate(params))
            .into_result()
            .unwrap();
        created
    };
    let diff = create("src/lib.rs", CommentContext::Diff);
    assert_eq!(diff.context, CommentContext::Diff);
    // One prompt cannot both edit a document and fix code.
    let doc = create("src/lib.rs", CommentContext::Doc);
    assert_eq!(
        error_code(client.call(send_comments(&[diff.id, doc.id], id))),
        ErrorCode::InvalidRequest
    );
    client
        .call(Request::CommentResolve(CommentRef { comment_id: doc.id }))
        .into_result::<DocComment>()
        .unwrap();

    let sent: Vec<DocComment> = client
        .call(send_comments(&[diff.id], id))
        .into_result()
        .unwrap();
    assert_eq!(sent[0].sent_snapshot.as_deref(), Some(code));
    wait_comment(&mut client, diff.id, CommentStatus::Addressed);
    let session = client.wait_state(id, SessionState::Exited);
    assert_eq!(session.exit_code, Some(0));
    assert_eq!(
        std::fs::read_to_string(repo.join("src/lib.rs")).unwrap(),
        format!("{code}Addressed {}: This subtracts.\n", diff.id)
    );

    let list = |context| CommentListParams {
        project_id: project.id,
        file_path: Some("src/lib.rs".into()),
        context,
    };
    let diffs: Vec<DocComment> = client
        .call(Request::CommentList(list(Some(CommentContext::Diff))))
        .into_result()
        .unwrap();
    let replies: Vec<_> = diffs
        .iter()
        .map(|c| (c.parent_id, c.context, c.author.as_str(), c.body.as_str()))
        .collect();
    assert_eq!(
        replies,
        [
            (
                None,
                CommentContext::Diff,
                DocComment::USER,
                "This subtracts."
            ),
            (
                Some(diff.id),
                CommentContext::Diff,
                "fixer",
                "Addressed: This subtracts."
            ),
        ]
    );
    let all: Vec<DocComment> = client
        .call(Request::CommentList(list(None)))
        .into_result()
        .unwrap();
    assert_eq!(all.len(), 3);
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
        dev_command: None,
        dev_autostart: None,
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
        worktree: false,
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
            worktree: Some(true),
        }))
        .into_result()
        .unwrap();
    assert_eq!(
        (edited.title.as_str(), edited.agent_profile.as_deref()),
        ("deux", None)
    );
    assert_eq!(edited.prompt, "prompt for two");
    assert!(edited.worktree && !first.worktree);
    assert_eq!(
        (edited.worktree_path.as_deref(), edited.branch.as_deref()),
        (None, None)
    );
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
        worktree: None,
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
            worktree_task_id: None,
            mode,
            since: None,
        })
    };
    let diff = |path: &str, mode, commit: Option<&str>| {
        Request::GitDiff(GitDiffParams {
            project_id: project.id,
            worktree_task_id: None,
            path: path.into(),
            mode,
            staged: false,
            commit: commit.map(str::to_owned),
            since: None,
        })
    };
    let since_status = |since: &str| {
        Request::GitStatus(GitStatusParams {
            project_id: project.id,
            worktree_task_id: None,
            mode: GitMode::Base,
            since: Some(since.into()),
        })
    };
    let since_diff = |since: &str| {
        Request::GitDiff(GitDiffParams {
            project_id: project.id,
            worktree_task_id: None,
            path: "a.txt".into(),
            mode: GitMode::Base,
            staged: false,
            commit: None,
            since: Some(since.into()),
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
    assert_eq!(against.merge_base, Some(fork.clone()));
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
    // A task's checkpoint needs no base: changes since it to the working tree.
    let since: GitStatus = client.call(since_status(&fork)).into_result().unwrap();
    assert_eq!(since.merge_base.as_deref(), Some(fork.as_str()));
    assert_eq!(since.against_base.len(), 1);
    let shown: FileDiff = client.call(since_diff(&fork)).into_result().unwrap();
    assert_eq!((shown.added, shown.hunks.len()), (1, 1));
    assert_eq!(
        error_code(client.call(since_status("nope"))),
        ErrorCode::NotFound
    );
    assert_eq!(
        error_code(client.call(status(&outside, GitMode::Working))),
        ErrorCode::InvalidState
    );
    for bad in [
        diff("../a.txt", GitMode::Working, None),
        diff("a.txt", GitMode::Working, Some("--output=x")),
        since_status("--output=x"),
        since_diff("--output=x"),
    ] {
        assert_eq!(error_code(client.call(bad)), ErrorCode::InvalidRequest);
    }
    let missing = Request::GitStatus(GitStatusParams {
        project_id: Ulid::new(),
        worktree_task_id: None,
        mode: GitMode::Working,
        since: None,
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
            worktree_task_id: None,
            mode: GitMode::Working,
            since: None,
        });
        client.call(request).into_result().unwrap()
    };
    let names = |files: &[mkrew_protocol::ChangedFile]| -> Vec<String> {
        files.iter().map(|f| f.path.clone()).collect()
    };
    let by_path = |paths: &[&str]| GitStageParams {
        project_id: id,
        worktree_task_id: None,
        paths: paths.iter().map(|&p| p.to_owned()).collect(),
        hunk: None,
    };
    let by_hunk = |hunk: &mkrew_protocol::Hunk| GitStageParams {
        project_id: id,
        worktree_task_id: None,
        paths: vec![],
        hunk: Some(HunkTarget {
            path: "a.txt".into(),
            hunk: hunk.clone(),
        }),
    };
    let diff = |client: &mut Client, staged| -> FileDiff {
        let request = Request::GitDiff(GitDiffParams {
            project_id: id,
            worktree_task_id: None,
            path: "a.txt".into(),
            mode: GitMode::Working,
            staged,
            commit: None,
            since: None,
        });
        client.call(request).into_result().unwrap()
    };
    let changed_event = Event::GitChanged(GitChangedEvent {
        project_id: id,
        worktree_task_id: None,
    });

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
            worktree_task_id: None,
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
            worktree_task_id: None,
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
            worktree_task_id: None,
            hunk: by_hunk(&unstaged[0]).hunk,
            ..by_path(&["a.txt"])
        }),
        Request::GitDiscard(GitDiscardParams {
            project_id: id,
            worktree_task_id: None,
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
            worktree_task_id: None,
            limit: GitLogParams::DEFAULT_LIMIT,
            path: path.map(str::to_owned),
        })
    };
    let show = |commit: &str| {
        Request::GitShow(GitShowParams {
            project_id: id,
            worktree_task_id: None,
            commit: commit.into(),
        })
    };
    let changed_event = Event::GitChanged(GitChangedEvent {
        project_id: id,
        worktree_task_id: None,
    });

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
    let push = Request::GitPush(GitRemoteParams {
        project_id: id,
        worktree_task_id: None,
    });
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
    let fetch = Request::GitFetch(GitRemoteParams {
        project_id: id,
        worktree_task_id: None,
    });
    let _: () = client.call(fetch).into_result().unwrap();
    client.wait_event(&changed_event);
    let status: GitStatus = client
        .call(Request::GitStatus(GitStatusParams {
            project_id: id,
            worktree_task_id: None,
            mode: GitMode::Working,
            since: None,
        }))
        .into_result()
        .unwrap();
    assert_eq!(status.upstream.as_deref(), Some("origin/main"));
    assert_eq!((status.ahead, status.behind), (0, 1));
    client.stop();
}

#[test]
fn project_dev_server_settings_are_added_updated_and_cleared() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    let project: Project = client
        .call(Request::ProjectAdd(ProjectAddParams {
            path: env.repo().to_string_lossy().into_owned(),
            name: None,
            dev_command: Some(" npm run dev ".into()),
        }))
        .into_result()
        .unwrap();
    assert_eq!(project.dev_command.as_deref(), Some("npm run dev"));
    assert!(!project.dev_autostart);
    client.wait_event(&Event::ProjectUpdated(project.clone()));

    let update = |dev_command, dev_autostart| {
        Request::ProjectUpdate(ProjectUpdateParams {
            project_id: project.id,
            name: None,
            default_agent: None,
            dev_command,
            dev_autostart,
        })
    };
    let on: Project = client.call(update(None, Some(true))).into_result().unwrap();
    assert_eq!(on.dev_command.as_deref(), Some("npm run dev"));
    assert!(on.dev_autostart);
    client.wait_event(&Event::ProjectUpdated(on.clone()));

    let cleared: Project = client.call(update(Some(None), None)).into_result().unwrap();
    assert_eq!(cleared.dev_command, None);
    assert!(cleared.dev_autostart);
    client.wait_event(&Event::ProjectUpdated(cleared.clone()));
    let listed: Vec<Project> = client.call(Request::ProjectList).into_result().unwrap();
    assert_eq!(listed, vec![cleared]);
    client.stop();
}

fn dev_start(project: &Project) -> Request {
    Request::ProjectDevStart(DevStartParams {
        project_id: project.id,
        cols: 80,
        rows: 24,
    })
}

fn dev_stop(project: &Project) -> Request {
    Request::ProjectDevStop(ProjectRef {
        project_id: project.id,
    })
}

fn update_dev(project: &Project, command: Option<&str>, autostart: Option<bool>) -> Request {
    Request::ProjectUpdate(ProjectUpdateParams {
        project_id: project.id,
        name: None,
        default_agent: None,
        dev_command: command.map(|c| Some(c.to_owned())),
        dev_autostart: autostart,
    })
}

/// Waits until `path` exists (written by a command typed into a shell).
fn wait_file(path: &Path) {
    let deadline = Instant::now() + TIMEOUT;
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "{} never appeared",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The project's live shell sessions titled "dev".
fn dev_sessions(client: &mut Client, project: &Project) -> Vec<Session> {
    let all: Vec<Session> = client
        .call(Request::SessionList(SessionListParams {
            project_id: Some(project.id),
        }))
        .into_result()
        .unwrap();
    all.into_iter()
        .filter(|s| s.title.as_deref() == Some("dev") && s.state != SessionState::Exited)
        .collect()
}

#[test]
fn dev_server_runs_once_in_its_own_tab_stops_and_auto_starts_after_a_restart() {
    let env = Env::new();
    let repo = env.repo();
    let marker = repo.join("dev-started.txt");
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    assert_eq!(
        error_code(client.call(dev_start(&project))),
        ErrorCode::InvalidState
    );
    // Nothing runs: stopping is still fine; an unknown project is not.
    client.call(dev_stop(&project)).into_result::<()>().unwrap();
    let unknown = Project {
        id: Ulid::new(),
        ..project.clone()
    };
    assert_eq!(
        error_code(client.call(dev_stop(&unknown))),
        ErrorCode::NotFound
    );
    // A fake dev server: works in cmd, pwsh and sh alike.
    let command = "echo started> dev-started.txt";
    client
        .call(update_dev(&project, Some(command), None))
        .into_result::<Project>()
        .unwrap();

    let first: DevServer = client.call(dev_start(&project)).into_result().unwrap();
    assert!(first.started);
    let session = client.session(first.session_id);
    assert_eq!(
        (
            session.kind,
            session.agent_profile.as_str(),
            session.title.as_deref()
        ),
        (SessionKind::Shell, "shell", Some("dev"))
    );
    let tabs = list_tabs(&mut client, &project);
    assert_eq!(tabs.len(), 1);
    assert_eq!(Some(tabs[0].id), first.tab_id);
    assert_eq!(tabs[0].title, "dev");
    assert_eq!(
        tabs[0].layout,
        json!({"session": first.session_id.to_string()})
    );
    client.wait_event(&Event::TabUpdated(tabs[0].clone()));
    wait_file(&marker);

    // Starting again answers with the running one.
    let again: DevServer = client.call(dev_start(&project)).into_result().unwrap();
    assert_eq!(
        again,
        DevServer {
            started: false,
            ..first.clone()
        }
    );
    client.call(dev_stop(&project)).into_result::<()>().unwrap();
    client.wait_state(first.session_id, SessionState::Exited);
    assert!(dev_sessions(&mut client, &project).is_empty());

    // Auto-start: the next daemon starts it in the same tab (its pane had exited).
    client
        .call(update_dev(&project, None, Some(true)))
        .into_result::<Project>()
        .unwrap();
    std::fs::remove_file(&marker).unwrap();
    client.stop();
    daemon.wait_exit();
    let (_daemon, mut client) = env.start();
    wait_file(&marker);
    let running = dev_sessions(&mut client, &project);
    assert_eq!(running.len(), 1);
    let tabs = list_tabs(&mut client, &project);
    assert_eq!(tabs.len(), 1);
    assert_eq!(
        tabs[0].layout,
        json!({"session": running[0].id.to_string()})
    );
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
        .into_result::<SessionSendResult>()
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

/// SPEC §8: an agent pane without a task that a tab showed when the daemon stopped comes back
/// in the same pane with its conversation. A session in no tab, or one that exited by itself
/// before the stop, stays exited, and each stop resumes the current pane, never an older one.
#[test]
fn restart_resumes_free_agent_panes_in_tabs_once() {
    let env = Env::new();
    let fake = built_bin("mkrew-fakeagent");
    let profile = |name: &str, steps: &str| {
        let scenario = env.dir.path().join(format!("{name}.toml"));
        std::fs::write(&scenario, steps).unwrap();
        env.write_fake_profile_as(name, &format!("['--scenario', '{}']", scenario.display()));
        let path = env.dir.path().join(format!("config/agents/{name}.toml"));
        let text = std::fs::read_to_string(&path).unwrap();
        let resume = format!(
            "resume = '\"{}\" --resume {{session_id}}'\n",
            fake.display()
        );
        std::fs::write(
            &path,
            text.replace("[state]", &format!("{resume}\n[state]")),
        )
        .unwrap();
    };
    profile(
        "resumable",
        "[[step]]\naction = \"hook\"\nevent = \"busy\"\nsession_id = \"native-7\"\n\
         [[step]]\naction = \"ask\"\n",
    );
    profile(
        "quits",
        "[[step]]\naction = \"hook\"\nevent = \"busy\"\nsession_id = \"native-8\"\n\
         [[step]]\naction = \"exit\"\ncode = 0\n",
    );

    let list = |client: &mut Client| -> Vec<Session> {
        client
            .call(Request::SessionList(SessionListParams::default()))
            .into_result()
            .unwrap()
    };
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let shown: SessionSpawnResult = client.spawn(&project, "resumable").into_result().unwrap();
    let hidden: SessionSpawnResult = client.spawn(&project, "resumable").into_result().unwrap();
    let quit: SessionSpawnResult = client.spawn(&project, "quits").into_result().unwrap();
    let layout = |shown: Ulid| {
        json!({"split": "vertical", "ratio": 0.5, "children": [
            {"session": shown.to_string()}, {"session": quit.session_id.to_string()}
        ]})
    };
    let tab: TerminalTab = client
        .call(create_tab(&project, "agents"))
        .into_result()
        .unwrap();
    client
        .call(Request::TabSetLayout(TabSetLayoutParams {
            tab_id: tab.id,
            layout: layout(shown.session_id),
        }))
        .into_result::<()>()
        .unwrap();
    for id in [shown.session_id, hidden.session_id] {
        let deadline = Instant::now() + TIMEOUT;
        while client.session(id).native_session_id.is_none() {
            assert!(Instant::now() < deadline, "no native session id reported");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    client.wait_state(quit.session_id, SessionState::Exited);
    assert_eq!(
        client.session(quit.session_id).native_session_id.as_deref(),
        Some("native-8")
    );
    client.stop();
    assert!(daemon.wait_exit().success());

    let (mut daemon, mut client) = env.start();
    let sessions = list(&mut client);
    assert_eq!(
        sessions.len(),
        4,
        "only the shown pane came back: {sessions:?}"
    );
    let resumed = sessions
        .iter()
        .find(|s| ![shown.session_id, hidden.session_id, quit.session_id].contains(&s.id))
        .unwrap()
        .clone();
    assert_eq!(
        (
            resumed.task_id,
            resumed.agent_profile.as_str(),
            resumed.native_session_id.as_deref()
        ),
        (None, "resumable", Some("native-7"))
    );
    client.wait_state(resumed.id, SessionState::NeedsInput);
    let attached: SessionAttachResult = client.attach(resumed.id).into_result().unwrap();
    let screen = screen_text(&attached.snapshot);
    assert!(screen.contains("Resumed: native-7"), "{screen}");
    assert_eq!(
        list_tabs(&mut client, &project)[0].layout,
        layout(resumed.id)
    );

    client.stop();
    assert!(daemon.wait_exit().success());
    let (_daemon, mut client) = env.start();
    let sessions = list(&mut client);
    assert_eq!(
        sessions.len(),
        5,
        "one more pane, resumed from the current one: {sessions:?}"
    );
    let newest = sessions
        .iter()
        .max_by_key(|s| (s.started_at, s.id))
        .unwrap();
    assert_eq!(
        list_tabs(&mut client, &project)[0].layout,
        layout(newest.id)
    );
    client.stop();
}

/// M13 worktree task (SPEC §20E.1): the first run adds the task's worktree and branch from
/// HEAD, copies `.worktreeinclude` entries, names the skipped ones in the session's first
/// line and runs the agent in the project's folder inside the worktree; the user's checkout
/// is untouched. The task keeps its worktree (no turning it off, no delete), and a daemon
/// restart resumes the agent in the same folder.
#[test]
fn worktree_tasks_run_and_resume_in_their_own_worktree() {
    let env = Env::new();
    let scenario = env.dir.path().join("worktree.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"write_file\"\npath = \"made.txt\"\ncontent = \"run\"\n\
         [[step]]\naction = \"hook\"\nevent = \"busy\"\nsession_id = \"native-wt\"\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    let resumed_scenario = env.dir.path().join("resumed.toml");
    std::fs::write(
        &resumed_scenario,
        "[[step]]\naction = \"write_file\"\npath = \"resumed.txt\"\ncontent = \"again\"\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    let args = format!("['--scenario', '{}']", scenario.display());
    env.write_fake_profile_as("wt", &args);
    let path = env.dir.path().join("config/agents/wt.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    let resume = format!(
        "resume = '\"{}\" --resume {{session_id}} --scenario \"{}\"'\n",
        built_bin("mkrew-fakeagent").display(),
        resumed_scenario.display()
    );
    std::fs::write(
        &path,
        text.replace("[state]", &format!("{resume}\n[state]")),
    )
    .unwrap();

    let repo = env.repo();
    let app = repo.join("app");
    std::fs::create_dir_all(&app).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join(".gitignore"), ".env\n").unwrap();
    std::fs::write(app.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    std::fs::write(repo.join(".env"), "SECRET=1\n").unwrap();
    std::fs::write(repo.join(".worktreeinclude"), ".env\nmissing.txt\n").unwrap();
    std::fs::write(app.join("a.txt"), "uncommitted\n").unwrap();
    let main_state = |repo: &Path| {
        [
            git(repo, &["rev-parse", "HEAD"]),
            git(repo, &["symbolic-ref", "HEAD"]),
            git(repo, &["status", "--porcelain", "--ignored"]),
        ]
    };
    let before = main_state(&repo);

    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&app);
    let task: Task = client
        .call(Request::TaskCreate(TaskCreateParams {
            project_id: project.id,
            title: "Fix the login".into(),
            prompt: String::new(),
            agent_profile: Some("wt".into()),
            position: None,
            worktree: true,
        }))
        .into_result()
        .unwrap();
    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    let task = wait_task(&mut client, task.id, |t| {
        t.worktree_path.is_some() && t.checkpoint.is_some()
    });
    let root = PathBuf::from(task.worktree_path.clone().unwrap());
    assert!(
        root.ends_with(Path::new(&project.id.to_string()).join(task.id.to_string())),
        "{}",
        root.display()
    );
    let branch = task.branch.clone().unwrap();
    assert!(branch.starts_with("mkrew/fix-the-login-"), "{branch}");
    assert_eq!(task.checkpoint.as_deref(), Some(before[0].as_str()));
    assert_eq!(
        git(&root, &["symbolic-ref", "HEAD"]),
        format!("refs/heads/{branch}")
    );
    let deadline = Instant::now() + TIMEOUT;
    while client.session(run.session_id).native_session_id.is_none() {
        assert!(Instant::now() < deadline, "no native session id reported");
        std::thread::sleep(Duration::from_millis(50));
    }
    client.wait_state(run.session_id, SessionState::NeedsInput);
    // The agent worked in the project's folder of the worktree, with the copied `.env`.
    assert_eq!(
        std::fs::read_to_string(root.join("app/made.txt")).unwrap(),
        "run"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("app/a.txt")).unwrap(),
        "a\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".env")).unwrap(),
        "SECRET=1\n"
    );
    assert!(!app.join("made.txt").exists());
    assert_eq!(main_state(&repo), before);
    let banner = "mkrew: .worktreeinclude skipped missing.txt: not found";
    let attached: SessionAttachResult = client.attach(run.session_id).into_result().unwrap();
    let screen = screen_text(&attached.snapshot);
    assert!(screen.contains(banner), "{screen}");

    // Undo looks at the worktree, not at the user's uncommitted `a.txt`.
    let preview: UndoPreview = client
        .call(Request::TaskUndoPreview(TaskRef { task_id: task.id }))
        .into_result()
        .unwrap();
    assert!(preview.changed.is_empty(), "{preview:?}");
    assert_eq!(preview.created, ["made.txt"]);
    // Turn snapshots are of the worktree: never restored into the project folder (T13.4).
    // The turn is recorded shortly after the state change.
    client.wait_event_where(
        |e| matches!(e, Event::SessionTurn(t) if t.session_id == run.session_id && t.n == 1),
    );
    let restore = Request::TurnRestorePreview(TurnRef {
        session_id: run.session_id,
        n: 1,
    });
    assert_eq!(error_code(client.call(restore)), ErrorCode::InvalidState);

    let turn_off = Request::TaskUpdate(TaskUpdateParams {
        task_id: task.id,
        title: None,
        prompt: None,
        agent_profile: None,
        worktree: Some(false),
    });
    assert_eq!(error_code(client.call(turn_off)), ErrorCode::InvalidState);
    let delete = Request::TaskDelete(TaskRef { task_id: task.id });
    assert_eq!(error_code(client.call(delete)), ErrorCode::InvalidState);

    client.stop();
    assert!(daemon.wait_exit().success());
    let log = session_log(&env, run.session_id);
    assert!(log.starts_with(&format!("{banner}\r\n")), "{log}");
    let (_daemon, mut client) = env.start();
    // Resumed in the same folder: no second worktree, the resumed agent writes there too.
    wait_file(&root.join("app/resumed.txt"));
    assert!(!app.join("resumed.txt").exists());
    let tasks = list_tasks(&mut client, &project);
    assert_eq!(tasks[0].worktree_path, task.worktree_path);
    assert_eq!(tasks[0].branch, task.branch);
    assert_eq!(main_state(&repo), before);
    client.stop();
}

/// T13.4: `fs.*` and `git.*` with `worktree_task_id` work on the project's folder inside the
/// task's worktree, which is watched (`git.changed` names the task); the user's checkout and
/// its views are untouched.
#[test]
fn fs_and_git_requests_target_a_task_worktree() {
    let env = Env::new();
    let scenario = env.dir.path().join("worktree.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"write_file\"\npath = \"made.txt\"\ncontent = \"run\"\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    env.write_fake_profile_as("wt", &format!("['--scenario', '{}']", scenario.display()));
    let repo = env.repo();
    let app = repo.join("app");
    std::fs::create_dir_all(&app).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    for (key, value) in [
        ("core.autocrlf", "false"),
        ("user.name", "t"),
        ("user.email", "t@t"),
        ("commit.gpgsign", "false"),
    ] {
        git(&repo, &["config", key, value]);
    }
    std::fs::write(app.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let main_head = git(&repo, &["rev-parse", "HEAD"]);

    let (_daemon, mut client) = env.start();
    let project = client.add_project(&app);
    let create = |worktree| {
        Request::TaskCreate(TaskCreateParams {
            project_id: project.id,
            title: "Worktree views".into(),
            prompt: String::new(),
            agent_profile: Some("wt".into()),
            position: None,
            worktree,
        })
    };
    let task: Task = client.call(create(true)).into_result().unwrap();
    let plain: Task = client.call(create(false)).into_result().unwrap();
    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    client.wait_state(run.session_id, SessionState::NeedsInput);
    let task = wait_task(&mut client, task.id, |t| t.worktree_path.is_some());
    let root = PathBuf::from(task.worktree_path.clone().unwrap());
    let wt = Some(task.id);

    let list = |client: &mut Client, worktree_task_id| -> Vec<String> {
        let request = Request::FsList(FsListParams {
            project_id: project.id,
            worktree_task_id,
            dir: String::new(),
            show_ignored: false,
        });
        let entries: Vec<FsEntry> = client.call(request).into_result().unwrap();
        entries.into_iter().map(|e| e.path).collect()
    };
    assert_eq!(list(&mut client, wt), ["a.txt", "made.txt"]);
    assert_eq!(list(&mut client, None), ["a.txt"]);
    let read = Request::FsRead(FsReadParams {
        project_id: project.id,
        worktree_task_id: wt,
        path: "made.txt".into(),
    });
    let content: FsContent = client.call(read).into_result().unwrap();
    assert_eq!(content.content.as_deref(), Some("run"));
    let find = |client: &mut Client, worktree_task_id| -> Vec<String> {
        let request = Request::FsFind(FsFindParams {
            project_id: project.id,
            worktree_task_id,
            query: "made".into(),
            limit: 10,
        });
        let found: Vec<FsMatch> = client.call(request).into_result().unwrap();
        found.into_iter().map(|m| m.path).collect()
    };
    assert_eq!(find(&mut client, wt), ["made.txt"]);
    assert!(find(&mut client, None).is_empty());

    let status = |client: &mut Client, worktree_task_id| -> GitStatus {
        let request = Request::GitStatus(GitStatusParams {
            project_id: project.id,
            worktree_task_id,
            mode: GitMode::Working,
            since: None,
        });
        client.call(request).into_result().unwrap()
    };
    let in_worktree = status(&mut client, wt);
    assert_eq!(in_worktree.branch, task.branch);
    assert_eq!(in_worktree.untracked[0].path, "made.txt");
    let main = status(&mut client, None);
    assert_eq!(main.branch.as_deref(), Some("main"));
    assert!(main.untracked.is_empty());

    // The worktree's own watcher reports its changes.
    client.events.clear();
    std::fs::write(root.join("app/later.txt"), "x").unwrap();
    client.wait_event(&Event::GitChanged(GitChangedEvent {
        project_id: project.id,
        worktree_task_id: wt,
    }));
    // ... and drops the worktree's cached `fs.find` list.
    let request = Request::FsFind(FsFindParams {
        project_id: project.id,
        worktree_task_id: wt,
        query: "later".into(),
        limit: 10,
    });
    let found: Vec<FsMatch> = client.call(request).into_result().unwrap();
    assert_eq!(found[0].path, "later.txt");

    // Stage and commit inside the worktree: its branch moves, `main` does not.
    let stage = Request::GitStage(GitStageParams {
        project_id: project.id,
        worktree_task_id: wt,
        paths: vec!["made.txt".into()],
        hunk: None,
    });
    let _: () = client.call(stage).into_result().unwrap();
    let commit = Request::GitCommit(GitCommitParams {
        project_id: project.id,
        worktree_task_id: wt,
        message: "made in the worktree".into(),
    });
    let made: GitCommitResult = client.call(commit).into_result().unwrap();
    let log = |client: &mut Client, worktree_task_id| -> Vec<String> {
        let request = Request::GitLog(GitLogParams {
            project_id: project.id,
            worktree_task_id,
            limit: 10,
            path: None,
        });
        let commits: Vec<Commit> = client.call(request).into_result().unwrap();
        commits.into_iter().map(|c| c.summary).collect()
    };
    assert_eq!(log(&mut client, wt), ["made in the worktree", "one"]);
    assert_eq!(log(&mut client, None), ["one"]);
    let show = Request::GitShow(GitShowParams {
        project_id: project.id,
        worktree_task_id: wt,
        commit: made.commit,
    });
    let detail: CommitDetail = client.call(show).into_result().unwrap();
    assert_eq!(detail.files[0].path, "made.txt");
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), main_head);
    assert!(!app.join("made.txt").exists());

    // Unknown task, a task without a worktree, a task of another project.
    let other = client.add_project(env.dir.path());
    let fs_list = |project_id, worktree_task_id| {
        Request::FsList(FsListParams {
            project_id,
            worktree_task_id,
            dir: String::new(),
            show_ignored: false,
        })
    };
    assert_eq!(
        error_code(client.call(fs_list(project.id, Some(Ulid::new())))),
        ErrorCode::NotFound
    );
    assert_eq!(
        error_code(client.call(fs_list(project.id, Some(plain.id)))),
        ErrorCode::InvalidState
    );
    assert_eq!(
        error_code(client.call(fs_list(other.id, wt))),
        ErrorCode::InvalidRequest
    );
    client.stop();
}

/// T13.5: `task.merge` needs `confirm` and a clean worktree, then merges the task's branch into
/// the user's checkout with a merge commit; `task.remove_worktree` is refused while the agent
/// runs there, then removes the (watched) worktree and its merged branch so the task can be
/// deleted.
#[test]
fn worktree_tasks_merge_back_and_remove_their_worktree() {
    let env = Env::new();
    let scenario = env.dir.path().join("worktree.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"write_file\"\npath = \"made.txt\"\ncontent = \"run\"\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    env.write_fake_profile_as("wt", &format!("['--scenario', '{}']", scenario.display()));
    let repo = env.repo();
    let app = repo.join("app");
    std::fs::create_dir_all(&app).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    for (key, value) in [
        ("core.autocrlf", "false"),
        ("user.name", "t"),
        ("user.email", "t@t"),
        ("commit.gpgsign", "false"),
    ] {
        git(&repo, &["config", key, value]);
    }
    std::fs::write(app.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let main_head = git(&repo, &["rev-parse", "HEAD"]);

    let (_daemon, mut client) = env.start();
    let project = client.add_project(&app);
    let task: Task = client
        .call(Request::TaskCreate(TaskCreateParams {
            project_id: project.id,
            title: "Merge me".into(),
            prompt: String::new(),
            agent_profile: Some("wt".into()),
            position: None,
            worktree: true,
        }))
        .into_result()
        .unwrap();
    let merge = |confirm| {
        Request::TaskMerge(TaskConfirmParams {
            task_id: task.id,
            confirm,
        })
    };
    let remove = |confirm| {
        Request::TaskRemoveWorktree(TaskConfirmParams {
            task_id: task.id,
            confirm,
        })
    };
    // No worktree yet.
    assert_eq!(
        error_code(client.call(merge(true))),
        ErrorCode::InvalidState
    );
    assert_eq!(
        error_code(client.call(remove(true))),
        ErrorCode::InvalidState
    );

    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    client.wait_state(run.session_id, SessionState::NeedsInput);
    let task = wait_task(&mut client, task.id, |t| t.worktree_path.is_some());
    let root = PathBuf::from(task.worktree_path.clone().unwrap());
    let branch = task.branch.clone().unwrap();
    // A Files view on the worktree keeps it watched.
    let list = Request::FsList(FsListParams {
        project_id: project.id,
        worktree_task_id: Some(task.id),
        dir: String::new(),
        show_ignored: false,
    });
    let _: Vec<FsEntry> = client.call(list).into_result().unwrap();

    assert_eq!(
        error_code(client.call(merge(false))),
        ErrorCode::InvalidRequest
    );
    // `made.txt` is not committed yet.
    assert_eq!(
        error_code(client.call(merge(true))),
        ErrorCode::InvalidState
    );
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), main_head);

    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "made"]);
    let tip = git(&root, &["rev-parse", "HEAD"]);
    let merged: TaskMergeResult = client.call(merge(true)).into_result().unwrap();
    assert_eq!(
        merged,
        TaskMergeResult {
            merged: true,
            conflicts: Vec::new()
        }
    );
    assert_eq!(
        git(&repo, &["rev-parse", "HEAD^1", "HEAD^2"]),
        format!("{main_head}\n{tip}")
    );
    assert_eq!(git(&repo, &["symbolic-ref", "HEAD"]), "refs/heads/main");
    assert_eq!(
        std::fs::read_to_string(app.join("made.txt")).unwrap(),
        "run"
    );

    assert_eq!(
        error_code(client.call(remove(false))),
        ErrorCode::InvalidRequest
    );
    // The agent still runs in the worktree.
    assert_eq!(
        error_code(client.call(remove(true))),
        ErrorCode::InvalidState
    );
    let kill = Request::SessionKill(SessionKillParams {
        session_id: run.session_id,
        force: true,
    });
    client.call(kill).into_result::<()>().unwrap();
    client.wait_state(run.session_id, SessionState::Exited);

    let removed: RemoveWorktreeResult = client.call(remove(true)).into_result().unwrap();
    assert!(!removed.branch_kept);
    assert!(!root.exists());
    assert!(git(&repo, &["branch", "--list", &branch]).is_empty());
    let task = wait_task(&mut client, task.id, |t| t.worktree_path.is_none());
    assert!(task.worktree && task.branch.is_none());
    let delete = Request::TaskDelete(TaskRef { task_id: task.id });
    client.call(delete).into_result::<()>().unwrap();
    client.stop();
}

/// T13.7 fan-out (SPEC §20E.2): `task.fanout` creates one child task per profile (own
/// worktree, title ` · <agent>`), runs them all and moves the original to Running; only in a
/// git repo, with 2–5 distinct profiles, once per task. A restart leaves the original alone.
#[test]
fn fanout_runs_each_agent_in_its_own_worktree() {
    let env = Env::new();
    for (id, name) in [("fa", "Agent A"), ("fb", "Agent B")] {
        let scenario = env.dir.path().join(format!("{id}.toml"));
        std::fs::write(
            &scenario,
            format!(
                "[[step]]\naction = \"write_file\"\npath = \"made.txt\"\ncontent = \"{id}\"\n\
                 [[step]]\naction = \"ask\"\n"
            ),
        )
        .unwrap();
        env.write_fake_profile_as(id, &format!("['--scenario', '{}']", scenario.display()));
        let path = env.dir.path().join(format!("config/agents/{id}.toml"));
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, text.replace("Fake agent", name)).unwrap();
    }
    let repo = env.repo();
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let task: Task = client
        .call(create_task(&project, "Fix the login", None))
        .into_result()
        .unwrap();
    let fanout = |task_id: Ulid, profiles: &[&str]| {
        Request::TaskFanout(TaskFanoutParams {
            task_id,
            agent_profiles: profiles.iter().map(|p| (*p).to_owned()).collect(),
            cols: 80,
            rows: 24,
        })
    };
    let not_git = client.call(fanout(task.id, &["fa", "fb"]));
    assert_eq!(error_code(not_git), ErrorCode::InvalidState);

    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let before = git(&repo, &["status", "--porcelain", "--ignored"]);
    let one = client.call(fanout(task.id, &["fa"]));
    assert_eq!(error_code(one), ErrorCode::InvalidRequest);
    let twice = client.call(fanout(task.id, &["fa", "fa"]));
    assert_eq!(error_code(twice), ErrorCode::InvalidRequest);
    let unknown = client.call(fanout(task.id, &["fa", "nope"]));
    assert_eq!(error_code(unknown), ErrorCode::NotFound);
    assert_eq!(list_tasks(&mut client, &project).len(), 1);

    let children: Vec<Task> = client
        .call(fanout(task.id, &["fa", "fb"]))
        .into_result()
        .unwrap();
    let titles: Vec<_> = children.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(
        titles,
        ["Fix the login · Agent A", "Fix the login · Agent B"]
    );
    for (child, id) in children.iter().zip(["fa", "fb"]) {
        assert_eq!(child.fanout_of, Some(task.id));
        assert_eq!(child.agent_profile.as_deref(), Some(id));
        assert!(child.worktree && child.branch.is_some());
        let root = PathBuf::from(child.worktree_path.clone().unwrap());
        wait_file(&root.join("made.txt"));
        assert_eq!(std::fs::read_to_string(root.join("made.txt")).unwrap(), id);
    }
    assert_ne!(children[0].worktree_path, children[1].worktree_path);
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let tasks = list_tasks(&mut client, &project);
        let status = |id: Ulid| tasks.iter().find(|t| t.id == id).unwrap().status;
        if status(task.id) == TaskStatus::Running
            && children
                .iter()
                .all(|c| status(c.id) == TaskStatus::NeedsInput)
        {
            break;
        }
        assert!(Instant::now() < deadline, "{tasks:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!repo.join("made.txt").exists());
    assert_eq!(git(&repo, &["status", "--porcelain", "--ignored"]), before);
    let again = client.call(fanout(task.id, &["fa", "fb"]));
    assert_eq!(error_code(again), ErrorCode::InvalidState);
    let nested = client.call(fanout(children[0].id, &["fa", "fb"]));
    assert_eq!(error_code(nested), ErrorCode::InvalidState);
    let six = ["fa", "fb", "c", "d", "e", "f"];
    assert_eq!(
        error_code(client.call(fanout(task.id, &six))),
        ErrorCode::InvalidRequest
    );
    // The original itself no longer runs: its children do.
    assert_eq!(
        error_code(client.call(run_task(task.id))),
        ErrorCode::InvalidState
    );

    client.stop();
    assert!(daemon.wait_exit().success());
    // The children (no resume command) are flagged; the original runs nothing to resume.
    let (_daemon, mut client) = env.start();
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let tasks = list_tasks(&mut client, &project);
        let found = |id: Ulid| tasks.iter().find(|t| t.id == id).unwrap().clone();
        if children.iter().all(|c| found(c.id).error_flag) {
            let original = found(task.id);
            assert_eq!(original.status, TaskStatus::Running);
            assert!(!original.error_flag);
            break;
        }
        assert!(Instant::now() < deadline, "{tasks:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
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
            worktree: false,
        }))
        .into_result()
        .unwrap();

    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    client.wait_state(run.session_id, SessionState::NeedsInput);
    client
        .call(send(run.session_id, "y"))
        .into_result::<SessionSendResult>()
        .unwrap();
    wait_task(&mut client, task.id, |t| t.status == TaskStatus::Review);
    let log = session_log(&env, run.session_id);
    assert!(log.contains(&format!("Task: {prompt}")), "{log:?}");
    client.stop();
}

#[test]
fn fs_list_read_and_find_follow_file_changes() {
    let env = Env::new();
    let repo = env.repo();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["config", "user.name", "t"]);
    git(&repo, &["config", "user.email", "t@t"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    std::fs::create_dir(repo.join("src")).unwrap();
    std::fs::write(repo.join(".gitignore"), "out/\n").unwrap();
    std::fs::write(repo.join("src/main.rs"), "fn main() {}\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    std::fs::write(repo.join("src/main.rs"), "fn main() { run() }\n").unwrap();
    std::fs::create_dir(repo.join("out")).unwrap();
    std::fs::write(repo.join("out/app.bin"), b"\0\x01").unwrap();
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;
    let list = |client: &mut Client, dir: &str, show_ignored| -> Vec<FsEntry> {
        let request = Request::FsList(FsListParams {
            project_id: id,
            worktree_task_id: None,
            dir: dir.into(),
            show_ignored,
        });
        client.call(request).into_result().unwrap()
    };
    let find = |client: &mut Client, query: &str| -> Vec<String> {
        let request = Request::FsFind(FsFindParams {
            project_id: id,
            worktree_task_id: None,
            query: query.into(),
            limit: 10,
        });
        let found: Vec<FsMatch> = client.call(request).into_result().unwrap();
        found.into_iter().map(|m| m.path).collect()
    };
    let read = |path: &str| {
        Request::FsRead(FsReadParams {
            project_id: id,
            worktree_task_id: None,
            path: path.into(),
        })
    };

    let top = list(&mut client, "", false);
    let names: Vec<&str> = top.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["src", ".gitignore"]);
    assert!(top[0].has_changes);
    let shown = list(&mut client, "", true);
    assert!(shown.iter().any(|e| e.name == "out" && e.ignored));
    let src = list(&mut client, "src", false);
    assert_eq!(src[0].path, "src/main.rs");
    assert_eq!(src[0].marker, Some(GitMarker::Modified));

    let content: FsContent = client.call(read("src\\main.rs")).into_result().unwrap();
    assert_eq!(content.content.as_deref(), Some("fn main() { run() }\n"));
    assert_eq!(content.language.as_deref(), Some("rust"));
    let binary: FsContent = client.call(read("out/app.bin")).into_result().unwrap();
    assert!(binary.binary && binary.content.is_none());
    assert_eq!(
        error_code(client.call(read("../x"))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(read(".git/HEAD"))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(read("nope.rs"))),
        ErrorCode::NotFound
    );
    let missing = Request::FsList(FsListParams {
        project_id: id,
        worktree_task_id: None,
        dir: "nope".into(),
        show_ignored: false,
    });
    assert_eq!(error_code(client.call(missing)), ErrorCode::NotFound);

    // The cached file list follows the watcher.
    assert_eq!(find(&mut client, "main"), ["src/main.rs"]);
    assert!(
        find(&mut client, "app").is_empty(),
        "ignored files are not found"
    );
    // Let any batch from earlier writes pass, so the event waited for is this file's.
    std::thread::sleep(Duration::from_millis(500));
    client.call(Request::DaemonStatus);
    client.events.clear();
    std::fs::write(repo.join("src/maintenance.rs"), "").unwrap();
    client.wait_event(&Event::GitChanged(GitChangedEvent {
        project_id: id,
        worktree_task_id: None,
    }));
    assert_eq!(
        find(&mut client, "main"),
        ["src/main.rs", "src/maintenance.rs"]
    );
}

fn session_log(env: &Env, id: Ulid) -> String {
    let path = env.data_dir().join("sessions").join(format!("{id}.log"));
    String::from_utf8_lossy(&std::fs::read(path).unwrap()).into_owned()
}

fn generate(project_id: Ulid, profile: Option<&str>, kind: AiKind, input: &str) -> (Ulid, Request) {
    let generation_id = Ulid::new();
    let request = Request::AiGenerate(AiGenerateParams {
        generation_id,
        project_id,
        worktree_task_id: None,
        agent_profile: profile.map(str::to_owned),
        kind,
        input: input.into(),
    });
    (generation_id, request)
}

fn generated(client: &mut Client, id: Ulid) -> AiGeneratedEvent {
    let event =
        client.wait_event_where(|e| matches!(e, Event::AiGenerated(g) if g.generation_id == id));
    let Event::AiGenerated(done) = event else {
        unreachable!("matched above")
    };
    done
}

fn init_repo(repo: &Path) {
    git(repo, &["init", "-q", "-b", "main"]);
    git(repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-q", "-m", "one"]);
}

/// SPEC §6 `oneshot`: the agent's stdout is the answer, without blank edges or a code fence.
#[test]
fn ai_generate_answers_with_the_oneshot_output() {
    let env = Env::new();
    let repo = env.repo();
    init_repo(&repo);
    std::fs::write(repo.join("a.txt"), "a\nb\n").unwrap();
    git(&repo, &["add", "."]);
    let scenario = env.dir.path().join("answer.toml");
    let steps = ["```", "feat: add b", "```"]
        .map(|text| format!("[[step]]\naction = \"print\"\ntext = \"{text}\"\n"))
        .join("\n");
    std::fs::write(&scenario, steps).unwrap();
    env.write_oneshot_profile("gen", &scenario);
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);

    let (id, request) = generate(project.id, Some("gen"), AiKind::CommitMessage, "");
    client.call(request).into_result::<()>().unwrap();
    let done = generated(&mut client, id);
    assert_eq!(
        (done.text.as_deref(), done.error),
        (Some("feat: add b"), None)
    );
    assert_eq!(done.project_id, project.id);

    let (id, request) = generate(project.id, Some("gen"), AiKind::EnhancePrompt, "add b");
    client.call(request).into_result::<()>().unwrap();
    assert_eq!(
        generated(&mut client, id).text.as_deref(),
        Some("feat: add b")
    );
}

/// What a user meets with old profiles, no default agent, a failing agent or no changes.
#[test]
fn ai_generate_reports_what_is_missing() {
    let env = Env::new();
    let repo = env.repo();
    init_repo(&repo);
    env.write_fake_profile(); // `fake`: no `oneshot`
    let failing = env.dir.path().join("fail.toml");
    std::fs::write(&failing, "[[step]]\naction = \"exit\"\ncode = 3\n").unwrap();
    env.write_oneshot_profile("broken", &failing);
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);

    // A profile seeded before `oneshot` existed: the error says what to add, and where.
    let (_, request) = generate(project.id, Some("fake"), AiKind::EnhancePrompt, "x");
    let outcome = client.call(request);
    let message = format!("{outcome:?}");
    assert_eq!(error_code(outcome), ErrorCode::InvalidState);
    assert!(
        message.contains("oneshot") && message.contains("fake.toml"),
        "{message}"
    );

    // No profile given and no default agent.
    let (_, request) = generate(project.id, None, AiKind::EnhancePrompt, "x");
    assert_eq!(error_code(client.call(request)), ErrorCode::InvalidState);

    // Nothing to improve.
    let (_, request) = generate(project.id, Some("broken"), AiKind::EnhancePrompt, "  ");
    assert_eq!(error_code(client.call(request)), ErrorCode::InvalidRequest);

    // The agent fails: the event carries the reason.
    let (id, request) = generate(project.id, Some("broken"), AiKind::EnhancePrompt, "x");
    client.call(request).into_result::<()>().unwrap();
    let done = generated(&mut client, id);
    assert_eq!(done.text, None);
    assert!(
        done.error.as_deref().is_some_and(|e| e.contains('3')),
        "{done:?}"
    );

    // A clean repo has nothing to describe; the agent is not even started.
    let (id, request) = generate(project.id, Some("broken"), AiKind::CommitMessage, "");
    client.call(request).into_result::<()>().unwrap();
    assert_eq!(
        generated(&mut client, id).error.as_deref(),
        Some("No changes to describe.")
    );
}
