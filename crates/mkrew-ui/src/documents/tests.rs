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

/// A file of the project folder.
fn k(path: &str) -> FileKey {
    FileKey::project(path)
}

/// A page for tests: what the background build would return.
fn page(text: &str) -> docs::Shown {
    docs::Shown::from_text(text)
}

/// A store showing README.md as read with `text`.
fn opened(text: &str) -> Documents {
    let mut docs = Documents::new();
    docs.ensure(&k(P), MdMode::default());
    // The shell's first draw asks for the read, then sends it.
    assert!(doc_mut(&mut docs).take_read());
    assert!(docs.begin_read(&k(P)));
    docs.read(&k(P), file_text(text));
    docs
}

fn doc(docs: &Documents) -> &Doc {
    docs.get(&k(P)).expect("README.md is open")
}

fn doc_mut(docs: &mut Documents) -> &mut Doc {
    docs.get_mut(&k(P)).expect("README.md is open")
}

fn editor(docs: &mut Documents) -> &mut Editor {
    doc_mut(docs).editor_mut().expect("editable")
}

fn text(docs: &Documents) -> String {
    (doc(docs).shown())
        .map(|shown| shown.editor.text())
        .unwrap_or_default()
}

/// The paths of the unsaved docs.
fn unsaved_paths(docs: &Documents) -> Vec<String> {
    (docs.unsaved_files().into_iter())
        .map(|file| file.path)
        .collect()
}

/// The first unsaved doc's path.
fn unsaved(docs: &Documents) -> Option<String> {
    docs.unsaved_files()
        .into_iter()
        .next()
        .map(|file| file.path)
}

/// Another file, read.
fn read_ok(docs: &mut Documents, path: &str, text: &str) {
    docs.ensure(&k(path), MdMode::default());
    assert!(docs.get_mut(&k(path)).unwrap().take_read());
    assert!(docs.begin_read(&k(path)));
    docs.read(&k(path), file_text_at(path, text));
}

#[test]
fn a_doc_is_dropped_when_unused_and_clean_and_kept_when_open_or_dirty() {
    let mut docs = Documents::new();
    for path in ["open.rs", "clean.rs", "dirty.rs"] {
        read_ok(&mut docs, path, "a\n");
    }
    (docs.get_mut(&k("dirty.rs")).unwrap().editor_mut().unwrap()).insert("x");
    let open: HashSet<FileKey> = [k("open.rs")].into();
    docs.retain_open(&open);
    assert!(docs.get(&k("open.rs")).is_some(), "its leaf is in a layout");
    assert!(docs.get(&k("clean.rs")).is_none(), "unused and clean");
    assert!(
        docs.get(&k("dirty.rs")).is_some(),
        "unsaved text is never dropped"
    );
}

#[test]
fn ensure_keeps_the_doc_and_its_opening() {
    let mut docs = Documents::new();
    let first = docs.ensure(&k("a.rs"), MdMode::Source).opening;
    assert_eq!(docs.ensure(&k("a.rs"), MdMode::Preview).opening, first);
    assert_eq!(
        docs.get(&k("a.rs")).unwrap().mode,
        MdMode::Source,
        "the first mode stays"
    );
}

#[test]
fn refresh_reads_what_is_shown_and_marks_the_rest_stale() {
    let mut docs = Documents::new();
    read_ok(&mut docs, "shown.rs", "a\n");
    read_ok(&mut docs, "hidden.rs", "b\n");
    let reads = docs.refresh(None, &[k("shown.rs")]);
    assert_eq!(reads, [k("shown.rs")]);
    assert!(!docs.get_mut(&k("shown.rs")).unwrap().take_read());
    assert!(
        docs.get_mut(&k("hidden.rs")).unwrap().take_read(),
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
    docs.ensure(&k(P), MdMode::default()).set_line(Some((3, 1)));
    assert_eq!(
        doc_mut(&mut docs).apply_pending_line(),
        None,
        "still loading"
    );
    assert!(docs.begin_read(&k(P)));
    docs.read(&k(P), file_text("a\nb\nc\nd\n"));
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
    docs.ensure(&k(P), MdMode::default()).set_line(Some((5, 1)));
    assert!(docs.begin_read(&k(P)));
    docs.read(&k(P), Err("gone".into()));
    assert_eq!(doc_mut(&mut docs).apply_pending_line(), None);
    // Retry shows the file from the top.
    assert!(doc_mut(&mut docs).retry());
    assert!(docs.begin_read(&k(P)));
    docs.read(&k(P), file_text("a\nb\n"));
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
    assert_eq!(docs.ensure(&k(P), MdMode::default()).opening, first);
    assert!(docs.begin_read(&k(P)));
    docs.read(&k(P), file_text("b\n"));
    assert_eq!(doc(&docs).opening, first, "the same file re-read");
    docs.retain_open(&HashSet::new());
    let again = docs.ensure(&k(P), MdMode::default()).opening;
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
    let other = docs.ensure(&k("other.md"), MdMode::default());
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
    assert!(docs.begin_read(&k(P)));
    docs.read(&k(P), file_text("b\n"));
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
    let doc = docs.get_mut(&k("src/main.rs")).unwrap();
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
    docs.begin_read(&k(P));
    docs.read(&k(P), file_text("a\nbc\n"));
    assert!(editor(&mut docs).redo(), "the undo history survived");
    editor(&mut docs).undo();
    // An agent shortened it: the new text shows, the caret stays where it can.
    docs.begin_read(&k(P));
    docs.read(&k(P), file_text("a\n"));
    assert_eq!(
        (text(&docs), doc(&docs).line()),
        ("a\n".to_owned(), Some(1))
    );
    // Unsaved text is never replaced: not by a change on disk, not by a failed read.
    editor(&mut docs).insert("mine ");
    for read in [file_text("theirs\n"), Err("gone".into())] {
        docs.begin_read(&k(P));
        docs.read(&k(P), read);
        assert_eq!(text(&docs), "amine \n");
    }
    assert_eq!(unsaved(&docs).as_deref(), Some("README.md"));
    // Another file is its own doc: the unsaved one stays.
    read_ok(&mut docs, "src/main.rs", "m\n");
    assert_eq!(unsaved_paths(&docs), ["README.md"]);
}

#[test]
fn a_save_marks_what_it_sent_and_one_runs_at_a_time() {
    let mut docs = opened("a\n");
    assert_eq!(docs.begin_save(&k(P)), None, "nothing to save");
    editor(&mut docs).insert("b");
    let save = docs.begin_save(&k(P)).unwrap();
    assert_eq!(
        (save.text.as_str(), save.expected_hash.as_str()),
        ("ba\n", "#a\n")
    );
    assert_eq!(docs.begin_save(&k(P)), None, "one save at a time");
    // Typed while it was saving: still unsaved afterwards.
    editor(&mut docs).key(EditKey::Insert("c".into()), 10);
    let written = FsWritten {
        saved: true,
        hash: "#ba\n".into(),
    };
    docs.saved(&k(P), save.point, Ok(written));
    assert_eq!(unsaved(&docs).as_deref(), Some("README.md"));
    let save = docs.begin_save(&k(P)).unwrap();
    assert_eq!(
        save.expected_hash, "#ba\n",
        "the saved text is the new base"
    );
    let written = FsWritten {
        saved: true,
        hash: "#bca\n".into(),
    };
    docs.saved(&k(P), save.point, Ok(written));
    assert_eq!(unsaved(&docs), None);
    // Our own save comes back through the watcher: nothing changes.
    docs.begin_read(&k(P));
    docs.read(&k(P), file_text("bca\n"));
    assert_eq!(text(&docs), "bca\n");
    // A failed save says why and keeps the text.
    editor(&mut docs).insert("d");
    let save = docs.begin_save(&k(P)).unwrap();
    docs.saved(&k(P), save.point, Err("disk full".into()));
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
    let save = docs.begin_save(&k(P)).unwrap();
    // The user closed the file ("Don't save") while the write was in flight.
    docs.drop_changes(&k(P));
    docs.retain_open(&HashSet::new());
    assert!(docs.get(&k(P)).is_none());
    let failed = docs.saved(&k(P), save.point, Err("disk full".into()));
    assert_eq!(failed.as_deref(), Some("Not saved: disk full"));
    let newer = FsWritten {
        saved: false,
        hash: "#other".into(),
    };
    assert_eq!(
        docs.saved(&k(P), None, Ok(newer)).as_deref(),
        Some("Not saved: the file changed on disk meanwhile.")
    );
    let fine = FsWritten {
        saved: true,
        hash: "#x".into(),
    };
    assert_eq!(
        docs.saved(&k(P), None, Ok(fine)),
        None,
        "a good save is no news"
    );
    // Shown: the banner has it, nothing more to say.
    let mut docs = opened("a\n");
    editor(&mut docs).insert("b");
    let save = docs.begin_save(&k(P)).unwrap();
    assert_eq!(docs.saved(&k(P), save.point, Err("disk full".into())), None);
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
    docs.ensure(&k(P), MdMode::default());
    docs.begin_read(&k(P));
    docs.read(&k(P), file_with_base("a\nb\n", "a\n"));
    assert_eq!(marks(&docs), [None, Some(Added)]);
    editor(&mut docs).set_cursor(Pos::new(1, 0), false);

    docs.begin_read(&k(P));
    docs.read(&k(P), file_with_base("a\nb\n", "a\nb\n"));
    assert_eq!(marks(&docs), [None, None], "staged: no marks");
    assert_eq!(doc(&docs).line(), Some(2), "the caret stayed");

    editor(&mut docs).insert("x");
    docs.begin_read(&k(P));
    docs.read(&k(P), file_with_base("a\nb\n", "a\n"));
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
        docs.begin_read(&k(P));
        docs.read(&k(P), file_text(text));
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
    let save = docs.begin_save(&k(P)).unwrap();
    assert_eq!(save.expected_hash, "#two\n");
    // The save meets yet another version: the banner again, the text stays.
    let written = FsWritten {
        saved: false,
        hash: "#three\n".into(),
    };
    docs.saved(&k(P), save.point, Ok(written));
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
    docs.begin_read(&k(P));
    docs.read(&k(P), file_text("two\n"));
    assert!(doc(&docs).shown().unwrap().disk_changed.is_some());
    let save = docs.begin_save(&k(P)).unwrap();
    let written = FsWritten {
        saved: true,
        hash: "#xone\n".into(),
    };
    docs.saved(&k(P), save.point, Ok(written));
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
    let other = docs.ensure(&k("other.md"), MdMode::default());
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
    docs.ensure(&k("a.rs"), MdMode::default());
    docs.ensure(&k("b.rs"), MdMode::default());
    assert!(docs.begin_read(&k("a.rs")));
    docs.read(&k("a.rs"), file_text_at("a.rs", "a\n"));
    assert!(
        docs.get(&k("b.rs")).unwrap().shown().is_none(),
        "the other doc is still loading"
    );
    assert_eq!(
        docs.get(&k("a.rs")).unwrap().shown().unwrap().editor.text(),
        "a\n"
    );
}

#[test]
fn a_read_for_a_dropped_doc_is_ignored() {
    let mut docs = Documents::new();
    assert!(!docs.begin_read(&k("gone.rs")), "no doc, no read");
    assert_eq!(
        docs.read(&k("gone.rs"), file_text_at("gone.rs", "x\n")),
        None
    );
    assert!(docs.get(&k("gone.rs")).is_none());
}

#[test]
fn a_save_and_a_highlight_land_in_their_own_doc() {
    let mut docs = Documents::new();
    read_ok(&mut docs, "a.rs", "a\n");
    read_ok(&mut docs, "b.rs", "m\n");
    let a = docs.get_mut(&k("a.rs")).unwrap();
    a.editor_mut().unwrap().insert("x");
    let version = a.shown().unwrap().editor.version();
    let save = docs.begin_save(&k("a.rs")).unwrap();
    // The highlight of a.rs lands in a.rs and leaves b.rs alone.
    let a = docs.get_mut(&k("a.rs")).unwrap();
    assert_eq!(
        a.highlight_request(version).unwrap().content.as_deref(),
        Some("xa\n")
    );
    assert!(a.highlighted(version, Code::plain("xa\n")));
    let written = FsWritten {
        saved: true,
        hash: "#xa\n".into(),
    };
    assert_eq!(docs.saved(&k("a.rs"), save.point, Ok(written)), None);
    assert_eq!(unsaved_paths(&docs), Vec::<String>::new());
    assert_eq!(
        docs.get(&k("b.rs")).unwrap().shown().unwrap().editor.text(),
        "m\n",
        "the other doc is untouched"
    );
    let refused = FsWritten {
        saved: false,
        hash: "#z".into(),
    };
    assert!(docs.saved(&k("gone.rs"), None, Ok(refused)).is_some());
}

#[test]
fn unsaved_paths_lists_every_dirty_doc() {
    let mut docs = Documents::new();
    for path in ["b.rs", "a.rs", "c.rs"] {
        read_ok(&mut docs, path, "a\n");
    }
    for path in ["a.rs", "b.rs"] {
        docs.get_mut(&k(path))
            .unwrap()
            .editor_mut()
            .unwrap()
            .insert("x");
    }
    assert_eq!(unsaved_paths(&docs), ["a.rs", "b.rs"]);
}

/// After a lost connection only the docs with unsaved text carry over (their hash stays for
/// the save), and they are read again when drawn.
#[test]
fn into_unsaved_keeps_only_the_dirty_docs_and_reads_them_again() {
    let mut docs = Documents::new();
    for path in ["a.rs", "b.rs"] {
        read_ok(&mut docs, path, "a\n");
    }
    docs.get_mut(&k("b.rs"))
        .unwrap()
        .editor_mut()
        .unwrap()
        .insert("x");
    let mut kept = docs.into_unsaved().expect("one doc is unsaved");
    assert_eq!(unsaved_paths(&kept), ["b.rs"]);
    assert!(kept.get(&k("a.rs")).is_none());
    assert!(
        kept.get_mut(&k("b.rs")).unwrap().take_read(),
        "read when drawn"
    );
    // Nothing is in flight any more.
    assert!(kept.begin_read(&k("b.rs")));
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
        docs.get_mut(&k(path))
            .unwrap()
            .editor_mut()
            .unwrap()
            .insert("x");
    }
    let paths = [k("a.rs"), k("b.rs")];
    assert_eq!(
        docs.first_unsaved(&paths).map(|file| file.path).as_deref(),
        Some("a.rs")
    );
    let save = docs.begin_save(&k("a.rs")).unwrap();
    docs.get_mut(&k("a.rs"))
        .unwrap()
        .editor_mut()
        .unwrap()
        .insert("y"); // typed while saving
    let written = FsWritten {
        saved: true,
        hash: "#xa\n".into(),
    };
    docs.saved(&k("a.rs"), save.point, Ok(written));
    assert_eq!(
        docs.first_unsaved(&paths).map(|file| file.path).as_deref(),
        Some("a.rs"),
        "asked again"
    );
    docs.drop_changes(&k("a.rs"));
    assert_eq!(
        docs.first_unsaved(&paths).map(|file| file.path).as_deref(),
        Some("b.rs")
    );
}

/// "Don't save" drops the changes and reads the file again, so the doc shows what is on disk.
#[test]
fn dropping_changes_rereads_the_file() {
    let mut docs = opened("a\n");
    editor(&mut docs).insert("x");
    assert_eq!(
        docs.drop_changes(&k(P)).map(|file| file.path).as_deref(),
        Some("README.md")
    );
    assert_eq!(unsaved(&docs), None);
    docs.begin_read(&k(P));
    docs.read(&k(P), file_text("a\n"));
    assert_eq!(text(&docs), "a\n", "the disk's text, not the dropped edit");
    assert_eq!(docs.drop_changes(&k("gone.rs")), None);
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

use mkrew_protocol::TerminalTab;
use serde_json::json;

fn wire(project: Ulid, position: f64, layout: serde_json::Value) -> TerminalTab {
    TerminalTab {
        id: Ulid::new(),
        project_id: project,
        title: "Terminal".to_owned(),
        position,
        layout,
        pinned: false,
    }
}

/// One tab per layout, the first one active.
fn tabs_of(layouts: Vec<serde_json::Value>) -> Tabs {
    let project = Ulid::new();
    let mut tabs = Tabs::new(project);
    let wires = (layouts.into_iter().enumerate())
        .map(|(at, layout)| wire(project, at as f64 + 1.0, layout))
        .collect();
    tabs.load(wires);
    tabs
}

fn file(path: &str) -> serde_json::Value {
    json!({"file": path})
}

fn preview(path: &str) -> serde_json::Value {
    json!({"file": path, "preview": true})
}

fn key(path: &str) -> PaneKey {
    PaneKey::File(FileKey::project(path))
}

#[test]
fn opening_what_is_open_focuses_it_whatever_the_place() {
    let tabs = tabs_of(vec![file("a.rs"), file("b.rs")]);
    for place in [
        Place::Preview,
        Place::NewTab,
        Place::Split(key("a.rs"), Edge::Left),
    ] {
        // b.rs sits in the second tab: it is focused there, never opened a second time.
        assert_eq!(
            resolve_place(&tabs, None, &key("b.rs"), &place),
            Resolution::Focus(key("b.rs"))
        );
    }
}

#[test]
fn a_single_click_reuses_the_preview_tab_and_otherwise_opens_one() {
    let tabs = tabs_of(vec![file("a.rs"), preview("p.rs")]);
    assert_eq!(
        resolve_place(&tabs, None, &key("c.rs"), &Place::Preview),
        Resolution::Reuse { old: key("p.rs") }
    );
    let none = tabs_of(vec![file("a.rs")]);
    assert_eq!(
        resolve_place(&none, None, &key("c.rs"), &Place::Preview),
        Resolution::NewTab {
            position: none.position_after_active()
        }
    );
}

#[test]
fn a_kept_open_never_takes_the_preview_tab() {
    let tabs = tabs_of(vec![file("a.rs"), preview("p.rs")]);
    assert_eq!(
        resolve_place(&tabs, None, &key("c.rs"), &Place::NewTab),
        Resolution::NewTab {
            position: tabs.position_after_active()
        }
    );
}

#[test]
fn a_new_tab_goes_right_after_the_active_one() {
    let tabs = tabs_of(vec![file("a.rs"), file("b.rs")]);
    let position = tabs.position_after_active().expect("two tabs");
    let second = tabs.tabs()[1].position;
    assert!(position > tabs.tabs()[0].position && position < second);
    assert_eq!(
        resolve_place(&tabs, None, &key("c.rs"), &Place::NewTab),
        Resolution::NewTab {
            position: Some(position)
        }
    );
}

#[test]
fn a_drop_splits_the_target_at_the_edge() {
    let tabs = tabs_of(vec![file("a.rs")]);
    assert_eq!(
        resolve_place(
            &tabs,
            None,
            &key("c.rs"),
            &Place::Split(key("a.rs"), Edge::Top)
        ),
        Resolution::Split {
            target: key("a.rs"),
            edge: Edge::Top
        }
    );
}

#[test]
fn the_diff_never_takes_a_preview_tab() {
    let tabs = tabs_of(vec![preview("p.rs")]);
    assert_eq!(
        resolve_place(&tabs, None, &PaneKey::Diff, &Place::Preview),
        Resolution::NewTab {
            position: tabs.position_after_active()
        }
    );
}

#[test]
fn a_preview_with_unsaved_text_is_not_replaced() {
    let tabs = tabs_of(vec![preview("p.rs")]);
    let mut docs = Documents::new();
    read_ok(&mut docs, "p.rs", "a\n");
    docs.get_mut(&k("p.rs"))
        .unwrap()
        .editor_mut()
        .unwrap()
        .insert("x");
    assert_eq!(
        resolve_place(&tabs, Some(&docs), &key("c.rs"), &Place::Preview),
        Resolution::NewTab {
            position: tabs.position_after_active()
        }
    );
}

/// An edit keeps the preview tab: the leaf stops being a preview, so the next single click
/// opens another tab instead of replacing the file being edited.
#[test]
fn an_edit_flips_the_preview_leaf_to_kept_once() {
    let mut tabs = tabs_of(vec![preview("p.rs")]);
    assert!(tabs.preview_tab().is_some());
    let write = tabs.update_leaf(&key("p.rs"), keep_leaf);
    assert!(write.is_some(), "the layout is saved");
    assert!(tabs.preview_tab().is_none());
    assert!(
        tabs.update_leaf(&key("p.rs"), keep_leaf).is_none(),
        "nothing changed: no write"
    );
    assert_eq!(
        resolve_place(&tabs, None, &key("c.rs"), &Place::Preview),
        Resolution::NewTab {
            position: tabs.position_after_active()
        }
    );
}

#[test]
fn the_markdown_mode_is_kept_in_the_leaf() {
    let mut tabs = tabs_of(vec![file("a.md")]);
    let set = |leaf: &mut Leaf| set_leaf_mode(leaf, MdMode::Split);
    assert!(tabs.update_leaf(&key("a.md"), set).is_some());
    assert!(tabs.update_leaf(&key("a.md"), set).is_none());
    let layout = tabs.tabs()[0].layout().expect("a layout");
    assert_eq!(
        layout.leaf_of(&key("a.md")),
        Some(&Leaf::File {
            file: "a.md".into(),
            task: None,
            md: Some(MdMode::Split),
            preview: false
        })
    );
}

fn dirty_store(paths: &[&str]) -> Documents {
    let mut docs = Documents::new();
    for path in paths {
        read_ok(&mut docs, path, "a\n");
        docs.get_mut(&k(path))
            .unwrap()
            .editor_mut()
            .unwrap()
            .insert("x");
    }
    docs
}

fn set(paths: &[&str]) -> HashSet<FileKey> {
    paths.iter().map(|path| k(path)).collect()
}

#[test]
fn closing_a_leaf_with_unsaved_text_asks_and_moving_it_never_does() {
    let docs = dirty_store(&["a.rs", "b.rs"]);
    // Close: b.rs leaves every layout.
    assert_eq!(
        needs_save_question(&set(&["a.rs", "b.rs"]), &set(&["a.rs"]), &docs),
        [k("b.rs")]
    );
    // Move (drag, tear-off, merge): the leaf is somewhere else afterwards, still open.
    assert!(
        needs_save_question(&set(&["a.rs", "b.rs"]), &set(&["a.rs", "b.rs"]), &docs).is_empty()
    );
    // A clean file closes without a question.
    let clean = Documents::new();
    assert!(needs_save_question(&set(&["a.rs"]), &set(&[]), &clean).is_empty());
    // Several leaving at once are asked in path order.
    assert_eq!(
        needs_save_question(&set(&["b.rs", "a.rs"]), &set(&[]), &docs),
        [k("a.rs"), k("b.rs")]
    );
}

#[test]
fn closing_tabs_and_panes_are_told_from_moves_by_the_remaining_open_set() {
    let tabs = tabs_of(vec![
        file("a.rs"),
        json!({"split": "vertical", "ratio": 0.5, "children": [file("b.rs"), file("c.rs")]}),
    ]);
    let second = tabs.tabs()[1].id;
    assert_eq!(open_files(&tabs), set(&["a.rs", "b.rs", "c.rs"]));
    // "Close others" of the first tab: b.rs and c.rs go.
    assert_eq!(open_files_without(&tabs, &[second], &[]), set(&["a.rs"]));
    // Closing one pane.
    assert_eq!(
        open_files_without(&tabs, &[], &[key("c.rs")]),
        set(&["a.rs", "b.rs"])
    );
    // A pane moved to the first tab is in both sets: the difference is empty.
    let docs = dirty_store(&["b.rs"]);
    assert!(needs_save_question(&open_files(&tabs), &open_files(&tabs), &docs).is_empty());
}

#[test]
fn a_hidden_worktree_file_keeps_its_worktree_in_the_watch_set() {
    let (t1, t2) = (Ulid::new(), Ulid::new());
    let tabs = tabs_of(vec![
        file("a.rs"),
        json!({"file": "b.rs", "task": t1.to_string()}),
        json!({"split": "vertical", "ratio": 0.5, "children": [
            json!({"file": "c.rs", "task": t2.to_string()}),
            json!({"file": "d.rs", "task": t2.to_string()}),
        ]}),
    ]);
    // The first tab is the shown one: t1 and t2 are of hidden tabs and still count.
    assert_eq!(worktrees(open_files(&tabs)), HashSet::from([t1, t2]));
    // Closing the last file of t1 drops it; t2 still has two files.
    let closed = PaneKey::File(FileKey::new("b.rs", Some(t1)));
    let rest = worktrees(open_files_without(&tabs, &[], &[closed]));
    assert_eq!(rest, HashSet::from([t2]));
    // The Files tree on t1 keeps it regardless.
    let needs = crate::watches::Needs::of(Some(Ulid(1)), Some(t1), None, rest);
    assert_eq!(
        needs,
        crate::watches::Needs::of(Some(Ulid(1)), None, None, [t1, t2])
    );
}

#[test]
fn a_doc_loads_once_on_its_first_draw() {
    let mut docs = Documents::new();
    docs.ensure(&k("a.rs"), MdMode::Source);
    // The shell's first draw asks; a second draw before the answer asks for nothing.
    assert!(docs.get_mut(&k("a.rs")).unwrap().take_read());
    assert!(!docs.get_mut(&k("a.rs")).unwrap().take_read());
    assert!(docs.begin_read(&k("a.rs")));
    assert!(
        !docs.begin_read(&k("a.rs")),
        "one read in flight; the next is queued"
    );
    assert_eq!(
        docs.read(&k("a.rs"), file_text_at("a.rs", "x\n")),
        Some(k("a.rs"))
    );
}

#[test]
fn a_failed_read_is_retried_by_retry_and_not_by_a_draw() {
    let mut docs = Documents::new();
    docs.ensure(&k("a.rs"), MdMode::Source);
    assert!(docs.get_mut(&k("a.rs")).unwrap().take_read());
    assert!(docs.begin_read(&k("a.rs")));
    docs.read(
        &k("a.rs"),
        Err("no file a.rs in project 01J0000000000000000000000".into()),
    );
    assert!(docs.get(&k("a.rs")).unwrap().removed());
    // Drawing again asks for nothing: no loop on a failing file.
    assert!(!docs.get_mut(&k("a.rs")).unwrap().take_read());
    // Retry reads again, and a coalesced read is sent once the first answers.
    assert!(docs.get_mut(&k("a.rs")).unwrap().retry());
    assert!(docs.begin_read(&k("a.rs")));
    assert!(!docs.begin_read(&k("a.rs")));
    assert_eq!(
        docs.read(&k("a.rs"), file_text_at("a.rs", "ok\n")),
        Some(k("a.rs"))
    );
    assert!(!docs.get(&k("a.rs")).unwrap().removed());
}

#[test]
fn a_file_that_comes_back_replaces_the_removed_state() {
    let mut docs = Documents::new();
    docs.ensure(&k("a.rs"), MdMode::Source);
    assert!(docs.begin_read(&k("a.rs")));
    docs.read(
        &k("a.rs"),
        Err("no file a.rs in project 01J0000000000000000000000".into()),
    );
    // git.changed: the drawn doc is read again and now exists.
    assert_eq!(docs.refresh(None, &[k("a.rs")]), [k("a.rs")]);
    assert!(docs.begin_read(&k("a.rs")));
    docs.read(&k("a.rs"), file_text_at("a.rs", "back\n"));
    assert_eq!(
        docs.get(&k("a.rs")).unwrap().shown().unwrap().editor.text(),
        "back\n"
    );
}

#[test]
fn removed_unsaved_text_is_kept() {
    let mut docs = opened("a\n");
    editor(&mut docs).insert("x");
    assert!(docs.begin_read(&k(P)));
    docs.read(
        &k(P),
        Err("no file README.md in project 01J0000000000000000000000".into()),
    );
    assert!(
        doc(&docs).is_dirty(),
        "a failed re-read never replaces unsaved text"
    );
    assert!(!doc(&docs).removed());
}

#[test]
fn find_follows_a_reread_file() {
    let mut docs = opened("todo\nok");
    let mut find = TextFind::default();
    find.query = "todo".into();
    doc_mut(&mut docs).find = Some(find);
    doc_mut(&mut docs).search();
    let label = |docs: &Documents| doc(docs).find.as_ref().map(TextFind::label);
    assert_eq!(label(&docs).as_deref(), Some("1 of 1"));
    // An agent edits the file: the re-read is searched again.
    docs.begin_read(&k(P));
    docs.read(&k(P), file_text("todo\ntodo"));
    assert_eq!(label(&docs).as_deref(), Some("1 of 2"));
}

#[test]
fn find_opens_seeded_with_a_one_line_selection_at_the_caret() {
    let mut docs = opened("let a = b;\nlet b = a;\n");
    editor(&mut docs).select(Pos::new(1, 4)..Pos::new(1, 5));
    assert!(doc_mut(&mut docs).open_find(false));
    let find = doc(&docs).find.as_ref().unwrap();
    assert_eq!((find.query.as_str(), find.replace.as_deref()), ("b", None));
    let at = find.current().map(|m| (m.line, m.range.start));
    assert_eq!(at, Some((1, 4)), "the selected one is current");
    // Ctrl+H adds the replace row; a multi-line selection keeps the query.
    editor(&mut docs).select(Pos::new(0, 0)..Pos::new(1, 1));
    assert!(doc_mut(&mut docs).open_find(true));
    let find = doc(&docs).find.as_ref().unwrap();
    assert_eq!(
        (find.query.as_str(), find.replace.as_deref()),
        ("b", Some(""))
    );
    // Without a file there is nothing to find in.
    assert!(!Doc::new(&k("a.rs"), MdMode::Source).open_find(false));
}

#[test]
fn replace_one_moves_on_and_replace_all_is_one_undo_step() {
    let mut docs = opened("a1 A2\na3\n");
    assert!(doc_mut(&mut docs).open_find(true));
    let find = doc_mut(&mut docs).find.as_mut().unwrap();
    find.query = "a".into();
    find.replace = Some("aa".into());
    doc_mut(&mut docs).search();
    assert!(doc_mut(&mut docs).replace_one());
    assert_eq!(text(&docs), "aa1 A2\na3\n");
    let at = (doc(&docs).find.as_ref())
        .and_then(TextFind::current)
        .map(|m| (m.line, m.range.start));
    assert_eq!(
        at,
        Some((0, 4)),
        "the next match, not one inside the replacement"
    );
    assert!(doc_mut(&mut docs).replace_one());
    assert!(doc_mut(&mut docs).replace_one());
    assert_eq!(text(&docs), "aa1 aa2\naa3\n");
    // Replace all: each match once (the replacement holds the query), ignoring case,
    // in UTF-8; one undo step; not saved.
    let mut docs = opened("Tiếng việt, VIỆT\nviệt\n");
    doc_mut(&mut docs).open_find(true);
    let find = doc_mut(&mut docs).find.as_mut().unwrap();
    find.query = "việt".into();
    find.replace = Some("Nam việt".into());
    assert_eq!(doc_mut(&mut docs).replace_all(), 3);
    assert_eq!(text(&docs), "Tiếng Nam việt, Nam việt\nNam việt\n");
    assert_eq!(unsaved(&docs).as_deref(), Some("README.md"));
    let label = doc(&docs).find.as_ref().map(TextFind::label);
    assert_eq!(label.as_deref(), Some("1 of 3"));
    assert!(editor(&mut docs).undo());
    assert_eq!(text(&docs), "Tiếng việt, VIỆT\nviệt\n");
    assert_eq!(unsaved(&docs), None);
    // Without the replace row, or a match, nothing is replaced (no empty undo step).
    doc_mut(&mut docs).find.as_mut().unwrap().replace = None;
    assert_eq!(doc_mut(&mut docs).replace_all(), 0);
    assert!(!doc_mut(&mut docs).replace_one());
    let find = doc_mut(&mut docs).find.as_mut().unwrap();
    (find.query, find.replace) = ("zzz".into(), Some(Input::default()));
    assert_eq!(doc_mut(&mut docs).replace_all(), 0);
    assert!(!editor(&mut docs).undo());
}

/// Find and a link's line need the editor on screen.
#[test]
fn find_and_a_link_line_switch_to_source() {
    let mut docs = opened("a\nb\n");
    assert!(doc_mut(&mut docs).open_find(false));
    assert_eq!(doc(&docs).mode, MdMode::Source);
    let mut docs = opened("a\nb\n");
    doc_mut(&mut docs).set_line(Some((2, 1)));
    assert_eq!(doc(&docs).mode, MdMode::Source);
    // A request without a line keeps the mode.
    let mut docs = opened("a\n");
    doc_mut(&mut docs).set_line(None);
    assert_eq!(doc(&docs).mode, MdMode::Preview);
}

#[test]
fn a_split_doc_keeps_split_for_find_and_links() {
    let mut docs = Documents::new();
    docs.ensure(&k(P), MdMode::Split);
    assert!(docs.begin_read(&k(P)));
    docs.read(&k(P), file_text("a\nb\n"));
    assert!(doc(&docs).splitting() && !doc(&docs).previewing());
    // The editor is on screen next to the page: find and a link's line keep the split.
    assert!(doc_mut(&mut docs).open_find(false));
    doc_mut(&mut docs).set_line(Some((2, 1)));
    assert_eq!(doc(&docs).mode, MdMode::Split);
}

#[test]
fn ctrl_f_leaves_the_preview_and_drops_edit_and_selection() {
    let mut docs = previewed("a\n\nb\n");
    let readme = doc_mut(&mut docs);
    assert!(readme.open_block_edit(0));
    readme.preview_select_all();
    assert!(readme.open_find(false));
    assert!(readme.block_edit.is_none() && readme.preview_selection.is_none());
}

/// A link's line is asked before the leaf is in a layout (a new tab is made first): the clean
/// doc must survive the sync that runs in between.
#[test]
fn a_doc_with_a_pending_line_survives_until_its_leaf_arrives() {
    let mut docs = Documents::new();
    docs.ensure(&k("link.rs"), MdMode::default());
    docs.get_mut(&k("link.rs")).unwrap().set_line(Some((7, 1)));
    docs.retain_open(&HashSet::new());
    assert!(
        docs.get(&k("link.rs")).is_some(),
        "its line is still wanted"
    );
    assert_eq!(
        docs.get_mut(&k("link.rs")).unwrap().apply_pending_line(),
        None,
        "not shown yet"
    );
    docs.get_mut(&k("link.rs")).unwrap().set_line(None);
    docs.retain_open(&HashSet::new());
    assert!(
        docs.get(&k("link.rs")).is_none(),
        "nothing pending, no leaf, clean"
    );
}

/// A file of a task worktree is its own doc, apart from the project's file of the same path.
#[test]
fn the_same_path_in_a_worktree_is_another_doc() {
    let task = Ulid::new();
    let (project, worktree) = (k("a.rs"), FileKey::new("a.rs", Some(task)));
    let mut docs = Documents::new();
    docs.ensure(&project, MdMode::Source);
    docs.ensure(&worktree, MdMode::Source);
    assert_eq!(docs.get(&worktree).unwrap().file, worktree);
    assert_eq!(docs.get(&project).unwrap().file, project);
    // `git.changed` of the worktree touches its docs only.
    assert!(docs.get_mut(&project).unwrap().take_read());
    assert!(docs.get_mut(&worktree).unwrap().take_read());
    assert_eq!(
        docs.refresh(Some(task), std::slice::from_ref(&worktree)),
        std::slice::from_ref(&worktree)
    );
    assert!(
        !docs.get_mut(&project).unwrap().take_read(),
        "the project's doc was not touched"
    );
    assert_eq!(docs.refresh(None, &[]), Vec::<FileKey>::new());
    assert!(
        docs.get_mut(&project).unwrap().take_read(),
        "stale: read when drawn"
    );
}

/// Reads and saves go to the folder of the file, never to the Files tree's: the requests
/// are built from the file's own task.
#[test]
fn a_worktree_file_is_read_and_saved_in_its_worktree() {
    let (project_id, task) = (Ulid::new(), Ulid::new());
    let file = FileKey::new("src/a.rs", Some(task));
    let read = read_params(project_id, &file);
    assert_eq!(
        (read.worktree_task_id, read.path.as_str()),
        (Some(task), "src/a.rs")
    );
    let mut docs = Documents::new();
    docs.ensure(&file, MdMode::Source);
    assert!(docs.begin_read(&file));
    docs.read(&file, file_text_at("src/a.rs", "a\n"));
    docs.get_mut(&file)
        .unwrap()
        .editor_mut()
        .unwrap()
        .insert("x");
    let save = docs.begin_save(&file).expect("unsaved text");
    assert_eq!(save.task, Some(task));
    let write = write_params(project_id, save);
    assert_eq!(
        (write.worktree_task_id, write.path.as_str()),
        (Some(task), "src/a.rs")
    );
    // A project file has no task.
    let plain = read_params(project_id, &k("src/a.rs"));
    assert_eq!(plain.worktree_task_id, None);
}

/// After a lost connection an unsaved worktree doc still saves into its worktree.
#[test]
fn an_unsaved_worktree_doc_keeps_its_folder_over_a_reconnect() {
    let (project_id, task) = (Ulid::new(), Ulid::new());
    let file = FileKey::new("a.rs", Some(task));
    let mut docs = Documents::new();
    docs.ensure(&file, MdMode::Source);
    assert!(docs.begin_read(&file));
    docs.read(&file, file_text_at("a.rs", "a\n"));
    docs.get_mut(&file)
        .unwrap()
        .editor_mut()
        .unwrap()
        .insert("x");
    let mut kept = docs.into_unsaved().expect("unsaved");
    assert_eq!(kept.unsaved_files(), std::slice::from_ref(&file));
    let save = kept.begin_save(&file).expect("saves with the old hash");
    assert_eq!(write_params(project_id, save).worktree_task_id, Some(task));
}
