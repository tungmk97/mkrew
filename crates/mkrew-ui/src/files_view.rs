//! Files view-model (SPEC §12A): a lazily listed tree of the workspace (`fs.list` per folder,
//! with git markers and an optional "show ignored"), the tree cursor moved by arrow keys, and
//! the opened file (`fs.read`, highlighted off the UI thread into a [`Code`]) in an [`Editor`],
//! saved with `fs.write`. `git.changed` re-lists the open folders and re-reads the shown file.
//! Pure and tested headlessly; `main.rs` does the requests and renders it.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;

use gpui::Keystroke;
use mkrew_protocol::{Event, FsContent, FsEntry, FsWritten};
use ulid::Ulid;

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

/// The opened file.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    /// No file opened.
    None,
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
    /// The opened file's path.
    pub file: Option<String>,
    pub content: Content,
    /// Why the last "Open in Zed" failed.
    pub zed_error: Option<String>,
    /// The find bar over the opened file (SPEC §12A), `None` = closed. Kept when another
    /// file opens; searched again whenever the content changes.
    pub find: Option<TextFind>,
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
    /// An `fs.write` is in flight.
    saving: bool,
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
            file: None,
            content: Content::None,
            saving: false,
            zed_error: None,
            find: None,
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

    /// Shows `path` in the viewer. Returns it when it has to be read: another file, or the
    /// same one whose read failed (a retry).
    pub fn open(&mut self, path: &str) -> Option<String> {
        let same = self.file.as_deref() == Some(path);
        if same && !matches!(self.content, Content::Failed(_)) {
            return None;
        }
        self.file = Some(path.to_owned());
        self.content = Content::Loading;
        Some(path.to_owned())
    }

    /// Opens file `path` from outside the tree (quick open): its folders open (listed as
    /// needed), the cursor moves to it and it shows in the viewer.
    pub fn reveal(&mut self, path: &str) -> Requests {
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
        Requests {
            list,
            read: self.open(path),
        }
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

    /// The `fs.read` answer (with its highlight) for `path`; shown if `path` is still the
    /// opened file. Returns the file to read again (asked for meanwhile).
    pub fn read(&mut self, path: &str, read: Result<(FsContent, Code), String>) -> Option<String> {
        self.reading = false;
        let again = std::mem::take(&mut self.read_again);
        if self.file.as_deref() == Some(path) {
            self.show(read);
        }
        if again { self.file.clone() } else { None }
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
            }
            return;
        }
        self.content = match read {
            Err(error) => Content::Failed(error),
            // The same version again (our own save, a change elsewhere): caret and undo stay.
            Ok((content, _)) if hash.is_some() && content.hash == hash => return,
            Ok((content, code)) => {
                let mut editor = Editor::new(content.content.as_deref().unwrap_or(""));
                if let Some(cursor) = cursor {
                    editor.set_cursor(cursor, false);
                }
                Content::Shown(Box::new(Shown {
                    code: Rc::new(code),
                    editor,
                    hash: content.hash,
                    language: content.language,
                    disk_changed: None,
                    save_error: None,
                }))
            }
        };
        self.search();
    }

    /// Runs the find bar's query over the opened file's text again.
    pub fn search(&mut self) {
        if let (Some(find), Content::Shown(shown)) = (&mut self.find, &self.content) {
            find.search(shown.editor.lines());
        }
    }

    /// Ctrl+F / Ctrl+H: opens the find bar, with the replace row when `replace`. A selection
    /// on one line becomes the query, and the current match is the first one at the selection
    /// or caret. Returns `false` when no file is open.
    pub fn open_find(&mut self, replace: bool) -> bool {
        let Content::Shown(shown) = &self.content else {
            return false;
        };
        if self.file.is_none() {
            return false;
        }
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

    /// The find bar and the editor of an editable file (not binary or too large).
    fn find_and_editor(&mut self) -> Option<(&mut TextFind, &mut Editor)> {
        match (&mut self.find, &mut self.content) {
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

    pub fn shown(&self) -> Option<&Shown> {
        match &self.content {
            Content::Shown(shown) => Some(shown),
            _ => None,
        }
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

    /// The opened file while it has unsaved changes.
    pub fn unsaved(&self) -> Option<&str> {
        self.shown()
            .filter(|shown| shown.editor.is_dirty())
            .and(self.file.as_deref())
    }

    /// The caret's line (1-based): "Open in Zed" and Send selection go there.
    pub fn line(&self) -> Option<u32> {
        let shown = self.shown()?;
        u32::try_from(shown.editor.cursor().line + 1).ok()
    }

    /// Save (Ctrl+S). `None` when nothing is unsaved, the file cannot be saved, or a save is
    /// already in flight.
    pub fn begin_save(&mut self) -> Option<SaveRequest> {
        if self.saving {
            return None;
        }
        let path = self.file.clone()?;
        let Content::Shown(shown) = &mut self.content else {
            return None;
        };
        if !shown.editor.is_dirty() {
            return None;
        }
        let expected_hash = shown.hash.clone()?;
        self.saving = true;
        Some(SaveRequest {
            path,
            text: shown.editor.text(),
            expected_hash,
            point: shown.editor.save_point(),
        })
    }

    /// The `fs.write` answer for `path`, which saved the state `point`.
    pub fn saved(&mut self, path: &str, point: Option<u64>, written: Result<FsWritten, String>) {
        self.saving = false;
        if self.file.as_deref() != Some(path) {
            return;
        }
        let Content::Shown(shown) = &mut self.content else {
            return;
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
    }

    /// The row Enter opens: the cursor's, while that row is still listed (an agent may have
    /// deleted it, or the ignored rows were hidden).
    pub fn enter_target(&self) -> Option<String> {
        let cursor = self.cursor.as_ref()?;
        self.entry(cursor).map(|_| cursor.clone())
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

    /// "Reload": drops the unsaved changes; returns the file to read again.
    pub fn reload(&mut self) -> Option<String> {
        self.discard();
        self.file.clone()
    }

    /// "Keep mine": the text stays and Save overwrites the version on disk.
    pub fn keep_mine(&mut self) {
        if let Content::Shown(shown) = &mut self.content
            && let Some(disk) = shown.disk_changed.take()
        {
            shown.hash = Some(disk);
        }
    }

    /// Whether opening `path` (a click, Enter, quick open) leaves the unsaved changes of
    /// another file.
    pub fn leaves_unsaved(&self, path: &str) -> bool {
        self.unsaved().is_some_and(|file| file != path)
            && self.entry(path).is_none_or(|entry| !entry.is_dir)
    }

    /// The text to highlight again once typing paused at `version`; `None` if it changed
    /// since (a later pause does it).
    pub fn highlight_request(&self, version: u64) -> Option<FsContent> {
        let (Some(path), Some(shown)) = (&self.file, self.shown()) else {
            return None;
        };
        (shown.editor.version() == version).then(|| FsContent {
            path: path.clone(),
            content: Some(shown.editor.text()),
            size: shown.code.size,
            binary: false,
            too_large: false,
            language: shown.language.clone(),
            hash: None,
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
        self.relayout();
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
    /// opened) and the opened file is read. Also used when the view is shown again, since
    /// hidden views do not follow `git.changed`.
    pub fn refresh(&mut self) -> Requests {
        self.generation += 1;
        let open = self.open_dirs();
        self.dirs
            .retain(|dir, listing| open.contains(dir) || matches!(listing, Listing::Loading));
        self.relayout();
        Requests {
            list: open,
            read: self.file.clone(),
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

    use crate::editor::EditKey;
    use mkrew_protocol::FsWritten;

    fn file_text_at(path: &str, text: &str) -> Result<(FsContent, Code), String> {
        let content = FsContent {
            path: path.into(),
            content: Some(text.into()),
            size: text.len() as u64,
            binary: false,
            too_large: false,
            language: None,
            hash: Some(format!("#{text}")),
        };
        Ok((content, Code::plain(text)))
    }

    fn file_text(text: &str) -> Result<(FsContent, Code), String> {
        file_text_at("README.md", text)
    }

    /// A view showing README.md as read with `text`.
    fn opened(text: &str) -> FilesView {
        let mut view = listed();
        view.open("README.md");
        view.begin_read();
        view.read("README.md", file_text(text));
        view
    }

    fn text(view: &FilesView) -> String {
        view.shown()
            .map(|shown| shown.editor.text())
            .unwrap_or_default()
    }

    fn paths(view: &FilesView) -> Vec<String> {
        view.rows()
            .iter()
            .map(|row| match row {
                Row::Entry {
                    entry,
                    depth,
                    expanded,
                } => format!(
                    "{}{}{}",
                    "  ".repeat(*depth),
                    entry.path,
                    if *expanded { "/" } else { "" }
                ),
                Row::Note { depth, text } => format!("{}({text})", "  ".repeat(*depth)),
            })
            .collect()
    }

    /// A view with `src/` (holding `src/main.rs`) and `README.md` listed.
    fn listed() -> FilesView {
        let mut view = FilesView::new(Ulid::new(), None);
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
    fn revealing_a_file_opens_its_folders_and_shows_it() {
        let mut view = listed();
        let revealed = view.reveal("src/app/main.rs");
        assert_eq!(revealed.list, ["src", "src/app"]);
        assert_eq!(revealed.read.as_deref(), Some("src/app/main.rs"));
        assert_eq!(view.cursor.as_deref(), Some("src/app/main.rs"));
        // The deeper folder may answer first.
        assert!(view.begin_list("src/app"));
        assert!(view.begin_list("src"));
        view.listed("src/app", Ok(vec![file("src/app/main.rs", None)]));
        assert!(view.listed("src", Ok(vec![dir("src/app")])).is_empty());
        assert_eq!(
            paths(&view),
            ["src/", "  src/app/", "    src/app/main.rs", "README.md"]
        );
        // Again: everything is open and listed, the file is already shown.
        assert_eq!(view.reveal("src/app/main.rs"), Requests::default());
        // A file at the root opens no folder.
        let root = view.reveal("README.md");
        assert!(root.list.is_empty());
        assert_eq!(root.read.as_deref(), Some("README.md"));
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
        let mut view = FilesView::new(Ulid::new(), None);
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
        let again = view.read("README.md", file_text("old"));
        assert_eq!(again.as_deref(), Some("src/main.rs"));
        assert_eq!(view.content, Content::Loading);
        assert!(view.begin_read());
        assert_eq!(view.read("src/main.rs", Err("too large".into())), None);
        assert_eq!(view.content, Content::Failed("too large".into()));
        assert_eq!(view.open("src/main.rs").as_deref(), Some("src/main.rs"));
        assert!(view.begin_read());
        view.read("src/main.rs", file_text_at("src/main.rs", "a\nb"));
        assert_eq!(
            view.shown().map(|shown| shown.editor.lines().len()),
            Some(2)
        );
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
        view.read("README.md", file_text("x"));
        let changed = |project_id| {
            Event::GitChanged(GitChangedEvent {
                project_id,
                worktree_task_id: None,
            })
        };
        assert_eq!(view.apply(&changed(Ulid::new())), Requests::default());
        let in_worktree = Event::GitChanged(GitChangedEvent {
            project_id: view.project_id,
            worktree_task_id: Some(Ulid::new()),
        });
        assert_eq!(view.apply(&in_worktree), Requests::default());
        let requests = view.apply(&changed(view.project_id));
        assert_eq!(requests.list, ["", "src"]);
        assert_eq!(requests.read.as_deref(), Some("README.md"));
        // A view of that worktree follows its changes, not the project folder's.
        let mut worktree = FilesView::new(view.project_id, Some(Ulid::new()));
        worktree.begin_list("");
        assert_eq!(worktree.apply(&in_worktree), Requests::default());
        assert_eq!(
            worktree.apply(&changed(view.project_id)),
            Requests::default()
        );
        let Event::GitChanged(mut own) = in_worktree else {
            unreachable!()
        };
        own.worktree_task_id = worktree.worktree_task_id;
        assert_eq!(worktree.apply(&Event::GitChanged(own)).list, [""]);
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
        let mut view = FilesView::new(Ulid::new(), None);
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
        let mut view = FilesView::new(Ulid::new(), None);
        let changed = Event::GitChanged(GitChangedEvent {
            project_id: view.project_id,
            worktree_task_id: None,
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

    #[test]
    fn find_follows_a_reread_file() {
        let mut view = listed();
        view.open("README.md");
        view.begin_read();
        view.read("README.md", file_text("todo\nok"));
        let mut find = TextFind::default();
        find.query = "todo".into();
        view.find = Some(find);
        view.search();
        let label = |view: &FilesView| view.find.as_ref().map(TextFind::label);
        assert_eq!(label(&view).as_deref(), Some("1 of 1"));
        // An agent edits the file: the re-read is searched again.
        view.begin_read();
        view.read("README.md", file_text("todo\ntodo"));
        assert_eq!(label(&view).as_deref(), Some("1 of 2"));
    }

    #[test]
    fn find_opens_seeded_with_a_one_line_selection_at_the_caret() {
        let mut view = opened("let a = b;\nlet b = a;\n");
        view.editor_mut()
            .unwrap()
            .select(Pos::new(1, 4)..Pos::new(1, 5));
        assert!(view.open_find(false));
        let find = view.find.as_ref().unwrap();
        assert_eq!((find.query.as_str(), find.replace.as_deref()), ("b", None));
        let at = find.current().map(|m| (m.line, m.range.start));
        assert_eq!(at, Some((1, 4)), "the selected one is current");
        // Ctrl+H adds the replace row; a multi-line selection keeps the query.
        view.editor_mut()
            .unwrap()
            .select(Pos::new(0, 0)..Pos::new(1, 1));
        assert!(view.open_find(true));
        let find = view.find.as_ref().unwrap();
        assert_eq!(
            (find.query.as_str(), find.replace.as_deref()),
            ("b", Some(""))
        );
        // Without a file there is nothing to find in.
        assert!(!FilesView::new(Ulid::new(), None).open_find(false));
    }

    #[test]
    fn replace_one_moves_on_and_replace_all_is_one_undo_step() {
        let mut view = opened("a1 A2\na3\n");
        assert!(view.open_find(true));
        let find = view.find.as_mut().unwrap();
        find.query = "a".into();
        find.replace = Some("aa".into());
        view.search();
        assert!(view.replace_one());
        assert_eq!(text(&view), "aa1 A2\na3\n");
        let at = (view.find.as_ref())
            .and_then(TextFind::current)
            .map(|m| (m.line, m.range.start));
        assert_eq!(
            at,
            Some((0, 4)),
            "the next match, not one inside the replacement"
        );
        assert!(view.replace_one());
        assert!(view.replace_one());
        assert_eq!(text(&view), "aa1 aa2\naa3\n");
        // Replace all: each match once (the replacement holds the query), ignoring case,
        // in UTF-8; one undo step; not saved.
        let mut view = opened("Tiếng việt, VIỆT\nviệt\n");
        view.open_find(true);
        let find = view.find.as_mut().unwrap();
        find.query = "việt".into();
        find.replace = Some("Nam việt".into());
        assert_eq!(view.replace_all(), 3);
        assert_eq!(text(&view), "Tiếng Nam việt, Nam việt\nNam việt\n");
        assert_eq!(view.unsaved(), Some("README.md"));
        let label = view.find.as_ref().map(TextFind::label);
        assert_eq!(label.as_deref(), Some("1 of 3"));
        assert!(view.editor_mut().unwrap().undo());
        assert_eq!(text(&view), "Tiếng việt, VIỆT\nviệt\n");
        assert_eq!(view.unsaved(), None);
        // Without the replace row, or a match, nothing is replaced (no empty undo step).
        view.find.as_mut().unwrap().replace = None;
        assert_eq!(view.replace_all(), 0);
        assert!(!view.replace_one());
        let find = view.find.as_mut().unwrap();
        (find.query, find.replace) = ("zzz".into(), Some(Input::default()));
        assert_eq!(view.replace_all(), 0);
        assert!(!view.editor_mut().unwrap().undo());
    }

    #[test]
    fn a_reread_follows_the_disk_only_while_nothing_is_unsaved() {
        let mut view = opened("a\nbc\n");
        view.editor_mut().unwrap().set_cursor(Pos::new(1, 2), false);
        assert_eq!(view.line(), Some(2));
        // The same text again (our own save, another file changed): caret and undo stay.
        view.editor_mut().unwrap().insert("x");
        view.editor_mut().unwrap().undo();
        view.begin_read();
        view.read("README.md", file_text("a\nbc\n"));
        assert!(
            view.editor_mut().unwrap().redo(),
            "the undo history survived"
        );
        view.editor_mut().unwrap().undo();
        // An agent shortened it: the new text shows, the caret stays where it can.
        view.begin_read();
        view.read("README.md", file_text("a\n"));
        assert_eq!((text(&view), view.line()), ("a\n".to_owned(), Some(1)));
        // Unsaved text is never replaced: not by a change on disk, not by a failed read.
        view.editor_mut().unwrap().insert("mine ");
        for read in [file_text("theirs\n"), Err("gone".into())] {
            view.begin_read();
            view.read("README.md", read);
            assert_eq!(text(&view), "amine \n");
        }
        assert_eq!(view.unsaved(), Some("README.md"));
        // Opening another file is still possible (the shell asks first, Task 6).
        assert_eq!(view.open("src/main.rs").as_deref(), Some("src/main.rs"));
        assert_eq!(view.unsaved(), None);
    }

    #[test]
    fn a_save_marks_what_it_sent_and_one_runs_at_a_time() {
        let mut view = opened("a\n");
        assert_eq!(view.begin_save(), None, "nothing to save");
        view.editor_mut().unwrap().insert("b");
        let save = view.begin_save().unwrap();
        assert_eq!(
            (save.text.as_str(), save.expected_hash.as_str()),
            ("ba\n", "#a\n")
        );
        assert_eq!(view.begin_save(), None, "one save at a time");
        // Typed while it was saving: still unsaved afterwards.
        view.editor_mut()
            .unwrap()
            .key(EditKey::Insert("c".into()), 10);
        let written = FsWritten {
            saved: true,
            hash: "#ba\n".into(),
        };
        view.saved("README.md", save.point, Ok(written));
        assert_eq!(view.unsaved(), Some("README.md"));
        let save = view.begin_save().unwrap();
        assert_eq!(
            save.expected_hash, "#ba\n",
            "the saved text is the new base"
        );
        let written = FsWritten {
            saved: true,
            hash: "#bca\n".into(),
        };
        view.saved("README.md", save.point, Ok(written));
        assert_eq!(view.unsaved(), None);
        // Our own save comes back through the watcher: nothing changes.
        view.begin_read();
        view.read("README.md", file_text("bca\n"));
        assert_eq!(text(&view), "bca\n");
        // A failed save says why and keeps the text.
        view.editor_mut().unwrap().insert("d");
        let save = view.begin_save().unwrap();
        view.saved("README.md", save.point, Err("disk full".into()));
        let error = view.shown().and_then(|shown| shown.save_error.clone());
        assert_eq!(error.as_deref(), Some("Not saved: disk full"));
        assert_eq!(view.unsaved(), Some("README.md"));
    }

    #[test]
    fn a_highlight_is_kept_only_for_the_text_it_was_made_of() {
        let mut view = opened("a\n");
        view.editor_mut().unwrap().insert("b");
        let version = view.shown().unwrap().editor.version();
        let request = view.highlight_request(version).unwrap();
        assert_eq!(request.content.as_deref(), Some("ba\n"));
        view.editor_mut().unwrap().insert("c");
        assert_eq!(view.highlight_request(version), None, "typing went on");
        assert!(!view.highlighted(version, Code::plain("ba\n")), "stale");
        let version = view.shown().unwrap().editor.version();
        assert!(view.highlighted(version, Code::plain("bca\n")));
    }

    #[test]
    fn a_change_on_disk_under_unsaved_text_offers_reload_or_keep_mine() {
        let mut view = opened("one\n");
        view.editor_mut().unwrap().insert("x");
        let reread = |view: &mut FilesView, text: &str| {
            view.begin_read();
            view.read("README.md", file_text(text));
        };
        let disk = |view: &FilesView| view.shown().and_then(|shown| shown.disk_changed.clone());
        reread(&mut view, "one\n");
        assert_eq!(disk(&view), None, "the same version: nothing to say");
        reread(&mut view, "two\n");
        assert_eq!(
            (text(&view), disk(&view)),
            ("xone\n".to_owned(), Some("#two\n".to_owned()))
        );
        reread(&mut view, "one\n");
        assert_eq!(disk(&view), None, "the agent undid its change");
        // Keep mine: Save overwrites the disk's version.
        reread(&mut view, "two\n");
        view.keep_mine();
        assert_eq!(disk(&view), None);
        let save = view.begin_save().unwrap();
        assert_eq!(save.expected_hash, "#two\n");
        // The save meets yet another version: the banner again, the text stays.
        let written = FsWritten {
            saved: false,
            hash: "#three\n".into(),
        };
        view.saved("README.md", save.point, Ok(written));
        assert_eq!(
            (text(&view), disk(&view)),
            ("xone\n".to_owned(), Some("#three\n".to_owned()))
        );
        // Reload: the disk's text replaces it.
        assert_eq!(view.reload().as_deref(), Some("README.md"));
        reread(&mut view, "three\n");
        assert_eq!(
            (text(&view), view.unsaved(), disk(&view)),
            ("three\n".to_owned(), None, None)
        );
    }

    #[test]
    fn only_leaving_unsaved_text_for_another_file_asks() {
        let mut view = opened("a\n");
        assert!(!view.leaves_unsaved("src/main.rs"), "nothing unsaved");
        view.editor_mut().unwrap().insert("b");
        assert!(
            view.leaves_unsaved("src/main.rs"),
            "quick open: not a listed row"
        );
        assert!(!view.leaves_unsaved("README.md"), "the same file");
        assert!(!view.leaves_unsaved("src"), "a folder only opens");
        view.discard();
        assert!(!view.leaves_unsaved("src/main.rs"), "Don't save");
        assert_eq!(leave_choice(Some(0)), Some(Leave::Save));
        assert_eq!(leave_choice(Some(1)), Some(Leave::DontSave));
        assert_eq!(leave_choice(Some(2)), None, "Cancel");
        assert_eq!(leave_choice(None), None, "Esc / dialog closed");
    }

    #[test]
    fn a_successful_save_clears_the_changed_on_disk_banner() {
        let mut view = opened("one\n");
        view.editor_mut().unwrap().insert("x");
        view.begin_read();
        view.read("README.md", file_text("two\n"));
        assert!(view.shown().unwrap().disk_changed.is_some());
        let save = view.begin_save().unwrap();
        let written = FsWritten {
            saved: true,
            hash: "#xone\n".into(),
        };
        view.saved("README.md", save.point, Ok(written));
        let shown = view.shown().unwrap();
        assert_eq!(shown.disk_changed, None);
        assert_eq!(shown.hash.as_deref(), Some("#xone\n"));
    }

    #[test]
    fn enter_opens_the_cursor_row_only_while_it_is_listed() {
        let mut view = opened("a\n");
        assert_eq!(view.enter_target(), None, "no cursor yet");
        view.cursor = Some("README.md".into());
        assert_eq!(view.enter_target().as_deref(), Some("README.md"));
        view.cursor = Some("gone.txt".into());
        assert_eq!(view.enter_target(), None, "the row is gone");
    }
}
