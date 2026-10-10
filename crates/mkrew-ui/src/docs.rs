//! Docs view-model (SPEC §12): a workspace's reviewable Markdown files (`doc.list`, with
//! open-comment counts), the selected file's content (`doc.read`) parsed for the preview, and
//! preview / source mode. `doc.changed` re-reads the shown file and re-lists; `comment.updated`
//! re-lists (counts). Pure and tested headlessly; `main.rs` does the requests and renders it.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use mkrew_protocol::{CommentContext, DocContent, DocFile, Event};
use ulid::Ulid;

use crate::comments::Comments;
use crate::diagram::{self, Diagram};
use crate::highlight::{self, Code};
use crate::markdown::{self, Block, BlockKind};
use crate::palette;
use crate::text_box::Input;

static NEXT_OPENING: AtomicU64 = AtomicU64::new(1);

/// A number for one opening of a file in a view (Docs, Files): its preview page starts at the
/// top, while re-reads of the same opening keep the place. Unique in the process, so the same
/// path in another workspace, or opened again, is another opening.
pub fn new_opening() -> u64 {
    NEXT_OPENING.fetch_add(1, Ordering::Relaxed)
}

/// The file list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Files {
    Loading,
    Loaded(Vec<DocFile>),
    /// Why `doc.list` failed (the last loaded list, if any, is gone).
    Failed(String),
}

/// The selected file's content.
#[derive(Debug, Clone, PartialEq)]
pub enum Doc {
    /// No file selected.
    None,
    Loading,
    Shown(Rc<Shown>),
    Failed(String),
    /// `doc.changed` said the file is gone.
    Deleted,
}

/// A read file, parsed once (off the UI thread: highlighting is slow).
#[derive(Debug, PartialEq)]
pub struct Shown {
    pub hash: String,
    pub blocks: Vec<Block>,
    /// Source lines without their line ending; line `n` (1-based) is `lines[n - 1]`.
    pub lines: Vec<String>,
    /// Every code block, highlighted, by its first source line (ADR 0021).
    pub code: HashMap<u32, Code>,
    /// Every ```mermaid block drawn (or why not), by its first source line (ADR 0021).
    pub diagrams: HashMap<u32, Result<Diagram, String>>,
}

impl Shown {
    pub fn new(content: &DocContent) -> Shown {
        Shown {
            hash: content.hash.clone(),
            ..Shown::from_text(&content.content)
        }
    }

    /// A text parsed for the preview, with `hash` empty. The Files view parses its editor's
    /// text, which is not a `DocContent`.
    pub fn from_text(text: &str) -> Shown {
        let blocks = markdown::parse(text);
        let mut code = HashMap::new();
        let mut diagrams = HashMap::new();
        markdown::walk(&blocks, &mut |block| {
            if let BlockKind::Code { lang, text } = &block.kind {
                code.insert(block.lines.start, highlight::fenced(lang.as_deref(), text));
                if lang
                    .as_deref()
                    .is_some_and(|lang| lang.eq_ignore_ascii_case("mermaid"))
                {
                    diagrams.insert(block.lines.start, diagram::render(text));
                }
            }
        });
        Shown {
            hash: String::new(),
            lines: source_lines(text),
            blocks,
            code,
            diagrams,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Preview,
    Source,
}

/// What an event asks of the shell.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Reaction {
    /// Call `doc.list` again.
    pub relist: bool,
    /// `doc.read` this file again.
    pub reread: Option<String>,
}

/// One workspace's Docs view.
#[derive(Debug)]
pub struct Docs {
    pub project_id: Ulid,
    pub files: Files,
    /// Relative path of the selected file.
    pub selected: Option<String>,
    pub doc: Doc,
    /// The selection's [`new_opening`] number (0 before the first).
    pub opening: u64,
    pub mode: Mode,
    /// The list filter (fuzzy over the path, SPEC §12); empty = every file. Kept across
    /// relists.
    pub filter: Input,
    /// Why the last "Open in Zed" failed.
    pub zed_error: Option<String>,
    /// The selected file's review comments.
    pub comments: Comments,
    listing: bool,
    list_again: bool,
    /// The hash the last `doc.changed` announced for the selected file: reads answering with
    /// older content (sent before it, answered late) are dropped.
    changed_to: Option<String>,
}

impl Docs {
    pub fn new(project_id: Ulid) -> Docs {
        Docs {
            project_id,
            files: Files::Loading,
            selected: None,
            doc: Doc::None,
            opening: 0,
            mode: Mode::default(),
            filter: Input::default(),
            zed_error: None,
            comments: Comments::default(),
            listing: false,
            list_again: false,
            changed_to: None,
        }
    }

    /// A `doc.list` is about to be sent; `false` = one is in flight already (it is sent again
    /// once that one answers).
    pub fn begin_list(&mut self) -> bool {
        if self.listing {
            self.list_again = true;
            return false;
        }
        self.listing = true;
        true
    }

    /// The `doc.list` answer. Returns the file to read when nothing was selected yet (the
    /// first one) and whether to list again (asked meanwhile).
    pub fn listed(&mut self, listed: Result<Vec<DocFile>, String>) -> (Option<String>, bool) {
        self.listing = false;
        let again = std::mem::take(&mut self.list_again);
        let read = match listed {
            Ok(files) => {
                let first = files.first().map(|file| file.file_path.clone());
                self.files = Files::Loaded(files);
                match (&self.selected, first) {
                    (None, Some(first)) => self.select(&first),
                    _ => None,
                }
            }
            Err(error) => {
                self.files = Files::Failed(error);
                None
            }
        };
        (read, again)
    }

    /// Shows `file_path`. Returns it when it has to be read: another file, or the same one
    /// whose read failed (a retry).
    pub fn select(&mut self, file_path: &str) -> Option<String> {
        let same = self.selected.as_deref() == Some(file_path);
        if same && !matches!(self.doc, Doc::Failed(_) | Doc::Deleted) {
            return None;
        }
        if !same {
            self.opening = new_opening();
        }
        self.selected = Some(file_path.to_owned());
        self.doc = Doc::Loading;
        self.changed_to = None;
        Some(file_path.to_owned())
    }

    /// A `doc.read` answer, parsed; ignored when another file got selected meanwhile, or when
    /// it predates the last `doc.changed` (the read that change asked for follows).
    pub fn read(&mut self, file_path: &str, read: Result<Shown, String>) {
        if self.selected.as_deref() != Some(file_path) {
            return;
        }
        if let (Ok(shown), Some(changed_to)) = (&read, &self.changed_to)
            && shown.hash != *changed_to
        {
            return;
        }
        self.doc = match read {
            Ok(shown) => Doc::Shown(Rc::new(shown)),
            Err(error) => Doc::Failed(error),
        };
    }

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            Mode::Preview => Mode::Source,
            Mode::Source => Mode::Preview,
        };
    }

    /// Follows `doc.changed` / `comment.updated` of this workspace (diff comments are the Git
    /// view's).
    pub fn apply(&mut self, event: &Event) -> Reaction {
        match event {
            Event::DocChanged(changed) if changed.project_id == self.project_id => {
                let mut reaction = Reaction {
                    relist: true,
                    reread: None,
                };
                if self.selected.as_deref() != Some(changed.file_path.as_str()) {
                    return reaction;
                }
                self.changed_to.clone_from(&changed.hash);
                match &changed.hash {
                    None => self.doc = Doc::Deleted,
                    Some(hash) => {
                        let current = matches!(&self.doc, Doc::Shown(shown) if shown.hash == *hash);
                        // A read in flight may predate the change: read again after it.
                        if !current {
                            self.doc = Doc::Loading;
                            reaction.reread = Some(changed.file_path.clone());
                        }
                    }
                }
                reaction
            }
            // `Comments` ignores diff comments too; checked here so they never relist.
            Event::CommentUpdated(comment)
                if comment.project_id == self.project_id
                    && comment.context == CommentContext::Doc =>
            {
                self.comments.apply_event(self.project_id, comment);
                Reaction {
                    relist: true,
                    reread: None,
                }
            }
            _ => Reaction::default(),
        }
    }

    /// The selected file's list entry, if listed.
    pub fn selected_file(&self) -> Option<&DocFile> {
        let Files::Loaded(files) = &self.files else {
            return None;
        };
        let selected = self.selected.as_deref()?;
        files.iter().find(|file| file.file_path == selected)
    }

    /// The listed files the filter keeps, best match first (the daemon's order on ties and
    /// with an empty filter), each with its matched char indices in `file_path`.
    pub fn shown_files(&self) -> Vec<(&DocFile, Vec<u32>)> {
        let Files::Loaded(files) = &self.files else {
            return Vec::new();
        };
        let mut shown: Vec<(i64, &DocFile, Vec<u32>)> = files
            .iter()
            .filter_map(|file| {
                let (score, indices) = palette::fuzzy(&self.filter, &file.file_path)?;
                Some((score, file, indices))
            })
            .collect();
        // Stable: ties keep the listed order.
        shown.sort_by_key(|(score, _, _)| std::cmp::Reverse(*score));
        shown
            .into_iter()
            .map(|(_, file, indices)| (file, indices))
            .collect()
    }

    /// ↑ / ↓ in the filter: the shown file `offset` places from the selected one, clamped
    /// (the first shown one when the selection is filtered out); `None` without shown files.
    pub fn step_selection(&self, offset: isize) -> Option<String> {
        let shown = self.shown_files();
        let last = shown.len().checked_sub(1)?;
        let at = shown
            .iter()
            .position(|(file, _)| Some(file.file_path.as_str()) == self.selected.as_deref());
        let next = at.map_or(0, |at| at.saturating_add_signed(offset).min(last));
        shown.get(next).map(|(file, _)| file.file_path.clone())
    }
}

/// `content` split into lines as comment anchors count them (`\n` or `\r\n` endings; a final
/// line ending does not start another line).
pub fn source_lines(content: &str) -> Vec<String> {
    content
        .lines()
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
        .collect()
}

/// Line number `n` right-aligned to the width of the largest one (`total`), for a monospaced
/// gutter.
pub fn gutter(n: usize, total: usize) -> String {
    let width = total.max(1).to_string().len();
    format!("{n:>width$}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_protocol::{CommentId, CommentStatus, DocChangedEvent, DocComment};

    fn file(path: &str, open: u32) -> DocFile {
        DocFile {
            file_path: path.into(),
            open_comments: open,
            last_session_id: None,
            last_changed_at: None,
        }
    }

    fn content(path: &str, text: &str, hash: &str) -> DocContent {
        DocContent {
            file_path: path.into(),
            content: text.into(),
            hash: hash.into(),
        }
    }

    fn shown(path: &str, text: &str, hash: &str) -> Shown {
        Shown::new(&content(path, text, hash))
    }

    #[test]
    fn code_blocks_are_highlighted_once_also_inside_lists() {
        let doc = "```rust\nfn main() {}\n```\n\n- item\n\n  ```\n  plain\n  ```\n\n```ts\nlet a = 1;\n```\n";
        let shown = shown("a.md", doc, "h");
        let rust = &shown.code[&1];
        assert_eq!(rust.syntax.as_deref(), Some("Rust"));
        assert!(!rust.spans[0].is_empty(), "colored");
        let nested = shown.code.get(&7).expect("the code block in the list item");
        assert_eq!(nested.lines, ["plain"]);
        assert_eq!(nested.syntax, None);
        assert_eq!(
            shown.code[&11].syntax.as_deref(),
            Some("JavaScript"),
            "ts reads as js"
        );
    }

    #[test]
    fn mermaid_blocks_are_drawn_and_other_code_is_not() {
        let doc = "```mermaid\ngraph TD\n  A --> B\n```\n\n```sh\nls\n```\n";
        let shown = shown("a.md", doc, "h");
        assert!(matches!(shown.diagrams.get(&1), Some(Ok(_))));
        assert!(!shown.diagrams.contains_key(&6));
        assert!(
            shown.code.contains_key(&1),
            "the source stays for a failed drawing"
        );
    }

    fn changed(project_id: Ulid, path: &str, hash: Option<&str>) -> Event {
        Event::DocChanged(DocChangedEvent {
            project_id,
            file_path: path.into(),
            hash: hash.map(Into::into),
        })
    }

    fn shown_hash(docs: &Docs) -> Option<&str> {
        match &docs.doc {
            Doc::Shown(shown) => Some(&shown.hash),
            _ => None,
        }
    }

    #[test]
    fn first_listing_selects_the_first_file_and_reads_it() {
        let mut docs = Docs::new(Ulid::new());
        assert!(docs.begin_list());
        let (read, again) = docs.listed(Ok(vec![file("PLAN.md", 2), file("docs/a.md", 0)]));
        assert_eq!(read.as_deref(), Some("PLAN.md"));
        assert!(!again);
        assert_eq!(docs.doc, Doc::Loading);
        docs.read(
            "PLAN.md",
            Ok(shown("PLAN.md", "# Plan\r\n\r\nGo.\r\n", "h1")),
        );
        assert_eq!(shown_hash(&docs), Some("h1"));
        let Doc::Shown(shown) = &docs.doc else {
            unreachable!()
        };
        assert_eq!(shown.lines, ["# Plan", "", "Go."]);
        assert_eq!(shown.blocks.len(), 2);
        assert_eq!(docs.selected_file().map(|f| f.open_comments), Some(2));
        // A later listing keeps the selection.
        assert!(docs.begin_list());
        assert_eq!(docs.listed(Ok(vec![file("docs/a.md", 0)])).0, None);
        assert_eq!(docs.selected.as_deref(), Some("PLAN.md"));
        assert_eq!(docs.selected_file(), None);
    }

    #[test]
    fn empty_and_failed_listings_select_nothing() {
        let mut docs = Docs::new(Ulid::new());
        docs.begin_list();
        assert_eq!(docs.listed(Ok(vec![])), (None, false));
        assert_eq!(docs.files, Files::Loaded(vec![]));
        assert_eq!(docs.doc, Doc::None);
        docs.begin_list();
        assert_eq!(docs.listed(Err("boom".into())), (None, false));
        assert_eq!(docs.files, Files::Failed("boom".into()));
    }

    #[test]
    fn a_listing_asked_for_while_one_is_in_flight_runs_after_it() {
        let mut docs = Docs::new(Ulid::new());
        assert!(docs.begin_list());
        assert!(!docs.begin_list());
        assert!(docs.listed(Ok(vec![])).1);
        assert!(docs.begin_list());
        assert!(!docs.listed(Ok(vec![])).1);
    }

    #[test]
    fn selecting_reads_once_and_retries_failures_and_ignores_stale_reads() {
        let mut docs = Docs::new(Ulid::new());
        assert_eq!(docs.select("a.md").as_deref(), Some("a.md"));
        assert_eq!(docs.select("b.md").as_deref(), Some("b.md"));
        // The answer for `a.md` arrives late.
        docs.read("a.md", Ok(shown("a.md", "a", "ha")));
        assert_eq!(docs.doc, Doc::Loading);
        docs.read("b.md", Err("too large".into()));
        assert_eq!(docs.doc, Doc::Failed("too large".into()));
        assert_eq!(docs.select("b.md").as_deref(), Some("b.md"));
        docs.read("b.md", Ok(shown("b.md", "b", "hb")));
        assert_eq!(docs.select("b.md"), None);
        assert_eq!(shown_hash(&docs), Some("hb"));
    }

    /// Each time a file gets selected it is a new opening (its preview starts at the top);
    /// selecting the shown file again is not. Another workspace's Docs never shares one.
    #[test]
    fn every_selection_of_another_file_is_a_new_opening() {
        let mut docs = Docs::new(Ulid::new());
        docs.select("a.md");
        let a = docs.opening;
        docs.read("a.md", Ok(shown("a.md", "a", "ha")));
        assert_eq!(docs.select("a.md"), None);
        assert_eq!(docs.opening, a, "the shown file again");
        docs.select("b.md");
        assert_ne!(docs.opening, a);
        docs.select("a.md");
        let again = docs.opening;
        assert_ne!(again, a, "back to a.md: opened again");
        let mut other = Docs::new(Ulid::new());
        other.select("a.md");
        assert_ne!(other.opening, again, "the same path in another workspace");
    }

    #[test]
    fn doc_changed_rereads_the_shown_file_once_per_new_hash() {
        let project = Ulid::new();
        let mut docs = Docs::new(project);
        docs.select("a.md");
        docs.read("a.md", Ok(shown("a.md", "a", "h1")));
        // Same hash (e.g. our own `doc.read` made the daemon notice it): only re-list.
        let same = docs.apply(&changed(project, "a.md", Some("h1")));
        assert_eq!(
            same,
            Reaction {
                relist: true,
                reread: None
            }
        );
        let edited = docs.apply(&changed(project, "a.md", Some("h2")));
        assert_eq!(edited.reread.as_deref(), Some("a.md"));
        assert_eq!(docs.doc, Doc::Loading);
        // Another file of the workspace: re-list only; another workspace: nothing.
        let other = docs.apply(&changed(project, "b.md", Some("x")));
        assert_eq!((other.relist, other.reread), (true, None));
        assert_eq!(
            docs.apply(&changed(Ulid::new(), "a.md", Some("h3"))),
            Reaction::default()
        );
        // A read sent before the change answers late with the old content: dropped.
        docs.read("a.md", Ok(shown("a.md", "a", "h1")));
        assert_eq!(docs.doc, Doc::Loading);
        docs.read("a.md", Ok(shown("a.md", "a2", "h2")));
        assert_eq!(shown_hash(&docs), Some("h2"));
    }

    #[test]
    fn a_deleted_shown_file_says_so_and_comes_back_on_select() {
        let project = Ulid::new();
        let mut docs = Docs::new(project);
        docs.select("a.md");
        docs.read("a.md", Ok(shown("a.md", "a", "h1")));
        let reaction = docs.apply(&changed(project, "a.md", None));
        assert_eq!((reaction.relist, reaction.reread), (true, None));
        assert_eq!(docs.doc, Doc::Deleted);
        assert_eq!(docs.select("a.md").as_deref(), Some("a.md"));
    }

    #[test]
    fn comment_updates_relist_for_their_workspace_and_docs_only() {
        let project = Ulid::new();
        let mut docs = Docs::new(project);
        let event = |project_id, context| {
            Event::CommentUpdated(Box::new(DocComment {
                id: CommentId(Ulid::new()),
                project_id,
                file_path: "a.md".into(),
                context,
                parent_id: None,
                author: DocComment::USER.into(),
                body: "Why?".into(),
                start_line: Some(1),
                end_line: Some(1),
                quote: Some("a".into()),
                status: CommentStatus::Open,
                sent_to: None,
                sent_snapshot: None,
                created_at: 1,
                updated_at: 1,
            }))
        };
        let comment = |project_id| event(project_id, CommentContext::Doc);
        assert!(docs.apply(&comment(project)).relist);
        assert_eq!(docs.apply(&comment(Ulid::new())), Reaction::default());
        assert_eq!(
            docs.apply(&event(project, CommentContext::Diff)),
            Reaction::default(),
            "a diff comment on the same file"
        );
    }

    #[test]
    fn mode_toggles_between_preview_and_source() {
        let mut docs = Docs::new(Ulid::new());
        assert_eq!(docs.mode, Mode::Preview);
        docs.toggle_mode();
        assert_eq!(docs.mode, Mode::Source);
        docs.toggle_mode();
        assert_eq!(docs.mode, Mode::Preview);
    }

    #[test]
    fn source_lines_and_gutter() {
        assert_eq!(source_lines("a\r\nb\n\nc"), ["a", "b", "", "c"]);
        assert_eq!(source_lines("a\n"), ["a"]);
        assert!(source_lines("").is_empty());
        assert_eq!(gutter(7, 120), "  7");
        assert_eq!(gutter(120, 120), "120");
        assert_eq!(gutter(1, 0), "1");
    }

    fn paths(docs: &Docs) -> Vec<String> {
        docs.shown_files()
            .into_iter()
            .map(|(file, _)| file.file_path.clone())
            .collect()
    }

    fn three_files() -> Docs {
        let mut docs = Docs::new(Ulid::new());
        docs.begin_list();
        docs.listed(Ok(vec![
            file("PLAN.md", 0),
            file("docs/plan-v2.md", 0),
            file("docs/notes.md", 1),
        ]));
        docs
    }

    #[test]
    fn the_filter_ranks_paths_and_survives_relisting() {
        let mut docs = three_files();
        assert_eq!(
            paths(&docs),
            ["PLAN.md", "docs/plan-v2.md", "docs/notes.md"]
        );
        docs.filter = "notes".into();
        assert_eq!(paths(&docs), ["docs/notes.md"]);
        assert_eq!(docs.shown_files()[0].1, [5, 6, 7, 8, 9]);
        // Equal scores keep the daemon's order.
        docs.filter = "md".into();
        assert_eq!(
            paths(&docs),
            ["PLAN.md", "docs/plan-v2.md", "docs/notes.md"]
        );
        // A relist (doc.changed / comment.updated) keeps the filter.
        docs.filter = "v2".into();
        docs.begin_list();
        docs.listed(Ok(vec![
            file("docs/plan-v2.md", 3),
            file("docs/other.md", 0),
        ]));
        assert_eq!(paths(&docs), ["docs/plan-v2.md"]);
        docs.filter = "zz".into();
        assert!(paths(&docs).is_empty());
    }

    #[test]
    fn arrows_in_the_filter_move_over_the_shown_files() {
        let mut docs = three_files();
        docs.filter = "docs".into();
        assert_eq!(
            docs.step_selection(1).as_deref(),
            Some("docs/plan-v2.md"),
            "the selected PLAN.md is filtered out: the first shown file"
        );
        docs.select("docs/plan-v2.md");
        assert_eq!(docs.step_selection(1).as_deref(), Some("docs/notes.md"));
        docs.select("docs/notes.md");
        assert_eq!(
            docs.step_selection(1).as_deref(),
            Some("docs/notes.md"),
            "stays at the end"
        );
        assert_eq!(docs.step_selection(-1).as_deref(), Some("docs/plan-v2.md"));
        docs.filter = "zz".into();
        assert_eq!(docs.step_selection(1), None);
    }
}
