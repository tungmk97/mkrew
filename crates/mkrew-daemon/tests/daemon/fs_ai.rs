//! `fs.*` requests and `ai.generate`.

use super::*;

#[test]
fn fs_list_read_and_find_follow_file_changes() {
    let env = Env::new();
    let repo = env.repo();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["config", "user.name", "t"]);
    git(&repo, &["config", "user.email", "t@t"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    std::fs::create_dir(repo.join("src")).unwrap();
    std::fs::write(repo.join(".gitignore"), "out/\n").unwrap();
    std::fs::write(repo.join("src/main.rs"), "fn main() {}\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    std::fs::write(repo.join("src/main.rs"), "fn main() { run() }\n").unwrap();
    std::fs::create_dir(repo.join("out")).unwrap();
    std::fs::write(repo.join("out/app.bin"), b"\0\x01").unwrap();
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;
    let list = |client: &mut Client, dir: &str, show_ignored| -> Vec<FsEntry> {
        let request = Request::FsList(FsListParams {
            project_id: id,
            worktree_task_id: None,
            dir: dir.into(),
            show_ignored,
        });
        client.call(request).into_result().unwrap()
    };
    let find = |client: &mut Client, query: &str| -> Vec<String> {
        let request = Request::FsFind(FsFindParams {
            project_id: id,
            worktree_task_id: None,
            query: query.into(),
            limit: 10,
        });
        let found: Vec<FsMatch> = client.call(request).into_result().unwrap();
        found.into_iter().map(|m| m.path).collect()
    };
    let read = |path: &str| {
        Request::FsRead(FsReadParams {
            project_id: id,
            worktree_task_id: None,
            path: path.into(),
            index_text: false,
        })
    };

    let top = list(&mut client, "", false);
    let names: Vec<&str> = top.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["src", ".gitignore"]);
    assert!(top[0].has_changes);
    let shown = list(&mut client, "", true);
    assert!(shown.iter().any(|e| e.name == "out" && e.ignored));
    let src = list(&mut client, "src", false);
    assert_eq!(src[0].path, "src/main.rs");
    assert_eq!(src[0].marker, Some(GitMarker::Modified));

    let content: FsContent = client.call(read("src\\main.rs")).into_result().unwrap();
    assert_eq!(content.content.as_deref(), Some("fn main() { run() }\n"));
    assert_eq!(content.language.as_deref(), Some("rust"));
    let binary: FsContent = client.call(read("out/app.bin")).into_result().unwrap();
    assert!(binary.binary && binary.content.is_none());
    assert_eq!(
        error_code(client.call(read("../x"))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(read(".git/HEAD"))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(read("nope.rs"))),
        ErrorCode::NotFound
    );
    let save = |path: &str, content: &str, expected_hash: &str| {
        Request::FsWrite(FsWriteParams {
            project_id: id,
            worktree_task_id: None,
            path: path.into(),
            content: content.into(),
            expected_hash: expected_hash.into(),
        })
    };
    assert_eq!(binary.hash, None);
    let read_hash = content.hash.clone().expect("text has a hash");
    let saved: FsWritten = client
        .call(save("src/main.rs", "fn main() { saved() }\n", &read_hash))
        .into_result()
        .unwrap();
    assert!(saved.saved);
    assert_eq!(
        std::fs::read_to_string(repo.join("src/main.rs")).unwrap(),
        "fn main() { saved() }\n"
    );
    let reread: FsContent = client.call(read("src/main.rs")).into_result().unwrap();
    assert_eq!(reread.hash.as_deref(), Some(saved.hash.as_str()));
    // An old hash saves nothing.
    let stale: FsWritten = client
        .call(save("src/main.rs", "lost\n", &read_hash))
        .into_result()
        .unwrap();
    assert_eq!((stale.saved, stale.hash), (false, saved.hash));
    for (path, code) in [
        ("../x", ErrorCode::InvalidRequest),
        (".git/HEAD", ErrorCode::InvalidRequest),
        ("new.rs", ErrorCode::NotFound),
    ] {
        assert_eq!(
            error_code(client.call(save(path, "x", "h"))),
            code,
            "{path}"
        );
    }
    let missing = Request::FsList(FsListParams {
        project_id: id,
        worktree_task_id: None,
        dir: "nope".into(),
        show_ignored: false,
    });
    assert_eq!(error_code(client.call(missing)), ErrorCode::NotFound);

    // The cached file list follows the watcher.
    assert_eq!(find(&mut client, "main"), ["src/main.rs"]);
    assert!(
        find(&mut client, "app").is_empty(),
        "ignored files are not found"
    );
    // Let any batch from earlier writes pass, so the event waited for is this file's.
    std::thread::sleep(Duration::from_millis(500));
    client.call(Request::DaemonStatus);
    client.events.clear();
    std::fs::write(repo.join("src/maintenance.rs"), "").unwrap();
    client.wait_event(&Event::GitChanged(GitChangedEvent {
        project_id: id,
        worktree_task_id: None,
    }));
    assert_eq!(
        find(&mut client, "main"),
        ["src/main.rs", "src/maintenance.rs"]
    );
}

/// `fs.read` with `index_text` adds the file's text in the index (SPEC §12A change gutter): of
/// the project's repository, or of the child repository holding the file (ADR 0029); none for
/// an untracked file or when not asked.
#[test]
fn fs_read_adds_the_index_text_when_asked() {
    let env = Env::new();
    let repo = env.repo();
    let init = |dir: &std::path::Path| {
        git(dir, &["init", "-q", "-b", "main"]);
        git(dir, &["config", "core.autocrlf", "false"]);
        git(dir, &["config", "user.name", "t"]);
        git(dir, &["config", "user.email", "t@t"]);
        git(dir, &["config", "commit.gpgsign", "false"]);
    };
    init(&repo);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    std::fs::write(repo.join("a.txt"), "two\n").unwrap();
    std::fs::write(repo.join("new.txt"), "new\n").unwrap();
    let child = repo.join("fe");
    std::fs::create_dir(&child).unwrap();
    init(&child);
    std::fs::write(child.join("b.txt"), "child\n").unwrap();
    git(&child, &["add", "."]);
    std::fs::write(child.join("b.txt"), "edited\n").unwrap();
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;
    let read = |client: &mut Client, path: &str, index_text: bool| -> FsContent {
        let request = Request::FsRead(FsReadParams {
            project_id: id,
            worktree_task_id: None,
            path: path.into(),
            index_text,
        });
        client.call(request).into_result().unwrap()
    };

    let a = read(&mut client, "a.txt", true);
    assert_eq!(a.content.as_deref(), Some("two\n"));
    assert_eq!(a.index_text.as_deref(), Some("one\n"));
    assert_eq!(
        read(&mut client, "a.txt", false).index_text,
        None,
        "not asked"
    );
    assert_eq!(
        read(&mut client, "new.txt", true).index_text,
        None,
        "untracked"
    );
    let b = read(&mut client, "fe/b.txt", true);
    assert_eq!(
        b.index_text.as_deref(),
        Some("child\n"),
        "the child repo's index"
    );
}

/// SPEC §6 `oneshot`: the agent's stdout is the answer, without blank edges or a code fence.
#[test]
fn ai_generate_answers_with_the_oneshot_output() {
    let env = Env::new();
    let repo = env.repo();
    init_repo(&repo);
    std::fs::write(repo.join("a.txt"), "a\nb\n").unwrap();
    git(&repo, &["add", "."]);
    let scenario = env.dir.path().join("answer.toml");
    let steps = ["```", "feat: add b", "```"]
        .map(|text| format!("[[step]]\naction = \"print\"\ntext = \"{text}\"\n"))
        .join("\n");
    std::fs::write(&scenario, steps).unwrap();
    env.write_oneshot_profile("gen", &scenario);
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);

    let (id, request) = generate(project.id, Some("gen"), AiKind::CommitMessage, "");
    client.call(request).into_result::<()>().unwrap();
    let done = generated(&mut client, id);
    assert_eq!(
        (done.text.as_deref(), done.error),
        (Some("feat: add b"), None)
    );
    assert_eq!(done.project_id, Some(project.id));

    let (id, request) = generate(project.id, Some("gen"), AiKind::EnhancePrompt, "add b");
    client.call(request).into_result::<()>().unwrap();
    assert_eq!(
        generated(&mut client, id).text.as_deref(),
        Some("feat: add b")
    );
}

/// What a user meets with old profiles, no default agent, a failing agent or no changes.
#[test]
fn ai_generate_reports_what_is_missing() {
    let env = Env::new();
    let repo = env.repo();
    init_repo(&repo);
    env.write_fake_profile(); // `fake`: no `oneshot`
    let failing = env.dir.path().join("fail.toml");
    std::fs::write(&failing, "[[step]]\naction = \"exit\"\ncode = 3\n").unwrap();
    env.write_oneshot_profile("broken", &failing);
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);

    // A profile seeded before `oneshot` existed: the error says what to add, and where.
    let (_, request) = generate(project.id, Some("fake"), AiKind::EnhancePrompt, "x");
    let outcome = client.call(request);
    let message = format!("{outcome:?}");
    assert_eq!(error_code(outcome), ErrorCode::InvalidState);
    assert!(
        message.contains("oneshot") && message.contains("fake.toml"),
        "{message}"
    );

    // No profile given and no default agent.
    let (_, request) = generate(project.id, None, AiKind::EnhancePrompt, "x");
    assert_eq!(error_code(client.call(request)), ErrorCode::InvalidState);

    // Nothing to improve.
    let (_, request) = generate(project.id, Some("broken"), AiKind::EnhancePrompt, "  ");
    assert_eq!(error_code(client.call(request)), ErrorCode::InvalidRequest);

    // The agent fails: the event carries the reason.
    let (id, request) = generate(project.id, Some("broken"), AiKind::EnhancePrompt, "x");
    client.call(request).into_result::<()>().unwrap();
    let done = generated(&mut client, id);
    assert_eq!(done.text, None);
    assert!(
        done.error.as_deref().is_some_and(|e| e.contains('3')),
        "{done:?}"
    );

    // A clean repo has nothing to describe; the agent is not even started.
    let (id, request) = generate(project.id, Some("broken"), AiKind::CommitMessage, "");
    client.call(request).into_result::<()>().unwrap();
    assert_eq!(
        generated(&mut client, id).error.as_deref(),
        Some("No changes to describe.")
    );
}

/// ADR 0061: a question needs no project, runs in the scratch folder, and its answer keeps a
/// code fence (it is shown as Markdown).
#[test]
fn ai_generate_asks_outside_every_project() {
    let env = Env::new();
    let scenario = env.dir.path().join("answer.toml");
    let mut steps =
        String::from("[[step]]\naction = \"write_file\"\npath = \"here.txt\"\ncontent = \"x\"\n");
    for text in ["```sh", "ls", "```"] {
        steps.push_str(&format!(
            "\n[[step]]\naction = \"print\"\ntext = \"{text}\"\n"
        ));
    }
    std::fs::write(&scenario, steps).unwrap();
    let fake = built_bin("mkrew-fakeagent");
    let toml = format!(
        "command = '{fake}'\nask = '\"{fake}\" --scenario \"{scenario}\"'\n",
        fake = fake.display(),
        scenario = scenario.display(),
    );
    env.write_profile("chat", &toml);
    let (_daemon, mut client) = env.start();

    let (id, request) = ask(Some("chat"), "how do I list files?", false);
    client.call(request).into_result::<()>().unwrap();
    let done = generated(&mut client, id);
    assert_eq!(
        (done.text.as_deref(), done.error, done.project_id),
        (Some("```sh\nls\n```"), None, None)
    );
    assert!(env.data_dir().join("scratch").join("here.txt").is_file());

    // Web search needs its own command; the error says which line to add, and where.
    let (_, request) = ask(Some("chat"), "news?", true);
    let outcome = client.call(request);
    let message = format!("{outcome:?}");
    assert_eq!(error_code(outcome), ErrorCode::InvalidState);
    assert!(
        message.contains("ask_web") && message.contains("chat.toml"),
        "{message}"
    );

    // No agent named (there is no project default to fall back to), or no question.
    assert_eq!(
        error_code(client.call(ask(None, "x", false).1)),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(ask(Some("chat"), " ", false).1)),
        ErrorCode::InvalidRequest
    );
}
