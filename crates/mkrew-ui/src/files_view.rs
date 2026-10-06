//! Files view-model (SPEC §12A): a lazily listed tree of the workspace (`fs.list` per folder,
//! with git markers and an optional "show ignored"), the tree cursor moved by arrow keys, and
//! the opened file (`fs.read`, highlighted off the UI thread into a [`Code`]) with a selected
//! line. `git.changed` re-lists the open folders and re-reads the shown file. Pure and tested
//! headlessly; `main.rs` does the requests and renders it.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui::Keystroke;
use mkrew_protocol::{Event, FsEntry};
use ulid::Ulid;

use crate::highlight::Code;

/// One folder's children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listing {
    Loading,
    Loaded(Vec<FsEntry>),
    Failed(String),
}

/// The opened file.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    /// No file opened.
    None,
    Loading,
    Shown(Rc<Code>),
    Failed(String),
}

/// One visible tree row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    Entry {
        entry: FsEntry,
        depth: usize,
        expanded: bool,
    },
    /// An open folder whose listing is loading or failed (the text says which).
    Note { depth: usize, text: String },
}

/// Tree keys (SPEC §13: ← → ↑ ↓; Enter opens like a click).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
}

/// What a change asks of the shell: folders to `fs.list`, a file to `fs.read`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Requests {
    pub list: Vec<String>,
    pub read: Option<String>,
}

/// One workspace's Files view.
#[derive(Debug)]
pub struct FilesView {
    pub project_id: Ulid,
    pub show_ignored: bool,
    /// Listings by folder path (`""` = the root).
    dirs: HashMap<String, Listing>,
    expanded: HashSet<String>,
    /// The tree cursor (a file or folder path).
    pub cursor: Option<String>,
    /// The opened file's path.
    pub file: Option<String>,
    pub content: Content,
    /// The selected line of the opened file (1-based), for "Open in Zed" at that line.
    pub line: Option<u32>,
    /// Why the last "Open in Zed" failed.
    pub zed_error: Option<String>,
    /// Folders with an `fs.list` in flight, and those asked again meanwhile.
    listing: HashSet<String>,
    list_again: HashSet<String>,
    /// Bumped by every refresh and ignored-toggle: a listing sent before it is stale.
    generation: u64,
    /// The generation each in-flight `fs.list` was sent in.
    sent_in: HashMap<String, u64>,
    /// An `fs.read` is in flight, and another was asked for meanwhile (sent once it answers,
    /// so a burst of changes reads and highlights the file once more, not once per change).
    reading: bool,
    read_again: bool,
}

impl FilesView {
    pub fn new(project_id: Ulid) -> FilesView {
        FilesView {
            project_id,
            show_ignored: false,
            dirs: HashMap::new(),
            expanded: HashSet::new(),
            cursor: None,
            file: None,
            content: Content::None,
            line: None,
            zed_error: None,
            listing: HashSet::new(),
            list_again: HashSet::new(),
            generation: 0,
            sent_in: HashMap::new(),
            reading: false,
            read_again: false,
        }
    }

    /// The root listing.
    pub fn root(&self) -> &Listing {
        self.dirs.get("").unwrap_or(&Listing::Loading)
    }

    /// Whether the root was ever asked for (the view loads it when first shown).
    pub fn started(&self) -> bool {
        self.dirs.contains_key("")
    }

    /// An `fs.list` of `dir` is about to be sent; `false` = one is in flight already (it is
    /// sent again once that one answers).
    pub fn begin_list(&mut self, dir: &str) -> bool {
        self.dirs.entry(dir.to_owned()).or_insert(Listing::Loading);
        if !self.listing.insert(dir.to_owned()) {
            self.list_again.insert(dir.to_owned());
            return false;
        }
        self.sent_in.insert(dir.to_owned(), self.generation);
        true
    }

    /// The `fs.list` answer of `dir`; returns the folders to list now: `dir` again if asked
    /// meanwhile, and open subfolders not listed yet (after a toggle only the root is asked
    /// for). A failed re-list of a folder that was listed before keeps the old children; an
    /// answer sent before the last refresh / toggle is dropped (an open folder is re-listed).
    pub fn listed(&mut self, dir: &str, listed: Result<Vec<FsEntry>, String>) -> Vec<String> {
        self.listing.remove(dir);
        let again = self.list_again.remove(dir);
        let stale = self
            .sent_in
            .remove(dir)
            .is_some_and(|sent| sent < self.generation);
        let open = self.open_dirs().iter().any(|open| open == dir);
        if stale {
            if again || open {
                return vec![dir.to_owned()];
            }
            self.dirs.remove(dir);
            return vec![];
        }
        let mut list: Vec<String> = again.then(|| dir.to_owned()).into_iter().collect();
        let listing = match listed {
            Ok(entries) => {
                list.extend(
                    entries
                        .iter()
                        .filter(|entry| {
                            entry.is_dir
                                && self.expanded.contains(&entry.path)
                                && !self.dirs.contains_key(&entry.path)
                        })
                        .map(|entry| entry.path.clone()),
                );
                Listing::Loaded(entries)
            }
            Err(error) => match self.dirs.get(dir) {
                Some(Listing::Loaded(_)) if again => return list,
                _ => Listing::Failed(error),
            },
        };
        self.dirs.insert(dir.to_owned(), listing);
        list
    }

    /// The visible rows, top to bottom: folders first as the daemon sorts them, open folders
    /// followed by their children.
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        if let Listing::Loaded(entries) = self.root() {
            self.push_rows(entries, 0, &mut rows);
        }
        rows
    }

    fn push_rows(&self, entries: &[FsEntry], depth: usize, rows: &mut Vec<Row>) {
        for entry in entries {
            let expanded = entry.is_dir && self.expanded.contains(&entry.path);
            rows.push(Row::Entry {
                entry: entry.clone(),
                depth,
                expanded,
            });
            if !expanded {
                continue;
            }
            match self.dirs.get(&entry.path) {
                Some(Listing::Loaded(children)) => self.push_rows(children, depth + 1, rows),
                Some(Listing::Failed(error)) => rows.push(Row::Note {
                    depth: depth + 1,
                    text: format!("Cannot list this folder: {error}"),
                }),
                Some(Listing::Loading) | None => rows.push(Row::Note {
                    depth: depth + 1,
                    text: "Loading…".into(),
                }),
            }
        }
    }

    fn entry(&self, path: &str) -> Option<FsEntry> {
        self.rows().into_iter().find_map(|row| match row {
            Row::Entry { entry, .. } if entry.path == path => Some(entry),
            _ => None,
        })
    }

    /// A click on a row: the cursor moves there; a folder opens or closes, a file opens.
    pub fn click(&mut self, path: &str) -> Requests {
        self.cursor = Some(path.to_owned());
        let Some(entry) = self.entry(path) else {
            return Requests::default();
        };
        if entry.is_dir {
            return self.toggle(&entry.path);
        }
        Requests {
            list: vec![],
            read: self.open(&entry.path),
        }
    }

    /// Opens or closes folder `dir`; opening lists it unless already listed.
    pub fn toggle(&mut self, dir: &str) -> Requests {
        if self.expanded.remove(dir) {
            return Requests::default();
        }
        self.expand(dir)
    }

    fn expand(&mut self, dir: &str) -> Requests {
        self.expanded.insert(dir.to_owned());
        let list = match self.dirs.get(dir) {
            Some(Listing::Loaded(_) | Listing::Loading) => vec![],
            Some(Listing::Failed(_)) | None => vec![dir.to_owned()],
        };
        Requests { list, read: None }
    }

    /// Shows `path` in the viewer. Returns it when it has to be read: another file, or the
    /// same one whose read failed (a retry).
    pub fn open(&mut self, path: &str) -> Option<String> {
        let same = self.file.as_deref() == Some(path);
        if same && !matches!(self.content, Content::Failed(_)) {
            return None;
        }
        self.file = Some(path.to_owned());
        self.content = Content::Loading;
        self.line = None;
        Some(path.to_owned())
    }

    /// An `fs.read` of the opened file is about to be sent; `false` = one is in flight already
    /// (the opened file is read again once that one answers).
    pub fn begin_read(&mut self) -> bool {
        if self.reading {
            self.read_again = true;
            return false;
        }
        self.reading = true;
        true
    }

    /// The `fs.read` answer (already highlighted) for `path`; shown if `path` is still the
    /// opened file. Returns the file to read again (asked for meanwhile).
    pub fn read(&mut self, path: &str, read: Result<Code, String>) -> Option<String> {
        self.reading = false;
        let again = std::mem::take(&mut self.read_again);
        if self.file.as_deref() == Some(path) {
            self.show(read);
        }
        if again { self.file.clone() } else { None }
    }

    fn show(&mut self, read: Result<Code, String>) {
        self.content = match read {
            Ok(code) => {
                if self.line.is_some_and(|line| {
                    usize::try_from(line).unwrap_or(usize::MAX) > code.lines.len()
                }) {
                    self.line = None;
                }
                Content::Shown(Rc::new(code))
            }
            Err(error) => Content::Failed(error),
        };
    }

    /// Selects line `line` of the opened file (a second click on it clears the selection).
    pub fn pick_line(&mut self, line: u32) {
        self.line = (self.line != Some(line)).then_some(line);
    }

    /// Moves the cursor or opens/closes folders.
    pub fn key(&mut self, key: Key) -> Requests {
        let rows: Vec<(FsEntry, usize, bool)> = self
            .rows()
            .into_iter()
            .filter_map(|row| match row {
                Row::Entry {
                    entry,
                    depth,
                    expanded,
                } => Some((entry, depth, expanded)),
                Row::Note { .. } => None,
            })
            .collect();
        let Some(first) = rows.first() else {
            return Requests::default();
        };
        let at = self
            .cursor
            .as_deref()
            .and_then(|cursor| rows.iter().position(|(entry, ..)| entry.path == cursor));
        let Some(at) = at else {
            // No cursor yet (or its row is gone): any key lands on the first row.
            self.cursor = Some(first.0.path.clone());
            return Requests::default();
        };
        let (entry, depth, expanded) = rows[at].clone();
        let mut moved = |to: usize| self.cursor = Some(rows[to].0.path.clone());
        match key {
            Key::Up => moved(at.saturating_sub(1)),
            Key::Down => moved((at + 1).min(rows.len() - 1)),
            Key::Left if expanded => return self.toggle(&entry.path),
            Key::Left => {
                // To the parent folder: the nearest row above that is one level up.
                if let Some(parent) = rows[..at].iter().rposition(|(_, d, _)| *d + 1 == depth) {
                    moved(parent);
                }
            }
            Key::Right if entry.is_dir && !expanded => return self.expand(&entry.path),
            Key::Right if entry.is_dir => {
                if rows.get(at + 1).is_some_and(|(_, d, _)| *d > depth) {
                    moved(at + 1);
                }
            }
            Key::Right => {}
            Key::Enter => return self.click(&entry.path),
        }
        Requests::default()
    }

    /// Shows or hides git-ignored files: every listing is dropped and the root is listed again
    /// (open folders follow as their parents answer).
    pub fn toggle_ignored(&mut self) -> Requests {
        self.show_ignored = !self.show_ignored;
        self.generation += 1;
        self.dirs.retain(|dir, _| self.listing.contains(dir));
        Requests {
            list: vec![String::new()],
            read: None,
        }
    }

    /// Follows `git.changed` of this workspace (sent for every file-watcher batch): the root
    /// and open folders are listed again (closed ones when next opened) and the opened file is
    /// read again.
    pub fn apply(&mut self, event: &Event) -> Requests {
        let Event::GitChanged(changed) = event else {
            return Requests::default();
        };
        if changed.project_id != self.project_id || !self.started() {
            return Requests::default();
        }
        self.refresh()
    }

    /// Everything on screen again: the root and open folders are listed (closed ones when next
    /// opened) and the opened file is read. Also used when the view is shown again, since
    /// hidden views do not follow `git.changed`.
    pub fn refresh(&mut self) -> Requests {
        self.generation += 1;
        let open = self.open_dirs();
        self.dirs
            .retain(|dir, listing| open.contains(dir) || matches!(listing, Listing::Loading));
        Requests {
            list: open,
            read: self.file.clone(),
        }
    }

    /// The root and every open folder on screen (not those inside a closed or deleted
    /// folder), top to bottom.
    fn open_dirs(&self) -> Vec<String> {
        let open = self.rows().into_iter().filter_map(|row| match row {
            Row::Entry {
                entry,
                expanded: true,
                ..
            } => Some(entry.path),
            _ => None,
        });
        std::iter::once(String::new()).chain(open).collect()
    }
}

/// The tree key `keystroke` stands for (unmodified arrows and Enter).
pub fn key(keystroke: &Keystroke) -> Option<Key> {
    if keystroke.modifiers.modified() {
        return None;
    }
    match keystroke.key.as_str() {
        "up" => Some(Key::Up),
        "down" => Some(Key::Down),
        "left" => Some(Key::Left),
        "right" => Some(Key::Right),
        "enter" => Some(Key::Enter),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_protocol::{GitChangedEvent, GitMarker};

    fn dir(path: &str) -> FsEntry {
        FsEntry {
            name: path.rsplit('/').next().unwrap_or(path).into(),
            path: path.into(),
            is_dir: true,
            ignored: false,
            marker: None,
            has_changes: false,
        }
    }

    fn file(path: &str, marker: Option<GitMarker>) -> FsEntry {
        FsEntry {
            is_dir: false,
            marker,
            ..dir(path)
        }
    }

    fn code(text: &str) -> Code {
        Code::plain(text)
    }

    fn paths(view: &FilesView) -> Vec<String> {
        view.rows()
            .into_iter()
            .map(|row| match row {
                Row::Entry {
                    entry,
                    depth,
                    expanded,
                } => format!(
                    "{}{}{}",
                    "  ".repeat(depth),
                    entry.path,
                    if expanded { "/" } else { "" }
                ),
                Row::Note { depth, text } => format!("{}({text})", "  ".repeat(depth)),
            })
            .collect()
    }

    /// A view with `src/` (holding `src/main.rs`) and `README.md` listed.
    fn listed() -> FilesView {
        let mut view = FilesView::new(Ulid::new());
        assert!(view.begin_list(""));
        view.listed("", Ok(vec![dir("src"), file("README.md", None)]));
        view
    }

    #[test]
    fn folders_open_lazily_and_close() {
        let mut view = listed();
        assert_eq!(paths(&view), ["src", "README.md"]);
        let opened = view.click("src");
        assert_eq!(opened.list, ["src"]);
        assert!(view.begin_list("src"));
        assert_eq!(paths(&view), ["src/", "  (Loading…)", "README.md"]);
        view.listed(
            "src",
            Ok(vec![file("src/main.rs", Some(GitMarker::Modified))]),
        );
        assert_eq!(paths(&view), ["src/", "  src/main.rs", "README.md"]);
        // Closing and reopening reuses the listing.
        assert_eq!(view.click("src"), Requests::default());
        assert_eq!(paths(&view), ["src", "README.md"]);
        assert_eq!(view.click("src"), Requests::default());
        assert_eq!(paths(&view), ["src/", "  src/main.rs", "README.md"]);
    }

    #[test]
    fn a_failed_folder_says_so_and_retries_when_reopened() {
        let mut view = listed();
        view.click("src");
        view.begin_list("src");
        view.listed("src", Err("gone".into()));
        assert_eq!(
            paths(&view),
            ["src/", "  (Cannot list this folder: gone)", "README.md"]
        );
        view.click("src");
        assert_eq!(view.click("src").list, ["src"]);
    }

    #[test]
    fn a_listing_asked_for_while_one_is_in_flight_runs_after_it() {
        let mut view = FilesView::new(Ulid::new());
        assert!(view.begin_list(""));
        assert!(!view.begin_list(""));
        assert_eq!(view.listed("", Ok(vec![])), [""]);
        assert!(view.begin_list(""));
        assert!(view.listed("", Ok(vec![])).is_empty());
    }

    #[test]
    fn opening_reads_once_retries_failures_and_coalesces_reads() {
        let mut view = listed();
        assert_eq!(view.click("README.md").read.as_deref(), Some("README.md"));
        assert!(view.begin_read());
        assert_eq!(view.click("README.md").read, None);
        // Another file is opened while the read is in flight: it is read once that answers.
        assert_eq!(view.click("src/main.rs").read, None, "not a visible row");
        view.open("src/main.rs");
        assert!(!view.begin_read());
        assert!(!view.begin_read());
        let again = view.read("README.md", Ok(code("old")));
        assert_eq!(again.as_deref(), Some("src/main.rs"));
        assert_eq!(view.content, Content::Loading);
        assert!(view.begin_read());
        assert_eq!(view.read("src/main.rs", Err("too large".into())), None);
        assert_eq!(view.content, Content::Failed("too large".into()));
        assert_eq!(view.open("src/main.rs").as_deref(), Some("src/main.rs"));
        assert!(view.begin_read());
        view.read("src/main.rs", Ok(code("a\nb")));
        let Content::Shown(shown) = &view.content else {
            panic!("not shown: {:?}", view.content);
        };
        assert_eq!(shown.lines.len(), 2);
    }

    #[test]
    fn lines_are_picked_and_cleared() {
        let mut view = listed();
        view.open("README.md");
        view.begin_read();
        view.read("README.md", Ok(code("a\nb\nc")));
        view.pick_line(2);
        assert_eq!(view.line, Some(2));
        view.pick_line(2);
        assert_eq!(view.line, None);
        view.pick_line(3);
        // A re-read that got shorter drops a line that no longer exists.
        view.begin_read();
        view.read("README.md", Ok(code("a")));
        assert_eq!(view.line, None);
        // Opening another file clears it too.
        view.pick_line(1);
        view.open("src/main.rs");
        assert_eq!(view.line, None);
    }

    #[test]
    fn arrow_keys_move_open_and_close() {
        let mut view = listed();
        // No cursor yet: the first key lands on the first row.
        assert_eq!(view.key(Key::Down), Requests::default());
        assert_eq!(view.cursor.as_deref(), Some("src"));
        assert_eq!(view.key(Key::Right).list, ["src"]);
        view.begin_list("src");
        view.listed("src", Ok(vec![file("src/main.rs", None)]));
        // Right on an open folder goes to its first child; Left goes back up.
        view.key(Key::Right);
        assert_eq!(view.cursor.as_deref(), Some("src/main.rs"));
        view.key(Key::Left);
        assert_eq!(view.cursor.as_deref(), Some("src"));
        view.key(Key::Down);
        view.key(Key::Down);
        assert_eq!(view.cursor.as_deref(), Some("README.md"));
        view.key(Key::Down);
        assert_eq!(view.cursor.as_deref(), Some("README.md"));
        assert_eq!(view.key(Key::Enter).read.as_deref(), Some("README.md"));
        view.key(Key::Up);
        view.key(Key::Up);
        assert_eq!(view.cursor.as_deref(), Some("src"));
        view.key(Key::Left);
        assert_eq!(paths(&view), ["src", "README.md"]);
        view.key(Key::Up);
        assert_eq!(view.cursor.as_deref(), Some("src"));
    }

    #[test]
    fn git_changed_relists_open_folders_and_rereads_the_shown_file() {
        let mut view = listed();
        view.click("src");
        view.begin_list("src");
        view.listed("src", Ok(vec![dir("src/a")]));
        view.click("src/a");
        view.begin_list("src/a");
        view.listed("src/a", Ok(vec![]));
        view.click("src/a");
        view.click("README.md");
        view.begin_read();
        view.read("README.md", Ok(code("x")));
        let changed = |project_id| Event::GitChanged(GitChangedEvent { project_id });
        assert_eq!(view.apply(&changed(Ulid::new())), Requests::default());
        let requests = view.apply(&changed(view.project_id));
        assert_eq!(requests.list, ["", "src"]);
        assert_eq!(requests.read.as_deref(), Some("README.md"));
        // The closed `src/a` lost its listing: opening it lists it again.
        assert_eq!(view.click("src/a").list, ["src/a"]);
        // Old rows stay on screen while the re-lists run.
        assert_eq!(paths(&view)[0], "src/");
    }

    #[test]
    fn refresh_skips_folders_inside_closed_or_deleted_ones_and_drops_stale_answers() {
        let mut view = listed();
        view.click("src");
        view.begin_list("src");
        view.listed("src", Ok(vec![dir("src/a")]));
        view.click("src/a");
        assert!(view.begin_list("src/a"));
        // `src` closes while `src/a`'s listing is in flight: only the root is re-listed.
        view.click("src");
        assert_eq!(view.refresh().list, [""]);
        // The answer predates the refresh and `src/a` is not on screen: dropped. Reopening
        // `src` lists it fresh, and its still-open `src/a` follows.
        assert!(view.listed("src/a", Ok(vec![])).is_empty());
        assert_eq!(view.click("src").list, ["src"]);
        assert!(view.begin_list("src"));
        assert_eq!(view.listed("src", Ok(vec![dir("src/a")])), ["src/a"]);
        assert_eq!(
            paths(&view),
            ["src/", "  src/a/", "    (Loading…)", "README.md"]
        );
        // `src` is deleted: the root no longer lists it, so nothing below it is re-listed.
        view.begin_list("");
        view.listed("", Ok(vec![file("README.md", None)]));
        assert_eq!(view.refresh().list, [""]);
    }

    #[test]
    fn refreshing_a_view_never_shown_lists_the_root() {
        let mut view = FilesView::new(Ulid::new());
        assert_eq!(
            view.refresh(),
            Requests {
                list: vec![String::new()],
                read: None
            }
        );
    }

    #[test]
    fn keys_are_unmodified_arrows_and_enter() {
        let parse = |keys: &str| key(&Keystroke::parse(keys).unwrap());
        assert_eq!(parse("up"), Some(Key::Up));
        assert_eq!(parse("down"), Some(Key::Down));
        assert_eq!(parse("left"), Some(Key::Left));
        assert_eq!(parse("right"), Some(Key::Right));
        assert_eq!(parse("enter"), Some(Key::Enter));
        assert_eq!(parse("alt-left"), None);
        assert_eq!(parse("j"), None);
    }

    #[test]
    fn git_changed_before_the_view_was_shown_does_nothing() {
        let mut view = FilesView::new(Ulid::new());
        let changed = Event::GitChanged(GitChangedEvent {
            project_id: view.project_id,
        });
        assert_eq!(view.apply(&changed), Requests::default());
    }

    #[test]
    fn toggling_ignored_relists_the_root_then_open_folders() {
        let mut view = listed();
        view.click("src");
        view.begin_list("src");
        view.listed("src", Ok(vec![dir("src/a")]));
        view.click("src/a");
        // `src/a`'s listing (without ignored files) is in flight during the toggle.
        assert!(view.begin_list("src/a"));
        let requests = view.toggle_ignored();
        assert!(view.show_ignored);
        assert_eq!(requests.list, [""]);
        assert_eq!(view.root(), &Listing::Loading);
        assert!(view.begin_list(""));
        // The root answers: its open folder `src` follows.
        let root = vec![dir("src"), dir("target"), file("README.md", None)];
        assert_eq!(view.listed("", Ok(root)), ["src"]);
        assert!(view.begin_list("src"));
        // `src/a` is still in flight, so `src` does not ask for it again…
        assert!(view.listed("src", Ok(vec![dir("src/a")])).is_empty());
        // …and its stale answer re-lists it with the new setting instead of being shown.
        assert_eq!(
            view.listed("src/a", Ok(vec![file("src/a/x", None)])),
            ["src/a"]
        );
        assert!(view.begin_list("src/a"));
        view.listed("src/a", Ok(vec![file("src/a/y", None)]));
        assert_eq!(
            paths(&view),
            ["src/", "  src/a/", "    src/a/y", "target", "README.md"]
        );
    }
}
