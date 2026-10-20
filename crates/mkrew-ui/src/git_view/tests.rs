use super::*;
use crate::highlight::Token;
use mkrew_protocol::{ChangeKind, DiffLineKind, GitChangedEvent, Hunk, ViewedFile};

fn file(path: &str, kind: ChangeKind) -> ChangedFile {
    ChangedFile {
        path: path.into(),
        old_path: None,
        kind,
        added: Some(1),
        removed: Some(0),
    }
}

fn working() -> GitStatus {
    GitStatus {
        branch: Some("main".into()),
        staged: vec![file("a.rs", ChangeKind::Modified)],
        unstaged: vec![
            file("a.rs", ChangeKind::Modified),
            file("b.rs", ChangeKind::Deleted),
        ],
        untracked: vec![file("new.txt", ChangeKind::Untracked)],
        ..GitStatus::default()
    }
}

fn target(group: Group, path: &str) -> Target {
    Target {
        group,
        path: path.into(),
        commit: None,
    }
}

fn line(kind: DiffLineKind, old: Option<u32>, new: Option<u32>) -> DiffLine {
    DiffLine {
        kind,
        text: "x".into(),
        old_line: old,
        new_line: new,
        no_newline: false,
        crlf: false,
    }
}

fn hunk(old_start: u32, old_lines: u32, new_start: u32, new_lines: u32) -> Hunk {
    Hunk {
        old_start,
        old_lines,
        new_start,
        new_lines,
        header: String::new(),
        lines: vec![line(
            DiffLineKind::Context,
            Some(old_start),
            Some(new_start),
        )],
    }
}

fn diff(hunks: Vec<Hunk>) -> FileDiff {
    FileDiff {
        path: "a.rs".into(),
        hunks,
        ..FileDiff::default()
    }
}

/// [`diff`] laid out, as `diff_loaded` gets it.
fn laid_out(hunks: Vec<Hunk>) -> Shown {
    Shown::new(diff(hunks))
}

fn del(old: u32) -> DiffLine {
    line(DiffLineKind::Del, Some(old), None)
}

fn add(new: u32) -> DiffLine {
    line(DiffLineKind::Add, None, Some(new))
}

fn ctx(old: u32, new: u32) -> DiffLine {
    line(DiffLineKind::Context, Some(old), Some(new))
}

/// A hunk holding `lines` (its counts only matter for the gaps between hunks).
fn hunk_of(lines: Vec<DiffLine>, base: Hunk) -> Hunk {
    Hunk { lines, ..base }
}

/// A Working view with [`working`] loaded and its first file selected.
fn loaded() -> GitView {
    let mut view = GitView::new(Ulid::new(), None);
    assert!(view.begin_status().is_some());
    let requests = view.status_loaded(Ok(working()));
    assert_eq!(requests.diff, Some(target(Group::Staged, "a.rs")));
    view
}

#[test]
fn files_are_grouped_and_the_first_is_selected() {
    let view = loaded();
    let rows = view.list_rows();
    let labels: Vec<String> = rows
        .iter()
        .map(|row| match row {
            ListRow::Header(group, count) => format!("{} {count}", group.title()),
            ListRow::File(_, file) => file.path.clone(),
            ListRow::Hint(hint) => (*hint).to_owned(),
            ListRow::Commit { .. } | ListRow::Note(_) => unreachable!("not in History"),
        })
        .collect();
    assert_eq!(
        labels,
        [
            "Staged 1",
            "a.rs",
            "Changes 2",
            "a.rs",
            "b.rs",
            "Untracked 1",
            "new.txt"
        ]
    );
    assert_eq!(view.diff, Diff::Loading);
}

#[test]
fn diff_params_follow_the_group() {
    let project = Ulid::new();
    let staged = target(Group::Staged, "a").params(project, None, None);
    assert!(staged.staged);
    assert_eq!(staged.mode, GitMode::Working);
    assert!(
        !target(Group::Untracked, "a")
            .params(project, None, None)
            .staged
    );
    assert_eq!(
        target(Group::Base, "a").params(project, None, None).mode,
        GitMode::Base
    );
    assert_eq!(
        target(Group::Base, "a").params(project, None, None).since,
        None
    );
}

#[test]
fn a_status_asked_while_one_is_in_flight_runs_after_it() {
    let mut view = GitView::new(Ulid::new(), None);
    assert!(view.begin_status().is_some());
    assert!(view.begin_status().is_none());
    assert!(view.status_loaded(Ok(GitStatus::default())).status);
    assert!(view.begin_status().is_some());
    let requests = view.status_loaded(Ok(GitStatus::default()));
    assert_eq!(requests, Requests::default());
    assert_eq!(view.selected, None);
}

#[test]
fn the_selection_follows_its_file_across_groups_or_falls_back_to_the_first() {
    let mut view = loaded();
    view.select(target(Group::Untracked, "new.txt"));
    // `new.txt` got staged: the selection moves with it and its diff is read.
    let mut status = working();
    status.untracked.clear();
    status.staged.push(file("new.txt", ChangeKind::Added));
    view.begin_status();
    let requests = view.status_loaded(Ok(status.clone()));
    assert_eq!(requests.diff, Some(target(Group::Staged, "new.txt")));
    // Unchanged: nothing to read.
    view.begin_status();
    assert_eq!(view.status_loaded(Ok(status)), Requests::default());
    // Gone: the first file.
    view.begin_status();
    let mut status = working();
    status.untracked.clear();
    let requests = view.status_loaded(Ok(status));
    assert_eq!(requests.diff, Some(target(Group::Staged, "a.rs")));
    // Nothing changed any more: nothing selected.
    view.begin_status();
    view.status_loaded(Ok(GitStatus::default()));
    assert_eq!(view.selected, None);
    assert_eq!(view.diff, Diff::None);
}

#[test]
fn a_failed_status_says_so_but_a_failed_refresh_keeps_the_list() {
    let mut view = loaded();
    view.begin_status();
    view.begin_status();
    // A refresh fails (asked again meanwhile): the list stays, the error is noted and the
    // status is asked again.
    assert!(view.status_loaded(Err("index.lock".into())).status);
    assert!(matches!(view.status, Status::Loaded(_)));
    assert_eq!(view.refresh_error.as_deref(), Some("index.lock"));
    assert!(view.selected.is_some());
    view.begin_status();
    view.status_loaded(Ok(working()));
    assert_eq!(view.refresh_error, None);
    // Nothing loaded yet: the failure is shown.
    view.toggle_mode();
    view.begin_status();
    assert_eq!(
        view.status_loaded(Err("no base".into())),
        Requests::default()
    );
    assert_eq!(view.status, Status::Failed("no base".into()));
    assert_eq!(view.selected, None);
}

#[test]
fn toggling_the_mode_reloads_and_drops_answers_for_the_old_mode() {
    let mut view = loaded();
    view.begin_status();
    let requests = view.key(Key::ToggleMode);
    assert_eq!(view.mode, GitMode::Base);
    assert!(requests.status);
    assert_eq!(view.status, Status::Loading);
    assert_eq!(view.selected, None);
    // The Working answer in flight is dropped and the status asked again…
    assert!(view.begin_status().is_none());
    assert!(view.status_loaded(Ok(working())).status);
    assert_eq!(view.status, Status::Loading);
    // …and the Base answer lists `against_base` without group titles.
    let params = view.begin_status().unwrap();
    assert_eq!(params.mode, GitMode::Base);
    let status = GitStatus {
        base_ref: Some("main".into()),
        against_base: vec![file("x.rs", ChangeKind::Added)],
        ..GitStatus::default()
    };
    let requests = view.status_loaded(Ok(status));
    assert_eq!(requests.diff, Some(target(Group::Base, "x.rs")));
    assert_eq!(
        view.list_rows(),
        [ListRow::File(
            target(Group::Base, "x.rs"),
            file("x.rs", ChangeKind::Added)
        )]
    );
}

fn task_changes(checkpoint: &str) -> Since {
    Since::task(Ulid::new(), "Fix login", checkpoint.into())
}

fn since(files: &[&str]) -> GitStatus {
    GitStatus {
        merge_base: Some("abc".into()),
        against_base: files
            .iter()
            .map(|path| file(path, ChangeKind::Modified))
            .collect(),
        ..GitStatus::default()
    }
}

#[test]
fn a_tasks_changes_list_and_diff_since_its_checkpoint() {
    let mut view = loaded();
    let task = task_changes("abc");
    let requests = view.show_since(task.clone());
    assert!(requests.status);
    assert_eq!(view.section(), Section::Since);
    assert_eq!(
        (view.status.clone(), view.selected.clone()),
        (Status::Loading, None)
    );
    assert_eq!(view.show_since(task.clone()), Requests::default());
    let params = view.begin_status().unwrap();
    assert_eq!(
        (params.mode, params.since.as_deref()),
        (GitMode::Base, Some("abc"))
    );
    assert!(!view.commit_box_shown());

    let requests = view.status_loaded(Ok(since(&["a.rs"])));
    let shown = Target {
        group: Group::Since,
        path: "a.rs".into(),
        commit: Some("abc".into()),
    };
    assert_eq!(requests.diff, Some(shown.clone()));
    let diff = shown.params(view.project_id, None, None);
    assert_eq!(diff.mode, GitMode::Base);
    assert_eq!((diff.commit, diff.since.as_deref()), (None, Some("abc")));
    assert_eq!(Stage::of(Group::Since), None);
    assert!(view.discard(&shown).is_none());

    // The History goes back to the task's kept list; the mode key leaves the task.
    view.toggle_history();
    assert_eq!(view.section(), Section::History);
    assert!(view.since.is_some());
    let requests = view.toggle_history();
    assert_eq!((view.section(), requests.status), (Section::Since, true));
    assert!(matches!(view.status, Status::Loaded(_)));
    view.toggle_mode();
    assert_eq!(
        (view.section(), view.since.clone()),
        (Section::Working, None)
    );
    assert_eq!(view.status, Status::Loading);
}

#[test]
fn answers_for_another_task_or_against_base_are_dropped() {
    let mut view = GitView::new(Ulid::new(), None);
    view.show(Section::Base);
    view.begin_status();
    // Against base was in flight: dropped, the task's status asked again.
    view.show_since(task_changes("abc"));
    assert!(view.status_loaded(Ok(since(&["base.rs"]))).status);
    assert_eq!(view.status, Status::Loading);
    view.begin_status();
    // Another task meanwhile: its own checkpoint.
    view.show_since(task_changes("def"));
    assert_eq!(view.status, Status::Loading);
    assert!(view.status_loaded(Ok(since(&["old.rs"]))).status);
    assert_eq!(view.begin_status().unwrap().since.as_deref(), Some("def"));
    view.status_loaded(Ok(since(&["new.rs"])));
    assert_eq!(view.files()[0].0.commit.as_deref(), Some("def"));
    // Against base from a task: no checkpoint any more.
    view.show(Section::Base);
    assert_eq!(view.begin_status().unwrap().since, None);
}

#[test]
fn the_commit_box_shows_with_working_changes_or_a_draft() {
    let mut view = GitView::new(Ulid::new(), None);
    assert!(!view.commit_box_shown(), "still loading");
    let mut view_loaded = loaded();
    assert!(view_loaded.commit_box_shown());
    view_loaded.key(Key::ToggleMode);
    assert!(!view_loaded.commit_box_shown(), "against base");

    view.begin_status();
    view.status_loaded(Ok(GitStatus::default()));
    assert!(!view.commit_box_shown(), "nothing changed, nothing typed");
    view.commit.insert("WIP");
    assert!(view.commit_box_shown(), "a draft stays reachable");
}

#[test]
fn diffs_are_read_once_coalesced_and_dropped_when_stale() {
    let mut view = loaded();
    let a = target(Group::Staged, "a.rs");
    assert!(view.begin_diff());
    assert_eq!(view.select(a.clone()), None, "already selected");
    // Another file is selected while the read is in flight: read once that answers.
    let b = target(Group::Unstaged, "b.rs");
    assert_eq!(view.select(b.clone()), Some(b.clone()));
    assert!(!view.begin_diff());
    assert_eq!(view.diff_loaded(&a, Ok(laid_out(vec![]))), Some(b.clone()));
    assert_eq!(view.diff, Diff::Loading, "a's answer is not shown");
    assert!(view.begin_diff());
    assert_eq!(view.diff_loaded(&b, Err("gone".into())), None);
    assert_eq!(view.diff, Diff::Failed("gone".into()));
    // Selecting a failed diff again retries.
    assert_eq!(view.select(b.clone()), Some(b));
}

#[test]
fn keys_move_files_and_hunks() {
    let mut view = loaded();
    assert_eq!(
        view.key(Key::Down).diff,
        Some(target(Group::Unstaged, "a.rs"))
    );
    view.key(Key::Down);
    view.key(Key::Down);
    assert_eq!(view.key(Key::Down).diff, None, "stays on the last file");
    assert_eq!(view.selected, Some(target(Group::Untracked, "new.txt")));
    assert_eq!(
        view.key(Key::Up).diff,
        Some(target(Group::Unstaged, "b.rs"))
    );

    // Hunks: none shown yet → nothing; then first / last and clamped.
    view.key(Key::NextHunk);
    assert_eq!(view.hunk, None);
    let shown = target(Group::Unstaged, "b.rs");
    view.begin_diff();
    view.diff_loaded(
        &shown,
        Ok(laid_out(vec![hunk(1, 1, 1, 1), hunk(20, 1, 20, 1)])),
    );
    view.key(Key::PreviousHunk);
    assert_eq!(view.hunk, Some(1));
    view.key(Key::NextHunk);
    assert_eq!(view.hunk, Some(1));
    view.key(Key::PreviousHunk);
    view.key(Key::PreviousHunk);
    assert_eq!(view.hunk, Some(0));
    // A refreshed diff with fewer hunks drops a hunk that no longer exists.
    view.key(Key::NextHunk);
    view.begin_diff();
    view.diff_loaded(&shown, Ok(laid_out(vec![hunk(1, 1, 1, 1)])));
    assert_eq!(view.hunk, None);
    // Another file clears it.
    view.key(Key::NextHunk);
    view.key(Key::Up);
    assert_eq!(view.hunk, None);
}

#[test]
fn git_changed_refreshes_the_status_and_the_shown_diff() {
    let mut view = GitView::new(Ulid::new(), None);
    let changed = |project_id| {
        Event::GitChanged(GitChangedEvent {
            project_id,
            worktree_task_id: None,
        })
    };
    assert_eq!(
        view.apply(&changed(view.project_id)),
        Requests::default(),
        "never shown"
    );
    view.begin_status();
    view.status_loaded(Ok(working()));
    assert_eq!(view.apply(&changed(Ulid::new())), Requests::default());
    let in_worktree = Event::GitChanged(GitChangedEvent {
        project_id: view.project_id,
        worktree_task_id: Some(Ulid::new()),
    });
    assert_eq!(view.apply(&in_worktree), Requests::default());
    assert_eq!(
        view.apply(&changed(view.project_id)),
        Requests {
            status: true,
            diff: Some(target(Group::Staged, "a.rs")),
            ..Requests::default()
        }
    );
}

#[test]
fn a_worktree_view_asks_its_worktree_and_follows_only_its_changes() {
    let task_id = Some(Ulid::new());
    let mut view = GitView::new(Ulid::new(), task_id);
    assert_eq!(view.begin_status().unwrap().worktree_task_id, task_id);
    view.status_loaded(Ok(working()));
    let project_id = view.project_id;
    let changed = |worktree_task_id| {
        Event::GitChanged(GitChangedEvent {
            project_id,
            worktree_task_id,
        })
    };
    assert_eq!(view.apply(&changed(None)), Requests::default());
    assert_eq!(view.apply(&changed(Some(Ulid::new()))), Requests::default());
    assert!(view.apply(&changed(task_id)).status);
    let staged = target(Group::Staged, "a.rs");
    let untracked = target(Group::Untracked, "new.txt");
    let staged_params = staged.params(project_id, task_id, None);
    assert_eq!(staged_params.worktree_task_id, task_id);
    let change = view.file_change(&staged).unwrap();
    assert_eq!(change.params.worktree_task_id, task_id);
    let discard = view.discard(&untracked).unwrap();
    assert_eq!(discard.params().worktree_task_id, task_id);
    assert_eq!(view.show_params("abc".into()).worktree_task_id, task_id);
}

fn repos(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn repo_chips_show_with_two_repositories_and_name_the_workspace_folder() {
    assert_eq!(repo_choices(&repos(&[]), "EA"), vec![]);
    // One repository: nothing to choose.
    assert_eq!(repo_choices(&repos(&["fe"]), "EA"), vec![]);
    assert_eq!(repo_choices(&repos(&[""]), "EA"), vec![]);
    assert_eq!(
        repo_choices(&repos(&["", "be", "fe"]), "EA"),
        vec![
            (None, "EA"),
            (Some("be".to_owned()), "be"),
            (Some("fe".to_owned()), "fe"),
        ]
    );
}

#[test]
fn the_shown_repo_is_kept_while_listed_else_the_default_is_picked() {
    // The workspace folder is no repository: the first child one, not an error.
    assert_eq!(
        pick_repo(None, &repos(&["be", "fe"])),
        Some("be".to_owned())
    );
    // Kept when the list refreshes.
    assert_eq!(
        pick_repo(Some("fe"), &repos(&["be", "fe"])),
        Some("fe".to_owned())
    );
    // Gone (folder removed or renamed): back to the default.
    assert_eq!(pick_repo(Some("fe"), &repos(&["", "be"])), None);
    assert_eq!(
        pick_repo(Some("fe"), &repos(&["be"])),
        Some("be".to_owned())
    );
    // The workspace folder is a repository: it stays the default.
    assert_eq!(pick_repo(None, &repos(&["", "fe"])), None);
    // No repository at all: the workspace folder (its error says why).
    assert_eq!(pick_repo(Some("fe"), &repos(&[])), None);
}

#[test]
fn a_repo_view_sends_its_repo_on_every_git_request() {
    let fe = Some("fe".to_owned());
    let mut view = GitView::new(Ulid::new(), None);
    view.repo = fe.clone();
    assert_eq!(view.at(), (None, fe.clone()));
    assert_eq!(view.begin_status().unwrap().repo, fe);
    view.status_loaded(Ok(working()));
    assert_eq!(view.show_params("abc".into()).repo, fe);
    let staged = target(Group::Staged, "a.rs");
    assert_eq!(view.file_change(&staged).unwrap().params.repo, fe);
    assert_eq!(
        view.discard(&target(Group::Untracked, "new.txt"))
            .unwrap()
            .params()
            .repo,
        fe
    );
    view.toggle_history();
    assert_eq!(view.begin_log().unwrap().repo, fe);
}

#[test]
fn rows_collapse_unchanged_lines_between_hunks() {
    let shown = Shown::new(diff(vec![
        hunk(1, 3, 1, 4),
        hunk(10, 2, 11, 2),
        // Pure insertion after old line 20.
        hunk(20, 0, 21, 1),
    ]));
    let kinds: Vec<String> = shown
        .rows
        .iter()
        .map(|row| match row {
            DiffRow::Gap(n) => format!("gap {n}"),
            DiffRow::Hunk(i) => format!("hunk {i}"),
            DiffRow::Line(_) => "line".into(),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "hunk 0", "line", "gap 6", "hunk 1", "line", "gap 9", "hunk 2", "line"
        ]
    );
    assert_eq!(shown.hunk_row(1, DiffMode::Unified), Some(3));
    assert_eq!(shown.hunk_row(3, DiffMode::Unified), None);
    assert_eq!(shown.number_width, 2, "line 21 is the largest");
    // A first hunk further down starts with a gap.
    assert_eq!(rows(&diff(vec![hunk(5, 2, 5, 2)]))[0], DiffRow::Gap(4));
}

#[test]
fn line_rows_carry_their_syntax_spans() {
    let mut first = hunk(1, 1, 1, 1);
    first.lines[0].text = "// one".into();
    let mut second = hunk(10, 1, 10, 1);
    second.lines[0].text = "fn two() {}".into();
    let shown = Shown::new(diff(vec![first, second]));
    assert_eq!(shown.spans.len(), shown.rows.len());
    assert!(shown.highlighted);
    assert!(
        !Shown::new(diff(vec![hunk(1, 1, 1, 1)])).highlighted,
        "\"x\" has no tokens"
    );
    // hunk 0, line, gap, hunk 1, line.
    assert!(shown.spans[0].is_empty() && shown.spans[2].is_empty());
    assert_eq!(shown.spans[1], [(0..6, Token::Comment)]);
    assert!(
        shown.spans[4].contains(&(0..2, Token::Keyword)),
        "{:?}",
        shown.spans[4]
    );
}

/// Rows of `hunk_of(…)` below: 0 = hunk header, then the lines in order.
#[test]
fn split_pairs_a_change_in_order_and_pads_the_shorter_side() {
    // ctx, 3 removed, 2 added, ctx.
    let shown = Shown::new(diff(vec![hunk_of(
        vec![ctx(1, 1), del(2), del(3), del(4), add(2), add(3), ctx(5, 4)],
        hunk(1, 1, 1, 1),
    )]));
    assert_eq!(
        shown.split,
        [
            SplitRow::Span(0),
            SplitRow::Same(1),
            SplitRow::Change {
                old: Some(2),
                new: Some(5)
            },
            SplitRow::Change {
                old: Some(3),
                new: Some(6)
            },
            SplitRow::Change {
                old: Some(4),
                new: None
            },
            SplitRow::Same(7),
        ]
    );
    // More added than removed: the left side is padded.
    let shown = Shown::new(diff(vec![hunk_of(
        vec![del(2), add(2), add(3), add(4)],
        hunk(1, 1, 1, 1),
    )]));
    assert_eq!(
        shown.split,
        [
            SplitRow::Span(0),
            SplitRow::Change {
                old: Some(1),
                new: Some(2)
            },
            SplitRow::Change {
                old: None,
                new: Some(3)
            },
            SplitRow::Change {
                old: None,
                new: Some(4)
            },
        ]
    );
    // A pure insertion and a pure deletion are one-sided (base `hunk(1, 1, 1, 1)` keeps
    // the first hunk free of a leading gap row).
    let shown = Shown::new(diff(vec![hunk_of(vec![add(1), add(2)], hunk(1, 1, 1, 1))]));
    assert_eq!(
        shown.split[1..],
        [
            SplitRow::Change {
                old: None,
                new: Some(1)
            },
            SplitRow::Change {
                old: None,
                new: Some(2)
            },
        ]
    );
    let shown = Shown::new(diff(vec![hunk_of(vec![del(1)], hunk(1, 1, 1, 0))]));
    assert_eq!(
        shown.split[1..],
        [SplitRow::Change {
            old: Some(1),
            new: None
        }]
    );
}

/// Unified colors a changed-in-place line amber: only rows of a removed + added pair.
#[test]
fn only_paired_rows_are_modified() {
    // rows: 0 hunk, 1 ctx, 2 del, 3 del, 4 add, 5 ctx, 6 add (lone), 7 ctx, 8 del (lone).
    let shown = Shown::new(diff(vec![hunk_of(
        vec![
            ctx(1, 1),
            del(2),
            del(3),
            add(2),
            ctx(4, 3),
            add(4),
            ctx(5, 5),
            del(6),
        ],
        hunk(1, 1, 1, 1),
    )]));
    assert_eq!(
        shown.modified,
        [false, false, true, false, true, false, false, false, false]
    );
}

#[test]
fn split_never_pairs_across_a_gap_or_a_hunk_header() {
    // Hunk 0 ends with a removed line, hunk 1 (far below) starts with an added one.
    let shown = Shown::new(diff(vec![
        hunk_of(vec![del(1)], hunk(1, 1, 1, 0)),
        hunk_of(vec![add(10)], hunk(10, 1, 10, 1)),
    ]));
    // rows: 0 hunk, 1 del, 2 gap, 3 hunk, 4 add.
    assert_eq!(
        shown.split,
        [
            SplitRow::Span(0),
            SplitRow::Change {
                old: Some(1),
                new: None
            },
            SplitRow::Span(2),
            SplitRow::Span(3),
            SplitRow::Change {
                old: None,
                new: Some(4)
            },
        ]
    );
}

/// Comment picking and the quote read new-side numbers: the right side must hold every
/// new-side line, in order, and only added / unchanged lines; the left side the old ones.
#[test]
fn split_sides_keep_every_old_and_new_line_in_order() {
    let shown = Shown::new(diff(vec![hunk_of(
        vec![
            ctx(1, 1),
            del(2),
            add(2),
            add(3),
            ctx(3, 4),
            del(4),
            ctx(5, 5),
        ],
        hunk(1, 5, 1, 5),
    )]));
    let line_at = |at: usize| match &shown.rows[at] {
        DiffRow::Line(line) => line.clone(),
        other => panic!("not a line: {other:?}"),
    };
    let (mut olds, mut news) = (Vec::new(), Vec::new());
    for row in &shown.split {
        let (left, right) = match row {
            SplitRow::Span(_) => continue,
            SplitRow::Same(at) => (Some(*at), Some(*at)),
            SplitRow::Change { old, new } => {
                assert!(
                    old.iter()
                        .all(|&at| matches!(line_at(at).kind, DiffLineKind::Del))
                );
                assert!(
                    new.iter()
                        .all(|&at| matches!(line_at(at).kind, DiffLineKind::Add))
                );
                (*old, *new)
            }
        };
        olds.extend(left.and_then(|at| line_at(at).old_line));
        news.extend(right.and_then(|at| line_at(at).new_line));
    }
    assert_eq!(olds, [1, 2, 3, 4, 5]);
    assert_eq!(news, [1, 2, 3, 4, 5]);
}

/// Hunk navigation (n / p) scrolls to the header's row of the mode on screen.
#[test]
fn hunk_rows_follow_the_diff_mode() {
    let shown = Shown::new(diff(vec![
        hunk_of(vec![ctx(1, 1), del(2), add(2), add(3)], hunk(1, 2, 1, 3)),
        hunk(10, 1, 10, 1),
    ]));
    // Unified: hunk, ctx, del, add, add, gap, hunk, ctx. Split pairs del+add: one row less.
    assert_eq!(shown.hunk_row(1, DiffMode::Unified), Some(6));
    assert_eq!(shown.hunk_row(1, DiffMode::Split), Some(5));
    assert_eq!(shown.hunk_row(0, DiffMode::Split), Some(0));
    assert_eq!(shown.hunk_row(2, DiffMode::Split), None);
    assert_eq!(shown.list_len(DiffMode::Unified), 8);
    assert_eq!(shown.list_len(DiffMode::Split), 7);
}

fn paths(request: &StageRequest) -> (Stage, Vec<String>, Option<u32>) {
    let hunk = request.params.hunk.as_ref().map(|target| {
        assert_eq!(target.path, "a.rs");
        target.hunk.old_start
    });
    (request.stage, request.params.paths.clone(), hunk)
}

#[test]
fn files_stage_and_unstage_by_path_from_their_group() {
    let view = loaded();
    let staged = view.file_change(&target(Group::Staged, "a.rs")).unwrap();
    assert_eq!(paths(&staged), (Stage::Unstage, vec!["a.rs".into()], None));
    assert!(matches!(staged.request(), Request::GitUnstage(_)));
    let untracked = view
        .file_change(&target(Group::Untracked, "new.txt"))
        .unwrap();
    assert_eq!(untracked.stage, Stage::Stage);
    assert!(matches!(untracked.request(), Request::GitStage(_)));
    assert_eq!(view.file_change(&target(Group::Unstaged, "gone.rs")), None);
    assert_eq!(view.file_change(&target(Group::Base, "a.rs")), None);

    // A staged rename unstages both paths.
    let mut view = GitView::new(Ulid::new(), None);
    view.begin_status();
    let mut status = working();
    status.staged = vec![ChangedFile {
        old_path: Some("old.rs".into()),
        ..file("new.rs", ChangeKind::Renamed)
    }];
    view.status_loaded(Ok(status));
    let renamed = view.file_change(&target(Group::Staged, "new.rs")).unwrap();
    assert_eq!(renamed.params.paths, ["new.rs", "old.rs"]);
}

#[test]
fn keys_stage_the_current_hunk_of_a_modified_file_else_the_file() {
    let mut view = loaded();
    let a = target(Group::Staged, "a.rs");
    // The selection is staged: `s` does nothing, `u` unstages the file.
    assert_eq!(view.stage_key(Stage::Stage), None);
    let file = view.stage_key(Stage::Unstage).unwrap();
    assert_eq!(paths(&file), (Stage::Unstage, vec!["a.rs".into()], None));
    // With a current hunk, only that hunk.
    view.begin_diff();
    view.diff_loaded(&a, Ok(laid_out(vec![hunk(1, 1, 1, 1), hunk(20, 1, 20, 1)])));
    view.key(Key::PreviousHunk);
    let current = view.stage_key(Stage::Unstage).unwrap();
    assert_eq!(paths(&current), (Stage::Unstage, vec![], Some(20)));
    assert_eq!(view.hunk_change(5), None);
    // A deleted file goes by path even with a current hunk.
    let b = target(Group::Unstaged, "b.rs");
    view.select(b.clone());
    view.begin_diff();
    view.diff_loaded(&b, Ok(laid_out(vec![hunk(1, 1, 0, 0)])));
    view.key(Key::NextHunk);
    assert_eq!(view.hunk_change(0), None);
    let file = view.stage_key(Stage::Stage).unwrap();
    assert_eq!(paths(&file), (Stage::Stage, vec!["b.rs".into()], None));
}

#[test]
fn one_stage_at_a_time_and_failures_are_shown() {
    let mut view = loaded();
    assert!(view.begin_stage());
    assert!(!view.begin_stage());
    assert_eq!(view.stage_key(Stage::Unstage), None, "in flight");
    assert_eq!(
        view.staged(Err("patch does not apply".into())),
        Requests::default()
    );
    assert_eq!(view.stage_error.as_deref(), Some("patch does not apply"));
    assert!(view.begin_stage());
    assert_eq!(view.stage_error, None);
    assert_eq!(
        view.staged(Ok(())),
        Requests {
            status: true,
            diff: Some(target(Group::Staged, "a.rs")),
            ..Requests::default()
        }
    );
}

#[test]
fn only_unstaged_and_untracked_files_can_be_discarded() {
    let mut view = loaded();
    assert_eq!(view.discard(&target(Group::Staged, "a.rs")), None);
    assert_eq!(view.discard(&target(Group::Unstaged, "gone.rs")), None);
    let a = view.discard(&target(Group::Unstaged, "a.rs")).unwrap();
    assert!(a.also_staged);
    assert!(a.sentence().contains("Its staged changes stay."));
    let params = a.params();
    assert!(params.confirm);
    assert_eq!(params.paths, ["a.rs"]);
    let b = view.discard(&target(Group::Unstaged, "b.rs")).unwrap();
    assert!(!b.also_staged);
    let new = view.discard(&target(Group::Untracked, "new.txt")).unwrap();
    assert!(new.sentence().contains("is deleted"));
    // What is lost: the shown diff, once the file is selected and its diff loaded.
    let target_b = target(Group::Unstaged, "b.rs");
    assert_eq!(view.shown_diff(&target_b), None);
    view.select(target_b.clone());
    view.begin_diff();
    view.diff_loaded(&target_b, Ok(laid_out(vec![hunk(1, 1, 0, 0)])));
    assert!(view.shown_diff(&target_b).is_some());
    assert_eq!(view.shown_diff(&target(Group::Staged, "a.rs")), None);
}

#[test]
fn labels() {
    let mut status = GitStatus::default();
    assert_eq!(tracking(&status), "No upstream");
    status.upstream = Some("origin/main".into());
    assert_eq!(tracking(&status), "Tracking origin/main · up to date");
    status.ahead = 1;
    assert_eq!(tracking(&status), "Tracking origin/main · 1 commit ahead");
    status.behind = 2;
    assert_eq!(
        tracking(&status),
        "Tracking origin/main · 1 ahead, 2 behind"
    );
    status.ahead = 0;
    assert_eq!(tracking(&status), "Tracking origin/main · 2 commits behind");
    let mut binary = file("x", ChangeKind::Modified);
    assert_eq!(counts(&binary), Some((1, 0)));
    binary.added = None;
    assert_eq!(counts(&binary), None);
}

fn commit(hash: &str) -> Commit {
    Commit {
        hash: hash.into(),
        parents: vec![],
        author_name: "Ann".into(),
        author_email: "ann@example.com".into(),
        authored_at: 0,
        summary: format!("commit {hash}"),
        body: String::new(),
    }
}

fn detail(hash: &str, paths: &[&str]) -> CommitDetail {
    CommitDetail {
        commit: commit(hash),
        files: paths
            .iter()
            .map(|path| file(path, ChangeKind::Modified))
            .collect(),
    }
}

fn commit_target(hash: &str, path: &str) -> Target {
    Target {
        group: Group::Commit,
        path: path.into(),
        commit: Some(hash.into()),
    }
}

/// [`loaded`] switched to the History, with commits `b` (x, y) and `a` (z) listed and
/// `b`'s first file selected.
fn history() -> GitView {
    let mut view = loaded();
    let requests = view.key(Key::ToggleHistory);
    assert!(requests.log);
    assert_eq!(view.section(), Section::History);
    assert_eq!(view.selected, None);
    let params = view.begin_log().unwrap();
    assert_eq!(params.limit, 50);
    let requests = view.logged(Ok(vec![commit("b"), commit("a")]));
    assert_eq!(requests.show.as_deref(), Some("b"));
    let requests = view.commit_shown("b", Ok(detail("b", &["x", "y"])));
    assert_eq!(requests.diff, Some(commit_target("b", "x")));
    view
}

fn history_labels(view: &GitView) -> Vec<String> {
    view.list_rows()
        .iter()
        .map(|row| match row {
            ListRow::Commit {
                commit, expanded, ..
            } => format!("{} {expanded}", commit.hash),
            ListRow::File(target, _) => format!("  {}", target.path),
            ListRow::Note(note) => format!("  {note}"),
            ListRow::Header(group, _) => group.title().to_owned(),
            ListRow::Hint(hint) => (*hint).to_owned(),
        })
        .collect()
}

#[test]
fn the_history_lists_commits_with_the_selected_ones_files() {
    let mut view = history();
    assert_eq!(history_labels(&view), ["b true", "  x", "  y", "a false"]);
    assert!(!view.commit_box_shown());
    // The commit's diff asks for that commit's file; nothing stages it.
    let params = commit_target("b", "x").params(view.project_id, None, None);
    assert_eq!(params.commit.as_deref(), Some("b"));
    assert_eq!(view.stage_key(Stage::Stage), None);
    assert_eq!(view.discard(&commit_target("b", "x")), None);
    // Another commit: loading, then its first file.
    assert_eq!(view.select_commit("a".into()).as_deref(), Some("a"));
    assert_eq!(view.selected, None);
    assert_eq!(history_labels(&view), ["b false", "a true", "  Loading…"]);
    // A late answer for `b` is dropped.
    assert_eq!(
        view.commit_shown("b", Ok(detail("b", &["x"]))),
        Requests::default()
    );
    let requests = view.commit_shown("a", Ok(detail("a", &[])));
    assert_eq!(requests, Requests::default());
    assert_eq!(
        history_labels(&view),
        ["b false", "a true", "  No files changed."]
    );
}

/// The files listed under a commit carry its lanes on; the working changes have no graph.
#[test]
fn rows_under_a_commit_carry_its_lanes_on() {
    let mut view = history();
    // `history()` lists b (expanded, files x and y) then a. Give b a second parent.
    let mut b = commit("b");
    b.parents = vec!["a".into(), "side".into()];
    view.logged(Ok(vec![b, commit("a")]));
    assert_eq!(view.graph_lanes(), 2);
    let rows = view.list_rows();
    let commit_at = rows
        .iter()
        .position(|row| matches!(row, ListRow::Commit { commit, .. } if commit.hash == "b"))
        .unwrap();
    assert!(matches!(graph_cell(&rows, commit_at), Some(GraphCell::Commit(row)) if row.lane == 0));
    assert_eq!(
        graph_cell(&rows, commit_at + 1),
        Some(GraphCell::Through(vec![0, 1]))
    );
    assert!(
        (0..commit_at).all(|index| graph_cell(&rows, index).is_none()),
        "rows above the History have no graph"
    );
}

#[test]
fn moving_past_a_commits_files_moves_to_the_next_commit() {
    let mut view = history();
    assert_eq!(view.key(Key::Up), Requests::default(), "already first");
    assert_eq!(view.key(Key::Down).diff, Some(commit_target("b", "y")));
    let requests = view.key(Key::Down);
    assert_eq!(requests.show.as_deref(), Some("a"));
    assert_eq!(view.selected, None);
    view.commit_shown("a", Ok(detail("a", &["z"])));
    assert_eq!(view.key(Key::Down), Requests::default(), "last commit");
    assert_eq!(view.key(Key::Up).show.as_deref(), Some("b"));
}

#[test]
fn the_history_refreshes_its_log_and_goes_back_to_the_changes_it_left() {
    let mut view = history();
    assert_eq!(
        view.apply(&Event::GitChanged(GitChangedEvent {
            project_id: view.project_id,
            worktree_task_id: None,
        })),
        Requests {
            status: true,
            log: true,
            ..Requests::default()
        }
    );
    // The status only feeds the header: the selection stays.
    view.begin_status();
    assert_eq!(view.status_loaded(Ok(working())), Requests::default());
    assert_eq!(view.selected, Some(commit_target("b", "x")));
    // A new commit on top: the selection stays.
    view.begin_log();
    let requests = view.logged(Ok(vec![commit("c"), commit("b"), commit("a")]));
    assert_eq!(requests, Requests::default());
    assert_eq!(view.selected, Some(commit_target("b", "x")));
    // `b` back to Working changes: the kept status lists at once, its refresh selects.
    let requests = view.key(Key::ToggleMode);
    assert_eq!(view.section(), Section::Working);
    assert!(requests.status);
    assert!(matches!(view.status, Status::Loaded(_)));
    view.begin_status();
    let requests = view.status_loaded(Ok(working()));
    assert_eq!(requests.diff, Some(target(Group::Staged, "a.rs")));
    // From Against base, History and back return to Against base.
    view.show(Section::Base);
    view.toggle_history();
    assert_eq!(view.begin_log().map(|params| params.path), Some(None));
    view.toggle_history();
    assert_eq!(view.section(), Section::Base);
    // A late log answer after leaving is dropped.
    assert_eq!(view.logged(Ok(vec![commit("a")])), Requests::default());
    // Left and re-entered while a log is in flight: no second log until it answers, then
    // one more (the answer may predate re-entering).
    view.toggle_history();
    assert!(view.begin_log().is_some());
    view.toggle_history();
    view.toggle_history();
    assert_eq!(view.begin_log(), None);
    let requests = view.logged(Ok(vec![commit("a")]));
    assert!(requests.log);
    assert!(view.begin_log().is_some());
}

#[test]
fn a_status_refresh_in_the_history_keeps_a_failed_logs_error() {
    let mut view = history();
    view.begin_log();
    view.logged(Err("index.lock".into()));
    assert_eq!(view.refresh_error.as_deref(), Some("index.lock"));
    view.begin_status();
    view.status_loaded(Ok(working()));
    assert_eq!(view.refresh_error.as_deref(), Some("index.lock"));
    view.begin_log();
    view.logged(Ok(vec![commit("b")]));
    assert_eq!(view.refresh_error, None);
}

fn text_line(kind: DiffLineKind, old: Option<u32>, new: Option<u32>, text: &str) -> DiffLine {
    DiffLine {
        text: text.into(),
        ..line(kind, old, new)
    }
}

#[test]
fn diff_comments_quote_the_picked_new_side_lines() {
    let mut first = hunk(1, 2, 1, 2);
    first.lines = vec![
        text_line(DiffLineKind::Context, Some(1), Some(1), "  fn a() {"),
        text_line(DiffLineKind::Del, Some(2), None, "old();"),
        text_line(DiffLineKind::Add, None, Some(2), "new();  "),
    ];
    let mut second = hunk(9, 1, 9, 1);
    second.lines = vec![text_line(DiffLineKind::Add, None, Some(9), "end();")];
    let shown = laid_out(vec![first, second]);
    let pick = |start, end| Selection { start, end };
    assert_eq!(
        shown.quote(pick(1, 2)).as_deref(),
        Some("fn a() {\nnew();"),
        "removed lines are not new-side lines"
    );
    assert_eq!(
        shown.quote(pick(2, 9)).as_deref(),
        Some("new();  \nend();"),
        "only shown lines"
    );
    assert_eq!(shown.quote(pick(4, 8)), None);
    assert_eq!(
        shown.new_side_text(pick(1, 2)).as_deref(),
        Some("  fn a() {\nnew();  "),
        "sent selections keep the indent"
    );

    let mut view = loaded();
    assert_eq!(view.begin_comments().as_deref(), Some("a.rs"));
    assert_eq!(view.begin_comments(), None, "same file");
    view.comments.listed("a.rs", Ok(vec![]));
    assert!(
        !view.comments_shown(),
        "nothing to show: the diff keeps its width"
    );
    view.comments.pick(2, 2, false);
    assert!(
        !view.comments_shown(),
        "picked lines get the inline selection bar, not the margin"
    );
    assert!(!view.start_comment(), "diff still loading");
    assert!(view.begin_diff());
    view.diff_loaded(&target(Group::Staged, "a.rs"), Ok(shown));
    assert!(view.start_comment());
    let draft = view.comments.draft.as_ref().unwrap();
    assert_eq!(draft.quote, "new();");
    assert_eq!(view.comments.context, CommentContext::Diff);

    // The same file's other diff keeps its threads but not the picked lines.
    view.comments.draft = None;
    view.comments.pick(2, 2, false);
    view.select(target(Group::Unstaged, "a.rs"));
    assert_eq!(view.begin_comments(), None, "same file: listed already");
    assert_eq!(view.comments.selection, None);

    // In the History no line is commented on.
    view.comments.pick(2, 2, false);
    view.toggle_history();
    assert!(!view.comments_shown() && !view.start_comment());
    assert_eq!(view.begin_comments(), None, "not listed in the History");
    view.toggle_history();
    assert_eq!(
        view.comments.selection,
        Some(Selection { start: 2, end: 2 })
    );

    // Another file starts its comments over.
    view.select(target(Group::Unstaged, "b.rs"));
    assert_eq!(view.begin_comments().as_deref(), Some("b.rs"));
    assert_eq!(view.comments.selection, None);
    assert!(!view.comments_shown());
}

fn session(id: u128, task: Option<u128>, started_at: i64, state: SessionState) -> Session {
    use mkrew_protocol::{SessionKind, StateSource};
    Session {
        id: Ulid(id),
        task_id: task.map(Ulid),
        project_id: Ulid(1),
        agent_profile: "fake".into(),
        kind: SessionKind::Agent,
        title: None,
        terminal_title: None,
        color: None,
        native_session_id: None,
        state,
        state_source: StateSource::Screen,
        state_changed_at: 0,
        exit_code: None,
        cols: 80,
        rows: 24,
        started_at,
        ended_at: None,
        pinned: false,
        position: 0.0,
        plan_path: None,
    }
}

#[test]
fn send_to_agent_offers_the_tasks_newest_session_first() {
    let (idle, exited) = (SessionState::Idle, SessionState::Exited);
    let mut sessions = vec![
        session(1, Some(7), 1, idle),
        session(2, Some(7), 5, idle),
        session(3, None, 9, idle),
    ];
    let mut view = GitView::new(Ulid(1), None);
    assert_eq!(view.since_session(&sessions), None, "not a task's changes");
    view.show_since(Since::task(Ulid(7), "Fix it", "abc".into()));
    assert_eq!(view.since_session(&sessions), Some(Ulid(2)));
    sessions[1].state = exited;
    assert_eq!(view.since_session(&sessions), Some(Ulid(1)), "live first");
    sessions[0].state = exited;
    assert_eq!(view.since_session(&sessions), Some(Ulid(2)), "else newest");
    view.toggle_history();
    assert_eq!(view.since_session(&sessions), None, "in the History");
    // A turn's changes: its own session, even exited.
    view.show_since(Since::turn(&sessions[2], &sessions, 3, "tree".into()));
    assert_eq!(view.since_session(&sessions), Some(Ulid(3)));
}

#[test]
fn a_turns_changes_list_and_diff_since_its_start_tree() {
    let sessions = [
        session(2, None, 5, SessionState::Idle),
        session(1, None, 1, SessionState::Idle),
    ];
    let turn = Since::turn(&sessions[0], &sessions, 3, "tree3".into());
    assert_eq!(turn.label, "Since turn 3 of fake #2");
    let mut view = GitView::new(Ulid(1), None);
    view.show_since(turn);
    assert_eq!(view.section(), Section::Since);
    let params = view.begin_status().unwrap();
    assert_eq!(
        (params.mode, params.since.as_deref()),
        (GitMode::Base, Some("tree3"))
    );
    let requests = view.status_loaded(Ok(since(&["a.rs"])));
    let diff = requests.diff.unwrap().params(view.project_id, None, None);
    assert_eq!(
        (diff.mode, diff.since.as_deref()),
        (GitMode::Base, Some("tree3"))
    );
    assert_eq!(
        Since::task(Ulid(7), "Fix it", "0123456789abcdef".into()).label,
        "Since 0123456 · task: Fix it"
    );
}

#[test]
fn sessions_are_numbered_only_among_namesakes_of_the_workspace() {
    let idle = SessionState::Idle;
    let mut other = session(4, None, 0, idle);
    other.project_id = Ulid(9);
    let mut titled = session(5, None, 2, idle);
    titled.title = Some("review".into());
    let sessions = [
        session(3, None, 7, idle),
        session(2, None, 3, idle),
        other,
        titled,
    ];
    let label = |i: usize| session_label(&sessions[i], &sessions);
    assert_eq!(label(0), "fake #2");
    assert_eq!(label(1), "fake #1");
    assert_eq!(label(2), "fake", "alone in its workspace");
    assert_eq!(label(3), "review");
}

#[test]
fn changed_by_keeps_only_the_sessions_files() {
    let idle = SessionState::Idle;
    let sessions = [session(1, None, 1, idle), session(2, None, 2, idle)];
    let mut view = loaded();
    let (params, late) = view.begin_session_files(CHANGED_BY_WINDOW_MS + 5);
    assert_eq!(params.since_ms, 5, "the last 24 h");
    let (_, serial) = view.begin_session_files(0);
    let stale = SessionFiles::from([(Ulid(9), vec!["a.rs".into()])]);
    view.session_files_loaded(late, stale);
    assert!(
        view.changers(&sessions).is_empty(),
        "a late answer is dropped"
    );
    let loaded = view.session_files_loaded(
        serial,
        SessionFiles::from([
            (Ulid(2), vec!["b.rs".into(), "gone.rs".into()]),
            (Ulid(1), vec!["new.txt".into()]),
            (Ulid(3), vec!["a.rs".into()]),
            (Ulid(4), vec![]),
            (Ulid(5), vec!["a.rs".into()]),
        ]),
    );
    assert_eq!(loaded, Requests::default(), "All still lists every file");
    let labels: Vec<String> = view
        .changers(&sessions)
        .into_iter()
        .map(|changer| changer.label)
        .collect();
    let line = format!("Changed by: All | {}", labels.join(" | "));
    insta::assert_snapshot!(line);

    // The selected a.rs is filtered out: the first kept file is selected.
    let requests = view.filter_changed_by(Some(Ulid(2)));
    assert_eq!(requests.diff, Some(target(Group::Unstaged, "b.rs")));
    let paths: Vec<String> = view.files().into_iter().map(|(t, _)| t.path).collect();
    assert_eq!(paths, ["b.rs"]);
    assert_eq!(view.changed_by_label(&sessions).as_deref(), Some("fake #2"));
    assert_eq!(view.filter_changed_by(Some(Ulid(2))), Requests::default());

    // Its files are gone from `session.files`: back to All.
    let (_, serial) = view.begin_session_files(0);
    view.session_files_loaded(
        serial,
        SessionFiles::from([(Ulid(1), vec!["new.txt".into()])]),
    );
    assert_eq!(view.changed_by.session, None);
    assert_eq!(view.files().len(), 4);

    // Nothing kept: nothing selected.
    view.filter_changed_by(Some(Ulid(1)));
    let (_, serial) = view.begin_session_files(0);
    view.session_files_loaded(serial, SessionFiles::from([(Ulid(1), vec!["x.rs".into()])]));
    assert!(view.files().is_empty());
    assert_eq!(view.selected, None);

    // The History lists commits: no filter there.
    view.toggle_history();
    assert!(view.changers(&sessions).is_empty());
}

#[test]
fn keys_are_unmodified_arrows_and_j_k() {
    let parse = |keys: &str| key(&Keystroke::parse(keys).unwrap());
    assert_eq!(parse("up"), Some(Key::Up));
    assert_eq!(parse("k"), Some(Key::Up));
    assert_eq!(parse("down"), Some(Key::Down));
    assert_eq!(parse("j"), Some(Key::Down));
    // Keymap commands now (they would act twice otherwise).
    for keys in ["n", "p", "b", "s", "u"] {
        assert_eq!(parse(keys), None, "{keys}");
    }
    assert_eq!(parse("ctrl-k"), None);
    assert_eq!(parse("x"), None);
}

fn viewed_file(path: &str, staged: bool) -> ViewedFile {
    ViewedFile {
        path: path.into(),
        staged,
    }
}

/// A [`loaded`] view whose daemon answered that only the staged `a.rs` is viewed.
fn with_viewed() -> GitView {
    let mut view = loaded();
    let (params, serial) = view.begin_viewed();
    view.viewed_loaded(
        serial,
        (params.mode, params.since),
        vec![viewed_file("a.rs", true)],
    );
    view
}

#[test]
fn the_viewed_counter_counts_every_listed_file_whatever_the_filters_hide() {
    let view = loaded();
    // a.rs staged + a.rs unstaged + b.rs + new.txt.
    assert_eq!(view.viewed_counter(), Some((0, 4)));
    let mut view = with_viewed();
    assert_eq!(view.viewed_counter(), Some((1, 4)));
    assert!(view.viewed.contains(Group::Staged, "a.rs"));
    assert!(!view.viewed.contains(Group::Unstaged, "a.rs"));
    // The file filter narrows the list, not the counter.
    view.filter.insert("new");
    assert_eq!(view.files().len(), 1);
    assert_eq!(view.viewed_counter(), Some((1, 4)));
    // No files: no counter; the History never has one.
    let mut empty = GitView::new(Ulid::new(), None);
    empty.begin_status();
    empty.status_loaded(Ok(GitStatus::default()));
    assert_eq!(empty.viewed_counter(), None);
    let _ = view.show(Section::History);
    assert_eq!(view.viewed_counter(), None);
    assert!(!view.wants_viewed());
}

#[test]
fn an_answer_for_older_asks_or_other_changes_is_dropped() {
    let mut view = loaded();
    assert!(view.wants_viewed());
    let (params, first) = view.begin_viewed();
    assert_eq!(
        (params.mode, params.since.clone()),
        (GitMode::Working, None)
    );
    let (_, second) = view.begin_viewed();
    // The first ask answers late: dropped. The second counts.
    view.viewed_loaded(
        first,
        (GitMode::Working, None),
        vec![viewed_file("b.rs", false)],
    );
    assert!(!view.viewed.contains(Group::Unstaged, "b.rs"));
    // An answer for Against base while Working is listed: dropped.
    view.viewed_loaded(
        second,
        (GitMode::Base, None),
        vec![viewed_file("b.rs", false)],
    );
    assert!(!view.viewed.contains(Group::Unstaged, "b.rs"));
    view.viewed_loaded(
        second,
        (GitMode::Working, None),
        vec![viewed_file("b.rs", false)],
    );
    assert!(view.viewed.contains(Group::Unstaged, "b.rs"));
    // Another list starts without the marks of the first.
    view.show(Section::Base);
    assert!(!view.viewed.contains(Group::Unstaged, "b.rs"));
    assert!(!view.wants_viewed(), "its status is loading");
}

#[test]
fn toggling_marks_at_once_and_describes_the_diff_to_send() {
    let mut view = loaded();
    let staged = target(Group::Staged, "a.rs");
    let params = view.toggle_viewed(&staged).unwrap();
    assert!(params.viewed);
    assert_eq!(params.diff, staged.params(view.project_id, None, None));
    assert!(params.diff.staged);
    assert!(view.viewed.contains(Group::Staged, "a.rs"));
    assert!(!view.viewed.contains(Group::Unstaged, "a.rs"));
    // Again: cleared.
    assert!(!view.toggle_viewed(&staged).unwrap().viewed);
    assert!(!view.viewed.contains(Group::Staged, "a.rs"));
    // A toggle makes an older in-flight answer stale.
    let (_, serial) = view.begin_viewed();
    view.toggle_viewed(&staged);
    view.viewed_loaded(serial, (GitMode::Working, None), vec![]);
    assert!(view.viewed.contains(Group::Staged, "a.rs"));
    // A History file has nothing to mark.
    assert_eq!(view.toggle_viewed(&commit_target("abc", "x")), None);
}

#[test]
fn a_viewed_selected_file_collapses_until_show_diff() {
    let mut view = with_viewed();
    // a.rs staged is selected by `loaded()` and viewed.
    assert!(view.diff_collapsed());
    view.unfold();
    assert!(!view.diff_collapsed());
    // Marking it again, or selecting another file, starts collapsed / open again.
    let staged = target(Group::Staged, "a.rs");
    view.toggle_viewed(&staged);
    assert!(!view.diff_collapsed(), "not viewed any more");
    view.toggle_viewed(&staged);
    assert!(view.diff_collapsed());
    view.unfold();
    view.select(target(Group::Unstaged, "b.rs"));
    view.select(staged);
    assert!(view.diff_collapsed(), "selecting resets Show diff");
}

#[test]
fn a_collapsed_diff_ignores_hunk_keys_and_stages_the_whole_file() {
    let mut view = with_viewed();
    let a = target(Group::Staged, "a.rs");
    view.begin_diff();
    view.diff_loaded(&a, Ok(laid_out(vec![hunk(1, 1, 1, 1), hunk(20, 1, 20, 1)])));
    assert!(view.diff_collapsed());
    view.key(Key::NextHunk);
    view.key(Key::PreviousHunk);
    assert_eq!(view.hunk, None);
    // A hunk chosen before the collapse is not staged blind.
    view.hunk = Some(1);
    let file = view.stage_key(Stage::Unstage).unwrap();
    assert_eq!(paths(&file), (Stage::Unstage, vec!["a.rs".into()], None));
    // Show diff: hunk keys and hunk staging work again.
    view.unfold();
    let hunk = view.stage_key(Stage::Unstage).unwrap();
    assert_eq!(paths(&hunk), (Stage::Unstage, vec![], Some(20)));
    view.hunk = None;
    view.key(Key::NextHunk);
    assert_eq!(view.hunk, Some(0));
    // Marking the selected file drops its current hunk (no stale "Hunk i of N" after Show diff).
    view.hunk = Some(1);
    view.toggle_viewed(&a);
    view.toggle_viewed(&a);
    assert_eq!(view.hunk, None);
}
