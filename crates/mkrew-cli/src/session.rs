//! `mk session list | spawn` (SPEC §9).

use mkrew_protocol::{
    Project, Request, Session, SessionKind, SessionListParams, SessionSpawnParams,
    SessionSpawnResult, SessionState,
};

use crate::client::Client;

/// Terminal size for sessions spawned without a terminal to size them after.
const DEFAULT_SIZE: (u16, u16) = (120, 40);

/// Finds a project by id or name.
fn resolve_project(projects: &[Project], key: &str) -> anyhow::Result<Project> {
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
        agent_profile: agent,
        kind: SessionKind::Agent,
        tab_id: None,
        title: None,
        cols,
        rows,
    }))?;
    println!("{}", spawned.session_id);
    Ok(())
}

/// The wire name (`needs_input`, …).
fn state_name(state: SessionState) -> String {
    match serde_json::to_value(state) {
        Ok(serde_json::Value::String(name)) => name,
        _ => format!("{state:?}"),
    }
}
