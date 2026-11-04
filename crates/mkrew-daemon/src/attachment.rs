//! Pasted images (SPEC §20B.5): `attachment.save` writes `<dir>/<session_id>/<ulid>.<ext>` or
//! `<dir>/projects/<project_id>/<ulid>.<ext>`; the hourly backup check ([`crate::backup`])
//! deletes the dirs of sessions that ended more than [`KEEP_MS`] ago and, per file, the
//! project images no open task names ([`cleanup_projects`]).

use std::io;
use std::path::{Path, PathBuf};

use mkrew_protocol::{AttachmentOwner, TaskStatus};
use mkrew_store::Store;
use ulid::Ulid;

/// The folder under the attachments dir that holds project images; it is not a ULID, so the
/// session cleanup skips it.
pub const PROJECTS: &str = "projects";

/// Accepted file extensions, compared exactly: clients send them lowercase.
pub const EXTS: [&str; 4] = ["png", "jpg", "gif", "webp"];
/// Largest accepted file: 10 MiB.
pub const MAX_BYTES: usize = 10 * 1024 * 1024;
/// Attachments, session logs and old events (`retention`) are kept this long after the
/// session ended: 7 days.
pub const KEEP_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// Why `attachment.save` params are rejected, as a message for `invalid_request`.
pub fn check(ext: &str, len: usize) -> Result<(), String> {
    if !EXTS.contains(&ext) {
        return Err(format!(
            "ext must be one of {}, got {ext:?}",
            EXTS.join(", ")
        ));
    }
    if len == 0 {
        return Err("data is empty".into());
    }
    if len > MAX_BYTES {
        return Err(format!("data is {len} bytes, the limit is {MAX_BYTES}"));
    }
    Ok(())
}

/// Writes `data` to a new file in the owner's dir and returns its path. `ext` must have
/// passed [`check`].
pub fn save(dir: &Path, owner: AttachmentOwner, ext: &str, data: &[u8]) -> io::Result<PathBuf> {
    let dir = match owner {
        AttachmentOwner::Session(id) => dir.join(id.to_string()),
        AttachmentOwner::Project(id) => dir.join(PROJECTS).join(id.to_string()),
    };
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.{ext}", Ulid::new()));
    std::fs::write(&path, data)?;
    Ok(path)
}

/// Deletes the dir of every session that ended more than [`KEEP_MS`] before `now_ms`;
/// `ended_at` gives a session's end (`None` = live or unknown, kept). Other entries are left
/// alone. A dir that cannot be deleted (e.g. a file still open on Windows) is logged and
/// retried at the next check; the others still go. Returns the deleted dirs.
pub fn cleanup(
    dir: &Path,
    now_ms: i64,
    mut ended_at: impl FnMut(Ulid) -> Option<i64>,
) -> io::Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut removed = Vec::new();
    for entry in entries.flatten() {
        let Some(id) = entry
            .file_name()
            .to_str()
            .and_then(|name| Ulid::from_string(name).ok())
        else {
            continue;
        };
        let expired = ended_at(id).is_some_and(|ended| now_ms - ended > KEEP_MS);
        if expired && entry.file_type().is_ok_and(|t| t.is_dir()) {
            let path = entry.path();
            match std::fs::remove_dir_all(&path) {
                Ok(()) => removed.push(path),
                Err(e) => tracing::warn!(path = %path.display(), "delete attachments: {e}"),
            }
        }
    }
    Ok(removed)
}

/// Prompts of `project`'s tasks that keep a project image alive: tasks that are not done
/// (Todo, Running, NeedsInput, Review) and done tasks updated no more than [`KEEP_MS`] before
/// `now_ms`, so an image goes 7 days after its task is done, like a session's files.
/// `None` when the tasks cannot be read: the caller must then keep every file.
pub fn open_task_prompts(store: &Store, project: Ulid, now_ms: i64) -> Option<Vec<String>> {
    match store.list_tasks(Some(project), None) {
        Ok(tasks) => Some(
            tasks
                .into_iter()
                .filter(|task| {
                    task.status != TaskStatus::Done || now_ms - task.updated_at <= KEEP_MS
                })
                .map(|task| task.prompt)
                .collect(),
        ),
        Err(e) => {
            tracing::warn!(%project, "read tasks for attachment cleanup: {e}");
            None
        }
    }
}

/// When a project image was made: the time in the ULID that [`save`] gave its name.
fn created_ms(path: &Path) -> Option<i64> {
    let stem = path.file_stem()?.to_str()?;
    i64::try_from(Ulid::from_string(stem).ok()?.timestamp_ms()).ok()
}

/// Deletes the files of `<dir>/projects/<project_id>/` that are more than [`KEEP_MS`] old
/// before `now_ms` and whose name no prompt of `open_prompts(project)` contains (SPEC §20B.5).
/// `open_prompts` is asked once per project and only when it has an old file; `None` (the
/// tasks could not be read) keeps the project's files. Entries whose names are not ULIDs are
/// left alone. A file that cannot be deleted is logged and retried at the next check.
/// Returns the deleted files.
pub fn cleanup_projects(
    dir: &Path,
    now_ms: i64,
    mut open_prompts: impl FnMut(Ulid) -> Option<Vec<String>>,
) -> io::Result<Vec<PathBuf>> {
    let projects = match std::fs::read_dir(dir.join(PROJECTS)) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut removed = Vec::new();
    for entry in projects.flatten() {
        let Some(project) = entry
            .file_name()
            .to_str()
            .and_then(|name| Ulid::from_string(name).ok())
        else {
            continue;
        };
        let files = match std::fs::read_dir(entry.path()) {
            Ok(files) => files,
            Err(e) if e.kind() == io::ErrorKind::NotADirectory => continue,
            Err(e) => {
                tracing::warn!(path = %entry.path().display(), "read attachments: {e}");
                continue;
            }
        };
        let mut prompts: Option<Option<Vec<String>>> = None;
        for file in files.flatten() {
            let path = file.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let Some(created) = created_ms(&path) else {
                continue;
            };
            if now_ms - created <= KEEP_MS {
                continue;
            }
            if !file.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            let open = prompts.get_or_insert_with(|| open_prompts(project));
            let Some(open) = open.as_ref() else {
                continue;
            };
            if open.iter().any(|prompt| prompt.contains(name)) {
                continue;
            }
            match std::fs::remove_file(&path) {
                Ok(()) => removed.push(path),
                Err(e) => tracing::warn!(path = %path.display(), "delete attachment: {e}"),
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    use mkrew_protocol::TaskStatus;
    use mkrew_store::Store;

    #[test]
    fn only_whitelisted_non_empty_files_up_to_10_mib_pass() {
        for ext in EXTS {
            assert_eq!(check(ext, 1), Ok(()));
        }
        assert_eq!(check("png", MAX_BYTES), Ok(()));
        assert!(check("png", MAX_BYTES + 1).is_err());
        assert!(check("png", 0).is_err());
        for ext in ["PNG", "jpeg", "exe", "", "png/../x", "../png"] {
            assert!(check(ext, 1).is_err(), "{ext}");
        }
    }

    #[test]
    fn saved_files_land_in_the_session_dir_under_new_names() {
        let dir = tempfile::tempdir().unwrap();
        let session = Ulid::new();
        let first = save(dir.path(), AttachmentOwner::Session(session), "png", b"one").unwrap();
        let second = save(dir.path(), AttachmentOwner::Session(session), "png", b"two").unwrap();
        assert_ne!(first, second);
        for (path, data) in [(&first, "one"), (&second, "two")] {
            assert_eq!(path.parent().unwrap(), dir.path().join(session.to_string()));
            assert_eq!(path.extension().unwrap(), "png");
            let stem = path.file_stem().unwrap().to_str().unwrap();
            assert!(Ulid::from_string(stem).is_ok(), "{stem}");
            assert_eq!(std::fs::read_to_string(path).unwrap(), data);
        }
    }

    #[test]
    fn project_files_land_in_the_projects_dir() {
        let dir = tempfile::tempdir().unwrap();
        let project = Ulid::new();
        let path = save(
            dir.path(),
            AttachmentOwner::Project(project),
            "jpg",
            b"shot",
        )
        .unwrap();
        assert_eq!(
            path.parent().unwrap(),
            dir.path().join(PROJECTS).join(project.to_string())
        );
        assert_eq!(path.extension().unwrap(), "jpg");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "shot");
    }

    #[test]
    fn session_cleanup_ignores_the_projects_dir() {
        let dir = tempfile::tempdir().unwrap();
        let project = Ulid::new();
        save(dir.path(), AttachmentOwner::Project(project), "png", b"x").unwrap();
        // Even when every session looks long dead, `projects` is not a session dir.
        let removed = cleanup(dir.path(), 100 * KEEP_MS, |_| Some(0)).unwrap();
        assert!(removed.is_empty());
        assert!(dir.path().join(PROJECTS).join(project.to_string()).is_dir());
    }

    #[test]
    fn cleanup_deletes_only_sessions_ended_more_than_7_days_ago() {
        let dir = tempfile::tempdir().unwrap();
        let now = 100 * KEEP_MS;
        let (old, recent, live, unknown) = (Ulid::new(), Ulid::new(), Ulid::new(), Ulid::new());
        for id in [old, recent, live, unknown] {
            save(dir.path(), AttachmentOwner::Session(id), "png", b"x").unwrap();
        }
        std::fs::create_dir(dir.path().join("notes")).unwrap();
        std::fs::write(dir.path().join(Ulid::new().to_string()), "a file").unwrap();

        let removed = cleanup(dir.path(), now, |id| {
            if id == old {
                Some(now - KEEP_MS - 1)
            } else if id == recent {
                Some(now - KEEP_MS)
            } else {
                None
            }
        })
        .unwrap();

        assert_eq!(removed, [dir.path().join(old.to_string())]);
        assert!(!dir.path().join(old.to_string()).exists());
        for id in [recent, live, unknown] {
            assert!(dir.path().join(id.to_string()).is_dir(), "{id}");
        }
        assert!(dir.path().join("notes").is_dir());
    }

    #[test]
    fn cleanup_without_the_dir_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("attachments");
        assert!(cleanup(&missing, 0, |_| Some(0)).unwrap().is_empty());
    }

    const NOW: i64 = 100 * KEEP_MS;

    /// A project image created `age_ms` before [`NOW`]; the ULID in the name carries the age.
    fn project_file(dir: &Path, project: Ulid, age_ms: i64) -> (PathBuf, String) {
        let created = u64::try_from(NOW - age_ms).unwrap();
        let name = format!("{}.png", Ulid::from_parts(created, 1));
        let folder = dir.join(PROJECTS).join(project.to_string());
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join(&name);
        std::fs::write(&path, b"x").unwrap();
        (path, name)
    }

    #[test]
    fn project_files_older_than_7_days_go_unless_a_prompt_names_them() {
        let dir = tempfile::tempdir().unwrap();
        let project = Ulid::new();
        let (unused, _) = project_file(dir.path(), project, KEEP_MS + 1);
        let (named, named_name) = project_file(dir.path(), project, KEEP_MS + 2);
        let (boundary, _) = project_file(dir.path(), project, KEEP_MS);
        let (fresh, _) = project_file(dir.path(), project, 1);

        let removed = cleanup_projects(dir.path(), NOW, |id| {
            assert_eq!(id, project);
            Some(vec![
                "Fix login".into(),
                format!("see \"C:\\a b\\{named_name}\" too"),
            ])
        })
        .unwrap();

        assert_eq!(removed, std::slice::from_ref(&unused));
        assert!(!unused.exists());
        for kept in [named, boundary, fresh] {
            assert!(kept.exists(), "{}", kept.display());
        }
    }

    #[test]
    fn an_unreadable_task_list_keeps_every_file() {
        let dir = tempfile::tempdir().unwrap();
        let project = Ulid::new();
        let (old, _) = project_file(dir.path(), project, 3 * KEEP_MS);
        let removed = cleanup_projects(dir.path(), NOW, |_| None).unwrap();
        assert!(removed.is_empty());
        assert!(old.exists());
    }

    #[test]
    fn the_tasks_are_read_only_for_projects_with_old_files() {
        let dir = tempfile::tempdir().unwrap();
        let (fresh, _) = project_file(dir.path(), Ulid::new(), 1);
        let calls = Cell::new(0);
        cleanup_projects(dir.path(), NOW, |_| {
            calls.set(calls.get() + 1);
            Some(Vec::new())
        })
        .unwrap();
        assert_eq!(calls.get(), 0);
        assert!(fresh.exists());
    }

    #[test]
    fn stray_entries_under_projects_are_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let project = Ulid::new();
        let folder = dir.path().join(PROJECTS).join(project.to_string());
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("notes.txt"), "mine").unwrap();
        std::fs::write(folder.join("not-a-ulid.png"), "mine").unwrap();
        std::fs::create_dir_all(dir.path().join(PROJECTS).join("scratch")).unwrap();
        std::fs::write(dir.path().join(PROJECTS).join("readme.txt"), "mine").unwrap();

        let removed = cleanup_projects(dir.path(), NOW, |_| Some(Vec::new())).unwrap();

        assert!(removed.is_empty());
        assert!(folder.join("notes.txt").is_file());
        assert!(folder.join("not-a-ulid.png").is_file());
        assert!(dir.path().join(PROJECTS).join("scratch").is_dir());
        assert!(dir.path().join(PROJECTS).join("readme.txt").is_file());
    }

    #[test]
    fn cleaning_projects_without_the_dir_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let removed = cleanup_projects(&dir.path().join("attachments"), NOW, |_| None).unwrap();
        assert!(removed.is_empty());
    }

    fn store_with_project(dir: &Path, name: &str) -> (Store, mkrew_protocol::Project) {
        let store = Store::open(&dir.join("mkrew.db")).unwrap();
        let repo = dir.join(name);
        std::fs::create_dir_all(&repo).unwrap();
        let project = store.add_project(&repo, None).unwrap();
        (store, project)
    }

    #[test]
    fn open_task_prompts_lists_every_prompt_but_done_tasks() {
        let data = tempfile::tempdir().unwrap();
        let (store, project) = store_with_project(data.path(), "repo");
        for (title, status) in [
            ("todo", TaskStatus::Todo),
            ("run", TaskStatus::Running),
            ("input", TaskStatus::NeedsInput),
            ("review", TaskStatus::Review),
            ("done", TaskStatus::Done),
        ] {
            let task = store
                .create_task(
                    project.id,
                    title,
                    &format!("prompt {title}"),
                    None,
                    None,
                    false,
                )
                .unwrap();
            store.set_task_status(task.id, status, false).unwrap();
        }
        let other = data.path().join("other");
        std::fs::create_dir_all(&other).unwrap();
        let other = store.add_project(&other, None).unwrap();
        store
            .create_task(other.id, "x", "prompt other", None, None, false)
            .unwrap();

        let mut prompts = open_task_prompts(&store, project.id, i64::MAX).unwrap();
        prompts.sort();
        assert_eq!(
            prompts,
            ["prompt input", "prompt review", "prompt run", "prompt todo"]
        );
        assert_eq!(
            open_task_prompts(&store, Ulid::new(), i64::MAX),
            Some(Vec::new())
        );
    }

    /// SPEC §20B.5: a task in the backlog keeps its screenshot; a done task's goes 7 days
    /// after it is done.
    #[test]
    fn retention_keeps_the_screenshot_of_a_task_until_7_days_after_it_is_done() {
        let data = tempfile::tempdir().unwrap();
        let attachments = data.path().join("attachments");
        let (store, project) = store_with_project(data.path(), "repo");
        let (path, _) = project_file(&attachments, project.id, 30 * 24 * 60 * 60 * 1000);
        let task = store
            .create_task(
                project.id,
                "Fix",
                &format!("Look at {}", path.display()),
                None,
                None,
                false,
            )
            .unwrap();
        // The file is 30 days old at NOW; `open_at` is the clock the task check sees.
        let check = |open_at: i64| {
            cleanup_projects(&attachments, NOW, |id| {
                open_task_prompts(&store, id, open_at)
            })
            .unwrap()
        };

        assert!(check(NOW).is_empty(), "a Todo task keeps it");
        store
            .set_task_status(task.id, TaskStatus::Review, false)
            .unwrap();
        assert!(check(NOW).is_empty(), "a Review task keeps it");
        store
            .set_task_status(task.id, TaskStatus::Done, false)
            .unwrap();
        let done_at = store.list_tasks(Some(project.id), None).unwrap()[0].updated_at;

        assert!(
            check(done_at + 24 * 60 * 60 * 1000).is_empty(),
            "done 1 day ago keeps it"
        );
        assert!(
            check(done_at + KEEP_MS).is_empty(),
            "done exactly 7 days ago keeps it"
        );
        assert!(path.exists());
        assert_eq!(check(done_at + KEEP_MS + 1), std::slice::from_ref(&path));
        assert!(!path.exists());
    }
}
