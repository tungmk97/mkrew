//! The text inputs (SPEC §20B.1, §13): [`Input`] is one line, [`TextBox`] many (Enter types a
//! new line, Ctrl+Enter submits, Esc cancels). Text is typed and deleted at the caret, which
//! stays at the end unless the field forwards caret keys ([`Input::edit`]: arrows, Home/End,
//! Delete, Shift to select); Ctrl+A selects all (the next edit replaces it) and Ctrl+Backspace
//! deletes the word before the caret. Pure and tested headlessly; keys come from
//! [`crate::task_dialog::dialog_key`], `main.rs` renders the text.

use std::ops::{Deref, Range};

use crate::editor::Motion;
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

/// One field's text, with a caret and an optional selection (Ctrl+A, Shift+arrows). The next
/// edit replaces the selection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Input {
    text: String,
    /// Byte offset of the caret, always on a char boundary.
    cursor: usize,
    /// The other end of the selection; the caret is one end.
    anchor: Option<usize>,
}

impl Input {
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Byte offset of the caret.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The selected bytes; `None` when nothing is selected.
    pub fn selection(&self) -> Option<Range<usize>> {
        let anchor = self.anchor?;
        (anchor != self.cursor).then(|| anchor.min(self.cursor)..anchor.max(self.cursor))
    }

    pub fn selected_text(&self) -> Option<&str> {
        self.selection().map(|range| &self.text[range])
    }

    /// Whether the whole text is selected (drawn highlighted; the next edit replaces it).
    pub fn all_selected(&self) -> bool {
        !self.text.is_empty() && self.selection() == Some(0..self.text.len())
    }

    /// Ctrl+A: nothing to select in an empty field.
    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.cursor = self.text.len();
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.anchor = None;
    }

    /// Removes the selection (the caret goes to its start). `true` when there was one.
    fn take_selection(&mut self) -> bool {
        let selection = self.selection();
        self.anchor = None;
        let Some(range) = selection else {
            return false;
        };
        self.cursor = range.start;
        self.text.replace_range(range, "");
        true
    }

    /// Types `text` (already shaped for the field: one line, no control chars) at the caret,
    /// replacing the selection. `false`, and nothing changes, when `text` is empty.
    pub fn insert(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        self.take_selection();
        self.text.insert_str(self.cursor, text);
        self.cursor += text.len();
        true
    }

    /// Backspace: the selection, else the char before the caret. `true` when the text changed.
    pub fn backspace(&mut self) -> bool {
        if self.take_selection() {
            return true;
        }
        let Some(c) = self.text[..self.cursor].chars().next_back() else {
            return false;
        };
        self.cursor -= c.len_utf8();
        self.text.remove(self.cursor);
        true
    }

    /// Delete: the selection, else the char after the caret. `true` when the text changed.
    pub fn delete(&mut self) -> bool {
        if self.take_selection() {
            return true;
        }
        if self.cursor == self.text.len() {
            return false;
        }
        self.text.remove(self.cursor);
        true
    }

    /// Ctrl+Backspace: the selection, else the word before the caret. `true` when the text
    /// changed.
    pub fn delete_word(&mut self) -> bool {
        if self.take_selection() {
            return true;
        }
        let mut before = self.text[..self.cursor].to_owned();
        if !delete_word(&mut before) {
            return false;
        }
        self.text.replace_range(before.len()..self.cursor, "");
        self.cursor = before.len();
        true
    }

    /// Ctrl+Delete: the selection, else the blanks and the word after the caret.
    pub fn delete_word_after(&mut self) -> bool {
        if self.take_selection() {
            return true;
        }
        let end = self.target(Motion::WordRight);
        self.text.replace_range(self.cursor..end, "");
        end != self.cursor
    }

    /// Moves the caret; `extend` (Shift) keeps or starts a selection. Without it, ← / → on a
    /// selection go to its start / end.
    pub fn move_caret(&mut self, motion: Motion, extend: bool) {
        let to = match (motion, self.selection()) {
            (Motion::Left, Some(range)) if !extend => range.start,
            (Motion::Right, Some(range)) if !extend => range.end,
            _ => self.target(motion),
        };
        self.anchor = extend.then(|| self.anchor.unwrap_or(self.cursor));
        self.cursor = to;
    }

    /// A press (or drag, `extend`) at byte `at`: one click puts the caret there, `extend`
    /// (Shift, a drag) selects from the anchor; two select the word (or run of blanks or
    /// punctuation) under it, three its line.
    pub fn click(&mut self, at: usize, extend: bool, clicks: usize) {
        let mut at = at.min(self.text.len());
        while !self.text.is_char_boundary(at) {
            at -= 1;
        }
        let text = self.text.as_str();
        let (start, end) = match clicks {
            0 | 1 => {
                self.anchor = extend.then(|| self.anchor.unwrap_or(self.cursor));
                self.cursor = at;
                return;
            }
            2 => {
                let Some(kind) = (text[at..].chars().next())
                    .filter(|c| *c != '\n')
                    .or_else(|| text[..at].chars().next_back())
                    .map(class)
                else {
                    return;
                };
                let same = |c: char| c != '\n' && class(c) == kind;
                (
                    text[..at].trim_end_matches(same).len(),
                    text.len() - text[at..].trim_start_matches(same).len(),
                )
            }
            _ => (
                text[..at].rfind('\n').map_or(0, |i| i + 1),
                text[at..].find('\n').map_or(text.len(), |i| at + i),
            ),
        };
        self.anchor = Some(start);
        self.cursor = end;
    }

    /// Where `motion` takes the caret. Home / End stay on the caret's line; ↑ / ↓ keep its
    /// column (in chars) and stop at the text's start / end.
    fn target(&self, motion: Motion) -> usize {
        let (text, at) = (self.text.as_str(), self.cursor);
        let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
        let line_end = text[at..].find('\n').map_or(text.len(), |i| at + i);
        match motion {
            Motion::Left => text[..at]
                .chars()
                .next_back()
                .map_or(at, |c| at - c.len_utf8()),
            Motion::Right => text[at..].chars().next().map_or(at, |c| at + c.len_utf8()),
            Motion::WordLeft => {
                let before = text[..at].trim_end_matches(char::is_whitespace);
                before.chars().next_back().map_or(0, |last| {
                    let kind = class(last);
                    before.trim_end_matches(|c| class(c) == kind).len()
                })
            }
            Motion::WordRight => {
                let after = text[at..].trim_start_matches(char::is_whitespace);
                after.chars().next().map_or(text.len(), |first| {
                    let kind = class(first);
                    text.len() - after.trim_start_matches(|c| class(c) == kind).len()
                })
            }
            Motion::Home => line_start,
            Motion::End => line_end,
            Motion::DocStart | Motion::PageUp => 0,
            Motion::DocEnd | Motion::PageDown => text.len(),
            Motion::Up | Motion::Down => {
                let column = text[line_start..at].chars().count();
                let line = if motion == Motion::Up {
                    if line_start == 0 {
                        return 0;
                    }
                    let end = line_start - 1;
                    text[..end].rfind('\n').map_or(0, |i| i + 1)..end
                } else {
                    if line_end == text.len() {
                        return text.len();
                    }
                    let start = line_end + 1;
                    start..text[start..].find('\n').map_or(text.len(), |i| start + i)
                };
                let offset: usize = text[line.clone()]
                    .chars()
                    .take(column)
                    .map(char::len_utf8)
                    .sum();
                line.start + offset
            }
        }
    }

    /// Applies an editing or caret key (Backspace, Delete, their word forms, Ctrl+A, arrows,
    /// Home/End, Cut after the caller copied); every other key does nothing. `true` when the
    /// text changed.
    pub fn edit(&mut self, key: &DialogKey) -> bool {
        match key {
            DialogKey::Backspace => self.backspace(),
            DialogKey::DeleteWord => self.delete_word(),
            DialogKey::Delete { word: false } => self.delete(),
            DialogKey::Delete { word: true } => self.delete_word_after(),
            DialogKey::Cut => self.take_selection(),
            DialogKey::SelectAll => {
                self.select_all();
                false
            }
            DialogKey::Move { motion, extend } => {
                self.move_caret(*motion, *extend);
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
            cursor: text.len(),
            text,
            anchor: None,
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

    /// The caret and selection (see [`Input`]).
    pub fn input(&self) -> &Input {
        &self.input
    }

    pub fn clear(&mut self) {
        self.input.clear();
    }

    /// Applies `key`; the caller inserts the clipboard text on [`DialogKey::Paste`]. Caret
    /// keys are ignored here (the caret stays at the end); fields that show the caret use
    /// [`Self::edit`].
    pub fn key(&mut self, key: DialogKey) -> Outcome {
        match key {
            DialogKey::Cancel => return Outcome::Cancel,
            DialogKey::Save => return Outcome::Submit,
            DialogKey::Enter => self.insert("\n"),
            DialogKey::Insert(text) => self.insert(&text),
            key @ (DialogKey::Backspace | DialogKey::DeleteWord | DialogKey::SelectAll) => {
                self.input.edit(&key);
            }
            DialogKey::SwitchField
            | DialogKey::Paste
            | DialogKey::Copy
            | DialogKey::Cut
            | DialogKey::Delete { .. }
            | DialogKey::Move { .. } => {}
        }
        Outcome::Edited
    }

    /// [`Input::edit`] on the box: ↑ / ↓ move between lines.
    pub fn edit(&mut self, key: &DialogKey) -> bool {
        self.input.edit(key)
    }

    /// [`Input::click`] on the box.
    pub fn click(&mut self, at: usize, extend: bool, clicks: usize) {
        self.input.click(at, extend, clicks);
    }

    /// Types typed or pasted text at the caret (replacing the selection), turning `\r\n` and
    /// `\r` line breaks into `\n`.
    pub fn insert(&mut self, text: &str) {
        self.input
            .insert(&text.replace("\r\n", "\n").replace('\r', "\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clicks_place_the_caret_and_select_words_and_lines() {
        let mut input = Input::from("npm run dev");
        input.click(4, false, 1);
        assert_eq!((input.cursor(), input.selection()), (4, None));
        input.insert("x");
        assert_eq!(
            input.as_str(),
            "npm xrun dev",
            "typing goes to the clicked place"
        );
        input.click(99, true, 1);
        assert_eq!(
            input.selection(),
            Some(5..12),
            "a drag extends from the anchor"
        );
        input.click(6, false, 2);
        assert_eq!(input.selected_text(), Some("xrun"));
        input.click(input.len(), false, 2);
        assert_eq!(
            input.selected_text(),
            Some("dev"),
            "at the end: the word before"
        );
        let mut text = TextBox::default();
        text.insert("A=1\nPORT=8080");
        text.click(6, false, 3);
        assert_eq!(text.input().selected_text(), Some("PORT=8080"));
        text.click(3, false, 2);
        assert_eq!(
            text.input().selected_text(),
            Some("1"),
            "a word stops at the line end"
        );
        let mut wide = Input::from("ví dụ");
        wide.click(2, false, 1);
        assert_eq!(wide.cursor(), 1, "a byte inside a char goes to its start");
    }

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
