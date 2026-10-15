//! "Ask AI…" (ADR 0061): one question to an agent's `ask` / `ask_web` command, outside every
//! project; the answer arrives as `ai.generated` and is shown as Markdown. No history: each
//! question stands alone. Pure and tested headlessly; `main.rs` renders it.

use std::rc::Rc;

use mkrew_protocol::{AgentProfileInfo, AiGenerateParams, AiKind, SessionKind};
use ulid::Ulid;

use crate::docs::Shown;
use crate::task_dialog::DialogKey;
use crate::text_box::{Outcome, TextBox};
use crate::workspace_menu::Agents;

/// Where the question is.
#[derive(Debug, Clone, PartialEq)]
pub enum State {
    /// Being typed.
    Idle,
    /// Asked; the answer to this generation is awaited.
    Waiting(Ulid),
    /// The answer, and its Markdown parsed once for the preview.
    Answered {
        text: String,
        shown: Rc<Shown>,
    },
    Failed(String),
}

/// The open dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    pub question: TextBox,
    pub agents: Agents,
    /// The agent picked in the dialog; `None` = [`Self::default_agent`], else the first one.
    pub agent: Option<String>,
    /// The active workspace's default agent, if any.
    pub default_agent: Option<String>,
    /// Ask with web search (`ask_web`).
    pub web: bool,
    pub state: State,
}

impl Ask {
    pub fn new(default_agent: Option<String>, web: bool) -> Self {
        Self {
            question: TextBox::default(),
            agents: Agents::Loading,
            agent: None,
            default_agent,
            web,
            state: State::Idle,
        }
    }

    /// The installed agents that can answer, in profile order.
    pub fn choices(&self) -> Vec<&AgentProfileInfo> {
        match &self.agents {
            Agents::Loaded(profiles) => profiles
                .iter()
                .filter(|p| p.kind == SessionKind::Agent && p.installed && p.ask)
                .collect(),
            Agents::Loading | Agents::Failed(_) => Vec::new(),
        }
    }

    /// The agent that answers: the picked one, else the workspace default, else the first.
    pub fn selected(&self) -> Option<&AgentProfileInfo> {
        let choices = self.choices();
        let named = |id: &Option<String>| {
            let id = id.as_deref()?;
            choices.iter().copied().find(|p| p.id == id)
        };
        named(&self.agent)
            .or_else(|| named(&self.default_agent))
            .or_else(|| choices.first().copied())
    }

    /// Whether web search is used: switched on and the agent has `ask_web`.
    pub fn uses_web(&self) -> bool {
        self.web && self.selected().is_some_and(|p| p.ask_web)
    }

    pub fn waiting(&self) -> bool {
        matches!(self.state, State::Waiting(_))
    }

    /// Applies a key to the question: Enter types a new line, Ctrl+Enter asks, Esc closes;
    /// caret keys move the caret. Paste and copy are the caller's.
    pub fn key(&mut self, key: DialogKey) -> Outcome {
        match key {
            DialogKey::Cancel | DialogKey::Save | DialogKey::Enter | DialogKey::Insert(_) => {
                return self.question.key(key);
            }
            DialogKey::Paste | DialogKey::Copy | DialogKey::SwitchField => {}
            key => {
                self.question.edit(&key);
            }
        }
        Outcome::Edited
    }

    /// The request for the typed question, now awaited; `None` while one is awaited, with a
    /// blank question or without an agent.
    pub fn begin(&mut self) -> Option<AiGenerateParams> {
        if self.waiting() || self.question.is_blank() {
            return None;
        }
        let params = AiGenerateParams {
            generation_id: Ulid::new(),
            project_id: None,
            worktree_task_id: None,
            repo: None,
            agent_profile: Some(self.selected()?.id.clone()),
            kind: AiKind::Ask,
            input: self.question.text().trim().to_owned(),
            web: self.uses_web(),
        };
        self.state = State::Waiting(params.generation_id);
        Some(params)
    }

    /// The answer (or failure) of `generation_id`; others are not this dialog's.
    pub fn answered(&mut self, generation_id: Ulid, result: Result<String, String>) {
        if self.state == State::Waiting(generation_id) {
            self.state = match result {
                Ok(text) => State::Answered {
                    shown: Rc::new(Shown::from_text(&text)),
                    text,
                },
                Err(error) => State::Failed(error),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str, ask: bool, ask_web: bool) -> AgentProfileInfo {
        AgentProfileInfo {
            id: id.into(),
            name: id.to_uppercase(),
            kind: SessionKind::Agent,
            command: id.into(),
            icon: None,
            installed: true,
            ask,
            ask_web,
        }
    }

    fn dialog(default_agent: Option<&str>) -> Ask {
        let mut ask = Ask::new(default_agent.map(str::to_owned), true);
        let shell = AgentProfileInfo {
            kind: SessionKind::Shell,
            ..profile("pwsh", true, true)
        };
        let missing = AgentProfileInfo {
            installed: false,
            ..profile("gone", true, true)
        };
        ask.agents = Agents::Loaded(vec![
            profile("codex", false, false),
            shell,
            missing,
            profile("claude", true, true),
            profile("omp", true, false),
        ]);
        ask
    }

    #[test]
    fn only_installed_agents_that_can_answer_are_offered() {
        let ask = dialog(None);
        let ids: Vec<&str> = ask.choices().iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["claude", "omp"]);
    }

    #[test]
    fn the_picked_agent_wins_then_the_workspace_default_then_the_first() {
        let mut ask = dialog(Some("omp"));
        assert_eq!(ask.selected().unwrap().id, "omp");
        ask.agent = Some("claude".into());
        assert_eq!(ask.selected().unwrap().id, "claude");
        // A default that cannot answer falls back to the first choice.
        assert_eq!(dialog(Some("codex")).selected().unwrap().id, "claude");
    }

    #[test]
    fn web_search_needs_an_agent_with_ask_web() {
        let mut ask = dialog(Some("claude"));
        assert!(ask.uses_web());
        ask.agent = Some("omp".into());
        assert!(!ask.uses_web(), "omp has no `ask_web`");
        ask.agent = Some("claude".into());
        ask.web = false;
        assert!(!ask.uses_web());
    }

    #[test]
    fn asking_sends_the_trimmed_question_once_and_takes_only_its_answer() {
        let mut ask = dialog(None);
        assert_eq!(ask.begin(), None, "blank question");
        ask.question.insert("  what is 2+2?\n");
        let params = ask.begin().unwrap();
        assert_eq!(
            (
                params.kind,
                params.project_id,
                params.agent_profile.as_deref(),
                params.input.as_str(),
                params.web
            ),
            (AiKind::Ask, None, Some("claude"), "what is 2+2?", true)
        );
        assert_eq!(ask.begin(), None, "one question at a time");
        ask.answered(Ulid::new(), Ok("other".into()));
        assert!(ask.waiting(), "another generation's answer is ignored");
        ask.answered(params.generation_id, Ok("4".into()));
        assert!(matches!(&ask.state, State::Answered { text, .. } if text == "4"));
        // The question stays, so it can be asked again (or edited first).
        assert!(ask.begin().is_some());
    }

    #[test]
    fn no_agent_no_request() {
        let mut ask = Ask::new(None, false);
        ask.question.insert("hi");
        assert_eq!(ask.begin(), None, "agents still loading");
    }
}
