//! The prompt composer docked at an agent pane's bottom (SPEC §20B.1): one draft per session,
//! kept in UI memory while the composer is hidden or another pane has the keyboard, sent with
//! `session.send` + Enter. Pure and tested headlessly; `main.rs` renders it.

use std::collections::HashMap;

use mkrew_protocol::SessionSendParams;
use ulid::Ulid;

use crate::task_dialog::DialogKey;
use crate::text_box::{self, TextBox};

/// One session's composer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Draft {
    pub text: TextBox,
    /// Docked under the pane (Esc hides it, the draft stays).
    pub shown: bool,
    /// A `session.send` is in flight.
    pub busy: bool,
    /// Why the last send failed.
    pub error: Option<String>,
}

/// Every session's draft.
#[derive(Debug, Clone, Default)]
pub struct Composers {
    drafts: HashMap<Ulid, Draft>,
}

impl Composers {
    /// Shows `session`'s composer with its draft (a new one the first time).
    pub fn open(&mut self, session: Ulid) {
        self.drafts.entry(session).or_default().shown = true;
    }

    /// Hides `session`'s composer, keeping its draft.
    pub fn hide(&mut self, session: Ulid) {
        if let Some(draft) = self.drafts.get_mut(&session) {
            draft.shown = false;
        }
    }

    /// `session`'s draft while its composer is shown.
    pub fn shown(&self, session: Ulid) -> Option<&Draft> {
        self.drafts.get(&session).filter(|draft| draft.shown)
    }

    /// Applies a key typed in `session`'s composer (ignored while sending, except Esc).
    pub fn key(&mut self, session: Ulid, key: DialogKey) -> text_box::Outcome {
        let Some(draft) = self.drafts.get_mut(&session) else {
            return text_box::Outcome::Edited;
        };
        match key {
            DialogKey::Cancel => text_box::Outcome::Cancel,
            _ if draft.busy => text_box::Outcome::Edited,
            key => {
                let outcome = draft.text.key(key);
                if outcome == text_box::Outcome::Edited {
                    draft.error = None;
                }
                outcome
            }
        }
    }

    /// Pastes `text` into `session`'s draft (ignored while sending).
    pub fn insert(&mut self, session: Ulid, text: &str) {
        if let Some(draft) = self.drafts.get_mut(&session).filter(|d| !d.busy) {
            draft.text.insert(text);
            draft.error = None;
        }
    }

    /// Marks `session`'s draft as sending and returns its `session.send` (text + Enter);
    /// `None` while one is in flight or when nothing but blanks was typed.
    pub fn begin_send(&mut self, session: Ulid) -> Option<SessionSendParams> {
        let draft = self
            .drafts
            .get_mut(&session)
            .filter(|draft| !draft.busy && !draft.text.is_blank())?;
        draft.busy = true;
        draft.error = None;
        Some(SessionSendParams {
            session_id: session,
            text: without_blank_edges(draft.text.text()).to_owned(),
            submit: true,
        })
    }

    /// The send finished: written or queued empties the draft, a failure keeps it.
    pub fn sent(&mut self, session: Ulid, result: Result<(), String>) {
        let Some(draft) = self.drafts.get_mut(&session) else {
            return;
        };
        draft.busy = false;
        match result {
            Ok(()) => draft.text.clear(),
            Err(error) => draft.error = Some(format!("Cannot send: {error}")),
        }
    }
}

/// `text` without trailing blanks and leading blank lines; the first line keeps its indent
/// (pasted code, a list).
fn without_blank_edges(text: &str) -> &str {
    let text = text.trim_end();
    let first = text.find(|c: char| !c.is_whitespace()).unwrap_or(0);
    let start = text[..first].rfind('\n').map_or(0, |newline| newline + 1);
    &text[start..]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_box::Outcome;

    fn type_text(composers: &mut Composers, session: Ulid, text: &str) {
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 {
                composers.key(session, DialogKey::Enter);
            }
            composers.key(session, DialogKey::Insert(line.into()));
        }
    }

    #[test]
    fn drafts_survive_hiding_and_switching_sessions() {
        let (a, b) = (Ulid::new(), Ulid::new());
        let mut composers = Composers::default();
        assert_eq!(composers.shown(a), None);
        composers.open(a);
        type_text(&mut composers, a, "first\nsecond");
        composers.open(b);
        type_text(&mut composers, b, "other");
        assert_eq!(composers.key(a, DialogKey::Cancel), Outcome::Cancel);
        composers.hide(a);
        assert_eq!(composers.shown(a), None);
        assert_eq!(composers.shown(b).map(|d| d.text.text()), Some("other"));

        composers.open(a);
        assert_eq!(
            composers.shown(a).map(|d| d.text.text()),
            Some("first\nsecond")
        );
    }

    #[test]
    fn a_multi_line_send_goes_as_one_text_and_empties_the_draft() {
        let session = Ulid::new();
        let mut composers = Composers::default();
        composers.open(session);
        assert_eq!(composers.begin_send(session), None, "nothing typed");
        type_text(&mut composers, session, " \n\n  explain\nthis\n");
        assert_eq!(composers.key(session, DialogKey::Save), Outcome::Submit);
        assert_eq!(
            composers.begin_send(session),
            Some(SessionSendParams {
                session_id: session,
                text: "  explain\nthis".into(),
                submit: true,
            })
        );
        assert_eq!(composers.begin_send(session), None, "already sending");
        composers.key(session, DialogKey::Insert("x".into()));
        composers.insert(session, "y");
        assert_eq!(
            composers.shown(session).map(|d| d.text.text()),
            Some(" \n\n  explain\nthis\n"),
            "no edits while sending"
        );

        composers.sent(session, Ok(()));
        let draft = composers.shown(session).unwrap();
        assert_eq!((draft.text.text(), draft.busy), ("", false));
    }

    #[test]
    fn a_failed_send_keeps_the_draft_until_edited() {
        let session = Ulid::new();
        let mut composers = Composers::default();
        composers.open(session);
        composers.insert(session, "hi");
        composers.begin_send(session);
        composers.sent(session, Err("session exited".into()));
        let draft = composers.shown(session).unwrap();
        assert_eq!(draft.text.text(), "hi");
        assert_eq!(draft.error.as_deref(), Some("Cannot send: session exited"));

        composers.key(session, DialogKey::Insert("!".into()));
        assert_eq!(composers.shown(session).unwrap().error, None);
    }
}
