//! Task board view-model (SPEC §10): a workspace's tasks in one column per status, ordered by
//! position, kept live by `task.updated` / `task.deleted`. Each card shows its newest session's
//! status (§11) and run time. Pure and tested headlessly; `main.rs` renders it.

use std::collections::HashSet;

use mkrew_core::task::{self, Placement};
use mkrew_protocol::{Event, Session, SessionState, Task, TaskMoveParams, TaskStatus};
use ulid::Ulid;

use crate::fanout::{self, ChildLine};
use crate::status;
use crate::worktree_target;

/// The value carried while a card is dragged (SPEC §10).
#[derive(Debug, Clone)]
pub struct DraggedCard {
    pub task_id: Ulid,
    pub title: String,
}

/// What the main area of a workspace shows (SPEC §10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    Board,
    /// The terminal tabs; the default, so a workspace opens on its sessions.
    #[default]
    Terminals,
    /// The Markdown review (SPEC §12).
    Docs,
    /// The opened files' tabs and editor (SPEC §12A); the tree is in the right panel.
    Editor,
    /// The open diff (SPEC §12B); the Git list is in the right panel.
    Diff,
}

impl View {
    pub const ALL: [View; 5] = [
        View::Terminals,
        View::Board,
        View::Docs,
        View::Editor,
        View::Diff,
    ];

    /// The view bar's views: Docs while `[features] docs` is on, Editor while files are
    /// open, Diff while a diff is open.
    pub fn bar(docs: bool, editor: bool, diff: bool) -> impl Iterator<Item = View> {
        View::ALL.into_iter().filter(move |&view| match view {
            View::Docs => docs,
            View::Editor => editor,
            View::Diff => diff,
            View::Board | View::Terminals => true,
        })
    }

    pub fn title(self) -> &'static str {
        match self {
            View::Board => "Board",
            View::Terminals => "Terminals",
            View::Docs => "Docs",
            View::Editor => "Editor",
            View::Diff => "Diff",
        }
    }

    /// The keymap command that shows this view (its rail tooltip shows the keys).
    pub fn command(self) -> crate::keymap::Command {
        use crate::keymap::Command;
        match self {
            View::Board => Command::ShowBoard,
            View::Terminals => Command::ShowTerminals,
            View::Docs => Command::ShowDocs,
            View::Editor => Command::ShowEditor,
            View::Diff => Command::ShowDiff,
        }
    }
}

/// Board columns, left to right.
pub const COLUMNS: [TaskStatus; 5] = [
    TaskStatus::Todo,
    TaskStatus::Running,
    TaskStatus::NeedsInput,
    TaskStatus::Review,
    TaskStatus::Done,
];

pub fn column_title(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Todo => "Todo",
        TaskStatus::Running => "Running",
        TaskStatus::NeedsInput => "Needs input",
        TaskStatus::Review => "Review",
        TaskStatus::Done => "Done",
    }
}

/// Shown for a task without its own agent in a workspace without a default agent.
pub const DEFAULT_AGENT_LABEL: &str = "default agent";

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub status: TaskStatus,
    pub cards: Vec<Card>,
}

/// The button a card shows for its column (SPEC §10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardAction {
    /// Todo: `task.run`.
    Run,
    /// Review: move it to Done.
    Approve,
}

/// Board keys (SPEC §13 "Board"): the arrows move the selection between cards, Shift+←/→ move
/// the selected card one column, Enter opens it, `e` edits it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardKey {
    Up,
    Down,
    Left,
    Right,
    MoveLeft,
    MoveRight,
    Open,
    Edit,
}

impl CardKey {
    pub const ALL: [CardKey; 8] = [
        CardKey::Up,
        CardKey::Down,
        CardKey::Left,
        CardKey::Right,
        CardKey::MoveLeft,
        CardKey::MoveRight,
        CardKey::Open,
        CardKey::Edit,
    ];

    /// The part of the `keymap.toml` name after `card_`.
    pub fn name(self) -> &'static str {
        match self {
            CardKey::Up => "up",
            CardKey::Down => "down",
            CardKey::Left => "left",
            CardKey::Right => "right",
            CardKey::MoveLeft => "move_left",
            CardKey::MoveRight => "move_right",
            CardKey::Open => "open",
            CardKey::Edit => "edit",
        }
    }
}

/// What Enter does on a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardOpen {
    /// Show the card's live session.
    Session(Ulid),
    /// No live session: the edit dialog.
    Edit,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub task_id: Ulid,
    pub title: String,
    pub status: TaskStatus,
    /// The task's agent, else the workspace default, else [`DEFAULT_AGENT_LABEL`].
    pub agent: String,
    /// The last run exited non-zero.
    pub error_flag: bool,
    /// A run stored the project's git HEAD (`task.checkpoint`), so it can be undone.
    pub checkpoint: bool,
    /// `⎇ mkrew/<branch>` once the task's worktree exists (SPEC §20E.1).
    pub branch: Option<String>,
    /// The task this one is a fan-out child of (SPEC §20E.2).
    pub fanout_of: Option<Ulid>,
    /// A fan-out original's children, oldest first: the card is their group card.
    pub children: Vec<ChildLine>,
    /// The task's newest session, if it ever ran.
    pub session: Option<Session>,
}

impl Card {
    pub fn action(&self) -> Option<CardAction> {
        match self.status {
            // A fan-out original cannot run itself.
            TaskStatus::Todo if !self.children.is_empty() => None,
            TaskStatus::Todo => Some(CardAction::Run),
            TaskStatus::Review => Some(CardAction::Approve),
            TaskStatus::Running | TaskStatus::NeedsInput | TaskStatus::Done => None,
        }
    }

    /// Whether the card offers "Run with…" (SPEC §20B.6): a Todo or Review card without a live
    /// session, unless it fanned out.
    pub fn can_run_with(&self) -> bool {
        matches!(self.status, TaskStatus::Todo | TaskStatus::Review)
            && self.children.is_empty()
            && self
                .session
                .as_ref()
                .is_none_or(|session| session.state == SessionState::Exited)
    }

    /// Whether the card offers "Run with several agents…" (SPEC §20E.2): as "Run with…", but
    /// not on a fan-out child.
    pub fn can_fan_out(&self) -> bool {
        self.can_run_with() && self.fanout_of.is_none()
    }

    /// The profile the card runs with (`None`: neither the task nor the workspace has one, or
    /// a group card, which runs none).
    pub fn agent_profile(&self) -> Option<&str> {
        Some(self.agent.as_str())
            .filter(|&agent| agent != DEFAULT_AGENT_LABEL && self.children.is_empty())
    }

    /// Whether the group card offers "Compare" (SPEC §20E.2): every child is in Review and
    /// none was picked yet (a Pick moves the original out of Running).
    pub fn can_compare(&self) -> bool {
        matches!(self.status, TaskStatus::Running | TaskStatus::NeedsInput)
            && !self.children.is_empty()
            && self
                .children
                .iter()
                .all(|child| child.status == TaskStatus::Review)
    }

    /// Whether the card offers "Undo task": a finished (Review / Done) run with a checkpoint.
    pub fn can_undo(&self) -> bool {
        self.checkpoint && matches!(self.status, TaskStatus::Review | TaskStatus::Done)
    }

    /// Whether the card offers "Merge back…" and "Remove worktree…" (SPEC §20E.1): a Review /
    /// Done card whose worktree exists.
    pub fn manages_worktree(&self) -> bool {
        self.branch.is_some() && matches!(self.status, TaskStatus::Review | TaskStatus::Done)
    }

    /// Whether the card offers "View changes" (since the checkpoint, in the Git tab): a
    /// Review card with a checkpoint (SPEC §12B).
    pub fn can_view_changes(&self) -> bool {
        self.checkpoint && self.status == TaskStatus::Review
    }

    /// The session a click on the card shows: its newest one, except on a Todo card (a card
    /// dragged back to Todo is about to run again; clicking it only selects it).
    pub fn opens_session(&self) -> Option<Ulid> {
        self.session
            .as_ref()
            .filter(|_| self.status != TaskStatus::Todo)
            .map(|session| session.id)
    }

    /// The live session a follow-up typed on the card goes to (running / needs-input cards).
    pub fn follow_up_session(&self) -> Option<Ulid> {
        self.session
            .as_ref()
            .filter(|session| session.state != SessionState::Exited)
            .filter(|_| matches!(self.status, TaskStatus::Running | TaskStatus::NeedsInput))
            .map(|session| session.id)
    }

    /// How long the newest session ran (until now while it still runs).
    pub fn run_time(&self, now_ms: i64) -> Option<String> {
        let session = self.session.as_ref()?;
        Some(status::format_duration(
            run_end(session, now_ms) - session.started_at,
        ))
    }

    /// Whether the card adds a "last run failed" line: not when its session's status already
    /// shows the exit.
    pub fn shows_error_row(&self) -> bool {
        self.error_flag
            && self
                .session
                .as_ref()
                .is_none_or(|session| session.state != SessionState::Exited)
    }
}

/// Where a run's clock stops: at exit, or when the agent finished its turn (a `done` agent
/// still waits at its prompt), else it still runs.
pub(crate) fn run_end(session: &Session, now_ms: i64) -> i64 {
    match (session.ended_at, session.state) {
        (Some(ended_at), _) => ended_at,
        (None, SessionState::Done) => session.state_changed_at,
        (None, _) => now_ms,
    }
}

#[derive(Debug, Default)]
pub struct Board {
    /// Every workspace's tasks, as listed and updated by events.
    tasks: Vec<Task>,
    /// Set once a `task.list` arrived.
    loaded: bool,
    /// Task events applied since [`Board::begin_load`], replayed by [`Board::load`] (events
    /// arrive on another channel than replies, so the list may predate them).
    since_load: Option<Vec<Event>>,
}

impl Board {
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// A `task.list` (all workspaces) was just sent.
    pub fn begin_load(&mut self) {
        self.since_load = Some(Vec::new());
    }

    /// The list could not be read: stop recording events.
    pub fn cancel_load(&mut self) {
        self.since_load = None;
    }

    /// Replaces the tasks with a fresh `task.list`, then replays the events applied since
    /// [`Board::begin_load`].
    pub fn load(&mut self, tasks: Vec<Task>) {
        self.tasks = tasks;
        self.loaded = true;
        for event in self.since_load.take().unwrap_or_default() {
            self.apply(&event);
        }
    }

    /// Applies a daemon event; returns whether the board changed.
    pub fn apply(&mut self, event: &Event) -> bool {
        if let Some(since_load) = &mut self.since_load
            && matches!(event, Event::TaskUpdated(_) | Event::TaskDeleted(_))
        {
            since_load.push(event.clone());
        }
        match event {
            Event::TaskUpdated(task) => match self.tasks.iter_mut().find(|t| t.id == task.id) {
                // A replayed event older than the listed task does not undo it.
                Some(existing) if existing.updated_at > task.updated_at => false,
                Some(existing) => {
                    *existing = task.clone();
                    true
                }
                None => {
                    self.tasks.push(task.clone());
                    true
                }
            },
            Event::TaskDeleted(deleted) => {
                let before = self.tasks.len();
                self.tasks.retain(|task| task.id != deleted.task_id);
                self.tasks.len() != before
            }
            _ => false,
        }
    }

    /// Every workspace's tasks.
    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub fn task(&self, task_id: Ulid) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == task_id)
    }

    /// Whether `project_id` has any task.
    pub fn has_tasks(&self, project_id: Ulid) -> bool {
        self.tasks.iter().any(|task| task.project_id == project_id)
    }

    /// The columns of `project_id`'s board: every status in [`COLUMNS`] order, cards by
    /// ascending position. `default_agent` is the workspace's.
    pub fn columns(
        &self,
        project_id: Ulid,
        default_agent: Option<&str>,
        sessions: &[Session],
    ) -> Vec<Column> {
        let tasks = self.ordered(project_id);
        // Built once per call: `fanout::children` scans every task and formats a prefix, which
        // only fan-out originals need (this runs on every board render).
        let originals: HashSet<Ulid> = self.tasks.iter().filter_map(|t| t.fanout_of).collect();
        COLUMNS
            .iter()
            .map(|&status| Column {
                status,
                cards: tasks
                    .iter()
                    .filter(|task| task.status == status)
                    .map(|task| {
                        let children = if originals.contains(&task.id) {
                            fanout::children(task, &self.tasks, sessions)
                        } else {
                            Vec::new()
                        };
                        // A group card runs no agent of its own.
                        let agent = match children.len() {
                            0 => task
                                .agent_profile
                                .as_deref()
                                .or(default_agent)
                                .unwrap_or(DEFAULT_AGENT_LABEL)
                                .to_owned(),
                            1 => "1 agent".to_owned(),
                            n => format!("{n} agents"),
                        };
                        Card {
                            task_id: task.id,
                            title: task.title.clone(),
                            status: task.status,
                            agent,
                            error_flag: task.error_flag,
                            checkpoint: task.checkpoint.is_some(),
                            branch: worktree_target::branch_line(task),
                            fanout_of: task.fanout_of,
                            // A group card shows its children's states, not a run of its own
                            // from before it fanned out.
                            session: newest_session(sessions, task.id)
                                .filter(|_| children.is_empty())
                                .cloned(),
                            children,
                        }
                    })
                    .collect(),
            })
            .collect()
    }

    /// `project_id`'s tasks in board order: by position, ties by id.
    fn ordered(&self, project_id: Ulid) -> Vec<&Task> {
        let mut tasks: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|task| task.project_id == project_id)
            .collect();
        tasks.sort_by(|a, b| a.position.total_cmp(&b.position).then(a.id.cmp(&b.id)));
        tasks
    }

    /// The `task.move`s that drop `task_id` into the `status` column right before the card
    /// `before` (`None` = at the end): one for the dropped card, or one per card whose position
    /// changes when the gap collapsed. Empty when the card would stay where it is. A `before`
    /// no longer in that column (moved meanwhile) means the end.
    pub fn drop_moves(
        &self,
        task_id: Ulid,
        status: TaskStatus,
        before: Option<Ulid>,
    ) -> Vec<TaskMoveParams> {
        let Some(dropped) = self.task(task_id) else {
            return Vec::new();
        };
        if before == Some(task_id) {
            return Vec::new();
        }
        let column = self.ordered(dropped.project_id);
        let others: Vec<&Task> = column
            .iter()
            .copied()
            .filter(|task| task.status == status && task.id != task_id)
            .collect();
        let index = before
            .and_then(|before| others.iter().position(|task| task.id == before))
            .unwrap_or(others.len());
        let current = column
            .iter()
            .filter(|task| task.status == status)
            .position(|task| task.id == task_id);
        if current == Some(index) {
            return Vec::new();
        }
        let positions: Vec<f64> = others.iter().map(|task| task.position).collect();
        match task::place(&positions, index) {
            Placement::At(position) => vec![TaskMoveParams {
                task_id,
                status,
                position,
            }],
            Placement::Renumber(fresh) => {
                let mut order = others;
                order.insert(index, dropped);
                order
                    .into_iter()
                    .zip(fresh)
                    .filter(|(task, position)| task.id == task_id || task.position != *position)
                    .map(|(task, position)| TaskMoveParams {
                        task_id: task.id,
                        status,
                        position,
                    })
                    .collect()
            }
        }
    }

    /// Whether `task_id` can run now (it waits in Todo and did not fan out); Ctrl+Enter runs
    /// the selected card.
    pub fn can_run(&self, task_id: Ulid) -> bool {
        self.task(task_id)
            .is_some_and(|task| task.status == TaskStatus::Todo)
            && !self.fanned_out(task_id)
    }

    /// Whether `task_id` is a fan-out original (some task is its child).
    fn fanned_out(&self, task_id: Ulid) -> bool {
        self.tasks
            .iter()
            .any(|task| task.fanout_of == Some(task_id))
    }

    /// Whether "Run with several agents…" may still fan `task_id` out (see
    /// [`Card::can_fan_out`]).
    pub fn can_fan_out(&self, task_id: Ulid, sessions: &[Session]) -> bool {
        self.can_run_with(task_id, sessions)
            && self
                .task(task_id)
                .is_some_and(|task| task.fanout_of.is_none())
    }

    /// Whether "Run with…" may still run `task_id` (see [`Card::can_run_with`]).
    pub fn can_run_with(&self, task_id: Ulid, sessions: &[Session]) -> bool {
        !self.fanned_out(task_id)
            && self.task(task_id).is_some_and(|task| {
                matches!(task.status, TaskStatus::Todo | TaskStatus::Review)
                    && !sessions.iter().any(|session| {
                        session.task_id == Some(task_id) && session.state != SessionState::Exited
                    })
            })
    }

    /// The card Ctrl+Enter runs on `project_id`'s board: the selected one, if it is that
    /// board's and can run.
    pub fn run_target(&self, selected: Option<Ulid>, project_id: Ulid) -> Option<Ulid> {
        selected.filter(|&id| {
            self.can_run(id)
                && self
                    .task(id)
                    .is_some_and(|task| task.project_id == project_id)
        })
    }

    /// The card `a` approves on `project_id`'s board: the selected one, if it is that board's
    /// and sits in Review (what its Approve button needs).
    pub fn approve_target(&self, selected: Option<Ulid>, project_id: Ulid) -> Option<Ulid> {
        selected.filter(|&id| {
            self.task(id).is_some_and(|task| {
                task.project_id == project_id && task::approve(task.status).is_some()
            })
        })
    }

    /// Whether `task_id`'s card still shows a follow-up field for `session_id` (see
    /// [`Card::follow_up_session`]).
    pub fn takes_follow_up(&self, task_id: Ulid, session_id: Ulid, sessions: &[Session]) -> bool {
        self.task(task_id).is_some_and(|task| {
            matches!(task.status, TaskStatus::Running | TaskStatus::NeedsInput)
                && newest_session(sessions, task_id).is_some_and(|session| {
                    session.id == session_id && session.state != SessionState::Exited
                })
        })
    }

    /// The `task.move`s that approve a Review card: to the end of Done. Empty for any other
    /// card.
    pub fn approve_moves(&self, task_id: Ulid) -> Vec<TaskMoveParams> {
        match self
            .task(task_id)
            .and_then(|task| task::approve(task.status))
        {
            Some(done) => self.drop_moves(task_id, done, None),
            None => Vec::new(),
        }
    }

    /// `project_id`'s cards, one list of ids per column of [`COLUMNS`] (board order).
    fn grid(&self, project_id: Ulid) -> Vec<Vec<Ulid>> {
        let tasks = self.ordered(project_id);
        COLUMNS
            .iter()
            .map(|&status| {
                tasks
                    .iter()
                    .filter(|task| task.status == status)
                    .map(|task| task.id)
                    .collect()
            })
            .collect()
    }

    /// The card an arrow key selects on `project_id`'s board: ↑ / ↓ the neighbour in the column,
    /// ← / → the card at the nearest row of the next column that has cards. Nothing at an edge.
    /// Without a (valid) selection any arrow selects the first card. Other keys: `None`.
    pub fn select_step(
        &self,
        project_id: Ulid,
        selected: Option<Ulid>,
        key: CardKey,
    ) -> Option<Ulid> {
        let step: isize = match key {
            CardKey::Up | CardKey::Left => -1,
            CardKey::Down | CardKey::Right => 1,
            _ => return None,
        };
        let grid = self.grid(project_id);
        let Some((col, row)) = selected.and_then(|id| locate(&grid, id)) else {
            return grid.iter().find_map(|ids| ids.first().copied());
        };
        if matches!(key, CardKey::Up | CardKey::Down) {
            return grid[col].get(row.checked_add_signed(step)?).copied();
        }
        let mut next = col;
        loop {
            next = next.checked_add_signed(step)?;
            let ids = grid.get(next)?;
            if let Some(last) = ids.len().checked_sub(1) {
                return Some(ids[row.min(last)]);
            }
        }
    }

    /// The `task.move`s of Shift+← / Shift+→ on `task_id`: a drop into the neighbouring column
    /// right before the card at the same row (the end when that column is shorter), so it goes
    /// through [`Board::drop_moves`] like a drag. Empty at the first / last column.
    pub fn shift_moves(&self, task_id: Ulid, key: CardKey) -> Vec<TaskMoveParams> {
        let step: isize = match key {
            CardKey::MoveLeft => -1,
            CardKey::MoveRight => 1,
            _ => return Vec::new(),
        };
        let Some(task) = self.task(task_id) else {
            return Vec::new();
        };
        let grid = self.grid(task.project_id);
        let Some((col, row)) = locate(&grid, task_id) else {
            return Vec::new();
        };
        let to = col.checked_add_signed(step);
        let (Some(ids), Some(&status)) = (
            to.and_then(|to| grid.get(to)),
            to.and_then(|to| COLUMNS.get(to)),
        ) else {
            return Vec::new();
        };
        self.drop_moves(task_id, status, ids.get(row).copied())
    }

    /// What Enter does on `task_id`'s card: its live session, else the edit dialog (a Todo
    /// card and a fan-out group card have no session to show). `None`: no such task.
    pub fn card_open(&self, task_id: Ulid, sessions: &[Session]) -> Option<CardOpen> {
        let task = self.task(task_id)?;
        let live = newest_session(sessions, task_id)
            .filter(|session| session.state != SessionState::Exited)
            .filter(|_| task.status != TaskStatus::Todo && !self.fanned_out(task_id));
        Some(live.map_or(CardOpen::Edit, |session| CardOpen::Session(session.id)))
    }

    /// Whether a run time on `project_id`'s board reads differently at `now_ms` than at
    /// `last_ms` (session spinners are covered by [`status::redraw_due`]).
    pub fn redraw_due(
        &self,
        project_id: Ulid,
        sessions: &[Session],
        last_ms: i64,
        now_ms: i64,
    ) -> bool {
        self.tasks
            .iter()
            .filter(|task| task.project_id == project_id)
            .filter_map(|task| newest_session(sessions, task.id))
            .any(|session| {
                status::format_duration(run_end(session, last_ms) - session.started_at)
                    != status::format_duration(run_end(session, now_ms) - session.started_at)
            })
    }

    /// Whether `task_id`'s card offers "Merge back…" and "Remove worktree…" (see
    /// [`Card::manages_worktree`]).
    pub fn manages_worktree(&self, task_id: Ulid) -> bool {
        self.task(task_id).is_some_and(|task| {
            worktree_target::branch_line(task).is_some()
                && matches!(task.status, TaskStatus::Review | TaskStatus::Done)
        })
    }
}

pub(crate) fn newest_session(sessions: &[Session], task_id: Ulid) -> Option<&Session> {
    sessions
        .iter()
        .filter(|session| session.task_id == Some(task_id))
        .max_by_key(|session| (session.started_at, session.id))
}

/// The column and row of `task_id` in a [`Board::grid`].
fn locate(grid: &[Vec<Ulid>], task_id: Ulid) -> Option<(usize, usize)> {
    grid.iter()
        .enumerate()
        .find_map(|(col, ids)| Some((col, ids.iter().position(|&id| id == task_id)?)))
}

#[cfg(test)]
mod tests;
