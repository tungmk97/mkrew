//! Review comments of the shown doc (SPEC §12) or diff (SPEC §12B, `context = diff`): the line
//! selection a new comment anchors to, the file's threads (`comment.list`, kept live by
//! `comment.updated`), and the one draft being typed (a new comment or a reply). Pure and
//! tested headlessly; `main.rs` sends the requests and renders the threads in the right margin.

use mkrew_protocol::{
    CommentContext, CommentCreateParams, CommentId, CommentReplyParams, CommentStatus, DocComment,
    Request,
};
use ulid::Ulid;

use crate::task_dialog::DialogKey;
use crate::text_box::Input;

/// Lines picked in the preview (a block) or the source (a line); 1-based, inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub start: u32,
    pub end: u32,
}

impl Selection {
    /// Picks `start..=end`; `extend` (Shift+click) grows `current` to cover it as well.
    pub fn pick(current: Option<Selection>, start: u32, end: u32, extend: bool) -> Selection {
        match current {
            Some(current) if extend => Selection {
                start: current.start.min(start),
                end: current.end.max(end),
            },
            _ => Selection { start, end },
        }
    }

    pub fn contains(self, start: u32, end: u32) -> bool {
        start <= self.end && self.start <= end
    }
}

/// "Line 4" or "Lines 4–7".
pub fn lines_label(start: u32, end: u32) -> String {
    if start == end {
        format!("Line {start}")
    } else {
        format!("Lines {start}–{end}")
    }
}

/// The text a comment on `selection` quotes: those source lines, trimmed (the daemon
/// re-anchors by finding it again, whitespace-tolerant).
pub fn quote(lines: &[String], selection: Selection) -> String {
    let start = (selection.start as usize).saturating_sub(1);
    let end = (selection.end as usize).min(lines.len());
    lines
        .get(start..end)
        .unwrap_or_default()
        .join("\n")
        .trim()
        .to_owned()
}

/// A thread: its root (anchor, status) and the replies, oldest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thread<'a> {
    pub root: &'a DocComment,
    pub replies: Vec<&'a DocComment>,
}

/// Threads in anchor order (outdated ones keep their last lines); replies whose root is
/// missing are dropped.
pub fn threads(comments: &[DocComment]) -> Vec<Thread<'_>> {
    let mut threads: Vec<Thread<'_>> = comments
        .iter()
        .filter(|comment| comment.parent_id.is_none())
        .map(|root| Thread {
            root,
            replies: Vec::new(),
        })
        .collect();
    threads.sort_by_key(|thread| (thread.root.start_line, thread.root.created_at));
    let at: std::collections::HashMap<CommentId, usize> = threads
        .iter()
        .enumerate()
        .map(|(index, thread)| (thread.root.id, index))
        .collect();
    for reply in comments.iter().filter(|c| c.parent_id.is_some()) {
        if let Some(&index) = reply.parent_id.as_ref().and_then(|id| at.get(id)) {
            threads[index].replies.push(reply);
        }
    }
    for thread in &mut threads {
        thread.replies.sort_by_key(|reply| reply.created_at);
    }
    threads
}

/// What a thread's status shows: an icon (status is never color alone) and a word.
pub fn status_label(status: CommentStatus) -> (&'static str, &'static str) {
    match status {
        CommentStatus::Open => ("○", "Open"),
        CommentStatus::Sent => ("↑", "Sent"),
        CommentStatus::Addressed => ("↩", "Addressed"),
        CommentStatus::Resolved => ("✓", "Resolved"),
        CommentStatus::Outdated => ("⚠", "Outdated"),
    }
}

/// Resolved threads offer Reopen, every other one Resolve.
pub fn can_reopen(status: CommentStatus) -> bool {
    status == CommentStatus::Resolved
}

/// The shown file's comments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum List {
    Loading,
    Loaded(Vec<DocComment>),
    Failed(String),
}

/// What the draft is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    New(Selection),
    Reply(CommentId),
}

/// What a key press in the draft does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Edited,
    /// Esc: drop the draft.
    Cancel,
    /// Enter: save it.
    Submit,
}

/// The comment or reply being typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub target: Target,
    /// A new comment's quote, taken when the draft opened (the file may change meanwhile).
    pub quote: String,
    pub text: Input,
    /// Its request is in flight.
    pub busy: bool,
    pub error: Option<String>,
    /// Tells this draft's answers from those of a draft dropped before it.
    pub serial: u64,
}

impl Draft {
    fn new(target: Target, quote: String, serial: u64) -> Draft {
        Draft {
            target,
            quote,
            text: Input::default(),
            busy: false,
            error: None,
            serial,
        }
    }

    pub fn key(&mut self, key: DialogKey) -> Outcome {
        match key {
            DialogKey::Cancel => return Outcome::Cancel,
            DialogKey::Enter | DialogKey::Save => return Outcome::Submit,
            // The caller inserts the clipboard text.
            DialogKey::SwitchField
            | DialogKey::Paste
            | DialogKey::Delete { .. }
            | DialogKey::Move { .. }
            | DialogKey::Copy
            | DialogKey::Cut => {}
            key @ (DialogKey::Backspace | DialogKey::DeleteWord | DialogKey::SelectAll) => {
                if !self.busy {
                    self.text.edit(&key);
                }
            }
            DialogKey::Insert(text) => self.insert(&text),
        }
        Outcome::Edited
    }

    /// Appends typed or pasted text (pasted line breaks are kept); ignored while saving.
    pub fn insert(&mut self, text: &str) {
        if self.busy {
            return;
        }
        self.text.insert(&text.replace("\r\n", "\n"));
        self.error = None;
    }
}

/// The comments of one doc (the selected file of a Docs view) or of one diff (the selected
/// file of a Git view), and the draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comments {
    /// Which comments of the file these are (the others are ignored).
    pub context: CommentContext,
    /// The file `list` belongs to.
    pub file_path: Option<String>,
    pub list: List,
    pub selection: Option<Selection>,
    pub draft: Option<Draft>,
    /// Why the last Resolve / Reopen failed.
    pub error: Option<String>,
    /// `comment.updated` that arrived while listing, applied on top of the answer.
    early: Vec<DocComment>,
    /// The last draft's serial (kept across files).
    serial: u64,
}

impl Default for Comments {
    fn default() -> Self {
        Comments::new(CommentContext::Doc)
    }
}

impl Comments {
    pub fn new(context: CommentContext) -> Comments {
        Comments {
            context,
            file_path: None,
            list: List::Loading,
            selection: None,
            draft: None,
            error: None,
            early: Vec::new(),
            serial: 0,
        }
    }

    /// Starts over for `file_path` when it is not this one's, or its listing failed; returns
    /// it when `comment.list` has to be sent.
    pub fn begin_list(&mut self, file_path: &str) -> Option<String> {
        let same = self.file_path.as_deref() == Some(file_path);
        if same && !matches!(self.list, List::Failed(_)) {
            return None;
        }
        if !same {
            *self = Comments {
                serial: self.serial,
                ..Comments::new(self.context)
            };
        }
        self.file_path = Some(file_path.to_owned());
        self.list = List::Loading;
        Some(file_path.to_owned())
    }

    /// The `comment.list` answer for `file_path`; dropped if another file is shown now.
    pub fn listed(&mut self, file_path: &str, listed: Result<Vec<DocComment>, String>) {
        if self.file_path.as_deref() != Some(file_path) {
            return;
        }
        self.list = match listed {
            Ok(comments) => List::Loaded(comments),
            Err(error) => List::Failed(error),
        };
        for comment in std::mem::take(&mut self.early) {
            self.apply(&comment);
        }
    }

    /// A `comment.updated` (or a request's answer) of `project_id` in this context: added, or
    /// replacing an older copy.
    pub fn apply_event(&mut self, project_id: Ulid, comment: &DocComment) {
        if comment.project_id == project_id && comment.context == self.context {
            self.apply(comment);
        }
    }

    fn apply(&mut self, comment: &DocComment) {
        if self.file_path.as_deref() != Some(comment.file_path.as_str()) {
            return;
        }
        let comments = match &mut self.list {
            List::Loaded(comments) => comments,
            List::Loading => &mut self.early,
            List::Failed(_) => return,
        };
        match comments.iter_mut().find(|c| c.id == comment.id) {
            Some(known) if known.updated_at > comment.updated_at => {}
            Some(known) => *known = comment.clone(),
            None => comments.push(comment.clone()),
        }
    }

    /// A click on `start..=end`.
    pub fn pick(&mut self, start: u32, end: u32, extend: bool) {
        self.selection = Some(Selection::pick(self.selection, start, end, extend));
    }

    /// Opens a draft for a new comment on the selection of the shown `lines`, quoting them;
    /// `false` = nothing selected (a selection past the end, after an edit, is dropped).
    pub fn start_comment(&mut self, lines: &[String]) -> bool {
        let quoted = self
            .selection
            .filter(|selection| selection.end as usize <= lines.len())
            .map(|selection| quote(lines, selection));
        self.start_quoted(quoted)
    }

    /// Opens a draft for a new comment on the selection quoting `quoted` (what the caller
    /// read off the shown lines); `false` = nothing selected, or `quoted` is `None` (the
    /// selection is not shown any more: it is dropped).
    pub fn start_quoted(&mut self, quoted: Option<String>) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };
        let Some(quoted) = quoted else {
            self.selection = None;
            return false;
        };
        self.open(Target::New(selection), quoted);
        true
    }

    /// Opens a draft replying to `root`.
    pub fn start_reply(&mut self, root: CommentId) {
        self.open(Target::Reply(root), String::new());
    }

    /// Keeps the draft (and what was typed) if it is for `target` already.
    fn open(&mut self, target: Target, quote: String) {
        if self.draft.as_ref().is_none_or(|d| d.target != target) {
            self.serial += 1;
            self.draft = Some(Draft::new(target, quote, self.serial));
        }
    }

    /// The request saving the draft, marking it busy; `None` while one is in flight, when
    /// nothing but blanks was typed, or when no file is shown.
    pub fn submit(&mut self, project_id: Ulid) -> Option<Request> {
        let file_path = self.file_path.clone()?;
        let draft = self.draft.as_mut()?;
        let body = draft.text.trim();
        if draft.busy || body.is_empty() {
            return None;
        }
        let request = match draft.target {
            Target::New(selection) => Request::CommentCreate(CommentCreateParams {
                project_id,
                file_path,
                start_line: selection.start,
                end_line: selection.end,
                quote: draft.quote.clone(),
                body: body.to_owned(),
                context: self.context,
            }),
            Target::Reply(comment_id) => Request::CommentReply(CommentReplyParams {
                comment_id,
                body: body.to_owned(),
                mkrew_session_id: None,
            }),
        };
        draft.busy = true;
        draft.error = None;
        Some(request)
    }

    /// The answer to the draft's request: saved → the draft (and a new comment's selection)
    /// goes; failed → it stays with the reason. Only the comment is kept if the draft with
    /// `serial` is gone (dropped or replaced meanwhile).
    pub fn submitted(&mut self, serial: u64, saved: Result<DocComment, String>) {
        let Some(draft) = self.draft.as_mut().filter(|d| d.serial == serial) else {
            if let Ok(comment) = &saved {
                self.apply(comment);
            }
            return;
        };
        draft.busy = false;
        match saved {
            Ok(comment) => {
                if matches!(draft.target, Target::New(_)) {
                    self.selection = None;
                }
                self.draft = None;
                self.apply(&comment);
            }
            Err(error) => draft.error = Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comment(id: u128, parent: Option<u128>, lines: Option<(u32, u32)>, at: i64) -> DocComment {
        DocComment {
            id: CommentId(Ulid(id)),
            project_id: Ulid(1),
            file_path: "PLAN.md".into(),
            context: Default::default(),
            parent_id: parent.map(|p| CommentId(Ulid(p))),
            author: DocComment::USER.into(),
            body: format!("body {id}"),
            start_line: lines.map(|l| l.0),
            end_line: lines.map(|l| l.1),
            quote: lines.map(|_| "q".into()),
            status: CommentStatus::Open,
            sent_to: None,
            sent_snapshot: None,
            created_at: at,
            updated_at: at,
        }
    }

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(Into::into).collect()
    }

    #[test]
    fn picking_and_extending_a_selection() {
        let one = Selection::pick(None, 5, 7, true);
        assert_eq!(one, Selection { start: 5, end: 7 });
        assert_eq!(
            Selection::pick(Some(one), 2, 2, true),
            Selection { start: 2, end: 7 }
        );
        assert_eq!(
            Selection::pick(Some(one), 9, 9, false),
            Selection { start: 9, end: 9 }
        );
        assert!(one.contains(7, 8) && one.contains(1, 5) && !one.contains(8, 9));
        assert_eq!(lines_label(3, 3), "Line 3");
        assert_eq!(lines_label(3, 5), "Lines 3–5");
    }

    #[test]
    fn quote_is_the_trimmed_selected_lines() {
        let text = lines("# Plan\n\n  Use Redis  \nfor caching\n");
        assert_eq!(
            quote(&text, Selection { start: 3, end: 4 }),
            "Use Redis  \nfor caching"
        );
        assert_eq!(quote(&text, Selection { start: 4, end: 9 }), "for caching");
        assert_eq!(quote(&text, Selection { start: 9, end: 9 }), "");
    }

    #[test]
    fn threads_follow_anchor_order_with_their_replies() {
        let comments = vec![
            comment(1, None, Some((9, 9)), 1),
            comment(2, None, Some((3, 4)), 2),
            comment(3, Some(1), None, 5),
            comment(4, Some(1), None, 4),
            comment(5, Some(77), None, 6),
        ];
        let threads = threads(&comments);
        let shape: Vec<_> = threads
            .iter()
            .map(|t| {
                let replies: Vec<_> = t.replies.iter().map(|r| r.id.0.0).collect();
                (t.root.id.0.0, replies)
            })
            .collect();
        assert_eq!(shape, [(2, vec![]), (1, vec![4, 3])]);
        assert_eq!(status_label(CommentStatus::Outdated), ("⚠", "Outdated"));
        assert!(can_reopen(CommentStatus::Resolved));
        assert!(!can_reopen(CommentStatus::Addressed));
    }

    #[test]
    fn listing_is_per_file_and_keeps_updates_that_raced_it() {
        let mut comments = Comments::default();
        assert_eq!(comments.begin_list("PLAN.md").as_deref(), Some("PLAN.md"));
        assert_eq!(comments.begin_list("PLAN.md"), None, "in flight");

        // Updated while listing: newer than the answer's copy, so it wins.
        let mut resolved = comment(1, None, Some((1, 1)), 1);
        resolved.status = CommentStatus::Resolved;
        resolved.updated_at = 10;
        comments.apply_event(Ulid(1), &resolved);
        comments.apply_event(Ulid(2), &comment(8, None, Some((1, 1)), 1));
        let mut elsewhere = comment(9, None, Some((1, 1)), 1);
        elsewhere.file_path = "other.md".into();
        comments.apply_event(Ulid(1), &elsewhere);

        comments.listed("other.md", Ok(vec![]));
        assert_eq!(comments.list, List::Loading, "another file's answer");
        comments.listed("PLAN.md", Ok(vec![comment(1, None, Some((1, 1)), 1)]));
        assert_eq!(comments.list, List::Loaded(vec![resolved.clone()]));

        // An older copy never replaces a newer one; new ones are added.
        comments.apply_event(Ulid(1), &comment(1, None, Some((1, 1)), 1));
        comments.apply_event(Ulid(1), &comment(2, Some(1), None, 11));
        let List::Loaded(list) = &comments.list else {
            panic!("not loaded");
        };
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].status, CommentStatus::Resolved);

        // Another file starts over; a failure lists again.
        comments.pick(1, 1, false);
        assert!(comments.begin_list("other.md").is_some());
        assert_eq!(comments.selection, None);
        comments.listed("other.md", Err("down".into()));
        assert_eq!(comments.begin_list("other.md").as_deref(), Some("other.md"));
    }

    #[test]
    fn a_new_comment_needs_a_selection_and_saves_its_quote() {
        let text = lines("# Plan\n\nUse Redis\n");
        let mut comments = Comments::default();
        comments.begin_list("PLAN.md");
        comments.listed("PLAN.md", Ok(vec![]));
        assert!(!comments.start_comment(&text), "nothing selected");
        comments.pick(3, 4, false);
        assert!(!comments.start_comment(&text), "past the end");
        assert_eq!(comments.selection, None);

        comments.pick(3, 3, false);
        assert!(comments.start_comment(&text));
        assert_eq!(comments.submit(Ulid(1)), None, "nothing typed");
        let draft = comments.draft.as_mut().unwrap();
        for key in [
            DialogKey::Insert(" Why".into()),
            DialogKey::Insert("x".into()),
            DialogKey::Backspace,
            DialogKey::Insert("?\r\n".into()),
        ] {
            assert_eq!(draft.key(key), Outcome::Edited);
        }
        assert_eq!(draft.key(DialogKey::Enter), Outcome::Submit);
        let serial = draft.serial;
        assert_eq!(
            comments.submit(Ulid(1)),
            Some(Request::CommentCreate(CommentCreateParams {
                project_id: Ulid(1),
                file_path: "PLAN.md".into(),
                start_line: 3,
                end_line: 3,
                quote: "Use Redis".into(),
                body: "Why?".into(),
                context: Default::default(),
            }))
        );
        assert_eq!(comments.submit(Ulid(1)), None, "in flight");

        comments.submitted(serial, Err("daemon down".into()));
        let draft = comments.draft.as_ref().unwrap();
        assert_eq!(
            (draft.busy, draft.error.as_deref()),
            (false, Some("daemon down"))
        );

        comments.submit(Ulid(1)).unwrap();
        comments.submitted(serial, Ok(comment(1, None, Some((3, 3)), 1)));
        assert_eq!((comments.draft.as_ref(), comments.selection), (None, None));
        assert!(matches!(&comments.list, List::Loaded(list) if list.len() == 1));
    }

    #[test]
    fn diff_comments_ignore_doc_comments_and_save_as_diff() {
        let mut comments = Comments::new(CommentContext::Diff);
        comments.pick(2, 2, false);
        assert!(comments.begin_list("src/a.rs").is_some());
        assert_eq!(comments.context, CommentContext::Diff, "kept across files");
        comments.listed("src/a.rs", Ok(vec![]));
        let mut doc = comment(1, None, Some((1, 1)), 1);
        doc.file_path = "src/a.rs".into();
        comments.apply_event(Ulid(1), &doc);
        let mut diff = doc.clone();
        diff.id = CommentId(Ulid(2));
        diff.context = CommentContext::Diff;
        comments.apply_event(Ulid(1), &diff);
        assert_eq!(comments.list, List::Loaded(vec![diff]));

        assert!(!comments.start_quoted(Some("x".into())), "nothing selected");
        comments.pick(4, 5, false);
        assert!(!comments.start_quoted(None), "lines not shown");
        assert_eq!(comments.selection, None);
        comments.pick(4, 5, false);
        assert!(comments.start_quoted(Some("let a = 1;\nlet b = 2;".into())));
        comments.draft.as_mut().unwrap().insert("Rename");
        let Some(Request::CommentCreate(params)) = comments.submit(Ulid(1)) else {
            panic!("no create");
        };
        assert_eq!(
            (params.context, params.start_line, params.end_line),
            (CommentContext::Diff, 4, 5)
        );
        assert_eq!(params.quote, "let a = 1;\nlet b = 2;");
    }

    #[test]
    fn a_reply_keeps_its_draft_until_saved() {
        let mut comments = Comments::default();
        comments.begin_list("PLAN.md");
        comments.listed("PLAN.md", Ok(vec![comment(1, None, Some((1, 1)), 1)]));
        let root = CommentId(Ulid(1));
        comments.start_reply(root);
        comments.draft.as_mut().unwrap().insert("ok");
        comments.start_reply(root);
        assert_eq!(comments.draft.as_ref().unwrap().text, "ok", "same target");
        let serial = comments.draft.as_ref().unwrap().serial;
        assert_eq!(
            comments.submit(Ulid(1)),
            Some(Request::CommentReply(CommentReplyParams {
                comment_id: root,
                body: "ok".into(),
                mkrew_session_id: None,
            }))
        );

        // Esc'd and the same reply reopened meanwhile: the old answer leaves the new draft.
        comments.draft = None;
        comments.start_reply(root);
        comments.submitted(serial, Ok(comment(2, Some(1), None, 2)));
        assert!(comments.draft.is_some());
        assert!(matches!(&comments.list, List::Loaded(list) if list.len() == 2));
        let draft = comments.draft.as_mut().unwrap();
        assert_eq!(draft.key(DialogKey::Cancel), Outcome::Cancel);
    }

    #[test]
    fn a_draft_takes_select_all_and_ctrl_backspace_unless_it_is_saving() {
        let mut draft = Draft::new(
            Target::New(Selection { start: 1, end: 1 }),
            String::new(),
            1,
        );
        draft.insert("why this");
        draft.key(DialogKey::DeleteWord);
        assert_eq!(draft.text, "why ");
        draft.key(DialogKey::SelectAll);
        draft.key(DialogKey::Insert("ok".into()));
        assert_eq!(draft.text, "ok");

        draft.busy = true;
        draft.key(DialogKey::SelectAll);
        draft.key(DialogKey::DeleteWord);
        assert_eq!(draft.text, "ok", "frozen while saving");
        assert!(!draft.text.all_selected());
    }
}
