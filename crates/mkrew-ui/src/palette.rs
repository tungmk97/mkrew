//! Command palette (Ctrl+Shift+P, SPEC §13): every keymap command that can act in the current
//! view, with its title and shortcut, palette-only actions that have no key (task "View changes",
//! "Merge back…", "Remove worktree…", fan-out, broadcast, insert prompt), plus "Go to" workspace /
//! session / task items (the §13 quick open of those). Typing filters with a fuzzy subsequence match
//! ranked by word starts and runs; Enter picks. Pure and tested headlessly; `main.rs` renders it and runs the pick.

use std::ops::Range;

use mkrew_core::settings::FeatureSettings;
use mkrew_protocol::{Project, Session, SessionState, Task};
use ulid::Ulid;

use crate::board::CardKey;
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
    /// The active workspace's open file tabs.
    pub file_tabs: usize,
    /// The file tree is painted in the right panel (Open in a kept tab acts on its cursor).
    pub file_tree: bool,
    /// A terminal pane had the keyboard (copy, paste, find… act on it).
    pub terminal: bool,
    /// The Git list's selected file can be discarded (unstaged or untracked).
    pub discard: bool,
    /// The active workspace's Git is on screen (the panel's Git tab or the Diff view): the
    /// Git keys act.
    pub git: bool,
    /// The active workspace has an open diff (the Diff view exists).
    pub diff: bool,
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
    /// A machine's window: Phone access and the in-app update belong to this computer's.
    pub machine: bool,
    /// `[features]` (ADR 0046): Docs and Turns entries are listed only when on.
    pub features: FeatureSettings,
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
    Turns,
    FollowInZed,
    RunSetup,
    RunShowTab,
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
    JiraMine,
    JiraLayout,
    JiraRefresh,
    CopyLastError,
    DismissToasts,
    History,
    InstallUpdate,
    AddMachine,
}

impl Extra {
    pub const ALL: [Self; 41] = [
        Self::ViewTaskChanges,
        Self::MergeBack,
        Self::RemoveWorktree,
        Self::FanOut,
        Self::Broadcast,
        Self::InsertPrompt,
        Self::Turns,
        Self::FollowInZed,
        Self::RunSetup,
        Self::RunShowTab,
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
        Self::JiraMine,
        Self::JiraLayout,
        Self::JiraRefresh,
        Self::CopyLastError,
        Self::DismissToasts,
        Self::History,
        Self::InstallUpdate,
        Self::AddMachine,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::ViewTaskChanges => "Task: View changes of selected task",
            Self::MergeBack => "Task: Merge back selected task…",
            Self::RemoveWorktree => "Task: Remove worktree of selected task…",
            Self::FanOut => "Agent: Run selected task with several agents…",
            Self::Broadcast => "Agent: Broadcast to agents…",
            Self::InsertPrompt => "Agent: Insert prompt…",
            Self::Turns => "Agent: Turns…",
            Self::FollowInZed => "Agent: Follow in Zed (on/off)",
            Self::RunSetup => "Run: Setup…",
            Self::RunShowTab => "Run: Show Run tab",
            Self::PhoneAccess => "Global: Phone access…",
            Self::CheckUpdates => "Global: Check for updates",
            Self::MarkUnread => "Agent: Mark unread",
            Self::WorkspacePin => "Workspace: Pin / Unpin",
            Self::WorkspaceExplorer => "Workspace: Open in File Explorer",
            Self::WorkspaceDefaultAgent => "Workspace: Set default agent…",
            Self::WorkspaceRemove => "Workspace: Remove…",
            Self::WorkspaceMenu => "Workspace: Open workspace menu",
            Self::TabPin => "Tab: Pin / Unpin",
            Self::TabCloseOthers => "Tab: Close others",
            Self::TabCloseRight => "Tab: Close tabs to the right",
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
            Self::JiraMine => "Jira: Toggle All / Mine",
            Self::JiraLayout => "Jira: Toggle List / Kanban",
            Self::JiraRefresh => "Jira: Refresh",
            Self::CopyLastError => "Global: Copy last error",
            Self::DismissToasts => "Global: Dismiss notifications",
            Self::History => "Global: History…",
            Self::InstallUpdate => "Global: Install update",
            Self::AddMachine => "Machine: Add…",
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
            Self::GitChangeBase => context.workspace && context.git && context.git_base,
            Self::GitSwitchRepo => context.workspace && context.git && context.git_repos,
            Self::JiraMine | Self::JiraLayout | Self::JiraRefresh => {
                in_view(View::Board) && context.jira
            }
            Self::CopyLastError => context.last_error,
            Self::DismissToasts => context.toast,
            Self::History => context.workspace,
            Self::InstallUpdate => context.update_ready,
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
    /// The theme picker's row for `theme::THEMES[i]` (ADR 0078).
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
        Command::SendSelection => "Agent: Send selection to an agent".into(),
        Command::CopyMode => "Terminal: Copy mode (select and copy with the keyboard)".into(),
        Command::LinkHints => "Terminal: Open a link by its label".into(),
        Command::ShowBoard => "View: Board".into(),
        Command::ShowTerminals => "View: Terminals".into(),
        Command::ShowDocs => "View: Docs".into(),
        Command::ShowFileTree => "View: File tree".into(),
        Command::ShowEditor => "View: Editor".into(),
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
        Command::ToggleIgnored => "File tree: Toggle ignored files".into(),
        Command::OpenKept => "File tree: Open file in a kept tab".into(),
        Command::SaveFile => "Editor: Save file".into(),
        Command::FindInFile => "Editor: Find in file".into(),
        Command::ReplaceInFile => "Editor: Replace in file".into(),
        Command::CloseFileTab => "Editor: Close tab".into(),
        Command::ToggleFilePreview => "Editor: Toggle preview / source".into(),
        Command::QuickOpen => "Files: Quick open file".into(),
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
        Command::GitDiscard => "Git: Discard changes to selected file…".into(),
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
        | Command::QuickOpen
        | Command::ToggleInspector
        | Command::NewTask
        | Command::ContextMenu
        | Command::FocusSidebar => context.workspace,
        Command::ShowDiff => context.workspace && context.diff,
        Command::ShowDocs => context.workspace && context.features.docs,
        Command::ShowEditor => context.workspace && context.file_tabs > 0,
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
        Command::Find => {
            (on_panes && context.terminal) || in_view(View::Editor) || in_view(View::Docs)
        }
        Command::OpenComposer => on_panes && context.agent_pane,
        Command::SendSelection => {
            in_view(View::Editor) || in_view(View::Diff) || (on_panes && context.terminal)
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
        | Command::ResolveComment => in_view(View::Docs) || in_view(View::Diff),
        Command::MoveTabLeft | Command::MoveTabRight => {
            on_panes || (in_view(View::Editor) && context.file_tabs > 0)
        }
        Command::ToggleIgnored => context.workspace,
        Command::OpenKept => context.workspace && context.file_tree,
        Command::SaveFile
        | Command::FindInFile
        | Command::ReplaceInFile
        | Command::CloseFileTab
        | Command::ToggleFilePreview => in_view(View::Editor),
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
        .map(|error| Entry {
            action: Action::Run(Command::OpenSettings),
            title: format!("Keymap error: {error}"),
            shortcut: keymap.label(Command::OpenSettings),
        })
        .collect()
}

/// The `settings.toml` problem (SPEC §20.0), listed before the commands like keymap errors;
/// picking it opens the Settings dialog, which shows the problem and "Fix in Zed".
pub fn settings_error(keymap: &Keymap, error: Option<&str>) -> Vec<Entry> {
    error
        .map(|error| Entry {
            action: Action::Run(Command::OpenSettings),
            title: format!("Settings error: {error}"),
            shortcut: keymap.label(Command::OpenSettings),
        })
        .into_iter()
        .collect()
}

/// The theme picker's rows (ADR 0078): every built-in theme, its appearance on the right, the
/// shown one marked.
pub fn theme_entries(current: &str) -> Vec<Entry> {
    crate::theme::THEMES
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let look = if entry.dark { "Dark" } else { "Light" };
            Entry {
                action: Action::Theme(index),
                title: entry.name.into(),
                shortcut: if entry.name == current {
                    format!("✓ {look}")
                } else {
                    look.into()
                },
            }
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
        .map(|agent| Entry {
            action: Action::NewAgentPane(agent.index),
            title: format!("Agent: New {} pane", agent.name),
            shortcut: if agent.default {
                keymap.label(Command::SplitAgent)
            } else {
                String::new()
            },
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
            entries.push(Entry {
                action: Action::RunCommand(action, index),
                title: format!("Run: {verb} {}", row.name),
                shortcut: String::new(),
            });
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
        .map(|(index, name)| Entry {
            action: Action::OpenMachine(index),
            title: format!("Machine: Open {name}"),
            shortcut: String::new(),
        })
        .collect()
}

/// The palette's entries: "Go to" every session that needs input first, then the available
/// commands in keymap order, the available palette-only actions, then "Go to" every workspace,
/// other live session and task (archived workspaces and their items left out). Session rows
/// show their state on the right.
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
    let (asking, others): (Vec<_>, Vec<_>) = sessions
        .iter()
        .filter(|session| session.state != SessionState::Exited)
        .filter_map(|session| {
            let name = workspace(session.project_id)?;
            let status = crate::sidebar::status_display(session.state);
            let entry = Entry {
                shortcut: format!("{} {}", status.icon, status.label),
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

    /// Picks the row showing entry `entry_index` (the theme picker opens on the current theme).
    pub fn pick_entry(&mut self, entry_index: usize) {
        if let Some(row) = self
            .shown
            .iter()
            .position(|(index, _)| *index == entry_index)
        {
            self.selected = row;
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
                    self.filter();
                }
            }
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
mod tests;
