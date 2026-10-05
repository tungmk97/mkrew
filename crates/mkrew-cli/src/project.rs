//! `mk project add | list` (SPEC §9).

use std::path::Path;

use anyhow::Context;
use mkrew_core::Paths;
use mkrew_protocol::{Project, ProjectAddParams, Request};

use crate::client::Client;

/// Connects to a running daemon; commands other than `daemon start` never spawn one.
pub fn connect(paths: &Paths) -> anyhow::Result<Client> {
    match crate::daemon::try_connect(paths)? {
        Some(client) => Ok(client),
        None => anyhow::bail!("the daemon is not running; start it with `mk daemon start`"),
    }
}

pub fn add(client: &mut Client, path: &Path, name: Option<String>) -> anyhow::Result<()> {
    // The daemon has its own working directory, so send an absolute path.
    let path = std::path::absolute(path).with_context(|| format!("resolve {}", path.display()))?;
    let project: Project = client.call(Request::ProjectAdd(ProjectAddParams {
        path: path.to_string_lossy().into_owned(),
        name,
    }))?;
    println!(
        "added project {} ({}) at {}",
        project.name, project.id, project.path
    );
    Ok(())
}

pub fn list(client: &mut Client) -> anyhow::Result<()> {
    let projects: Vec<Project> = client.call(Request::ProjectList)?;
    if projects.is_empty() {
        println!("no projects; add one with `mk project add <path>`");
        return Ok(());
    }
    let name_width = projects
        .iter()
        .map(|p| p.name.len())
        .max()
        .unwrap_or(0)
        .max(4);
    println!("{:<26}  {:<name_width$}  PATH", "ID", "NAME");
    for project in projects {
        println!(
            "{:<26}  {:<name_width$}  {}",
            project.id, project.name, project.path
        );
    }
    Ok(())
}
