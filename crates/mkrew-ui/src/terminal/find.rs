//! Find bar over the scrollback (SPEC §10, §13): which keys edit the query and which move
//! between matches. Pure, so it is unit-tested; the emulator does the regex search.

use gpui::Keystroke;

use crate::terminal::emulator::Emulator;
use crate::text_box::Input;

/// What a key press in the find bar does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindKey {
    /// Esc: close the bar, back to the terminal.
    Close,
    /// Enter: next match towards older output.
    Older,
    /// Shift+Enter: previous match (towards newer output).
    Newer,
    Backspace,
    /// Ctrl+Backspace (Cmd+Backspace on macOS): delete the word before the end.
    DeleteWord,
    /// Ctrl+A (Cmd+A): the next edit replaces the whole text.
    SelectAll,
    /// Ctrl+V (Cmd+V): the caller inserts the clipboard text.
    Paste,
    Insert(String),
}

impl FindKey {
    /// Applies an editing key to `input` (the terminal bar, the Files bars, the rename);
    /// every other key does nothing. `true` when the text changed.
    pub fn edit(&self, input: &mut Input) -> bool {
        match self {
            FindKey::Backspace => input.backspace(),
            FindKey::DeleteWord => input.delete_word(),
            FindKey::SelectAll => {
                input.select_all();
                false
            }
            _ => false,
        }
    }
}

/// The find-bar meaning of `keystroke`; `None` = ignored (it never reaches the PTY).
pub fn find_key(keystroke: &Keystroke) -> Option<FindKey> {
    let m = keystroke.modifiers;
    let command = m.platform || m.control;
    match keystroke.key.as_str() {
        "escape" => return Some(FindKey::Close),
        "enter" if m.shift => return Some(FindKey::Newer),
        "enter" => return Some(FindKey::Older),
        "backspace" if command && !m.alt => return Some(FindKey::DeleteWord),
        "backspace" => return Some(FindKey::Backspace),
        "v" if command && !m.alt && !m.shift => return Some(FindKey::Paste),
        "a" if command && !m.alt && !m.shift => return Some(FindKey::SelectAll),
        _ => {}
    }
    // AltGr arrives as Ctrl+Alt with the composed char; plain Ctrl+key types nothing.
    if m.platform || (m.control && !m.alt) {
        return None;
    }
    keystroke
        .key_char
        .as_deref()
        .filter(|text| !text.is_empty() && !text.chars().any(char::is_control))
        .map(|text| FindKey::Insert(text.to_owned()))
}

/// Result of the last query edit or move, shown next to the query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindStatus {
    /// Nothing typed yet.
    Empty,
    Found,
    NoMatch,
    /// The query is not a valid regex.
    Invalid,
}

impl FindStatus {
    pub fn label(self) -> &'static str {
        match self {
            FindStatus::Empty | FindStatus::Found => "",
            FindStatus::NoMatch => "No matches",
            FindStatus::Invalid => "Invalid pattern",
        }
    }
}

/// The open find bar of one pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindBar {
    query: Input,
    status: FindStatus,
}

impl Default for FindBar {
    fn default() -> Self {
        Self {
            query: Input::default(),
            status: FindStatus::Empty,
        }
    }
}

impl FindBar {
    pub fn query(&self) -> &str {
        self.query.as_str()
    }

    /// Whether Ctrl+A selected the whole query (drawn highlighted).
    pub fn selected(&self) -> bool {
        self.query.all_selected()
    }

    pub fn status(&self) -> FindStatus {
        self.status
    }

    /// Applies a key to the query and the emulator's search. Returns `false` for
    /// [`FindKey::Close`] (the caller closes the bar and clears the search).
    pub fn key(&mut self, key: FindKey, emulator: &mut Emulator) -> bool {
        match key {
            FindKey::Close => return false,
            FindKey::Older | FindKey::Newer => {
                if self.status != FindStatus::Invalid && !self.query.is_empty() {
                    self.status = moved(emulator.search_next(key == FindKey::Older));
                }
            }
            FindKey::Insert(text) => self.insert(&text, emulator),
            // The caller reads the clipboard and calls `insert`.
            FindKey::Paste => {}
            key => {
                if key.edit(&mut self.query) {
                    self.search(emulator);
                }
            }
        }
        true
    }

    /// Appends typed or pasted text (only its first line) and searches again.
    pub fn insert(&mut self, text: &str, emulator: &mut Emulator) {
        let line = text.lines().next().unwrap_or_default();
        if self.query.insert(line) {
            self.search(emulator);
        }
    }

    /// Re-applies the query to a replaced emulator (fresh daemon snapshot) without moving.
    pub fn reapply(&self, emulator: &mut Emulator) {
        emulator.set_search(&self.query);
    }

    /// Searches the edited query from the view's bottom edge towards older output (nearby
    /// only; Enter searches the whole history).
    fn search(&mut self, emulator: &mut Emulator) {
        self.status = if !emulator.set_search(&self.query) {
            FindStatus::Invalid
        } else if self.query.is_empty() {
            FindStatus::Empty
        } else {
            moved(emulator.search_nearby())
        };
    }
}

fn moved(found: bool) -> FindStatus {
    if found {
        FindStatus::Found
    } else {
        FindStatus::NoMatch
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::emulator::Found;

    fn key(spec: &str) -> Option<FindKey> {
        let mut keystroke = Keystroke::parse(spec).unwrap();
        if keystroke.key.chars().count() == 1 && !keystroke.modifiers.control {
            let c = if keystroke.modifiers.shift {
                keystroke.key.to_uppercase()
            } else {
                keystroke.key.clone()
            };
            keystroke.key_char = Some(c);
        }
        find_key(&keystroke)
    }

    #[test]
    fn keys_map_to_find_actions() {
        assert_eq!(key("escape"), Some(FindKey::Close));
        assert_eq!(key("enter"), Some(FindKey::Older));
        assert_eq!(key("shift-enter"), Some(FindKey::Newer));
        assert_eq!(key("backspace"), Some(FindKey::Backspace));
        assert_eq!(key("a"), Some(FindKey::Insert("a".into())));
        assert_eq!(key("shift-a"), Some(FindKey::Insert("A".into())));
        assert_eq!(key("ctrl-a"), Some(FindKey::SelectAll));
        assert_eq!(key("ctrl-v"), Some(FindKey::Paste));
        assert_eq!(key("ctrl-backspace"), Some(FindKey::DeleteWord));
        assert_eq!(key("alt-backspace"), Some(FindKey::Backspace));
        assert_eq!(key("ctrl-b"), None, "other Ctrl+letters still type nothing");
        assert_eq!(key("left"), None);
    }

    #[test]
    fn typing_searches_and_enter_moves() {
        let mut emu = Emulator::new(10, 2);
        for n in 0..6 {
            emu.advance(format!("item{n}\r\n").as_bytes());
        }
        let mut bar = FindBar::default();
        assert_eq!(bar.status(), FindStatus::Empty);
        for c in "item[13]".chars() {
            assert!(bar.key(FindKey::Insert(c.into()), &mut emu));
        }
        assert_eq!(bar.query(), "item[13]");
        assert_eq!(bar.status(), FindStatus::Found);
        assert_eq!(emu.grid().to_text().lines().next(), Some("item3"));
        assert_eq!(emu.grid().rows[0][0].found, Found::Current);
        bar.key(FindKey::Older, &mut emu);
        assert_eq!(emu.grid().to_text().lines().next(), Some("item1"));
        bar.key(FindKey::Newer, &mut emu);
        assert_eq!(emu.grid().to_text().lines().nth(1), Some("item3"));
        assert!(!bar.key(FindKey::Close, &mut emu));
    }

    #[test]
    fn invalid_and_missing_patterns_are_reported() {
        let mut emu = Emulator::new(10, 2);
        emu.advance(b"abc");
        let mut bar = FindBar::default();
        bar.insert("a(", &mut emu);
        assert_eq!(bar.status(), FindStatus::Invalid);
        assert_eq!(bar.status().label(), "Invalid pattern");
        bar.key(FindKey::Older, &mut emu);
        assert_eq!(bar.status(), FindStatus::Invalid);
        bar.key(FindKey::Backspace, &mut emu);
        assert_eq!(bar.status(), FindStatus::Found);
        bar.insert("x\nignored", &mut emu);
        assert_eq!(bar.query(), "ax");
        assert_eq!(bar.status().label(), "No matches");
        bar.key(FindKey::Backspace, &mut emu);
        bar.key(FindKey::Backspace, &mut emu);
        assert_eq!(bar.status(), FindStatus::Empty);
        // Backspace on an empty query changes nothing.
        bar.key(FindKey::Backspace, &mut emu);
        assert_eq!(bar.status(), FindStatus::Empty);
    }

    #[test]
    fn select_all_and_ctrl_backspace_edit_the_query_and_search_again() {
        let mut emu = Emulator::new(10, 2);
        emu.advance(b"foo bar");
        let mut bar = FindBar::default();
        bar.insert("foo bar", &mut emu);
        assert!(bar.key(FindKey::DeleteWord, &mut emu));
        assert_eq!(bar.query(), "foo ");
        assert_eq!(bar.status(), FindStatus::Found);

        bar.key(FindKey::SelectAll, &mut emu);
        assert!(bar.selected());
        bar.key(FindKey::Insert("bar".into()), &mut emu);
        assert_eq!((bar.query(), bar.selected()), ("bar", false));
        assert_eq!(bar.status(), FindStatus::Found);

        bar.key(FindKey::SelectAll, &mut emu);
        bar.key(FindKey::Backspace, &mut emu);
        assert_eq!(bar.query(), "");
        assert_eq!(bar.status(), FindStatus::Empty);
        // Paste is the caller's (it reads the clipboard), like in the dialogs.
        assert!(bar.key(FindKey::Paste, &mut emu));
        assert_eq!(bar.query(), "");
    }

    #[test]
    fn reapply_restores_highlights_on_a_new_emulator() {
        let mut bar = FindBar::default();
        let mut emu = Emulator::new(10, 2);
        bar.insert("abc", &mut emu);
        let mut fresh = Emulator::new(10, 2);
        fresh.advance(b"abc");
        bar.reapply(&mut fresh);
        assert_eq!(fresh.grid().rows[0][0].found, Found::Match);
    }
}
