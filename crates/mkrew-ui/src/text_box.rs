//! The text inputs (SPEC §20B.1, §13): [`Input`] is one line, [`TextBox`] many (Enter types a
//! new line, Ctrl+Enter submits, Esc cancels). Text is typed and deleted at the end, like the
//! other fields; Ctrl+A selects all (the next edit replaces it) and Ctrl+Backspace deletes
//! the word before the end. Pure and tested headlessly; keys come from
//! [`crate::task_dialog::dialog_key`], `main.rs` renders the text.

use std::ops::Deref;

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

/// Blanks, word chars, punctuation: Ctrl+Backspace (and the editor's Ctrl+arrows) work on one
/// run of the same class.
// ponytail: a decomposed (NFD) Vietnamese letter has a combining mark, which counts as
// punctuation, so Ctrl+Backspace stops at it; Windows IMEs type precomposed letters. Upgrade
// path: unicode-segmentation word boundaries.
pub fn class(c: char) -> u8 {
    if c.is_whitespace() {
        0
    } else if c.is_alphanumeric() || c == '_' {
        1
    } else {
        2
    }
}

/// Ctrl+Backspace: removes the trailing blanks, then the run of word chars (or of
/// punctuation) before them; a trailing line break goes alone. `true` when `text` changed.
pub fn delete_word(text: &mut String) -> bool {
    let before = text.len();
    while text
        .chars()
        .next_back()
        .is_some_and(|c| c != '\n' && c.is_whitespace())
    {
        text.pop();
    }
    match text.chars().next_back() {
        Some('\n') => {
            if text.len() == before {
                text.pop();
            }
        }
        Some(last) => {
            let kind = class(last);
            while text.chars().next_back().is_some_and(|c| class(c) == kind) {
                text.pop();
            }
        }
        None => {}
    }
    text.len() != before
}

/// One field's text. Typing and deleting happen at the end; Ctrl+A selects everything and the
/// next edit replaces it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Input {
    text: String,
    /// Ctrl+A was pressed: the next edit replaces the whole text.
    all_selected: bool,
}

impl Input {
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Whether the whole text is selected (drawn highlighted; the next edit replaces it).
    pub fn all_selected(&self) -> bool {
        self.all_selected
    }

    /// Ctrl+A: nothing to select in an empty field.
    pub fn select_all(&mut self) {
        self.all_selected = !self.text.is_empty();
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.all_selected = false;
    }

    /// Types `text` (already shaped for the field: one line, no control chars), replacing a
    /// select-all. `false`, and nothing changes, when `text` is empty.
    pub fn insert(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        if std::mem::take(&mut self.all_selected) {
            self.text.clear();
        }
        self.text.push_str(text);
        true
    }

    /// Backspace: the selection, else the last char. `true` when the text changed.
    pub fn backspace(&mut self) -> bool {
        if std::mem::take(&mut self.all_selected) {
            return !std::mem::take(&mut self.text).is_empty();
        }
        self.text.pop().is_some()
    }

    /// Ctrl+Backspace: the selection, else the last word. `true` when the text changed.
    pub fn delete_word(&mut self) -> bool {
        if std::mem::take(&mut self.all_selected) {
            return !std::mem::take(&mut self.text).is_empty();
        }
        delete_word(&mut self.text)
    }

    /// Applies Backspace, Ctrl+Backspace or Ctrl+A; every other key does nothing. `true` when
    /// the text changed (Ctrl+A only moves the highlight).
    pub fn edit(&mut self, key: &DialogKey) -> bool {
        match key {
            DialogKey::Backspace => self.backspace(),
            DialogKey::DeleteWord => self.delete_word(),
            DialogKey::SelectAll => {
                self.select_all();
                false
            }
            _ => false,
        }
    }
}

impl Deref for Input {
    type Target = str;

    fn deref(&self) -> &str {
        &self.text
    }
}

impl From<&str> for Input {
    fn from(text: &str) -> Self {
        text.to_owned().into()
    }
}

impl From<String> for Input {
    fn from(text: String) -> Self {
        Self {
            text,
            all_selected: false,
        }
    }
}

impl PartialEq<&str> for Input {
    fn eq(&self, other: &&str) -> bool {
        self.text == *other
    }
}

impl PartialEq<str> for Input {
    fn eq(&self, other: &str) -> bool {
        self.text == other
    }
}

/// The text of a multi-line box; line breaks are always `\n`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextBox {
    input: Input,
}

impl TextBox {
    pub fn text(&self) -> &str {
        self.input.as_str()
    }

    /// Whether nothing but blanks was typed.
    pub fn is_blank(&self) -> bool {
        self.input.trim().is_empty()
    }

    /// Whether the whole text is selected (drawn highlighted; the next edit replaces it).
    pub fn all_selected(&self) -> bool {
        self.input.all_selected()
    }

    pub fn clear(&mut self) {
        self.input.clear();
    }

    /// Applies `key`; the caller inserts the clipboard text on [`DialogKey::Paste`].
    pub fn key(&mut self, key: DialogKey) -> Outcome {
        match key {
            DialogKey::Cancel => return Outcome::Cancel,
            DialogKey::Save => return Outcome::Submit,
            DialogKey::Enter => self.insert("\n"),
            DialogKey::Insert(text) => self.insert(&text),
            key @ (DialogKey::Backspace | DialogKey::DeleteWord | DialogKey::SelectAll) => {
                self.input.edit(&key);
            }
            DialogKey::SwitchField | DialogKey::Paste => {}
        }
        Outcome::Edited
    }

    /// Appends typed or pasted text (replacing a select-all), turning `\r\n` and `\r` line
    /// breaks into `\n`.
    pub fn insert(&mut self, text: &str) {
        self.input
            .insert(&text.replace("\r\n", "\n").replace('\r', "\n"));
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

    #[test]
    fn select_all_makes_the_next_edit_replace_everything() {
        let mut text = TextBox::default();
        text.insert("one\ntwo");
        text.key(DialogKey::SelectAll);
        assert!(text.all_selected());
        text.key(DialogKey::Backspace);
        assert_eq!(text.text(), "", "Backspace removes everything");
        assert!(!text.all_selected());

        text.key(DialogKey::SelectAll);
        assert!(!text.all_selected(), "nothing to select in an empty box");

        text.insert("old");
        text.key(DialogKey::SelectAll);
        text.key(DialogKey::Enter);
        assert_eq!(
            text.text(),
            "\n",
            "Enter replaces the selection with a new line"
        );
    }

    #[test]
    fn delete_word_removes_the_last_word_like_a_text_field() {
        // (before, after)
        let cases = [
            ("hello world", "hello "),
            ("hello world   ", "hello "),
            ("hello", ""),
            ("snake_case x", "snake_case "),
            ("call(a, b)", "call(a, b"),
            ("foo(), ", "foo"),
            ("src/main.rs", "src/main."),
            ("việt nam", "việt "),
            ("việt ", ""),
            ("   ", ""),
            ("one\ntwo", "one\n"),
            ("one\n", "one"),
            ("one\n  ", "one\n"),
            ("", ""),
        ];
        for (before, after) in cases {
            let mut text = before.to_owned();
            assert_eq!(delete_word(&mut text), before != after, "{before:?}");
            assert_eq!(text, after, "{before:?}");
        }
    }

    #[test]
    fn select_all_then_typing_replaces_and_backspace_clears() {
        let mut input = Input::from("old name");
        input.select_all();
        assert!(input.all_selected());
        assert!(input.insert("new"));
        assert_eq!((input.as_str(), input.all_selected()), ("new", false));

        input.select_all();
        assert!(input.backspace());
        assert_eq!(input, "");
        input.select_all();
        assert!(!input.all_selected(), "nothing to select in an empty field");
        assert!(!input.backspace());
    }

    #[test]
    fn ctrl_backspace_deletes_a_word_or_the_selection() {
        let mut input = Input::from("fix the việt nam");
        assert!(input.delete_word());
        assert_eq!(input, "fix the việt ");
        input.select_all();
        assert!(input.delete_word());
        assert_eq!(input, "");
        assert!(!input.delete_word());
    }

    #[test]
    fn plain_backspace_erases_one_char_and_an_empty_insert_keeps_the_selection() {
        let mut input = Input::from("việt");
        assert!(input.backspace());
        assert_eq!(input, "việ");
        input.select_all();
        assert!(!input.insert(""), "typing nothing changes nothing");
        assert!(input.all_selected());
        assert_eq!(input, "việ");
    }

    #[test]
    fn a_text_box_deletes_words_line_by_line_and_clears_a_selection() {
        let mut text = TextBox::default();
        text.insert("fix the\nlogin bug");
        text.key(DialogKey::DeleteWord);
        assert_eq!(text.text(), "fix the\nlogin ");
        text.key(DialogKey::DeleteWord);
        assert_eq!(text.text(), "fix the\n");
        text.key(DialogKey::DeleteWord);
        assert_eq!(text.text(), "fix the", "the line break goes alone");
        text.key(DialogKey::SelectAll);
        text.key(DialogKey::DeleteWord);
        assert_eq!(text.text(), "");
    }
}
