//! Keyboard shortcuts (SPEC §13): the commands, their defaults and `<config>/mkrew/keymap.toml`.
//! Pure, so it is unit-tested; `main.rs` turns the bindings into GPUI key bindings.
//!
//! The file maps command names to keystrokes in GPUI syntax, e.g.
//! `close_pane = "ctrl-shift-q"`. An unknown command, an invalid keystroke or a keystroke
//! already bound to another command is reported and that entry keeps its default.
//!
//! While a terminal has focus only bound keystrokes are intercepted; everything else goes to
//! the session, so the defaults avoid plain `Ctrl+<letter>` (Ctrl+C, Ctrl+R… reach the agent).

use std::path::Path;

use gpui::Keystroke;

use crate::split::Direction;

/// Something a shortcut does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    AddWorkspace,
    /// Jump to workspace 1–9 (sidebar order).
    Workspace(u8),
    NextWorkspace,
    PreviousWorkspace,
    NewShellTab,
    NewAgentTab,
    NextTab,
    PreviousTab,
    /// Jump to tab 1–9.
    Tab(u8),
    /// Edit the active tab's title inline.
    RenameTab,
    CloseTab,
    SplitRight,
    SplitDown,
    Focus(Direction),
    /// Move the divider next to the focused pane.
    Resize(Direction),
    /// Toggle the focused pane filling the whole tab.
    ZoomPane,
    ClosePane,
    /// Terminal only: copy the mouse selection.
    Copy,
    /// Terminal only: paste the clipboard (bracketed when the program asked for it).
    Paste,
    /// Terminal only: one screen back into / forward through the UI-side scrollback.
    PageUp,
    PageDown,
    /// Terminal only: open (or refocus) the find bar over the scrollback.
    Find,
    /// Open the active workspace in Zed.
    OpenInZed,
    /// Jump to the next Inbox entry (after the focused session, wrapping).
    NextInbox,
    /// Show the active workspace's task board.
    ShowBoard,
    /// Show the active workspace's terminal tabs.
    ShowTerminals,
    /// Show the active workspace's Markdown docs.
    ShowDocs,
    /// Docs only: switch between the rendered preview and the source with line numbers.
    ToggleDocSource,
    /// Docs only: comment on the selected lines.
    AddComment,
    /// Docs only: send the shown file's open (or selected) comments to an agent session.
    SendComments,
    /// Show the active workspace's file tree and viewer.
    ShowFiles,
    /// Files only: show or hide git-ignored files in the tree.
    ToggleIgnored,
    /// Show the active workspace's Git view.
    ShowGit,
    /// Git only: switch between Working changes and Against base.
    GitToggleMode,
    /// Git only: switch to the commit History and back.
    GitToggleHistory,
    /// Git only: the next / previous hunk of the shown diff.
    GitNextHunk,
    GitPreviousHunk,
    /// Git only: stage / unstage the selected file, or its current hunk.
    GitStage,
    GitUnstage,
    /// Git only: move the keyboard to the commit message box (Working changes).
    GitFocusCommit,
    /// Fuzzy-find a file of the active workspace and open it in the Files view.
    QuickOpen,
    /// Open the new task dialog for the active workspace.
    NewTask,
    /// Board only: run the selected Todo card.
    RunTask,
}

/// Key context of a focused terminal pane (`key_context` on its root element).
pub const TERMINAL_CONTEXT: &str = "Terminal";

/// Key context of the window while a board is shown (so Ctrl+Enter still reaches agents).
pub const BOARD_CONTEXT: &str = "Board";

/// Key context of the window while a Docs view is shown.
pub const DOCS_CONTEXT: &str = "Docs";

/// Key context of the window while a Files view is shown.
pub const FILES_CONTEXT: &str = "Files";

/// Key context of the Git view's changed-file list while it has focus (so its single-key
/// shortcuts never reach a text field).
pub const GIT_CONTEXT: &str = "Git";

/// Comment commands work in the Docs view and in the Git view's list (diff comments).
pub const COMMENTS_CONTEXT: &str = "Docs || Git";

/// The GPUI action every binding dispatches. A terminal pane handles [`Command::Paste`] and
/// propagates the rest to the window shell.
#[derive(Debug, Clone, PartialEq, gpui::Action)]
#[action(namespace = mkrew, no_json)]
pub struct Run(pub Command);

impl Command {
    /// Every command, in the order bindings are listed.
    pub fn all() -> Vec<Command> {
        let mut all = vec![Command::AddWorkspace];
        all.extend((1..=9).map(Command::Workspace));
        all.extend([
            Command::NextWorkspace,
            Command::PreviousWorkspace,
            Command::NewShellTab,
            Command::NewAgentTab,
            Command::NextTab,
            Command::PreviousTab,
        ]);
        all.extend((1..=9).map(Command::Tab));
        all.extend([
            Command::RenameTab,
            Command::CloseTab,
            Command::SplitRight,
            Command::SplitDown,
            Command::Focus(Direction::Left),
            Command::Focus(Direction::Right),
            Command::Focus(Direction::Up),
            Command::Focus(Direction::Down),
            Command::Resize(Direction::Left),
            Command::Resize(Direction::Right),
            Command::Resize(Direction::Up),
            Command::Resize(Direction::Down),
            Command::ZoomPane,
            Command::ClosePane,
            Command::Copy,
            Command::Paste,
            Command::PageUp,
            Command::PageDown,
            Command::Find,
            Command::OpenInZed,
            Command::NextInbox,
            Command::ShowBoard,
            Command::ShowTerminals,
            Command::ShowDocs,
            Command::ToggleDocSource,
            Command::AddComment,
            Command::SendComments,
            Command::ShowFiles,
            Command::ToggleIgnored,
            Command::ShowGit,
            Command::GitToggleMode,
            Command::GitToggleHistory,
            Command::GitNextHunk,
            Command::GitPreviousHunk,
            Command::GitStage,
            Command::GitUnstage,
            Command::GitFocusCommit,
            Command::QuickOpen,
            Command::NewTask,
            Command::RunTask,
        ]);
        all
    }

    /// Name in `keymap.toml`.
    pub fn name(self) -> String {
        match self {
            Command::AddWorkspace => "add_workspace".into(),
            Command::Workspace(n) => format!("workspace_{n}"),
            Command::NextWorkspace => "next_workspace".into(),
            Command::PreviousWorkspace => "previous_workspace".into(),
            Command::NewShellTab => "new_shell_tab".into(),
            Command::NewAgentTab => "new_agent_tab".into(),
            Command::NextTab => "next_tab".into(),
            Command::PreviousTab => "previous_tab".into(),
            Command::Tab(n) => format!("tab_{n}"),
            Command::RenameTab => "rename_tab".into(),
            Command::CloseTab => "close_tab".into(),
            Command::SplitRight => "split_right".into(),
            Command::SplitDown => "split_down".into(),
            Command::Focus(Direction::Left) => "focus_left".into(),
            Command::Focus(Direction::Right) => "focus_right".into(),
            Command::Focus(Direction::Up) => "focus_up".into(),
            Command::Focus(Direction::Down) => "focus_down".into(),
            Command::Resize(Direction::Left) => "resize_left".into(),
            Command::Resize(Direction::Right) => "resize_right".into(),
            Command::Resize(Direction::Up) => "resize_up".into(),
            Command::Resize(Direction::Down) => "resize_down".into(),
            Command::ZoomPane => "zoom_pane".into(),
            Command::ClosePane => "close_pane".into(),
            Command::Copy => "copy".into(),
            Command::Paste => "paste".into(),
            Command::PageUp => "page_up".into(),
            Command::PageDown => "page_down".into(),
            Command::Find => "find".into(),
            Command::OpenInZed => "open_in_zed".into(),
            Command::NextInbox => "next_inbox".into(),
            Command::ShowBoard => "show_board".into(),
            Command::ShowTerminals => "show_terminals".into(),
            Command::ShowDocs => "show_docs".into(),
            Command::ToggleDocSource => "toggle_doc_source".into(),
            Command::AddComment => "add_comment".into(),
            Command::SendComments => "send_comments".into(),
            Command::ShowFiles => "show_files".into(),
            Command::ToggleIgnored => "toggle_ignored".into(),
            Command::ShowGit => "show_git".into(),
            Command::GitToggleMode => "git_toggle_mode".into(),
            Command::GitToggleHistory => "git_toggle_history".into(),
            Command::GitNextHunk => "git_next_hunk".into(),
            Command::GitPreviousHunk => "git_previous_hunk".into(),
            Command::GitStage => "git_stage".into(),
            Command::GitUnstage => "git_unstage".into(),
            Command::GitFocusCommit => "git_focus_commit".into(),
            Command::QuickOpen => "quick_open".into(),
            Command::NewTask => "new_task".into(),
            Command::RunTask => "run_task".into(),
        }
    }

    pub fn from_name(name: &str) -> Option<Command> {
        Command::all()
            .into_iter()
            .find(|command| command.name() == name)
    }

    /// SPEC §13 default, in GPUI keystroke syntax.
    pub fn default_keys(self) -> String {
        match self {
            Command::AddWorkspace => "ctrl-shift-k".into(),
            Command::Workspace(n) => format!("alt-{n}"),
            Command::NextWorkspace => "ctrl-alt-down".into(),
            Command::PreviousWorkspace => "ctrl-alt-up".into(),
            Command::NewShellTab => "ctrl-shift-t".into(),
            Command::NewAgentTab => "ctrl-shift-a".into(),
            Command::NextTab => "ctrl-tab".into(),
            Command::PreviousTab => "ctrl-shift-tab".into(),
            Command::Tab(n) => format!("ctrl-alt-{n}"),
            Command::RenameTab => "ctrl-shift-r".into(),
            Command::CloseTab => "ctrl-shift-w".into(),
            Command::SplitRight => "alt-shift-=".into(),
            Command::SplitDown => "alt-shift--".into(),
            Command::Focus(Direction::Left) => "alt-left".into(),
            Command::Focus(Direction::Right) => "alt-right".into(),
            Command::Focus(Direction::Up) => "alt-up".into(),
            Command::Focus(Direction::Down) => "alt-down".into(),
            Command::Resize(Direction::Left) => "alt-shift-left".into(),
            Command::Resize(Direction::Right) => "alt-shift-right".into(),
            Command::Resize(Direction::Up) => "alt-shift-up".into(),
            Command::Resize(Direction::Down) => "alt-shift-down".into(),
            Command::ZoomPane => "ctrl-shift-m".into(),
            Command::ClosePane => "ctrl-shift-x".into(),
            Command::Copy => "ctrl-shift-c".into(),
            Command::Paste => "ctrl-shift-v".into(),
            Command::PageUp => "shift-pageup".into(),
            Command::PageDown => "shift-pagedown".into(),
            Command::Find => "ctrl-shift-f".into(),
            Command::OpenInZed => "ctrl-shift-z".into(),
            Command::NextInbox => "ctrl-shift-i".into(),
            Command::ShowBoard => "ctrl-shift-1".into(),
            Command::ShowTerminals => "ctrl-shift-2".into(),
            Command::ShowDocs => "ctrl-shift-3".into(),
            Command::ToggleDocSource => "ctrl-shift-e".into(),
            Command::AddComment => "ctrl-alt-m".into(),
            Command::SendComments => "ctrl-shift-s".into(),
            Command::ShowFiles => "ctrl-shift-4".into(),
            Command::ToggleIgnored => "ctrl-shift-h".into(),
            Command::ShowGit => "ctrl-shift-5".into(),
            Command::GitToggleMode => "b".into(),
            Command::GitToggleHistory => "h".into(),
            Command::GitNextHunk => "n".into(),
            Command::GitPreviousHunk => "p".into(),
            Command::GitStage => "s".into(),
            Command::GitUnstage => "u".into(),
            Command::GitFocusCommit => "c".into(),
            Command::QuickOpen => "ctrl-shift-o".into(),
            Command::NewTask => "ctrl-shift-n".into(),
            Command::RunTask => "ctrl-enter".into(),
        }
    }

    /// Key context the binding is limited to; `None` = anywhere in the window.
    pub fn context(self) -> Option<&'static str> {
        match self {
            Command::Copy
            | Command::Paste
            | Command::PageUp
            | Command::PageDown
            | Command::Find => Some(TERMINAL_CONTEXT),
            Command::RunTask => Some(BOARD_CONTEXT),
            Command::ToggleDocSource => Some(DOCS_CONTEXT),
            Command::AddComment | Command::SendComments => Some(COMMENTS_CONTEXT),
            Command::ToggleIgnored => Some(FILES_CONTEXT),
            Command::GitToggleMode
            | Command::GitToggleHistory
            | Command::GitNextHunk
            | Command::GitPreviousHunk
            | Command::GitStage
            | Command::GitUnstage
            | Command::GitFocusCommit => Some(GIT_CONTEXT),
            _ => None,
        }
    }
}

/// A `keymap.toml` problem; the affected entry (or the whole file) uses the defaults.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeymapError {
    #[error("cannot read {path}: {message}")]
    Read { path: String, message: String },
    #[error("keymap.toml is not valid TOML: {0}")]
    Syntax(String),
    #[error("unknown command `{0}` in keymap.toml")]
    UnknownCommand(String),
    #[error("`{command}` must be a keystroke string like \"ctrl-shift-k\"")]
    NotAString { command: String },
    #[error("`{command} = \"{keys}\"` is not a valid keystroke")]
    InvalidKeys { command: String, keys: String },
    #[error("`{command} = \"{keys}\"` is already bound to `{other}`")]
    Duplicate {
        command: String,
        keys: String,
        other: String,
    },
}

/// The effective binding of every command.
#[derive(Debug, Clone)]
pub struct Keymap {
    bindings: Vec<(Command, Keystroke)>,
}

impl Keymap {
    /// The SPEC §13 defaults.
    pub fn defaults() -> Keymap {
        let bindings = Command::all()
            .into_iter()
            .filter_map(|command| Some((command, Keystroke::parse(&command.default_keys()).ok()?)))
            .collect();
        Keymap { bindings }
    }

    /// Reads `path`; a missing file means the defaults, without error.
    pub fn load(path: &Path) -> (Keymap, Vec<KeymapError>) {
        match std::fs::read_to_string(path) {
            Ok(text) => Keymap::parse(&text),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (Keymap::defaults(), Vec::new())
            }
            Err(error) => (
                Keymap::defaults(),
                vec![KeymapError::Read {
                    path: path.display().to_string(),
                    message: error.to_string(),
                }],
            ),
        }
    }

    /// The defaults overridden by the entries of a `keymap.toml`.
    pub fn parse(text: &str) -> (Keymap, Vec<KeymapError>) {
        let mut keymap = Keymap::defaults();
        let table = match text.parse::<toml::Table>() {
            Ok(table) => table,
            Err(error) => return (keymap, vec![KeymapError::Syntax(error.message().into())]),
        };
        let mut errors = Vec::new();
        // Applied all at once, so two commands can swap keys.
        let mut overrides: Vec<(Command, String)> = Vec::new();
        for (name, value) in table {
            let Some(command) = Command::from_name(&name) else {
                errors.push(KeymapError::UnknownCommand(name));
                continue;
            };
            let Some(keys) = value.as_str() else {
                errors.push(KeymapError::NotAString { command: name });
                continue;
            };
            let Some(keystroke) = parse_keys(keys) else {
                errors.push(KeymapError::InvalidKeys {
                    command: name,
                    keys: keys.into(),
                });
                continue;
            };
            keymap.set(command, keystroke);
            overrides.push((command, keys.into()));
        }
        // A keystroke bound twice: the override falls back to its default. Defaults are
        // unique, so each round reverts one override and the loop ends.
        while let Some((index, other)) = overrides.iter().enumerate().find_map(|(i, (c, _))| {
            let keystroke = keymap.keystroke(*c)?;
            let other = keymap
                .bindings
                .iter()
                .find(|(o, k)| o != c && same_keys(k, keystroke))?;
            Some((i, other.0))
        }) {
            let (command, keys) = overrides.remove(index);
            if let Ok(default) = Keystroke::parse(&command.default_keys()) {
                keymap.set(command, default);
            }
            errors.push(KeymapError::Duplicate {
                command: command.name(),
                keys,
                other: other.name(),
            });
        }
        (keymap, errors)
    }

    fn keystroke(&self, command: Command) -> Option<&Keystroke> {
        self.bindings
            .iter()
            .find(|(c, _)| *c == command)
            .map(|(_, k)| k)
    }

    fn set(&mut self, command: Command, keystroke: Keystroke) {
        match self.bindings.iter_mut().find(|(c, _)| *c == command) {
            Some(binding) => binding.1 = keystroke,
            None => self.bindings.push((command, keystroke)),
        }
    }

    /// Every binding, in [`Command::all`] order.
    pub fn bindings(&self) -> &[(Command, Keystroke)] {
        &self.bindings
    }

    /// The command `keystroke` triggers, if any (ignoring the typed character).
    pub fn command_for(&self, keystroke: &Keystroke) -> Option<Command> {
        self.bindings
            .iter()
            .find(|(_, bound)| same_keys(bound, keystroke))
            .map(|(command, _)| *command)
    }

    /// GPUI key bindings for [`Keymap::bindings`].
    pub fn key_bindings(&self) -> Vec<gpui::KeyBinding> {
        self.bindings
            .iter()
            .map(|(command, keystroke)| {
                // `KeyBinding::new` panics on an unparsable keystroke or context; neither can
                // happen: the keystroke was parsed and is unparsed back, the context is a
                // plain identifier or `COMMENTS_CONTEXT` (parsed in the tests).
                gpui::KeyBinding::new(&keystroke.unparse(), Run(*command), command.context())
            })
            .collect()
    }

    /// Shortcut text for tooltips and menus, e.g. "Ctrl+Shift+X"; empty if unbound.
    pub fn label(&self, command: Command) -> String {
        self.bindings
            .iter()
            .find(|(c, _)| *c == command)
            .map(|(_, keystroke)| label(keystroke))
            .unwrap_or_default()
    }
}

/// One keystroke (no chords). `Keystroke::parse` accepts an empty key, spaces and a lone
/// modifier as the key, so those are refused here.
fn parse_keys(keys: &str) -> Option<Keystroke> {
    if keys.contains(char::is_whitespace) {
        return None;
    }
    let keystroke = Keystroke::parse(keys).ok()?;
    let modifier_only = matches!(
        keystroke.key.as_str(),
        "" | "shift" | "control" | "alt" | "platform" | "function"
    );
    (!modifier_only).then_some(keystroke)
}

/// Same modifiers and key (the typed character does not matter).
fn same_keys(a: &Keystroke, b: &Keystroke) -> bool {
    a.modifiers == b.modifiers && a.key.eq_ignore_ascii_case(&b.key)
}

fn label(keystroke: &Keystroke) -> String {
    let m = keystroke.modifiers;
    let mut parts: Vec<String> = Vec::new();
    for (on, name) in [
        (m.control, "Ctrl"),
        (m.alt, "Alt"),
        (m.shift, "Shift"),
        (m.platform, "Win"),
    ] {
        if on {
            parts.push(name.into());
        }
    }
    let key = match keystroke.key.as_str() {
        "left" => "←".to_owned(),
        "right" => "→".to_owned(),
        "up" => "↑".to_owned(),
        "down" => "↓".to_owned(),
        "pageup" => "PgUp".to_owned(),
        "pagedown" => "PgDn".to_owned(),
        key => {
            let mut chars = key.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
    };
    parts.push(key);
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::input::{InputMode, key_to_bytes};

    fn keystroke(keys: &str) -> Keystroke {
        Keystroke::parse(keys).unwrap()
    }

    #[test]
    fn every_default_parses_and_is_unique() {
        let keymap = Keymap::defaults();
        assert_eq!(keymap.bindings().len(), Command::all().len());
        for (command, keys) in keymap.bindings() {
            assert_eq!(keymap.command_for(keys), Some(*command), "{command:?}");
        }
    }

    #[test]
    fn names_round_trip() {
        for command in Command::all() {
            assert_eq!(Command::from_name(&command.name()), Some(command));
        }
        assert_eq!(Command::from_name("nope"), None);
    }

    #[test]
    fn spec_defaults() {
        let keymap = Keymap::defaults();
        let bound = |keys| keymap.command_for(&keystroke(keys));
        assert_eq!(bound("ctrl-shift-k"), Some(Command::AddWorkspace));
        assert_eq!(bound("alt-3"), Some(Command::Workspace(3)));
        assert_eq!(bound("ctrl-alt-9"), Some(Command::Tab(9)));
        assert_eq!(bound("ctrl-tab"), Some(Command::NextTab));
        assert_eq!(bound("ctrl-shift-tab"), Some(Command::PreviousTab));
        assert_eq!(bound("alt-left"), Some(Command::Focus(Direction::Left)));
        assert_eq!(bound("alt-shift--"), Some(Command::SplitDown));
        assert_eq!(bound("ctrl-shift-x"), Some(Command::ClosePane));
        assert_eq!(bound("ctrl-shift-c"), Some(Command::Copy));
        assert_eq!(bound("shift-pageup"), Some(Command::PageUp));
        assert_eq!(bound("shift-pagedown"), Some(Command::PageDown));
        assert_eq!(bound("ctrl-shift-f"), Some(Command::Find));
        assert_eq!(bound("ctrl-shift-r"), Some(Command::RenameTab));
        assert_eq!(bound("ctrl-shift-m"), Some(Command::ZoomPane));
        assert_eq!(bound("ctrl-shift-z"), Some(Command::OpenInZed));
        assert_eq!(bound("ctrl-shift-i"), Some(Command::NextInbox));
        assert_eq!(bound("ctrl-shift-1"), Some(Command::ShowBoard));
        assert_eq!(bound("ctrl-shift-2"), Some(Command::ShowTerminals));
        assert_eq!(bound("ctrl-shift-3"), Some(Command::ShowDocs));
        assert_eq!(bound("ctrl-shift-e"), Some(Command::ToggleDocSource));
        assert_eq!(bound("ctrl-alt-m"), Some(Command::AddComment));
        assert_eq!(bound("ctrl-shift-s"), Some(Command::SendComments));
        assert_eq!(bound("ctrl-shift-4"), Some(Command::ShowFiles));
        assert_eq!(bound("ctrl-shift-h"), Some(Command::ToggleIgnored));
        assert_eq!(bound("ctrl-shift-5"), Some(Command::ShowGit));
        assert_eq!(bound("ctrl-shift-o"), Some(Command::QuickOpen));
        assert_eq!(bound("ctrl-shift-n"), Some(Command::NewTask));
        assert_eq!(bound("ctrl-enter"), Some(Command::RunTask));
        assert_eq!(
            bound("alt-shift-left"),
            Some(Command::Resize(Direction::Left))
        );
        assert_eq!(
            bound("alt-shift-down"),
            Some(Command::Resize(Direction::Down))
        );
        for command in [
            Command::Copy,
            Command::Paste,
            Command::PageUp,
            Command::PageDown,
            Command::Find,
        ] {
            assert_eq!(command.context(), Some(TERMINAL_CONTEXT), "{command:?}");
        }
        assert_eq!(Command::ClosePane.context(), None);
        // Ctrl+Enter only runs a card while the board is shown; terminals still get it.
        assert_eq!(Command::RunTask.context(), Some(BOARD_CONTEXT));
        // Ctrl+Shift+E only toggles while Docs are shown; terminals still get it.
        assert_eq!(Command::ToggleDocSource.context(), Some(DOCS_CONTEXT));
        // Comments: in Docs, and on diff lines while the Git list has focus.
        for command in [Command::AddComment, Command::SendComments] {
            assert_eq!(command.context(), Some(COMMENTS_CONTEXT), "{command:?}");
            let predicate = gpui::KeyBindingContextPredicate::parse(COMMENTS_CONTEXT).unwrap();
            for (context, matches) in [(DOCS_CONTEXT, true), (GIT_CONTEXT, true), ("Files", false)]
            {
                let stack = [gpui::KeyContext::parse(context).unwrap()];
                assert_eq!(predicate.eval(&stack), matches, "{context}");
            }
        }
        // Ctrl+Shift+H only toggles ignored files while Files are shown.
        assert_eq!(Command::ToggleIgnored.context(), Some(FILES_CONTEXT));
        // SPEC §13 Git single keys, only while the Git list has focus.
        for (keys, command) in [
            ("b", Command::GitToggleMode),
            ("h", Command::GitToggleHistory),
            ("n", Command::GitNextHunk),
            ("p", Command::GitPreviousHunk),
            ("s", Command::GitStage),
            ("u", Command::GitUnstage),
            ("c", Command::GitFocusCommit),
        ] {
            assert_eq!(bound(keys), Some(command), "{keys}");
            assert_eq!(command.context(), Some(GIT_CONTEXT), "{command:?}");
        }
        assert_eq!(keymap.label(Command::GitNextHunk), "N");
    }

    /// SPEC §13: unbound keys reach the agent — Ctrl+C, Ctrl+R and Esc are never intercepted
    /// and encode to the bytes a terminal sends.
    #[test]
    fn agent_keys_are_unbound_and_reach_the_pty() {
        let keymap = Keymap::defaults();
        let mode = InputMode::default();
        for (keys, bytes) in [
            ("ctrl-c", vec![0x03]),
            ("ctrl-r", vec![0x12]),
            ("escape", vec![0x1b]),
        ] {
            let keystroke = keystroke(keys);
            assert_eq!(keymap.command_for(&keystroke), None, "{keys} is bound");
            assert_eq!(key_to_bytes(&keystroke, mode), Some(bytes), "{keys}");
        }
        // No default is a plain Ctrl+<letter>.
        for (command, keys) in keymap.bindings() {
            let m = keys.modifiers;
            let plain_ctrl = m.control && !m.shift && !m.alt && keys.key.len() == 1;
            assert!(
                !plain_ctrl || !keys.key.chars().all(|c| c.is_ascii_alphabetic()),
                "{command:?}"
            );
        }
    }

    #[test]
    fn file_overrides_a_binding() {
        let (keymap, errors) = Keymap::parse("close_pane = \"ctrl-shift-q\"\n");
        assert_eq!(errors, []);
        assert_eq!(
            keymap.command_for(&keystroke("ctrl-shift-q")),
            Some(Command::ClosePane)
        );
        assert_eq!(keymap.command_for(&keystroke("ctrl-shift-x")), None);
        assert_eq!(keymap.label(Command::ClosePane), "Ctrl+Shift+Q");
    }

    #[test]
    fn bad_entries_keep_their_defaults() {
        let text = "nope = \"ctrl-1\"\nclose_tab = 5\nsplit_right = \"hyper-k\"\n\
                    split_down = \"\"\nnext_tab = \"ctrl-k ctrl-n\"\n\
                    close_pane = \"ctrl-shift-k\"\n";
        let (keymap, errors) = Keymap::parse(text);
        assert_eq!(errors.len(), 6, "{errors:?}");
        assert!(errors.contains(&KeymapError::UnknownCommand("nope".into())));
        assert!(errors.contains(&KeymapError::NotAString {
            command: "close_tab".into()
        }));
        assert!(errors.contains(&KeymapError::Duplicate {
            command: "close_pane".into(),
            keys: "ctrl-shift-k".into(),
            other: "add_workspace".into(),
        }));
        for name in ["split_right", "split_down", "next_tab"] {
            assert!(
                errors.iter().any(
                    |e| matches!(e, KeymapError::InvalidKeys { command, .. } if command == name)
                ),
                "{name}"
            );
        }
        let defaults = Keymap::defaults();
        for command in [
            Command::CloseTab,
            Command::SplitRight,
            Command::SplitDown,
            Command::NextTab,
            Command::ClosePane,
        ] {
            assert_eq!(keymap.label(command), defaults.label(command));
        }
    }

    #[test]
    fn two_commands_can_swap_keys() {
        let text = "add_workspace = \"ctrl-shift-x\"\nclose_pane = \"ctrl-shift-k\"\n";
        let (keymap, errors) = Keymap::parse(text);
        assert_eq!(errors, []);
        assert_eq!(keymap.label(Command::AddWorkspace), "Ctrl+Shift+X");
        assert_eq!(keymap.label(Command::ClosePane), "Ctrl+Shift+K");
    }

    #[test]
    fn modifier_only_keys_are_invalid() {
        for keys in ["shift", "ctrl-", "ctrl-alt"] {
            let (_, errors) = Keymap::parse(&format!("close_pane = \"{keys}\"\n"));
            assert!(
                matches!(errors.as_slice(), [KeymapError::InvalidKeys { .. }]),
                "{keys}: {errors:?}"
            );
        }
    }

    #[test]
    fn broken_file_means_defaults() {
        let (keymap, errors) = Keymap::parse("close_pane = ");
        assert!(matches!(errors.as_slice(), [KeymapError::Syntax(_)]));
        assert_eq!(keymap.label(Command::ClosePane), "Ctrl+Shift+X");
    }

    #[test]
    fn missing_file_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let (keymap, errors) = Keymap::load(&dir.path().join("keymap.toml"));
        assert_eq!(errors, []);
        assert_eq!(keymap.bindings().len(), Command::all().len());
    }

    #[test]
    fn every_binding_becomes_a_gpui_key_binding() {
        let (keymap, _) = Keymap::parse("split_down = \"ctrl-alt--\"\n");
        assert_eq!(keymap.key_bindings().len(), Command::all().len());
    }

    #[test]
    fn labels() {
        let keymap = Keymap::defaults();
        assert_eq!(keymap.label(Command::AddWorkspace), "Ctrl+Shift+K");
        assert_eq!(keymap.label(Command::Focus(Direction::Up)), "Alt+↑");
        assert_eq!(keymap.label(Command::NextTab), "Ctrl+Tab");
        assert_eq!(keymap.label(Command::SplitRight), "Alt+Shift+=");
        assert_eq!(keymap.label(Command::PageUp), "Shift+PgUp");
    }
}
