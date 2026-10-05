//! Startup resume (SPEC §8): tasks the last daemon left running/needs_input get their agent
//! conversation back, or are flagged for the user to run again.

use mkrew_protocol::{Event, Session, Task, TaskStatus};
use serde_json::Value;
use ulid::Ulid;

use super::{Daemon, Start};

impl Daemon {
    /// For each running/needs_input task: relaunch its last session with the profile's
    /// `resume` command when the agent reported its own id, else move the card to needs_input
    /// with `error_flag` ("session stopped — run again?").
    pub(super) fn resume_tasks(&self) {
        let found = {
            let store = self.store();
            store
                .list_tasks(None, None)
                .and_then(|tasks| Ok((tasks, store.list_sessions(None)?)))
        };
        let (tasks, sessions) = match found {
            Ok(found) => found,
            Err(e) => {
                tracing::error!("cannot resume tasks: {e}");
                return;
            }
        };
        let interrupted = tasks
            .iter()
            .filter(|t| matches!(t.status, TaskStatus::Running | TaskStatus::NeedsInput));
        for task in interrupted {
            let resumed = last_session(&sessions, task.id).and_then(|last| self.resume(task, last));
            if resumed.is_none() {
                self.mark_stopped(task.id);
            }
        }
    }

    /// Spawns `last`'s resume command as a new session of `task` in `last`'s pane slot.
    fn resume(&self, task: &Task, last: &Session) -> Option<Ulid> {
        let native = last.native_session_id.as_deref()?;
        let profile = match self.load_profile(&last.agent_profile) {
            Ok(profile) => profile.resumed(native)?,
            Err(outcome) => {
                tracing::warn!(task_id = %task.id, "cannot resume: {outcome:?}");
                return None;
            }
        };
        let project = match self.store().get_project(task.project_id) {
            Ok(project) => project,
            Err(e) => {
                tracing::warn!(task_id = %task.id, "cannot resume: {e}");
                return None;
            }
        };
        let project_id = project.id;
        let start = Start {
            project,
            profile,
            task_id: Some(task.id),
            title: last.title.clone(),
            cols: last.cols,
            rows: last.rows,
            typed: None,
            // Resumable again even if the daemon stops before the agent's next hook.
            native_session_id: Some(native.to_owned()),
        };
        match self.start_session(start) {
            Ok(session_id) => {
                tracing::info!(task_id = %task.id, %session_id, native, "task resumed");
                self.replace_in_tabs(project_id, last.id, session_id);
                Some(session_id)
            }
            Err(outcome) => {
                tracing::warn!(task_id = %task.id, "cannot resume: {outcome:?}");
                None
            }
        }
    }

    fn mark_stopped(&self, task_id: Ulid) {
        let updated = self
            .store()
            .set_task_status(task_id, TaskStatus::NeedsInput, true);
        match updated {
            Ok(task) => {
                tracing::info!(%task_id, "session stopped — run again?");
                let _ = self.hub.events.send(Event::TaskUpdated(task));
            }
            Err(e) => tracing::warn!(%task_id, "task not flagged: {e}"),
        }
    }

    /// Puts `new` where `old` was in every tab layout of the project.
    fn replace_in_tabs(&self, project_id: Ulid, old: Ulid, new: Ulid) {
        let updated = {
            // One lock, so a concurrent `tab.set_layout` is not overwritten.
            let store = self.store();
            store.list_tabs(project_id).and_then(|tabs| {
                tabs.into_iter()
                    .filter_map(|tab| {
                        let layout = replace_session(&tab.layout, old, new)?;
                        Some(store.set_tab_layout(tab.id, &layout))
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
        };
        match updated {
            Ok(tabs) => {
                for tab in tabs {
                    let _ = self.hub.events.send(Event::TabUpdated(tab));
                }
            }
            Err(e) => tracing::warn!(%project_id, "resumed session not placed in its tab: {e}"),
        }
    }
}

/// The task's most recently started session.
fn last_session(sessions: &[Session], task_id: Ulid) -> Option<&Session> {
    sessions
        .iter()
        .filter(|s| s.task_id == Some(task_id))
        .max_by_key(|s| (s.started_at, s.id))
}

/// `layout` with every `{"session": old}` leaf naming `new`; `None` if `old` is not in it.
/// The layout is the UI's opaque split tree (SPEC §5), so only leaves are touched.
fn replace_session(layout: &Value, old: Ulid, new: Ulid) -> Option<Value> {
    fn walk(value: &mut Value, old: &str, new: &str) -> bool {
        match value {
            Value::Object(map) => {
                let mut found = false;
                for (key, child) in map.iter_mut() {
                    if key == "session" && child.as_str() == Some(old) {
                        *child = Value::String(new.to_owned());
                        found = true;
                    } else {
                        found |= walk(child, old, new);
                    }
                }
                found
            }
            Value::Array(items) => items
                .iter_mut()
                .fold(false, |found, item| walk(item, old, new) | found),
            _ => false,
        }
    }
    let mut layout = layout.clone();
    walk(&mut layout, &old.to_string(), &new.to_string()).then_some(layout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_protocol::{SessionKind, SessionState, StateSource};
    use serde_json::json;

    fn session(task_id: Option<Ulid>, started_at: i64) -> Session {
        Session {
            id: Ulid::new(),
            task_id,
            project_id: Ulid::new(),
            agent_profile: "fake".into(),
            kind: SessionKind::Agent,
            title: None,
            native_session_id: None,
            state: SessionState::Exited,
            state_source: StateSource::Daemon,
            state_changed_at: started_at,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at,
            ended_at: None,
        }
    }

    #[test]
    fn last_session_is_the_newest_of_the_task() {
        let task = Ulid::new();
        let sessions = [
            session(Some(task), 5),
            session(Some(task), 9),
            session(None, 20),
            session(Some(Ulid::new()), 30),
        ];
        assert_eq!(
            last_session(&sessions, task).map(|s| s.id),
            Some(sessions[1].id)
        );
        assert!(last_session(&sessions, Ulid::new()).is_none());
    }

    #[test]
    fn replace_session_swaps_matching_leaves_only() {
        let (a, b, new) = (Ulid::new(), Ulid::new(), Ulid::new());
        let layout = json!({"split": "vertical", "ratio": 0.5, "children": [
            {"session": a.to_string()},
            {"split": "horizontal", "ratio": 0.6, "children": [
                {"session": b.to_string()}, {"session": a.to_string()}
            ]}
        ]});
        let replaced = replace_session(&layout, a, new).unwrap();
        assert_eq!(
            replaced,
            json!({"split": "vertical", "ratio": 0.5, "children": [
                {"session": new.to_string()},
                {"split": "horizontal", "ratio": 0.6, "children": [
                    {"session": b.to_string()}, {"session": new.to_string()}
                ]}
            ]})
        );
        assert_eq!(replace_session(&layout, Ulid::new(), new), None);
        assert_eq!(replace_session(&Value::Null, a, new), None);
    }
}
