# Files view tabs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Files view keeps several opened files as tabs (VS Code style transient tab, close, pin, reorder, keys) instead of replacing the shown file.

**Architecture:** The per-file state of `FilesView` moves into a new `FileTab` struct; `FilesView`
keeps the tree, the shared find bar and `tabs: Vec<FileTab>` + `active: Option<usize>`. Every
daemon answer (`fs.read`, `fs.write`, highlight, Markdown page) is routed to its tab by path (or
text id), never to "the opened file". The shell gets a tab strip above the file header, a tab
menu, reorder via the existing `shell/reorder.rs`, and routes the tab keys to file tabs while the
Files view is shown.

**Tech Stack:** Rust, GPUI (`gpui-pre` 0.3.7), existing `mkrew-ui` crate only. No protocol,
storage or daemon change.

**Spec:** `docs/superpowers/specs/2026-10-26-files-tabs-design.md`

## Global Constraints

- Every color, spacing, radius and font size comes from `crates/mkrew-ui/src/theme.rs` (AGENTS.md, SPEC §14). No hex or raw pixel literal in components.
- No `unwrap`/`expect` outside tests unless provably infallible (comment why).
- `cargo fmt --all`; `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Tabs live in memory only; a new worktree (`FilesView::new`) starts with no tabs.
- Unsaved text is never replaced or dropped without "Save / Don't save / Cancel" (SPEC §2.5).
- New key: `ctrl-w` = `CloseFileTab`, context `FILES_CONTEXT` only.
- ADR number `0080` (re-check `docs/decisions/` for the next free number before writing it).
- Verify with `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL).
- Commit with `git -c user.email=tungnv975@gmail.com commit`; stage only files you changed; Conventional Commits.

## Review Focus

1. A read answer for a tab that was closed or replaced while it was in flight must be dropped, not shown in another tab — test in Task 1 (`a_read_for_a_closed_tab_is_dropped`).
2. Two tabs editing at once: typing in tab B while tab A's highlight timer runs must not paint A's highlight onto B (versions are per editor and can collide) — highlight is routed by path, test in Task 1 (`a_highlight_lands_in_its_own_tab`).
3. Closing several tabs where one is unsaved and the user picks Cancel: nothing closes — exercised in Task 3 smoke step.
4. `git.changed` while several tabs are open: only the active tab is read; switching to another reads it once — test in Task 1 (`git_changed_reads_the_active_tab_and_marks_the_others_stale`).
5. A transient tab with unsaved text must never be replaced by the next single click — test in Task 1 (`a_dirty_transient_tab_is_never_replaced`).

---

### Task 1: `FileTab` model in `files_view.rs`

**Files:**
- Modify: `crates/mkrew-ui/src/files_view.rs`
- Modify: `crates/mkrew-ui/src/files_view/tests.rs`

**Interfaces:**
- Produces (used by Tasks 2–4):
  - `pub enum Open { Transient, Kept }`
  - `pub enum Close { One, Others, Right, All }`
  - `pub struct FileTab { pub path: String, pub content: Content, pub mode: Mode, pub transient: bool, pub pinned: bool, pub opening: u64, pub preview_selection: Option<Selection>, pub block_edit: Option<BlockEdit>, .. }`
  - `FileTab`: `name(&self) -> &str`, `is_dirty(&self) -> bool`, `shown()`, `editor_mut()`, `is_markdown()`, `previewing()`, `toggle_mode()`, `preview()`, `preview_request()`, `preview_click()`, `preview_drag()`, `preview_select_all()`, `preview_copy()`, `clear_preview_selection()`, `open_block_edit()`, `block_edit_key()`, `apply_block_edit()`, `line()`, `apply_pending_line()`, `discard()`, `keep_mine()`, `highlight_request(version)`, `highlighted(version, code)`
  - `FilesView`: `tabs(&self) -> &[FileTab]`, `active_index(&self) -> Option<usize>`, `tab()`, `tab_mut()`, `tab_for(path)`, `tab_for_mut(path)`, `file(&self) -> Option<&str>`, `shown()`, `editor_mut()`, `unsaved_paths(&self) -> Vec<String>`, `keep(&mut self)`, `open(path, Open) -> Option<String>`, `click(path, Open) -> Requests`, `reveal(path, line, Open) -> Requests`, `activate(at) -> Option<String>`, `cycle(forward: bool) -> Option<String>`, `select_nth(n: u8) -> Option<String>`, `close(paths: &[String]) -> Option<String>`, `close_targets(path, Close) -> Vec<String>`, `pin(path, pinned)`, `move_tab(pinned, from, to)`, `begin_read(path) -> bool`, `read(path, result) -> Option<String>`, `begin_save(path) -> Option<SaveRequest>`, `saved(path, point, written) -> Option<String>`, `preview_ready(stamp, page) -> bool`, `refresh()`, `search()`, `open_find()`, `replace_one()`, `replace_all()`
  - Removed: `FilesView::{unsaved, leaves_unsaved, reload, line, discard, keep_mine, …}`, the pub fields `file`, `content`, `mode`, `opening`, `preview_selection`, `block_edit`, and `Content::None`.

The binary (`crates/mkrew-ui/src/main.rs` + `shell/`) does not compile after this task; Task 2
fixes it. Do not commit at the end of Task 1; Task 2 commits both.

- [ ] **Step 1: Write the failing tests** — append to `crates/mkrew-ui/src/files_view/tests.rs`:

```rust
/// A view with README.md, src/main.rs and src/lib.rs listed (src open).
fn three() -> FilesView {
    let mut view = listed();
    view.toggle("src");
    view.begin_list("src");
    view.listed("src", Ok(vec![file("src/main.rs", None), file("src/lib.rs", None)]));
    view
}

fn read_ok(view: &mut FilesView, path: &str, text: &str) {
    assert!(view.begin_read(path));
    view.read(path, file_text_at(path, text));
}

fn tab_paths(view: &FilesView) -> Vec<&str> {
    view.tabs().iter().map(|tab| tab.path.as_str()).collect()
}

#[test]
fn a_single_click_replaces_the_transient_tab() {
    let mut view = three();
    assert_eq!(view.click("README.md", Open::Transient).read.as_deref(), Some("README.md"));
    assert_eq!(view.click("src/main.rs", Open::Transient).read.as_deref(), Some("src/main.rs"));
    assert_eq!(tab_paths(&view), ["src/main.rs"]);
    assert!(view.tab().unwrap().transient);
}

#[test]
fn a_double_click_or_an_edit_keeps_the_tab() {
    let mut view = three();
    view.click("README.md", Open::Kept);
    view.click("src/main.rs", Open::Transient);
    read_ok(&mut view, "src/main.rs", "a\n");
    view.keep();
    view.click("src/lib.rs", Open::Transient);
    assert_eq!(tab_paths(&view), ["README.md", "src/main.rs", "src/lib.rs"]);
    // A double click on an open transient tab keeps it.
    view.click("src/lib.rs", Open::Kept);
    assert!(!view.tab().unwrap().transient);
}

#[test]
fn a_dirty_transient_tab_is_never_replaced() {
    let mut view = three();
    view.click("README.md", Open::Transient);
    read_ok(&mut view, "README.md", "a\n");
    view.editor_mut().unwrap().insert("x");
    view.click("src/main.rs", Open::Transient);
    assert_eq!(tab_paths(&view), ["README.md", "src/main.rs"]);
}

#[test]
fn opening_an_open_file_activates_its_tab() {
    let mut view = three();
    view.click("README.md", Open::Kept);
    read_ok(&mut view, "README.md", "a\n");
    view.click("src/main.rs", Open::Kept);
    assert_eq!(view.click("README.md", Open::Transient).read, None, "already read");
    assert_eq!(view.file(), Some("README.md"));
    assert_eq!(view.tabs().len(), 2);
}

#[test]
fn new_tabs_go_right_of_the_active_one_after_the_pinned() {
    let mut view = three();
    view.open("README.md", Open::Kept);
    view.open("src/main.rs", Open::Kept);
    view.activate(0);
    view.open("src/lib.rs", Open::Kept);
    assert_eq!(tab_paths(&view), ["README.md", "src/lib.rs", "src/main.rs"]);
    view.pin("src/main.rs", true);
    view.activate(0);
    view.open("a.txt", Open::Kept);
    assert_eq!(tab_paths(&view), ["src/main.rs", "a.txt", "README.md", "src/lib.rs"]);
}

#[test]
fn closing_the_active_tab_activates_the_right_then_the_left_one() {
    let mut view = three();
    for path in ["README.md", "src/main.rs", "src/lib.rs"] {
        view.open(path, Open::Kept);
    }
    view.activate(1);
    view.close(&["src/main.rs".to_owned()]);
    assert_eq!(view.file(), Some("src/lib.rs"));
    view.close(&["src/lib.rs".to_owned()]);
    assert_eq!(view.file(), Some("README.md"));
    view.close(&["README.md".to_owned()]);
    assert_eq!((view.file(), view.active_index()), (None, None));
}

#[test]
fn closing_another_tab_keeps_the_active_one() {
    let mut view = three();
    for path in ["README.md", "src/main.rs", "src/lib.rs"] {
        view.open(path, Open::Kept);
    }
    view.close(&["README.md".to_owned()]);
    assert_eq!(view.file(), Some("src/lib.rs"));
}

#[test]
fn bulk_closes_skip_pinned_tabs() {
    let mut view = three();
    for path in ["README.md", "src/main.rs", "src/lib.rs", "a.txt"] {
        view.open(path, Open::Kept);
    }
    view.pin("README.md", true);
    assert_eq!(view.close_targets("src/main.rs", Close::One), ["src/main.rs"]);
    assert_eq!(view.close_targets("src/main.rs", Close::Others), ["src/lib.rs", "a.txt"]);
    assert_eq!(view.close_targets("src/main.rs", Close::Right), ["src/lib.rs", "a.txt"]);
    assert_eq!(view.close_targets("src/lib.rs", Close::Right), ["a.txt"]);
    assert_eq!(view.close_targets("README.md", Close::All), ["src/main.rs", "src/lib.rs", "a.txt"]);
    assert_eq!(view.close_targets("gone.rs", Close::All), Vec::<String>::new());
}

#[test]
fn pinning_moves_a_tab_to_the_pinned_group_and_keeps_it() {
    let mut view = three();
    view.open("README.md", Open::Kept);
    view.open("src/main.rs", Open::Kept);
    view.open("src/lib.rs", Open::Transient);
    view.pin("src/lib.rs", true);
    assert_eq!(tab_paths(&view), ["src/lib.rs", "README.md", "src/main.rs"]);
    assert!(!view.tabs()[0].transient);
    assert_eq!(view.file(), Some("src/lib.rs"), "the active tab follows its move");
    view.pin("src/main.rs", true);
    assert_eq!(tab_paths(&view), ["src/lib.rs", "src/main.rs", "README.md"]);
    view.pin("src/lib.rs", false);
    assert_eq!(tab_paths(&view), ["src/main.rs", "src/lib.rs", "README.md"]);
}

#[test]
fn reordering_stays_inside_the_group() {
    let mut view = three();
    for path in ["README.md", "src/main.rs", "src/lib.rs"] {
        view.open(path, Open::Kept);
    }
    view.pin("README.md", true);
    // Unpinned group: [src/main.rs, src/lib.rs]; move 0 → 1.
    view.move_tab(false, 0, 1);
    assert_eq!(tab_paths(&view), ["README.md", "src/lib.rs", "src/main.rs"]);
    // Out of the group: nothing moves.
    view.move_tab(false, 0, 2);
    view.move_tab(true, 0, 1);
    assert_eq!(tab_paths(&view), ["README.md", "src/lib.rs", "src/main.rs"]);
}

#[test]
fn next_previous_wrap_and_nine_is_the_last() {
    let mut view = three();
    for path in ["README.md", "src/main.rs", "src/lib.rs"] {
        view.open(path, Open::Kept);
    }
    view.cycle(true);
    assert_eq!(view.file(), Some("README.md"), "wraps forward");
    view.cycle(false);
    assert_eq!(view.file(), Some("src/lib.rs"), "wraps back");
    view.select_nth(1);
    assert_eq!(view.file(), Some("README.md"));
    view.select_nth(9);
    assert_eq!(view.file(), Some("src/lib.rs"));
    assert_eq!(view.select_nth(5), None);
    assert_eq!(view.file(), Some("src/lib.rs"), "no tab 5: nothing changes");
}

#[test]
fn a_read_lands_in_its_own_tab() {
    let mut view = three();
    view.open("README.md", Open::Kept);
    assert!(view.begin_read("README.md"));
    view.open("src/main.rs", Open::Kept);
    view.read("README.md", file_text_at("README.md", "readme\n"));
    assert!(view.shown().is_none(), "the active tab is still loading");
    view.activate(0);
    assert_eq!(text(&view), "readme\n");
}

#[test]
fn a_read_for_a_closed_tab_is_dropped() {
    let mut view = three();
    view.open("README.md", Open::Transient);
    assert!(view.begin_read("README.md"));
    view.open("src/main.rs", Open::Transient); // replaces README.md
    assert_eq!(view.read("README.md", file_text_at("README.md", "x\n")), None);
    assert_eq!(tab_paths(&view), ["src/main.rs"]);
    assert!(view.shown().is_none());
    assert!(!view.begin_read("README.md"), "no tab, no read");
}

#[test]
fn a_save_lands_in_its_own_tab_and_a_closed_one_is_reported() {
    let mut view = three();
    view.open("README.md", Open::Kept);
    read_ok(&mut view, "README.md", "a\n");
    view.editor_mut().unwrap().insert("x");
    let save = view.begin_save("README.md").unwrap();
    view.open("src/main.rs", Open::Kept);
    let written = FsWritten { saved: true, hash: "#xa\n".into() };
    assert_eq!(view.saved("README.md", save.point, Ok(written)), None);
    assert_eq!(view.unsaved_paths(), Vec::<String>::new());
    let refused = FsWritten { saved: false, hash: "#z".into() };
    assert!(view.saved("gone.rs", None, Ok(refused)).is_some());
}

#[test]
fn a_highlight_lands_in_its_own_tab() {
    let mut view = three();
    view.open("README.md", Open::Kept);
    read_ok(&mut view, "README.md", "a\n");
    view.editor_mut().unwrap().insert("x");
    let version = view.shown().unwrap().editor.version();
    view.open("src/main.rs", Open::Kept);
    read_ok(&mut view, "src/main.rs", "m\n");
    let tab = view.tab_for("README.md").unwrap();
    assert_eq!(tab.highlight_request(version).unwrap().content.as_deref(), Some("xa\n"));
    assert!(view.tab_for_mut("README.md").unwrap().highlighted(version, Code::plain("xa\n")));
    assert_eq!(text(&view), "m\n", "the active tab is untouched");
}

#[test]
fn git_changed_reads_the_active_tab_and_marks_the_others_stale() {
    let mut view = three();
    view.open("README.md", Open::Kept);
    read_ok(&mut view, "README.md", "a\n");
    view.open("src/main.rs", Open::Kept);
    read_ok(&mut view, "src/main.rs", "m\n");
    assert_eq!(view.refresh().read.as_deref(), Some("src/main.rs"));
    assert_eq!(view.activate(0).as_deref(), Some("README.md"), "stale: read once");
    assert_eq!(view.activate(1), None);
    assert_eq!(view.activate(0), None, "read already asked");
}

#[test]
fn unsaved_paths_lists_every_dirty_tab() {
    let mut view = three();
    for path in ["README.md", "src/main.rs"] {
        view.open(path, Open::Kept);
        read_ok(&mut view, path, "a\n");
        view.editor_mut().unwrap().insert("x");
    }
    assert_eq!(view.unsaved_paths(), ["README.md", "src/main.rs"]);
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p mkrew-ui --lib files_view`
Expected: compile errors (`Open`, `Close`, `tabs`, `close_targets`, … not found).

- [ ] **Step 3: Add the types** — in `files_view.rs`, replace the `FilesView` struct (lines 142–194) and add `Open`, `Close`, `FileTab`:

```rust
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
    /// An `fs.read` is in flight, and another was asked for meanwhile.
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
```

Delete `Content::None` (and its doc line): a tab starts as `Content::Loading`. `FilesView::new`
drops the removed fields and sets `tabs: Vec::new(), active: None`.

- [ ] **Step 4: Move the per-file methods to `impl FileTab`**

Add `impl FileTab` with:

```rust
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

    /// The `fs.read` answer; returns whether another read was asked for meanwhile.
    fn read(&mut self, read: Result<(FsContent, Code), String>) -> bool {
        self.reading = false;
        let again = std::mem::take(&mut self.read_again);
        self.show(read);
        again
    }
}
```

Then MOVE these methods from `impl FilesView` into `impl FileTab`, bodies unchanged except the
renames below: `show` (drop its final `self.search();` line — `FilesView::read` searches),
`shown`, `is_markdown`, `previewing`, `toggle_mode`, `stamp`, `preview`, `preview_request`,
`preview_ready` (private), `preview_click`, `preview_drag`, `preview_select_all`,
`preview_copy`, `clear_preview_selection`, `open_block_edit`, `block_edit_key`,
`apply_block_edit`, `editor_mut`, `line`, `apply_pending_line`, `discard`, `keep_mine`,
`highlight_request`, `highlighted`. Renames inside the moved bodies:

| Old | New |
|---|---|
| `self.file` (`Option<String>`) | `self.path` (`String`): `is_markdown` uses `self.path.rsplit_once('.')`; `highlight_request` uses `path: self.path.clone()` and only checks `self.shown()` |
| `self.preview` (the `Option<MdPreview>` field) | `self.page` |
| `self.preview_asked` | `self.page_asked` |

Move `begin_save` and `saved` too, as:

```rust
    /// Save (Ctrl+S). `None` when nothing is unsaved, the file cannot be saved, or a save is
    /// already in flight.
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
```

Delete from `FilesView`: `unsaved`, `leaves_unsaved`, `reload`, and the old `open`,
`begin_read`, `read`, `saved`, `begin_save`, `preview_ready` (replaced in Step 5).

- [ ] **Step 5: Add the tab API to `impl FilesView`**

```rust
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
        let replace = (self.tabs.iter()).position(|tab| tab.transient && !tab.is_dirty());
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
        self.activate(if forward { (at + 1) % len } else { (at + len - 1) % len })
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
```

Replace the routed methods:

```rust
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

    /// The `fs.read` answer for `path`, shown in its tab (dropped when the tab is gone).
    /// Returns the file to read again (asked for meanwhile).
    pub fn read(&mut self, path: &str, read: Result<(FsContent, Code), String>) -> Option<String> {
        let again = self.tab_for_mut(path)?.read(read);
        if self.file() == Some(path) {
            self.search();
        }
        again.then(|| path.to_owned())
    }

    /// Save `path`'s tab (Ctrl+S, "Save" in the unsaved-changes question).
    pub fn begin_save(&mut self, path: &str) -> Option<SaveRequest> {
        self.tab_for_mut(path)?.begin_save()
    }

    /// The `fs.write` answer for `path`, which saved the state `point`. `Some(why)` when the
    /// answer has nowhere to show (its tab was closed): the shell toasts it.
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
        let shown = self.active.and_then(|at| self.tabs.get(at)).and_then(FileTab::shown);
        if let (Some(find), Some(shown)) = (&mut self.find, shown) {
            find.search(shown.editor.lines());
        }
    }
```

`reveal` gets an `how: Open` parameter and works on the active tab:

```rust
    pub fn reveal(&mut self, path: &str, line: Option<u32>, how: Open) -> Requests {
        // … the folder loop stays as it is …
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
```

`open_find` and `find_and_editor` reach the active tab through the field (so `self.find` can be
borrowed at the same time):

```rust
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
        // … the rest of the old body unchanged (seed, from, find.get_or_insert_with, …) …
    }

    fn find_and_editor(&mut self) -> Option<(&mut TextFind, &mut Editor)> {
        let tab = self.active.and_then(|at| self.tabs.get_mut(at))?;
        match (&mut self.find, &mut tab.content) {
            (Some(find), Content::Shown(shown)) if !shown.code.binary && !shown.code.too_large => {
                Some((find, &mut shown.editor))
            }
            _ => None,
        }
    }
```

`key(Key::Enter)` → `return self.click(&entry.path, Open::Transient)`.

`refresh` reads the active tab and marks the others stale:

```rust
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
```

Update the module doc (lines 1–5): "…the tree cursor moved by arrow keys, and the opened files
as tabs ([`FileTab`]: `fs.read`, highlighted off the UI thread into a [`Code`], in an
[`Editor`], saved with `fs.write`). `git.changed` re-lists the open folders and re-reads the
active tab; the others are read when activated."

- [ ] **Step 6: Port the existing tests** in `files_view/tests.rs` mechanically:

| Old | New |
|---|---|
| `view.open("p")` | `view.open("p", Open::Transient)` |
| `view.click("p")` | `view.click("p", Open::Transient)` |
| `view.reveal(p, line)` | `view.reveal(p, line, Open::Transient)` |
| `view.begin_read()` | `view.begin_read(<path just opened>)` |
| `view.begin_save()` | `view.begin_save(<the opened path>)` |
| `view.mode`, `view.opening`, `view.content`, `view.block_edit`, `view.preview_selection` | `view.tab().unwrap().<field>` (assignments: `view.tab_mut().unwrap().<field> = …`) |
| `view.file` | `view.file()` |
| `view.unsaved()` | `view.unsaved_paths().first().map(String::as_str)` |
| `view.<moved method>(…)` (`toggle_mode`, `preview_request`, `preview`, `previewing`, `is_markdown`, `preview_click`, `preview_drag`, `preview_select_all`, `preview_copy`, `clear_preview_selection`, `open_block_edit`, `block_edit_key`, `apply_block_edit`, `line`, `apply_pending_line`, `discard`, `keep_mine`, `highlight_request`, `highlighted`) | `view.tab_mut().unwrap().<method>(…)` (or `view.tab().unwrap()` for `&self` ones) |
| `view.reload()` | `{ let tab = view.tab_mut().unwrap(); tab.discard(); Some(tab.path.clone()) }` |
| `Content::None` | `Content::Loading` |

Tests whose meaning changes with tabs, rewrite as follows:
- `every_open_of_another_file_is_a_new_opening`: README.md (Kept) and src/main.rs (Kept) have different `opening`s; re-activating README.md keeps its `opening`; closing README.md and opening it again gives a new one; another workspace's view gets another one.
- `another_file_starts_in_preview_without_the_old_page`: the new src/main.rs tab is in `Mode::Preview` with no page; README.md's tab keeps its page.
- `opening_reads_once_retries_failures_and_coalesces_reads`: the retry of a failed read is `view.open("src/main.rs", Open::Transient)` returning `Some("src/main.rs")` and setting `Content::Loading` (via `activate` → `take_read`).
- Any test asserting that opening another file asks about / drops unsaved text, or using `leaves_unsaved`: delete it (covered by `a_dirty_transient_tab_is_never_replaced`).

- [ ] **Step 7: Run the tests**

Run: `cargo test -p mkrew-ui --lib files_view`
Expected: all pass (the binary still fails to build; that is Task 2).

Do not commit yet.

---

### Task 2: Shell wiring — open, close, save, leave by tab

**Files:**
- Create: `crates/mkrew-ui/src/shell/file_tabs.rs`
- Modify: `crates/mkrew-ui/src/shell.rs` (add `mod file_tabs;` after `mod files;`)
- Modify: `crates/mkrew-ui/src/shell/files.rs`, `files_render.rs`, `files_preview.rs`, `navigation.rs`, `composer.rs`, `sessions.rs`, `workspace_actions.rs`
- Modify: `crates/mkrew-ui/src/main.rs` (field `file_tabs_scroll: ScrollHandle`, imports), `shell/connection.rs` (init)

**Interfaces:**
- Consumes: Task 1's `FilesView` / `FileTab` API, `Open`, `Close`.
- Produces (used by Tasks 3–4):
  - `Shell::click_file(&mut self, project_id: Ulid, path: &str, how: Open, window, cx)`
  - `Shell::reveal_file(&mut self, project_id, target: Option<Ulid>, path: String, line: Option<u32>, how: Open, window, cx)`
  - `Shell::save_file(&mut self, project_id: Ulid, path: String, then: Option<AfterSave>, window, cx)`
  - `Shell::leave_files(&mut self, project_id: Ulid, paths: Vec<String>, then: AfterSave, window, cx)`
  - `Shell::file_tab_shown(&mut self, project_id: Ulid, read: Option<String>, window, cx)`
  - `Shell::open_file_tab(&mut self, project_id, path: &str, how: Open, window, cx)`
  - `Shell::close_file_tabs(&mut self, project_id, path: &str, which: Close, window, cx)`
  - `Shell::close_active_file_tab(&mut self, project_id, window, cx)`
  - `Shell::cycle_file_tab(&mut self, project_id, forward: bool, window, cx)`
  - `Shell::nth_file_tab(&mut self, project_id, n: u8, window, cx)`
  - `Shell::pin_file_tab(&mut self, project_id, path: &str, pinned: bool, cx)`

- [ ] **Step 1: Create `shell/file_tabs.rs`**

```rust
//! Files view tabs (SPEC §12A, ADR 0080): switching, closing (unsaved text is asked about
//! first), pinning. The strip and its menu are drawn in `file_tabs_render`.

use crate::*;

impl Shell {
    /// After the active tab changed: the editor shows its caret, the strip shows the tab, a
    /// stale or failed tab is read and a `.md` page is built.
    pub(crate) fn file_tab_shown(
        &mut self,
        project_id: Ulid,
        read: Option<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(view) = self.files.get(&project_id) {
            if let Some(shown) = view.shown() {
                self.file_code_scroll
                    .scroll_to_item(shown.editor.cursor().line, ScrollStrategy::Center);
            }
            if let Some(at) = view.active_index() {
                self.file_tabs_scroll.scroll_to_item(at);
            }
        }
        let requests = Requests { list: vec![], read };
        self.files_requests(project_id, requests, window, cx);
        self.build_preview(project_id, window, cx);
    }

    /// A click on a tab (`Kept` for a double click), or any open of a file: its tab shows.
    pub(crate) fn open_file_tab(
        &mut self,
        project_id: Ulid,
        path: &str,
        how: Open,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let read = self.files_view(project_id).open(path, how);
        self.file_tab_shown(project_id, read, window, cx);
    }

    pub(crate) fn cycle_file_tab(
        &mut self,
        project_id: Ulid,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let read = self.files_view(project_id).cycle(forward);
        self.file_tab_shown(project_id, read, window, cx);
    }

    pub(crate) fn nth_file_tab(
        &mut self,
        project_id: Ulid,
        n: u8,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let read = self.files_view(project_id).select_nth(n);
        self.file_tab_shown(project_id, read, window, cx);
    }

    /// ✕, middle click, Ctrl+W and the tab menu's closes: asks about each unsaved tab first
    /// (Cancel closes nothing).
    pub(crate) fn close_file_tabs(
        &mut self,
        project_id: Ulid,
        path: &str,
        which: Close,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let paths = self.files_view(project_id).close_targets(path, which);
        if paths.is_empty() {
            return;
        }
        let closing = paths.clone();
        let then: AfterSave = Box::new(move |shell, window, cx| {
            let read = shell.files_view(project_id).close(&closing);
            shell.file_tab_shown(project_id, read, window, cx);
        });
        self.leave_files(project_id, paths, then, window, cx);
    }

    /// Ctrl+W / Ctrl+Shift+W in the Files view.
    pub(crate) fn close_active_file_tab(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let active = (self.files.get(&project_id))
            .and_then(FilesView::file)
            .map(str::to_owned);
        if let Some(path) = active {
            self.close_file_tabs(project_id, &path, Close::One, window, cx);
        }
    }

    pub(crate) fn pin_file_tab(
        &mut self,
        project_id: Ulid,
        path: &str,
        pinned: bool,
        cx: &mut Context<'_, Self>,
    ) {
        self.files_view(project_id).pin(path, pinned);
        cx.notify();
    }
}
```

Add to `main.rs` `struct Shell`, next to `files_scroll`:

```rust
    /// Keeps the active Files tab in view in the tab strip.
    file_tabs_scroll: ScrollHandle,
```

and `file_tabs_scroll: ScrollHandle::new(),` next to `files_scroll` in `shell/connection.rs`.
Make `files_view::{Close, FileTab, Open}` reachable from `crate::*` the same way `FilesView`
is (find where `main.rs` imports `FilesView` and add them there).

- [ ] **Step 2: Replace `leave_file` with `leave_files` in `shell/files.rs`**

```rust
    /// Runs `then` once the unsaved changes of `paths` (tabs of `project_id`'s Files view)
    /// are dealt with, asking about one tab at a time (shown while asked):
    /// - at once when none is unsaved;
    /// - after "Save" and a successful save, or "Don't save", for each of them.
    ///
    /// "Cancel" stops: `then` does not run (§2.5).
    pub(crate) fn leave_files(
        &mut self,
        project_id: Ulid,
        mut paths: Vec<String>,
        then: AfterSave,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let unsaved =
            (self.files.get(&project_id)).map_or_else(Vec::new, FilesView::unsaved_paths);
        paths.retain(|path| unsaved.contains(path));
        if paths.is_empty() {
            return then(self, window, cx);
        }
        let path = paths.remove(0);
        // Its tab exists (it is unsaved): `open` only shows it.
        self.open_file_tab(project_id, &path, Open::Transient, window, cx);
        let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
        let workspace = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .map_or_else(String::new, |project| format!(" in {}", project.name));
        let detail = format!("{path}{workspace} — your changes are lost if you don't save them.");
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Save changes to {name}?"),
            Some(detail.as_str()),
            &files_view::LEAVE_ANSWERS,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let choice = files_view::leave_choice(answer.await.ok());
            let _ = this.update_in(cx, |shell, window, cx| {
                let rest: AfterSave = Box::new(move |shell, window, cx| {
                    shell.leave_files(project_id, paths, then, window, cx);
                });
                match choice {
                    Some(files_view::Leave::Save) => {
                        shell.save_file(project_id, path, Some(rest), window, cx);
                    }
                    Some(files_view::Leave::DontSave) => {
                        if let Some(tab) = shell.files_view(project_id).tab_for_mut(&path) {
                            tab.discard();
                        }
                        rest(shell, window, cx);
                    }
                    None => {}
                }
            });
        })
        .detach();
    }
```

Callers in `files.rs`:
- `set_files_target`: replace the `unsaved().is_some()` block with
  ```rust
  let unsaved =
      (self.files.get(&project_id)).map_or_else(Vec::new, FilesView::unsaved_paths);
  if !unsaved.is_empty() {
      let again: AfterSave = Box::new(move |shell, window, cx| {
          shell.set_files_target(project_id, target, window, cx);
      });
      return self.leave_files(project_id, unsaved, again, window, cx);
  }
  ```
- `ask_unsaved`:
  ```rust
  let unsaved = (self.files.values()).find_map(|view| {
      let paths = view.unsaved_paths();
      (!paths.is_empty()).then_some((view.project_id, paths))
  });
  let Some((project_id, paths)) = unsaved else {
      return false;
  };
  ```
  and at the end `self.leave_files(project_id, paths, then, window, cx);`.
- `save_file(project_id, path: String, then, window, cx)`: `let Some(save) = view.begin_save(&path) else { return; };` (rest unchanged).

- [ ] **Step 3: Route reads, highlights and edits by tab** (`shell/files.rs`)

- `read_file`: `if !view.begin_read(&path) { return; }` (rest unchanged).
- `build_preview`: `let Some((stamp, text)) = view.tab_mut().and_then(FileTab::preview_request) else { return; };`
- `click_file(project_id, path, how: Open, window, cx)`: delete the `leaves_unsaved` block; then
  (the shown path is compared, not the index: a file opened over the transient tab keeps the
  same index)
  ```rust
  let view = self.files_view(project_id);
  let before = view.file().map(str::to_owned);
  let requests = view.click(path, how);
  if view.file() != before.as_deref() {
      // A file click lists nothing; only the read matters.
      return self.file_tab_shown(project_id, requests.read, window, cx);
  }
  self.files_requests(project_id, requests, window, cx);
  ```
- `files_key_down`: `self.click_file(project_id, &cursor, Open::Transient, window, cx)`.
- `file_edited`:
  ```rust
  let view = self.files_view(project_id);
  // An edit makes a transient tab permanent (SPEC §12A).
  view.keep();
  view.search();
  let Some((path, version)) = view
      .tab()
      .and_then(|tab| Some((tab.path.clone(), tab.shown()?.editor.version())))
  else {
      return;
  };
  ```
  In the spawned task use
  `.and_then(|view| view.tab_for(&path)).and_then(|tab| tab.highlight_request(version))` and
  `view.and_then(|view| view.tab_for_mut(&path)).is_some_and(|tab| tab.highlighted(version, code))`.
- `open_file_in_zed`:
  ```rust
  let (Some(path), line) = (
      view.file().map(str::to_owned),
      view.tab().and_then(FileTab::line),
  ) else {
      return;
  };
  ```

- [ ] **Step 4: Navigation** (`shell/navigation.rs`, `shell/workspace_actions.rs`)

- Delete `open_revealed`. `open_quick_pick` calls `self.reveal_file(project_id, open.worktree_task_id, path, None, Open::Transient, window, cx)`.
- `open_file_at`: the worktree switch uses `leave_files` with `unsaved_paths()` (same shape as `set_files_target` in Step 2, `again` re-calls `open_file_at`); the last line becomes `self.reveal_file(project_id, target, path, line, Open::Transient, window, cx)`. Update its doc ("…unsaved text is asked about first").
- `reveal_file(…, line: Option<u32>, how: Open, …)`: `view.reveal(&path, line, how)`; after `self.show_pending_line(project_id, target);` add `self.file_tab_shown(project_id, None, window, cx);` — but `show_pending_line` already scrolled to the link's line, so call `file_tab_shown` only when `line.is_none()`.
- `show_pending_line`: `(self.files_at(project_id, target)).and_then(|view| view.tab_mut()?.apply_pending_line())`.
- `workspace_actions.rs:469` (a new file was created): `self.reveal_file(project_id, target, path, None, Open::Kept, window, cx)`.

- [ ] **Step 5: Remaining accesses** — fix every compile error with this table:

| File | Old | New |
|---|---|---|
| `composer.rs:223` | `view.file.clone()` | `view.file().map(str::to_owned)` |
| `composer.rs:258` | `(&files.file, files.shown())` | `(files.file(), files.shown())` (adapt the `path` uses to `&str`) |
| `sessions.rs:144` | `view.find.is_some() && !view.previewing()` | `view.find.is_some() && !view.tab().is_some_and(FileTab::previewing)` |
| `sessions.rs:318` | `view.file.is_some()` | `view.file().is_some()` |
| `sessions.rs:385` | `self.save_file(project_id, None, window, cx)` | `if let Some(path) = self.files.get(&project_id).and_then(FilesView::file).map(str::to_owned) { self.save_file(project_id, path, None, window, cx); }` |
| `files_render.rs:117` | `view.file.clone()` | `view.file().map(str::to_owned)` |
| `files_render.rs:260` | `let Some(path) = view.file.clone() else` | `let Some(tab) = view.tab() else` … then `let path = tab.path.clone();` after the empty state |
| `files_render.rs:319` | `view.unsaved().is_some()` | `tab.is_dirty()` |
| `files_render.rs:324` | `view.line()` | `tab.line()` |
| `files_render.rs:328,364` | `view.is_markdown()` | `tab.is_markdown()` |
| `files_render.rs:331` | `view.mode` | `tab.mode` |
| `files_render.rs:336,339` | `shell.files_view(project_id).toggle_mode()` / `.previewing()` | `.tab_mut().map(FileTab::toggle_mode);` / `.tab().is_some_and(FileTab::previewing)` |
| `files_render.rs:435` | `shell.save_file(project_id, None, window, cx)` | capture `let save_path = path.clone();` and call `shell.save_file(project_id, save_path.clone(), None, window, cx)` |
| `files_render.rs:460,660` | `view.previewing()` | `tab.previewing()` |
| `files_render.rs:621` | `match &view.content` / `Content::None \| Content::Loading` | `match &tab.content` / `Content::Loading` |
| `files_render.rs:635` | `shell.files_view(project_id).open(&path)` + `files_requests` | `shell.open_file_tab(project_id, &path, Open::Transient, window, cx)` |
| `files_render.rs:692` | `shell.files_view(project_id).reload()` | `shell.files_view(project_id).tab_mut().map(\|tab\| { tab.discard(); tab.path.clone() })` |
| `files_render.rs:705` | `.keep_mine()` | `.tab_mut().map(FileTab::keep_mine);` |
| `files_render.rs:1003` (`file_row`) | `.on_click(cx.listener(move \|shell, _, window, cx\| { shell.click_file(project_id, &path, window, cx); }))` | `.on_click(cx.listener(move \|shell, event: &gpui::ClickEvent, window, cx\| { let how = if event.click_count() == 2 { Open::Kept } else { Open::Transient }; shell.click_file(project_id, &path, how, window, cx); }))` |
| `files_preview.rs` | `view.preview()`, `view.opening`, `view.preview_selection`, `view.block_edit`, `preview_click/drag/select_all/copy`, `clear_preview_selection`, `open_block_edit`, `apply_block_edit`, `block_edit_key` | the same on `view.tab()` / `view.tab_mut()`. `render_md_preview` and `file_block_wrap`: take `tab: &FileTab` instead of `view: &FilesView` where they only read per-file state, and pass `tab` from `render_file` |

- [ ] **Step 6: Build, test, smoke**

Run: `cargo build -p mkrew-ui` then `cargo test -p mkrew-ui`
Expected: builds; all tests pass. If `mkrew.exe` is locked by a running app, rename it to
`mkrew.locked-old.exe` (never kill the running app or daemon).

Smoke (`cargo run -p mkrew-ui --bin mkrew` against the running daemon; never stop the daemon):
1. Files view: type in README.md, click src/main.rs → src/main.rs shows with no "Save changes?" prompt (README.md is kept as a dirty tab; the strip comes in Task 3).
2. Switch worktree in the chooser → "Save changes to README.md?"; Cancel keeps everything.

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-ui/src/files_view.rs crates/mkrew-ui/src/files_view/tests.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/shell/file_tabs.rs crates/mkrew-ui/src/shell/files.rs crates/mkrew-ui/src/shell/files_render.rs crates/mkrew-ui/src/shell/files_preview.rs crates/mkrew-ui/src/shell/navigation.rs crates/mkrew-ui/src/shell/composer.rs crates/mkrew-ui/src/shell/sessions.rs crates/mkrew-ui/src/shell/workspace_actions.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/main.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): files view keeps opened files as tabs"
```

---

### Task 3: Tab strip, tab menu, drag to reorder

**Files:**
- Create: `crates/mkrew-ui/src/shell/file_tabs_render.rs`
- Modify: `crates/mkrew-ui/src/shell.rs` (`mod file_tabs_render;`)
- Modify: `crates/mkrew-ui/src/shell/files_render.rs` (strip above the header)
- Modify: `crates/mkrew-ui/src/shell/reorder.rs` (`ReorderList::FileTabs`)
- Modify: `crates/mkrew-ui/src/main.rs` (`FileTabMenu`, `file_tab_menu` field, render)
- Modify: `crates/mkrew-ui/src/shell/connection.rs`, `shell/focus.rs`, `shell/file_tabs.rs`

**Interfaces:**
- Consumes: Task 2's `open_file_tab`, `close_file_tabs`, `pin_file_tab`, `file_tabs_scroll`; `Shell::copy_file_path` (`workspace_actions.rs` ~line 475, takes `project_id, target, &path, cx`); `Shell::reorderable` / `drop_reordered` (`reorder.rs`); widgets `menu_panel`, `menu_item`, `row_action`, `pin_mark`, `tooltip`, `icon_svg`.
- Produces: `Shell::render_file_tabs(&self, project_id, view: &FilesView, theme: &Theme, cx) -> gpui::Stateful<gpui::Div>`, `Shell::render_file_tab_menu(&self, menu: &FileTabMenu, theme, cx) -> gpui::AnyElement`, `Shell::open_file_tab_menu(&mut self, project_id, path: String, position, cx)`.

- [ ] **Step 1: Reorder list** — in `reorder.rs` `enum ReorderList`:

```rust
    /// A workspace's pinned (`true`) or unpinned Files tabs.
    FileTabs(Ulid, bool),
```

and in `drop_reordered`:

```rust
            ReorderList::FileTabs(project_id, pinned) => {
                self.files_view(project_id).move_tab(pinned, item.index, to);
            }
```

Module doc first line: "…for the sidebar's workspace cards and rows, the tab strip and the Files tabs…".

- [ ] **Step 2: Menu state** — `main.rs`, after `struct FileMenu`:

```rust
/// The right-click menu of a Files tab (SPEC §12A): Close, Close others, Close to the right,
/// Close all, Pin / Unpin, Copy path.
struct FileTabMenu {
    project_id: Ulid,
    path: String,
    /// Window position of the click.
    position: gpui::Point<gpui::Pixels>,
}
```

Field on `Shell` after `file_menu`:

```rust
    /// The open right-click menu of a Files tab.
    file_tab_menu: Option<FileTabMenu>,
```

Init `file_tab_menu: None,` in `connection.rs`; add `self.file_tab_menu = None;` to
`focus.rs::close_menus`; add `self.file_tab_menu = None;` as the first line of
`close_file_tabs` and of `pin_file_tab` in `file_tabs.rs`. In `main.rs` render, after the
`file_menu` children:

```rust
                    .children(
                        (self.file_tab_menu.as_ref())
                            .filter(|menu| {
                                self.workspace == Some(menu.project_id)
                                    && self.view() == View::Files
                                    && !self.modal_open()
                            })
                            .map(|menu| self.render_file_tab_menu(menu, &theme, cx)),
                    )
```

- [ ] **Step 3: `shell/file_tabs_render.rs`**

```rust
//! The Files view's tab strip and tab menu (SPEC §12A, ADR 0080).

use crate::*;

impl Shell {
    pub(crate) fn open_file_tab_menu(
        &mut self,
        project_id: Ulid,
        path: String,
        position: Point<Pixels>,
        cx: &mut Context<'_, Self>,
    ) {
        self.close_menus();
        self.file_tab_menu = Some(FileTabMenu {
            project_id,
            path,
            position,
        });
        cx.notify();
    }

    /// One tab per opened file: pin, name (its folder too when two tabs share a name), `●`
    /// for unsaved text, ✕ on the active tab and on hover; italic while transient.
    pub(crate) fn render_file_tabs(
        &self,
        project_id: Ulid,
        view: &FilesView,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let tabs = view.tabs();
        let active = view.active_index();
        let pinned_count = tabs.iter().filter(|tab| tab.pinned).count();
        let hover = theme.hover;
        let close_keys = self.keymap.label(Command::CloseTab);
        div()
            .id("file-tabs")
            .flex_none()
            .h(theme::rems_px(theme::TAB_STRIP_HEIGHT))
            .flex()
            .items_center()
            .gap(px(space::XS))
            .px(px(space::S))
            .overflow_x_scroll()
            .track_scroll(&self.file_tabs_scroll)
            .border_b(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .on_drop(cx.listener(|shell, item: &DraggedItem, window, cx| {
                shell.drop_reordered(item, window, cx);
            }))
            .children(tabs.iter().enumerate().map(|(at, tab)| {
                let is_active = active == Some(at);
                let dirty = tab.is_dirty();
                let name = tab.name();
                let twin = tabs.iter().filter(|other| other.name() == name).count() > 1;
                let folder = (tab.path.rsplit_once('/'))
                    .filter(|_| twin)
                    .map(|(dir, _)| dir.to_owned());
                let group = SharedString::from(format!("file-tab-{}", tab.path));
                let item = DraggedItem {
                    list: ReorderList::FileTabs(project_id, tab.pinned),
                    index: if tab.pinned { at } else { at - pinned_count },
                    title: name.to_owned().into(),
                };
                let (click_path, middle_path, menu_path, close_path) = (
                    tab.path.clone(),
                    tab.path.clone(),
                    tab.path.clone(),
                    tab.path.clone(),
                );
                let element = div()
                    .id(group.clone())
                    .group(group.clone())
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .h(theme::rems_px(theme::CONTROL_HEIGHT))
                    .pl(px(space::M))
                    .pr(px(space::S))
                    .rounded(px(radius::MEDIUM))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(gpui::transparent_black())
                    .cursor_pointer()
                    .whitespace_nowrap()
                    .text_size(font_size::BODY)
                    .when(is_active, |el| {
                        el.bg(rgb(theme.card))
                            .border_color(rgb(theme.selected))
                            .font_weight(FontWeight(weight::MEDIUM))
                            .text_color(rgb(theme.text))
                    })
                    .when(!is_active, |el| {
                        el.text_color(rgb(theme.text_muted))
                            .hover(move |style| style.bg(rgb(hover)))
                            .active(|style| style.opacity(opacity::ACTIVE))
                    })
                    .when(tab.transient && !dirty, |el| el.italic())
                    .tooltip(tooltip(theme, &tab.path, ""))
                    .on_click(cx.listener(move |shell, event: &gpui::ClickEvent, window, cx| {
                        let how = if event.click_count() == 2 {
                            Open::Kept
                        } else {
                            Open::Transient
                        };
                        shell.open_file_tab(project_id, &click_path, how, window, cx);
                    }))
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(move |shell, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            shell.close_file_tabs(project_id, &middle_path, Close::One, window, cx);
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |shell, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            shell.open_file_tab_menu(project_id, menu_path.clone(), event.position, cx);
                        }),
                    )
                    .when(tab.pinned, |el| el.child(pin_mark()))
                    .child(icon_svg(Icon::File, theme::ICON_SIZE_SMALL, theme.text_subtle))
                    .child(
                        div()
                            .max_w(px(theme::TAB_TITLE_MAX_WIDTH))
                            .truncate()
                            .child(format!("{}{name}", if dirty { "● " } else { "" })),
                    )
                    .children(folder.map(|folder| {
                        div()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_subtle))
                            .child(folder)
                    }))
                    .child(
                        row_action(
                            SharedString::from(format!("file-tab-close-{}", tab.path)),
                            icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted),
                            theme,
                        )
                        // Always on the active tab, on hover for the others.
                        .when(!is_active, |el| {
                            el.opacity(opacity::HIDDEN)
                                .group_hover(group, |style| style.opacity(opacity::FULL))
                        })
                        .tooltip(tooltip(
                            theme,
                            "Close tab",
                            if is_active { close_keys.clone() } else { String::new() },
                        ))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            // Never activate the tab being closed.
                            cx.stop_propagation();
                            shell.close_file_tabs(project_id, &close_path, Close::One, window, cx);
                        })),
                    )
                    .into_any_element();
                self.reorderable(element, item, true, space::XS, theme, cx)
            }))
    }

    /// The tab menu at the click; a click outside closes it.
    pub(crate) fn render_file_tab_menu(
        &self,
        menu: &FileTabMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let project_id = menu.project_id;
        let view = self.files.get(&project_id);
        let target = view.and_then(|view| view.worktree_task_id);
        let pinned = (view.and_then(|view| view.tab_for(&menu.path))).is_some_and(|tab| tab.pinned);
        let separator = || {
            div()
                .h(px(theme::BORDER_WIDTH))
                .my(px(space::XS))
                .bg(rgb(theme.border))
        };
        let close = |id: &'static str, label: &'static str, which: Close, keys: String| {
            let path = menu.path.clone();
            let enabled =
                view.is_some_and(|view| !view.close_targets(&menu.path, which).is_empty());
            menu_item(id.into(), theme)
                .child(div().flex_1().text_color(rgb(theme.text)).child(label))
                .child(div().text_size(font_size::META).child(keys))
                .when(!enabled, |el| el.opacity(opacity::DISABLED))
                .when(enabled, |el| {
                    el.on_click(cx.listener(move |shell, _, window, cx| {
                        shell.close_file_tabs(project_id, &path, which, window, cx);
                    }))
                })
        };
        let (pin_path, copy_path) = (menu.path.clone(), menu.path.clone());
        let panel = menu_panel("file-tab-menu", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.file_tab_menu = None;
                cx.notify();
            }))
            .child(close(
                "file-tab-close",
                "Close",
                Close::One,
                self.keymap.label(Command::CloseTab),
            ))
            .child(close("file-tab-close-others", "Close others", Close::Others, String::new()))
            .child(close("file-tab-close-right", "Close to the right", Close::Right, String::new()))
            .child(close("file-tab-close-all", "Close all", Close::All, String::new()))
            .child(separator())
            .child(
                menu_item("file-tab-pin".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child(if pinned { "Unpin" } else { "Pin" }),
                    )
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        shell.pin_file_tab(project_id, &pin_path, !pinned, cx);
                    })),
            )
            .child(
                menu_item("file-tab-copy-path".into(), theme)
                    .child(div().flex_1().text_color(rgb(theme.text)).child("Copy path"))
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        shell.file_tab_menu = None;
                        shell.copy_file_path(project_id, target, &copy_path, cx);
                    })),
            );
        deferred(
            anchored()
                .position(menu.position)
                .snap_to_window_with_margin(px(space::S))
                .child(panel),
        )
        .into_any_element()
    }
}
```

The two `Command::CloseTab` labels become `Command::CloseFileTab` in Task 4.
Match `copy_file_path`'s exact signature in `workspace_actions.rs`. Accept `cargo fmt` reflows.

- [ ] **Step 4: Put the strip in the file pane** — `files_render.rs::render_file`, the final chain:

```rust
        panel
            .child(self.render_file_tabs(project_id, view, theme, cx))
            .child(header)
```

The empty state (no tab) stays as it is, without a strip.

- [ ] **Step 5: Build and smoke**

Run: `cargo build -p mkrew-ui && cargo test -p mkrew-ui`
Expected: PASS.

Smoke (app against the running daemon; never stop the daemon):
1. Click README.md, then src/main.rs: one italic tab, replaced. Double-click src/main.rs in the tree: no longer italic. Click README.md: two tabs.
2. Type in a tab → `●` shows and the tab is no longer italic.
3. ✕ on the dirty tab → "Save changes to …?"; Cancel keeps it; Don't save closes it and the right (else left) neighbour shows.
4. Two dirty tabs → right-click → Close all → answer Save for the first, Cancel for the second → no tab closes; the first file is saved.
5. Right-click → Pin: the tab moves left with the pin mark; Close all leaves it.
6. Drag a tab past another: the accent drop line shows and the order changes; an unpinned tab cannot land among pinned ones.
7. Open 15 files: the strip scrolls sideways; switching keeps the active tab visible.
8. Middle-click a tab: it closes.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/shell/file_tabs_render.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/shell/files_render.rs crates/mkrew-ui/src/shell/reorder.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/focus.rs crates/mkrew-ui/src/shell/file_tabs.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): files tab strip with menu, pin and drag"
```

---

### Task 4: Keys — Ctrl+W, tab keys in Files, palette

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs`
- Modify: `crates/mkrew-ui/src/palette.rs`, `crates/mkrew-ui/src/palette/tests.rs`
- Modify: `crates/mkrew-ui/src/shell/sessions.rs`, `shell/navigation.rs` (palette context), `shell/file_tabs_render.rs`

**Interfaces:**
- Consumes: Task 2's `cycle_file_tab`, `nth_file_tab`, `close_active_file_tab`.
- Produces: `Command::CloseFileTab` (name `close_file_tab`, default `ctrl-w`, context `FILES_CONTEXT`, palette "Files: Close tab").

- [ ] **Step 1: Write the failing tests** — in `keymap.rs` tests:

```rust
    #[test]
    fn ctrl_w_closes_a_file_tab_in_files_only() {
        let keymap = Keymap::defaults();
        assert_eq!(keymap.command_for(&keystroke("ctrl-w")), Some(Command::CloseFileTab));
        assert_eq!(Command::CloseFileTab.context(), Some(FILES_CONTEXT));
        assert_eq!(keymap.label(Command::CloseFileTab), "Ctrl+W");
    }
```

and in `palette/tests.rs`, next to the `SaveFile` assertions (~line 264):

```rust
    assert!(commands(&context(View::Files)).contains(&Command::CloseFileTab));
    assert!(!commands(&context(View::Terminals)).contains(&Command::CloseFileTab));
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p mkrew-ui --lib keymap` and `cargo test -p mkrew-ui --lib palette`
Expected: compile error, `CloseFileTab` not found.

- [ ] **Step 3: Add the command**

`keymap.rs`:
- enum `Command`, after `ReplaceInFile`: `/// Close the active Files tab (asks about unsaved text).` + `CloseFileTab,`
- `all()`: after `Command::ReplaceInFile,` add `Command::CloseFileTab,`
- name: `Command::CloseFileTab => "close_file_tab".into(),`
- default keys: `Command::CloseFileTab => "ctrl-w".into(),`
- `context()`: add `| Command::CloseFileTab` to the `FILES_CONTEXT` arm.
- If any other exhaustive `match` on `Command` fails to compile (settings dialog, docs listing), add the arm there in the same style.

`palette.rs`:
- label: `Command::CloseFileTab => "Files: Close tab".into(),`
- availability: add `| Command::CloseFileTab` to the `in_view(View::Files)` arm.

`navigation.rs::open_palette`, the `tabs` field:

```rust
            tabs: match (workspace, self.view()) {
                (Some(id), View::Files) => self.files.get(&id).map_or(0, |view| view.tabs().len()),
                (Some(id), _) => self.tabs.get(&id).map_or(0, |tabs| tabs.tabs().len()),
                (None, _) => 0,
            },
```

`file_tabs_render.rs`: both `self.keymap.label(Command::CloseTab)` → `Command::CloseFileTab`.

- [ ] **Step 4: Route the tab keys in Files** — `sessions.rs::run`, right before `if self.view() != View::Terminals {` (~line 194):

```rust
        // Tab keys act on the Files tabs while the Files view is shown (SPEC §12A, §13).
        if self.view() == View::Files
            && let Some(project_id) = self.workspace
        {
            match *command {
                Command::NextTab => return self.cycle_file_tab(project_id, true, window, cx),
                Command::PreviousTab => return self.cycle_file_tab(project_id, false, window, cx),
                Command::Tab(n) => return self.nth_file_tab(project_id, n, window, cx),
                Command::CloseTab | Command::CloseFileTab => {
                    return self.close_active_file_tab(project_id, window, cx);
                }
                _ => {}
            }
        }
```

In the big `match *command` add:

```rust
            // Bound in the Files view only; handled above.
            Command::CloseFileTab => {}
```

In the find-field branch (~line 145) add `| Command::CloseFileTab` to `files_command` (Ctrl+W
works while the find field has the keyboard).

- [ ] **Step 5: Run the tests**

Run: `cargo test -p mkrew-ui`
Expected: PASS, including `agent_keys_are_unbound_and_reach_the_pty` (plain Ctrl+W is allowed: its context is `FILES_CONTEXT`).

Smoke: in Files with three tabs, Ctrl+Tab / Ctrl+Shift+Tab cycle and wrap (also with the caret in the editor), Ctrl+Alt+1 / Ctrl+Alt+9 jump, Ctrl+W and Ctrl+Shift+W close (asking for unsaved text). In Terminals the same keys still act on terminal tabs, and Ctrl+W in a shell deletes a word (it reaches the PTY).

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette/tests.rs crates/mkrew-ui/src/shell/sessions.rs crates/mkrew-ui/src/shell/navigation.rs crates/mkrew-ui/src/shell/file_tabs_render.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): ctrl+w and tab keys for files tabs"
```

(Add any other file Step 3 touched.)

---

### Task 5: Docs, full verify

**Files:**
- Create: `docs/decisions/0080-files-view-tabs.md`
- Modify: `docs/SPEC.md` (§12A, §13)
- Modify: `docs/HUMAN_CHECKS.md` ("To verify")

- [ ] **Step 1: ADR** — check `docs/decisions/` for the next free number (expected `0080`) and write:

```markdown
# 0080 — Files view tabs

- Date: 2026-10-26
- Status: accepted
- Milestone: none (user request: view and close several files at once, like Zed / VS Code)

## Context

The Files view showed one file; opening another replaced it and asked "Save / Don't save /
Cancel" when it had unsaved text (SPEC §12A).

## Options

1. Per-file state in a `FileTab`, a tab list in `FilesView`; answers routed by path. (Chosen.)
2. Keep one shown file and swap parked copies in and out: an `fs.read` / `fs.write` answer that
   arrives after a switch lands in the wrong file.
3. Keep only unsaved buffers and re-read on switch: caret, undo and scroll are lost.

## Decision

- VS Code's transient tab: a single click opens an italic tab that the next single click
  replaces while it is clean; a double click, an edit, Pin or "New file…" keeps it.
- Close (✕, middle click, Ctrl+W, Ctrl+Shift+W, tab menu: Close / others / to the right / all)
  asks about each unsaved tab; Cancel closes nothing. Bulk closes skip pinned tabs.
- Pin and drag-to-reorder reuse the terminal tab strip's look and `shell/reorder.rs`.
- Ctrl+Tab, Ctrl+Shift+Tab, Ctrl+Alt+1–9 (9 = last) act on file tabs while Files is shown.
- One find bar per view; `git.changed` reads the active tab and marks the others stale.
- Tabs are in memory only: a restart or another worktree starts with none.

## Consequences

- SPEC §12A and §13 change; no protocol, storage or daemon change.
- Opening another file never asks about unsaved text any more; leaving a worktree or closing
  the window asks once per unsaved tab.
- New key `ctrl-w` (`close_file_tab`) in the Files context only (ADR 0028 allows plain
  Ctrl+letter there).
- A Markdown preview starts at the top when its tab is shown again (one shared page).
```

- [ ] **Step 2: SPEC §12A** — in the "Unsaved text is never replaced" bullet replace
"Opening another file, switching to another worktree or closing the window with unsaved changes
asks "Save / Don't save / Cancel"." with "Closing a tab, switching to another worktree or closing
the window with unsaved changes asks "Save / Don't save / Cancel" (once per unsaved tab)." In the
Markdown preview bullet replace "Another file starts in Preview." with "A new tab starts in
Preview." and "another file (or the same file opened again) starts at the top" with "another tab
(or the same file opened again) starts at the top". Add after the **Editor** bullet:

```markdown
- **Tabs** (ADR 0080): every opened file has a tab above the header (pinned tabs first; the
  strip scrolls sideways). A single click (tree, Enter, quick open, a terminal link) opens a
  transient tab (italic) that the next one replaces while it is clean; a double click, an edit,
  Pin or "New file…" keeps it. `●` marks unsaved text; ✕ (on the active tab and on hover), a
  middle click, `Ctrl+W` or `Ctrl+Shift+W` closes a tab, asking "Save / Don't save / Cancel"
  first (Cancel closes nothing); the right neighbour, else the left one, is shown. Right-click:
  Close, Close others, Close to the right, Close all (these three skip pinned tabs), Pin /
  Unpin, Copy path. Tabs are dragged within their group (pinned / unpinned). `Ctrl+Tab` /
  `Ctrl+Shift+Tab` cycle, `Ctrl+Alt+1–9` jump (9 = last). One find bar for the view, searched
  on the shown tab. `git.changed` reads the shown tab; the others are read when shown. Tabs
  live in memory only; another worktree starts with none.
```

- [ ] **Step 3: SPEC §13** — in the shortcuts table add a `Close Files tab` row with `Ctrl+W`
(Files view only), matching the table's columns, and add "(file tabs while the Files view is
shown)" to the Next / Previous tab, Jump to tab 1–9 and Close tab rows.

- [ ] **Step 4: HUMAN_CHECKS** — under "To verify":
`- [ ] Files tabs (ADR 0080): check the tab strip visually in dark and light (active, transient italic, ●, pin, drop line, menu).`

- [ ] **Step 5: Full verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt check, clippy `-D warnings` and every test pass. Fix and re-run until green.
Literal check on the branch diff:
`git diff HEAD~4 -- crates/mkrew-ui/src | grep -nE '^\+.*(#[0-9a-fA-F]{6}|px\([0-9])'` → no hits.

- [ ] **Step 6: Commit**

```bash
git add docs/decisions/0080-files-view-tabs.md docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "docs(spec): files view tabs (ADR 0080)"
```
