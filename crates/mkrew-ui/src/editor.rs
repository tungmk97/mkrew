//! The Files editor's text (SPEC §12A, ADR 0028): the raw lines of the opened file, a caret
//! with an optional selection, and undo / redo. Pure and tested headlessly; `files_view.rs`
//! keeps one per opened file, `main.rs` draws it and feeds it keys.

use std::borrow::Cow;
use std::ops::Range;

use gpui::Keystroke;

use crate::highlight::TAB;
use crate::text_box::class;

/// A place in the text: a line (0-based) and a byte offset into it, on a char boundary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

impl Pos {
    pub fn new(line: usize, col: usize) -> Pos {
        Pos { line, col }
    }
}

/// Where a caret key moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    WordLeft,
    WordRight,
    Up,
    Down,
    /// To the indent, then to the line start.
    Home,
    End,
    DocStart,
    DocEnd,
    PageUp,
    PageDown,
}

/// What a key press in the editor does (VS Code keys; not in the keymap, ADR 0028).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditKey {
    Move {
        motion: Motion,
        extend: bool,
    },
    Backspace {
        word: bool,
    },
    Delete {
        word: bool,
    },
    Enter,
    Tab,
    SelectAll,
    Undo,
    Redo,
    /// The shell does the clipboard: Ctrl+C, Ctrl+X, Ctrl+V.
    Copy,
    Cut,
    Paste,
    Insert(String),
}

/// One change: `removed` at `at` became `inserted` (`\n` line breaks).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Edit {
    at: Pos,
    removed: String,
    inserted: String,
}

/// One undo step: its edits in the order they were made, and the caret around them.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Step {
    /// Unique per editor: the saved state is the id of the step on top of the undo stack.
    id: u64,
    edits: Vec<Edit>,
    before: (Pos, Option<Pos>),
    after: Pos,
}

// Invariant behind the plain `self.lines[..]` indexing and slicing below: `lines` is never
// empty, and `cursor`, `anchor` and every stored `Pos` are clamped (`clamp`) to an existing
// line and a char boundary before they are used, so none of it can panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editor {
    /// Without line endings; never empty.
    lines: Vec<String>,
    /// Lines end in `\r\n` (the first line's ending decides).
    crlf: bool,
    final_newline: bool,
    cursor: Pos,
    /// The other end of the selection.
    anchor: Option<Pos>,
    /// The screen column ↑ / ↓ aim for.
    goal: Option<usize>,
    undo: Vec<Step>,
    redo: Vec<Step>,
    next_id: u64,
    /// The undo top that is on disk; `None` = the text it was opened with.
    saved: Option<u64>,
    /// The last edit was a typed char: the next one joins its undo step.
    typing: bool,
    /// Bumped by every text change (re-highlighting, find).
    version: u64,
    /// Index of the widest line in screen cells ([`Self::longest_line`]), found when the text
    /// changes instead of on every frame.
    longest: usize,
}

impl Editor {
    pub fn new(text: &str) -> Editor {
        let crlf = text
            .find('\n')
            .is_some_and(|end| text[..end].ends_with('\r'));
        let final_newline = text.ends_with('\n');
        let body = text.strip_suffix('\n').unwrap_or(text);
        // A `\r` ends a line only when a `\n` follows; at the very end of the text it is content.
        let last = body.matches('\n').count();
        let lines = (body.split('\n').enumerate())
            .map(|(i, line)| {
                let ended = i < last || final_newline;
                let line = if ended {
                    line.strip_suffix('\r').unwrap_or(line)
                } else {
                    line
                };
                line.to_owned()
            })
            .collect::<Vec<String>>();
        let longest = widest_line(&lines);
        Editor {
            lines,
            crlf,
            final_newline,
            cursor: Pos::default(),
            anchor: None,
            goal: None,
            undo: Vec::new(),
            redo: Vec::new(),
            next_id: 0,
            saved: None,
            typing: false,
            version: 0,
            longest,
        }
    }

    /// The text to save, with the file's line endings.
    pub fn text(&self) -> String {
        let eol = if self.crlf { "\r\n" } else { "\n" };
        let mut text = self.lines.join(eol);
        if self.final_newline {
            text.push_str(eol);
        }
        text
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn cursor(&self) -> Pos {
        self.cursor
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    /// Index of the widest line in screen cells (tabs and multibyte chars differ from the
    /// byte length), for the editor's horizontal scroll width.
    pub fn longest_line(&self) -> usize {
        self.longest
    }

    /// The selected range, start first; `None` when nothing is selected.
    pub fn selection(&self) -> Option<Range<Pos>> {
        let anchor = self.anchor.filter(|anchor| *anchor != self.cursor)?;
        Some(anchor.min(self.cursor)..anchor.max(self.cursor))
    }

    pub fn selected_text(&self) -> Option<String> {
        self.selection().map(|range| self.text_in(range))
    }

    /// Whether the text differs from the saved (or opened) one by its undo history.
    pub fn is_dirty(&self) -> bool {
        self.undo.last().map(|step| step.id) != self.saved
    }

    /// The state a save sends. Typing after it starts a new undo step, so that state stays the
    /// saved one even when more is typed while the save is in flight.
    pub fn save_point(&mut self) -> Option<u64> {
        self.typing = false;
        self.undo.last().map(|step| step.id)
    }

    /// The state from [`Self::save_point`] is on disk now.
    pub fn mark_saved(&mut self, point: Option<u64>) {
        self.saved = point;
    }

    /// Moves the caret (clamped into the text); `extend` keeps or starts a selection.
    pub fn set_cursor(&mut self, pos: Pos, extend: bool) {
        let pos = self.clamp(pos);
        self.anchor = extend.then(|| self.anchor.unwrap_or(self.cursor));
        self.cursor = pos;
        self.goal = None;
        self.typing = false;
    }

    pub fn select(&mut self, range: Range<Pos>) {
        self.set_cursor(range.start, false);
        self.set_cursor(range.end, true);
    }

    pub fn select_all(&mut self) {
        self.select(Pos::default()..self.end());
    }

    /// Puts `text` in place of `range` as one undo step; the caret goes after it.
    pub fn replace(&mut self, range: Range<Pos>, text: &str) {
        self.replace_all(vec![(range, text.to_owned())]);
    }

    /// Puts each text in place of its range as ONE undo step (Replace all). The ranges are
    /// sorted and do not overlap; the caret goes after the first replacement.
    pub fn replace_all(&mut self, edits: Vec<(Range<Pos>, String)>) {
        if edits.is_empty() {
            return;
        }
        let before = (self.cursor, self.anchor);
        let mut done = Vec::with_capacity(edits.len());
        let mut after = self.cursor;
        // Last first: the earlier ranges stay where they were.
        for (range, text) in edits.into_iter().rev() {
            debug_assert!(range.start <= range.end, "reversed range");
            let range = self.clamp(range.start)..self.clamp(range.end);
            let text = text.replace("\r\n", "\n").replace('\r', "\n");
            let (removed, end) = self.splice(range.clone(), &text);
            after = end;
            done.push(Edit {
                at: range.start,
                removed,
                inserted: text,
            });
        }
        self.cursor = after;
        self.anchor = None;
        self.goal = None;
        self.typing = false;
        self.push(Step {
            id: 0,
            edits: done,
            before,
            after,
        });
    }

    /// Pasted text: replaces the selection; one undo step.
    pub fn insert(&mut self, text: &str) {
        let range = self.selection().unwrap_or(self.cursor..self.cursor);
        self.replace(range, text);
    }

    /// Ctrl+X: removes the selection and returns it.
    pub fn cut(&mut self) -> Option<String> {
        let range = self.selection()?;
        let text = self.text_in(range.clone());
        self.replace(range, "");
        Some(text)
    }

    pub fn undo(&mut self) -> bool {
        let Some(step) = self.undo.pop() else {
            return false;
        };
        for edit in step.edits.iter().rev() {
            self.splice(edit.at..end_of(edit.at, &edit.inserted), &edit.removed);
        }
        (self.cursor, self.anchor) = step.before;
        self.goal = None;
        self.typing = false;
        self.redo.push(step);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(step) = self.redo.pop() else {
            return false;
        };
        for edit in &step.edits {
            self.splice(edit.at..end_of(edit.at, &edit.removed), &edit.inserted);
        }
        self.cursor = step.after;
        self.anchor = None;
        self.goal = None;
        self.typing = false;
        self.undo.push(step);
        true
    }

    /// Applies `key`. Copy, Cut and Paste belong to the shell and do nothing here. `page` is
    /// the number of lines on screen. Returns whether the text changed.
    pub fn key(&mut self, key: EditKey, page: usize) -> bool {
        let version = self.version;
        match key {
            EditKey::Move { motion, extend } => self.move_caret(motion, extend, page),
            EditKey::Backspace { word } => self.delete(word, false),
            EditKey::Delete { word } => self.delete(word, true),
            EditKey::Enter => {
                let line = &self.lines[self.cursor.line];
                let text = format!("\n{}", &line[..indent_len(line).min(self.cursor.col)]);
                self.insert(&text);
            }
            EditKey::Tab => {
                // ponytail: a tab-indented file gets a tab, other files 4 spaces; no Shift+Tab
                // outdent and no block indent. Add them if asked for.
                let tabs = self.lines.iter().any(|line| line.starts_with('\t'));
                self.insert(if tabs { "\t" } else { TAB });
            }
            EditKey::SelectAll => self.select_all(),
            EditKey::Undo => {
                self.undo();
            }
            EditKey::Redo => {
                self.redo();
            }
            EditKey::Insert(text) => self.type_text(&text),
            EditKey::Copy | EditKey::Cut | EditKey::Paste => {}
        }
        self.version != version
    }

    /// A typed char: like [`Self::insert`], but a run of them is one undo step.
    fn type_text(&mut self, text: &str) {
        let joins = self.typing
            && self.selection().is_none()
            && (self.undo.last())
                .is_some_and(|step| Some(step.id) != self.saved && step.after == self.cursor);
        if !joins {
            self.insert(text);
            self.typing = true;
            return;
        }
        let (_, after) = self.splice(self.cursor..self.cursor, text);
        self.cursor = after;
        self.redo.clear();
        if let Some(step) = self.undo.last_mut() {
            step.after = after;
            if let Some(edit) = step.edits.last_mut() {
                edit.inserted.push_str(text);
            }
        }
    }

    fn delete(&mut self, word: bool, forward: bool) {
        let range = self.selection().unwrap_or_else(|| {
            let to = match (word, forward) {
                (false, false) => self.left(self.cursor),
                (false, true) => self.right(self.cursor),
                (true, false) => self.word_left(self.cursor),
                (true, true) => self.word_right(self.cursor),
            };
            self.cursor.min(to)..self.cursor.max(to)
        });
        if range.start != range.end {
            self.replace(range, "");
        }
    }

    fn move_caret(&mut self, motion: Motion, extend: bool, page: usize) {
        let at = self.cursor;
        let last = self.lines.len() - 1;
        let goal = (self.goal).unwrap_or_else(|| display_col(&self.lines[at.line], at.col));
        let vertical = |line: usize| Pos::new(line, byte_for_display_col(&self.lines[line], goal));
        let to = match (motion, self.selection()) {
            (Motion::Left, Some(selection)) if !extend => selection.start,
            (Motion::Right, Some(selection)) if !extend => selection.end,
            (Motion::Left, _) => self.left(at),
            (Motion::Right, _) => self.right(at),
            (Motion::WordLeft, _) => self.word_left(at),
            (Motion::WordRight, _) => self.word_right(at),
            (Motion::Up, _) if at.line == 0 => Pos::default(),
            (Motion::Up, _) => vertical(at.line - 1),
            (Motion::Down, _) if at.line == last => self.end(),
            (Motion::Down, _) => vertical(at.line + 1),
            (Motion::PageUp, _) => vertical(at.line.saturating_sub(page)),
            (Motion::PageDown, _) => vertical((at.line + page).min(last)),
            (Motion::Home, _) => {
                let indent = indent_len(&self.lines[at.line]);
                Pos::new(at.line, if at.col == indent { 0 } else { indent })
            }
            (Motion::End, _) => Pos::new(at.line, self.lines[at.line].len()),
            (Motion::DocStart, _) => Pos::default(),
            (Motion::DocEnd, _) => self.end(),
        };
        self.set_cursor(to, extend);
        if matches!(
            motion,
            Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown
        ) {
            self.goal = Some(goal);
        }
    }

    fn end(&self) -> Pos {
        let line = self.lines.len() - 1;
        Pos::new(line, self.lines[line].len())
    }

    fn clamp(&self, pos: Pos) -> Pos {
        let line = pos.line.min(self.lines.len() - 1);
        let text = &self.lines[line];
        let mut col = pos.col.min(text.len());
        while !text.is_char_boundary(col) {
            col -= 1;
        }
        Pos::new(line, col)
    }

    fn left(&self, pos: Pos) -> Pos {
        match self.lines[pos.line][..pos.col].chars().next_back() {
            Some(c) => Pos::new(pos.line, pos.col - c.len_utf8()),
            None if pos.line > 0 => Pos::new(pos.line - 1, self.lines[pos.line - 1].len()),
            None => pos,
        }
    }

    fn right(&self, pos: Pos) -> Pos {
        match self.lines[pos.line][pos.col..].chars().next() {
            Some(c) => Pos::new(pos.line, pos.col + c.len_utf8()),
            None if pos.line + 1 < self.lines.len() => Pos::new(pos.line + 1, 0),
            None => pos,
        }
    }

    /// Ctrl+←: skips blanks, then one run of word chars or of punctuation (like VS Code). At
    /// a line start it goes to the end of the line above.
    fn word_left(&self, pos: Pos) -> Pos {
        if pos.col == 0 {
            return self.left(pos);
        }
        let before = self.lines[pos.line][..pos.col].trim_end_matches(char::is_whitespace);
        let Some(last) = before.chars().next_back() else {
            return Pos::new(pos.line, 0);
        };
        let kind = class(last);
        Pos::new(
            pos.line,
            before.trim_end_matches(|c| class(c) == kind).len(),
        )
    }

    /// Ctrl+→: the mirror of [`Self::word_left`].
    fn word_right(&self, pos: Pos) -> Pos {
        let line = &self.lines[pos.line];
        if pos.col == line.len() {
            return self.right(pos);
        }
        let after = line[pos.col..].trim_start_matches(char::is_whitespace);
        let Some(first) = after.chars().next() else {
            return Pos::new(pos.line, line.len());
        };
        let kind = class(first);
        let rest = after.trim_start_matches(|c| class(c) == kind);
        Pos::new(pos.line, line.len() - rest.len())
    }

    fn text_in(&self, range: Range<Pos>) -> String {
        let (start, end) = (range.start, range.end);
        if start.line == end.line {
            return self.lines[start.line][start.col..end.col].to_owned();
        }
        let mut text = self.lines[start.line][start.col..].to_owned();
        for line in &self.lines[start.line + 1..end.line] {
            text.push('\n');
            text.push_str(line);
        }
        text.push('\n');
        text.push_str(&self.lines[end.line][..end.col]);
        text
    }

    /// The only text change: `text` (with `\n` breaks) replaces `range`. Returns the removed
    /// text and the position where the new text ends.
    fn splice(&mut self, range: Range<Pos>, text: &str) -> (String, Pos) {
        let removed = self.text_in(range.clone());
        let (start, end) = (range.start, range.end);
        let tail = self.lines[end.line][end.col..].to_owned();
        let mut new: Vec<String> = text.split('\n').map(str::to_owned).collect();
        new[0].insert_str(0, &self.lines[start.line][..start.col]);
        let last = new.len() - 1;
        let after = Pos::new(start.line + last, new[last].len());
        new[last].push_str(&tail);
        self.lines.splice(start.line..=end.line, new);
        self.version += 1;
        // ponytail: one scan per text change (it was one per frame); update from the replaced
        // lines only if typing in a 2 MB file ever feels slow.
        self.longest = widest_line(&self.lines);
        (removed, after)
    }

    fn push(&mut self, mut step: Step) {
        step.id = self.next_id;
        self.next_id += 1;
        self.undo.push(step);
        self.redo.clear();
    }
}

/// Bytes of leading blanks of `line`.
fn indent_len(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

/// Where `text` ends when it starts at `at`.
fn end_of(at: Pos, text: &str) -> Pos {
    match text.rsplit_once('\n') {
        Some((head, last)) => Pos::new(at.line + head.matches('\n').count() + 1, last.len()),
        None => Pos::new(at.line, at.col + text.len()),
    }
}

/// `line` as drawn: tabs as [`TAB`] (like `highlight::Code`).
pub fn display_line(line: &str) -> Cow<'_, str> {
    if line.contains('\t') {
        Cow::Owned(line.replace('\t', TAB))
    } else {
        Cow::Borrowed(line)
    }
}

/// The screen column (chars from the line start) of byte `byte` of `line`.
// ponytail: one cell per char, so wide CJK glyphs misplace the caret. Measure the shaped line
// if that ever matters.
pub fn display_col(line: &str, byte: usize) -> usize {
    let before = line.get(..byte).unwrap_or(line);
    before
        .chars()
        .map(|c| if c == '\t' { TAB.len() } else { 1 })
        .sum()
}

/// Index of the widest of `lines` in screen cells ([`display_col`] of its end); ties go to the
/// last one, an empty slice to 0.
fn widest_line(lines: &[String]) -> usize {
    lines
        .iter()
        .enumerate()
        .max_by_key(|(_, line)| display_col(line, line.len()))
        .map_or(0, |(index, _)| index)
}

/// The byte offset in [`display_line`] of byte `byte` of `line` (find highlights).
pub fn display_byte(line: &str, byte: usize) -> usize {
    let before = line.get(..byte).unwrap_or(line);
    before.len() + before.matches('\t').count() * (TAB.len() - 1)
}

/// The byte of `line` nearest to screen column `col` (clicks, ↑ / ↓), clamped to its end.
pub fn byte_for_display_col(line: &str, col: usize) -> usize {
    let mut at = 0;
    for (byte, c) in line.char_indices() {
        let width = if c == '\t' { TAB.len() } else { 1 };
        if col < at + width {
            return if col - at <= width / 2 {
                byte
            } else {
                byte + c.len_utf8()
            };
        }
        at += width;
    }
    line.len()
}

/// The screen cells of line `index` (text `line`) inside `selection`. When the line break is
/// selected too, this includes one cell past the text.
pub fn selected_cells(line: &str, index: usize, selection: &Range<Pos>) -> Option<Range<usize>> {
    if index < selection.start.line || index > selection.end.line {
        return None;
    }
    let start = if index == selection.start.line {
        display_col(line, selection.start.col)
    } else {
        0
    };
    let end = if index == selection.end.line {
        display_col(line, selection.end.col)
    } else {
        display_col(line, line.len()) + 1
    };
    (start < end).then_some(start..end)
}

/// The editor meaning of `keystroke` (VS Code keys). `None` means the key is not the editor's:
/// window shortcuts like Ctrl+Shift+Z, Alt+arrows and Ctrl+S go on to the keymap.
pub fn edit_key(keystroke: &Keystroke) -> Option<EditKey> {
    let m = keystroke.modifiers;
    let ctrl = m.control || m.platform;
    let plain_ctrl = ctrl && !m.alt && !m.shift;
    let caret = |plain, word| {
        let motion = if ctrl { word } else { plain };
        (!m.alt).then_some(EditKey::Move {
            motion,
            extend: m.shift,
        })
    };
    match keystroke.key.as_str() {
        "left" => return caret(Motion::Left, Motion::WordLeft),
        "right" => return caret(Motion::Right, Motion::WordRight),
        "up" if !ctrl => return caret(Motion::Up, Motion::Up),
        "down" if !ctrl => return caret(Motion::Down, Motion::Down),
        "home" => return caret(Motion::Home, Motion::DocStart),
        "end" => return caret(Motion::End, Motion::DocEnd),
        "pageup" if !ctrl => return caret(Motion::PageUp, Motion::PageUp),
        "pagedown" if !ctrl => return caret(Motion::PageDown, Motion::PageDown),
        "backspace" if !m.alt => return Some(EditKey::Backspace { word: ctrl }),
        "delete" if !m.alt => return Some(EditKey::Delete { word: ctrl }),
        "enter" if !ctrl && !m.alt => return Some(EditKey::Enter),
        "tab" if !ctrl && !m.alt && !m.shift => return Some(EditKey::Tab),
        "a" if plain_ctrl => return Some(EditKey::SelectAll),
        "z" if plain_ctrl => return Some(EditKey::Undo),
        "y" if plain_ctrl => return Some(EditKey::Redo),
        "c" if plain_ctrl => return Some(EditKey::Copy),
        "x" if plain_ctrl => return Some(EditKey::Cut),
        "v" if plain_ctrl => return Some(EditKey::Paste),
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
        .map(|text| EditKey::Insert(text.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn type_chars(editor: &mut Editor, text: &str) {
        for c in text.chars() {
            editor.key(EditKey::Insert(c.to_string()), 10);
        }
    }

    fn press(editor: &mut Editor, motion: Motion, extend: bool) {
        editor.key(EditKey::Move { motion, extend }, 10);
    }

    #[test]
    fn text_round_trips_line_endings_tabs_and_empty_files() {
        for text in [
            "",
            "\n",
            "a",
            "a\n",
            "a\r\nb\r\n",
            "a\r\nb",
            "\tx\n\n",
            "x\r\r\n",
            "a\r",
            "\r",
            "a\r\nb\r",
            "việt\n",
        ] {
            assert_eq!(Editor::new(text).text(), text, "{text:?}");
        }
        // ponytail: mixed endings become the first line's.
        assert_eq!(Editor::new("a\r\nb\nc").text(), "a\r\nb\r\nc");
        assert_eq!(Editor::new("a\r\nb\n").lines(), ["a", "b"]);
    }

    #[test]
    fn typing_and_arrows_stay_on_char_boundaries() {
        let mut editor = Editor::new("việt");
        for _ in 0..3 {
            press(&mut editor, Motion::Right, false);
        }
        assert_eq!(editor.cursor(), Pos::new(0, 5), "past the 3-byte ệ");
        editor.key(EditKey::Backspace { word: false }, 10);
        assert_eq!(
            (editor.text().as_str(), editor.cursor()),
            ("vit", Pos::new(0, 2))
        );
        type_chars(&mut editor, "ệ");
        assert_eq!(editor.text(), "việt");
        // A click below the text or inside a char lands on a boundary.
        editor.set_cursor(Pos::new(5, 3), false);
        assert_eq!(editor.cursor(), Pos::new(0, 2));
        editor.set_cursor(Pos::new(0, 99), false);
        assert_eq!(editor.cursor(), Pos::new(0, 6));
        // ← / → and Backspace cross line breaks.
        let mut editor = Editor::new("ab\ncd");
        editor.set_cursor(Pos::new(1, 0), false);
        press(&mut editor, Motion::Left, false);
        assert_eq!(editor.cursor(), Pos::new(0, 2));
        press(&mut editor, Motion::Right, false);
        assert_eq!(editor.cursor(), Pos::new(1, 0));
        editor.key(EditKey::Backspace { word: false }, 10);
        assert_eq!(editor.text(), "abcd");
    }

    #[test]
    fn ctrl_arrows_and_ctrl_delete_go_by_words_and_punctuation() {
        let mut editor = Editor::new("hello world");
        press(&mut editor, Motion::End, false);
        editor.key(EditKey::Backspace { word: true }, 10);
        assert_eq!(
            editor.text(),
            "hello ",
            "like the terminal's Ctrl+Backspace"
        );
        let mut editor = Editor::new("foo.bar(baz)\nnext");
        press(&mut editor, Motion::End, false);
        let mut stops = vec![];
        for _ in 0..6 {
            press(&mut editor, Motion::WordLeft, false);
            stops.push(editor.cursor().col);
        }
        assert_eq!(stops, [11, 8, 7, 4, 3, 0]);
        press(&mut editor, Motion::WordRight, false);
        assert_eq!(editor.cursor(), Pos::new(0, 3));
        // At a line start, Ctrl+Backspace joins the line above.
        editor.set_cursor(Pos::new(1, 0), false);
        editor.key(EditKey::Backspace { word: true }, 10);
        assert_eq!(editor.text(), "foo.bar(baz)next");
        // Ctrl+Delete eats the blanks and the next word.
        let mut editor = Editor::new("a   bc d");
        editor.set_cursor(Pos::new(0, 1), false);
        editor.key(EditKey::Delete { word: true }, 10);
        assert_eq!(editor.text(), "a d");
    }

    #[test]
    fn undo_takes_back_a_typed_run_at_once_and_dirty_follows_the_saved_state() {
        let mut editor = Editor::new("x");
        assert!(!editor.is_dirty());
        type_chars(&mut editor, "abc");
        assert!(editor.is_dirty());
        press(&mut editor, Motion::End, false);
        type_chars(&mut editor, "!");
        assert_eq!(editor.text(), "abcx!");
        assert!(editor.undo());
        assert_eq!(editor.text(), "abcx");
        assert!(editor.undo());
        assert_eq!(
            (editor.text().as_str(), editor.cursor()),
            ("x", Pos::new(0, 0))
        );
        assert!(!editor.is_dirty(), "back to what was read");
        assert!(!editor.undo());
        assert!(editor.redo());
        assert_eq!(editor.text(), "abcx");
        // Saved here: undo makes it dirty, redo clean again; a new edit drops the redo.
        let point = editor.save_point();
        editor.mark_saved(point);
        assert!(!editor.is_dirty());
        editor.undo();
        assert!(editor.is_dirty());
        editor.redo();
        assert!(!editor.is_dirty());
        editor.undo();
        type_chars(&mut editor, "z");
        assert!(!editor.redo());
        assert!(editor.is_dirty());
    }

    #[test]
    fn typing_while_a_save_is_in_flight_stays_unsaved() {
        let mut editor = Editor::new("");
        type_chars(&mut editor, "ab");
        let point = editor.save_point();
        type_chars(&mut editor, "c");
        editor.mark_saved(point);
        assert!(editor.is_dirty(), "the c was not saved");
        editor.undo();
        assert_eq!(editor.text(), "ab");
        assert!(!editor.is_dirty());
    }

    #[test]
    fn a_selection_is_replaced_by_typing_and_paste_and_removed_by_cut() {
        let mut editor = Editor::new("one\ntwo\nthree");
        editor.set_cursor(Pos::new(0, 1), false);
        press(&mut editor, Motion::Down, true);
        press(&mut editor, Motion::Right, true);
        assert_eq!(editor.selected_text().as_deref(), Some("ne\ntw"));
        type_chars(&mut editor, "X");
        assert_eq!(editor.text(), "oXo\nthree");
        editor.undo();
        assert_eq!(editor.text(), "one\ntwo\nthree");
        assert_eq!(
            editor.selected_text().as_deref(),
            Some("ne\ntw"),
            "undo brings the selection back"
        );
        // ← collapses the selection to its start.
        press(&mut editor, Motion::Left, false);
        assert_eq!(
            (editor.cursor(), editor.selection()),
            (Pos::new(0, 1), None)
        );
        editor.key(EditKey::SelectAll, 10);
        assert_eq!(editor.cut().as_deref(), Some("one\ntwo\nthree"));
        assert_eq!(editor.text(), "");
        editor.insert("a\r\nb");
        assert_eq!(
            (editor.lines().len(), editor.cursor()),
            (2, Pos::new(1, 1)),
            "pasted CRLF splits lines"
        );
    }

    #[test]
    fn enter_keeps_the_indent_and_tab_follows_the_file() {
        let mut editor = Editor::new("    fn x() {");
        press(&mut editor, Motion::End, false);
        editor.key(EditKey::Enter, 10);
        assert_eq!(
            (editor.text().as_str(), editor.cursor()),
            ("    fn x() {\n    ", Pos::new(1, 4))
        );
        editor.key(EditKey::Tab, 10);
        assert_eq!(editor.lines()[1], "        ");
        let mut tabs = Editor::new("\tlet a = 1;\n");
        tabs.key(EditKey::Tab, 10);
        assert_eq!(tabs.lines()[0], "\t\tlet a = 1;");
        // Home: to the indent first, then to the line start.
        let mut editor = Editor::new("  x");
        press(&mut editor, Motion::End, false);
        press(&mut editor, Motion::Home, false);
        assert_eq!(editor.cursor(), Pos::new(0, 2));
        press(&mut editor, Motion::Home, false);
        assert_eq!(editor.cursor(), Pos::new(0, 0));
    }

    #[test]
    fn up_and_down_keep_the_column_on_screen_across_tabs() {
        assert_eq!(display_col("\tab", 2), 5);
        assert_eq!(display_byte("\tab", 2), 5);
        assert_eq!(display_line("\tab"), "    ab");
        assert_eq!(
            byte_for_display_col("\tab", 1),
            0,
            "inside the tab, nearer its start"
        );
        assert_eq!(byte_for_display_col("\tab", 3), 1);
        assert_eq!(byte_for_display_col("việt", 3), 5);
        assert_eq!(byte_for_display_col("ab", 9), 2);
        let selection = Pos::new(0, 1)..Pos::new(2, 1);
        assert_eq!(
            selected_cells("\tab", 0, &selection),
            Some(4..7),
            "with the line break"
        );
        assert_eq!(selected_cells("", 1, &selection), Some(0..1));
        assert_eq!(selected_cells("xy", 2, &selection), Some(0..1));
        assert_eq!(selected_cells("xy", 3, &selection), None);
        let mut editor = Editor::new("\tab\nx\nabcdefgh");
        press(&mut editor, Motion::End, false); // screen column 6
        press(&mut editor, Motion::Down, false);
        assert_eq!(editor.cursor(), Pos::new(1, 1));
        press(&mut editor, Motion::Down, false);
        assert_eq!(
            editor.cursor(),
            Pos::new(2, 6),
            "the column survives a short line"
        );
        press(&mut editor, Motion::PageUp, false);
        assert_eq!(editor.cursor(), Pos::new(0, 3));
    }

    #[test]
    fn replace_all_is_one_undo_step() {
        let mut editor = Editor::new("a-a\na");
        editor.replace_all(vec![
            (Pos::new(0, 0)..Pos::new(0, 1), "bb".into()),
            (Pos::new(0, 2)..Pos::new(0, 3), "bb".into()),
            (Pos::new(1, 0)..Pos::new(1, 1), "bb".into()),
        ]);
        assert_eq!(editor.text(), "bb-bb\nbb");
        assert_eq!(
            editor.cursor(),
            Pos::new(0, 2),
            "after the first replacement"
        );
        assert!(editor.undo());
        assert_eq!(editor.text(), "a-a\na");
        assert!(!editor.is_dirty());
        assert!(editor.redo());
        assert_eq!(editor.text(), "bb-bb\nbb");
    }

    #[test]
    fn keys_follow_vs_code_but_leave_the_app_shortcuts_alone() {
        let key = |keys: &str| edit_key(&Keystroke::parse(keys).unwrap());
        assert_eq!(
            key("ctrl-shift-left"),
            Some(EditKey::Move {
                motion: Motion::WordLeft,
                extend: true
            })
        );
        assert_eq!(
            key("ctrl-end"),
            Some(EditKey::Move {
                motion: Motion::DocEnd,
                extend: false
            })
        );
        assert_eq!(
            key("ctrl-backspace"),
            Some(EditKey::Backspace { word: true })
        );
        assert_eq!(key("ctrl-z"), Some(EditKey::Undo));
        assert_eq!(key("ctrl-y"), Some(EditKey::Redo));
        assert_eq!(key("ctrl-v"), Some(EditKey::Paste));
        // Window shortcuts go on to the keymap: Ctrl+Shift+Z is Open in Zed, Alt+arrows move
        // pane focus, Ctrl+S is Save.
        for keys in [
            "ctrl-shift-z",
            "alt-left",
            "ctrl-s",
            "ctrl-shift-tab",
            "ctrl-up",
        ] {
            assert_eq!(key(keys), None, "{keys}");
        }
        // AltGr arrives as Ctrl+Alt with the composed char: it is typed.
        let mut altgr = Keystroke::parse("ctrl-alt-q").unwrap();
        altgr.key_char = Some("@".into());
        assert_eq!(edit_key(&altgr), Some(EditKey::Insert("@".into())));
    }

    /// The widest row in screen cells (a tab is `TAB` cells, more than its one byte) follows
    /// every edit and undo, so the render never scans the file again.
    #[test]
    fn longest_line_is_the_widest_row_and_follows_edits() {
        let widest = |editor: &Editor| {
            let line = &editor.lines()[editor.longest_line()];
            display_col(line, line.len())
        };
        let tabs = 2 * TAB.len() + 1;
        let mut editor = Editor::new("short\n\t\tx\n12345678\n");
        assert_eq!(
            editor.longest_line(),
            1,
            "two tabs and x are wider than 8 bytes"
        );
        assert_eq!(widest(&editor), tabs);
        // A longer line typed at the end of the last row wins…
        editor.set_cursor(Pos::new(2, 8), false);
        editor.insert("9012345678");
        assert_eq!((editor.longest_line(), widest(&editor)), (2, 18));
        // …and undo gives the old widest row back.
        assert!(editor.undo());
        assert_eq!(widest(&editor), tabs);
        // Cutting the widest row's text leaves the next widest.
        editor.set_cursor(Pos::new(1, 0), false);
        editor.set_cursor(Pos::new(1, 3), true);
        editor.cut();
        assert_eq!((editor.longest_line(), widest(&editor)), (2, 8));
        // An empty file still has a line 0.
        assert_eq!(Editor::new("").longest_line(), 0);
    }
}
