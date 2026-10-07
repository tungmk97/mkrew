//! Fan-out on the board (SPEC §20E.2): the "Run with several agents…" picker of one card and
//! the lines of a group card, one per child task. Pure and tested headlessly; `main.rs`
//! renders it.

use mkrew_protocol::{Session, Task, TaskFanoutParams, TaskStatus};
use ulid::Ulid;

use crate::board;
use crate::lifecycle;
use crate::status;
use crate::workspace_menu::Agents;

/// The open "Run with several agents…" picker of one card: the agent profiles, fetched when it
/// opens, and the ones ticked so far, in the order they were ticked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FanOutPicker {
    pub task_id: Ulid,
    pub agents: Agents,
    pub picked: Vec<String>,
}

impl FanOutPicker {
    pub fn new(task_id: Ulid) -> Self {
        Self {
            task_id,
            agents: Agents::Loading,
            picked: Vec::new(),
        }
    }

    pub fn is_picked(&self, profile: &str) -> bool {
        self.picked.iter().any(|picked| picked == profile)
    }

    /// Whether every slot is taken: the unticked rows are disabled.
    pub fn is_full(&self) -> bool {
        self.picked.len() >= TaskFanoutParams::MAX_PROFILES
    }

    /// Ticks or unticks `profile`; a tick beyond the maximum is ignored.
    pub fn toggle(&mut self, profile: &str) {
        if self.is_picked(profile) {
            self.picked.retain(|picked| picked != profile);
        } else if !self.is_full() {
            self.picked.push(profile.to_owned());
        }
    }

    /// The run button's text: what is still missing, or how many agents it starts.
    pub fn button_label(&self) -> String {
        if self.picked.len() < TaskFanoutParams::MIN_PROFILES {
            format!(
                "Pick {}–{} agents",
                TaskFanoutParams::MIN_PROFILES,
                TaskFanoutParams::MAX_PROFILES
            )
        } else {
            format!("Run {} agents", self.picked.len())
        }
    }

    /// The `task.fanout` the run button sends; `None` until enough agents are ticked.
    pub fn request(&self) -> Option<TaskFanoutParams> {
        (self.picked.len() >= TaskFanoutParams::MIN_PROFILES)
            .then(|| lifecycle::fanout_params(self.task_id, self.picked.clone()))
    }
}

/// One child of a group card: its agent and where it is.
#[derive(Debug, Clone, PartialEq)]
pub struct ChildLine {
    pub task_id: Ulid,
    pub agent: String,
    pub status: TaskStatus,
    /// The child's newest session.
    pub session: Option<Session>,
}

impl ChildLine {
    /// The child's state: its session's status, else its column (a child that did not start).
    pub fn state_text(&self, now_ms: i64) -> String {
        match &self.session {
            Some(session) => status::text(
                session.state,
                session.exit_code,
                session.state_changed_at,
                now_ms,
            ),
            None => board::column_title(self.status).to_lowercase(),
        }
    }
}

/// The children of `original` (tasks whose `fanout_of` is it), oldest first; empty for a task
/// that never fanned out. A child's agent is its title past the original's `<title> · `, else
/// its profile.
pub fn children(original: &Task, tasks: &[Task], sessions: &[Session]) -> Vec<ChildLine> {
    let mut children: Vec<&Task> = tasks
        .iter()
        .filter(|task| task.fanout_of == Some(original.id))
        .collect();
    children.sort_by_key(|task| (task.created_at, task.id));
    let prefix = format!("{} · ", original.title);
    children
        .into_iter()
        .map(|child| ChildLine {
            task_id: child.id,
            agent: child
                .title
                .strip_prefix(&prefix)
                .or(child.agent_profile.as_deref())
                .unwrap_or(&child.title)
                .to_owned(),
            status: child.status,
            session: board::newest_session(sessions, child.id).cloned(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{SessionKind, SessionState, StateSource};

    use super::*;

    fn task(n: u128, title: &str, fanout_of: Option<Ulid>) -> Task {
        Task {
            id: Ulid(n),
            project_id: Ulid(1),
            title: title.into(),
            prompt: String::new(),
            agent_profile: None,
            status: TaskStatus::Running,
            position: 0.0,
            checkpoint: None,
            error_flag: false,
            worktree: fanout_of.is_some(),
            worktree_path: None,
            branch: None,
            fanout_of,
            created_at: n as i64,
            updated_at: 0,
        }
    }

    fn session(task: &Task, state: SessionState, exit_code: Option<i32>) -> Session {
        Session {
            id: Ulid::new(),
            task_id: Some(task.id),
            project_id: task.project_id,
            agent_profile: "fake".into(),
            kind: SessionKind::Agent,
            title: None,
            native_session_id: None,
            state,
            state_source: StateSource::Daemon,
            state_changed_at: 0,
            exit_code,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
        }
    }

    #[test]
    fn the_picker_takes_two_to_five_agents_in_tick_order() {
        let mut picker = FanOutPicker::new(Ulid(7));
        assert_eq!(picker.agents, Agents::Loading);
        let mut shown = vec![picker.button_label()];
        picker.toggle("claude");
        shown.push(picker.button_label());
        assert_eq!(picker.request(), None, "one agent is not a fan-out");
        picker.toggle("codex");
        shown.push(picker.button_label());
        for profile in ["omp", "a", "b", "c"] {
            picker.toggle(profile);
        }
        assert!(picker.is_full());
        assert!(!picker.is_picked("c"), "a sixth tick is ignored");
        shown.push(picker.button_label());
        picker.toggle("claude");
        assert!(!picker.is_full() && !picker.is_picked("claude"));
        shown.push(picker.button_label());
        insta::assert_snapshot!(shown.join("\n"));

        let request = picker.request().unwrap();
        assert_eq!(request.task_id, Ulid(7));
        assert_eq!(request.agent_profiles, ["codex", "omp", "a", "b"]);
        assert!(request.cols > 0 && request.rows > 0);
    }

    #[test]
    fn a_group_card_lists_its_children_with_their_states() {
        let original = task(2, "Fix login", None);
        let mut claude = task(5, "Fix login · Claude Code", Some(original.id));
        claude.status = TaskStatus::Review;
        let codex = task(4, "Fix login · Codex", Some(original.id));
        let mut renamed = task(6, "Something else", Some(original.id));
        renamed.agent_profile = Some("omp".into());
        renamed.status = TaskStatus::Todo;
        let unrelated = task(3, "Other", None);
        let tasks = [
            original.clone(),
            claude.clone(),
            codex.clone(),
            renamed,
            unrelated.clone(),
        ];
        let sessions = [
            session(&claude, SessionState::Exited, Some(0)),
            session(&codex, SessionState::NeedsInput, None),
        ];

        let lines = children(&original, &tasks, &sessions);
        let shown: Vec<String> = lines
            .iter()
            .map(|line| format!("{} — {}", line.agent, line.state_text(0)))
            .collect();
        insta::assert_snapshot!(shown.join("\n"));
        assert_eq!(lines[0].task_id, codex.id, "oldest first");
        assert!(children(&unrelated, &tasks, &sessions).is_empty());
    }
}
