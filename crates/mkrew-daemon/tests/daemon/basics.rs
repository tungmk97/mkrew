//! Daemon startup, settings, hello, projects, agent profiles, `session.wait` errors.

use super::*;

#[test]
fn starting_twice_never_yields_two_daemons() {
    let env = Env::new();
    let (mut first, mut client) = env.start();

    let mut second = env.spawn();
    assert_eq!(
        second.wait_exit().code(),
        Some(i32::from(EXIT_ALREADY_RUNNING))
    );

    // The first daemon is unaffected.
    let status: DaemonStatusResult = client.call(Request::DaemonStatus).into_result().unwrap();
    assert_eq!(status.pid, first.0.id());
    assert_eq!(status.protocol_version, PROTOCOL_VERSION);

    // daemon.stop exits cleanly and releases the lock, so a new daemon can start.
    client
        .call(Request::DaemonStop)
        .into_result::<()>()
        .unwrap();
    assert!(first.wait_exit().success());
    let (_third, mut client) = env.start();
    client
        .call(Request::DaemonStop)
        .into_result::<()>()
        .unwrap();
}

#[test]
fn settings_are_reread_and_errors_reported_in_status() {
    // No config dir yet: the daemon creates it (seeding profiles) and watches it.
    let env = Env::new();
    let file = env.dir.path().join("config").join("settings.toml");
    let (_daemon, mut client) = env.start();
    assert_eq!(wait_error(&mut client, false), None);
    std::fs::write(&file, "[notify]\nsound = \"loud\"\n").unwrap();
    assert!(wait_error(&mut client, true).is_some());
    std::fs::write(&file, "[notify]\nsound = false\n").unwrap();

    /// Polls `daemon.status` until `settings_error` is (not) set; returns it.
    fn wait_error(client: &mut Client, wanted: bool) -> Option<String> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let status: DaemonStatusResult =
                client.call(Request::DaemonStatus).into_result().unwrap();
            if status.settings_error.is_some() == wanted {
                return status.settings_error;
            }
            assert!(
                Instant::now() < deadline,
                "settings_error stayed {:?}",
                status.settings_error
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    assert_eq!(wait_error(&mut client, false), None);

    std::fs::write(&file, "[notify]\nsound = \"loud\"\n").unwrap();
    let error = wait_error(&mut client, true).unwrap();
    assert!(error.contains("settings.toml:2:"), "{error}");

    std::fs::write(&file, "[notify]\nsound = true\n").unwrap();
    assert_eq!(wait_error(&mut client, false), None);
    client.call(Request::DaemonStop);
}

#[test]
fn hello_is_required_and_version_checked() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();

    let mut fresh = Client::connect(&env.socket).unwrap();
    assert_eq!(
        error_code(fresh.call(Request::ProjectList)),
        ErrorCode::HelloRequired
    );

    let mut old = Client::connect(&env.socket).unwrap();
    let outcome = old.call(Request::Hello(HelloParams {
        client: ClientKind::Ui,
        protocol_version: PROTOCOL_VERSION + 1,
    }));
    assert_eq!(error_code(outcome), ErrorCode::VersionMismatch);

    client
        .call(Request::DaemonStop)
        .into_result::<()>()
        .unwrap();
}

#[test]
fn projects_are_added_and_listed() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    let repo = env.dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();

    let add = |client: &mut Client, path: &Path| {
        client.call(Request::ProjectAdd(ProjectAddParams {
            path: path.to_string_lossy().into_owned(),
            name: Some("demo".into()),
        }))
    };
    let project: Project = add(&mut client, &repo).into_result().unwrap();
    assert_eq!(project.name, "demo");
    assert_eq!(
        error_code(add(&mut client, &repo)),
        ErrorCode::AlreadyExists
    );
    assert_eq!(
        error_code(add(&mut client, &repo.join("missing"))),
        ErrorCode::InvalidRequest
    );

    let listed: Vec<Project> = client.call(Request::ProjectList).into_result().unwrap();
    assert_eq!(listed, vec![project]);
    client
        .call(Request::DaemonStop)
        .into_result::<()>()
        .unwrap();
}

#[test]
fn startup_marks_stale_sessions_exited() {
    let env = Env::new();
    std::fs::create_dir_all(env.data_dir()).unwrap();
    let db = env.data_dir().join("mkrew.db");
    let session_id = Ulid::new();
    {
        let store = Store::open(&db).unwrap();
        let project = store.add_project(env.dir.path(), None).unwrap();
        store
            .insert_session(&Session {
                id: session_id,
                task_id: None,
                project_id: project.id,
                agent_profile: "fake".into(),
                kind: SessionKind::Agent,
                title: None,
                terminal_title: None,
                color: None,
                native_session_id: None,
                state: SessionState::Working,
                state_source: StateSource::Screen,
                state_changed_at: 1,
                exit_code: None,
                cols: 80,
                rows: 24,
                started_at: 1,
                ended_at: None,
                pinned: false,
                plan_path: None,
            })
            .unwrap();
    }

    let (mut daemon, mut client) = env.start();
    let status: DaemonStatusResult = client.call(Request::DaemonStatus).into_result().unwrap();
    assert_eq!(status.sessions, 0);
    // Startup also writes today's DB backup (SPEC §8).
    let backups = env.data_dir().join("backups");
    let deadline = Instant::now() + Duration::from_secs(10);
    let backup = loop {
        let found = std::fs::read_dir(&backups).ok().and_then(|entries| {
            entries
                .map(|e| e.unwrap().path())
                .find(|p| p.extension().is_some_and(|x| x == "db"))
        });
        if let Some(path) = found {
            break path;
        }
        assert!(
            Instant::now() < deadline,
            "no backup in {}",
            backups.display()
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    client
        .call(Request::DaemonStop)
        .into_result::<()>()
        .unwrap();
    assert!(daemon.wait_exit().success());

    let sessions = Store::open(&db).unwrap().list_sessions(None).unwrap();
    assert_eq!(sessions[0].id, session_id);
    assert_eq!(sessions[0].state, SessionState::Exited);
    assert!(env.data_dir().join("logs").read_dir().unwrap().count() > 0);
    let backed_up = Store::open(&backup).unwrap().list_sessions(None).unwrap();
    assert_eq!(backed_up[0].id, session_id);
}

#[test]
fn agent_profiles_lists_samples_and_broken_files() {
    let env = Env::new();
    env.write_profile("broken", "name = \"Broken\"\ncommand = \"x\"\nbogus = 1\n");
    env.write_profile(
        "ghost",
        "kind = \"shell\"\ncommand = \"mkrew-no-such-shell\"\n",
    );
    let (_daemon, mut client) = env.start();

    let result: AgentProfilesResult = client.call(Request::AgentProfiles).into_result().unwrap();
    let ids: Vec<&str> = result.profiles.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "bash",
            "claude",
            "cmd",
            "codex",
            "copilot",
            "fake",
            "ghost",
            "git-bash",
            "omp",
            "powershell",
            "pwsh",
            "shell"
        ]
    );
    let installed = |id: &str| {
        result
            .profiles
            .iter()
            .find(|p| p.id == id)
            .unwrap()
            .installed
    };
    // The OS default shell always resolves; a shell missing from PATH is marked for the UI.
    assert!(installed("shell"));
    assert!(!installed("ghost"));
    assert_eq!(result.errors.len(), 1, "{:?}", result.errors);
    let error = &result.errors[0];
    assert!(error.file.ends_with("broken.toml"), "{error:?}");
    assert_eq!(error.line, Some(3));

    // The daemon keeps serving, and spawning the broken profile names the problem.
    let project = client.add_project(&env.repo());
    let outcome = client.spawn(&project, "broken");
    let Err(ResponseError::Daemon(body)) = outcome.into_result::<SessionSpawnResult>() else {
        panic!("spawning a broken profile must fail");
    };
    assert_eq!(body.code, ErrorCode::NotFound);
    assert!(body.message.contains("broken.toml:3"), "{}", body.message);
    assert_eq!(
        error_code(client.spawn(&project, "nope")),
        ErrorCode::NotFound
    );
    client.stop();
}

#[test]
fn wait_times_out_and_rejects_bad_requests() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::NeedsInput);

    // Nothing answers the prompt, so the session never gets done.
    let started = Instant::now();
    let outcome = client.call(wait(id, &[SessionState::Done], Some(300)));
    assert_eq!(error_code(outcome), ErrorCode::Timeout);
    assert!(started.elapsed() < Duration::from_secs(5));
    // Already there: answers at once.
    let now: SessionWaitResult = client
        .call(wait(id, &[SessionState::NeedsInput], Some(300)))
        .into_result()
        .unwrap();
    assert_eq!(now.state, SessionState::NeedsInput);

    assert_eq!(
        error_code(client.call(wait(id, &[], None))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(wait(Ulid::new(), &[SessionState::Idle], None))),
        ErrorCode::NotFound
    );
    assert_eq!(
        error_code(client.call(send(Ulid::new(), "y"))),
        ErrorCode::NotFound
    );
    client.stop();
}
