//! `mk session list | spawn | history`, `mk send`, `mk wait` (SPEC §9).

use std::fs::File;
use std::io::Write;

use anyhow::Context;
use mkrew_protocol::{
    Project, Request, Session, SessionHistory, SessionKind, SessionListParams, SessionRef,
    SessionSendParams, SessionSendResult, SessionSpawnParams, SessionSpawnResult, SessionState,
    SessionWaitParams, SessionWaitResult,
};
use ulid::Ulid;

use crate::client::Client;

/// Terminal size for sessions spawned without a terminal to size them after.
pub const DEFAULT_SIZE: (u16, u16) = (120, 40);

/// Finds a project by id or name.
pub fn resolve_project(projects: &[Project], key: &str) -> anyhow::Result<Project> {
    projects
        .iter()
        .find(|p| p.id.to_string() == key || p.name == key)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no project with id or name {key:?}"))
}

pub fn list(client: &mut Client, project: Option<String>) -> anyhow::Result<()> {
    let projects: Vec<Project> = client.call(Request::ProjectList)?;
    let project_id = match project {
        Some(key) => Some(resolve_project(&projects, &key)?.id),
        None => None,
    };
    let sessions: Vec<Session> =
        client.call(Request::SessionList(SessionListParams { project_id }))?;
    if sessions.is_empty() {
        println!("no sessions; start one with `mk session spawn <project> --agent <profile>`");
        return Ok(());
    }
    let project_name = |s: &Session| {
        projects
            .iter()
            .find(|p| p.id == s.project_id)
            .map_or_else(|| s.project_id.to_string(), |p| p.name.clone())
    };
    println!("{:<26}  {:<12}  {:<10}  PROJECT", "ID", "STATE", "AGENT");
    for session in &sessions {
        println!(
            "{:<26}  {:<12}  {:<10}  {}",
            session.id,
            state_name(session.state),
            session.agent_profile,
            project_name(session)
        );
    }
    Ok(())
}

/// Spawns a session and prints its id (alone, so scripts can capture it).
pub fn spawn(client: &mut Client, project: &str, agent: String) -> anyhow::Result<()> {
    let projects: Vec<Project> = client.call(Request::ProjectList)?;
    let project = resolve_project(&projects, project)?;
    let (cols, rows) = crate::attach::terminal_size().unwrap_or(DEFAULT_SIZE);
    let spawned: SessionSpawnResult = client.call(Request::SessionSpawn(SessionSpawnParams {
        project_id: project.id,
        agent_profile: Some(agent),
        kind: SessionKind::Agent,
        tab_id: None,
        title: None,
        cols,
        rows,
    }))?;
    println!("{}", spawned.session_id);
    Ok(())
}

/// Writes the session's raw output log (all rotated parts, oldest first) to stdout. Works for
/// exited sessions, including those from before a daemon restart.
pub fn history(client: &mut Client, session_id: Ulid) -> anyhow::Result<()> {
    let history: SessionHistory =
        client.call(Request::SessionHistory(SessionRef { session_id }))?;
    let mut out = std::io::stdout().lock();
    for file in &history.files {
        let mut log = match File::open(file) {
            Ok(log) => log,
            // A live session's log can rotate between the reply and here: best effort.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e).with_context(|| format!("open {}", file.display())),
        };
        std::io::copy(&mut log, &mut out).with_context(|| format!("read {}", file.display()))?;
    }
    out.flush()?;
    Ok(())
}

/// Types `text` into the session (plus Enter unless `no_submit`); the daemon queues it while
/// the agent is working.
pub fn send(
    client: &mut Client,
    session_id: Ulid,
    text: String,
    no_submit: bool,
) -> anyhow::Result<()> {
    client
        .call::<SessionSendResult>(Request::SessionSend(SessionSendParams {
            session_id,
            text,
            submit: !no_submit,
        }))
        .map(|_| ())
}

/// Blocks until the session reaches one of `until` (or exits) and prints the state, plus the
/// exit code once exited.
pub fn wait(
    client: &mut Client,
    session_id: Ulid,
    until: Vec<SessionState>,
    timeout_ms: Option<u64>,
) -> anyhow::Result<()> {
    let result: SessionWaitResult = client.call(Request::SessionWait(SessionWaitParams {
        session_id,
        until,
        timeout_ms,
    }))?;
    match result.exit_code {
        Some(code) => println!("{} (exit code {code})", state_name(result.state)),
        None => println!("{}", state_name(result.state)),
    }
    Ok(())
}

/// Parses a wire state name (`needs_input`, …) for `--until`.
pub fn parse_state(name: &str) -> Result<SessionState, String> {
    serde_json::from_value(serde_json::Value::String(name.to_owned())).map_err(|_| {
        format!(
            "unknown state {name:?}; expected starting, working, needs_input, idle, done, exited or unknown"
        )
    })
}

/// The wire name (`needs_input`, …).
fn state_name(state: SessionState) -> String {
    match serde_json::to_value(state) {
        Ok(serde_json::Value::String(name)) => name,
        _ => format!("{state:?}"),
    }
}
