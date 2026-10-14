//! "Broadcast to agents…" (SPEC §20A.6): one text sent to several live agent sessions of the
//! workspace, one `session.send` (with submit) per checked session, each row showing how its
//! send went. No daemon request of its own. Pure and tested headlessly; `main.rs` renders it.

use gpui::Keystroke;
use mkrew_protocol::{Session, SessionKind, SessionSendParams, SessionState};
use ulid::Ulid;

use crate::history;
use crate::task_dialog::{DialogKey, dialog_key};
use crate::text_box::Input;

/// How one session's send went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    Sending,
    /// Written to the agent.
    Sent,
    /// Accepted while the agent was working: the daemon delivers it once the turn ends
    /// (SPEC §8 send queue).
    Queued,
    Failed(String),
}

impl Delivery {
    /// The icon before the result text (§11 never shows a state by color alone).
    pub fn icon(&self) -> &'static str {
        match self {
            Delivery::Sending => "◌",
            Delivery::Sent => "✓",
            Delivery::Queued => "⧗",
            Delivery::Failed(_) => "✕",
        }
    }

    /// The row's result text.
    pub fn text(&self) -> String {
        match self {
            Delivery::Sending => "sending…".into(),
            Delivery::Sent => "sent".into(),
            Delivery::Queued => "queued".into(),
            Delivery::Failed(error) => format!("failed: {error}"),
        }
    }
}

/// One live agent session of the workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub session_id: Ulid,
    /// The session title, else its agent profile.
    pub label: String,
    pub checked: bool,
    /// The last broadcast's result for it; `None` before the first send or if unchecked then.
    pub delivery: Option<Delivery>,
}

/// What a key press in the dialog means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Keep it open (the draft, the cursor or a checkbox may have changed).
    Edited,
    /// Esc.
    Close,
    /// Enter: send to every checked session.
    Send,
    /// Ctrl+V: the caller inserts the clipboard text.
    Paste,
}

/// The open dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Broadcast {
    pub project_id: Ulid,
    pub rows: Vec<Row>,
    /// The text being typed (one line; Enter sends).
    pub text: Input,
    /// The row ↑ / ↓ move and Tab toggles.
    pub cursor: usize,
}

/// Whether `session` is a live (not exited) agent session of `project_id`.
fn target(session: &Session, project_id: Ulid) -> bool {
    session.project_id == project_id
        && session.kind == SessionKind::Agent
        && session.state != SessionState::Exited
}

/// Whether the workspace has a session to broadcast to (the palette offers it only then).
pub fn has_targets(project_id: Ulid, sessions: &[Session]) -> bool {
    sessions.iter().any(|session| target(session, project_id))
}

impl Broadcast {
    /// The workspace's live (not exited) agent sessions, all checked, in `sessions` order.
    pub fn new(project_id: Ulid, sessions: &[Session]) -> Self {
        let rows = sessions
            .iter()
            .filter(|session| target(session, project_id))
            .map(|session| Row {
                session_id: session.id,
                label: history::label(session),
                checked: true,
                delivery: None,
            })
            .collect();
        Self {
            project_id,
            rows,
            text: Input::default(),
            cursor: 0,
        }
    }

    /// A send is in flight for some row; the text and checkboxes are locked meanwhile.
    pub fn busy(&self) -> bool {
        self.rows
            .iter()
            .any(|row| row.delivery == Some(Delivery::Sending))
    }

    /// Applies a key press: ↑ / ↓ move the cursor, Tab toggles its row, typing edits the text.
    pub fn key(&mut self, keystroke: &Keystroke) -> Outcome {
        if !keystroke.modifiers.modified() {
            match keystroke.key.as_str() {
                "up" => {
                    self.cursor = self.cursor.saturating_sub(1);
                    return Outcome::Edited;
                }
                "down" => {
                    self.cursor = (self.cursor + 1).min(self.rows.len().saturating_sub(1));
                    return Outcome::Edited;
                }
                _ => {}
            }
        }
        match dialog_key(keystroke) {
            Some(DialogKey::Cancel) => Outcome::Close,
            Some(DialogKey::Enter | DialogKey::Save) => Outcome::Send,
            Some(DialogKey::Paste) => Outcome::Paste,
            Some(DialogKey::SwitchField) => {
                self.toggle(self.cursor);
                Outcome::Edited
            }
            Some(key @ (DialogKey::Backspace | DialogKey::DeleteWord | DialogKey::SelectAll)) => {
                if !self.busy() {
                    self.text.edit(&key);
                }
                Outcome::Edited
            }
            Some(DialogKey::Insert(text)) => {
                self.insert(&text);
                Outcome::Edited
            }
            Some(
                DialogKey::Delete { .. }
                | DialogKey::Move { .. }
                | DialogKey::Copy
                | DialogKey::Cut,
            )
            | None => Outcome::Edited,
        }
    }

    /// Checks or unchecks row `index` (and moves the cursor there). Ignored while sending.
    pub fn toggle(&mut self, index: usize) {
        if self.busy() {
            return;
        }
        if let Some(row) = self.rows.get_mut(index) {
            row.checked = !row.checked;
            self.cursor = index;
        }
    }

    /// Appends typed or pasted text; line breaks become spaces (Enter sends). Ignored while
    /// sending.
    pub fn insert(&mut self, text: &str) {
        if self.busy() {
            return;
        }
        let line: String = text
            .chars()
            .map(|c| if c == '\r' || c == '\n' { ' ' } else { c })
            .collect();
        self.text.insert(&line);
    }

    /// Whether Send can act: some row checked, some text, nothing in flight.
    pub fn can_send(&self) -> bool {
        !self.busy() && !self.text.trim().is_empty() && self.rows.iter().any(|row| row.checked)
    }

    /// Starts a broadcast: one `session.send` (text + submit) per checked row, which turns
    /// `Sending`; unchecked rows lose their old result. Empty when [`Self::can_send`] is false.
    pub fn begin(&mut self) -> Vec<SessionSendParams> {
        if !self.can_send() {
            return Vec::new();
        }
        let text = self.text.trim().to_owned();
        let mut sends = Vec::new();
        for row in &mut self.rows {
            if row.checked {
                row.delivery = Some(Delivery::Sending);
                sends.push(SessionSendParams {
                    session_id: row.session_id,
                    text: text.clone(),
                    submit: true,
                });
            } else {
                row.delivery = None;
            }
        }
        sends
    }

    /// The send to `session_id` finished; `Ok(queued)` is the daemon's `queued` (the text waits
    /// for the turn to end). The draft empties once every send succeeded.
    pub fn sent(&mut self, session_id: Ulid, result: Result<bool, String>) {
        let Some(row) = self
            .rows
            .iter_mut()
            .find(|row| row.session_id == session_id && row.delivery == Some(Delivery::Sending))
        else {
            return;
        };
        row.delivery = Some(match result {
            Ok(true) => Delivery::Queued,
            Ok(false) => Delivery::Sent,
            Err(error) => Delivery::Failed(error),
        });
        let all_ok = self
            .rows
            .iter()
            .all(|row| matches!(row.delivery, None | Some(Delivery::Sent | Delivery::Queued)));
        if !self.busy() && all_ok {
            self.text.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::StateSource;

    use super::*;

    fn session(
        project_id: Ulid,
        kind: SessionKind,
        title: Option<&str>,
        state: SessionState,
    ) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id,
            agent_profile: "claude".into(),
            kind,
            title: title.map(Into::into),
            terminal_title: None,
            color: None,
            native_session_id: None,
            state,
            state_source: StateSource::Screen,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
            pinned: false,
            position: 0.0,
            plan_path: None,
        }
    }

    fn press(dialog: &mut Broadcast, keys: &str) -> Outcome {
        dialog.key(&Keystroke::parse(keys).unwrap())
    }

    /// A dialog on two working-or-idle agents of one workspace.
    fn two() -> Broadcast {
        let web = Ulid::new();
        let sessions = vec![
            session(web, SessionKind::Agent, Some("api"), SessionState::Idle),
            session(web, SessionKind::Agent, None, SessionState::Working),
        ];
        Broadcast::new(web, &sessions)
    }

    #[test]
    fn lists_the_live_agent_sessions_of_the_workspace_all_checked() {
        let web = Ulid::new();
        let api = Ulid::new();
        let sessions = vec![
            session(web, SessionKind::Agent, Some("fix"), SessionState::Working),
            session(web, SessionKind::Shell, Some("sh"), SessionState::Idle),
            session(web, SessionKind::Agent, Some("old"), SessionState::Exited),
            session(api, SessionKind::Agent, Some("other"), SessionState::Idle),
            session(web, SessionKind::Agent, None, SessionState::NeedsInput),
        ];
        let dialog = Broadcast::new(web, &sessions);
        let rows: Vec<_> = dialog
            .rows
            .iter()
            .map(|row| (row.session_id, row.label.as_str(), row.checked))
            .collect();
        assert_eq!(
            rows,
            [
                (sessions[0].id, "fix", true),
                (sessions[4].id, "claude", true)
            ]
        );
        assert!(Broadcast::new(Ulid::new(), &sessions).rows.is_empty());
        assert!(has_targets(web, &sessions));
        assert!(!has_targets(api, &sessions[..3]));
    }

    #[test]
    fn keys_move_the_cursor_toggle_rows_and_edit_the_text() {
        let mut dialog = two();
        assert_eq!(press(&mut dialog, "up"), Outcome::Edited);
        assert_eq!(dialog.cursor, 0);
        press(&mut dialog, "down");
        press(&mut dialog, "down");
        assert_eq!(dialog.cursor, 1);
        assert_eq!(press(&mut dialog, "tab"), Outcome::Edited);
        assert!(!dialog.rows[1].checked);
        press(&mut dialog, "tab");
        assert!(dialog.rows[1].checked);
        dialog.toggle(0);
        assert!(!dialog.rows[0].checked);
        assert_eq!(dialog.cursor, 0);
        for c in ["o", "k"] {
            let mut key = Keystroke::parse("a").unwrap();
            key.key = c.into();
            key.key_char = Some(c.into());
            assert_eq!(dialog.key(&key), Outcome::Edited);
        }
        press(&mut dialog, "backspace");
        dialog.insert(" go\r\non");
        assert_eq!(dialog.text, "o go  on");
        assert_eq!(press(&mut dialog, "escape"), Outcome::Close);
        assert_eq!(press(&mut dialog, "enter"), Outcome::Send);
        assert_eq!(press(&mut dialog, "ctrl-v"), Outcome::Paste);
    }

    #[test]
    fn the_text_takes_select_all_and_ctrl_backspace() {
        let mut dialog = two();
        dialog.insert("run the tests");
        assert_eq!(press(&mut dialog, "ctrl-backspace"), Outcome::Edited);
        assert_eq!(dialog.text, "run the ");
        assert_eq!(press(&mut dialog, "ctrl-a"), Outcome::Edited);
        assert!(dialog.text.all_selected());
        dialog.insert("go");
        assert_eq!(dialog.text, "go");
    }

    #[test]
    fn sends_one_text_per_checked_session_and_shows_each_result() {
        let mut dialog = two();
        assert!(!dialog.can_send());
        assert!(dialog.begin().is_empty());
        dialog.insert("  run the tests  ");
        let ids: Vec<_> = dialog.rows.iter().map(|row| row.session_id).collect();
        let sends = dialog.begin();
        assert_eq!(
            sends,
            ids.iter()
                .map(|id| SessionSendParams {
                    session_id: *id,
                    text: "run the tests".into(),
                    submit: true,
                })
                .collect::<Vec<_>>()
        );
        assert!(dialog.busy());
        // Locked while sending.
        assert!(dialog.begin().is_empty());
        dialog.toggle(0);
        dialog.insert("x");
        assert!(dialog.rows[0].checked);
        assert_eq!(dialog.text, "  run the tests  ");

        dialog.sent(ids[1], Ok(true));
        assert_eq!(dialog.rows[1].delivery, Some(Delivery::Queued));
        assert!(dialog.busy());
        dialog.sent(ids[0], Ok(false));
        assert_eq!(dialog.rows[0].delivery, Some(Delivery::Sent));
        assert!(!dialog.busy());
        assert_eq!(dialog.text, "");
        // A late duplicate answer changes nothing.
        dialog.sent(ids[0], Err("gone".into()));
        assert_eq!(dialog.rows[0].delivery, Some(Delivery::Sent));
    }

    #[test]
    fn a_failure_keeps_the_text_and_unchecked_rows_lose_their_old_result() {
        let mut dialog = two();
        dialog.insert("go");
        let ids: Vec<_> = dialog.rows.iter().map(|row| row.session_id).collect();
        dialog.begin();
        dialog.sent(ids[0], Err("session exited".into()));
        dialog.sent(ids[1], Ok(false));
        assert_eq!(
            dialog.rows[0].delivery,
            Some(Delivery::Failed("session exited".into()))
        );
        assert_eq!(dialog.text, "go");
        // Retry only the failed one.
        dialog.toggle(1);
        let sends = dialog.begin();
        assert_eq!(sends.len(), 1);
        assert_eq!(sends[0].session_id, ids[0]);
        assert_eq!(dialog.rows[1].delivery, None);
        // Nothing checked: nothing to send.
        dialog.sent(ids[0], Ok(false));
        dialog.text = "again".into();
        dialog.toggle(0);
        assert!(!dialog.can_send());
    }

    #[test]
    fn result_texts() {
        assert_eq!(Delivery::Sending.text(), "sending…");
        assert_eq!(Delivery::Sent.text(), "sent");
        assert_eq!(Delivery::Queued.text(), "queued");
        assert_eq!(Delivery::Failed("x".into()).text(), "failed: x");
        assert_eq!(Delivery::Queued.icon(), "⧗");
        assert_eq!(Delivery::Failed("x".into()).icon(), "✕");
    }
}
