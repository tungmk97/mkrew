//! Task board view-model (SPEC §10): a workspace's tasks in one column per status, ordered by
//! position, kept live by `task.updated` / `task.deleted`. Each card shows its newest session's
//! status (§11) and run time. Pure and tested headlessly; `main.rs` renders it.

use mkrew_core::task::{self, Placement};
use mkrew_protocol::{Event, Session, SessionState, Task, TaskMoveParams, TaskStatus};
use ulid::Ulid;

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
    /// The file tree and read-only viewer (SPEC §12A).
    Files,
    /// Changed files and their diffs (SPEC §12B).
    Git,
}

impl View {
    pub const ALL: [View; 5] = [
        View::Board,
        View::Terminals,
        View::Docs,
        View::Files,
        View::Git,
    ];

    pub fn title(self) -> &'static str {
        match self {
            View::Board => "Board",
            View::Terminals => "Terminals",
            View::Docs => "Docs",
            View::Files => "Files",
            View::Git => "Git",
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
    /// The task's newest session, if it ever ran.
    pub session: Option<Session>,
}

impl Card {
    pub fn action(&self) -> Option<CardAction> {
        match self.status {
            TaskStatus::Todo => Some(CardAction::Run),
            TaskStatus::Review => Some(CardAction::Approve),
            TaskStatus::Running | TaskStatus::NeedsInput | TaskStatus::Done => None,
        }
    }

    /// Whether the card offers "Run with…" (SPEC §20B.6): a Todo or Review card without a live
    /// session.
    pub fn can_run_with(&self) -> bool {
        matches!(self.status, TaskStatus::Todo | TaskStatus::Review)
            && self
                .session
                .as_ref()
                .is_none_or(|session| session.state == SessionState::Exited)
    }

    /// The profile the card runs with (`None`: neither the task nor the workspace has one).
    pub fn agent_profile(&self) -> Option<&str> {
        Some(self.agent.as_str()).filter(|&agent| agent != DEFAULT_AGENT_LABEL)
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

    /// Whether the card offers "View changes" (since the checkpoint, in the Git view): a
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
fn run_end(session: &Session, now_ms: i64) -> i64 {
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
        COLUMNS
            .iter()
            .map(|&status| Column {
                status,
                cards: tasks
                    .iter()
                    .filter(|task| task.status == status)
                    .map(|task| Card {
                        task_id: task.id,
                        title: task.title.clone(),
                        status: task.status,
                        agent: task
                            .agent_profile
                            .as_deref()
                            .or(default_agent)
                            .unwrap_or(DEFAULT_AGENT_LABEL)
                            .to_owned(),
                        error_flag: task.error_flag,
                        checkpoint: task.checkpoint.is_some(),
                        branch: worktree_target::branch_line(task),
                        session: newest_session(sessions, task.id).cloned(),
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

    /// Whether `task_id` can run now (it waits in Todo); Ctrl+Enter runs the selected card.
    pub fn can_run(&self, task_id: Ulid) -> bool {
        self.task(task_id)
            .is_some_and(|task| task.status == TaskStatus::Todo)
    }

    /// Whether "Run with…" may still run `task_id` (see [`Card::can_run_with`]).
    pub fn can_run_with(&self, task_id: Ulid, sessions: &[Session]) -> bool {
        self.task(task_id).is_some_and(|task| {
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
}

fn newest_session(sessions: &[Session], task_id: Ulid) -> Option<&Session> {
    sessions
        .iter()
        .filter(|session| session.task_id == Some(task_id))
        .max_by_key(|session| (session.started_at, session.id))
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{SessionKind, StateSource, TaskDeletedEvent};

    use super::*;

    fn task(project_id: Ulid, title: &str, status: TaskStatus, position: f64) -> Task {
        Task {
            id: Ulid::new(),
            project_id,
            title: title.to_owned(),
            prompt: String::new(),
            agent_profile: None,
            status,
            position,
            checkpoint: None,
            error_flag: false,
            worktree: false,
            worktree_path: None,
            branch: None,
            fanout_of: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn session(task: &Task, started_at: i64, ended_at: Option<i64>) -> Session {
        Session {
            id: Ulid::new(),
            task_id: Some(task.id),
            project_id: task.project_id,
            agent_profile: "claude".to_owned(),
            kind: SessionKind::Agent,
            title: None,
            native_session_id: None,
            state: SessionState::Working,
            state_source: StateSource::Daemon,
            state_changed_at: started_at,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at,
            ended_at,
        }
    }

    fn titles(column: &Column) -> Vec<&str> {
        column
            .cards
            .iter()
            .map(|card| card.title.as_str())
            .collect()
    }

    #[test]
    fn columns_group_by_status_in_position_order_per_workspace() {
        let (api, web) = (Ulid::new(), Ulid::new());
        let mut board = Board::default();
        assert!(!board.is_loaded());
        board.load(vec![
            task(api, "second", TaskStatus::Todo, 2.0),
            task(api, "first", TaskStatus::Todo, 1.0),
            task(api, "review me", TaskStatus::Review, 1.0),
            task(web, "elsewhere", TaskStatus::Todo, 0.5),
        ]);
        assert!(board.is_loaded());
        assert!(board.has_tasks(api) && !board.has_tasks(Ulid::new()));

        let columns = board.columns(api, None, &[]);
        let statuses: Vec<TaskStatus> = columns.iter().map(|c| c.status).collect();
        assert_eq!(statuses, COLUMNS);
        assert_eq!(titles(&columns[0]), ["first", "second"]);
        assert!(columns[1].cards.is_empty() && columns[2].cards.is_empty());
        assert_eq!(titles(&columns[3]), ["review me"]);
        assert_eq!(column_title(TaskStatus::NeedsInput), "Needs input");
    }

    #[test]
    fn cards_name_their_agent_and_show_the_newest_session() {
        let api = Ulid::new();
        let mut own = task(api, "own agent", TaskStatus::Running, 1.0);
        own.agent_profile = Some("codex".to_owned());
        own.error_flag = true;
        let inherits = task(api, "default", TaskStatus::Todo, 1.0);
        let mut board = Board::default();
        board.load(vec![own.clone(), inherits]);

        let old = session(&own, 1_000, Some(5_000));
        let new = session(&own, 10_000, None);
        let sessions = [new.clone(), old];
        let columns = board.columns(api, Some("claude"), &sessions);
        let card = &columns[1].cards[0];
        assert_eq!(card.agent, "codex");
        assert!(card.error_flag);
        assert_eq!(card.session.as_ref().map(|s| s.id), Some(new.id));
        assert_eq!(card.run_time(130_000).as_deref(), Some("2m"));

        let todo = &columns[0].cards[0];
        assert_eq!(todo.agent, "claude");
        assert_eq!((todo.session.as_ref(), todo.run_time(0)), (None, None));
        assert_eq!(
            board.columns(api, None, &sessions)[0].cards[0].agent,
            DEFAULT_AGENT_LABEL
        );
    }

    #[test]
    fn a_finished_run_shows_its_length() {
        let api = Ulid::new();
        let done = task(api, "done", TaskStatus::Review, 1.0);
        let mut board = Board::default();
        board.load(vec![done.clone()]);
        let sessions = [session(&done, 1_000, Some(46_000))];
        let card = &board.columns(api, None, &sessions)[3].cards[0];
        assert_eq!(card.run_time(999_999).as_deref(), Some("45s"));
        // Nothing still runs, so nothing ticks.
        assert!(!board.redraw_due(api, &sessions, 0, 999_999));
    }

    #[test]
    fn a_done_turn_stops_the_clock_and_an_exit_shows_the_failure_once() {
        let api = Ulid::new();
        let mut failed = task(api, "failed", TaskStatus::Review, 1.0);
        failed.error_flag = true;
        let mut board = Board::default();
        board.load(vec![failed.clone()]);

        // The agent finished its turn at 21 s but still waits at its prompt.
        let mut done = session(&failed, 1_000, None);
        done.state = SessionState::Done;
        done.state_changed_at = 21_000;
        let card = &board.columns(api, None, std::slice::from_ref(&done))[3].cards[0];
        assert_eq!(card.run_time(999_999).as_deref(), Some("20s"));
        assert!(!board.redraw_due(api, std::slice::from_ref(&done), 30_000, 99_000));
        assert!(card.shows_error_row());

        let mut exited = done;
        exited.state = SessionState::Exited;
        exited.ended_at = Some(31_000);
        let card = &board.columns(api, None, &[exited])[3].cards[0];
        assert!(!card.shows_error_row(), "the exited status already says it");
        let no_session = &board.columns(api, None, &[])[3].cards[0];
        assert!(no_session.shows_error_row());
    }

    #[test]
    fn events_move_add_and_remove_cards() {
        let api = Ulid::new();
        let mut card = task(api, "card", TaskStatus::Todo, 1.0);
        let mut board = Board::default();
        board.load(vec![card.clone()]);

        card.status = TaskStatus::Running;
        card.updated_at = 1;
        assert!(board.apply(&Event::TaskUpdated(card.clone())));
        assert_eq!(titles(&board.columns(api, None, &[])[1]), ["card"]);

        let added = task(api, "added", TaskStatus::Todo, 1.0);
        assert!(board.apply(&Event::TaskUpdated(added.clone())));
        assert_eq!(titles(&board.columns(api, None, &[])[0]), ["added"]);

        let deleted = Event::TaskDeleted(TaskDeletedEvent {
            task_id: added.id,
            project_id: api,
        });
        assert!(board.apply(&deleted));
        assert!(!board.apply(&deleted), "already gone");
        assert!(board.columns(api, None, &[])[0].cards.is_empty());
    }

    #[test]
    fn events_newer_than_the_list_survive_a_load_and_older_ones_do_not_undo_it() {
        let api = Ulid::new();
        let mut moved = task(api, "moved", TaskStatus::Todo, 1.0);
        let gone = task(api, "gone", TaskStatus::Todo, 2.0);
        let mut board = Board::default();

        board.begin_load();
        // Handled before the reply: the card ran, another one was deleted.
        let stale = Event::TaskUpdated(moved.clone());
        moved.status = TaskStatus::Running;
        moved.updated_at = 5;
        board.apply(&Event::TaskUpdated(moved.clone()));
        board.apply(&Event::TaskDeleted(TaskDeletedEvent {
            task_id: gone.id,
            project_id: api,
        }));
        let mut listed = moved.clone();
        listed.title = "renamed later".to_owned();
        listed.updated_at = 9;
        board.load(vec![listed, gone]);

        let columns = board.columns(api, None, &[]);
        assert!(columns[0].cards.is_empty(), "the deleted card stays gone");
        assert_eq!(titles(&columns[1]), ["renamed later"]);
        assert!(!board.apply(&stale), "an older update is ignored");
    }

    fn ids(column: &Column) -> Vec<Ulid> {
        column.cards.iter().map(|card| card.task_id).collect()
    }

    /// Applies `moves` as the daemon would echo them.
    fn apply_moves(board: &mut Board, moves: &[TaskMoveParams]) {
        for moved in moves {
            let mut task = board.task(moved.task_id).cloned().unwrap();
            task.status = moved.status;
            task.position = moved.position;
            task.updated_at += 1;
            board.apply(&Event::TaskUpdated(task));
        }
    }

    #[test]
    fn dropping_moves_a_card_between_and_within_columns() {
        let api = Ulid::new();
        let (a, b, c) = (
            task(api, "a", TaskStatus::Todo, 1.0),
            task(api, "b", TaskStatus::Todo, 2.0),
            task(api, "c", TaskStatus::Review, 1.0),
        );
        let mut board = Board::default();
        board.load(vec![a.clone(), b.clone(), c.clone()]);

        // Into another column, before its only card.
        let moves = board.drop_moves(a.id, TaskStatus::Review, Some(c.id));
        assert_eq!(
            moves,
            [TaskMoveParams {
                task_id: a.id,
                status: TaskStatus::Review,
                position: 0.0,
            }]
        );
        apply_moves(&mut board, &moves);
        let columns = board.columns(api, None, &[]);
        assert_eq!(ids(&columns[0]), [b.id]);
        assert_eq!(ids(&columns[3]), [a.id, c.id]);

        // Within the column, to its end; then onto an empty column.
        let moves = board.drop_moves(a.id, TaskStatus::Review, None);
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0].position, 2.0);
        apply_moves(&mut board, &moves);
        assert_eq!(ids(&board.columns(api, None, &[])[3]), [c.id, a.id]);
        let moves = board.drop_moves(b.id, TaskStatus::Done, None);
        apply_moves(&mut board, &moves);
        let columns = board.columns(api, None, &[]);
        assert!(columns[0].cards.is_empty());
        assert_eq!(ids(&columns[4]), [b.id]);
    }

    #[test]
    fn dropping_a_card_where_it_already_is_moves_nothing() {
        let api = Ulid::new();
        let (a, b) = (
            task(api, "a", TaskStatus::Todo, 1.0),
            task(api, "b", TaskStatus::Todo, 2.0),
        );
        let mut board = Board::default();
        board.load(vec![a.clone(), b.clone()]);
        assert!(
            board
                .drop_moves(a.id, TaskStatus::Todo, Some(a.id))
                .is_empty()
        );
        assert!(
            board
                .drop_moves(a.id, TaskStatus::Todo, Some(b.id))
                .is_empty()
        );
        assert!(board.drop_moves(b.id, TaskStatus::Todo, None).is_empty());
        assert!(
            board
                .drop_moves(Ulid::new(), TaskStatus::Todo, None)
                .is_empty()
        );
        // The same slot in another column is a move.
        assert_eq!(board.drop_moves(b.id, TaskStatus::Done, None).len(), 1);
    }

    #[test]
    fn a_collapsed_gap_renumbers_the_column_and_the_order_holds() {
        let api = Ulid::new();
        let (a, b, c) = (
            task(api, "a", TaskStatus::Todo, 1.0),
            task(api, "b", TaskStatus::Todo, 1.0 + f64::EPSILON),
            task(api, "c", TaskStatus::Running, 0.0),
        );
        let mut board = Board::default();
        board.load(vec![a.clone(), b.clone(), c.clone()]);

        let moves = board.drop_moves(c.id, TaskStatus::Todo, Some(b.id));
        let moved: Vec<(Ulid, f64)> = moves.iter().map(|m| (m.task_id, m.position)).collect();
        // No room between `a` and `b`: the column becomes 0, 1, 2.
        assert_eq!(moved, [(a.id, 0.0), (c.id, 1.0), (b.id, 2.0)]);
        assert!(moves.iter().all(|m| m.status == TaskStatus::Todo));
        apply_moves(&mut board, &moves);
        assert_eq!(ids(&board.columns(api, None, &[])[0]), [a.id, c.id, b.id]);
    }

    #[test]
    fn a_running_card_ticks_when_its_run_time_changes() {
        let api = Ulid::new();
        let running = task(api, "running", TaskStatus::NeedsInput, 1.0);
        let mut board = Board::default();
        board.load(vec![running.clone()]);
        let sessions = [session(&running, 0, None)];
        assert!(board.redraw_due(api, &sessions, 1_000, 2_000));
        assert!(
            !board.redraw_due(api, &sessions, 61_000, 62_000),
            "still 1m"
        );
        assert!(!board.redraw_due(Ulid::new(), &sessions, 1_000, 2_000));
    }

    #[test]
    fn cards_offer_the_action_of_their_column() {
        let api = Ulid::new();
        let tasks: Vec<Task> = COLUMNS
            .iter()
            .map(|&status| task(api, "t", status, 1.0))
            .collect();
        let mut board = Board::default();
        board.load(tasks.clone());
        let actions: Vec<Option<CardAction>> = board
            .columns(api, None, &[])
            .iter()
            .map(|column| column.cards[0].action())
            .collect();
        assert_eq!(
            actions,
            [
                Some(CardAction::Run),
                None,
                None,
                Some(CardAction::Approve),
                None
            ]
        );
        let runnable: Vec<bool> = tasks.iter().map(|t| board.can_run(t.id)).collect();
        assert_eq!(runnable, [true, false, false, false, false]);
        assert!(!board.can_run(Ulid::new()));

        // Ctrl+Enter: only a selected Todo card of the shown board.
        let todo = tasks[0].id;
        assert_eq!(board.run_target(Some(todo), api), Some(todo));
        assert_eq!(board.run_target(Some(todo), Ulid::new()), None);
        assert_eq!(board.run_target(Some(tasks[1].id), api), None);
        assert_eq!(board.run_target(None, api), None);
    }

    #[test]
    fn finished_cards_with_a_checkpoint_offer_undo_and_review_ones_their_changes() {
        let api = Ulid::new();
        let mut tasks: Vec<Task> = COLUMNS
            .iter()
            .map(|&status| task(api, "t", status, 1.0))
            .collect();
        for task in &mut tasks {
            task.checkpoint = Some("abc".to_owned());
        }
        for status in [TaskStatus::Review, TaskStatus::Done] {
            tasks.push(task(api, "no checkpoint", status, 2.0));
        }
        let mut board = Board::default();
        board.load(tasks);
        let offered = |offers: fn(&Card) -> bool| -> Vec<Vec<bool>> {
            board
                .columns(api, None, &[])
                .iter()
                .map(|column| column.cards.iter().map(offers).collect())
                .collect()
        };
        assert_eq!(
            offered(Card::can_undo),
            [
                vec![false],
                vec![false],
                vec![false],
                vec![true, false],
                vec![true, false]
            ]
        );
        assert_eq!(
            offered(Card::can_view_changes),
            [
                vec![false],
                vec![false],
                vec![false],
                vec![true, false],
                vec![false, false]
            ]
        );
    }

    #[test]
    fn finished_cards_with_a_worktree_offer_merge_back_and_removal() {
        let api = Ulid::new();
        let mut tasks: Vec<Task> = COLUMNS
            .iter()
            .map(|&status| task(api, "t", status, 1.0))
            .collect();
        for task in &mut tasks {
            task.worktree = true;
            task.worktree_path = Some("/data/wt".to_owned());
            task.branch = Some("mkrew/t-1".to_owned());
        }
        // Worktree on, but not created yet (or removed).
        let mut pending = task(api, "pending", TaskStatus::Review, 2.0);
        pending.worktree = true;
        tasks.push(pending);
        let mut board = Board::default();
        board.load(tasks);
        let offered: Vec<Vec<bool>> = board
            .columns(api, None, &[])
            .iter()
            .map(|column| column.cards.iter().map(Card::manages_worktree).collect())
            .collect();
        assert_eq!(
            offered,
            [
                vec![false],
                vec![false],
                vec![false],
                vec![true, false],
                vec![true]
            ]
        );
    }

    #[test]
    fn run_with_is_offered_on_todo_and_review_cards_without_a_live_session() {
        let api = Ulid::new();
        let tasks: Vec<Task> = COLUMNS
            .iter()
            .map(|&status| task(api, "t", status, 1.0))
            .collect();
        let review = tasks[3].clone();
        let mut board = Board::default();
        board.load(tasks);
        let offered = |sessions: &[Session]| -> Vec<bool> {
            board
                .columns(api, None, sessions)
                .iter()
                .map(|column| column.cards[0].can_run_with())
                .collect()
        };
        assert_eq!(offered(&[]), [true, false, false, true, false]);
        let first = |default_agent| {
            board.columns(api, default_agent, &[])[0].cards[0]
                .agent_profile()
                .map(str::to_owned)
        };
        assert_eq!(first(None), None);
        assert_eq!(first(Some("codex")), Some("codex".to_owned()));
        // A Review agent still at its prompt is live.
        let mut live = session(&review, 1, None);
        live.state = SessionState::Done;
        assert_eq!(offered(&[live.clone()]), [true, false, false, false, false]);
        assert!(!board.can_run_with(review.id, &[live]));
        let mut exited = session(&review, 1, Some(2));
        exited.state = SessionState::Exited;
        assert_eq!(
            offered(&[exited.clone()]),
            [true, false, false, true, false]
        );
        assert!(board.can_run_with(review.id, &[exited]));
        assert!(!board.can_run_with(Ulid::new(), &[]));
    }

    #[test]
    fn a_follow_up_field_lasts_while_its_session_is_the_live_newest_one() {
        let api = Ulid::new();
        let mut running = task(api, "running", TaskStatus::Running, 1.0);
        let mut board = Board::default();
        board.load(vec![running.clone()]);
        let first = session(&running, 0, None);
        let mut sessions = vec![first.clone()];
        assert!(board.takes_follow_up(running.id, first.id, &sessions));
        assert!(!board.takes_follow_up(Ulid::new(), first.id, &sessions));

        // A newer run replaces the session the draft was for.
        let second = session(&running, 10, None);
        sessions.push(second.clone());
        assert!(!board.takes_follow_up(running.id, first.id, &sessions));
        assert!(board.takes_follow_up(running.id, second.id, &sessions));

        sessions[1].state = SessionState::Exited;
        assert!(!board.takes_follow_up(running.id, second.id, &sessions));
        sessions[1].state = SessionState::Working;
        running.status = TaskStatus::Review;
        running.updated_at = 1;
        board.apply(&Event::TaskUpdated(running.clone()));
        assert!(!board.takes_follow_up(running.id, second.id, &sessions));
    }

    #[test]
    fn clicks_open_the_newest_session_and_follow_ups_need_a_live_one() {
        let api = Ulid::new();
        let tasks: Vec<Task> = COLUMNS
            .iter()
            .map(|&status| task(api, "t", status, 1.0))
            .collect();
        let mut sessions: Vec<Session> = tasks.iter().map(|t| session(t, 0, None)).collect();
        let mut board = Board::default();
        board.load(tasks.clone());
        let cards = |sessions: &[Session]| -> Vec<Card> {
            board
                .columns(api, None, sessions)
                .into_iter()
                .map(|mut column| column.cards.remove(0))
                .collect()
        };

        let opens: Vec<bool> = cards(&sessions)
            .iter()
            .map(|card| card.opens_session().is_some())
            .collect();
        // A Todo card only gets selected.
        assert_eq!(opens, [false, true, true, true, true]);
        assert_eq!(cards(&sessions)[3].opens_session(), Some(sessions[3].id));
        let follows: Vec<bool> = cards(&sessions)
            .iter()
            .map(|card| card.follow_up_session().is_some())
            .collect();
        assert_eq!(follows, [false, true, true, false, false]);
        assert_eq!(
            cards(&sessions)[2].follow_up_session(),
            Some(sessions[2].id)
        );

        // An exited session takes no input; a card that never ran has none.
        sessions[1].state = SessionState::Exited;
        assert_eq!(cards(&sessions)[1].follow_up_session(), None);
        assert_eq!(cards(&[])[1].follow_up_session(), None);
        assert_eq!(cards(&[])[1].opens_session(), None);
    }

    #[test]
    fn approving_moves_a_review_card_to_the_end_of_done() {
        let api = Ulid::new();
        let (review, done, todo) = (
            task(api, "review", TaskStatus::Review, 1.0),
            task(api, "done", TaskStatus::Done, 4.0),
            task(api, "todo", TaskStatus::Todo, 0.0),
        );
        let mut board = Board::default();
        board.load(vec![review.clone(), done.clone(), todo.clone()]);

        let moves = board.approve_moves(review.id);
        assert_eq!(
            moves,
            [TaskMoveParams {
                task_id: review.id,
                status: TaskStatus::Done,
                position: 5.0,
            }]
        );
        apply_moves(&mut board, &moves);
        assert_eq!(ids(&board.columns(api, None, &[])[4]), [done.id, review.id]);
        // Only Review cards are approved.
        assert!(board.approve_moves(todo.id).is_empty());
        assert!(board.approve_moves(review.id).is_empty());
        assert!(board.approve_moves(Ulid::new()).is_empty());
    }
}
