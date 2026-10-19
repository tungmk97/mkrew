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
    CommitDetail, DaemonStatusResult, DocChangedEvent, DocComment, DocContent, DocFile,
    DocListParams, DocReadParams, ErrorCode, Event, FileDiff, Frame, FrameDecoder, FsContent,
    FsEntry, FsFindParams, FsListParams, FsMatch, FsReadParams, FsWriteParams, FsWritten,
    GitChangedEvent, GitCommitParams, GitCommitResult, GitDiffParams, GitDiscardParams,
    GitLogParams, GitMarker, GitMode, GitRemoteParams, GitSetBaseParams, GitSetViewedParams,
    GitShowParams, GitStageParams, GitStatus, GitStatusParams, HelloParams, HelloResult,
    HookReportParams, HunkTarget, Message, Outcome, PROTOCOL_VERSION, PlanItem, Project,
    ProjectAddParams, ProjectRef, ProjectUpdateParams, QueuedSend, RemoveWorktreeResult, Request,
    ResponseError, RunCommand, RunStartParams, RunStarted, RunStopParams, ScreenSnapshot, Session,
    SessionAttachResult, SessionExitedEvent, SessionFiles, SessionFilesParams, SessionHistory,
    SessionInputParams, SessionKillParams, SessionKind, SessionLinkPlanParams, SessionListParams,
    SessionMoveParams, SessionPinParams, SessionPlanEvent, SessionPortsEvent, SessionQueueEvent,
    SessionReadParams, SessionReadResult, SessionRef, SessionRenameParams, SessionResizeParams,
    SessionResumeParams, SessionSendParams, SessionSendResult, SessionSetColorParams,
    SessionSpawnParams, SessionSpawnResult, SessionState, SessionUnqueueParams, SessionUsageEvent,
    SessionWaitParams, SessionWaitResult, StateSource, TabCreateParams, TabListParams,
    TabMoveParams, TabPinParams, TabRef, TabRenameParams, TabSetLayoutParams, Task,
    TaskConfirmParams, TaskCreateParams, TaskDeletedEvent, TaskFanoutParams, TaskListParams,
    TaskMergeResult, TaskMoveParams, TaskRef, TaskRunParams, TaskStatus, TaskUndoParams,
    TaskUpdateParams, TerminalTab, Turn, TurnRef, TurnRestoreParams, UndoPreview, ViewedFile,
    WORKSPACE_COLORS, encode,
};
use mkrew_store::Store;
use serde_json::{Value, json};
use ulid::Ulid;

mod basics;
mod comments;
mod docs;
mod fs_ai;
mod git;
mod io;
mod runs;
mod states;
mod tabs_tasks;
mod task_runs;
mod turns;
mod web;
mod workspaces;
mod worktrees;

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

    /// `settings.toml` with `[features] turns = true`: sessions take turn snapshots (ADR 0046).
    fn enable_turns(&self) {
        let dir = self.dir.path().join("config");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.toml"), "[features]\nturns = true\n").unwrap();
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
        let sample = include_str!("../../../../agents/fake.toml");
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
        self.spawn_with(&[])
    }

    /// Like [`Env::spawn`], with extra environment variables for the daemon.
    fn spawn_with(&self, vars: &[(&str, &Path)]) -> Daemon {
        let child = Command::new(env!("CARGO_BIN_EXE_mkrew-daemon"))
            .env("MKREW_DATA_DIR", self.data_dir())
            .env("MKREW_CONFIG_DIR", self.dir.path().join("config"))
            .env("MKREW_SOCKET", &self.socket)
            .env("MKREW_LOG", "debug")
            .env(mkrew_daemon::ENV_NOTIFY, self.notifications())
            // Transcripts for token usage (SPEC §20E.3): never the user's real ones.
            .env("CLAUDE_CONFIG_DIR", self.dir.path().join("claude"))
            .env("CODEX_HOME", self.dir.path().join("codex"))
            .envs(vars.iter().copied())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        Daemon(child)
    }

    /// Spawns a daemon and waits until it answers `hello`.
    fn start(&self) -> (Daemon, Client) {
        self.start_with(&[])
    }

    /// Like [`Env::start`], with extra environment variables for the daemon.
    fn start_with(&self, vars: &[(&str, &Path)]) -> (Daemon, Client) {
        let mut daemon = self.spawn_with(vars);
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

fn save_attachment(session_id: Ulid, ext: &str, data: Vec<u8>) -> Request {
    Request::AttachmentSave(AttachmentSaveParams {
        session_id,
        ext: ext.into(),
        data,
    })
}

/// Screen mode: the fake agent works, asks, works again, then goes quiet.
const SCREEN_SCENARIO: &str = r#"
[[step]]
action = "print"
text = "Reading files"
[[step]]
action = "spinner"
text = "Planning"
ms = 2500
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
ms = 10000
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
ms = 20000
[[step]]
action = "hook"
event = "done"
[[step]]
action = "sleep"
ms = 300
"#;

/// The fake agent's `omp` scenario (crates/mkrew-fakeagent/scenarios/omp.toml) with quiet gaps
/// of 9 s against the profile's `idle_after_ms` of 3000.
const OMP_SCENARIO: &str = r#"
[[step]]
action = "spinner"
text = "Thinking"
ms = 1500
[[step]]
action = "print"
text = "Answered: the build uses cargo."
[[step]]
action = "sleep"
ms = 9000
[[step]]
action = "spinner"
text = "Editing src/lib.rs"
ms = 1500
[[step]]
action = "print"
text = "Edited src/lib.rs"
color = "green"
[[step]]
action = "sleep"
ms = 9000
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

fn screen_text(snapshot: &ScreenSnapshot) -> String {
    snapshot
        .lines
        .iter()
        .map(|runs| runs.iter().map(|r| r.text.as_str()).collect::<String>() + "\n")
        .collect()
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

fn unqueue(session_id: Ulid, send_id: Ulid) -> Request {
    Request::SessionUnqueue(SessionUnqueueParams {
        session_id,
        send_id,
    })
}

fn read(session_id: Ulid, lines: u16) -> Request {
    Request::SessionRead(SessionReadParams { session_id, lines })
}

/// The editor scenarios wait for an answer before their `done` hook: the watcher has recorded
/// `notes.txt` (it saw the file only after its quiet period) before the turn can end.
fn answer_after_watcher_saw_notes(env: &Env, client: &mut Client, id: Ulid) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !changed_files(env, id).contains("notes.txt") {
        assert!(Instant::now() < deadline, "notes.txt never recorded");
        std::thread::sleep(Duration::from_millis(50));
    }
    client.input(id, b"go\r").into_result::<()>().unwrap();
}

fn send_comments(comment_ids: &[CommentId], session_id: Ulid) -> Request {
    Request::CommentSendToAgent(CommentSendParams {
        comment_ids: comment_ids.to_vec(),
        session_id,
    })
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
        default_shell: None,
        run_commands: None,
        run_autostart: None,
        color: None,
        pinned: None,
    })
}

fn set_default_shell(project: &Project, shell: Option<&str>) -> Request {
    Request::ProjectUpdate(ProjectUpdateParams {
        project_id: project.id,
        name: None,
        default_agent: None,
        default_shell: Some(shell.map(str::to_owned)),
        run_commands: None,
        run_autostart: None,
        color: None,
        pinned: None,
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

/// Polls `tab.list` until the project's first tab has exactly `layout`. A pane leaves its tab a
/// moment after the session's exit is listed (the daemon edits the tab right after storing it).
fn wait_layout(client: &mut Client, project: &Project, layout: &serde_json::Value) {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let tabs = list_tabs(client, project);
        if tabs.first().is_some_and(|t| t.layout == *layout) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "layout never became {layout}: {tabs:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
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
        repo: None,
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

fn run_command(name: &str, dir: &str, command: &str) -> RunCommand {
    RunCommand {
        name: name.into(),
        dir: dir.into(),
        command: command.into(),
        env_file: None,
        env: Vec::new(),
    }
}

/// A command for the test shell: cmd on Windows, sh elsewhere.
fn shell_line(windows: &str, unix: &str) -> String {
    if cfg!(windows) { windows } else { unix }.to_owned()
}

/// `windows`/`unix`, then stays alive for a minute.
fn long(windows: &str, unix: &str) -> String {
    shell_line(
        &format!("{windows} & ping -n 60 127.0.0.1 > nul"),
        &format!("{unix}; sleep 60"),
    )
}

fn set_runs(project: &Project, commands: Vec<RunCommand>, autostart: Option<bool>) -> Request {
    Request::ProjectUpdate(ProjectUpdateParams {
        project_id: project.id,
        name: None,
        default_agent: None,
        default_shell: None,
        run_commands: Some(commands),
        run_autostart: autostart,
        color: None,
        pinned: None,
    })
}

fn run_params(project: &Project, name: Option<&str>) -> RunStartParams {
    RunStartParams {
        project_id: project.id,
        name: name.map(Into::into),
        cols: 80,
        rows: 24,
    }
}

fn run_start(project: &Project, name: Option<&str>) -> Request {
    Request::ProjectRunStart(run_params(project, name))
}

fn run_restart(project: &Project, name: Option<&str>) -> Request {
    Request::ProjectRunRestart(run_params(project, name))
}

fn run_stop(project: &Project, name: Option<&str>) -> Request {
    Request::ProjectRunStop(RunStopParams {
        project_id: project.id,
        name: name.map(Into::into),
    })
}

/// Every session of the project titled `run:…`, exited ones too.
fn run_sessions(client: &mut Client, project: &Project) -> Vec<Session> {
    let all: Vec<Session> = client
        .call(Request::SessionList(SessionListParams {
            project_id: Some(project.id),
        }))
        .into_result()
        .unwrap();
    all.into_iter()
        .filter(|s| s.title.as_deref().is_some_and(|t| t.starts_with("run:")))
        .collect()
}

/// Waits until `path` has non-blank text and returns it trimmed.
fn wait_text(path: &Path) -> String {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Ok(text) = std::fs::read_to_string(path)
            && !text.trim().is_empty()
        {
            return text.trim().to_owned();
        }
        assert!(
            Instant::now() < deadline,
            "{} never written",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn pair(a: Ulid, b: Ulid) -> serde_json::Value {
    json!({"split": "vertical", "ratio": 0.5, "children": [
        {"session": a.to_string()}, {"session": b.to_string()}
    ]})
}

fn rename(session_id: Ulid, title: Option<&str>) -> Request {
    Request::SessionRename(SessionRenameParams {
        session_id,
        title: title.map(Into::into),
    })
}

fn pin(session_id: Ulid, pinned: bool) -> Request {
    Request::SessionPin(SessionPinParams { session_id, pinned })
}

fn move_to(session_id: Ulid, position: f64) -> Request {
    Request::SessionMove(SessionMoveParams {
        session_id,
        position,
    })
}

fn link_plan(session_id: Ulid, path: Option<&str>) -> Request {
    Request::SessionLinkPlan(SessionLinkPlanParams {
        session_id,
        path: path.map(Into::into),
    })
}

fn set_color(session_id: Ulid, color: Option<&str>) -> Request {
    Request::SessionSetColor(SessionSetColorParams {
        session_id,
        color: color.map(Into::into),
    })
}
