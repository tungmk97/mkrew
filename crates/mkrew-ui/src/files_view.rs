//! Files view-model (SPEC §12A): a lazily listed tree of the workspace (`fs.list` per folder,
//! with git markers and an optional "show ignored"), the tree cursor moved by arrow keys, and
//! the opened files as tabs ([`FileTab`], ADR 0080: `fs.read`, highlighted off the UI thread
//! into a [`Code`], in an [`Editor`], saved with `fs.write`). `git.changed` re-lists the open
//! folders and re-reads the active tab; the others are read when activated.
//! Pure and tested headlessly; `main.rs` does the requests and renders it.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use gpui::Keystroke;
use mkrew_protocol::{Event, FsContent, FsEntry, FsWritten};
use ulid::Ulid;

use crate::docs::{self, Mode};
use crate::editor::{Editor, Pos};
use crate::highlight::Code;
use crate::markdown::Lines;
use crate::preview_select::{self, Point, Selection};
use crate::task_dialog::DialogKey;
use crate::text_box::{Input, TextBox};
use crate::text_find::{self, TextFind};

/// One folder's children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listing {
    Loading,
    Loaded(Vec<FsEntry>),
    Failed(String),
}

/// A tab's file.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Loading,
    Shown(Box<Shown>),
    Failed(String),
}

/// The opened file in the editor (SPEC §12A, ADR 0028).
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    /// The last highlight, with the binary / too-large flags. Its spans are drawn only on
    /// lines whose text it still matches; the text is highlighted again once typing pauses.
    pub code: Rc<Code>,
    pub editor: Editor,
    /// `fs.read`'s hash of the version on disk that the text started from (or was last saved
    /// as). `None` = cannot be saved (binary, too large, or an old daemon).
    pub hash: Option<String>,
    pub language: Option<String>,
    /// The file's text in the git index from the last read (`fs.read` `index_text`); the text
    /// highlighted again after typing is diffed with it (SPEC §12A gutter).
    pub index_text: Option<String>,
    /// The file on disk changed under unsaved text: its hash ("Changed on disk — Reload ·
    /// Keep mine").
    pub disk_changed: Option<String>,
    /// Why the last save failed.
    pub save_error: Option<String>,
}

/// An `fs.write` of the opened file (SPEC §7): its text, the hash the file must still have
/// on disk, and the undo state it saves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveRequest {
    pub path: String,
    pub text: String,
    pub expected_hash: String,
    pub point: Option<u64>,
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

/// Numbers every text the view shows (across views), so that a page built for one text is
/// never taken for another's.
static NEXT_TEXT: AtomicU64 = AtomicU64::new(1);

/// What a preview was built from: the number of the text in the editor and the editor's
/// version of it. Later texts and later edits have larger stamps.
pub type Stamp = (u64, u64);

/// The rendered page of a `.md` file (ADR 0021) and what it was built from.
#[derive(Debug)]
struct MdPreview {
    stamp: Stamp,
    shown: Rc<docs::Shown>,
}

/// The top-level block of the preview whose Markdown is edited in place (SPEC §12A, ADR 0072):
/// its source lines as of `stamp`, and the field's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockEdit {
    pub block: usize,
    pub lines: Lines,
    stamp: Stamp,
    pub text: TextBox,
}

/// What a key in the block edit field does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockEditOutcome {
    Edited,
    /// Esc: close it, the typed text is dropped.
    Cancel,
    /// Ctrl+Enter.
    Apply,
}

/// The text changed since the field opened: applying could hit the wrong lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaleEdit;

/// How a file opens (SPEC §12A, ADR 0080).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Open {
    /// A single click, Enter, quick open, a link: the transient (italic) tab, replaced by the
    /// next one while it is clean.
    Transient,
    /// A double click or a new file: a permanent tab.
    Kept,
}

/// Which tabs a close asks for (tab menu; pinned tabs stay, except for `One`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Close {
    One,
    Others,
    Right,
    All,
}

/// One opened file of the Files view (SPEC §12A, ADR 0080).
#[derive(Debug)]
pub struct FileTab {
    /// Relative to the shown folder, `/` separators.
    pub path: String,
    pub content: Content,
    /// `.md` files: the rendered preview or the editor. A new tab starts in Preview.
    pub mode: Mode,
    /// VS Code's preview tab: italic, replaced by the next transient open while clean.
    pub transient: bool,
    /// Pinned tabs lead the strip and are skipped by bulk closes.
    pub pinned: bool,
    /// The tab's [`docs::new_opening`] number: the shell's page restarts when it changes.
    pub opening: u64,
    /// Number of the text in the editor: new for every re-read that replaced it.
    text_id: u64,
    /// The last page built, and the stamp last asked for (one build per text).
    page: Option<MdPreview>,
    page_asked: Option<Stamp>,
    /// The preview's text selection; cleared with every new page.
    pub preview_selection: Option<Selection>,
    /// The block being edited in the preview, `None` = none.
    pub block_edit: Option<BlockEdit>,
    /// An `fs.write` is in flight.
    saving: bool,
    /// An `fs.read` is in flight, and another was asked for meanwhile (sent once it answers,
    /// so a burst of changes reads and highlights the file once more, not once per change).
    reading: bool,
    read_again: bool,
    /// `git.changed` arrived while another tab was active: read when activated.
    stale: bool,
    /// A line (1-based) that a link asked for, put under the caret once the file is shown.
    pending_line: Option<u32>,
}

/// One workspace's Files view.
#[derive(Debug)]
pub struct FilesView {
    pub project_id: Ulid,
    /// The task worktree shown instead of the project folder (SPEC §20E.1).
    pub worktree_task_id: Option<Ulid>,
    pub show_ignored: bool,
    /// Listings by folder path (`""` = the root).
    dirs: HashMap<String, Listing>,
    expanded: HashSet<String>,
    /// The visible rows laid out from `dirs` / `expanded` (see [`Self::relayout`]): the
    /// render and every key read them, so they are not rebuilt (and cloned) per frame.
    rows: Rc<[Row]>,
    /// The tree cursor (a file or folder path).
    pub cursor: Option<String>,
    /// The opened files, pinned ones first; at most one per path.
    tabs: Vec<FileTab>,
    /// The tab shown; `None` exactly when `tabs` is empty.
    active: Option<usize>,
    /// Why the last "Open in Zed" failed.
    pub zed_error: Option<String>,
    /// The find bar (SPEC §12A), `None` = closed. Shared by the tabs; searched again on the
    /// active tab's text whenever it changes.
    pub find: Option<TextFind>,
    /// Folders with an `fs.list` in flight, and those asked again meanwhile.
    listing: HashSet<String>,
    list_again: HashSet<String>,
    /// Bumped by every refresh and ignored-toggle: a listing sent before it is stale.
    generation: u64,
    /// The generation each in-flight `fs.list` was sent in.
    sent_in: HashMap<String, u64>,
}

/// A save's answer that has no file view to land in: only a failure or a refused save is news.
pub fn unshown_save(written: Result<FsWritten, String>) -> Option<String> {
    match written {
        Ok(written) if written.saved => None,
        Ok(_) => Some("Not saved: the file changed on disk meanwhile.".to_owned()),
        Err(error) => Some(format!("Not saved: {error}")),
    }
}

impl FilesView {
    pub fn new(project_id: Ulid, worktree_task_id: Option<Ulid>) -> FilesView {
        FilesView {
            project_id,
            worktree_task_id,
            show_ignored: false,
            dirs: HashMap::new(),
            expanded: HashSet::new(),
            rows: Rc::new([]),
            cursor: None,
            tabs: Vec::new(),
            active: None,
            zed_error: None,
            find: None,
            listing: HashSet::new(),
            list_again: HashSet::new(),
            generation: 0,
            sent_in: HashMap::new(),
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
        if !self.dirs.contains_key(dir) {
            self.dirs.insert(dir.to_owned(), Listing::Loading);
            self.relayout();
        }
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
            self.relayout();
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
        self.relayout();
        list
    }

    /// The visible rows, top to bottom: folders first as the daemon sorts them, open folders
    /// followed by their children.
    pub fn rows(&self) -> Rc<[Row]> {
        Rc::clone(&self.rows)
    }

    /// Lays the rows out again; called by everything that changes `dirs` or `expanded`.
    fn relayout(&mut self) {
        let mut rows = Vec::new();
        if let Listing::Loaded(entries) = self.root() {
            self.push_rows(entries, 0, &mut rows);
        }
        self.rows = rows.into();
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
        self.rows.iter().find_map(|row| match row {
            Row::Entry { entry, .. } if entry.path == path => Some(entry.clone()),
            _ => None,
        })
    }

    /// A click on a row: the cursor moves there; a folder opens or closes, a file opens.
    pub fn click(&mut self, path: &str, how: Open) -> Requests {
        self.cursor = Some(path.to_owned());
        let Some(entry) = self.entry(path) else {
            return Requests::default();
        };
        if entry.is_dir {
            return self.toggle(&entry.path);
        }
        Requests {
            list: vec![],
            read: self.open(&entry.path, how),
        }
    }

    /// Opens or closes folder `dir`; opening lists it unless already listed.
    pub fn toggle(&mut self, dir: &str) -> Requests {
        if self.expanded.remove(dir) {
            self.relayout();
            return Requests::default();
        }
        self.expand(dir)
    }

    fn expand(&mut self, dir: &str) -> Requests {
        if self.expanded.insert(dir.to_owned()) {
            self.relayout();
        }
        let list = match self.dirs.get(dir) {
            Some(Listing::Loaded(_) | Listing::Loading) => vec![],
            Some(Listing::Failed(_)) | None => vec![dir.to_owned()],
        };
        Requests { list, read: None }
    }

    pub fn tabs(&self) -> &[FileTab] {
        &self.tabs
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active
    }

    /// The active tab.
    pub fn tab(&self) -> Option<&FileTab> {
        self.tabs.get(self.active?)
    }

    pub fn tab_mut(&mut self) -> Option<&mut FileTab> {
        self.tabs.get_mut(self.active?)
    }

    pub fn tab_for(&self, path: &str) -> Option<&FileTab> {
        self.tabs.iter().find(|tab| tab.path == path)
    }

    pub fn tab_for_mut(&mut self, path: &str) -> Option<&mut FileTab> {
        self.tabs.iter_mut().find(|tab| tab.path == path)
    }

    /// The active tab's path.
    pub fn file(&self) -> Option<&str> {
        self.tab().map(|tab| tab.path.as_str())
    }

    pub fn shown(&self) -> Option<&Shown> {
        self.tab()?.shown()
    }

    /// The active tab's editor, when its file is editable.
    pub fn editor_mut(&mut self) -> Option<&mut Editor> {
        self.tab_mut()?.editor_mut()
    }

    /// Every tab with unsaved changes, in strip order.
    pub fn unsaved_paths(&self) -> Vec<String> {
        (self.tabs.iter())
            .filter(|tab| tab.is_dirty())
            .map(|tab| tab.path.clone())
            .collect()
    }

    /// The first of `paths` whose tab is still unsaved (asked again after a save that text
    /// typed meanwhile made unsaved again).
    pub fn first_unsaved(&self, paths: &[String]) -> Option<String> {
        (paths.iter())
            .find(|path| self.tab_for(path).is_some_and(FileTab::is_dirty))
            .cloned()
    }

    /// "Don't save" / "Reload": drops `path`'s unsaved changes; returns the file to read
    /// again, so the tab shows what is on disk.
    pub fn drop_changes(&mut self, path: &str) -> Option<String> {
        let tab = self.tab_for_mut(path)?;
        tab.discard();
        Some(tab.path.clone())
    }

    /// The active tab becomes permanent (an edit).
    pub fn keep(&mut self) {
        if let Some(tab) = self.tab_mut() {
            tab.transient = false;
        }
    }

    fn position(&self, path: &str) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.path == path)
    }

    fn pinned_count(&self) -> usize {
        self.tabs.iter().filter(|tab| tab.pinned).count()
    }

    /// Shows `path`: its tab when open (`Kept` makes it permanent), else a new tab that takes
    /// the clean transient tab's place, or goes right of the active tab (after the pinned
    /// ones). Returns the file to read.
    pub fn open(&mut self, path: &str, how: Open) -> Option<String> {
        if let Some(at) = self.position(path) {
            if how == Open::Kept {
                self.tabs[at].transient = false;
            }
            return self.activate(at);
        }
        let tab = FileTab::new(path, how == Open::Transient);
        // Only a transient open takes the transient tab's place.
        let replace = (how == Open::Transient)
            .then(|| (self.tabs.iter()).position(|tab| tab.transient && !tab.is_dirty()))
            .flatten();
        let at = match replace {
            Some(at) => {
                self.tabs[at] = tab;
                at
            }
            None => {
                let at = (self.active.map_or(self.tabs.len(), |active| active + 1))
                    .max(self.pinned_count());
                self.tabs.insert(at, tab);
                at
            }
        };
        self.active = Some(at);
        Some(path.to_owned())
    }

    /// Shows tab `at`; returns its file when it must be read (stale or failed).
    pub fn activate(&mut self, at: usize) -> Option<String> {
        if at >= self.tabs.len() {
            return None;
        }
        self.active = Some(at);
        self.search();
        let tab = &mut self.tabs[at];
        tab.take_read().then(|| tab.path.clone())
    }

    /// Ctrl+Tab / Ctrl+Shift+Tab: the next or previous tab, wrapping around.
    pub fn cycle(&mut self, forward: bool) -> Option<String> {
        let (at, len) = (self.active?, self.tabs.len());
        self.activate(if forward {
            (at + 1) % len
        } else {
            (at + len - 1) % len
        })
    }

    /// Ctrl+Alt+`n` (1–9): tab `n`, 9 = the last one.
    pub fn select_nth(&mut self, n: u8) -> Option<String> {
        let at = if n == 9 {
            self.tabs.len().checked_sub(1)?
        } else {
            usize::from(n).checked_sub(1)?
        };
        self.activate(at)
    }

    /// The tabs a close of `path`'s tab asks for (pinned tabs stay, except for `One`).
    pub fn close_targets(&self, path: &str, which: Close) -> Vec<String> {
        let Some(at) = self.position(path) else {
            return Vec::new();
        };
        (self.tabs.iter().enumerate())
            .filter(|(i, tab)| match which {
                Close::One => *i == at,
                Close::Others => *i != at && !tab.pinned,
                Close::Right => *i > at && !tab.pinned,
                Close::All => !tab.pinned,
            })
            .map(|(_, tab)| tab.path.clone())
            .collect()
    }

    /// Closes the tabs of `paths` (unsaved text was asked about first). A closed active tab
    /// passes to its right neighbour, else its left one. Returns the file to read.
    pub fn close(&mut self, paths: &[String]) -> Option<String> {
        let active = self.active;
        let mut next = None;
        let mut kept = Vec::with_capacity(self.tabs.len());
        for (i, tab) in std::mem::take(&mut self.tabs).into_iter().enumerate() {
            if paths.contains(&tab.path) {
                continue;
            }
            // The active tab itself, else the first one right of it.
            if active.is_some_and(|active| i >= active) && next.is_none() {
                next = Some(kept.len());
            }
            kept.push(tab);
        }
        self.tabs = kept;
        // None right of it: the last one (its left neighbour), or none at all.
        self.active = next.or(self.tabs.len().checked_sub(1));
        let at = self.active?;
        self.activate(at)
    }

    /// Pins or unpins `path`'s tab: it goes to the end of the pinned tabs, or the start of
    /// the unpinned ones. A pinned tab is permanent.
    pub fn pin(&mut self, path: &str, pinned: bool) {
        let Some(at) = self.position(path) else {
            return;
        };
        let active = self.file().map(str::to_owned);
        let mut tab = self.tabs.remove(at);
        tab.pinned = pinned;
        tab.transient &= !pinned;
        let to = self.pinned_count();
        self.tabs.insert(to, tab);
        self.active = active.and_then(|path| self.position(&path));
    }

    /// A reorder drop: the `from`-th of the pinned or unpinned tabs goes to place `to` among
    /// them (`to` counted without it, as `sidebar::drop_index` gives it).
    pub fn move_tab(&mut self, pinned: bool, from: usize, to: usize) {
        let pinned_count = self.pinned_count();
        let (start, end) = if pinned {
            (0, pinned_count)
        } else {
            (pinned_count, self.tabs.len())
        };
        if start + from >= end || start + to >= end {
            return;
        }
        let active = self.file().map(str::to_owned);
        let tab = self.tabs.remove(start + from);
        self.tabs.insert(start + to, tab);
        self.active = active.and_then(|path| self.position(&path));
    }

    /// Opens file `path` from outside the tree (quick open, a terminal link): its folders open
    /// (listed as needed), the cursor moves to it and its tab shows; with `line`, the caret
    /// goes there once the file is shown ([`FileTab::apply_pending_line`]).
    pub fn reveal(&mut self, path: &str, line: Option<u32>, how: Open) -> Requests {
        let mut list = Vec::new();
        let mut dir = String::new();
        let mut parts: Vec<&str> = path.split('/').collect();
        parts.pop();
        for part in parts {
            if !dir.is_empty() {
                dir.push('/');
            }
            dir.push_str(part);
            list.extend(self.expand(&dir).list);
        }
        self.cursor = Some(path.to_owned());
        let read = self.open(path, how);
        if let Some(tab) = self.tab_mut() {
            tab.pending_line = line;
            // The caret of a link's line shows in the editor, not in the preview.
            if line.is_some() {
                tab.mode = Mode::Source;
            }
        }
        Requests { list, read }
    }

    /// Opens folder `dir` and its parent folders (listed as needed) and moves the cursor to it
    /// (a folder made by "New folder…").
    pub fn reveal_dir(&mut self, dir: &str) -> Requests {
        let mut list = Vec::new();
        for (end, _) in dir.match_indices('/').chain([(dir.len(), "")]) {
            list.extend(self.expand(&dir[..end]).list);
        }
        self.cursor = Some(dir.to_owned());
        Requests { list, read: None }
    }

    /// An `fs.read` of `path` is about to be sent; `false` = its tab is gone, or one is in
    /// flight already (sent again once that one answers).
    pub fn begin_read(&mut self, path: &str) -> bool {
        let Some(tab) = self.tab_for_mut(path) else {
            return false;
        };
        if tab.reading {
            tab.read_again = true;
            return false;
        }
        tab.reading = true;
        true
    }

    /// The `fs.read` answer (with its highlight) for `path`, shown in its tab (dropped when
    /// the tab is gone). Returns the file to read again (asked for meanwhile).
    pub fn read(&mut self, path: &str, read: Result<(FsContent, Code), String>) -> Option<String> {
        let again = self.tab_for_mut(path)?.read(read);
        if self.file() == Some(path) {
            self.search();
        }
        again.then(|| path.to_owned())
    }

    /// Save `path`'s tab (Ctrl+S, "Save" in the unsaved-changes question). `None` when
    /// nothing is unsaved, the file cannot be saved, or a save is already in flight.
    pub fn begin_save(&mut self, path: &str) -> Option<SaveRequest> {
        self.tab_for_mut(path)?.begin_save()
    }

    /// The `fs.write` answer for `path`, which saved the state `point`. `Some(why)` when the
    /// answer has nowhere to show because its tab was closed meanwhile: the shell toasts it.
    pub fn saved(
        &mut self,
        path: &str,
        point: Option<u64>,
        written: Result<FsWritten, String>,
    ) -> Option<String> {
        match self.tab_for_mut(path) {
            Some(tab) => tab.saved(point, written),
            None => unshown_save(written),
        }
    }

    /// The page built for `stamp`, for the tab whose text it was built from. Returns `true`
    /// when an open block edit was closed by the new page.
    pub fn preview_ready(&mut self, stamp: Stamp, shown: docs::Shown) -> bool {
        (self.tabs.iter_mut())
            .find(|tab| tab.text_id == stamp.0)
            .is_some_and(|tab| tab.preview_ready(stamp, shown))
    }

    /// Runs the find bar's query over the active tab's text again.
    pub fn search(&mut self) {
        let shown = (self.active)
            .and_then(|at| self.tabs.get(at))
            .and_then(FileTab::shown);
        if let (Some(find), Some(shown)) = (&mut self.find, shown) {
            find.search(shown.editor.lines());
        }
    }

    /// Ctrl+F / Ctrl+H: opens the find bar, with the replace row when `replace`. A selection
    /// on one line becomes the query, and the current match is the first one at the selection
    /// or caret. Returns `false` when no file is open.
    pub fn open_find(&mut self, replace: bool) -> bool {
        let Some(tab) = self.active.and_then(|at| self.tabs.get_mut(at)) else {
            return false;
        };
        if !matches!(tab.content, Content::Shown(_)) {
            return false;
        }
        // Matches and the replace row act on the editor's text.
        tab.mode = Mode::Source;
        tab.block_edit = None;
        tab.preview_selection = None;
        let Content::Shown(shown) = &tab.content else {
            return false;
        };
        let editor = &shown.editor;
        let seed = (editor.selected_text()).filter(|text| !text.contains('\n'));
        let from = editor
            .selection()
            .map_or(editor.cursor(), |selection| selection.start);
        let find = self.find.get_or_insert_with(TextFind::default);
        if let Some(seed) = seed {
            find.query = seed.into();
        }
        if replace {
            find.replace.get_or_insert_with(Input::default);
        }
        find.search_from(editor.lines(), (from.line, from.col));
        true
    }

    /// The find bar and the editor of the active tab's editable file (not binary or too large).
    fn find_and_editor(&mut self) -> Option<(&mut TextFind, &mut Editor)> {
        let tab = self.active.and_then(|at| self.tabs.get_mut(at))?;
        match (&mut self.find, &mut tab.content) {
            (Some(find), Content::Shown(shown)) if !shown.code.binary && !shown.code.too_large => {
                Some((find, &mut shown.editor))
            }
            _ => None,
        }
    }

    /// Replace (Enter in the replace field): the current match becomes the replacement and the
    /// next match after it is current. One undo step; not saved (like VS Code).
    pub fn replace_one(&mut self) -> bool {
        let Some((find, editor)) = self.find_and_editor() else {
            return false;
        };
        let (Some(with), Some(found)) = (find.replace.as_deref(), find.current().cloned()) else {
            return false;
        };
        let line = found.line;
        editor.replace(
            Pos::new(line, found.range.start)..Pos::new(line, found.range.end),
            with,
        );
        // Continue after the replacement, so a replacement that holds the query is not
        // replaced again.
        let after = found.range.start + with.len();
        find.search_from(editor.lines(), (line, after));
        true
    }

    /// Replace all (Ctrl+Alt+Enter, "Replace all"): every match at once, as one undo step;
    /// not saved. Returns how many matches were replaced.
    pub fn replace_all(&mut self) -> usize {
        let Some((find, editor)) = self.find_and_editor() else {
            return 0;
        };
        let Some(with) = find.replace.as_deref() else {
            return 0;
        };
        let edits: Vec<(Range<Pos>, String)> = (editor.lines().iter().enumerate())
            .flat_map(|(line, text)| {
                text_find::find_in_line(text, &find.query)
                    .into_iter()
                    .map(move |range| {
                        let at = Pos::new(line, range.start)..Pos::new(line, range.end);
                        (at, with.to_owned())
                    })
            })
            .collect();
        let count = edits.len();
        editor.replace_all(edits);
        find.search(editor.lines());
        count
    }

    /// The row Enter opens: the cursor's, while that row is still listed (an agent may have
    /// deleted it, or the ignored rows were hidden).
    pub fn enter_target(&self) -> Option<String> {
        let cursor = self.cursor.as_ref()?;
        self.entry(cursor).map(|_| cursor.clone())
    }

    /// Moves the cursor or opens/closes folders.
    pub fn key(&mut self, key: Key) -> Requests {
        let rows = self.rows();
        let rows: Vec<(&FsEntry, usize, bool)> = rows
            .iter()
            .filter_map(|row| match row {
                Row::Entry {
                    entry,
                    depth,
                    expanded,
                } => Some((entry, *depth, *expanded)),
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
        let (entry, depth, expanded) = rows[at];
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
            Key::Enter => return self.click(&entry.path, Open::Transient),
        }
        Requests::default()
    }

    /// Shows or hides git-ignored files: every listing is dropped and the root is listed again
    /// (open folders follow as their parents answer).
    pub fn toggle_ignored(&mut self) -> Requests {
        self.show_ignored = !self.show_ignored;
        self.generation += 1;
        self.dirs.retain(|dir, _| self.listing.contains(dir));
        self.relayout();
        Requests {
            list: vec![String::new()],
            read: None,
        }
    }

    /// Follows `git.changed` of this workspace (sent for every file-watcher batch): the root
    /// and open folders are listed again (closed ones when next opened) and the active tab is
    /// read again.
    pub fn apply(&mut self, event: &Event) -> Requests {
        let Event::GitChanged(changed) = event else {
            return Requests::default();
        };
        // A task worktree's changes (SPEC §20E.1) are only that worktree's.
        if changed.project_id != self.project_id
            || changed.worktree_task_id != self.worktree_task_id
            || !self.started()
        {
            return Requests::default();
        }
        self.refresh()
    }

    /// Everything on screen again: the root and open folders are listed (closed ones when next
    /// opened) and the active tab is read; the other tabs are read when activated. Also used
    /// when the view is shown again, since hidden views do not follow `git.changed`.
    pub fn refresh(&mut self) -> Requests {
        self.generation += 1;
        let open = self.open_dirs();
        self.dirs
            .retain(|dir, listing| open.contains(dir) || matches!(listing, Listing::Loading));
        self.relayout();
        let active = self.active;
        for (at, tab) in self.tabs.iter_mut().enumerate() {
            tab.stale |= Some(at) != active;
        }
        Requests {
            list: open,
            read: self.file().map(str::to_owned),
        }
    }

    /// The root and every open folder on screen (not those inside a closed or deleted
    /// folder), top to bottom.
    fn open_dirs(&self) -> Vec<String> {
        let open = self.rows.iter().filter_map(|row| match row {
            Row::Entry {
                entry,
                expanded: true,
                ..
            } => Some(entry.path.clone()),
            _ => None,
        });
        std::iter::once(String::new()).chain(open).collect()
    }
}

impl FileTab {
    fn new(path: &str, transient: bool) -> FileTab {
        FileTab {
            path: path.to_owned(),
            content: Content::Loading,
            mode: Mode::Preview,
            transient,
            pinned: false,
            opening: docs::new_opening(),
            text_id: 0,
            page: None,
            page_asked: None,
            preview_selection: None,
            block_edit: None,
            saving: false,
            reading: false,
            read_again: false,
            stale: false,
            pending_line: None,
        }
    }

    /// The file name (the tab's title).
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// The text has unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.shown().is_some_and(|shown| shown.editor.is_dirty())
    }

    /// Whether activating the tab must read it: `git.changed` came while it was hidden, or
    /// its last read failed (a retry). Clears the stale mark.
    fn take_read(&mut self) -> bool {
        let failed = matches!(self.content, Content::Failed(_));
        if failed {
            self.content = Content::Loading;
        }
        std::mem::take(&mut self.stale) || failed
    }

    /// The `fs.read` answer; returns whether another read was asked for meanwhile. A failed
    /// read drops a link's line (a retry shows the file from the top).
    fn read(&mut self, read: Result<(FsContent, Code), String>) -> bool {
        self.reading = false;
        let again = std::mem::take(&mut self.read_again);
        if read.is_err() {
            self.pending_line = None;
        }
        self.show(read);
        again
    }

    /// Puts the caret on the line [`FilesView::reveal`] was given, once the file is shown
    /// (call it after every read and after `reveal`). Returns the 0-based line to scroll to,
    /// clamped to the text; `None` while nothing is shown or nothing is pending.
    pub fn apply_pending_line(&mut self) -> Option<usize> {
        let Content::Shown(shown) = &mut self.content else {
            return None;
        };
        let line = usize::try_from(self.pending_line.take()?)
            .ok()?
            .saturating_sub(1)
            .min(shown.editor.lines().len().saturating_sub(1));
        shown.editor.set_cursor(Pos::new(line, 0), false);
        Some(line)
    }

    fn show(&mut self, read: Result<(FsContent, Code), String>) {
        let (dirty, hash, cursor) = match &self.content {
            Content::Shown(shown) => (
                shown.editor.is_dirty(),
                shown.hash.clone(),
                Some(shown.editor.cursor()),
            ),
            _ => (false, None, None),
        };
        // Unsaved text is never replaced by a re-read (SPEC §2.5). Another version on disk is
        // offered as "Reload · Keep mine"; the version the text started from clears that.
        if dirty {
            if let (Ok((content, _)), Content::Shown(shown)) = (&read, &mut self.content) {
                shown.disk_changed =
                    (content.hash != hash).then(|| content.hash.clone().unwrap_or_default());
                shown.index_text = content.index_text.clone();
            }
            return;
        }
        // The same version again (our own save, a change elsewhere): caret and undo stay; a
        // staged or committed change still moves the gutter.
        if let (Ok((content, code)), Content::Shown(shown)) = (&read, &mut self.content)
            && hash.is_some()
            && content.hash == hash
        {
            if shown.index_text != content.index_text {
                shown.index_text = content.index_text.clone();
                shown.code = Rc::new(code.clone());
            }
            return;
        }
        self.content = match read {
            Err(error) => Content::Failed(error),
            Ok((content, code)) => {
                self.text_id = NEXT_TEXT.fetch_add(1, Ordering::Relaxed);
                let mut editor = Editor::new(content.content.as_deref().unwrap_or(""));
                if let Some(cursor) = cursor {
                    editor.set_cursor(cursor, false);
                }
                Content::Shown(Box::new(Shown {
                    code: Rc::new(code),
                    editor,
                    hash: content.hash,
                    language: content.language,
                    index_text: content.index_text,
                    disk_changed: None,
                    save_error: None,
                }))
            }
        };
    }

    pub fn shown(&self) -> Option<&Shown> {
        match &self.content {
            Content::Shown(shown) => Some(shown),
            _ => None,
        }
    }

    /// Whether the file is Markdown (`.md`).
    pub fn is_markdown(&self) -> bool {
        (self.path.rsplit_once('.')).is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("md"))
    }

    /// The `.md` file is shown as its rendered page, not as the editor.
    pub fn previewing(&self) -> bool {
        self.mode == Mode::Preview && self.is_markdown() && self.stamp().is_some()
    }

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            Mode::Preview => Mode::Source,
            Mode::Source => Mode::Preview,
        };
        self.preview_selection = None;
        self.block_edit = None;
    }

    /// What a page of the editor's text now would be built from (`None`: nothing editable
    /// shown).
    fn stamp(&self) -> Option<Stamp> {
        let shown = (self.shown()).filter(|shown| !shown.code.binary && !shown.code.too_large)?;
        Some((self.text_id, shown.editor.version()))
    }

    /// The last page of this text; while an edited text is parsed again it is the page of the
    /// text before the edit. `None` after another text replaced it.
    pub fn preview(&self) -> Option<&Rc<docs::Shown>> {
        (self.page.as_ref())
            .filter(|page| page.stamp.0 == self.text_id)
            .map(|page| &page.shown)
    }

    /// The text to parse for the preview, once per text: `None` when no preview is shown, the
    /// page is current, or one is being built for this text. The shell parses it off the UI
    /// thread and answers with [`FilesView::preview_ready`].
    pub fn preview_request(&mut self) -> Option<(Stamp, String)> {
        if !self.previewing() {
            return None;
        }
        let stamp = self.stamp()?;
        let current = (self.page.as_ref()).is_some_and(|page| page.stamp == stamp);
        if current || self.page_asked == Some(stamp) {
            return None;
        }
        self.page_asked = Some(stamp);
        Some((stamp, self.shown()?.editor.text()))
    }

    /// The page built for `stamp`. Dropped when another text replaced the one it was built
    /// from, or when a newer page is already here (builds can finish out of order). Returns
    /// `true` when an open block edit was closed by the new page.
    fn preview_ready(&mut self, stamp: Stamp, shown: docs::Shown) -> bool {
        let newer = (self.page.as_ref()).is_some_and(|page| page.stamp >= stamp);
        if stamp.0 != self.text_id || newer {
            return false;
        }
        self.page = Some(MdPreview {
            stamp,
            shown: Rc::new(shown),
        });
        self.preview_selection = None;
        self.block_edit.take().is_some()
    }

    /// A press on the preview at `at`: a caret, Shift extends, a double click takes the word.
    pub fn preview_click(&mut self, at: Point, extend: bool, clicks: usize) {
        let Some(shown) = self.preview().cloned() else {
            return;
        };
        self.preview_selection = Some(match self.preview_selection {
            _ if clicks == 2 => preview_select::word_at(&shown, at),
            Some(selection) if extend => Selection {
                head: at,
                ..selection
            },
            _ => Selection::caret(at),
        });
    }

    pub fn preview_drag(&mut self, at: Point) {
        if let Some(selection) = &mut self.preview_selection {
            selection.head = at;
        }
    }

    pub fn preview_select_all(&mut self) {
        self.preview_selection = self
            .preview()
            .and_then(|shown| preview_select::select_all(shown));
    }

    /// Ctrl+C in the preview: the selected text as shown; `None` when nothing is selected.
    pub fn preview_copy(&self) -> Option<String> {
        let text = preview_select::copy_text(self.preview()?, self.preview_selection.as_ref()?);
        (!text.is_empty()).then_some(text)
    }

    /// Esc: `true` when there was a selection.
    pub fn clear_preview_selection(&mut self) -> bool {
        self.preview_selection.take().is_some()
    }

    /// ✎ on block `block`: its source lines go into the field. Refused while the page is of an
    /// older text (its lines may have moved) or the file cannot be edited.
    pub fn open_block_edit(&mut self, block: usize) -> bool {
        let Some(stamp) = self.stamp() else {
            return false;
        };
        let current = self.page.as_ref().is_some_and(|page| page.stamp == stamp);
        let lines = self
            .preview()
            .and_then(|shown| shown.blocks.get(block))
            .map(|block| block.lines);
        let (Some(lines), true) = (lines, current) else {
            return false;
        };
        let Some(shown) = self.shown() else {
            return false;
        };
        let source = shown.editor.lines();
        let from = (lines.start as usize).saturating_sub(1);
        let to = (lines.end as usize).min(source.len());
        if from >= to {
            return false;
        }
        let mut text = TextBox::default();
        text.insert(&source[from..to].join("\n"));
        self.block_edit = Some(BlockEdit {
            block,
            lines,
            stamp,
            text,
        });
        true
    }

    /// A key in the field. Clipboard keys are the shell's: it reads the selection before Copy /
    /// Cut and inserts the clipboard on Paste.
    pub fn block_edit_key(&mut self, key: DialogKey) -> BlockEditOutcome {
        let Some(edit) = &mut self.block_edit else {
            return BlockEditOutcome::Cancel;
        };
        match key {
            DialogKey::Cancel => return BlockEditOutcome::Cancel,
            DialogKey::Save => return BlockEditOutcome::Apply,
            DialogKey::Enter => edit.text.insert("\n"),
            DialogKey::Insert(text) => edit.text.insert(&text),
            DialogKey::Cut => {
                if edit.text.input().selection().is_some() {
                    edit.text.edit(&DialogKey::Backspace);
                }
            }
            DialogKey::Paste | DialogKey::Copy | DialogKey::SwitchField => {}
            key => {
                edit.text.edit(&key);
            }
        }
        BlockEditOutcome::Edited
    }

    /// Ctrl+Enter: the field's text replaces the block's lines as one undo step and the field
    /// closes (the caller rebuilds the page). The field stays when the text changed since it
    /// opened.
    pub fn apply_block_edit(&mut self) -> Result<(), StaleEdit> {
        let Some(edit) = self.block_edit.take() else {
            return Ok(());
        };
        if self.stamp() != Some(edit.stamp) {
            self.block_edit = Some(edit);
            return Err(StaleEdit);
        }
        let Some(editor) = self.editor_mut() else {
            return Ok(());
        };
        let from = (edit.lines.start as usize).saturating_sub(1);
        let last = (edit.lines.end as usize)
            .min(editor.lines().len())
            .saturating_sub(1);
        let Some(end_col) = editor.lines().get(last).map(String::len) else {
            return Ok(());
        };
        editor.replace(
            Pos { line: from, col: 0 }..Pos {
                line: last,
                col: end_col,
            },
            edit.text.text(),
        );
        self.preview_selection = None;
        Ok(())
    }

    /// The editor of an editable file (not binary or too large).
    pub fn editor_mut(&mut self) -> Option<&mut Editor> {
        match &mut self.content {
            Content::Shown(shown) if !shown.code.binary && !shown.code.too_large => {
                Some(&mut shown.editor)
            }
            _ => None,
        }
    }

    /// The caret's line (1-based): "Open in Zed" and Send selection go there.
    pub fn line(&self) -> Option<u32> {
        let shown = self.shown()?;
        u32::try_from(shown.editor.cursor().line + 1).ok()
    }

    fn begin_save(&mut self) -> Option<SaveRequest> {
        if self.saving {
            return None;
        }
        let Content::Shown(shown) = &mut self.content else {
            return None;
        };
        if !shown.editor.is_dirty() {
            return None;
        }
        let expected_hash = shown.hash.clone()?;
        self.saving = true;
        Some(SaveRequest {
            path: self.path.clone(),
            text: shown.editor.text(),
            expected_hash,
            point: shown.editor.save_point(),
        })
    }

    fn saved(&mut self, point: Option<u64>, written: Result<FsWritten, String>) -> Option<String> {
        self.saving = false;
        let Content::Shown(shown) = &mut self.content else {
            return unshown_save(written);
        };
        match written {
            Ok(written) if written.saved => {
                shown.editor.mark_saved(point);
                shown.hash = Some(written.hash);
                shown.save_error = None;
                shown.disk_changed = None;
            }
            // Another version on disk: the banner offers Reload / Keep mine.
            Ok(written) => shown.disk_changed = Some(written.hash),
            Err(error) => shown.save_error = Some(format!("Not saved: {error}")),
        }
        None
    }

    /// "Don't save" / "Reload": drops the unsaved changes; the next read replaces the text.
    pub fn discard(&mut self) {
        if let Content::Shown(shown) = &mut self.content {
            let point = shown.editor.save_point();
            shown.editor.mark_saved(point);
            shown.disk_changed = None;
            // No version to keep: whatever is read next is shown.
            shown.hash = None;
        }
    }

    /// "Keep mine": the text stays and Save overwrites the version on disk.
    pub fn keep_mine(&mut self) {
        if let Content::Shown(shown) = &mut self.content
            && let Some(disk) = shown.disk_changed.take()
        {
            shown.hash = Some(disk);
        }
    }

    /// The text to highlight again once typing paused at `version`; `None` if it changed
    /// since (a later pause does it).
    pub fn highlight_request(&self, version: u64) -> Option<FsContent> {
        let shown = self.shown()?;
        (shown.editor.version() == version).then(|| FsContent {
            path: self.path.clone(),
            content: Some(shown.editor.text()),
            size: shown.code.size,
            binary: false,
            too_large: false,
            language: shown.language.clone(),
            hash: None,
            index_text: shown.index_text.clone(),
        })
    }

    /// The highlight of `version`; dropped (`false`) when the text changed meanwhile.
    pub fn highlighted(&mut self, version: u64, code: Code) -> bool {
        match &mut self.content {
            Content::Shown(shown) if shown.editor.version() == version => {
                shown.code = Rc::new(code);
                true
            }
            _ => false,
        }
    }
}

/// Answers of "Save changes to …?", in `window.prompt` order ("Cancel" is also Esc).
pub const LEAVE_ANSWERS: [&str; 3] = ["Save", "Don't save", "Cancel"];

/// What to do with unsaved changes before leaving them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leave {
    Save,
    DontSave,
}

/// The choice for an answer (an index into [`LEAVE_ANSWERS`]; `None` = the dialog went
/// away). Returns `None` to stay.
pub fn leave_choice(answer: Option<usize>) -> Option<Leave> {
    match answer {
        Some(0) => Some(Leave::Save),
        Some(1) => Some(Leave::DontSave),
        _ => None,
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
mod tests;
