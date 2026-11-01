//! The Ports dialog's model (spec 2026-11-01): `port.list` rows grouped by workspace, the
//! selection and the Stop question. Pure; `shell/ports.rs` renders it.

use gpui::Keystroke;
use mkrew_protocol::{ErrorBody, ErrorCode, PortRow, Project};
use ulid::Ulid;

/// Heading of the ports no shown workspace owns.
pub const OUTSIDE: &str = "OUTSIDE MKREW";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub title: String,
    pub rows: Vec<PortRow>,
}

/// One group per workspace with ports, in `projects` (sidebar) order, main folder before
/// worktrees, then by port; the rest (no workspace, or one not shown) last under [`OUTSIDE`].
pub fn groups(rows: &[PortRow], projects: &[Project]) -> Vec<Group> {
    let mut groups: Vec<Group> = projects
        .iter()
        .filter_map(|project| {
            let mut mine: Vec<PortRow> = rows
                .iter()
                .filter(|r| r.project_id == Some(project.id))
                .cloned()
                .collect();
            mine.sort_by(|a, b| {
                (a.branch.is_some(), &a.branch, a.port, a.pid).cmp(&(
                    b.branch.is_some(),
                    &b.branch,
                    b.port,
                    b.pid,
                ))
            });
            (!mine.is_empty()).then(|| Group {
                title: project.name.to_uppercase(),
                rows: mine,
            })
        })
        .collect();
    let shown = |id: Option<Ulid>| id.is_some_and(|id| projects.iter().any(|p| p.id == id));
    let mut outside: Vec<PortRow> = rows
        .iter()
        .filter(|r| !shown(r.project_id))
        .cloned()
        .collect();
    if !outside.is_empty() {
        outside.sort_by_key(|r| (r.port, r.pid));
        groups.push(Group {
            title: OUTSIDE.to_owned(),
            rows: outside,
        });
    }
    groups
}

/// `⎇ <branch>` in a task worktree, `main` in the workspace folder, empty outside.
pub fn place(row: &PortRow) -> String {
    match (&row.branch, row.project_id) {
        (Some(branch), _) => format!("⎇ {branch}"),
        (None, Some(_)) => "main".to_owned(),
        (None, None) => String::new(),
    }
}

/// The run command, else the session title, else the process name.
pub fn source(row: &PortRow) -> String {
    row.run_command
        .clone()
        .or_else(|| row.session_title.clone())
        .unwrap_or_else(|| row.process.clone())
}

/// Every Stop asks first except a run command's (it stops like ■ in the Run menu).
pub fn asks_first(row: &PortRow) -> bool {
    row.run_command.is_none()
}

pub fn question(row: &PortRow) -> String {
    let ask = format!("Stop {} (pid {}) on :{}?", row.process, row.pid, row.port);
    if row.session_id.is_some() {
        format!("{ask} The pane stays open.")
    } else {
        ask
    }
}

/// `not_found` means the port went away meanwhile: the next list shows it, no error.
pub fn stop_failure(pid: u32, body: &ErrorBody) -> Option<String> {
    (body.code != ErrorCode::NotFound).then(|| format!("Can't stop pid {pid}: {}", body.message))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Close,
    Move(isize),
    Open,
    Stop,
}

pub fn key(keystroke: &Keystroke) -> Option<Key> {
    let modified = keystroke.modifiers.modified();
    match keystroke.key.as_str() {
        "escape" => Some(Key::Close),
        "up" if !modified => Some(Key::Move(-1)),
        "down" if !modified => Some(Key::Move(1)),
        "enter" if !modified => Some(Key::Open),
        "delete" if !modified => Some(Key::Stop),
        _ => None,
    }
}

/// The open dialog. A row is known by `(port, pid)` across refreshes.
#[derive(Debug, Default)]
pub struct PortsDialog {
    pub rows: Vec<PortRow>,
    /// A first answer came (until then the dialog says "Loading…").
    pub loaded: bool,
    pub error: Option<String>,
    pub selected: Option<(u16, u32)>,
    /// The row whose Stop question shows in the footer.
    pub confirm: Option<PortRow>,
    polling: bool,
}

impl PortsDialog {
    /// `false` while a `port.list` is in flight: that tick skips.
    pub fn begin_poll(&mut self) -> bool {
        !std::mem::replace(&mut self.polling, true)
    }

    pub fn listed(&mut self, listed: Result<Vec<PortRow>, String>) {
        self.polling = false;
        self.loaded = true;
        match listed {
            Ok(rows) => {
                self.rows = rows;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        let alive = |rows: &[PortRow], port: u16, pid: u32| {
            rows.iter().any(|r| r.port == port && r.pid == pid)
        };
        if self
            .selected
            .is_some_and(|(port, pid)| !alive(&self.rows, port, pid))
        {
            self.selected = None;
        }
        if self
            .confirm
            .as_ref()
            .is_some_and(|r| !alive(&self.rows, r.port, r.pid))
        {
            self.confirm = None;
        }
    }

    /// Moves the selection through the rows in shown order, without wrapping. With no
    /// selection, down picks the first row and up the last.
    pub fn select_by(&mut self, offset: isize, projects: &[Project]) {
        let order: Vec<(u16, u32)> = groups(&self.rows, projects)
            .into_iter()
            .flat_map(|g| g.rows)
            .map(|r| (r.port, r.pid))
            .collect();
        let Some(last) = order.len().checked_sub(1) else {
            return;
        };
        let at = match self
            .selected
            .and_then(|id| order.iter().position(|o| *o == id))
        {
            Some(at) => at.saturating_add_signed(offset).min(last),
            None if offset < 0 => last,
            None => 0,
        };
        self.selected = Some(order[at]);
    }

    pub fn selected_row(&self) -> Option<&PortRow> {
        let (port, pid) = self.selected?;
        self.rows.iter().find(|r| r.port == port && r.pid == pid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn project(id: u128, name: &str) -> Project {
        serde_json::from_value(json!({
            "id": Ulid(id),
            "name": name,
            "path": "C:/work",
            "default_agent": null,
            "review_globs": null,
            "created_at": 0,
            "archived": false,
        }))
        .unwrap()
    }

    fn row(port: u16, pid: u32, project_id: Option<u128>) -> PortRow {
        PortRow {
            port,
            addr: "0.0.0.0".into(),
            pid,
            process: "node.exe".into(),
            project_id: project_id.map(Ulid),
            worktree_task_id: None,
            branch: None,
            session_id: None,
            session_title: None,
            run_command: None,
        }
    }

    #[test]
    fn groups_follow_the_sidebar_and_end_outside() {
        let projects = [project(2, "api"), project(1, "app")];
        let mut worktree = row(5000, 3, Some(1));
        worktree.branch = Some("fix".into());
        let rows = [
            row(9000, 1, None),
            row(4000, 2, Some(1)),
            worktree,
            row(8080, 4, Some(2)),
            row(7000, 5, Some(99)),
        ];
        let shown: Vec<(String, Vec<u16>)> = groups(&rows, &projects)
            .into_iter()
            .map(|g| (g.title, g.rows.iter().map(|r| r.port).collect()))
            .collect();
        assert_eq!(
            shown,
            [
                ("API".to_owned(), vec![8080]),
                ("APP".to_owned(), vec![4000, 5000]),
                (OUTSIDE.to_owned(), vec![7000, 9000]),
            ]
        );
    }

    #[test]
    fn labels_say_where_and_what() {
        let mut run = row(5173, 7, Some(1));
        run.branch = Some("fix".into());
        run.session_id = Some(Ulid(3));
        run.session_title = Some("run:web".into());
        run.run_command = Some("web".into());
        assert_eq!(
            (place(&run), source(&run)),
            ("⎇ fix".to_owned(), "web".to_owned())
        );
        let main = row(3000, 8, Some(1));
        assert_eq!(
            (place(&main), source(&main)),
            ("main".to_owned(), "node.exe".to_owned())
        );
        assert_eq!(place(&row(1, 1, None)), "");
    }

    #[test]
    fn only_a_run_command_stops_without_asking() {
        let mut run = row(5173, 7, Some(1));
        run.session_id = Some(Ulid(3));
        run.run_command = Some("web".into());
        assert!(!asks_first(&run));
        let mut pane = row(3000, 8, Some(1));
        pane.session_id = Some(Ulid(4));
        assert!(asks_first(&pane));
        assert_eq!(
            question(&pane),
            "Stop node.exe (pid 8) on :3000? The pane stays open."
        );
        assert_eq!(
            question(&row(5432, 9, None)),
            "Stop node.exe (pid 9) on :5432?"
        );
    }

    #[test]
    fn a_vanished_port_is_no_error() {
        let gone = ErrorBody {
            code: ErrorCode::NotFound,
            message: "gone".into(),
        };
        assert_eq!(stop_failure(4, &gone), None);
        let refused = ErrorBody {
            code: ErrorCode::InvalidState,
            message: "access denied".into(),
        };
        assert_eq!(
            stop_failure(4, &refused).as_deref(),
            Some("Can't stop pid 4: access denied")
        );
    }

    #[test]
    fn selection_moves_in_shown_order_and_survives_a_refresh() {
        let projects = [project(1, "app")];
        let mut dialog = PortsDialog::default();
        assert!(dialog.begin_poll());
        assert!(!dialog.begin_poll(), "one poll at a time");
        dialog.listed(Ok(vec![row(9000, 1, None), row(3000, 2, Some(1))]));
        dialog.select_by(1, &projects);
        assert_eq!(dialog.selected, Some((3000, 2)), "first shown row");
        dialog.select_by(1, &projects);
        assert_eq!(dialog.selected, Some((9000, 1)));
        dialog.select_by(1, &projects);
        assert_eq!(dialog.selected, Some((9000, 1)), "stays on the last");
        dialog.confirm = Some(row(9000, 1, None));
        assert!(dialog.begin_poll());
        dialog.listed(Ok(vec![row(3000, 2, Some(1))]));
        assert_eq!(dialog.selected, None, "its port went away");
        assert_eq!(dialog.confirm, None);
        dialog.listed(Err("daemon busy".into()));
        assert_eq!(dialog.rows.len(), 1, "rows stay on an error");
        assert_eq!(dialog.error.as_deref(), Some("daemon busy"));
    }
}
