//! The "History" dialog of a workspace (SPEC §10, §18 M5): its finished sessions, newest first,
//! and the selected one's output log (`session.history`) replayed read-only — tail-bounded,
//! never attached. Pure and tested headlessly; `main.rs` renders it and owns the replay pane.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

use gpui::Keystroke;
use mkrew_protocol::{Session, SessionState};
use ulid::Ulid;

use crate::status;

/// Most log bytes replayed (the newest ones); `mk session history <id>` prints all of it.
pub const TAIL_BYTES: u64 = 2 * 1024 * 1024;

/// The workspace's sessions, as far as `session.list` answered.
#[derive(Debug, Clone, PartialEq)]
pub enum Listing {
    Loading,
    /// Finished sessions, newest first.
    Loaded(Vec<Session>),
    Failed(String),
}

/// The selected session's log.
#[derive(Debug, Clone, PartialEq)]
pub enum Log {
    Loading,
    /// Replayed in the pane; `cut` = older output was left out ([`TAIL_BYTES`]).
    Shown {
        cut: bool,
    },
    /// The session logged no output (or its files are gone).
    Empty,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryDialog {
    pub project_id: Ulid,
    /// The workspace's name, for the title.
    pub workspace: String,
    pub listing: Listing,
    pub selected: Option<Ulid>,
    pub log: Log,
}

impl HistoryDialog {
    pub fn new(project_id: Ulid, workspace: String) -> Self {
        Self {
            project_id,
            workspace,
            listing: Listing::Loading,
            selected: None,
            log: Log::Loading,
        }
    }

    /// The `session.list` reply: keeps the workspace's finished sessions and selects the
    /// newest. Returns the session whose log to read now.
    pub fn load(&mut self, sessions: Vec<Session>) -> Option<Ulid> {
        let finished = finished(sessions, self.project_id);
        let newest = finished.first().map(|session| session.id);
        self.listing = Listing::Loaded(finished);
        newest.filter(|&id| self.select(id))
    }

    /// The finished sessions, once listed.
    pub fn sessions(&self) -> &[Session] {
        match &self.listing {
            Listing::Loaded(sessions) => sessions,
            Listing::Loading | Listing::Failed(_) => &[],
        }
    }

    pub fn selected_session(&self) -> Option<&Session> {
        let id = self.selected?;
        self.sessions().iter().find(|session| session.id == id)
    }

    /// Selects a listed session; true = its log must be read (it was not selected already).
    pub fn select(&mut self, id: Ulid) -> bool {
        if self.selected == Some(id) || !self.sessions().iter().any(|session| session.id == id) {
            return false;
        }
        self.selected = Some(id);
        self.log = Log::Loading;
        true
    }

    /// The session `offset` rows away from the selected one (clamped to the list), for ↑ / ↓.
    pub fn neighbour(&self, offset: isize) -> Option<Ulid> {
        let sessions = self.sessions();
        let last = sessions.len().checked_sub(1)?;
        let at = self
            .selected
            .and_then(|id| sessions.iter().position(|session| session.id == id))
            .map_or(0, |index| index.saturating_add_signed(offset).min(last));
        Some(sessions[at].id)
    }
}

/// The finished (exited) sessions of `project_id`, the last one to end first.
pub fn finished(sessions: Vec<Session>, project_id: Ulid) -> Vec<Session> {
    let mut finished: Vec<Session> = sessions
        .into_iter()
        .filter(|session| session.project_id == project_id && session.state == SessionState::Exited)
        .collect();
    finished.sort_by_key(|session| std::cmp::Reverse((ended_at(session), session.id)));
    finished
}

/// When the session ended (its last state change if the end time was never recorded).
fn ended_at(session: &Session) -> i64 {
    session.ended_at.unwrap_or(session.state_changed_at)
}

/// A session's name (ADR 0031): the user's title, else the title its agent gave the terminal.
pub fn title(session: &Session) -> Option<&str> {
    session
        .title
        .as_deref()
        .or(session.terminal_title.as_deref())
}

/// The row's title: [`title`], else its agent profile.
pub fn label(session: &Session) -> String {
    title(session).unwrap_or(&session.agent_profile).to_owned()
}

/// The row's details at `now_ms`: `exited (1) · ran 4m · 2h ago`.
pub fn details(session: &Session, now_ms: i64) -> String {
    let ended = ended_at(session);
    format!(
        "{} · ran {} · {} ago",
        status::text(SessionState::Exited, session.exit_code, ended, now_ms),
        status::format_duration(ended - session.started_at),
        status::format_duration(now_ms - ended),
    )
}

/// The session exited with a non-zero code (its ✕ is drawn in the exited color; a clean exit
/// stays muted, so a long list is not a wall of red).
pub fn failed(session: &Session) -> bool {
    session.exit_code.is_some_and(|code| code != 0)
}

/// Esc closes the dialog; ↑ / ↓ pick the previous / next session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Close,
    Move(isize),
}

pub fn key(keystroke: &Keystroke) -> Option<Key> {
    let modified = keystroke.modifiers.modified();
    match keystroke.key.as_str() {
        "escape" => Some(Key::Close),
        "up" if !modified => Some(Key::Move(-1)),
        "down" if !modified => Some(Key::Move(1)),
        _ => None,
    }
}

/// The newest output of a session's log `files` (oldest first, as `session.history` lists
/// them), at most `limit` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tail {
    pub bytes: Vec<u8>,
    /// Older output was left out.
    pub cut: bool,
}

/// Reads the last `limit` bytes of `files` as one stream. A cut stream starts after its first
/// line break, so the replay does not begin inside an escape sequence. A file that is gone
/// (rotated away meanwhile) is skipped.
pub fn read_tail(files: &[PathBuf], limit: u64) -> io::Result<Tail> {
    let mut parts = Vec::new();
    let mut left = limit;
    let mut cut = false;
    for path in files.iter().rev() {
        let mut file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let len = file.metadata()?.len();
        if len > left {
            cut = true;
            file.seek(SeekFrom::Start(len - left))?;
        }
        let mut part = Vec::new();
        file.take(left).read_to_end(&mut part)?;
        // `read_to_end` stops at `left`: far below u64::MAX.
        left -= part.len() as u64;
        parts.push(part);
    }
    let mut bytes: Vec<u8> = parts.into_iter().rev().flatten().collect();
    if cut && let Some(newline) = bytes.iter().position(|&byte| byte == b'\n') {
        bytes.drain(..=newline);
    }
    Ok(Tail { bytes, cut })
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{SessionKind, StateSource};

    use super::*;

    fn session(project_id: Ulid, state: SessionState, ended_at: Option<i64>) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id,
            agent_profile: "claude".into(),
            kind: SessionKind::Agent,
            title: None,
            terminal_title: None,
            color: None,
            native_session_id: None,
            state,
            state_source: StateSource::Daemon,
            state_changed_at: 5_000,
            exit_code: Some(0),
            cols: 80,
            rows: 24,
            started_at: 1_000,
            ended_at,
            pinned: false,
            plan_path: None,
        }
    }

    #[test]
    fn finished_keeps_the_workspaces_exited_sessions_newest_first() {
        let project = Ulid::new();
        let old = session(project, SessionState::Exited, Some(2_000));
        let new = session(project, SessionState::Exited, Some(9_000));
        // No end time recorded: its last state change (5 s) counts.
        let middle = session(project, SessionState::Exited, None);
        let live = session(project, SessionState::Working, None);
        let other = session(Ulid::new(), SessionState::Exited, Some(10_000));
        let ids: Vec<Ulid> = finished(
            vec![old.clone(), live, new.clone(), other, middle.clone()],
            project,
        )
        .iter()
        .map(|session| session.id)
        .collect();
        assert_eq!(ids, vec![new.id, middle.id, old.id]);
    }

    #[test]
    fn loading_selects_the_newest_session() {
        let project = Ulid::new();
        let old = session(project, SessionState::Exited, Some(2_000));
        let new = session(project, SessionState::Exited, Some(9_000));
        let mut dialog = HistoryDialog::new(project, "api".into());
        assert_eq!(dialog.load(vec![old.clone(), new.clone()]), Some(new.id));
        assert_eq!(dialog.selected_session(), Some(&new));
        assert_eq!(dialog.log, Log::Loading);

        let mut empty = HistoryDialog::new(project, "api".into());
        assert_eq!(empty.load(vec![]), None);
        assert_eq!(empty.listing, Listing::Loaded(vec![]));
        assert_eq!(empty.neighbour(1), None);
    }

    #[test]
    fn select_reads_a_log_only_for_a_new_listed_session() {
        let project = Ulid::new();
        let a = session(project, SessionState::Exited, Some(3_000));
        let b = session(project, SessionState::Exited, Some(2_000));
        let mut dialog = HistoryDialog::new(project, "api".into());
        dialog.load(vec![a.clone(), b.clone()]);
        dialog.log = Log::Shown { cut: false };
        assert!(!dialog.select(a.id), "already selected");
        assert!(!dialog.select(Ulid::new()), "not listed");
        assert_eq!(dialog.log, Log::Shown { cut: false });
        assert!(dialog.select(b.id));
        assert_eq!(dialog.log, Log::Loading);
    }

    #[test]
    fn neighbour_moves_within_the_list() {
        let project = Ulid::new();
        let a = session(project, SessionState::Exited, Some(3_000));
        let b = session(project, SessionState::Exited, Some(2_000));
        let c = session(project, SessionState::Exited, Some(1_000));
        let mut dialog = HistoryDialog::new(project, "api".into());
        dialog.load(vec![c.clone(), a.clone(), b.clone()]);
        assert_eq!(dialog.neighbour(1), Some(b.id));
        assert_eq!(dialog.neighbour(-1), Some(a.id), "clamped at the top");
        dialog.select(c.id);
        assert_eq!(dialog.neighbour(1), Some(c.id), "clamped at the bottom");
        assert_eq!(dialog.neighbour(-1), Some(b.id));
    }

    #[test]
    fn rows_show_title_or_agent_and_the_run() {
        let mut row = session(Ulid::new(), SessionState::Exited, Some(241_000));
        row.exit_code = Some(1);
        assert_eq!(label(&row), "claude");
        assert_eq!(
            details(&row, 241_000 + 7_200_000),
            "exited (1) · ran 4m · 2h ago"
        );
        assert!(failed(&row));
        row.exit_code = Some(0);
        assert!(!failed(&row));
        row.exit_code = None;
        assert!(!failed(&row));
        row.title = Some("Fix login".into());
        assert_eq!(label(&row), "Fix login");
    }

    #[test]
    fn the_users_title_wins_then_the_terminal_title_then_the_agent() {
        let mut s = session(Ulid::new(), SessionState::Working, None);
        assert_eq!((title(&s), label(&s).as_str()), (None, "claude"));
        s.terminal_title = Some("Fix login".into());
        assert_eq!(label(&s), "Fix login");
        s.title = Some("Mine".into());
        assert_eq!(title(&s), Some("Mine"));
        assert_eq!(label(&s), "Mine");
        // Clearing the user's title shows the terminal title again.
        s.title = None;
        assert_eq!(title(&s), Some("Fix login"));
    }

    #[test]
    fn keys_close_and_move() {
        let parse = |keys: &str| key(&Keystroke::parse(keys).unwrap());
        assert_eq!(parse("escape"), Some(Key::Close));
        assert_eq!(parse("up"), Some(Key::Move(-1)));
        assert_eq!(parse("down"), Some(Key::Move(1)));
        assert_eq!(parse("alt-down"), None);
        assert_eq!(parse("enter"), None);
    }

    #[test]
    fn read_tail_joins_the_parts_and_cuts_at_a_line_break() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("s.log.1");
        let new = dir.path().join("s.log");
        std::fs::write(&old, b"one\ntwo\n").unwrap();
        std::fs::write(&new, b"three\n").unwrap();
        let files = vec![old.clone(), new.clone()];

        let all = read_tail(&files, TAIL_BYTES).unwrap();
        assert_eq!(all.bytes, b"one\ntwo\nthree\n");
        assert!(!all.cut);

        // The last 9 bytes start inside "two": the partial line is dropped.
        let tail = read_tail(&files, 9).unwrap();
        assert_eq!(tail.bytes, b"three\n");
        assert!(tail.cut);

        // Exactly the newest file: the older part is left out, so it counts as cut.
        let newest = read_tail(&files, 6).unwrap();
        assert!(newest.cut);
        assert_eq!(
            newest.bytes, b"",
            "everything up to the first line break goes"
        );

        // An empty older part leaves nothing out.
        std::fs::write(&old, b"").unwrap();
        let exact = read_tail(&files, 6).unwrap();
        assert_eq!(exact.bytes, b"three\n");
        assert!(!exact.cut);

        assert_eq!(
            read_tail(&[], TAIL_BYTES).unwrap(),
            Tail {
                bytes: Vec::new(),
                cut: false
            }
        );

        std::fs::remove_file(&old).unwrap();
        let rotated = read_tail(&files, TAIL_BYTES).unwrap();
        assert_eq!(rotated.bytes, b"three\n");
        assert!(!rotated.cut);
    }

    #[test]
    fn a_replayed_tail_shows_the_sessions_last_screen() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("s.log.1");
        let new = dir.path().join("s.log");
        std::fs::write(&old, b"\x1b[31mlong gone\x1b[0m\r\n").unwrap();
        std::fs::write(&new, b"Task: hi\r\n\x1b[1mDone\x1b[0m\r\n").unwrap();
        // The cut lands inside the old part's last escape sequence: that partial line goes.
        let tail = read_tail(&[old, new], 28).unwrap();
        assert!(tail.cut);
        let mut emulator = crate::terminal::emulator::Emulator::new(20, 3);
        emulator.advance(&tail.bytes);
        assert_eq!(emulator.grid().to_text(), "Task: hi\nDone\n\n");
    }
}
