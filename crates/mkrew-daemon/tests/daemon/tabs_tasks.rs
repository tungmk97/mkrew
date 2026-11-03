//! Tabs/layout and tasks persisting across daemon restarts.

use super::*;

#[test]
fn tabs_and_layout_persist_across_daemon_restart() {
    let env = Env::new();
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());

    let first: TerminalTab = client
        .call(create_tab(&project, "one"))
        .into_result()
        .unwrap();
    let second: TerminalTab = client
        .call(create_tab(&project, "two"))
        .into_result()
        .unwrap();
    let doomed: TerminalTab = client
        .call(create_tab(&project, "doomed"))
        .into_result()
        .unwrap();
    assert_eq!(first.layout, serde_json::Value::Null);
    assert!(first.position < second.position && second.position < doomed.position);
    client.wait_event(&Event::TabUpdated(first.clone()));

    let renamed: TerminalTab = client
        .call(Request::TabRename(TabRenameParams {
            tab_id: first.id,
            title: "uno".into(),
        }))
        .into_result()
        .unwrap();
    client
        .call(Request::TabMove(TabMoveParams {
            tab_id: renamed.id,
            position: second.position + 0.5,
        }))
        .into_result::<TerminalTab>()
        .unwrap();
    let layout = serde_json::json!({"split": "vertical", "ratio": 0.5, "children": [
        {"session": Ulid::new().to_string()},
        {"split": "horizontal", "ratio": 0.6, "children": [
            {"session": Ulid::new().to_string()}, {"session": Ulid::new().to_string()}
        ]}
    ]});
    client
        .call(Request::TabSetLayout(TabSetLayoutParams {
            tab_id: second.id,
            layout: layout.clone(),
        }))
        .into_result::<()>()
        .unwrap();
    client
        .call(Request::TabClose(TabRef { tab_id: doomed.id }))
        .into_result::<()>()
        .unwrap();
    let before = list_tabs(&mut client, &project);
    let titles: Vec<&str> = before.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, ["two", "uno"]);
    assert_eq!(before[0].layout, layout);
    client.wait_event(&Event::TabUpdated(before[0].clone()));

    let unknown = TabRef {
        tab_id: Ulid::new(),
    };
    assert_eq!(
        error_code(client.call(Request::TabClose(unknown))),
        ErrorCode::NotFound
    );
    let orphan = Request::TabCreate(TabCreateParams {
        project_id: Ulid::new(),
        title: "x".into(),
        position: None,
    });
    assert_eq!(error_code(client.call(orphan)), ErrorCode::NotFound);

    client.stop();
    assert!(daemon.wait_exit().success());
    let (_daemon, mut client) = env.start();
    assert_eq!(list_tabs(&mut client, &project), before);
    client.stop();
}

/// A restart brings back only resumable agents: the shell and run panes it cannot bring back
/// leave their tabs and the session list, and a tab they emptied closes unless pinned. A tab
/// that was empty already stays.
#[test]
fn restart_clears_the_panes_it_cannot_bring_back() {
    let env = Env::new();
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    client
        .call(set_default_shell(&project, Some("shell")))
        .into_result::<Project>()
        .unwrap();
    let shell_in = |client: &mut Client, title: &str| {
        let tab: TerminalTab = client
            .call(create_tab(&project, title))
            .into_result()
            .unwrap();
        let spawned: SessionSpawnResult = client
            .call(spawn_params(&project, SessionKind::Shell, Some(tab.id)))
            .into_result()
            .unwrap();
        (tab, spawned.session_id)
    };
    let (_, loose) = shell_in(&mut client, "shells");
    let (kept, pinned) = shell_in(&mut client, "kept");
    client
        .call(Request::TabPin(TabPinParams {
            tab_id: kept.id,
            pinned: true,
        }))
        .into_result::<TerminalTab>()
        .unwrap();
    client
        .call(create_tab(&project, "empty"))
        .into_result::<TerminalTab>()
        .unwrap();
    client
        .call(set_runs(
            &project,
            vec![run_command("dev", ".", &long("echo dev", "echo dev"))],
            Some(false),
        ))
        .into_result::<Project>()
        .unwrap();
    client
        .call(run_start(&project, None))
        .into_result::<RunStarted>()
        .unwrap();
    assert_eq!(list_tabs(&mut client, &project).len(), 4);
    assert_eq!(run_sessions(&mut client, &project).len(), 1);
    client.stop();
    assert!(daemon.wait_exit().success());

    let (_daemon, mut client) = env.start();
    let tabs = list_tabs(&mut client, &project);
    let left: Vec<(&str, &serde_json::Value)> =
        tabs.iter().map(|t| (t.title.as_str(), &t.layout)).collect();
    assert_eq!(
        left,
        [
            ("kept", &serde_json::Value::Null),
            ("empty", &serde_json::Value::Null)
        ]
    );
    let listed: Vec<Ulid> = client
        .call(Request::SessionList(SessionListParams {
            project_id: Some(project.id),
        }))
        .into_result::<Vec<Session>>()
        .unwrap()
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert!(
        !listed.contains(&loose) && !listed.contains(&pinned),
        "{listed:?}"
    );
    assert!(run_sessions(&mut client, &project).is_empty());
    client.stop();
}

#[test]
fn tasks_crud_and_order_persist_across_daemon_restart() {
    let env = Env::new();
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());

    let first: Task = client
        .call(create_task(&project, "one", None))
        .into_result()
        .unwrap();
    let second: Task = client
        .call(create_task(&project, "two", Some("claude")))
        .into_result()
        .unwrap();
    let doomed: Task = client
        .call(create_task(&project, "doomed", None))
        .into_result()
        .unwrap();
    assert_eq!(first.status, TaskStatus::Todo);
    assert_eq!(first.prompt, "prompt for one");
    assert_eq!(second.agent_profile.as_deref(), Some("claude"));
    assert!(first.position < second.position && second.position < doomed.position);
    client.wait_event(&Event::TaskUpdated(first.clone()));

    let edited: Task = client
        .call(Request::TaskUpdate(TaskUpdateParams {
            task_id: second.id,
            title: Some("deux".into()),
            prompt: None,
            agent_profile: Some(None),
            worktree: Some(true),
        }))
        .into_result()
        .unwrap();
    assert_eq!(
        (edited.title.as_str(), edited.agent_profile.as_deref()),
        ("deux", None)
    );
    assert_eq!(edited.prompt, "prompt for two");
    assert!(edited.worktree && !first.worktree);
    assert_eq!(
        (edited.worktree_path.as_deref(), edited.branch.as_deref()),
        (None, None)
    );
    client.wait_event(&Event::TaskUpdated(edited.clone()));

    // Drop "one" into Review, then "deux" in front of it.
    let moved: Task = client
        .call(Request::TaskMove(TaskMoveParams {
            task_id: first.id,
            status: TaskStatus::Review,
            position: 1.0,
        }))
        .into_result()
        .unwrap();
    assert_eq!(moved.status, TaskStatus::Review);
    client.wait_event(&Event::TaskUpdated(moved.clone()));
    client
        .call(Request::TaskMove(TaskMoveParams {
            task_id: second.id,
            status: TaskStatus::Review,
            position: 0.5,
        }))
        .into_result::<Task>()
        .unwrap();

    client
        .call(Request::TaskDelete(TaskRef { task_id: doomed.id }))
        .into_result::<()>()
        .unwrap();
    client.wait_event(&Event::TaskDeleted(TaskDeletedEvent {
        task_id: doomed.id,
        project_id: project.id,
    }));
    let review: Vec<Task> = client
        .call(Request::TaskList(TaskListParams {
            project_id: Some(project.id),
            status: Some(TaskStatus::Review),
        }))
        .into_result()
        .unwrap();
    let titles: Vec<&str> = review.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, ["deux", "one"]);
    let before = list_tasks(&mut client, &project);
    assert_eq!(before.len(), 2);

    let unknown = || TaskRef {
        task_id: Ulid::new(),
    };
    assert_eq!(
        error_code(client.call(Request::TaskDelete(unknown()))),
        ErrorCode::NotFound
    );
    let missing_move = Request::TaskMove(TaskMoveParams {
        task_id: unknown().task_id,
        status: TaskStatus::Done,
        position: 0.0,
    });
    assert_eq!(error_code(client.call(missing_move)), ErrorCode::NotFound);
    let orphan = create_task(
        &Project {
            id: Ulid::new(),
            ..project.clone()
        },
        "x",
        None,
    );
    assert_eq!(error_code(client.call(orphan)), ErrorCode::NotFound);
    assert_eq!(
        error_code(client.call(create_task(&project, "  ", None))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(create_task(&project, "x", Some("shell")))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(create_task(&project, "x", Some("nope")))),
        ErrorCode::NotFound
    );
    let bad_edit = Request::TaskUpdate(TaskUpdateParams {
        task_id: first.id,
        title: None,
        prompt: None,
        agent_profile: Some(Some("shell".into())),
        worktree: None,
    });
    assert_eq!(error_code(client.call(bad_edit)), ErrorCode::InvalidRequest);

    client.stop();
    assert!(daemon.wait_exit().success());
    let (_daemon, mut client) = env.start();
    assert_eq!(list_tasks(&mut client, &project), before);
    client.stop();
}
