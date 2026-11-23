//! The palette (Ctrl+Shift+P, SPEC §13): one list with scope tabs (All, Commands, Sessions,
//! Files, Search, Branches, Commits). Commands are every keymap command that can act in the
//! current view plus palette-only actions that have no key; Sessions are "Go to" workspace /
//! session / task items; Files, Search, Branches and Commits are asked of the daemon
//! (`fs.find`, `fs.search`, `git.branches`, `git.log`), each call numbered so an old answer
//! is dropped. Typing filters with a fuzzy subsequence match ranked by word starts and runs;
//! Enter picks. `Ctrl+Shift+O` opens it on Files, `Ctrl+Alt+Shift+F` on Search, `@` in a
//! prompt on Files in insert mode. Pure and tested headlessly; `main.rs` renders it and runs
//! the pick.

use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use gpui::Keystroke;
use mkrew_core::settings::FeatureSettings;
use mkrew_protocol::{
    Branch, Commit, FsFindParams, FsMatch, FsSearchParams, FsSearchResult, GitBranchesParams,
    GitLogParams, Project, SearchHit, Session, SessionKind, SessionState, Task,
};
use ulid::Ulid;

use crate::board::CardKey;
use crate::board::View;
use crate::history;
use crate::keymap::{Command, Keymap, KeymapError};
use crate::split::Direction;
use crate::task_dialog::{DialogKey, dialog_key};
use crate::text_box::Input;
use crate::theme;

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
    /// A file pane had the keyboard (save, find, preview… act on it).
    pub file_pane: bool,
    /// The file tree is painted in the right panel (Open in a kept tab acts on its cursor).
    pub file_tree: bool,
    /// A terminal pane had the keyboard (copy, paste, find… act on it).
    pub terminal: bool,
    /// The Git list's selected file can be discarded (unstaged or untracked).
    pub discard: bool,
    /// The active workspace's Git is on screen (the panel's Git tab or a diff pane): the
    /// Git keys act.
    pub git: bool,
    /// The focused pane is the diff: the diff comments and Send selection act on it.
    pub diff_pane: bool,
    /// The selected board card offers "View changes".
    pub task_changes: bool,
    /// The selected board card offers "Merge back…" and "Remove worktree…".
    pub worktree_task: bool,
    /// The selected board card offers "Run with several agents…".
    pub fan_out: bool,
    /// The active workspace has a live agent session.
    pub agents: bool,
    /// The terminal pane that had the keyboard runs an agent (it has a composer).
    pub agent_pane: bool,
    /// The focused pane's session can be marked unread (a finished agent, `inbox::can_mark_unread`).
    pub markable: bool,
    /// "Run: Start all" would start something (no command yet: it opens Run setup).
    pub run_stopped: bool,
    /// A run session of the active workspace lives.
    pub run_live: bool,
    /// The active terminal tab has other unpinned tabs ("Close others").
    pub tab_others: bool,
    /// The active terminal tab has unpinned tabs to its right.
    pub tab_right: bool,
    /// A terminal pane is focused.
    pub pane: bool,
    /// The focused pane's session can be stopped (`inspector::stop_action`).
    pub live_session: bool,
    /// The focused agent pane's session has a transcript file.
    pub transcript: bool,
    /// The focused agent pane can show Chat (a readable transcript format).
    pub chat: bool,
    /// The focused agent pane's session has a linked plan.
    pub plan_linked: bool,
    /// The focused pane's prompt composer is open.
    pub composer_open: bool,
    /// The selected board card is in Review (`a` approves it).
    pub approve_card: bool,
    /// The Git list shows Against base.
    pub git_base: bool,
    /// The Git list shows several repositories.
    pub git_repos: bool,
    /// The Board shows Jira.
    pub jira: bool,
    /// An error toast was shown this run.
    pub last_error: bool,
    /// A toast is shown.
    pub toast: bool,
    /// An update is ready to install.
    pub update_ready: bool,
    /// Running from `Mkrew.app`: "Install mk command" has something to link.
    pub mk_install: bool,
    /// A machine's window: Phone access and the in-app update belong to this computer's.
    pub machine: bool,
    /// The palette was opened in a detached window (ADR 0096): only its commands are listed.
    pub detached_window: bool,
    /// `[features]` (ADR 0046): Docs and Turns entries are listed only when on.
    pub features: FeatureSettings,
    /// Idle agents and terminals the sidebar could remove (`Sidebar::idle_agents_and_terminals`).
    pub idle_removable: usize,
    /// Some workspace is expanded in the sidebar (`Sidebar::any_expanded`).
    pub workspaces_open: bool,
}

/// A palette-only action: no key is bound to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extra {
    ViewTaskChanges,
    MergeBack,
    RemoveWorktree,
    FanOut,
    Broadcast,
    InsertPrompt,
    ManagePrompts,
    Turns,
    FollowInZed,
    RunSetup,
    RunShowTab,
    Ports,
    PhoneAccess,
    CheckUpdates,
    MarkUnread,
    WorkspacePin,
    WorkspaceExplorer,
    WorkspaceDefaultAgent,
    WorkspaceRemove,
    WorkspaceMenu,
    TabPin,
    TabCloseOthers,
    TabCloseRight,
    TabMoveToMainWindow,
    WindowMoveAllBack,
    WindowClose,
    PanePin,
    PaneReading,
    WorkspaceDefaultShell,
    WorkspaceMoveUp,
    WorkspaceMoveDown,
    AgentStop,
    AgentTranscript,
    AgentLinkPlan,
    AgentUnlinkPlan,
    AgentImprove,
    GitChangeBase,
    GitSwitchRepo,
    BoardScope,
    JiraMine,
    JiraLayout,
    JiraRefresh,
    CopyLastError,
    DismissToasts,
    History,
    InstallUpdate,
    AddMachine,
    InstallMkCommand,
    /// The Appearance gallery's two buttons (ADR 0101), for the keyboard.
    NewThemeFromCurrent,
    OpenThemesFolder,
    /// The sidebar's former header buttons: Remove idle (shown while something is idle) and
    /// Collapse / Expand all.
    SidebarRemoveIdle,
    SidebarToggleAll,
}

impl Extra {
    pub const ALL: [Self; 52] = [
        Self::ViewTaskChanges,
        Self::MergeBack,
        Self::RemoveWorktree,
        Self::FanOut,
        Self::Broadcast,
        Self::InsertPrompt,
        Self::ManagePrompts,
        Self::Turns,
        Self::FollowInZed,
        Self::RunSetup,
        Self::RunShowTab,
        Self::Ports,
        Self::PhoneAccess,
        Self::CheckUpdates,
        Self::MarkUnread,
        Self::WorkspacePin,
        Self::WorkspaceExplorer,
        Self::WorkspaceDefaultAgent,
        Self::WorkspaceRemove,
        Self::WorkspaceMenu,
        Self::TabPin,
        Self::TabCloseOthers,
        Self::TabCloseRight,
        Self::TabMoveToMainWindow,
        Self::WindowMoveAllBack,
        Self::WindowClose,
        Self::PanePin,
        Self::PaneReading,
        Self::WorkspaceDefaultShell,
        Self::WorkspaceMoveUp,
        Self::WorkspaceMoveDown,
        Self::AgentStop,
        Self::AgentTranscript,
        Self::AgentLinkPlan,
        Self::AgentUnlinkPlan,
        Self::AgentImprove,
        Self::GitChangeBase,
        Self::GitSwitchRepo,
        Self::BoardScope,
        Self::JiraMine,
        Self::JiraLayout,
        Self::JiraRefresh,
        Self::CopyLastError,
        Self::DismissToasts,
        Self::History,
        Self::InstallUpdate,
        Self::AddMachine,
        Self::InstallMkCommand,
        Self::NewThemeFromCurrent,
        Self::OpenThemesFolder,
        Self::SidebarRemoveIdle,
        Self::SidebarToggleAll,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::ViewTaskChanges => "Task: View changes of selected task",
            Self::MergeBack => "Task: Merge back selected task…",
            Self::RemoveWorktree => "Task: Remove worktree of selected task…",
            Self::FanOut => "Agent: Run selected task with several agents…",
            Self::Broadcast => "Agent: Broadcast to agents…",
            Self::InsertPrompt => "Agent: Insert prompt…",
            Self::ManagePrompts => "Global: Manage prompts…",
            Self::Turns => "Agent: Turns…",
            Self::FollowInZed => "Agent: Follow in Zed (on/off)",
            Self::RunSetup => "Run: Setup…",
            Self::RunShowTab => "Run: Show Run tab",
            Self::Ports => "Ports: Show…",
            Self::PhoneAccess => "Global: Phone access…",
            Self::CheckUpdates => "Global: Check for updates",
            Self::MarkUnread => "Agent: Mark unread",
            Self::WorkspacePin => "Workspace: Pin / Unpin",
            Self::WorkspaceExplorer => crate::os_text!(
                "Workspace: Open in File Explorer",
                "Workspace: Reveal in Finder",
            ),
            Self::WorkspaceDefaultAgent => "Workspace: Set default agent…",
            Self::WorkspaceRemove => "Workspace: Remove…",
            Self::WorkspaceMenu => "Workspace: Open workspace menu",
            Self::TabPin => "Tab: Pin / Unpin",
            Self::TabCloseOthers => "Tab: Close others",
            Self::TabCloseRight => "Tab: Close tabs to the right",
            Self::TabMoveToMainWindow => "Tab: Move to main window",
            Self::WindowMoveAllBack => "Window: Move all tabs back",
            Self::WindowClose => "Window: Close",
            Self::PanePin => "Pane: Pin / Unpin",
            Self::PaneReading => "Pane: Reading width (on / off)",
            Self::WorkspaceDefaultShell => "Workspace: Set default shell…",
            Self::WorkspaceMoveUp => "Workspace: Move up",
            Self::WorkspaceMoveDown => "Workspace: Move down",
            Self::AgentStop => "Agent: Stop session…",
            Self::AgentTranscript => "Agent: Open transcript",
            Self::AgentLinkPlan => "Agent: Link plan…",
            Self::AgentUnlinkPlan => "Agent: Unlink plan",
            Self::AgentImprove => "Agent: Improve prompt",
            Self::GitChangeBase => "Git: Change base…",
            Self::GitSwitchRepo => "Git: Switch repository…",
            Self::BoardScope => "Board: Toggle This workspace / All workspaces",
            Self::JiraMine => "Jira: Toggle All / Mine",
            Self::JiraLayout => "Jira: Toggle List / Kanban",
            Self::JiraRefresh => "Jira: Refresh",
            Self::CopyLastError => "Global: Copy last error",
            Self::DismissToasts => "Global: Dismiss notifications",
            Self::History => "Global: History…",
            Self::InstallUpdate => "Global: Install update",
            Self::AddMachine => "Machine: Add…",
            Self::InstallMkCommand => "Global: Install mk command",
            Self::NewThemeFromCurrent => "Theme: New theme from current",
            Self::OpenThemesFolder => "Theme: Open themes folder",
            Self::SidebarRemoveIdle => "Sidebar: Remove idle agents and terminals",
            Self::SidebarToggleAll => "Sidebar: Collapse / Expand all workspaces",
        }
    }

    /// The title shown in the palette: [`Self::title`], with the count of the idle agents and
    /// terminals the sidebar action removes, and the toggle named for what it does now.
    pub fn label(self, context: &Context) -> String {
        match self {
            Self::SidebarRemoveIdle => format!("{} ({})", self.title(), context.idle_removable),
            Self::SidebarToggleAll if context.workspaces_open => {
                "Sidebar: Collapse all workspaces".to_owned()
            }
            Self::SidebarToggleAll => "Sidebar: Expand all workspaces".to_owned(),
            _ => self.title().to_owned(),
        }
    }

    /// Whether the action can act in `context` (where its buttons are on screen).
    pub fn available(self, context: &Context) -> bool {
        let in_view = |view| context.workspace && context.view == view;
        match self {
            Self::ViewTaskChanges => in_view(View::Board) && context.task_changes,
            Self::MergeBack | Self::RemoveWorktree => in_view(View::Board) && context.worktree_task,
            Self::FanOut => in_view(View::Board) && context.fan_out,
            Self::Broadcast => context.workspace && context.agents,
            // Into the focused agent pane's composer (opened if hidden); the focused agent
            // pane's "Turns…" and Share → "Follow in Zed".
            Self::Turns => {
                context.features.turns
                    && in_view(View::Terminals)
                    && context.tabs > 0
                    && context.agent_pane
            }
            Self::InsertPrompt | Self::FollowInZed => {
                in_view(View::Terminals) && context.tabs > 0 && context.agent_pane
            }
            Self::RunShowTab => context.workspace && context.run_live,
            Self::RunSetup
            | Self::WorkspacePin
            | Self::WorkspaceExplorer
            | Self::WorkspaceDefaultAgent
            | Self::WorkspaceRemove
            | Self::WorkspaceMenu => context.workspace,
            Self::MarkUnread => context.markable,
            Self::TabPin => in_view(View::Terminals) && context.tabs > 0,
            Self::TabCloseOthers => in_view(View::Terminals) && context.tab_others,
            Self::TabCloseRight => in_view(View::Terminals) && context.tab_right,
            Self::TabMoveToMainWindow | Self::WindowMoveAllBack | Self::WindowClose => {
                context.detached_window
            }
            Self::PanePin => in_view(View::Terminals) && context.tabs > 0 && context.pane,
            Self::PaneReading | Self::AgentLinkPlan => {
                in_view(View::Terminals) && context.tabs > 0 && context.agent_pane
            }
            Self::WorkspaceDefaultShell | Self::WorkspaceMoveUp | Self::WorkspaceMoveDown => {
                context.workspace
            }
            Self::AgentStop => in_view(View::Terminals) && context.live_session,
            Self::AgentTranscript => in_view(View::Terminals) && context.transcript,
            Self::AgentUnlinkPlan => {
                in_view(View::Terminals) && context.agent_pane && context.plan_linked
            }
            Self::AgentImprove => in_view(View::Terminals) && context.composer_open,
            Self::PhoneAccess | Self::CheckUpdates | Self::AddMachine => !context.machine,
            Self::Ports
            | Self::ManagePrompts
            | Self::NewThemeFromCurrent
            | Self::OpenThemesFolder => true,
            Self::GitChangeBase => context.workspace && context.git && context.git_base,
            Self::GitSwitchRepo => context.workspace && context.git && context.git_repos,
            // The switch heads the Tasks side of the Board.
            Self::BoardScope => in_view(View::Board) && !context.jira,
            Self::JiraMine | Self::JiraLayout | Self::JiraRefresh => {
                in_view(View::Board) && context.jira
            }
            Self::CopyLastError => context.last_error,
            Self::DismissToasts => context.toast,
            Self::History => context.workspace,
            Self::InstallUpdate => context.update_ready,
            Self::InstallMkCommand => context.mk_install,
            Self::SidebarRemoveIdle => context.idle_removable > 0,
            Self::SidebarToggleAll => context.workspaces > 0,
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

/// What the palette searches (Polish spec 2026-11-21 §7.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    All,
    Commands,
    Sessions,
    Files,
    Search,
    Branches,
    Commits,
}

impl Scope {
    /// The tab order: Tab moves to the next scope, Shift+Tab to the previous one, wrapping.
    pub const ORDER: [Scope; 7] = [
        Scope::All,
        Scope::Commands,
        Scope::Sessions,
        Scope::Files,
        Scope::Search,
        Scope::Branches,
        Scope::Commits,
    ];

    /// The tab label and the section overline.
    pub fn title(self) -> &'static str {
        match self {
            Scope::All => "All",
            Scope::Commands => "Commands",
            Scope::Sessions => "Sessions",
            Scope::Files => "Files",
            Scope::Search => "Search",
            Scope::Branches => "Branches",
            Scope::Commits => "Commits",
        }
    }

    /// The muted line when the scope lists nothing and nothing is typed.
    pub fn empty(self) -> &'static str {
        match self {
            Scope::All => "Nothing to show yet.",
            Scope::Commands => "No commands here.",
            Scope::Sessions => "No sessions yet.",
            Scope::Files => "No files in this workspace.",
            Scope::Search => "Type to search the files.",
            Scope::Branches => "No branches in this repository.",
            Scope::Commits => "No commits in this repository.",
        }
    }

    /// The query field's placeholder.
    pub fn placeholder(self) -> &'static str {
        match self {
            Scope::All => "Search commands, sessions, files, branches and commits",
            Scope::Commands => "Type a command",
            Scope::Sessions => "Type a workspace, session or task",
            Scope::Files => "Type part of a file name or path",
            Scope::Search => "Search the text of the files",
            Scope::Branches => "Type a branch name",
            Scope::Commits => "Type a commit message or hash",
        }
    }

    pub fn next(self) -> Scope {
        self.shifted(1)
    }

    pub fn prev(self) -> Scope {
        self.shifted(Self::ORDER.len() - 1)
    }

    fn shifted(self, by: usize) -> Scope {
        let at = Self::ORDER
            .iter()
            .position(|scope| *scope == self)
            .unwrap_or(0);
        Self::ORDER[(at + by) % Self::ORDER.len()]
    }
}

/// Tab (`Some(1)`) and Shift+Tab (`Some(-1)`) without another modifier: the scope switch of
/// the open palette. Every other key is not one.
pub fn scope_step(keystroke: &Keystroke) -> Option<isize> {
    let m = keystroke.modifiers;
    (keystroke.key == "tab" && !m.control && !m.alt && !m.platform).then_some(if m.shift {
        -1
    } else {
        1
    })
}

/// The scope a palette-opening command opens on (`Ctrl+Shift+O` Files, `Ctrl+Alt+Shift+F`
/// Search, `Ctrl+Shift+P` All); `None` for every other command.
pub fn scope_for(command: Command) -> Option<Scope> {
    match command {
        Command::QuickOpen => Some(Scope::Files),
        Command::SearchInFiles => Some(Scope::Search),
        Command::CommandPalette => Some(Scope::All),
        _ => None,
    }
}

/// What picking an entry does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Run(Command),
    Extra(Extra),
    GoTo(Target),
    /// The theme picker's row for the theme at this index of the theme catalog (ADR 0101).
    Theme(usize),
    /// "Agent: New <Profile> pane": a new pane with the profile at this position of the
    /// shell's profile list (see [`agent_entries`]).
    NewAgentPane(usize),
    /// Run: Start / Restart / Stop `<command>`: the `usize`-th command of the active workspace's
    /// Run summary (like `NewAgentPane`).
    RunCommand(crate::run::RunAction, usize),
    /// "Machine: Open <name>": the `usize`-th machine of the shell's list (see
    /// [`machine_entries`]).
    OpenMachine(usize),
    /// Files scope: open the file in the Files view (`@` insert mode types its path instead).
    OpenFile {
        path: String,
    },
    /// Search scope: open the file with the caret on `line` (1-based).
    OpenSearchHit {
        path: String,
        line: u32,
    },
    /// Branches scope: `git switch` (after a question when something is at risk). `repo` is
    /// the repository of the Git view (ADR 0029), `None` = the workspace folder. `current`
    /// marks the branch already checked out: the pick only says so.
    Checkout {
        repo: Option<String>,
        branch: String,
        current: bool,
    },
    /// Commits scope: the Git view's History on that commit.
    ShowCommit {
        repo: Option<String>,
        sha: String,
    },
}

impl Action {
    /// The scope whose section lists this action.
    pub fn scope(&self) -> Scope {
        match self {
            Action::Run(_)
            | Action::Extra(_)
            | Action::Theme(_)
            | Action::NewAgentPane(_)
            | Action::RunCommand(..)
            | Action::OpenMachine(_) => Scope::Commands,
            Action::GoTo(_) => Scope::Sessions,
            Action::OpenFile { .. } => Scope::Files,
            Action::OpenSearchHit { .. } => Scope::Search,
            Action::Checkout { .. } => Scope::Branches,
            Action::ShowCommit { .. } => Scope::Commits,
        }
    }
}

/// Whether picking `action` does something in a detached window (ADR 0096): its own tab's
/// commands, its window extras and the jumps to a session. The palette lists nothing else there.
pub fn acts_in_detached_window(action: &Action) -> bool {
    match action {
        Action::Run(command) => crate::detached::handles(*command),
        Action::Extra(extra) => matches!(
            extra,
            Extra::TabMoveToMainWindow
                | Extra::WindowMoveAllBack
                | Extra::WindowClose
                | Extra::TabPin
        ),
        Action::GoTo(Target::Session(_)) => true,
        _ => false,
    }
}

/// One palette line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub action: Action,
    /// Matched against the query, e.g. "Tabs: New shell tab".
    pub title: String,
    /// Shortcut label, e.g. "Ctrl+Shift+T"; empty if none.
    pub shortcut: String,
    /// Muted text after the title: a path, a short hash, a session state, a branch note.
    pub detail: Option<String>,
    /// Right-aligned age of a commit or branch (`git_history::age`).
    pub age: Option<String>,
    /// Byte ranges of `title` to highlight when the daemon ranked the entry (Files, Search);
    /// empty = highlight what the query matched.
    pub marks: Vec<Range<usize>>,
}

impl Entry {
    pub fn new(action: Action, title: impl Into<String>, shortcut: impl Into<String>) -> Self {
        Self {
            action,
            title: title.into(),
            shortcut: shortcut.into(),
            detail: None,
            age: None,
            marks: Vec::new(),
        }
    }
}

/// A shown entry with the byte ranges of its title the query matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row<'a> {
    pub entry: &'a Entry,
    pub ranges: Vec<Range<usize>>,
    /// The scope section the entry belongs to.
    pub section: Scope,
    pub detail: Option<&'a str>,
    pub age: Option<&'a str>,
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
        Command::AddWorkspace => "Workspace: Add workspace (folder or clone repository)".into(),
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
        Command::Layout(preset) => format!("Panes: Layout {}", preset.label()),
        Command::EqualizePanes => "Panes: Equalize pane sizes".into(),
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
        Command::PreviousInbox => "Inbox: Jump to previous item needing attention".into(),
        Command::Peek => "Agent: Peek the first Inbox entry".into(),
        Command::OpenComposer => "Agent: Open the prompt composer".into(),
        Command::ToggleChat => "Agent: Toggle chat view".into(),
        Command::SendSelection => "Agent: Send selection to an agent".into(),
        Command::CopyMode => "Terminal: Copy mode (select and copy with the keyboard)".into(),
        Command::LinkHints => "Terminal: Open a link by its label".into(),
        Command::ShowBoard => "View: Board".into(),
        Command::ShowTerminals => "View: Terminals".into(),
        Command::ShowDocs => "View: Docs".into(),
        Command::ShowFileTree => "View: File tree".into(),
        Command::ShowGitPanel => "View: Git panel".into(),
        Command::ShowDiff => "View: Diff".into(),
        Command::ToggleDocSource => "Docs: Toggle preview / source".into(),
        Command::AddComment => "Comments: Comment on selected lines".into(),
        Command::SendComments => "Comments: Send comments to agent".into(),
        Command::PickDown => "Comments: Pick the next line or block (grow the pick down)".into(),
        Command::PickUp => "Comments: Pick the previous line or block (grow the pick up)".into(),
        Command::NextComment => "Comments: Next comment thread".into(),
        Command::PreviousComment => "Comments: Previous comment thread".into(),
        Command::ReplyComment => "Comments: Reply to the selected thread".into(),
        Command::ResolveComment => "Comments: Resolve or reopen the selected thread".into(),
        Command::MoveTabLeft => "Tabs: Move tab left".into(),
        Command::MoveTabRight => "Tabs: Move tab right".into(),
        Command::MoveTabToNewWindow => "Tab: Move to new window".into(),
        Command::ToggleIgnored => "File tree: Toggle ignored files".into(),
        Command::OpenKept => "File tree: Open file in a kept tab".into(),
        Command::SaveFile => "Editor: Save file".into(),
        Command::FindInFile => "Editor: Find in file".into(),
        Command::ReplaceInFile => "Editor: Replace in file".into(),
        Command::CloseFileTab => "File: Close file".into(),
        Command::ToggleFilePreview => "Editor: Toggle preview / source".into(),
        Command::ToggleFileSplit => "Editor: Toggle split source | preview".into(),
        Command::QuickOpen => "Files: Quick open file".into(),
        Command::SearchInFiles => "Files: Search in files".into(),
        Command::GitToggleMode => "Git: Toggle Working changes / Against base".into(),
        Command::GitToggleHistory => "Git: Toggle History".into(),
        Command::GitNextHunk => "Git: Next hunk".into(),
        Command::GitPreviousHunk => "Git: Previous hunk".into(),
        Command::GitStage => "Git: Stage selected file or hunk".into(),
        Command::GitUnstage => "Git: Unstage selected file or hunk".into(),
        Command::GitFocusCommit => "Git: Focus commit message".into(),
        Command::GitToggleDiffMode => "Git: Toggle Unified / Split diff".into(),
        Command::GitToggleWrap => "Git: Toggle Wrap of the diff".into(),
        Command::GitToggleViewed => "Git: Mark file as viewed / not viewed".into(),
        Command::GitStageAll => "Git: Stage all (selected group, else every change)".into(),
        Command::GitUnstageAll => "Git: Unstage all".into(),
        Command::GitDiscard => "Git: Discard changes to selected file or hunk…".into(),
        Command::GitFetch => "Git: Fetch".into(),
        Command::GitPush => "Git: Push".into(),
        Command::NewTask => "Task: New task".into(),
        Command::RunTask => "Task: Run selected task".into(),
        Command::ApproveCard => "Task: Approve selected task".into(),
        Command::Card(key) => match key {
            CardKey::Up => "Board: Select card above".into(),
            CardKey::Down => "Board: Select card below".into(),
            CardKey::Left => "Board: Select card in the column to the left".into(),
            CardKey::Right => "Board: Select card in the column to the right".into(),
            CardKey::MoveLeft => "Board: Move selected card one column left".into(),
            CardKey::MoveRight => "Board: Move selected card one column right".into(),
            CardKey::Open => "Board: Open selected card (session, else edit)".into(),
            CardKey::Edit => "Board: Edit selected task".into(),
        },
        Command::RunStart => "Run: Start all".into(),
        Command::RunRestart => "Run: Restart all".into(),
        Command::RunStop => "Run: Stop all".into(),
        Command::CommandPalette => "Global: Command palette".into(),
        Command::OpenSettings => "Global: Open settings".into(),
        Command::ToggleTheme => "Global: Toggle theme (System → Light → Dark)".into(),
        Command::SelectTheme => "Theme: Select theme…".into(),
        Command::ToggleFullScreen => "Global: Toggle full screen".into(),
        Command::ToggleSidebar => "Global: Toggle sidebar".into(),
        Command::ToggleCompactSidebar => "Global: Toggle compact sidebar".into(),
        Command::ToggleInspector => "View: Toggle right panel".into(),
        Command::VoiceInput => "Global: Voice input (start / stop)".into(),
        Command::AskAi => "Global: Ask AI…".into(),
        Command::ContextMenu => "Global: Open context menu".into(),
        Command::FocusSidebar => "Sidebar: Focus the filter".into(),
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
        | Command::PreviousInbox
        | Command::Peek
        | Command::OpenSettings
        | Command::ToggleTheme
        | Command::SelectTheme
        | Command::ToggleFullScreen
        | Command::ToggleSidebar
        | Command::ToggleCompactSidebar
        | Command::VoiceInput
        | Command::AskAi => true,
        Command::Workspace(n) => usize::from(n) <= context.workspaces,
        Command::NextWorkspace | Command::PreviousWorkspace => context.workspaces > 1,
        Command::NewShellTab
        | Command::NewAgentTab
        | Command::OpenInZed
        | Command::ShowBoard
        | Command::ShowTerminals
        | Command::ShowFileTree
        | Command::ShowGitPanel
        | Command::SearchInFiles
        | Command::QuickOpen
        | Command::ToggleInspector
        | Command::NewTask
        | Command::ContextMenu
        | Command::ShowDiff
        | Command::FocusSidebar => context.workspace,
        Command::ShowDocs => context.workspace && context.features.docs,
        Command::NextTab | Command::PreviousTab => context.workspace && context.tabs > 1,
        Command::Tab(n) => context.workspace && usize::from(n) <= context.tabs,
        Command::SplitRight
        | Command::SplitDown
        | Command::SplitAgent
        | Command::SplitShell
        | Command::Layout(_) => context.workspace && context.tabs > 0,
        Command::RenameTab
        | Command::CloseTab
        | Command::Focus(_)
        | Command::Resize(_)
        | Command::EqualizePanes
        | Command::ZoomPane
        | Command::RenamePane
        | Command::ClosePane => on_panes,
        Command::Copy
        | Command::Paste
        | Command::PageUp
        | Command::PageDown
        | Command::CopyMode
        | Command::LinkHints => on_panes && context.terminal,
        Command::Find => (on_panes && context.terminal) || context.file_pane || in_view(View::Docs),
        Command::OpenComposer => on_panes && context.agent_pane,
        Command::ToggleChat => on_panes && context.agent_pane && context.chat,
        Command::SendSelection => {
            context.file_pane
                || context.diff_pane
                || in_view(View::Docs)
                || (on_panes && context.terminal)
        }
        Command::RunTask => in_view(View::Board),
        Command::ApproveCard => in_view(View::Board) && context.approve_card,
        Command::RunStart => context.workspace && context.run_stopped,
        Command::RunRestart | Command::RunStop => context.workspace && context.run_live,
        Command::ToggleDocSource => in_view(View::Docs),
        Command::AddComment
        | Command::SendComments
        | Command::PickDown
        | Command::PickUp
        | Command::NextComment
        | Command::PreviousComment
        | Command::ReplyComment
        | Command::ResolveComment => in_view(View::Docs) || context.diff_pane,
        Command::MoveTabLeft | Command::MoveTabRight => on_panes,
        Command::MoveTabToNewWindow => {
            context.workspace && context.view == View::Terminals && context.tabs > 0
        }
        Command::ToggleIgnored => context.workspace,
        Command::OpenKept => context.workspace && context.file_tree,
        Command::SaveFile
        | Command::FindInFile
        | Command::ReplaceInFile
        | Command::CloseFileTab
        | Command::ToggleFilePreview
        | Command::ToggleFileSplit => context.file_pane,
        Command::GitToggleMode
        | Command::GitToggleHistory
        | Command::GitNextHunk
        | Command::GitPreviousHunk
        | Command::GitStage
        | Command::GitUnstage
        | Command::GitToggleDiffMode
        | Command::GitToggleWrap
        | Command::GitToggleViewed
        | Command::GitStageAll
        | Command::GitUnstageAll
        | Command::GitFetch
        | Command::GitPush
        | Command::GitFocusCommit => context.workspace && context.git,
        Command::GitDiscard => context.workspace && context.git && context.discard,
        Command::Card(_) => in_view(View::Board),
    }
}

/// One entry per `keymap.toml` problem found at start (SPEC §13), listed before the
/// commands; picking one opens the keymap settings to fix it.
pub fn keymap_errors(keymap: &Keymap, errors: &[KeymapError]) -> Vec<Entry> {
    errors
        .iter()
        .map(|error| {
            Entry::new(
                Action::Run(Command::OpenSettings),
                format!("Keymap error: {error}"),
                keymap.label(Command::OpenSettings),
            )
        })
        .collect()
}

/// The `settings.toml` problem (SPEC §20.0), listed before the commands like keymap errors;
/// picking it opens the Settings dialog, which shows the problem and "Fix in Zed".
pub fn settings_error(keymap: &Keymap, error: Option<&str>) -> Vec<Entry> {
    error
        .map(|error| {
            Entry::new(
                Action::Run(Command::OpenSettings),
                format!("Settings error: {error}"),
                keymap.label(Command::OpenSettings),
            )
        })
        .into_iter()
        .collect()
}

/// The theme picker's rows (ADR 0078, ADR 0101): every theme of the catalog, its appearance
/// on the right, the shown one marked. `Action::Theme(i)` is the index into the catalog.
pub fn theme_entries(catalog: &crate::theme_catalog::ThemeCatalog, current: &str) -> Vec<Entry> {
    catalog
        .themes()
        .iter()
        .enumerate()
        .map(|(index, listed)| {
            let look = if listed.dark { "Dark" } else { "Light" };
            Entry::new(
                Action::Theme(index),
                listed.name.to_string(),
                if &*listed.name == current {
                    format!("✓ {look}")
                } else {
                    look.into()
                },
            )
        })
        .collect()
}

/// "Agent: New <Profile> pane", one entry per installed agent profile (`agents`, the default
/// first, see [`crate::lifecycle::pane_agents`]) where "New agent pane" is available. The
/// default shows the key that starts it.
pub fn agent_entries(
    keymap: &Keymap,
    context: &Context,
    agents: &[crate::lifecycle::PaneAgent],
) -> Vec<Entry> {
    if !available(Command::SplitAgent, context) {
        return Vec::new();
    }
    agents
        .iter()
        .map(|agent| {
            Entry::new(
                Action::NewAgentPane(agent.index),
                format!("Agent: New {} pane", agent.name),
                if agent.default {
                    keymap.label(Command::SplitAgent)
                } else {
                    String::new()
                },
            )
        })
        .collect()
}

/// "Run: Start / Restart / Stop <command>", one group per command of the workspace in Run menu
/// order (a running command offers Restart and Stop, any other Start), like the Run menu's
/// row buttons. The `usize` is the row's place in `summary`, resolved again when picked.
pub fn run_entries(summary: &crate::run::Summary) -> Vec<Entry> {
    use crate::run::{RowState, RunAction};
    let mut entries = Vec::new();
    for (index, row) in summary.rows.iter().enumerate() {
        let actions: &[(RunAction, &str)] = if matches!(row.state, RowState::Running(_)) {
            &[(RunAction::Restart, "Restart"), (RunAction::Stop, "Stop")]
        } else {
            &[(RunAction::Start, "Start")]
        };
        for &(action, verb) in actions {
            entries.push(Entry::new(
                Action::RunCommand(action, index),
                format!("Run: {verb} {}", row.name),
                "",
            ));
        }
    }
    entries
}

/// "Machine: Open <name>", one entry per machine of `machines.toml` (this computer's window only).
pub fn machine_entries(context: &Context, names: &[String]) -> Vec<Entry> {
    if context.machine {
        return Vec::new();
    }
    names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            Entry::new(
                Action::OpenMachine(index),
                format!("Machine: Open {name}"),
                "",
            )
        })
        .collect()
}

/// The palette's entries: "Go to" every session that needs input first, then the available
/// commands in keymap order, the available palette-only actions, then "Go to" every workspace,
/// other live session and task (archived workspaces and their items left out). Session rows
/// show their state as the muted detail.
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
        .map(|command| Entry::new(Action::Run(command), title(command), keymap.label(command)))
        .collect();
    entries.extend(
        Extra::ALL
            .into_iter()
            .filter(|extra| extra.available(context))
            .map(|extra| Entry::new(Action::Extra(extra), extra.label(context), "")),
    );
    let workspace = |id: Ulid| {
        projects
            .iter()
            .find(|project| project.id == id && !project.archived)
            .map(|project| project.name.as_str())
    };
    let go_to = |target, title: String| Entry::new(Action::GoTo(target), title, "");
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
    let (asking, others): (Vec<_>, Vec<_>) = sessions
        .iter()
        .filter(|session| session.state != SessionState::Exited)
        .filter_map(|session| {
            let name = workspace(session.project_id)?;
            let status = crate::sidebar::status_display(session.state);
            let entry = Entry {
                detail: Some(format!("{} {}", status.icon, status.label)),
                ..go_to(
                    Target::Session(session.id),
                    format!("Go to session: {} · {name}", history::label(session)),
                )
            };
            Some((session.state == SessionState::NeedsInput, entry))
        })
        .partition(|(asking, _)| *asking);
    entries.extend(others.into_iter().map(|(_, entry)| entry));
    entries.splice(0..0, asking.into_iter().map(|(_, entry)| entry));
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

/// What a key press in a picker's query field means: the palette, the prompt and `/` pickers
/// and the filters of the Docs, Git and Jira lists share this map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    /// Esc.
    Close,
    /// ↑ / ↓ (-1 / +1).
    Move(i32),
    /// Enter (or Ctrl+Enter).
    Open,
    /// Ctrl+V: the caller inserts the clipboard text.
    Paste,
    Insert(String),
    /// An editing or caret key for [`Input::edit`] (Backspace, Ctrl+A, ← →, Ctrl+Z…). On
    /// Copy / Cut the caller puts the selection on the clipboard first.
    Edit(DialogKey),
}

/// The picker meaning of `keystroke`; `None` = ignored.
pub fn key(keystroke: &Keystroke) -> Option<Key> {
    if !keystroke.modifiers.modified() {
        match keystroke.key.as_str() {
            "up" => return Some(Key::Move(-1)),
            "down" => return Some(Key::Move(1)),
            _ => {}
        }
    }
    match dialog_key(keystroke)? {
        DialogKey::Cancel => Some(Key::Close),
        DialogKey::Enter | DialogKey::Save => Some(Key::Open),
        DialogKey::Paste => Some(Key::Paste),
        DialogKey::Insert(text) => Some(Key::Insert(text)),
        DialogKey::SwitchField => None,
        key => Some(Key::Edit(key)),
    }
}

/// The byte ranges of `path` to highlight for the matched char `indices` (as `fs.find`
/// returns them), adjacent chars merged; indices past the end are ignored.
pub fn match_ranges(path: &str, indices: &[u32]) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for (index, (start, c)) in path.char_indices().enumerate() {
        let matched = u32::try_from(index).is_ok_and(|index| indices.contains(&index));
        if !matched {
            continue;
        }
        let end = start + c.len_utf8();
        match ranges.last_mut() {
            Some(last) if last.end == start => last.end = end,
            _ => ranges.push(start..end),
        }
    }
    ranges
}

/// The commits the Commits scope lists (one `git.log` per opening, filtered here), and how
/// many the History reads after a "Show commit" pick so the commit is listed there too.
pub const COMMIT_LIMIT: u32 = 500;

/// The Search scope asks the daemon from this many characters on.
pub const SEARCH_MIN_CHARS: usize = 2;

/// What Files, Search, Branches and Commits say without a workspace.
const NO_WORKSPACE: &str = "Open a workspace to search it.";

/// The folder the daemon scopes read (spec 7.2): the active workspace, the task worktree its
/// Files view shows (the folder `fs.find` lists) and, outside a worktree, the repository its Git
/// view shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub project_id: Ulid,
    pub worktree_task_id: Option<Ulid>,
    pub repo: Option<String>,
}

impl Source {
    /// The daemon refuses `repo` together with a worktree (ADR 0029), so a worktree drops it.
    pub fn new(project_id: Ulid, worktree_task_id: Option<Ulid>, repo: Option<String>) -> Self {
        Self {
            project_id,
            repo: if worktree_task_id.is_some() {
                None
            } else {
                repo
            },
            worktree_task_id,
        }
    }
}

/// The answers of the checkout question, in `window.prompt` order.
pub const CHECKOUT_ANSWERS: [&str; 2] = ["Switch", "Cancel"];

/// Whether the checkout question's answer (an index into [`CHECKOUT_ANSWERS`]) switches.
pub fn confirms_checkout(answer: Option<usize>) -> bool {
    answer == Some(0)
}

/// The question before a checkout (message, detail), or `None` when nothing is at risk:
/// `changes` files with uncommitted changes in the repository (git refuses a switch that
/// would overwrite one, and the others follow to the new branch) and `agents` live agent
/// sessions running in that folder.
pub fn checkout_question(branch: &str, changes: usize, agents: usize) -> Option<(String, String)> {
    let mut risks = Vec::new();
    if changes > 0 {
        let files = if changes == 1 { "file" } else { "files" };
        risks.push(format!(
            "{changes} {files} with uncommitted changes. Git refuses the switch when a change would be overwritten."
        ));
    }
    match agents {
        0 => {}
        1 => risks.push(
            "1 live agent session runs in this folder; its files change under it.".to_owned(),
        ),
        n => risks.push(format!(
            "{n} live agent sessions run in this folder; their files change under them."
        )),
    }
    (!risks.is_empty()).then(|| (format!("Switch to “{branch}”?"), risks.join("\n")))
}

/// Live agent sessions that run in the folder a checkout changes: with no `worktree_task_id`
/// the project folder (sessions of tasks that have no worktree of their own, per
/// `has_worktree`), else that task's worktree.
pub fn agents_in_folder(
    sessions: &[Session],
    project_id: Ulid,
    worktree_task_id: Option<Ulid>,
    has_worktree: impl Fn(Ulid) -> bool,
) -> usize {
    sessions
        .iter()
        .filter(|session| {
            session.project_id == project_id
                && session.kind == SessionKind::Agent
                && session.state != SessionState::Exited
        })
        .filter(|session| match worktree_task_id {
            Some(task) => session.task_id == Some(task),
            None => session.task_id.is_none_or(|task| !has_worktree(task)),
        })
        .count()
}

/// Records a command run from the palette as the latest of the recent ones (All lists them
/// while the query is empty). Other actions are not remembered.
// ponytail: UI memory only; save it in the UI state file if users ask for it across restarts.
pub fn remember(recent: &mut Vec<Action>, action: &Action) {
    if !matches!(action, Action::Run(_)) {
        return;
    }
    recent.retain(|other| other != action);
    recent.insert(0, action.clone());
    recent.truncate(theme::PALETTE_GROUP_LIMIT);
}

/// What a daemon scope holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Load {
    /// Nothing asked yet.
    Idle,
    Loading,
    Ready,
    Failed(String),
}

/// A daemon call the palette wants made now. The shell sends it after `delay` (when the
/// palette still wants it, see [`Palette::is_current`]) and gives the answer back to
/// [`Palette::answer`] with the same `serial`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetch {
    pub scope: Scope,
    pub serial: u64,
    pub delay: Duration,
    pub call: Call,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    Files(FsFindParams),
    Search(FsSearchParams),
    Branches(GitBranchesParams),
    Commits(GitLogParams),
}

impl Call {
    /// The answer for this call when it could not be sent (no daemon): the same kind as the
    /// real answer, carrying `error`, so the scope shows it as its muted failure row.
    pub fn failed(&self, error: impl Into<String>) -> Answer {
        let error = error.into();
        match self {
            Call::Files(_) => Answer::Files(Err(error)),
            Call::Search(_) => Answer::Search(Err(error)),
            Call::Branches(_) => Answer::Branches(Err(error)),
            Call::Commits(_) => Answer::Commits(Err(error)),
        }
    }
}

/// The daemon's answer to a [`Call`]; an error is its message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Files(Result<Vec<FsMatch>, String>),
    Search(Result<FsSearchResult, String>),
    Branches(Result<Vec<Branch>, String>),
    Commits(Result<Vec<Commit>, String>),
}

/// One drawn line of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line<'a> {
    /// A section overline (All).
    Header(&'static str),
    /// A row; `pick` is its place among the selectable lines (for a click).
    Row { row: Row<'a>, pick: usize },
    /// "+N more": picking it switches to `scope`.
    More {
        scope: Scope,
        hidden: usize,
        pick: usize,
    },
    /// A muted line: a wait, a failure, a missing workspace.
    Note { text: String, error: bool },
}

/// Where a shown item's entry lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Store {
    Local,
    Files,
    Search,
    Branches,
    Commits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Slot {
    Header(&'static str),
    Item {
        store: Store,
        index: usize,
        ranges: Vec<Range<usize>>,
    },
    More(Scope, usize),
    Note(String, bool),
}

impl Slot {
    fn selectable(&self) -> bool {
        matches!(self, Slot::Item { .. } | Slot::More(..))
    }
}

/// One daemon scope: its rows, what it is doing, and the number of the last call, so an
/// older answer is dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Remote {
    load: Load,
    rows: Vec<Entry>,
    /// The query the last call carried (Files, Search).
    asked: Option<String>,
    serial: u64,
}

impl Remote {
    fn new() -> Self {
        Self {
            load: Load::Idle,
            rows: Vec::new(),
            asked: None,
            serial: 0,
        }
    }

    /// A call is about to be sent: the scope waits for it, under a number no other call of
    /// any palette has.
    fn start(&mut self, query: Option<&str>) -> u64 {
        self.load = Load::Loading;
        self.asked = query.map(str::to_owned);
        self.serial = next_serial();
        self.serial
    }
}

/// Unique across palettes, so an answer meant for a closed one never lands in the next.
fn next_serial() -> u64 {
    static SENT: AtomicU64 = AtomicU64::new(0);
    SENT.fetch_add(1, Ordering::Relaxed) + 1
}

/// The open palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    /// Commands and Sessions, listed by the shell when the palette opened.
    entries: Vec<Entry>,
    /// Private: every edit must lay out again ([`Self::key`], [`Self::insert`]).
    query: Input,
    scope: Scope,
    /// The scope tabs show and Tab switches them.
    tabs: bool,
    /// Scopes and daemon sources work. Only the theme picker is one flat list instead (a
    /// detached window's palette is scoped too).
    scoped: bool,
    source: Option<Source>,
    /// Opened by `@` in a prompt: Enter on a file types its path there instead of opening it.
    insert_into: Option<crate::composer::Target>,
    /// The commands run from the palette lately, newest first.
    recent: Vec<Action>,
    files: Remote,
    hits: Remote,
    branches: Remote,
    commits: Remote,
    shown: Vec<Slot>,
    /// Index among the selectable lines (rows and "+N more") of the picked one.
    pub selected: usize,
}

impl Palette {
    fn base(entries: Vec<Entry>, scope: Scope) -> Self {
        Self {
            entries,
            query: Input::default(),
            scope,
            tabs: false,
            scoped: false,
            source: None,
            insert_into: None,
            recent: Vec::new(),
            files: Remote::new(),
            hits: Remote::new(),
            branches: Remote::new(),
            commits: Remote::new(),
            shown: Vec::new(),
            selected: 0,
        }
    }

    /// A flat list of `entries` filtered by the query: the theme picker (every other palette,
    /// a detached window's included, is [`Self::scoped`]).
    pub fn new(entries: Vec<Entry>) -> Self {
        let mut palette = Self::base(entries, Scope::All);
        palette.layout(false);
        palette
    }

    /// The scoped palette on `scope`. `source` is what Files, Search, Branches and Commits
    /// read (`None`: they ask for a workspace), `recent` the commands run from it lately.
    pub fn scoped(
        entries: Vec<Entry>,
        source: Option<Source>,
        scope: Scope,
        recent: Vec<Action>,
    ) -> Self {
        let mut palette = Self {
            tabs: true,
            scoped: true,
            source,
            recent,
            ..Self::base(entries, scope)
        };
        palette.layout(false);
        palette
    }

    /// The `@` picker of a prompt: Files only, no tabs; Enter hands the path to `target`.
    pub fn file_ref(source: Source, target: crate::composer::Target) -> Self {
        let mut palette = Self {
            scoped: true,
            source: Some(source),
            insert_into: Some(target),
            ..Self::base(Vec::new(), Scope::Files)
        };
        palette.layout(false);
        palette
    }

    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// The scope tabs show (and Tab switches them).
    pub fn tabs(&self) -> bool {
        self.tabs
    }

    pub fn source(&self) -> Option<&Source> {
        self.source.as_ref()
    }

    /// The prompt an `@` pick types its path into.
    pub fn insert_into(&self) -> Option<crate::composer::Target> {
        self.insert_into
    }

    /// Shows `scope`, the pick on its first row.
    pub fn set_scope(&mut self, scope: Scope) {
        if self.scope != scope {
            self.scope = scope;
            self.layout(false);
        }
    }

    /// Tab (`by` > 0) or Shift+Tab: the next or previous scope, wrapping; nothing in a list
    /// without tabs.
    pub fn step_scope(&mut self, by: isize) {
        if self.tabs {
            let scope = if by < 0 {
                self.scope.prev()
            } else {
                self.scope.next()
            };
            self.set_scope(scope);
        }
    }

    /// Picks the row showing entry `entry_index` (the theme picker opens on the current theme).
    pub fn pick_entry(&mut self, entry_index: usize) {
        let at = self
            .shown
            .iter()
            .filter(|slot| slot.selectable())
            .position(|slot| {
                matches!(slot, Slot::Item { store: Store::Local, index, .. } if *index == entry_index)
            });
        if let Some(at) = at {
            self.selected = at;
        }
    }

    /// The typed query with its caret and selection.
    pub fn query(&self) -> &Input {
        &self.query
    }

    /// A press or drag in the query (see [`Input::click`]).
    pub fn click(&mut self, at: usize, extend: bool, clicks: usize) {
        self.query.click(at, extend, clicks);
    }

    /// Applies a key; Esc, Enter and Ctrl+V are the caller's (close, [`Self::picked`],
    /// [`Self::insert`] the clipboard), and so is copying the selection before Copy / Cut.
    pub fn key(&mut self, key: Key) {
        match key {
            Key::Move(by) => self.step(by),
            Key::Edit(key) => {
                if self.query.edit(&key) {
                    self.layout(false);
                }
            }
            Key::Insert(text) => self.insert(&text),
            Key::Close | Key::Open | Key::Paste => {}
        }
    }

    /// Appends typed or pasted text (line breaks dropped) and lays the list out again.
    pub fn insert(&mut self, text: &str) {
        let line: String = text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
        if self.query.insert(&line) {
            self.layout(false);
        }
    }

    /// The daemon calls the palette wants now, after it opened, a key or a scope switch:
    /// Files per query (no wait while the query is empty), Search per query from
    /// [`SEARCH_MIN_CHARS`] characters (only on its own scope), Branches and Commits once per
    /// opening, when their scope or All is first shown. Asking the same thing twice returns
    /// nothing. Too short a query for Search empties its hits.
    pub fn wanted(&mut self) -> Vec<Fetch> {
        let Some(source) = self.source.clone() else {
            return Vec::new();
        };
        let query = self.query.as_str().to_owned();
        let all = self.scope == Scope::All;
        let mut fetches = Vec::new();
        if (self.scope == Scope::Files || (all && !query.trim().is_empty()))
            && self.files.asked.as_deref() != Some(query.as_str())
        {
            let serial = self.files.start(Some(&query));
            fetches.push(Fetch {
                scope: Scope::Files,
                serial,
                delay: if query.is_empty() {
                    Duration::ZERO
                } else {
                    theme::PALETTE_FILES_DELAY
                },
                call: Call::Files(FsFindParams {
                    project_id: source.project_id,
                    worktree_task_id: source.worktree_task_id,
                    query: query.clone(),
                    limit: FsFindParams::DEFAULT_LIMIT,
                }),
            });
        }
        if self.scope == Scope::Search {
            let long_enough = query.trim().chars().count() >= SEARCH_MIN_CHARS;
            if long_enough && self.hits.asked.as_deref() != Some(query.as_str()) {
                let serial = self.hits.start(Some(&query));
                fetches.push(Fetch {
                    scope: Scope::Search,
                    serial,
                    delay: theme::PALETTE_SEARCH_DELAY,
                    call: Call::Search(FsSearchParams {
                        project_id: source.project_id,
                        worktree_task_id: source.worktree_task_id,
                        query: query.clone(),
                        regex: false,
                        limit: FsSearchParams::DEFAULT_LIMIT,
                    }),
                });
            } else if !long_enough && self.hits.asked.is_some() {
                // Too short to search: the old hits go, and their late answer is dropped by
                // its number (a fresh `Remote` has number 0).
                self.hits = Remote::new();
                self.layout(false);
            }
        }
        if (self.scope == Scope::Branches || all) && self.branches.load == Load::Idle {
            let serial = self.branches.start(None);
            fetches.push(Fetch {
                scope: Scope::Branches,
                serial,
                delay: Duration::ZERO,
                call: Call::Branches(GitBranchesParams {
                    project_id: source.project_id,
                    worktree_task_id: source.worktree_task_id,
                    repo: source.repo.clone(),
                }),
            });
        }
        if (self.scope == Scope::Commits || all) && self.commits.load == Load::Idle {
            let serial = self.commits.start(None);
            fetches.push(Fetch {
                scope: Scope::Commits,
                serial,
                delay: Duration::ZERO,
                call: Call::Commits(GitLogParams {
                    project_id: source.project_id,
                    worktree_task_id: source.worktree_task_id,
                    repo: source.repo.clone(),
                    limit: COMMIT_LIMIT,
                    path: None,
                }),
            });
        }
        if !fetches.is_empty() {
            self.layout(true);
        }
        fetches
    }

    /// Whether call number `serial` of `scope` is still the latest (the shell checks after a
    /// debounce, before it sends).
    pub fn is_current(&self, scope: Scope, serial: u64) -> bool {
        match scope {
            Scope::Files => self.files.serial == serial,
            Scope::Search => self.hits.serial == serial,
            Scope::Branches => self.branches.serial == serial,
            Scope::Commits => self.commits.serial == serial,
            Scope::All | Scope::Commands | Scope::Sessions => false,
        }
    }

    /// The answer to call number `serial`; kept only if no newer call of that scope was sent
    /// (the previous rows stay on screen until then). `now_ms` dates the ages. Returns `true`
    /// when the answer was kept. An answer for the scope on screen puts the pick on its best
    /// row; any other answer keeps the pick where it was.
    pub fn answer(&mut self, serial: u64, answer: Answer, now_ms: i64) -> bool {
        let (scope, current) = match &answer {
            Answer::Files(_) => (Scope::Files, self.files.serial),
            Answer::Search(_) => (Scope::Search, self.hits.serial),
            Answer::Branches(_) => (Scope::Branches, self.branches.serial),
            Answer::Commits(_) => (Scope::Commits, self.commits.serial),
        };
        if serial != current {
            return false;
        }
        let repo = self.source.as_ref().and_then(|source| source.repo.clone());
        let repo = repo.as_deref();
        let (remote, rows) = match answer {
            Answer::Files(found) => (
                &mut self.files,
                found.map(|matches| matches.into_iter().map(file_entry).collect()),
            ),
            Answer::Search(found) => (
                &mut self.hits,
                found.map(|result| result.hits.into_iter().map(hit_entry).collect()),
            ),
            Answer::Branches(found) => (
                &mut self.branches,
                found.map(|branches| {
                    (branches.into_iter())
                        .map(|branch| branch_entry(branch, repo, now_ms))
                        .collect()
                }),
            ),
            Answer::Commits(found) => (
                &mut self.commits,
                found.map(|commits| {
                    (commits.into_iter())
                        .map(|commit| commit_entry(commit, repo, now_ms))
                        .collect()
                }),
            ),
        };
        match rows {
            Ok(rows) => {
                remote.rows = rows;
                remote.load = Load::Ready;
            }
            Err(error) => {
                remote.rows.clear();
                remote.load = Load::Failed(error);
            }
        }
        self.layout(self.scope != scope);
        true
    }

    /// The list as it is drawn: overlines, rows, "+N more" and notes.
    pub fn lines(&self) -> Vec<Line<'_>> {
        let mut pick = 0;
        let mut lines = Vec::with_capacity(self.shown.len());
        for slot in &self.shown {
            match slot {
                Slot::Header(title) => lines.push(Line::Header(title)),
                Slot::Note(text, error) => lines.push(Line::Note {
                    text: text.clone(),
                    error: *error,
                }),
                Slot::Item {
                    store,
                    index,
                    ranges,
                } => {
                    if let Some(entry) = self.entry(*store, *index) {
                        lines.push(Line::Row {
                            row: Row {
                                entry,
                                ranges: ranges.clone(),
                                section: entry.action.scope(),
                                detail: entry.detail.as_deref(),
                                age: entry.age.as_deref(),
                            },
                            pick,
                        });
                    }
                    pick += 1;
                }
                Slot::More(scope, hidden) => {
                    lines.push(Line::More {
                        scope: *scope,
                        hidden: *hidden,
                        pick,
                    });
                    pick += 1;
                }
            }
        }
        lines
    }

    /// The shown rows, best match first ("+N more" and notes left out).
    pub fn rows(&self) -> Vec<Row<'_>> {
        self.lines()
            .into_iter()
            .filter_map(|line| match line {
                Line::Row { row, .. } => Some(row),
                _ => None,
            })
            .collect()
    }

    /// The index in [`Self::lines`] of the picked line (what to scroll into view).
    pub fn selected_line(&self) -> usize {
        self.shown
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.selectable())
            .nth(self.selected)
            .map_or(0, |(line, _)| line)
    }

    /// Moves the pick by `by`, wrapping around the selectable lines.
    pub fn step(&mut self, by: i32) {
        let count = self.shown.iter().filter(|slot| slot.selectable()).count();
        if count == 0 {
            return;
        }
        let count = i64::try_from(count).unwrap_or(i64::MAX);
        let at = i64::try_from(self.selected).unwrap_or(0);
        self.selected = usize::try_from((at + i64::from(by)).rem_euclid(count)).unwrap_or(0);
    }

    /// Picks selectable line `index` (a click).
    pub fn pick(&mut self, index: usize) {
        if index < self.shown.iter().filter(|slot| slot.selectable()).count() {
            self.selected = index;
        }
    }

    /// What the picked row does; `None` on "+N more" or with nothing shown.
    pub fn picked(&self) -> Option<Action> {
        match self.picked_slot()? {
            Slot::Item { store, index, .. } => {
                self.entry(*store, *index).map(|entry| entry.action.clone())
            }
            _ => None,
        }
    }

    /// The scope of the picked "+N more" row, if that is the pick.
    pub fn picked_more(&self) -> Option<Scope> {
        match self.picked_slot()? {
            Slot::More(scope, _) => Some(*scope),
            _ => None,
        }
    }

    fn picked_slot(&self) -> Option<&Slot> {
        self.shown
            .iter()
            .filter(|slot| slot.selectable())
            .nth(self.selected)
    }

    fn entry(&self, store: Store, index: usize) -> Option<&Entry> {
        match store {
            Store::Local => self.entries.get(index),
            Store::Files => self.files.rows.get(index),
            Store::Search => self.hits.rows.get(index),
            Store::Branches => self.branches.rows.get(index),
            Store::Commits => self.commits.rows.get(index),
        }
    }

    /// Lays the list out again and picks its first row, or, with `keep`, the line that was
    /// picked: a row by its action, a "+N more" by its scope (an answer must not move a pick
    /// that is not about it).
    fn layout(&mut self, keep: bool) {
        let before = if keep {
            self.picked_slot().cloned()
        } else {
            None
        };
        let before = before.map(|slot| match slot {
            Slot::Item { store, index, .. } => (
                self.entry(store, index).map(|entry| entry.action.clone()),
                None,
            ),
            Slot::More(scope, _) => (None, Some(scope)),
            Slot::Header(_) | Slot::Note(..) => (None, None),
        });
        self.shown = self.slots();
        self.selected = 0;
        let Some((action, more)) = before else {
            return;
        };
        let at = self
            .shown
            .iter()
            .filter(|slot| slot.selectable())
            .position(|slot| match slot {
                Slot::Item { store, index, .. } => action.as_ref().is_some_and(|action| {
                    self.entry(*store, *index)
                        .is_some_and(|entry| entry.action == *action)
                }),
                Slot::More(scope, _) => more == Some(*scope),
                Slot::Header(_) | Slot::Note(..) => false,
            });
        if let Some(at) = at {
            self.selected = at;
        }
    }

    fn slots(&self) -> Vec<Slot> {
        if !self.scoped {
            return ranked(Store::Local, &self.entries, &self.query, None);
        }
        if self.scope == Scope::All {
            return self.grouped();
        }
        let items = self.matches(self.scope);
        match self.note(self.scope) {
            Some(note) if items.is_empty() => vec![note],
            _ => items,
        }
    }

    /// The items of one scope for the query, best first.
    fn matches(&self, scope: Scope) -> Vec<Slot> {
        let query = self.query.as_str();
        match scope {
            // Ranked by the daemon: kept in its order, highlighted by its ranges.
            Scope::Files => ordered(Store::Files, &self.files.rows),
            Scope::Search => ordered(Store::Search, &self.hits.rows),
            Scope::Commands | Scope::Sessions => {
                ranked(Store::Local, &self.entries, query, Some(scope))
            }
            Scope::Branches => ranked(Store::Branches, &self.branches.rows, query, None),
            Scope::Commits => ranked(Store::Commits, &self.commits.rows, query, None),
            Scope::All => Vec::new(),
        }
    }

    /// Whether `scope` can have a section in All: every scope but Search. A scope without
    /// rows (not loaded yet, failed, no workspace) adds no section: its wait and its failure
    /// show in its own tab only ([`Self::group`] skips an empty one).
    fn joins_all(&self, scope: Scope) -> bool {
        !matches!(scope, Scope::All | Scope::Search)
    }

    /// All: with an empty query the sessions (waiting ones lead) and the recent commands;
    /// with a query one section per scope in [`Scope::ORDER`] except Search, each capped.
    fn grouped(&self) -> Vec<Slot> {
        let mut slots = Vec::new();
        if self.query.trim().is_empty() {
            let sessions = self.matches(Scope::Sessions);
            self.group(
                &mut slots,
                Scope::Sessions.title(),
                Scope::Sessions,
                sessions,
            );
            let recent: Vec<Slot> = self
                .recent
                .iter()
                .filter_map(|action| {
                    let index = self.entries.iter().position(|e| e.action == *action)?;
                    Some(Slot::Item {
                        store: Store::Local,
                        index,
                        ranges: Vec::new(),
                    })
                })
                .collect();
            self.group(&mut slots, "Recent", Scope::Commands, recent);
        } else {
            for scope in Scope::ORDER.into_iter().filter(|s| self.joins_all(*s)) {
                self.group(&mut slots, scope.title(), scope, self.matches(scope));
            }
        }
        slots
    }

    /// One section of All: its overline, at most `PALETTE_GROUP_LIMIT` rows and a "+N more"
    /// row. A scope without rows is left out: All has no loading or error lines (spec 7.2).
    fn group(&self, slots: &mut Vec<Slot>, title: &'static str, scope: Scope, items: Vec<Slot>) {
        if items.is_empty() {
            return;
        }
        slots.push(Slot::Header(title));
        let hidden = items.len().saturating_sub(theme::PALETTE_GROUP_LIMIT);
        slots.extend(items.into_iter().take(theme::PALETTE_GROUP_LIMIT));
        if hidden > 0 {
            slots.push(Slot::More(scope, hidden));
        }
    }

    /// The muted line of a daemon scope with no row: the missing workspace, the wait, the
    /// failure, or (Search) how to start.
    fn note(&self, scope: Scope) -> Option<Slot> {
        let remote = match scope {
            Scope::Files => &self.files,
            Scope::Search => &self.hits,
            Scope::Branches => &self.branches,
            Scope::Commits => &self.commits,
            Scope::All | Scope::Commands | Scope::Sessions => return None,
        };
        if self.source.is_none() {
            return Some(Slot::Note(NO_WORKSPACE.into(), false));
        }
        match &remote.load {
            Load::Loading => Some(Slot::Note("Loading…".into(), false)),
            Load::Failed(error) => Some(Slot::Note(error.clone(), true)),
            Load::Idle if scope == Scope::Search => Some(Slot::Note(
                format!("Type {SEARCH_MIN_CHARS} characters to search the files."),
                false,
            )),
            Load::Idle | Load::Ready => None,
        }
    }
}

/// `rows` in their order with the daemon's highlight ranges.
fn ordered(store: Store, rows: &[Entry]) -> Vec<Slot> {
    rows.iter()
        .enumerate()
        .map(|(index, entry)| Slot::Item {
            store,
            index,
            ranges: entry.marks.clone(),
        })
        .collect()
}

/// The `rows` of `section` (all when `None`) that `query` matches, best score first, ties in
/// their order. A commit is found by its short hash too.
fn ranked(store: Store, rows: &[Entry], query: &str, section: Option<Scope>) -> Vec<Slot> {
    let mut scored: Vec<(i64, usize, Vec<u32>)> = rows
        .iter()
        .enumerate()
        .filter(|(_, entry)| section.is_none_or(|section| entry.action.scope() == section))
        .filter_map(|(index, entry)| {
            let title = fuzzy(query, &entry.title);
            let detail = (store == Store::Commits)
                .then(|| {
                    entry
                        .detail
                        .as_deref()
                        .and_then(|detail| fuzzy(query, detail))
                })
                .flatten();
            let (score, indices) = match (title, detail) {
                (Some(title), Some(detail)) if detail.0 > title.0 => (detail.0, Vec::new()),
                (Some(title), _) => title,
                (None, Some(detail)) => (detail.0, Vec::new()),
                (None, None) => return None,
            };
            Some((score, index, indices))
        })
        .collect();
    scored.sort_by_key(|(score, ..)| std::cmp::Reverse(*score));
    scored
        .into_iter()
        .map(|(_, index, indices)| Slot::Item {
            store,
            index,
            ranges: match_ranges(&rows[index].title, &indices),
        })
        .collect()
}

fn file_entry(found: FsMatch) -> Entry {
    let marks = match_ranges(&found.path, &found.indices);
    Entry {
        marks,
        ..Entry::new(
            Action::OpenFile {
                path: found.path.clone(),
            },
            found.path,
            "",
        )
    }
}

fn hit_entry(hit: SearchHit) -> Entry {
    // The daemon marks on char boundaries inside the preview; a range that is not would
    // panic in text layout, so it is dropped here as Files indices are.
    let marks = (hit.ranges.iter())
        .map(|&(start, end)| start as usize..end as usize)
        .filter(|range| {
            range.start < range.end
                && hit.preview.is_char_boundary(range.start)
                && hit.preview.is_char_boundary(range.end)
        })
        .collect();
    Entry {
        detail: Some(format!("{}:{}", hit.path, hit.line)),
        marks,
        ..Entry::new(
            Action::OpenSearchHit {
                path: hit.path,
                line: hit.line,
            },
            hit.preview,
            "",
        )
    }
}

fn branch_entry(branch: Branch, repo: Option<&str>, now_ms: i64) -> Entry {
    let detail = if branch.current {
        format!("current · {}", branch.subject)
    } else {
        branch.subject
    };
    Entry {
        detail: Some(detail),
        age: Some(crate::git_history::age(branch.last_commit_at, now_ms)),
        ..Entry::new(
            Action::Checkout {
                repo: repo.map(str::to_owned),
                branch: branch.name.clone(),
                current: branch.current,
            },
            branch.name,
            "",
        )
    }
}

fn commit_entry(commit: Commit, repo: Option<&str>, now_ms: i64) -> Entry {
    Entry {
        detail: Some(crate::git_history::short_hash(&commit.hash).to_owned()),
        age: Some(crate::git_history::age(commit.authored_at, now_ms)),
        ..Entry::new(
            Action::ShowCommit {
                repo: repo.map(str::to_owned),
                sha: commit.hash,
            },
            commit.summary,
            "",
        )
    }
}

#[cfg(test)]
mod tests;
