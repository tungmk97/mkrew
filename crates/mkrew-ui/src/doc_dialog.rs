//! The Docs view's two dialogs (SPEC §12): "Send to agent" (which open comments go to which
//! live agent session; also the session picker of "Send selection", §20B.4) and "View
//! changes" (the file as it was when sent vs now). Pure and tested headlessly; `main.rs` sends
//! `comment.send_to_agent` and renders them.

use gpui::Keystroke;
use mkrew_protocol::{
    CommentId, CommentSendParams, CommentStatus, DocComment, Session, SessionKind, SessionState,
};
use similar::TextDiff;
use ulid::Ulid;

use crate::comments::{self, Selection};
use crate::docs;
use crate::history;
use crate::undo_dialog::{self, DIFF_LINE_LIMIT, DiffLine};

/// The open Docs dialog (modal; never open together with another dialog).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocDialog {
    Send(SendDialog),
    Changes(Changes),
}

/// The thread roots "Send to agent" sends, in anchor order: the open ones overlapping the
/// selected lines if any do, else every open one of the file.
pub fn sendable(comments: &[DocComment], selection: Option<Selection>) -> Vec<CommentId> {
    let open: Vec<&DocComment> = comments::threads(comments)
        .into_iter()
        .map(|thread| thread.root)
        .filter(|root| root.status == CommentStatus::Open)
        .collect();
    let picked: Vec<CommentId> = open
        .iter()
        .filter(|root| {
            selection
                .zip(root.start_line.zip(root.end_line))
                .is_some_and(|(selection, (start, end))| selection.contains(start, end))
        })
        .map(|root| root.id)
        .collect();
    if picked.is_empty() {
        open.iter().map(|root| root.id).collect()
    } else {
        picked
    }
}

/// A session the comments can go to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub session_id: Ulid,
    pub label: String,
    pub state: SessionState,
    /// Unix ms of its last state change.
    pub state_changed_at: i64,
}

/// What a key press in the send dialog does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Close,
    /// ↑ / ↓: the previous / next session.
    Move(isize),
    /// Enter.
    Send,
}

pub fn key(keystroke: &Keystroke) -> Option<Key> {
    let modified = keystroke.modifiers.modified();
    match keystroke.key.as_str() {
        "escape" => Some(Key::Close),
        "up" if !modified => Some(Key::Move(-1)),
        "down" if !modified => Some(Key::Move(1)),
        "enter" if !modified => Some(Key::Send),
        _ => None,
    }
}

/// The open "Send to agent" dialog. The sessions are read again on every use (they start,
/// exit and change state while it is open).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendDialog {
    pub project_id: Ulid,
    pub file_path: String,
    pub comment_ids: Vec<CommentId>,
    /// The session the file watcher last saw change the file (the default target), or for
    /// diff comments in a task's changes the task's session.
    pub last_session_id: Option<Ulid>,
    /// `last_session_id` is the task's session.
    pub task: bool,
    /// "Send selection" (§20B.4): the text the picked session's composer opens with
    /// (`comment_ids` is then empty).
    pub prefill: Option<String>,
    /// The session the user picked.
    pub picked: Option<Ulid>,
    /// The send is in flight.
    pub busy: bool,
    pub error: Option<String>,
}

impl SendDialog {
    pub fn new(
        project_id: Ulid,
        file_path: String,
        comment_ids: Vec<CommentId>,
        last_session_id: Option<Ulid>,
    ) -> SendDialog {
        SendDialog {
            project_id,
            file_path,
            comment_ids,
            last_session_id,
            task: false,
            prefill: None,
            picked: None,
            busy: false,
            error: None,
        }
    }

    /// "Send selection" of `prefill` from `file_path` (`path:a-b`; empty for a terminal), offering
    /// `task_session` (Git task changes) first, else the most recently active agent.
    pub fn selection(
        project_id: Ulid,
        file_path: String,
        prefill: String,
        task_session: Option<Ulid>,
        sessions: &[Session],
    ) -> SendDialog {
        // The task's session counts only while it is live (else it is not offered).
        let task_session = task_session.filter(|&id| {
            sessions
                .iter()
                .any(|s| s.id == id && s.state != SessionState::Exited)
        });
        let last = task_session.or_else(|| most_recently_active(sessions, project_id));
        SendDialog {
            task: task_session.is_some(),
            prefill: Some(prefill),
            ..SendDialog::new(project_id, file_path, Vec::new(), last)
        }
    }

    /// The workspace's live agent sessions, newest first.
    pub fn choices(&self, sessions: &[Session]) -> Vec<Choice> {
        let mut live: Vec<&Session> = sessions
            .iter()
            .filter(|s| {
                s.project_id == self.project_id
                    && s.kind == SessionKind::Agent
                    && s.state != SessionState::Exited
            })
            .collect();
        live.sort_by_key(|s| std::cmp::Reverse(s.started_at));
        live.into_iter()
            .map(|s| Choice {
                session_id: s.id,
                label: history::label(s),
                state: s.state,
                state_changed_at: s.state_changed_at,
            })
            .collect()
    }

    /// Which of `choices` is selected: the picked one, else the file's last modifying
    /// session, else the newest; `None` when there is none.
    pub fn selected(&self, choices: &[Choice]) -> Option<usize> {
        let index =
            |id: Option<Ulid>| id.and_then(|id| choices.iter().position(|c| c.session_id == id));
        index(self.picked)
            .or_else(|| index(self.last_session_id))
            .or_else(|| (!choices.is_empty()).then_some(0))
    }

    pub fn pick(&mut self, session_id: Ulid) {
        if !self.busy {
            self.picked = Some(session_id);
            self.error = None;
        }
    }

    /// ↑ / ↓ through the choices (stops at the ends).
    pub fn step(&mut self, sessions: &[Session], by: isize) {
        let choices = self.choices(sessions);
        let Some(current) = self.selected(&choices) else {
            return;
        };
        let next = current
            .saturating_add_signed(by)
            .min(choices.len().saturating_sub(1));
        self.pick(choices[next].session_id);
    }

    /// The selected session, if any.
    pub fn target(&self, sessions: &[Session]) -> Option<Ulid> {
        let choices = self.choices(sessions);
        Some(choices.get(self.selected(&choices)?)?.session_id)
    }

    /// The request sending the comments to the selected session, marking the dialog busy;
    /// `None` while one is in flight, or with no session or comment to send.
    pub fn send(&mut self, sessions: &[Session]) -> Option<CommentSendParams> {
        let session_id = self.target(sessions)?;
        if self.busy || self.comment_ids.is_empty() {
            return None;
        }
        self.busy = true;
        self.error = None;
        Some(CommentSendParams {
            comment_ids: self.comment_ids.clone(),
            session_id,
        })
    }

    /// Why the default session is offered first (shown next to it).
    pub fn last_label(&self) -> &'static str {
        if self.task {
            "ran this task"
        } else if self.prefill.is_some() {
            "active last"
        } else {
            "edited this file"
        }
    }

    /// The send failed: the dialog stays with the reason.
    pub fn failed(&mut self, error: String) {
        self.busy = false;
        self.error = Some(error);
    }
}

/// The workspace's live agent session whose state changed last (§20B.4's default target).
pub fn most_recently_active(sessions: &[Session], project_id: Ulid) -> Option<Ulid> {
    sessions
        .iter()
        .filter(|s| {
            s.project_id == project_id
                && s.kind == SessionKind::Agent
                && s.state != SessionState::Exited
        })
        .max_by_key(|s| (s.state_changed_at, s.started_at))
        .map(|s| s.id)
}

/// "View changes" of a sent thread: its file as sent vs as shown now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changes {
    pub file_path: String,
    /// The thread's anchor ("Lines 4–7").
    pub anchor: String,
    pub sent_to: Option<Ulid>,
    pub lines: Vec<DiffLine>,
    /// Diff lines past [`DIFF_LINE_LIMIT`].
    pub hidden: usize,
}

impl Changes {
    /// `None` when `root` was never sent.
    pub fn new(root: &DocComment, current: &[String]) -> Option<Changes> {
        let snapshot = root.sent_snapshot.as_deref()?;
        let (lines, hidden) = diff(snapshot, current);
        Some(Changes {
            file_path: root.file_path.clone(),
            anchor: root
                .start_line
                .zip(root.end_line)
                .map_or_else(String::new, |(start, end)| {
                    comments::lines_label(start, end)
                }),
            sent_to: root.sent_to,
            lines,
            hidden,
        })
    }
}

/// The unified diff (3 lines of context, `@@` hunk rows) from `snapshot` to the `current`
/// source lines; line endings do not count as changes. Empty = unchanged.
pub fn diff(snapshot: &str, current: &[String]) -> (Vec<DiffLine>, usize) {
    let text = |lines: &[String]| {
        lines
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>()
    };
    let old = text(&docs::source_lines(snapshot));
    let new = text(current);
    let unified = TextDiff::from_lines(&old, &new)
        .unified_diff()
        .context_radius(3)
        .to_string();
    undo_dialog::diff_lines(&unified, DIFF_LINE_LIMIT)
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::StateSource;

    use super::*;
    use crate::undo_dialog::LineKind;

    fn comment(id: u128, lines: Option<(u32, u32)>, status: CommentStatus) -> DocComment {
        DocComment {
            id: CommentId(Ulid(id)),
            project_id: Ulid(1),
            file_path: "PLAN.md".into(),
            context: Default::default(),
            parent_id: lines.is_none().then_some(CommentId(Ulid(1))),
            author: DocComment::USER.into(),
            body: format!("body {id}"),
            start_line: lines.map(|l| l.0),
            end_line: lines.map(|l| l.1),
            quote: lines.map(|_| "q".into()),
            status,
            sent_to: None,
            sent_snapshot: None,
            created_at: 1,
            updated_at: 1,
        }
    }

    fn session(id: u128, kind: SessionKind, state: SessionState, started_at: i64) -> Session {
        Session {
            id: Ulid(id),
            task_id: None,
            project_id: Ulid(1),
            agent_profile: format!("agent{id}"),
            kind,
            title: None,
            native_session_id: None,
            state,
            state_source: StateSource::Screen,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at,
            ended_at: None,
        }
    }

    fn ids(ids: &[CommentId]) -> Vec<u128> {
        ids.iter().map(|id| id.0.0).collect()
    }

    #[test]
    fn open_roots_go_in_anchor_order_or_only_the_selected_ones() {
        let comments = vec![
            comment(1, Some((9, 9)), CommentStatus::Open),
            comment(2, Some((3, 4)), CommentStatus::Open),
            comment(3, Some((5, 5)), CommentStatus::Addressed),
            comment(4, Some((1, 1)), CommentStatus::Resolved),
            comment(5, None, CommentStatus::Open),
        ];
        assert_eq!(ids(&sendable(&comments, None)), [2, 1]);
        let selected = |start, end| Some(Selection { start, end });
        assert_eq!(ids(&sendable(&comments, selected(4, 9))), [2, 1]);
        assert_eq!(ids(&sendable(&comments, selected(8, 12))), [1]);
        // Nothing open under the selection: all open ones.
        assert_eq!(ids(&sendable(&comments, selected(5, 5))), [2, 1]);
        assert!(sendable(&comments[2..4], None).is_empty());
    }

    #[test]
    fn the_last_modifying_session_is_the_default_target() {
        let sessions = vec![
            session(1, SessionKind::Agent, SessionState::Idle, 10),
            session(2, SessionKind::Agent, SessionState::Working, 30),
            session(3, SessionKind::Shell, SessionState::Idle, 40),
            session(4, SessionKind::Agent, SessionState::Exited, 50),
            session(5, SessionKind::Agent, SessionState::Idle, 20),
        ];
        let mut other = session(6, SessionKind::Agent, SessionState::Idle, 60);
        other.project_id = Ulid(2);
        let sessions = [sessions, vec![other]].concat();

        let open =
            |last| SendDialog::new(Ulid(1), "PLAN.md".into(), vec![CommentId(Ulid(9))], last);
        let dialog = open(Some(Ulid(1)));
        assert_eq!(dialog.last_label(), "edited this file");
        let task = SendDialog {
            task: true,
            ..open(Some(Ulid(1)))
        };
        assert_eq!(task.last_label(), "ran this task");
        let choices = dialog.choices(&sessions);
        let order: Vec<_> = choices.iter().map(|c| c.session_id.0).collect();
        assert_eq!(
            order,
            [2, 5, 1],
            "live agents of the workspace, newest first"
        );
        assert_eq!(choices[0].label, "agent2");
        assert_eq!(dialog.selected(&choices), Some(2));
        // The last modifying session exited (or is unknown): the newest.
        assert_eq!(open(Some(Ulid(4))).selected(&choices), Some(0));
        assert_eq!(open(None).selected(&[]), None);
    }

    #[test]
    fn a_selection_goes_to_the_task_session_else_the_most_recently_active_agent() {
        let at = |mut session: Session, changed| {
            session.state_changed_at = changed;
            session
        };
        let sessions = vec![
            at(session(1, SessionKind::Agent, SessionState::Idle, 10), 500),
            at(
                session(2, SessionKind::Agent, SessionState::Working, 30),
                100,
            ),
            at(session(3, SessionKind::Shell, SessionState::Idle, 40), 900),
            at(
                session(4, SessionKind::Agent, SessionState::Exited, 50),
                950,
            ),
        ];
        assert_eq!(most_recently_active(&sessions, Ulid(1)), Some(Ulid(1)));
        assert_eq!(most_recently_active(&sessions, Ulid(2)), None);

        let open =
            |task| SendDialog::selection(Ulid(1), "src/a.rs".into(), "x".into(), task, &sessions);
        let dialog = open(None);
        assert_eq!(dialog.last_session_id, Some(Ulid(1)));
        assert_eq!(dialog.last_label(), "active last");
        assert_eq!(dialog.target(&sessions), Some(Ulid(1)));
        assert!(dialog.comment_ids.is_empty());
        assert_eq!(dialog.prefill.as_deref(), Some("x"));
        let task = open(Some(Ulid(2)));
        assert_eq!(task.last_label(), "ran this task");
        assert_eq!(task.target(&sessions), Some(Ulid(2)));
        let exited = open(Some(Ulid(4)));
        assert!(!exited.task, "an exited task session is not offered");
        assert_eq!(exited.target(&sessions), Some(Ulid(1)));
        // No comments to send: only `target` acts.
        assert_eq!(open(None).send(&sessions), None);
    }

    #[test]
    fn picking_and_sending() {
        let mut sessions = vec![
            session(1, SessionKind::Agent, SessionState::Idle, 10),
            session(2, SessionKind::Agent, SessionState::Working, 30),
        ];
        let comment_ids = vec![CommentId(Ulid(7)), CommentId(Ulid(8))];
        let mut dialog = SendDialog::new(Ulid(1), "PLAN.md".into(), comment_ids.clone(), None);
        dialog.step(&sessions, 1);
        dialog.step(&sessions, 1);
        assert_eq!(dialog.picked, Some(Ulid(1)), "stops at the end");
        dialog.step(&sessions, -5);
        assert_eq!(dialog.picked, Some(Ulid(2)));

        let sent = dialog.send(&sessions);
        assert_eq!(
            sent,
            Some(CommentSendParams {
                comment_ids,
                session_id: Ulid(2),
            })
        );
        assert_eq!(dialog.send(&sessions), None, "in flight");
        dialog.pick(Ulid(1));
        assert_eq!(dialog.picked, Some(Ulid(2)), "no picking while busy");
        dialog.failed("not live".into());
        assert_eq!(
            (dialog.busy, dialog.error.as_deref()),
            (false, Some("not live"))
        );

        // The picked session exited meanwhile: the other one is selected.
        sessions[1].state = SessionState::Exited;
        assert_eq!(dialog.send(&sessions).map(|p| p.session_id), Some(Ulid(1)));
        sessions[0].state = SessionState::Exited;
        dialog.busy = false;
        assert_eq!(dialog.send(&sessions), None, "no live session");
    }

    #[test]
    fn keys() {
        let key = |text: &str| key(&Keystroke::parse(text).unwrap());
        assert_eq!(key("escape"), Some(Key::Close));
        assert_eq!(key("up"), Some(Key::Move(-1)));
        assert_eq!(key("down"), Some(Key::Move(1)));
        assert_eq!(key("enter"), Some(Key::Send));
        assert_eq!(key("ctrl-enter"), None);
        assert_eq!(key("a"), None);
    }

    #[test]
    fn changes_diff_the_sent_snapshot_against_the_current_lines() {
        let current: Vec<String> = [
            "# Plan",
            "",
            "Use an in-memory cache.",
            "",
            "## Deploy",
            "Roll back on failure.",
        ]
        .map(Into::into)
        .to_vec();
        let (lines, hidden) = diff("# Plan\r\n\r\nUse Redis.\r\n\r\n## Deploy\r\n", &current);
        let shape: Vec<(LineKind, &str)> =
            lines.iter().map(|l| (l.kind, l.text.as_str())).collect();
        assert_eq!(
            shape,
            [
                (LineKind::Hunk, "@@ -1,5 +1,6 @@"),
                (LineKind::Context, " # Plan"),
                (LineKind::Context, " "),
                (LineKind::Removed, "-Use Redis."),
                (LineKind::Added, "+Use an in-memory cache."),
                (LineKind::Context, " "),
                (LineKind::Context, " ## Deploy"),
                (LineKind::Added, "+Roll back on failure."),
            ]
        );
        assert_eq!(hidden, 0);
        assert_eq!(
            diff("# Plan\n", &["# Plan".into()]),
            (vec![], 0),
            "unchanged"
        );

        let mut sent = comment(1, Some((3, 3)), CommentStatus::Sent);
        assert_eq!(Changes::new(&sent, &current), None, "never sent");
        sent.sent_snapshot = Some("# Plan\n".into());
        sent.sent_to = Some(Ulid(5));
        let changes = Changes::new(&sent, &current).unwrap();
        assert_eq!(
            (
                changes.anchor.as_str(),
                changes.sent_to,
                changes.lines.len()
            ),
            ("Line 3", Some(Ulid(5)), 7)
        );
    }
}
