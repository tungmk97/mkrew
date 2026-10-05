//! `mk project add | list | update` (SPEC §9).

use std::path::Path;

use anyhow::Context;
use mkrew_core::Paths;
use mkrew_protocol::{GitSetBaseParams, Project, ProjectAddParams, Request};

use crate::client::Client;
use crate::session::resolve_project;

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

/// `mk project update <project> --base <ref> | --auto-base`; `None` = auto-detect.
pub fn set_base(client: &mut Client, project: &str, base: Option<String>) -> anyhow::Result<()> {
    let projects: Vec<Project> = client.call(Request::ProjectList)?;
    let project = resolve_project(&projects, project)?;
    let () = client.call(Request::GitSetBase(GitSetBaseParams {
        project_id: project.id,
        base_ref: base.clone(),
    }))?;
    let base = base.as_deref().map(str::trim).filter(|b| !b.is_empty());
    match base {
        Some(base) => println!("base of {}: {base}", project.name),
        None => println!("base of {}: auto-detect", project.name),
    }
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
