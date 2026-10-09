//! The "Run setup" dialog (SPEC §10, ADR 0018): one card per run command (name, folder,
//! command, env file, env lines) and "Start with the daemon"; saving sends `project.update`.
//! Pure; `main.rs` renders it.

use mkrew_core::run::{normalize, parse_env};
use mkrew_protocol::{Project, ProjectUpdateParams, RunCommand};
use ulid::Ulid;

use crate::task_dialog::DialogKey;
use crate::text_box::{Input, TextBox};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Card {
    pub name: Input,
    pub dir: Input,
    pub command: Input,
    pub env_file: Input,
    /// `KEY=VALUE` lines.
    pub env: TextBox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Dir,
    Command,
    EnvFile,
    Env,
}

impl Field {
    const ORDER: [Field; 5] = [
        Field::Name,
        Field::Dir,
        Field::Command,
        Field::EnvFile,
        Field::Env,
    ];
}

impl Card {
    /// Whether Ctrl+A selected the whole text of `field` (drawn highlighted).
    pub fn all_selected(&self, field: Field) -> bool {
        match field {
            Field::Name => self.name.all_selected(),
            Field::Dir => self.dir.all_selected(),
            Field::Command => self.command.all_selected(),
            Field::EnvFile => self.env_file.all_selected(),
            Field::Env => self.env.all_selected(),
        }
    }
}

/// Where typing goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Field(usize, Field),
    Autostart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Edited,
    Cancel,
    Save,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSetup {
    pub project_id: Ulid,
    pub cards: Vec<Card>,
    pub autostart: bool,
    pub focus: Focus,
    pub busy: bool,
    pub error: Option<String>,
    /// Opened from ▶ Run with no command: run everything after a successful save.
    pub run_after: bool,
}

impl RunSetup {
    pub fn new(project: &Project, run_after: bool) -> RunSetup {
        let mut cards: Vec<Card> = project.run_commands.iter().map(card).collect();
        if cards.is_empty() {
            cards.push(empty_card());
        }
        RunSetup {
            project_id: project.id,
            cards,
            autostart: project.run_autostart,
            focus: Focus::Field(0, Field::Name),
            busy: false,
            error: None,
            run_after,
        }
    }

    pub fn key(&mut self, key: DialogKey) -> Outcome {
        match key {
            DialogKey::Cancel => Outcome::Cancel,
            DialogKey::Save => Outcome::Save,
            DialogKey::Enter => match self.focus {
                Focus::Field(i, Field::Env) => {
                    self.cards[i].env.key(DialogKey::Enter);
                    Outcome::Edited
                }
                _ => Outcome::Save,
            },
            DialogKey::SwitchField => {
                self.focus = self.next_focus();
                Outcome::Edited
            }
            key @ (DialogKey::Backspace | DialogKey::DeleteWord | DialogKey::SelectAll) => {
                match self.focus {
                    Focus::Field(i, Field::Env) => {
                        self.cards[i].env.key(key);
                    }
                    Focus::Field(i, field) => {
                        line(&mut self.cards[i], field).edit(&key);
                    }
                    Focus::Autostart => {}
                }
                Outcome::Edited
            }
            // The caller reads the clipboard and calls `insert`.
            DialogKey::Paste => Outcome::Edited,
            DialogKey::Insert(text) => {
                self.insert(&text);
                Outcome::Edited
            }
        }
    }

    /// Typed or pasted text into the focused field; Space flips "Start with the daemon".
    pub fn insert(&mut self, text: &str) {
        match self.focus {
            Focus::Autostart => {
                if text == " " {
                    self.autostart = !self.autostart;
                }
            }
            Focus::Field(i, Field::Env) => self.cards[i].env.insert(text),
            Focus::Field(i, field) => {
                let text: String = text.chars().filter(|c| !c.is_control()).collect();
                line(&mut self.cards[i], field).insert(&text);
            }
        }
    }

    pub fn add_card(&mut self) {
        self.cards.push(empty_card());
        self.focus = Focus::Field(self.cards.len() - 1, Field::Name);
    }

    pub fn remove_card(&mut self, index: usize) {
        if index >= self.cards.len() {
            return;
        }
        self.cards.remove(index);
        self.focus = match self.cards.len() {
            0 => Focus::Autostart,
            len => Focus::Field(index.min(len - 1), Field::Name),
        };
    }

    /// The `project.update` to send, or `None` (in flight, or invalid: `error` says why).
    pub fn begin(&mut self) -> Option<ProjectUpdateParams> {
        if self.busy {
            return None;
        }
        let mut commands = Vec::with_capacity(self.cards.len());
        for card in &self.cards {
            let env = match parse_env(card.env.text()) {
                Ok(env) => env,
                Err(e) => {
                    self.error = Some(format!("{}: env: {e}", card.name.trim()));
                    return None;
                }
            };
            commands.push(RunCommand {
                name: card.name.as_str().to_owned(),
                dir: card.dir.as_str().to_owned(),
                command: card.command.as_str().to_owned(),
                env_file: Some(card.env_file.as_str().to_owned()),
                env,
            });
        }
        let commands = match normalize(commands) {
            Ok(commands) => commands,
            Err(e) => {
                self.error = Some(e.to_string());
                return None;
            }
        };
        self.busy = true;
        self.error = None;
        Some(ProjectUpdateParams {
            project_id: self.project_id,
            name: None,
            default_agent: None,
            default_shell: None,
            run_commands: Some(commands),
            run_autostart: Some(self.autostart),
            color: None,
        })
    }

    /// The `project.update` failed; the dialog stays open.
    pub fn failed(&mut self, error: String) {
        self.busy = false;
        self.error = Some(error);
    }

    fn next_focus(&self) -> Focus {
        match self.focus {
            Focus::Autostart if self.cards.is_empty() => Focus::Autostart,
            Focus::Autostart => Focus::Field(0, Field::Name),
            Focus::Field(i, field) => {
                let at = Field::ORDER.iter().position(|f| *f == field).unwrap_or(0);
                match Field::ORDER.get(at + 1) {
                    Some(next) => Focus::Field(i, *next),
                    None if i + 1 < self.cards.len() => Focus::Field(i + 1, Field::Name),
                    None => Focus::Autostart,
                }
            }
        }
    }
}

fn empty_card() -> Card {
    Card {
        dir: ".".into(),
        ..Card::default()
    }
}

fn card(command: &RunCommand) -> Card {
    let mut env = TextBox::default();
    let lines: Vec<String> = command
        .env
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    env.insert(&lines.join("\n"));
    Card {
        name: command.name.clone().into(),
        dir: command.dir.clone().into(),
        command: command.command.clone().into(),
        env_file: command.env_file.clone().unwrap_or_default().into(),
        env,
    }
}

/// The single-line field `field` of `card` (`Env` never reaches here).
fn line(card: &mut Card, field: Field) -> &mut Input {
    match field {
        Field::Name => &mut card.name,
        Field::Dir => &mut card.dir,
        Field::Command => &mut card.command,
        Field::EnvFile | Field::Env => &mut card.env_file,
    }
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{Project, RunCommand};

    use super::*;

    fn project(commands: Vec<RunCommand>) -> Project {
        Project {
            id: Ulid::new(),
            name: "EA".into(),
            path: "/ea".into(),
            run_commands: commands,
            run_autostart: true,
            default_agent: None,
            default_shell: None,
            review_globs: None,
            created_at: 0,
            archived: false,
            base_ref: None,
            color: None,
        }
    }

    fn be() -> RunCommand {
        RunCommand {
            name: "BE".into(),
            dir: "EA-BE".into(),
            command: r".venv\Scripts\python.exe main.py".into(),
            env_file: Some(".env".into()),
            env: vec![
                ("ENVIRONMENT".into(), "dev".into()),
                ("PYTHONPATH".into(), ".".into()),
            ],
        }
    }

    #[test]
    fn opens_with_the_saved_commands_and_saves_them_back() {
        let p = project(vec![be()]);
        let mut setup = RunSetup::new(&p, false);
        assert_eq!(setup.cards.len(), 1);
        assert_eq!(setup.cards[0].env.text(), "ENVIRONMENT=dev\nPYTHONPATH=.");
        assert!(setup.autostart);
        let params = setup.begin().unwrap();
        assert_eq!(params.project_id, p.id);
        assert_eq!(params.run_commands, Some(vec![be()]));
        assert_eq!(params.run_autostart, Some(true));
        assert!(setup.busy && setup.begin().is_none());
        setup.failed("daemon said no".into());
        assert!(!setup.busy);
        assert_eq!(setup.error.as_deref(), Some("daemon said no"));
    }

    #[test]
    fn a_new_setup_starts_with_one_empty_card_in_the_project_folder() {
        let mut setup = RunSetup::new(&project(Vec::new()), true);
        assert!(setup.run_after);
        assert_eq!(setup.cards.len(), 1);
        assert_eq!(setup.cards[0].dir, ".");
        assert_eq!(setup.focus, Focus::Field(0, Field::Name));
        setup.insert("FE");
        setup.key(DialogKey::SwitchField);
        setup.key(DialogKey::SwitchField);
        setup.insert("npm run dev\r\n");
        let params = setup.begin().unwrap();
        assert_eq!(
            params.run_commands,
            Some(vec![RunCommand {
                name: "FE".into(),
                dir: ".".into(),
                command: "npm run dev".into(),
                env_file: None,
                env: Vec::new(),
            }])
        );
    }

    #[test]
    fn tab_walks_every_field_then_autostart_then_wraps() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.add_card();
        assert_eq!(setup.focus, Focus::Field(1, Field::Name));
        setup.focus = Focus::Field(0, Field::Name);
        let mut seen = vec![setup.focus];
        for _ in 0..11 {
            setup.key(DialogKey::SwitchField);
            seen.push(setup.focus);
        }
        assert_eq!(seen[4], Focus::Field(0, Field::Env));
        assert_eq!(seen[5], Focus::Field(1, Field::Name));
        assert_eq!(seen[10], Focus::Autostart);
        assert_eq!(seen[11], Focus::Field(0, Field::Name));
        setup.focus = Focus::Autostart;
        setup.insert(" ");
        assert!(!setup.autostart);
    }

    #[test]
    fn enter_in_env_adds_a_line_and_bad_env_blocks_saving() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.focus = Focus::Field(0, Field::Env);
        assert_eq!(setup.key(DialogKey::Enter), Outcome::Edited);
        setup.insert("not a pair");
        assert!(setup.cards[0].env.text().ends_with("\nnot a pair"));
        assert_eq!(setup.begin(), None);
        assert_eq!(
            setup.error.as_deref(),
            Some("BE: env: line 3: expected KEY=VALUE")
        );
        assert!(!setup.busy);
        setup.focus = Focus::Field(0, Field::Command);
        assert_eq!(setup.key(DialogKey::Enter), Outcome::Save);
        assert_eq!(setup.key(DialogKey::Save), Outcome::Save);
        assert_eq!(setup.key(DialogKey::Cancel), Outcome::Cancel);
    }

    #[test]
    fn invalid_commands_block_saving_with_the_reason() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.add_card();
        setup.insert("BE");
        assert_eq!(setup.begin(), None);
        assert_eq!(
            setup.error.as_deref(),
            Some("two run commands are named \"BE\"")
        );
    }

    #[test]
    fn removing_cards_moves_focus_and_allows_an_empty_list() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.add_card();
        setup.remove_card(1);
        assert_eq!(setup.focus, Focus::Field(0, Field::Name));
        setup.remove_card(0);
        assert!(setup.cards.is_empty());
        assert_eq!(setup.focus, Focus::Autostart);
        assert_eq!(setup.begin().unwrap().run_commands, Some(Vec::new()));
    }

    #[test]
    fn backspace_edits_the_focused_field_only() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.focus = Focus::Field(0, Field::Dir);
        setup.key(DialogKey::Backspace);
        assert_eq!(setup.cards[0].dir, "EA-B");
        assert_eq!(setup.cards[0].name, "BE");
    }

    #[test]
    fn single_line_fields_and_the_env_box_take_select_all_and_ctrl_backspace() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.cards[0].command = "npm run dev".into();
        setup.focus = Focus::Field(0, Field::Command);
        setup.key(DialogKey::DeleteWord);
        assert_eq!(setup.cards[0].command, "npm run ");
        setup.key(DialogKey::SelectAll);
        assert!(setup.cards[0].all_selected(Field::Command));
        assert!(!setup.cards[0].all_selected(Field::Name), "per field");
        setup.insert("make");
        assert_eq!(setup.cards[0].command, "make");
        assert_eq!(setup.cards[0].name, "BE", "other fields untouched");

        setup.focus = Focus::Field(0, Field::Env);
        setup.key(DialogKey::SelectAll);
        setup.insert("A=1\nB=2");
        setup.key(DialogKey::DeleteWord);
        assert_eq!(setup.cards[0].env.text(), "A=1\nB=");
        setup.key(DialogKey::SelectAll);
        assert!(setup.cards[0].all_selected(Field::Env));
    }
}
