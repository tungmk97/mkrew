//! Git requests: base, status, diff, stage, commit, log, remotes, child repos.

use super::*;

#[test]
fn git_set_base_is_announced_persists_and_null_means_auto() {
    let env = Env::new();
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    assert_eq!(project.base_ref, None);
    let set_base = |base: Option<&str>| {
        Request::GitSetBase(GitSetBaseParams {
            project_id: project.id,
            base_ref: base.map(str::to_owned),
        })
    };

    // Returns `null`; the trimmed base comes back through `project.updated`.
    let _: () = client
        .call(set_base(Some(" develop ")))
        .into_result()
        .unwrap();
    let with_base = Project {
        base_ref: Some("develop".into()),
        ..project.clone()
    };
    client.wait_event(&Event::ProjectUpdated(with_base.clone()));
    for bad in ["--force", "a b"] {
        assert_eq!(
            error_code(client.call(set_base(Some(bad)))),
            ErrorCode::InvalidRequest
        );
    }
    let missing = Request::GitSetBase(GitSetBaseParams {
        project_id: Ulid::new(),
        base_ref: None,
    });
    assert_eq!(error_code(client.call(missing)), ErrorCode::NotFound);

    client.stop();
    assert!(daemon.wait_exit().success());
    let (_daemon, mut client) = env.start();
    let listed: Vec<Project> = client.call(Request::ProjectList).into_result().unwrap();
    assert_eq!(listed, vec![with_base]);
    // Blank is the same as `null`: back to auto-detect.
    let _: () = client.call(set_base(Some("  "))).into_result().unwrap();
    client.wait_event(&Event::ProjectUpdated(project.clone()));
    client.stop();
}

#[test]
fn git_status_and_diff_follow_the_project_base_and_reject_bad_input() {
    let env = Env::new();
    let repo = env.repo();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let fork = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["checkout", "-q", "-b", "feature"]);
    std::fs::write(repo.join("a.txt"), "one\ntwo\n").unwrap();
    let plain = env.dir.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let outside = client.add_project(&plain);
    let status = |project: &Project, mode| {
        Request::GitStatus(GitStatusParams {
            project_id: project.id,
            worktree_task_id: None,
            repo: None,
            mode,
            since: None,
        })
    };
    let diff = |path: &str, mode, commit: Option<&str>| {
        Request::GitDiff(GitDiffParams {
            project_id: project.id,
            worktree_task_id: None,
            repo: None,
            path: path.into(),
            mode,
            staged: false,
            commit: commit.map(str::to_owned),
            since: None,
        })
    };
    let since_status = |since: &str| {
        Request::GitStatus(GitStatusParams {
            project_id: project.id,
            worktree_task_id: None,
            repo: None,
            mode: GitMode::Base,
            since: Some(since.into()),
        })
    };
    let since_diff = |since: &str| {
        Request::GitDiff(GitDiffParams {
            project_id: project.id,
            worktree_task_id: None,
            repo: None,
            path: "a.txt".into(),
            mode: GitMode::Base,
            staged: false,
            commit: None,
            since: Some(since.into()),
        })
    };

    let working: GitStatus = client
        .call(status(&project, GitMode::Working))
        .into_result()
        .unwrap();
    assert_eq!(working.branch.as_deref(), Some("feature"));
    assert_eq!(working.base_ref.as_deref(), Some("main"));
    let unstaged: Vec<_> = working
        .unstaged
        .iter()
        .map(|f| (f.path.as_str(), f.added, f.removed))
        .collect();
    assert_eq!(unstaged, [("a.txt", Some(1), Some(0))]);
    let against: GitStatus = client
        .call(status(&project, GitMode::Base))
        .into_result()
        .unwrap();
    assert_eq!(against.merge_base, Some(fork.clone()));
    assert_eq!(against.against_base.len(), 1);
    let shown: FileDiff = client
        .call(diff("a.txt", GitMode::Base, None))
        .into_result()
        .unwrap();
    assert_eq!(
        (shown.path.as_str(), shown.added, shown.hunks.len()),
        ("a.txt", 1, 1)
    );

    // A configured base that names nothing: no merge-base, so no base diff.
    let set_base = Request::GitSetBase(GitSetBaseParams {
        project_id: project.id,
        base_ref: Some("develop".into()),
    });
    let _: () = client.call(set_base).into_result().unwrap();
    let against: GitStatus = client
        .call(status(&project, GitMode::Base))
        .into_result()
        .unwrap();
    assert_eq!(
        (
            against.base_ref.as_deref(),
            against.base_auto,
            against.merge_base
        ),
        (Some("develop"), false, None)
    );
    assert_eq!(
        error_code(client.call(diff("a.txt", GitMode::Base, None))),
        ErrorCode::InvalidState
    );
    // A task's checkpoint needs no base: changes since it to the working tree.
    let since: GitStatus = client.call(since_status(&fork)).into_result().unwrap();
    assert_eq!(since.merge_base.as_deref(), Some(fork.as_str()));
    assert_eq!(since.against_base.len(), 1);
    let shown: FileDiff = client.call(since_diff(&fork)).into_result().unwrap();
    assert_eq!((shown.added, shown.hunks.len()), (1, 1));
    assert_eq!(
        error_code(client.call(since_status("nope"))),
        ErrorCode::NotFound
    );
    assert_eq!(
        error_code(client.call(status(&outside, GitMode::Working))),
        ErrorCode::InvalidState
    );
    for bad in [
        diff("../a.txt", GitMode::Working, None),
        diff("a.txt", GitMode::Working, Some("--output=x")),
        since_status("--output=x"),
        since_diff("--output=x"),
    ] {
        assert_eq!(error_code(client.call(bad)), ErrorCode::InvalidRequest);
    }
    let missing = Request::GitStatus(GitStatusParams {
        project_id: Ulid::new(),
        worktree_task_id: None,
        repo: None,
        mode: GitMode::Working,
        since: None,
    });
    assert_eq!(error_code(client.call(missing)), ErrorCode::NotFound);
    client.stop();
}

/// T7.4: stage/unstage by path and by hunk, discard only when confirmed, commit; the watcher
/// started by `git.status` reports index changes as `git.changed`.
#[test]
fn git_stage_unstage_discard_and_commit_with_git_changed_events() {
    let env = Env::new();
    let repo = env.repo();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["config", "user.name", "t"]);
    git(&repo, &["config", "user.email", "t@t"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    let lines: Vec<String> = (1..=20).map(|n| format!("line {n}\n")).collect();
    std::fs::write(repo.join("a.txt"), lines.concat()).unwrap();
    std::fs::write(repo.join("b.txt"), "b\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let mut changed = lines.clone();
    changed[1] = "line two\n".into();
    changed[18] = "line nineteen\n".into();
    std::fs::write(repo.join("a.txt"), changed.concat()).unwrap();
    std::fs::write(repo.join("b.txt"), "b changed\n").unwrap();
    std::fs::write(repo.join("new.txt"), "new\n").unwrap();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let id = project.id;
    let status = |client: &mut Client| -> GitStatus {
        let request = Request::GitStatus(GitStatusParams {
            project_id: id,
            worktree_task_id: None,
            repo: None,
            mode: GitMode::Working,
            since: None,
        });
        client.call(request).into_result().unwrap()
    };
    let names = |files: &[mkrew_protocol::ChangedFile]| -> Vec<String> {
        files.iter().map(|f| f.path.clone()).collect()
    };
    let by_path = |paths: &[&str]| GitStageParams {
        project_id: id,
        worktree_task_id: None,
        repo: None,
        paths: paths.iter().map(|&p| p.to_owned()).collect(),
        hunk: None,
    };
    let by_hunk = |hunk: &mkrew_protocol::Hunk| GitStageParams {
        project_id: id,
        worktree_task_id: None,
        repo: None,
        paths: vec![],
        hunk: Some(HunkTarget {
            path: "a.txt".into(),
            hunk: hunk.clone(),
        }),
    };
    let diff = |client: &mut Client, staged| -> FileDiff {
        let request = Request::GitDiff(GitDiffParams {
            project_id: id,
            worktree_task_id: None,
            repo: None,
            path: "a.txt".into(),
            mode: GitMode::Working,
            staged,
            commit: None,
            since: None,
        });
        client.call(request).into_result().unwrap()
    };
    let changed_event = Event::GitChanged(GitChangedEvent {
        project_id: id,
        worktree_task_id: None,
    });

    // Changes made through the daemon are reported at once.
    assert_eq!(names(&status(&mut client).unstaged), ["a.txt", "b.txt"]);
    let _: () = client
        .call(Request::GitStage(by_path(&["b.txt", "new.txt"])))
        .into_result()
        .unwrap();
    client.wait_event(&changed_event);
    let now = status(&mut client);
    assert_eq!(names(&now.staged), ["b.txt", "new.txt"]);
    assert!(now.untracked.is_empty());
    let _: () = client
        .call(Request::GitUnstage(by_path(&["new.txt"])))
        .into_result()
        .unwrap();
    assert_eq!(names(&status(&mut client).untracked), ["new.txt"]);

    // Stage the second hunk of a.txt only, then unstage it from the staged diff.
    let unstaged = diff(&mut client, false).hunks;
    assert_eq!(unstaged.len(), 2);
    let _: () = client
        .call(Request::GitStage(by_hunk(&unstaged[1])))
        .into_result()
        .unwrap();
    let staged = diff(&mut client, true).hunks;
    assert_eq!(staged.len(), 1);
    assert_eq!(staged[0].lines, unstaged[1].lines);
    assert_eq!(diff(&mut client, false).hunks, [unstaged[0].clone()]);
    let _: () = client
        .call(Request::GitUnstage(by_hunk(&staged[0])))
        .into_result()
        .unwrap();
    assert_eq!(diff(&mut client, true).hunks, []);
    // The same hunk again no longer applies.
    assert_eq!(
        error_code(client.call(Request::GitUnstage(by_hunk(&staged[0])))),
        ErrorCode::Internal
    );

    // Discard only with confirm: a.txt back to the index, new.txt deleted.
    let discard = |confirm| {
        Request::GitDiscard(GitDiscardParams {
            project_id: id,
            worktree_task_id: None,
            repo: None,
            paths: vec!["a.txt".into(), "new.txt".into()],
            confirm,
        })
    };
    assert_eq!(
        error_code(client.call(discard(false))),
        ErrorCode::InvalidRequest
    );
    assert!(repo.join("new.txt").exists());
    let _: () = client.call(discard(true)).into_result().unwrap();
    assert_eq!(
        std::fs::read_to_string(repo.join("a.txt")).unwrap(),
        lines.concat()
    );
    assert!(!repo.join("new.txt").exists());

    // Commit the staged b.txt.
    let commit = |message: &str| {
        Request::GitCommit(GitCommitParams {
            project_id: id,
            worktree_task_id: None,
            repo: None,
            message: message.into(),
        })
    };
    let committed: GitCommitResult = client.call(commit("two")).into_result().unwrap();
    assert_eq!(committed.commit, git(&repo, &["rev-parse", "HEAD"]));
    assert_eq!(git(&repo, &["log", "-1", "--format=%s"]), "two");
    let clean = status(&mut client);
    assert!(clean.staged.is_empty() && clean.unstaged.is_empty() && clean.untracked.is_empty());

    // `git.status` started the watcher: a commit made outside Mkrew (only `.git` changes) is
    // reported too.
    std::thread::sleep(Duration::from_millis(500));
    client.call(Request::DaemonStatus);
    client.events.clear();
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "outside"]);
    client.wait_event(&changed_event);

    for bad in [
        commit("  \n"),
        Request::GitStage(by_path(&[])),
        Request::GitStage(by_path(&["../x"])),
        Request::GitStage(GitStageParams {
            worktree_task_id: None,
            repo: None,
            hunk: by_hunk(&unstaged[0]).hunk,
            ..by_path(&["a.txt"])
        }),
        Request::GitDiscard(GitDiscardParams {
            project_id: id,
            worktree_task_id: None,
            repo: None,
            paths: vec![],
            confirm: true,
        }),
    ] {
        assert_eq!(error_code(client.call(bad)), ErrorCode::InvalidRequest);
    }
    client.stop();
}

/// T7.5: history and one commit's files; push to the upstream and fetch from it, against a
/// local bare remote (no network). Both report `git.changed`.
#[test]
fn git_log_show_push_and_fetch_with_a_local_remote() {
    let env = Env::new();
    let repo = env.repo();
    let bare = env.dir.path().join("remote.git");
    git(
        env.dir.path(),
        &["init", "-q", "--bare", "-b", "main", "remote.git"],
    );
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["remote", "add", "origin", bare.to_str().unwrap()]);
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    std::fs::write(repo.join("b.txt"), "b\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "two\n\nbody"]);
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;
    let log = |path: Option<&str>| {
        Request::GitLog(GitLogParams {
            project_id: id,
            worktree_task_id: None,
            repo: None,
            limit: GitLogParams::DEFAULT_LIMIT,
            path: path.map(str::to_owned),
        })
    };
    let show = |commit: &str| {
        Request::GitShow(GitShowParams {
            project_id: id,
            worktree_task_id: None,
            repo: None,
            commit: commit.into(),
        })
    };
    let changed_event = Event::GitChanged(GitChangedEvent {
        project_id: id,
        worktree_task_id: None,
    });

    let commits: Vec<Commit> = client.call(log(None)).into_result().unwrap();
    let summaries: Vec<&str> = commits.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(summaries, ["two", "one"]);
    assert_eq!(commits[0].body, "body");
    let only_a: Vec<Commit> = client.call(log(Some("a.txt"))).into_result().unwrap();
    assert_eq!(only_a, [commits[1].clone()]);
    let detail: CommitDetail = client.call(show(&commits[0].hash)).into_result().unwrap();
    assert_eq!(detail.commit, commits[0]);
    let files: Vec<&str> = detail.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(files, ["b.txt"]);
    assert_eq!(error_code(client.call(show("nope"))), ErrorCode::NotFound);
    for bad in [show("--all"), log(Some("../x"))] {
        assert_eq!(error_code(client.call(bad)), ErrorCode::InvalidRequest);
    }

    // Push needs an upstream; then it updates the remote branch.
    let push = Request::GitPush(GitRemoteParams {
        project_id: id,
        worktree_task_id: None,
        repo: None,
    });
    assert_eq!(
        error_code(client.call(push.clone())),
        ErrorCode::InvalidState
    );
    git(&repo, &["push", "-q", "-u", "origin", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "three"]);
    client.events.clear();
    let _: () = client.call(push).into_result().unwrap();
    client.wait_event(&changed_event);
    assert_eq!(
        git(&bare, &["rev-parse", "main"]),
        git(&repo, &["rev-parse", "HEAD"])
    );

    // Fetch brings in someone else's commit: the branch is behind by one.
    let other = env.dir.path().join("other");
    git(
        env.dir.path(),
        &["clone", "-q", bare.to_str().unwrap(), "other"],
    );
    git(&other, &["commit", "-q", "--allow-empty", "-m", "theirs"]);
    git(&other, &["push", "-q"]);
    client.events.clear();
    let fetch = Request::GitFetch(GitRemoteParams {
        project_id: id,
        worktree_task_id: None,
        repo: None,
    });
    let _: () = client.call(fetch).into_result().unwrap();
    client.wait_event(&changed_event);
    let status: GitStatus = client
        .call(Request::GitStatus(GitStatusParams {
            project_id: id,
            worktree_task_id: None,
            repo: None,
            mode: GitMode::Working,
            since: None,
        }))
        .into_result()
        .unwrap();
    assert_eq!(status.upstream.as_deref(), Some("origin/main"));
    assert_eq!((status.ahead, status.behind), (0, 1));
    client.stop();
}

/// ADR 0029: a workspace folder that is no repository holds `fe/` and `be/` repositories.
/// `git.repos` finds them; `repo` routes each request; paths stay project-relative; a commit in
/// one leaves the other alone; the watcher follows a child's `.git`; fs.list marks both.
#[test]
fn git_requests_reach_child_repositories_with_project_relative_paths() {
    let env = Env::new();
    let root = env.repo();
    for name in ["fe", "be"] {
        let repo = root.join(name);
        std::fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "core.autocrlf", "false"]);
        // The daemon's own `git commit` needs an identity in the repo.
        git(&repo, &["config", "user.name", "t"]);
        git(&repo, &["config", "user.email", "t@t"]);
        git(&repo, &["config", "commit.gpgsign", "false"]);
        std::fs::write(repo.join("x.txt"), "one\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "one"]);
        std::fs::write(repo.join("x.txt"), "one\ntwo\n").unwrap();
    }
    std::fs::create_dir(root.join("docs")).unwrap();
    std::fs::write(root.join("docs/a.md"), "# A\n").unwrap();
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&root).id;
    let repos: Vec<String> = client
        .call(Request::GitRepos(ProjectRef { project_id: id }))
        .into_result()
        .unwrap();
    assert_eq!(repos, ["be", "fe"]);
    let status_of = |repo: Option<&str>| {
        Request::GitStatus(GitStatusParams {
            project_id: id,
            worktree_task_id: None,
            repo: repo.map(str::to_owned),
            mode: GitMode::Working,
            since: None,
        })
    };
    let names = |files: &[mkrew_protocol::ChangedFile]| -> Vec<String> {
        files.iter().map(|f| f.path.clone()).collect()
    };
    let fe: GitStatus = client.call(status_of(Some("fe"))).into_result().unwrap();
    assert_eq!(names(&fe.unstaged), ["fe/x.txt"]);
    // The project folder itself is no repository.
    assert_eq!(
        error_code(client.call(status_of(None))),
        ErrorCode::InvalidState
    );
    let diff: FileDiff = client
        .call(Request::GitDiff(GitDiffParams {
            project_id: id,
            worktree_task_id: None,
            repo: Some("fe".into()),
            path: "fe/x.txt".into(),
            mode: GitMode::Working,
            staged: false,
            commit: None,
            since: None,
        }))
        .into_result()
        .unwrap();
    assert_eq!((diff.path.as_str(), diff.added), ("fe/x.txt", 1));

    // Stage and commit in be: fe keeps its change and its history.
    let stage = |repo: &str, path: &str| {
        Request::GitStage(GitStageParams {
            project_id: id,
            worktree_task_id: None,
            repo: Some(repo.into()),
            paths: vec![path.into()],
            hunk: None,
        })
    };
    let _: () = client.call(stage("be", "be/x.txt")).into_result().unwrap();
    let committed: GitCommitResult = client
        .call(Request::GitCommit(GitCommitParams {
            project_id: id,
            worktree_task_id: None,
            repo: Some("be".into()),
            message: "two".into(),
        }))
        .into_result()
        .unwrap();
    assert_eq!(
        committed.commit,
        git(&root.join("be"), &["rev-parse", "HEAD"])
    );
    assert_eq!(git(&root.join("fe"), &["log", "-1", "--format=%s"]), "one");
    let fe: GitStatus = client.call(status_of(Some("fe"))).into_result().unwrap();
    assert!(fe.staged.is_empty());
    assert_eq!(names(&fe.unstaged), ["fe/x.txt"]);
    let shown: CommitDetail = client
        .call(Request::GitShow(GitShowParams {
            project_id: id,
            worktree_task_id: None,
            repo: Some("be".into()),
            commit: committed.commit.clone(),
        }))
        .into_result()
        .unwrap();
    assert_eq!(names(&shown.files), ["be/x.txt"]);

    // fs.list of the root: the repo folders carry the change dot, markers sit under fe/.
    let list = |client: &mut Client, dir: &str| -> Vec<FsEntry> {
        let request = Request::FsList(FsListParams {
            project_id: id,
            worktree_task_id: None,
            dir: dir.into(),
            show_ignored: false,
        });
        client.call(request).into_result().unwrap()
    };
    let top = list(&mut client, "");
    let dotted: Vec<&str> = top
        .iter()
        .filter(|e| e.has_changes)
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(dotted, ["fe"]);
    let in_fe = list(&mut client, "fe");
    let x = in_fe.iter().find(|e| e.path == "fe/x.txt").unwrap();
    assert_eq!(x.marker, Some(GitMarker::Modified));

    // `git.status` started the watcher: a commit in fe from a terminal is reported.
    std::thread::sleep(Duration::from_millis(500));
    client.call(Request::DaemonStatus);
    client.events.clear();
    git(
        &root.join("fe"),
        &["commit", "-q", "--allow-empty", "-m", "outside"],
    );
    client.wait_event(&Event::GitChanged(GitChangedEvent {
        project_id: id,
        worktree_task_id: None,
    }));

    // Bad repo names, paths outside the repo, repo + task worktree.
    for bad in [
        stage("../x", "x.txt"),
        stage("fe/.git", "fe/.git/x"),
        stage(".git", ".git/x"),
        stage("fe/src", "fe/src/a"),
        stage("fe", "be/x.txt"),
        stage("fe", "x.txt"),
        // A plain folder is no repository: git would walk up to the parent one.
        stage("docs", "docs/a.md"),
        Request::GitStatus(GitStatusParams {
            project_id: id,
            worktree_task_id: Some(Ulid::new()),
            repo: Some("fe".into()),
            mode: GitMode::Working,
            since: None,
        }),
    ] {
        assert_eq!(error_code(client.call(bad)), ErrorCode::InvalidRequest);
    }
    client.stop();
}
