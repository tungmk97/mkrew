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
        index_text: None,
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
    let revealed = view.reveal("src/app/main.rs", None);
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
    assert_eq!(view.reveal("src/app/main.rs", None), Requests::default());
    // A file at the root opens no folder.
    let root = view.reveal("README.md", None);
    assert!(root.list.is_empty());
    assert_eq!(root.read.as_deref(), Some("README.md"));
}

#[test]
fn revealing_a_folder_opens_it_and_its_parents() {
    let mut view = listed();
    let revealed = view.reveal_dir("src/app");
    assert_eq!(revealed.list, ["src", "src/app"]);
    assert_eq!(revealed.read, None);
    assert_eq!(view.cursor.as_deref(), Some("src/app"));
}

/// Ctrl+Click on `README.md:3`: the caret goes to the line once the read answers.
#[test]
fn a_link_line_waits_for_the_read_and_puts_the_caret_there() {
    let mut view = listed();
    let requests = view.reveal("README.md", Some(3));
    assert_eq!(requests.read.as_deref(), Some("README.md"));
    assert_eq!(view.apply_pending_line(), None, "still loading");
    view.begin_read();
    view.read("README.md", file_text("a\nb\nc\nd\n"));
    assert_eq!(view.apply_pending_line(), Some(2));
    assert_eq!(view.line(), Some(3));
    assert_eq!(view.apply_pending_line(), None, "applied once");
}

#[test]
fn a_line_past_the_end_lands_on_the_last_line() {
    let mut view = opened("a\nb");
    view.reveal("README.md", Some(99));
    assert_eq!(view.apply_pending_line(), Some(1));
    assert_eq!(view.line(), Some(2));
}

/// The file is already shown: nothing is read, the line applies at once, and unsaved text
/// stays.
#[test]
fn a_line_in_the_shown_file_applies_at_once_and_keeps_unsaved_text() {
    let mut view = opened("a\nb\nc");
    view.editor_mut().unwrap().insert("x");
    assert_eq!(view.reveal("README.md", Some(2)), Requests::default());
    assert_eq!(view.apply_pending_line(), Some(1));
    assert_eq!(view.line(), Some(2));
    assert_eq!(text(&view), "xa\nb\nc");
    assert!(view.unsaved().is_some());
}

#[test]
fn a_failed_read_drops_the_line() {
    let mut view = listed();
    view.reveal("README.md", Some(5));
    view.begin_read();
    view.read("README.md", Err("gone".into()));
    assert_eq!(view.apply_pending_line(), None);
    // Retry shows the file from the top.
    assert_eq!(view.open("README.md").as_deref(), Some("README.md"));
    view.begin_read();
    view.read("README.md", file_text("a\nb\n"));
    assert_eq!(view.apply_pending_line(), None);
    assert_eq!(view.line(), Some(1));
}

/// A page for tests: what the background build would return.
fn page(text: &str) -> docs::Shown {
    docs::Shown::from_text(text)
}

#[test]
fn markdown_opens_as_a_preview_that_is_parsed_once_per_text() {
    let mut view = opened("# T\n");
    assert!(view.is_markdown());
    assert_eq!(view.mode, Mode::Preview);
    assert!(view.previewing());
    let (stamp, text) = view.preview_request().expect("a new text is parsed");
    assert_eq!(text, "# T\n");
    // Asked once while it is being built.
    assert_eq!(view.preview_request(), None);
    view.preview_ready(stamp, page(&text));
    assert_eq!(
        view.preview().map(|shown| shown.lines.clone()),
        Some(vec!["# T".to_owned()])
    );
    assert_eq!(view.preview_request(), None, "the text did not change");
}

#[test]
fn a_late_preview_never_replaces_a_newer_one() {
    let mut view = opened("a\n");
    let (old, _) = view.preview_request().unwrap();
    view.toggle_mode();
    view.editor_mut().unwrap().insert("b");
    view.toggle_mode();
    let (new, _) = view.preview_request().unwrap();
    view.preview_ready(new, page("ba\n"));
    view.preview_ready(old, page("a\n"));
    assert_eq!(
        view.preview().map(|shown| shown.lines.clone()),
        Some(vec!["ba".to_owned()])
    );
    assert_eq!(view.preview_request(), None);
}

/// Each time a file gets opened it is a new opening (its preview starts at the top);
/// clicking the opened file again or re-reading it is not.
#[test]
fn every_open_of_another_file_is_a_new_opening() {
    let mut view = opened("a\n");
    let first = view.opening;
    assert_eq!(view.open("README.md"), None);
    view.begin_read();
    view.read("README.md", file_text("b\n"));
    assert_eq!(view.opening, first, "the same file re-read");
    view.open("src/main.rs");
    assert_ne!(view.opening, first);
    view.open("README.md");
    let again = view.opening;
    assert_ne!(again, first, "back to README.md: opened again");
    assert_ne!(opened("a\n").opening, again, "another workspace's view");
}

#[test]
fn another_file_starts_in_preview_without_the_old_page() {
    let mut view = opened("a\n");
    let (stamp, _) = view.preview_request().unwrap();
    view.preview_ready(stamp, page("a\n"));
    view.toggle_mode();
    view.open("src/main.rs");
    assert_eq!(view.mode, Mode::Preview);
    assert!(view.preview().is_none());
}

#[test]
fn a_reread_that_changes_the_text_builds_a_new_page() {
    let mut view = opened("a\n");
    let (stamp, _) = view.preview_request().unwrap();
    view.preview_ready(stamp, page("a\n"));
    view.begin_read();
    view.read("README.md", file_text("b\n"));
    assert!(view.preview().is_none(), "no page of the old text");
    assert_eq!(
        view.preview_request().map(|(_, text)| text).as_deref(),
        Some("b\n")
    );
}

#[test]
fn only_markdown_has_a_preview() {
    let mut view = listed();
    view.open("src/main.rs");
    view.begin_read();
    view.read("src/main.rs", file_text_at("src/main.rs", "fn a() {}"));
    assert!(!view.is_markdown());
    assert!(!view.previewing());
    assert_eq!(view.preview_request(), None);
}

/// Find and a link's line need the editor on screen.
#[test]
fn find_and_a_link_line_switch_to_source() {
    let mut view = opened("a\nb\n");
    assert!(view.open_find(false));
    assert_eq!(view.mode, Mode::Source);
    let mut view = opened("a\nb\n");
    view.reveal("README.md", Some(2));
    assert_eq!(view.mode, Mode::Source);
    // A plain reveal (quick open) of the same file keeps the mode.
    let mut view = opened("a\n");
    view.reveal("README.md", None);
    assert_eq!(view.mode, Mode::Preview);
}

#[test]
fn unsaved_edits_survive_the_toggle_and_the_preview_shows_them() {
    let mut view = opened("a\n");
    let (stamp, _) = view.preview_request().unwrap();
    view.preview_ready(stamp, page("a\n"));
    view.toggle_mode();
    assert_eq!(view.mode, Mode::Source);
    assert_eq!(view.preview_request(), None, "nothing to build in Source");
    view.editor_mut().unwrap().insert("x");
    view.toggle_mode();
    let (_, typed) = view.preview_request().expect("the edited text is parsed");
    assert_eq!(typed, "xa\n");
    assert_eq!(text(&view), "xa\n");
    assert!(view.unsaved().is_some());
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
fn a_save_that_comes_back_after_the_file_was_left_is_reported() {
    let mut view = opened("a\n");
    view.editor_mut().unwrap().insert("b");
    let save = view.begin_save().unwrap();
    // The user opened another file while the write was in flight.
    view.open("src/main.rs");
    let failed = view.saved("README.md", save.point, Err("disk full".into()));
    assert_eq!(failed.as_deref(), Some("Not saved: disk full"));
    let newer = FsWritten {
        saved: false,
        hash: "#other".into(),
    };
    assert_eq!(
        view.saved("README.md", None, Ok(newer)).as_deref(),
        Some("Not saved: the file changed on disk meanwhile.")
    );
    let fine = FsWritten {
        saved: true,
        hash: "#x".into(),
    };
    assert_eq!(
        view.saved("README.md", None, Ok(fine)),
        None,
        "a good save is no news"
    );
    // Shown: the banner has it, nothing more to say.
    let mut view = opened("a\n");
    view.editor_mut().unwrap().insert("b");
    let save = view.begin_save().unwrap();
    assert_eq!(
        view.saved("README.md", save.point, Err("disk full".into())),
        None
    );
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

fn file_with_base(text: &str, base: &str) -> Result<(FsContent, Code), String> {
    let (mut content, _) = file_text(text)?;
    content.index_text = Some(base.into());
    let code = Code::new(&content);
    Ok((content, code))
}

fn marks(view: &FilesView) -> Vec<Option<crate::gutter::Change>> {
    view.shown()
        .map(|shown| shown.code.changes.clone())
        .unwrap_or_default()
}

/// Staging or committing leaves the file on disk as it is (same hash) but moves the index:
/// the gutter follows, caret and undo stay. With unsaved text the new index text waits for
/// the next typing pause.
#[test]
fn a_new_index_text_moves_the_gutter_and_keeps_the_caret() {
    use crate::gutter::Change::Added;
    let mut view = listed();
    view.open("README.md");
    view.begin_read();
    view.read("README.md", file_with_base("a\nb\n", "a\n"));
    assert_eq!(marks(&view), [None, Some(Added)]);
    view.editor_mut().unwrap().set_cursor(Pos::new(1, 0), false);

    view.begin_read();
    view.read("README.md", file_with_base("a\nb\n", "a\nb\n"));
    assert_eq!(marks(&view), [None, None], "staged: no marks");
    assert_eq!(view.line(), Some(2), "the caret stayed");

    view.editor_mut().unwrap().insert("x");
    view.begin_read();
    view.read("README.md", file_with_base("a\nb\n", "a\n"));
    assert_eq!(text(&view), "a\nxb\n", "unsaved text stays");
    let version = view.shown().unwrap().editor.version();
    let request = view.highlight_request(version).unwrap();
    assert_eq!(
        request.index_text.as_deref(),
        Some("a\n"),
        "the pause diffs with it"
    );
    assert_eq!(
        Code::new(&request).changes,
        [None, Some(Added)],
        "xb is new against a"
    );
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

use crate::preview_select::Point;
use crate::task_dialog::DialogKey;

/// README.md opened with `text` and its preview built.
fn previewed(text: &str) -> FilesView {
    let mut view = opened(text);
    let (stamp, source) = view.preview_request().expect("a preview to build");
    view.preview_ready(stamp, page(&source));
    view
}

fn point(block: usize, byte: usize) -> Point {
    Point {
        block,
        piece: 0,
        byte,
    }
}

#[test]
fn a_drag_in_the_preview_copies_the_rendered_text() {
    let mut view = previewed("# Title\n\nold para\n\nend\n");
    view.preview_click(point(1, 0), false, 1);
    view.preview_drag(point(1, 3));
    assert_eq!(view.preview_copy().as_deref(), Some("old"));
    view.preview_click(point(2, 3), true, 1);
    assert_eq!(
        view.preview_copy().as_deref(),
        Some("old para\nend"),
        "Shift extends"
    );
    view.preview_click(point(1, 5), false, 2);
    assert_eq!(
        view.preview_copy().as_deref(),
        Some("para"),
        "double click: a word"
    );
    view.preview_select_all();
    assert_eq!(view.preview_copy().as_deref(), Some("Title\nold para\nend"));
    assert!(view.clear_preview_selection());
    assert_eq!(view.preview_copy(), None);
}

#[test]
fn edit_keys_do_nothing_while_previewing() {
    let mut view = previewed("a\n");
    view.preview_click(point(0, 0), false, 1);
    view.preview_select_all();
    let _ = view.preview_copy();
    assert_eq!(text(&view), "a\n");
    assert_eq!(view.unsaved(), None);
}

#[test]
fn applying_a_block_edit_replaces_its_lines_in_one_undo_step() {
    let mut view = previewed("# T\n\nold para\n\nend\n");
    assert!(view.open_block_edit(1));
    assert_eq!(
        view.block_edit.as_ref().map(|edit| edit.text.text()),
        Some("old para")
    );
    view.block_edit_key(DialogKey::SelectAll);
    view.block_edit_key(DialogKey::Insert("new".into()));
    view.block_edit_key(DialogKey::Enter);
    view.block_edit_key(DialogKey::Insert("line".into()));
    assert_eq!(
        view.block_edit_key(DialogKey::Save),
        BlockEditOutcome::Apply
    );
    assert_eq!(view.apply_block_edit(), Ok(()));
    assert_eq!(text(&view), "# T\n\nnew\nline\n\nend\n");
    assert!(view.block_edit.is_none());
    assert_eq!(view.unsaved(), Some("README.md"));
    view.editor_mut().unwrap().key(EditKey::Undo, 10);
    assert_eq!(text(&view), "# T\n\nold para\n\nend\n");
}

#[test]
fn an_emptied_block_leaves_a_blank_line() {
    let mut view = previewed("a\n\nb\n\nc\n");
    assert!(view.open_block_edit(1));
    view.block_edit_key(DialogKey::SelectAll);
    view.block_edit_key(DialogKey::Backspace);
    assert_eq!(view.apply_block_edit(), Ok(()));
    assert_eq!(text(&view), "a\n\n\n\nc\n");
}

#[test]
fn a_block_edit_is_refused_once_the_text_changed() {
    let mut view = previewed("a\n\nb\n");
    assert!(view.open_block_edit(1));
    view.editor_mut().unwrap().insert("x");
    assert_eq!(view.apply_block_edit(), Err(StaleEdit));
    assert_eq!(text(&view), "xa\n\nb\n", "nothing written");
    assert!(view.block_edit.is_some(), "the typed text stays to copy");
    view.block_edit = None;
    assert!(!view.open_block_edit(0), "the page is of the old text");
}

#[test]
fn esc_cancels_and_leaving_the_preview_drops_edit_and_selection() {
    let mut view = previewed("a\n\nb\n");
    assert!(view.open_block_edit(0));
    assert_eq!(
        view.block_edit_key(DialogKey::Cancel),
        BlockEditOutcome::Cancel
    );
    view.preview_click(point(0, 0), false, 1);
    view.preview_drag(point(0, 1));
    view.toggle_mode();
    assert!(view.block_edit.is_none() && view.preview_selection.is_none());
    view.toggle_mode();
    assert!(view.open_block_edit(0));
    view.preview_select_all();
    view.open("src/main.rs");
    assert!(view.block_edit.is_none() && view.preview_selection.is_none());
}

#[test]
fn a_new_page_clears_the_selection() {
    let mut view = previewed("a\n");
    assert!(view.open_block_edit(0));
    assert_eq!(view.apply_block_edit(), Ok(()));
    view.preview_select_all();
    assert!(
        view.preview_selection.is_some(),
        "the old page is still shown"
    );
    let (stamp, source) = view.preview_request().expect("the edited text is parsed");
    view.preview_ready(stamp, page(&source));
    assert_eq!(view.preview_selection, None);
}

#[test]
fn a_new_page_reports_the_block_edit_it_closed() {
    let mut view = previewed("a\n\nb\n");
    assert!(view.open_block_edit(0));
    view.editor_mut().unwrap().insert("x");
    let (stamp, source) = view.preview_request().expect("the new text is parsed");
    assert!(view.preview_ready(stamp, page(&source)), "an edit was open");
    assert!(view.block_edit.is_none());
    view.editor_mut().unwrap().insert("y");
    let (stamp, source) = view.preview_request().expect("the new text is parsed");
    assert!(!view.preview_ready(stamp, page(&source)), "nothing open");
}

#[test]
fn ctrl_f_leaves_the_preview_and_drops_edit_and_selection() {
    let mut view = previewed("a\n\nb\n");
    assert!(view.open_block_edit(0));
    view.preview_select_all();
    assert!(view.open_find(false));
    assert!(view.block_edit.is_none() && view.preview_selection.is_none());
}
