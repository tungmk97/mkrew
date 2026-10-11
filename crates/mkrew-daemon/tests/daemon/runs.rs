//! Project run settings and run commands.

use super::*;

#[test]
fn project_run_settings_are_checked_trimmed_and_saved() {
    let env = Env::new();
    let repo = env.repo();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    assert!(project.run_commands.is_empty() && !project.run_autostart);
    let raw = RunCommand {
        name: " BE ".into(),
        dir: "".into(),
        command: " uv run main.py ".into(),
        env_file: Some(" ".into()),
        env: vec![("ENVIRONMENT".into(), "dev".into())],
    };
    let saved: Project = client
        .call(Request::ProjectUpdate(ProjectUpdateParams {
            project_id: project.id,
            name: None,
            default_agent: None,
            default_shell: None,
            run_commands: Some(vec![raw]),
            run_autostart: Some(true),
            color: None,
            pinned: None,
        }))
        .into_result()
        .unwrap();
    assert_eq!(
        saved.run_commands,
        [RunCommand {
            name: "BE".into(),
            dir: ".".into(),
            command: "uv run main.py".into(),
            env_file: None,
            env: vec![("ENVIRONMENT".into(), "dev".into())],
        }]
    );
    assert!(saved.run_autostart);
    client.wait_event(&Event::ProjectUpdated(saved.clone()));
    let bad = RunCommand {
        name: "a/b".into(),
        ..saved.run_commands[0].clone()
    };
    let refused = client.call(Request::ProjectUpdate(ProjectUpdateParams {
        project_id: project.id,
        name: None,
        default_agent: None,
        default_shell: None,
        run_commands: Some(vec![bad]),
        run_autostart: None,
        color: None,
        pinned: None,
    }));
    assert_eq!(error_code(refused), ErrorCode::InvalidRequest);
    client.stop();
}

#[test]
fn run_commands_start_in_their_folders_with_env_side_by_side_restart_and_stop() {
    let env = Env::new();
    let repo = env.repo();
    for dir in ["api", "web"] {
        std::fs::create_dir(repo.join(dir)).unwrap();
    }
    std::fs::write(repo.join("api/.env"), "FROM_FILE=file\nBOTH=file\n").unwrap();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let be = RunCommand {
        env_file: Some(".env".into()),
        env: vec![("BOTH".into(), "override".into())],
        ..run_command(
            "BE",
            "api",
            &long(
                "echo %FROM_FILE% %BOTH%> env.txt",
                "echo $FROM_FILE $BOTH > env.txt",
            ),
        )
    };
    let fe = run_command(
        "FE",
        "web",
        &long("echo web> web.txt", "echo web > web.txt"),
    );
    client
        .call(set_runs(&project, vec![be, fe], None))
        .into_result::<Project>()
        .unwrap();

    let first: RunStarted = client
        .call(run_start(&project, None))
        .into_result()
        .unwrap();
    let started: Vec<_> = first
        .sessions
        .iter()
        .map(|s| (s.name.as_str(), s.started))
        .collect();
    assert_eq!(started, [("BE", true), ("FE", true)]);
    let (be_id, fe_id) = (first.sessions[0].session_id, first.sessions[1].session_id);
    let be_session = client.session(be_id);
    assert_eq!(
        (be_session.kind, be_session.title.as_deref()),
        (SessionKind::Shell, Some("run:BE"))
    );
    // Env file first, then the overrides; each command in its own folder.
    assert_eq!(wait_text(&repo.join("api/env.txt")), "file override");
    assert_eq!(wait_text(&repo.join("web/web.txt")), "web");
    let tabs = list_tabs(&mut client, &project);
    assert_eq!(tabs.len(), 1);
    assert_eq!(
        (tabs[0].title.as_str(), Some(tabs[0].id)),
        ("run", first.tab_id)
    );
    assert_eq!(tabs[0].layout, pair(be_id, fe_id));

    // Starting again answers with the running ones.
    let again: RunStarted = client
        .call(run_start(&project, None))
        .into_result()
        .unwrap();
    let ids: Vec<_> = again
        .sessions
        .iter()
        .map(|s| (s.session_id, s.started))
        .collect();
    assert_eq!(ids, [(be_id, false), (fe_id, false)]);

    client
        .call(set_color(be_id, Some("teal")))
        .into_result::<Session>()
        .unwrap();
    client
        .call(pin(be_id, true))
        .into_result::<Session>()
        .unwrap();
    // Restarting BE replaces it in the first slot; FE keeps running.
    let restarted: RunStarted = client
        .call(run_restart(&project, Some("BE")))
        .into_result()
        .unwrap();
    assert_eq!(restarted.sessions.len(), 1);
    let new_be = restarted.sessions[0].session_id;
    assert!(restarted.sessions[0].started);
    assert_ne!(new_be, be_id);
    let new_be_session = client.session(new_be);
    assert_eq!(
        (new_be_session.color.as_deref(), new_be_session.pinned),
        (Some("teal"), true)
    );
    assert_eq!(client.session(be_id).state, SessionState::Exited);
    assert_ne!(client.session(fe_id).state, SessionState::Exited);
    // The old BE's exit must not take the new BE's pane with it.
    wait_layout(&mut client, &project, &pair(new_be, fe_id));
    // Stopping FE removes only FE's pane: the new BE's leaf stays, alone.
    client
        .call(run_stop(&project, Some("FE")))
        .into_result::<()>()
        .unwrap();
    client.wait_state(fe_id, SessionState::Exited);
    wait_layout(
        &mut client,
        &project,
        &json!({"session": new_be.to_string()}),
    );

    // A command removed from the setup still stops with "stop all", and a stopped command's
    // pane leaves the run tab (the last one leaves an empty tab).
    client
        .call(set_runs(&project, Vec::new(), None))
        .into_result::<Project>()
        .unwrap();
    client
        .call(run_stop(&project, None))
        .into_result::<()>()
        .unwrap();
    client.wait_state(new_be, SessionState::Exited);
    client.wait_state(fe_id, SessionState::Exited);
    wait_layout(&mut client, &project, &serde_json::Value::Null);
    client.stop();
}

#[test]
fn invalid_run_setups_are_refused_before_anything_starts() {
    let env = Env::new();
    let repo = env.repo();
    std::fs::write(repo.join("bad.env"), "not a pair\n").unwrap();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    assert_eq!(
        error_code(client.call(run_start(&project, None))),
        ErrorCode::InvalidState
    );
    client
        .call(run_stop(&project, None))
        .into_result::<()>()
        .unwrap();

    let a = run_command("A", ".", "echo a> a.txt");
    let broken = [
        run_command("B", "missing", "echo b"),
        RunCommand {
            env_file: Some("nope.env".into()),
            ..run_command("B", ".", "echo b")
        },
        RunCommand {
            env_file: Some("bad.env".into()),
            ..run_command("B", ".", "echo b")
        },
    ];
    for b in broken {
        client
            .call(set_runs(&project, vec![a.clone(), b.clone()], None))
            .into_result::<Project>()
            .unwrap();
        assert_eq!(
            error_code(client.call(run_start(&project, None))),
            ErrorCode::InvalidRequest,
            "{b:?}"
        );
    }
    assert_eq!(
        error_code(client.call(run_start(&project, Some("Z")))),
        ErrorCode::NotFound
    );
    assert!(run_sessions(&mut client, &project).is_empty());
    assert!(!repo.join("a.txt").exists());
    client.stop();
}

#[test]
fn run_restarts_that_cannot_start_keep_the_running_command() {
    let env = Env::new();
    let repo = env.repo();
    std::fs::create_dir(repo.join("api")).unwrap();
    std::fs::write(repo.join("api/.env"), "PORT=1\n").unwrap();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let api = RunCommand {
        env_file: Some(".env".into()),
        ..run_command("api", "api", &long("echo up> up.txt", "echo up > up.txt"))
    };
    client
        .call(set_runs(&project, vec![api], None))
        .into_result::<Project>()
        .unwrap();
    let started: RunStarted = client
        .call(run_start(&project, None))
        .into_result()
        .unwrap();
    let id = started.sessions[0].session_id;
    wait_text(&repo.join("api/up.txt"));

    // The env file no longer parses: refused, the server keeps running.
    std::fs::write(repo.join("api/.env"), "not a pair\n").unwrap();
    for name in [None, Some("api")] {
        assert_eq!(
            error_code(client.call(run_restart(&project, name))),
            ErrorCode::InvalidRequest,
            "{name:?}"
        );
    }
    // An unknown name, and a live command no longer configured, are not found.
    assert_eq!(
        error_code(client.call(run_restart(&project, Some("Z")))),
        ErrorCode::NotFound
    );
    client
        .call(set_runs(
            &project,
            vec![run_command("web", ".", "echo web")],
            None,
        ))
        .into_result::<Project>()
        .unwrap();
    assert_eq!(
        error_code(client.call(run_restart(&project, Some("api")))),
        ErrorCode::NotFound
    );
    std::thread::sleep(Duration::from_millis(500));
    assert_ne!(client.session(id).state, SessionState::Exited);

    client
        .call(run_stop(&project, Some("api")))
        .into_result::<()>()
        .unwrap();
    client.wait_state(id, SessionState::Exited);
    client.stop();
}

#[test]
fn run_commands_auto_start_after_a_daemon_restart_and_end_with_their_command() {
    let env = Env::new();
    let repo = env.repo();
    let marker = repo.join("started.txt");
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    client
        .call(set_runs(
            &project,
            vec![run_command("dev", ".", "echo started> started.txt")],
            Some(true),
        ))
        .into_result::<Project>()
        .unwrap();
    client.stop();
    daemon.wait_exit();

    let (_daemon, mut client) = env.start();
    wait_file(&marker);
    let sessions = run_sessions(&mut client, &project);
    assert_eq!(sessions.len(), 1);
    // The typed line ends the shell together with the command.
    client.wait_state(sessions[0].id, SessionState::Exited);
    // The command ended by itself: its pane left the run tab.
    wait_layout(&mut client, &project, &serde_json::Value::Null);

    // Restarting an exited command starts it again.
    std::fs::remove_file(&marker).unwrap();
    let again: RunStarted = client
        .call(run_restart(&project, Some("dev")))
        .into_result()
        .unwrap();
    assert!(again.sessions[0].started);
    wait_file(&marker);
    client.stop();
}

#[test]
fn run_sessions_report_the_ports_they_print() {
    let env = Env::new();
    let repo = env.repo();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let web = run_command(
        "web",
        ".",
        &long(
            "echo Local: http://localhost:5173/",
            "echo 'Local: http://localhost:5173/'",
        ),
    );
    client
        .call(set_runs(&project, vec![web], None))
        .into_result::<Project>()
        .unwrap();
    let started: RunStarted = client
        .call(run_start(&project, None))
        .into_result()
        .unwrap();
    let id = started.sessions[0].session_id;
    client.wait_event_where(
        |e| matches!(e, Event::SessionPorts(p) if p.session_id == id && p.ports == [5173]),
    );
    let ports = |client: &mut Client| -> Vec<SessionPortsEvent> {
        client
            .call(Request::SessionPorts(ProjectRef {
                project_id: project.id,
            }))
            .into_result()
            .unwrap()
    };
    assert_eq!(
        ports(&mut client),
        [SessionPortsEvent {
            session_id: id,
            ports: vec![5173]
        }]
    );
    client
        .call(run_stop(&project, None))
        .into_result::<()>()
        .unwrap();
    client.wait_state(id, SessionState::Exited);
    assert!(ports(&mut client).is_empty());
    client.stop();
}
