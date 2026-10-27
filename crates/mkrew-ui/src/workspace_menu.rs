//! The workspace menu (SPEC §10): "Open in Zed", "Run setup…", the default agent and shell, and
//! removing the workspace. Pure and tested headlessly; `main.rs` renders it.

use mkrew_protocol::{AgentProfileInfo, ProjectUpdateParams, RunCommand, SessionKind};
use ulid::Ulid;

/// The open menu of one workspace, floating at `position` (window coordinates).
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceMenu {
    pub project_id: Ulid,
    pub agents: Agents,
    pub position: gpui::Point<gpui::Pixels>,
}

/// The agent profiles offered as default agent (`agent.profiles`, fetched when the menu opens).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Agents {
    Loading,
    Loaded(Vec<AgentProfileInfo>),
    Failed(String),
}

/// One row of the default-agent list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentChoice {
    /// Profile id (`default_agent`).
    pub id: String,
    pub label: String,
    pub selected: bool,
    /// The profile's badge (ADR 0025); `None` for rows without a loaded profile.
    pub icon: Option<String>,
}

impl WorkspaceMenu {
    pub fn new(project_id: Ulid, position: gpui::Point<gpui::Pixels>) -> Self {
        Self {
            project_id,
            agents: Agents::Loading,
            position,
        }
    }
}

/// The profiles of `kind` (agents, or shells found on PATH), with `current` selected. A
/// `current` that no longer has a listed profile stays, marked missing, so the setting is never
/// hidden.
pub fn agent_choices(
    profiles: &[AgentProfileInfo],
    kind: SessionKind,
    current: Option<&str>,
) -> Vec<AgentChoice> {
    let mut choices: Vec<AgentChoice> = profiles
        .iter()
        // Shells the machine lacks are hidden (several samples ship); agents always show.
        .filter(|profile| profile.kind == kind && (kind == SessionKind::Agent || profile.installed))
        .map(|profile| AgentChoice {
            id: profile.id.clone(),
            label: profile.name.clone(),
            selected: current == Some(profile.id.as_str()),
            icon: profile.icon.clone(),
        })
        .collect();
    if let Some(current) = current
        && !choices.iter().any(|choice| choice.selected)
    {
        let why = if profiles.iter().any(|profile| profile.id == current) {
            "not installed"
        } else {
            "profile missing"
        };
        choices.push(AgentChoice {
            id: current.to_owned(),
            label: format!("{current} ({why})"),
            selected: true,
            icon: None,
        });
    }
    choices
}

/// `project.update` making `agent` the workspace's default agent.
pub fn set_default_agent(project_id: Ulid, agent: &str) -> ProjectUpdateParams {
    ProjectUpdateParams {
        project_id,
        jira_keys: None,
        position: None,
        name: None,
        default_agent: Some(Some(agent.to_owned())),
        default_shell: None,
        run_commands: None,
        run_autostart: None,
        color: None,
        pinned: None,
    }
}

/// `project.update` making `shell` the workspace's default shell (ADR 0032).
pub fn set_default_shell(project_id: Ulid, shell: &str) -> ProjectUpdateParams {
    ProjectUpdateParams {
        project_id,
        jira_keys: None,
        position: None,
        name: None,
        default_agent: None,
        default_shell: Some(Some(shell.to_owned())),
        run_commands: None,
        run_autostart: None,
        color: None,
        pinned: None,
    }
}

/// `project.update` setting the workspace's color (`None` = no color).
pub fn set_color(project_id: Ulid, color: Option<&str>) -> ProjectUpdateParams {
    ProjectUpdateParams {
        project_id,
        jira_keys: None,
        position: None,
        name: None,
        default_agent: None,
        default_shell: None,
        run_commands: None,
        run_autostart: None,
        color: Some(color.map(str::to_owned)),
        pinned: None,
    }
}

/// `project.update` pinning or unpinning the workspace.
pub fn set_pinned(project_id: Ulid, pinned: bool) -> ProjectUpdateParams {
    ProjectUpdateParams {
        project_id,
        jira_keys: None,
        position: None,
        name: None,
        default_agent: None,
        default_shell: None,
        run_commands: None,
        run_autostart: None,
        color: None,
        pinned: Some(pinned),
    }
}

/// `project.update` with the workspace's new sidebar order (a drag of its card).
pub fn set_position(project_id: Ulid, position: i64) -> ProjectUpdateParams {
    ProjectUpdateParams {
        position: Some(position),
        pinned: None,
        ..set_pinned(project_id, false)
    }
}

/// `project.update` with the run commands in a new order (a drag in the sidebar's RUN group).
pub fn set_run_commands(project_id: Ulid, commands: Vec<RunCommand>) -> ProjectUpdateParams {
    ProjectUpdateParams {
        project_id,
        jira_keys: None,
        position: None,
        name: None,
        default_agent: None,
        default_shell: None,
        run_commands: Some(commands),
        run_autostart: None,
        color: None,
        pinned: None,
    }
}

/// `project.update` replacing the workspace's Jira project keys (`[]` clears them).
pub fn set_jira_keys(project_id: Ulid, keys: Vec<String>) -> ProjectUpdateParams {
    ProjectUpdateParams {
        project_id,
        name: None,
        default_agent: None,
        default_shell: None,
        run_commands: None,
        run_autostart: None,
        color: None,
        pinned: None,
        position: None,
        jira_keys: Some(keys),
    }
}

/// Answers of the "Remove …?" confirmations; index 0 removes ([`crate::task_dialog::confirms_delete`]).
pub const REMOVE_ANSWERS: [&str; 2] = ["Remove", "Cancel"];

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str, name: &str, kind: SessionKind) -> AgentProfileInfo {
        AgentProfileInfo {
            id: id.into(),
            name: name.into(),
            kind,
            command: id.into(),
            icon: None,
            installed: true,
            ask: false,
            ask_web: false,
        }
    }

    fn profiles() -> Vec<AgentProfileInfo> {
        vec![
            profile("claude", "Claude Code", SessionKind::Agent),
            profile("shell", "Shell", SessionKind::Shell),
            profile("codex", "Codex", SessionKind::Agent),
        ]
    }

    fn rows(choices: &[AgentChoice]) -> Vec<(&str, &str, bool)> {
        choices
            .iter()
            .map(|c| (c.id.as_str(), c.label.as_str(), c.selected))
            .collect()
    }

    #[test]
    fn lists_agents_only_with_the_current_one_selected() {
        let choices = agent_choices(&profiles(), SessionKind::Agent, Some("codex"));
        assert_eq!(
            rows(&choices),
            [("claude", "Claude Code", false), ("codex", "Codex", true)]
        );
    }

    #[test]
    fn no_default_selects_nothing() {
        let choices = agent_choices(&profiles(), SessionKind::Agent, None);
        assert!(choices.iter().all(|c| !c.selected));
        assert_eq!(choices.len(), 2);
    }

    #[test]
    fn a_default_without_a_profile_stays_visible() {
        let choices = agent_choices(&profiles(), SessionKind::Agent, Some("omp"));
        assert_eq!(
            rows(&choices).last(),
            Some(&("omp", "omp (profile missing)", true))
        );
        assert!(agent_choices(&[], SessionKind::Agent, Some("omp"))[0].selected);
    }

    #[test]
    fn shells_list_only_installed_shell_profiles() {
        let mut list = profiles();
        list.push(profile("pwsh", "PowerShell", SessionKind::Shell));
        list.push(AgentProfileInfo {
            installed: false,
            ..profile("bash", "Bash", SessionKind::Shell)
        });
        list.push(AgentProfileInfo {
            installed: false,
            ..profile("omp", "Oh My Pi", SessionKind::Agent)
        });
        assert_eq!(
            rows(&agent_choices(&list, SessionKind::Shell, Some("pwsh"))),
            [("shell", "Shell", false), ("pwsh", "PowerShell", true)]
        );
        // Agents stay listed when missing: a spawn then names the PATH it searched.
        assert!(
            agent_choices(&list, SessionKind::Agent, None)
                .iter()
                .any(|choice| choice.id == "omp")
        );
        // A default shell that is no longer found stays visible, like a missing agent.
        assert_eq!(
            rows(&agent_choices(&list, SessionKind::Shell, Some("bash"))).last(),
            Some(&("bash", "bash (not installed)", true))
        );
        assert_eq!(
            rows(&agent_choices(&list, SessionKind::Shell, Some("zsh"))).last(),
            Some(&("zsh", "zsh (profile missing)", true))
        );
    }

    #[test]
    fn choosing_an_agent_sets_only_the_default_agent() {
        let id = Ulid::new();
        let params = set_default_agent(id, "codex");
        assert_eq!(params.project_id, id);
        assert_eq!(params.name, None);
        assert_eq!(params.default_agent, Some(Some("codex".to_owned())));
    }

    #[test]
    fn pinning_sets_only_the_pin() {
        let id = Ulid::new();
        assert_eq!(
            serde_json::to_value(set_pinned(id, true)).unwrap(),
            serde_json::json!({"project_id": id, "pinned": true})
        );
    }
}
