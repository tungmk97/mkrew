//! The new / edit task dialog of the board (SPEC §10, §13): title, prompt and agent, and the
//! request saving them. Pure and tested headlessly; `main.rs` renders it.

use gpui::Keystroke;
use mkrew_protocol::{AgentProfileInfo, Request, Task, TaskCreateParams, TaskUpdateParams};
use ulid::Ulid;

use crate::text_box::TextBox;
use crate::workspace_menu::{AgentChoice, Agents, agent_choices};

/// Creating a task in a workspace, or editing one.
#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    New { project_id: Ulid },
    Edit(Task),
}

/// The text field keys go to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Title,
    Prompt,
}

/// What a key press in the dialog does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogKey {
    /// Esc.
    Cancel,
    /// Ctrl+Enter (Cmd+Enter on macOS) anywhere.
    Save,
    /// Enter: saves from the title, a new line in the prompt.
    Enter,
    /// Tab / Shift+Tab.
    SwitchField,
    Backspace,
    /// Ctrl+V (Cmd+V): the caller inserts the clipboard text.
    Paste,
    Insert(String),
}

/// The dialog meaning of `keystroke`; `None` = ignored.
pub fn dialog_key(keystroke: &Keystroke) -> Option<DialogKey> {
    let m = keystroke.modifiers;
    let command = m.platform || m.control;
    match keystroke.key.as_str() {
        "escape" => return Some(DialogKey::Cancel),
        "enter" if command => return Some(DialogKey::Save),
        "enter" => return Some(DialogKey::Enter),
        "tab" => return Some(DialogKey::SwitchField),
        "backspace" => return Some(DialogKey::Backspace),
        "v" if command && !m.alt && !m.shift => return Some(DialogKey::Paste),
        _ => {}
    }
    // AltGr arrives as Ctrl+Alt with the composed char; plain Ctrl+key types nothing.
    if m.platform || (m.control && !m.alt) {
        return None;
    }
    keystroke
        .key_char
        .as_deref()
        .filter(|text| !text.is_empty() && !text.chars().any(char::is_control))
        .map(|text| DialogKey::Insert(text.to_owned()))
}

/// What the dialog wants after a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Keep it open (the draft may have changed).
    Edited,
    Cancel,
    Save,
}

/// The open dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskDialog {
    pub mode: Mode,
    pub title: String,
    /// The multi-line prompt (§20B.1).
    pub prompt: TextBox,
    /// `None` = the workspace's default agent at run time.
    pub agent: Option<String>,
    pub field: Field,
    /// The agent profiles (`agent.profiles`, fetched when the dialog opens).
    pub agents: Agents,
    /// A create/update/delete is in flight.
    pub busy: bool,
    /// Why the last save or delete failed (or the draft is invalid).
    pub error: Option<String>,
}

impl TaskDialog {
    pub fn new(project_id: Ulid) -> Self {
        Self::open(Mode::New { project_id }, String::new(), String::new(), None)
    }

    pub fn edit(task: Task) -> Self {
        let (title, prompt, agent) = (
            task.title.clone(),
            task.prompt.clone(),
            task.agent_profile.clone(),
        );
        Self::open(Mode::Edit(task), title, prompt, agent)
    }

    fn open(mode: Mode, title: String, text: String, agent: Option<String>) -> Self {
        let mut prompt = TextBox::default();
        prompt.insert(&text);
        Self {
            mode,
            title,
            prompt,
            agent,
            field: Field::Title,
            agents: Agents::Loading,
            busy: false,
            error: None,
        }
    }

    pub fn project_id(&self) -> Ulid {
        match &self.mode {
            Mode::New { project_id } => *project_id,
            Mode::Edit(task) => task.project_id,
        }
    }

    /// The edited task, if editing.
    pub fn task_id(&self) -> Option<Ulid> {
        match &self.mode {
            Mode::New { .. } => None,
            Mode::Edit(task) => Some(task.id),
        }
    }

    /// Applies a key to the focused field. Paste is the caller's (see [`TaskDialog::insert`]).
    pub fn key(&mut self, key: DialogKey) -> Outcome {
        match key {
            DialogKey::Cancel => return Outcome::Cancel,
            DialogKey::Save => return Outcome::Save,
            DialogKey::Enter => match self.field {
                Field::Title => return Outcome::Save,
                Field::Prompt => {
                    self.prompt.key(DialogKey::Enter);
                }
            },
            DialogKey::SwitchField => {
                self.field = match self.field {
                    Field::Title => Field::Prompt,
                    Field::Prompt => Field::Title,
                };
            }
            DialogKey::Backspace => match self.field {
                Field::Title => {
                    self.title.pop();
                }
                Field::Prompt => {
                    self.prompt.key(DialogKey::Backspace);
                }
            },
            DialogKey::Insert(text) => self.insert(&text),
            DialogKey::Paste => {}
        }
        Outcome::Edited
    }

    /// Types `text` into the focused field; the title stays on one line.
    pub fn insert(&mut self, text: &str) {
        match self.field {
            Field::Title => self.title.extend(
                text.replace("\r\n", "\n")
                    .chars()
                    .map(|c| if c.is_control() { ' ' } else { c }),
            ),
            Field::Prompt => self.prompt.insert(text),
        }
    }

    /// Types a picked library prompt (§20B.3) at the end of the prompt, which gets the keys.
    pub fn insert_prompt(&mut self, text: &str) {
        self.field = Field::Prompt;
        self.prompt.insert(text);
    }

    /// The agent rows: the workspace default first (named after `default_agent` when set),
    /// then every agent profile.
    pub fn agent_choices(
        &self,
        profiles: &[AgentProfileInfo],
        default_agent: Option<&str>,
    ) -> Vec<AgentChoice> {
        let default_label = match default_agent {
            Some(agent) => format!("Workspace default ({agent})"),
            None => "Workspace default".to_owned(),
        };
        let mut choices = vec![AgentChoice {
            id: String::new(),
            label: default_label,
            selected: self.agent.is_none(),
        }];
        choices.extend(agent_choices(profiles, self.agent.as_deref()));
        choices
    }

    /// Picks an agent row ([`TaskDialog::agent_choices`]): `""` = the workspace default.
    pub fn choose_agent(&mut self, id: &str) {
        self.agent = (!id.is_empty()).then(|| id.to_owned());
    }

    /// The request saving the draft: `Ok(None)` = nothing changed (just close); `Err` = an
    /// invalid draft, to show in the dialog.
    pub fn save_request(&self) -> Result<Option<Request>, &'static str> {
        let title = self.title.trim();
        if title.is_empty() {
            return Err("Give the task a title.");
        }
        let prompt = self.prompt.text().trim();
        Ok(match &self.mode {
            Mode::New { project_id } => Some(Request::TaskCreate(TaskCreateParams {
                project_id: *project_id,
                title: title.to_owned(),
                prompt: prompt.to_owned(),
                agent_profile: self.agent.clone(),
                position: None,
            })),
            Mode::Edit(task) => {
                let params = TaskUpdateParams {
                    task_id: task.id,
                    title: (title != task.title).then(|| title.to_owned()),
                    prompt: (prompt != task.prompt).then(|| prompt.to_owned()),
                    agent_profile: (self.agent != task.agent_profile).then(|| self.agent.clone()),
                };
                let changed = params.title.is_some()
                    || params.prompt.is_some()
                    || params.agent_profile.is_some();
                changed.then_some(Request::TaskUpdate(params))
            }
        })
    }
}

/// The delete confirmation's answers, in `window.prompt` order.
pub const DELETE_ANSWERS: [&str; 2] = ["Delete", "Cancel"];

/// Whether the confirmation's answer (an index into [`DELETE_ANSWERS`]) deletes.
pub fn confirms_delete(answer: Option<usize>) -> bool {
    answer == Some(0)
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{SessionKind, TaskStatus};

    use super::*;

    fn key(spec: &str) -> Option<DialogKey> {
        let mut keystroke = Keystroke::parse(spec).unwrap();
        if keystroke.key_char.is_none() && keystroke.key.chars().count() == 1 {
            keystroke.key_char = Some(keystroke.key.clone());
        }
        dialog_key(&keystroke)
    }

    fn task() -> Task {
        Task {
            id: Ulid::new(),
            project_id: Ulid::new(),
            title: "Fix login".into(),
            prompt: "Fix the login bug".into(),
            agent_profile: Some("codex".into()),
            status: TaskStatus::Todo,
            position: 1.0,
            checkpoint: None,
            error_flag: false,
            created_at: 1,
            updated_at: 1,
        }
    }

    fn type_text(dialog: &mut TaskDialog, text: &str) {
        for c in text.chars() {
            dialog.key(DialogKey::Insert(c.to_string()));
        }
    }

    fn profile(id: &str, kind: SessionKind) -> AgentProfileInfo {
        AgentProfileInfo {
            id: id.into(),
            name: id.to_uppercase(),
            kind,
            command: id.into(),
        }
    }

    #[test]
    fn keys_map_to_dialog_actions() {
        assert_eq!(key("escape"), Some(DialogKey::Cancel));
        assert_eq!(key("enter"), Some(DialogKey::Enter));
        assert_eq!(key("ctrl-enter"), Some(DialogKey::Save));
        assert_eq!(key("cmd-enter"), Some(DialogKey::Save));
        assert_eq!(key("tab"), Some(DialogKey::SwitchField));
        assert_eq!(key("shift-tab"), Some(DialogKey::SwitchField));
        assert_eq!(key("backspace"), Some(DialogKey::Backspace));
        assert_eq!(key("ctrl-v"), Some(DialogKey::Paste));
        assert_eq!(key("a"), Some(DialogKey::Insert("a".into())));
        assert_eq!(key("ctrl-a"), None);
    }

    #[test]
    fn typing_fills_the_focused_field_and_enter_saves_from_the_title() {
        let mut dialog = TaskDialog::new(Ulid::new());
        type_text(&mut dialog, "Add tests");
        assert_eq!(dialog.key(DialogKey::SwitchField), Outcome::Edited);
        type_text(&mut dialog, "Cover x");
        assert_eq!(dialog.key(DialogKey::Enter), Outcome::Edited);
        type_text(&mut dialog, "and yy");
        dialog.key(DialogKey::Backspace);
        assert_eq!(dialog.title, "Add tests");
        assert_eq!(dialog.prompt.text(), "Cover x\nand y");
        dialog.key(DialogKey::SwitchField);
        assert_eq!(dialog.key(DialogKey::Enter), Outcome::Save);
        assert_eq!(dialog.key(DialogKey::Save), Outcome::Save);
        assert_eq!(dialog.key(DialogKey::Cancel), Outcome::Cancel);
    }

    #[test]
    fn pasting_keeps_the_title_on_one_line() {
        let mut dialog = TaskDialog::new(Ulid::new());
        dialog.insert("a\r\nb");
        dialog.key(DialogKey::SwitchField);
        dialog.insert("one\r\ntwo");
        assert_eq!(dialog.title, "a b");
        assert_eq!(dialog.prompt.text(), "one\ntwo");
    }

    #[test]
    fn a_picked_prompt_goes_to_the_end_of_the_prompt() {
        let mut dialog = TaskDialog::edit(task());
        assert_eq!(dialog.field, Field::Title);
        dialog.insert_prompt("\nReview it.\r\n");
        assert_eq!(dialog.field, Field::Prompt);
        assert_eq!(dialog.title, "Fix login");
        assert_eq!(dialog.prompt.text(), "Fix the login bug\nReview it.\n");
        assert_eq!(dialog.save_request().map(|r| r.is_some()), Ok(true));
    }

    #[test]
    fn a_new_task_needs_a_title_and_goes_to_the_chosen_agent() {
        let project_id = Ulid::new();
        let mut dialog = TaskDialog::new(project_id);
        assert_eq!(dialog.save_request(), Err("Give the task a title."));
        type_text(&mut dialog, "  Ship it ");
        dialog.key(DialogKey::SwitchField);
        type_text(&mut dialog, " go \n");
        dialog.choose_agent("claude");
        assert_eq!(
            dialog.save_request(),
            Ok(Some(Request::TaskCreate(TaskCreateParams {
                project_id,
                title: "Ship it".into(),
                prompt: "go".into(),
                agent_profile: Some("claude".into()),
                position: None,
            })))
        );
        dialog.choose_agent("");
        let Ok(Some(Request::TaskCreate(params))) = dialog.save_request() else {
            panic!("expected task.create");
        };
        assert_eq!(params.agent_profile, None);
    }

    #[test]
    fn editing_sends_only_the_changed_fields() {
        let task = task();
        let mut dialog = TaskDialog::edit(task.clone());
        assert_eq!(dialog.project_id(), task.project_id);
        assert_eq!(dialog.task_id(), Some(task.id));
        assert_eq!(dialog.save_request(), Ok(None));
        dialog.key(DialogKey::Insert("!".into()));
        dialog.choose_agent("");
        assert_eq!(
            dialog.save_request(),
            Ok(Some(Request::TaskUpdate(TaskUpdateParams {
                task_id: task.id,
                title: Some("Fix login!".into()),
                prompt: None,
                agent_profile: Some(None),
            })))
        );
    }

    #[test]
    fn agent_rows_start_with_the_workspace_default() {
        let profiles = [
            profile("claude", SessionKind::Agent),
            profile("pwsh", SessionKind::Shell),
        ];
        let mut dialog = TaskDialog::new(Ulid::new());
        let rows = |dialog: &TaskDialog, default| {
            dialog
                .agent_choices(&profiles, default)
                .into_iter()
                .map(|c| (c.id, c.label, c.selected))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            rows(&dialog, Some("claude")),
            [
                (String::new(), "Workspace default (claude)".into(), true),
                ("claude".into(), "CLAUDE".into(), false),
            ]
        );
        dialog.choose_agent("claude");
        assert_eq!(
            rows(&dialog, None),
            [
                (String::new(), "Workspace default".into(), false),
                ("claude".into(), "CLAUDE".into(), true),
            ]
        );
    }

    #[test]
    fn only_the_delete_answer_deletes() {
        assert!(confirms_delete(Some(0)));
        assert!(!confirms_delete(Some(1)));
        assert!(!confirms_delete(None));
    }
}
