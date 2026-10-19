use super::*;

const DOC: CommentContext = CommentContext::Doc;

fn open(dir: &tempfile::TempDir) -> Store {
    Store::open(&dir.path().join("mkrew.db")).unwrap()
}

fn session(project_id: Ulid, started_at: i64) -> Session {
    Session {
        id: Ulid::new(),
        task_id: None,
        project_id,
        agent_profile: "fake".into(),
        kind: SessionKind::Agent,
        title: None,
        terminal_title: None,
        color: None,
        native_session_id: None,
        state: SessionState::Starting,
        state_source: StateSource::Daemon,
        state_changed_at: started_at,
        exit_code: None,
        cols: 120,
        rows: 40,
        started_at,
        ended_at: None,
        pinned: false,
        position: 0.0,
        plan_path: None,
    }
}

#[test]
fn open_runs_migrations_and_enables_wal() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    assert_eq!(store.schema_version().unwrap(), MIGRATIONS.len());
    let mode: String = store
        .conn
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    // Reopening is a no-op migration-wise.
    drop(store);
    assert_eq!(open(&dir).schema_version().unwrap(), MIGRATIONS.len());
}

#[test]
fn newer_schema_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    store
        .conn
        .pragma_update(None, "user_version", MIGRATIONS.len() as i64 + 1)
        .unwrap();
    drop(store);
    let err = Store::open(&dir.path().join("mkrew.db")).err().unwrap();
    assert!(matches!(err, StoreError::SchemaTooNew { .. }), "{err}");
}

#[test]
fn backup_to_writes_a_readable_copy() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    let store = open(&dir);
    let project = store.add_project(&repo, Some("repo")).unwrap();
    let copy = dir.path().join("copy.db");
    store.backup_to(&copy).unwrap();
    let restored = Store::open(&copy).unwrap();
    assert_eq!(restored.list_projects().unwrap(), vec![project]);
    // An existing copy is never overwritten.
    assert!(store.backup_to(&copy).is_err());
}

#[test]
fn projects_persist_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("my-repo");
    std::fs::create_dir(&repo).unwrap();

    let added = open(&dir).add_project(&repo, None).unwrap();
    assert_eq!(added.name, "my-repo");
    assert_eq!(
        Path::new(&added.path),
        dunce::canonicalize(&repo).unwrap().as_path()
    );
    assert!(!added.path.starts_with(r"\\?\"));

    let named = open(&dir).add_project(dir.path(), Some("Root")).unwrap();
    assert_eq!(named.name, "Root");

    let listed = open(&dir).list_projects().unwrap();
    assert_eq!(listed, vec![added, named]);
}

#[test]
fn duplicate_project_path_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    let store = open(&dir);
    store.add_project(&repo, None).unwrap();

    // Same directory spelled differently still normalizes to one path.
    let err = store
        .add_project(&repo.join("..").join("repo"), Some("again"))
        .unwrap_err();
    assert!(matches!(err, StoreError::DuplicateProject(_)), "{err}");

    #[cfg(windows)]
    {
        let upper = PathBuf::from(repo.to_string_lossy().to_uppercase());
        let err = store.add_project(&upper, None).unwrap_err();
        assert!(matches!(err, StoreError::DuplicateProject(_)), "{err}");
    }
    assert_eq!(store.list_projects().unwrap().len(), 1);
}

#[test]
fn project_path_must_be_an_existing_directory() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let missing = store.add_project(&dir.path().join("nope"), None);
    assert!(matches!(missing, Err(StoreError::Path { .. })));

    let file = dir.path().join("file.txt");
    std::fs::write(&file, "x").unwrap();
    let not_dir = store.add_project(&file, None);
    assert!(matches!(not_dir, Err(StoreError::NotADirectory(_))));
}

#[test]
fn sessions_insert_update_list() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let a = store.add_project(dir.path(), None).unwrap();
    let repo_b = dir.path().join("b");
    std::fs::create_dir(&repo_b).unwrap();
    let b = store.add_project(&repo_b, None).unwrap();

    let mut s1 = session(a.id, 1);
    let s2 = Session {
        kind: SessionKind::Shell,
        title: Some("shell".into()),
        ..session(b.id, 2)
    };
    store.insert_session(&s1).unwrap();
    store.insert_session(&s2).unwrap();

    s1.state = SessionState::Exited;
    s1.exit_code = Some(3);
    s1.ended_at = Some(10);
    s1.native_session_id = Some("native".into());
    s1.state_source = StateSource::Hook;
    s1.state_changed_at = 9;
    s1.cols = 80;
    store.update_session(&s1).unwrap();

    assert_eq!(
        store.list_sessions(None).unwrap(),
        vec![s1.clone(), s2.clone()]
    );
    assert_eq!(store.list_sessions(Some(a.id)).unwrap(), vec![s1]);
    assert_eq!(store.list_sessions(Some(b.id)).unwrap(), vec![s2]);
}

/// A session's color is stored, changed by `update_session` (the actor's every save writes it,
/// so a live session keeps it) and cleared with `None`.
#[test]
fn session_color_is_stored_updated_and_cleared() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let mut s = Session {
        color: Some("teal".into()),
        ..session(p.id, 1)
    };
    store.insert_session(&s).unwrap();
    assert_eq!(
        store.get_session(s.id).unwrap().color.as_deref(),
        Some("teal")
    );

    s.color = Some("red".into());
    store.update_session(&s).unwrap();
    assert_eq!(
        store.list_sessions(None).unwrap()[0].color.as_deref(),
        Some("red")
    );

    s.color = None;
    store.update_session(&s).unwrap();
    assert_eq!(store.get_session(s.id).unwrap().color, None);
}

#[test]
fn every_session_state_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let states = [
        SessionState::Starting,
        SessionState::Working,
        SessionState::NeedsInput,
        SessionState::Idle,
        SessionState::Done,
        SessionState::Exited,
        SessionState::Unknown,
    ];
    for (i, state) in states.into_iter().enumerate() {
        let s = Session {
            state,
            ..session(p.id, i as i64)
        };
        store.insert_session(&s).unwrap();
    }
    let listed: Vec<_> = store
        .list_sessions(None)
        .unwrap()
        .into_iter()
        .map(|s| s.state)
        .collect();
    assert_eq!(listed, states);
}

#[test]
fn interrupted_sessions_are_taken_once() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let live = Session {
        state: SessionState::Working,
        ..session(p.id, 1)
    };
    let ended = Session {
        state: SessionState::Exited,
        exit_code: Some(0),
        ended_at: Some(5),
        ..session(p.id, 2)
    };
    let stopped = Session {
        state: SessionState::Exited,
        ended_at: Some(6),
        ..session(p.id, 3)
    };
    for s in [&live, &ended, &stopped] {
        store.insert_session(s).unwrap();
    }
    store.mark_interrupted(stopped.id).unwrap();
    // A reboot left `live` running: stale rows are interrupted too.
    assert_eq!(store.mark_stale_sessions_exited().unwrap(), 1);

    let mut taken = store.take_interrupted_sessions().unwrap();
    taken.sort();
    let mut expected = vec![live.id, stopped.id];
    expected.sort();
    assert_eq!(
        taken, expected,
        "the session that ended by itself is not resumed"
    );
    assert!(store.take_interrupted_sessions().unwrap().is_empty());
}

#[test]
fn stale_sessions_are_marked_exited() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let live = Session {
        state: SessionState::Working,
        ..session(p.id, 1)
    };
    let ended = Session {
        state: SessionState::Exited,
        exit_code: Some(0),
        ended_at: Some(5),
        ..session(p.id, 2)
    };
    store.insert_session(&live).unwrap();
    store.insert_session(&ended).unwrap();

    assert_eq!(store.mark_stale_sessions_exited().unwrap(), 1);
    let listed = store.list_sessions(None).unwrap();
    assert_eq!(listed[0].state, SessionState::Exited);
    assert!(listed[0].ended_at.is_some());
    assert_eq!(listed[0].state_source, StateSource::Daemon);
    assert_eq!(listed[0].state_changed_at, listed[0].ended_at.unwrap());
    assert_eq!(listed[1], ended);
    assert_eq!(store.mark_stale_sessions_exited().unwrap(), 0);
}

#[test]
fn exited_sessions_stay_listable_per_project_after_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let (a_dir, b_dir) = (dir.path().join("a"), dir.path().join("b"));
    std::fs::create_dir(&a_dir).unwrap();
    std::fs::create_dir(&b_dir).unwrap();
    let store = open(&dir);
    let a = store.add_project(&a_dir, None).unwrap();
    let b = store.add_project(&b_dir, None).unwrap();
    let ran = Session {
        state: SessionState::Working,
        ..session(a.id, 1)
    };
    store.insert_session(&ran).unwrap();
    store.insert_session(&session(b.id, 2)).unwrap();
    store.mark_stale_sessions_exited().unwrap();
    drop(store);

    let store = open(&dir);
    let listed = store.list_sessions(Some(a.id)).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, ran.id);
    assert_eq!(listed[0].state, SessionState::Exited);
    assert_eq!(store.get_session(ran.id).unwrap(), listed[0]);
    let unknown = Ulid::new();
    assert!(matches!(
        store.get_session(unknown),
        Err(StoreError::SessionNotFound(id)) if id == unknown
    ));
}

#[test]
fn session_events_are_stored_per_session_and_kind() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let (a, b) = (session(p.id, 1), session(p.id, 2));
    store.insert_session(&a).unwrap();
    store.insert_session(&b).unwrap();
    let first = serde_json::json!({"event": "Notification", "payload": {"session_id": "x"}});
    let second = serde_json::json!({"event": "Stop"});
    store.insert_session_event(a.id, "hook", &first, 5).unwrap();
    store
        .insert_session_event(b.id, "hook", &second, 6)
        .unwrap();
    store
        .insert_session_event(a.id, "hook", &second, 7)
        .unwrap();
    store
        .insert_session_event(a.id, "state_changed", &second, 8)
        .unwrap();

    let events = store.session_events(a.id, "hook").unwrap();
    assert_eq!(
        events,
        vec![
            SessionEvent {
                payload: first,
                at: 5
            },
            SessionEvent {
                payload: second,
                at: 7
            },
        ]
    );
}

#[test]
fn state_columns_backfill_existing_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mkrew.db");
    {
        // A database at schema 2 with one ended session.
        let conn = Connection::open(&path).unwrap();
        for sql in &MIGRATIONS[..2] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 2).unwrap();
        let project = Ulid::new().to_string();
        conn.execute(
            "INSERT INTO project (id, name, path, created_at) VALUES (?1, 'p', '/p', 0)",
            [&project],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, project_id, agent_profile, state, cols, rows, started_at,
                ended_at)
             VALUES (?1, ?2, 'fake', 'exited', 80, 24, 3, 9)",
            [Ulid::new().to_string(), project],
        )
        .unwrap();
    }
    let listed = Store::open(&path).unwrap().list_sessions(None).unwrap();
    assert_eq!(listed[0].state_source, StateSource::Daemon);
    assert_eq!(listed[0].state_changed_at, 9);
}

#[test]
fn update_unknown_session_errors() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let err = store.update_session(&session(p.id, 0)).unwrap_err();
    assert!(matches!(err, StoreError::SessionNotFound(_)), "{err}");
}

#[test]
fn project_update_sets_and_clears_fields() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();

    let set = store
        .update_project(
            p.id,
            ProjectChanges {
                name: Some("renamed"),
                default_agent: Some(Some("claude")),
                ..ProjectChanges::default()
            },
        )
        .unwrap();
    assert_eq!(set.name, "renamed");
    assert_eq!(set.default_agent.as_deref(), Some("claude"));
    // Untouched fields stay; the change persists.
    let kept = store
        .update_project(p.id, ProjectChanges::default())
        .unwrap();
    assert_eq!(kept, set);
    assert_eq!(open(&dir).get_project(p.id).unwrap(), set);

    let cleared = store
        .update_project(
            p.id,
            ProjectChanges {
                default_agent: Some(None),
                ..ProjectChanges::default()
            },
        )
        .unwrap();
    assert_eq!(cleared.default_agent, None);
    assert_eq!(cleared.name, "renamed");

    let missing = store.update_project(
        Ulid::new(),
        ProjectChanges {
            name: Some("x"),
            ..ProjectChanges::default()
        },
    );
    assert!(matches!(missing, Err(StoreError::ProjectNotFound(_))));
}

fn run(name: &str) -> RunCommand {
    RunCommand {
        name: name.into(),
        dir: "api".into(),
        command: "uv run main.py".into(),
        env_file: Some(".env".into()),
        env: vec![("ENVIRONMENT".into(), "dev".into())],
        exit_after: false,
    }
}

#[test]
fn run_commands_are_saved_in_order_and_replaced_as_a_whole() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    assert!(p.run_commands.is_empty() && !p.run_autostart);

    let two = [
        run("BE"),
        RunCommand {
            exit_after: true,
            ..run("FE")
        },
    ];
    let saved = store
        .update_project(
            p.id,
            ProjectChanges {
                run_commands: Some(&two),
                run_autostart: Some(true),
                ..ProjectChanges::default()
            },
        )
        .unwrap();
    assert_eq!(saved.run_commands, two);
    assert!(saved.run_autostart);
    assert_eq!(open(&dir).get_project(p.id).unwrap(), saved);
    assert_eq!(
        open(&dir).list_projects().unwrap(),
        std::slice::from_ref(&saved)
    );

    // Leaving the list out keeps it; a new list replaces it.
    let renamed = store
        .update_project(
            p.id,
            ProjectChanges {
                name: Some("x"),
                ..ProjectChanges::default()
            },
        )
        .unwrap();
    assert_eq!(renamed.run_commands, two);
    let one = [run("FE")];
    let replaced = store
        .update_project(
            p.id,
            ProjectChanges {
                run_commands: Some(&one),
                ..ProjectChanges::default()
            },
        )
        .unwrap();
    assert_eq!(open(&dir).get_project(p.id).unwrap().run_commands, one);
    assert!(replaced.run_autostart);
}

#[test]
fn the_dev_command_becomes_a_run_command_named_dev() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mkrew.db");
    let (with, without) = (Ulid::new(), Ulid::new());
    {
        let conn = Connection::open(&path).unwrap();
        for sql in &MIGRATIONS[..10] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 10).unwrap();
        conn.execute(
            "INSERT INTO project (id, name, path, dev_command, created_at, dev_autostart)
             VALUES (?1, 'a', '/a', 'npm run dev', 0, 1)",
            [with.to_string()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO project (id, name, path, created_at) VALUES (?1, 'b', '/b', 1)",
            [without.to_string()],
        )
        .unwrap();
    }
    let store = Store::open(&path).unwrap();
    let a = store.get_project(with).unwrap();
    assert_eq!(
        a.run_commands,
        [RunCommand {
            name: "dev".into(),
            dir: ".".into(),
            command: "npm run dev".into(),
            env_file: None,
            env: Vec::new(),
            exit_after: false,
        }]
    );
    assert!(a.run_autostart);
    let b = store.get_project(without).unwrap();
    assert!(b.run_commands.is_empty() && !b.run_autostart);
}

#[test]
fn env_files_move_from_the_folder_to_the_workspace_root() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mkrew.db");
    let project = Ulid::new();
    {
        let conn = Connection::open(&path).unwrap();
        for sql in &MIGRATIONS[..17] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 17).unwrap();
        conn.execute(
            "INSERT INTO project (id, name, path, created_at) VALUES (?1, 'ea', '/ea', 0)",
            [project.to_string()],
        )
        .unwrap();
        let rows = [
            ("root", ".", Some(".env")),
            ("be", "./EA-BE", Some(".env")),
            ("win", r".\EA-FE\", Some(r"config\.env")),
            ("none", "web", None),
            ("api", "api", Some(".env")),
            ("dot", "./", Some(".env")),
        ];
        for (position, (name, folder, env_file)) in rows.into_iter().enumerate() {
            conn.execute(
                "INSERT INTO run_command (project_id, position, name, dir, command, env_file)
                 VALUES (?1, ?2, ?3, ?4, 'x', ?5)",
                params![project.to_string(), position as i64, name, folder, env_file],
            )
            .unwrap();
        }
    }
    let store = Store::open(&path).unwrap();
    let files: Vec<Option<String>> = store
        .get_project(project)
        .unwrap()
        .run_commands
        .into_iter()
        .map(|c| c.env_file)
        .collect();
    assert_eq!(
        files,
        [
            Some(".env".to_owned()),
            Some("./EA-BE/.env".to_owned()),
            Some(r".\EA-FE/config\.env".to_owned()),
            None,
            Some("api/.env".to_owned()),
            Some(".env".to_owned()),
        ]
    );
}

#[test]
fn project_base_is_set_kept_and_cleared_to_auto() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    assert_eq!(p.base_ref, None);

    let set = store.set_project_base(p.id, Some("develop")).unwrap();
    assert_eq!(set.base_ref.as_deref(), Some("develop"));
    assert_eq!(open(&dir).get_project(p.id).unwrap(), set);
    // Other updates keep the base.
    let renamed = store
        .update_project(
            p.id,
            ProjectChanges {
                name: Some("renamed"),
                ..ProjectChanges::default()
            },
        )
        .unwrap();
    assert_eq!(renamed.base_ref.as_deref(), Some("develop"));

    let auto = store.set_project_base(p.id, None).unwrap();
    assert_eq!(auto.base_ref, None);
    assert_eq!(store.list_projects().unwrap(), vec![auto]);

    let missing = store.set_project_base(Ulid::new(), Some("main"));
    assert!(matches!(missing, Err(StoreError::ProjectNotFound(_))));
}

#[test]
fn comment_contexts_persist_and_replies_inherit_them() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    store
        .create_comment(p.id, "a.md", CommentContext::Doc, 1, 1, "quote", "body")
        .unwrap();
    let context: String = store
        .conn
        .query_row("SELECT context FROM doc_comment", [], |row| row.get(0))
        .unwrap();
    assert_eq!(context, "doc");

    let diff = store
        .create_comment(
            p.id,
            "src/lib.rs",
            CommentContext::Diff,
            3,
            4,
            "fn a()",
            "Rename",
        )
        .unwrap();
    let reply = store.reply_comment(diff.id, "claude", "Done").unwrap();
    assert_eq!(reply.context, CommentContext::Diff);
    drop(store);
    let store = open(&dir);
    let listed = store.list_comments(p.id, Some("src/lib.rs")).unwrap();
    assert_eq!(listed, vec![diff, reply]);
}

#[test]
fn tabs_are_ordered_edited_and_persist() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let a = store.add_project(dir.path(), None).unwrap();
    let repo_b = dir.path().join("b");
    std::fs::create_dir(&repo_b).unwrap();
    let b = store.add_project(&repo_b, None).unwrap();

    let first = store.create_tab(a.id, "one", None).unwrap();
    let second = store.create_tab(a.id, "two", None).unwrap();
    let other = store.create_tab(b.id, "b", None).unwrap();
    assert_eq!((first.position, second.position), (0.0, 1.0));
    assert_eq!(other.position, 0.0);
    assert_eq!(first.layout, serde_json::Value::Null);
    let front = store.create_tab(a.id, "front", Some(-1.0)).unwrap();

    let titles = |store: &Store| -> Vec<String> {
        let tabs = store.list_tabs(a.id).unwrap();
        tabs.into_iter().map(|t| t.title).collect()
    };
    assert_eq!(titles(&store), ["front", "one", "two"]);

    store.move_tab(front.id, 5.0).unwrap();
    let renamed = store.rename_tab(first.id, "uno").unwrap();
    assert_eq!(renamed.title, "uno");
    let layout = serde_json::json!({"split": "vertical", "ratio": 0.5, "children": [
        {"session": "01JA"}, {"session": "01JB"}
    ]});
    let laid_out = store.set_tab_layout(second.id, &layout).unwrap();
    assert_eq!(laid_out.layout, layout);

    store.close_tab(renamed.id).unwrap();
    let reopened = open(&dir);
    assert_eq!(titles(&reopened), ["two", "front"]);
    assert_eq!(reopened.get_tab(second.id).unwrap(), laid_out);
    assert_eq!(reopened.list_tabs(b.id).unwrap(), vec![other]);

    assert!(matches!(
        store.close_tab(first.id),
        Err(StoreError::TabNotFound(_))
    ));
    assert!(matches!(
        store.rename_tab(Ulid::new(), "x"),
        Err(StoreError::TabNotFound(_))
    ));
    assert!(matches!(
        store.create_tab(Ulid::new(), "x", None),
        Err(StoreError::ProjectNotFound(_))
    ));
}

#[test]
fn tasks_are_created_edited_moved_and_persist() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let a = store.add_project(dir.path(), None).unwrap();
    let repo_b = dir.path().join("b");
    std::fs::create_dir(&repo_b).unwrap();
    let b = store.add_project(&repo_b, None).unwrap();

    let first = store
        .create_task(a.id, "one", "", None, None, false)
        .unwrap();
    let second = store
        .create_task(a.id, "two", "do it", Some("claude"), None, true)
        .unwrap();
    let other = store.create_task(b.id, "b", "", None, None, false).unwrap();
    assert_eq!((first.position, second.position), (0.0, 1.0));
    assert_eq!(other.position, 0.0);
    assert_eq!(first.status, TaskStatus::Todo);
    assert_eq!(first.agent_profile, None);
    assert_eq!(second.agent_profile.as_deref(), Some("claude"));
    assert!(!first.error_flag);
    let front = store
        .create_task(a.id, "front", "", None, Some(-1.0), false)
        .unwrap();

    let titles = |store: &Store, status: Option<TaskStatus>| -> Vec<String> {
        let tasks = store.list_tasks(Some(a.id), status).unwrap();
        tasks.into_iter().map(|t| t.title).collect()
    };
    assert_eq!(titles(&store, None), ["front", "one", "two"]);

    // Fractional drop between "one" and "two" in another column, then back in Todo.
    let moved = store.move_task(front.id, TaskStatus::Review, 0.5).unwrap();
    assert_eq!((moved.status, moved.position), (TaskStatus::Review, 0.5));
    assert!(moved.updated_at >= front.updated_at);
    assert_eq!(titles(&store, Some(TaskStatus::Todo)), ["one", "two"]);
    assert_eq!(titles(&store, Some(TaskStatus::Review)), ["front"]);
    // A new Todo card goes after the last Todo card, ignoring other columns.
    let third = store
        .create_task(a.id, "three", "", None, None, false)
        .unwrap();
    assert_eq!(third.position, 2.0);

    let edited = store
        .update_task(second.id, Some("deux"), None, Some(None), None)
        .unwrap();
    assert_eq!(edited.title, "deux");
    assert!(edited.worktree);
    assert_eq!(edited.prompt, "do it");
    assert_eq!(edited.agent_profile, None);
    let edited = store
        .update_task(second.id, None, Some(""), Some(Some("codex")), Some(false))
        .unwrap();
    assert!(!edited.worktree);
    assert_eq!(
        (edited.title.as_str(), edited.prompt.as_str()),
        ("deux", "")
    );

    let reopened = open(&dir);
    assert_eq!(titles(&reopened, None), ["one", "front", "deux", "three"]);
    assert_eq!(reopened.get_task(second.id).unwrap(), edited);
    assert_eq!(reopened.list_tasks(Some(b.id), None).unwrap(), vec![other]);
    assert_eq!(reopened.list_tasks(None, None).unwrap().len(), 5);

    assert!(matches!(
        store.move_task(Ulid::new(), TaskStatus::Done, 0.0),
        Err(StoreError::TaskNotFound(_))
    ));
    assert!(matches!(
        store.update_task(Ulid::new(), Some("x"), None, None, None),
        Err(StoreError::TaskNotFound(_))
    ));
    assert!(matches!(
        store.create_task(Ulid::new(), "x", "", None, None, false),
        Err(StoreError::ProjectNotFound(_))
    ));
}

#[test]
fn run_lifecycle_sets_status_and_error_flag_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let task = store
        .create_task(p.id, "t", "", None, Some(3.5), false)
        .unwrap();
    let failed = store
        .set_task_status(task.id, TaskStatus::NeedsInput, true)
        .unwrap();
    assert_eq!(
        (failed.status, failed.error_flag, failed.position),
        (TaskStatus::NeedsInput, true, 3.5)
    );
    let rerun = store
        .set_task_status(task.id, TaskStatus::Running, false)
        .unwrap();
    assert!(!rerun.error_flag);
    assert_eq!(open(&dir).get_task(task.id).unwrap(), rerun);
    assert!(matches!(
        store.set_task_status(Ulid::new(), TaskStatus::Done, false),
        Err(StoreError::TaskNotFound(_))
    ));
}

#[test]
fn checkpoint_is_set_and_cleared() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let task = store.create_task(p.id, "t", "", None, None, false).unwrap();
    let head = "0123456789abcdef0123456789abcdef01234567";
    assert_eq!(store.task_checkpoint_at(task.id).unwrap(), None);
    let set = store
        .set_task_checkpoint(task.id, Some(head), 1_234)
        .unwrap();
    assert_eq!(set.checkpoint.as_deref(), Some(head));
    assert_eq!((set.status, set.position), (task.status, task.position));
    assert_eq!(open(&dir).get_task(task.id).unwrap(), set);
    assert_eq!(store.task_checkpoint_at(task.id).unwrap(), Some(1_234));
    let cleared = store.set_task_checkpoint(task.id, None, 5_678).unwrap();
    assert_eq!(cleared.checkpoint, None);
    assert_eq!(store.task_checkpoint_at(task.id).unwrap(), None);
    assert!(matches!(
        store.set_task_checkpoint(Ulid::new(), None, 0),
        Err(StoreError::TaskNotFound(_))
    ));
    assert!(matches!(
        store.task_checkpoint_at(Ulid::new()),
        Err(StoreError::TaskNotFound(_))
    ));
}

#[test]
fn worktree_and_fanout_fields_persist() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let parent = store
        .create_task(p.id, "fix", "", None, None, false)
        .unwrap();
    assert!(!parent.worktree);
    assert_eq!(
        (parent.worktree_path.clone(), parent.branch.clone()),
        (None, None)
    );
    assert_eq!(parent.fanout_of, None);

    let child = store
        .create_task(p.id, "fix · a", "", None, None, true)
        .unwrap();
    let other = store
        .create_task(p.id, "fix · b", "", None, None, true)
        .unwrap();
    let set = store
        .set_task_worktree(child.id, Some(("/wt/p/t", "mkrew/fix-01jx")))
        .unwrap();
    assert_eq!(set.worktree_path.as_deref(), Some("/wt/p/t"));
    assert_eq!(set.branch.as_deref(), Some("mkrew/fix-01jx"));
    let child = store.set_task_fanout_of(child.id, parent.id).unwrap();
    assert_eq!(child.fanout_of, Some(parent.id));
    let other = store.set_task_fanout_of(other.id, parent.id).unwrap();

    let reopened = open(&dir);
    assert_eq!(reopened.get_task(child.id).unwrap(), child);
    assert_eq!(
        reopened.list_fanout_children(parent.id).unwrap(),
        vec![child.clone(), other.clone()]
    );
    assert!(reopened.list_fanout_children(child.id).unwrap().is_empty());

    let removed = store.set_task_worktree(child.id, None).unwrap();
    assert_eq!((removed.worktree_path, removed.branch), (None, None));
    assert!(removed.worktree);
    assert!(matches!(
        store.set_task_worktree(Ulid::new(), None),
        Err(StoreError::TaskNotFound(_))
    ));
    assert!(matches!(
        store.set_task_fanout_of(child.id, Ulid::new()),
        Err(StoreError::TaskNotFound(_))
    ));

    // Deleting the original keeps its children as ordinary tasks.
    store.delete_task(parent.id).unwrap();
    assert_eq!(store.get_task(other.id).unwrap().fanout_of, None);
}

#[test]
fn delete_task_refuses_live_sessions_and_detaches_exited_ones() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let task = store.create_task(p.id, "t", "", None, None, false).unwrap();
    let mut old = session(p.id, 1);
    old.task_id = Some(task.id);
    old.state = SessionState::Exited;
    store.insert_session(&old).unwrap();
    let mut live = session(p.id, 2);
    live.task_id = Some(task.id);
    live.state = SessionState::NeedsInput;
    store.insert_session(&live).unwrap();

    assert!(matches!(
        store.delete_task(task.id),
        Err(StoreError::TaskHasLiveSession(_))
    ));
    assert_eq!(store.get_task(task.id).unwrap(), task);

    live.state = SessionState::Exited;
    store.update_session(&live).unwrap();
    assert_eq!(store.delete_task(task.id).unwrap(), task);
    assert!(matches!(
        store.get_task(task.id),
        Err(StoreError::TaskNotFound(_))
    ));
    let sessions = store.list_sessions(Some(p.id)).unwrap();
    assert_eq!(sessions.len(), 2);
    assert!(sessions.iter().all(|s| s.task_id.is_none()));
    assert!(matches!(
        store.delete_task(task.id),
        Err(StoreError::TaskNotFound(_))
    ));
}

#[test]
fn comment_threads_are_created_replied_and_persist() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    let store = open(&dir);
    let p = store.add_project(&repo, None).unwrap();

    let first = store
        .create_comment(p.id, "docs/plan.md", DOC, 12, 14, "Use Redis", "Why Redis?")
        .unwrap();
    assert_eq!(first.author, DocComment::USER);
    assert_eq!(first.status, CommentStatus::Open);
    assert_eq!((first.start_line, first.end_line), (Some(12), Some(14)));
    assert_eq!(first.quote.as_deref(), Some("Use Redis"));
    let other = store
        .create_comment(p.id, "README.md", DOC, 1, 1, "# Demo", "Rename")
        .unwrap();

    let reply = store
        .reply_comment(first.id, "claude", "Dropped it")
        .unwrap();
    assert_eq!(reply.parent_id, Some(first.id));
    assert_eq!(reply.file_path, "docs/plan.md");
    assert_eq!((reply.start_line, reply.quote.as_deref()), (None, None));
    // A reply to a reply joins the root's thread.
    let nested = store.reply_comment(reply.id, "user", "Thanks").unwrap();
    assert_eq!(nested.parent_id, Some(first.id));

    drop(store);
    let store = open(&dir);
    assert_eq!(
        store.list_comments(p.id, Some("docs/plan.md")).unwrap(),
        vec![first.clone(), reply, nested]
    );
    assert_eq!(store.list_comments(p.id, None).unwrap().len(), 4);
    assert_eq!(
        store.list_comments(p.id, Some("README.md")).unwrap(),
        vec![other]
    );
    assert!(store.list_comments(Ulid::new(), None).unwrap().is_empty());
    assert_eq!(store.get_comment(first.id).unwrap(), first);
}

#[test]
fn comment_status_and_sent_fields_update() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    let store = open(&dir);
    let p = store.add_project(&repo, None).unwrap();
    let c = store
        .create_comment(p.id, "plan.md", DOC, 3, 3, "x", "fix")
        .unwrap();
    let s = session(p.id, 1);
    store.insert_session(&s).unwrap();

    let sent = store.mark_comment_sent(c.id, s.id, "# Plan\nx\n").unwrap();
    assert_eq!(sent.status, CommentStatus::Sent);
    assert_eq!(sent.sent_to, Some(s.id));
    assert_eq!(sent.sent_snapshot.as_deref(), Some("# Plan\nx\n"));
    assert!(sent.updated_at >= c.updated_at);

    for status in [
        CommentStatus::Addressed,
        CommentStatus::Resolved,
        CommentStatus::Outdated,
        CommentStatus::Open,
    ] {
        let updated = store.set_comment_status(c.id, status).unwrap();
        assert_eq!(updated.status, status);
        // Status changes keep the send record ("View changes" still works).
        assert_eq!(updated.sent_to, Some(s.id));
    }

    let moved = store
        .set_comment_anchor(c.id, 5, 6, CommentStatus::Outdated)
        .unwrap();
    assert_eq!((moved.start_line, moved.end_line), (Some(5), Some(6)));
    assert_eq!(moved.status, CommentStatus::Outdated);
    assert_eq!(moved.quote.as_deref(), Some("x"));
    assert!(matches!(
        store.set_comment_anchor(CommentId::new(), 1, 1, CommentStatus::Open),
        Err(StoreError::CommentNotFound(_))
    ));
}

#[test]
fn project_session_events_span_sessions_since_a_time() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let other_dir = dir.path().join("other");
    std::fs::create_dir(&other_dir).unwrap();
    let other = store.add_project(&other_dir, None).unwrap();
    let (a, b, c) = (session(p.id, 1), session(p.id, 2), session(other.id, 3));
    for s in [&a, &b, &c] {
        store.insert_session(s).unwrap();
    }
    let paths = |p: &str| serde_json::json!({ "paths": [p] });
    store
        .insert_session_event(a.id, "files_changed", &paths("old.md"), 5)
        .unwrap();
    store
        .insert_session_event(b.id, "files_changed", &paths("b.md"), 20)
        .unwrap();
    store
        .insert_session_event(a.id, "files_changed", &paths("a.md"), 10)
        .unwrap();
    store
        .insert_session_event(a.id, "hook", &paths("hook.md"), 11)
        .unwrap();
    store
        .insert_session_event(c.id, "files_changed", &paths("c.md"), 12)
        .unwrap();

    let events = store
        .project_session_events(p.id, "files_changed", 10)
        .unwrap();
    let got: Vec<(Ulid, i64)> = events.iter().map(|(s, e)| (*s, e.at)).collect();
    assert_eq!(got, vec![(a.id, 10), (b.id, 20)]);
    assert_eq!(events[0].1.payload, paths("a.md"));
}

#[test]
fn turns_start_end_list_and_persist() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let (a, b) = (session(p.id, 1), session(p.id, 2));
    store.insert_session(&a).unwrap();
    store.insert_session(&b).unwrap();

    let t1 = store.start_turn(a.id, 1, 100, Some("tree1")).unwrap();
    assert_eq!(
        t1,
        Turn {
            n: 1,
            started_at: 100,
            ended_at: None,
            start_tree: Some("tree1".into()),
            end_tree: None,
            files: vec![],
        }
    );
    let files = vec![
        TurnFile {
            path: "src/a.rs".into(),
            added: Some(3),
            removed: Some(1),
        },
        TurnFile {
            path: "b.md".into(),
            added: None,
            removed: None,
        },
    ];
    let ended = store.end_turn(a.id, 1, 200, Some("tree2"), &files).unwrap();
    assert_eq!(ended.ended_at, Some(200));
    assert_eq!(ended.end_tree.as_deref(), Some("tree2"));
    assert_eq!(ended.files, files);
    store.start_turn(a.id, 2, 300, Some("tree2")).unwrap();
    // Outside a git repo: no trees.
    store.start_turn(b.id, 1, 150, None).unwrap();
    let b1 = store.end_turn(b.id, 1, 160, None, &[]).unwrap();
    assert_eq!((b1.start_tree, b1.end_tree), (None, None));

    drop(store);
    let store = open(&dir);
    let turns = store.list_turns(a.id).unwrap();
    assert_eq!(turns.iter().map(|t| t.n).collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(turns[0], ended);
    assert_eq!(turns[1].ended_at, None);
    assert_eq!(store.list_turns(b.id).unwrap().len(), 1);
    assert!(store.list_turns(Ulid::new()).unwrap().is_empty());

    // Ended turns of the project's sessions only; the running one has no end yet.
    std::fs::create_dir(dir.path().join("other")).unwrap();
    let other = store.add_project(&dir.path().join("other"), None).unwrap();
    let c = session(other.id, 3);
    store.insert_session(&c).unwrap();
    store.start_turn(c.id, 1, 100, None).unwrap();
    store.end_turn(c.id, 1, 300, None, &files).unwrap();
    let since = |at| {
        let turns = store.project_turns(p.id, at).unwrap();
        turns.iter().map(|(s, t)| (*s, t.n)).collect::<Vec<_>>()
    };
    assert_eq!(since(0), vec![(b.id, 1), (a.id, 1)]);
    assert_eq!(since(161), vec![(a.id, 1)]);
    assert_eq!(since(201), vec![]);
    assert_eq!(store.project_turns(p.id, 0).unwrap()[1].1, ended);
}

#[test]
fn turn_errors() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let s = session(p.id, 1);
    store.insert_session(&s).unwrap();
    let missing = Ulid::new();
    assert!(matches!(
        store.start_turn(missing, 1, 1, None),
        Err(StoreError::SessionNotFound(id)) if id == missing
    ));
    assert!(matches!(
        store.end_turn(s.id, 1, 2, None, &[]),
        Err(StoreError::TurnNotFound { session_id, n: 1 }) if session_id == s.id
    ));
    assert!(matches!(
        store.get_turn(s.id, 7),
        Err(StoreError::TurnNotFound { n: 7, .. })
    ));
    store.start_turn(s.id, 1, 1, None).unwrap();
    // A turn number is recorded once.
    assert!(store.start_turn(s.id, 1, 5, None).is_err());
}

#[test]
fn expired_turn_sessions_are_listed_until_their_trees_are_forgotten() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let ended = |started_at: i64, ended_at: Option<i64>| {
        let mut s = session(p.id, started_at);
        s.ended_at = ended_at;
        store.insert_session(&s).unwrap();
        s
    };
    let old = ended(1, Some(10));
    let old_no_git = ended(2, Some(10));
    let _old_no_turns = ended(3, Some(10));
    let recent = ended(4, Some(1_000));
    let live = ended(5, None);
    store.start_turn(old.id, 1, 5, Some("t")).unwrap();
    store.start_turn(old_no_git.id, 1, 5, None).unwrap();
    store.start_turn(recent.id, 1, 5, Some("t")).unwrap();
    store.start_turn(live.id, 1, 5, Some("t")).unwrap();

    assert_eq!(
        store.sessions_with_expired_turns(500).unwrap(),
        vec![(old.id, p.id)]
    );
    store.forget_turn_trees(old.id).unwrap();
    assert!(store.sessions_with_expired_turns(500).unwrap().is_empty());
    let kept = store.list_turns(old.id).unwrap();
    assert_eq!((kept.len(), kept[0].start_tree.clone()), (1, None));
}

#[test]
fn comment_errors() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let missing = CommentId::new();
    assert!(matches!(
        store.get_comment(missing),
        Err(StoreError::CommentNotFound(id)) if id == missing
    ));
    assert!(matches!(
        store.reply_comment(missing, "user", "hi"),
        Err(StoreError::CommentNotFound(_))
    ));
    assert!(matches!(
        store.set_comment_status(missing, CommentStatus::Resolved),
        Err(StoreError::CommentNotFound(_))
    ));
    assert!(matches!(
        store.create_comment(Ulid::new(), "a.md", DOC, 1, 1, "", "x"),
        Err(StoreError::ProjectNotFound(_))
    ));

    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    let p = store.add_project(&repo, None).unwrap();
    let s = session(p.id, 1);
    store.insert_session(&s).unwrap();
    assert!(matches!(
        store.mark_comment_sent(missing, s.id, ""),
        Err(StoreError::CommentNotFound(_))
    ));
    let c = store
        .create_comment(p.id, "a.md", DOC, 1, 1, "", "x")
        .unwrap();
    assert!(matches!(
        store.mark_comment_sent(c.id, Ulid::new(), ""),
        Err(StoreError::SessionNotFound(_))
    ));
    assert_eq!(store.get_comment(c.id).unwrap(), c);
}

#[test]
fn session_requires_existing_project() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    assert!(store.insert_session(&session(Ulid::new(), 0)).is_err());
}

#[test]
fn archived_projects_come_back_when_their_folder_is_added_again() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), Some("demo")).unwrap();
    let task = store.create_task(p.id, "t", "", None, None, false).unwrap();

    let archived = store.archive_project(p.id).unwrap();
    assert!(archived.archived);
    assert!(store.get_project(p.id).unwrap().archived);
    // Adding the folder again un-archives the same project: settings and tasks stay.
    let back = store.add_project(dir.path(), None).unwrap();
    assert_eq!(
        (back.id, back.name.as_str(), back.archived),
        (p.id, "demo", false)
    );
    assert_eq!(store.get_task(task.id).unwrap(), task);
    // Only an archived one comes back; an active one is still a duplicate.
    assert!(matches!(
        store.add_project(dir.path(), None),
        Err(StoreError::DuplicateProject(_))
    ));
    assert!(matches!(
        store.archive_project(Ulid::new()),
        Err(StoreError::ProjectNotFound(_))
    ));
}

#[test]
fn hidden_sessions_leave_the_list_but_keep_their_history() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let mut gone = session(p.id, 1);
    gone.state = SessionState::Exited;
    store.insert_session(&gone).unwrap();
    let kept = session(p.id, 2);
    store.insert_session(&kept).unwrap();
    store.start_turn(gone.id, 1, 1, None).unwrap();

    store.hide_session(gone.id).unwrap();
    assert_eq!(store.list_sessions(Some(p.id)).unwrap(), vec![kept.clone()]);
    assert_eq!(store.list_sessions(None).unwrap(), vec![kept]);
    assert_eq!(store.get_session(gone.id).unwrap(), gone);
    assert_eq!(store.list_turns(gone.id).unwrap().len(), 1);
    assert!(matches!(
        store.hide_session(Ulid::new()),
        Err(StoreError::SessionNotFound(_))
    ));
}

/// A session's linked plan (ADR 0039) is stored with it, survives a reopen, and follows
/// the session's own saves (the actor owns the row): changed, then cleared.
#[test]
fn a_linked_plan_is_saved_with_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let mut linked = session(p.id, 1);
    linked.plan_path = Some("docs/PLAN.md".into());
    store.insert_session(&linked).unwrap();
    let plain = session(p.id, 2);
    store.insert_session(&plain).unwrap();
    drop(store);

    let store = open(&dir);
    assert_eq!(
        store.get_session(linked.id).unwrap().plan_path.as_deref(),
        Some("docs/PLAN.md")
    );
    assert_eq!(
        store.list_sessions(Some(p.id)).unwrap(),
        vec![linked.clone(), plain]
    );

    linked.plan_path = Some("/elsewhere/other.md".into());
    linked.state = SessionState::Idle;
    store.update_session(&linked).unwrap();
    assert_eq!(store.get_session(linked.id).unwrap(), linked);
    linked.plan_path = None;
    store.update_session(&linked).unwrap();
    assert_eq!(store.get_session(linked.id).unwrap().plan_path, None);
}

#[test]
fn project_color_persists_and_clears() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    assert_eq!(p.color, None);
    let teal = ProjectChanges {
        color: Some(Some("teal")),
        ..ProjectChanges::default()
    };
    store.update_project(p.id, teal).unwrap();
    assert_eq!(
        open(&dir).get_project(p.id).unwrap().color.as_deref(),
        Some("teal")
    );
    let none = ProjectChanges {
        color: Some(None),
        ..ProjectChanges::default()
    };
    store.update_project(p.id, none).unwrap();
    assert_eq!(open(&dir).get_project(p.id).unwrap().color, None);
}

#[test]
fn pinned_persists_for_workspaces_sessions_and_tabs() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    assert!(!p.pinned);

    // Workspace: another change keeps the pin.
    let pin = ProjectChanges {
        pinned: Some(true),
        ..ProjectChanges::default()
    };
    assert!(store.update_project(p.id, pin).unwrap().pinned);
    let teal = ProjectChanges {
        color: Some(Some("teal")),
        ..ProjectChanges::default()
    };
    store.update_project(p.id, teal).unwrap();
    assert!(open(&dir).get_project(p.id).unwrap().pinned);

    // Session: inserted pinned; a save that does not touch the pin keeps it; update_session
    // is what a live session's task writes, so it must carry the flag both ways.
    let mut s = Session {
        pinned: true,
        ..session(p.id, 1)
    };
    store.insert_session(&s).unwrap();
    assert!(open(&dir).get_session(s.id).unwrap().pinned);
    s.state = SessionState::Working;
    store.update_session(&s).unwrap();
    assert!(store.get_session(s.id).unwrap().pinned);
    s.pinned = false;
    store.update_session(&s).unwrap();
    assert!(!open(&dir).list_sessions(None).unwrap()[0].pinned);

    // Tab: pinning leaves the position, so unpinning returns it to its place.
    let tab = store.create_tab(p.id, "one", None).unwrap();
    assert!(!tab.pinned);
    let pinned = store.set_tab_pinned(tab.id, true).unwrap();
    assert!(pinned.pinned);
    assert_eq!(pinned.position, tab.position);
    assert_eq!(open(&dir).list_tabs(p.id).unwrap(), vec![pinned]);
    assert!(matches!(
        store.set_tab_pinned(Ulid::new(), true),
        Err(StoreError::TabNotFound(_))
    ));
}

#[test]
fn session_position_persists_through_insert_and_update() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let mut s = Session {
        position: 1.5,
        ..session(p.id, 1)
    };
    store.insert_session(&s).unwrap();
    assert_eq!(open(&dir).get_session(s.id).unwrap().position, 1.5);
    s.position = -2.25;
    store.update_session(&s).unwrap();
    assert_eq!(open(&dir).list_sessions(None).unwrap()[0].position, -2.25);
}

#[test]
fn project_default_shell_persists_and_clears() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    assert_eq!(p.default_shell, None);
    let pwsh = ProjectChanges {
        default_shell: Some(Some("pwsh")),
        ..ProjectChanges::default()
    };
    let updated = store.update_project(p.id, pwsh).unwrap();
    assert_eq!(updated.default_agent, None, "only the shell changes");
    assert_eq!(
        open(&dir)
            .get_project(p.id)
            .unwrap()
            .default_shell
            .as_deref(),
        Some("pwsh")
    );
    // Changing another field keeps it.
    let agent = ProjectChanges {
        default_agent: Some(Some("claude")),
        ..ProjectChanges::default()
    };
    store.update_project(p.id, agent).unwrap();
    assert_eq!(
        open(&dir)
            .get_project(p.id)
            .unwrap()
            .default_shell
            .as_deref(),
        Some("pwsh")
    );
    let none = ProjectChanges {
        default_shell: Some(None),
        ..ProjectChanges::default()
    };
    store.update_project(p.id, none).unwrap();
    assert_eq!(open(&dir).get_project(p.id).unwrap().default_shell, None);
}

#[test]
fn sessions_ended_before_lists_ended_sessions_hidden_or_not() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let ended = |started, ended_at| Session {
        ended_at,
        ..session(p.id, started)
    };
    let (old, old_hidden, recent, live) = (
        ended(1, Some(10)),
        ended(2, Some(20)),
        ended(3, Some(500)),
        ended(4, None),
    );
    for s in [&old, &old_hidden, &recent, &live] {
        store.insert_session(s).unwrap();
    }
    store.hide_session(old_hidden.id).unwrap();
    let mut got = store.sessions_ended_before(100).unwrap();
    got.sort();
    let mut want = vec![old.id, old_hidden.id];
    want.sort();
    assert_eq!(got, want);
}

#[test]
fn prune_session_events_drops_only_old_rows_of_the_given_kinds() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let s = session(p.id, 1);
    store.insert_session(&s).unwrap();
    let v = serde_json::json!({});
    for (kind, at) in [
        ("files_changed", 5),
        ("hook", 6),
        ("usage", 7),
        ("files_changed", 200),
    ] {
        store.insert_session_event(s.id, kind, &v, at).unwrap();
    }
    assert_eq!(
        store
            .prune_session_events(100, &["files_changed", "hook"])
            .unwrap(),
        2
    );
    assert_eq!(
        store.session_events(s.id, "usage").unwrap().len(),
        1,
        "usage kept"
    );
    let left = store.session_events(s.id, "files_changed").unwrap();
    assert_eq!(left.iter().map(|e| e.at).collect::<Vec<_>>(), [200]);
    assert!(store.session_events(s.id, "hook").unwrap().is_empty());
}

#[test]
fn vacuum_if_sparse_shrinks_the_file_only_when_mostly_free() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let s = session(p.id, 1);
    store.insert_session(&s).unwrap();
    let big = serde_json::json!({ "paths": ["x".repeat(2000)] });
    for at in 0..2000 {
        store
            .insert_session_event(s.id, "files_changed", &big, at)
            .unwrap();
    }
    assert!(!store.vacuum_if_sparse().unwrap(), "nothing free yet");
    store
        .prune_session_events(i64::MAX, &["files_changed"])
        .unwrap();
    store
        .conn
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    let before = std::fs::metadata(dir.path().join("mkrew.db"))
        .unwrap()
        .len();
    assert!(store.vacuum_if_sparse().unwrap());
    // WAL mode: the VACUUM output must reach the main file without a manual checkpoint.
    let wal = std::fs::metadata(dir.path().join("mkrew.db-wal")).map_or(0, |m| m.len());
    assert!(wal * 4 < before, "wal {wal} vs db {before}");
    let after = std::fs::metadata(dir.path().join("mkrew.db"))
        .unwrap()
        .len();
    assert!(after * 4 < before, "{before} -> {after}");
    assert!(!store.vacuum_if_sparse().unwrap(), "compact now");
}
