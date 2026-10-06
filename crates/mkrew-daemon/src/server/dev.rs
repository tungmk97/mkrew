//! Dev server per workspace (SPEC §8, §10; ADR 0008): the project's `dev_command` typed into
//! a shell session titled "dev", shown alone in a tab titled "dev". At most one per project.

use mkrew_protocol::{
    DevServer, DevStartParams, ErrorCode, Event, Outcome, Session, SessionKind, SessionState,
};
use serde_json::Value;
use ulid::Ulid;

use super::{Daemon, Start, ok, store_outcome};
use crate::session::Command;

impl Daemon {
    /// Answers with the running dev server, else starts one through the `shell` profile.
    pub(super) fn dev_start(&self, params: DevStartParams) -> Outcome {
        if params.cols == 0 || params.rows == 0 {
            return Outcome::error(ErrorCode::InvalidRequest, "cols and rows must be > 0");
        }
        // Held from the check to the spawn, so two starts cannot both spawn.
        let _starting = self
            .dev_starting
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let found = {
            let store = self.store();
            store.get_project(params.project_id).and_then(|project| {
                let sessions = store.list_sessions(Some(project.id))?;
                Ok((project, sessions))
            })
        };
        let (project, sessions) = match found {
            Ok(found) => found,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        if let Some(running) = dev_session(&sessions) {
            let tab_id = match self.store().list_tabs(project.id) {
                Ok(tabs) => tabs
                    .iter()
                    .find(|t| holds_session(&t.layout, running.id))
                    .map(|t| t.id),
                Err(e) => return store_outcome::<()>(Err(e)),
            };
            return ok(&DevServer {
                session_id: running.id,
                tab_id,
                started: false,
            });
        }
        let Some(command) = project.dev_command.clone() else {
            let message = format!("project {} has no dev command", project.name);
            return Outcome::error(ErrorCode::InvalidState, message);
        };
        let profile = match self.load_profile("shell") {
            Ok(profile) => profile,
            Err(outcome) => return outcome,
        };
        let project_id = project.id;
        let start = Start {
            project,
            profile,
            task_id: None,
            title: Some(DevServer::TITLE.to_owned()),
            cols: params.cols,
            rows: params.rows,
            // Typed once the shell is ready, so it works whatever the shell is.
            typed: Some(format!("{command}\r")),
            native_session_id: None,
        };
        let session_id = match self.start_session(start) {
            Ok(session_id) => session_id,
            Err(outcome) => return outcome,
        };
        tracing::info!(%project_id, %session_id, "dev server started");
        let tab_id = self.show_dev_tab(project_id, session_id, &sessions);
        ok(&DevServer {
            session_id,
            tab_id,
            started: true,
        })
    }

    /// Asks the running dev server to terminate; ok when none runs.
    pub(super) fn dev_stop(&self, project_id: Ulid) -> Outcome {
        let found = {
            let store = self.store();
            // Also tells an unknown project from one without sessions.
            store
                .get_project(project_id)
                .and_then(|_| store.list_sessions(Some(project_id)))
        };
        let sessions = match found {
            Ok(sessions) => sessions,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        if let Some(running) = dev_session(&sessions) {
            tracing::info!(%project_id, session_id = %running.id, "stopping the dev server");
            self.hub.command(running.id, Command::Kill { force: false });
        }
        ok(&())
    }

    /// Starts the dev server of every active project with `dev_autostart` (SPEC §18 M8).
    pub(super) fn autostart_dev_servers(&self) {
        let projects = match self.store().list_projects() {
            Ok(projects) => projects,
            Err(e) => {
                tracing::error!("cannot auto-start dev servers: {e}");
                return;
            }
        };
        let wanted = projects
            .iter()
            .filter(|p| p.dev_autostart && !p.archived && p.dev_command.is_some());
        for project in wanted {
            let params = DevStartParams {
                project_id: project.id,
                cols: DevStartParams::COLS,
                rows: DevStartParams::ROWS,
            };
            if let Outcome::Error(e) = self.dev_start(params) {
                tracing::warn!(project_id = %project.id, "dev server not auto-started: {}", e.message);
            }
        }
    }

    /// Makes `session_id` the only pane of the project's "dev" tab: one whose layout is empty
    /// or a lone pane of an exited session, else a new tab. `None` if no tab could be saved.
    fn show_dev_tab(
        &self,
        project_id: Ulid,
        session_id: Ulid,
        sessions: &[Session],
    ) -> Option<Ulid> {
        let leaf = serde_json::json!({ "session": session_id.to_string() });
        let placed = {
            // One lock, so a concurrent `tab.set_layout` is not overwritten.
            let store = self.store();
            store.list_tabs(project_id).and_then(|tabs| {
                let reusable = tabs
                    .into_iter()
                    .find(|t| t.title == DevServer::TITLE && reusable(&t.layout, sessions));
                let tab = match reusable {
                    Some(tab) => tab,
                    None => store.create_tab(project_id, DevServer::TITLE, None)?,
                };
                store.set_tab_layout(tab.id, &leaf)
            })
        };
        match placed {
            Ok(tab) => {
                let tab_id = tab.id;
                let _ = self.hub.events.send(Event::TabUpdated(tab));
                Some(tab_id)
            }
            Err(e) => {
                tracing::warn!(%project_id, "dev server {session_id} not placed in a tab: {e}");
                None
            }
        }
    }
}

/// The project's live shell session titled "dev".
fn dev_session(sessions: &[Session]) -> Option<&Session> {
    sessions.iter().find(|s| {
        s.kind == SessionKind::Shell
            && s.state != SessionState::Exited
            && s.title.as_deref() == Some(DevServer::TITLE)
    })
}

/// A tab layout the dev server may take over: empty, or one pane of a session that exited.
fn reusable(layout: &Value, sessions: &[Session]) -> bool {
    if layout.is_null() {
        return true;
    }
    let Some(id) = layout
        .as_object()
        .filter(|leaf| leaf.len() == 1)
        .and_then(|leaf| leaf.get("session")?.as_str())
    else {
        return false;
    };
    !sessions
        .iter()
        .any(|s| s.id.to_string() == id && s.state != SessionState::Exited)
}

/// `layout` (the UI's opaque split tree, SPEC §5) has a `{"session": id}` leaf.
fn holds_session(layout: &Value, id: Ulid) -> bool {
    fn walk(value: &Value, id: &str) -> bool {
        match value {
            Value::Object(map) => map.iter().any(|(key, child)| {
                (key == "session" && child.as_str() == Some(id)) || walk(child, id)
            }),
            Value::Array(items) => items.iter().any(|item| walk(item, id)),
            _ => false,
        }
    }
    walk(layout, &id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_protocol::StateSource;
    use serde_json::json;

    fn session(kind: SessionKind, title: Option<&str>, state: SessionState) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id: Ulid::new(),
            agent_profile: "shell".into(),
            kind,
            title: title.map(str::to_owned),
            native_session_id: None,
            state,
            state_source: StateSource::Daemon,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
        }
    }

    #[test]
    fn dev_session_is_the_live_shell_titled_dev() {
        let exited = session(SessionKind::Shell, Some("dev"), SessionState::Exited);
        let agent = session(SessionKind::Agent, Some("dev"), SessionState::Idle);
        let other = session(SessionKind::Shell, Some("build"), SessionState::Idle);
        assert!(dev_session(&[exited.clone(), agent.clone(), other.clone()]).is_none());
        let live = session(SessionKind::Shell, Some("dev"), SessionState::Starting);
        let sessions = [exited, agent, other, live.clone()];
        assert_eq!(dev_session(&sessions).map(|s| s.id), Some(live.id));
    }

    #[test]
    fn only_empty_tabs_or_a_lone_exited_pane_are_reused() {
        let exited = session(SessionKind::Shell, Some("dev"), SessionState::Exited);
        let live = session(SessionKind::Shell, None, SessionState::Idle);
        let sessions = [exited.clone(), live.clone()];
        let leaf = |s: &Session| json!({"session": s.id.to_string()});
        assert!(reusable(&Value::Null, &sessions));
        assert!(reusable(&leaf(&exited), &sessions));
        // A session the daemon no longer knows counts as exited.
        assert!(reusable(
            &json!({"session": Ulid::new().to_string()}),
            &sessions
        ));
        assert!(!reusable(&leaf(&live), &sessions));
        let split = json!({"split": "vertical", "ratio": 0.5,
            "children": [leaf(&exited), leaf(&exited)]});
        assert!(!reusable(&split, &sessions));
    }

    #[test]
    fn holds_session_finds_nested_leaves() {
        let (a, b) = (Ulid::new(), Ulid::new());
        let layout = json!({"split": "vertical", "ratio": 0.5, "children": [
            {"session": Ulid::new().to_string()},
            {"split": "horizontal", "ratio": 0.6, "children": [{"session": a.to_string()}]}
        ]});
        assert!(holds_session(&layout, a));
        assert!(!holds_session(&layout, b));
        assert!(!holds_session(&Value::Null, a));
    }
}
