//! `mkrew`: the desktop app (SPEC §10). Connects to the daemon (starting it if needed), lists the
//! workspaces and their sessions in the sidebar and shows the active workspace's terminal tabs,
//! each a split tree of panes. Only the panes on screen are attached.

// No console window next to the app in release builds (debug keeps stderr visible).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
    AnyView, App, Bounds, ClipboardItem, Context, CursorStyle, DispatchPhase, DragMoveEvent,
    EmptyView, Entity, ExternalPaths, FocusHandle, FontWeight, HighlightStyle, Hsla, KeyDownEvent,
    ListAlignment, ListHorizontalSizingBehavior, ListOffset, ListState, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathBuilder, PathPromptOptions, Pixels, Point,
    PromptLevel, ScrollHandle, ScrollStrategy, SharedString, StyledText, Subscription,
    TitlebarOptions, UniformListScrollHandle, Window, WindowBounds, WindowControlArea,
    WindowOptions, anchored, canvas, deferred, div, fill, list, pattern_slash, point, px, relative,
    rgb, size, uniform_list,
};
use mkrew_core::Paths;
use mkrew_core::prompt::PromptVars;
use mkrew_core::settings::DiffMode;
use mkrew_protocol::{
    AgentProfilesResult, AiGenerateParams, AiGeneratedEvent, AiKind, ChangeKind, CommentContext,
    CommentId, CommentListParams, CommentRef, Commit, CommitDetail, DEFAULT_COLS, DEFAULT_ROWS,
    DiffLineKind, DocComment, DocContent, DocFile, DocListParams, DocReadParams, Event, FileDiff,
    FsContent, FsEntry, FsListParams, FsMatch, FsReadParams, FsWriteParams, FsWritten,
    GitCommitResult, GitMarker, GitMode, GitStatus, GitStatusParams, Outcome, Project,
    ProjectAddParams, ProjectClonedEvent, ProjectRef, ProjectUpdateParams, RemoveWorktreeResult,
    Request, RunStartParams, RunStarted, RunStopParams, Session, SessionAttachResult, SessionFiles,
    SessionHistory, SessionKillParams, SessionKind, SessionLinkPlanParams, SessionListParams,
    SessionMoveParams, SessionPinParams, SessionPlanEvent, SessionPortsEvent, SessionReadResult,
    SessionRef, SessionResumeParams, SessionSendParams, SessionSendResult, SessionSpawnResult,
    SessionState, SessionUnqueueParams, SessionUsageEvent, StateSource, TabCreateParams,
    TabListParams, TabPinParams, TabRef, TabSetLayoutParams, Task, TaskFanoutParams,
    TaskListParams, TaskMoveParams, TaskRef, TaskStatus, TerminalTab, Turn, UndoPreview,
    ViewedFile, WORKSPACE_COLORS,
};
use mkrew_ui::ask::{self, Ask};
use mkrew_ui::attention::{self, Tracker};
use mkrew_ui::base_dialog::{self, BaseDialog};
use mkrew_ui::board::{self, Board, Card, CardAction, DraggedCard, View};
use mkrew_ui::broadcast::{self, Broadcast, Delivery as BroadcastDelivery};
use mkrew_ui::client::{Connection, Incoming};
use mkrew_ui::clone_dialog::CloneDialog;
use mkrew_ui::comments::{self, Comments, Selection, Thread};
use mkrew_ui::composer::{self, Composers, Draft};
use mkrew_ui::conflicts::{self, Conflicts};
use mkrew_ui::context_menu::{LayoutMenu, PaneMenu, TabMenu};
use mkrew_ui::desktop::{Desktop, TrayEvent};
use mkrew_ui::doc_dialog::{self, Changes, DocDialog, SendDialog};
use mkrew_ui::docs::{Doc, Docs, Files, Mode, Shown};
use mkrew_ui::editor::{self, EditKey, Pos};
use mkrew_ui::fanout::{self, Compare, FanOutPicker};
use mkrew_ui::field::{Caret, caret_mouse, caret_text, copy_selection};
use mkrew_ui::file_ops;
use mkrew_ui::files_view::{self, Close, Content, FileTab, FilesView, Open, Requests};
use mkrew_ui::follow::{self, Follow};
use mkrew_ui::follow_up::{self, FollowUp};
use mkrew_ui::git_actions::{self, Remote};
use mkrew_ui::git_graph::GraphCell;
use mkrew_ui::git_history::{self, Commits};
use mkrew_ui::git_summary::{GitSummary, RepoSummary};
use mkrew_ui::git_view::{self, GitView, Since};
use mkrew_ui::highlight::{self, Code};
use mkrew_ui::history::{self, HistoryDialog, Listing, Log};
use mkrew_ui::icons::{self, Icon, icon_svg};
use mkrew_ui::ime;
use mkrew_ui::inbox::{self, Entry, Jump, Viewed};
use mkrew_ui::inspector;
use mkrew_ui::jira_board::{JiraView, Load, Source};
use mkrew_ui::keymap::{
    self as keymap, BOARD_CONTEXT, COMMENTS_CONTEXT, Command, DOCS_CONTEXT, FILES_CONTEXT,
    GIT_CONTEXT, Keymap, KeymapError, Run,
};
use mkrew_ui::lifecycle::{self, CloseAction, CloseChoice, RunWith};
use mkrew_ui::palette::{self, Extra, Palette};
use mkrew_ui::pane_text;
use mkrew_ui::peek::{self, Peek};
use mkrew_ui::phone_dialog::{self, LinkBlock, Local, Mark, Phone, PushSwitch, SetUp, Switch};
use mkrew_ui::plan::{self, Plans};
use mkrew_ui::preview;
use mkrew_ui::prompt_picker::{self, PromptPicker};
use mkrew_ui::quick_open::{self, QuickOpen, Results};
use mkrew_ui::router::{Delivery, Router};
use mkrew_ui::run::{self, Ports, RowState, RunAction, Summary};
use mkrew_ui::run_setup::{self, RunSetup};
use mkrew_ui::scrollbar;
use mkrew_ui::settings_dialog::{self, SettingsDialog};
use mkrew_ui::sidebar::{self, Change, Row, SessionRow, Sidebar, WorkspaceRow};
use mkrew_ui::split::{Axis, Divider, Preset};
use mkrew_ui::status;
use mkrew_ui::stt;
use mkrew_ui::tabs::{
    FIRST_TAB_TITLE, Tab, TabKind, Tabs, newest_unplaced, on_screen, rename_params, session_label,
    tab_kind,
};
use mkrew_ui::task_dialog::{self, DialogKey, Field, TaskDialog};
use mkrew_ui::terminal::find::{FindKey, find_key};
use mkrew_ui::terminal::view::{AttachFailed, OpenLink, TerminalPane};
use mkrew_ui::text_box::{self, Input};
use mkrew_ui::text_find::TextFind;
use mkrew_ui::theme::{self, ActiveTheme, Theme, font_size, opacity, radius, space, weight};
use mkrew_ui::toast::{self, Toasts};
use mkrew_ui::turn::Turns;
use mkrew_ui::turns_dialog::{self, TurnsDialog};
use mkrew_ui::undo_dialog::{self, LineKind, Preview, UndoDialog};
use mkrew_ui::usage::{self, Usages};
use mkrew_ui::voice;
use mkrew_ui::workspace_menu::{self, Agents, WorkspaceMenu};
use mkrew_ui::worktree_dialog::{self, WorktreeDialog};
use mkrew_ui::worktree_target;
use mkrew_ui::zed;
use tracing_subscriber::EnvFilter;
use ulid::Ulid;

mod shell;
use shell::cards::*;
use shell::files_render::*;
use shell::git_diff::*;
use shell::git_menus::*;
use shell::git_rows::*;
use shell::menus::*;
use shell::pane_render::*;
use shell::reorder::*;
use shell::sidebar::*;
use shell::thread_panel::*;
use shell::widgets::*;

/// Longest wait for the frame showing fed output before draining again anyway (e.g. the
/// window is minimized and never paints).
const PAINT_TIMEOUT: Duration = Duration::from_millis(50);
/// Shortest time between two output-driven frames (~60 fps). Without it a 165 Hz display
/// repaints the whole window 165 times a second while agents stream, saturating the UI thread
/// so keys wait. Typed keys still repaint at once; only the echo waits for the next slot.
const OUTPUT_FRAME: Duration = Duration::from_millis(16);
/// Most incoming frames handled per drain, so one drain never starves painting.
const DRAIN_BATCH: usize = 256;

/// Windows and macOS draw their own title bar (ADR 0034); elsewhere the OS bar stays.
const CUSTOM_TITLE_BAR: bool = cfg!(any(target_os = "windows", target_os = "macos"));
/// Only Windows draws ─ □ ✕ (macOS keeps its traffic lights).
const DRAWN_CONTROLS: bool = cfg!(target_os = "windows");

fn main() {
    let launched = Instant::now();
    let filter = EnvFilter::try_from_env("MKREW_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .init();
    let paths = match Paths::from_env() {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("mkrew: {error}");
            std::process::exit(2);
        }
    };
    let (keymap, keymap_errors) = Keymap::load(&paths.keymap_file());
    for error in &keymap_errors {
        tracing::warn!("{error}; using the default for it");
    }
    // Started by a toast click (`mkrew://` link, ADR 0079): show that session's pane.
    let jump = std::env::args()
        .nth(1)
        .and_then(|arg| mkrew_core::link::parse_session_link(&arg));
    // A second launch of the same exe on the same daemon shows the running window (it may be
    // hidden in the tray) instead of opening another one; a dev build at another path, or on
    // another `MKREW_SOCKET`, starts on its own.
    if mkrew_ui::desktop::raise_running_instance(&paths.socket, jump) {
        return;
    }
    gpui_platform::application()
        .with_assets(icons::Assets)
        .run(move |cx: &mut App| {
            let bindings = keymap.key_bindings(cx.keyboard_mapper().as_ref());
            cx.bind_keys(bindings);
            cx.set_global(ActiveKeymap(keymap.clone()));
            // Which char a shifted digit reports depends on the layout: bind again after a switch
            // (e.g. US ↔ Vietnamese), or Ctrl+Shift+1..5 stop working until a restart.
            cx.on_keyboard_layout_change(|cx| {
                let bindings = cx
                    .global::<ActiveKeymap>()
                    .0
                    .key_bindings(cx.keyboard_mapper().as_ref());
                cx.clear_key_bindings();
                cx.bind_keys(bindings);
            })
            .detach();
            // A broken file: the defaults until it is fixed (the daemon reports it, §20.0).
            let settings = mkrew_core::settings::load(&paths.settings_file()).unwrap_or_default();
            cx.set_global(theme::Fonts::from_settings(
                &settings,
                &cx.text_system().all_font_names(),
            ));
            cx.set_global(theme::ThemeNames::from_settings(&settings));
            // Confirmations are drawn in the window in Mkrew's style, never the OS dialog (§14).
            cx.set_prompt_builder(shell::confirm_prompt::build);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(theme::WINDOW_WIDTH), px(theme::WINDOW_HEIGHT)),
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Mkrew".into()),
                    appears_transparent: CUSTOM_TITLE_BAR,
                    traffic_light_position: cfg!(target_os = "macos").then(|| {
                        let (x, y) = theme::MAC_TRAFFIC_LIGHT_POS;
                        point(px(x), px(y))
                    }),
                }),
                window_min_size: Some(size(
                    px(theme::WINDOW_MIN_WIDTH),
                    px(theme::WINDOW_MIN_HEIGHT),
                )),
                // macOS: we move the window from our own bar, so AppKit must not delay the click.
                app_owns_titlebar_drag: cfg!(target_os = "macos"),
                ..Default::default()
            };
            let mode = settings.ui.theme;
            let opened = cx.open_window(options, |window, cx| {
                theme::refresh(mode, window, cx);
                cx.new(|cx| Shell::new(launched, paths, keymap, &keymap_errors, jump, window, cx))
            });
            if let Err(error) = opened {
                eprintln!("mkrew: cannot open window: {error:#}");
                cx.quit();
                return;
            }
            // Sessions live in the daemon, so closing the window simply ends the app.
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
}

enum Status {
    Connecting,
    Connected,
    Disconnected,
    /// The daemon cannot be reached.
    Failed(String),
    /// Listing workspaces or a workspace's tabs failed.
    ListFailed(String),
}

/// What runs after unsaved Files changes were saved or dropped.
type AfterSave = Box<dyn FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>)>;

/// One choice of a chooser row ([`Shell::chip_row`]); `pick` goes back to its `on_pick`.
struct Chip<P> {
    id: SharedString,
    label: SharedString,
    tip: String,
    active: bool,
    pick: P,
}

/// What the inline title field edits.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RenameTarget {
    Tab(Ulid),
    /// A pane's session (`session.rename`).
    Session(Ulid),
    /// The name of a new file or folder in the Files tree (SPEC §12A).
    NewEntry(NewEntry),
}

/// Where the Files tree's "New file…" / "New folder…" creates its entry.
#[derive(Debug, Clone, PartialEq, Eq)]
struct NewEntry {
    project_id: Ulid,
    /// The worktree the Files view showed (`None` = the project folder).
    target: Option<Ulid>,
    /// The folder, relative to that one (`""` = the folder itself).
    dir: String,
    folder: bool,
}

/// The inline title field: what it renames and the draft.
#[derive(Debug, Clone)]
struct Renaming {
    target: RenameTarget,
    draft: Input,
}

/// The right-click menu of a Files tree row or of the tree itself (SPEC §12A): New file,
/// New folder, Copy path, Add to .gitignore, Open in File Explorer, Open in Zed.
struct FileMenu {
    project_id: Ulid,
    /// The worktree the Files view showed when the menu opened (`None` = the project folder).
    target: Option<Ulid>,
    /// Relative to that folder, `/` separators; `""` = the folder itself.
    path: String,
    is_dir: bool,
    /// Window position of the click.
    position: gpui::Point<gpui::Pixels>,
}

/// The right-click menu of a Files tab (SPEC §12A): Close, Close others, Close to the right,
/// Close all, Pin / Unpin, Copy path.
struct FileTabMenu {
    project_id: Ulid,
    path: String,
    /// Window position of the click.
    position: gpui::Point<gpui::Pixels>,
}

/// The open popover of the Git view's side panel or diff toolbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitMenu {
    /// The repo strip's chevron: search, then one row per repository.
    Repos,
    /// "By: All ▾": the "Changed by" filter.
    ChangedBy,
    /// The diff toolbar's "⋯": Send to agent and the actions that do not fit.
    More,
}

/// The bindings in force; the keyboard layout observer rebinds from it, so keys set in the
/// Settings dialog survive a layout switch.
pub(crate) struct ActiveKeymap(pub Keymap);

impl gpui::Global for ActiveKeymap {}

struct Shell {
    status: Status,
    /// Dropping it closes the connection.
    connection: Option<Rc<Connection>>,
    /// A newer published release (SPEC §20K), shown as the title bar update button.
    update: Option<mkrew_ui::update::Release>,
    /// The update is downloading / being put in place: the button ignores clicks.
    updating: bool,
    sidebar: Sidebar,
    /// A `project.list` + `session.list` is in flight.
    refreshing: bool,
    /// Another refresh was asked for while one was in flight.
    refresh_again: bool,
    /// The workspace whose tabs are on screen.
    workspace: Option<Ulid>,
    /// Tabs per workspace, loaded when the workspace is first shown.
    tabs: HashMap<Ulid, Tabs>,
    /// Workspaces whose `tab.list` (or first `tab.create`) is in flight.
    loading_tabs: HashSet<Ulid>,
    /// `tab.updated` events of loading workspaces: the `tab.list` reply may predate them, so
    /// they are applied after it (events and replies arrive on different channels).
    early_tab_events: Vec<Event>,
    /// Session to show once its workspace's tabs are loaded (clicked in the sidebar).
    reveal: Option<Ulid>,
    /// The session a `mkrew://` link asked for when it started this UI (ADR 0079); shown
    /// after the first session list.
    pending_jump: Option<(Ulid, Ulid)>,
    /// Last failure of a tab action (new tab), shown in the tab bar.
    tab_error: Option<String>,
    /// The kind of pane `session.spawn` is starting (large agent exes can take seconds on a
    /// cold start): its buttons show a loading mark and further starts are ignored meanwhile.
    starting_pane: Option<SessionKind>,
    /// Which session frames go to a pane, and buffering while attaches are in flight.
    router: Router,
    /// Attached panes of the sessions on screen.
    panes: HashMap<Ulid, Entity<TerminalPane>>,
    /// Failed `session.attach`es of sessions on screen, shown in place of their pane.
    attach_errors: HashMap<Ulid, String>,
    /// Given to panes; signalled after each terminal paint.
    painted: async_channel::Sender<()>,
    /// The frame loop's end of `painted` (each connection's loop reads it).
    painted_rx: async_channel::Receiver<()>,
    /// Where the daemon is, for connecting again after a failure or a lost connection.
    paths: Paths,
    /// Bumped by every connect attempt; a pending auto-retry timer from an older one is dropped.
    connect_gen: u64,
    /// Failed or lost connects since the last good one (0 = none), indexing the backoff.
    reconnect_attempts: u32,
    /// The wait before the next automatic retry, shown in the status line.
    reconnect_in: Option<Duration>,
    /// Focused while no pane is, so window-level actions (Add workspace) still dispatch.
    focus_handle: FocusHandle,
    /// The tab or pane title being edited inline.
    renaming: Option<Renaming>,
    /// Focused while renaming; losing it commits the draft.
    rename_focus: FocusHandle,
    _rename_blur: Subscription,
    /// The bound shortcuts, for tooltips and labels.
    keymap: Keymap,
    /// What was wrong with `keymap.toml` at start (those entries use their defaults).
    keymap_error: Option<String>,
    /// The same problems, one palette entry each.
    keymap_errors: Vec<KeymapError>,
    /// `<config>/mkrew/keymap.toml` (the Settings dialog writes it).
    keymap_file: PathBuf,
    /// `<config>/mkrew/settings.toml`, read when a composer `@` pick is typed and polled for
    /// font changes (§20.0).
    settings_file: PathBuf,
    /// Its mtime at the last font check (ADR 0024); `None` until the first status tick reads it.
    settings_modified: Option<std::time::SystemTime>,
    /// Why the last read of `settings_file` failed; listed in the palette (§20.0).
    settings_error: Option<String>,
    /// `<config>/mkrew/prompts`, read (seeded the first time) when a prompt picker opens.
    prompts_dir: PathBuf,
    /// `<config>/mkrew`: `mk web` keeps `web_token` there (Phone access link, ADR 0053).
    config_dir: PathBuf,
    /// Phone access (ADR 0053): what `daemon.status` and Tailscale last said; the title bar
    /// icon and the dialog read it.
    phone: Phone,
    /// The "Phone access" dialog is open (modal).
    phone_open: bool,
    /// The open Settings dialog (modal, ADR 0073).
    settings_dialog: Option<SettingsDialog>,
    /// Scrolls its rows.
    settings_scroll: ScrollHandle,
    /// The open workspace menu (Open in Zed, default agent).
    workspace_menu: Option<WorkspaceMenu>,
    /// The open right-click menu of the Files tree.
    file_menu: Option<FileMenu>,
    /// The open right-click menu of a Files tab.
    file_tab_menu: Option<FileTabMenu>,
    /// The open tab menu (Pin, Split right / down, Rename, Close).
    tab_menu: Option<TabMenu>,
    /// The sidebar-row or tab reorder drag in progress (stale once no drag is active).
    reorder: Option<Reorder>,
    /// The open menu of the tab strip's split button (Split right / down, layouts).
    layout_menu: Option<LayoutMenu>,
    /// The open pane menu (Pin, Rename, Change color).
    pane_menu: Option<PaneMenu>,
    /// The open Share menu of an agent pane (Follow in Zed), at the click.
    share_menu: Option<PaneMenu>,
    /// The open "Run setup" of a workspace (modal).
    run_setup: Option<RunSetup>,
    /// Keeps the Run setup's focused card in view; its scroll bar's drag.
    run_setup_scroll: ScrollHandle,
    run_setup_drag: scrollbar::Drag,
    /// The workspace whose Run menu (▾) is open.
    run_menu: Option<Ulid>,
    /// The title bar's workspace dropdown is open.
    workspace_switcher: bool,
    /// The title bar's "+ New" dropdown is open.
    new_menu: bool,
    /// The sidebar's "N empty workspaces" line is expanded (in memory only).
    empty_workspaces_open: bool,
    /// Sidebar groups (a workspace's TERMINAL / RUN) folded to their label (in memory only).
    collapsed_groups: HashSet<(Ulid, RowGroup)>,
    /// Agent panes start at reading width (`[ui] reading_width`); a pane toggled since holds
    /// its own choice here (in memory only). Shell and run panes default to full width.
    reading_default: bool,
    reading_width: HashMap<Ulid, bool>,
    /// The run tab to show once its `tab.updated` arrives (it may trail `project.run_start`).
    run_reveal: Option<Ulid>,
    /// Ports of live run sessions (SPEC §10).
    ports: Ports,
    /// Profile id → `icon` (ADR 0025), reloaded on every refresh.
    agent_icons: HashMap<String, String>,
    /// The agent profiles of the last refresh: what "+ New" and the palette offer a new pane.
    agent_profiles: Vec<mkrew_protocol::AgentProfileInfo>,
    /// The open "Broadcast to agents…" dialog (modal).
    broadcast: Option<Broadcast>,
    /// The open "Ask AI…" dialog (modal, ADR 0061).
    ask: Option<Ask>,
    /// Its "Web search" switch, kept for the next question (in memory only).
    ask_web: bool,
    /// Finished sessions already seen, so they leave the Inbox; kept across restarts.
    viewed: Viewed,
    /// The OS around the window (ADR 0064): taskbar badge and flash, tray icon, hide / show.
    desktop: Desktop,
    /// What `desktop` last showed, so only a change reaches the OS.
    attention: Tracker,
    /// `[ui] close_to_tray`: the window's ✕ hides it while the tray icon is there.
    close_to_tray: bool,
    /// `[notify] flash_taskbar`.
    flash_taskbar: bool,
    /// Each session's last turn, summarized while it is done.
    turns: Turns,
    /// Each session's newest token usage (SPEC §20E.3).
    usages: Usages,
    /// Each linked session's plan checklist (ADR 0039).
    plans: Plans,
    /// Why the last link / unlink of a session failed (a file that is not a readable `.md`),
    /// shown in that session's Plan card.
    plan_error: Option<(Ulid, String)>,
    /// Full width, compact rail or hidden (Toggle sidebar / Toggle compact sidebar). Not
    /// remembered across restarts.
    sidebar_layout: sidebar::Layout,
    /// Every workspace's task cards.
    board: Board,
    /// Why the last `task.list` failed, shown on the board.
    board_error: Option<String>,
    /// Files edited by several sessions, warned about on their cards and pane headers.
    conflicts: Conflicts,
    /// Why the last card action (drop, Run, Approve) failed, shown on its workspace's board.
    card_error: Option<(Ulid, String)>,
    /// The Board's Jira side per workspace (SPEC §20I); kept across reconnects.
    jira: HashMap<Ulid, JiraView>,
    /// `[jira] site` is set: the Board shows the `Tasks | Jira` switch.
    jira_on: bool,
    /// `[jira] mine`: the `All | Mine` filter, every workspace.
    jira_mine: bool,
    /// `[jira] layout`: the `List | Kanban` switch, every workspace.
    jira_layout: mkrew_core::settings::JiraLayout,
    /// The Jira keys field.
    jira_keys_focus: FocusHandle,
    /// The Jira list's "Filter issues" field.
    jira_filter_focus: FocusHandle,
    /// The card Ctrl+Enter runs (clicked last).
    selected_card: Option<Ulid>,
    /// Tasks whose `task.run` is in flight (Run is disabled meanwhile).
    starting: HashSet<Ulid>,
    /// The open "Run with…" picker of one card (SPEC §20B.6).
    run_with: Option<RunWith>,
    /// The open "Run with several agents…" picker of one card (SPEC §20E.2).
    fan_out: Option<FanOutPicker>,
    /// The follow-up draft of one running / needs-input card.
    follow_up: Option<FollowUp>,
    /// Focused while the follow-up field takes keys; losing it drops an empty draft.
    follow_up_focus: FocusHandle,
    _follow_up_blur: Subscription,
    /// Keeps the follow-up field's end (where the caret is) in view.
    follow_up_scroll: ScrollHandle,
    /// The prompt composer draft of every agent session (§20B.1).
    composers: Composers,
    /// The pane under an OS file drag (§20B.5), tracked without GPUI's hover check: gpui-pre
    /// 0.3.7 keeps keyboard modality during OLE drags, so `on_drop` misses drops after typing.
    drop_hover: Option<(Ulid, ExternalPaths)>,
    /// Focused while the focused pane's composer takes keys.
    composer_focus: FocusHandle,
    /// Composing IME chars typed into the focused field so far (see `mkrew_ui::ime`).
    ime_marked: Rc<ime::Composing<FocusHandle>>,
    /// The text field that had the keyboard at the last paint (voice input types there, §20G).
    typing_target: ime::FocusedField<Shell>,
    /// Voice input (§20G).
    voice: voice::State,
    composer_scroll: ScrollHandle,
    /// The view shown per workspace (Terminals when not set).
    views: HashMap<Ulid, View>,
    /// The session panel right of Terminals is open (closed at every start; SPEC §10).
    inspector_open: bool,
    /// The session panel's Thread section (ADR 0057): the shown session's reading. `None`
    /// while the panel is closed or shows no agent with a native id.
    thread: Option<ThreadWatch>,
    /// Its width in px, from `[ui] inspector_width` and the drag.
    inspector_width: f32,
    /// The edge was dragged since the last mouse-up: the width is saved when it ends.
    inspector_dragging: bool,
    /// The workspace sidebar's width in px, from `[ui] sidebar_width` and the drag.
    sidebar_width: f32,
    /// Its edge was dragged since the last mouse-up: the width is saved when it ends.
    sidebar_dragging: bool,
    /// A divider between panes was dragged since the last mouse-up: the active tab's layout is
    /// saved when it ends.
    pane_dragging: bool,
    /// The Docs view of every workspace it was shown for.
    docs: HashMap<Ulid, Docs>,
    /// Focused while a Docs comment or reply draft takes keys (the draft stays on blur).
    comment_focus: FocusHandle,
    /// The Files view of every workspace it was shown for.
    files: HashMap<Ulid, FilesView>,
    /// Focused while a Files view is shown, so arrow keys move its tree cursor.
    files_focus: FocusHandle,
    /// Keeps the Files tree cursor in view.
    files_scroll: UniformListScrollHandle,
    /// Keeps the active Files tab in view in the tab strip.
    file_tabs_scroll: ScrollHandle,
    /// The Files viewer's find field (outside `FILES_CONTEXT`'s arrow keys).
    file_find_focus: FocusHandle,
    /// The Files find bar's replace field.
    file_replace_focus: FocusHandle,
    /// Keeps the current find match of the Files viewer in view.
    file_code_scroll: UniformListScrollHandle,
    /// Where the Files editor's scroll bar thumb is held while it is dragged.
    file_code_drag: scrollbar::Drag,
    /// What waits for a Files save in flight (a "Save" answered while one was already sent):
    /// `(workspace, path, then)`, run after that save succeeds.
    files_after_save: Vec<(Ulid, String, AfterSave)>,
    /// The Files editor (ADR 0028): typing goes there while it has focus.
    file_editor_focus: FocusHandle,
    /// The block edit field of the Files Markdown preview (ADR 0072).
    file_block_focus: FocusHandle,
    /// Where the editor's text starts on screen (set while painting), for clicks.
    file_text_left: Rc<Cell<Pixels>>,
    /// The Docs list filter field.
    docs_filter_focus: FocusHandle,
    /// The rendered Markdown page of Docs and of the Files view (virtual lists, kept across
    /// frames).
    doc_page: preview::Page,
    file_page: preview::Page,
    /// The Git view of every workspace it was shown for.
    git: HashMap<Ulid, GitView>,
    /// The `git.repos` of each workspace whose Git view was shown (ADR 0029), for the "Repo"
    /// chooser; kept in memory only.
    git_repos: HashMap<Ulid, Vec<String>>,
    /// The title bar's branch chip and `Git N` badge: the shown workspace only (hidden ones
    /// are never loaded), in memory only.
    git_summary: Option<(Ulid, GitSummary)>,
    /// A summary request is in flight / another change arrived meanwhile.
    git_summary_busy: bool,
    git_summary_again: bool,
    /// Focused while a Git view is shown, so its single-key shortcuts work.
    git_focus: FocusHandle,
    /// The commit box of the shown Git view (outside `GIT_CONTEXT`, so typing never stages).
    git_commit_focus: FocusHandle,
    /// The side panel's "Filter files" field (outside `GIT_CONTEXT`, so typing never stages).
    git_filter_focus: FocusHandle,
    /// The open Git popover (one at a time), and the search typed in the Repositories menu.
    git_menu: Option<GitMenu>,
    git_repo_query: Input,
    git_repo_query_focus: FocusHandle,
    /// Keep the selected changed file in view.
    git_files_scroll: UniformListScrollHandle,
    /// The diff rows (a variable-height list: wrapped lines are taller). `git_diff_layout` is
    /// what it was last laid out for (the diff, mode, row count; another resets it) and
    /// `git_diff_looks` what changes row heights in place (wrap, picked lines: remeasured).
    /// `git_diff_scroll_to` is a row to scroll to once the list is laid out for the shown diff.
    git_diff_list: ListState,
    git_diff_layout: Cell<Option<(usize, DiffMode, usize)>>,
    git_diff_looks: Cell<Option<(bool, Option<Selection>)>>,
    git_diff_scroll_to: Cell<Option<usize>>,
    /// How the Git diff viewer lays out a file (`[ui] diff_mode`, §12B); global, every repo.
    diff_mode: DiffMode,
    /// Long diff lines wrap (`[ui] diff_wrap`, on by default); global, every repo.
    diff_wrap: bool,
    /// `[features]` (ADR 0046): the optional modules switched on; re-read with the settings.
    features: mkrew_core::settings::FeatureSettings,
    /// The open "Discard changes" confirmation of a Git view file (modal).
    git_discard: Option<git_view::Discard>,
    /// The open base menu of a Git view (modal).
    git_base: Option<BaseDialog>,
    /// The open "Add workspace" dialog: a local folder or a clone (modal, ADR 0076).
    add_workspace: Option<CloneDialog>,
    /// The open new / edit task dialog (modal).
    task_dialog: Option<TaskDialog>,
    /// The open "Undo task" / "Restore before turn" confirmation (modal; never open together
    /// with `task_dialog` or `turns_dialog`).
    undo_dialog: Option<UndoDialog>,
    /// The open "Merge back…" / "Remove worktree…" of a card (modal, SPEC §20E.1).
    worktree_dialog: Option<WorktreeDialog>,
    /// The open "Compare" of a group card (modal, SPEC §20E.2).
    compare: Option<Compare>,
    /// The open "Turns…" of an agent pane (modal).
    turns_dialog: Option<TurnsDialog>,
    /// The Turns dialog a "Restore before turn n" left; cancelling the restore goes back to it.
    turns_return: Option<TurnsDialog>,
    /// The agent sessions followed in Zed (§20C.5; UI memory only).
    follow: Follow,
    /// The open "History" of a workspace (modal; never open together with another dialog).
    history: Option<HistoryDialog>,
    /// The read-only replay of the history's selected session (never attached).
    history_pane: Option<Entity<TerminalPane>>,
    /// The open Docs "Send to agent" / "View changes" dialog (modal).
    doc_dialog: Option<DocDialog>,
    /// The open quick open finder (modal).
    quick_open: Option<QuickOpen>,
    /// Keeps the quick open pick in view.
    quick_open_scroll: ScrollHandle,
    /// The open command palette (modal).
    palette: Option<Palette>,
    /// Keeps the palette pick in view.
    palette_scroll: ScrollHandle,
    /// What had the keyboard when the palette opened; a picked command acts on it.
    palette_return: Option<(FocusHandle, Option<Ulid>)>,
    /// The theme mode while the palette is the theme picker (ADR 0078); Esc rebuilds the theme
    /// for it from the current settings. `Some` while the palette is the theme picker.
    theme_pick: Option<mkrew_core::settings::ThemeMode>,
    /// The open prompt picker (modal; over the task dialog when it types into it).
    prompt_picker: Option<PromptPicker>,
    /// Keeps the prompt picker's pick in view.
    prompt_picker_scroll: ScrollHandle,
    /// The open peek popover (§20A.1).
    peek: Option<Peek>,
    /// The session row under the mouse, for the peek's hover delay.
    peek_hover: peek::Hover,
    /// Focused while the peek popover has the keyboard.
    peek_focus: FocusHandle,
    /// Focused while the sidebar's filter box has the keyboard.
    filter_focus: FocusHandle,
    /// What had the keyboard before the peek took it; it gets it back on close.
    peek_return: Option<(FocusHandle, Option<Ulid>)>,
    /// Focused while a dialog is open; the task dialog's keys edit the draft.
    dialog_focus: FocusHandle,
    /// Scrolls the dialog's prompt field to its end (where the caret is) after edits.
    prompt_scroll: ScrollHandle,
    /// Bumped per opened dialog, so a late reply never changes or closes a newer one.
    dialog_serial: u64,
    /// Process start, until the first frame and the first connected frame are logged
    /// (`scripts/measure.*`, SPEC §15 UI startup).
    launched: Option<Instant>,
    first_frame_logged: bool,
    /// The sidebar as its own view (see [`shell::sidebar_view::SidebarView`]); made on first
    /// render.
    sidebar_view: Option<Entity<shell::sidebar_view::SidebarView>>,
    /// The in-app toasts (SPEC §14): pushed by `toast_error` / `toast_info`, expired on the
    /// status tick.
    toasts: Toasts,
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        // Voice input types into the field whose typing canvas registers again during this
        // frame's paint (§20G); a field that is gone from the screen is no target. That field
        // paints the listening ring.
        self.typing_target.field.replace(None);
        self.typing_target.ring.set(self.voice_ring(&theme, cx));
        window.set_rem_size(px(cx.global::<theme::Fonts>().ui_size));
        if let Some(launched) = self.launched {
            if !self.first_frame_logged {
                self.first_frame_logged = true;
                tracing::info!(ms = launched.elapsed().as_millis(), "first frame");
            }
            if matches!(self.status, Status::Connected) {
                self.launched = None;
                tracing::info!(ms = launched.elapsed().as_millis(), "first connected frame");
            }
        }
        // One clock reading per frame, so every spinner and time agrees.
        let now = status::now_ms();
        // Rendering is what marks the focused pane of the active window viewed (its done leaves
        // the Inbox): every state change and window activation re-renders. A change in what
        // needs the user also reaches the badge and the tray.
        let mut viewed_changed = false;
        if window.is_window_active()
            && let Some(id) = self.focused_session()
            && let Some(session) = self.sidebar.sessions().iter().find(|s| s.id == id)
        {
            viewed_changed = self.viewed.view(session);
        }
        if viewed_changed {
            self.sync_attention(cx);
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .track_focus(&self.focus_handle)
            .when(
                self.view() == View::Board && !self.jira_side_shown(),
                |el| el.key_context(BOARD_CONTEXT),
            )
            .when(self.view() == View::Docs, |el| el.key_context(DOCS_CONTEXT))
            .when(self.view() == View::Files, |el| {
                el.key_context(FILES_CONTEXT)
            })
            .on_action(cx.listener(Self::run))
            // `/` focuses the sidebar filter while nothing else has the keyboard (a terminal or
            // a field keeps its `/`).
            .on_key_down(cx.listener(|shell, event: &KeyDownEvent, window, cx| {
                let filter_shown = !shell.sidebar_layout.hidden()
                    && !shell.sidebar_layout.compact()
                    && !shell.sidebar.is_empty();
                if event.keystroke.key == "/"
                    && !event.keystroke.modifiers.modified()
                    && filter_shown
                    && !shell.modal_open()
                    && shell.focus_handle.is_focused(window)
                {
                    cx.stop_propagation();
                    window.focus(&shell.filter_focus, cx);
                    cx.notify();
                }
            }))
            .child(self.render_title_bar(self.view(), now, &theme, window, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .relative()
                    // Hidden: not built at all (nothing to lay out or paint). Its own cached
                    // view: a pane printing does not lay it out again (`SidebarView`).
                    .children((!self.sidebar_layout.hidden()).then(|| {
                        let style = gpui::StyleRefinement::default().flex_none().h_full();
                        let style = if self.sidebar_layout.compact() {
                            style.w(theme::rems_px(theme::COMPACT_SIDEBAR_WIDTH))
                        } else {
                            style.w(px(self.sidebar_width))
                        };
                        let shell = cx.entity().downgrade();
                        self.sidebar_view
                            .get_or_insert_with(|| {
                                cx.new(|cx| shell::sidebar_view::SidebarView::new(shell, cx))
                            })
                            .clone()
                            .cached(style)
                    }))
                    .child(self.render_main(now, &theme, window, cx))
                    .children(
                        self.task_dialog
                            .as_ref()
                            .map(|dialog| self.render_task_dialog(dialog, &theme, cx)),
                    )
                    .children(
                        self.undo_dialog
                            .as_ref()
                            .map(|dialog| self.render_undo_dialog(dialog, &theme, cx)),
                    )
                    .children(
                        self.worktree_dialog
                            .as_ref()
                            .map(|dialog| self.render_worktree_dialog(dialog, &theme, cx)),
                    )
                    .children(
                        self.compare
                            .as_ref()
                            .map(|compare| self.render_compare(compare, now, &theme, cx)),
                    )
                    .children(
                        self.turns_dialog
                            .as_ref()
                            .map(|dialog| self.render_turns_dialog(dialog, now, &theme, cx)),
                    )
                    .children(
                        self.git_discard
                            .as_ref()
                            .map(|dialog| self.render_git_discard(dialog, &theme, cx)),
                    )
                    .children(
                        self.git_base
                            .as_ref()
                            .map(|dialog| self.render_git_base(dialog, &theme, cx)),
                    )
                    .children(
                        self.add_workspace
                            .as_ref()
                            .map(|dialog| self.render_add_workspace(dialog, &theme, cx)),
                    )
                    .children(
                        self.phone_open
                            .then(|| self.render_phone_access(&theme, cx)),
                    )
                    .children(self.settings_dialog.as_ref().map(|dialog| {
                        self.render_settings_dialog(
                            dialog,
                            window.viewport_size().height,
                            &theme,
                            cx,
                        )
                    }))
                    .children(
                        self.run_setup
                            .as_ref()
                            .map(|setup| self.render_run_setup(setup, &theme, cx)),
                    )
                    .children(
                        self.broadcast
                            .as_ref()
                            .map(|dialog| self.render_broadcast(dialog, &theme, cx)),
                    )
                    .children(
                        self.ask
                            .as_ref()
                            .map(|dialog| self.render_ask(dialog, &theme, cx)),
                    )
                    .children(
                        self.history
                            .as_ref()
                            .map(|dialog| self.render_history(dialog, now, &theme, cx)),
                    )
                    .children(self.doc_dialog.as_ref().map(|dialog| match dialog {
                        DocDialog::Send(dialog) => self.render_send_dialog(dialog, now, &theme, cx),
                        DocDialog::Changes(changes) => self.render_changes(changes, &theme, cx),
                    }))
                    .children(
                        self.quick_open
                            .as_ref()
                            .map(|open| self.render_quick_open(open, &theme, cx)),
                    )
                    .children(
                        self.palette
                            .as_ref()
                            .map(|palette| self.render_palette(palette, &theme, cx)),
                    )
                    .children(
                        self.prompt_picker
                            .as_ref()
                            .map(|picker| self.render_prompt_picker(picker, &theme, cx)),
                    )
                    .children(
                        self.tab_menu
                            .as_ref()
                            .filter(|_| self.view() == View::Terminals && !self.modal_open())
                            .map(|menu| self.render_tab_menu(menu, &theme, cx)),
                    )
                    .children(
                        self.layout_menu
                            .as_ref()
                            .filter(|_| self.view() == View::Terminals && !self.modal_open())
                            .map(|menu| self.render_layout_menu(menu, &theme, cx)),
                    )
                    .children(
                        self.pane_menu
                            .as_ref()
                            .filter(|_| self.view() == View::Terminals && !self.modal_open())
                            .and_then(|menu| self.render_pane_menu(menu, &theme, cx)),
                    )
                    .children(
                        self.share_menu
                            .as_ref()
                            .filter(|_| self.view() == View::Terminals && !self.modal_open())
                            .map(|menu| self.render_share_menu(menu, &theme, cx)),
                    )
                    .children(
                        (self.file_menu.as_ref())
                            .filter(|menu| {
                                self.workspace == Some(menu.project_id)
                                    && self.view() == View::Files
                                    && !self.modal_open()
                            })
                            .map(|menu| self.render_file_menu(menu, &theme, cx)),
                    )
                    .children(
                        (self.file_tab_menu.as_ref())
                            .filter(|menu| {
                                self.workspace == Some(menu.project_id)
                                    && self.view() == View::Files
                                    && !self.modal_open()
                            })
                            .map(|menu| self.render_file_tab_menu(menu, &theme, cx)),
                    )
                    .children(self.render_toasts(&theme, cx))
                    .children(self.render_voice_pill(&theme, cx)),
            )
    }
}
