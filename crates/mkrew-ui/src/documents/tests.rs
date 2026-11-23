use super::*;
use crate::editor::EditKey;

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
    file_text_at(P, text)
}

const P: &str = "README.md";

/// A page for tests: what the background build would return.
fn page(text: &str) -> docs::Shown {
    docs::Shown::from_text(text)
}

/// A store showing README.md as read with `text`.
fn opened(text: &str) -> Documents {
    let mut docs = Documents::new();
    docs.ensure(P, MdMode::default());
    assert!(docs.begin_read(P));
    docs.read(P, file_text(text));
    docs
}

fn doc(docs: &Documents) -> &Doc {
    docs.get(P).expect("README.md is open")
}

fn doc_mut(docs: &mut Documents) -> &mut Doc {
    docs.get_mut(P).expect("README.md is open")
}

fn editor(docs: &mut Documents) -> &mut Editor {
    doc_mut(docs).editor_mut().expect("editable")
}

fn text(docs: &Documents) -> String {
    (doc(docs).shown())
        .map(|shown| shown.editor.text())
        .unwrap_or_default()
}

/// The first unsaved doc's path.
fn unsaved(docs: &Documents) -> Option<String> {
    docs.unsaved_paths().into_iter().next()
}

/// Another file, read.
fn read_ok(docs: &mut Documents, path: &str, text: &str) {
    docs.ensure(path, MdMode::default());
    assert!(docs.begin_read(path));
    docs.read(path, file_text_at(path, text));
}

#[test]
fn a_doc_is_dropped_when_unused_and_clean_and_kept_when_open_or_dirty() {
    let mut docs = Documents::new();
    for path in ["open.rs", "clean.rs", "dirty.rs"] {
        read_ok(&mut docs, path, "a\n");
    }
    (docs.get_mut("dirty.rs").unwrap().editor_mut().unwrap()).insert("x");
    let open: HashSet<String> = ["open.rs".to_owned()].into();
    docs.retain_open(&open);
    assert!(docs.get("open.rs").is_some(), "its leaf is in a layout");
    assert!(docs.get("clean.rs").is_none(), "unused and clean");
    assert!(
        docs.get("dirty.rs").is_some(),
        "unsaved text is never dropped"
    );
}

#[test]
fn ensure_keeps_the_doc_and_its_opening() {
    let mut docs = Documents::new();
    let first = docs.ensure("a.rs", MdMode::Source).opening;
    assert_eq!(docs.ensure("a.rs", MdMode::Preview).opening, first);
    assert_eq!(
        docs.get("a.rs").unwrap().mode,
        MdMode::Source,
        "the first mode stays"
    );
}

#[test]
fn refresh_reads_what_is_shown_and_marks_the_rest_stale() {
    let mut docs = Documents::new();
    read_ok(&mut docs, "shown.rs", "a\n");
    read_ok(&mut docs, "hidden.rs", "b\n");
    let reads = docs.refresh(&["shown.rs".to_owned()]);
    assert_eq!(reads, ["shown.rs"]);
    assert!(!docs.get_mut("shown.rs").unwrap().take_read());
    assert!(
        docs.get_mut("hidden.rs").unwrap().take_read(),
        "read when it is drawn"
    );
}

#[test]
fn a_link_column_lands_on_a_char_boundary_and_is_clamped() {
    let mut docs = opened("aé b\nxyz\n");
    // Column 3 is inside the two-byte "é" (bytes 1-2): the caret moves back to the boundary.
    doc_mut(&mut docs).set_line(Some((1, 3)));
    assert_eq!(doc_mut(&mut docs).apply_pending_line(), Some(0));
    assert_eq!(doc(&docs).shown().unwrap().editor.cursor(), Pos::new(0, 1));
    doc_mut(&mut docs).set_line(Some((2, 99)));
    assert_eq!(doc_mut(&mut docs).apply_pending_line(), Some(1));
    assert_eq!(doc(&docs).shown().unwrap().editor.cursor(), Pos::new(1, 3));
}

/// Ctrl+Click on `README.md:3`: the caret goes to the line once the read answers.
#[test]
fn a_link_line_waits_for_the_read_and_puts_the_caret_there() {
    let mut docs = Documents::new();
    docs.ensure(P, MdMode::default()).set_line(Some((3, 1)));
    assert_eq!(
        doc_mut(&mut docs).apply_pending_line(),
        None,
        "still loading"
    );
    assert!(docs.begin_read(P));
    docs.read(P, file_text("a\nb\nc\nd\n"));
    assert_eq!(doc_mut(&mut docs).apply_pending_line(), Some(2));
    assert_eq!(doc(&docs).line(), Some(3));
    assert_eq!(
        doc_mut(&mut docs).apply_pending_line(),
        None,
        "applied once"
    );
}

#[test]
fn a_line_past_the_end_lands_on_the_last_line() {
    let mut docs = opened("a\nb");
    doc_mut(&mut docs).set_line(Some((99, 1)));
    assert_eq!(doc_mut(&mut docs).apply_pending_line(), Some(1));
    assert_eq!(doc(&docs).line(), Some(2));
}

/// The file is already shown: the line applies at once, and unsaved text stays.
#[test]
fn a_line_in_the_shown_file_applies_at_once_and_keeps_unsaved_text() {
    let mut docs = opened("a\nb\nc");
    editor(&mut docs).insert("x");
    doc_mut(&mut docs).set_line(Some((2, 1)));
    assert_eq!(doc_mut(&mut docs).apply_pending_line(), Some(1));
    assert_eq!(doc(&docs).line(), Some(2));
    assert_eq!(text(&docs), "xa\nb\nc");
    assert!(unsaved(&docs).is_some());
}

#[test]
fn a_failed_read_drops_the_line() {
    let mut docs = Documents::new();
    docs.ensure(P, MdMode::default()).set_line(Some((5, 1)));
    assert!(docs.begin_read(P));
    docs.read(P, Err("gone".into()));
    assert_eq!(doc_mut(&mut docs).apply_pending_line(), None);
    // Retry shows the file from the top.
    assert!(doc_mut(&mut docs).take_read());
    assert!(docs.begin_read(P));
    docs.read(P, file_text("a\nb\n"));
    assert_eq!(doc_mut(&mut docs).apply_pending_line(), None);
    assert_eq!(doc(&docs).line(), Some(1));
}

#[test]
fn markdown_opens_as_a_preview_that_is_parsed_once_per_text() {
    let mut docs = opened("# T\n");
    assert!(doc(&docs).is_markdown());
    assert_eq!(doc(&docs).mode, MdMode::Preview);
    assert!(doc(&docs).previewing());
    let (stamp, text) = doc_mut(&mut docs)
        .preview_request()
        .expect("a new text is parsed");
    assert_eq!(text, "# T\n");
    // Asked once while it is being built.
    assert_eq!(doc_mut(&mut docs).preview_request(), None);
    docs.preview_ready(stamp, page(&text));
    assert_eq!(
        doc(&docs).preview().map(|shown| shown.lines.clone()),
        Some(vec!["# T".to_owned()])
    );
    assert_eq!(
        doc_mut(&mut docs).preview_request(),
        None,
        "the text did not change"
    );
}

#[test]
fn an_edit_made_while_a_page_builds_is_built_once_it_lands() {
    let mut docs = opened("a\n");
    let (old, _) = doc_mut(&mut docs).preview_request().unwrap();
    editor(&mut docs).insert("b");
    assert_eq!(
        doc_mut(&mut docs).preview_request(),
        None,
        "one build at a time"
    );
    docs.preview_ready(old, page("a\n"));
    let (new, typed) = doc_mut(&mut docs).preview_request().expect("the edit");
    assert_eq!(typed, "ba\n");
    docs.preview_ready(new, page("ba\n"));
    // A late page of an older version never replaces a newer one.
    docs.preview_ready(old, page("a\n"));
    assert_eq!(
        doc(&docs).preview().map(|shown| shown.lines.clone()),
        Some(vec!["ba".to_owned()])
    );
    assert_eq!(doc_mut(&mut docs).preview_request(), None);
}

/// Each new doc is a new opening (its preview starts at the top); asking for it again or
/// re-reading it is not.
#[test]
fn every_new_doc_is_a_new_opening() {
    let mut docs = opened("a\n");
    let first = doc(&docs).opening;
    assert_eq!(docs.ensure(P, MdMode::default()).opening, first);
    assert!(docs.begin_read(P));
    docs.read(P, file_text("b\n"));
    assert_eq!(doc(&docs).opening, first, "the same file re-read");
    docs.retain_open(&HashSet::new());
    let again = docs.ensure(P, MdMode::default()).opening;
    assert_ne!(again, first, "dropped and opened again");
    assert_ne!(
        doc(&opened("a\n")).opening,
        again,
        "another workspace's store"
    );
}

#[test]
fn a_new_doc_starts_in_preview_and_the_old_doc_keeps_its_page() {
    let mut docs = opened("a\n");
    let (stamp, _) = doc_mut(&mut docs).preview_request().unwrap();
    docs.preview_ready(stamp, page("a\n"));
    doc_mut(&mut docs).set_mode(MdMode::Source);
    let other = docs.ensure("other.md", MdMode::default());
    assert_eq!(other.mode, MdMode::Preview);
    assert!(other.preview().is_none());
    let readme = doc(&docs);
    assert_eq!(readme.mode, MdMode::Source);
    assert!(readme.preview().is_some());
}

#[test]
fn a_reread_that_changes_the_text_builds_a_new_page() {
    let mut docs = opened("a\n");
    let (stamp, _) = doc_mut(&mut docs).preview_request().unwrap();
    docs.preview_ready(stamp, page("a\n"));
    assert!(docs.begin_read(P));
    docs.read(P, file_text("b\n"));
    assert!(doc(&docs).preview().is_none(), "no page of the old text");
    assert_eq!(
        (doc_mut(&mut docs).preview_request())
            .map(|(_, text)| text)
            .as_deref(),
        Some("b\n")
    );
}

#[test]
fn only_markdown_has_a_preview() {
    let mut docs = Documents::new();
    read_ok(&mut docs, "src/main.rs", "fn a() {}");
    let doc = docs.get_mut("src/main.rs").unwrap();
    assert!(!doc.is_markdown());
    assert!(!doc.previewing());
    assert_eq!(doc.preview_request(), None);
}

#[test]
fn unsaved_edits_survive_the_toggle_and_the_preview_shows_them() {
    let mut docs = opened("a\n");
    let (stamp, _) = doc_mut(&mut docs).preview_request().unwrap();
    docs.preview_ready(stamp, page("a\n"));
    doc_mut(&mut docs).set_mode(MdMode::Source);
    assert_eq!(doc(&docs).mode, MdMode::Source);
    assert_eq!(
        doc_mut(&mut docs).preview_request(),
        None,
        "nothing to build in Source"
    );
    editor(&mut docs).insert("x");
    doc_mut(&mut docs).set_mode(MdMode::Preview);
    let (_, typed) = doc_mut(&mut docs)
        .preview_request()
        .expect("the edited text is parsed");
    assert_eq!(typed, "xa\n");
    assert_eq!(text(&docs), "xa\n");
    assert!(unsaved(&docs).is_some());
}

#[test]
fn a_reread_follows_the_disk_only_while_nothing_is_unsaved() {
    let mut docs = opened("a\nbc\n");
    editor(&mut docs).set_cursor(Pos::new(1, 2), false);
    assert_eq!(doc(&docs).line(), Some(2));
    // The same text again (our own save, another file changed): caret and undo stay.
    editor(&mut docs).insert("x");
    editor(&mut docs).undo();
    docs.begin_read(P);
    docs.read(P, file_text("a\nbc\n"));
    assert!(editor(&mut docs).redo(), "the undo history survived");
    editor(&mut docs).undo();
    // An agent shortened it: the new text shows, the caret stays where it can.
    docs.begin_read(P);
    docs.read(P, file_text("a\n"));
    assert_eq!(
        (text(&docs), doc(&docs).line()),
        ("a\n".to_owned(), Some(1))
    );
    // Unsaved text is never replaced: not by a change on disk, not by a failed read.
    editor(&mut docs).insert("mine ");
    for read in [file_text("theirs\n"), Err("gone".into())] {
        docs.begin_read(P);
        docs.read(P, read);
        assert_eq!(text(&docs), "amine \n");
    }
    assert_eq!(unsaved(&docs).as_deref(), Some("README.md"));
    // Another file is its own doc: the unsaved one stays.
    read_ok(&mut docs, "src/main.rs", "m\n");
    assert_eq!(docs.unsaved_paths(), ["README.md"]);
}

#[test]
fn a_save_marks_what_it_sent_and_one_runs_at_a_time() {
    let mut docs = opened("a\n");
    assert_eq!(docs.begin_save(P), None, "nothing to save");
    editor(&mut docs).insert("b");
    let save = docs.begin_save(P).unwrap();
    assert_eq!(
        (save.text.as_str(), save.expected_hash.as_str()),
        ("ba\n", "#a\n")
    );
    assert_eq!(docs.begin_save(P), None, "one save at a time");
    // Typed while it was saving: still unsaved afterwards.
    editor(&mut docs).key(EditKey::Insert("c".into()), 10);
    let written = FsWritten {
        saved: true,
        hash: "#ba\n".into(),
    };
    docs.saved(P, save.point, Ok(written));
    assert_eq!(unsaved(&docs).as_deref(), Some("README.md"));
    let save = docs.begin_save(P).unwrap();
    assert_eq!(
        save.expected_hash, "#ba\n",
        "the saved text is the new base"
    );
    let written = FsWritten {
        saved: true,
        hash: "#bca\n".into(),
    };
    docs.saved(P, save.point, Ok(written));
    assert_eq!(unsaved(&docs), None);
    // Our own save comes back through the watcher: nothing changes.
    docs.begin_read(P);
    docs.read(P, file_text("bca\n"));
    assert_eq!(text(&docs), "bca\n");
    // A failed save says why and keeps the text.
    editor(&mut docs).insert("d");
    let save = docs.begin_save(P).unwrap();
    docs.saved(P, save.point, Err("disk full".into()));
    let error = doc(&docs)
        .shown()
        .and_then(|shown| shown.save_error.clone());
    assert_eq!(error.as_deref(), Some("Not saved: disk full"));
    assert_eq!(unsaved(&docs).as_deref(), Some("README.md"));
}

#[test]
fn a_save_that_comes_back_after_its_doc_was_dropped_is_reported() {
    let mut docs = opened("a\n");
    editor(&mut docs).insert("b");
    let save = docs.begin_save(P).unwrap();
    // The user closed the file ("Don't save") while the write was in flight.
    docs.drop_changes(P);
    docs.retain_open(&HashSet::new());
    assert!(docs.get(P).is_none());
    let failed = docs.saved(P, save.point, Err("disk full".into()));
    assert_eq!(failed.as_deref(), Some("Not saved: disk full"));
    let newer = FsWritten {
        saved: false,
        hash: "#other".into(),
    };
    assert_eq!(
        docs.saved(P, None, Ok(newer)).as_deref(),
        Some("Not saved: the file changed on disk meanwhile.")
    );
    let fine = FsWritten {
        saved: true,
        hash: "#x".into(),
    };
    assert_eq!(
        docs.saved(P, None, Ok(fine)),
        None,
        "a good save is no news"
    );
    // Shown: the banner has it, nothing more to say.
    let mut docs = opened("a\n");
    editor(&mut docs).insert("b");
    let save = docs.begin_save(P).unwrap();
    assert_eq!(docs.saved(P, save.point, Err("disk full".into())), None);
}

#[test]
fn a_highlight_is_kept_only_for_the_text_it_was_made_of() {
    let mut docs = opened("a\n");
    editor(&mut docs).insert("b");
    let version = doc(&docs).shown().unwrap().editor.version();
    let request = doc(&docs).highlight_request(version).unwrap();
    assert_eq!(request.content.as_deref(), Some("ba\n"));
    editor(&mut docs).insert("c");
    assert_eq!(
        doc(&docs).highlight_request(version),
        None,
        "typing went on"
    );
    assert!(
        !doc_mut(&mut docs).highlighted(version, Code::plain("ba\n")),
        "stale"
    );
    let version = doc(&docs).shown().unwrap().editor.version();
    assert!(doc_mut(&mut docs).highlighted(version, Code::plain("bca\n")));
}

fn file_with_base(text: &str, base: &str) -> Result<(FsContent, Code), String> {
    let (mut content, _) = file_text(text)?;
    content.index_text = Some(base.into());
    let code = Code::new(&content);
    Ok((content, code))
}

fn marks(docs: &Documents) -> Vec<Option<crate::gutter::Change>> {
    (doc(docs).shown())
        .map(|shown| shown.code.changes.clone())
        .unwrap_or_default()
}

/// Staging or committing leaves the file on disk as it is (same hash) but moves the index:
/// the gutter follows, caret and undo stay. With unsaved text the new index text waits for
/// the next typing pause.
#[test]
fn a_new_index_text_moves_the_gutter_and_keeps_the_caret() {
    use crate::gutter::Change::Added;
    let mut docs = Documents::new();
    docs.ensure(P, MdMode::default());
    docs.begin_read(P);
    docs.read(P, file_with_base("a\nb\n", "a\n"));
    assert_eq!(marks(&docs), [None, Some(Added)]);
    editor(&mut docs).set_cursor(Pos::new(1, 0), false);

    docs.begin_read(P);
    docs.read(P, file_with_base("a\nb\n", "a\nb\n"));
    assert_eq!(marks(&docs), [None, None], "staged: no marks");
    assert_eq!(doc(&docs).line(), Some(2), "the caret stayed");

    editor(&mut docs).insert("x");
    docs.begin_read(P);
    docs.read(P, file_with_base("a\nb\n", "a\n"));
    assert_eq!(text(&docs), "a\nxb\n", "unsaved text stays");
    let version = doc(&docs).shown().unwrap().editor.version();
    let request = doc(&docs).highlight_request(version).unwrap();
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
    let mut docs = opened("one\n");
    editor(&mut docs).insert("x");
    let reread = |docs: &mut Documents, text: &str| {
        docs.begin_read(P);
        docs.read(P, file_text(text));
    };
    let disk = |docs: &Documents| {
        doc(docs)
            .shown()
            .and_then(|shown| shown.disk_changed.clone())
    };
    reread(&mut docs, "one\n");
    assert_eq!(disk(&docs), None, "the same version: nothing to say");
    reread(&mut docs, "two\n");
    assert_eq!(
        (text(&docs), disk(&docs)),
        ("xone\n".to_owned(), Some("#two\n".to_owned()))
    );
    reread(&mut docs, "one\n");
    assert_eq!(disk(&docs), None, "the agent undid its change");
    // Keep mine: Save overwrites the disk's version.
    reread(&mut docs, "two\n");
    doc_mut(&mut docs).keep_mine();
    assert_eq!(disk(&docs), None);
    let save = docs.begin_save(P).unwrap();
    assert_eq!(save.expected_hash, "#two\n");
    // The save meets yet another version: the banner again, the text stays.
    let written = FsWritten {
        saved: false,
        hash: "#three\n".into(),
    };
    docs.saved(P, save.point, Ok(written));
    assert_eq!(
        (text(&docs), disk(&docs)),
        ("xone\n".to_owned(), Some("#three\n".to_owned()))
    );
    // Reload: the disk's text replaces it.
    doc_mut(&mut docs).discard();
    reread(&mut docs, "three\n");
    assert_eq!(
        (text(&docs), unsaved(&docs), disk(&docs)),
        ("three\n".to_owned(), None, None)
    );
}

#[test]
fn the_unsaved_question_answers() {
    assert_eq!(leave_choice(Some(0)), Some(Leave::Save));
    assert_eq!(leave_choice(Some(1)), Some(Leave::DontSave));
    assert_eq!(leave_choice(Some(2)), None, "Cancel");
    assert_eq!(leave_choice(None), None, "Esc / dialog closed");
}

#[test]
fn a_successful_save_clears_the_changed_on_disk_banner() {
    let mut docs = opened("one\n");
    editor(&mut docs).insert("x");
    docs.begin_read(P);
    docs.read(P, file_text("two\n"));
    assert!(doc(&docs).shown().unwrap().disk_changed.is_some());
    let save = docs.begin_save(P).unwrap();
    let written = FsWritten {
        saved: true,
        hash: "#xone\n".into(),
    };
    docs.saved(P, save.point, Ok(written));
    let shown = doc(&docs).shown().unwrap();
    assert_eq!(shown.disk_changed, None);
    assert_eq!(shown.hash.as_deref(), Some("#xone\n"));
}

/// README.md opened with `text` and its preview built.
fn previewed(text: &str) -> Documents {
    let mut docs = opened(text);
    let (stamp, source) = doc_mut(&mut docs)
        .preview_request()
        .expect("a preview to build");
    docs.preview_ready(stamp, page(&source));
    docs
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
    let mut docs = previewed("# Title\n\nold para\n\nend\n");
    let doc = doc_mut(&mut docs);
    doc.preview_click(point(1, 0), false, 1);
    doc.preview_drag(point(1, 3));
    assert_eq!(doc.preview_copy().as_deref(), Some("old"));
    doc.preview_click(point(2, 3), true, 1);
    assert_eq!(
        doc.preview_copy().as_deref(),
        Some("old para\nend"),
        "Shift extends"
    );
    doc.preview_click(point(1, 5), false, 2);
    assert_eq!(
        doc.preview_copy().as_deref(),
        Some("para"),
        "double click: a word"
    );
    doc.preview_select_all();
    assert_eq!(doc.preview_copy().as_deref(), Some("Title\nold para\nend"));
    assert!(doc.clear_preview_selection());
    assert_eq!(doc.preview_copy(), None);
}

#[test]
fn send_selection_from_the_preview_is_the_source_of_the_blocks_touched() {
    let mut docs = previewed("# Title\n\nold para\n\nend\n");
    let doc = doc_mut(&mut docs);
    // Nothing selected on the page: the editor's caret line, as before.
    assert_eq!(
        doc.picked_lines(),
        Some(("README.md".into(), 1, 1, "# Title".into()))
    );
    doc.preview_click(point(1, 0), false, 1);
    doc.preview_drag(point(2, 3));
    assert_eq!(
        doc.picked_lines(),
        Some(("README.md".into(), 3, 5, "old para\n\nend".into())),
        "the Markdown source of both blocks, blank line included"
    );
    // One block: its whole range, whatever part is selected.
    doc.preview_click(point(1, 1), false, 1);
    doc.preview_drag(point(1, 3));
    assert_eq!(
        doc.picked_lines(),
        Some(("README.md".into(), 3, 3, "old para".into()))
    );
}

#[test]
fn in_split_the_editors_selection_wins_over_the_page() {
    let mut docs = previewed("# Title\n\nold para\n\nend\n");
    let doc = doc_mut(&mut docs);
    doc.set_mode(MdMode::Split);
    doc.preview_click(point(1, 0), false, 1);
    doc.preview_drag(point(1, 3));
    assert_eq!(
        doc.picked_lines(),
        Some(("README.md".into(), 3, 3, "old para".into())),
        "the page selects, the editor does not"
    );
    if let Content::Shown(shown) = &mut doc.content {
        shown.editor.select_all();
    }
    assert_eq!(
        doc.picked_lines(),
        Some((
            "README.md".into(),
            1,
            5,
            "# Title\n\nold para\n\nend".into()
        )),
        "the editor's selection, from the first line"
    );
}

#[test]
fn in_split_a_page_of_older_text_gives_way_to_the_editor() {
    let mut docs = previewed("# Title\n\nold para\n\nend\n");
    let doc = doc_mut(&mut docs);
    doc.set_mode(MdMode::Split);
    doc.preview_click(point(1, 0), false, 1);
    doc.preview_drag(point(1, 3));
    assert_eq!(
        doc.picked_lines(),
        Some(("README.md".into(), 3, 3, "old para".into()))
    );
    // The text changed and the page is not rebuilt yet: its line numbers may have moved.
    editor(&mut docs).insert("x");
    assert_eq!(
        doc_mut(&mut docs).picked_lines(),
        Some(("README.md".into(), 1, 1, "x# Title".into())),
        "the caret's line, not the page's"
    );
}

#[test]
fn edit_keys_do_nothing_while_previewing() {
    let mut docs = previewed("a\n");
    let doc = doc_mut(&mut docs);
    doc.preview_click(point(0, 0), false, 1);
    doc.preview_select_all();
    let _ = doc.preview_copy();
    assert_eq!(text(&docs), "a\n");
    assert_eq!(unsaved(&docs), None);
}

#[test]
fn applying_a_block_edit_replaces_its_lines_in_one_undo_step() {
    let mut docs = previewed("# T\n\nold para\n\nend\n");
    let doc = doc_mut(&mut docs);
    assert!(doc.open_block_edit(1));
    assert_eq!(
        doc.block_edit.as_ref().map(|edit| edit.text.text()),
        Some("old para")
    );
    doc.block_edit_key(DialogKey::SelectAll);
    doc.block_edit_key(DialogKey::Insert("new".into()));
    doc.block_edit_key(DialogKey::Enter);
    doc.block_edit_key(DialogKey::Insert("line".into()));
    assert_eq!(doc.block_edit_key(DialogKey::Save), BlockEditOutcome::Apply);
    assert_eq!(doc.apply_block_edit(), Ok(()));
    assert!(doc.block_edit.is_none());
    assert_eq!(text(&docs), "# T\n\nnew\nline\n\nend\n");
    assert_eq!(unsaved(&docs).as_deref(), Some("README.md"));
    editor(&mut docs).key(EditKey::Undo, 10);
    assert_eq!(text(&docs), "# T\n\nold para\n\nend\n");
}

#[test]
fn an_emptied_block_leaves_a_blank_line() {
    let mut docs = previewed("a\n\nb\n\nc\n");
    let doc = doc_mut(&mut docs);
    assert!(doc.open_block_edit(1));
    doc.block_edit_key(DialogKey::SelectAll);
    doc.block_edit_key(DialogKey::Backspace);
    assert_eq!(doc.apply_block_edit(), Ok(()));
    assert_eq!(text(&docs), "a\n\n\n\nc\n");
}

#[test]
fn a_block_edit_is_refused_once_the_text_changed() {
    let mut docs = previewed("a\n\nb\n");
    assert!(doc_mut(&mut docs).open_block_edit(1));
    editor(&mut docs).insert("x");
    assert_eq!(doc_mut(&mut docs).apply_block_edit(), Err(StaleEdit));
    assert_eq!(text(&docs), "xa\n\nb\n", "nothing written");
    let doc = doc_mut(&mut docs);
    assert!(doc.block_edit.is_some(), "the typed text stays to copy");
    doc.block_edit = None;
    assert!(!doc.open_block_edit(0), "the page is of the old text");
}

#[test]
fn esc_cancels_and_leaving_the_preview_drops_edit_and_selection() {
    let mut docs = previewed("a\n\nb\n");
    let readme = doc_mut(&mut docs);
    assert!(readme.open_block_edit(0));
    assert_eq!(
        readme.block_edit_key(DialogKey::Cancel),
        BlockEditOutcome::Cancel
    );
    readme.preview_click(point(0, 0), false, 1);
    readme.preview_drag(point(0, 1));
    readme.set_mode(MdMode::Source);
    assert!(readme.block_edit.is_none() && readme.preview_selection.is_none());
    readme.set_mode(MdMode::Preview);
    assert!(readme.open_block_edit(0));
    readme.preview_select_all();
    // Another doc starts clean; README.md's doc keeps its edit.
    let other = docs.ensure("other.md", MdMode::default());
    assert!(other.block_edit.is_none() && other.preview_selection.is_none());
    assert!(doc(&docs).block_edit.is_some());
}

#[test]
fn a_new_page_clears_the_selection() {
    let mut docs = previewed("a\n");
    let readme = doc_mut(&mut docs);
    assert!(readme.open_block_edit(0));
    assert_eq!(readme.apply_block_edit(), Ok(()));
    readme.preview_select_all();
    assert!(
        readme.preview_selection.is_some(),
        "the old page is still shown"
    );
    let (stamp, source) = readme.preview_request().expect("the edited text is parsed");
    docs.preview_ready(stamp, page(&source));
    assert_eq!(doc(&docs).preview_selection, None);
}

#[test]
fn a_new_page_reports_the_block_edit_it_closed() {
    let mut docs = previewed("a\n\nb\n");
    assert!(doc_mut(&mut docs).open_block_edit(0));
    editor(&mut docs).insert("x");
    let (stamp, source) = (doc_mut(&mut docs).preview_request()).expect("the new text is parsed");
    assert!(docs.preview_ready(stamp, page(&source)), "an edit was open");
    assert!(doc(&docs).block_edit.is_none());
    editor(&mut docs).insert("y");
    let (stamp, source) = (doc_mut(&mut docs).preview_request()).expect("the new text is parsed");
    assert!(!docs.preview_ready(stamp, page(&source)), "nothing open");
}

#[test]
fn a_read_lands_in_its_own_doc() {
    let mut docs = Documents::new();
    docs.ensure("a.rs", MdMode::default());
    docs.ensure("b.rs", MdMode::default());
    assert!(docs.begin_read("a.rs"));
    docs.read("a.rs", file_text_at("a.rs", "a\n"));
    assert!(
        docs.get("b.rs").unwrap().shown().is_none(),
        "the other doc is still loading"
    );
    assert_eq!(
        docs.get("a.rs").unwrap().shown().unwrap().editor.text(),
        "a\n"
    );
}

#[test]
fn a_read_for_a_dropped_doc_is_ignored() {
    let mut docs = Documents::new();
    assert!(!docs.begin_read("gone.rs"), "no doc, no read");
    assert_eq!(docs.read("gone.rs", file_text_at("gone.rs", "x\n")), None);
    assert!(docs.get("gone.rs").is_none());
}

#[test]
fn a_save_and_a_highlight_land_in_their_own_doc() {
    let mut docs = Documents::new();
    read_ok(&mut docs, "a.rs", "a\n");
    read_ok(&mut docs, "b.rs", "m\n");
    let a = docs.get_mut("a.rs").unwrap();
    a.editor_mut().unwrap().insert("x");
    let version = a.shown().unwrap().editor.version();
    let save = docs.begin_save("a.rs").unwrap();
    // The highlight of a.rs lands in a.rs and leaves b.rs alone.
    let a = docs.get_mut("a.rs").unwrap();
    assert_eq!(
        a.highlight_request(version).unwrap().content.as_deref(),
        Some("xa\n")
    );
    assert!(a.highlighted(version, Code::plain("xa\n")));
    let written = FsWritten {
        saved: true,
        hash: "#xa\n".into(),
    };
    assert_eq!(docs.saved("a.rs", save.point, Ok(written)), None);
    assert_eq!(docs.unsaved_paths(), Vec::<String>::new());
    assert_eq!(
        docs.get("b.rs").unwrap().shown().unwrap().editor.text(),
        "m\n",
        "the other doc is untouched"
    );
    let refused = FsWritten {
        saved: false,
        hash: "#z".into(),
    };
    assert!(docs.saved("gone.rs", None, Ok(refused)).is_some());
}

#[test]
fn unsaved_paths_lists_every_dirty_doc() {
    let mut docs = Documents::new();
    for path in ["b.rs", "a.rs", "c.rs"] {
        read_ok(&mut docs, path, "a\n");
    }
    for path in ["a.rs", "b.rs"] {
        docs.get_mut(path)
            .unwrap()
            .editor_mut()
            .unwrap()
            .insert("x");
    }
    assert_eq!(docs.unsaved_paths(), ["a.rs", "b.rs"]);
}

/// After a lost connection only the docs with unsaved text carry over (their hash stays for
/// the save), and they are read again when drawn.
#[test]
fn into_unsaved_keeps_only_the_dirty_docs_and_reads_them_again() {
    let mut docs = Documents::new();
    for path in ["a.rs", "b.rs"] {
        read_ok(&mut docs, path, "a\n");
    }
    docs.get_mut("b.rs")
        .unwrap()
        .editor_mut()
        .unwrap()
        .insert("x");
    let mut kept = docs.into_unsaved().expect("one doc is unsaved");
    assert_eq!(kept.unsaved_paths(), ["b.rs"]);
    assert!(kept.get("a.rs").is_none());
    assert!(kept.get_mut("b.rs").unwrap().take_read(), "read when drawn");
    // Nothing is in flight any more.
    assert!(kept.begin_read("b.rs"));
    let mut clean = Documents::new();
    read_ok(&mut clean, "a.rs", "a\n");
    assert!(clean.into_unsaved().is_none());
}

/// Leaving several docs asks about the first one still unsaved, so text typed while its save
/// was in flight is asked about again.
#[test]
fn the_next_doc_to_ask_about_is_the_first_still_unsaved() {
    let mut docs = Documents::new();
    for path in ["a.rs", "b.rs"] {
        read_ok(&mut docs, path, "a\n");
        docs.get_mut(path)
            .unwrap()
            .editor_mut()
            .unwrap()
            .insert("x");
    }
    let paths = ["a.rs".to_owned(), "b.rs".to_owned()];
    assert_eq!(docs.first_unsaved(&paths).as_deref(), Some("a.rs"));
    let save = docs.begin_save("a.rs").unwrap();
    docs.get_mut("a.rs")
        .unwrap()
        .editor_mut()
        .unwrap()
        .insert("y"); // typed while saving
    let written = FsWritten {
        saved: true,
        hash: "#xa\n".into(),
    };
    docs.saved("a.rs", save.point, Ok(written));
    assert_eq!(
        docs.first_unsaved(&paths).as_deref(),
        Some("a.rs"),
        "asked again"
    );
    docs.drop_changes("a.rs");
    assert_eq!(docs.first_unsaved(&paths).as_deref(), Some("b.rs"));
}

/// "Don't save" drops the changes and reads the file again, so the doc shows what is on disk.
#[test]
fn dropping_changes_rereads_the_file() {
    let mut docs = opened("a\n");
    editor(&mut docs).insert("x");
    assert_eq!(docs.drop_changes(P).as_deref(), Some("README.md"));
    assert_eq!(unsaved(&docs), None);
    docs.begin_read(P);
    docs.read(P, file_text("a\n"));
    assert_eq!(text(&docs), "a\n", "the disk's text, not the dropped edit");
    assert_eq!(docs.drop_changes("gone.rs"), None);
}

#[test]
fn split_rebuilds_the_page_after_each_edit() {
    let mut docs = opened("a\n");
    doc_mut(&mut docs).set_mode(MdMode::Split);
    let (stamp, _) = doc_mut(&mut docs)
        .preview_request()
        .expect("split shows a page");
    docs.preview_ready(stamp, page("a\n"));
    assert_eq!(doc_mut(&mut docs).preview_request(), None, "current");
    editor(&mut docs).insert("x");
    let (_, typed) = (doc_mut(&mut docs).preview_request()).expect("the edit is parsed");
    assert_eq!(typed, "xa\n");
}

#[test]
fn a_task_box_click_ticks_the_source_once_the_page_is_current() {
    let mut docs = previewed("intro\n\n- [ ] one\n- [x] two\n");
    editor(&mut docs).set_cursor(Pos::new(0, 2), false);
    assert_eq!(doc_mut(&mut docs).toggle_task(3), Ok(true));
    assert_eq!(text(&docs), "intro\n\n- [x] one\n- [x] two\n");
    assert_eq!(
        editor(&mut docs).cursor(),
        Pos::new(0, 2),
        "the caret stays"
    );
    // The page is of the text before the tick until it is built again.
    assert_eq!(doc_mut(&mut docs).toggle_task(4), Err(StaleEdit));
    let (stamp, source) = doc_mut(&mut docs).preview_request().unwrap();
    docs.preview_ready(stamp, page(&source));
    assert_eq!(doc_mut(&mut docs).toggle_task(4), Ok(true));
    assert_eq!(text(&docs), "intro\n\n- [x] one\n- [ ] two\n");
    let (stamp, source) = doc_mut(&mut docs).preview_request().unwrap();
    docs.preview_ready(stamp, page(&source));
    assert_eq!(
        doc_mut(&mut docs).toggle_task(1),
        Ok(false),
        "no task on that line"
    );
}

#[test]
fn a_missing_file_is_told_from_other_read_errors() {
    assert!(is_missing(
        "no file src/a.rs in project 01J0000000000000000000000"
    ));
    assert!(is_missing(
        "fs.read failed: no file a.rs in project 01J0000000000000000000000"
    ));
    assert!(!is_missing(
        "Cannot read: stream did not contain valid UTF-8"
    ));
}
