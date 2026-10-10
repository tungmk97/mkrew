//! Plan checklists (SPEC §7 `session.link_plan`, ADR 0039): one file watcher per live
//! session with a linked plan. A change re-reads the file, parses it ([`mkrew_core::plan`])
//! and announces `session.plan` only when the checklist differs from the last one announced.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use mkrew_protocol::{Event, SessionPlanEvent};
use tokio::sync::broadcast;
use ulid::Ulid;

use crate::docs;
use crate::settings::{Watch, watch_file};

/// The last announced checklist and the watch of one linked session.
struct Linked {
    project_id: Ulid,
    last: Arc<Mutex<SessionPlanEvent>>,
    _watch: Option<Watch>,
}

/// The plans of the live sessions, by session id. Never held with another lock.
#[derive(Default)]
pub struct Plans {
    linked: Mutex<HashMap<Ulid, Linked>>,
}

impl Plans {
    /// Links `file` as the plan of `session_id` (replacing an earlier link): announces its
    /// checklist at once and again after each change. `None` unlinks and stops the watch.
    /// Must run inside the tokio runtime.
    pub fn set(
        &self,
        events: &broadcast::Sender<Event>,
        session_id: Ulid,
        project_id: Ulid,
        file: Option<PathBuf>,
    ) {
        lock(&self.linked).remove(&session_id);
        let Some(file) = file else {
            return;
        };
        let mut first = snapshot(session_id, &file);
        let last = Arc::new(Mutex::new(first.clone()));
        let on_change = {
            let (events, last, file) = (events.clone(), Arc::downgrade(&last), file.clone());
            move || {
                // Unlinked meanwhile: a change still pending in the debounce reports nothing.
                let Some(last) = last.upgrade() else {
                    return;
                };
                let next = snapshot(session_id, &file);
                let mut current = lock(&last);
                if *current != next {
                    *current = next.clone();
                    let _ = events.send(Event::SessionPlan(next));
                }
            }
        };
        let watch = match watch_file(&file, on_change) {
            Ok(watch) => Some(watch),
            Err(e) => {
                tracing::warn!(%session_id, file = %file.display(), "cannot watch the plan: {e}");
                first.error = Some(format!("cannot watch the file: {e}"));
                (first.done, first.total, first.items) = (0, 0, Vec::new());
                *lock(&last) = first.clone();
                None
            }
        };
        let _ = events.send(Event::SessionPlan(first));
        lock(&self.linked).insert(
            session_id,
            Linked {
                project_id,
                last,
                _watch: watch,
            },
        );
    }

    /// Stops the watch of an exiting session.
    pub fn stop(&self, session_id: Ulid) {
        lock(&self.linked).remove(&session_id);
    }

    /// The last announced checklist of each linked live session of the project.
    pub fn for_project(&self, project_id: Ulid) -> Vec<SessionPlanEvent> {
        let mut plans: Vec<SessionPlanEvent> = lock(&self.linked)
            .values()
            .filter(|linked| linked.project_id == project_id)
            .map(|linked| lock(&linked.last).clone())
            .collect();
        plans.sort_by_key(|plan| plan.session_id);
        plans
    }
}

/// A client's plan path as `(stored form, file)`: relative to `root` with `/` when it lies
/// inside the project (the folder can move), else as given (absolute). `Err` = the message for
/// `invalid_request`: not `.md`, missing, a folder, too large, not UTF-8 text.
// ponytail: a path that differs from `root` only in case or `\\?\` form is stored absolute;
// it still works, it just does not follow a moved folder.
pub fn locate(root: &Path, picked: &str) -> Result<(String, PathBuf), String> {
    let picked = picked.trim();
    if !docs::is_markdown(picked) {
        return Err("a plan must be a .md file".to_owned());
    }
    let mut file = PathBuf::new();
    for part in root.join(picked).components() {
        match part {
            Component::ParentDir => {
                file.pop();
            }
            other => file.push(other),
        }
    }
    docs::read(&file).map_err(|e| format!("cannot read the plan: {e}"))?;
    let stored = match file.strip_prefix(root) {
        Ok(relative) => relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => file.to_string_lossy().into_owned(),
    };
    Ok((stored, file))
}

/// The checklist of `file` now; unreadable = no counts and the reason.
pub fn snapshot(session_id: Ulid, file: &Path) -> SessionPlanEvent {
    let name = file
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    match docs::read(file) {
        Ok(text) => {
            let list = mkrew_core::plan::parse(&text);
            SessionPlanEvent {
                session_id,
                name,
                done: list.done,
                total: list.total,
                items: list.items,
                error: None,
            }
        }
        Err(e) => SessionPlanEvent {
            session_id,
            name,
            done: 0,
            total: 0,
            items: Vec::new(),
            error: Some(e.to_string()),
        },
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A map and a last event: still consistent if a holder panicked.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// Inside the project the path is stored relative with `/` (the folder can move); outside
    /// it stays absolute. A relative request resolves against the project folder.
    #[test]
    fn locate_stores_project_files_relative() {
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("docs")).unwrap();
        std::fs::write(root.path().join("docs/PLAN.md"), "- [ ] a\n").unwrap();
        std::fs::write(other.path().join("far.md"), "- [ ] a\n").unwrap();

        let inside = root.path().join("docs").join("PLAN.md");
        let expected = ("docs/PLAN.md".to_owned(), inside.clone());
        assert_eq!(
            locate(root.path(), inside.to_str().unwrap()).unwrap(),
            expected
        );
        assert_eq!(locate(root.path(), " docs/PLAN.md ").unwrap(), expected);

        let far = other.path().join("far.md");
        let (stored, file) = locate(root.path(), far.to_str().unwrap()).unwrap();
        assert_eq!((PathBuf::from(stored), file), (far.clone(), far));

        // `..` is resolved first: leaving the project stays absolute, coming back is relative.
        let back = root.path().join("docs/../docs/PLAN.md");
        assert_eq!(
            locate(root.path(), back.to_str().unwrap()).unwrap(),
            expected
        );
        let side = tempfile::tempdir().unwrap();
        let project = side.path().join("proj");
        std::fs::create_dir(&project).unwrap();
        std::fs::write(side.path().join("near.md"), "- [ ] a\n").unwrap();
        let (stored, file) = locate(&project, "../near.md").unwrap();
        assert_eq!(PathBuf::from(&stored), side.path().join("near.md"));
        assert_eq!(file, side.path().join("near.md"));
    }

    #[test]
    fn locate_refuses_what_is_not_a_readable_markdown_file() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("notes.txt"), "- [ ] a\n").unwrap();
        std::fs::create_dir(root.path().join("dir.md")).unwrap();
        for bad in ["notes.txt", "missing.md", "dir.md", "", "   "] {
            assert!(locate(root.path(), bad).is_err(), "{bad:?}");
        }
        std::fs::write(
            root.path().join("big.md"),
            vec![b'a'; docs::MAX_BYTES as usize + 1],
        )
        .unwrap();
        let error = locate(root.path(), "big.md").unwrap_err();
        assert!(error.contains("at most"), "{error}");
    }

    /// A plan that cannot be read keeps its name and says why; the link stays, so the counts
    /// return when the file does.
    #[test]
    fn snapshot_reports_a_missing_file_instead_of_counts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("PLAN.md");
        let id = Ulid::new();

        let gone = snapshot(id, &path);
        assert_eq!(
            (gone.name.as_str(), gone.total, gone.error.as_deref()),
            ("PLAN", 0, Some("no such file"))
        );
        assert!(gone.items.is_empty());

        std::fs::write(&path, "- [x] a\n- [ ] b\n").unwrap();
        let read = snapshot(id, &path);
        assert_eq!((read.done, read.total, read.error), (1, 2, None));
        assert_eq!(read.items.len(), 2);
    }
}
