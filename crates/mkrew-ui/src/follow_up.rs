//! The follow-up field of a running / needs-input card (SPEC §10, §20B.1): the multi-line text
//! box typed on the board and sent to the card's session with `session.send`. Pure and tested
//! headlessly; `main.rs` renders it and reads keys with [`crate::task_dialog::dialog_key`].

use mkrew_protocol::SessionSendParams;
use ulid::Ulid;

use crate::task_dialog::DialogKey;
use crate::text_box::{self, TextBox};

/// What a key press in the field does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Keep it open (the draft may have changed).
    Edited,
    /// Esc: leave the field; a typed draft stays on its card (like the composer).
    Leave,
    /// Ctrl+Enter: send the draft.
    Send,
}

/// The draft of the one card whose follow-up field has the keyboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUp {
    pub task_id: Ulid,
    /// The card's session when the field was opened.
    pub session_id: Ulid,
    pub text: TextBox,
    /// A `session.send` is in flight.
    pub busy: bool,
    pub error: Option<String>,
}

impl FollowUp {
    pub fn new(task_id: Ulid, session_id: Ulid) -> Self {
        Self {
            task_id,
            session_id,
            text: TextBox::default(),
            busy: false,
            error: None,
        }
    }

    /// Enter types a new line, Ctrl+Enter sends, Esc leaves; edits are ignored while sending.
    pub fn key(&mut self, key: DialogKey) -> Outcome {
        match key {
            DialogKey::Cancel => Outcome::Leave,
            DialogKey::Save => Outcome::Send,
            _ if self.busy => Outcome::Edited,
            key => {
                if self.text.key(key) == text_box::Outcome::Edited {
                    self.error = None;
                }
                Outcome::Edited
            }
        }
    }

    /// Appends typed or pasted text (ignored while sending).
    pub fn insert(&mut self, text: &str) {
        if self.busy {
            return;
        }
        self.text.insert(text);
        self.error = None;
    }

    /// The `session.send` (text + Enter) for the draft; `None` while one is in flight or when
    /// nothing but blanks was typed.
    pub fn send_params(&self) -> Option<SessionSendParams> {
        (!self.busy && !self.text.is_blank()).then(|| SessionSendParams {
            session_id: self.session_id,
            text: self.text.text().trim().to_owned(),
            submit: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_types_a_new_line_and_ctrl_enter_sends_the_trimmed_text() {
        let session = Ulid::new();
        let mut draft = FollowUp::new(Ulid::new(), session);
        assert_eq!(draft.send_params(), None, "nothing typed");
        for key in [
            DialogKey::Insert(" fix".into()),
            DialogKey::Enter,
            DialogKey::Insert("the testz".into()),
            DialogKey::Backspace,
            DialogKey::Insert("s ".into()),
            DialogKey::SwitchField,
        ] {
            assert_eq!(draft.key(key), Outcome::Edited);
        }
        assert_eq!(draft.key(DialogKey::Save), Outcome::Send);
        assert_eq!(
            draft.send_params(),
            Some(SessionSendParams {
                session_id: session,
                text: "fix\nthe tests".into(),
                submit: true,
            })
        );
        assert_eq!(draft.key(DialogKey::Cancel), Outcome::Leave);
    }

    #[test]
    fn pasted_lines_stay_and_nothing_changes_while_sending() {
        let mut draft = FollowUp::new(Ulid::new(), Ulid::new());
        draft.error = Some("Cannot send".into());
        draft.insert("one\r\ntwo");
        assert_eq!(draft.text.text(), "one\ntwo");
        assert_eq!(draft.error, None, "typing again clears the failure");

        draft.busy = true;
        draft.insert("more");
        draft.key(DialogKey::Backspace);
        draft.key(DialogKey::Enter);
        assert_eq!(draft.text.text(), "one\ntwo");
        assert_eq!(draft.send_params(), None);
        assert_eq!(draft.key(DialogKey::Save), Outcome::Send);

        draft.busy = false;
        draft.text.clear();
        draft.insert(" \n ");
        assert_eq!(draft.send_params(), None, "blanks only");
    }
}
