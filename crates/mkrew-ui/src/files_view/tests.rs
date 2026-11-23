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

/// A view showing README.md as read with `text` (one transient tab).
fn opened(text: &str) -> FilesView {
    let mut view = listed();
    view.open("README.md", Open::Transient);
    view.begin_read("README.md");
    view.read("README.md", file_text(text));
    view
}

fn text(view: &FilesView) -> String {
    view.shown()
        .map(|shown| shown.editor.text())
        .unwrap_or_default()
}

/// The active tab (most tests show one file).
fn tab(view: &FilesView) -> &Doc {
    view.tab().expect("a tab is open")
}

fn tab_mut(view: &mut FilesView) -> &mut Doc {
    view.tab_mut().expect("a tab is open")
}

/// The first unsaved tab's path.
fn unsaved(view: &FilesView) -> Option<String> {
    view.unsaved_paths().into_iter().next()
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
    let opened = view.click("src", Open::Transient);
    assert_eq!(opened.list, ["src"]);
    assert!(view.begin_list("src"));
    assert_eq!(paths(&view), ["src/", "  (Loading…)", "README.md"]);
    view.listed(
        "src",
        Ok(vec![file("src/main.rs", Some(GitMarker::Modified))]),
    );
    assert_eq!(paths(&view), ["src/", "  src/main.rs", "README.md"]);
    // Closing and reopening reuses the listing.
    assert_eq!(view.click("src", Open::Transient), Requests::default());
    assert_eq!(paths(&view), ["src", "README.md"]);
    assert_eq!(view.click("src", Open::Transient), Requests::default());
    assert_eq!(paths(&view), ["src/", "  src/main.rs", "README.md"]);
}

#[test]
fn revealing_a_file_opens_its_folders_and_shows_it() {
    let mut view = listed();
    let revealed = view.reveal("src/app/main.rs", None, Open::Transient);
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
    assert_eq!(
        view.reveal("src/app/main.rs", None, Open::Transient),
        Requests::default()
    );
    // A file at the root opens no folder.
    let root = view.reveal("README.md", None, Open::Transient);
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

/// A page for tests: what the background build would return.
fn page(text: &str) -> docs::Shown {
    docs::Shown::from_text(text)
}

/// Find and a link's line need the editor on screen.
#[test]
fn find_and_a_link_line_switch_to_source() {
    let mut view = opened("a\nb\n");
    assert!(view.open_find(false));
    assert_eq!(tab(&view).mode, MdMode::Source);
    let mut view = opened("a\nb\n");
    view.reveal("README.md", Some(2), Open::Transient);
    assert_eq!(tab(&view).mode, MdMode::Source);
    // A plain reveal (quick open) of the same file keeps the mode.
    let mut view = opened("a\n");
    view.reveal("README.md", None, Open::Transient);
    assert_eq!(tab(&view).mode, MdMode::Preview);
}

#[test]
fn a_failed_folder_says_so_and_retries_when_reopened() {
    let mut view = listed();
    view.click("src", Open::Transient);
    view.begin_list("src");
    view.listed("src", Err("gone".into()));
    assert_eq!(
        paths(&view),
        ["src/", "  (Cannot list this folder: gone)", "README.md"]
    );
    view.click("src", Open::Transient);
    assert_eq!(view.click("src", Open::Transient).list, ["src"]);
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
    let click = |view: &mut FilesView, path| view.click(path, Open::Transient).read;
    assert_eq!(click(&mut view, "README.md").as_deref(), Some("README.md"));
    assert!(view.begin_read("README.md"));
    assert_eq!(click(&mut view, "README.md"), None);
    // Asked again while the read is in flight: it is read once more after it answers.
    assert!(!view.begin_read("README.md"));
    assert!(!view.begin_read("README.md"));
    let again = view.read("README.md", file_text("old"));
    assert_eq!(again.as_deref(), Some("README.md"));
    assert_eq!(click(&mut view, "src/main.rs"), None, "not a visible row");
    view.open("src/main.rs", Open::Kept);
    assert_eq!(tab(&view).content, Content::Loading);
    assert!(view.begin_read("src/main.rs"));
    assert_eq!(view.read("src/main.rs", Err("too large".into())), None);
    assert_eq!(tab(&view).content, Content::Failed("too large".into()));
    // Opening it again retries.
    assert_eq!(
        view.open("src/main.rs", Open::Transient).as_deref(),
        Some("src/main.rs")
    );
    assert_eq!(tab(&view).content, Content::Loading);
    assert!(view.begin_read("src/main.rs"));
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
    view.click("src", Open::Transient);
    view.begin_list("src");
    view.listed("src", Ok(vec![dir("src/a")]));
    view.click("src/a", Open::Transient);
    view.begin_list("src/a");
    view.listed("src/a", Ok(vec![]));
    view.click("src/a", Open::Transient);
    view.click("README.md", Open::Transient);
    view.begin_read("README.md");
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
    assert_eq!(view.click("src/a", Open::Transient).list, ["src/a"]);
    // Old rows stay on screen while the re-lists run.
    assert_eq!(paths(&view)[0], "src/");
}

#[test]
fn refresh_skips_folders_inside_closed_or_deleted_ones_and_drops_stale_answers() {
    let mut view = listed();
    view.click("src", Open::Transient);
    view.begin_list("src");
    view.listed("src", Ok(vec![dir("src/a")]));
    view.click("src/a", Open::Transient);
    assert!(view.begin_list("src/a"));
    // `src` closes while `src/a`'s listing is in flight: only the root is re-listed.
    view.click("src", Open::Transient);
    assert_eq!(view.refresh().list, [""]);
    // The answer predates the refresh and `src/a` is not on screen: dropped. Reopening
    // `src` lists it fresh, and its still-open `src/a` follows.
    assert!(view.listed("src/a", Ok(vec![])).is_empty());
    assert_eq!(view.click("src", Open::Transient).list, ["src"]);
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
    view.click("src", Open::Transient);
    view.begin_list("src");
    view.listed("src", Ok(vec![dir("src/a")]));
    view.click("src/a", Open::Transient);
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
    view.open("README.md", Open::Transient);
    view.begin_read("README.md");
    view.read("README.md", file_text("todo\nok"));
    let mut find = TextFind::default();
    find.query = "todo".into();
    view.find = Some(find);
    view.search();
    let label = |view: &FilesView| view.find.as_ref().map(TextFind::label);
    assert_eq!(label(&view).as_deref(), Some("1 of 1"));
    // An agent edits the file: the re-read is searched again.
    view.begin_read("README.md");
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
    assert_eq!(unsaved(&view).as_deref(), Some("README.md"));
    let label = view.find.as_ref().map(TextFind::label);
    assert_eq!(label.as_deref(), Some("1 of 3"));
    assert!(view.editor_mut().unwrap().undo());
    assert_eq!(text(&view), "Tiếng việt, VIỆT\nviệt\n");
    assert_eq!(unsaved(&view), None);
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
fn enter_opens_the_cursor_row_only_while_it_is_listed() {
    let mut view = opened("a\n");
    assert_eq!(view.enter_target(), None, "no cursor yet");
    view.cursor = Some("README.md".into());
    assert_eq!(view.enter_target().as_deref(), Some("README.md"));
    view.cursor = Some("gone.txt".into());
    assert_eq!(view.enter_target(), None, "the row is gone");
}

/// README.md opened with `text` and its preview built.
fn previewed(text: &str) -> FilesView {
    let mut view = opened(text);
    let (stamp, source) = tab_mut(&mut view)
        .preview_request()
        .expect("a preview to build");
    view.preview_ready(stamp, page(&source));
    view
}

#[test]
fn ctrl_f_leaves_the_preview_and_drops_edit_and_selection() {
    let mut view = previewed("a\n\nb\n");
    let tab = tab_mut(&mut view);
    assert!(tab.open_block_edit(0));
    tab.preview_select_all();
    assert!(view.open_find(false));
    let tab = view.tab().unwrap();
    assert!(tab.block_edit.is_none() && tab.preview_selection.is_none());
}

/// A view with README.md, src/main.rs and src/lib.rs listed (src open).
fn three() -> FilesView {
    let mut view = listed();
    view.toggle("src");
    view.begin_list("src");
    view.listed(
        "src",
        Ok(vec![file("src/main.rs", None), file("src/lib.rs", None)]),
    );
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
    assert_eq!(
        view.click("README.md", Open::Transient).read.as_deref(),
        Some("README.md")
    );
    assert_eq!(
        view.click("src/main.rs", Open::Transient).read.as_deref(),
        Some("src/main.rs")
    );
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
    assert_eq!(
        view.click("README.md", Open::Transient).read,
        None,
        "already read"
    );
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
    assert_eq!(
        tab_paths(&view),
        ["src/main.rs", "a.txt", "README.md", "src/lib.rs"]
    );
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
    assert_eq!(
        view.close_targets("src/main.rs", Close::One),
        ["src/main.rs"]
    );
    assert_eq!(
        view.close_targets("src/main.rs", Close::Others),
        ["src/lib.rs", "a.txt"]
    );
    assert_eq!(
        view.close_targets("src/main.rs", Close::Right),
        ["src/lib.rs", "a.txt"]
    );
    assert_eq!(view.close_targets("src/lib.rs", Close::Right), ["a.txt"]);
    assert_eq!(
        view.close_targets("README.md", Close::All),
        ["src/main.rs", "src/lib.rs", "a.txt"]
    );
    assert_eq!(
        view.close_targets("gone.rs", Close::All),
        Vec::<String>::new()
    );
}

#[test]
fn the_editor_chip_close_takes_every_tab_pinned_too() {
    let mut view = three();
    for path in ["README.md", "src/main.rs"] {
        view.open(path, Open::Kept);
    }
    view.pin("README.md", true);
    view.pin("src/main.rs", true);
    assert_eq!(
        view.close_targets("README.md", Close::Every),
        ["README.md", "src/main.rs"]
    );
    assert!(view.close_targets("README.md", Close::All).is_empty());
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
    assert_eq!(
        view.file(),
        Some("src/lib.rs"),
        "the active tab follows its move"
    );
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
fn git_changed_reads_the_active_tab_and_marks_the_others_stale() {
    let mut view = three();
    view.open("README.md", Open::Kept);
    read_ok(&mut view, "README.md", "a\n");
    view.open("src/main.rs", Open::Kept);
    read_ok(&mut view, "src/main.rs", "m\n");
    assert_eq!(view.refresh().read.as_deref(), Some("src/main.rs"));
    assert_eq!(
        view.activate(0).as_deref(),
        Some("README.md"),
        "stale: read once"
    );
    assert_eq!(view.activate(1), None);
    assert_eq!(view.activate(0), None, "read already asked");
}

#[test]
fn a_file_tabs_place_is_counted_inside_its_pin_group() {
    let mut view = three();
    for path in ["README.md", "src/main.rs", "src/lib.rs"] {
        view.open(path, Open::Kept);
    }
    view.pin("README.md", true);
    // The active tab is the last opened: second of the two unpinned tabs.
    assert_eq!(view.active_place(), Some((false, 1, 2)));
    view.activate(0);
    assert_eq!(view.active_place(), Some((true, 0, 1)));
    assert_eq!(three().active_place(), None, "no tab");
}

#[test]
fn split_tabs_open_in_the_last_mode_and_keep_split_for_find_and_links() {
    let mut view = listed();
    view.md_mode = MdMode::Split;
    view.open("README.md", Open::Transient);
    view.begin_read("README.md");
    view.read("README.md", file_text("a\nb\n"));
    assert!(tab(&view).splitting() && !tab(&view).previewing());
    // The editor is on screen next to the page: find and a link's line keep the split.
    assert!(view.open_find(false));
    view.reveal("README.md", Some(2), Open::Transient);
    assert_eq!(tab(&view).mode, MdMode::Split);
}

#[test]
fn the_split_share_follows_the_pointer_within_its_limits() {
    assert_eq!(md_split_share(50), 0.5);
    assert_eq!(md_split_share(5), 0.2);
    assert_eq!(md_split_share(255), 0.8);
    // Halves 1000 px wide from x = 200.
    assert_eq!(md_split_share_at(200.0, 1000.0, 500.0), 0.3);
    assert_eq!(md_split_share_at(200.0, 1000.0, 100.0), 0.2);
    assert_eq!(md_split_share_at(200.0, 1000.0, 1300.0), 0.8);
    assert_eq!(md_split_share_at(0.0, 0.0, 10.0), 0.5, "not painted yet");
}
