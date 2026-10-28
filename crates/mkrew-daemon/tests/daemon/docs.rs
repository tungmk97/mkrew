//! Docs listing/reading/re-anchoring and edit conflicts.

use super::*;

/// `doc.list` = review globs + Markdown an agent just wrote (with its session) + open-comment
/// counts; `doc.read`; after the session exited the docs stay watched, so an external edit
/// sends `doc.changed` and re-anchors the comments (moved / outdated), and so does deleting.
#[test]
fn docs_are_listed_read_and_external_edits_reanchor_comments() {
    let env = Env::new();
    let repo = env.repo();
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    for dir in ["src", "notes", ".claude/plans"] {
        std::fs::create_dir_all(repo.join(dir)).unwrap();
    }
    std::fs::write(repo.join(".gitignore"), ".claude/\n").unwrap();
    std::fs::write(repo.join(".claude/plans/p.md"), "# P\n").unwrap();
    std::fs::write(repo.join("README.md"), "# Readme\n").unwrap();
    std::fs::write(repo.join("src/inner.md"), "not reviewed\n").unwrap();
    let plan = "# Plan\n\nUse Redis for caching.\n\nM3: deploy\n";
    std::fs::write(repo.join("docs/plan.md"), plan).unwrap();
    let scenario = env.dir.path().join("writer.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"write_file\"\npath = \"notes/todo.md\"\ncontent = \"- [ ] a\"\n\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    env.write_fake_profile_as(
        "writer",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);

    let files = doc_list(&mut client, &project);
    let paths: Vec<&str> = files.iter().map(|f| f.file_path.as_str()).collect();
    assert_eq!(paths, [".claude/plans/p.md", "README.md", "docs/plan.md"]);
    let read: DocContent = client
        .call(doc_read(&project, r"docs\plan.md"))
        .into_result()
        .unwrap();
    assert_eq!(
        (read.file_path.as_str(), read.content.as_str()),
        ("docs/plan.md", plan)
    );
    let missing = client.call(doc_read(&project, "docs/nope.md"));
    assert_eq!(error_code(missing), ErrorCode::NotFound);
    for bad in ["../outside.md", "", "/abs.md"] {
        let outcome = client.call(doc_read(&project, bad));
        assert_eq!(error_code(outcome), ErrorCode::InvalidRequest, "{bad}");
    }

    let mut comment = |start_line, quote: &str| {
        let params = CommentCreateParams {
            project_id: project.id,
            file_path: "docs/plan.md".into(),
            start_line,
            end_line: start_line,
            quote: quote.into(),
            body: "why?".into(),
            context: CommentContext::Doc,
        };
        let created: DocComment = client
            .call(Request::CommentCreate(params))
            .into_result()
            .unwrap();
        created
    };
    let redis = comment(3, "Use Redis");
    let deploy = comment(5, "M3: deploy");

    // An agent writes a Markdown file outside the globs: listed with its session.
    let spawned: SessionSpawnResult = client.spawn(&project, "writer").into_result().unwrap();
    let session = spawned.session_id;
    client.wait_state(session, SessionState::NeedsInput);
    let deadline = Instant::now() + TIMEOUT;
    let todo = loop {
        let files = doc_list(&mut client, &project);
        if let Some(todo) = files.iter().find(|f| f.file_path == "notes/todo.md") {
            break todo.clone();
        }
        assert!(
            Instant::now() < deadline,
            "notes/todo.md never listed: {files:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(todo.last_session_id, Some(session));
    assert!(todo.last_changed_at.is_some());
    let files = doc_list(&mut client, &project);
    let plan_file = files
        .iter()
        .find(|f| f.file_path == "docs/plan.md")
        .unwrap();
    assert_eq!(plan_file.open_comments, 2);
    let kill = SessionKillParams {
        session_id: session,
        force: true,
    };
    client.call(Request::SessionKill(kill));
    client.wait_state(session, SessionState::Exited);

    // An external edit (no session live): the passage moved, the other one is gone.
    client.events.clear();
    let edited = "# Plan\n\nIntro.\n\nUse Redis for caching.\n";
    std::fs::write(repo.join("docs/plan.md"), edited).unwrap();
    let moved = client.wait_event_where(
        |e| matches!(e, Event::CommentUpdated(c) if c.id == redis.id && c.start_line == Some(5)),
    );
    assert!(matches!(moved, Event::CommentUpdated(c) if c.status == CommentStatus::Open));
    let gone = wait_comment(&mut client, deploy.id, CommentStatus::Outdated);
    assert_eq!(gone.start_line, Some(5));
    // The watcher announced the final content (a write may show up in steps first).
    let read: DocContent = client
        .call(doc_read(&project, "docs/plan.md"))
        .into_result()
        .unwrap();
    let last_hash = client.events.iter().rev().find_map(|e| match e {
        Event::DocChanged(c) if c.file_path == "docs/plan.md" => Some(c.hash.clone()),
        _ => None,
    });
    assert_eq!(last_hash, Some(Some(read.hash)));
    // A git-ignored plan is listed and followed too.
    std::fs::write(repo.join(".claude/plans/p.md"), "# P\n\nedited\n").unwrap();
    client.wait_event_where(
        |e| matches!(e, Event::DocChanged(c) if c.file_path == ".claude/plans/p.md"),
    );

    // Deleting the file outdates the rest; nothing is deleted.
    client.events.clear();
    std::fs::remove_file(repo.join("docs/plan.md")).unwrap();
    wait_doc_changed(&mut client, &project, "docs/plan.md", None);
    wait_comment(&mut client, redis.id, CommentStatus::Outdated);
    let comments: Vec<DocComment> = client
        .call(Request::CommentList(CommentListParams {
            project_id: project.id,
            file_path: None,
            context: None,
        }))
        .into_result()
        .unwrap();
    assert_eq!(comments.len(), 2);
    client.stop();
}

#[test]
fn two_sessions_editing_one_file_raise_a_conflict() {
    let env = Env::new();
    let repo = env.repo();
    // The first spinner lets the project watcher start (it starts after the first spawn); the
    // second keeps the session `working` until the batch landed (300 ms quiet; a stream that
    // never goes quiet would wait ≤ `watch::MAX_DELAY_WORKING` = 5 s).
    let scenario = env.dir.path().join("writer.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"spinner\"\ntext = \"reading\"\nms = 500\n\n\
         [[step]]\naction = \"write_file\"\npath = \"shared.txt\"\ncontent = \"x\"\n\n\
         [[step]]\naction = \"spinner\"\ntext = \"thinking\"\nms = 3000\n\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    env.write_fake_profile_as(
        "writer",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let run = |client: &mut Client, title: &str| {
        let task: Task = client
            .call(create_task(&project, title, Some("writer")))
            .into_result()
            .unwrap();
        let spawned: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
        client.wait_state(spawned.session_id, SessionState::NeedsInput);
        (task.id, spawned.session_id)
    };
    // One after the other: each change has exactly one `working` session to blame.
    let (task_a, a) = run(&mut client, "A");
    let (task_b, b) = run(&mut client, "B");

    let conflict = |path: &'static str| move |e: &Event| matches!(e, Event::ConflictDetected(c) if c.path == path);
    let Event::ConflictDetected(found) = client.wait_event_where(conflict("shared.txt")) else {
        unreachable!()
    };
    let mut sessions = vec![a, b];
    sessions.sort();
    let mut tasks = vec![task_a, task_b];
    tasks.sort();
    assert_eq!(found.project_id, project.id);
    assert_eq!(found.session_ids, sessions);
    assert_eq!(found.task_ids, tasks);

    // Paths named in hook payloads count too (absolute or relative to the project).
    let edit = |path: String| json!({"tool_input": {"file_path": path}});
    let absolute = repo.join("src").join("hooked.rs").display().to_string();
    let reported = client.hook(a, "fake", "PostToolUse", edit(absolute));
    reported.into_result::<()>().unwrap();
    let reported = client.hook(b, "fake", "PostToolUse", edit("src/hooked.rs".into()));
    reported.into_result::<()>().unwrap();
    let Event::ConflictDetected(found) = client.wait_event_where(conflict("src/hooked.rs")) else {
        unreachable!()
    };
    assert_eq!(found.session_ids, sessions);
    client.stop();
}
