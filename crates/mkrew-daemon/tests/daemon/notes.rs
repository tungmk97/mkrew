//! `fs.new_note` and the notes-folder exception of `fs.read` / `fs.write` (Tern panes spec
//! 2026-11-23 section 7.2).

use std::collections::BTreeMap;

use super::*;

fn new_note(project_id: Ulid) -> Request {
    Request::FsNewNote(FsNewNoteParams {
        project_id,
        worktree_task_id: None,
    })
}

fn read(project_id: Ulid, path: &str) -> Request {
    Request::FsRead(FsReadParams {
        project_id,
        worktree_task_id: None,
        path: path.into(),
        index_text: false,
    })
}

fn write(project_id: Ulid, path: &str, content: &str, expected_hash: &str) -> Request {
    Request::FsWrite(FsWriteParams {
        project_id,
        worktree_task_id: None,
        path: path.into(),
        content: content.into(),
        expected_hash: expected_hash.into(),
    })
}

/// `/`-separated, like the paths the daemon answers.
fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap()
}

/// `note-YYYYMMDD-HHMMSS.md`, or with `-N` (N >= 2) before `.md`.
fn is_note_name(name: &str) -> bool {
    let Some(stem) = name
        .strip_prefix("note-")
        .and_then(|rest| rest.strip_suffix(".md"))
    else {
        return false;
    };
    let mut parts = stem.splitn(3, '-');
    let digits = |part: Option<&str>, len: usize| {
        part.is_some_and(|p| p.len() == len && p.bytes().all(|b| b.is_ascii_digit()))
    };
    let (date, time, number) = (parts.next(), parts.next(), parts.next());
    digits(date, 8)
        && digits(time, 6)
        && number.is_none_or(|n| n.parse::<u32>().is_ok_and(|n| n >= 2))
}

fn daemon_error(outcome: Outcome) -> mkrew_protocol::ErrorBody {
    match outcome.into_result::<serde_json::Value>() {
        Err(ResponseError::Daemon(body)) => body,
        other => panic!("expected a daemon error, got {other:?}"),
    }
}

#[test]
fn a_new_note_in_the_root_is_empty_named_by_time_and_answered_relative() {
    let env = Env::new();
    let repo = env.repo();
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;

    let made: FsCreated = client.call(new_note(id)).into_result().unwrap();
    assert!(is_note_name(&made.path), "{}", made.path);
    assert!(
        !made.path.contains('/'),
        "a note in the root has no folder: {}",
        made.path
    );
    assert_eq!(std::fs::read(repo.join(&made.path)).unwrap(), b"");
    let opened: FsContent = client.call(read(id, &made.path)).into_result().unwrap();
    assert_eq!(opened.content.as_deref(), Some(""));

    // An absolute path stays refused when no absolute notes folder is set, as before.
    let absolute = slash(&repo.join("a.txt"));
    assert_eq!(
        error_code(client.call(read(id, &absolute))),
        ErrorCode::InvalidRequest
    );
    client.stop();
}

#[test]
fn notes_made_in_the_same_second_get_numbers() {
    let env = Env::new();
    let repo = env.repo();
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;

    let mut paths = Vec::new();
    for _ in 0..5 {
        let made: FsCreated = client.call(new_note(id)).into_result().unwrap();
        paths.push(made.path);
    }
    // Group by the stamp: inside one second the names run base, -2, -3, ... in call order.
    let mut seconds: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    for path in &paths {
        assert!(is_note_name(path), "{path}");
        let parts: Vec<&str> = path.strip_suffix(".md").unwrap().split('-').collect();
        let number = parts.get(3).map_or(1, |n| n.parse().unwrap());
        seconds
            .entry(parts[..3].join("-"))
            .or_default()
            .push(number);
        assert!(repo.join(path).is_file(), "{path}");
    }
    for (stamp, numbers) in &seconds {
        let want: Vec<u32> = (1..=u32::try_from(numbers.len()).unwrap()).collect();
        assert_eq!(numbers, &want, "{stamp}");
    }
    assert!(
        seconds.values().any(|numbers| numbers.len() >= 2),
        "five calls cannot all fall in different seconds: {paths:?}"
    );
    client.stop();
}

#[test]
fn a_relative_folder_is_trimmed_made_and_answered_relative() {
    let env = Env::new();
    let repo = env.repo();
    // Spaces around it, forward slashes and a trailing one: what a user types.
    env.set_notes_folder(" notes/inbox/ ");
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;

    let made: FsCreated = client.call(new_note(id)).into_result().unwrap();
    assert!(made.path.starts_with("notes/inbox/"), "{}", made.path);
    assert!(is_note_name(file_name(&made.path)), "{}", made.path);
    assert!(
        repo.join("notes")
            .join("inbox")
            .join(file_name(&made.path))
            .is_file()
    );
    let opened: FsContent = client.call(read(id, &made.path)).into_result().unwrap();
    assert_eq!(opened.content.as_deref(), Some(""));
    client.stop();
}

#[test]
fn an_absolute_folder_inside_the_root_is_answered_relative() {
    let env = Env::new();
    let repo = env.repo();
    let notes = repo.join("notes");
    env.set_notes_folder(&slash(&notes));
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;

    let made: FsCreated = client.call(new_note(id)).into_result().unwrap();
    assert!(made.path.starts_with("notes/"), "{}", made.path);
    assert!(notes.join(file_name(&made.path)).is_file());
    assert!(!made.path.contains('\\'), "{}", made.path);
    // The absolute spelling of the same file opens too.
    let absolute = format!("{}/{}", slash(&notes), file_name(&made.path));
    let opened: FsContent = client.call(read(id, &absolute)).into_result().unwrap();
    assert_eq!(opened.content.as_deref(), Some(""));
    client.stop();
}

#[test]
fn an_absolute_folder_outside_the_root_is_answered_absolute_and_opens_and_saves() {
    let env = Env::new();
    let repo = env.repo();
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    let notes = env.dir.path().join("notes").join("deep");
    let outside = env.dir.path().join("other.md");
    let secret = env.dir.path().join("notes").join("secret.md");
    std::fs::create_dir_all(secret.parent().unwrap()).unwrap();
    std::fs::write(&outside, "outside\n").unwrap();
    std::fs::write(&secret, "secret\n").unwrap();
    // Forward slashes and a trailing separator, as a user may type it; the folder is missing.
    env.set_notes_folder(&format!("{}/", slash(&notes)));
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;

    let made: FsCreated = client.call(new_note(id)).into_result().unwrap();
    assert_eq!(
        made.path,
        format!("{}/{}", slash(&notes), file_name(&made.path))
    );
    assert!(is_note_name(file_name(&made.path)), "{}", made.path);
    assert!(notes.join(file_name(&made.path)).is_file());

    // The absolute path in the folder opens and saves.
    let opened: FsContent = client.call(read(id, &made.path)).into_result().unwrap();
    assert_eq!(opened.content.as_deref(), Some(""));
    assert_eq!(opened.path, made.path);
    assert!(
        !made.path.contains('\\') && !opened.path.contains('\\'),
        "{} {}",
        made.path,
        opened.path
    );
    let hash = opened.hash.expect("text has a hash");
    let saved: FsWritten = client
        .call(write(id, &made.path, "# a note\n", &hash))
        .into_result()
        .unwrap();
    assert!(saved.saved);
    assert_eq!(
        std::fs::read_to_string(notes.join(file_name(&made.path))).unwrap(),
        "# a note\n"
    );
    // The backslash spelling is the same place.
    let backslashed = made.path.replace('/', "\\");
    let again: FsContent = client.call(read(id, &backslashed)).into_result().unwrap();
    assert_eq!(again.content.as_deref(), Some("# a note\n"));
    assert!(!again.path.contains('\\'), "{}", again.path);
    assert_eq!(again.path, made.path);

    // Everything else is refused as today.
    let dotdot = format!("{}/../secret.md", slash(&notes));
    for path in [
        slash(&outside),
        slash(&secret),
        dotdot,
        slash(&notes),
        format!("{}/", slash(&notes)),
        slash(&repo.join("a.txt")),
        "../x".into(),
        "..\\other.md".into(),
        format!("{}/nope.md", slash(&notes)).replace("deep/nope.md", "deep/../nope.md"),
    ] {
        assert_eq!(
            error_code(client.call(read(id, &path))),
            ErrorCode::InvalidRequest,
            "read {path}"
        );
        assert_eq!(
            error_code(client.call(write(id, &path, "x", &hash))),
            ErrorCode::InvalidRequest,
            "write {path}"
        );
    }
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "outside\n");
    assert_eq!(std::fs::read_to_string(&secret).unwrap(), "secret\n");
    // A file of the root still opens by its relative path.
    let root_file: FsContent = client.call(read(id, "a.txt")).into_result().unwrap();
    assert_eq!(root_file.content.as_deref(), Some("a\n"));
    client.stop();
}

#[test]
fn unusable_folders_are_refused_with_a_reason_and_nothing_is_made() {
    for (setting, reason) in [
        ("../outside", ".."),
        ("a/../../outside", ".."),
        ("~", "full path"),
        ("~/notes", "full path"),
        (".git/notes", ".git"),
    ] {
        let env = Env::new();
        let repo = env.repo();
        env.set_notes_folder(setting);
        let (_daemon, mut client) = env.start();
        let id = client.add_project(&repo).id;

        let body = daemon_error(client.call(new_note(id)));
        assert_eq!(body.code, ErrorCode::InvalidRequest, "{setting}");
        assert!(body.message.contains(reason), "{setting}: {}", body.message);
        assert!(!env.dir.path().join("outside").exists(), "{setting}");
        assert!(
            std::fs::read_dir(&repo).unwrap().next().is_none(),
            "{setting}: the root stays empty"
        );
        client.stop();
    }
}

#[test]
fn a_folder_that_cannot_be_made_is_an_error_answer_and_nothing_is_made() {
    let env = Env::new();
    let repo = env.repo();
    // A file where a parent folder should be: `create_dir_all` fails on every OS.
    let blocker = env.dir.path().join("blocker");
    std::fs::write(&blocker, "a file").unwrap();
    env.set_notes_folder(&slash(&blocker.join("notes")));
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;

    let body = daemon_error(client.call(new_note(id)));
    assert_eq!(body.code, ErrorCode::InvalidRequest);
    assert!(body.message.contains("notes folder"), "{}", body.message);
    assert!(blocker.is_file());
    assert!(std::fs::read_dir(&repo).unwrap().next().is_none());
    client.stop();
}

#[cfg(windows)]
#[test]
fn a_folder_on_a_drive_that_does_not_exist_is_an_error_answer() {
    let letter = ('D'..='Z')
        .find(|letter| !Path::new(&format!("{letter}:\\")).exists())
        .expect("a free drive letter");
    let env = Env::new();
    let repo = env.repo();
    env.set_notes_folder(&format!("{letter}:/notes/"));
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;

    let body = daemon_error(client.call(new_note(id)));
    assert_eq!(body.code, ErrorCode::InvalidRequest);
    assert!(!Path::new(&format!("{letter}:\\notes")).exists());
    assert!(std::fs::read_dir(&repo).unwrap().next().is_none());
    client.stop();
}
