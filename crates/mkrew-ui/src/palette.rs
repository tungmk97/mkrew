//! Command palette (Ctrl+Shift+P, SPEC §13): every keymap command that can act in the current
//! view, with its title and shortcut, palette-only actions that have no key (Git fetch / push /
//! discard, task "View changes", fan-out, broadcast, insert prompt), plus "Go to" workspace /
//! session / task items (the §13 quick open of those). Typing filters with a fuzzy subsequence match
//! ranked by word starts and runs; Enter picks. Pure and tested headlessly; `main.rs` renders it and runs the pick.

use std::ops::Range;

use mkrew_protocol::{Project, Session, SessionState, Task};
use ulid::Ulid;

use crate::board::View;
use crate::history;
use crate::keymap::{Command, Keymap, KeymapError};
use crate::quick_open::{Key, match_ranges};
use crate::split::Direction;
use crate::text_box::Input;

/// What the window looks like when the palette opens; decides which commands are listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    /// A workspace is active.
    pub workspace: bool,
    /// The active workspace's view.
    pub view: View,
    /// Workspaces in the sidebar.
    pub workspaces: usize,
    /// Terminal tabs of the active workspace.
    pub tabs: usize,
    /// A terminal pane had the keyboard (copy, paste, find… act on it).
    pub terminal: bool,
    /// The Git view's selected file can be discarded (unstaged or untracked).
    pub discard: bool,
    /// The selected board card offers "View changes".
    pub task_changes: bool,
    /// The selected board card offers "Run with several agents…".
    pub fan_out: bool,
    /// The active workspace has a live agent session.
    pub agents: bool,
    /// The terminal pane that had the keyboard runs an agent (it has a composer).
    pub agent_pane: bool,
    /// "Run: Start all" would start something (no command yet: it opens Run setup).
    pub run_stopped: bool,
    /// A run session of the active workspace lives.
    pub run_live: bool,
}

/// A palette-only action: no key is bound to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extra {
    GitFetch,
    GitPush,
    GitDiscard,
    ViewTaskChanges,
    FanOut,
    Broadcast,
    InsertPrompt,
    Turns,
    FollowInZed,
    RunStart,
    RunRestart,
    RunStop,
    RunSetup,
    RunShowTab,
    OpenSettings,
}

impl Extra {
    pub const ALL: [Self; 15] = [
        Self::GitFetch,
        Self::GitPush,
        Self::GitDiscard,
        Self::ViewTaskChanges,
        Self::FanOut,
        Self::Broadcast,
        Self::InsertPrompt,
        Self::Turns,
        Self::FollowInZed,
        Self::RunStart,
        Self::RunRestart,
        Self::RunStop,
        Self::RunSetup,
        Self::RunShowTab,
        Self::OpenSettings,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::GitFetch => "Git: Fetch",
            Self::GitPush => "Git: Push",
            Self::GitDiscard => "Git: Discard changes to selected file…",
            Self::ViewTaskChanges => "Task: View changes of selected task",
            Self::FanOut => "Agent: Run selected task with several agents…",
            Self::Broadcast => "Agent: Broadcast to agents…",
            Self::InsertPrompt => "Agent: Insert prompt…",
            Self::Turns => "Agent: Turns…",
            Self::FollowInZed => "Agent: Follow in Zed (on/off)",
            Self::RunStart => "Run: Start all",
            Self::RunRestart => "Run: Restart all",
            Self::RunStop => "Run: Stop all",
            Self::RunSetup => "Run: Setup…",
            Self::RunShowTab => "Run: Show Run tab",
            Self::OpenSettings => "Global: Open settings",
        }
    }

    /// Whether the action can act in `context` (where its buttons are on screen).
    pub fn available(self, context: &Context) -> bool {
        let in_view = |view| context.workspace && context.view == view;
        match self {
            Self::GitFetch | Self::GitPush => in_view(View::Git),
            Self::GitDiscard => in_view(View::Git) && context.discard,
            Self::ViewTaskChanges => in_view(View::Board) && context.task_changes,
            Self::FanOut => in_view(View::Board) && context.fan_out,
            Self::Broadcast => context.workspace && context.agents,
            // Into the focused agent pane's composer (opened if hidden); the focused agent
            // pane's "Turns…" and "Follow in Zed".
            Self::InsertPrompt | Self::Turns | Self::FollowInZed => {
                in_view(View::Terminals) && context.tabs > 0 && context.agent_pane
            }
            Self::RunStart => context.workspace && context.run_stopped,
            Self::RunRestart | Self::RunStop | Self::RunShowTab => {
                context.workspace && context.run_live
            }
            Self::RunSetup => context.workspace,
            Self::OpenSettings => true,
        }
    }
}

/// Where a "Go to" item leads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Workspace(Ulid),
    Session(Ulid),
    Task { project_id: Ulid, task_id: Ulid },
}

/// What picking an entry does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Run(Command),
    Extra(Extra),
    GoTo(Target),
}

/// One palette line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub action: Action,
    /// Matched against the query, e.g. "Tabs: New shell tab".
    pub title: String,
    /// Shortcut label, e.g. "Ctrl+Shift+T"; empty if none.
    pub shortcut: String,
}

/// A shown entry with the byte ranges of its title the query matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row<'a> {
    pub entry: &'a Entry,
    pub ranges: Vec<Range<usize>>,
}

/// Palette title of `command`, "Group: Action" like the SPEC §13 table.
pub fn title(command: Command) -> String {
    let direction = |direction| match direction {
        Direction::Left => "left",
        Direction::Right => "right",
        Direction::Up => "up",
        Direction::Down => "down",
    };
    match command {
        Command::AddWorkspace => "Workspace: Add workspace".into(),
        Command::Workspace(n) => format!("Workspace: Jump to workspace {n}"),
        Command::NextWorkspace => "Workspace: Next workspace".into(),
        Command::PreviousWorkspace => "Workspace: Previous workspace".into(),
        Command::NewShellTab => "Tabs: New shell tab".into(),
        Command::NewAgentTab => "Tabs: New agent tab".into(),
        Command::NextTab => "Tabs: Next tab".into(),
        Command::PreviousTab => "Tabs: Previous tab".into(),
        Command::Tab(n) => format!("Tabs: Jump to tab {n}"),
        Command::RenameTab => "Tabs: Rename tab".into(),
        Command::CloseTab => "Tabs: Close tab".into(),
        Command::SplitRight => "Panes: Split right".into(),
        Command::SplitDown => "Panes: Split down".into(),
        Command::SplitAgent => "Panes: New agent pane".into(),
        Command::SplitShell => "Panes: New shell pane".into(),
        Command::Focus(d) => format!("Panes: Move focus {}", direction(d)),
        Command::Resize(d) => format!("Panes: Resize {}", direction(d)),
        Command::ZoomPane => "Panes: Zoom / unzoom pane".into(),
        Command::ClosePane => "Panes: Close pane".into(),
        Command::RenamePane => "Panes: Rename pane".into(),
        Command::Copy => "Terminal: Copy".into(),
        Command::Paste => "Terminal: Paste".into(),
        Command::PageUp => "Terminal: Page up".into(),
        Command::PageDown => "Terminal: Page down".into(),
        Command::Find => "Find: in scrollback, the open file or the Docs list".into(),
        Command::OpenInZed => "Zed: Open workspace or current file in Zed".into(),
        Command::NextInbox => "Inbox: Jump to next item needing attention".into(),
        Command::Peek => "Agent: Peek the first Inbox entry".into(),
        Command::OpenComposer => "Agent: Open the prompt composer".into(),
        Command::SendSelection => "Agent: Send selection to an agent".into(),
        Command::ShowBoard => "View: Board".into(),
        Command::ShowTerminals => "View: Terminals".into(),
        Command::ShowDocs => "View: Docs".into(),
        Command::ShowFiles => "View: Files".into(),
        Command::ShowGit => "View: Git".into(),
        Command::ToggleDocSource => "Docs: Toggle preview / source".into(),
        Command::AddComment => "Comments: Comment on selected lines".into(),
        Command::SendComments => "Comments: Send comments to agent".into(),
        Command::ToggleIgnored => "Files: Toggle ignored files".into(),
        Command::SaveFile => "Files: Save file".into(),
        Command::FindInFile => "Files: Find in file".into(),
        Command::ReplaceInFile => "Files: Replace in file".into(),
        Command::QuickOpen => "Files: Quick open file".into(),
        Command::GitToggleMode => "Git: Toggle Working changes / Against base".into(),
        Command::GitToggleHistory => "Git: Toggle History".into(),
        Command::GitNextHunk => "Git: Next hunk".into(),
        Command::GitPreviousHunk => "Git: Previous hunk".into(),
        Command::GitStage => "Git: Stage selected file or hunk".into(),
        Command::GitUnstage => "Git: Unstage selected file or hunk".into(),
        Command::GitFocusCommit => "Git: Focus commit message".into(),
        Command::GitToggleDiffMode => "Git: Toggle Unified / Split diff".into(),
        Command::NewTask => "Task: New task".into(),
        Command::RunTask => "Task: Run selected task".into(),
        Command::CommandPalette => "Global: Command palette".into(),
        Command::OpenKeymapSettings => "Global: Open keymap settings".into(),
        Command::ToggleTheme => "Global: Toggle theme (System → Light → Dark)".into(),
        Command::ToggleFullScreen => "Global: Toggle full screen".into(),
        Command::ToggleSidebar => "Global: Toggle sidebar".into(),
        Command::ToggleInspector => "View: Toggle session panel".into(),
    }
}

/// Whether `command` can act in `context` (mirrors the guards of the shell's `run`).
pub fn available(command: Command, context: &Context) -> bool {
    let in_view = |view| context.workspace && context.view == view;
    let on_panes = in_view(View::Terminals) && context.tabs > 0;
    match command {
        // Already open.
        Command::CommandPalette => false,
        Command::AddWorkspace
        | Command::NextInbox
        | Command::Peek
        | Command::OpenKeymapSettings
        | Command::ToggleTheme
        | Command::ToggleFullScreen
        | Command::ToggleSidebar => true,
        Command::Workspace(n) => usize::from(n) <= context.workspaces,
        Command::NextWorkspace | Command::PreviousWorkspace => context.workspaces > 1,
        Command::NewShellTab
        | Command::NewAgentTab
        | Command::OpenInZed
        | Command::ShowBoard
        | Command::ShowTerminals
        | Command::ShowDocs
        | Command::ShowFiles
        | Command::ShowGit
        | Command::QuickOpen
        | Command::ToggleInspector
        | Command::NewTask => context.workspace,
        Command::NextTab | Command::PreviousTab => context.workspace && context.tabs > 1,
        Command::Tab(n) => context.workspace && usize::from(n) <= context.tabs,
        Command::SplitRight | Command::SplitDown | Command::SplitAgent | Command::SplitShell => {
            context.workspace && context.tabs > 0
        }
        Command::RenameTab
        | Command::CloseTab
        | Command::Focus(_)
        | Command::Resize(_)
        | Command::ZoomPane
        | Command::RenamePane
        | Command::ClosePane => on_panes,
        Command::Copy | Command::Paste | Command::PageUp | Command::PageDown => {
            on_panes && context.terminal
        }
        Command::Find => {
            (on_panes && context.terminal) || in_view(View::Files) || in_view(View::Docs)
        }
        Command::OpenComposer => on_panes && context.agent_pane,
        Command::SendSelection => {
            in_view(View::Files) || in_view(View::Git) || (on_panes && context.terminal)
        }
        Command::RunTask => in_view(View::Board),
        Command::ToggleDocSource => in_view(View::Docs),
        Command::AddComment | Command::SendComments => in_view(View::Docs) || in_view(View::Git),
        Command::ToggleIgnored
        | Command::SaveFile
        | Command::FindInFile
        | Command::ReplaceInFile => in_view(View::Files),
        Command::GitToggleMode
        | Command::GitToggleHistory
        | Command::GitNextHunk
        | Command::GitPreviousHunk
        | Command::GitStage
        | Command::GitUnstage
        | Command::GitToggleDiffMode
        | Command::GitFocusCommit => in_view(View::Git),
    }
}

/// One entry per `keymap.toml` problem found at start (SPEC §13), listed before the
/// commands; picking one opens the keymap settings to fix it.
pub fn keymap_errors(keymap: &Keymap, errors: &[KeymapError]) -> Vec<Entry> {
    errors
        .iter()
        .map(|error| Entry {
            action: Action::Run(Command::OpenKeymapSettings),
            title: format!("Keymap error: {error}"),
            shortcut: keymap.label(Command::OpenKeymapSettings),
        })
        .collect()
}

/// The `settings.toml` problem (SPEC §20.0), listed before the commands like keymap errors;
/// picking it opens the settings to fix it.
pub fn settings_error(error: Option<&str>) -> Vec<Entry> {
    error
        .map(|error| Entry {
            action: Action::Extra(Extra::OpenSettings),
            title: format!("Settings error: {error}"),
            shortcut: String::new(),
        })
        .into_iter()
        .collect()
}

/// The palette's entries: the available commands in keymap order, the available
/// palette-only actions, then "Go to" every workspace, live session and task (archived
/// workspaces and their items left out).
pub fn entries(
    keymap: &Keymap,
    context: &Context,
    projects: &[Project],
    sessions: &[Session],
    tasks: &[Task],
) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Command::all()
        .into_iter()
        .filter(|command| available(*command, context))
        .map(|command| Entry {
            action: Action::Run(command),
            title: title(command),
            shortcut: keymap.label(command),
        })
        .collect();
    entries.extend(
        Extra::ALL
            .into_iter()
            .filter(|extra| extra.available(context))
            .map(|extra| Entry {
                action: Action::Extra(extra),
                title: extra.title().into(),
                shortcut: String::new(),
            }),
    );
    let workspace = |id: Ulid| {
        projects
            .iter()
            .find(|project| project.id == id && !project.archived)
            .map(|project| project.name.as_str())
    };
    let go_to = |target, title| Entry {
        action: Action::GoTo(target),
        title,
        shortcut: String::new(),
    };
    entries.extend(
        projects
            .iter()
            .filter(|project| !project.archived)
            .map(|project| {
                go_to(
                    Target::Workspace(project.id),
                    format!("Go to workspace: {}", project.name),
                )
            }),
    );
    entries.extend(
        sessions
            .iter()
            .filter(|session| session.state != SessionState::Exited)
            .filter_map(|session| {
                let name = workspace(session.project_id)?;
                Some(go_to(
                    Target::Session(session.id),
                    format!("Go to session: {} · {name}", history::label(session)),
                ))
            }),
    );
    entries.extend(tasks.iter().filter_map(|task| {
        let name = workspace(task.project_id)?;
        Some(go_to(
            Target::Task {
                project_id: task.project_id,
                task_id: task.id,
            },
            format!("Go to task: {} · {name}", task.title),
        ))
    }));
    entries
}

/// Fuzzy match of `query` in `text`: every non-space query char in order, ignoring case.
/// Returns the score of the best alignment (word starts and runs of adjacent chars count
/// more; ties go to the earliest chars) and its matched char indices; `None` if `text` does
/// not contain the query.
pub fn fuzzy(query: &str, text: &str) -> Option<(i64, Vec<u32>)> {
    let chars: Vec<char> = text.chars().collect();
    let wanted: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if wanted.is_empty() {
        return Some((0, Vec::new()));
    }
    // best[i][j]: the best score of the first i + 1 query chars with the last one at char j,
    // and where the previous one is. Titles are short, so O(query × text²) is fine.
    let mut best: Vec<Vec<Option<(i64, usize)>>> = vec![vec![None; chars.len()]; wanted.len()];
    for (i, want) in wanted.iter().enumerate() {
        for j in 0..chars.len() {
            if !chars[j].to_lowercase().eq([*want]) {
                continue;
            }
            let here = 1 + if word_start(&chars, j) { 8 } else { 0 };
            best[i][j] = match i.checked_sub(1) {
                None => Some((here, j)),
                Some(previous) => (0..j)
                    .filter_map(|k| {
                        let (score, _) = best[previous][k]?;
                        let run = if k + 1 == j { 4 } else { 0 };
                        Some((score + here + run, k))
                    })
                    .max_by_key(|&(score, k)| (score, std::cmp::Reverse(k))),
            };
        }
    }
    let (mut at, score) = best[wanted.len() - 1]
        .iter()
        .enumerate()
        .filter_map(|(j, cell)| Some((j, cell.as_ref()?.0)))
        .max_by_key(|&(j, score)| (score, std::cmp::Reverse(j)))?;
    let mut indices = vec![0; wanted.len()];
    for i in (0..wanted.len()).rev() {
        indices[i] = u32::try_from(at).ok()?;
        at = best[i][at]?.1;
    }
    Some((score, indices))
}

/// The first char, one after a non-alphanumeric char, or an upper-case char after a
/// lower-case one ("OpenInZed").
fn word_start(chars: &[char], at: usize) -> bool {
    let Some(previous) = at.checked_sub(1).map(|i| chars[i]) else {
        return true;
    };
    !previous.is_alphanumeric() || (previous.is_lowercase() && chars[at].is_uppercase())
}

/// The open palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    entries: Vec<Entry>,
    /// Private: every edit must filter again ([`Self::key`], [`Self::insert`]).
    query: Input,
    /// Indices into `entries` of the matches, best first, with their matched char indices.
    shown: Vec<(usize, Vec<u32>)>,
    /// Index into the shown rows of the picked one.
    pub selected: usize,
}

impl Palette {
    pub fn new(entries: Vec<Entry>) -> Self {
        let mut palette = Self {
            entries,
            query: Input::default(),
            shown: Vec::new(),
            selected: 0,
        };
        palette.filter();
        palette
    }

    pub fn query(&self) -> &str {
        self.query.as_str()
    }

    /// Whether Ctrl+A selected the whole query (drawn highlighted).
    pub fn query_selected(&self) -> bool {
        self.query.all_selected()
    }

    /// Applies a key; Esc, Enter and Ctrl+V are the caller's (close, [`Self::picked`],
    /// [`Self::insert`] the clipboard).
    pub fn key(&mut self, key: Key) {
        match key {
            Key::Move(by) => self.step(by),
            Key::Backspace => {
                if self.query.backspace() {
                    self.filter();
                }
            }
            Key::DeleteWord => {
                if self.query.delete_word() {
                    self.filter();
                }
            }
            Key::SelectAll => self.query.select_all(),
            Key::Insert(text) => self.insert(&text),
            Key::Close | Key::Open | Key::Paste => {}
        }
    }

    /// Appends typed or pasted text (line breaks dropped) and filters again.
    pub fn insert(&mut self, text: &str) {
        let line: String = text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
        if self.query.insert(&line) {
            self.filter();
        }
    }

    /// Keeps the entries matching the query, best score first (ties keep entry order), and
    /// picks the best one.
    fn filter(&mut self) {
        let mut scored: Vec<(i64, usize, Vec<u32>)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let (score, indices) = fuzzy(&self.query, &entry.title)?;
                Some((score, index, indices))
            })
            .collect();
        scored.sort_by_key(|(score, ..)| std::cmp::Reverse(*score));
        self.shown = scored
            .into_iter()
            .map(|(_, index, indices)| (index, indices))
            .collect();
        self.selected = 0;
    }

    /// Moves the pick by `by`, wrapping around the list.
    pub fn step(&mut self, by: i32) {
        let count = self.shown.len();
        if count == 0 {
            return;
        }
        let count = i64::try_from(count).unwrap_or(i64::MAX);
        let at = i64::try_from(self.selected).unwrap_or(0);
        self.selected = usize::try_from((at + i64::from(by)).rem_euclid(count)).unwrap_or(0);
    }

    /// Picks row `index` (a click).
    pub fn pick(&mut self, index: usize) {
        if index < self.shown.len() {
            self.selected = index;
        }
    }

    /// The shown rows, best match first.
    pub fn rows(&self) -> Vec<Row<'_>> {
        self.shown
            .iter()
            .map(|(index, indices)| {
                let entry = &self.entries[*index];
                Row {
                    entry,
                    ranges: match_ranges(&entry.title, indices),
                }
            })
            .collect()
    }

    /// What the picked row does.
    pub fn picked(&self) -> Option<Action> {
        let (index, _) = self.shown.get(self.selected)?;
        Some(self.entries[*index].action)
    }
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{SessionKind, StateSource, TaskStatus};

    use super::*;

    fn context(view: View) -> Context {
        Context {
            workspace: true,
            view,
            workspaces: 2,
            tabs: 2,
            terminal: false,
            discard: false,
            task_changes: false,
            fan_out: false,
            agents: false,
            agent_pane: false,
            run_stopped: false,
            run_live: false,
        }
    }

    fn commands(context: &Context) -> Vec<Command> {
        entries(&Keymap::defaults(), context, &[], &[], &[])
            .into_iter()
            .filter_map(|entry| match entry.action {
                Action::Run(command) => Some(command),
                Action::Extra(_) | Action::GoTo(_) => None,
            })
            .collect()
    }

    fn project(name: &str, archived: bool) -> Project {
        Project {
            id: Ulid::new(),
            name: name.into(),
            path: format!("C:/{name}"),
            run_commands: Vec::new(),
            run_autostart: false,
            default_agent: None,
            default_shell: None,
            review_globs: None,
            created_at: 0,
            archived,
            base_ref: None,
            color: None,
            pinned: false,
        }
    }

    fn session(project_id: Ulid, title: Option<&str>, state: SessionState) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id,
            agent_profile: "claude".into(),
            kind: SessionKind::Agent,
            title: title.map(Into::into),
            terminal_title: None,
            color: None,
            native_session_id: None,
            state,
            state_source: StateSource::default(),
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
            pinned: false,
            plan_path: None,
        }
    }

    fn task(project_id: Ulid, title: &str) -> Task {
        Task {
            id: Ulid::new(),
            project_id,
            title: title.into(),
            prompt: String::new(),
            agent_profile: None,
            status: TaskStatus::Todo,
            position: 0.0,
            checkpoint: None,
            error_flag: false,
            worktree: false,
            worktree_path: None,
            branch: None,
            fanout_of: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn every_command_has_a_distinct_title() {
        let titles: Vec<String> = Command::all().into_iter().map(title).collect();
        for (i, title) in titles.iter().enumerate() {
            assert!(!title.is_empty());
            assert!(!titles[..i].contains(title), "{title}");
        }
    }

    #[test]
    fn keymap_errors_open_the_keymap_settings() {
        let (keymap, errors) =
            Keymap::parse("close_pane = \"ctrl-shift-k\"\nnope = \"ctrl-1\"\nfind = \"\"\n");
        let listed = keymap_errors(&keymap, &errors);
        assert_eq!(listed.len(), 3);
        for entry in &listed {
            assert_eq!(entry.action, Action::Run(Command::OpenKeymapSettings));
            assert_eq!(entry.shortcut, "Ctrl+,");
        }
        let titles: Vec<&str> = listed.iter().map(|entry| entry.title.as_str()).collect();
        assert!(titles.contains(
            &"Keymap error: `close_pane = \"ctrl-shift-k\"` is already bound to `add_workspace`"
        ));
        assert!(titles.contains(&"Keymap error: unknown command `nope` in keymap.toml"));
        assert!(titles.contains(&"Keymap error: `find = \"\"` is not a valid keystroke"));
        // Typing "keymap" finds them and the settings command.
        let mut all = listed;
        all.extend(entries(&keymap, &context(View::Terminals), &[], &[], &[]));
        let mut palette = Palette::new(all);
        palette.insert("keymap");
        assert_eq!(palette.rows().len(), 4);
    }

    #[test]
    fn a_settings_error_opens_the_settings() {
        let error = "settings.toml:2:13: font_size must be 8–32 px, got 4";
        let listed = settings_error(Some(error));
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].action, Action::Extra(Extra::OpenSettings));
        assert_eq!(listed[0].title, format!("Settings error: {error}"));
        assert!(settings_error(None).is_empty());
    }

    #[test]
    fn commands_carry_their_shortcut() {
        let (keymap, errors) = Keymap::parse("new_shell_tab = \"ctrl-shift-g\"\n");
        assert_eq!(errors, []);
        let all = entries(&keymap, &context(View::Terminals), &[], &[], &[]);
        let shell = all
            .iter()
            .find(|entry| entry.action == Action::Run(Command::NewShellTab))
            .unwrap();
        assert_eq!(shell.title, "Tabs: New shell tab");
        assert_eq!(shell.shortcut, "Ctrl+Shift+G");
    }

    #[test]
    fn availability_follows_the_view() {
        let terminals = commands(&context(View::Terminals));
        for command in [
            Command::NewShellTab,
            Command::CloseTab,
            Command::ClosePane,
            Command::RenamePane,
            Command::Tab(2),
            Command::NextTab,
            Command::Workspace(2),
            Command::ShowGit,
            Command::ToggleInspector,
        ] {
            assert!(terminals.contains(&command), "{command:?}");
        }
        for command in [
            Command::CommandPalette,
            Command::Tab(3),
            Command::Workspace(3),
            Command::Copy,
            Command::RunTask,
            Command::GitStage,
            Command::ToggleDocSource,
            Command::AddComment,
            Command::ToggleIgnored,
            Command::SaveFile,
            Command::FindInFile,
            Command::ReplaceInFile,
        ] {
            assert!(!terminals.contains(&command), "{command:?}");
        }
        let focused = commands(&Context {
            terminal: true,
            ..context(View::Terminals)
        });
        assert!(focused.contains(&Command::Copy));
        assert!(focused.contains(&Command::Find));
        assert!(focused.contains(&Command::SendSelection));
        assert!(
            !terminals.contains(&Command::SendSelection),
            "no terminal focused"
        );
        assert!(!focused.contains(&Command::OpenComposer), "a shell pane");
        let agent = commands(&Context {
            terminal: true,
            agent_pane: true,
            ..context(View::Terminals)
        });
        assert!(agent.contains(&Command::OpenComposer));

        let git = commands(&context(View::Git));
        assert!(git.contains(&Command::GitStage));
        assert!(git.contains(&Command::AddComment));
        assert!(git.contains(&Command::SendSelection));
        assert!(
            !git.contains(&Command::ClosePane),
            "panes are not on screen"
        );
        assert!(
            !git.contains(&Command::RenamePane),
            "panes are not on screen"
        );
        assert!(!git.contains(&Command::RunTask));
        assert!(git.contains(&Command::NewShellTab), "shows the terminals");
        assert!(
            git.contains(&Command::ToggleInspector),
            "shows the terminals"
        );
        // Like the buttons: from any view (they show Terminals first), once a tab exists.
        for command in [Command::SplitAgent, Command::SplitShell] {
            assert!(terminals.contains(&command), "{command:?}");
            assert!(git.contains(&command), "{command:?}");
        }
        let no_tabs = commands(&Context {
            tabs: 0,
            ..context(View::Terminals)
        });
        assert!(!no_tabs.contains(&Command::SplitAgent));
        assert!(commands(&context(View::Board)).contains(&Command::RunTask));
        let docs = commands(&context(View::Docs));
        assert!(docs.contains(&Command::ToggleDocSource));
        assert!(docs.contains(&Command::SendComments));
        assert!(commands(&context(View::Files)).contains(&Command::ToggleIgnored));
        assert!(commands(&context(View::Files)).contains(&Command::SaveFile));
        assert!(commands(&context(View::Files)).contains(&Command::ReplaceInFile));

        let lone = commands(&Context {
            workspaces: 1,
            tabs: 1,
            ..context(View::Terminals)
        });
        assert!(!lone.contains(&Command::NextWorkspace));
        assert!(!lone.contains(&Command::NextTab));
        assert!(lone.contains(&Command::Tab(1)));

        let none = Context {
            workspace: false,
            workspaces: 0,
            tabs: 0,
            agents: true,
            run_stopped: true,
            run_live: true,
            ..context(View::Terminals)
        };
        assert_eq!(
            commands(&none),
            [
                Command::AddWorkspace,
                Command::NextInbox,
                Command::Peek,
                Command::OpenKeymapSettings,
                Command::ToggleTheme,
                Command::ToggleFullScreen,
                Command::ToggleSidebar
            ]
        );
        assert_eq!(extras(&none), []);
        assert!(
            entries(&Keymap::defaults(), &none, &[], &[], &[])
                .iter()
                .any(|e| e.action == Action::Extra(Extra::OpenSettings)),
            "settings open without a workspace"
        );
    }

    fn extras(context: &Context) -> Vec<Extra> {
        entries(&Keymap::defaults(), context, &[], &[], &[])
            .into_iter()
            .filter_map(|entry| match entry.action {
                Action::Extra(extra) if !matches!(extra, Extra::RunSetup | Extra::OpenSettings) => {
                    Some(extra)
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn palette_only_actions_follow_the_view_and_state() {
        assert_eq!(extras(&context(View::Terminals)), []);
        assert_eq!(
            extras(&context(View::Git)),
            [Extra::GitFetch, Extra::GitPush]
        );
        let git = Context {
            discard: true,
            task_changes: true,
            fan_out: true,
            ..context(View::Git)
        };
        assert_eq!(
            extras(&git),
            [Extra::GitFetch, Extra::GitPush, Extra::GitDiscard]
        );
        let board = Context {
            discard: true,
            task_changes: true,
            ..context(View::Board)
        };
        assert_eq!(extras(&board), [Extra::ViewTaskChanges]);
        let fan_out = Context {
            fan_out: true,
            ..context(View::Board)
        };
        assert_eq!(extras(&fan_out), [Extra::FanOut]);
        let agents = Context {
            agents: true,
            ..context(View::Board)
        };
        assert_eq!(extras(&agents), [Extra::Broadcast]);
        let agent_pane = Context {
            agent_pane: true,
            ..context(View::Terminals)
        };
        assert_eq!(
            extras(&agent_pane),
            [Extra::InsertPrompt, Extra::Turns, Extra::FollowInZed]
        );
        let elsewhere = Context {
            agent_pane: true,
            ..context(View::Board)
        };
        assert_eq!(extras(&elsewhere), [], "the composer is not on screen");
        let stopped = Context {
            run_stopped: true,
            ..context(View::Docs)
        };
        assert_eq!(extras(&stopped), [Extra::RunStart]);
        let live = Context {
            run_live: true,
            ..context(View::Docs)
        };
        assert_eq!(
            extras(&live),
            [Extra::RunRestart, Extra::RunStop, Extra::RunShowTab]
        );
        let all = entries(&Keymap::defaults(), &context(View::Board), &[], &[], &[]);
        assert!(
            all.iter()
                .any(|e| matches!(e.action, Action::Extra(Extra::RunSetup)))
        );

        let commands: Vec<String> = Command::all().into_iter().map(title).collect();
        let titles: Vec<&str> = Extra::ALL.into_iter().map(Extra::title).collect();
        for (i, title) in titles.iter().enumerate() {
            assert!(!titles[..i].contains(title), "{title}");
            assert!(!commands.iter().any(|command| command == title), "{title}");
        }
        assert!(titles.contains(&"Global: Open settings"));
        // Palette-only actions come after the commands, before "Go to".
        let all = entries(
            &Keymap::defaults(),
            &git,
            &[project("app", false)],
            &[],
            &[],
        );
        let kinds: Vec<u8> = all
            .iter()
            .map(|entry| match entry.action {
                Action::Run(_) => 0,
                Action::Extra(_) => 1,
                Action::GoTo(_) => 2,
            })
            .collect();
        assert!(kinds.is_sorted(), "{kinds:?}");
        assert!(kinds.contains(&1) && kinds.contains(&2));
    }

    #[test]
    fn go_to_workspaces_live_sessions_and_tasks() {
        let app = project("app", false);
        let old = project("old", true);
        let sessions = [
            session(app.id, Some("fix login"), SessionState::Working),
            session(app.id, None, SessionState::Idle),
            session(app.id, Some("gone"), SessionState::Exited),
            session(old.id, Some("archived"), SessionState::Working),
        ];
        let tasks = [task(app.id, "Add tests"), task(old.id, "Archived task")];
        let all = entries(
            &Keymap::defaults(),
            &context(View::Terminals),
            &[app.clone(), old],
            &sessions,
            &tasks,
        );
        let go_to: Vec<(Action, &str)> = all
            .iter()
            .filter(|entry| matches!(entry.action, Action::GoTo(_)))
            .map(|entry| (entry.action, entry.title.as_str()))
            .collect();
        assert_eq!(
            go_to,
            [
                (
                    Action::GoTo(Target::Workspace(app.id)),
                    "Go to workspace: app"
                ),
                (
                    Action::GoTo(Target::Session(sessions[0].id)),
                    "Go to session: fix login · app"
                ),
                (
                    Action::GoTo(Target::Session(sessions[1].id)),
                    "Go to session: claude · app"
                ),
                (
                    Action::GoTo(Target::Task {
                        project_id: app.id,
                        task_id: tasks[0].id
                    }),
                    "Go to task: Add tests · app"
                ),
            ]
        );
        // Commands come first.
        assert!(matches!(all[0].action, Action::Run(_)));
    }

    #[test]
    fn fuzzy_matches_subsequences_ignoring_case_and_spaces() {
        let (_, indices) = fuzzy("nst", "Tabs: New shell tab").unwrap();
        assert_eq!(indices, [6, 10, 16]);
        assert!(fuzzy("git stage", "Git: Stage selected file or hunk").is_some());
        assert_eq!(fuzzy("zz", "Git: Stage"), None);
        assert_eq!(fuzzy("", "anything"), Some((0, vec![])));
        assert_eq!(fuzzy("é", "Café"), Some((1, vec![3])));
        // The best alignment, not the first chars found: "Split", not the "s" of "Panes".
        let (_, indices) = fuzzy("split", "Panes: Split right").unwrap();
        assert_eq!(indices, [7, 8, 9, 10, 11]);
        let (_, indices) = fuzzy("ab", "xab ab").unwrap();
        assert_eq!(indices, [4, 5], "the word start wins");
        let (_, indices) = fuzzy("ab", "ab ab").unwrap();
        assert_eq!(indices, [0, 1], "ties go to the earliest chars");
        // Word starts and runs beat scattered chars.
        let start = fuzzy("st", "Git: Stage").unwrap().0;
        let scattered = fuzzy("st", "Last item").unwrap().0;
        assert!(start > scattered, "{start} vs {scattered}");
        let camel = fuzzy("z", "OpenInZed").unwrap().0;
        let inner = fuzzy("z", "Puzzle").unwrap().0;
        assert!(camel > inner);
    }

    #[test]
    fn typing_filters_ranks_and_resets_the_pick() {
        let mut palette = Palette::new(entries(
            &Keymap::defaults(),
            &context(View::Git),
            &[],
            &[],
            &[],
        ));
        let all = palette.rows().len();
        assert_eq!(palette.picked(), Some(Action::Run(Command::AddWorkspace)));
        palette.step(-1);
        assert_eq!(palette.selected, all - 1, "up from the top wraps");

        palette.key(Key::Insert("vg".into()));
        assert_eq!(palette.selected, 0);
        // "View: Git" (two word starts) beats later scattered matches.
        assert_eq!(palette.picked(), Some(Action::Run(Command::ShowGit)));
        let rows = palette.rows();
        assert_eq!(rows[0].entry.title, "View: Git");
        assert_eq!(rows[0].ranges, [0..1, 6..7]);
        assert_eq!(rows[0].entry.shortcut, "Ctrl+Shift+5");

        palette.key(Key::Insert("zzz".into()));
        assert!(palette.rows().is_empty());
        assert_eq!(palette.picked(), None);
        palette.step(1);
        palette.pick(0);
        assert_eq!(palette.selected, 0);

        for _ in 0..3 {
            palette.key(Key::Backspace);
        }
        assert_eq!(palette.query(), "vg");
        palette.key(Key::Move(1));
        palette.pick(0);
        assert_eq!(palette.picked(), Some(Action::Run(Command::ShowGit)));
        palette.pick(999);
        assert_eq!(palette.selected, 0, "out of range ignored");

        palette.insert("\r\n");
        assert_eq!(palette.query(), "vg");
        palette.key(Key::Paste);
        palette.key(Key::Open);
        palette.key(Key::Close);
        assert_eq!(palette.query(), "vg");
    }

    #[test]
    fn the_query_takes_select_all_and_ctrl_backspace() {
        let mut palette = Palette::new(Vec::new());
        palette.insert("git push");
        palette.key(Key::DeleteWord);
        assert_eq!(palette.query(), "git ");
        palette.key(Key::SelectAll);
        assert!(palette.query_selected());
        palette.key(Key::Insert("view".into()));
        assert_eq!((palette.query(), palette.query_selected()), ("view", false));
    }
}
