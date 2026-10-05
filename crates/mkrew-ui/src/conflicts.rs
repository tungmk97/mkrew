//! Conflict warnings (SPEC §11, M5): `conflict.detected` says two sessions of a workspace edited
//! the same file within 60 s. The board cards of their tasks and their pane headers show a
//! warning until the tasks leave Running / Needs input (sessions: until they exit) or the user
//! dismisses it. Pure and tested headlessly; `main.rs` renders it.

use mkrew_protocol::{ConflictDetectedEvent, Event, Session, SessionState, TaskStatus};
use ulid::Ulid;

use crate::board::Board;

/// Warning icon (text presentation, so it is not drawn as a color emoji); always shown next to
/// text (SPEC §11: never color alone).
pub const ICON: &str = "⚠\u{FE0E}";

/// One file edited by several sessions, and who still shows the warning.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Conflict {
    project_id: Ulid,
    path: String,
    session_ids: Vec<Ulid>,
    task_ids: Vec<Ulid>,
}

#[derive(Debug, Default)]
pub struct Conflicts {
    list: Vec<Conflict>,
}

impl Conflicts {
    /// Records a `conflict.detected` (a later one for the same file adds its editors); returns
    /// whether the warnings changed. Call [`Conflicts::prune`] afterwards.
    pub fn apply(&mut self, event: &Event) -> bool {
        let Event::ConflictDetected(ConflictDetectedEvent {
            project_id,
            path,
            session_ids,
            task_ids,
        }) = event
        else {
            return false;
        };
        match self
            .list
            .iter_mut()
            .find(|c| c.project_id == *project_id && c.path == *path)
        {
            Some(conflict) => {
                add_all(&mut conflict.session_ids, session_ids);
                add_all(&mut conflict.task_ids, task_ids);
            }
            None => self.list.push(Conflict {
                project_id: *project_id,
                path: path.clone(),
                session_ids: session_ids.clone(),
                task_ids: task_ids.clone(),
            }),
        }
        true
    }

    /// Stops warning on tasks that left Running / Needs input (or were deleted) and on
    /// sessions that exited or whose task stopped warning; drops conflicts nobody shows any
    /// more. Returns whether the warnings changed.
    pub fn prune(&mut self, board: &Board, sessions: &[Session]) -> bool {
        // Before the first `task.list` nothing is known about the tasks: keep them.
        let task_active = |id: Ulid| match board.task(id) {
            Some(task) => matches!(task.status, TaskStatus::Running | TaskStatus::NeedsInput),
            None => !board.is_loaded(),
        };
        let mut changed = false;
        for conflict in &mut self.list {
            let (tasks, sessions_before) = (conflict.task_ids.len(), conflict.session_ids.len());
            conflict.task_ids.retain(|&id| task_active(id));
            // A session the sidebar does not list yet keeps its warning.
            conflict.session_ids.retain(|&id| {
                sessions.iter().find(|s| s.id == id).is_none_or(|session| {
                    session.state != SessionState::Exited && session.task_id.is_none_or(task_active)
                })
            });
            changed |=
                conflict.task_ids.len() != tasks || conflict.session_ids.len() != sessions_before;
        }
        let before = self.list.len();
        self.list
            .retain(|c| !c.task_ids.is_empty() || !c.session_ids.is_empty());
        changed || self.list.len() != before
    }

    /// The files `task_id`'s card warns about, in detection order.
    pub fn for_task(&self, task_id: Ulid) -> Vec<&str> {
        self.paths(|c| c.task_ids.contains(&task_id))
    }

    /// The files `session_id`'s pane header warns about, in detection order.
    pub fn for_session(&self, session_id: Ulid) -> Vec<&str> {
        self.paths(|c| c.session_ids.contains(&session_id))
    }

    /// Dismisses every warning `task_id`'s card shows, on every card and pane showing them.
    pub fn dismiss_task(&mut self, task_id: Ulid) {
        self.list.retain(|c| !c.task_ids.contains(&task_id));
    }

    /// Dismisses every warning `session_id`'s pane shows, on every card and pane showing them.
    pub fn dismiss_session(&mut self, session_id: Ulid) {
        self.list.retain(|c| !c.session_ids.contains(&session_id));
    }

    fn paths(&self, shows: impl Fn(&Conflict) -> bool) -> Vec<&str> {
        self.list
            .iter()
            .filter(|c| shows(c))
            .map(|c| c.path.as_str())
            .collect()
    }
}

fn add_all(ids: &mut Vec<Ulid>, more: &[Ulid]) {
    for id in more {
        if !ids.contains(id) {
            ids.push(*id);
        }
    }
}

/// The badge text for `paths`: the first file, plus how many more (empty for no paths).
pub fn label(paths: &[&str]) -> String {
    match paths {
        [] => String::new(),
        [path] => (*path).to_owned(),
        [path, rest @ ..] => format!("{path} +{}", rest.len()),
    }
}

/// The badge tooltip: what happened and every file.
pub fn hover(paths: &[&str]) -> String {
    format!(
        "Another session edited {} within {} s. Check the changes before approving.",
        paths.join(", "),
        mkrew_core::conflict::WINDOW_MS / 1000
    )
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{SessionKind, StateSource, Task};

    use super::*;

    fn task(project_id: Ulid, status: TaskStatus) -> Task {
        Task {
            id: Ulid::new(),
            project_id,
            title: "t".to_owned(),
            prompt: String::new(),
            agent_profile: None,
            status,
            position: 1.0,
            checkpoint: None,
            error_flag: false,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn session(task: Option<&Task>, project_id: Ulid) -> Session {
        Session {
            id: Ulid::new(),
            task_id: task.map(|t| t.id),
            project_id,
            agent_profile: "claude".to_owned(),
            kind: SessionKind::Agent,
            title: None,
            native_session_id: None,
            state: SessionState::Working,
            state_source: StateSource::Daemon,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
        }
    }

    fn detected(project_id: Ulid, path: &str, sessions: &[&Session]) -> Event {
        let mut task_ids: Vec<Ulid> = sessions.iter().filter_map(|s| s.task_id).collect();
        task_ids.sort();
        Event::ConflictDetected(ConflictDetectedEvent {
            project_id,
            path: path.to_owned(),
            session_ids: sessions.iter().map(|s| s.id).collect(),
            task_ids,
        })
    }

    fn updated(board: &mut Board, task: &Task, status: TaskStatus) {
        let mut task = task.clone();
        task.status = status;
        task.updated_at += 1;
        board.apply(&Event::TaskUpdated(task));
    }

    #[test]
    fn both_cards_and_panes_warn_until_their_task_leaves_running() {
        let project = Ulid::new();
        let (a, b) = (
            task(project, TaskStatus::Running),
            task(project, TaskStatus::NeedsInput),
        );
        let mut board = Board::default();
        board.load(vec![a.clone(), b.clone()]);
        let (sa, sb) = (session(Some(&a), project), session(Some(&b), project));
        let sessions = vec![sa.clone(), sb.clone()];

        let mut conflicts = Conflicts::default();
        assert!(!conflicts.apply(&Event::TaskUpdated(a.clone())));
        assert!(conflicts.apply(&detected(project, "src/a.rs", &[&sa, &sb])));
        assert!(!conflicts.prune(&board, &sessions));
        for id in [a.id, b.id] {
            assert_eq!(conflicts.for_task(id), ["src/a.rs"]);
        }
        for id in [sa.id, sb.id] {
            assert_eq!(conflicts.for_session(id), ["src/a.rs"]);
        }

        // A goes to Review: its card and pane stop warning, B's keep warning.
        updated(&mut board, &a, TaskStatus::Review);
        assert!(conflicts.prune(&board, &sessions));
        assert!(conflicts.for_task(a.id).is_empty() && conflicts.for_session(sa.id).is_empty());
        assert_eq!(conflicts.for_task(b.id), ["src/a.rs"]);
        assert_eq!(conflicts.for_session(sb.id), ["src/a.rs"]);

        // Running A again does not bring the old warning back.
        updated(&mut board, &a, TaskStatus::Running);
        assert!(!conflicts.prune(&board, &sessions));
        assert!(conflicts.for_task(a.id).is_empty());

        // B is deleted: nobody shows the conflict any more.
        board.apply(&Event::TaskDeleted(mkrew_protocol::TaskDeletedEvent {
            task_id: b.id,
            project_id: project,
        }));
        assert!(conflicts.prune(&board, &sessions));
        assert!(conflicts.list.is_empty());
    }

    #[test]
    fn sessions_without_a_task_warn_until_they_exit() {
        let project = Ulid::new();
        let mut board = Board::default();
        board.load(Vec::new());
        let (sa, mut sb) = (session(None, project), session(None, project));
        let mut conflicts = Conflicts::default();
        conflicts.apply(&detected(project, "a.rs", &[&sa, &sb]));
        // Not listed by the sidebar yet: kept.
        assert!(!conflicts.prune(&board, &[]));
        assert_eq!(conflicts.for_session(sa.id), ["a.rs"]);

        sb.state = SessionState::Exited;
        assert!(conflicts.prune(&board, &[sa.clone(), sb.clone()]));
        assert_eq!(conflicts.for_session(sa.id), ["a.rs"]);
        assert!(conflicts.for_session(sb.id).is_empty());
    }

    #[test]
    fn tasks_are_kept_until_the_board_is_loaded() {
        let project = Ulid::new();
        let a = task(project, TaskStatus::Running);
        let sa = session(Some(&a), project);
        let mut conflicts = Conflicts::default();
        conflicts.apply(&detected(project, "a.rs", &[&sa]));
        let mut board = Board::default();
        assert!(!conflicts.prune(&board, &[]));
        assert_eq!(conflicts.for_task(a.id), ["a.rs"]);
        board.load(Vec::new());
        assert!(conflicts.prune(&board, &[]));
        assert!(conflicts.for_task(a.id).is_empty());
    }

    #[test]
    fn a_later_detection_of_the_same_file_adds_its_editors() {
        let project = Ulid::new();
        let (sa, sb, sc) = (
            session(None, project),
            session(None, project),
            session(None, project),
        );
        let mut conflicts = Conflicts::default();
        conflicts.apply(&detected(project, "a.rs", &[&sa, &sb]));
        conflicts.apply(&detected(project, "a.rs", &[&sb, &sc]));
        conflicts.apply(&detected(project, "b.rs", &[&sa, &sc]));
        // Same path in another workspace is another conflict.
        conflicts.apply(&detected(Ulid::new(), "a.rs", &[&sa]));
        assert_eq!(conflicts.list.len(), 3);
        assert_eq!(conflicts.list[0].session_ids, [sa.id, sb.id, sc.id]);
        assert_eq!(conflicts.for_session(sc.id), ["a.rs", "b.rs"]);
    }

    #[test]
    fn dismissing_clears_the_warning_everywhere() {
        let project = Ulid::new();
        let (a, b) = (
            task(project, TaskStatus::Running),
            task(project, TaskStatus::Running),
        );
        let (sa, sb) = (session(Some(&a), project), session(Some(&b), project));
        let mut conflicts = Conflicts::default();
        conflicts.apply(&detected(project, "a.rs", &[&sa, &sb]));
        conflicts.apply(&detected(project, "b.rs", &[&sb]));
        conflicts.dismiss_task(a.id);
        assert!(conflicts.for_task(a.id).is_empty() && conflicts.for_session(sa.id).is_empty());
        assert_eq!(conflicts.for_task(b.id), ["b.rs"]);
        conflicts.dismiss_session(sb.id);
        assert!(conflicts.list.is_empty());
    }

    #[test]
    fn badge_names_the_first_file_and_the_tooltip_all_of_them() {
        assert_eq!(label(&["src/a.rs"]), "src/a.rs");
        assert_eq!(label(&["src/a.rs", "b.rs", "c.rs"]), "src/a.rs +2");
        assert_eq!(
            hover(&["src/a.rs", "b.rs"]),
            "Another session edited src/a.rs, b.rs within 60 s. Check the changes before approving."
        );
    }
}
