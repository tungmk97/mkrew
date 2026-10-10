//! Fan-out on the board (SPEC §20E.2): the "Run with several agents…" picker of one card, the
//! lines of a group card, one per child task, the "Compare" view of its children and the
//! "Pick" of one of them. Pure and tested headlessly; `main.rs` renders it.

use mkrew_protocol::{
    GitMode, GitStatus, GitStatusParams, RemoveWorktreeResult, Request, Session, SessionState,
    Task, TaskConfirmParams, TaskFanoutParams, TaskStatus,
};
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
    if children.is_empty() {
        // Most cards never fanned out: no title prefix to build (runs per card per frame).
        return Vec::new();
    }
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

/// The "Compare" view of a group card whose children are all in Review: one column per child.
#[derive(Debug, Clone, PartialEq)]
pub struct Compare {
    pub original_id: Ulid,
    pub project_id: Ulid,
    pub title: String,
    pub columns: Vec<CompareColumn>,
}

/// One child in the Compare view.
#[derive(Debug, Clone, PartialEq)]
pub struct CompareColumn {
    pub task_id: Ulid,
    pub agent: String,
    /// `⎇ mkrew/<branch>`.
    pub branch: Option<String>,
    /// Where its changes are counted from: its worktree's HEAD when the run started.
    pub checkpoint: Option<String>,
    /// Its worktree still exists (so it can be merged or removed).
    pub has_worktree: bool,
    /// Its newest session (run time, live or not).
    pub session: Option<Session>,
    pub changes: Changes,
}

/// A child's changes since its checkpoint (`git.status` in base mode on its worktree).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Changes {
    Loading,
    Failed(String),
    Counted {
        files: usize,
        added: u32,
        removed: u32,
    },
}

impl Changes {
    pub fn of(status: &GitStatus) -> Self {
        let files = &status.against_base;
        Changes::Counted {
            files: files.len(),
            added: files.iter().filter_map(|file| file.added).sum(),
            removed: files.iter().filter_map(|file| file.removed).sum(),
        }
    }

    /// `3 files · +40 −12`.
    pub fn text(&self) -> String {
        match self {
            Changes::Loading => "Loading…".into(),
            Changes::Failed(error) => error.clone(),
            Changes::Counted {
                files,
                added,
                removed,
            } => {
                let files = if *files == 1 {
                    "1 file".to_owned()
                } else {
                    format!("{files} files")
                };
                format!("{files} · +{added} −{removed}")
            }
        }
    }
}

impl Compare {
    /// The view of `original`'s children; `None` unless it fanned out and every child is in
    /// Review.
    pub fn new(original: &Task, tasks: &[Task], sessions: &[Session]) -> Option<Self> {
        let lines = children(original, tasks, sessions);
        if lines.is_empty() || lines.iter().any(|line| line.status != TaskStatus::Review) {
            return None;
        }
        let columns = lines
            .into_iter()
            .filter_map(|line| {
                let task = tasks.iter().find(|task| task.id == line.task_id)?;
                Some(CompareColumn {
                    task_id: line.task_id,
                    agent: line.agent,
                    branch: crate::worktree_target::branch_line(task),
                    checkpoint: task.checkpoint.clone(),
                    has_worktree: task.worktree_path.is_some(),
                    session: line.session,
                    changes: match (&task.checkpoint, &task.worktree_path) {
                        (Some(_), Some(_)) => Changes::Loading,
                        (None, _) => Changes::Failed("No run to compare".into()),
                        (_, None) => Changes::Failed("Worktree removed".into()),
                    },
                })
            })
            .collect();
        Some(Self {
            original_id: original.id,
            project_id: original.project_id,
            title: original.title.clone(),
            columns,
        })
    }

    /// The `git.status` reading each loading column's changes since its checkpoint.
    pub fn status_requests(&self) -> Vec<(Ulid, Request)> {
        self.columns
            .iter()
            .filter(|column| column.changes == Changes::Loading)
            .filter_map(|column| {
                let request = Request::GitStatus(GitStatusParams {
                    project_id: self.project_id,
                    worktree_task_id: Some(column.task_id),
                    repo: None,
                    mode: GitMode::Base,
                    since: Some(column.checkpoint.clone()?),
                });
                Some((column.task_id, request))
            })
            .collect()
    }

    /// A column's `git.status` answer.
    pub fn counted(&mut self, task_id: Ulid, status: Result<GitStatus, String>) {
        if let Some(column) = self.columns.iter_mut().find(|c| c.task_id == task_id) {
            column.changes = match status {
                Ok(status) => Changes::of(&status),
                Err(error) => Changes::Failed(format!("Cannot read its changes: {error}")),
            };
        }
    }

    /// The Pick of `task_id`: its merge, then the others' worktrees to remove. `None` for a
    /// child without a worktree (nothing to merge).
    pub fn pick(&self, task_id: Ulid) -> Option<Pick> {
        self.columns
            .iter()
            .find(|column| column.task_id == task_id && column.has_worktree)?;
        Some(Pick {
            original_id: self.original_id,
            original_title: self.title.clone(),
            others: self
                .columns
                .iter()
                .filter(|column| column.task_id != task_id && column.has_worktree)
                .map(|column| Other {
                    task_id: column.task_id,
                    agent: column.agent.clone(),
                    outcome: None,
                })
                .collect(),
            removing: false,
        })
    }
}

impl CompareColumn {
    /// How long its newest session ran.
    pub fn run_time(&self, now_ms: i64) -> Option<String> {
        let session = self.session.as_ref()?;
        Some(status::format_duration(
            board::run_end(session, now_ms) - session.started_at,
        ))
    }
}

/// "Pick" in the Compare view: the merge of one child (the Merge back dialog). Once it merged,
/// the original moves to Review and the other children's worktrees can be removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    pub original_id: Ulid,
    pub original_title: String,
    /// The other children with a worktree.
    pub others: Vec<Other>,
    /// Their removal was asked.
    pub removing: bool,
}

/// Another child of a Pick and how removing its worktree went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Other {
    pub task_id: Ulid,
    pub agent: String,
    /// `Ok(branch_kept)` or why it was not removed; `None` = not asked or in flight.
    pub outcome: Option<Result<bool, String>>,
}

impl Pick {
    /// The offer's button, until it was taken (none without others).
    pub fn offer(&self) -> Option<String> {
        match self.others.len() {
            _ if self.removing => None,
            0 => None,
            1 => Some("Remove the other worktree".into()),
            n => Some(format!("Remove the other {n} worktrees")),
        }
    }

    /// Takes the offer: the `task.remove_worktree` of each other child. One whose terminal still
    /// runs is not asked (the daemon refuses it): it reports why at once.
    pub fn start_removal(&mut self, sessions: &[Session]) -> Vec<(Ulid, Request)> {
        self.removing = true;
        let mut requests = Vec::new();
        for other in &mut self.others {
            let live = sessions
                .iter()
                .any(|s| s.task_id == Some(other.task_id) && s.state != SessionState::Exited);
            if live {
                other.outcome = Some(Err("Close its terminal first.".into()));
            } else {
                requests.push((
                    other.task_id,
                    Request::TaskRemoveWorktree(TaskConfirmParams {
                        task_id: other.task_id,
                        confirm: true,
                    }),
                ));
            }
        }
        requests
    }

    /// The answer for one other child.
    pub fn removed(&mut self, task_id: Ulid, result: Result<RemoveWorktreeResult, String>) {
        if let Some(other) = self.others.iter_mut().find(|o| o.task_id == task_id) {
            other.outcome = Some(result.map(|removed| removed.branch_kept));
        }
    }

    /// What the merged dialog adds: the original's move, then each removal once asked.
    pub fn lines(&self) -> Vec<String> {
        let mut lines = vec![format!("“{}” moves to Review.", self.original_title)];
        if self.removing {
            lines.extend(self.others.iter().map(|other| {
                // Icon + text (SPEC §11).
                let (icon, outcome) = match &other.outcome {
                    None => ("…", "removing…".to_owned()),
                    Some(Ok(true)) => ("✓", "worktree removed, unmerged branch kept".to_owned()),
                    Some(Ok(false)) => ("✓", "worktree and branch removed".to_owned()),
                    Some(Err(why)) => ("✕", format!("not removed: {why}")),
                };
                format!("{icon} {} — {outcome}", other.agent)
            }));
        }
        lines
    }
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
            terminal_title: None,
            color: None,
            native_session_id: None,
            state,
            state_source: StateSource::Daemon,
            state_changed_at: 0,
            exit_code,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
            pinned: false,
            plan_path: None,
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

    /// A child in Review with its worktree and a checkpoint.
    fn reviewed(n: u128, agent: &str, original: &Task) -> Task {
        let mut child = task(
            n,
            &format!("{} · {agent}", original.title),
            Some(original.id),
        );
        child.status = TaskStatus::Review;
        child.checkpoint = Some(format!("{n:040}"));
        child.worktree_path = Some(format!("/data/wt/{n}"));
        child.branch = Some(format!("mkrew/fix-login-{n}"));
        child
    }

    fn changed(
        path: &str,
        added: Option<u32>,
        removed: Option<u32>,
    ) -> mkrew_protocol::ChangedFile {
        mkrew_protocol::ChangedFile {
            path: path.into(),
            old_path: None,
            kind: mkrew_protocol::ChangeKind::Modified,
            added,
            removed,
        }
    }

    #[test]
    fn compare_waits_for_every_child_in_review_and_counts_their_changes() {
        let original = task(2, "Fix login", None);
        let claude = reviewed(4, "Claude Code", &original);
        let mut codex = reviewed(5, "Codex", &original);
        codex.status = TaskStatus::Running;
        let mut tasks = vec![original.clone(), claude.clone(), codex.clone()];
        assert_eq!(
            Compare::new(&original, &tasks, &[]),
            None,
            "Codex still runs"
        );
        assert_eq!(Compare::new(&claude, &tasks, &[]), None, "not a fan-out");

        tasks[2].status = TaskStatus::Review;
        let mut no_run = reviewed(6, "omp", &original);
        no_run.checkpoint = None;
        tasks.push(no_run);
        let mut ran = session(&claude, SessionState::Exited, Some(0));
        ran.started_at = 1_000;
        ran.ended_at = Some(126_000);
        let mut compare = Compare::new(&original, &tasks, &[ran]).unwrap();
        assert_eq!(compare.original_id, original.id);

        let requests = compare.status_requests();
        assert_eq!(requests.len(), 2, "the child without a run is not read");
        let Request::GitStatus(params) = &requests[0].1 else {
            panic!("git.status");
        };
        assert_eq!(params.worktree_task_id, Some(claude.id));
        assert_eq!(params.mode, GitMode::Base);
        assert_eq!(params.since, claude.checkpoint);

        let status = GitStatus {
            against_base: vec![
                changed("src/login.rs", Some(40), Some(12)),
                changed("logo.png", None, None),
                changed("README.md", Some(2), Some(0)),
            ],
            ..Default::default()
        };
        let one = GitStatus {
            against_base: vec![changed("a.rs", Some(1), Some(0))],
            ..Default::default()
        };
        assert_eq!(Changes::of(&one).text(), "1 file · +1 −0");
        compare.counted(claude.id, Ok(status));
        let shown_before_codex: Vec<String> =
            compare.columns.iter().map(|c| c.changes.text()).collect();
        compare.counted(codex.id, Err("no such worktree".into()));
        let shown: Vec<String> = compare
            .columns
            .iter()
            .map(|c| {
                format!(
                    "{} | {} | {} | {}",
                    c.agent,
                    c.branch.as_deref().unwrap_or("-"),
                    c.changes.text(),
                    c.run_time(0).unwrap_or_else(|| "-".into())
                )
            })
            .collect();
        insta::assert_snapshot!(format!(
            "{}\n---\n{}",
            shown_before_codex.join("\n"),
            shown.join("\n")
        ));
    }

    #[test]
    fn a_pick_moves_the_original_and_offers_removing_the_other_worktrees() {
        let original = task(2, "Fix login", None);
        let claude = reviewed(4, "Claude Code", &original);
        let codex = reviewed(5, "Codex", &original);
        let omp = reviewed(6, "omp", &original);
        let mut gone = reviewed(7, "pi", &original);
        gone.worktree_path = None;
        let tasks = [
            original.clone(),
            claude,
            codex.clone(),
            omp.clone(),
            gone.clone(),
        ];
        let compare = Compare::new(&original, &tasks, &[]).unwrap();
        assert_eq!(compare.pick(gone.id), None, "nothing to merge");

        let mut pick = compare.pick(codex.id).unwrap();
        assert_eq!(pick.original_id, original.id);
        let others: Vec<&str> = pick.others.iter().map(|o| o.agent.as_str()).collect();
        assert_eq!(
            others,
            ["Claude Code", "omp"],
            "the picked one and pi's are not removed"
        );
        let offered = pick.offer().unwrap();
        let before = pick.lines();

        let live = session(&omp, SessionState::Idle, None);
        let requests = pick.start_removal(&[live]);
        assert_eq!(pick.offer(), None, "asked once");
        assert_eq!(requests.len(), 1, "a live terminal is not asked");
        assert!(matches!(
            &requests[0],
            (_, Request::TaskRemoveWorktree(params)) if params.confirm
        ));
        let in_flight = pick.lines();
        pick.removed(
            requests[0].0,
            Ok(RemoveWorktreeResult { branch_kept: true }),
        );
        insta::assert_snapshot!(
            [
                offered,
                before.join("\n"),
                in_flight.join("\n"),
                pick.lines().join("\n")
            ]
            .join("\n---\n")
        );
    }
}
