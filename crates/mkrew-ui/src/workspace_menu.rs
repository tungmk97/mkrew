//! The workspace menu (SPEC §10): "Open in Zed" and the workspace's default agent, used for
//! new agent panes. Pure and tested headlessly; `main.rs` renders it.

use mkrew_protocol::{AgentProfileInfo, ProjectUpdateParams, SessionKind};
use ulid::Ulid;

/// The open menu of one workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceMenu {
    pub project_id: Ulid,
    pub agents: Agents,
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
}

impl WorkspaceMenu {
    pub fn new(project_id: Ulid) -> Self {
        Self {
            project_id,
            agents: Agents::Loading,
        }
    }
}

/// The agent profiles (shells are not agents), with `current` selected. A `current` that no
/// longer has a profile stays listed, marked missing, so the setting is never hidden.
pub fn agent_choices(profiles: &[AgentProfileInfo], current: Option<&str>) -> Vec<AgentChoice> {
    let mut choices: Vec<AgentChoice> = profiles
        .iter()
        .filter(|profile| profile.kind == SessionKind::Agent)
        .map(|profile| AgentChoice {
            id: profile.id.clone(),
            label: profile.name.clone(),
            selected: current == Some(profile.id.as_str()),
        })
        .collect();
    if let Some(current) = current
        && !choices.iter().any(|choice| choice.selected)
    {
        choices.push(AgentChoice {
            id: current.to_owned(),
            label: format!("{current} (profile missing)"),
            selected: true,
        });
    }
    choices
}

/// `project.update` making `agent` the workspace's default agent.
pub fn set_default_agent(project_id: Ulid, agent: &str) -> ProjectUpdateParams {
    ProjectUpdateParams {
        project_id,
        name: None,
        default_agent: Some(Some(agent.to_owned())),
        run_commands: None,
        run_autostart: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str, name: &str, kind: SessionKind) -> AgentProfileInfo {
        AgentProfileInfo {
            id: id.into(),
            name: name.into(),
            kind,
            command: id.into(),
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
        let choices = agent_choices(&profiles(), Some("codex"));
        assert_eq!(
            rows(&choices),
            [("claude", "Claude Code", false), ("codex", "Codex", true)]
        );
    }

    #[test]
    fn no_default_selects_nothing() {
        let choices = agent_choices(&profiles(), None);
        assert!(choices.iter().all(|c| !c.selected));
        assert_eq!(choices.len(), 2);
    }

    #[test]
    fn a_default_without_a_profile_stays_visible() {
        let choices = agent_choices(&profiles(), Some("omp"));
        assert_eq!(
            rows(&choices).last(),
            Some(&("omp", "omp (profile missing)", true))
        );
        assert!(agent_choices(&[], Some("omp"))[0].selected);
    }

    #[test]
    fn choosing_an_agent_sets_only_the_default_agent() {
        let id = Ulid::new();
        let params = set_default_agent(id, "codex");
        assert_eq!(params.project_id, id);
        assert_eq!(params.name, None);
        assert_eq!(params.default_agent, Some(Some("codex".to_owned())));
    }
}
