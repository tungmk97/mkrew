//! Archive, remove, rename, pin, color, plans, terminal titles.

use super::*;

#[test]
fn archived_workspaces_stop_their_sessions_and_come_back_when_added_again() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    client.wait_state(spawned.session_id, SessionState::NeedsInput);

    let archive = |project_id| Request::ProjectArchive(ProjectRef { project_id });
    let archived: Project = client.call(archive(project.id)).into_result().unwrap();
    assert!(archived.archived);
    client.wait_event(&Event::ProjectUpdated(archived.clone()));
    client.wait_state(spawned.session_id, SessionState::Exited);
    // Still listed (clients hide archived projects).
    let listed: Vec<Project> = client.call(Request::ProjectList).into_result().unwrap();
    assert_eq!(listed, vec![archived]);

    // Adding the folder again brings back the same workspace.
    let back = client.add_project(&env.repo());
    assert_eq!((back.id, back.archived), (project.id, false));
    client.wait_event(&Event::ProjectUpdated(back));
    assert_eq!(
        error_code(client.call(archive(Ulid::new()))),
        ErrorCode::NotFound
    );
    client.stop();
}

/// A removed workspace's tasks are not resumed on daemon start (no hidden live agent), but
/// they are left as they were, so adding the folder again resumes them on the next start.
#[test]
fn archived_workspaces_are_not_resumed_until_added_again() {
    let env = Env::new();
    let scenario = env.dir.path().join("native.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"hook\"\nevent = \"busy\"\nsession_id = \"native-7\"\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    let fake = built_bin("mkrew-fakeagent");
    env.write_fake_profile_as(
        "resumable",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let path = env.dir.path().join("config/agents/resumable.toml");
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

    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let task: Task = client
        .call(create_task(&project, "archived", Some("resumable")))
        .into_result()
        .unwrap();
    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    let deadline = Instant::now() + TIMEOUT;
    while client.session(run.session_id).native_session_id.is_none() {
        assert!(Instant::now() < deadline, "no native session id reported");
        std::thread::sleep(Duration::from_millis(50));
    }
    client.wait_state(run.session_id, SessionState::NeedsInput);
    client
        .call(Request::ProjectArchive(ProjectRef {
            project_id: project.id,
        }))
        .into_result::<Project>()
        .unwrap();
    client.wait_state(run.session_id, SessionState::Exited);
    let before = list_tasks(&mut client, &project);
    let new_sessions = |client: &mut Client| -> Vec<Session> {
        let all: Vec<Session> = client
            .call(Request::SessionList(SessionListParams::default()))
            .into_result()
            .unwrap();
        all.into_iter()
            .filter(|s| s.task_id == Some(task.id) && s.id != run.session_id)
            .collect()
    };

    client.stop();
    assert!(daemon.wait_exit().success());
    let (mut daemon, mut client) = env.start();
    assert_eq!(new_sessions(&mut client), vec![]);
    assert_eq!(list_tasks(&mut client, &project), before);

    client.add_project(&env.repo());
    client.stop();
    assert!(daemon.wait_exit().success());
    let (_daemon, mut client) = env.start();
    assert_eq!(new_sessions(&mut client).len(), 1);
    client.stop();
}

#[test]
fn removed_sessions_stop_and_leave_the_list() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::NeedsInput);

    let remove = |session_id| Request::SessionRemove(SessionRef { session_id });
    client.call(remove(id)).into_result::<()>().unwrap();
    client.wait_event_where(|e| matches!(e, Event::SessionExited(x) if x.session_id == id));
    let listed: Vec<Session> = client
        .call(Request::SessionList(SessionListParams::default()))
        .into_result()
        .unwrap();
    assert!(listed.iter().all(|s| s.id != id), "{listed:?}");
    // Its output log stays readable.
    let history: SessionHistory = client
        .call(Request::SessionHistory(SessionRef { session_id: id }))
        .into_result()
        .unwrap();
    assert!(!history.files.is_empty());
    assert_eq!(
        error_code(client.call(remove(Ulid::new()))),
        ErrorCode::NotFound
    );
    client.stop();
}

/// `session.rename` (ADR 0031): a live session keeps the name through its own later saves
/// (state changes, exit); an exited one is renamed in the store; blank clears; run sessions
/// and `run:` titles are refused.
#[test]
fn sessions_are_renamed_live_and_exited_but_run_sessions_refuse() {
    let env = Env::new();
    env.write_fake_profile();
    let repo = env.repo();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::NeedsInput);

    let renamed: Session = client
        .call(rename(id, Some("  Fix login  ")))
        .into_result()
        .unwrap();
    assert_eq!(renamed.title.as_deref(), Some("Fix login"));
    client.wait_event_where(|e| {
        matches!(e, Event::SessionUpdated(s) if s.id == id && s.title.as_deref() == Some("Fix login"))
    });
    // The actor's final save (exit) keeps the new title.
    let kill = Request::SessionKill(SessionKillParams {
        session_id: id,
        force: true,
    });
    client.call(kill).into_result::<()>().unwrap();
    let exited = client.wait_state(id, SessionState::Exited);
    assert_eq!(exited.title.as_deref(), Some("Fix login"));

    // Exited: renamed in the store; blank clears.
    let cleared: Session = client.call(rename(id, Some("   "))).into_result().unwrap();
    assert_eq!(cleared.title, None);
    assert_eq!(client.session(id).title, None);
    client.wait_event_where(
        |e| matches!(e, Event::SessionUpdated(s) if s.id == id && s.title.is_none()),
    );
    assert_eq!(
        error_code(client.call(rename(id, Some("run:web")))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(rename(Ulid::new(), Some("x")))),
        ErrorCode::NotFound
    );

    // A run session is named by its command.
    let web = run_command("web", ".", &long("echo up", "echo up"));
    client
        .call(set_runs(&project, vec![web], None))
        .into_result::<Project>()
        .unwrap();
    let started: RunStarted = client
        .call(run_start(&project, None))
        .into_result()
        .unwrap();
    let web = started.sessions[0].session_id;
    assert_eq!(
        error_code(client.call(rename(web, Some("mine")))),
        ErrorCode::InvalidState
    );
    client
        .call(run_stop(&project, None))
        .into_result::<()>()
        .unwrap();
    client.wait_state(web, SessionState::Exited);
    client.stop();
}

/// `session.link_plan` (ADR 0039): the checklist is announced at once and after each
/// change of the file; a change that keeps the checklist sends nothing; a deleted file reports
/// an error; unlinking stops the watch. Bad links are refused and change nothing.
#[test]
fn a_linked_plan_reports_its_checklist_until_it_is_unlinked() {
    let env = Env::new();
    env.write_fake_profile();
    let repo = env.repo();
    let plan = repo.join("PLAN.md");
    std::fs::write(&plan, "# Plan\n- [x] a\n- [ ] b\n").unwrap();
    std::fs::write(repo.join("notes.txt"), "- [ ] x\n").unwrap();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::NeedsInput);

    for bad in ["notes.txt", "missing.md"] {
        assert_eq!(
            error_code(client.call(link_plan(id, Some(bad)))),
            ErrorCode::InvalidRequest,
            "{bad}"
        );
    }
    assert_eq!(
        error_code(client.call(link_plan(Ulid::new(), Some("PLAN.md")))),
        ErrorCode::NotFound
    );
    assert_eq!(client.session(id).plan_path, None);

    let linked: Session = client
        .call(link_plan(id, Some(plan.to_str().unwrap())))
        .into_result()
        .unwrap();
    assert_eq!(
        linked.plan_path.as_deref(),
        Some("PLAN.md"),
        "inside the project: stored relative"
    );
    assert_eq!(client.session(id).plan_path.as_deref(), Some("PLAN.md"));
    let counts = |done: u32, total: u32| {
        move |e: &Event| {
            matches!(e, Event::SessionPlan(p) if p.session_id == id
                && (p.done, p.total) == (done, total) && p.error.is_none())
        }
    };
    let Event::SessionPlan(first) = client.wait_event_where(counts(1, 2)) else {
        unreachable!()
    };
    assert_eq!(first.name, "PLAN");
    assert_eq!(
        first.items,
        [
            PlanItem {
                done: true,
                text: "a".into()
            },
            PlanItem {
                done: false,
                text: "b".into()
            }
        ]
    );
    let listed = |client: &mut Client| -> Vec<SessionPlanEvent> {
        client
            .call(Request::SessionPlan(ProjectRef {
                project_id: project.id,
            }))
            .into_result()
            .unwrap()
    };
    assert_eq!(listed(&mut client), vec![first]);

    // A prose-only edit keeps the checklist: nothing is announced (wait out the debounce).
    std::fs::write(&plan, "# Plan, renamed\n- [x] a\n- [ ] b\n").unwrap();
    std::thread::sleep(Duration::from_millis(1200));
    std::fs::write(&plan, "# Plan, renamed\n- [x] a\n- [x] b\n").unwrap();
    client.wait_event_where(counts(2, 2));
    let announced = client
        .events
        .iter()
        .filter(|e| matches!(e, Event::SessionPlan(p) if p.session_id == id))
        .count();
    assert_eq!(
        announced, 2,
        "the link and the edit; the prose edit sent nothing"
    );

    std::fs::remove_file(&plan).unwrap();
    let Event::SessionPlan(gone) = client.wait_event_where(
        |e| matches!(e, Event::SessionPlan(p) if p.session_id == id && p.error.is_some()),
    ) else {
        unreachable!()
    };
    assert_eq!((gone.total, gone.items.len()), (0, 0));

    let unlinked: Session = client.call(link_plan(id, None)).into_result().unwrap();
    assert_eq!(unlinked.plan_path, None);
    assert!(listed(&mut client).is_empty());
    let seen = client.events.len();
    std::fs::write(&plan, "- [ ] a\n- [ ] b\n- [ ] c\n").unwrap();
    std::thread::sleep(Duration::from_millis(1200));
    client.session(id); // reads what the daemon pushed meanwhile
    assert!(
        !client.events[seen..]
            .iter()
            .any(|e| matches!(e, Event::SessionPlan(_))),
        "no watch after unlink"
    );
    client.stop();
}

/// `session.pin` (SPEC §7): like a rename, a live session keeps its pin through its own later
/// saves (state changes, exit); an exited one is pinned in the store; both are announced.
#[test]
fn sessions_are_pinned_live_and_exited_and_keep_the_pin_through_saves() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::NeedsInput);
    assert!(!client.session(id).pinned);

    let pinned: Session = client.call(pin(id, true)).into_result().unwrap();
    assert!(pinned.pinned);
    client.wait_event_where(|e| matches!(e, Event::SessionUpdated(s) if s.id == id && s.pinned));
    // The actor's final save (exit) keeps the pin.
    let kill = Request::SessionKill(SessionKillParams {
        session_id: id,
        force: true,
    });
    client.call(kill).into_result::<()>().unwrap();
    let exited = client.wait_state(id, SessionState::Exited);
    assert!(exited.pinned);

    // Exited: unpinned in the store.
    let unpinned: Session = client.call(pin(id, false)).into_result().unwrap();
    assert!(!unpinned.pinned);
    assert!(!client.session(id).pinned);
    client.wait_event_where(|e| matches!(e, Event::SessionUpdated(s) if s.id == id && !s.pinned));
    assert_eq!(
        error_code(client.call(pin(Ulid::new(), true))),
        ErrorCode::NotFound
    );
    client.stop();
}

/// `session.move` (SPEC §7): a live session keeps its position through its own later saves
/// (exit), an exited one is moved in the store; both are announced. A non-finite position
/// cannot be sent (JSON has no NaN/inf, the frame is malformed); `session_move` refuses it anyway.
#[test]
fn sessions_are_moved_live_and_exited_and_the_position_persists() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::NeedsInput);
    assert_eq!(client.session(id).position, 0.0);

    let moved: Session = client.call(move_to(id, 1.5)).into_result().unwrap();
    assert_eq!(moved.position, 1.5);
    client.wait_event_where(
        |e| matches!(e, Event::SessionUpdated(s) if s.id == id && s.position == 1.5),
    );
    assert_eq!(client.session(id).position, 1.5);
    // The actor's final save (exit) keeps the position.
    let kill = Request::SessionKill(SessionKillParams {
        session_id: id,
        force: true,
    });
    client.call(kill).into_result::<()>().unwrap();
    assert_eq!(client.wait_state(id, SessionState::Exited).position, 1.5);

    // Exited: moved in the store.
    let again: Session = client.call(move_to(id, -3.0)).into_result().unwrap();
    assert_eq!(again.position, -3.0);
    assert_eq!(client.session(id).position, -3.0);
    client.wait_event_where(
        |e| matches!(e, Event::SessionUpdated(s) if s.id == id && s.position == -3.0),
    );
    assert_eq!(
        error_code(client.call(move_to(Ulid::new(), 1.0))),
        ErrorCode::NotFound
    );
    client.stop();
}

/// `tab.pin` and `project.update` `pinned` / `position`: stored, announced, `position`
/// untouched by a pin, kept by other changes, and still there after a daemon restart.
#[test]
fn workspace_and_tab_pins_persist_across_daemon_restart() {
    let env = Env::new();
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let _first: TerminalTab = client
        .call(create_tab(&project, "one"))
        .into_result()
        .unwrap();
    let second: TerminalTab = client
        .call(create_tab(&project, "two"))
        .into_result()
        .unwrap();
    let tab_pin = |tab: &TerminalTab, pinned| {
        Request::TabPin(TabPinParams {
            tab_id: tab.id,
            pinned,
        })
    };
    let pin_project = |pinned| {
        Request::ProjectUpdate(ProjectUpdateParams {
            project_id: project.id,
            jira_keys: None,
            position: None,
            name: None,
            default_agent: None,
            default_shell: None,
            run_commands: None,
            run_autostart: None,
            color: None,
            pinned: Some(pinned),
        })
    };

    let pinned: TerminalTab = client.call(tab_pin(&second, true)).into_result().unwrap();
    assert!(pinned.pinned);
    assert_eq!(
        pinned.position, second.position,
        "pinning never moves a tab"
    );
    client.wait_event(&Event::TabUpdated(pinned));
    let unknown = Request::TabPin(TabPinParams {
        tab_id: Ulid::new(),
        pinned: true,
    });
    assert_eq!(error_code(client.call(unknown)), ErrorCode::NotFound);

    let saved: Project = client.call(pin_project(true)).into_result().unwrap();
    assert!(saved.pinned);
    client.wait_event(&Event::ProjectUpdated(saved));
    // Another field changes, the pin stays.
    let other: Project = client
        .call(set_default_agent(&project, None))
        .into_result()
        .unwrap();
    assert!(other.pinned);
    // A sidebar drag: announced, kept by the pin.
    let moved = Request::ProjectUpdate(ProjectUpdateParams {
        project_id: project.id,
        jira_keys: None,
        position: Some(3),
        name: None,
        default_agent: None,
        default_shell: None,
        run_commands: None,
        run_autostart: None,
        color: None,
        pinned: None,
    });
    let moved: Project = client.call(moved).into_result().unwrap();
    assert_eq!((moved.position, moved.pinned), (3, true));
    client.wait_event(&Event::ProjectUpdated(moved));

    client.stop();
    assert!(daemon.wait_exit().success());
    let (_daemon, mut client) = env.start();
    let listed: Vec<Project> = client.call(Request::ProjectList).into_result().unwrap();
    assert!(listed[0].pinned);
    assert_eq!(listed[0].position, 3);
    let tabs = list_tabs(&mut client, &project);
    let tabs: Vec<(&str, bool)> = tabs.iter().map(|t| (t.title.as_str(), t.pinned)).collect();
    assert_eq!(tabs, [("one", false), ("two", true)]);

    let unpinned: TerminalTab = client.call(tab_pin(&second, false)).into_result().unwrap();
    assert!(!unpinned.pinned);
    assert_eq!(
        unpinned.position, second.position,
        "unpinning keeps the position"
    );
    let unpinned: Project = client.call(pin_project(false)).into_result().unwrap();
    assert!(!unpinned.pinned);
    client.stop();
}

/// `session.set_color` (ADR 0037): a live session keeps its color through its own later
/// saves (state changes, exit); an exited one is colored in the store; `null` clears; a name
/// outside the palette is refused; run sessions may be colored.
#[test]
fn sessions_are_colored_live_and_exited_and_the_name_is_checked() {
    let env = Env::new();
    env.write_fake_profile();
    let repo = env.repo();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::NeedsInput);

    let colored: Session = client
        .call(set_color(id, Some("teal")))
        .into_result()
        .unwrap();
    assert_eq!(colored.color.as_deref(), Some("teal"));
    client.wait_event_where(|e| {
        matches!(e, Event::SessionUpdated(s) if s.id == id && s.color.as_deref() == Some("teal"))
    });
    // The actor's final save (exit) keeps the color.
    let kill = Request::SessionKill(SessionKillParams {
        session_id: id,
        force: true,
    });
    client.call(kill).into_result::<()>().unwrap();
    let exited = client.wait_state(id, SessionState::Exited);
    assert_eq!(exited.color.as_deref(), Some("teal"));

    // Exited: colored in the store; null clears.
    let red: Session = client
        .call(set_color(id, Some("red")))
        .into_result()
        .unwrap();
    assert_eq!(red.color.as_deref(), Some("red"));
    assert_eq!(client.session(id).color.as_deref(), Some("red"));
    let cleared: Session = client.call(set_color(id, None)).into_result().unwrap();
    assert_eq!(cleared.color, None);
    assert_eq!(client.session(id).color, None);
    client.wait_event_where(
        |e| matches!(e, Event::SessionUpdated(s) if s.id == id && s.color.is_none()),
    );

    assert_eq!(
        error_code(client.call(set_color(id, Some("#ff0000")))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(set_color(Ulid::new(), Some("red")))),
        ErrorCode::NotFound
    );

    // A run session is named by its command, but its color is free.
    let web = run_command("web", ".", &long("echo up", "echo up"));
    client
        .call(set_runs(&project, vec![web], None))
        .into_result::<Project>()
        .unwrap();
    let started: RunStarted = client
        .call(run_start(&project, None))
        .into_result()
        .unwrap();
    let web = started.sessions[0].session_id;
    let blue: Session = client
        .call(set_color(web, Some("blue")))
        .into_result()
        .unwrap();
    assert_eq!(blue.color.as_deref(), Some("blue"));
    client
        .call(run_stop(&project, None))
        .into_result::<()>()
        .unwrap();
    client.wait_state(web, SessionState::Exited);
    client.stop();
}

/// An agent's OSC title names its session (ADR 0031): cleaned, announced once per change,
/// listed with the session; a user title is kept beside it.
#[test]
fn agent_terminal_titles_name_the_session_once_per_change() {
    let env = Env::new();
    let scenario = env.dir.path().join("titled.toml");
    // Two spinner frames of one title in separate batches (the sleeps keep the daemon from
    // merging them), then a real change, then the prompt.
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"print\"\ntext = \"\\u001b]0;\\u2733 Fix login\\u0007\"\n\n\
         [[step]]\naction = \"sleep\"\nms = 300\n\n\
         [[step]]\naction = \"print\"\ntext = \"\\u001b]0;\\u2736 Fix login\\u0007\"\n\n\
         [[step]]\naction = \"sleep\"\nms = 300\n\n\
         [[step]]\naction = \"print\"\ntext = \"\\u001b]0;\\u2736 Fix tests\\u0007\"\n\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    env.write_fake_profile_as(
        "titled",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "titled").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::NeedsInput);
    // ConPTY may announce its own initial title: only the cleaned ones are looked at.
    let ours = |e: &Event| match e {
        Event::SessionTerminalTitle(t) if t.session_id == id => {
            t.title.clone().filter(|t| t.starts_with("Fix "))
        }
        _ => None,
    };
    client.wait_event_where(|e| ours(e).as_deref() == Some("Fix tests"));
    let announced: Vec<String> = client.events.iter().filter_map(ours).collect();
    assert_eq!(announced, ["Fix login", "Fix tests"]);
    assert_eq!(
        client.session(id).terminal_title.as_deref(),
        Some("Fix tests")
    );

    // The user's name is stored beside it (clients show it first).
    let renamed: Session = client.call(rename(id, Some("Mine"))).into_result().unwrap();
    assert_eq!(
        (renamed.title.as_deref(), renamed.terminal_title.as_deref()),
        (Some("Mine"), Some("Fix tests"))
    );
    assert_eq!(client.events.iter().filter_map(ours).count(), 2);
    client.stop();
}

#[test]
fn workspace_colors_are_set_cleared_and_checked() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let color = |color: Option<&str>| {
        Request::ProjectUpdate(ProjectUpdateParams {
            project_id: project.id,
            jira_keys: None,
            position: None,
            name: None,
            default_agent: None,
            default_shell: None,
            run_commands: None,
            run_autostart: None,
            color: Some(color.map(Into::into)),
            pinned: None,
        })
    };
    let teal: Project = client.call(color(Some("teal"))).into_result().unwrap();
    assert_eq!(teal.color.as_deref(), Some("teal"));
    client.wait_event(&Event::ProjectUpdated(teal));
    // Every palette name is accepted, including `lime` (added in 2026-10-10).
    for name in WORKSPACE_COLORS {
        let set: Project = client.call(color(Some(name))).into_result().unwrap();
        assert_eq!(set.color.as_deref(), Some(name));
    }
    assert_eq!(
        error_code(client.call(color(Some("#ff0000")))),
        ErrorCode::InvalidRequest
    );
    let cleared: Project = client.call(color(None)).into_result().unwrap();
    assert_eq!(cleared.color, None);
    client.stop();
}

#[test]
fn jira_keys_are_normalised_saved_and_cleared() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    assert!(project.jira_keys.is_empty());
    let update = |keys: Vec<&str>| {
        Request::ProjectUpdate(ProjectUpdateParams {
            project_id: project.id,
            name: None,
            default_agent: None,
            default_shell: None,
            run_commands: None,
            run_autostart: None,
            color: None,
            pinned: None,
            position: None,
            jira_keys: Some(keys.into_iter().map(str::to_owned).collect()),
        })
    };
    let saved: Project = client
        .call(update(vec![" proj ", "Bill", "PROJ"]))
        .into_result()
        .unwrap();
    assert_eq!(saved.jira_keys, ["PROJ", "BILL"]);
    client.wait_event(&Event::ProjectUpdated(saved));
    assert_eq!(
        error_code(client.call(update(vec!["PR-1"]))),
        ErrorCode::InvalidRequest
    );
    let cleared: Project = client.call(update(Vec::new())).into_result().unwrap();
    assert!(cleared.jira_keys.is_empty());
    client.stop();
}
