//! Peek + quick reply (SPEC §20A.1): a popover next to a sidebar session row with the
//! session's last screen lines (`session.read`, never attaching) and a one-line reply. Hovering
//! a row for [`crate::theme::PEEK_HOVER_DELAY`] opens it; the Peek shortcut opens the first
//! session that needs you with the keyboard in it (↑ / ↓ cycle those sessions, Alt+Y / Alt+N
//! answer, Ctrl+O opens the pane, Esc closes). Pure and tested headlessly; `main.rs` renders it.

use gpui::Keystroke;
use mkrew_protocol::{SessionReadParams, SessionSendParams, SessionState};
use ulid::Ulid;

use crate::inbox::Entry;
use crate::task_dialog::{DialogKey, dialog_key};
use crate::text_box::Input;

/// Screen lines shown.
pub const LINES: u16 = 8;

/// The session's screen as last read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Reading,
    Read(Vec<String>),
    Failed(String),
}

/// What a key press in the popover means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Keep it open (the draft may have changed).
    Edited,
    /// Esc.
    Close,
    /// Enter: send the draft.
    Send,
    /// ↑ / ↓ (-1 / +1): peek the previous / next session that needs you.
    Cycle(i32),
    /// Ctrl+V: the caller inserts the clipboard text.
    Paste,
    /// Alt+Y / Alt+N: send that letter (only while the session needs input, [`Peek::answer`]).
    Answer(char),
    /// Ctrl+O: open the session's pane and close the popover.
    Open,
}

/// The open popover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peek {
    pub session_id: Ulid,
    pub screen: Screen,
    /// The reply being typed.
    pub text: Input,
    /// A `session.send` is in flight.
    pub busy: bool,
    pub error: Option<String>,
    /// The keyboard is in the popover (opened with the shortcut, or clicked).
    pub focused: bool,
    /// The mouse is over the popover.
    pub hovered: bool,
    /// Number of the last `session.read`; older answers are dropped.
    reading: u64,
}

impl Peek {
    pub fn new(session_id: Ulid, focused: bool) -> Self {
        Self {
            session_id,
            screen: Screen::Reading,
            text: Input::default(),
            busy: false,
            error: None,
            focused,
            hovered: false,
            reading: 0,
        }
    }

    /// The `session.read` of the screen as it is now, with its number for [`Self::read`].
    /// The lines shown so far stay until the answer.
    pub fn read_params(&mut self) -> (u64, SessionReadParams) {
        self.reading += 1;
        let params = SessionReadParams {
            session_id: self.session_id,
            lines: LINES,
        };
        (self.reading, params)
    }

    /// The answer to read number `serial`; ignored if a newer read was sent.
    pub fn read(&mut self, serial: u64, read: Result<Vec<String>, String>) {
        if serial == self.reading {
            self.screen = match read {
                Ok(lines) => Screen::Read(lines),
                Err(error) => Screen::Failed(error),
            };
        }
    }

    /// Applies a key press while the popover has the keyboard.
    pub fn key(&mut self, keystroke: &Keystroke) -> Outcome {
        let modifiers = &keystroke.modifiers;
        if !modifiers.modified() {
            match keystroke.key.as_str() {
                "up" => return Outcome::Cycle(-1),
                "down" => return Outcome::Cycle(1),
                _ => {}
            }
        }
        // Alt alone: Ctrl+Alt is AltGr on Windows, which types characters.
        if modifiers.alt && !modifiers.control && !modifiers.platform && !modifiers.shift {
            match keystroke.key.as_str() {
                "y" => return Outcome::Answer('y'),
                "n" => return Outcome::Answer('n'),
                _ => {}
            }
        }
        if modifiers.secondary() && !modifiers.alt && !modifiers.shift && keystroke.key == "o" {
            return Outcome::Open;
        }
        match dialog_key(keystroke) {
            Some(DialogKey::Cancel) => Outcome::Close,
            Some(DialogKey::Enter | DialogKey::Save) => Outcome::Send,
            Some(DialogKey::Paste) => Outcome::Paste,
            Some(DialogKey::Insert(text)) => {
                self.insert(&text);
                Outcome::Edited
            }
            Some(DialogKey::SwitchField | DialogKey::Copy) | None => Outcome::Edited,
            // The caller copies the selection before Cut.
            Some(key) => {
                if !self.busy {
                    self.text.edit(&key);
                }
                Outcome::Edited
            }
        }
    }

    /// Appends typed or pasted text; line breaks become spaces (Enter sends). Ignored while
    /// sending.
    pub fn insert(&mut self, text: &str) {
        if self.busy {
            return;
        }
        let line: String = text
            .chars()
            .map(|c| if c == '\r' || c == '\n' { ' ' } else { c })
            .collect();
        self.text.insert(&line);
        self.error = None;
    }

    /// The `session.send` (text + submit) of the draft; `None` while one is in flight or when
    /// nothing but blanks was typed.
    pub fn send_params(&self) -> Option<SessionSendParams> {
        (!self.busy && !self.text.trim().is_empty()).then(|| SessionSendParams {
            session_id: self.session_id,
            text: self.text.trim().to_owned(),
            submit: true,
        })
    }

    /// The `session.send` of a `y` / `n` button: only while the session needs input.
    pub fn answer(&self, letter: char, state: SessionState) -> Option<SessionSendParams> {
        (!self.busy && state == SessionState::NeedsInput).then(|| SessionSendParams {
            session_id: self.session_id,
            text: letter.to_string(),
            submit: true,
        })
    }

    /// The send finished: a sent draft empties, a failure is shown.
    pub fn sent(&mut self, result: Result<(), String>, draft: bool) {
        self.busy = false;
        match result {
            Ok(()) if draft => self.text.clear(),
            Ok(()) => {}
            Err(error) => self.error = Some(format!("Cannot send: {error}")),
        }
    }

    /// Whether a hover over another row may replace it: not while it has the keyboard or holds
    /// a reply.
    pub fn yields_to_hover(&self) -> bool {
        !self.focused && !self.busy && self.text.is_empty()
    }

    /// Whether it stays open now that the mouse is elsewhere (`row_hovered`: still over its
    /// row).
    pub fn stays(&self, row_hovered: bool) -> bool {
        row_hovered || self.hovered || !self.yields_to_hover()
    }
}

/// The row under the mouse, and the serial of its pending hover delay.
#[derive(Debug, Default)]
pub struct Hover {
    row: Option<Ulid>,
    serial: u64,
}

impl Hover {
    /// The mouse entered a row: returns the serial to pass to [`Hover::fire`] after the delay.
    pub fn enter(&mut self, session_id: Ulid) -> u64 {
        self.serial += 1;
        self.row = Some(session_id);
        self.serial
    }

    /// The mouse left a row (ignored if it is already over another one).
    pub fn leave(&mut self, session_id: Ulid) {
        if self.row == Some(session_id) {
            self.row = None;
        }
    }

    /// The delay of hover `serial` ran out: the row to peek, if the mouse stayed on it.
    pub fn fire(&self, serial: u64) -> Option<Ulid> {
        self.row.filter(|_| serial == self.serial)
    }

    pub fn is_over(&self, session_id: Ulid) -> bool {
        self.row == Some(session_id)
    }
}

/// The Inbox entry `by` steps from `current`, wrapping; an unlisted `current` goes to the
/// first entry (↓) or the last (↑). `None` if the Inbox is empty.
pub fn cycle(entries: &[Entry], current: Ulid, by: i32) -> Option<Ulid> {
    let count = i64::try_from(entries.len()).ok().filter(|n| *n > 0)?;
    let at = match entries.iter().position(|e| e.session_id == current) {
        Some(at) => i64::try_from(at).ok()? + i64::from(by),
        None if by < 0 => count - 1,
        None => 0,
    };
    let at = usize::try_from(at.rem_euclid(count)).ok()?;
    entries.get(at).map(|e| e.session_id)
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::StateSource;

    use super::*;

    fn keystroke(keys: &str) -> Keystroke {
        Keystroke::parse(keys).unwrap()
    }

    fn typed(peek: &mut Peek, text: &str) {
        for c in text.chars() {
            let mut key = keystroke(if c == ' ' { "space" } else { "a" });
            key.key = c.to_string();
            key.key_char = Some(c.to_string());
            assert_eq!(peek.key(&key), Outcome::Edited);
        }
    }

    fn entry(at: i64) -> Entry {
        Entry {
            session_id: Ulid::new(),
            project_id: Ulid::new(),
            workspace: "api".into(),
            agent: "claude".into(),
            title: None,
            state: SessionState::NeedsInput,
            state_source: StateSource::Hook,
            state_changed_at: at,
        }
    }

    #[test]
    fn reads_the_last_lines_and_keeps_only_the_newest_answer() {
        let session = Ulid::new();
        let mut peek = Peek::new(session, true);
        assert_eq!(peek.screen, Screen::Reading);
        let (first, params) = peek.read_params();
        assert_eq!(
            params,
            SessionReadParams {
                session_id: session,
                lines: 8
            }
        );
        let (second, _) = peek.read_params();
        peek.read(first, Ok(vec!["old".into()]));
        assert_eq!(peek.screen, Screen::Reading, "a stale answer is dropped");
        peek.read(second, Ok(vec!["Apply edit? (y/n)".into()]));
        assert_eq!(peek.screen, Screen::Read(vec!["Apply edit? (y/n)".into()]));
        let (third, _) = peek.read_params();
        peek.read(third, Err("session is not live".into()));
        assert_eq!(peek.screen, Screen::Failed("session is not live".into()));
    }

    #[test]
    fn keys_edit_the_reply_send_cycle_and_close() {
        let session = Ulid::new();
        let mut peek = Peek::new(session, true);
        assert_eq!(peek.send_params(), None, "nothing typed");
        typed(&mut peek, " go on!");
        assert_eq!(peek.key(&keystroke("backspace")), Outcome::Edited);
        assert_eq!(peek.text, " go on");
        assert_eq!(peek.key(&keystroke("up")), Outcome::Cycle(-1));
        assert_eq!(peek.key(&keystroke("down")), Outcome::Cycle(1));
        assert_eq!(peek.key(&keystroke("ctrl-v")), Outcome::Paste);
        assert_eq!(peek.key(&keystroke("escape")), Outcome::Close);
        assert_eq!(peek.key(&keystroke("enter")), Outcome::Send);
        assert_eq!(peek.key(&keystroke("alt-y")), Outcome::Answer('y'));
        assert_eq!(peek.key(&keystroke("alt-n")), Outcome::Answer('n'));
        assert_eq!(peek.key(&keystroke("ctrl-o")), Outcome::Open);
        // AltGr (Ctrl+Alt) types into the reply instead of answering.
        assert_eq!(peek.key(&keystroke("ctrl-alt-y")), Outcome::Edited);
        assert_eq!(
            peek.send_params(),
            Some(SessionSendParams {
                session_id: session,
                text: "go on".into(),
                submit: true,
            })
        );

        peek.busy = true;
        assert_eq!(peek.send_params(), None, "one send at a time");
        peek.insert("more");
        peek.key(&keystroke("backspace"));
        assert_eq!(peek.text, " go on");
        peek.sent(Err("session ended".into()), true);
        assert_eq!(peek.error.as_deref(), Some("Cannot send: session ended"));
        assert_eq!(peek.text, " go on", "a failed reply is kept");
        peek.insert("\r\nnow");
        assert_eq!(peek.text, " go on  now");
        assert_eq!(peek.error, None, "typing again clears the failure");
        peek.busy = true;
        peek.sent(Ok(()), true);
        assert_eq!((peek.busy, peek.text.as_str()), (false, ""));
    }

    #[test]
    fn the_reply_takes_select_all_and_ctrl_backspace() {
        let mut peek = Peek::new(Ulid::new(), true);
        peek.insert("go on now");
        assert_eq!(peek.key(&keystroke("ctrl-backspace")), Outcome::Edited);
        assert_eq!(peek.text, "go on ");
        assert_eq!(peek.key(&keystroke("ctrl-a")), Outcome::Edited);
        peek.insert("y");
        assert_eq!(peek.text, "y");
    }

    #[test]
    fn y_and_n_answer_only_a_session_that_needs_input() {
        let session = Ulid::new();
        let mut peek = Peek::new(session, false);
        assert_eq!(
            peek.answer('y', SessionState::NeedsInput),
            Some(SessionSendParams {
                session_id: session,
                text: "y".into(),
                submit: true,
            })
        );
        assert_eq!(peek.answer('n', SessionState::Working), None);
        peek.text = "draft".into();
        peek.busy = true;
        assert_eq!(peek.answer('n', SessionState::NeedsInput), None);
        peek.sent(Ok(()), false);
        assert_eq!(peek.text, "draft", "a button keeps the draft");
    }

    #[test]
    fn a_hover_peek_closes_unless_it_is_in_use() {
        let mut peek = Peek::new(Ulid::new(), false);
        assert!(peek.stays(true), "still over its row");
        assert!(!peek.stays(false));
        peek.hovered = true;
        assert!(peek.stays(false), "over the popover");
        peek.hovered = false;
        peek.text = "y".into();
        assert!(
            peek.stays(false) && !peek.yields_to_hover(),
            "holds a reply"
        );
        peek.text.clear();
        peek.focused = true;
        assert!(
            peek.stays(false) && !peek.yields_to_hover(),
            "has the keyboard"
        );
        peek.focused = false;
        assert!(peek.yields_to_hover());
    }

    #[test]
    fn hover_fires_only_while_the_mouse_stays_on_the_row() {
        let (a, b) = (Ulid::new(), Ulid::new());
        let mut hover = Hover::default();
        let first = hover.enter(a);
        assert_eq!(hover.fire(first), Some(a));
        assert!(hover.is_over(a));
        assert!(!hover.is_over(b));

        // Moving to another row: the old delay is void; a late leave of the old row is ignored.
        let second = hover.enter(b);
        hover.leave(a);
        assert_eq!(hover.fire(first), None);
        assert_eq!(hover.fire(second), Some(b));

        hover.leave(b);
        assert_eq!(hover.fire(second), None);
        assert!(!hover.is_over(b));
    }

    #[test]
    fn cycling_wraps_through_the_inbox() {
        let entries = [entry(3), entry(2), entry(1)];
        let id = |i: usize| entries[i].session_id;
        assert_eq!(cycle(&entries, id(0), 1), Some(id(1)));
        assert_eq!(cycle(&entries, id(2), 1), Some(id(0)));
        assert_eq!(cycle(&entries, id(0), -1), Some(id(2)));
        // A peeked session that left the Inbox: ↓ starts at the first entry, ↑ at the last.
        assert_eq!(cycle(&entries, Ulid::new(), 1), Some(id(0)));
        assert_eq!(cycle(&entries, Ulid::new(), -1), Some(id(2)));
        assert_eq!(cycle(&[], id(0), 1), None);
    }
}
