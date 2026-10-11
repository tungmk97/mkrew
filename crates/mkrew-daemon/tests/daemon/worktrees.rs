//! Worktree tasks, worktree-targeted fs/git, merge back, fan-out, npm shims.

use super::*;

/// M13 worktree task (SPEC §20E.1): the first run adds the task's worktree and branch from
/// HEAD, copies `.worktreeinclude` entries, names the skipped ones in the session's first
/// line and runs the agent in the project's folder inside the worktree; the user's checkout
/// is untouched. The task keeps its worktree (no turning it off, no delete), and a daemon
/// restart resumes the agent in the same folder.
#[test]
fn worktree_tasks_run_and_resume_in_their_own_worktree() {
    let env = Env::new();
    let scenario = env.dir.path().join("worktree.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"write_file\"\npath = \"made.txt\"\ncontent = \"run\"\n\
         [[step]]\naction = \"hook\"\nevent = \"busy\"\nsession_id = \"native-wt\"\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    let resumed_scenario = env.dir.path().join("resumed.toml");
    std::fs::write(
        &resumed_scenario,
        "[[step]]\naction = \"write_file\"\npath = \"resumed.txt\"\ncontent = \"again\"\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    let args = format!("['--scenario', '{}']", scenario.display());
    env.write_fake_profile_as("wt", &args);
    let path = env.dir.path().join("config/agents/wt.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    let resume = format!(
        "resume = '\"{}\" --resume {{session_id}} --scenario \"{}\"'\n",
        built_bin("mkrew-fakeagent").display(),
        resumed_scenario.display()
    );
    std::fs::write(
        &path,
        text.replace("[state]", &format!("{resume}\n[state]")),
    )
    .unwrap();

    let repo = env.repo();
    let app = repo.join("app");
    std::fs::create_dir_all(&app).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join(".gitignore"), ".env\n").unwrap();
    std::fs::write(app.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    std::fs::write(repo.join(".env"), "SECRET=1\n").unwrap();
    std::fs::write(repo.join(".worktreeinclude"), ".env\nmissing.txt\n").unwrap();
    std::fs::write(app.join("a.txt"), "uncommitted\n").unwrap();
    let main_state = |repo: &Path| {
        [
            git(repo, &["rev-parse", "HEAD"]),
            git(repo, &["symbolic-ref", "HEAD"]),
            git(repo, &["status", "--porcelain", "--ignored"]),
        ]
    };
    let before = main_state(&repo);

    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&app);
    let task: Task = client
        .call(Request::TaskCreate(TaskCreateParams {
            project_id: project.id,
            title: "Fix the login".into(),
            prompt: String::new(),
            agent_profile: Some("wt".into()),
            position: None,
            worktree: true,
        }))
        .into_result()
        .unwrap();
    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    let task = wait_task(&mut client, task.id, |t| {
        t.worktree_path.is_some() && t.checkpoint.is_some()
    });
    let root = PathBuf::from(task.worktree_path.clone().unwrap());
    assert!(
        root.ends_with(Path::new(&project.id.to_string()).join(task.id.to_string())),
        "{}",
        root.display()
    );
    let branch = task.branch.clone().unwrap();
    assert!(branch.starts_with("mkrew/fix-the-login-"), "{branch}");
    assert_eq!(task.checkpoint.as_deref(), Some(before[0].as_str()));
    assert_eq!(
        git(&root, &["symbolic-ref", "HEAD"]),
        format!("refs/heads/{branch}")
    );
    let deadline = Instant::now() + TIMEOUT;
    while client.session(run.session_id).native_session_id.is_none() {
        assert!(Instant::now() < deadline, "no native session id reported");
        std::thread::sleep(Duration::from_millis(50));
    }
    client.wait_state(run.session_id, SessionState::NeedsInput);
    // The agent worked in the project's folder of the worktree, with the copied `.env`.
    assert_eq!(
        std::fs::read_to_string(root.join("app/made.txt")).unwrap(),
        "run"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("app/a.txt")).unwrap(),
        "a\n"
    );
    assert_eq!(
        std::fs::read_to_string(root.join(".env")).unwrap(),
        "SECRET=1\n"
    );
    assert!(!app.join("made.txt").exists());
    assert_eq!(main_state(&repo), before);
    let banner = "mkrew: .worktreeinclude skipped missing.txt: not found";
    let attached: SessionAttachResult = client.attach(run.session_id).into_result().unwrap();
    let screen = screen_text(&attached.snapshot);
    assert!(screen.contains(banner), "{screen}");

    // Undo looks at the worktree, not at the user's uncommitted `a.txt`.
    let preview: UndoPreview = client
        .call(Request::TaskUndoPreview(TaskRef { task_id: task.id }))
        .into_result()
        .unwrap();
    assert!(preview.changed.is_empty(), "{preview:?}");
    assert_eq!(preview.created, ["made.txt"]);
    // Turn snapshots are of the worktree: never restored into the project folder (T13.4).
    // The turn is recorded shortly after the state change.
    client.wait_event_where(
        |e| matches!(e, Event::SessionTurn(t) if t.session_id == run.session_id && t.n == 1),
    );
    let restore = Request::TurnRestorePreview(TurnRef {
        session_id: run.session_id,
        n: 1,
    });
    assert_eq!(error_code(client.call(restore)), ErrorCode::InvalidState);

    let turn_off = Request::TaskUpdate(TaskUpdateParams {
        task_id: task.id,
        title: None,
        prompt: None,
        agent_profile: None,
        worktree: Some(false),
    });
    assert_eq!(error_code(client.call(turn_off)), ErrorCode::InvalidState);
    let delete = Request::TaskDelete(TaskRef { task_id: task.id });
    assert_eq!(error_code(client.call(delete)), ErrorCode::InvalidState);

    client.stop();
    assert!(daemon.wait_exit().success());
    let log = session_log(&env, run.session_id);
    assert!(log.starts_with(&format!("{banner}\r\n")), "{log}");
    let (_daemon, mut client) = env.start();
    // Resumed in the same folder: no second worktree, the resumed agent writes there too.
    wait_file(&root.join("app/resumed.txt"));
    assert!(!app.join("resumed.txt").exists());
    let tasks = list_tasks(&mut client, &project);
    assert_eq!(tasks[0].worktree_path, task.worktree_path);
    assert_eq!(tasks[0].branch, task.branch);
    assert_eq!(main_state(&repo), before);
    client.stop();
}

/// T13.4: `fs.*` and `git.*` with `worktree_task_id` work on the project's folder inside the
/// task's worktree, which is watched (`git.changed` names the task); the user's checkout and
/// its views are untouched.
#[test]
fn fs_and_git_requests_target_a_task_worktree() {
    let env = Env::new();
    let scenario = env.dir.path().join("worktree.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"write_file\"\npath = \"made.txt\"\ncontent = \"run\"\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    env.write_fake_profile_as("wt", &format!("['--scenario', '{}']", scenario.display()));
    let repo = env.repo();
    let app = repo.join("app");
    std::fs::create_dir_all(&app).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    for (key, value) in [
        ("core.autocrlf", "false"),
        ("user.name", "t"),
        ("user.email", "t@t"),
        ("commit.gpgsign", "false"),
    ] {
        git(&repo, &["config", key, value]);
    }
    std::fs::write(app.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let main_head = git(&repo, &["rev-parse", "HEAD"]);

    let (_daemon, mut client) = env.start();
    let project = client.add_project(&app);
    let create = |worktree| {
        Request::TaskCreate(TaskCreateParams {
            project_id: project.id,
            title: "Worktree views".into(),
            prompt: String::new(),
            agent_profile: Some("wt".into()),
            position: None,
            worktree,
        })
    };
    let task: Task = client.call(create(true)).into_result().unwrap();
    let plain: Task = client.call(create(false)).into_result().unwrap();
    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    client.wait_state(run.session_id, SessionState::NeedsInput);
    let task = wait_task(&mut client, task.id, |t| t.worktree_path.is_some());
    let root = PathBuf::from(task.worktree_path.clone().unwrap());
    let wt = Some(task.id);

    let list = |client: &mut Client, worktree_task_id| -> Vec<String> {
        let request = Request::FsList(FsListParams {
            project_id: project.id,
            worktree_task_id,
            dir: String::new(),
            show_ignored: false,
        });
        let entries: Vec<FsEntry> = client.call(request).into_result().unwrap();
        entries.into_iter().map(|e| e.path).collect()
    };
    assert_eq!(list(&mut client, wt), ["a.txt", "made.txt"]);
    assert_eq!(list(&mut client, None), ["a.txt"]);
    let read = Request::FsRead(FsReadParams {
        project_id: project.id,
        worktree_task_id: wt,
        path: "made.txt".into(),
        index_text: false,
    });
    let content: FsContent = client.call(read).into_result().unwrap();
    assert_eq!(content.content.as_deref(), Some("run"));
    let find = |client: &mut Client, worktree_task_id| -> Vec<String> {
        let request = Request::FsFind(FsFindParams {
            project_id: project.id,
            worktree_task_id,
            query: "made".into(),
            limit: 10,
        });
        let found: Vec<FsMatch> = client.call(request).into_result().unwrap();
        found.into_iter().map(|m| m.path).collect()
    };
    assert_eq!(find(&mut client, wt), ["made.txt"]);
    assert!(find(&mut client, None).is_empty());

    let status = |client: &mut Client, worktree_task_id| -> GitStatus {
        let request = Request::GitStatus(GitStatusParams {
            project_id: project.id,
            worktree_task_id,
            repo: None,
            mode: GitMode::Working,
            since: None,
        });
        client.call(request).into_result().unwrap()
    };
    let in_worktree = status(&mut client, wt);
    assert_eq!(in_worktree.branch, task.branch);
    assert_eq!(in_worktree.untracked[0].path, "made.txt");
    let main = status(&mut client, None);
    assert_eq!(main.branch.as_deref(), Some("main"));
    assert!(main.untracked.is_empty());

    // The worktree's own watcher reports its changes.
    client.events.clear();
    std::fs::write(root.join("app/later.txt"), "x").unwrap();
    client.wait_event(&Event::GitChanged(GitChangedEvent {
        project_id: project.id,
        worktree_task_id: wt,
    }));
    // ... and drops the worktree's cached `fs.find` list.
    let request = Request::FsFind(FsFindParams {
        project_id: project.id,
        worktree_task_id: wt,
        query: "later".into(),
        limit: 10,
    });
    let found: Vec<FsMatch> = client.call(request).into_result().unwrap();
    assert_eq!(found[0].path, "later.txt");

    // Stage and commit inside the worktree: its branch moves, `main` does not.
    let stage = Request::GitStage(GitStageParams {
        project_id: project.id,
        worktree_task_id: wt,
        repo: None,
        paths: vec!["made.txt".into()],
        hunk: None,
    });
    let _: () = client.call(stage).into_result().unwrap();
    let commit = Request::GitCommit(GitCommitParams {
        project_id: project.id,
        worktree_task_id: wt,
        repo: None,
        message: "made in the worktree".into(),
    });
    let made: GitCommitResult = client.call(commit).into_result().unwrap();
    let log = |client: &mut Client, worktree_task_id| -> Vec<String> {
        let request = Request::GitLog(GitLogParams {
            project_id: project.id,
            worktree_task_id,
            repo: None,
            limit: 10,
            path: None,
        });
        let commits: Vec<Commit> = client.call(request).into_result().unwrap();
        commits.into_iter().map(|c| c.summary).collect()
    };
    assert_eq!(log(&mut client, wt), ["made in the worktree", "one"]);
    assert_eq!(log(&mut client, None), ["one"]);
    let show = Request::GitShow(GitShowParams {
        project_id: project.id,
        worktree_task_id: wt,
        repo: None,
        commit: made.commit,
    });
    let detail: CommitDetail = client.call(show).into_result().unwrap();
    assert_eq!(detail.files[0].path, "made.txt");
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), main_head);
    assert!(!app.join("made.txt").exists());

    // Unknown task, a task without a worktree, a task of another project.
    let other = client.add_project(env.dir.path());
    let fs_list = |project_id, worktree_task_id| {
        Request::FsList(FsListParams {
            project_id,
            worktree_task_id,
            dir: String::new(),
            show_ignored: false,
        })
    };
    assert_eq!(
        error_code(client.call(fs_list(project.id, Some(Ulid::new())))),
        ErrorCode::NotFound
    );
    assert_eq!(
        error_code(client.call(fs_list(project.id, Some(plain.id)))),
        ErrorCode::InvalidState
    );
    assert_eq!(
        error_code(client.call(fs_list(other.id, wt))),
        ErrorCode::InvalidRequest
    );
    client.stop();
}

/// T13.5: `task.merge` needs `confirm` and a clean worktree, then merges the task's branch into
/// the user's checkout with a merge commit; `task.remove_worktree` is refused while the agent
/// runs there, then removes the (watched) worktree and its merged branch so the task can be
/// deleted.
#[test]
fn worktree_tasks_merge_back_and_remove_their_worktree() {
    let env = Env::new();
    let scenario = env.dir.path().join("worktree.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"write_file\"\npath = \"made.txt\"\ncontent = \"run\"\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    env.write_fake_profile_as("wt", &format!("['--scenario', '{}']", scenario.display()));
    let repo = env.repo();
    let app = repo.join("app");
    std::fs::create_dir_all(&app).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    for (key, value) in [
        ("core.autocrlf", "false"),
        ("user.name", "t"),
        ("user.email", "t@t"),
        ("commit.gpgsign", "false"),
    ] {
        git(&repo, &["config", key, value]);
    }
    std::fs::write(app.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let main_head = git(&repo, &["rev-parse", "HEAD"]);

    let (_daemon, mut client) = env.start();
    let project = client.add_project(&app);
    let task: Task = client
        .call(Request::TaskCreate(TaskCreateParams {
            project_id: project.id,
            title: "Merge me".into(),
            prompt: String::new(),
            agent_profile: Some("wt".into()),
            position: None,
            worktree: true,
        }))
        .into_result()
        .unwrap();
    let merge = |confirm| {
        Request::TaskMerge(TaskConfirmParams {
            task_id: task.id,
            confirm,
        })
    };
    let remove = |confirm| {
        Request::TaskRemoveWorktree(TaskConfirmParams {
            task_id: task.id,
            confirm,
        })
    };
    // No worktree yet.
    assert_eq!(
        error_code(client.call(merge(true))),
        ErrorCode::InvalidState
    );
    assert_eq!(
        error_code(client.call(remove(true))),
        ErrorCode::InvalidState
    );

    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    client.wait_state(run.session_id, SessionState::NeedsInput);
    let task = wait_task(&mut client, task.id, |t| t.worktree_path.is_some());
    let root = PathBuf::from(task.worktree_path.clone().unwrap());
    let branch = task.branch.clone().unwrap();
    // A Files view on the worktree keeps it watched.
    let list = Request::FsList(FsListParams {
        project_id: project.id,
        worktree_task_id: Some(task.id),
        dir: String::new(),
        show_ignored: false,
    });
    let _: Vec<FsEntry> = client.call(list).into_result().unwrap();

    assert_eq!(
        error_code(client.call(merge(false))),
        ErrorCode::InvalidRequest
    );
    // `made.txt` is not committed yet.
    assert_eq!(
        error_code(client.call(merge(true))),
        ErrorCode::InvalidState
    );
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), main_head);

    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "made"]);
    let tip = git(&root, &["rev-parse", "HEAD"]);
    let merged: TaskMergeResult = client.call(merge(true)).into_result().unwrap();
    assert_eq!(
        merged,
        TaskMergeResult {
            merged: true,
            conflicts: Vec::new()
        }
    );
    assert_eq!(
        git(&repo, &["rev-parse", "HEAD^1", "HEAD^2"]),
        format!("{main_head}\n{tip}")
    );
    assert_eq!(git(&repo, &["symbolic-ref", "HEAD"]), "refs/heads/main");
    assert_eq!(
        std::fs::read_to_string(app.join("made.txt")).unwrap(),
        "run"
    );

    assert_eq!(
        error_code(client.call(remove(false))),
        ErrorCode::InvalidRequest
    );
    // The agent still runs in the worktree.
    assert_eq!(
        error_code(client.call(remove(true))),
        ErrorCode::InvalidState
    );
    let kill = Request::SessionKill(SessionKillParams {
        session_id: run.session_id,
        force: true,
    });
    client.call(kill).into_result::<()>().unwrap();
    client.wait_state(run.session_id, SessionState::Exited);

    let removed: RemoveWorktreeResult = client.call(remove(true)).into_result().unwrap();
    assert!(!removed.branch_kept);
    assert!(!root.exists());
    assert!(git(&repo, &["branch", "--list", &branch]).is_empty());
    let task = wait_task(&mut client, task.id, |t| t.worktree_path.is_none());
    assert!(task.worktree && task.branch.is_none());
    let delete = Request::TaskDelete(TaskRef { task_id: task.id });
    client.call(delete).into_result::<()>().unwrap();
    client.stop();
}

/// T13.7 fan-out (SPEC §20E.2): `task.fanout` creates one child task per profile (own
/// worktree, title ` · <agent>`), runs them all and moves the original to Running; only in a
/// git repo, with 2–5 distinct profiles, once per task. A restart leaves the original alone.
#[test]
fn fanout_runs_each_agent_in_its_own_worktree() {
    let env = Env::new();
    for (id, name) in [("fa", "Agent A"), ("fb", "Agent B")] {
        let scenario = env.dir.path().join(format!("{id}.toml"));
        std::fs::write(
            &scenario,
            format!(
                "[[step]]\naction = \"write_file\"\npath = \"made.txt\"\ncontent = \"{id}\"\n\
                 [[step]]\naction = \"ask\"\n"
            ),
        )
        .unwrap();
        env.write_fake_profile_as(id, &format!("['--scenario', '{}']", scenario.display()));
        let path = env.dir.path().join(format!("config/agents/{id}.toml"));
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, text.replace("Fake agent", name)).unwrap();
    }
    let repo = env.repo();
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let task: Task = client
        .call(create_task(&project, "Fix the login", None))
        .into_result()
        .unwrap();
    let fanout = |task_id: Ulid, profiles: &[&str]| {
        Request::TaskFanout(TaskFanoutParams {
            task_id,
            agent_profiles: profiles.iter().map(|p| (*p).to_owned()).collect(),
            cols: 80,
            rows: 24,
        })
    };
    let not_git = client.call(fanout(task.id, &["fa", "fb"]));
    assert_eq!(error_code(not_git), ErrorCode::InvalidState);

    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let before = git(&repo, &["status", "--porcelain", "--ignored"]);
    let one = client.call(fanout(task.id, &["fa"]));
    assert_eq!(error_code(one), ErrorCode::InvalidRequest);
    let twice = client.call(fanout(task.id, &["fa", "fa"]));
    assert_eq!(error_code(twice), ErrorCode::InvalidRequest);
    let unknown = client.call(fanout(task.id, &["fa", "nope"]));
    assert_eq!(error_code(unknown), ErrorCode::NotFound);
    assert_eq!(list_tasks(&mut client, &project).len(), 1);

    let children: Vec<Task> = client
        .call(fanout(task.id, &["fa", "fb"]))
        .into_result()
        .unwrap();
    let titles: Vec<_> = children.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(
        titles,
        ["Fix the login · Agent A", "Fix the login · Agent B"]
    );
    for (child, id) in children.iter().zip(["fa", "fb"]) {
        assert_eq!(child.fanout_of, Some(task.id));
        assert_eq!(child.agent_profile.as_deref(), Some(id));
        assert!(child.worktree && child.branch.is_some());
        let root = PathBuf::from(child.worktree_path.clone().unwrap());
        wait_file(&root.join("made.txt"));
        assert_eq!(std::fs::read_to_string(root.join("made.txt")).unwrap(), id);
    }
    assert_ne!(children[0].worktree_path, children[1].worktree_path);
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let tasks = list_tasks(&mut client, &project);
        let status = |id: Ulid| tasks.iter().find(|t| t.id == id).unwrap().status;
        if status(task.id) == TaskStatus::Running
            && children
                .iter()
                .all(|c| status(c.id) == TaskStatus::NeedsInput)
        {
            break;
        }
        assert!(Instant::now() < deadline, "{tasks:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!repo.join("made.txt").exists());
    assert_eq!(git(&repo, &["status", "--porcelain", "--ignored"]), before);
    let again = client.call(fanout(task.id, &["fa", "fb"]));
    assert_eq!(error_code(again), ErrorCode::InvalidState);
    let nested = client.call(fanout(children[0].id, &["fa", "fb"]));
    assert_eq!(error_code(nested), ErrorCode::InvalidState);
    let six = ["fa", "fb", "c", "d", "e", "f"];
    assert_eq!(
        error_code(client.call(fanout(task.id, &six))),
        ErrorCode::InvalidRequest
    );
    // The original itself no longer runs: its children do.
    assert_eq!(
        error_code(client.call(run_task(task.id))),
        ErrorCode::InvalidState
    );

    client.stop();
    assert!(daemon.wait_exit().success());
    // The children (no resume command) are flagged; the original runs nothing to resume.
    let (_daemon, mut client) = env.start();
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let tasks = list_tasks(&mut client, &project);
        let found = |id: Ulid| tasks.iter().find(|t| t.id == id).unwrap().clone();
        if children.iter().all(|c| found(c.id).error_flag) {
            let original = found(task.id);
            assert_eq!(original.status, TaskStatus::Running);
            assert!(!original.error_flag);
            break;
        }
        assert!(Instant::now() < deadline, "{tasks:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
    client.stop();
}

/// npm installs `claude`/`codex` as `.cmd` shims; an `arg` prompt full of cmd.exe
/// metacharacters must still reach the agent verbatim (HUMAN_CHECKS T1.8). The `.js` →
/// node shim form needs a real node, so it is covered by `launch.rs` unit tests (argv) only.
#[cfg(windows)]
#[test]
fn task_run_passes_metacharacter_prompts_through_npm_cmd_shims() {
    let env = Env::new();
    let bin = env.dir.path().join("npm");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::copy(built_bin("mkrew-fakeagent"), bin.join("fake-cli.exe")).unwrap();
    let shim = bin.join("fake-cli.cmd");
    std::fs::write(&shim, "@ECHO off\r\n\"%dp0%\\fake-cli.exe\"   %*\r\n").unwrap();
    let sample = include_str!("../../../../agents/fake.toml");
    let profile = sample
        .replace(
            "command = \"mkrew-fakeagent\"",
            &format!("command = '{}'", shim.display()),
        )
        .replace("prompt_mode = \"none\"", "prompt_mode = \"arg\"");
    env.write_profile("shimmed", &profile);
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let prompt = "fix \"a&b\" at 100% ^!|<x>";
    let task: Task = client
        .call(Request::TaskCreate(TaskCreateParams {
            project_id: project.id,
            title: "shim".into(),
            prompt: prompt.into(),
            agent_profile: Some("shimmed".into()),
            position: None,
            worktree: false,
        }))
        .into_result()
        .unwrap();

    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    client.wait_state(run.session_id, SessionState::NeedsInput);
    client
        .call(send(run.session_id, "y"))
        .into_result::<SessionSendResult>()
        .unwrap();
    wait_task(&mut client, task.id, |t| t.status == TaskStatus::Review);
    let log = session_log(&env, run.session_id);
    assert!(log.contains(&format!("Task: {prompt}")), "{log:?}");
    client.stop();
}
