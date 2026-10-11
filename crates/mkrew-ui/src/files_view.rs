//! Files view-model (SPEC §12A): a lazily listed tree of the workspace (`fs.list` per folder,
//! with git markers and an optional "show ignored"), the tree cursor moved by arrow keys, and
//! the opened file (`fs.read`, highlighted off the UI thread into a [`Code`]) in an [`Editor`],
//! saved with `fs.write`. `git.changed` re-lists the open folders and re-reads the shown file.
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
    /// A line (1-based) that a link asked for, put under the caret once the file is shown.
    pending_line: Option<u32>,
    /// `.md` files: the rendered preview or the editor (SPEC §12A). Another file starts in
    /// Preview.
    pub mode: Mode,
    /// The opened file's [`docs::new_opening`] number (0 before the first).
    pub opening: u64,
    /// Number of the text in the editor: new for every file and every re-read that replaced it.
    text_id: u64,
    /// The last page built, and the stamp last asked for (one build per text).
    preview: Option<MdPreview>,
    preview_asked: Option<Stamp>,
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
            pending_line: None,
            mode: Mode::Preview,
            opening: 0,
            text_id: 0,
            preview: None,
            preview_asked: None,
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
        if !same {
            self.opening = docs::new_opening();
        }
        self.file = Some(path.to_owned());
        self.content = Content::Loading;
        self.pending_line = None;
        self.mode = Mode::Preview;
        self.preview = None;
        self.preview_asked = None;
        Some(path.to_owned())
    }

    /// Opens file `path` from outside the tree (quick open, a terminal link): its folders open
    /// (listed as needed), the cursor moves to it and it shows in the viewer; with `line`, the
    /// caret goes there once the file is shown ([`Self::apply_pending_line`]).
    pub fn reveal(&mut self, path: &str, line: Option<u32>) -> Requests {
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
        let read = self.open(path);
        self.pending_line = line;
        // The caret of a link's line shows in the editor, not in the preview.
        if line.is_some() {
            self.mode = Mode::Source;
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

    /// Puts the caret on the line [`Self::reveal`] was given, once the file is shown (call it
    /// after every read and after `reveal`). Returns the 0-based line to scroll to, clamped to
    /// the text; `None` while nothing is shown or nothing is pending.
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
        if !matches!(self.content, Content::Shown(_)) || self.file.is_none() {
            return false;
        }
        // Matches and the replace row act on the editor's text.
        self.mode = Mode::Source;
        let Content::Shown(shown) = &self.content else {
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

    /// Whether the opened file is Markdown (`.md`).
    pub fn is_markdown(&self) -> bool {
        (self.file.as_deref())
            .and_then(|file| file.rsplit_once('.'))
            .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("md"))
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
        (self.preview.as_ref())
            .filter(|preview| preview.stamp.0 == self.text_id)
            .map(|preview| &preview.shown)
    }

    /// The text to parse for the preview, once per text: `None` when no preview is shown, the
    /// page is current, or one is being built for this text. The shell parses it off the UI
    /// thread and answers with [`Self::preview_ready`].
    pub fn preview_request(&mut self) -> Option<(Stamp, String)> {
        if !self.previewing() {
            return None;
        }
        let stamp = self.stamp()?;
        let current = (self.preview.as_ref()).is_some_and(|preview| preview.stamp == stamp);
        if current || self.preview_asked == Some(stamp) {
            return None;
        }
        self.preview_asked = Some(stamp);
        Some((stamp, self.shown()?.editor.text()))
    }

    /// The page built for `stamp`. Dropped when another text replaced the one it was built
    /// from, or when a newer page is already here (builds can finish out of order).
    pub fn preview_ready(&mut self, stamp: Stamp, shown: docs::Shown) {
        let newer = (self.preview.as_ref()).is_some_and(|preview| preview.stamp >= stamp);
        if stamp.0 != self.text_id || newer {
            return;
        }
        self.preview = Some(MdPreview {
            stamp,
            shown: Rc::new(shown),
        });
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
            git_base: None,
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
mod tests;
