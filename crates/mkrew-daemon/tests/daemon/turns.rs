//! Turns: edits, token usage, snapshots, restore, hook-attributed diffs.

use super::*;

/// A fake agent edits two files (one twice, with hooks) and writes a third (no hook) in one
/// turn; at `done`, `session.turn` carries each path once, relative, sorted (SPEC §20A.3).
#[test]
fn a_turn_that_edits_two_files_reports_both() {
    let env = Env::new();
    let scenario = env.dir.path().join("editor.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"hook\"\nevent = \"busy\"\n\n\
         [[step]]\naction = \"write_file\"\npath = \"notes.txt\"\ncontent = \"x\"\n\n\
         [[step]]\naction = \"edit_files\"\npaths = [\"src/b.rs\", \"a.rs\", \"a.rs\"]\n\n\
         [[step]]\naction = \"ask\"\ntext = \"go\"\n\n\
         [[step]]\naction = \"hook\"\nevent = \"done\"\n\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    env.write_fake_profile_as(
        "editor",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    // Hooks decide the states: a slow `mk hook` run must not look like a quiet (idle) period.
    // Spawning `mk` prints nothing visible, so that wait no longer counts as work (ADR 0015).
    let path = env.dir.path().join("config/agents/editor.toml");
    let profile = std::fs::read_to_string(&path).unwrap();
    let hooks = profile
        .replace("mode = \"screen\"", "mode = \"hook+screen\"")
        .replace("idle_after_ms = 1000", "idle_after_ms = 8000");
    assert_ne!(hooks, profile);
    env.write_profile("editor", &hooks);
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "editor").into_result().unwrap();
    let id = spawned.session_id;
    answer_after_watcher_saw_notes(&env, &mut client, id);

    let event =
        client.wait_event_where(|e| matches!(e, Event::SessionTurn(t) if t.session_id == id));
    let Event::SessionTurn(turn) = event else {
        unreachable!()
    };
    assert_eq!(turn.n, 1);
    assert!(turn.started_at <= turn.ended_at, "{turn:?}");
    // The turn ended when the `done` hook arrived.
    client.wait_event_where(|e| {
        matches!(e, Event::SessionState(s) if s.session_id == id
            && s.state == SessionState::Done && s.changed_at == turn.ended_at)
    });
    let paths: Vec<&str> = turn.files.iter().map(|f| f.path.as_str()).collect();
    // `notes.txt` was written without a hook: only the file watcher saw it.
    assert_eq!(paths, ["a.rs", "notes.txt", "src/b.rs"]);
    assert!(
        turn.files
            .iter()
            .all(|f| f.added.is_none() && f.removed.is_none())
    );
    client.stop();
}

/// A profile with `[usage]`: at the turn end the daemon finds the transcript of the agent's
/// native session id, announces `session.usage` and `session.usage` lists it (SPEC §20E.3).
#[test]
fn a_turn_end_reads_token_usage_from_the_agent_transcript() {
    let env = Env::new();
    let transcripts = env.dir.path().join("claude/projects/C--repo");
    std::fs::create_dir_all(&transcripts).unwrap();
    std::fs::write(
        transcripts.join("native-7.jsonl"),
        include_bytes!("../../../mkrew-core/tests/fixtures/usage/claude.jsonl"),
    )
    .unwrap();
    let scenario = env.dir.path().join("usage.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"hook\"\nevent = \"busy\"\nsession_id = \"native-7\"\n\n\
         [[step]]\naction = \"sleep\"\nms = 300\n\n\
         [[step]]\naction = \"hook\"\nevent = \"done\"\nsession_id = \"native-7\"\n\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    env.write_fake_profile_as(
        "usage",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let path = env.dir.path().join("config/agents/usage.toml");
    let profile = std::fs::read_to_string(&path).unwrap();
    let hooks = profile.replace("mode = \"screen\"", "mode = \"hook+screen\"");
    assert_ne!(hooks, profile);
    env.write_profile(
        "usage",
        &format!("{hooks}\n[usage]\nformat = \"claude_jsonl\"\ncontext_window = 200000\n"),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "usage").into_result().unwrap();
    let id = spawned.session_id;

    let event =
        client.wait_event_where(|e| matches!(e, Event::SessionUsage(u) if u.session_id == id));
    let Event::SessionUsage(usage) = event else {
        unreachable!()
    };
    // The located file (`Path` equality ignores the `/` the test's `join` mixed in).
    let located = usage.transcript.as_deref().expect("a located transcript");
    assert_eq!(
        std::path::Path::new(located),
        transcripts.join("native-7.jsonl")
    );
    // The totals of `mkrew-core`'s Claude fixture: cache split out of `input` (ADR 0035),
    // the context as before, the model of the newest main-thread message.
    let expected = SessionUsageEvent {
        session_id: id,
        input: 2 + 3 + 1,
        output: 206 + 40 + 512,
        cache_read: 48215 + 9000 + 48903,
        cache_write: 688 + 1200,
        context: 1 + 1200 + 48903,
        context_window: Some(200_000),
        model: Some("claude-opus-5".into()),
        transcript: usage.transcript.clone(),
    };
    assert_eq!(usage, expected);
    let listed: Vec<SessionUsageEvent> = client
        .call(Request::SessionUsage(ProjectRef {
            project_id: project.id,
        }))
        .into_result()
        .unwrap();
    assert_eq!(listed, [expected]);
    let unknown = client.call(Request::SessionUsage(ProjectRef {
        project_id: Ulid::new(),
    }));
    assert_eq!(error_code(unknown), ErrorCode::NotFound);
    client.stop();
}

/// `session.usage` lists an event stored before the cache split (no cache fields, no model)
/// as it was: both cache fields 0, no model, the old `input` (SPEC §20E.3, ADR 0035).
#[test]
fn session_usage_lists_events_stored_before_the_cache_split() {
    let env = Env::new();
    std::fs::create_dir_all(env.data_dir()).unwrap();
    let session_id = Ulid::new();
    let project_id = {
        let store = Store::open(&env.data_dir().join("mkrew.db")).unwrap();
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
                state: SessionState::Exited,
                state_source: StateSource::Daemon,
                state_changed_at: 1,
                exit_code: None,
                cols: 80,
                rows: 24,
                started_at: 1,
                ended_at: Some(2),
                pinned: false,
                position: 0.0,
                plan_path: None,
            })
            .unwrap();
        let old = json!({
            "session_id": session_id, "input": 1_000, "output": 20, "context": 900,
            "context_window": 200_000
        });
        store
            .insert_session_event(session_id, "usage", &old, 10)
            .unwrap();
        project.id
    };
    let (_daemon, mut client) = env.start();
    let listed: Vec<SessionUsageEvent> = client
        .call(Request::SessionUsage(ProjectRef { project_id }))
        .into_result()
        .unwrap();
    assert_eq!(
        listed,
        [SessionUsageEvent {
            session_id,
            input: 1_000,
            output: 20,
            cache_read: 0,
            cache_write: 0,
            context: 900,
            context_window: Some(200_000),
            model: None,
            transcript: None,
        }]
    );
    client.stop();
}

/// In a git repo the `hook.report` that starts a turn is answered after the start snapshot:
/// the agent's first write (right after its hook returns) is not in it. The turn's files get
/// line counts from start to end snapshot; branch and index stay as they were (SPEC §20C.1).
#[test]
fn a_hook_turn_start_is_snapshotted_before_the_agent_edits() {
    let env = Env::new();
    env.enable_turns();
    let repo = env.repo();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let head = git(&repo, &["rev-parse", "HEAD"]);
    let scenario = env.dir.path().join("editor.toml");
    // The agent's start output is screen-detected as turn 1; the `done` hook ends it, so the
    // `busy` hook starts turn 2.
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"hook\"\nevent = \"done\"\n\n\
         [[step]]\naction = \"hook\"\nevent = \"busy\"\n\n\
         [[step]]\naction = \"write_file\"\npath = \"notes.txt\"\ncontent = \"x\\n\"\n\n\
         [[step]]\naction = \"edit_files\"\npaths = [\"a.txt\", \"a.txt\"]\n\n\
         [[step]]\naction = \"ask\"\ntext = \"go\"\n\n\
         [[step]]\naction = \"hook\"\nevent = \"done\"\n\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    env.write_fake_profile_as(
        "editor",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let path = env.dir.path().join("config/agents/editor.toml");
    let profile = std::fs::read_to_string(&path).unwrap();
    env.write_profile(
        "editor",
        &profile.replace("mode = \"screen\"", "mode = \"hook+screen\""),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let spawned: SessionSpawnResult = client.spawn(&project, "editor").into_result().unwrap();
    let id = spawned.session_id;
    answer_after_watcher_saw_notes(&env, &mut client, id);

    let event = client
        .wait_event_where(|e| matches!(e, Event::SessionTurn(t) if t.session_id == id && t.n == 2));
    let Event::SessionTurn(turn) = event else {
        unreachable!()
    };
    let counts: Vec<_> = turn
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.added, f.removed))
        .collect();
    assert_eq!(
        counts,
        [("a.txt", Some(2), Some(0)), ("notes.txt", Some(1), Some(0))]
    );
    let start = format!("refs/mkrew/turns/{id}/2");
    assert_eq!(git(&repo, &["ls-tree", "--name-only", &start]), "a.txt");
    assert_eq!(git(&repo, &["show", &format!("{start}:a.txt")]), "one");
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        git(&repo, &["branch", "--format=%(refname)"]),
        "refs/heads/main"
    );
    assert_eq!(git(&repo, &["diff", "--cached", "--name-only"]), "");
    client.stop();
}

/// M11 restore (SPEC §20C.2) over 2 turns of the fake agent, driven by hooks (no screen
/// detection, so turn numbers do not depend on timing): `turn.list` returns both;
/// `turn.restore` is refused while the agent works, without `confirm` or with a stale
/// fingerprint; it puts back the files from before turn 2, then before turn 1, and leaves
/// HEAD and the index.
#[test]
fn turns_are_listed_previewed_and_restored() {
    let env = Env::new();
    env.enable_turns();
    let repo = env.repo();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let head = git(&repo, &["rev-parse", "HEAD"]);
    let scenario = env.dir.path().join("idle.toml");
    std::fs::write(&scenario, "[[step]]\naction = \"sleep\"\nms = 60000\n").unwrap();
    env.write_fake_profile_as(
        "editor",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let path = env.dir.path().join("config/agents/editor.toml");
    let profile = std::fs::read_to_string(&path).unwrap();
    env.write_profile(
        "editor",
        &profile.replace("mode = \"screen\"", "mode = \"hook\""),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let spawned: SessionSpawnResult = client.spawn(&project, "editor").into_result().unwrap();
    let id = spawned.session_id;
    let hook = |client: &mut Client, event: &str| {
        client
            .hook(id, "fake", event, Value::Null)
            .into_result::<()>()
            .unwrap();
    };
    let restore = |n, confirm, created: &[String], fingerprint: &str| {
        Request::TurnRestore(TurnRestoreParams {
            session_id: id,
            n,
            confirm,
            created: created.to_vec(),
            fingerprint: fingerprint.into(),
        })
    };
    let preview = |n| Request::TurnRestorePreview(TurnRef { session_id: id, n });

    let read = |path: &str| std::fs::read_to_string(repo.join(path)).unwrap();

    // Turn 1 edits a.txt. The `busy` reply comes after the start snapshot.
    hook(&mut client, "busy");
    std::fs::write(repo.join("a.txt"), "two\n").unwrap();
    hook(&mut client, "done");
    // Turn 2 creates notes.txt and edits a.txt again; restoring is refused meanwhile.
    hook(&mut client, "busy");
    std::fs::write(repo.join("a.txt"), "three\n").unwrap();
    std::fs::write(repo.join("notes.txt"), "x\n").unwrap();
    match client.call(restore(1, true, &[], "")).into_result::<()>() {
        Err(ResponseError::Daemon(body)) => {
            assert_eq!(body.code, ErrorCode::InvalidState);
            assert!(body.message.contains("working"), "{}", body.message);
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    hook(&mut client, "done");
    client
        .wait_event_where(|e| matches!(e, Event::SessionTurn(t) if t.session_id == id && t.n == 2));

    let turns: Vec<Turn> = client
        .call(Request::TurnList(SessionRef { session_id: id }))
        .into_result()
        .unwrap();
    assert_eq!(turns.iter().map(|t| t.n).collect::<Vec<_>>(), [1, 2]);
    let start = git(&repo, &["rev-parse", &format!("refs/mkrew/turns/{id}/2")]);
    assert_eq!(turns[1].start_tree.as_deref(), Some(start.as_str()));
    assert!(
        turns
            .iter()
            .all(|t| t.ended_at.is_some() && t.end_tree.is_some())
    );

    let shown: UndoPreview = client.call(preview(2)).into_result().unwrap();
    assert_eq!(shown.checkpoint, start);
    assert_eq!(shown.changed, ["a.txt"]);
    assert_eq!(shown.created, ["notes.txt"]);
    assert!(shown.added.is_empty());
    assert!(shown.diff.contains("-two") && shown.diff.contains("+three"));
    let unconfirmed = client.call(restore(2, false, &shown.created, &shown.fingerprint));
    assert_eq!(error_code(unconfirmed), ErrorCode::InvalidRequest);
    let stale = client.call(restore(2, true, &shown.created, "stale"));
    assert_eq!(error_code(stale), ErrorCode::InvalidState);
    assert_eq!(error_code(client.call(preview(9))), ErrorCode::NotFound);

    client
        .call(restore(2, true, &shown.created, &shown.fingerprint))
        .into_result::<()>()
        .unwrap();
    assert_eq!(read("a.txt"), "two\n");
    assert!(!repo.join("notes.txt").exists());

    let shown: UndoPreview = client.call(preview(1)).into_result().unwrap();
    assert_eq!(
        (shown.changed.as_slice(), shown.created.len()),
        (&["a.txt".to_owned()][..], 0)
    );
    client
        .call(restore(1, true, &shown.created, &shown.fingerprint))
        .into_result::<()>()
        .unwrap();
    assert_eq!(read("a.txt"), "one\n");
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&repo, &["status", "--porcelain"]), "");
    let unknown = client.call(Request::TurnList(SessionRef {
        session_id: Ulid::new(),
    }));
    assert_eq!(error_code(unknown), ErrorCode::NotFound);
    client.stop();
}

/// SPEC §20C.3–§20C.5: a hook-reported edit is announced with `session.edited` (line from the
/// payload) and attributed by `session.files`; `git.status`/`git.diff` with `since` = the
/// turn's start tree show everything changed after it, untracked files included.
#[test]
fn hook_edits_are_announced_attributed_and_diffed_since_a_turn() {
    let env = Env::new();
    env.enable_turns();
    let repo = env.repo();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let scenario = env.dir.path().join("idle.toml");
    std::fs::write(&scenario, "[[step]]\naction = \"sleep\"\nms = 60000\n").unwrap();
    env.write_fake_profile_as(
        "editor",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let path = env.dir.path().join("config/agents/editor.toml");
    let profile = std::fs::read_to_string(&path).unwrap();
    env.write_profile(
        "editor",
        &profile.replace("mode = \"screen\"", "mode = \"hook\""),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let spawned: SessionSpawnResult = client.spawn(&project, "editor").into_result().unwrap();
    let id = spawned.session_id;
    let files = |client: &mut Client, since_ms| {
        let files: SessionFiles = client
            .call(Request::SessionFiles(SessionFilesParams {
                project_id: project.id,
                since_ms,
            }))
            .into_result()
            .unwrap();
        files
    };

    client
        .hook(id, "fake", "busy", Value::Null)
        .into_result::<()>()
        .unwrap();
    std::fs::write(repo.join("a.txt"), "two\n").unwrap();
    let edit = json!({
        "tool_name": "Edit",
        "tool_input": {"file_path": "a.txt"},
        "tool_response": {"structuredPatch": [{"newStart": 1, "lines": ["-one", "+two"]}]}
    });
    client
        .hook(id, "fake", "PostToolUse", edit)
        .into_result::<()>()
        .unwrap();
    let Event::SessionEdited(edited) =
        client.wait_event_where(|e| matches!(e, Event::SessionEdited(e) if e.session_id == id))
    else {
        unreachable!()
    };
    assert_eq!((edited.path.as_str(), edited.line), ("a.txt", Some(1)));
    // The running turn's hook edits count already.
    assert_eq!(
        files(&mut client, 0),
        SessionFiles::from([(id, vec!["a.txt".into()])])
    );
    client
        .hook(id, "fake", "done", Value::Null)
        .into_result::<()>()
        .unwrap();
    client
        .wait_event_where(|e| matches!(e, Event::SessionTurn(t) if t.session_id == id && t.n == 1));
    assert_eq!(
        files(&mut client, 0),
        SessionFiles::from([(id, vec!["a.txt".into()])])
    );
    assert!(files(&mut client, i64::MAX).is_empty());
    let unknown = client.call(Request::SessionFiles(SessionFilesParams {
        project_id: Ulid::new(),
        since_ms: 0,
    }));
    assert_eq!(error_code(unknown), ErrorCode::NotFound);

    std::fs::write(repo.join("notes.txt"), "x\n").unwrap();
    let start = git(&repo, &["rev-parse", &format!("refs/mkrew/turns/{id}/1")]);
    let status: GitStatus = client
        .call(Request::GitStatus(GitStatusParams {
            project_id: project.id,
            worktree_task_id: None,
            repo: None,
            mode: GitMode::Base,
            since: Some(start.clone()),
        }))
        .into_result()
        .unwrap();
    assert_eq!(status.merge_base.as_deref(), Some(start.as_str()));
    let paths: Vec<&str> = status
        .against_base
        .iter()
        .map(|f| f.path.as_str())
        .collect();
    assert_eq!(paths, ["a.txt", "notes.txt"]);
    let diff: FileDiff = client
        .call(Request::GitDiff(GitDiffParams {
            project_id: project.id,
            worktree_task_id: None,
            repo: None,
            path: "a.txt".into(),
            mode: GitMode::Base,
            staged: false,
            commit: None,
            since: Some(start),
        }))
        .into_result()
        .unwrap();
    assert_eq!((diff.added, diff.removed), (1, 1));
    // The Git view's reads changed nothing.
    assert_eq!(
        git(&repo, &["status", "--porcelain"]),
        "M a.txt\n?? notes.txt"
    );
    client.stop();
}
