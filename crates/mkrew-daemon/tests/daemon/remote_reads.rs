//! Reads and writes a window on another computer needs from the daemon (spec 2026-10-29 §5).

use super::*;

/// A removed session is hidden and exited; its log files stay (ADR 0019).
fn exited_fake_session(env: &Env, client: &mut Client) -> Ulid {
    env.write_fake_profile();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::NeedsInput);
    client
        .call(Request::SessionRemove(SessionRef { session_id: id }))
        .into_result::<()>()
        .unwrap();
    client.wait_event_where(|e| matches!(e, Event::SessionExited(x) if x.session_id == id));
    id
}

/// `.log.1` + `.log` are joined oldest first; `max_bytes` keeps the newest; `cut` tells.
#[test]
fn session_log_tail_returns_the_newest_bytes_across_rotated_files() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    let id = exited_fake_session(&env, &mut client);
    let log = env.data_dir().join("sessions").join(format!("{id}.log"));
    std::fs::write(
        env.data_dir().join("sessions").join(format!("{id}.log.1")),
        b"old-part|",
    )
    .unwrap();
    std::fs::write(&log, b"new-part").unwrap();
    let mut tail = |max_bytes| -> SessionLogTail {
        client
            .call(Request::SessionLogTail(SessionLogTailParams {
                session_id: id,
                max_bytes,
            }))
            .into_result()
            .unwrap()
    };
    let all = tail(1024);
    assert_eq!(all.data, b"old-part|new-part");
    assert!(!all.cut);
    let last = tail(4);
    assert_eq!(last.data, b"part");
    assert!(last.cut);
    let huge = tail(u64::MAX);
    assert_eq!(
        huge.data, b"old-part|new-part",
        "capped at READ_WINDOW, not an error"
    );
    let unknown = client.call(Request::SessionLogTail(SessionLogTailParams {
        session_id: Ulid::new(),
        max_bytes: 4,
    }));
    assert_eq!(error_code(unknown), ErrorCode::NotFound);
    client.stop();
}

/// The fake agent's profile with hooks (a hook report carries the native id) and `[usage]`.
fn write_usage_profile(env: &Env) {
    env.write_fake_profile();
    let path = env.dir.path().join("config/agents/fake.toml");
    let fake = std::fs::read_to_string(&path).unwrap();
    let hooked = fake.replace("mode = \"screen\"", "mode = \"hook+screen\"");
    assert_ne!(hooked, fake);
    env.write_profile(
        "usage",
        &format!("{hooked}\n[usage]\nformat = \"claude_jsonl\"\n"),
    );
}

#[test]
fn session_transcript_reads_windows_and_answers_not_found_without_a_file() {
    let env = Env::new();
    let transcripts = env.dir.path().join("claude/projects/C--repo");
    std::fs::create_dir_all(&transcripts).unwrap();
    let body = include_bytes!("../../../mkrew-core/tests/fixtures/usage/claude.jsonl");
    std::fs::write(transcripts.join("native-7.jsonl"), body).unwrap();
    write_usage_profile(&env);
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "usage").into_result().unwrap();
    let id = spawned.session_id;
    let transcript = |client: &mut Client, offset| -> SessionTranscript {
        client
            .call(Request::SessionTranscript(SessionTranscriptParams {
                session_id: id,
                offset,
            }))
            .into_result()
            .unwrap()
    };

    // No native id yet: nothing found, but the profile's format is known.
    let none = transcript(&mut client, 0);
    assert_eq!(
        none,
        SessionTranscript {
            found: false,
            format: "claude_jsonl".into(),
            start: 0,
            len: 0,
            data: Vec::new(),
        }
    );

    client
        .hook(id, "fake", "needs_input", json!({"session_id": "native-7"}))
        .into_result::<()>()
        .unwrap();
    let len = body.len() as u64;
    let first = transcript(&mut client, 0);
    assert!(first.found);
    assert_eq!(
        (first.format.as_str(), first.start, first.len),
        ("claude_jsonl", 0, len)
    );
    assert_eq!(first.data, body);
    let caught_up = transcript(&mut client, len);
    assert_eq!((caught_up.start, caught_up.len), (len, len));
    assert!(caught_up.data.is_empty());
    let from = transcript(&mut client, 10);
    assert_eq!(from.start, 10);
    assert_eq!(from.data, &body[10..]);
    let shrunk = transcript(&mut client, len + 100);
    assert_eq!(shrunk.start, 0, "past the end: from the start");
    assert_eq!(shrunk.data, body);

    // The file is gone: not found again.
    std::fs::remove_file(transcripts.join("native-7.jsonl")).unwrap();
    assert!(!transcript(&mut client, 0).found);
    let unknown = client.call(Request::SessionTranscript(SessionTranscriptParams {
        session_id: Ulid::new(),
        offset: 0,
    }));
    assert_eq!(error_code(unknown), ErrorCode::NotFound);
    client.stop();
}

#[test]
fn session_transcript_of_a_profile_without_usage_is_not_found_with_no_format() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    let id = exited_fake_session(&env, &mut client);
    let read: SessionTranscript = client
        .call(Request::SessionTranscript(SessionTranscriptParams {
            session_id: id,
            offset: 0,
        }))
        .into_result()
        .unwrap();
    assert!(!read.found && read.format.is_empty() && read.data.is_empty());
    client.stop();
}

fn create(project_id: Ulid, dir: &str, name: &str, folder: bool) -> Request {
    Request::FsCreate(FsCreateParams {
        project_id,
        worktree_task_id: None,
        dir: dir.into(),
        name: name.into(),
        folder,
    })
}

#[test]
fn fs_create_makes_files_and_folders_and_refuses_existing_and_outside() {
    let env = Env::new();
    let repo = env.repo();
    init_repo(&repo);
    let (_daemon, mut client) = env.start();
    let project_id = client.add_project(&repo).id;
    let made: FsCreated = client
        .call(create(project_id, "src", "a/b.rs", false))
        .into_result()
        .unwrap();
    assert_eq!(made.path, "src/a/b.rs");
    assert!(repo.join("src/a/b.rs").is_file());
    let again = client.call(create(project_id, "src", "a/b.rs", false));
    assert_eq!(error_code(again), ErrorCode::AlreadyExists);
    for (dir, name) in [
        ("..", "x"),
        ("", "../x"),
        (".git", "x"),
        ("", ".git/hooks/x"),
        ("", " .git \\hooks/x"),
    ] {
        let refused = client.call(create(project_id, dir, name, true));
        assert_eq!(
            error_code(refused),
            ErrorCode::InvalidRequest,
            "{dir} {name}"
        );
    }
    assert!(!repo.parent().unwrap().join("x").exists());
    assert!(!repo.join(".git/hooks/x").exists());
    client.stop();
}

#[test]
fn fs_ignore_appends_once_and_keeps_line_endings() {
    let env = Env::new();
    let repo = env.repo();
    init_repo(&repo);
    std::fs::write(repo.join(".gitignore"), "target\r\n").unwrap();
    let (_daemon, mut client) = env.start();
    let project_id = client.add_project(&repo).id;
    for _ in 0..2 {
        client
            .call(Request::FsIgnore(FsIgnoreParams {
                project_id,
                worktree_task_id: None,
                path: "build".into(),
                is_dir: true,
            }))
            .into_result::<()>()
            .unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(repo.join(".gitignore")).unwrap(),
        "target\r\n/build/\r\n"
    );
    let refused = client.call(Request::FsIgnore(FsIgnoreParams {
        project_id,
        worktree_task_id: None,
        path: ".git/x".into(),
        is_dir: false,
    }));
    assert_eq!(error_code(refused), ErrorCode::InvalidRequest);
    client.stop();
}
