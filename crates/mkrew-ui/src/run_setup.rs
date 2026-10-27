//! The "Run setup" dialog (SPEC §10, ADR 0018): one card per run command (name, folder,
//! command, env file, env lines) and "Start with the daemon"; saving sends `project.update`.
//! The fields take caret keys and selection; the folder and env file can also be picked from
//! the workspace ([`Picker`], fed by `fs.list`). Pure; `main.rs` renders it.

use mkrew_core::run::{MAX_COMMANDS, MAX_NAME_CHARS, RunError, normalize, parse_env};
use mkrew_protocol::{FsEntry, Project, ProjectUpdateParams, RunCommand};
use ulid::Ulid;

use crate::editor::Motion;
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
    /// "Close the pane when it ends": an exit is typed after the command.
    pub exit_after: bool,
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
    /// The text, caret and selection of `field`.
    pub fn input(&self, field: Field) -> &Input {
        match field {
            Field::Name => &self.name,
            Field::Dir => &self.dir,
            Field::Command => &self.command,
            Field::EnvFile => &self.env_file,
            Field::Env => self.env.input(),
        }
    }
}

/// Choosing a card's folder ([`Field::Dir`]) or env file ([`Field::EnvFile`]) among the
/// workspace's, one folder listed at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker {
    pub card: usize,
    pub field: Field,
    /// The listed folder, relative to the workspace (`""` = its root, where ↑ Up stops).
    pub dir: String,
    /// What `fs.list` answered (only folders when picking a folder); `None` while it runs.
    pub entries: Option<Result<Vec<FsEntry>, String>>,
    /// The entry ↑ / ↓ picked, if any (a new listing clears it).
    pub cursor: Option<usize>,
}

impl Picker {
    pub fn at_root(&self) -> bool {
        self.dir.is_empty()
    }

    /// The listed entries (none while loading or after an error).
    pub fn listed_entries(&self) -> &[FsEntry] {
        match &self.entries {
            Some(Ok(entries)) => entries,
            _ => &[],
        }
    }
}

/// Where typing goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Field(usize, Field),
    /// The card's "Close the pane when it ends" (Space flips it).
    Exit(usize),
    Autostart,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Edited,
    Cancel,
    Save,
    /// The picker shows another folder: `fs.list` it.
    List(String),
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
    /// The open folder / env file picker; it covers the cards.
    pub picker: Option<Picker>,
    /// The card last entered by the keyboard: it stays painted while `focus` is `Autostart`.
    open: usize,
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
            picker: None,
            open: 0,
        }
    }

    pub fn key(&mut self, key: DialogKey) -> Outcome {
        if self.picker.is_some() {
            return match key {
                DialogKey::Cancel => {
                    self.picker = None;
                    Outcome::Edited
                }
                DialogKey::Move {
                    motion: Motion::Down,
                    ..
                } => {
                    self.picker_step(true);
                    Outcome::Edited
                }
                DialogKey::Move {
                    motion: Motion::Up, ..
                } => {
                    self.picker_step(false);
                    Outcome::Edited
                }
                DialogKey::Move {
                    motion: Motion::Right,
                    ..
                } => self.picker_open(),
                DialogKey::Move {
                    motion: Motion::Left,
                    ..
                } => self.browse_up().map_or(Outcome::Edited, Outcome::List),
                DialogKey::Enter | DialogKey::Save => self.picker_enter(),
                _ => Outcome::Edited,
            };
        }
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
                self.tab(true);
                Outcome::Edited
            }
            // The caller reads the clipboard and calls `insert`, or copies the selection.
            DialogKey::Paste | DialogKey::Copy => Outcome::Edited,
            DialogKey::Insert(text) => {
                self.insert(&text);
                Outcome::Edited
            }
            key => {
                match self.focus {
                    Focus::Field(i, Field::Env) => {
                        self.cards[i].env.edit(&key);
                    }
                    Focus::Field(i, field) => {
                        line(&mut self.cards[i], field).edit(&key);
                    }
                    Focus::Exit(_) | Focus::Autostart => {}
                }
                Outcome::Edited
            }
        }
    }

    /// The selected text of the focused field (Ctrl+C / Ctrl+X copy it).
    pub fn selected_text(&self) -> Option<&str> {
        match self.focus {
            Focus::Field(i, field) => self.cards.get(i)?.input(field).selected_text(),
            Focus::Exit(_) | Focus::Autostart => None,
        }
    }

    /// A press or drag at byte `at` of `field` in card `card` (see `Input::click`): the field
    /// takes the keyboard and its caret goes there.
    pub fn click(&mut self, card: usize, field: Field, at: usize, extend: bool, clicks: usize) {
        let Some(c) = self.cards.get_mut(card) else {
            return;
        };
        // A press in another field starts there, not from that field's old anchor.
        let extend = extend && self.focus == Focus::Field(card, field);
        match field {
            Field::Env => c.env.click(at, extend, clicks),
            field => line(c, field).click(at, extend, clicks),
        }
        self.set_focus(Focus::Field(card, field));
    }

    /// Opens the picker for `field` (`Dir` or `EnvFile`) of card `card`; returns the folder to
    /// `fs.list`. Both browse the whole workspace; the env file picker starts in the card's
    /// folder (the root when that folder is not one of the workspace).
    pub fn open_picker(&mut self, card: usize, field: Field) -> Option<String> {
        let dir = match field {
            Field::Dir => String::new(),
            Field::EnvFile => workspace_dir(self.cards.get(card)?.dir.as_str()).unwrap_or_default(),
            _ => return None,
        };
        self.set_focus(Focus::Field(card, field));
        self.picker = Some(Picker {
            card,
            field,
            dir: dir.clone(),
            entries: None,
            cursor: None,
        });
        Some(dir)
    }

    /// Shows folder `dir` (relative to the workspace) in the picker; returns it to `fs.list`.
    pub fn browse(&mut self, dir: String) -> Option<String> {
        let picker = self.picker.as_mut()?;
        picker.dir = dir.clone();
        picker.entries = None;
        picker.cursor = None;
        Some(dir)
    }

    /// ↓ / ↑ in the picker: the next / previous entry (the first / last when there is no
    /// cursor yet).
    fn picker_step(&mut self, down: bool) {
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        let count = picker.listed_entries().len();
        if count == 0 {
            return;
        }
        picker.cursor = Some(match (picker.cursor, down) {
            (None, true) => 0,
            (None, false) => count - 1,
            (Some(at), true) => (at + 1).min(count - 1),
            (Some(at), false) => at.saturating_sub(1),
        });
    }

    /// → in the picker: opens the folder under the cursor.
    fn picker_open(&mut self) -> Outcome {
        let folder = self.picker.as_ref().and_then(|picker| {
            let entry = picker.listed_entries().get(picker.cursor?)?;
            entry.is_dir.then(|| entry.path.clone())
        });
        folder
            .and_then(|dir| self.browse(dir))
            .map_or(Outcome::Edited, Outcome::List)
    }

    /// Enter in the picker. Picking a folder: the folder under the cursor, else the listed
    /// one. Picking an env file: a file under the cursor is taken, a folder under it is opened,
    /// nothing under it does nothing.
    fn picker_enter(&mut self) -> Outcome {
        let Some(picker) = self.picker.as_ref() else {
            return Outcome::Edited;
        };
        let under = picker
            .cursor
            .and_then(|at| picker.listed_entries().get(at))
            .map(|entry| (entry.path.clone(), entry.is_dir));
        match (picker.field, under) {
            (Field::Dir, Some((path, _))) => self.pick(Some(&path)),
            (Field::Dir, None) => self.pick(None),
            (_, Some((path, true))) => {
                return self.browse(path).map_or(Outcome::Edited, Outcome::List);
            }
            (_, Some((path, false))) => self.pick(Some(&path)),
            (_, None) => {}
        }
        Outcome::Edited
    }

    /// ↑ Up: the parent of the listed folder, never above the workspace root.
    pub fn browse_up(&mut self) -> Option<String> {
        let picker = self.picker.as_ref()?;
        if picker.at_root() {
            return None;
        }
        let parent = picker.dir.rsplit_once('/').map_or("", |(parent, _)| parent);
        self.browse(parent.to_owned())
    }

    /// The `fs.list` answer for `dir`; ignored once the picker shows another folder.
    pub fn listed(&mut self, dir: &str, entries: Result<Vec<FsEntry>, String>) {
        let Some(picker) = self.picker.as_mut().filter(|p| p.dir == dir) else {
            return;
        };
        let folders_only = picker.field == Field::Dir;
        picker.entries = Some(entries.map(|mut entries| {
            entries.retain(|e| e.is_dir || !folders_only);
            entries
        }));
    }

    /// Takes the listed folder (`None`) or the file `path` (relative to the workspace) as the
    /// card's folder / env file and closes the picker.
    pub fn pick(&mut self, path: Option<&str>) {
        let Some(picker) = self.picker.take() else {
            return;
        };
        let Some(card) = self.cards.get_mut(picker.card) else {
            return;
        };
        match (picker.field, path) {
            (Field::Dir, None) if picker.dir.is_empty() => card.dir = ".".into(),
            (Field::Dir, None) => card.dir = picker.dir.into(),
            (Field::Dir, Some(path)) => card.dir = path.into(),
            (Field::EnvFile, Some(path)) => card.env_file = path.into(),
            _ => {}
        }
        self.set_focus(Focus::Field(picker.card, picker.field));
    }

    /// Typed or pasted text into the focused field; Space flips a focused checkbox.
    pub fn insert(&mut self, text: &str) {
        match self.focus {
            Focus::Autostart => {
                if text == " " {
                    self.autostart = !self.autostart;
                }
            }
            Focus::Exit(i) => {
                if text == " " {
                    self.cards[i].exit_after = !self.cards[i].exit_after;
                }
            }
            Focus::Field(i, Field::Env) => self.cards[i].env.insert(text),
            Focus::Field(i, field) => {
                let text: String = text.chars().filter(|c| !c.is_control()).collect();
                line(&mut self.cards[i], field).insert(&text);
            }
        }
    }

    /// Appends an empty card and focuses its name. Refused at [`MAX_COMMANDS`].
    pub fn add_card(&mut self) {
        if self.cards.len() >= MAX_COMMANDS {
            self.error = Some(RunError::TooMany.to_string());
            return;
        }
        self.cards.push(empty_card());
        self.set_focus(Focus::Field(self.cards.len() - 1, Field::Name));
    }

    /// The card holding the keyboard, if any; the dialog shows only its fields.
    pub fn focused_card(&self) -> Option<usize> {
        match self.focus {
            Focus::Field(i, _) | Focus::Exit(i) => Some(i),
            Focus::Autostart => None,
        }
    }

    /// The card whose fields and buttons are painted: the keyboard's, else the last one it was
    /// in, so the forward Tab edge from "Start with the daemon" still reaches its Duplicate,
    /// Remove and Browse. Scrolling follows [`Self::focused_card`].
    pub fn shown_card(&self) -> Option<usize> {
        self.focused_card()
            .or((self.open < self.cards.len()).then_some(self.open))
    }

    pub fn remove_card(&mut self, index: usize) {
        if index >= self.cards.len() {
            return;
        }
        self.cards.remove(index);
        let focus = match self.cards.len() {
            0 => Focus::Autostart,
            len => Focus::Field(index.min(len - 1), Field::Name),
        };
        self.set_focus(focus);
    }

    /// Inserts a copy of card `index` right below it, named `<name> copy` (then `copy 2`, …,
    /// the name cut so it fits) and focuses its name. Refused at [`MAX_COMMANDS`].
    pub fn duplicate_card(&mut self, index: usize) {
        let Some(original) = self.cards.get(index) else {
            return;
        };
        if self.cards.len() >= MAX_COMMANDS {
            self.error = Some(RunError::TooMany.to_string());
            return;
        }
        let mut copy = original.clone();
        copy.name = self.free_copy_name(original.name.as_str().trim()).into();
        self.cards.insert(index + 1, copy);
        self.set_focus(Focus::Field(index + 1, Field::Name));
    }

    /// `<base> copy`, `<base> copy 2`, …: the first name no card has, `base` cut so the whole
    /// name fits in [`MAX_NAME_CHARS`].
    fn free_copy_name(&self, base: &str) -> String {
        (1..)
            .map(|n| {
                let suffix = if n == 1 {
                    " copy".to_owned()
                } else {
                    format!(" copy {n}")
                };
                let room = MAX_NAME_CHARS.saturating_sub(suffix.chars().count());
                let base: String = base.chars().take(room).collect();
                format!("{}{suffix}", base.trim_end())
                    .trim_start()
                    .to_owned()
            })
            .find(|name| {
                self.cards
                    .iter()
                    .all(|c| c.name.as_str().trim() != name.as_str())
            })
            // An endless range: `find` always returns; ≤ MAX_COMMANDS cards take ≤ 8 names.
            .unwrap_or_default()
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
                exit_after: card.exit_after,
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
            jira_keys: None,
            position: None,
            name: None,
            default_agent: None,
            default_shell: None,
            run_commands: Some(commands),
            run_autostart: Some(self.autostart),
            color: None,
            pinned: None,
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
                    None => Focus::Exit(i),
                }
            }
            Focus::Exit(i) if i + 1 < self.cards.len() => Focus::Field(i + 1, Field::Name),
            Focus::Exit(_) => Focus::Autostart,
        }
    }

    /// Puts the keyboard on `focus`; a card it enters becomes the one the dialog keeps open.
    pub fn set_focus(&mut self, focus: Focus) {
        if let Focus::Field(i, _) | Focus::Exit(i) = focus {
            self.open = i;
        }
        self.focus = focus;
    }

    /// Tab (`forward`) / Shift+Tab along the fields, the two checkboxes and back.
    pub fn tab(&mut self, forward: bool) {
        let focus = if forward {
            self.next_focus()
        } else {
            self.prev_focus()
        };
        self.set_focus(focus);
    }

    /// Tab (`forward`) / Shift+Tab has no field left in that direction: the keyboard leaves the
    /// fields for the buttons ([`crate::dialog_focus::nav`]). The picker has no fields.
    pub fn at_edge(&self, forward: bool) -> bool {
        if self.picker.is_some() {
            return true;
        }
        match (self.focus, forward) {
            (Focus::Autostart, true) => true,
            (Focus::Autostart, false) => self.cards.is_empty(),
            (Focus::Field(0, Field::Name), false) => true,
            _ => false,
        }
    }

    /// The keyboard came back to the fields from the buttons: Tab lands on the first field,
    /// Shift+Tab on "Start with the daemon".
    pub fn enter(&mut self, forward: bool) {
        if self.picker.is_some() {
            return;
        }
        let focus = if forward && !self.cards.is_empty() {
            Focus::Field(0, Field::Name)
        } else {
            Focus::Autostart
        };
        self.set_focus(focus);
    }

    fn prev_focus(&self) -> Focus {
        match self.focus {
            Focus::Autostart => match self.cards.len() {
                0 => Focus::Autostart,
                count => Focus::Exit(count - 1),
            },
            Focus::Field(i, field) => {
                let at = Field::ORDER.iter().position(|f| *f == field).unwrap_or(0);
                match at.checked_sub(1) {
                    Some(prev) => Focus::Field(i, Field::ORDER[prev]),
                    None if i == 0 => Focus::Autostart,
                    None => Focus::Exit(i - 1),
                }
            }
            Focus::Exit(i) => Focus::Field(i, Field::Env),
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
        exit_after: command.exit_after,
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

/// A card's folder as `fs.list` takes it (`"."` → `""`, `\` → `/`); `None` when it leaves the
/// workspace (absolute, `..`).
fn workspace_dir(dir: &str) -> Option<String> {
    let dir = dir.trim().replace('\\', "/");
    if dir.starts_with('/') || dir.contains(':') {
        return None;
    }
    let parts: Vec<&str> = dir
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.contains(&"..") {
        return None;
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{Project, RunCommand};

    use super::*;

    fn project(commands: Vec<RunCommand>) -> Project {
        Project {
            id: Ulid::new(),
            jira_keys: Vec::new(),
            position: 0,
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
            pinned: false,
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
            exit_after: true,
        }
    }

    #[test]
    fn duplicating_a_card_copies_it_below_under_a_free_name() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.duplicate_card(0);
        assert_eq!(setup.cards.len(), 2);
        assert_eq!(setup.cards[1].name, "BE copy");
        assert_eq!(setup.cards[1].dir, setup.cards[0].dir);
        assert_eq!(setup.cards[1].command, setup.cards[0].command);
        assert_eq!(setup.cards[1].env_file, setup.cards[0].env_file);
        assert_eq!(setup.cards[1].env.text(), setup.cards[0].env.text());
        assert_eq!(setup.focus, Focus::Field(1, Field::Name));

        setup.duplicate_card(0);
        let names: Vec<&str> = setup.cards.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["BE", "BE copy 2", "BE copy"]);
        assert!(setup.begin().is_some(), "the copies save");
    }

    #[test]
    fn duplicating_an_unnamed_card_twice_gives_copy_and_copy_2() {
        let mut unnamed = be();
        unnamed.name = String::new();
        let mut setup = RunSetup::new(&project(vec![unnamed]), false);
        setup.duplicate_card(0);
        setup.duplicate_card(0);
        let names: Vec<&str> = setup.cards.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["", "copy 2", "copy"]);
    }

    #[test]
    fn a_long_name_is_cut_to_fit_and_a_full_list_refuses() {
        let mut long = be();
        long.name = "abcdefghijklmnopqrst".into(); // 20 chars, the most
        let mut setup = RunSetup::new(&project(vec![long]), false);
        setup.duplicate_card(0);
        assert_eq!(setup.cards[1].name, "abcdefghijklmno copy");

        while setup.cards.len() < MAX_COMMANDS {
            setup.duplicate_card(0);
        }
        assert!(
            setup.begin().is_some(),
            "every generated name is valid and unique"
        );
        setup.busy = false;
        setup.duplicate_card(0);
        assert_eq!(setup.cards.len(), MAX_COMMANDS);
        assert_eq!(
            setup.error.as_deref(),
            Some(format!("at most {MAX_COMMANDS} run commands").as_str())
        );
        setup.error = None;
        setup.add_card();
        assert_eq!(setup.cards.len(), MAX_COMMANDS, "Add is refused too");
        assert!(setup.error.is_some());
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
                exit_after: false,
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
        for _ in 0..13 {
            setup.key(DialogKey::SwitchField);
            seen.push(setup.focus);
        }
        assert_eq!(seen[4], Focus::Field(0, Field::Env));
        assert_eq!(seen[5], Focus::Exit(0));
        assert_eq!(seen[6], Focus::Field(1, Field::Name));
        assert_eq!(seen[12], Focus::Autostart);
        assert_eq!(seen[13], Focus::Field(0, Field::Name));
        setup.focus = Focus::Autostart;
        setup.insert(" ");
        assert!(!setup.autostart);
        setup.focus = Focus::Exit(1);
        setup.insert(" ");
        assert!(setup.cards[1].exit_after, "Space flips the card's exit");
        assert_eq!(setup.focused_card(), Some(1));
    }

    #[test]
    fn a_click_focuses_the_field_and_moves_its_caret() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.click(0, Field::Command, 0, false, 1);
        setup.insert("uv run ");
        assert_eq!(setup.focus, Focus::Field(0, Field::Command));
        assert!(setup.cards[0].command.starts_with("uv run .venv"));
        // Shift+click in another field does not extend from that field's old anchor.
        setup.click(0, Field::Env, 3, true, 1);
        assert_eq!(setup.cards[0].env.input().selection(), None);
        setup.click(0, Field::Env, 9, true, 1);
        assert_eq!(setup.selected_text(), Some("IRONME"));
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
        assert!(setup.cards[0].input(Field::Command).all_selected());
        assert!(
            !setup.cards[0].input(Field::Name).all_selected(),
            "per field"
        );
        setup.insert("make");
        assert_eq!(setup.cards[0].command, "make");
        assert_eq!(setup.cards[0].name, "BE", "other fields untouched");

        setup.focus = Focus::Field(0, Field::Env);
        setup.key(DialogKey::SelectAll);
        setup.insert("A=1\nB=2");
        setup.key(DialogKey::DeleteWord);
        assert_eq!(setup.cards[0].env.text(), "A=1\nB=");
        setup.key(DialogKey::SelectAll);
        assert!(setup.cards[0].input(Field::Env).all_selected());
    }

    fn moved(motion: Motion, extend: bool) -> DialogKey {
        DialogKey::Move { motion, extend }
    }

    #[test]
    fn fields_edit_at_the_caret_and_replace_a_shift_selection() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.cards[0].command = "npm run dev".into();
        setup.focus = Focus::Field(0, Field::Command);
        setup.key(moved(Motion::WordLeft, false));
        setup.key(moved(Motion::End, true));
        assert_eq!(setup.selected_text(), Some("dev"));
        setup.insert("build");
        assert_eq!(setup.cards[0].command, "npm run build");
        setup.key(moved(Motion::Home, false));
        setup.key(DialogKey::Delete { word: false });
        setup.key(moved(Motion::Right, false));
        setup.insert("n");
        assert_eq!(setup.cards[0].command, "pnm run build");
        setup.key(moved(Motion::WordRight, false));
        setup.key(DialogKey::Backspace);
        assert_eq!(
            setup.cards[0].command, "pn run build",
            "Backspace at the caret"
        );
        setup.key(DialogKey::Delete { word: true });
        assert_eq!(setup.cards[0].command, "pn build");
        setup.key(moved(Motion::Left, true));
        setup.key(DialogKey::Cut);
        assert_eq!(
            setup.cards[0].command, "p build",
            "the caller copied it first"
        );

        // The env box: ↑ keeps the column, clamped to the shorter line above.
        setup.focus = Focus::Field(0, Field::Env);
        setup.key(DialogKey::SelectAll);
        setup.insert("A=1\nBB=2");
        setup.key(moved(Motion::Up, false));
        setup.insert("0");
        setup.key(moved(Motion::Down, false));
        setup.insert("!");
        assert_eq!(setup.cards[0].env.text(), "A=10\nBB=2!");
    }

    fn entry(path: &str, is_dir: bool) -> FsEntry {
        FsEntry {
            name: path.rsplit('/').next().unwrap_or(path).into(),
            path: path.into(),
            is_dir,
            ignored: false,
            marker: None,
            has_changes: false,
        }
    }

    #[test]
    fn the_folder_picker_lists_folders_and_takes_the_shown_one() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        assert_eq!(setup.open_picker(0, Field::Dir).as_deref(), Some(""));
        assert_eq!(setup.key(DialogKey::Insert("x".into())), Outcome::Edited);
        assert_eq!(setup.cards[0].name, "BE", "the cards wait under the picker");
        setup.listed(
            "",
            Ok(vec![entry("EA-BE", true), entry("README.md", false)]),
        );
        let picker = setup.picker.as_ref().unwrap();
        assert_eq!(picker.entries, Some(Ok(vec![entry("EA-BE", true)])));
        assert_eq!(
            setup.browse("EA-BE/api".into()).as_deref(),
            Some("EA-BE/api")
        );
        setup.listed("", Ok(Vec::new()));
        assert_eq!(
            setup.picker.as_ref().unwrap().entries,
            None,
            "a late answer"
        );
        assert_eq!(setup.browse_up().as_deref(), Some("EA-BE"));
        assert_eq!(setup.browse_up().as_deref(), Some(""));
        assert_eq!(setup.browse_up(), None, "never above the workspace");
        setup.browse("EA-BE".into());
        assert_eq!(setup.key(DialogKey::Enter), Outcome::Edited);
        assert_eq!(
            (setup.cards[0].dir.as_str(), setup.picker.is_none()),
            ("EA-BE", true)
        );
        assert_eq!(setup.focus, Focus::Field(0, Field::Dir));

        setup.open_picker(0, Field::Dir);
        setup.browse_up();
        setup.key(DialogKey::Save);
        assert_eq!(setup.cards[0].dir, ".", "the root is \".\"");
        setup.open_picker(0, Field::Dir);
        assert_eq!(
            setup.key(DialogKey::Cancel),
            Outcome::Edited,
            "Esc only closes the picker"
        );
        assert!(setup.picker.is_none());
    }

    #[test]
    fn the_env_file_picker_starts_in_the_cards_folder_and_can_leave_it() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.cards[0].dir = r".worktrees\fix\EA-BE\".into();
        assert_eq!(
            setup.open_picker(0, Field::EnvFile).as_deref(),
            Some(".worktrees/fix/EA-BE")
        );
        assert_eq!(setup.browse_up().as_deref(), Some(".worktrees/fix"));
        assert_eq!(setup.browse_up().as_deref(), Some(".worktrees"));
        assert_eq!(setup.browse_up().as_deref(), Some(""));
        assert_eq!(setup.browse_up(), None, "never above the workspace");
        setup.browse("EA-BE".into());
        setup.key(DialogKey::Enter);
        assert!(setup.picker.is_some(), "Enter picks only folders");
        setup.pick(Some("EA-BE/.env"));
        assert_eq!(setup.cards[0].env_file, "EA-BE/.env");
        assert_eq!(
            setup.cards[0].dir, r".worktrees\fix\EA-BE\",
            "the folder stays"
        );
        assert_eq!(setup.focus, Focus::Field(0, Field::EnvFile));

        // A folder outside the workspace: the picker starts at the root.
        setup.cards[0].dir = "../other".into();
        assert_eq!(setup.open_picker(0, Field::EnvFile).as_deref(), Some(""));
        assert_eq!(setup.error, None);
    }

    #[test]
    fn the_last_open_card_stays_shown_while_the_keyboard_is_on_autostart() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.add_card();
        setup.tab(true);
        setup.tab(true);
        setup.tab(true);
        setup.tab(true);
        setup.tab(true);
        setup.tab(true);
        assert_eq!(setup.focus, Focus::Autostart);
        assert_eq!(setup.focused_card(), None);
        assert_eq!(
            setup.shown_card(),
            Some(1),
            "the last card the keyboard was in"
        );
        setup.tab(true);
        setup.tab(false);
        assert_eq!(setup.focus, Focus::Autostart);
        assert_eq!(setup.shown_card(), Some(0), "its buttons stay Tab stops");
        setup.remove_card(1);
        setup.remove_card(0);
        assert_eq!(setup.shown_card(), None, "no card left to show");
    }

    #[test]
    fn a_clicked_card_is_the_one_shown_once_tab_reaches_autostart() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.add_card();
        setup.set_focus(Focus::Field(0, Field::Name));
        // A click on the folded last card (what the card's click handler does).
        setup.set_focus(Focus::Field(1, Field::Command));
        // Tab itself calls `set_focus` for every field it passes, so check the click alone.
        setup.focus = Focus::Autostart;
        assert_eq!(
            setup.shown_card(),
            Some(1),
            "the click made card 1 the open one"
        );
    }

    #[test]
    fn shift_tab_walks_back_and_the_ends_leave_the_fields() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.add_card();
        setup.focus = Focus::Field(0, Field::Name);
        assert!(
            setup.at_edge(false),
            "Shift+Tab from the first field leaves"
        );
        assert!(!setup.at_edge(true));
        setup.focus = Focus::Field(1, Field::Name);
        setup.tab(false);
        assert_eq!(setup.focus, Focus::Exit(0));
        setup.tab(false);
        assert_eq!(setup.focus, Focus::Field(0, Field::Env));
        setup.focus = Focus::Autostart;
        assert!(setup.at_edge(true), "Tab from the last stop leaves");
        assert!(!setup.at_edge(false));
        setup.tab(false);
        assert_eq!(setup.focus, Focus::Exit(1));
        setup.enter(true);
        assert_eq!(
            setup.focus,
            Focus::Field(0, Field::Name),
            "Tab from the last button"
        );
        setup.enter(false);
        assert_eq!(
            setup.focus,
            Focus::Autostart,
            "Shift+Tab from the first button"
        );
        // The picker has no fields: Tab always goes on to its buttons.
        setup.open_picker(0, Field::Dir);
        assert!(setup.at_edge(true) && setup.at_edge(false));
    }

    #[test]
    fn the_picker_cursor_walks_the_entries_and_enter_takes_the_one_under_it() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.open_picker(0, Field::Dir);
        setup.listed("", Ok(vec![entry("EA-BE", true), entry("EA-FE", true)]));
        let cursor = |setup: &RunSetup| setup.picker.as_ref().unwrap().cursor;
        assert_eq!(cursor(&setup), None);
        for _ in 0..3 {
            setup.key(moved(Motion::Down, false));
        }
        assert_eq!(cursor(&setup), Some(1), "clamped to the last entry");
        setup.key(moved(Motion::Up, false));
        assert_eq!(cursor(&setup), Some(0));
        // → opens the folder under the cursor; the new listing has no cursor yet.
        assert_eq!(
            setup.key(moved(Motion::Right, false)),
            Outcome::List("EA-BE".into())
        );
        assert_eq!(cursor(&setup), None);
        setup.listed("EA-BE", Ok(vec![entry("EA-BE/api", true)]));
        // ← goes to the parent, and does nothing at the workspace root.
        assert_eq!(
            setup.key(moved(Motion::Left, false)),
            Outcome::List(String::new())
        );
        assert_eq!(setup.key(moved(Motion::Left, false)), Outcome::Edited);
        // Enter with the cursor on a folder takes that folder, not the listed one.
        setup.listed("", Ok(vec![entry("EA-FE", true)]));
        setup.key(moved(Motion::Down, false));
        assert_eq!(setup.key(DialogKey::Enter), Outcome::Edited);
        assert_eq!(setup.cards[0].dir, "EA-FE");
        assert!(setup.picker.is_none());
    }

    #[test]
    fn in_the_env_file_picker_enter_opens_a_folder_and_picks_a_file() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        // The picker starts in the card's folder.
        setup.open_picker(0, Field::EnvFile);
        setup.listed(
            "EA-BE",
            Ok(vec![entry("EA-BE/api", true), entry("EA-BE/.env", false)]),
        );
        setup.key(moved(Motion::Down, false));
        assert_eq!(
            setup.key(DialogKey::Enter),
            Outcome::List("EA-BE/api".into()),
            "Enter on a folder opens it"
        );
        assert!(setup.picker.is_some());
        setup.listed("EA-BE/api", Ok(vec![entry("EA-BE/api/.env", false)]));
        setup.key(moved(Motion::Down, false));
        assert_eq!(setup.key(DialogKey::Enter), Outcome::Edited);
        assert_eq!(setup.cards[0].env_file, "EA-BE/api/.env");
        assert!(setup.picker.is_none());
        assert_eq!(setup.focus, Focus::Field(0, Field::EnvFile));
    }
}
