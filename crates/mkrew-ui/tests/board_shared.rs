//! `mk task` and the board share data (SPEC §9, §10): tasks the real `mk` CLI adds and runs show
//! up on the UI's `Board` view-model through daemon events alone, follow their session through
//! Running → Needs input → Review, and a task the UI creates is listed by `mk task list`. Runs the
//! real `mkrew-daemon`, `mk` and fake agent in a temp dir.
//!
//! Needs `target/<profile>/mkrew-daemon`, `mk` and `mkrew-fakeagent` built (`cargo test --workspace`).

// Helpers outside `#[test]` fns are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use futures_lite::future::block_on;
use mkrew_core::Paths;
use mkrew_protocol::{
    Project, ProjectAddParams, Request, Session, SessionListParams, SessionState, Task,
    TaskCreateParams, TaskListParams, TaskStatus,
};
use mkrew_ui::board::{Board, Card};
use mkrew_ui::client::{Connection, Inbox, Incoming};
use mkrew_ui::connect::{DaemonLauncher, POLL, TIMEOUT, connect_or_spawn};
use ulid::Ulid;

fn sibling_exe(name: &str) -> PathBuf {
    let exe = Path::new(env!("CARGO_BIN_EXE_mkrew"))
        .with_file_name(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    assert!(
        exe.is_file(),
        "{} is missing; run `cargo build --workspace`",
        exe.display()
    );
    exe
}

fn paths(dir: &Path) -> Paths {
    let socket = if cfg!(windows) {
        format!(r"\\.\pipe\mkrew-test-{}", Ulid::new())
    } else {
        dir.join("mkrew.sock").to_string_lossy().into_owned()
    };
    Paths {
        data_dir: dir.join("data"),
        config_dir: dir.join("config"),
        socket,
    }
}

/// `fake-arg.toml`: the built fake agent (default `ask` scenario) taking the prompt as an arg.
fn write_fake_arg_profile(paths: &Paths) {
    let fake = sibling_exe("mkrew-fakeagent");
    let sample = include_str!("../../../agents/fake.toml");
    let profile = sample
        .replace(
            "command = \"mkrew-fakeagent\"",
            &format!("command = '{}'", fake.display()),
        )
        .replace("prompt_mode = \"none\"", "prompt_mode = \"arg\"");
    assert_ne!(profile, sample);
    let dir = paths.config_dir.join("agents");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("fake-arg.toml"), profile).unwrap();
}

fn connect(paths: &Paths) -> Connection {
    let mut launcher =
        DaemonLauncher::new(paths.clone(), sibling_exe("mkrew-daemon")).without_notifications();
    connect_or_spawn(&mut launcher, TIMEOUT, POLL).unwrap()
}

/// Stops the spawned daemon even when an assertion fails.
struct StopOnDrop(Paths);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        if let Ok(Ok(connection)) = Connection::open(&self.0.socket) {
            let _ = block_on(connection.request(Request::DaemonStop));
        }
    }
}

/// Runs the real `mk` against the test daemon, asserts success and returns trimmed stdout.
fn mk(paths: &Paths, args: &[&str]) -> String {
    let output = Command::new(sibling_exe("mk"))
        .args(args)
        .env("MKREW_DATA_DIR", &paths.data_dir)
        .env("MKREW_CONFIG_DIR", &paths.config_dir)
        .env("MKREW_SOCKET", &paths.socket)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "mk {args:?} failed: {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// The UI side: the board fed only by the connection's events, as `Shell` does.
struct Ui {
    connection: Connection,
    inbox: Inbox,
    board: Board,
}

impl Ui {
    fn new(connection: Connection) -> Self {
        let inbox = connection.incoming();
        let mut board = Board::default();
        board.begin_load();
        let tasks: Vec<Task> = block_on(connection.call(Request::TaskList(TaskListParams {
            project_id: None,
            status: None,
        })))
        .unwrap();
        board.load(tasks);
        Self {
            connection,
            inbox,
            board,
        }
    }

    fn sessions(&self) -> Vec<Session> {
        block_on(
            self.connection
                .call(Request::SessionList(SessionListParams::default())),
        )
        .unwrap()
    }

    /// Applies events until `task_id`'s card satisfies `done`; returns it. Checked after every
    /// applied event, so a short-lived column (Running) is not skipped when events queue up.
    fn wait_card(
        &mut self,
        project_id: Ulid,
        task_id: Ulid,
        what: &str,
        done: impl Fn(TaskStatus, &Card) -> bool,
    ) -> Card {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let sessions = self.sessions();
            let matches = |board: &Board| {
                board
                    .columns(project_id, None, &sessions)
                    .into_iter()
                    .find_map(|column| {
                        let card = column.cards.into_iter().find(|c| c.task_id == task_id)?;
                        done(column.status, &card).then_some(card)
                    })
            };
            if let Some(card) = matches(&self.board) {
                return card;
            }
            while let Ok(incoming) = self.inbox.try_recv() {
                if let Incoming::Event(event) = incoming
                    && self.board.apply(&event)
                    && let Some(card) = matches(&self.board)
                {
                    return card;
                }
            }
            assert!(Instant::now() < deadline, "the board never showed {what}");
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[test]
fn mk_task_changes_reach_the_board_and_ui_tasks_reach_mk() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path());
    write_fake_arg_profile(&paths);
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    let _stop = StopOnDrop(paths.clone());

    let connection = connect(&paths);
    let project: Project = block_on(connection.call(Request::ProjectAdd(ProjectAddParams {
        path: repo.to_string_lossy().into_owned(),
        name: Some("demo".into()),
    })))
    .unwrap();
    let mut ui = Ui::new(connection);
    assert!(!ui.board.has_tasks(project.id));

    // `mk task add` → a Todo card, with its agent, from the `task.updated` event alone.
    let id = mk(
        &paths,
        &[
            "task",
            "add",
            "demo",
            "Fix the login bug",
            "--prompt",
            "fix login",
            "--agent",
            "fake-arg",
        ],
    );
    let task_id: Ulid = id.parse().unwrap();
    let card = ui.wait_card(
        project.id,
        task_id,
        "the added card in Todo",
        |status, _| status == TaskStatus::Todo,
    );
    assert_eq!(card.title, "Fix the login bug");
    assert_eq!(card.agent, "fake-arg");
    assert!(card.session.is_none());

    // `mk task run` → Running with its session, then Needs input at the fake's question.
    let session_id: Ulid = mk(&paths, &["task", "run", &id]).parse().unwrap();
    ui.wait_card(project.id, task_id, "the card running", |status, card| {
        status == TaskStatus::Running && card.opens_session() == Some(session_id)
    });
    let card = ui.wait_card(
        project.id,
        task_id,
        "the card needing input",
        |status, _| status == TaskStatus::NeedsInput,
    );
    assert_eq!(card.follow_up_session(), Some(session_id));

    // The answer from `mk send`; the agent exits 0 → Review, no error row.
    mk(&paths, &["send", &session_id.to_string(), "y"]);
    let card = ui.wait_card(project.id, task_id, "the card in Review", |status, card| {
        status == TaskStatus::Review
            && card
                .session
                .as_ref()
                .is_some_and(|s| s.state == SessionState::Exited)
    });
    assert!(!card.shows_error_row());

    // The other way: a task the UI's dialog creates is listed by `mk task list`.
    let created: Task = block_on(ui.connection.call(Request::TaskCreate(TaskCreateParams {
        project_id: project.id,
        title: "Write docs".into(),
        prompt: String::new(),
        agent_profile: None,
        position: None,
        worktree: false,
    })))
    .unwrap();
    let list = mk(&paths, &["task", "list", "--project", "demo"]);
    let row = |id: Ulid| {
        list.lines()
            .find(|line| line.starts_with(&id.to_string()))
            .unwrap_or_else(|| panic!("{id} not in:\n{list}"))
            .to_owned()
    };
    assert!(row(created.id).contains("todo"), "{list}");
    assert!(row(task_id).contains("review"), "{list}");
}
