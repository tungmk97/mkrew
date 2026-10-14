//! Spawn defaults, task runs, checkpoints, undo, resume after restart.

use super::*;

#[test]
fn spawn_defaults_to_the_project_agent_or_the_shell_and_fills_an_empty_tab() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    // Adding is announced too, so every UI shows the new workspace.
    client.wait_event(&Event::ProjectUpdated(project.clone()));

    // No default agent yet: the client must name one.
    assert_eq!(
        error_code(client.call(spawn_params(&project, SessionKind::Agent, None))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(set_default_agent(&project, Some("shell")))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(set_default_agent(&project, Some("nope")))),
        ErrorCode::NotFound
    );
    let updated: Project = client
        .call(set_default_agent(&project, Some("fake")))
        .into_result()
        .unwrap();
    assert_eq!(updated.default_agent.as_deref(), Some("fake"));
    client.wait_event(&Event::ProjectUpdated(updated.clone()));
    let listed: Vec<Project> = client.call(Request::ProjectList).into_result().unwrap();
    assert_eq!(listed, vec![updated]);

    // The first pane of a new tab becomes its layout; later panes are the client's job.
    let tab: TerminalTab = client
        .call(create_tab(&project, "agents"))
        .into_result()
        .unwrap();
    let agent: SessionSpawnResult = client
        .call(spawn_params(&project, SessionKind::Agent, Some(tab.id)))
        .into_result()
        .unwrap();
    let session = client.session(agent.session_id);
    assert_eq!(session.agent_profile, "fake");
    assert_eq!(session.kind, SessionKind::Agent);
    let leaf = serde_json::json!({"session": agent.session_id.to_string()});
    let placed = TerminalTab {
        layout: leaf.clone(),
        ..tab.clone()
    };
    client.wait_event(&Event::TabUpdated(placed));

    // No default shell yet: refused like a missing default agent (ADR 0032).
    assert_eq!(
        error_code(client.call(spawn_params(&project, SessionKind::Shell, Some(tab.id)))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(set_default_shell(&project, Some("fake")))),
        ErrorCode::InvalidRequest,
        "an agent is not a shell"
    );
    assert_eq!(
        error_code(client.call(set_default_shell(&project, Some("nope")))),
        ErrorCode::NotFound
    );
    let with_shell: Project = client
        .call(set_default_shell(&project, Some("shell")))
        .into_result()
        .unwrap();
    assert_eq!(with_shell.default_shell.as_deref(), Some("shell"));
    assert_eq!(
        with_shell.default_agent.as_deref(),
        Some("fake"),
        "left unchanged"
    );
    let shell: SessionSpawnResult = client
        .call(spawn_params(&project, SessionKind::Shell, Some(tab.id)))
        .into_result()
        .unwrap();
    let session = client.session(shell.session_id);
    assert_eq!(session.agent_profile, "shell");
    assert_eq!(session.kind, SessionKind::Shell);
    assert_eq!(list_tabs(&mut client, &project)[0].layout, leaf);

    // A tab of another workspace is refused; clearing the default works.
    let other_repo = env.dir.path().join("other");
    std::fs::create_dir(&other_repo).unwrap();
    let other = client.add_project(&other_repo);
    assert_eq!(
        error_code(client.call(spawn_params(&other, SessionKind::Shell, Some(tab.id)))),
        ErrorCode::InvalidRequest
    );
    let cleared: Project = client
        .call(set_default_agent(&project, None))
        .into_result()
        .unwrap();
    assert_eq!(cleared.default_agent, None);

    for session_id in [agent.session_id, shell.session_id] {
        let kill = SessionKillParams {
            session_id,
            force: true,
        };
        client
            .call(Request::SessionKill(kill))
            .into_result::<()>()
            .unwrap();
        client.wait_state(session_id, SessionState::Exited);
    }
    client.stop();
}

#[test]
fn task_run_delivers_the_prompt_per_mode_and_the_card_follows_the_session() {
    let env = Env::new();
    env.write_fake_profile();
    env.write_fake_profile_with_prompt("fake-arg", "arg");
    env.write_fake_profile_with_prompt("fake-type", "type");
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let task = |client: &mut Client, title: &str, agent: Option<&str>| -> Task {
        client
            .call(create_task(&project, title, agent))
            .into_result()
            .unwrap()
    };
    let by_default = task(&mut client, "arg", None);
    let typed = task(&mut client, "type", Some("fake-type"));
    let silent = task(&mut client, "none", Some("fake"));

    // No task agent and no project default: nothing to run.
    assert_eq!(
        error_code(client.call(run_task(by_default.id))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(run_task(Ulid::new()))),
        ErrorCode::NotFound
    );
    let zero = Request::TaskRun(TaskRunParams {
        task_id: silent.id,
        cols: 0,
        rows: 24,
    });
    assert_eq!(error_code(client.call(zero)), ErrorCode::InvalidRequest);
    client
        .call(set_default_agent(&project, Some("fake-arg")))
        .into_result::<Project>()
        .unwrap();

    // `arg`: the default agent gets the prompt as its last argument; the card follows the
    // session through needs_input back to running and into review on exit 0.
    let run: SessionSpawnResult = client.call(run_task(by_default.id)).into_result().unwrap();
    let session = client.session(run.session_id);
    assert_eq!(session.agent_profile, "fake-arg");
    assert_eq!(session.task_id, Some(by_default.id));
    assert_eq!(session.project_id, project.id);
    assert_eq!(session.title.as_deref(), Some("arg"));
    wait_task(&mut client, by_default.id, |t| {
        t.status == TaskStatus::Running
    });
    client.wait_state(run.session_id, SessionState::NeedsInput);
    wait_task(&mut client, by_default.id, |t| {
        t.status == TaskStatus::NeedsInput
    });
    client
        .call(send(run.session_id, "y"))
        .into_result::<SessionSendResult>()
        .unwrap();
    let done = wait_task(&mut client, by_default.id, |t| {
        t.status == TaskStatus::Review
    });
    assert!(!done.error_flag);
    assert_eq!(client.session(run.session_id).exit_code, Some(0));
    let log = session_log(&env, run.session_id);
    assert!(log.contains("Task: prompt for arg"), "{log}");

    // `type`: the prompt is typed once the agent first waits for input (here it answers the
    // fake's question), so the run finishes on its own.
    let run: SessionSpawnResult = client.call(run_task(typed.id)).into_result().unwrap();
    assert_eq!(client.session(run.session_id).agent_profile, "fake-type");
    wait_task(&mut client, typed.id, |t| t.status == TaskStatus::Review);
    let log = session_log(&env, run.session_id);
    assert!(log.contains("prompt for type"), "{log}");
    assert!(!log.contains("Task:"), "{log}");

    // `none`: no prompt; a second run or a delete is refused while the session lives.
    let run: SessionSpawnResult = client.call(run_task(silent.id)).into_result().unwrap();
    wait_task(&mut client, silent.id, |t| {
        t.status == TaskStatus::NeedsInput
    });
    assert!(!session_log(&env, run.session_id).contains("prompt for none"));
    assert_eq!(
        error_code(client.call(run_task(silent.id))),
        ErrorCode::InvalidState
    );
    assert_eq!(
        error_code(client.call(Request::TaskDelete(TaskRef { task_id: silent.id }))),
        ErrorCode::InvalidState
    );
    // Killed (non-zero exit): needs_input with the error flag; running again clears it.
    let kill = |session_id| {
        Request::SessionKill(SessionKillParams {
            session_id,
            force: true,
        })
    };
    client
        .call(kill(run.session_id))
        .into_result::<()>()
        .unwrap();
    let failed = wait_task(&mut client, silent.id, |t| t.error_flag);
    assert_eq!(failed.status, TaskStatus::NeedsInput);
    client.events.clear();
    let rerun: SessionSpawnResult = client.call(run_task(silent.id)).into_result().unwrap();
    let running = wait_task(&mut client, silent.id, |t| t.status == TaskStatus::Running);
    assert!(!running.error_flag);

    // Stopping the daemon ends the session but is no failure: the card stays running for
    // the startup resume (SPEC §8), which cannot resume a profile without `resume` and asks
    // the user to run it again.
    client.stop();
    assert!(daemon.wait_exit().success());
    let (_daemon, mut client) = env.start();
    assert_eq!(client.session(rerun.session_id).state, SessionState::Exited);
    let after = list_tasks(&mut client, &project);
    let after = after.iter().find(|t| t.id == silent.id).unwrap();
    assert_eq!(
        (after.status, after.error_flag),
        (TaskStatus::NeedsInput, true)
    );
    client.stop();
}

/// M5 checkpoint: each run in a git repo stores HEAD in `task.checkpoint`; outside a repo the
/// task runs without one.
#[test]
fn task_run_stores_head_as_checkpoint() {
    let env = Env::new();
    env.write_fake_profile();
    let repo = env.repo();
    git(&repo, &["init", "-q"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "one"]);
    let first = git(&repo, &["rev-parse", "HEAD"]);
    let plain = env.dir.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let (mut daemon, mut client) = env.start();
    let in_repo = client.add_project(&repo);
    let outside = client.add_project(&plain);
    let task: Task = client
        .call(create_task(&in_repo, "t", Some("fake")))
        .into_result()
        .unwrap();
    assert_eq!(task.checkpoint, None);
    let kill = |session_id| {
        Request::SessionKill(SessionKillParams {
            session_id,
            force: true,
        })
    };

    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    wait_task(&mut client, task.id, |t| {
        t.checkpoint.as_deref() == Some(first.as_str())
    });
    client
        .call(kill(run.session_id))
        .into_result::<()>()
        .unwrap();
    wait_task(&mut client, task.id, |t| t.error_flag);

    // A later run takes a fresh checkpoint.
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "two"]);
    let second = git(&repo, &["rev-parse", "HEAD"]);
    client.events.clear();
    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    wait_task(&mut client, task.id, |t| {
        t.checkpoint.as_deref() == Some(second.as_str())
    });
    client
        .call(kill(run.session_id))
        .into_result::<()>()
        .unwrap();

    let other: Task = client
        .call(create_task(&outside, "t", Some("fake")))
        .into_result()
        .unwrap();
    client.events.clear();
    client
        .call(run_task(other.id))
        .into_result::<SessionSpawnResult>()
        .unwrap();
    let running = wait_task(&mut client, other.id, |t| t.status == TaskStatus::Running);
    assert_eq!(running.checkpoint, None);
    let listed = list_tasks(&mut client, &outside);
    assert_eq!(listed[0].checkpoint, None);
    client.stop();
    assert!(daemon.wait_exit().success());
}

/// M5 undo (ADR 0005): the preview lists what the run changed and created; `task.undo` needs
/// `confirm`, a stopped task and a checkpoint, then restores tracked files and deletes only
/// the untracked files created since the checkpoint.
#[test]
fn task_undo_previews_then_discards_changes_since_the_checkpoint() {
    let env = Env::new();
    env.write_fake_profile();
    let repo = env.repo();
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["init", "-q"]);
    // Byte-exact restores whatever the machine's global autocrlf says.
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let old = repo.join("old.txt");
    std::fs::write(&old, "mine\n").unwrap();
    let file = std::fs::File::options().write(true).open(&old).unwrap();
    file.set_modified(std::time::SystemTime::now() - Duration::from_secs(3600))
        .unwrap();
    drop(file);
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let task: Task = client
        .call(create_task(&project, "t", Some("fake")))
        .into_result()
        .unwrap();
    let preview = |task_id| Request::TaskUndoPreview(TaskRef { task_id });
    let undo = |task_id, confirm| {
        Request::TaskUndo(TaskUndoParams {
            task_id,
            confirm,
            created: Vec::new(),
            fingerprint: String::new(),
        })
    };

    assert_eq!(
        error_code(client.call(preview(task.id))),
        ErrorCode::InvalidState
    );
    assert_eq!(
        error_code(client.call(undo(task.id, true))),
        ErrorCode::InvalidState
    );
    assert_eq!(
        error_code(client.call(preview(Ulid::new()))),
        ErrorCode::NotFound
    );

    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    wait_task(&mut client, task.id, |t| t.checkpoint.is_some());
    std::fs::write(repo.join("a.txt"), "two\n").unwrap();
    std::fs::create_dir(repo.join("new")).unwrap();
    std::fs::write(repo.join("new").join("b.txt"), "b\n").unwrap();
    // Staged by the run (e.g. an agent's `git add -A`): not in the checkpoint, so undo
    // deletes it whatever its mtime, and the preview lists it as `added`.
    let notes = repo.join("notes.md");
    std::fs::write(&notes, "notes\n").unwrap();
    git(&repo, &["add", "notes.md"]);

    // Refused while the task's session is live, and without confirm.
    assert_eq!(
        error_code(client.call(undo(task.id, true))),
        ErrorCode::InvalidState
    );
    client
        .call(Request::SessionKill(SessionKillParams {
            session_id: run.session_id,
            force: true,
        }))
        .into_result::<()>()
        .unwrap();
    wait_task(&mut client, task.id, |t| t.error_flag);
    assert_eq!(
        error_code(client.call(undo(task.id, false))),
        ErrorCode::InvalidRequest
    );

    let shown: UndoPreview = client.call(preview(task.id)).into_result().unwrap();
    assert_eq!(shown.changed, ["a.txt"]);
    assert_eq!(shown.added, ["notes.md"]);
    assert_eq!(shown.created, ["new/b.txt"]);
    assert!(shown.diff.contains("-one\n+two"), "{}", shown.diff);
    assert!(!shown.diff_truncated);
    // Nothing is touched by the preview or the refusals.
    assert_eq!(
        std::fs::read_to_string(repo.join("a.txt")).unwrap(),
        "two\n"
    );

    // Staged after the preview was shown: the tracked changes no longer match it, so the undo
    // is refused and touches nothing.
    let staged = repo.join("staged.txt");
    std::fs::write(&staged, "staged\n").unwrap();
    git(&repo, &["add", "staged.txt"]);
    let confirmed = |shown: &UndoPreview| {
        Request::TaskUndo(TaskUndoParams {
            task_id: task.id,
            confirm: true,
            created: shown.created.clone(),
            fingerprint: shown.fingerprint.clone(),
        })
    };
    assert_eq!(
        error_code(client.call(confirmed(&shown))),
        ErrorCode::InvalidState
    );
    assert!(staged.exists());
    assert_eq!(
        std::fs::read_to_string(repo.join("a.txt")).unwrap(),
        "two\n"
    );
    git(&repo, &["rm", "-q", "--cached", "staged.txt"]);
    std::fs::remove_file(&staged).unwrap();

    // Created after the preview was shown: untracked, not confirmed, so kept.
    let late = repo.join("late.txt");
    std::fs::write(&late, "late\n").unwrap();
    client.call(confirmed(&shown)).into_result::<()>().unwrap();
    assert_eq!(
        std::fs::read_to_string(repo.join("a.txt")).unwrap(),
        "one\n"
    );
    assert!(!repo.join("new").join("b.txt").exists());
    assert!(!notes.exists());
    assert_eq!(std::fs::read_to_string(&old).unwrap(), "mine\n");
    assert!(late.exists());
    let after: UndoPreview = client.call(preview(task.id)).into_result().unwrap();
    assert!(
        after.changed.is_empty() && after.added.is_empty() && after.created == ["late.txt"],
        "{after:?}"
    );
    client.stop();
    assert!(daemon.wait_exit().success());
}

/// SPEC §8 resume: after a daemon restart a task whose agent reported its own session id runs
/// the profile's `resume` command with that id, in the old session's pane.
#[test]
fn restart_resumes_running_tasks_with_their_native_session_id() {
    let env = Env::new();
    let scenario = env.dir.path().join("native.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"hook\"\nevent = \"busy\"\nsession_id = \"native-42\"\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    let fake = built_bin("mkrew-fakeagent");
    let args = format!("args = ['--scenario', '{}']", scenario.display());
    env.write_fake_profile_as("resumable", &args["args = ".len()..]);
    let path = env.dir.path().join("config/agents/resumable.toml");
    let text = std::fs::read_to_string(&path).unwrap();
    // TOML literal string: the quoted word keeps backslashes and spaces. The resumed fake
    // runs the default scenario (no hook), so its native id can only be carried over.
    let resume = format!(
        "resume = '\"{}\" --resume {{session_id}}'\n",
        fake.display()
    );
    let text = text.replace("[state]", &format!("{resume}\n[state]"));
    std::fs::write(&path, &text).unwrap();
    // Resumable profile, but the agent never reports its id: nothing to resume.
    env.write_profile("resumable-silent", &text.replace(&args, "args = []"));

    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let task: Task = client
        .call(create_task(&project, "resume me", Some("resumable")))
        .into_result()
        .unwrap();
    let silent: Task = client
        .call(create_task(&project, "no id", Some("resumable-silent")))
        .into_result()
        .unwrap();
    let silent_run: SessionSpawnResult = client.call(run_task(silent.id)).into_result().unwrap();
    client.wait_state(silent_run.session_id, SessionState::NeedsInput);
    let tab: TerminalTab = client
        .call(create_tab(&project, "agents"))
        .into_result()
        .unwrap();
    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    let layout = json!({"split": "vertical", "ratio": 0.5, "children": [
        {"session": Ulid::new().to_string()}, {"session": run.session_id.to_string()}
    ]});
    client
        .call(Request::TabSetLayout(TabSetLayoutParams {
            tab_id: tab.id,
            layout: layout.clone(),
        }))
        .into_result::<()>()
        .unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while client.session(run.session_id).native_session_id.is_none() {
        assert!(Instant::now() < deadline, "no native session id reported");
        std::thread::sleep(Duration::from_millis(50));
    }
    client.wait_state(run.session_id, SessionState::NeedsInput);

    client.stop();
    assert!(daemon.wait_exit().success());
    let (_daemon, mut client) = env.start();
    let sessions: Vec<Session> = client
        .call(Request::SessionList(SessionListParams::default()))
        .into_result()
        .unwrap();
    let resumed = sessions
        .iter()
        .find(|s| s.task_id == Some(task.id) && s.id != run.session_id)
        .expect("the task got a new session")
        .clone();
    assert_eq!(
        (resumed.agent_profile.as_str(), resumed.title.as_deref()),
        ("resumable", Some("resume me"))
    );
    // Carried over: a second restart before the agent's next hook resumes it again.
    assert_eq!(resumed.native_session_id.as_deref(), Some("native-42"));
    assert!(
        !sessions
            .iter()
            .any(|s| s.task_id == Some(silent.id) && s.id != silent_run.session_id)
    );
    let flagged = list_tasks(&mut client, &project);
    let flagged = flagged.iter().find(|t| t.id == silent.id).unwrap();
    assert_eq!(
        (flagged.status, flagged.error_flag),
        (TaskStatus::NeedsInput, true)
    );
    client.wait_state(resumed.id, SessionState::NeedsInput);
    let attached: SessionAttachResult = client.attach(resumed.id).into_result().unwrap();
    let screen = screen_text(&attached.snapshot);
    assert!(screen.contains("Resumed: native-42"), "{screen}");
    // The card follows the resumed session like a fresh run.
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let tasks = list_tasks(&mut client, &project);
        let after = tasks.iter().find(|t| t.id == task.id).unwrap();
        assert!(!after.error_flag);
        if after.status == TaskStatus::NeedsInput {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "task stuck in {:?}",
            after.status
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let tabs = list_tabs(&mut client, &project);
    let expected = layout
        .to_string()
        .replace(&run.session_id.to_string(), &resumed.id.to_string());
    assert_eq!(
        tabs[0].layout,
        serde_json::from_str::<Value>(&expected).unwrap()
    );
    client.stop();
}

/// SPEC §8: an agent pane without a task that a tab showed when the daemon stopped comes back
/// in the same pane with its conversation. A session in no tab, or one that exited by itself
/// before the stop, stays exited, and each stop resumes the current pane, never an older one.
#[test]
fn restart_resumes_free_agent_panes_in_tabs_once() {
    let env = Env::new();
    let fake = built_bin("mkrew-fakeagent");
    let profile = |name: &str, steps: &str| {
        let scenario = env.dir.path().join(format!("{name}.toml"));
        std::fs::write(&scenario, steps).unwrap();
        env.write_fake_profile_as(name, &format!("['--scenario', '{}']", scenario.display()));
        let path = env.dir.path().join(format!("config/agents/{name}.toml"));
        let text = std::fs::read_to_string(&path).unwrap();
        let resume = format!(
            "resume = '\"{}\" --resume {{session_id}}'\n",
            fake.display()
        );
        std::fs::write(
            &path,
            text.replace("[state]", &format!("{resume}\n[state]")),
        )
        .unwrap();
    };
    profile(
        "resumable",
        "[[step]]\naction = \"hook\"\nevent = \"busy\"\nsession_id = \"native-7\"\n\
         [[step]]\naction = \"ask\"\n",
    );
    profile(
        "quits",
        "[[step]]\naction = \"hook\"\nevent = \"busy\"\nsession_id = \"native-8\"\n\
         [[step]]\naction = \"sleep\"\nms = 1500\n\
         [[step]]\naction = \"exit\"\ncode = 0\n",
    );

    let list = |client: &mut Client| -> Vec<Session> {
        client
            .call(Request::SessionList(SessionListParams::default()))
            .into_result()
            .unwrap()
    };
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let shown: SessionSpawnResult = client.spawn(&project, "resumable").into_result().unwrap();
    let hidden: SessionSpawnResult = client.spawn(&project, "resumable").into_result().unwrap();
    let quit: SessionSpawnResult = client.spawn(&project, "quits").into_result().unwrap();
    let layout = |shown: Ulid| {
        json!({"split": "vertical", "ratio": 0.5, "children": [
            {"session": shown.to_string()}, {"session": quit.session_id.to_string()}
        ]})
    };
    let tab: TerminalTab = client
        .call(create_tab(&project, "agents"))
        .into_result()
        .unwrap();
    let set_layout = |client: &mut Client, shown: Ulid| {
        client
            .call(Request::TabSetLayout(TabSetLayoutParams {
                tab_id: tab.id,
                layout: layout(shown),
            }))
            .into_result::<()>()
            .unwrap();
    };
    set_layout(&mut client, shown.session_id);
    for id in [shown.session_id, hidden.session_id] {
        let deadline = Instant::now() + TIMEOUT;
        while client.session(id).native_session_id.is_none() {
            assert!(Instant::now() < deadline, "no native session id reported");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    client
        .call(rename(shown.session_id, Some("Fix login")))
        .into_result::<Session>()
        .unwrap();
    let repo = env.repo();
    std::fs::write(repo.join("PLAN.md"), "- [ ] a\n- [x] b\n").unwrap();
    client
        .call(link_plan(shown.session_id, Some("PLAN.md")))
        .into_result::<Session>()
        .unwrap();
    client
        .call(pin(shown.session_id, true))
        .into_result::<Session>()
        .unwrap();
    client
        .call(set_color(shown.session_id, Some("teal")))
        .into_result::<Session>()
        .unwrap();
    client.wait_state(quit.session_id, SessionState::Exited);
    // The exit hook removes `quit`'s leaf a moment after the exit is stored. Once it has run it
    // never runs for `quit` again, so the layout written next keeps the leaf of the session
    // that exited before the stop (a pane left over from an older version).
    wait_layout(
        &mut client,
        &project,
        &json!({"session": shown.session_id.to_string()}),
    );
    set_layout(&mut client, shown.session_id);
    assert_eq!(
        client.session(quit.session_id).native_session_id.as_deref(),
        Some("native-8")
    );
    client.stop();
    assert!(daemon.wait_exit().success());

    let (mut daemon, mut client) = env.start();
    let sessions = list(&mut client);
    assert_eq!(
        sessions.len(),
        4,
        "only the shown pane came back: {sessions:?}"
    );
    let resumed = sessions
        .iter()
        .find(|s| ![shown.session_id, hidden.session_id, quit.session_id].contains(&s.id))
        .unwrap()
        .clone();
    assert_eq!(
        (
            resumed.task_id,
            resumed.agent_profile.as_str(),
            resumed.native_session_id.as_deref()
        ),
        (None, "resumable", Some("native-7"))
    );
    assert_eq!(
        resumed.title.as_deref(),
        Some("Fix login"),
        "the resumed pane keeps its name"
    );
    assert!(resumed.pinned, "the resumed pane keeps its pin");
    assert_eq!(
        resumed.color.as_deref(),
        Some("teal"),
        "the resumed pane keeps its color"
    );
    assert_eq!(
        resumed.plan_path.as_deref(),
        Some("PLAN.md"),
        "the resumed pane keeps its plan"
    );
    let plans: Vec<SessionPlanEvent> = client
        .call(Request::SessionPlan(ProjectRef {
            project_id: project.id,
        }))
        .into_result()
        .unwrap();
    assert_eq!(
        plans
            .iter()
            .map(|p| (p.session_id, p.done, p.total))
            .collect::<Vec<_>>(),
        [(resumed.id, 1, 2)],
        "and watches it again"
    );
    client.wait_state(resumed.id, SessionState::NeedsInput);
    let attached: SessionAttachResult = client.attach(resumed.id).into_result().unwrap();
    let screen = screen_text(&attached.snapshot);
    assert!(screen.contains("Resumed: native-7"), "{screen}");
    assert_eq!(
        list_tabs(&mut client, &project)[0].layout,
        layout(resumed.id)
    );

    client.stop();
    assert!(daemon.wait_exit().success());
    let (_daemon, mut client) = env.start();
    let sessions = list(&mut client);
    assert_eq!(
        sessions.len(),
        5,
        "one more pane, resumed from the current one: {sessions:?}"
    );
    let newest = sessions
        .iter()
        .max_by_key(|s| (s.started_at, s.id))
        .unwrap();
    assert!(newest.pinned, "every resume carries the pin on");
    assert_eq!(
        list_tabs(&mut client, &project)[0].layout,
        layout(newest.id)
    );
    client.stop();
}

/// ADR 0055: `session.resume` relaunches an exited agent session into its conversation as a
/// new session, placed in an empty tab; refused while it or its conversation still runs, and
/// for a session whose agent never reported its id.
#[test]
fn an_exited_agent_session_resumes_on_request() {
    let env = Env::new();
    let fake = built_bin("mkrew-fakeagent");
    let profile = |name: &str, steps: &str| {
        let scenario = env.dir.path().join(format!("{name}.toml"));
        std::fs::write(&scenario, steps).unwrap();
        env.write_fake_profile_as(name, &format!("['--scenario', '{}']", scenario.display()));
        let path = env.dir.path().join(format!("config/agents/{name}.toml"));
        let text = std::fs::read_to_string(&path).unwrap();
        let resume = format!(
            "resume = '\"{}\" --resume {{session_id}}'\n",
            fake.display()
        );
        std::fs::write(
            &path,
            text.replace("[state]", &format!("{resume}\n[state]")),
        )
        .unwrap();
    };
    profile(
        "quits",
        "[[step]]\naction = \"hook\"\nevent = \"busy\"\nsession_id = \"native-9\"\n\
         [[step]]\naction = \"sleep\"\nms = 500\n\
         [[step]]\naction = \"exit\"\ncode = 0\n",
    );
    profile("silent", "[[step]]\naction = \"exit\"\ncode = 0\n");
    let resume = |session_id: Ulid, tab_id: Option<Ulid>| {
        Request::SessionResume(SessionResumeParams { session_id, tab_id })
    };

    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let quit: SessionSpawnResult = client.spawn(&project, "quits").into_result().unwrap();
    assert_eq!(
        error_code(client.call(resume(quit.session_id, None))),
        ErrorCode::InvalidState,
        "still running"
    );
    client.wait_state(quit.session_id, SessionState::Exited);
    let silent: SessionSpawnResult = client.spawn(&project, "silent").into_result().unwrap();
    client.wait_state(silent.session_id, SessionState::Exited);
    assert_eq!(
        error_code(client.call(resume(silent.session_id, None))),
        ErrorCode::InvalidState,
        "no native session id"
    );

    let tab: TerminalTab = client
        .call(create_tab(&project, "resumed"))
        .into_result()
        .unwrap();
    let resumed: SessionSpawnResult = client
        .call(resume(quit.session_id, Some(tab.id)))
        .into_result()
        .unwrap();
    assert_ne!(resumed.session_id, quit.session_id, "a new session");
    let session = client.session(resumed.session_id);
    assert_eq!(
        (
            session.agent_profile.as_str(),
            session.native_session_id.as_deref()
        ),
        ("quits", Some("native-9"))
    );
    wait_layout(
        &mut client,
        &project,
        &json!({"session": resumed.session_id.to_string()}),
    );
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let attached: SessionAttachResult =
            client.attach(resumed.session_id).into_result().unwrap();
        if screen_text(&attached.snapshot).contains("Resumed: native-9") {
            break;
        }
        assert!(Instant::now() < deadline, "the resume command did not run");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        error_code(client.call(resume(quit.session_id, None))),
        ErrorCode::InvalidState,
        "its conversation already runs"
    );
    client.stop();
}

/// ADR 0033: a session that exits while the daemon runs leaves its tab (shell or agent
/// alike, the sibling takes its place); a daemon stop does not, so the pane is still there
/// for the resume (ADR 0017) after the restart.
#[test]
fn an_exited_session_leaves_its_tab_but_a_daemon_stop_keeps_the_pane() {
    let env = Env::new();
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    client
        .call(set_default_shell(&project, Some("shell")))
        .into_result::<Project>()
        .unwrap();
    let shell = |client: &mut Client| -> Ulid {
        client
            .call(spawn_params(&project, SessionKind::Shell, None))
            .into_result::<SessionSpawnResult>()
            .unwrap()
            .session_id
    };
    let (a, b) = (shell(&mut client), shell(&mut client));
    let tab: TerminalTab = client
        .call(create_tab(&project, "shells"))
        .into_result()
        .unwrap();
    client
        .call(Request::TabSetLayout(TabSetLayoutParams {
            tab_id: tab.id,
            layout: pair(a, b),
        }))
        .into_result::<()>()
        .unwrap();

    // `a` ends while the daemon runs: its pane goes, `b` takes the whole tab.
    client
        .call(Request::SessionKill(SessionKillParams {
            session_id: a,
            force: true,
        }))
        .into_result::<()>()
        .unwrap();
    client.wait_state(a, SessionState::Exited);
    let alone = json!({"session": b.to_string()});
    wait_layout(&mut client, &project, &alone);
    // The session itself is still listed: History keeps its exit code and output.
    assert_eq!(client.session(a).state, SessionState::Exited);

    // The daemon stop ends `b` too, but its pane stays for the next start.
    client.stop();
    assert!(daemon.wait_exit().success());
    let (_daemon, mut client) = env.start();
    assert_eq!(list_tabs(&mut client, &project)[0].layout, alone);
    client.stop();
}
