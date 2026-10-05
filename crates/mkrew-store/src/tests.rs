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
    assert_eq!(listed[1], ended);
    assert_eq!(store.mark_stale_sessions_exited().unwrap(), 0);
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
fn session_requires_existing_project() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    assert!(store.insert_session(&session(Ulid::new(), 0)).is_err());
}
