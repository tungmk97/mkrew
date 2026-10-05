//! The "Undo task" confirmation of a Review / Done card (SPEC §12B, ADR 0005): the files and
//! the diff `task.undo_preview` says would be discarded, and the `task.undo` confirming it.
//! Pure and tested headlessly; `main.rs` renders it.

use gpui::Keystroke;
use mkrew_protocol::{Session, SessionState, Task, TaskUndoParams, UndoPreview};
use ulid::Ulid;

/// Most diff lines the dialog draws; the rest is summarized (`mk task undo` prints it all).
pub const DIFF_LINE_LIMIT: usize = 2000;

/// What `task.undo` does to a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChange {
    /// Tracked and changed: restored to the checkpoint.
    Restored,
    /// Tracked since the checkpoint: deleted.
    Added,
    /// Untracked, created since the checkpoint: deleted.
    Created,
}

impl FileChange {
    /// Short marker (git status letters) and what happens to the file.
    pub fn label(self) -> (&'static str, &'static str) {
        match self {
            Self::Restored => ("M", "restored"),
            Self::Added => ("A", "deleted"),
            Self::Created => ("?", "deleted"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRow {
    pub path: String,
    pub change: FileChange,
}

/// Every file the undo touches: restored ones, then tracked and untracked deletions.
pub fn files(preview: &UndoPreview) -> Vec<FileRow> {
    let rows = |paths: &[String], change| {
        paths
            .iter()
            .map(move |path| FileRow {
                path: path.clone(),
                change,
            })
            .collect::<Vec<_>>()
    };
    let mut files = rows(&preview.changed, FileChange::Restored);
    files.extend(rows(&preview.added, FileChange::Added));
    files.extend(rows(&preview.created, FileChange::Created));
    files
}

/// How a diff line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// `diff --git`, `index`, `---`/`+++` … before a file's first hunk.
    Header,
    /// `@@ … @@`.
    Hunk,
    Added,
    Removed,
    Context,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    /// Tabs expanded, so columns line up in the UI font renderer.
    pub text: String,
}

/// The first `limit` lines of a unified diff, classified, and how many more there are.
pub fn diff_lines(diff: &str, limit: usize) -> (Vec<DiffLine>, usize) {
    let mut in_header = false;
    let mut lines = Vec::new();
    let mut hidden = 0;
    for line in diff.lines() {
        let kind = if line.starts_with("diff --git ") {
            in_header = true;
            LineKind::Header
        } else if line.starts_with("@@") {
            in_header = false;
            LineKind::Hunk
        } else if in_header {
            LineKind::Header
        } else if line.starts_with('+') {
            LineKind::Added
        } else if line.starts_with('-') {
            LineKind::Removed
        } else {
            LineKind::Context
        };
        if lines.len() == limit {
            hidden += 1;
            continue;
        }
        lines.push(DiffLine {
            kind,
            text: line.replace('\t', "    "),
        });
    }
    (lines, hidden)
}

/// A loaded preview, with its rows parsed once (renders only read them).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub preview: UndoPreview,
    pub files: Vec<FileRow>,
    pub lines: Vec<DiffLine>,
    /// Diff lines past [`DIFF_LINE_LIMIT`].
    pub hidden: usize,
}

impl Shown {
    /// Whether the diff shown is not all of it (cut by the daemon or by the dialog).
    pub fn cut(&self) -> bool {
        self.hidden > 0 || self.preview.diff_truncated
    }
}

/// The preview being shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Preview {
    Loading,
    Loaded(Box<Shown>),
    Failed(String),
}

impl Preview {
    pub fn loaded(preview: UndoPreview) -> Self {
        let files = files(&preview);
        let (lines, hidden) = diff_lines(&preview.diff, DIFF_LINE_LIMIT);
        Self::Loaded(Box::new(Shown {
            preview,
            files,
            lines,
            hidden,
        }))
    }
}

/// The open dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoDialog {
    pub task_id: Ulid,
    pub project_id: Ulid,
    pub title: String,
    pub preview: Preview,
    /// The `task.undo` is in flight.
    pub busy: bool,
    /// Why the last undo failed.
    pub error: Option<String>,
}

impl UndoDialog {
    pub fn new(task: &Task) -> Self {
        Self {
            task_id: task.id,
            project_id: task.project_id,
            title: task.title.clone(),
            preview: Preview::Loading,
            busy: false,
            error: None,
        }
    }

    /// Whether the loaded preview has nothing to discard (the dialog then only offers Close;
    /// the daemon would accept the undo as a no-op).
    pub fn nothing_changed(&self) -> bool {
        matches!(&self.preview, Preview::Loaded(shown) if shown.files.is_empty())
    }

    /// The `task.undo` discarding exactly what the preview shows, or why it cannot run now.
    /// `sessions` = every known session: the daemon refuses while one of the task's sessions
    /// is live.
    pub fn confirm_params(&self, sessions: &[Session]) -> Result<TaskUndoParams, &'static str> {
        let preview = match &self.preview {
            Preview::Loading => return Err("Loading the changes…"),
            Preview::Failed(_) => return Err("The changes could not be listed."),
            Preview::Loaded(shown) => &shown.preview,
        };
        if self.busy {
            return Err("Discarding…");
        }
        if self.nothing_changed() {
            return Err("Nothing changed since the task ran.");
        }
        if sessions
            .iter()
            .any(|s| s.task_id == Some(self.task_id) && s.state != SessionState::Exited)
        {
            return Err("Close the task's terminal first.");
        }
        Ok(TaskUndoParams {
            task_id: self.task_id,
            confirm: true,
            created: preview.created.clone(),
            fingerprint: preview.fingerprint.clone(),
        })
    }

    /// `task.undo` failed: show why, and drop the preview so it is read again (the daemon
    /// refuses one that went stale; a confirm must match what is shown).
    pub fn undo_failed(&mut self, error: &str) {
        self.busy = false;
        self.error = Some(format!("Cannot undo the task: {error}"));
        self.preview = Preview::Loading;
    }
}

/// Esc closes the dialog; nothing else is bound (discarding takes an explicit click).
pub fn cancels(keystroke: &Keystroke) -> bool {
    keystroke.key == "escape"
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{SessionKind, StateSource, TaskStatus};

    use super::*;

    fn task() -> Task {
        Task {
            id: Ulid::new(),
            project_id: Ulid::new(),
            title: "Fix login".into(),
            prompt: String::new(),
            agent_profile: None,
            status: TaskStatus::Review,
            position: 1.0,
            checkpoint: Some("abc".into()),
            error_flag: false,
            created_at: 1,
            updated_at: 1,
        }
    }

    fn preview() -> UndoPreview {
        UndoPreview {
            checkpoint: "abc".into(),
            fingerprint: "f00d".into(),
            changed: vec!["src/a.rs".into()],
            added: vec!["src/b.rs".into()],
            created: vec!["notes.txt".into()],
            diff: String::new(),
            diff_truncated: false,
        }
    }

    fn session(task_id: Ulid, state: SessionState) -> Session {
        Session {
            id: Ulid::new(),
            task_id: Some(task_id),
            project_id: Ulid::new(),
            agent_profile: "claude".into(),
            kind: SessionKind::Agent,
            title: None,
            native_session_id: None,
            state,
            state_source: StateSource::Daemon,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
        }
    }

    #[test]
    fn files_list_restored_then_deleted_files() {
        let rows: Vec<_> = files(&preview())
            .into_iter()
            .map(|row| (row.path, row.change.label()))
            .collect();
        assert_eq!(
            rows,
            [
                ("src/a.rs".to_owned(), ("M", "restored")),
                ("src/b.rs".to_owned(), ("A", "deleted")),
                ("notes.txt".to_owned(), ("?", "deleted")),
            ]
        );
    }

    #[test]
    fn diff_lines_are_classified_and_capped() {
        let diff = "diff --git a/x b/x\nindex 1..2 100644\n--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n \tkeep\n--- old\n+new\n";
        let (lines, hidden) = diff_lines(diff, 100);
        let kinds: Vec<_> = lines.iter().map(|line| line.kind).collect();
        assert_eq!(
            kinds,
            [
                LineKind::Header,
                LineKind::Header,
                LineKind::Header,
                LineKind::Header,
                LineKind::Hunk,
                LineKind::Context,
                LineKind::Removed,
                LineKind::Added,
            ]
        );
        assert_eq!(lines[5].text, "     keep");
        assert_eq!(hidden, 0);
        let (lines, hidden) = diff_lines(diff, 3);
        assert_eq!((lines.len(), hidden), (3, 5));
    }

    #[test]
    fn confirm_sends_the_previewed_created_files_and_fingerprint() {
        let task = task();
        let mut dialog = UndoDialog::new(&task);
        assert_eq!(dialog.confirm_params(&[]), Err("Loading the changes…"));
        dialog.preview = Preview::loaded(preview());
        assert_eq!(
            dialog.confirm_params(&[session(task.id, SessionState::Exited)]),
            Ok(TaskUndoParams {
                task_id: task.id,
                confirm: true,
                created: vec!["notes.txt".into()],
                fingerprint: "f00d".into(),
            })
        );
        dialog.busy = true;
        assert_eq!(dialog.confirm_params(&[]), Err("Discarding…"));
    }

    #[test]
    fn a_failed_undo_shows_why_and_reads_the_preview_again() {
        let mut dialog = UndoDialog::new(&task());
        dialog.preview = Preview::loaded(preview());
        dialog.busy = true;
        dialog.undo_failed("preview again");
        assert!(!dialog.busy);
        assert_eq!(
            dialog.error.as_deref(),
            Some("Cannot undo the task: preview again")
        );
        assert_eq!(dialog.preview, Preview::Loading);
        assert!(dialog.confirm_params(&[]).is_err());
    }

    #[test]
    fn a_loaded_preview_is_parsed_once() {
        let Preview::Loaded(shown) = Preview::loaded(UndoPreview {
            diff: "@@ -1 +1 @@\n-a\n+b\n".into(),
            diff_truncated: true,
            ..preview()
        }) else {
            panic!("expected a loaded preview");
        };
        assert_eq!((shown.files.len(), shown.lines.len()), (3, 3));
        assert!(shown.cut(), "the daemon cut the diff");
    }

    #[test]
    fn confirm_is_refused_while_nothing_changed_or_a_session_is_live() {
        let task = task();
        let mut dialog = UndoDialog::new(&task);
        dialog.preview = Preview::Failed("boom".into());
        assert!(dialog.confirm_params(&[]).is_err());
        assert!(!dialog.nothing_changed());
        dialog.preview = Preview::loaded(UndoPreview {
            changed: Vec::new(),
            added: Vec::new(),
            created: Vec::new(),
            ..preview()
        });
        assert!(dialog.nothing_changed());
        assert_eq!(
            dialog.confirm_params(&[]),
            Err("Nothing changed since the task ran.")
        );
        dialog.preview = Preview::loaded(preview());
        let other = session(Ulid::new(), SessionState::Working);
        assert!(dialog.confirm_params(&[other]).is_ok());
        assert_eq!(
            dialog.confirm_params(&[session(task.id, SessionState::Done)]),
            Err("Close the task's terminal first.")
        );
    }

    #[test]
    fn only_escape_cancels() {
        assert!(cancels(&Keystroke::parse("escape").unwrap()));
        assert!(!cancels(&Keystroke::parse("enter").unwrap()));
    }
}
