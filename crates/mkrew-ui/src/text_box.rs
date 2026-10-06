//! The multi-line text box (SPEC §20B.1): Enter types a new line, Ctrl+Enter submits, Esc
//! cancels. Text is typed and deleted at the end, like the other fields. Pure and tested
//! headlessly; keys come from [`crate::task_dialog::dialog_key`], `main.rs` renders the text.

use crate::task_dialog::DialogKey;

/// What a key press in the box does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Keep it open (the text may have changed).
    Edited,
    /// Esc.
    Cancel,
    /// Ctrl+Enter (Cmd+Enter on macOS).
    Submit,
}

/// The text of a multi-line box; line breaks are always `\n`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextBox {
    text: String,
}

impl TextBox {
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether nothing but blanks was typed.
    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }

    pub fn clear(&mut self) {
        self.text.clear();
    }

    /// Applies `key`; the caller inserts the clipboard text on [`DialogKey::Paste`].
    pub fn key(&mut self, key: DialogKey) -> Outcome {
        match key {
            DialogKey::Cancel => return Outcome::Cancel,
            DialogKey::Save => return Outcome::Submit,
            DialogKey::Enter => self.text.push('\n'),
            DialogKey::Backspace => {
                self.text.pop();
            }
            DialogKey::Insert(text) => self.insert(&text),
            DialogKey::SwitchField | DialogKey::Paste => {}
        }
        Outcome::Edited
    }

    /// Appends typed or pasted text, turning `\r\n` and `\r` line breaks into `\n`.
    pub fn insert(&mut self, text: &str) {
        self.text
            .push_str(&text.replace("\r\n", "\n").replace('\r', "\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_types_a_new_line_and_ctrl_enter_submits() {
        let mut text = TextBox::default();
        assert!(text.is_blank());
        for key in [
            DialogKey::Insert("fix".into()),
            DialogKey::Enter,
            DialogKey::Insert("the testz".into()),
            DialogKey::Backspace,
            DialogKey::Insert("s".into()),
            DialogKey::SwitchField,
            DialogKey::Paste,
        ] {
            assert_eq!(text.key(key), Outcome::Edited);
        }
        assert_eq!(text.text(), "fix\nthe tests");
        assert_eq!(text.key(DialogKey::Save), Outcome::Submit);
        assert_eq!(text.key(DialogKey::Cancel), Outcome::Cancel);
        assert_eq!(
            text.text(),
            "fix\nthe tests",
            "submit and cancel keep the text"
        );
        text.clear();
        assert_eq!(text.text(), "");
    }

    #[test]
    fn pasted_line_breaks_become_new_lines() {
        let mut text = TextBox::default();
        text.insert("one\r\ntwo\rthree\n");
        assert_eq!(text.text(), "one\ntwo\nthree\n");
        text.key(DialogKey::Backspace);
        text.key(DialogKey::Backspace);
        assert_eq!(text.text(), "one\ntwo\nthre");
        text.clear();
        text.insert(" \n\t");
        assert!(text.is_blank());
    }
}
