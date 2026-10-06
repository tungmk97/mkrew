//! The Docs view-model against the real daemon (SPEC §12): `doc.list` lists the workspace's
//! Markdown files, the first one is read and rendered, an external edit (as from Zed) arrives
//! as `doc.changed` and re-reads it, and a new comment re-lists with its open count. The shown
//! file's threads follow `comment.updated`; comments and replies are saved from the margin's
//! drafts.
//!
//! Needs `target/<profile>/mkrew-daemon` built (`cargo test --workspace`).

// Helpers outside `#[test]` fns are not covered by `allow-unwrap-in-tests`.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures_lite::future::block_on;
use mkrew_core::Paths;
use mkrew_protocol::{
    CommentCreateParams, CommentListParams, CommentRef, CommentStatus, DocComment, DocListParams,
    DocReadParams, Project, ProjectAddParams, Request,
};
use mkrew_ui::client::{Connection, Inbox, Incoming};
use mkrew_ui::comments::{self, List};
use mkrew_ui::connect::{DaemonLauncher, POLL, TIMEOUT, connect_or_spawn};
use mkrew_ui::docs::{Doc, Docs, Files};
use mkrew_ui::markdown::BlockKind;
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

/// The UI side: `Docs` fed by requests and the connection's events, as `Shell` does.
struct Ui {
    connection: Connection,
    inbox: Inbox,
    docs: Docs,
}

impl Ui {
    fn list(&mut self) {
        assert!(self.docs.begin_list());
        let listed = block_on(self.connection.call(Request::DocList(DocListParams {
            project_id: self.docs.project_id,
        })))
        .map_err(|error| format!("{error:#}"));
        let (read, again) = self.docs.listed(listed);
        assert!(!again);
        if let Some(file_path) = read {
            self.read(file_path);
        }
    }

    fn read(&mut self, file_path: String) {
        let read = block_on(self.connection.call(Request::DocRead(DocReadParams {
            project_id: self.docs.project_id,
            file_path: file_path.clone(),
        })))
        .map_err(|error| format!("{error:#}"));
        if let Some(file_path) = self.docs.comments.begin_list(&file_path) {
            let listed = block_on(
                self.connection
                    .call(Request::CommentList(CommentListParams {
                        project_id: self.docs.project_id,
                        file_path: Some(file_path.clone()),
                        context: None,
                    })),
            )
            .map_err(|error| format!("{error:#}"));
            self.docs.comments.listed(&file_path, listed);
        }
        self.docs.read(&file_path, read);
    }

    /// Saves the open draft as the shell does.
    fn submit(&mut self) {
        let serial = self.docs.comments.draft.as_ref().unwrap().serial;
        let request = self.docs.comments.submit(self.docs.project_id).unwrap();
        let saved =
            block_on(self.connection.call::<DocComment>(request)).map_err(|e| format!("{e:#}"));
        self.docs.comments.submitted(serial, saved);
    }

    /// Opens a new-comment draft on the selection of the shown file.
    fn start_comment(&mut self) -> bool {
        let Doc::Shown(shown) = &self.docs.doc else {
            panic!("no doc shown");
        };
        self.docs.comments.start_comment(&shown.lines)
    }

    /// Applies events (re-reading and re-listing as asked) until `done` holds.
    fn wait(&mut self, what: &str, done: impl Fn(&Docs) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !done(&self.docs) {
            while let Ok(incoming) = self.inbox.try_recv() {
                if let Incoming::Event(event) = incoming {
                    let reaction = self.docs.apply(&event);
                    if let Some(file_path) = reaction.reread {
                        self.read(file_path);
                    }
                    if reaction.relist {
                        self.list();
                    }
                }
            }
            assert!(
                Instant::now() < deadline,
                "the Docs view never showed {what}"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn heading(docs: &Docs) -> Option<String> {
    let Doc::Shown(shown) = &docs.doc else {
        return None;
    };
    shown.blocks.iter().find_map(|block| match &block.kind {
        BlockKind::Heading { runs, .. } => Some(runs.iter().map(|r| r.text.as_str()).collect()),
        _ => None,
    })
}

/// The shown file's threads as (lines, status, bodies of root and replies).
fn threads(docs: &Docs) -> Vec<((u32, u32), CommentStatus, Vec<String>)> {
    let List::Loaded(list) = &docs.comments.list else {
        return Vec::new();
    };
    comments::threads(list)
        .iter()
        .map(|thread| {
            let root = thread.root;
            let lines = (root.start_line.unwrap(), root.end_line.unwrap());
            let bodies = std::iter::once(root)
                .chain(thread.replies.iter().copied())
                .map(|c| c.body.clone())
                .collect();
            (lines, root.status, bodies)
        })
        .collect()
}

fn open_comments(docs: &Docs, file_path: &str) -> Option<u32> {
    let Files::Loaded(files) = &docs.files else {
        return None;
    };
    let file = files.iter().find(|file| file.file_path == file_path)?;
    Some(file.open_comments)
}

#[test]
fn docs_list_render_and_follow_external_edits_and_comments() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths(dir.path());
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    std::fs::write(repo.join("PLAN.md"), "# Plan\n\n- [ ] Ship\n").unwrap();
    std::fs::write(repo.join("docs/notes.md"), "Notes\n").unwrap();
    std::fs::write(repo.join("main.rs"), "fn main() {}\n").unwrap();
    let _stop = StopOnDrop(paths.clone());

    let connection = connect(&paths);
    let project: Project = block_on(connection.call(Request::ProjectAdd(ProjectAddParams {
        path: repo.to_string_lossy().into_owned(),
        name: Some("demo".into()),
        dev_command: None,
    })))
    .unwrap();
    let inbox = connection.incoming();
    let mut ui = Ui {
        connection,
        inbox,
        docs: Docs::new(project.id),
    };

    // Listed (Markdown only, sorted), the first file read and parsed.
    ui.list();
    let Files::Loaded(files) = &ui.docs.files else {
        panic!("not listed: {:?}", ui.docs.files);
    };
    let listed: Vec<_> = files.iter().map(|f| f.file_path.as_str()).collect();
    assert_eq!(listed, ["PLAN.md", "docs/notes.md"]);
    assert_eq!(ui.docs.selected.as_deref(), Some("PLAN.md"));
    assert_eq!(heading(&ui.docs).as_deref(), Some("Plan"));

    // An external edit refreshes the shown file.
    std::fs::write(repo.join("PLAN.md"), "# Plan v2\n\n- [x] Ship\n").unwrap();
    ui.wait("the edited heading", |docs| {
        heading(docs).as_deref() == Some("Plan v2")
    });

    // A new comment re-lists with its count.
    assert_eq!(open_comments(&ui.docs, "PLAN.md"), Some(0));
    let when: DocComment = block_on(ui.connection.call(Request::CommentCreate(
        CommentCreateParams {
            project_id: project.id,
            file_path: "PLAN.md".into(),
            start_line: 3,
            end_line: 3,
            quote: "Ship".into(),
            body: "When?".into(),
            context: Default::default(),
        },
    )))
    .unwrap();
    ui.wait("one open comment", |docs| {
        open_comments(docs, "PLAN.md") == Some(1)
    });
    // The margin got it from `comment.updated`.
    assert_eq!(
        threads(&ui.docs),
        [((3, 3), CommentStatus::Open, vec!["When?".to_owned()])]
    );

    // A comment on the picked heading and a reply, saved from drafts.
    ui.docs.comments.pick(1, 1, false);
    assert!(ui.start_comment());
    ui.docs
        .comments
        .draft
        .as_mut()
        .unwrap()
        .insert("Rename it?");
    ui.submit();
    assert_eq!(ui.docs.comments.draft, None);
    ui.docs.comments.start_reply(when.id);
    ui.docs.comments.draft.as_mut().unwrap().insert("Friday");
    ui.submit();
    let saved: DocComment = block_on(ui.connection.call(Request::CommentList(CommentListParams {
        project_id: project.id,
        file_path: Some("PLAN.md".into()),
        context: None,
    })))
    .map(|list: Vec<DocComment>| list.into_iter().find(|c| c.body == "Rename it?").unwrap())
    .unwrap();
    assert_eq!(saved.quote.as_deref(), Some("# Plan v2"));

    // Resolving arrives as an update of the thread.
    let _: DocComment = block_on(ui.connection.call(Request::CommentResolve(CommentRef {
        comment_id: when.id,
    })))
    .unwrap();
    ui.wait("the resolved thread", |docs| {
        threads(docs)
            == [
                ((1, 1), CommentStatus::Open, vec!["Rename it?".to_owned()]),
                (
                    (3, 3),
                    CommentStatus::Resolved,
                    vec!["When?".to_owned(), "Friday".to_owned()],
                ),
            ]
    });

    // Another file; deleting the shown one says so.
    assert_eq!(
        ui.docs.select("docs/notes.md").as_deref(),
        Some("docs/notes.md")
    );
    ui.read("docs/notes.md".into());
    assert!(matches!(ui.docs.doc, Doc::Shown(_)));
    std::fs::remove_file(repo.join("docs/notes.md")).unwrap();
    ui.wait("the deleted file", |docs| docs.doc == Doc::Deleted);
}
