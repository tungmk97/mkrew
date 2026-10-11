//! Doc comment threads and comments sent to agents.

use super::*;

/// Threads: create, reply, resolve, reopen; an agent reply (via its session) addresses the
/// thread, a user reply on an addressed thread reopens it. Every change is broadcast.
#[test]
fn comment_threads_reply_resolve_reopen_and_agent_replies_address_them() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let session: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();

    let create = |file: &str, start_line, end_line, body: &str| {
        Request::CommentCreate(CommentCreateParams {
            project_id: project.id,
            file_path: file.into(),
            start_line,
            end_line,
            quote: "Use Redis".into(),
            body: body.into(),
            context: CommentContext::Doc,
        })
    };
    for bad in [
        create("docs/plan.md", 0, 1, "why?"),
        create("docs/plan.md", 3, 2, "why?"),
        create("docs/plan.md", 1, 1, " "),
        create(" ", 1, 1, "why?"),
    ] {
        assert_eq!(error_code(client.call(bad)), ErrorCode::InvalidRequest);
    }
    let root: DocComment = client
        .call(create(r"docs\plan.md", 12, 14, "Why not in-memory?"))
        .into_result()
        .unwrap();
    assert_eq!(
        (root.file_path.as_str(), root.status, root.author.as_str()),
        ("docs/plan.md", CommentStatus::Open, DocComment::USER)
    );
    wait_comment(&mut client, root.id, CommentStatus::Open);
    let other: DocComment = client
        .call(create("README.md", 1, 1, "Typo"))
        .into_result()
        .unwrap();

    let reply = |id, body: &str, session| {
        Request::CommentReply(CommentReplyParams {
            comment_id: id,
            body: body.into(),
            mkrew_session_id: session,
        })
    };
    let user: DocComment = client
        .call(reply(root.id, "Also check TTLs", None))
        .into_result()
        .unwrap();
    assert_eq!(user.parent_id, Some(root.id));
    assert_eq!(user.author, DocComment::USER);

    // The agent answers (naming the user reply joins the same thread): addressed.
    let agent: DocComment = client
        .call(reply(
            user.id,
            "Switched to in-memory",
            Some(session.session_id),
        ))
        .into_result()
        .unwrap();
    assert_eq!(agent.parent_id, Some(root.id));
    assert_eq!(agent.author, "fake");
    wait_comment(&mut client, root.id, CommentStatus::Addressed);

    let list = |client: &mut Client, file: Option<&str>| -> Vec<DocComment> {
        client
            .call(Request::CommentList(CommentListParams {
                project_id: project.id,
                file_path: file.map(Into::into),
                context: None,
            }))
            .into_result()
            .unwrap()
    };
    // The user answers back: open again.
    client
        .call(reply(root.id, "Still too big", None))
        .into_result::<DocComment>()
        .unwrap();
    assert_eq!(list(&mut client, None)[0].status, CommentStatus::Open);

    // Resolve / reopen act on the thread root, also when given a reply id.
    let resolved: DocComment = client
        .call(Request::CommentResolve(CommentRef {
            comment_id: user.id,
        }))
        .into_result()
        .unwrap();
    assert_eq!(
        (resolved.id, resolved.status),
        (root.id, CommentStatus::Resolved)
    );
    let reopened: DocComment = client
        .call(Request::CommentReopen(CommentRef {
            comment_id: root.id,
        }))
        .into_result()
        .unwrap();
    assert_eq!(reopened.status, CommentStatus::Open);

    let thread = list(&mut client, Some(r"docs\plan.md"));
    let bodies: Vec<_> = thread.iter().map(|c| c.body.as_str()).collect();
    assert_eq!(
        bodies,
        [
            "Why not in-memory?",
            "Also check TTLs",
            "Switched to in-memory",
            "Still too big"
        ]
    );
    assert_eq!(thread[0].status, CommentStatus::Open);
    assert_eq!(list(&mut client, None).len(), 5);
    assert!(
        list(&mut client, Some("README.md"))
            .iter()
            .all(|c| c.id == other.id)
    );

    let missing = CommentId::new();
    assert_eq!(
        error_code(client.call(reply(missing, "hi", None))),
        ErrorCode::NotFound
    );
    assert_eq!(
        error_code(client.call(reply(root.id, "hi", Some(Ulid::new())))),
        ErrorCode::NotFound
    );
    assert_eq!(
        error_code(client.call(reply(root.id, "", None))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(Request::CommentResolve(CommentRef {
            comment_id: missing
        }))),
        ErrorCode::NotFound
    );
    client.stop();
}

/// Comments sent while the agent works are marked `sent` at once (snapshot = file content)
/// and typed only once it asks; reply ids stand for their thread; bad targets are refused.
#[test]
fn comments_sent_to_a_working_agent_are_typed_once_it_asks() {
    let env = Env::new();
    env.write_fake_profile();
    let repo = env.repo();
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    let plan = "# Plan\n\nUse Redis for caching.\n\nM3: deploy\n";
    std::fs::write(repo.join("docs/plan.md"), plan).unwrap();
    std::fs::write(repo.join("README.md"), "# Readme\n").unwrap();
    let other_repo = env.dir.path().join("other");
    std::fs::create_dir_all(&other_repo).unwrap();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let other_project = client.add_project(&other_repo);

    let mut create = |file: &str, line, quote: &str, body: &str| {
        let params = CommentCreateParams {
            project_id: project.id,
            file_path: file.into(),
            start_line: line,
            end_line: line,
            quote: quote.into(),
            body: body.into(),
            context: CommentContext::Doc,
        };
        let created: DocComment = client
            .call(Request::CommentCreate(params))
            .into_result()
            .unwrap();
        created
    };
    let redis = create("docs/plan.md", 3, "Use Redis", "Why not in-memory?");
    let deploy = create("docs/plan.md", 5, "M3: deploy", "Add a rollback step.");
    let readme = create("README.md", 1, "# Readme", "Typo");
    let reply: DocComment = client
        .call(Request::CommentReply(CommentReplyParams {
            comment_id: redis.id,
            body: "Also check TTLs".into(),
            mkrew_session_id: None,
        }))
        .into_result()
        .unwrap();

    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    let other: SessionSpawnResult = client.spawn(&other_project, "fake").into_result().unwrap();
    for bad in [
        send_comments(&[], id),
        send_comments(&[redis.id, readme.id], id),
        send_comments(&[redis.id], other.session_id),
    ] {
        assert_eq!(error_code(client.call(bad)), ErrorCode::InvalidRequest);
    }
    let unknown = client.call(send_comments(&[CommentId(Ulid::new())], id));
    assert_eq!(error_code(unknown), ErrorCode::NotFound);

    let working: SessionWaitResult = client
        .call(wait(id, &[SessionState::Working], Some(15_000)))
        .into_result()
        .unwrap();
    assert_eq!(working.state, SessionState::Working);
    let sent: Vec<DocComment> = client
        .call(send_comments(&[reply.id, deploy.id, redis.id], id))
        .into_result()
        .unwrap();
    assert_eq!(
        sent.iter().map(|c| c.id).collect::<Vec<_>>(),
        [redis.id, deploy.id]
    );
    for comment in &sent {
        assert_eq!(comment.status, CommentStatus::Sent);
        assert_eq!(comment.sent_to, Some(id));
        assert_eq!(comment.sent_snapshot.as_deref(), Some(plan));
    }
    wait_comment(&mut client, deploy.id, CommentStatus::Sent);

    // Queued while working: the console echoes the prompt's first line after the agent asks.
    let session = client.wait_state(id, SessionState::Exited);
    assert_eq!(session.exit_code, Some(0));
    let log = session_log(&env, id);
    let asked = log.find("(y/n)").unwrap();
    let typed = log.find("Update docs/plan.md").unwrap();
    assert!(asked < typed, "{log}");

    let exited = client.call(send_comments(&[deploy.id], id));
    assert_eq!(error_code(exited), ErrorCode::InvalidState);
    client.stop();
}

/// The review round trip (M6 E2E): the fake writes a plan, two comments are sent to it, it
/// edits the file for each and answers each with `mk comment reply` → both `addressed`.
#[test]
fn fake_agent_addresses_sent_comments_and_replies_to_each() {
    let env = Env::new();
    let repo = env.repo();
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    let plan = "# Plan\n\nUse Redis for caching.\n\nM3: deploy\n";
    // The spinner keeps the fake working while the comments are sent, so the prompt is
    // usually queued until it asks (either way it must arrive whole).
    let scenario = env.dir.path().join("reviewer.toml");
    std::fs::write(
        &scenario,
        format!(
            "[[step]]\naction = \"write_file\"\npath = \"docs/plan.md\"\ncontent = '''{plan}'''\n\n\
             [[step]]\naction = \"spinner\"\ntext = \"planning\"\nms = 600\n\n\
             [[step]]\naction = \"reply_comments\"\ncomments = 2\n\n\
             [[step]]\naction = \"print\"\ntext = \"Edited docs/plan.md\"\n\n\
             [[step]]\naction = \"print\"\ntext = \"Replied to 2 comments\"\n\n\
             [[step]]\naction = \"print\"\ntext = \"All done\"\n\n\
             [[step]]\naction = \"print\"\ntext = \"Done.\"\n"
        ),
    )
    .unwrap();
    env.write_fake_profile_as(
        "reviewer",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let spawned: SessionSpawnResult = client.spawn(&project, "reviewer").into_result().unwrap();
    let id = spawned.session_id;
    let deadline = Instant::now() + TIMEOUT;
    while std::fs::read_to_string(repo.join("docs/plan.md"))
        .ok()
        .as_deref()
        != Some(plan)
    {
        assert!(Instant::now() < deadline, "the fake never wrote the plan");
        std::thread::sleep(Duration::from_millis(20));
    }

    let mut create = |line, quote: &str, body: &str| {
        let params = CommentCreateParams {
            project_id: project.id,
            file_path: "docs/plan.md".into(),
            start_line: line,
            end_line: line,
            quote: quote.into(),
            body: body.into(),
            context: CommentContext::Doc,
        };
        let created: DocComment = client
            .call(Request::CommentCreate(params))
            .into_result()
            .unwrap();
        created
    };
    let redis = create(3, "Use Redis", "Why not in-memory?");
    let deploy = create(5, "M3: deploy", "Add a rollback step.");
    let sent: Vec<DocComment> = client
        .call(send_comments(&[redis.id, deploy.id], id))
        .into_result()
        .unwrap();
    assert_eq!(sent.len(), 2);

    for comment in [&redis, &deploy] {
        wait_comment(&mut client, comment.id, CommentStatus::Addressed);
    }
    let session = client.wait_state(id, SessionState::Exited);
    assert_eq!(session.exit_code, Some(0));
    assert_eq!(
        std::fs::read_to_string(repo.join("docs/plan.md")).unwrap(),
        format!(
            "{plan}Addressed {}: Why not in-memory?\nAddressed {}: Add a rollback step.\n",
            redis.id, deploy.id
        )
    );

    let comments: Vec<DocComment> = client
        .call(Request::CommentList(CommentListParams {
            project_id: project.id,
            file_path: Some("docs/plan.md".into()),
            context: None,
        }))
        .into_result()
        .unwrap();
    for (root, body) in [
        (&redis, "Addressed: Why not in-memory?"),
        (&deploy, "Addressed: Add a rollback step."),
    ] {
        let stored = comments.iter().find(|c| c.id == root.id).unwrap();
        assert_eq!(stored.status, CommentStatus::Addressed);
        let replies: Vec<_> = comments
            .iter()
            .filter(|c| c.parent_id == Some(root.id))
            .map(|c| (c.author.as_str(), c.body.as_str()))
            .collect();
        assert_eq!(replies, [("reviewer", body)]);
    }
    client.stop();
}

/// Diff-line comments (M7 E2E): a comment on new-side lines of a code file is sent with the
/// "fix the code" prompt; the fake edits the file and answers with `mk comment reply`.
#[test]
fn fake_agent_fixes_code_for_diff_comments_and_replies() {
    let env = Env::new();
    let repo = env.repo();
    std::fs::create_dir_all(repo.join("src")).unwrap();
    let code = "pub fn add(a: i32, b: i32) -> i32 {\n    a - b\n}\n";
    let scenario = env.dir.path().join("fixer.toml");
    std::fs::write(
        &scenario,
        format!(
            "[[step]]\naction = \"write_file\"\npath = \"src/lib.rs\"\ncontent = '''{code}'''\n\n\
             [[step]]\naction = \"spinner\"\ntext = \"coding\"\nms = 600\n\n\
             [[step]]\naction = \"reply_comments\"\ncomments = 1\n\n\
             [[step]]\naction = \"print\"\ntext = \"Fixed src/lib.rs\"\n"
        ),
    )
    .unwrap();
    env.write_fake_profile_as(
        "fixer",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let spawned: SessionSpawnResult = client.spawn(&project, "fixer").into_result().unwrap();
    let id = spawned.session_id;
    let deadline = Instant::now() + TIMEOUT;
    while std::fs::read_to_string(repo.join("src/lib.rs"))
        .ok()
        .as_deref()
        != Some(code)
    {
        assert!(Instant::now() < deadline, "the fake never wrote the code");
        std::thread::sleep(Duration::from_millis(20));
    }

    let mut create = |file: &str, context| {
        let params = CommentCreateParams {
            project_id: project.id,
            file_path: file.into(),
            start_line: 2,
            end_line: 2,
            quote: "a - b".into(),
            body: "This subtracts.".into(),
            context,
        };
        let created: DocComment = client
            .call(Request::CommentCreate(params))
            .into_result()
            .unwrap();
        created
    };
    let diff = create("src/lib.rs", CommentContext::Diff);
    assert_eq!(diff.context, CommentContext::Diff);
    // One prompt cannot both edit a document and fix code.
    let doc = create("src/lib.rs", CommentContext::Doc);
    assert_eq!(
        error_code(client.call(send_comments(&[diff.id, doc.id], id))),
        ErrorCode::InvalidRequest
    );
    client
        .call(Request::CommentResolve(CommentRef { comment_id: doc.id }))
        .into_result::<DocComment>()
        .unwrap();

    let sent: Vec<DocComment> = client
        .call(send_comments(&[diff.id], id))
        .into_result()
        .unwrap();
    assert_eq!(sent[0].sent_snapshot.as_deref(), Some(code));
    wait_comment(&mut client, diff.id, CommentStatus::Addressed);
    let session = client.wait_state(id, SessionState::Exited);
    assert_eq!(session.exit_code, Some(0));
    assert_eq!(
        std::fs::read_to_string(repo.join("src/lib.rs")).unwrap(),
        format!("{code}Addressed {}: This subtracts.\n", diff.id)
    );

    let list = |context| CommentListParams {
        project_id: project.id,
        file_path: Some("src/lib.rs".into()),
        context,
    };
    let diffs: Vec<DocComment> = client
        .call(Request::CommentList(list(Some(CommentContext::Diff))))
        .into_result()
        .unwrap();
    let replies: Vec<_> = diffs
        .iter()
        .map(|c| (c.parent_id, c.context, c.author.as_str(), c.body.as_str()))
        .collect();
    assert_eq!(
        replies,
        [
            (
                None,
                CommentContext::Diff,
                DocComment::USER,
                "This subtracts."
            ),
            (
                Some(diff.id),
                CommentContext::Diff,
                "fixer",
                "Addressed: This subtracts."
            ),
        ]
    );
    let all: Vec<DocComment> = client
        .call(Request::CommentList(list(None)))
        .into_result()
        .unwrap();
    assert_eq!(all.len(), 3);
    client.stop();
}
