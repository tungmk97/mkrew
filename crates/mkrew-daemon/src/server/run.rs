//! Run commands per workspace (SPEC §8, §10; ADR 0018): each configured command typed into its
//! own shell session titled `run:<name>`, the live ones side by side in the project's `run`
//! tab. Replaces the single dev server of ADR 0008.

use std::path::{Path, PathBuf};
use std::time::Duration;

use mkrew_core::run;
use mkrew_protocol::{
    ErrorCode, Event, Outcome, Project, RunCommand, RunSession, RunStartParams, RunStarted,
    RunStopParams, Session, SessionState,
};
use tokio::sync::broadcast::error::RecvError;
use ulid::Ulid;

use super::{Daemon, Start, ok, store_outcome};
use crate::session::Command;

/// How long `project.run_restart` waits for the stopped sessions to exit (each one is
/// force-killed after its own 5 s grace).
const RESTART_WAIT: Duration = Duration::from_secs(10);

impl Daemon {
    /// Starts the targeted commands that are not running; answers with all targets.
    pub(super) async fn run_start(&self, params: RunStartParams) -> Outcome {
        let _starting = self.run_starting.lock().await;
        self.start_runs(params, &[])
    }

    /// Stops the targets, waits until they exited, then starts them like `run_start`. Targets
    /// that could not start again are refused before anything stops.
    pub(super) async fn run_restart(&self, params: RunStartParams) -> Outcome {
        let _starting = self.run_starting.lock().await;
        let (project, before) = match self.project_sessions(params.project_id) {
            Ok(found) => found,
            Err(outcome) => return outcome,
        };
        let targets = match targets(&project, params.name.as_deref()) {
            Ok(targets) => targets,
            Err(outcome) => return outcome,
        };
        for command in targets {
            if let Err(message) = prepare(Path::new(&project.path), command) {
                return Outcome::error(ErrorCode::InvalidRequest, message);
            }
        }
        let stopped = match self.stop_runs(params.project_id, params.name.as_deref()) {
            Ok(stopped) => stopped,
            Err(outcome) => return outcome,
        };
        if !self.wait_exited(&stopped, RESTART_WAIT).await {
            tracing::warn!(project_id = %params.project_id, "run commands did not stop in time");
            return Outcome::error(
                ErrorCode::Timeout,
                "the run commands did not stop within 10 s",
            );
        }
        self.start_runs(params, &before)
    }

    /// Asks the targets to terminate; ok when nothing runs.
    pub(super) fn run_stop(&self, params: RunStopParams) -> Outcome {
        match self.stop_runs(params.project_id, params.name.as_deref()) {
            Ok(_) => ok(&()),
            Err(outcome) => outcome,
        }
    }

    /// Runs every command of each active project with `run_autostart` (SPEC §8).
    pub(super) async fn autostart_runs(&self) {
        let projects = match self.store().list_projects() {
            Ok(projects) => projects,
            Err(e) => {
                tracing::error!("cannot auto-start run commands: {e}");
                return;
            }
        };
        let wanted = projects
            .iter()
            .filter(|p| p.run_autostart && !p.archived && !p.run_commands.is_empty());
        for project in wanted {
            let params = RunStartParams {
                project_id: project.id,
                name: None,
                cols: mkrew_protocol::DEFAULT_COLS,
                rows: mkrew_protocol::DEFAULT_ROWS,
            };
            if let Outcome::Error(e) = self.run_start(params).await {
                tracing::warn!(project_id = %project.id, "run commands not auto-started: {}", e.message);
            }
        }
    }

    /// `run_start` without the lock (`run_restart` holds it already). `replaced` are the
    /// sessions a restart stopped: their color and pin carry over by command name.
    fn start_runs(&self, params: RunStartParams, replaced: &[Session]) -> Outcome {
        if params.cols == 0 || params.rows == 0 {
            return Outcome::error(ErrorCode::InvalidRequest, "cols and rows must be > 0");
        }
        let (project, sessions) = match self.project_sessions(params.project_id) {
            Ok(found) => found,
            Err(outcome) => return outcome,
        };
        let targets = match targets(&project, params.name.as_deref()) {
            Ok(targets) => targets,
            Err(outcome) => return outcome,
        };
        let mut answer = Vec::new();
        let mut to_start = Vec::new();
        for command in targets {
            match live_run(&sessions, &command.name) {
                Some(running) => answer.push(RunSession {
                    name: command.name.clone(),
                    session_id: running.id,
                    started: false,
                }),
                None => match prepare(Path::new(&project.path), command) {
                    Ok(prepared) => to_start.push((command, prepared)),
                    // Everything is checked before anything spawns.
                    Err(message) => return Outcome::error(ErrorCode::InvalidRequest, message),
                },
            }
        }
        if !to_start.is_empty() {
            let profile = match self.load_profile("shell") {
                Ok(profile) => profile,
                Err(outcome) => return outcome,
            };
            let program = crate::launch::program(&profile);
            for (command, (dir, env)) in to_start {
                let old = live_run(replaced, &command.name);
                let start = Start {
                    project: project.clone(),
                    profile: profile.clone(),
                    task_id: None,
                    title: Some(run::session_title(&command.name)),
                    color: old.and_then(|s| s.color.clone()),
                    pinned: old.is_some_and(|s| s.pinned),
                    cols: params.cols,
                    rows: params.rows,
                    typed: Some(run::typed_line(&program, &command.command)),
                    native_session_id: None,
                    dir: Some(dir),
                    banner: None,
                    env,
                    plan_path: None,
                };
                match self.start_session(start) {
                    Ok(session_id) => {
                        tracing::info!(project_id = %project.id, %session_id, name = %command.name, "run command started");
                        answer.push(RunSession {
                            name: command.name.clone(),
                            session_id,
                            started: true,
                        });
                    }
                    // The ones already started keep running and stay in the tab.
                    Err(outcome) => {
                        self.show_run_tab(&project);
                        return outcome;
                    }
                }
            }
        }
        answer.sort_by_key(|s| project.run_commands.iter().position(|c| c.name == s.name));
        let tab_id = self.show_run_tab(&project);
        ok(&RunStarted {
            tab_id,
            sessions: answer,
        })
    }

    /// Asks the live run sessions of the targets to terminate; returns their ids. A `name`
    /// neither configured nor live is `not_found`.
    fn stop_runs(&self, project_id: Ulid, name: Option<&str>) -> Result<Vec<Ulid>, Outcome> {
        let (project, sessions) = self.project_sessions(project_id)?;
        if let Some(name) = name
            && !project.run_commands.iter().any(|c| c.name == name)
            && live_run(&sessions, name).is_none()
        {
            let message = format!("project {} has no run command {name:?}", project.name);
            return Err(Outcome::error(ErrorCode::NotFound, message));
        }
        let ids: Vec<Ulid> = sessions
            .iter()
            .filter(|s| s.state != SessionState::Exited)
            .filter(|s| run::command_name(s).is_some_and(|n| name.is_none_or(|wanted| wanted == n)))
            .map(|s| s.id)
            .collect();
        for &id in &ids {
            tracing::info!(%project_id, session_id = %id, "stopping a run command");
            self.hub.command(id, Command::Kill { force: false });
        }
        Ok(ids)
    }

    /// Waits until every one of `ids` is `exited` in the store; `false` after `limit`. The
    /// store, not the live map: `finish` leaves the live map before it writes `exited`, so a
    /// start right after could otherwise still see the old session as running.
    async fn wait_exited(&self, ids: &[Ulid], limit: Duration) -> bool {
        // Subscribed before the first check, so an exit in between is not missed.
        let mut events = self.hub.events.subscribe();
        let deadline = tokio::time::Instant::now() + limit;
        loop {
            let exited = {
                let store = self.store();
                ids.iter().all(|&id| {
                    store
                        .get_session(id)
                        .is_ok_and(|s| s.state == SessionState::Exited)
                })
            };
            if exited {
                return true;
            }
            match tokio::time::timeout_at(deadline, events.recv()).await {
                Err(_) | Ok(Err(RecvError::Closed)) => return false,
                // Any event (or a lag) may follow an exit: check again.
                Ok(_) => {}
            }
        }
    }

    fn project_sessions(&self, project_id: Ulid) -> Result<(Project, Vec<Session>), Outcome> {
        let found = {
            let store = self.store();
            store.get_project(project_id).and_then(|project| {
                let sessions = store.list_sessions(Some(project.id))?;
                Ok((project, sessions))
            })
        };
        found.map_err(|e| store_outcome::<()>(Err(e)))
    }

    /// Lays the project's live run sessions side by side in its `run` tab (created when
    /// missing); `None` when the tab could not be written (logged).
    fn show_run_tab(&self, project: &Project) -> Option<Ulid> {
        let placed = {
            // One lock, so a concurrent `tab.set_layout` is not overwritten.
            let store = self.store();
            store.list_sessions(Some(project.id)).and_then(|sessions| {
                let layout = run::layout(&run::ordered_live(&project.run_commands, &sessions));
                let existing = store
                    .list_tabs(project.id)?
                    .into_iter()
                    .find(|t| t.title == run::TAB_TITLE);
                let tab = match existing {
                    Some(tab) => tab,
                    None => store.create_tab(project.id, run::TAB_TITLE, None)?,
                };
                store.set_tab_layout(tab.id, &layout)
            })
        };
        match placed {
            Ok(tab) => {
                let tab_id = tab.id;
                let _ = self.hub.events.send(Event::TabUpdated(tab));
                Some(tab_id)
            }
            Err(e) => {
                tracing::warn!(project_id = %project.id, "run tab not written: {e}");
                None
            }
        }
    }
}

/// The live run session of the command `name`.
fn live_run<'a>(sessions: &'a [Session], name: &str) -> Option<&'a Session> {
    sessions
        .iter()
        .find(|s| s.state != SessionState::Exited && run::command_name(s) == Some(name))
}

/// The configured commands `name` picks (all when `None`); `invalid_state` when there are
/// none, `not_found` for an unknown name.
fn targets<'a>(project: &'a Project, name: Option<&str>) -> Result<Vec<&'a RunCommand>, Outcome> {
    if project.run_commands.is_empty() {
        let message = format!("project {} has no run commands", project.name);
        return Err(Outcome::error(ErrorCode::InvalidState, message));
    }
    let Some(name) = name else {
        return Ok(project.run_commands.iter().collect());
    };
    match project.run_commands.iter().find(|c| c.name == name) {
        Some(command) => Ok(vec![command]),
        None => {
            let message = format!("project {} has no run command {name:?}", project.name);
            Err(Outcome::error(ErrorCode::NotFound, message))
        }
    }
}

/// The folder and extra environment of `command`, or why it cannot start.
fn prepare(
    project_dir: &Path,
    command: &RunCommand,
) -> Result<(PathBuf, Vec<(String, String)>), String> {
    let dir = project_dir.join(&command.dir);
    if !dir.is_dir() {
        return Err(format!(
            "{}: folder {} not found",
            command.name,
            dir.display()
        ));
    }
    let mut env = Vec::new();
    if let Some(file) = &command.env_file {
        let path = dir.join(file);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("{}: cannot read {}: {e}", command.name, path.display()))?;
        env = run::parse_env(&text)
            .map_err(|e| format!("{}: {}: {e}", command.name, path.display()))?;
    }
    env.extend(command.env.iter().cloned());
    Ok((dir, env))
}
