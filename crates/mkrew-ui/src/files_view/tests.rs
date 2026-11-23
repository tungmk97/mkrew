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
fn a_click_on_a_file_only_moves_the_cursor_and_the_shell_opens_it() {
    let mut view = listed();
    assert!(view.is_file("README.md"));
    assert!(!view.is_file("src"), "a folder");
    assert!(!view.is_file("gone.rs"), "not a listed row");
    assert_eq!(view.click("README.md"), Requests::default());
    assert_eq!(view.cursor.as_deref(), Some("README.md"));
}

#[test]
fn revealing_a_file_opens_its_folders_and_shows_it() {
    let mut view = listed();
    let revealed = view.reveal("src/app/main.rs");
    assert_eq!(revealed.list, ["src", "src/app"]);
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
    // Again: everything is open and listed.
    assert_eq!(view.reveal("src/app/main.rs"), Requests::default());
    // A file at the root opens no folder.
    let root = view.reveal("README.md");
    assert!(root.list.is_empty());
    assert_eq!(view.cursor.as_deref(), Some("README.md"));
}

#[test]
fn revealing_a_folder_opens_it_and_its_parents() {
    let mut view = listed();
    let revealed = view.reveal_dir("src/app");
    assert_eq!(revealed.list, ["src", "src/app"]);
    assert_eq!(view.cursor.as_deref(), Some("src/app"));
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
    // Enter is the shell's (it opens the file before `key` runs).
    assert_eq!(view.key(Key::Enter), Requests::default());
    view.key(Key::Up);
    view.key(Key::Up);
    assert_eq!(view.cursor.as_deref(), Some("src"));
    view.key(Key::Left);
    assert_eq!(paths(&view), ["src", "README.md"]);
    view.key(Key::Up);
    assert_eq!(view.cursor.as_deref(), Some("src"));
}

#[test]
fn git_changed_relists_open_folders() {
    let mut view = listed();
    view.click("src");
    view.begin_list("src");
    view.listed("src", Ok(vec![dir("src/a")]));
    view.click("src/a");
    view.begin_list("src/a");
    view.listed("src/a", Ok(vec![]));
    view.click("src/a");
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
            list: vec![String::new()]
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
fn enter_opens_the_cursor_row_only_while_it_is_listed() {
    let mut view = listed();
    assert_eq!(view.enter_target(), None, "no cursor yet");
    view.cursor = Some("README.md".into());
    assert_eq!(view.enter_target().as_deref(), Some("README.md"));
    view.cursor = Some("gone.txt".into());
    assert_eq!(view.enter_target(), None, "the row is gone");
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
