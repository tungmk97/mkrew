//! The open files of a workspace (SPEC §12A, ADR 0108): one [`Doc`] per [`FileKey`] (path and
//! folder) in a [`Documents`] store per workspace, shared by every window. A [`Doc`] holds what
//! an open file is (text, editor, page, find bar, flags); where it is drawn is the layout's
//! `Leaf::File`. Pure and tested headlessly; `shell/places.rs` and `shell/file_pane.rs` do the
//! rest.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

pub use mkrew_core::settings::MdMode;
use mkrew_protocol::{FsContent, FsReadParams, FsWriteParams, FsWritten};
use ulid::Ulid;

use crate::docs;
use crate::editor::{Editor, Pos};
use crate::highlight::Code;
use crate::markdown::{self, Lines};
use crate::preview_select::{self, Point, Selection};
use crate::split::{Edge, FileKey, Leaf, PaneKey};
use crate::tabs::{Tab, Tabs};
use crate::task_dialog::DialogKey;
use crate::text_box::{Input, TextBox};
use crate::text_find::{self, TextFind};

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
    /// The file on disk changed under unsaved text: its hash ("Changed on disk: Reload ·
    /// Keep mine").
    pub disk_changed: Option<String>,
    /// Why the last save failed.
    pub save_error: Option<String>,
}

/// An `fs.write` of the opened file (SPEC §7): its text, the hash the file must still have
/// on disk, and the undo state it saves. `task` is the worktree the file is of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveRequest {
    pub path: String,
    pub task: Option<Ulid>,
    pub text: String,
    pub expected_hash: String,
    pub point: Option<u64>,
}

/// The `fs.write` of `save`: in the folder the file is of, never the Files tree's target.
pub fn write_params(project_id: Ulid, save: SaveRequest) -> FsWriteParams {
    FsWriteParams {
        project_id,
        worktree_task_id: save.task,
        path: save.path,
        content: save.text,
        expected_hash: save.expected_hash,
    }
}

/// The `fs.read` of `file`: in the folder it is of, never the Files tree's target.
pub fn read_params(project_id: Ulid, file: &FileKey) -> FsReadParams {
    FsReadParams {
        project_id,
        worktree_task_id: file.task,
        path: file.path.clone(),
        index_text: true,
    }
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

/// One open file (SPEC §12A, ADR 0080 as amended by ADR 0108).
#[derive(Debug)]
pub struct Doc {
    /// The file: its path (relative to the folder it is of, `/` separators) and that folder
    /// (`task`: a task worktree, `None` = the project folder). The leaf's key.
    pub file: FileKey,
    pub content: Content,
    /// `.md` files: the rendered preview, the editor, or both side by side. Starts as the
    /// leaf's `md`, else `[ui] md_mode`.
    pub mode: MdMode,
    /// The doc's [`docs::new_opening`] number: the page restarts when it changes.
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
    /// `git.changed` arrived while the doc was not drawn: read when it is.
    stale: bool,
    /// A (line, column), 1-based, that a link asked for, put under the caret once shown.
    pending_line: Option<(u32, u32)>,
    /// The find bar (SPEC §12A), `None` = closed; searched again whenever the text changes.
    pub find: Option<TextFind>,
}

/// A save's answer that has no file view to land in: only a failure or a refused save is news.
pub fn unshown_save(written: Result<FsWritten, String>) -> Option<String> {
    match written {
        Ok(written) if written.saved => None,
        Ok(_) => Some("Not saved: the file changed on disk meanwhile.".to_owned()),
        Err(error) => Some(format!("Not saved: {error}")),
    }
}

impl Doc {
    pub fn new(file: &FileKey, mode: MdMode) -> Doc {
        Doc {
            file: file.clone(),
            content: Content::Loading,
            mode,
            opening: docs::new_opening(),
            text_id: 0,
            page: None,
            page_asked: None,
            preview_selection: None,
            block_edit: None,
            saving: false,
            reading: false,
            read_again: false,
            stale: true,
            pending_line: None,
            find: None,
        }
    }

    /// The file name (the tab's title).
    pub fn name(&self) -> &str {
        self.file.path.rsplit('/').next().unwrap_or(&self.file.path)
    }

    /// The text has unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.shown().is_some_and(|shown| shown.editor.is_dirty())
    }

    /// An `fs.write` of this doc is in flight.
    pub fn is_saving(&self) -> bool {
        self.saving
    }

    /// Whether the doc must be read now: it was never drawn, or `git.changed` came while it
    /// was hidden. Clears the mark. A failed read is not read again by a draw (see `retry`).
    pub fn take_read(&mut self) -> bool {
        std::mem::take(&mut self.stale)
    }

    /// Retry after a failed read: shows `Loading` again. `true` = read it.
    pub fn retry(&mut self) -> bool {
        let failed = matches!(self.content, Content::Failed(_));
        if failed {
            self.content = Content::Loading;
        }
        failed
    }

    /// An `fs.read` is about to be sent; `false` = one is in flight (sent again once it
    /// answers).
    pub fn begin_read(&mut self) -> bool {
        if self.reading {
            self.read_again = true;
            return false;
        }
        self.reading = true;
        true
    }

    /// Nothing in flight any more (the connection was lost).
    pub fn reset_flight(&mut self) {
        self.saving = false;
        self.reading = false;
        self.read_again = false;
    }

    /// `git.changed` came while the doc was not drawn: read it when it is.
    pub fn mark_stale(&mut self) {
        self.stale = true;
    }

    /// The text number a page is built for ([`Documents::preview_ready`] finds the doc by it).
    pub fn text_id(&self) -> u64 {
        self.text_id
    }

    /// A link's line (and column): the caret goes there once the file is shown; the preview
    /// alone switches to the editor so the caret shows. `None` clears an earlier request.
    pub fn set_line(&mut self, line: Option<(u32, u32)>) {
        self.pending_line = line;
        if line.is_some() && self.mode == MdMode::Preview {
            self.mode = MdMode::Source;
        }
    }

    /// The `fs.read` answer; returns whether another read was asked for meanwhile. A failed
    /// read drops a link's line (a retry shows the file from the top).
    pub fn read(&mut self, read: Result<(FsContent, Code), String>) -> bool {
        self.reading = false;
        let again = std::mem::take(&mut self.read_again);
        if read.is_err() {
            self.pending_line = None;
        }
        self.show(read);
        self.search();
        again
    }

    /// Puts the caret on the (line, column) a link asked for, once the file is shown (call it
    /// after every read and after a request). Returns the 0-based line to scroll to, clamped
    /// to the text; `None` while nothing is shown or nothing is pending.
    pub fn apply_pending_line(&mut self) -> Option<usize> {
        let Content::Shown(shown) = &mut self.content else {
            return None;
        };
        let (line, col) = self.pending_line.take()?;
        let line = usize::try_from(line)
            .ok()?
            .saturating_sub(1)
            .min(shown.editor.lines().len().saturating_sub(1));
        let text: &str = &shown.editor.lines()[line];
        let mut col = usize::try_from(col).ok()?.saturating_sub(1).min(text.len());
        while !text.is_char_boundary(col) {
            col -= 1;
        }
        shown.editor.set_cursor(Pos::new(line, col), false);
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
        (self.file.path.rsplit_once('.')).is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("md"))
    }

    /// The `.md` file is shown as its rendered page alone, not as the editor.
    pub fn previewing(&self) -> bool {
        self.mode == MdMode::Preview && self.shows_md()
    }

    /// The `.md` file is shown as the editor and its rendered page side by side.
    pub fn splitting(&self) -> bool {
        self.mode == MdMode::Split && self.shows_md()
    }

    /// An editable `.md` file is shown.
    fn shows_md(&self) -> bool {
        self.is_markdown() && self.stamp().is_some()
    }

    /// Shows the file in `mode`; the preview's selection and block edit end with a change.
    pub fn set_mode(&mut self, mode: MdMode) {
        if self.mode != mode {
            self.mode = mode;
            self.preview_selection = None;
            self.block_edit = None;
        }
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

    /// The text to parse for the preview: `None` when no preview is shown, the page is current,
    /// or one is being built for this text (one at a time: the Split view rebuilds per edit,
    /// and an edit made meanwhile is asked for once that build lands). The shell parses it off
    /// the UI thread and answers with [`Documents::preview_ready`].
    pub fn preview_request(&mut self) -> Option<(Stamp, String)> {
        if !(self.previewing() || self.splitting()) {
            return None;
        }
        let stamp = self.stamp()?;
        let current = (self.page.as_ref()).is_some_and(|page| page.stamp == stamp);
        let building = self.page_asked.is_some_and(|asked| asked.0 == stamp.0);
        if current || building {
            return None;
        }
        self.page_asked = Some(stamp);
        Some((stamp, self.shown()?.editor.text()))
    }

    /// The page built for `stamp`. Dropped when another text replaced the one it was built
    /// from, or when a newer page is already here. Returns `true` when an open block edit was
    /// closed by the new page.
    pub fn preview_ready(&mut self, stamp: Stamp, shown: docs::Shown) -> bool {
        if self.page_asked == Some(stamp) {
            self.page_asked = None;
        }
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

    /// What "Send selection" sends from this tab (§20B.4): the file, first and last line and
    /// their text. In Preview the page's selection (the source lines of the blocks it touches,
    /// as Markdown); in Split the editor's own selection first, then the page's; else the
    /// editor's selected lines, else its caret's line. The editor's caret always answers, so
    /// the page is asked before it.
    pub fn picked_lines(&self) -> Option<(String, u32, u32, String)> {
        let editor = &self.shown()?.editor;
        let from_page = self.previewing() || (self.splitting() && editor.selection().is_none());
        let page = from_page.then(|| self.preview_lines()).flatten();
        if let Some((first, last, text)) = page {
            return Some((self.file.path.clone(), first, last, text));
        }
        // The selected lines, else the caret's line.
        let (first, last) = match editor.selection() {
            // A selection ending at a line's start does not include that line.
            Some(range) if range.end.col == 0 => (range.start.line, range.end.line - 1),
            Some(range) => (range.start.line, range.end.line),
            None => (editor.cursor().line, editor.cursor().line),
        };
        let text = editor.lines().get(first..=last)?.join("\n");
        let line = |index: usize| u32::try_from(index + 1).ok();
        Some((self.file.path.clone(), line(first)?, line(last)?, text))
    }

    /// The page's selection as source lines and their text; `None` without a selection, outside
    /// Preview and Split, or while the page is of an older text (its lines may have moved).
    fn preview_lines(&self) -> Option<(u32, u32, String)> {
        if !(self.previewing() || self.splitting()) {
            return None;
        }
        let current = self
            .page
            .as_ref()
            .is_some_and(|page| Some(page.stamp) == self.stamp());
        let page = self.preview().filter(|_| current)?;
        let lines = preview_select::source_lines(page, self.preview_selection.as_ref()?)?;
        let text = docs::source_text(&page.lines, lines.start, lines.end)?;
        Some((lines.start, lines.end, text))
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

    /// A click on the check box of the task item whose first source line (1-based) is `line`:
    /// ticks or unticks it in the text as one undo step; the caret stays. Refused while the page
    /// is of an older text (its lines may have moved); `Ok(false)` when the line holds no task
    /// marker.
    pub fn toggle_task(&mut self, line: u32) -> Result<bool, StaleEdit> {
        let stamp = self.stamp();
        if stamp.is_none() || self.page.as_ref().map(|page| page.stamp) != stamp {
            return Err(StaleEdit);
        }
        let Some(editor) = self.editor_mut() else {
            return Ok(false);
        };
        let at = (line as usize).saturating_sub(1);
        let Some((col, mark)) =
            (editor.lines().get(at)).and_then(|text| markdown::task_toggle(text))
        else {
            return Ok(false);
        };
        let caret = editor.cursor();
        let anchor = (editor.selection()).map(|s| if s.start == caret { s.end } else { s.start });
        editor.replace(
            Pos { line: at, col }..Pos {
                line: at,
                col: col + 1,
            },
            mark,
        );
        // One byte for one byte: the caret's and selection's places still exist.
        editor.set_cursor(anchor.unwrap_or(caret), false);
        editor.set_cursor(caret, anchor.is_some());
        self.preview_selection = None;
        Ok(true)
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

    pub fn begin_save(&mut self) -> Option<SaveRequest> {
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
            path: self.file.path.clone(),
            task: self.file.task,
            text: shown.editor.text(),
            expected_hash,
            point: shown.editor.save_point(),
        })
    }

    pub fn saved(
        &mut self,
        point: Option<u64>,
        written: Result<FsWritten, String>,
    ) -> Option<String> {
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
            path: self.file.path.clone(),
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

/// What the daemon answers for a file that is gone: `no file <path> in project <id>`.
/// The UI only sees the text, so this is the one place that reads it.
pub fn is_missing(error: &str) -> bool {
    error.contains("no file ") && error.contains(" in project ")
}

impl Doc {
    /// The last read said the file is gone (SPEC §9: "This file was removed.").
    pub fn removed(&self) -> bool {
        matches!(&self.content, Content::Failed(error) if is_missing(error))
    }
}

/// The open files of one workspace, by [`FileKey`] (`Leaf::File`'s path and task).
#[derive(Debug, Default)]
pub struct Documents {
    docs: HashMap<FileKey, Doc>,
}

impl Documents {
    pub fn new() -> Documents {
        Documents::default()
    }

    pub fn get(&self, file: &FileKey) -> Option<&Doc> {
        self.docs.get(file)
    }

    pub fn get_mut(&mut self, file: &FileKey) -> Option<&mut Doc> {
        self.docs.get_mut(file)
    }

    /// The doc of `file`, made (and left to be read on its first draw) when missing. An
    /// existing doc keeps its mode.
    pub fn ensure(&mut self, file: &FileKey, mode: MdMode) -> &mut Doc {
        (self.docs.entry(file.clone())).or_insert_with(|| Doc::new(file, mode))
    }

    /// Drops the docs no layout holds a leaf for, except those with unsaved text (asked about
    /// when the window closes or the app quits).
    pub fn retain_open(&mut self, open: &HashSet<FileKey>) {
        self.docs.retain(|file, doc| {
            open.contains(file) || doc.is_dirty() || doc.pending_line.is_some()
        });
    }

    pub fn clear(&mut self) {
        self.docs.clear();
    }

    /// Every doc with unsaved changes.
    pub fn unsaved_files(&self) -> Vec<FileKey> {
        let mut files: Vec<FileKey> = (self.docs.values())
            .filter(|doc| doc.is_dirty())
            .map(|doc| doc.file.clone())
            .collect();
        files.sort();
        files
    }

    /// The first of `files` that is still unsaved (asked again after a save that text typed
    /// meanwhile made unsaved again).
    pub fn first_unsaved(&self, files: &[FileKey]) -> Option<FileKey> {
        (files.iter())
            .find(|file| self.get(file).is_some_and(Doc::is_dirty))
            .cloned()
    }

    /// "Don't save" / "Reload": drops `file`'s unsaved changes; returns the file to read
    /// again, so the doc shows what is on disk.
    pub fn drop_changes(&mut self, file: &FileKey) -> Option<FileKey> {
        let doc = self.get_mut(file)?;
        doc.discard();
        Some(doc.file.clone())
    }

    /// The store as it carries over a lost connection (spec §8): the docs with unsaved text,
    /// kept with their hash and folder so Save still lands in the right one, nothing in
    /// flight, read again when drawn. `None` when nothing is unsaved.
    pub fn into_unsaved(mut self) -> Option<Documents> {
        self.docs.retain(|_, doc| doc.is_dirty());
        if self.docs.is_empty() {
            return None;
        }
        for doc in self.docs.values_mut() {
            doc.reset_flight();
            doc.mark_stale();
        }
        Some(self)
    }

    /// An `fs.read` of `file` is about to be sent; `false` = its doc is gone, or one is in
    /// flight already (sent again once that one answers).
    pub fn begin_read(&mut self, file: &FileKey) -> bool {
        self.get_mut(file).is_some_and(Doc::begin_read)
    }

    /// The `fs.read` answer (with its highlight) for `file` (dropped when the doc is gone).
    /// Returns the file to read again (asked for meanwhile).
    pub fn read(
        &mut self,
        file: &FileKey,
        read: Result<(FsContent, Code), String>,
    ) -> Option<FileKey> {
        let again = self.get_mut(file)?.read(read);
        again.then(|| file.clone())
    }

    /// Save `file` (Ctrl+S, "Save" in the unsaved-changes question). `None` when nothing is
    /// unsaved, the file cannot be saved, or a save is already in flight.
    pub fn begin_save(&mut self, file: &FileKey) -> Option<SaveRequest> {
        self.get_mut(file)?.begin_save()
    }

    /// The `fs.write` answer for `file`, which saved the state `point`. `Some(why)` when the
    /// answer has nowhere to show because its doc is gone: the shell toasts it.
    pub fn saved(
        &mut self,
        file: &FileKey,
        point: Option<u64>,
        written: Result<FsWritten, String>,
    ) -> Option<String> {
        match self.get_mut(file) {
            Some(doc) => doc.saved(point, written),
            None => unshown_save(written),
        }
    }

    /// The page built for `stamp`, for the doc whose text it was built from. Returns `true`
    /// when an open block edit was closed by the new page.
    pub fn preview_ready(&mut self, stamp: Stamp, shown: docs::Shown) -> bool {
        (self.docs.values_mut())
            .find(|doc| doc.text_id() == stamp.0)
            .is_some_and(|doc| doc.preview_ready(stamp, shown))
    }

    /// `git.changed` of the folder `task` (`None` = the project's): its docs in `shown` (drawn
    /// now) are read again, the others are marked stale and read when drawn; docs of other
    /// folders are left alone. Returns the files to read.
    pub fn refresh(&mut self, task: Option<Ulid>, shown: &[FileKey]) -> Vec<FileKey> {
        let mut reads = Vec::new();
        for (file, doc) in self.docs.iter_mut().filter(|(file, _)| file.task == task) {
            if shown.contains(file) {
                reads.push(file.clone());
            } else {
                doc.mark_stale();
            }
        }
        reads.sort();
        reads
    }
}

/// Where an opened file or the diff goes (SPEC §6.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// A single click in the Files tree: replaces the workspace's preview tab.
    Preview,
    /// A new tab right after the active one.
    NewTab,
    /// A drop: split that pane at that edge.
    Split(PaneKey, Edge),
}

/// What opening `key` does to the layouts.
#[derive(Debug, Clone, PartialEq)]
pub enum Resolution {
    /// It is open already, in whatever tab or window: only focus it.
    Focus(PaneKey),
    /// Replace the preview tab's leaf `old` with it, in place.
    Reuse {
        old: PaneKey,
    },
    /// A new tab at `position` (`None`: no tabs yet).
    NewTab {
        position: Option<f64>,
    },
    Split {
        target: PaneKey,
        edge: Edge,
    },
}

/// Decides where `key` goes. A key that is open anywhere is focused, never opened twice
/// (one pane per file per workspace). `Preview` reuses the preview tab only for a file, and
/// never one whose text is unsaved.
pub fn resolve_place(
    tabs: &Tabs,
    docs: Option<&Documents>,
    key: &PaneKey,
    place: &Place,
) -> Resolution {
    if tabs.tab_with(key).is_some() {
        return Resolution::Focus(key.clone());
    }
    let new_tab = || Resolution::NewTab {
        position: tabs.position_after_active(),
    };
    match place {
        Place::Preview if matches!(key, PaneKey::File(_)) => {
            match replaceable_preview(tabs, docs) {
                Some(old) => Resolution::Reuse { old },
                None => new_tab(),
            }
        }
        Place::Preview | Place::NewTab => new_tab(),
        Place::Split(target, edge) => Resolution::Split {
            target: target.clone(),
            edge: *edge,
        },
    }
}

/// The preview leaf a single click may replace: clean, in the workspace's preview tab.
fn replaceable_preview(tabs: &Tabs, docs: Option<&Documents>) -> Option<PaneKey> {
    let tab = tabs.preview_tab()?;
    let layout = tab.layout()?;
    tab.keys().into_iter().find(|key| {
        let PaneKey::File(file) = key else {
            return false;
        };
        matches!(layout.leaf_of(key), Some(Leaf::File { preview: true, .. }))
            && !docs
                .and_then(|docs| docs.get(file))
                .is_some_and(Doc::is_dirty)
    })
}

/// The file of every file leaf of `tabs` (all tabs, all windows).
pub fn open_files(tabs: &Tabs) -> HashSet<FileKey> {
    open_files_without(tabs, &[], &[])
}

/// `(workspace, worktree task)` of every file leaf of every workspace's `tabs` (all tabs, all
/// windows, shown or not): what the watch set keeps so that a hidden workspace's worktree
/// files stay current until their last file closes.
pub fn open_worktrees<'a>(
    all: impl IntoIterator<Item = (&'a Ulid, &'a Tabs)>,
) -> HashSet<(Ulid, Ulid)> {
    all.into_iter()
        .flat_map(|(&workspace, tabs)| {
            open_files(tabs)
                .into_iter()
                .filter_map(move |file| Some((workspace, file.task?)))
        })
        .collect()
}

/// [`open_files`] as it would be once the tabs `skip_tabs` and the leaves `skip_keys` are
/// gone: what a close leaves open.
pub fn open_files_without(
    tabs: &Tabs,
    skip_tabs: &[Ulid],
    skip_keys: &[PaneKey],
) -> HashSet<FileKey> {
    (tabs.tabs().iter())
        .filter(|tab| !skip_tabs.contains(&tab.id))
        .flat_map(Tab::keys)
        .filter(|key| !skip_keys.contains(key))
        .filter_map(|key| match key {
            PaneKey::File(file) => Some(file),
            _ => None,
        })
        .collect()
}

/// The files to ask "Save changes?" about when the open set goes from `before` to `after`:
/// those that left every layout with unsaved text. A move leaves the file in `after`, so it
/// never asks.
pub fn needs_save_question(
    before: &HashSet<FileKey>,
    after: &HashSet<FileKey>,
    docs: &Documents,
) -> Vec<FileKey> {
    let mut leaving: Vec<FileKey> = before
        .difference(after)
        .filter(|file| docs.get(file).is_some_and(Doc::is_dirty))
        .cloned()
        .collect();
    leaving.sort();
    leaving
}

/// The preview leaf becomes a kept one (an edit, a double click, a pin, a drag).
pub fn keep_leaf(leaf: &mut Leaf) {
    if let Leaf::File { preview, .. } = leaf {
        *preview = false;
    }
}

/// The leaf remembers the pane's Markdown mode across restarts.
pub fn set_leaf_mode(leaf: &mut Leaf, mode: MdMode) {
    if let Leaf::File { md, .. } = leaf {
        *md = Some(mode);
    }
}

impl Doc {
    /// Runs the find bar's query over the text again.
    pub fn search(&mut self) {
        if let (Some(find), Content::Shown(shown)) = (&mut self.find, &self.content) {
            find.search(shown.editor.lines());
        }
    }

    /// Ctrl+F / Ctrl+H: opens the find bar, with the replace row when `replace`. A selection
    /// on one line becomes the query, and the current match is the first one at the selection
    /// or caret. Returns `false` when nothing is shown.
    pub fn open_find(&mut self, replace: bool) -> bool {
        if !matches!(self.content, Content::Shown(_)) {
            return false;
        }
        // Matches and the replace row act on the editor's text: the preview alone hides it.
        if self.mode == MdMode::Preview {
            self.mode = MdMode::Source;
        }
        self.block_edit = None;
        self.preview_selection = None;
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
}

#[cfg(test)]
mod tests;
