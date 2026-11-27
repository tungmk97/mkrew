//! Startup resume (SPEC §8): tasks the last daemon left running/needs_input, and agent panes
//! without a task that a tab still showed when the daemon stopped, get their agent
//! conversation back. A task that cannot resume is flagged for the user to run again. A
//! fan-out's original task is left alone. Also `session.resume` (History) and
//! `session.relaunch` (an omp pane switching between TSP and omp's own terminal UI).

use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use mkrew_protocol::{
    ErrorCode, Event, Outcome, Session, SessionKind, SessionRelaunchParams, SessionResumeParams,
    SessionSpawnResult, SessionState, Task, TaskStatus,
};
use serde_json::Value;
use ulid::Ulid;

use super::run::RESTART_WAIT;
use super::{Daemon, Start, ok, store_outcome, task_dir};
use crate::session::Command;

impl Daemon {
    /// For each running/needs_input task: relaunch its last session with the profile's
    /// `resume` command when the agent reported its own id, else move the card to needs_input
    /// with `error_flag` ("session stopped — run again?"). Then the same for every interrupted
    /// agent session without a task that is still in a tab; one that cannot resume stays
    /// exited. Last, what the stop interrupted and nothing brought back leaves the screen: its
    /// panes go (a tab left empty closes unless pinned), and its shells and run commands leave
    /// the session list (an agent stays in History, to resume by hand).
    pub(super) async fn resume_sessions(&self) {
        let found = {
            let store = self.store();
            store.take_interrupted_sessions().and_then(|interrupted| {
                Ok((
                    store.list_tasks(None, None)?,
                    store.list_sessions(None)?,
                    interrupted,
                    store.list_projects()?,
                ))
            })
        };
        let (tasks, sessions, interrupted, projects) = match found {
            Ok(found) => found,
            Err(e) => {
                tracing::error!("cannot resume sessions: {e}");
                return;
            }
        };
        // A removed workspace runs nothing; its tasks stay as they are, so adding the folder
        // again resumes them on the next start.
        let archived: Vec<Ulid> = projects
            .iter()
            .filter(|p| p.archived)
            .map(|p| p.id)
            .collect();
        // A fan-out's original runs nothing itself: its children are resumed (SPEC §20E.2).
        let fanned_out: Vec<Ulid> = tasks.iter().filter_map(|t| t.fanout_of).collect();
        let stopped_tasks = tasks.iter().filter(|t| {
            matches!(t.status, TaskStatus::Running | TaskStatus::NeedsInput)
                && !fanned_out.contains(&t.id)
                && !archived.contains(&t.project_id)
        });
        for task in stopped_tasks {
            let resumed = match last_session(&sessions, task.id) {
                Some(last) => self.resume(last, Some(task)).await,
                None => Err(no_resume("the task never ran")),
            };
            if let Err(outcome) = resumed {
                tracing::info!(task_id = %task.id, "task not resumed: {outcome:?}");
                self.mark_stopped(task.id);
            }
        }
        let panes = sessions.iter().filter(|s| {
            s.task_id.is_none()
                && s.kind == SessionKind::Agent
                && interrupted.contains(&s.id)
                && !archived.contains(&s.project_id)
        });
        for last in panes {
            if self.in_a_tab(last)
                && let Err(outcome) = self.resume(last, None).await
            {
                tracing::info!(session_id = %last.id, "pane not resumed: {outcome:?}");
            }
        }
        // Resumed panes already name their new session, so only the leftovers are cut.
        for project in projects.iter().filter(|p| !p.archived) {
            let left: Vec<Ulid> = sessions
                .iter()
                .filter(|s| s.project_id == project.id && interrupted.contains(&s.id))
                .map(|s| s.id)
                .collect();
            if !left.is_empty()
                && let Err(e) = self.hub.drop_panes(project.id, &left)
            {
                tracing::warn!(project_id = %project.id, "stopped panes not removed: {e}");
            }
        }
        let dead_shells = sessions.iter().filter(|s| {
            s.kind == SessionKind::Shell
                && interrupted.contains(&s.id)
                && !archived.contains(&s.project_id)
        });
        for shell in dead_shells {
            if let Err(e) = self.store().hide_session(shell.id) {
                tracing::warn!(session_id = %shell.id, "stopped shell not hidden: {e}");
            }
        }
    }

    /// `session.resume` (ADR 0055): the user picked an exited agent session in History. Refused
    /// while its conversation, or its task, already runs in a live session; the new pane goes
    /// where the old one still is, else into `tab_id` when that tab is empty (the client
    /// places it otherwise).
    pub(super) async fn session_resume(&self, params: SessionResumeParams) -> Outcome {
        let found = {
            let store = self.store();
            store.get_session(params.session_id).and_then(|last| {
                let sessions = store.list_sessions(Some(last.project_id))?;
                let task = last.task_id.map(|id| store.get_task(id)).transpose()?;
                let project = store.get_project(last.project_id)?;
                Ok((last, sessions, task, project.archived))
            })
        };
        let (last, sessions, task, archived) = match found {
            Ok(found) => found,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        let refused = if last.state != SessionState::Exited {
            Some("the session is still running")
        } else if last.kind != SessionKind::Agent {
            Some("only an agent session can resume")
        } else if archived {
            Some("the workspace was removed")
        } else if sessions.iter().any(|s| {
            s.state != SessionState::Exited
                && (s.native_session_id.is_some() && s.native_session_id == last.native_session_id
                    || task.is_some() && s.task_id == last.task_id)
        }) {
            Some("its conversation or task is already running")
        } else {
            None
        };
        if let Some(reason) = refused {
            return Outcome::error(ErrorCode::InvalidState, reason);
        }
        match self.resume(&last, task.as_ref()).await {
            Ok(session_id) => {
                if let Some(tab_id) = params.tab_id {
                    self.place_in_empty_tab(tab_id, session_id);
                }
                ok(&SessionSpawnResult { session_id })
            }
            Err(outcome) => outcome,
        }
    }

    /// `session.relaunch` (ADR 0120): stops the live session and resumes its conversation in
    /// the same pane with TSP on or off, saved on the pane (`"tsp"` on its layout leaf) so every
    /// later resume keeps it. Refused before anything stops while the agent works (the turn
    /// would be cut off) or when the session could not resume.
    pub(super) async fn session_relaunch(&self, params: SessionRelaunchParams) -> Outcome {
        let found = {
            let store = self.store();
            store.get_session(params.session_id).and_then(|last| {
                let task = last.task_id.map(|id| store.get_task(id)).transpose()?;
                Ok((last, task))
            })
        };
        let (last, task) = match found {
            Ok(found) => found,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        let refused = if last.kind != SessionKind::Agent {
            Some("only an agent session can relaunch")
        } else if last.state == SessionState::Exited {
            Some("the session has exited; resume it from History")
        } else if last.state == SessionState::Working {
            Some("the agent is working; wait until it is idle")
        } else {
            None
        };
        if let Some(reason) = refused {
            return Outcome::error(ErrorCode::InvalidState, reason);
        }
        if let Err(outcome) = self.resume_profile(&last) {
            return outcome;
        }
        let (project_id, id, tsp) = (last.project_id, last.id, params.tsp);
        if let Err(e) = (self.hub).edit_tabs(project_id, |l| set_pane_tsp(l, id, tsp)) {
            tracing::warn!(session_id = %id, "pane TSP choice not saved: {e}");
        }
        self.hub.keep_pane(id);
        self.hub.command(id, Command::Kill { force: false });
        if !self.wait_exited(&[id], RESTART_WAIT).await {
            self.hub.take_kept_pane(id);
            tracing::warn!(session_id = %id, "session did not stop for the relaunch");
            return Outcome::error(ErrorCode::Timeout, "the session did not stop within 10 s");
        }
        // Exited now, maybe with a newer native id than before the stop.
        let last = match self.store().get_session(id) {
            Ok(last) => last,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        match self.resume(&last, task.as_ref()).await {
            Ok(session_id) => ok(&SessionSpawnResult { session_id }),
            Err(outcome) => {
                // Nothing came back: the exited session's pane goes, as after any exit.
                self.hub.leave_tabs(project_id, id);
                outcome
            }
        }
    }

    /// The profile that resumes `last`'s conversation, with TSP as its pane saved it.
    fn resume_profile(&self, last: &Session) -> Result<mkrew_core::AgentProfile, Outcome> {
        let native = (last.native_session_id.as_deref())
            .ok_or_else(|| no_resume("the agent never reported its session id"))?;
        let mut profile = self
            .load_profile(&last.agent_profile)?
            .resumed(native)
            .ok_or_else(|| {
                no_resume(&format!(
                    "profile {} has no `resume` command",
                    last.agent_profile
                ))
            })?;
        if let Some(tsp) = self.pane_tsp(last) {
            profile.tsp = tsp;
        }
        Ok(profile)
    }

    /// Spawns `last`'s resume command as a new session (of `task`, if any) in `last`'s pane slot.
    async fn resume(&self, last: &Session, task: Option<&Task>) -> Result<Ulid, Outcome> {
        let profile = self.resume_profile(last)?;
        let native = last.native_session_id.clone();
        let project = self
            .store()
            .get_project(last.project_id)
            .map_err(|e| store_outcome::<()>(Err(e)))?;
        let project_id = project.id;
        // A worktree task resumes in its worktree (SPEC §20E.1), never in the project folder.
        let dir = match task.and_then(|t| t.worktree_path.as_ref()) {
            Some(root) => {
                let git = Arc::clone(&self.git);
                let (project_dir, root) = (PathBuf::from(&project.path), PathBuf::from(root));
                let found =
                    tokio::task::spawn_blocking(move || task_dir(&*git, &project_dir, Some(&root)))
                        .await
                        .unwrap_or_else(|e| Err(io::Error::other(e)));
                match found {
                    Ok(dir) => Some(dir),
                    Err(e) => return Err(no_resume(&format!("task folder: {e}"))),
                }
            }
            None => None,
        };
        let start = Start {
            project,
            profile,
            task_id: task.map(|t| t.id),
            title: last.title.clone(),
            color: last.color.clone(),
            pinned: last.pinned,
            position: last.position,
            cols: last.cols,
            rows: last.rows,
            typed: None,
            // Resumable again even if the daemon stops before the agent's next hook.
            native_session_id: native.clone(),
            dir,
            banner: None,
            env: Vec::new(),
            plan_path: last.plan_path.clone(),
        };
        let new_id = self.start_session(start)?;
        tracing::info!(session_id = %last.id, %new_id, ?native, "session resumed");
        self.replace_in_tabs(project_id, last.id, new_id);
        Ok(new_id)
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
        if let Err(e) = self
            .hub
            .edit_tabs(project_id, |l| replace_session(l, old, new))
        {
            tracing::warn!(%project_id, "resumed session not placed in its tab: {e}");
        }
    }

    /// The TSP choice saved on `session`'s pane, if a tab shows it with one.
    fn pane_tsp(&self, session: &Session) -> Option<bool> {
        let tabs = self.store().list_tabs(session.project_id).ok()?;
        tabs.iter()
            .find_map(|tab| leaf_tsp(&tab.layout, session.id))
    }

    /// Whether a tab of `session`'s project still shows it (the user did not close or hide it).
    fn in_a_tab(&self, session: &Session) -> bool {
        let tabs = self.store().list_tabs(session.project_id);
        match tabs {
            Ok(tabs) => tabs
                .iter()
                .any(|tab| replace_session(&tab.layout, session.id, session.id).is_some()),
            Err(e) => {
                tracing::warn!(session_id = %session.id, "tabs not read, pane not resumed: {e}");
                false
            }
        }
    }
}

fn no_resume(reason: &str) -> Outcome {
    Outcome::error(ErrorCode::InvalidState, format!("cannot resume: {reason}"))
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
pub(crate) fn replace_session(layout: &Value, old: Ulid, new: Ulid) -> Option<Value> {
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

/// The `"tsp"` flag of `session`'s leaf in `layout`.
fn leaf_tsp(layout: &Value, session: Ulid) -> Option<bool> {
    fn walk(value: &Value, id: &str) -> Option<bool> {
        match value {
            Value::Object(map) if map.get("session").and_then(Value::as_str) == Some(id) => {
                map.get("tsp").and_then(Value::as_bool)
            }
            Value::Object(map) => map.values().find_map(|child| walk(child, id)),
            Value::Array(items) => items.iter().find_map(|item| walk(item, id)),
            _ => None,
        }
    }
    walk(layout, &session.to_string())
}

/// `layout` with `"tsp": tsp` on `session`'s leaf; `None` if `session` is not in it.
fn set_pane_tsp(layout: &Value, session: Ulid, tsp: bool) -> Option<Value> {
    fn walk(value: &mut Value, id: &str, tsp: bool) -> bool {
        match value {
            Value::Object(map) if map.get("session").and_then(Value::as_str) == Some(id) => {
                map.insert("tsp".to_owned(), Value::Bool(tsp));
                true
            }
            Value::Object(map) => map.values_mut().any(|child| walk(child, id, tsp)),
            Value::Array(items) => items.iter_mut().any(|item| walk(item, id, tsp)),
            _ => false,
        }
    }
    let mut layout = layout.clone();
    walk(&mut layout, &session.to_string(), tsp).then_some(layout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_protocol::{SessionState, StateSource};
    use serde_json::json;

    fn session(task_id: Option<Ulid>, started_at: i64) -> Session {
        Session {
            id: Ulid::new(),
            task_id,
            project_id: Ulid::new(),
            agent_profile: "fake".into(),
            kind: SessionKind::Agent,
            title: None,
            terminal_title: None,
            color: None,
            pinned: false,
            position: 0.0,
            native_session_id: None,
            state: SessionState::Exited,
            state_source: StateSource::Daemon,
            state_changed_at: started_at,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at,
            ended_at: None,
            plan_path: None,
            tsp: mkrew_protocol::TspState::Off,
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

    #[test]
    fn replace_session_keeps_the_panes_chat_mode() {
        let (old, new) = (Ulid::new(), Ulid::new());
        let layout = json!({"split": "vertical", "ratio": 0.5, "children": [
            {"session": old.to_string(), "chat": true},
            {"session": Ulid::new().to_string(), "chat": false}
        ]});
        let replaced = replace_session(&layout, old, new).unwrap();
        assert_eq!(
            replaced["children"][0],
            json!({"session": new.to_string(), "chat": true})
        );
        assert_eq!(replaced["children"][1], layout["children"][1]);
    }

    #[test]
    fn the_pane_tsp_choice_is_saved_on_its_leaf_and_read_back() {
        let (a, b) = (Ulid::new(), Ulid::new());
        let layout = json!({"split": "vertical", "ratio": 0.5, "children": [
            {"session": a.to_string(), "chat": false},
            {"session": b.to_string()}
        ]});
        assert_eq!(leaf_tsp(&layout, b), None);
        let saved = set_pane_tsp(&layout, b, false).unwrap();
        assert_eq!(
            saved["children"][1],
            json!({"session": b.to_string(), "tsp": false})
        );
        assert_eq!(saved["children"][0], layout["children"][0]);
        assert_eq!(leaf_tsp(&saved, b), Some(false));
        assert_eq!(leaf_tsp(&saved, a), None);
        // A resume puts the new session in the leaf and the choice stays with the pane.
        let new = Ulid::new();
        let resumed = replace_session(&saved, b, new).unwrap();
        assert_eq!(leaf_tsp(&resumed, new), Some(false));
        assert_eq!(set_pane_tsp(&layout, Ulid::new(), true), None);
    }
}
