//! The workspace's dev server (SPEC §10, ADR 0008): which session is it, how its tab is
//! labelled, and the "Dev server" settings dialog (command + auto-start) that sends
//! `project.update`. Pure and tested headlessly; `main.rs` renders it.

use gpui::Keystroke;
use mkrew_protocol::{DevServer, Project, ProjectUpdateParams, Session, SessionKind, SessionState};
use ulid::Ulid;

use crate::task_dialog::{DialogKey, dialog_key};

/// Label of a tab holding the running dev server.
pub const TAB_LABEL: &str = "Dev server";

/// The running dev server of `project_id`: its live shell session titled "dev" (the same rule
/// as the daemon's).
pub fn running(sessions: &[Session], project_id: Ulid) -> Option<&Session> {
    sessions.iter().find(|s| {
        s.project_id == project_id
            && s.kind == SessionKind::Shell
            && s.state != SessionState::Exited
            && s.title.as_deref() == Some(DevServer::TITLE)
    })
}

/// What the tab bar shows for a tab titled `title`: [`TAB_LABEL`] while it holds the running
/// dev server, else its own title.
pub fn tab_label(title: &str, holds_dev_server: bool) -> &str {
    if holds_dev_server { TAB_LABEL } else { title }
}

/// What a key does in the dev server dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Close,
    /// Enter: save both fields.
    Save,
    /// Tab: flips "Start with the daemon".
    ToggleAutostart,
    Backspace,
    Paste,
    Insert(String),
}

pub fn key(keystroke: &Keystroke) -> Option<Key> {
    match dialog_key(keystroke)? {
        DialogKey::Cancel => Some(Key::Close),
        DialogKey::Enter | DialogKey::Save => Some(Key::Save),
        DialogKey::SwitchField => Some(Key::ToggleAutostart),
        DialogKey::Backspace => Some(Key::Backspace),
        DialogKey::Paste => Some(Key::Paste),
        DialogKey::Insert(text) => Some(Key::Insert(text)),
    }
}

/// The open "Dev server" settings of one workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevDialog {
    pub project_id: Ulid,
    /// The command being typed; starts as the configured one.
    pub command: String,
    pub autostart: bool,
    pub busy: bool,
    pub error: Option<String>,
}

impl DevDialog {
    pub fn new(project: &Project) -> DevDialog {
        DevDialog {
            project_id: project.id,
            command: project.dev_command.clone().unwrap_or_default(),
            autostart: project.dev_autostart,
            busy: false,
            error: None,
        }
    }

    /// Edits the command or flips auto-start; `true` when something changed.
    pub fn key(&mut self, key: &Key) -> bool {
        match key {
            Key::Backspace => self.command.pop().is_some(),
            Key::Insert(text) => self.insert(text),
            Key::ToggleAutostart => {
                self.autostart = !self.autostart;
                true
            }
            Key::Close | Key::Save | Key::Paste => false,
        }
    }

    /// Appends typed or pasted text (the command is one line). `true` when it changed.
    pub fn insert(&mut self, text: &str) -> bool {
        let before = self.command.len();
        self.command
            .extend(text.chars().filter(|c| *c != '\r' && *c != '\n'));
        self.command.len() != before
    }

    /// The `project.update` to send: the trimmed command (blank clears it) and auto-start.
    /// `None` while one is in flight.
    pub fn begin(&mut self) -> Option<ProjectUpdateParams> {
        if self.busy {
            return None;
        }
        self.busy = true;
        self.error = None;
        let command = self.command.trim();
        Some(ProjectUpdateParams {
            project_id: self.project_id,
            name: None,
            default_agent: None,
            dev_command: Some((!command.is_empty()).then(|| command.to_owned())),
            dev_autostart: Some(self.autostart),
        })
    }

    /// The `project.update` failed; the dialog stays open.
    pub fn failed(&mut self, error: String) {
        self.busy = false;
        self.error = Some(error);
    }
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::StateSource;

    use super::*;

    fn session(project_id: Ulid, kind: SessionKind, title: &str, state: SessionState) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id,
            agent_profile: "shell".into(),
            kind,
            title: Some(title.into()),
            native_session_id: None,
            state,
            state_source: StateSource::Screen,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
        }
    }

    fn project(dev_command: Option<&str>, dev_autostart: bool) -> Project {
        Project {
            id: Ulid::new(),
            name: "web".into(),
            path: "/web".into(),
            dev_command: dev_command.map(Into::into),
            dev_autostart,
            default_agent: None,
            review_globs: None,
            created_at: 0,
            archived: false,
            base_ref: None,
        }
    }

    #[test]
    fn the_dev_server_is_the_live_shell_titled_dev_of_the_workspace() {
        let web = Ulid::new();
        let api = Ulid::new();
        let sessions = vec![
            session(web, SessionKind::Shell, "dev", SessionState::Exited),
            session(web, SessionKind::Agent, "dev", SessionState::Idle),
            session(web, SessionKind::Shell, "build", SessionState::Idle),
            session(api, SessionKind::Shell, "dev", SessionState::Working),
        ];
        assert!(running(&sessions, web).is_none());
        let mut sessions = sessions;
        sessions.push(session(
            web,
            SessionKind::Shell,
            "dev",
            SessionState::Starting,
        ));
        assert_eq!(running(&sessions, web).map(|s| s.id), Some(sessions[4].id));
        assert_eq!(running(&sessions, api).map(|s| s.id), Some(sessions[3].id));
    }

    #[test]
    fn only_the_tab_holding_the_dev_server_is_relabelled() {
        assert_eq!(tab_label("dev", true), "Dev server");
        assert_eq!(tab_label("dev", false), "dev");
        assert_eq!(tab_label("Terminal", false), "Terminal");
    }

    #[test]
    fn opens_with_the_workspace_settings() {
        let set = DevDialog::new(&project(Some("npm run dev"), true));
        assert_eq!(set.command, "npm run dev");
        assert!(set.autostart);
        let unset = DevDialog::new(&project(None, false));
        assert_eq!(unset.command, "");
        assert!(!unset.autostart);
    }

    #[test]
    fn typing_edits_one_line_and_saving_sends_both_fields() {
        let project = project(None, false);
        let mut dialog = DevDialog::new(&project);
        assert!(dialog.key(&Key::Insert("npm run devx".into())));
        assert!(dialog.key(&Key::Backspace));
        assert!(!dialog.insert("\r\n"));
        assert!(dialog.insert("  \n"));
        assert!(dialog.key(&Key::ToggleAutostart));
        let params = dialog.begin().unwrap();
        assert_eq!(params.project_id, project.id);
        assert_eq!(params.name, None);
        assert_eq!(params.default_agent, None);
        assert_eq!(params.dev_command, Some(Some("npm run dev".to_owned())));
        assert_eq!(params.dev_autostart, Some(true));
        // One at a time.
        assert!(dialog.begin().is_none());
        dialog.failed("unknown project".into());
        assert!(!dialog.busy);
        assert_eq!(dialog.error.as_deref(), Some("unknown project"));
        assert!(dialog.begin().is_some());
        assert_eq!(dialog.error, None);
    }

    #[test]
    fn a_blank_command_clears_it() {
        let mut dialog = DevDialog::new(&project(Some("make serve"), true));
        dialog.command = "   ".into();
        let params = dialog.begin().unwrap();
        assert_eq!(params.dev_command, Some(None));
        assert_eq!(params.dev_autostart, Some(true));
    }

    #[test]
    fn keys() {
        let parse = |keys: &str| key(&Keystroke::parse(keys).unwrap());
        assert_eq!(parse("escape"), Some(Key::Close));
        assert_eq!(parse("enter"), Some(Key::Save));
        assert_eq!(parse("tab"), Some(Key::ToggleAutostart));
        assert_eq!(parse("backspace"), Some(Key::Backspace));
        assert_eq!(parse("ctrl-v"), Some(Key::Paste));
        assert_eq!(parse("ctrl-x"), None);
    }
}
