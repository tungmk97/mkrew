//! Session states from the fake agent: hooks, screen, attachments, env, the file watcher.

use super::*;

#[test]
fn fake_agent_session_reaches_needs_input_and_is_killed() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());

    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    // The prompt is only visible on the emulated screen: the PTY works end to end.
    let session = client.wait_state(id, SessionState::NeedsInput);
    assert_eq!((session.cols, session.rows), (80, 24));
    let event = client.wait_event_where(|e| {
        matches!(e, Event::SessionState(s) if s.session_id == id && s.state == SessionState::NeedsInput)
    });
    let Event::SessionState(event) = event else {
        unreachable!()
    };
    // Found by the screen pattern; the event and the stored row agree.
    assert_eq!(event.source, StateSource::Screen);
    assert_eq!(session.state_source, StateSource::Screen);
    assert_eq!(event.changed_at, session.state_changed_at);
    assert!(event.changed_at >= session.started_at);
    // Entering needs_input notifies (SPEC §8).
    let notice = env.wait_notification(id);
    assert_eq!(notice["project_id"], project.id.to_string());
    assert_eq!(notice["body"], "Needs input");
    let status: DaemonStatusResult = client.call(Request::DaemonStatus).into_result().unwrap();
    assert_eq!(status.sessions, 1);

    let kill = |session_id, force| Request::SessionKill(SessionKillParams { session_id, force });
    client.call(kill(id, true)).into_result::<()>().unwrap();
    let session = client.wait_state(id, SessionState::Exited);
    assert!(session.ended_at.is_some());
    client.wait_event(&Event::SessionExited(SessionExitedEvent {
        session_id: id,
        exit_code: session.exit_code,
    }));
    // Killing an exited session is fine; an unknown one is not found.
    client.call(kill(id, false)).into_result::<()>().unwrap();
    assert_eq!(
        error_code(client.call(kill(Ulid::new(), false))),
        ErrorCode::NotFound
    );

    let log = session_log(&env, id);
    assert!(log.contains("Do you want to proceed? (y/n)"), "{log}");
    client.stop();
}

/// A pasted image lands in the session's attachments dir; bad params are rejected
/// (SPEC §20B.5).
#[test]
fn attachments_are_saved_per_session_and_bad_ones_rejected() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;

    let image = vec![0x89, b'P', b'N', b'G', 0, 1, 2];
    let saved: AttachmentSaveResult = client
        .call(save_attachment(id, "png", image.clone()))
        .into_result()
        .unwrap();
    let path = PathBuf::from(&saved.path);
    let dir = env.data_dir().join("attachments").join(id.to_string());
    assert_eq!(path.parent().unwrap(), dir);
    assert_eq!(path.extension().unwrap(), "png");
    assert_eq!(std::fs::read(&path).unwrap(), image);

    let too_big = vec![0; 10 * 1024 * 1024 + 1];
    for request in [
        save_attachment(id, "exe", vec![1]),
        save_attachment(id, "png", Vec::new()),
        save_attachment(id, "png", too_big),
    ] {
        assert_eq!(error_code(client.call(request)), ErrorCode::InvalidRequest);
    }
    assert_eq!(
        error_code(client.call(save_attachment(Ulid::new(), "png", vec![1]))),
        ErrorCode::NotFound
    );
    assert_eq!(dir.read_dir().unwrap().count(), 1);
    client.stop();
}

#[test]
fn hook_reports_drive_the_state_of_live_sessions_only() {
    let env = Env::new();
    env.write_fake_profile();
    // The fake profile with hooks enabled.
    let path = env
        .dir
        .path()
        .join("config")
        .join("agents")
        .join("fake.toml");
    let fake = std::fs::read_to_string(&path).unwrap();
    let hooked = fake.replace("mode = \"screen\"", "mode = \"hook+screen\"");
    assert_ne!(hooked, fake);
    env.write_profile("hooked", &hooked);
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "hooked").into_result().unwrap();
    let id = spawned.session_id;
    let started = client.session(id);
    assert_eq!(started.state_source, StateSource::Daemon);
    assert_eq!(started.state_changed_at, started.started_at);

    // Only a live session may report, and only for a known agent.
    let unknown = client.hook(Ulid::new(), "fake", "busy", Value::Null);
    assert_eq!(error_code(unknown), ErrorCode::NotFound);
    let unknown_agent = client.hook(id, "gemini", "busy", Value::Null);
    assert_eq!(error_code(unknown_agent), ErrorCode::InvalidRequest);

    // needs_input first: the fake agent's first output may already have made it `working`.
    let native = json!({"session_id": "native-1"});
    client
        .hook(id, "fake", "needs_input", native)
        .into_result::<()>()
        .unwrap();
    let session = client.wait_state(id, SessionState::NeedsInput);
    assert_eq!(session.state_source, StateSource::Hook);
    assert_eq!(session.native_session_id.as_deref(), Some("native-1"));
    client.wait_event_where(|e| {
        matches!(e, Event::SessionState(s)
            if s.session_id == id && s.state == SessionState::NeedsInput
                && s.source == StateSource::Hook && s.changed_at == session.state_changed_at)
    });
    client
        .hook(id, "fake", "busy", Value::Null)
        .into_result::<()>()
        .unwrap();
    let session = client.wait_state(id, SessionState::Working);
    assert_eq!(session.state_source, StateSource::Hook);
    // A known agent's unknown event is recorded but changes nothing.
    client
        .hook(id, "fake", "thinking", Value::Null)
        .into_result::<()>()
        .unwrap();
    client
        .hook(id, "fake", "done", Value::Null)
        .into_result::<()>()
        .unwrap();
    let session = client.wait_state(id, SessionState::Done);
    assert_eq!(session.state_source, StateSource::Hook);
    // Hook events take priority: the fake agent's own (y/n) prompt does not override `done`.
    std::thread::sleep(Duration::from_millis(1_500));
    assert_eq!(client.session(id).state, SessionState::Done);

    let events = live_store(&env).session_events(id, "hook").unwrap();
    let names: Vec<&str> = events
        .iter()
        .map(|e| e.payload["event"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["needs_input", "busy", "thinking", "done"]);
    assert_eq!(events[0].payload["agent"], "fake");
    assert_eq!(events[0].payload["payload"]["session_id"], "native-1");

    // Exited sessions are rejected; the exit itself comes from the daemon.
    let kill = Request::SessionKill(SessionKillParams {
        session_id: id,
        force: true,
    });
    client.call(kill).into_result::<()>().unwrap();
    let session = client.wait_state(id, SessionState::Exited);
    assert_eq!(session.state_source, StateSource::Daemon);
    assert_eq!(session.native_session_id.as_deref(), Some("native-1"));
    let late = client.hook(id, "fake", "busy", Value::Null);
    assert_eq!(error_code(late), ErrorCode::InvalidState);
    client.stop();
}

#[test]
fn screen_only_profiles_record_hooks_but_ignore_them() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    client
        .hook(id, "fake", "done", json!({"session_id": "n"}))
        .into_result::<()>()
        .unwrap();
    // The screen still decides: the fake agent's prompt makes it needs_input, not done.
    let session = client.wait_state(id, SessionState::NeedsInput);
    assert_eq!(session.state_source, StateSource::Screen);
    assert_eq!(session.native_session_id.as_deref(), Some("n"));
    let events = live_store(&env).session_events(id, "hook").unwrap();
    assert_eq!(events.len(), 1);
    client.stop();
}

/// The M3 e2e: every state transition, as `session.state` events, for a screen-mode agent,
/// a hooked agent and an omp-like agent, all running at once in one daemon.
#[test]
fn fake_agents_go_through_every_state_transition() {
    use SessionState::*;
    use StateSource::{Daemon as D, Hook as H, Screen as S};

    let env = Env::new();
    // `mk` next to the fake agent: its `hook` steps run `mk hook fake <event>`.
    built_bin("mk");
    // Quiet gaps far beyond each profile's `idle_after`, so a stalled daemon still sees Idle.
    let omp_scenario = env.dir.path().join("omp-scenario.toml");
    std::fs::write(&omp_scenario, OMP_SCENARIO).unwrap();
    for (name, scenario) in [("screen", SCREEN_SCENARIO), ("hooks", HOOK_SCENARIO)] {
        let path = env.dir.path().join(format!("{name}.toml"));
        std::fs::write(&path, scenario).unwrap();
        env.write_fake_profile_as(name, &format!("['--scenario', '{}']", path.display()));
    }
    let profiles = env.dir.path().join("config").join("agents");
    // Agent start-up under a loaded test run (and a first `mk hook` run that the virus scanner
    // holds up for seconds) must not look like a quiet (idle) period.
    let idle_after = |text: String, ms: &str| {
        let changed = text.replace("idle_after_ms = 1000", &format!("idle_after_ms = {ms}"));
        assert_ne!(changed, text);
        changed
    };
    let screen = std::fs::read_to_string(profiles.join("screen.toml")).unwrap();
    env.write_profile("screen", &idle_after(screen, "2000"));
    // Idle only shows after the 10 s hook window anyway, so a long quiet period costs nothing.
    let hooks = std::fs::read_to_string(profiles.join("hooks.toml")).unwrap();
    let hooks = idle_after(hooks, "8000").replace("mode = \"screen\"", "mode = \"hook+screen\"");
    assert!(hooks.contains("hook+screen"), "{hooks}");
    env.write_profile("hooks", &hooks);
    // The shipped omp profile, running a copy of the fake agent's `omp` scenario with longer
    // quiet gaps.
    let omp = include_str!("../../../../agents/omp.toml").replace(
        "command = \"omp\"",
        &format!(
            "command = '{}'\nargs = ['--scenario', '{}']",
            built_bin("mkrew-fakeagent").display(),
            omp_scenario.display()
        ),
    );
    assert!(omp.contains("--scenario"), "{omp}");
    env.write_profile("omp", &omp);

    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let ids: Vec<Ulid> = ["screen", "hooks", "omp"]
        .iter()
        .map(|profile| {
            let spawned: SessionSpawnResult =
                client.spawn(&project, profile).into_result().unwrap();
            spawned.session_id
        })
        .collect();
    // Sessions are created `starting` (no event); `client.events` holds every change since
    // the spawn, so the first recorded event is the transition out of `starting`.
    let mut states = record_states(&mut client, &ids);
    // ponytail: on Windows the PTY reader can deliver ConPTY's last output before the wait
    // thread reports the exit, which reads as one screen `working` right before `exited`
    // (seen on CI only). Drop that blip; every earlier transition is still checked exactly.
    for list in states.values_mut() {
        let n = list.len();
        if n >= 2 && list[n - 2] == (Working, S) && list[n - 1] == (Exited, D) {
            list.remove(n - 2);
        }
    }

    // Screen mode never reports done: only a hook can say a turn finished.
    assert_eq!(
        states[&ids[0]],
        [
            (Working, S),
            (NeedsInput, S),
            (Working, S),
            (Idle, S),
            (Exited, D)
        ]
    );
    assert_eq!(
        states[&ids[1]],
        [
            (Working, S),
            (NeedsInput, H),
            (Working, H),
            (Idle, S),
            (Done, H),
            (Exited, D)
        ]
    );
    // omp: spinner redraws are working, each quiet gap is idle; no patterns, no hooks.
    assert_eq!(
        states[&ids[2]],
        [
            (Working, S),
            (Idle, S),
            (Working, S),
            (Idle, S),
            (Exited, D)
        ]
    );
    for id in &ids {
        assert_eq!(client.session(*id).exit_code, Some(0));
    }
    client.stop();
}

/// Hooks inside the pane run the daemon's own `mk` (SPEC §6, ADR 0059).
#[test]
fn session_env_names_the_daemons_mk() {
    let env = Env::new();
    let script = if cfg!(windows) {
        "command = 'cmd.exe'\nargs = ['/c', 'echo', 'mk=[%MKREW_MK%]']"
    } else {
        "command = 'sh'\nargs = ['-c', 'echo \"mk=[$MKREW_MK]\"']"
    };
    env.write_profile("probe", &format!("name = \"Probe\"\n{script}\n"));
    let mk = env.data_dir().join("bin").join("mk.exe");
    let (_daemon, mut client) = env.start_with(&[(mkrew_daemon::ENV_MK_EXE, mk.as_path())]);
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "probe").into_result().unwrap();
    client.wait_state(spawned.session_id, SessionState::Exited);
    let log = session_log(&env, spawned.session_id);
    assert!(log.contains(&format!("mk=[{}]", mk.display())), "{log}");
    client.stop();
}

#[test]
fn session_env_cwd_and_exit_code_reach_the_store() {
    let env = Env::new();
    let script = if cfg!(windows) {
        "command = 'cmd.exe'\nargs = ['/c', 'echo', 'id=%MKREW_SESSION_ID%', 'term=%TERM%', '&', 'cd', '&', 'exit', '3']"
    } else {
        "command = 'sh'\nargs = ['-c', 'echo id=$MKREW_SESSION_ID term=$TERM; pwd; exit 3']"
    };
    env.write_profile("probe", &format!("name = \"Probe\"\n{script}\n"));
    let (_daemon, mut client) = env.start();
    let repo = env.repo();
    let project = client.add_project(&repo);

    let spawned: SessionSpawnResult = client.spawn(&project, "probe").into_result().unwrap();
    let id = spawned.session_id;
    let session = client.wait_state(id, SessionState::Exited);
    assert_eq!(session.exit_code, Some(3));

    let log = session_log(&env, id);
    assert!(log.contains(&format!("id={id}")), "{log}");
    assert!(log.contains("term=xterm-256color"), "{log}");
    let repo_name = repo.file_name().unwrap().to_string_lossy().into_owned();
    assert!(log.contains(&repo_name), "cwd missing: {log}");
    client.stop();
}

#[test]
fn file_watcher_records_changed_files_of_live_sessions_except_ignored_ones() {
    let env = Env::new();
    let repo = env.repo();
    std::fs::write(repo.join(".gitignore"), "*.log\n").unwrap();
    for dir in ["src", "target", "node_modules"] {
        std::fs::create_dir_all(repo.join(dir)).unwrap();
    }
    // Ignored writes first: once `src/lib.rs` is recorded, their batch has been handled too.
    let scenario = env.dir.path().join("writer.toml");
    let mut steps = String::new();
    for path in ["target/a.o", "node_modules/b.js", "debug.log", "src/lib.rs"] {
        steps +=
            &format!("[[step]]\naction = \"write_file\"\npath = \"{path}\"\ncontent = \"x\"\n\n");
    }
    std::fs::write(&scenario, steps + "[[step]]\naction = \"ask\"\n").unwrap();
    env.write_fake_profile_as(
        "writer",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let spawned: SessionSpawnResult = client.spawn(&project, "writer").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::NeedsInput);

    let deadline = Instant::now() + Duration::from_secs(10);
    while !changed_files(&env, id).contains("src/lib.rs") {
        assert!(Instant::now() < deadline, "src/lib.rs never recorded");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(
        changed_files(&env, id),
        BTreeSet::from(["src/lib.rs".to_owned()])
    );
    client.stop();
}

/// One watcher batch is one `files_changed` row, however many sessions are live, and
/// `doc.list` still names the changed Markdown file (ADR 0060).
#[test]
fn one_batch_is_stored_once_for_all_live_sessions() {
    let env = Env::new();
    let repo = env.repo();
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    let writer = env.dir.path().join("writer.toml");
    std::fs::write(
        &writer,
        "[[step]]\naction = \"write_file\"\npath = \"docs/plan.md\"\ncontent = \"# Plan\"\n\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    let idle = env.dir.path().join("idle.toml");
    std::fs::write(&idle, "[[step]]\naction = \"ask\"\n").unwrap();
    env.write_fake_profile_as("writer", &format!("['--scenario', '{}']", writer.display()));
    env.write_fake_profile_as("idle", &format!("['--scenario', '{}']", idle.display()));
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    for profile in ["idle", "idle"] {
        let spawned: SessionSpawnResult = client.spawn(&project, profile).into_result().unwrap();
        client.wait_state(spawned.session_id, SessionState::NeedsInput);
    }
    let spawned: SessionSpawnResult = client.spawn(&project, "writer").into_result().unwrap();
    client.wait_state(spawned.session_id, SessionState::NeedsInput);

    let rows = || {
        live_store(&env)
            .project_session_events(project.id, "files_changed", 0)
            .unwrap()
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !rows()
        .iter()
        .any(|(_, e)| e.payload["paths"].to_string().contains("docs/plan.md"))
    {
        assert!(Instant::now() < deadline, "docs/plan.md never recorded");
        std::thread::sleep(Duration::from_millis(50));
    }
    let rows = rows();
    let mut times: Vec<i64> = rows.iter().map(|(_, e)| e.at).collect();
    times.dedup();
    assert_eq!(
        times.len(),
        rows.len(),
        "a batch was stored more than once: {rows:?}"
    );

    let docs = doc_list(&mut client, &project);
    let plan = docs
        .iter()
        .find(|d| d.file_path == "docs/plan.md")
        .expect("plan listed");
    assert!(
        plan.last_session_id.is_some() && plan.last_changed_at.is_some(),
        "{plan:?}"
    );
    client.stop();
}
