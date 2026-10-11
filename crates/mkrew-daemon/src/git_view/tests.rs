use std::process::Command;

use mkrew_protocol::DiffLineKind;
use ulid::Ulid;

use super::*;
use crate::git::GitCli;

/// `git` in `dir` with a throwaway identity; returns trimmed stdout.
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// A repo on `main` with `app/` (the project folder) and a file outside it.
fn repo() -> (tempfile::TempDir, PathBuf) {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    git(root, &["init", "-q", "-b", "main"]);
    // Byte-exact diffs whatever the machine's global autocrlf says.
    git(root, &["config", "core.autocrlf", "false"]);
    let dir = root.join("app");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(root.join("outside.txt"), "a\n").unwrap();
    std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
    std::fs::write(dir.join("b.txt"), "b\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "one"]);
    (repo, dir)
}

fn working(cache: &GitCache, dir: &Path) -> GitStatus {
    status(&GitCli, cache, dir, None, GitMode::Working, None, &unused).unwrap()
}

fn base(cache: &GitCache, dir: &Path, configured: Option<&str>) -> GitStatus {
    status(
        &GitCli,
        cache,
        dir,
        configured,
        GitMode::Base,
        None,
        &unused,
    )
    .unwrap()
}

/// The working-state snapshot of calls that compare with no tree.
fn unused() -> io::Result<String> {
    unreachable!("no tree to compare with")
}

/// A snapshot of the working state of the repo containing `dir`, like a turn's.
fn snapshot(dir: &Path) -> io::Result<String> {
    GitCli.snapshot(dir, &GitCli.index_path(dir)?)
}

fn summary(files: &[ChangedFile]) -> Vec<String> {
    files
        .iter()
        .map(|f| {
            let counts = match (f.added, f.removed) {
                (Some(a), Some(r)) => format!(" +{a} -{r}"),
                _ => String::new(),
            };
            format!("{:?} {}{counts}", f.kind, f.path)
        })
        .collect()
}

#[test]
fn working_status_groups_counted_changes_of_the_folder_only() {
    let (repo, dir) = repo();
    let root = repo.path();
    std::fs::write(dir.join("a.txt"), "one\n2\nthree\n").unwrap();
    git(&dir, &["add", "a.txt"]);
    std::fs::write(dir.join("a.txt"), "one\n2\nthree\nfour\n").unwrap();
    std::fs::remove_file(dir.join("b.txt")).unwrap();
    std::fs::create_dir(dir.join("new")).unwrap();
    std::fs::write(dir.join("new").join("c d.txt"), "c\n").unwrap();
    std::fs::write(root.join("outside.txt"), "b\n").unwrap();
    std::fs::write(root.join("outside-new.txt"), "b\n").unwrap();

    let cache = GitCache::default();
    let status = working(&cache, &dir);
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert_eq!(status.head, Some(git(root, &["rev-parse", "HEAD"])));
    assert_eq!((status.upstream, status.ahead, status.behind), (None, 0, 0));
    assert_eq!(status.base_ref.as_deref(), Some("main"));
    assert!(status.base_auto);
    assert_eq!(status.merge_base, None);
    assert_eq!(summary(&status.staged), ["Modified a.txt +2 -1"]);
    assert_eq!(
        summary(&status.unstaged),
        ["Modified a.txt +1 -0", "Deleted b.txt +0 -1"]
    );
    assert_eq!(summary(&status.untracked), ["Untracked new/c d.txt"]);
    assert!(status.against_base.is_empty());

    // Not a repo; a folder that becomes one is found on the next call.
    let plain = tempfile::tempdir().unwrap();
    let error = super::status(
        &GitCli,
        &cache,
        plain.path(),
        None,
        GitMode::Working,
        None,
        &unused,
    );
    assert!(matches!(error, Err(ViewError::NotARepo)), "{error:?}");
    git(plain.path(), &["init", "-q", "-b", "main"]);
    let fresh = working(&cache, plain.path());
    assert_eq!((fresh.branch.as_deref(), fresh.head), (Some("main"), None));
    assert_eq!(fresh.base_ref, None);
}

#[test]
fn base_mode_diffs_from_the_cached_merge_base() {
    let (repo, dir) = repo();
    let root = repo.path();
    let fork = git(root, &["rev-parse", "HEAD"]);
    git(root, &["checkout", "-q", "-b", "feature"]);
    std::fs::write(dir.join("b.txt"), "b\nfeature\n").unwrap();
    git(root, &["commit", "-q", "-am", "feature"]);
    git(root, &["checkout", "-q", "main"]);
    std::fs::write(dir.join("a.txt"), "main moved on\n").unwrap();
    git(root, &["commit", "-q", "-am", "main"]);
    git(root, &["checkout", "-q", "feature"]);
    std::fs::write(dir.join("new.txt"), "n\n").unwrap();

    let cache = GitCache::default();
    let status = base(&cache, &dir, None);
    assert_eq!(status.base_ref.as_deref(), Some("main"));
    assert_eq!(status.merge_base.as_deref(), Some(fork.as_str()));
    // Only this branch's changes (not main's a.txt), plus untracked files.
    assert_eq!(
        summary(&status.against_base),
        ["Modified b.txt +1 -0", "Untracked new.txt"]
    );
    assert!(status.staged.is_empty() && status.unstaged.is_empty());
    assert!(status.untracked.is_empty());

    // Cached until HEAD or the base moves: a planted answer is returned as is…
    let head = status.head.clone().unwrap();
    lock(&cache.merge_bases).get_mut(&dir).unwrap().merge_base = Some(head.clone());
    assert_eq!(base(&cache, &dir, None).merge_base, Some(head));
    // …and recomputed once HEAD moves.
    git(root, &["commit", "-q", "--allow-empty", "-m", "more"]);
    assert_eq!(base(&cache, &dir, None).merge_base, Some(fork.clone()));

    // `origin/HEAD` wins over `main`; a configured base wins over both.
    git(root, &["update-ref", "refs/remotes/origin/trunk", &fork]);
    git(
        root,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/trunk",
        ],
    );
    let remote = base(&cache, &dir, None);
    assert_eq!(remote.base_ref.as_deref(), Some("origin/trunk"));
    assert_eq!(remote.merge_base, Some(fork));
    let configured = base(&cache, &dir, Some("feature~1"));
    assert_eq!(configured.base_ref.as_deref(), Some("feature~1"));
    assert!(!configured.base_auto);
    assert_eq!(summary(&configured.against_base), ["Untracked new.txt"]);
    // A base that names nothing: reported, nothing to compare.
    let missing = base(&cache, &dir, Some("develop"));
    assert_eq!(missing.base_ref.as_deref(), Some("develop"));
    assert_eq!((missing.merge_base, missing.against_base), (None, vec![]));
}

fn params(mode: GitMode, staged: bool, commit: Option<&str>) -> GitDiffParams {
    GitDiffParams {
        project_id: Ulid::new(),
        worktree_task_id: None,
        repo: None,
        path: String::new(),
        mode,
        staged,
        commit: commit.map(str::to_owned),
        since: None,
    }
}

fn lines(diff: &FileDiff) -> Vec<String> {
    let sign = |kind| match kind {
        DiffLineKind::Context => ' ',
        DiffLineKind::Add => '+',
        DiffLineKind::Del => '-',
    };
    diff.hunks
        .iter()
        .flat_map(|h| &h.lines)
        .map(|l| format!("{}{}", sign(l.kind), l.text))
        .collect()
}

#[test]
fn diffs_of_each_side_are_cached_until_the_file_or_index_changes() {
    let (repo, dir) = repo();
    let root = repo.path();
    let first = git(root, &["rev-parse", "HEAD"]);
    std::fs::write(dir.join("a.txt"), "one\nTWO\n").unwrap();
    git(&dir, &["add", "a.txt"]);
    std::fs::write(dir.join("a.txt"), "one\nTWO\nthree\n").unwrap();
    std::fs::write(dir.join("new.txt"), "n1\nn2\n").unwrap();

    let cache = GitCache::default();
    let diff = |path: &str, p: &GitDiffParams| {
        super::diff(&GitCli, &cache, &dir, None, p, path, &unused).unwrap()
    };
    let unstaged = diff("a.txt", &params(GitMode::Working, false, None));
    assert_eq!(lines(&unstaged), [" one", " TWO", "+three"]);
    assert_eq!((unstaged.added, unstaged.removed), (1, 0));
    let staged = diff("a.txt", &params(GitMode::Working, true, None));
    assert_eq!(lines(&staged), [" one", "-two", "+TWO"]);
    let untracked = diff("new.txt", &params(GitMode::Working, false, None));
    assert_eq!(untracked.path, "new.txt");
    assert_eq!(lines(&untracked), ["+n1", "+n2"]);
    assert_eq!(lock(&cache.diffs).len(), 3);
    // Same request again: served from the cache.
    assert_eq!(diff("a.txt", &params(GitMode::Working, true, None)), staged);
    assert_eq!(lock(&cache.diffs).len(), 3);

    // Staging changes the index: both sides are recomputed.
    git(&dir, &["add", "a.txt"]);
    assert_eq!(
        diff("a.txt", &params(GitMode::Working, false, None)).hunks,
        []
    );
    let staged = diff("a.txt", &params(GitMode::Working, true, None));
    assert_eq!(lines(&staged), [" one", "-two", "+TWO", "+three"]);
    let against = diff("a.txt", &params(GitMode::Base, false, None));
    assert_eq!(against, staged);

    // A commit's own diff; an unchanged file has no hunks.
    git(root, &["commit", "-q", "-m", "two"]);
    let second = git(root, &["rev-parse", "HEAD"]);
    let commit = diff("a.txt", &params(GitMode::Working, false, Some(&second)));
    assert_eq!(lines(&commit), lines(&staged));
    assert_eq!(
        diff("b.txt", &params(GitMode::Working, false, Some(&second))),
        FileDiff {
            path: "b.txt".into(),
            ..FileDiff::default()
        }
    );
    let root_commit = diff("b.txt", &params(GitMode::Working, false, Some(&first)));
    assert_eq!(lines(&root_commit), ["+b"]);
    // Names are resolved before the cache: `HEAD` follows new commits.
    let by_name = diff("a.txt", &params(GitMode::Working, false, Some("HEAD")));
    assert_eq!(by_name, commit);
    std::fs::write(dir.join("a.txt"), "1\n").unwrap();
    git(root, &["commit", "-q", "-am", "three"]);
    let moved = diff("a.txt", &params(GitMode::Working, false, Some("HEAD")));
    assert_eq!(lines(&moved), ["-one", "-TWO", "-three", "+1"]);
    for bad in ["HEAD~1..HEAD", "nope"] {
        let error = super::diff(
            &GitCli,
            &cache,
            &dir,
            None,
            &params(GitMode::Working, false, Some(bad)),
            "a.txt",
            &unused,
        );
        assert!(
            matches!(error, Err(ViewError::UnknownCommit(_))),
            "{error:?}"
        );
    }
}

#[test]
fn since_a_commit_lists_and_diffs_everything_up_to_the_working_tree() {
    let (repo, dir) = repo();
    let root = repo.path();
    let checkpoint = git(root, &["rev-parse", "HEAD"]);
    // A task's run: one commit, then uncommitted and untracked changes.
    std::fs::write(dir.join("a.txt"), "one\nTWO\n").unwrap();
    git(root, &["commit", "-q", "-am", "task"]);
    std::fs::write(dir.join("b.txt"), "b\nmore\n").unwrap();
    std::fs::write(dir.join("new.txt"), "n\n").unwrap();

    let cache = GitCache::default();
    let since = |name: &str| {
        status(
            &GitCli,
            &cache,
            &dir,
            None,
            GitMode::Base,
            Some(name),
            &unused,
        )
    };
    let status = since(&checkpoint).unwrap();
    assert_eq!(status.merge_base.as_deref(), Some(checkpoint.as_str()));
    assert_eq!(
        summary(&status.against_base),
        [
            "Modified a.txt +1 -1",
            "Modified b.txt +1 -0",
            "Untracked new.txt"
        ]
    );
    // Names resolve to their commit; unknown ones are refused.
    assert_eq!(
        since("HEAD~1").unwrap().merge_base,
        Some(checkpoint.clone())
    );
    let error = since("nope");
    assert!(
        matches!(error, Err(ViewError::UnknownCommit(_))),
        "{error:?}"
    );
    // Working mode ignores it.
    let working = super::status(
        &GitCli,
        &cache,
        &dir,
        None,
        GitMode::Working,
        Some("nope"),
        &unused,
    );
    assert_eq!(working.unwrap().merge_base, None);

    let diff = |path: &str, since: &str| {
        let params = GitDiffParams {
            worktree_task_id: None,
            repo: None,
            since: Some(since.to_owned()),
            ..params(GitMode::Base, false, None)
        };
        super::diff(&GitCli, &cache, &dir, None, &params, path, &unused)
    };
    assert_eq!(
        lines(&diff("a.txt", &checkpoint).unwrap()),
        [" one", "-two", "+TWO"]
    );
    assert_eq!(lines(&diff("b.txt", &checkpoint).unwrap()), [" b", "+more"]);
    assert_eq!(lines(&diff("new.txt", &checkpoint).unwrap()), ["+n"]);
    // Since HEAD: only the uncommitted part.
    assert_eq!(diff("a.txt", "HEAD").unwrap().hunks, []);
    let error = diff("a.txt", "nope");
    assert!(
        matches!(error, Err(ViewError::UnknownCommit(_))),
        "{error:?}"
    );
}

/// SPEC §20C.3: `since` = a turn's start tree compares it with the working state, so
/// files untracked before the turn and unchanged since are not listed.
#[test]
fn since_a_tree_lists_and_diffs_the_changes_after_the_snapshot() {
    let (repo, dir) = repo();
    let root = repo.path();
    std::fs::write(dir.join("old.txt"), "untracked\n").unwrap();
    std::fs::write(dir.join("gone.txt"), "g\n").unwrap();
    let tree = snapshot(&dir).unwrap();
    // The turn: an edit, a commit, a new file, a deleted untracked file, one outside.
    std::fs::write(dir.join("a.txt"), "one\nTWO\n").unwrap();
    git(root, &["commit", "-q", "-am", "turn"]);
    std::fs::write(dir.join("new.txt"), "n\n").unwrap();
    std::fs::remove_file(dir.join("gone.txt")).unwrap();
    std::fs::write(root.join("outside.txt"), "changed\n").unwrap();

    let cache = GitCache::default();
    let now = || snapshot(&dir);
    let status = status(
        &GitCli,
        &cache,
        &dir,
        None,
        GitMode::Base,
        Some(&tree),
        &now,
    );
    let status = status.unwrap();
    assert_eq!(status.merge_base.as_deref(), Some(tree.as_str()));
    assert_eq!(
        summary(&status.against_base),
        [
            "Modified a.txt +1 -1",
            "Deleted gone.txt +0 -1",
            "Added new.txt +1 -0"
        ]
    );
    let diff = |path: &str| {
        let params = GitDiffParams {
            worktree_task_id: None,
            repo: None,
            since: Some(tree.clone()),
            ..params(GitMode::Base, false, None)
        };
        super::diff(&GitCli, &cache, &dir, None, &params, path, &now).unwrap()
    };
    assert_eq!(lines(&diff("a.txt")), [" one", "-two", "+TWO"]);
    assert_eq!(lines(&diff("new.txt")), ["+n"]);
    assert_eq!(lines(&diff("gone.txt")), ["-g"]);
    assert_eq!(diff("old.txt").hunks, []);
    // A tree is no commit: `commit` still refuses it.
    let error = super::diff(
        &GitCli,
        &cache,
        &dir,
        None,
        &params(GitMode::Working, false, Some(&tree)),
        "a.txt",
        &unused,
    );
    assert!(
        matches!(error, Err(ViewError::UnknownCommit(_))),
        "{error:?}"
    );
}

#[test]
fn log_and_show_cover_the_folder_only() {
    let (repo, dir) = repo();
    let root = repo.path();
    let cache = GitCache::default();
    let first = git(root, &["rev-parse", "HEAD"]);
    std::fs::write(root.join("outside.txt"), "b\n").unwrap();
    git(root, &["commit", "-q", "-am", "outside only"]);
    let outside = git(root, &["rev-parse", "HEAD"]);
    std::fs::write(dir.join("a.txt"), "one\n").unwrap();
    std::fs::write(dir.join("c.txt"), "c\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "two\n\nthe body"]);
    let second = git(root, &["rev-parse", "HEAD"]);
    // Settings that change `git log`'s output are overridden.
    git(root, &["config", "log.showSignature", "true"]);
    git(root, &["config", "log.follow", "true"]);

    let commits = log(&GitCli, &cache, &dir, 50, None).unwrap();
    let hashes: Vec<&str> = commits.iter().map(|c| c.hash.as_str()).collect();
    assert_eq!(hashes, [second.as_str(), first.as_str()]);
    assert_eq!(
        (commits[0].summary.as_str(), commits[0].body.as_str()),
        ("two", "the body")
    );
    // `--parents`: the parent is the listed commit, not the hidden `outside only`.
    assert_eq!(commits[0].parents, std::slice::from_ref(&first));
    assert_eq!(
        (
            commits[0].author_name.as_str(),
            commits[0].author_email.as_str()
        ),
        ("t", "t@t")
    );
    assert!(commits[0].authored_at > 1_600_000_000_000);
    assert_eq!(commits[1].summary, "one");
    let only_c = log(&GitCli, &cache, &dir, 50, Some("c.txt")).unwrap();
    assert_eq!(only_c.len(), 1);
    assert_eq!(log(&GitCli, &cache, &dir, 1, None).unwrap().len(), 1);

    let shown = show(&GitCli, &cache, &dir, "HEAD").unwrap();
    // `show` is not path-filtered: it names the real parent.
    assert_eq!(shown.commit.parents, std::slice::from_ref(&outside));
    let mut listed = shown.commit.clone();
    listed.parents.clone_from(&commits[0].parents);
    assert_eq!(listed, commits[0]);
    assert_eq!(
        summary(&shown.files),
        ["Modified a.txt +0 -1", "Added c.txt +1 -0"]
    );
    let elsewhere = show(&GitCli, &cache, &dir, &outside).unwrap();
    assert_eq!(elsewhere.commit.summary, "outside only");
    assert!(elsewhere.files.is_empty());
    let error = show(&GitCli, &cache, &dir, "nope");
    assert!(
        matches!(error, Err(ViewError::UnknownCommit(_))),
        "{error:?}"
    );

    // Before the first commit: no history.
    let fresh = tempfile::tempdir().unwrap();
    git(fresh.path(), &["init", "-q", "-b", "main"]);
    assert_eq!(log(&GitCli, &cache, fresh.path(), 50, None).unwrap(), []);
}

/// Push goes to the upstream only, fast-forward only; fetch moves the remote-tracking ref.
/// The remote is a local bare repo: no network.
#[test]
fn push_and_fetch_follow_the_upstream_without_forcing() {
    let (repo, dir) = repo();
    let root = repo.path();
    let remotes = tempfile::tempdir().unwrap();
    let bare = remotes.path().join("remote.git");
    git(
        remotes.path(),
        &["init", "-q", "--bare", "-b", "main", "remote.git"],
    );
    git(root, &["remote", "add", "origin", bare.to_str().unwrap()]);

    // No upstream yet; detached HEAD has no branch at all.
    let error = push(&GitCli, &dir);
    assert!(
        matches!(&error, Err(ViewError::NoUpstream(b)) if b == "main"),
        "{error:?}"
    );
    git(root, &["push", "-q", "-u", "origin", "main"]);
    git(root, &["checkout", "-q", "--detach"]);
    assert!(matches!(push(&GitCli, &dir), Err(ViewError::NoBranch)));
    git(root, &["checkout", "-q", "main"]);

    git(root, &["commit", "-q", "--allow-empty", "-m", "local"]);
    push(&GitCli, &dir).unwrap();
    let head = git(root, &["rev-parse", "HEAD"]);
    assert_eq!(git(&bare, &["rev-parse", "main"]), head);

    // Someone else pushes; fetch brings it in, and a diverged push is refused.
    let other = remotes.path().join("other");
    git(
        remotes.path(),
        &["clone", "-q", bare.to_str().unwrap(), "other"],
    );
    git(&other, &["commit", "-q", "--allow-empty", "-m", "theirs"]);
    git(&other, &["push", "-q"]);
    let theirs = git(&other, &["rev-parse", "HEAD"]);
    GitCli.fetch(&dir).unwrap();
    assert_eq!(git(root, &["rev-parse", "origin/main"]), theirs);
    git(root, &["commit", "-q", "--allow-empty", "-m", "mine"]);
    let error = push(&GitCli, &dir);
    assert!(matches!(error, Err(ViewError::Git(_))), "{error:?}");
    assert_eq!(git(&bare, &["rev-parse", "main"]), theirs);
}

/// SPEC §15: a status refresh of a 10,000-file repo takes < 150 ms. Asserted in release
/// builds only; debug builds log the time.
#[test]
fn status_of_a_10000_file_repo_meets_the_budget() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "core.autocrlf", "false"]);
    // One `fast-import` pack instead of `git add`'s 10,000 loose objects (minutes on
    // Windows); a parallel checkout then writes the files and the index.
    let mut stream = String::from("commit refs/heads/main\ncommitter t <t@t> 0 +0000\n");
    stream.push_str("data 5\nfiles\n");
    for d in 0..100 {
        for f in 0..100 {
            let content = format!("{d} {f}\n");
            stream.push_str(&format!("M 100644 inline d{d:02}/f{f:02}.txt\n"));
            stream.push_str(&format!("data {}\n{content}\n", content.len()));
        }
    }
    let stream_file = root.join(".git").join("stream");
    std::fs::write(&stream_file, stream).unwrap();
    let imported = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["fast-import", "--quiet"])
        .stdin(std::fs::File::open(&stream_file).unwrap())
        .status()
        .unwrap();
    assert!(imported.success());
    git(root, &["-c", "checkout.workers=0", "reset", "-q", "--hard"]);
    for d in 0..10 {
        std::fs::write(root.join(format!("d{d:02}/f00.txt")), "changed\n").unwrap();
    }
    std::fs::write(root.join("new.txt"), "new\n").unwrap();
    // The user's own git keeps the index fresh (here: rehashes the files the checkout left
    // racily clean); the view itself never writes it.
    git(root, &["status", "-s"]);

    let cache = GitCache::default();
    let refresh = || {
        let started = std::time::Instant::now();
        let status = working(&cache, root);
        (started.elapsed(), status)
    };
    let (_, status) = refresh();
    assert_eq!((status.unstaged.len(), status.untracked.len()), (10, 1));
    let mut times: Vec<_> = (0..5).map(|_| refresh().0).collect();
    times.sort();
    let median = times[2];
    eprintln!("git.status on 10,000 files: median {median:?} of {times:?}");
    if !cfg!(debug_assertions) {
        assert!(median < std::time::Duration::from_millis(150), "{median:?}");
    }
}

#[test]
fn diffs_over_the_limit_keep_only_their_counts() {
    let line = "x".repeat(99);
    let header = "diff --git a/big b/big\n--- a/big\n+++ b/big\n";
    let small = file_diff(&format!("{header}@@ -0,0 +1,1 @@\n+{line}\n"), "big");
    assert_eq!(
        (small.added, small.too_large, small.hunks.len()),
        (1, false, 1)
    );
    let count = MAX_DIFF_BYTES / 100 + 1;
    let added = format!("+{line}\n").repeat(count);
    let big = file_diff(&format!("{header}@@ -0,0 +1,{count} @@\n{added}"), "big");
    assert!(big.too_large);
    let count = u32::try_from(count).unwrap();
    assert_eq!((big.added, big.removed, big.hunks.len()), (count, 0, 0));
    let removed = format!("-{line}\n").repeat(usize::try_from(count).unwrap());
    let both = file_diff(
        &format!("{header}@@ -1,{count} +1,{count} @@\n{removed}{added}"),
        "big",
    );
    assert_eq!(
        (both.added, both.removed, both.path.as_str()),
        (count, count, "big")
    );
    assert_eq!(file_diff("", "gone").path, "gone");
}
