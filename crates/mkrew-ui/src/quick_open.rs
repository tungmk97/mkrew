//! Quick open (Ctrl+Shift+O, SPEC §12A): a modal fuzzy finder over the active workspace's
//! files. Every edit of the query asks the daemon's `fs.find` (it walks with `ignore` and
//! ranks with `nucleo-matcher`); only the answer to the latest query is shown. Enter opens the
//! picked file in the Files view. Pure and tested headlessly; `main.rs` renders it.

use std::sync::atomic::{AtomicU64, Ordering};

use mkrew_protocol::{FsFindParams, FsMatch};
use ulid::Ulid;

use crate::composer::Target;
use crate::palette::Key;
use crate::text_box::Input;

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
    /// The task worktree the Files view shows (`None` = the project folder): searched and
    /// revealed there.
    pub worktree_task_id: Option<Ulid>,
    pub query: Input,
    pub results: Results,
    /// Index of the picked match.
    pub selected: usize,
    /// Opened by `@` in a prompt (a composer, the task dialog): the pick is typed there
    /// instead of opened.
    pub insert_into: Option<Target>,
    /// Number of the last `fs.find` sent; older answers are dropped. Unique across finders,
    /// so an answer meant for a closed one never lands in the next.
    sent: u64,
}

impl QuickOpen {
    pub fn new(project_id: Ulid, worktree_task_id: Option<Ulid>) -> Self {
        Self {
            project_id,
            worktree_task_id,
            query: Input::default(),
            results: Results::Loading,
            selected: 0,
            insert_into: None,
            sent: 0,
        }
    }

    /// The `fs.find` for the query as it is now, with its number for [`Self::found`].
    pub fn search(&mut self) -> (u64, FsFindParams) {
        static SENT: AtomicU64 = AtomicU64::new(0);
        self.sent = SENT.fetch_add(1, Ordering::Relaxed) + 1;
        let params = FsFindParams {
            project_id: self.project_id,
            worktree_task_id: self.worktree_task_id,
            query: self.query.as_str().to_owned(),
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
            Key::Edit(key) => self.query.edit(&key),
            Key::Insert(text) => self.insert(&text),
            Key::Close | Key::Open | Key::Paste => false,
        }
    }

    /// Appends typed or pasted text (line breaks dropped: a path is one line). Returns `true`
    /// when the query changed.
    pub fn insert(&mut self, text: &str) -> bool {
        let line: String = text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
        self.query.insert(&line)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_dialog::DialogKey;

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
        let mut open = QuickOpen::new(project, None);
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

        assert!(open.key(Key::Edit(DialogKey::Backspace)));
        let (third, _) = open.search();
        open.found(third, Err("no such workspace".into()));
        assert_eq!(open.results, Results::Failed("no such workspace".into()));
        assert_eq!(open.picked(), None);
    }

    /// An `@` picker closed and opened again: the old finder's late answer has another
    /// number, so it never shows in the new one.
    #[test]
    fn a_closed_finders_answer_never_lands_in_the_next() {
        let project = Ulid::new();
        let mut closed = QuickOpen::new(project, None);
        closed.search();
        let (late, _) = closed.search();
        let mut open = QuickOpen::new(project, None);
        open.search();
        let (current, _) = open.search();
        assert_ne!(late, current);
        assert!(!open.found(late, Ok(found(&["stale.rs"]))));
        assert_eq!(open.results, Results::Loading);
    }

    #[test]
    fn the_pick_wraps_and_resets_on_new_results() {
        let mut open = QuickOpen::new(Ulid::new(), None);
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
        let mut open = QuickOpen::new(Ulid::new(), None);
        assert!(
            !open.key(Key::Edit(DialogKey::Backspace)),
            "nothing to delete"
        );
        assert!(open.insert("src/\r\nmain"));
        assert_eq!(open.query, "src/main");
        assert!(!open.insert("\n"), "only a line break");
        assert!(!open.key(Key::Open));
        assert!(!open.key(Key::Paste));
        assert!(!open.key(Key::Close));
        assert_eq!(open.query, "src/main");
    }

    #[test]
    fn select_all_and_ctrl_backspace_edit_the_query() {
        let mut open = QuickOpen::new(Ulid::new(), None);
        open.insert("src/main.rs");
        assert!(
            open.key(Key::Edit(DialogKey::DeleteWord)),
            "the query changed: search again"
        );
        assert_eq!(open.query, "src/main.");
        assert!(
            !open.key(Key::Edit(DialogKey::SelectAll)),
            "only the highlight changes"
        );
        assert!(open.query.all_selected());
        assert!(open.insert("lib"));
        assert_eq!(open.query, "lib", "typing replaces the selection");
        open.key(Key::Edit(DialogKey::SelectAll));
        assert!(open.key(Key::Edit(DialogKey::Backspace)));
        assert_eq!(open.query, "");
        assert!(
            open.key(Key::Edit(DialogKey::Undo)),
            "Ctrl+Z brings it back"
        );
        assert_eq!(open.query, "lib");
    }
}
