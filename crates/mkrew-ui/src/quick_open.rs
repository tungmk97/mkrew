//! Quick open (Ctrl+Shift+O, SPEC §12A): a modal fuzzy finder over the active workspace's
//! files. Every edit of the query asks the daemon's `fs.find` (it walks with `ignore` and
//! ranks with `nucleo-matcher`); only the answer to the latest query is shown. Enter opens the
//! picked file in the Files view. Pure and tested headlessly; `main.rs` renders it.

use std::ops::Range;

use gpui::Keystroke;
use mkrew_protocol::{FsFindParams, FsMatch};
use ulid::Ulid;

use crate::task_dialog::{DialogKey, dialog_key};

/// What a key press in the finder means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    /// Esc.
    Close,
    /// ↑ / ↓ (-1 / +1).
    Move(i32),
    /// Enter (or Ctrl+Enter).
    Open,
    Backspace,
    /// Ctrl+V: the caller inserts the clipboard text.
    Paste,
    Insert(String),
}

/// The finder meaning of `keystroke`; `None` = ignored.
pub fn key(keystroke: &Keystroke) -> Option<Key> {
    if !keystroke.modifiers.modified() {
        match keystroke.key.as_str() {
            "up" => return Some(Key::Move(-1)),
            "down" => return Some(Key::Move(1)),
            _ => {}
        }
    }
    match dialog_key(keystroke)? {
        DialogKey::Cancel => Some(Key::Close),
        DialogKey::Enter | DialogKey::Save => Some(Key::Open),
        DialogKey::Backspace => Some(Key::Backspace),
        DialogKey::Paste => Some(Key::Paste),
        DialogKey::Insert(text) => Some(Key::Insert(text)),
        DialogKey::SwitchField => None,
    }
}

/// The matches of the latest query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Results {
    Loading,
    Loaded(Vec<FsMatch>),
    Failed(String),
}

/// The open finder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickOpen {
    pub project_id: Ulid,
    pub query: String,
    pub results: Results,
    /// Index of the picked match.
    pub selected: usize,
    /// Opened by `@` in this session's composer: the pick is typed there instead of opened.
    pub composer: Option<Ulid>,
    /// Number of the last `fs.find` sent; older answers are dropped.
    sent: u64,
}

impl QuickOpen {
    pub fn new(project_id: Ulid) -> Self {
        Self {
            project_id,
            query: String::new(),
            results: Results::Loading,
            selected: 0,
            composer: None,
            sent: 0,
        }
    }

    /// The `fs.find` for the query as it is now, with its number for [`Self::found`].
    pub fn search(&mut self) -> (u64, FsFindParams) {
        self.sent += 1;
        let params = FsFindParams {
            project_id: self.project_id,
            worktree_task_id: None,
            query: self.query.clone(),
            limit: FsFindParams::DEFAULT_LIMIT,
        };
        (self.sent, params)
    }

    /// The answer to search number `serial`; kept only if no newer search was sent (the
    /// previous matches stay on screen until then). The pick goes back to the best match.
    /// Returns `true` when the answer was kept.
    pub fn found(&mut self, serial: u64, found: Result<Vec<FsMatch>, String>) -> bool {
        if serial != self.sent {
            return false;
        }
        self.results = match found {
            Ok(matches) => Results::Loaded(matches),
            Err(error) => Results::Failed(error),
        };
        self.selected = 0;
        true
    }

    /// Applies a key. Returns `true` when the query changed (search again).
    pub fn key(&mut self, key: Key) -> bool {
        match key {
            Key::Move(by) => {
                self.step(by);
                false
            }
            Key::Backspace => self.query.pop().is_some(),
            Key::Insert(text) => self.insert(&text),
            Key::Close | Key::Open | Key::Paste => false,
        }
    }

    /// Appends typed or pasted text (line breaks dropped: a path is one line). Returns `true`
    /// when the query changed.
    pub fn insert(&mut self, text: &str) -> bool {
        let before = self.query.len();
        self.query
            .extend(text.chars().filter(|c| *c != '\r' && *c != '\n'));
        self.query.len() != before
    }

    /// Moves the pick by `by`, wrapping around the list.
    pub fn step(&mut self, by: i32) {
        let count = self.matches().len();
        if count == 0 {
            return;
        }
        let count = i64::try_from(count).unwrap_or(i64::MAX);
        let at = i64::try_from(self.selected).unwrap_or(0);
        self.selected = usize::try_from((at + i64::from(by)).rem_euclid(count)).unwrap_or(0);
    }

    /// Picks match `index` (a click).
    pub fn pick(&mut self, index: usize) {
        if index < self.matches().len() {
            self.selected = index;
        }
    }

    pub fn matches(&self) -> &[FsMatch] {
        match &self.results {
            Results::Loaded(matches) => matches,
            Results::Loading | Results::Failed(_) => &[],
        }
    }

    /// The picked file's path, to open.
    pub fn picked(&self) -> Option<&str> {
        self.matches()
            .get(self.selected)
            .map(|found| found.path.as_str())
    }
}

/// The byte ranges of `path` to highlight for the matched char `indices` (as `fs.find`
/// returns them), adjacent chars merged; indices past the end are ignored.
pub fn match_ranges(path: &str, indices: &[u32]) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for (index, (start, c)) in path.char_indices().enumerate() {
        let matched = u32::try_from(index).is_ok_and(|index| indices.contains(&index));
        if !matched {
            continue;
        }
        let end = start + c.len_utf8();
        match ranges.last_mut() {
            Some(last) if last.end == start => last.end = end,
            _ => ranges.push(start..end),
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(paths: &[&str]) -> Vec<FsMatch> {
        paths
            .iter()
            .map(|path| FsMatch {
                path: (*path).to_owned(),
                score: 1,
                indices: vec![],
            })
            .collect()
    }

    #[test]
    fn only_the_answer_to_the_latest_query_is_shown() {
        let project = Ulid::new();
        let mut open = QuickOpen::new(project);
        let (first, params) = open.search();
        assert_eq!(params.project_id, project);
        assert_eq!(params.query, "");
        assert_eq!(params.limit, FsFindParams::DEFAULT_LIMIT);

        assert!(open.key(Key::Insert("ma".into())));
        let (second, params) = open.search();
        assert_eq!(params.query, "ma");

        assert!(open.found(second, Ok(found(&["src/main.rs", "Makefile"]))));
        open.step(1);
        assert_eq!(open.picked(), Some("Makefile"));
        // The first query's answer arrives late: dropped.
        assert!(!open.found(first, Ok(found(&["README.md"]))));
        assert_eq!(open.matches().len(), 2);
        assert_eq!(open.picked(), Some("Makefile"));

        assert!(open.key(Key::Backspace));
        let (third, _) = open.search();
        open.found(third, Err("no such workspace".into()));
        assert_eq!(open.results, Results::Failed("no such workspace".into()));
        assert_eq!(open.picked(), None);
    }

    #[test]
    fn the_pick_wraps_and_resets_on_new_results() {
        let mut open = QuickOpen::new(Ulid::new());
        open.step(1);
        assert_eq!(open.selected, 0, "nothing to pick yet");
        let (serial, _) = open.search();
        open.found(serial, Ok(found(&["a", "b", "c"])));
        assert!(!open.key(Key::Move(-1)));
        assert_eq!(open.picked(), Some("c"), "up from the top wraps");
        open.step(1);
        assert_eq!(open.picked(), Some("a"), "down from the bottom wraps");
        open.pick(1);
        assert_eq!(open.picked(), Some("b"));
        open.pick(7);
        assert_eq!(open.picked(), Some("b"), "out of range ignored");

        let (serial, _) = open.search();
        open.found(serial, Ok(found(&["x", "y"])));
        assert_eq!(open.picked(), Some("x"));
    }

    #[test]
    fn editing_the_query() {
        let mut open = QuickOpen::new(Ulid::new());
        assert!(!open.key(Key::Backspace), "nothing to delete");
        assert!(open.insert("src/\r\nmain"));
        assert_eq!(open.query, "src/main");
        assert!(!open.insert("\n"), "only a line break");
        assert!(!open.key(Key::Open));
        assert!(!open.key(Key::Paste));
        assert!(!open.key(Key::Close));
        assert_eq!(open.query, "src/main");
    }

    #[test]
    fn keys() {
        let parse = |text: &str| key(&Keystroke::parse(text).unwrap());
        assert_eq!(parse("up"), Some(Key::Move(-1)));
        assert_eq!(parse("down"), Some(Key::Move(1)));
        assert_eq!(parse("escape"), Some(Key::Close));
        assert_eq!(parse("enter"), Some(Key::Open));
        assert_eq!(parse("ctrl-enter"), Some(Key::Open));
        assert_eq!(parse("backspace"), Some(Key::Backspace));
        assert_eq!(parse("ctrl-v"), Some(Key::Paste));
        assert_eq!(parse("tab"), None);
        assert_eq!(parse("ctrl-up"), None);
        assert_eq!(parse("ctrl-x"), None);
    }

    #[test]
    fn matched_chars_become_merged_byte_ranges() {
        assert_eq!(
            match_ranges("src/main.rs", &[4, 5, 6, 9]),
            vec![4..7, 9..10]
        );
        // Multi-byte chars: indices are chars, ranges are bytes.
        assert_eq!(match_ranges("é/ab", &[0, 1, 3]), vec![0..3, 4..5]);
        assert_eq!(match_ranges("ab", &[5]), Vec::<Range<usize>>::new());
    }
}
