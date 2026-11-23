//! Files view-model (SPEC §12A): a lazily listed tree of the workspace (`fs.list` per folder,
//! with git markers and an optional "show ignored"), the tree cursor moved by arrow keys, and
//! the opened files as tabs ([`Doc`], ADR 0080: `fs.read`, highlighted off the UI thread
//! into a [`Code`], in an [`Editor`], saved with `fs.write`). `git.changed` re-lists the open
//! folders and re-reads the active tab; the others are read when activated.
//! Pure and tested headlessly; `main.rs` does the requests and renders it.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;

use gpui::Keystroke;
use mkrew_protocol::{Event, FsContent, FsEntry, FsWritten};
use ulid::Ulid;

use crate::docs;
pub use crate::documents::MdMode;
use crate::documents::{Content, Doc, SaveRequest, Shown, Stamp, unshown_save};
use crate::editor::{Editor, Pos};
use crate::highlight::Code;
use crate::text_box::Input;
use crate::text_find::{self, TextFind};

/// One folder's children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listing {
    Loading,
    Loaded(Vec<FsEntry>),
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

/// The source's share of the Markdown Split view that `[ui] md_split` (percent) stands for,
/// kept between the limits.
pub fn md_split_share(percent: u8) -> f32 {
    split_share(f32::from(percent) / 100.0)
}

/// The share while the divider is dragged to window x `pointer_x` over halves `width` wide
/// starting at `left`.
pub fn md_split_share_at(left: f32, width: f32, pointer_x: f32) -> f32 {
    if width <= 0.0 {
        return md_split_share(mkrew_core::settings::MD_SPLIT_DEFAULT);
    }
    split_share((pointer_x - left) / width)
}

fn split_share(share: f32) -> f32 {
    share.clamp(
        crate::theme::MD_SPLIT_MIN_SHARE,
        crate::theme::MD_SPLIT_MAX_SHARE,
    )
}

/// How a file opens (SPEC §12A, ADR 0080).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Open {
    /// A single click, Enter, quick open, a link: the transient (italic) tab, replaced by the
    /// next one while it is clean.
    Transient,
    /// A double click or a new file: a permanent tab.
    Kept,
}

/// Which tabs a close asks for (tab menu; pinned tabs stay, except for `One` and `Every`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Close {
    One,
    Others,
    Right,
    All,
    /// Every tab, pinned or not (the Editor chip's ✕).
    Every,
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
    tabs: Vec<Doc>,
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
    /// The mode a newly opened Markdown tab starts in (`[ui] md_mode`, the last one switched to).
    pub md_mode: MdMode,
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
            md_mode: MdMode::default(),
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

    pub fn tabs(&self) -> &[Doc] {
        &self.tabs
    }

    pub fn active_index(&self) -> Option<usize> {
        self.active
    }

    /// The active tab.
    pub fn tab(&self) -> Option<&Doc> {
        self.tabs.get(self.active?)
    }

    pub fn tab_mut(&mut self) -> Option<&mut Doc> {
        self.tabs.get_mut(self.active?)
    }

    pub fn tab_for(&self, path: &str) -> Option<&Doc> {
        self.tabs.iter().find(|tab| tab.path == path)
    }

    pub fn tab_for_mut(&mut self, path: &str) -> Option<&mut Doc> {
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

    /// The view as it carries over a lost connection (spec §8): the tabs with unsaved text, kept
    /// with their hash so Save still works, and nothing in flight; the tree and every clean
    /// tab are read again. `None` when no tab is unsaved.
    pub fn into_unsaved(mut self) -> Option<FilesView> {
        let shown = self.file().map(str::to_owned);
        self.tabs.retain(Doc::is_dirty);
        if self.tabs.is_empty() {
            return None;
        }
        for tab in &mut self.tabs {
            tab.reset_flight();
        }
        self.active = shown
            .and_then(|path| self.tabs.iter().position(|tab| tab.path == path))
            .or(Some(0));
        self.dirs.clear();
        self.listing.clear();
        self.list_again.clear();
        self.sent_in.clear();
        self.generation += 1;
        self.relayout();
        Some(self)
    }

    /// The first of `paths` whose tab is still unsaved (asked again after a save that text
    /// typed meanwhile made unsaved again).
    pub fn first_unsaved(&self, paths: &[String]) -> Option<String> {
        (paths.iter())
            .find(|path| self.tab_for(path).is_some_and(Doc::is_dirty))
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
        let mut tab = Doc::new(path, self.md_mode);
        tab.transient = how == Open::Transient;
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

    /// The tabs a close of `path`'s tab asks for (pinned tabs stay, except for `One` and `Every`).
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
                Close::Every => true,
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

    /// The active tab's pin group and place in it (see `Tabs::active_place`).
    pub fn active_place(&self) -> Option<(bool, usize, usize)> {
        let active = self.tab()?;
        let group: Vec<&str> = (self.tabs.iter())
            .filter(|tab| tab.pinned == active.pinned)
            .map(|tab| tab.path.as_str())
            .collect();
        let at = group.iter().position(|path| *path == active.path)?;
        Some((active.pinned, at, group.len()))
    }

    /// Opens file `path` from outside the tree (quick open, a terminal link): its folders open
    /// (listed as needed), the cursor moves to it and its tab shows; with `line`, the caret
    /// goes there once the file is shown ([`Doc::apply_pending_line`]).
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
            tab.set_line(line.map(|line| (line, 1)));
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
        self.tab_for_mut(path).is_some_and(Doc::begin_read)
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
            .find(|tab| tab.text_id() == stamp.0)
            .is_some_and(|tab| tab.preview_ready(stamp, shown))
    }

    /// Runs the find bar's query over the active tab's text again.
    pub fn search(&mut self) {
        let shown = (self.active)
            .and_then(|at| self.tabs.get(at))
            .and_then(Doc::shown);
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
        // Matches and the replace row act on the editor's text: the preview alone hides it.
        if tab.mode == MdMode::Preview {
            tab.mode = MdMode::Source;
        }
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
            if Some(at) != active {
                tab.mark_stale();
            }
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
