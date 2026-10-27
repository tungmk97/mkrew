//! The Settings dialog (SPEC §20.0, §13, ADR 0073): pages of `settings.toml` rows built from
//! `mkrew_core::settings_fields::SETTINGS`, and a Keyboard page of every command. Pure: the
//! shell turns an [`Effect`] into a file write and feeds the result back.

use std::collections::HashMap;

use gpui::Keystroke;
use mkrew_core::settings::Settings;
use mkrew_core::settings_fields::{Kind, Page, SETTINGS, Setting};

use crate::editor::Motion;
use crate::keymap::{Command, Keymap, KeymapError, keys_label};
use crate::palette;
use crate::task_dialog::{DialogKey, dialog_key};
use crate::text_box::Input;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Settings(Page),
    Keyboard,
}

impl Tab {
    pub fn all() -> Vec<Tab> {
        let mut all: Vec<Tab> = Page::ALL.into_iter().map(Tab::Settings).collect();
        all.push(Tab::Keyboard);
        all
    }

    pub fn title(self) -> &'static str {
        match self {
            Tab::Settings(page) => page.title(),
            Tab::Keyboard => "Keyboard",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Setting(&'static Setting),
    Binding(Command),
}

impl Row {
    /// Key of the row's error.
    pub fn id(&self) -> String {
        match self {
            Row::Setting(setting) => setting.id(),
            Row::Binding(command) => command.name(),
        }
    }

    /// A click on the row only moves the keyboard to it: a text row waits for typing, a choice
    /// row changes through its own segments (Enter / Space cycle it).
    fn click_focuses_only(&self) -> bool {
        matches!(
            self,
            Row::Setting(s) if matches!(s.kind, Kind::Text | Kind::Number | Kind::Choice(_))
        )
    }
}

/// Whether `keystroke` may become `command`'s binding. A command that works everywhere would
/// take a plain key (Enter, Tab, a letter) from the terminal and every text field, so it needs
/// Ctrl, Alt or Win, or a function key; a command of one view (Git, Files, Board, Docs) takes
/// any key, like its defaults.
fn bindable(command: Command, keystroke: &Keystroke) -> bool {
    let m = keystroke.modifiers;
    let function = keystroke
        .key
        .strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n));
    m.control
        || m.alt
        || m.platform
        || function
        || command
            .context()
            .is_some_and(|context| !context.contains("Terminal"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The page list on the left.
    Tabs,
    /// The Keyboard page's search box.
    Search,
    Row(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListPick {
    pub setting: &'static Setting,
    pub query: Input,
    pub selected: usize,
}

/// What the shell does after a key or click.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    None,
    Close,
    Write {
        setting: &'static Setting,
        text: String,
    },
    /// `None` keys: back to the default.
    Bind {
        command: Command,
        keys: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsDialog {
    pub tab: Tab,
    pub focus: Focus,
    /// `Err`: the file does not parse; the settings pages are read-only.
    pub settings: Result<Settings, String>,
    /// Installed font families, for the font pick.
    pub fonts: Vec<String>,
    /// The focused text or number row's text while it has the keyboard.
    pub draft: Option<(&'static Setting, Input)>,
    /// Per row id: why its last write failed.
    pub errors: HashMap<String, String>,
    pub search: Input,
    /// The command waiting for its new keystroke.
    pub capture: Option<Command>,
    pub list_pick: Option<ListPick>,
}

impl SettingsDialog {
    pub fn new(settings: Result<Settings, String>, fonts: Vec<String>) -> Self {
        Self {
            tab: Tab::Settings(Page::General),
            focus: Focus::Tabs,
            settings,
            fonts,
            draft: None,
            errors: HashMap::new(),
            search: Input::default(),
            capture: None,
            list_pick: None,
        }
    }

    pub fn read_only(&self) -> bool {
        self.settings.is_err()
    }

    pub fn rows(&self, keymap: &Keymap) -> Vec<Row> {
        match self.tab {
            Tab::Settings(page) => SETTINGS
                .iter()
                .filter(|s| s.page == page && !s.hidden)
                .map(Row::Setting)
                .collect(),
            Tab::Keyboard => {
                let query = self.search.trim().to_lowercase();
                Command::all()
                    .into_iter()
                    .filter(|&command| {
                        query.is_empty()
                            || palette::title(command).to_lowercase().contains(&query)
                            || keymap.label(command).to_lowercase().contains(&query)
                    })
                    .map(Row::Binding)
                    .collect()
            }
        }
    }

    /// The value the file holds (the defaults when the file is broken).
    fn current(&self, setting: &Setting) -> String {
        match &self.settings {
            Ok(settings) => setting.current(settings),
            Err(_) => setting.current(&Settings::default()),
        }
    }

    /// What the row shows: the draft while it is edited, else the file's value.
    pub fn shown(&self, setting: &Setting) -> String {
        match &self.draft {
            Some((edited, input)) if edited.id() == setting.id() => input.to_string(),
            _ => self.current(setting),
        }
    }

    fn write(&self, setting: &'static Setting, text: String) -> Effect {
        if self.read_only() {
            return Effect::None;
        }
        Effect::Write { setting, text }
    }

    /// Ends the draft; a changed one is written.
    fn commit_draft(&mut self) -> Effect {
        match self.draft.take() {
            Some((setting, input)) if *input != *self.current(setting) => {
                self.write(setting, input.to_string())
            }
            _ => Effect::None,
        }
    }

    /// Moves the keyboard to `focus`, writing a draft it leaves; a text row starts a draft.
    fn move_focus(&mut self, focus: Focus, keymap: &Keymap) -> Effect {
        let effect = self.commit_draft();
        self.capture = None;
        self.list_pick = None;
        self.focus = focus;
        if let Focus::Row(i) = focus
            && let Some(Row::Setting(setting)) = self.rows(keymap).get(i).copied()
            && matches!(setting.kind, Kind::Text | Kind::Number)
        {
            self.draft = Some((setting, Input::from(self.current(setting))));
        }
        effect
    }

    pub fn set_tab(&mut self, tab: Tab) -> Effect {
        let effect = self.commit_draft();
        self.tab = tab;
        self.focus = Focus::Tabs;
        self.capture = None;
        self.list_pick = None;
        effect
    }

    /// Enter / Space / a click on the focused row.
    fn activate(&mut self, keymap: &Keymap) -> Effect {
        let Focus::Row(i) = self.focus else {
            return Effect::None;
        };
        match self.rows(keymap).get(i).copied() {
            Some(Row::Binding(command)) => {
                self.capture = Some(command);
                Effect::None
            }
            Some(Row::Setting(setting)) => match setting.kind {
                Kind::Bool => {
                    let on = self.current(setting) == "true";
                    self.write(setting, (!on).to_string())
                }
                Kind::Choice(choices) => {
                    let now = self.current(setting);
                    let at = choices.iter().position(|c| *c == now).unwrap_or(0);
                    self.write(setting, choices[(at + 1) % choices.len()].to_owned())
                }
                Kind::Font | Kind::List(_) if !self.read_only() => {
                    self.list_pick = Some(ListPick {
                        setting,
                        query: Input::default(),
                        selected: 0,
                    });
                    Effect::None
                }
                Kind::Font | Kind::List(_) => Effect::None,
                // Enter in a text row: write it and keep editing.
                Kind::Text | Kind::Number => {
                    let effect = self.commit_draft();
                    let text = match &effect {
                        Effect::Write { text, .. } => text.clone(),
                        _ => self.current(setting),
                    };
                    self.draft = Some((setting, Input::from(text)));
                    effect
                }
            },
            None => Effect::None,
        }
    }

    /// A click: focus, then act like Enter (a text or choice row only takes the keyboard). A
    /// click that leaves a changed draft writes that draft first; the next click acts.
    pub fn click(&mut self, focus: Focus, keymap: &Keymap) -> Effect {
        let left = self.move_focus(focus, keymap);
        if left != Effect::None {
            return left;
        }
        match focus {
            Focus::Row(i)
                if !self
                    .rows(keymap)
                    .get(i)
                    .is_some_and(Row::click_focuses_only) =>
            {
                self.activate(keymap)
            }
            _ => Effect::None,
        }
    }

    pub fn choose(&mut self, setting: &'static Setting, text: &str) -> Effect {
        self.list_pick = None;
        self.write(setting, text.to_owned())
    }

    pub fn reset(&mut self, command: Command) -> Effect {
        self.capture = None;
        Effect::Bind {
            command,
            keys: None,
        }
    }

    /// The pick's choices matching its query: for a font `""` (Default) first, then the
    /// installed fonts; for a list its names.
    pub fn list_choices(&self) -> Vec<String> {
        let Some(pick) = self.list_pick.as_ref() else {
            return Vec::new();
        };
        let query = pick.query.trim().to_lowercase();
        let matches = |name: &str| name.to_lowercase().contains(&query);
        if let Kind::List(names) = pick.setting.kind {
            return names
                .iter()
                .filter(|name| matches(name))
                .map(|name| (*name).to_owned())
                .collect();
        }
        let mut choices = Vec::new();
        if query.is_empty() {
            choices.push(String::new());
        }
        choices.extend(self.fonts.iter().filter(|font| matches(font)).cloned());
        choices
    }

    fn step_focus(&mut self, back: bool, keymap: &Keymap) -> Effect {
        let mut order = vec![Focus::Tabs];
        if self.tab == Tab::Keyboard {
            order.push(Focus::Search);
        }
        order.extend((0..self.rows(keymap).len()).map(Focus::Row));
        let at = order.iter().position(|f| *f == self.focus).unwrap_or(0);
        let next = if back {
            at.checked_sub(1).unwrap_or(order.len() - 1)
        } else {
            (at + 1) % order.len()
        };
        self.move_focus(order[next], keymap)
    }

    /// Tab (`!back`) / Shift+Tab has no stop left in the page list, search box and rows: the
    /// keyboard leaves them for the dialog's buttons ([`crate::dialog_focus::nav`]). Never while
    /// a shortcut is being captured or a list pick is open: those keep Tab.
    pub fn chain_edge(&self, back: bool, keymap: &Keymap) -> bool {
        if self.capture.is_some() || self.list_pick.is_some() {
            return false;
        }
        if back {
            return self.focus == Focus::Tabs;
        }
        let rows = self.rows(keymap).len();
        match self.focus {
            Focus::Row(i) => i + 1 >= rows,
            Focus::Search => rows == 0,
            Focus::Tabs => self.tab != Tab::Keyboard && rows == 0,
        }
    }

    /// Writes a draft before the keyboard leaves for the buttons.
    pub fn leave(&mut self) -> Effect {
        self.commit_draft()
    }

    /// The keyboard came back from the buttons: Tab lands on the page list, Shift+Tab on the
    /// last row.
    pub fn enter(&mut self, forward: bool, keymap: &Keymap) {
        let last = match self.rows(keymap).len() {
            0 if self.tab == Tab::Keyboard => Focus::Search,
            0 => Focus::Tabs,
            count => Focus::Row(count - 1),
        };
        // `leave` already wrote any draft, so there is no effect to apply.
        let _ = self.move_focus(if forward { Focus::Tabs } else { last }, keymap);
    }

    fn list_key(&mut self, key: DialogKey) -> Effect {
        let choices = self.list_choices();
        let last = choices.len().saturating_sub(1);
        let Some(pick) = self.list_pick.as_mut() else {
            return Effect::None;
        };
        match key {
            DialogKey::Cancel => self.list_pick = None,
            DialogKey::Move {
                motion: Motion::Up, ..
            } => pick.selected = pick.selected.saturating_sub(1),
            DialogKey::Move {
                motion: Motion::Down,
                ..
            } => pick.selected = (pick.selected + 1).min(last),
            DialogKey::Enter | DialogKey::Save => {
                let setting = pick.setting;
                let choice = choices.get(pick.selected).cloned();
                self.list_pick = None;
                return choice.map_or(Effect::None, |font| self.write(setting, font));
            }
            DialogKey::Insert(text) => {
                pick.query.insert(&text);
                pick.selected = 0;
            }
            other => {
                if pick.query.edit(&other) {
                    pick.selected = 0;
                }
            }
        }
        Effect::None
    }

    pub fn key(&mut self, keystroke: &Keystroke, paste: Option<&str>, keymap: &Keymap) -> Effect {
        if let Some(command) = self.capture.take() {
            if keystroke.key == "escape" && keystroke.modifiers == Default::default() {
                return Effect::None;
            }
            if !bindable(command, keystroke) {
                self.errors.insert(
                    command.name(),
                    "Use Ctrl, Alt or Win with the key (or a function key)".to_owned(),
                );
                return Effect::None;
            }
            return Effect::Bind {
                command,
                keys: Some(keystroke.unparse()),
            };
        }
        let Some(key) = dialog_key(keystroke) else {
            return Effect::None;
        };
        if self.list_pick.is_some() {
            return self.list_key(key);
        }
        match key {
            DialogKey::Cancel => {
                if let Some((setting, input)) = &self.draft
                    && **input != *self.current(setting)
                {
                    let setting = *setting;
                    self.draft = Some((setting, Input::from(self.current(setting))));
                    return Effect::None;
                }
                if self.focus == Focus::Search && !self.search.is_empty() {
                    self.search = Input::default();
                    return Effect::None;
                }
                Effect::Close
            }
            DialogKey::SwitchField => self.step_focus(keystroke.modifiers.shift, keymap),
            DialogKey::Move {
                motion: motion @ (Motion::Up | Motion::Down),
                extend: false,
            } => {
                let up = motion == Motion::Up;
                match self.focus {
                    Focus::Tabs => {
                        let tabs = Tab::all();
                        let at = tabs.iter().position(|t| *t == self.tab).unwrap_or(0);
                        let next = if up {
                            at.saturating_sub(1)
                        } else {
                            (at + 1).min(tabs.len() - 1)
                        };
                        self.set_tab(tabs[next])
                    }
                    Focus::Row(i) => {
                        let last = self.rows(keymap).len().saturating_sub(1);
                        let next = if up {
                            i.saturating_sub(1)
                        } else {
                            (i + 1).min(last)
                        };
                        self.move_focus(Focus::Row(next), keymap)
                    }
                    Focus::Search => Effect::None,
                }
            }
            DialogKey::Enter | DialogKey::Save => self.activate(keymap),
            DialogKey::Insert(text)
                if text == " " && self.draft.is_none() && self.focus != Focus::Search =>
            {
                self.activate(keymap)
            }
            DialogKey::Paste => {
                if let Some(text) = paste {
                    self.edit_text(|input| {
                        input.insert(text);
                    });
                }
                Effect::None
            }
            DialogKey::Insert(text) => {
                self.edit_text(|input| {
                    input.insert(&text);
                });
                Effect::None
            }
            other => {
                self.edit_text(|input| {
                    input.edit(&other);
                });
                Effect::None
            }
        }
    }

    /// Applies `edit` to the text that has the keyboard: the search box or the draft.
    fn edit_text(&mut self, edit: impl FnOnce(&mut Input)) {
        if self.focus == Focus::Search {
            edit(&mut self.search);
        } else if let Some((_, input)) = &mut self.draft {
            edit(input);
        }
    }

    /// The text a typed key edits: the font search, the search box or the draft (`None` while
    /// a shortcut is being recorded). The shell copies its selection on Ctrl+C / Ctrl+X.
    pub fn typing(&self) -> Option<&Input> {
        if self.capture.is_some() {
            None
        } else if let Some(pick) = &self.list_pick {
            Some(&pick.query)
        } else if self.focus == Focus::Search {
            Some(&self.search)
        } else {
            self.draft.as_ref().map(|(_, input)| input)
        }
    }

    /// The file changed (a write, a hand edit): new values; the open draft stays.
    pub fn reload(&mut self, settings: Result<Settings, String>) {
        self.settings = settings;
    }

    pub fn written(&mut self, setting: &Setting, result: Result<(), String>) {
        match result {
            Ok(()) => self.errors.remove(&setting.id()),
            Err(error) => self.errors.insert(setting.id(), error),
        };
    }

    pub fn bound(&mut self, command: Command, result: Result<(), String>) {
        match result {
            Ok(()) => self.errors.remove(&command.name()),
            Err(error) => self.errors.insert(command.name(), error),
        };
    }
}

/// A refused binding, for its row.
pub fn binding_error(error: &KeymapError) -> String {
    match error {
        KeymapError::Duplicate { keys, other, .. } => match Command::from_name(other) {
            Some(other) => format!(
                "{} is used by \"{}\"",
                keys_label(keys),
                palette::title(other)
            ),
            None => error.to_string(),
        },
        KeymapError::InvalidKeys { keys, .. } => format!("{keys} cannot be a shortcut"),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_core::settings::Settings;

    fn key(keys: &str) -> Keystroke {
        Keystroke::parse(keys).unwrap()
    }

    /// A key that types `text` (Space types " ").
    fn typed(text: &str) -> Keystroke {
        let mut keystroke = key(if text == " " { "space" } else { text });
        keystroke.key_char = Some(text.into());
        keystroke
    }

    fn dialog() -> SettingsDialog {
        SettingsDialog::new(
            Ok(Settings::default()),
            vec!["Inter".into(), "JetBrains Mono".into()],
        )
    }

    fn setting(id: &str) -> &'static Setting {
        SETTINGS.iter().find(|s| s.id() == id).unwrap()
    }

    /// Puts the keyboard on the row of `id` with Tab / arrows only (no click: a click on a
    /// switch would toggle it).
    fn focus_on(d: &mut SettingsDialog, id: &str, keymap: &Keymap) {
        d.set_tab(Tab::Settings(setting(id).page));
        let index = d
            .rows(keymap)
            .iter()
            .position(|row| matches!(row, Row::Setting(s) if s.id() == id))
            .unwrap();
        d.key(&key("tab"), None, keymap);
        for _ in 0..index {
            d.key(&key("down"), None, keymap);
        }
        assert_eq!(d.focus, Focus::Row(index));
    }

    #[test]
    fn esc_cancels_capture_then_draft_then_closes() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.set_tab(Tab::Keyboard);
        assert_eq!(d.click(Focus::Row(0), &keymap), Effect::None);
        assert_eq!(d.capture, Some(Command::AddWorkspace));
        assert_eq!(d.key(&key("escape"), None, &keymap), Effect::None);
        assert_eq!(d.capture, None);
        focus_on(&mut d, "composer.file_ref", &keymap);
        d.key(&typed("x"), None, &keymap);
        assert_eq!(d.shown(setting("composer.file_ref")), "@{path}x");
        assert_eq!(d.key(&key("escape"), None, &keymap), Effect::None);
        assert_eq!(d.shown(setting("composer.file_ref")), "@{path}");
        assert_eq!(d.key(&key("escape"), None, &keymap), Effect::Close);
    }

    #[test]
    fn a_captured_keystroke_becomes_a_binding() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.set_tab(Tab::Keyboard);
        d.click(Focus::Row(0), &keymap);
        assert_eq!(
            d.key(&key("ctrl-shift-y"), None, &keymap),
            Effect::Bind {
                command: Command::AddWorkspace,
                keys: Some("ctrl-shift-y".into())
            }
        );
        assert_eq!(d.capture, None);
    }

    #[test]
    fn enter_writes_a_changed_draft_and_leaving_the_row_does_too() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        focus_on(&mut d, "ui.font_size", &keymap);
        d.key(&key("backspace"), None, &keymap);
        d.key(&typed("4"), None, &keymap);
        assert_eq!(
            d.key(&key("enter"), None, &keymap),
            Effect::Write {
                setting: setting("ui.font_size"),
                text: "14".into()
            }
        );
        d.key(&key("backspace"), None, &keymap);
        d.key(&typed("5"), None, &keymap);
        assert_eq!(
            d.key(&key("tab"), None, &keymap),
            Effect::Write {
                setting: setting("ui.font_size"),
                text: "15".into()
            }
        );
        // An unchanged draft writes nothing.
        focus_on(&mut d, "ui.font_size", &keymap);
        assert_eq!(d.key(&key("enter"), None, &keymap), Effect::None);
    }

    #[test]
    fn space_toggles_a_bool_and_enter_cycles_a_choice() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        focus_on(&mut d, "ui.diff_wrap", &keymap);
        assert_eq!(
            d.key(&typed(" "), None, &keymap),
            Effect::Write {
                setting: setting("ui.diff_wrap"),
                text: "false".into()
            }
        );
        focus_on(&mut d, "ui.theme", &keymap);
        assert_eq!(
            d.key(&key("enter"), None, &keymap),
            Effect::Write {
                setting: setting("ui.theme"),
                text: "light".into()
            }
        );
    }

    #[test]
    fn a_broken_file_makes_settings_read_only_but_not_keys() {
        let keymap = Keymap::defaults();
        let mut d = SettingsDialog::new(Err("settings.toml:2:1: oops".into()), Vec::new());
        assert!(d.read_only());
        focus_on(&mut d, "ui.diff_wrap", &keymap);
        assert_eq!(d.key(&typed(" "), None, &keymap), Effect::None);
        assert_eq!(d.choose(setting("ui.theme"), "dark"), Effect::None);
        assert_eq!(
            d.reset(Command::ClosePane),
            Effect::Bind {
                command: Command::ClosePane,
                keys: None
            }
        );
    }

    #[test]
    fn reload_keeps_the_open_draft() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        focus_on(&mut d, "composer.file_ref", &keymap);
        d.key(&typed("x"), None, &keymap);
        let mut edited = Settings::default();
        edited.ui.diff_wrap = false;
        d.reload(Ok(edited));
        assert_eq!(d.shown(setting("composer.file_ref")), "@{path}x");
        assert_eq!(d.shown(setting("ui.diff_wrap")), "false");
    }

    #[test]
    fn keyboard_search_filters_by_title_and_by_shortcut() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.set_tab(Tab::Keyboard);
        d.click(Focus::Search, &keymap);
        for c in ["t", "h", "e", "m", "e"] {
            d.key(&typed(c), None, &keymap);
        }
        assert_eq!(
            d.rows(&keymap),
            vec![
                Row::Binding(Command::ToggleTheme),
                Row::Binding(Command::SelectTheme)
            ]
        );
        d.search = Input::from("ctrl+,");
        assert_eq!(d.rows(&keymap), vec![Row::Binding(Command::OpenSettings)]);
    }

    #[test]
    fn the_font_pick_lists_default_first_and_filters() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        focus_on(&mut d, "ui.font_family", &keymap);
        d.key(&key("enter"), None, &keymap);
        assert_eq!(d.list_choices(), vec!["", "Inter", "JetBrains Mono"]);
        for c in ["m", "o", "n"] {
            d.key(&typed(c), None, &keymap);
        }
        assert_eq!(d.list_choices(), vec!["JetBrains Mono"]);
        assert_eq!(
            d.key(&key("enter"), None, &keymap),
            Effect::Write {
                setting: setting("ui.font_family"),
                text: "JetBrains Mono".into()
            }
        );
        assert!(d.list_pick.is_none());
    }

    #[test]
    fn the_theme_pick_lists_the_names_and_filters() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        focus_on(&mut d, "ui.dark_theme", &keymap);
        d.key(&key("enter"), None, &keymap);
        assert_eq!(
            d.list_choices().len(),
            mkrew_core::settings::THEME_NAMES.len(),
            "every name, no Default row"
        );
        for c in ["d", "r", "a", "c"] {
            d.key(&typed(c), None, &keymap);
        }
        assert_eq!(d.list_choices(), vec!["Dracula"]);
        assert_eq!(
            d.key(&key("enter"), None, &keymap),
            Effect::Write {
                setting: setting("ui.dark_theme"),
                text: "Dracula".into()
            }
        );
        assert!(d.list_pick.is_none());
    }

    /// Review finding: a click on a choice row (its label, or the selected segment) moved it
    /// to the next choice; only a click on another segment, Enter or Space changes it.
    #[test]
    fn a_click_on_a_choice_row_only_focuses_it() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.set_tab(Tab::Settings(Page::Appearance));
        let index = d
            .rows(&keymap)
            .iter()
            .position(|row| matches!(row, Row::Setting(s) if s.id() == "ui.theme"))
            .unwrap();
        assert_eq!(d.click(Focus::Row(index), &keymap), Effect::None);
        assert_eq!(d.focus, Focus::Row(index));
    }

    /// Review finding: any key was captured, so Enter or Tab (to move on) became a global
    /// binding that stole that key from the terminal.
    #[test]
    fn a_plain_key_is_not_captured_for_a_global_command() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.set_tab(Tab::Keyboard);
        d.click(Focus::Row(0), &keymap);
        assert_eq!(d.capture, Some(Command::AddWorkspace));
        assert_eq!(d.key(&key("enter"), None, &keymap), Effect::None);
        assert_eq!(d.capture, None);
        assert!(d.errors.contains_key("add_workspace"), "the row says why");
        // A key with Ctrl / Alt / Win, or a function key, is fine.
        d.click(Focus::Row(0), &keymap);
        assert_eq!(
            d.key(&key("f5"), None, &keymap),
            Effect::Bind {
                command: Command::AddWorkspace,
                keys: Some("f5".into())
            }
        );
        // A command that only works in a view (Git here) takes a plain key.
        d.capture = Some(Command::GitStage);
        assert_eq!(
            d.key(&key("x"), None, &keymap),
            Effect::Bind {
                command: Command::GitStage,
                keys: Some("x".into())
            }
        );
    }

    #[test]
    fn a_duplicate_names_the_other_command() {
        let error = KeymapError::Duplicate {
            command: "close_pane".into(),
            keys: "ctrl-shift-k".into(),
            other: "add_workspace".into(),
        };
        assert_eq!(
            binding_error(&error),
            format!(
                "Ctrl+Shift+K is used by \"{}\"",
                palette::title(Command::AddWorkspace)
            )
        );
    }

    #[test]
    fn tab_leaves_the_page_list_and_rows_at_their_ends() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        assert!(
            d.chain_edge(true, &keymap),
            "Shift+Tab from the page list leaves"
        );
        assert!(!d.chain_edge(false, &keymap));
        let last = d.rows(&keymap).len() - 1;
        for _ in 0..=last {
            d.key(&key("tab"), None, &keymap);
        }
        assert_eq!(d.focus, Focus::Row(last));
        assert!(d.chain_edge(false, &keymap), "Tab from the last row leaves");
        assert!(!d.chain_edge(true, &keymap));
        d.enter(true, &keymap);
        assert_eq!(d.focus, Focus::Tabs, "Tab from the last button");
        d.enter(false, &keymap);
        assert_eq!(d.focus, Focus::Row(last), "Shift+Tab from the first button");
        // A shortcut being captured keeps Tab.
        d.set_tab(Tab::Keyboard);
        d.click(Focus::Row(0), &keymap);
        assert!(d.capture.is_some());
        assert!(!d.chain_edge(false, &keymap) && !d.chain_edge(true, &keymap));
    }
}
