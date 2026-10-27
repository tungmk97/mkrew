//! Keyboard shortcuts (SPEC §13): the commands, their defaults and `<config>/mkrew/keymap.toml`.
//! Pure, so it is unit-tested; `main.rs` turns the bindings into GPUI key bindings.
//!
//! The file maps command names to keystrokes in GPUI syntax, e.g.
//! `close_pane = "ctrl-shift-q"`. An unknown command, an invalid keystroke or a keystroke
//! already bound to another command is reported and that entry keeps its default.
//!
//! While a terminal has focus only bound keystrokes are intercepted; everything else goes to
//! the session, so the defaults avoid plain `Ctrl+<letter>` (Ctrl+C, Ctrl+R… reach the agent),
//! except Files-only commands (ADR 0028).

use std::path::Path;

use gpui::Keystroke;

use crate::board::CardKey;
use crate::split::{Direction, Preset};

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
    /// Split the focused pane to the right with the workspace's default agent (+ Agent).
    SplitAgent,
    /// Split the focused pane to the right with the workspace's default shell (+ Shell).
    SplitShell,
    /// Lay the active tab out as a preset (SPEC §10), starting the panes it lacks.
    Layout(Preset),
    /// Give every pane of the active tab an equal share.
    EqualizePanes,
    Focus(Direction),
    /// Move the divider next to the focused pane.
    Resize(Direction),
    /// Toggle the focused pane filling the whole tab.
    ZoomPane,
    ClosePane,
    /// Edit the focused pane's (session's) title inline in its header.
    RenamePane,
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
    /// Jump to the previous Inbox entry (before the focused session, wrapping).
    PreviousInbox,
    /// Peek the first Inbox entry (↑ / ↓ cycle, Esc closes; §20A.1).
    Peek,
    /// Open the prompt composer of the focused agent pane (§20B.1).
    OpenComposer,
    /// Send the picked lines (Files, Git diff) or the terminal selection to an agent's
    /// composer (§20B.4).
    SendSelection,
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
    /// Open the right panel on the file tree and give it the keyboard.
    ShowFileTree,
    /// Show the Editor view (the opened files) while a file is open.
    ShowEditor,
    /// Tree only: show or hide git-ignored files in the tree.
    ToggleIgnored,
    /// Editor only: save the opened file (Ctrl+S, ADR 0028).
    SaveFile,
    /// Editor only: open the find bar over the opened file (Ctrl+F).
    FindInFile,
    /// Editor only: open the find bar with its replace row (Ctrl+H).
    ReplaceInFile,
    /// Editor only: close the active tab, asking about unsaved text (Ctrl+W).
    CloseFileTab,
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
    /// Git only: switch the diff viewer between Unified and Split.
    GitToggleDiffMode,
    /// Git only: mark the selected file as viewed, or clear the mark (SPEC §12B, ADR 0065).
    GitToggleViewed,
    /// Git only: stage every file of the selected file's group (Unstaged + Untracked without
    /// one).
    GitStageAll,
    /// Git only: unstage every Staged file.
    GitUnstageAll,
    /// Git only: open the discard confirmation of the selected file.
    GitDiscard,
    /// Git only: fetch the shown repository.
    GitFetch,
    /// Git only: push the shown repository's branch.
    GitPush,
    /// Fuzzy-find a file of the active workspace and open it in the Editor view.
    QuickOpen,
    /// Open the new task dialog for the active workspace.
    NewTask,
    /// Board only: run the selected Todo card.
    RunTask,
    /// Board only: select a card (arrows), move the selected card a column (Shift+←/→), open
    /// it (Enter) or edit it (`e`).
    Card(CardKey),
    /// Start the workspace's run commands (opens Run setup when none is saved yet).
    RunStart,
    /// Restart the workspace's live run sessions.
    RunRestart,
    /// Stop the workspace's live run sessions.
    RunStop,
    /// Open the command palette (every command, plus "Go to" workspace / session / task).
    CommandPalette,
    /// Open the Settings dialog (ADR 0073).
    OpenSettings,
    /// Cycle the theme System → Light → Dark (ADR 0027), saved as `[ui] theme`.
    ToggleTheme,
    /// Pick a built-in theme for the slot shown (ADR 0078), saved as `[ui] dark_theme` /
    /// `light_theme`.
    SelectTheme,
    /// Toggle the window filling the whole screen (not remembered across restarts).
    ToggleFullScreen,
    /// Hide or show the workspace sidebar (remembered as `[ui] sidebar`).
    ToggleSidebar,
    /// Narrow the workspace sidebar to the compact rail, or widen it back (a hidden sidebar
    /// comes back as the rail). Remembered as `[ui] sidebar`.
    ToggleCompactSidebar,
    /// Show or hide the session panel right of Terminals (SPEC §10); from another view it
    /// shows Terminals with the panel open.
    ToggleInspector,
    /// Start recording the microphone, or stop and type the speech as text into the field or
    /// terminal that has the keyboard (SPEC §20G).
    VoiceInput,
    /// Ask an agent any question, outside every project (ADR 0061).
    AskAi,
}

/// Key context of a focused terminal pane (`key_context` on its root element).
pub const TERMINAL_CONTEXT: &str = "Terminal";

/// Key context of the window while a board is shown (so Ctrl+Enter still reaches agents).
pub const BOARD_CONTEXT: &str = "Board";

/// Key context of the window while a Docs view is shown.
pub const DOCS_CONTEXT: &str = "Docs";

/// Key context of the window while the Editor view is shown.
pub const EDITOR_CONTEXT: &str = "Editor";

/// Key context of the file tree in the right panel while it has the keyboard.
pub const FILE_TREE_CONTEXT: &str = "FileTree";

/// Key context of the Git view's changed-file list while it has focus (so its single-key
/// shortcuts never reach a text field).
pub const GIT_CONTEXT: &str = "Git";

/// Comment commands work in the Docs view and in the Git view's list (diff comments).
pub const COMMENTS_CONTEXT: &str = "Docs || Git";

/// Find: in a terminal's scrollback, the Editor's opened file, and the Docs list filter.
pub const FIND_CONTEXT: &str = "Terminal || Editor || Docs";

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
            Command::SplitAgent,
            Command::SplitShell,
            Command::Layout(Preset::Columns2),
            Command::Layout(Preset::Rows2),
            Command::Layout(Preset::Columns3),
            Command::Layout(Preset::MainAndStack),
            Command::Layout(Preset::Grid),
            Command::EqualizePanes,
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
            Command::RenamePane,
            Command::Copy,
            Command::Paste,
            Command::PageUp,
            Command::PageDown,
            Command::Find,
            Command::OpenInZed,
            Command::NextInbox,
            Command::PreviousInbox,
            Command::Peek,
            Command::OpenComposer,
            Command::SendSelection,
            Command::ShowBoard,
            Command::ShowTerminals,
            Command::ShowDocs,
            Command::ToggleDocSource,
            Command::AddComment,
            Command::SendComments,
            Command::ShowFileTree,
            Command::ShowEditor,
            Command::ToggleIgnored,
            Command::SaveFile,
            Command::FindInFile,
            Command::ReplaceInFile,
            Command::CloseFileTab,
            Command::ShowGit,
            Command::GitToggleMode,
            Command::GitToggleHistory,
            Command::GitNextHunk,
            Command::GitPreviousHunk,
            Command::GitStage,
            Command::GitUnstage,
            Command::GitFocusCommit,
            Command::GitToggleDiffMode,
            Command::GitToggleViewed,
            Command::GitStageAll,
            Command::GitUnstageAll,
            Command::GitDiscard,
            Command::GitFetch,
            Command::GitPush,
            Command::QuickOpen,
            Command::NewTask,
            Command::RunTask,
            Command::RunStart,
            Command::RunRestart,
            Command::RunStop,
            Command::CommandPalette,
            Command::OpenSettings,
            Command::ToggleTheme,
            Command::SelectTheme,
            Command::ToggleFullScreen,
            Command::ToggleSidebar,
            Command::ToggleCompactSidebar,
            Command::ToggleInspector,
            Command::VoiceInput,
            Command::AskAi,
        ]);
        all.extend(CardKey::ALL.map(Command::Card));
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
            Command::SplitAgent => "split_agent".into(),
            Command::SplitShell => "split_shell".into(),
            Command::Layout(preset) => format!("layout_{}", preset.name()),
            Command::EqualizePanes => "equalize_panes".into(),
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
            Command::RenamePane => "rename_pane".into(),
            Command::Copy => "copy".into(),
            Command::Paste => "paste".into(),
            Command::PageUp => "page_up".into(),
            Command::PageDown => "page_down".into(),
            Command::Find => "find".into(),
            Command::OpenInZed => "open_in_zed".into(),
            Command::NextInbox => "next_inbox".into(),
            Command::PreviousInbox => "previous_inbox".into(),
            Command::Peek => "peek".into(),
            Command::OpenComposer => "open_composer".into(),
            Command::SendSelection => "send_selection".into(),
            Command::ShowBoard => "show_board".into(),
            Command::ShowTerminals => "show_terminals".into(),
            Command::ShowDocs => "show_docs".into(),
            Command::ToggleDocSource => "toggle_doc_source".into(),
            Command::AddComment => "add_comment".into(),
            Command::SendComments => "send_comments".into(),
            Command::ShowFileTree => "show_file_tree".into(),
            Command::ShowEditor => "show_editor".into(),
            Command::ToggleIgnored => "toggle_ignored".into(),
            Command::SaveFile => "save_file".into(),
            Command::FindInFile => "find_in_file".into(),
            Command::ReplaceInFile => "replace_in_file".into(),
            Command::CloseFileTab => "close_file_tab".into(),
            Command::ShowGit => "show_git".into(),
            Command::GitToggleMode => "git_toggle_mode".into(),
            Command::GitToggleHistory => "git_toggle_history".into(),
            Command::GitNextHunk => "git_next_hunk".into(),
            Command::GitPreviousHunk => "git_previous_hunk".into(),
            Command::GitStage => "git_stage".into(),
            Command::GitUnstage => "git_unstage".into(),
            Command::GitFocusCommit => "git_focus_commit".into(),
            Command::GitToggleDiffMode => "git_toggle_diff_mode".into(),
            Command::GitToggleViewed => "git_toggle_viewed".into(),
            Command::GitStageAll => "git_stage_all".into(),
            Command::GitUnstageAll => "git_unstage_all".into(),
            Command::GitDiscard => "git_discard".into(),
            Command::GitFetch => "git_fetch".into(),
            Command::GitPush => "git_push".into(),
            Command::QuickOpen => "quick_open".into(),
            Command::NewTask => "new_task".into(),
            Command::RunTask => "run_task".into(),
            Command::Card(key) => format!("card_{}", key.name()),
            Command::RunStart => "run_start".into(),
            Command::RunRestart => "run_restart".into(),
            Command::RunStop => "run_stop".into(),
            Command::CommandPalette => "command_palette".into(),
            Command::OpenSettings => "open_settings".into(),
            Command::ToggleTheme => "toggle_theme".into(),
            Command::SelectTheme => "select_theme".into(),
            Command::ToggleFullScreen => "toggle_full_screen".into(),
            Command::ToggleSidebar => "toggle_sidebar".into(),
            Command::ToggleCompactSidebar => "toggle_compact_sidebar".into(),
            Command::ToggleInspector => "toggle_inspector".into(),
            Command::VoiceInput => "voice_input".into(),
            Command::AskAi => "ask_ai".into(),
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
            Command::SplitAgent => "alt-shift-a".into(),
            Command::SplitShell => "alt-shift-s".into(),
            Command::Layout(preset) => match preset {
                Preset::Columns2 => "alt-shift-1",
                Preset::Rows2 => "alt-shift-2",
                Preset::Columns3 => "alt-shift-3",
                Preset::MainAndStack => "alt-shift-4",
                Preset::Grid => "alt-shift-5",
            }
            .into(),
            Command::EqualizePanes => "alt-shift-0".into(),
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
            Command::RenamePane => "f2".into(),
            Command::Copy => "ctrl-shift-c".into(),
            Command::Paste => "ctrl-shift-v".into(),
            Command::PageUp => "shift-pageup".into(),
            Command::PageDown => "shift-pagedown".into(),
            Command::Find => "ctrl-shift-f".into(),
            Command::OpenInZed => "ctrl-shift-z".into(),
            Command::NextInbox => "ctrl-shift-i".into(),
            Command::PreviousInbox => "ctrl-alt-i".into(),
            Command::Peek => "ctrl-shift-space".into(),
            Command::OpenComposer => "ctrl-shift-enter".into(),
            Command::SendSelection => "ctrl-shift-l".into(),
            Command::ShowBoard => "ctrl-shift-2".into(),
            Command::ShowTerminals => "ctrl-shift-1".into(),
            Command::ShowDocs => "ctrl-shift-5".into(),
            Command::ToggleDocSource => "ctrl-shift-e".into(),
            Command::AddComment => "ctrl-alt-m".into(),
            Command::SendComments => "ctrl-shift-s".into(),
            Command::ShowFileTree => "ctrl-shift-3".into(),
            Command::ShowEditor => "ctrl-shift-6".into(),
            Command::ToggleIgnored => "ctrl-shift-h".into(),
            Command::SaveFile => "ctrl-s".into(),
            Command::FindInFile => "ctrl-f".into(),
            Command::ReplaceInFile => "ctrl-h".into(),
            Command::CloseFileTab => "ctrl-w".into(),
            Command::ShowGit => "ctrl-shift-4".into(),
            Command::GitToggleMode => "b".into(),
            Command::GitToggleHistory => "h".into(),
            Command::GitNextHunk => "n".into(),
            Command::GitPreviousHunk => "p".into(),
            Command::GitStage => "s".into(),
            Command::GitUnstage => "u".into(),
            Command::GitFocusCommit => "c".into(),
            Command::GitToggleDiffMode => "d".into(),
            Command::GitToggleViewed => "v".into(),
            Command::GitStageAll => "shift-s".into(),
            Command::GitUnstageAll => "shift-u".into(),
            Command::GitDiscard => "x".into(),
            Command::GitFetch => "f".into(),
            Command::GitPush => "shift-p".into(),
            Command::QuickOpen => "ctrl-shift-o".into(),
            Command::NewTask => "ctrl-shift-n".into(),
            Command::RunTask => "ctrl-enter".into(),
            Command::Card(key) => match key {
                CardKey::Up => "up",
                CardKey::Down => "down",
                CardKey::Left => "left",
                CardKey::Right => "right",
                CardKey::MoveLeft => "shift-left",
                CardKey::MoveRight => "shift-right",
                CardKey::Open => "enter",
                CardKey::Edit => "e",
            }
            .into(),
            // Not F5 / Shift+F5 alone: terminal programs use those.
            Command::RunStart => "ctrl-f5".into(),
            Command::RunRestart => "ctrl-shift-f5".into(),
            Command::RunStop => "ctrl-alt-f5".into(),
            Command::CommandPalette => "ctrl-shift-p".into(),
            Command::OpenSettings => "ctrl-,".into(),
            Command::ToggleTheme => "ctrl-shift-j".into(),
            Command::SelectTheme => "ctrl-alt-j".into(),
            Command::ToggleFullScreen => "f11".into(),
            Command::ToggleSidebar => "ctrl-shift-b".into(),
            Command::ToggleCompactSidebar => "ctrl-alt-b".into(),
            Command::ToggleInspector => "ctrl-shift-y".into(),
            Command::VoiceInput => "ctrl-shift-d".into(),
            Command::AskAi => "ctrl-shift-g".into(),
        }
    }

    /// Key context the binding is limited to; `None` = anywhere in the window.
    pub fn context(self) -> Option<&'static str> {
        match self {
            Command::Copy | Command::Paste | Command::PageUp | Command::PageDown => {
                Some(TERMINAL_CONTEXT)
            }
            Command::Find => Some(FIND_CONTEXT),
            Command::RunTask | Command::Card(_) => Some(BOARD_CONTEXT),
            Command::ToggleDocSource => Some(DOCS_CONTEXT),
            Command::AddComment | Command::SendComments => Some(COMMENTS_CONTEXT),
            Command::ToggleIgnored => Some(FILE_TREE_CONTEXT),
            Command::SaveFile
            | Command::FindInFile
            | Command::ReplaceInFile
            | Command::CloseFileTab => Some(EDITOR_CONTEXT),
            Command::GitToggleMode
            | Command::GitToggleHistory
            | Command::GitNextHunk
            | Command::GitPreviousHunk
            | Command::GitStage
            | Command::GitUnstage
            | Command::GitToggleDiffMode
            | Command::GitToggleViewed
            | Command::GitStageAll
            | Command::GitUnstageAll
            | Command::GitDiscard
            | Command::GitFetch
            | Command::GitPush
            | Command::GitFocusCommit => Some(GIT_CONTEXT),
            _ => None,
        }
    }
}

/// A fresh `keymap.toml`: every command commented out at its default, in [`Command::all`]
/// order, so uncommenting a line and changing its keys is all it takes.
pub fn template() -> String {
    let mut text = String::from(
        "# Mkrew keyboard shortcuts. The Settings dialog (Ctrl+,) writes this file; a hand edit\n\
         # applies after a restart. Uncomment a line and change its keys.\n\
         # Keys use GPUI syntax: modifiers ctrl, alt, shift joined by `-`, e.g. \"ctrl-shift-k\".\n\
         # A keystroke bound twice, an unknown command or an invalid keystroke is listed in the\n\
         # command palette (Ctrl+Shift+P) and that entry keeps its default.\n\n",
    );
    for command in Command::all() {
        text.push_str(&format!(
            "# {} = \"{}\"\n",
            command.name(),
            command.default_keys()
        ));
    }
    text
}

/// Writes `contents` to `path` (creating its folder) unless the file exists; an existing
/// file is never touched.
pub fn ensure_file(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            let written = std::io::Write::write_all(&mut file, contents.as_bytes());
            if written.is_err() {
                // A half-written file would never be rewritten; the next open tries again.
                drop(file);
                let _ = std::fs::remove_file(path);
            }
            written
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

/// `src` with the line of `name` replaced by `line`: its uncommented line, else its commented
/// template line, else appended. Everything else is kept byte for byte (CRLF too).
fn set_binding(src: &str, name: &str, line: &str) -> String {
    let eol = if src.contains("\r\n") { "\r\n" } else { "\n" };
    let is_entry = |l: &str| {
        l.trim_start()
            .strip_prefix(name)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    };
    let mut lines: Vec<&str> = src.lines().collect();
    let at = lines.iter().position(|l| is_entry(l)).or_else(|| {
        lines
            .iter()
            .position(|l| l.trim_start().strip_prefix('#').is_some_and(is_entry))
    });
    match at {
        Some(i) => lines[i] = line,
        None => lines.push(line),
    }
    let mut out = lines.join(eol);
    if src.is_empty() || src.ends_with('\n') || at.is_none() {
        out.push_str(eol);
    }
    out
}

/// Saves `command`'s keystroke (`None`: back to its default, the template's commented line)
/// in `path` (the template when missing). Writes nothing when the result has a problem the
/// file did not have already (a duplicate, an invalid keystroke), or the file is not TOML.
/// Returns the new keymap and the problems the file still has.
pub fn write_binding(
    path: &Path,
    command: Command,
    keys: Option<&str>,
) -> Result<(Keymap, Vec<KeymapError>), KeymapError> {
    let read_error = |error: std::io::Error| KeymapError::Read {
        path: path.display().to_string(),
        message: error.to_string(),
    };
    let src = match std::fs::read_to_string(path) {
        Ok(src) => src,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => template(),
        Err(error) => return Err(read_error(error)),
    };
    let (_, before) = Keymap::parse(&src);
    if let Some(syntax) = before.iter().find(|e| matches!(e, KeymapError::Syntax(_))) {
        return Err(syntax.clone());
    }
    let name = command.name();
    let line = match keys {
        Some(keys) => format!("{name} = {}", toml::Value::String(keys.to_owned())),
        None => format!("# {name} = \"{}\"", command.default_keys()),
    };
    let out = set_binding(&src, &name, &line);
    let (keymap, after) = Keymap::parse(&out);
    if let Some(new) = after.iter().find(|e| !before.contains(e)) {
        return Err(new.clone());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(read_error)?;
    }
    std::fs::write(path, out).map_err(read_error)?;
    Ok((keymap, after))
}

/// The command, other than `command`, bound to the same key as `keys` once both are folded by
/// `mapper` the way the platform reports typed keys. On Windows Ctrl+Shift+2 arrives as `ctrl-@`
/// (US layout), so a captured `ctrl-@` clashes with the default `ctrl-shift-2`; the plain
/// [`Keymap::parse`] duplicate check cannot see that, and GPUI would pick one of the two by
/// command order.
pub fn folded_clash(
    keymap: &Keymap,
    command: Command,
    keys: &str,
    mapper: &dyn gpui::PlatformKeyboardMapper,
) -> Option<Command> {
    let fold = |command: Command, keys: &str| {
        let binding =
            gpui::KeyBinding::load(keys, Box::new(Run(command)), None, true, None, mapper).ok()?;
        binding
            .keystrokes()
            .first()
            .map(|keystroke| keystroke.inner().clone())
    };
    let wanted = fold(command, keys)?;
    keymap
        .bindings()
        .iter()
        .filter(|(other, _)| *other != command)
        .find(|(other, bound)| {
            fold(*other, &bound.unparse()).is_some_and(|folded| same_keys(&folded, &wanted))
        })
        .map(|(other, _)| *other)
}

/// `keys` as shortcut text ("Ctrl+Shift+K"); the raw text when it is not a keystroke.
pub fn keys_label(keys: &str) -> String {
    parse_keys(keys).map_or_else(|| keys.to_owned(), |keystroke| label(&keystroke))
}

/// The sidebar's one-line summary of `keymap.toml` problems.
pub fn notice(errors: &[KeymapError]) -> Option<String> {
    errors.first().map(|first| match errors.len() {
        1 => format!("{first}; using the default. Fix it in Settings (Ctrl+,)."),
        n => format!(
            "{first} (+{} more); using the defaults. Fix them in Settings (Ctrl+,).",
            n - 1
        ),
    })
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

    /// GPUI key bindings for [`Keymap::bindings`], folded by `mapper` the way the platform
    /// reports typed keys (on Windows Shift folds into digits and punctuation: Ctrl+Shift+2
    /// arrives as `ctrl-@` on a US layout). Rebuild them when the keyboard layout changes.
    pub fn key_bindings(&self, mapper: &dyn gpui::PlatformKeyboardMapper) -> Vec<gpui::KeyBinding> {
        self.bindings
            .iter()
            .filter_map(|(command, keystroke)| {
                // Never fails in practice: the keystroke was parsed and is unparsed back, the
                // contexts are plain identifiers, `COMMENTS_CONTEXT` or `FIND_CONTEXT` (all
                // loaded in the tests).
                let context = match command
                    .context()
                    .map(gpui::KeyBindingContextPredicate::parse)
                {
                    None => None,
                    Some(Ok(predicate)) => Some(predicate.into()),
                    Some(Err(error)) => {
                        tracing::warn!("{command:?}: bad key context: {error}");
                        return None;
                    }
                };
                let keys = keystroke.unparse();
                let run = Box::new(Run(*command));
                gpui::KeyBinding::load(&keys, run, context, true, None, mapper)
                    .inspect_err(|error| tracing::warn!("{command:?}: cannot bind {keys}: {error}"))
                    .ok()
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

    /// Whether `command`'s binding differs from its SPEC §13 default.
    pub fn changed(&self, command: Command) -> bool {
        let default = Keystroke::parse(&command.default_keys()).ok();
        match (self.keystroke(command), default) {
            (Some(bound), Some(default)) => !same_keys(bound, &default),
            (bound, default) => bound.is_some() != default.is_some(),
        }
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
        // The + Agent / + Shell buttons (split right with the workspace default).
        assert_eq!(bound("alt-shift-a"), Some(Command::SplitAgent));
        assert_eq!(bound("alt-shift-s"), Some(Command::SplitShell));
        assert_eq!(keymap.label(Command::SplitShell), "Alt+Shift+S");
        assert_eq!(keymap.label(Command::Layout(Preset::Grid)), "Alt+Shift+5");
        // Global like the split keys: a focused terminal no longer gets ESC A / ESC S.
        assert_eq!(Command::SplitAgent.context(), None);
        assert_eq!(bound("ctrl-shift-x"), Some(Command::ClosePane));
        assert_eq!(bound("ctrl-shift-c"), Some(Command::Copy));
        assert_eq!(bound("shift-pageup"), Some(Command::PageUp));
        assert_eq!(bound("shift-pagedown"), Some(Command::PageDown));
        assert_eq!(bound("ctrl-shift-f"), Some(Command::Find));
        assert_eq!(bound("ctrl-shift-r"), Some(Command::RenameTab));
        assert_eq!(bound("ctrl-shift-m"), Some(Command::ZoomPane));
        assert_eq!(bound("f2"), Some(Command::RenamePane));
        assert_eq!(keymap.label(Command::RenamePane), "F2");
        // Global: also while a terminal has focus (it no longer gets ESC O Q).
        assert_eq!(Command::RenamePane.context(), None);
        assert_eq!(bound("ctrl-shift-z"), Some(Command::OpenInZed));
        assert_eq!(bound("ctrl-shift-i"), Some(Command::NextInbox));
        assert_eq!(bound("ctrl-shift-space"), Some(Command::Peek));
        assert_eq!(keymap.label(Command::Peek), "Ctrl+Shift+Space");
        assert_eq!(bound("ctrl-shift-enter"), Some(Command::OpenComposer));
        assert_eq!(bound("ctrl-shift-l"), Some(Command::SendSelection));
        assert_eq!(bound("ctrl-shift-1"), Some(Command::ShowTerminals));
        assert_eq!(bound("ctrl-shift-2"), Some(Command::ShowBoard));
        assert_eq!(bound("ctrl-shift-5"), Some(Command::ShowDocs));
        assert_eq!(bound("ctrl-shift-e"), Some(Command::ToggleDocSource));
        assert_eq!(bound("ctrl-alt-m"), Some(Command::AddComment));
        assert_eq!(bound("ctrl-shift-s"), Some(Command::SendComments));
        assert_eq!(bound("ctrl-shift-3"), Some(Command::ShowFileTree));
        assert_eq!(bound("ctrl-shift-6"), Some(Command::ShowEditor));
        assert_eq!(bound("ctrl-shift-h"), Some(Command::ToggleIgnored));
        // Editor only: plain Ctrl+S saves the opened file (the terminal never has focus there).
        assert_eq!(bound("ctrl-s"), Some(Command::SaveFile));
        assert_eq!(Command::SaveFile.context(), Some(EDITOR_CONTEXT));
        assert_eq!(bound("ctrl-f"), Some(Command::FindInFile));
        assert_eq!(bound("ctrl-h"), Some(Command::ReplaceInFile));
        for command in [Command::FindInFile, Command::ReplaceInFile] {
            assert_eq!(command.context(), Some(EDITOR_CONTEXT), "{command:?}");
        }
        assert_eq!(bound("ctrl-shift-4"), Some(Command::ShowGit));
        assert_eq!(bound("ctrl-shift-o"), Some(Command::QuickOpen));
        assert_eq!(bound("ctrl-shift-n"), Some(Command::NewTask));
        assert_eq!(bound("ctrl-enter"), Some(Command::RunTask));
        assert_eq!(bound("ctrl-shift-p"), Some(Command::CommandPalette));
        assert_eq!(bound("ctrl-,"), Some(Command::OpenSettings));
        assert_eq!(bound("ctrl-alt-j"), Some(Command::SelectTheme));
        assert_eq!(Command::SelectTheme.context(), None);
        assert_eq!(keymap.label(Command::OpenSettings), "Ctrl+,");
        assert_eq!(bound("f11"), Some(Command::ToggleFullScreen));
        assert_eq!(keymap.label(Command::ToggleFullScreen), "F11");
        // Global: also while a terminal has focus (it no longer gets ESC[23~).
        assert_eq!(Command::ToggleFullScreen.context(), None);
        assert_eq!(bound("ctrl-shift-b"), Some(Command::ToggleSidebar));
        assert_eq!(keymap.label(Command::ToggleSidebar), "Ctrl+Shift+B");
        // Global: it works while a terminal has focus.
        assert_eq!(Command::ToggleSidebar.context(), None);
        assert_eq!(bound("ctrl-alt-b"), Some(Command::ToggleCompactSidebar));
        assert_eq!(keymap.label(Command::ToggleCompactSidebar), "Ctrl+Alt+B");
        // Global like Toggle sidebar: it works while a terminal has focus.
        assert_eq!(Command::ToggleCompactSidebar.context(), None);
        assert_eq!(bound("ctrl-shift-y"), Some(Command::ToggleInspector));
        assert_eq!(keymap.label(Command::ToggleInspector), "Ctrl+Shift+Y");
        // Global: also while a terminal has focus (it must not reach the agent).
        assert_eq!(Command::ToggleInspector.context(), None);
        assert_eq!(bound("ctrl-shift-d"), Some(Command::VoiceInput));
        assert_eq!(keymap.label(Command::VoiceInput), "Ctrl+Shift+D");
        // Global: it must reach the shell while a terminal or a text field has the keyboard.
        assert_eq!(Command::VoiceInput.context(), None);
        assert_eq!(bound("ctrl-shift-g"), Some(Command::AskAi));
        assert_eq!(Command::AskAi.context(), None);
        assert_eq!(Command::OpenSettings.context(), None);
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
        ] {
            assert_eq!(command.context(), Some(TERMINAL_CONTEXT), "{command:?}");
        }
        assert_eq!(Command::Find.context(), Some(FIND_CONTEXT));
        assert_eq!(Command::ClosePane.context(), None);
        // Ctrl+Enter only runs a card while the board is shown; terminals still get it.
        assert_eq!(Command::RunTask.context(), Some(BOARD_CONTEXT));
        // Ctrl+Shift+E only toggles while Docs are shown; terminals still get it.
        assert_eq!(Command::ToggleDocSource.context(), Some(DOCS_CONTEXT));
        // Comments: in Docs, and on diff lines while the Git list has focus.
        for command in [Command::AddComment, Command::SendComments] {
            assert_eq!(command.context(), Some(COMMENTS_CONTEXT), "{command:?}");
            let predicate = gpui::KeyBindingContextPredicate::parse(COMMENTS_CONTEXT).unwrap();
            for (context, matches) in [(DOCS_CONTEXT, true), (GIT_CONTEXT, true), ("Editor", false)]
            {
                let stack = [gpui::KeyContext::parse(context).unwrap()];
                assert_eq!(predicate.eval(&stack), matches, "{context}");
            }
        }
        // Ctrl+Shift+H only toggles ignored files while the tree has the keyboard.
        assert_eq!(Command::ToggleIgnored.context(), Some(FILE_TREE_CONTEXT));
        // SPEC §13 Git single keys, only while the Git list has focus.
        for (keys, command) in [
            ("b", Command::GitToggleMode),
            ("h", Command::GitToggleHistory),
            ("n", Command::GitNextHunk),
            ("p", Command::GitPreviousHunk),
            ("s", Command::GitStage),
            ("u", Command::GitUnstage),
            ("c", Command::GitFocusCommit),
            ("d", Command::GitToggleDiffMode),
            ("v", Command::GitToggleViewed),
            ("shift-s", Command::GitStageAll),
            ("shift-u", Command::GitUnstageAll),
            ("x", Command::GitDiscard),
            ("f", Command::GitFetch),
            ("shift-p", Command::GitPush),
        ] {
            assert_eq!(bound(keys), Some(command), "{keys}");
            assert_eq!(command.context(), Some(GIT_CONTEXT), "{command:?}");
        }
        assert_eq!(keymap.label(Command::GitNextHunk), "N");
        // SPEC §13 Board single keys, only while a board is shown.
        for (keys, key) in [
            ("up", CardKey::Up),
            ("down", CardKey::Down),
            ("left", CardKey::Left),
            ("right", CardKey::Right),
            ("shift-left", CardKey::MoveLeft),
            ("shift-right", CardKey::MoveRight),
            ("enter", CardKey::Open),
            ("e", CardKey::Edit),
        ] {
            assert_eq!(bound(keys), Some(Command::Card(key)), "{keys}");
            assert_eq!(Command::Card(key).context(), Some(BOARD_CONTEXT));
        }
        assert_eq!(keymap.label(Command::Card(CardKey::MoveLeft)), "Shift+←");
        assert_eq!(keymap.label(Command::Card(CardKey::Open)), "Enter");
        assert_eq!(keymap.label(Command::GitPush), "Shift+P");
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
        // No default is a plain Ctrl+<letter>, except in the Editor view (ADR 0028): no
        // terminal has focus there (SPEC §13).
        for (command, keys) in keymap.bindings() {
            let m = keys.modifiers;
            let plain_ctrl = m.control && !m.shift && !m.alt && keys.key.len() == 1;
            let letter = keys.key.chars().all(|c| c.is_ascii_alphabetic());
            assert!(
                !plain_ctrl || !letter || command.context() == Some(EDITOR_CONTEXT),
                "{command:?}"
            );
        }
    }

    #[test]
    fn ctrl_w_closes_a_file_tab_in_the_editor_only() {
        let keymap = Keymap::defaults();
        assert_eq!(
            keymap.command_for(&keystroke("ctrl-w")),
            Some(Command::CloseFileTab)
        );
        assert_eq!(Command::CloseFileTab.context(), Some(EDITOR_CONTEXT));
        assert_eq!(keymap.label(Command::CloseFileTab), "Ctrl+W");
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
        let bindings = keymap.key_bindings(&gpui::DummyKeyboardMapper);
        assert_eq!(bindings.len(), Command::all().len());
    }

    /// gpui-pre-windows reports Ctrl+Shift+2 as `ctrl-@` (Shift folded into the digit, layout
    /// dependent; keyboard.rs `get_keystroke_key`). A binding kept as `ctrl-shift-2` never
    /// matches, so every shifted digit / `=` / `-` binding must be folded the same way.
    #[cfg(windows)]
    #[test]
    fn shifted_digit_bindings_are_folded_like_windows_reports_them() {
        let platform = gpui_platform::current_platform(true);
        let mapper = platform.keyboard_mapper();
        let keymap = Keymap::defaults();
        let bindings = keymap.key_bindings(mapper.as_ref());
        let mut checked = 0;
        for ((command, keystroke), binding) in keymap.bindings().iter().zip(&bindings) {
            let folded = keystroke.modifiers.shift
                && (keystroke.key.chars().all(|c| c.is_ascii_digit())
                    || keystroke.key == "="
                    || keystroke.key == "-");
            if !folded {
                continue;
            }
            let inner = binding.keystrokes()[0].inner();
            assert!(
                !inner.modifiers.shift,
                "{command:?} still needs Shift: {inner:?}"
            );
            assert_ne!(
                inner.key, keystroke.key,
                "{command:?} kept the unshifted key"
            );
            checked += 1;
        }
        assert!(
            checked >= 7,
            "Ctrl+Shift+1..5 and Alt+Shift+=/- are shifted defaults"
        );
    }

    #[test]
    fn template_parses_to_the_defaults_commented_or_not() {
        let text = template();
        let defaults = Keymap::defaults();
        let (keymap, errors) = Keymap::parse(&text);
        assert_eq!(errors, []);
        assert_eq!(keymap.bindings(), defaults.bindings());
        // One commented line per command; uncommented they round-trip to the defaults too.
        let lines: Vec<&str> = text
            .lines()
            .filter_map(|line| line.strip_prefix("# "))
            .filter(|line| line.contains(" = \""))
            .collect();
        assert_eq!(lines.len(), Command::all().len());
        let (keymap, errors) = Keymap::parse(&lines.join("\n"));
        assert_eq!(errors, []);
        assert_eq!(keymap.bindings(), defaults.bindings());
    }

    #[test]
    fn edited_template_reports_a_duplicate() {
        let text = template().replace(
            "# close_pane = \"ctrl-shift-x\"",
            "close_pane = \"ctrl-shift-k\"",
        );
        let (keymap, errors) = Keymap::parse(&text);
        assert_eq!(
            errors,
            [KeymapError::Duplicate {
                command: "close_pane".into(),
                keys: "ctrl-shift-k".into(),
                other: "add_workspace".into(),
            }]
        );
        assert_eq!(keymap.label(Command::ClosePane), "Ctrl+Shift+X");
    }

    #[test]
    fn ensure_file_creates_the_template_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mkrew").join("keymap.toml");
        ensure_file(&path, &template()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), template());
        let edited = "close_pane = \"ctrl-shift-q\"\n";
        std::fs::write(&path, edited).unwrap();
        ensure_file(&path, &template()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), edited);
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

    fn temp_keymap(text: Option<&str>) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keymap.toml");
        if let Some(text) = text {
            std::fs::write(&path, text).unwrap();
        }
        (dir, path)
    }

    #[test]
    fn write_binding_sets_replaces_and_resets() {
        let (_dir, path) = temp_keymap(None);
        let (keymap, _) = write_binding(&path, Command::ClosePane, Some("ctrl-shift-q")).unwrap();
        assert_eq!(keymap.label(Command::ClosePane), "Ctrl+Shift+Q");
        assert!(keymap.changed(Command::ClosePane));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.starts_with("# Mkrew keyboard shortcuts"),
            "template written first"
        );
        assert!(
            text.contains("\nclose_pane = \"ctrl-shift-q\"\n"),
            "replaces the commented line"
        );
        assert!(!text.contains("# close_pane ="));
        write_binding(&path, Command::ClosePane, Some("ctrl-alt-shift-y")).unwrap();
        let (loaded, errors) = Keymap::load(&path);
        assert!(errors.is_empty());
        assert_eq!(loaded.label(Command::ClosePane), "Ctrl+Alt+Shift+Y");
        let (keymap, _) = write_binding(&path, Command::ClosePane, None).unwrap();
        assert!(!keymap.changed(Command::ClosePane));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            template(),
            "back to the template"
        );
    }

    #[test]
    fn write_binding_keeps_comments_and_crlf() {
        let (_dir, path) = temp_keymap(Some("# mine\r\n"));
        write_binding(&path, Command::ClosePane, Some("ctrl-shift-q")).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# mine\r\nclose_pane = \"ctrl-shift-q\"\r\n"
        );
    }

    #[test]
    fn a_duplicate_or_invalid_binding_is_refused_and_nothing_written() {
        let (_dir, path) = temp_keymap(Some("# mine\n"));
        let taken = Command::AddWorkspace.default_keys();
        match write_binding(&path, Command::ClosePane, Some(&taken)) {
            Err(KeymapError::Duplicate { other, .. }) => assert_eq!(other, "add_workspace"),
            other => panic!("expected a duplicate, got {other:?}"),
        }
        assert!(matches!(
            write_binding(&path, Command::ClosePane, Some("ctrl-")),
            Err(KeymapError::InvalidKeys { .. })
        ));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# mine\n");
    }

    #[test]
    fn an_old_problem_does_not_block_a_new_binding() {
        let (_dir, path) = temp_keymap(Some("open_keymap_settings = \"ctrl-,\"\n"));
        let (keymap, errors) =
            write_binding(&path, Command::ClosePane, Some("ctrl-shift-q")).unwrap();
        assert_eq!(keymap.label(Command::ClosePane), "Ctrl+Shift+Q");
        assert_eq!(
            errors,
            vec![KeymapError::UnknownCommand("open_keymap_settings".into())]
        );
        // A file that is not TOML at all is not edited.
        std::fs::write(&path, "close_pane = \n").unwrap();
        assert!(matches!(
            write_binding(&path, Command::Find, Some("ctrl-alt-shift-y")),
            Err(KeymapError::Syntax(_))
        ));
    }

    #[test]
    fn keys_label_matches_the_shortcut_text() {
        assert_eq!(keys_label("ctrl-shift-k"), "Ctrl+Shift+K");
        assert_eq!(keys_label("nonsense key"), "nonsense key");
    }

    #[test]
    fn a_clash_is_found_among_the_other_commands_only() {
        let keymap = Keymap::defaults();
        let mapper = gpui::DummyKeyboardMapper;
        let taken = Command::AddWorkspace.default_keys();
        assert_eq!(
            folded_clash(&keymap, Command::ClosePane, &taken, &mapper),
            Some(Command::AddWorkspace)
        );
        assert_eq!(
            folded_clash(&keymap, Command::AddWorkspace, &taken, &mapper),
            None
        );
        assert_eq!(
            folded_clash(&keymap, Command::ClosePane, "ctrl-alt-shift-y", &mapper),
            None
        );
    }

    /// Review finding: a key captured as Windows reports it (`ctrl-@`) must clash with the
    /// default written with Shift (`ctrl-shift-2`), or the dialog accepts a binding that GPUI
    /// resolves against the default by command order.
    #[cfg(windows)]
    #[test]
    fn a_captured_folded_key_clashes_with_the_shifted_default_it_shadows() {
        let platform = gpui_platform::current_platform(true);
        let mapper = platform.keyboard_mapper();
        let keymap = Keymap::defaults();
        let reported = gpui::KeyBinding::load(
            "ctrl-shift-2",
            Box::new(Run(Command::ShowBoard)),
            None,
            true,
            None,
            mapper.as_ref(),
        )
        .unwrap()
        .keystrokes()[0]
            .inner()
            .unparse();
        assert_ne!(reported, "ctrl-shift-2", "Windows folds the shifted digit");
        assert_eq!(
            folded_clash(&keymap, Command::AddWorkspace, &reported, mapper.as_ref()),
            Some(Command::ShowBoard)
        );
        assert_eq!(
            folded_clash(&keymap, Command::ShowBoard, &reported, mapper.as_ref()),
            None
        );
    }
}
