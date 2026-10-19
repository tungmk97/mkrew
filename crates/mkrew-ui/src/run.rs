//! The Run button (SPEC §10, ADR 0018): per-command state from the session list, the ports
//! each run session printed, and the button/menu/pane texts. Pure; `main.rs` renders it.

use std::collections::{HashMap, HashSet};

use crate::theme::Theme;
use mkrew_core::run::{MAX_PORTS, command_name};
use mkrew_protocol::{Event, Project, Session, SessionPortsEvent, SessionState};
use ulid::Ulid;

/// What a Run control asks the daemon to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunAction {
    Start,
    Restart,
    Stop,
}

/// The glyph color of a Run control and the faint fill it shows on hover, in the Run pill's
/// menu: Start and Restart gray (color is for status only), Stop red (it ends processes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionColors {
    pub fg: u32,
    pub tint: u32,
}

pub fn action_colors(action: RunAction, theme: &Theme) -> ActionColors {
    let (fg, tint) = match action {
        // `border`: one step above the menu panel's `surface_raised`, like a menu item hover.
        RunAction::Start | RunAction::Restart => (theme.text_muted, theme.border),
        RunAction::Stop => (theme.status_exited, theme.danger_bg),
    };
    ActionColors { fg, tint }
}

/// Ports of live run sessions: loaded per workspace with `session.ports`, then kept live by
/// `session.ports` events; dropped when the session exits.
#[derive(Debug, Default)]
pub struct Ports {
    by_session: HashMap<Ulid, Vec<u16>>,
    requested: HashSet<Ulid>,
}

impl Ports {
    /// The workspaces among `project_ids` not requested yet, marked requested.
    pub fn to_request(&mut self, project_ids: impl IntoIterator<Item = Ulid>) -> Vec<Ulid> {
        project_ids
            .into_iter()
            .filter(|id| self.requested.insert(*id))
            .collect()
    }

    /// `project_id`'s `session.ports` failed: asked again next time.
    pub fn request_failed(&mut self, project_id: Ulid) {
        self.requested.remove(&project_id);
    }

    /// Keeps a `session.ports` event, drops an exited session; whether anything changed.
    pub fn apply(&mut self, event: &Event) -> bool {
        match event {
            Event::SessionPorts(ports) => self.keep(ports.clone()),
            Event::SessionExited(exited) => self.by_session.remove(&exited.session_id).is_some(),
            _ => false,
        }
    }

    /// Keeps one workspace's `session.ports` answer; whether anything changed.
    pub fn load(&mut self, listed: Vec<SessionPortsEvent>) -> bool {
        let mut changed = false;
        for ports in listed {
            changed |= self.keep(ports);
        }
        changed
    }

    /// A session's list only grows, so the longer one wins (an answer read before an event
    /// handled first does not undo it).
    fn keep(&mut self, ports: SessionPortsEvent) -> bool {
        match self.by_session.get(&ports.session_id) {
            Some(known) if known.len() >= ports.ports.len() => false,
            _ => {
                self.by_session.insert(ports.session_id, ports.ports);
                true
            }
        }
    }

    pub fn get(&self, session_id: Ulid) -> &[u16] {
        self.by_session.get(&session_id).map_or(&[], Vec::as_slice)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowState {
    /// Never started (or its sessions are gone).
    Stopped,
    Running(SessionState),
    Exited(Option<i32>),
}

/// One configured command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub state: RowState,
    /// Its live session, else its newest one.
    pub session_id: Option<Ulid>,
    pub ports: Vec<u16>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    /// The configured commands in order.
    pub rows: Vec<Row>,
    /// Live run sessions whose command left the config.
    pub stray: usize,
}

impl Summary {
    /// Configured commands running now.
    pub fn live(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| matches!(row.state, RowState::Running(_)))
            .count()
    }

    /// Anything to restart or stop.
    pub fn any_live(&self) -> bool {
        self.live() > 0 || self.stray > 0
    }

    /// Something runs but not every configured command.
    pub fn warning(&self) -> bool {
        self.any_live() && self.live() < self.rows.len()
    }

    /// Distinct ports of the running commands, in config order.
    pub fn ports(&self) -> Vec<u16> {
        let mut ports = Vec::new();
        for port in self.rows.iter().flat_map(|row| &row.ports) {
            if !ports.contains(port) {
                ports.push(*port);
            }
        }
        ports
    }

    /// The pill's words: `Run`, else `2 running`; `1/2 running` when one is down (the dot
    /// turns amber, the icon + text rule of SPEC §11).
    pub fn label(&self) -> String {
        if !self.any_live() {
            "Run".to_owned()
        } else if self.warning() {
            format!("{}/{} running", self.live(), self.rows.len())
        } else {
            format!("{} running", self.live() + self.stray)
        }
    }

    /// The pill's ports in config order, `:8080 :5180` (`+N` past [`MAX_PORTS`]); `None`
    /// without any.
    pub fn ports_text(&self) -> Option<String> {
        let ports = self.ports();
        if ports.is_empty() {
            return None;
        }
        let mut text = (ports.iter().take(MAX_PORTS))
            .map(|p| format!(":{p}"))
            .collect::<Vec<_>>()
            .join(" ");
        if ports.len() > MAX_PORTS {
            text.push_str(&format!(" +{}", ports.len() - MAX_PORTS));
        }
        Some(text)
    }

    /// One line per command: `BE — :8080`, `FE — no port yet`, `API — stopped`.
    pub fn tooltip(&self) -> String {
        self.rows
            .iter()
            .map(|row| {
                let what = match &row.state {
                    RowState::Running(_) if row.ports.is_empty() => "no port yet".to_owned(),
                    RowState::Running(_) => row
                        .ports
                        .iter()
                        .map(|p| format!(":{p}"))
                        .collect::<Vec<_>>()
                        .join(" "),
                    other => state_text(other)
                        .trim_start_matches(['○', '✕', ' '])
                        .to_owned(),
                };
                format!("{} — {what}", row.name)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The state of `project`'s run commands.
pub fn summary(project: &Project, sessions: &[Session], ports: &Ports) -> Summary {
    let runs: Vec<(&str, &Session)> = sessions
        .iter()
        .filter(|s| s.project_id == project.id)
        .filter_map(|s| Some((command_name(s)?, s)))
        .collect();
    let rows = project
        .run_commands
        .iter()
        .map(|command| {
            let mine = || {
                runs.iter()
                    .filter(|(name, _)| *name == command.name)
                    .map(|(_, s)| *s)
            };
            let live = mine().find(|s| s.state != SessionState::Exited);
            let newest = mine().max_by_key(|s| (s.started_at, s.id));
            let name = command.name.clone();
            match (live, newest) {
                (Some(s), _) => Row {
                    name,
                    state: RowState::Running(s.state),
                    session_id: Some(s.id),
                    ports: ports.get(s.id).to_vec(),
                },
                (None, Some(s)) => Row {
                    name,
                    state: RowState::Exited(s.exit_code),
                    session_id: Some(s.id),
                    ports: Vec::new(),
                },
                (None, None) => Row {
                    name,
                    state: RowState::Stopped,
                    session_id: None,
                    ports: Vec::new(),
                },
            }
        })
        .collect();
    let stray = runs
        .iter()
        .filter(|(name, s)| {
            s.state != SessionState::Exited && !project.run_commands.iter().any(|c| c.name == *name)
        })
        .count();
    Summary { rows, stray }
}

/// Icon + text of a command's state (SPEC §11: never color alone).
pub fn state_text(state: &RowState) -> String {
    match state {
        RowState::Stopped => "○ stopped".to_owned(),
        RowState::Running(_) => "● running".to_owned(),
        RowState::Exited(Some(code)) => format!("✕ exited {code}"),
        RowState::Exited(None) => "✕ exited".to_owned(),
    }
}

/// The pane header title: a live run session shows `name · :port …`, an exited one its name,
/// any other session its own title.
pub fn pane_title(session: &Session, ports: &[u16]) -> Option<String> {
    let Some(name) = command_name(session) else {
        return crate::history::title(session).map(str::to_owned);
    };
    if session.state == SessionState::Exited || ports.is_empty() {
        return Some(name.to_owned());
    }
    let ports: Vec<String> = ports.iter().map(|p| format!(":{p}")).collect();
    Some(format!("{name} · {}", ports.join(" ")))
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{Project, RunCommand, SessionExitedEvent, SessionKind, StateSource};

    use super::*;

    fn project(names: &[&str]) -> Project {
        Project {
            id: Ulid::new(),
            jira_keys: Vec::new(),
            name: "EA".into(),
            path: "/ea".into(),
            run_commands: names
                .iter()
                .map(|n| RunCommand {
                    name: (*n).into(),
                    dir: ".".into(),
                    command: "x".into(),
                    env_file: None,
                    env: Vec::new(),
                })
                .collect(),
            run_autostart: false,
            default_agent: None,
            default_shell: None,
            review_globs: None,
            created_at: 0,
            archived: false,
            base_ref: None,
            color: None,
            pinned: false,
        }
    }

    fn run(project: &Project, name: &str, state: SessionState, started_at: i64) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id: project.id,
            agent_profile: "shell".into(),
            kind: SessionKind::Shell,
            title: Some(format!("run:{name}")),
            terminal_title: None,
            color: None,
            native_session_id: None,
            state,
            state_source: StateSource::Screen,
            state_changed_at: 0,
            exit_code: (state == SessionState::Exited).then_some(1),
            cols: 80,
            rows: 24,
            started_at,
            ended_at: None,
            pinned: false,
            position: 0.0,
            plan_path: None,
        }
    }

    fn ports_of(entries: &[(Ulid, &[u16])]) -> Ports {
        let mut ports = Ports::default();
        ports.load(
            entries
                .iter()
                .map(|(id, p)| SessionPortsEvent {
                    session_id: *id,
                    ports: p.to_vec(),
                })
                .collect(),
        );
        ports
    }

    #[test]
    fn nothing_running_offers_run() {
        let p = project(&["BE", "FE"]);
        let s = summary(&p, &[], &Ports::default());
        assert_eq!(s.label(), "Run");
        assert!(!s.any_live() && !s.warning());
        assert_eq!(state_text(&s.rows[0].state), "○ stopped");
        assert_eq!(
            summary(&project(&[]), &[], &Ports::default()).label(),
            "Run"
        );
    }

    #[test]
    fn running_commands_show_count_and_ports_in_config_order() {
        let p = project(&["BE", "FE"]);
        let fe = run(&p, "FE", SessionState::Working, 2);
        let be = run(&p, "BE", SessionState::Idle, 1);
        let old_be = run(&p, "BE", SessionState::Exited, 0);
        let ports = ports_of(&[(fe.id, &[5180]), (be.id, &[8080, 5180])]);
        let s = summary(&p, &[fe, old_be, be.clone()], &ports);
        assert_eq!(s.label(), "2 running");
        assert_eq!(s.ports_text().as_deref(), Some(":8080 :5180"));
        assert!(!s.warning());
        assert_eq!(s.rows[0].session_id, Some(be.id));
        assert_eq!(s.tooltip(), "BE — :8080 :5180\nFE — :5180");
    }

    #[test]
    fn an_exited_command_while_others_run_is_a_warning() {
        let p = project(&["BE", "FE"]);
        let fe = run(&p, "FE", SessionState::Working, 2);
        let be = run(&p, "BE", SessionState::Exited, 1);
        let s = summary(&p, &[fe, be], &Ports::default());
        assert_eq!(s.label(), "1/2 running");
        assert_eq!(s.ports_text(), None);
        assert!(s.warning());
        assert_eq!(state_text(&s.rows[0].state), "✕ exited 1");
        assert_eq!(s.tooltip(), "BE — exited 1\nFE — no port yet");
    }

    #[test]
    fn more_than_four_ports_are_counted() {
        let p = project(&["A"]);
        let a = run(&p, "A", SessionState::Working, 0);
        let ports = ports_of(&[(a.id, &[1, 2, 3, 4])]);
        let mut s = summary(&p, &[a], &ports);
        s.rows[0].ports.push(5);
        assert_eq!(s.ports_text().as_deref(), Some(":1 :2 :3 :4 +1"));
    }

    #[test]
    fn a_live_run_whose_command_was_removed_still_counts_as_running() {
        let p = project(&["BE"]);
        let gone = run(&p, "old", SessionState::Working, 0);
        let s = summary(&p, &[gone], &Ports::default());
        assert_eq!(s.stray, 1);
        assert!(s.any_live() && s.warning());
        assert_eq!(s.label(), "0/1 running");
    }

    #[test]
    fn ports_follow_events_and_drop_on_exit() {
        let id = Ulid::new();
        let mut ports = Ports::default();
        let event = Event::SessionPorts(SessionPortsEvent {
            session_id: id,
            ports: vec![8080],
        });
        assert!(ports.apply(&event));
        assert!(!ports.apply(&event));
        assert_eq!(ports.get(id), [8080]);
        let exited = Event::SessionExited(SessionExitedEvent {
            session_id: id,
            exit_code: None,
        });
        assert!(ports.apply(&exited));
        assert!(ports.get(id).is_empty());
        let project_id = Ulid::new();
        assert_eq!(ports.to_request([project_id]), [project_id]);
        assert!(ports.to_request([project_id]).is_empty());
        ports.request_failed(project_id);
        assert_eq!(ports.to_request([project_id]), [project_id]);
    }

    #[test]
    fn the_pane_title_of_a_run_session_is_its_name_and_ports() {
        let p = project(&["BE"]);
        let be = run(&p, "BE", SessionState::Working, 0);
        assert_eq!(pane_title(&be, &[8080]).as_deref(), Some("BE · :8080"));
        assert_eq!(pane_title(&be, &[]).as_deref(), Some("BE"));
        let exited = run(&p, "BE", SessionState::Exited, 0);
        assert_eq!(pane_title(&exited, &[8080]).as_deref(), Some("BE"));
        let shell = Session {
            title: Some("notes".into()),
            ..be
        };
        assert_eq!(pane_title(&shell, &[8080]).as_deref(), Some("notes"));
    }

    /// The glyph stays visible on every surface it sits on (title bar, menu panel, its own
    /// hover fill).
    #[test]
    fn run_action_colors_are_visible_in_both_themes() {
        use crate::theme::contrast;
        for theme in [Theme::dark(), Theme::light()] {
            for action in [RunAction::Start, RunAction::Restart, RunAction::Stop] {
                let colors = action_colors(action, &theme);
                for bg in [theme.bg, theme.hover, theme.surface_raised, colors.tint] {
                    let ratio = contrast(colors.fg, bg);
                    assert!(
                        ratio >= 3.0,
                        "{action:?} {:06X} on {bg:06X}: {ratio:.2}",
                        colors.fg
                    );
                }
            }
            // A hovered tint differs from the menu panel it sits on.
            for action in [RunAction::Start, RunAction::Restart, RunAction::Stop] {
                assert_ne!(action_colors(action, &theme).tint, theme.surface_raised);
            }
        }
    }
}
