//! `mk project add | list | update` (SPEC §9).

use std::path::Path;

use anyhow::Context;
use mkrew_core::Paths;
use mkrew_protocol::{GitSetBaseParams, Project, ProjectAddParams, ProjectUpdateParams, Request};

use crate::client::Client;
use crate::session::resolve_project;

/// Connects to a running daemon; commands other than `daemon start` never spawn one.
pub fn connect(paths: &Paths) -> anyhow::Result<Client> {
    match crate::daemon::try_connect(paths)? {
        Some(client) => Ok(client),
        None => anyhow::bail!("the daemon is not running; start it with `mk daemon start`"),
    }
}

pub fn add(
    client: &mut Client,
    path: &Path,
    name: Option<String>,
    dev: Option<String>,
) -> anyhow::Result<()> {
    // The daemon has its own working directory, so send an absolute path.
    let path = std::path::absolute(path).with_context(|| format!("resolve {}", path.display()))?;
    let project: Project = client.call(Request::ProjectAdd(ProjectAddParams {
        path: path.to_string_lossy().into_owned(),
        name,
        dev_command: dev,
    }))?;
    println!(
        "added project {} ({}) at {}",
        project.name, project.id, project.path
    );
    Ok(())
}

/// `--dev-autostart on|off`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Switch {
    On,
    Off,
}

/// `mk project update` changes; `None` = unchanged, `Some(None)` = auto-detect base /
/// no dev command.
pub struct Changes {
    pub base: Option<Option<String>>,
    pub dev: Option<Option<String>>,
    pub dev_autostart: Option<bool>,
}

/// `mk project update <project> [--base <ref> | --auto-base] [--dev "…" | --no-dev]
/// [--dev-autostart on|off]`.
pub fn update(client: &mut Client, project: &str, changes: Changes) -> anyhow::Result<()> {
    let projects: Vec<Project> = client.call(Request::ProjectList)?;
    let project = resolve_project(&projects, project)?;
    if let Some(base) = changes.base {
        set_base(client, &project, base)?;
    }
    if changes.dev.is_some() || changes.dev_autostart.is_some() {
        let updated: Project = client.call(Request::ProjectUpdate(ProjectUpdateParams {
            project_id: project.id,
            name: None,
            default_agent: None,
            dev_command: changes.dev,
            dev_autostart: changes.dev_autostart,
        }))?;
        let autostart = if updated.dev_autostart { "on" } else { "off" };
        match &updated.dev_command {
            Some(command) => println!(
                "dev server of {}: {command} (auto-start {autostart})",
                updated.name
            ),
            None => println!(
                "dev server of {}: none (auto-start {autostart})",
                updated.name
            ),
        }
    }
    Ok(())
}

/// `None` = auto-detect.
fn set_base(client: &mut Client, project: &Project, base: Option<String>) -> anyhow::Result<()> {
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
