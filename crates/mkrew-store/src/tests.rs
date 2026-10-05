use super::*;

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
        native_session_id: None,
        state: SessionState::Starting,
        state_source: StateSource::Daemon,
        state_changed_at: started_at,
        exit_code: None,
        cols: 120,
        rows: 40,
        started_at,
        ended_at: None,
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
        .update_project(p.id, Some("renamed"), Some(Some("claude")))
        .unwrap();
    assert_eq!(set.name, "renamed");
    assert_eq!(set.default_agent.as_deref(), Some("claude"));
    // Untouched fields stay; the change persists.
    let kept = store.update_project(p.id, None, None).unwrap();
    assert_eq!(kept, set);
    assert_eq!(open(&dir).get_project(p.id).unwrap(), set);

    let cleared = store.update_project(p.id, None, Some(None)).unwrap();
    assert_eq!(cleared.default_agent, None);
    assert_eq!(cleared.name, "renamed");

    let missing = store.update_project(Ulid::new(), Some("x"), None);
    assert!(matches!(missing, Err(StoreError::ProjectNotFound(_))));
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

    let first = store.create_task(a.id, "one", "", None, None).unwrap();
    let second = store
        .create_task(a.id, "two", "do it", Some("claude"), None)
        .unwrap();
    let other = store.create_task(b.id, "b", "", None, None).unwrap();
    assert_eq!((first.position, second.position), (0.0, 1.0));
    assert_eq!(other.position, 0.0);
    assert_eq!(first.status, TaskStatus::Todo);
    assert_eq!(first.agent_profile, None);
    assert_eq!(second.agent_profile.as_deref(), Some("claude"));
    assert!(!first.error_flag);
    let front = store
        .create_task(a.id, "front", "", None, Some(-1.0))
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
    let third = store.create_task(a.id, "three", "", None, None).unwrap();
    assert_eq!(third.position, 2.0);

    let edited = store
        .update_task(second.id, Some("deux"), None, Some(None))
        .unwrap();
    assert_eq!(edited.title, "deux");
    assert_eq!(edited.prompt, "do it");
    assert_eq!(edited.agent_profile, None);
    let edited = store
        .update_task(second.id, None, Some(""), Some(Some("codex")))
        .unwrap();
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
        store.update_task(Ulid::new(), Some("x"), None, None),
        Err(StoreError::TaskNotFound(_))
    ));
    assert!(matches!(
        store.create_task(Ulid::new(), "x", "", None, None),
        Err(StoreError::ProjectNotFound(_))
    ));
}

#[test]
fn run_lifecycle_sets_status_and_error_flag_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let task = store.create_task(p.id, "t", "", None, Some(3.5)).unwrap();
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
    let task = store.create_task(p.id, "t", "", None, None).unwrap();
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
fn delete_task_refuses_live_sessions_and_detaches_exited_ones() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let task = store.create_task(p.id, "t", "", None, None).unwrap();
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
fn session_requires_existing_project() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    assert!(store.insert_session(&session(Ulid::new(), 0)).is_err());
}
