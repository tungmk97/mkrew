//! `mkrew`: the desktop app (SPEC §10). Connects to the daemon (starting it if needed), lists the
//! workspaces and their sessions in the sidebar and shows the active workspace's terminal tabs,
//! each a split tree of panes. Only the panes on screen are attached.

// No console window next to the app in release builds (debug keeps stderr visible).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::cell::{Cell, RefCell};
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
use mkrew_core::UsageFormat;
use mkrew_core::prompt::PromptVars;
use mkrew_core::settings::{DiffMode, NotifySettings, OpenWorkspace, PanelSide, WindowMode};
use mkrew_protocol::{
    AgentProfilesResult, AiGenerateParams, AiGeneratedEvent, AiKind, ChangeKind, CommentContext,
    CommentId, CommentListParams, CommentRef, Commit, CommitDetail, DEFAULT_COLS, DEFAULT_ROWS,
    DiffLineKind, DocComment, DocContent, DocFile, DocListParams, DocReadParams, Event, FileDiff,
    FsContent, FsCreateParams, FsCreated, FsEntry, FsIgnoreParams, FsListParams, FsMatch,
    FsNewNoteParams, FsWritten, GitCheckoutParams, GitCommitResult, GitMarker, GitMode, GitStatus,
    GitStatusParams, Outcome, Project, ProjectAddParams, ProjectClonedEvent, ProjectRef,
    ProjectUpdateParams, RemoveWorktreeResult, Request, RunStartParams, RunStarted, RunStopParams,
    Session, SessionAttachResult, SessionFiles, SessionKillParams, SessionKind,
    SessionLinkPlanParams, SessionListParams, SessionLogTail, SessionLogTailParams,
    SessionMoveParams, SessionPinParams, SessionPlanEvent, SessionPortsEvent, SessionReadResult,
    SessionRef, SessionResumeParams, SessionSendParams, SessionSendResult, SessionSpawnResult,
    SessionState, SessionTranscript, SessionTranscriptParams, SessionUnqueueParams,
    SessionUsageEvent, SlashListResult, StateSource, TabCreateParams, TabListParams, TabPinParams,
    TabRef, TabSetLayoutParams, Task, TaskFanoutParams, TaskListParams, TaskMoveParams, TaskRef,
    TaskStatus, TerminalTab, Turn, UndoPreview, ViewedFile,
};
use mkrew_ui::ask::{self, Ask};
use mkrew_ui::attention::{self, Tracker};
use mkrew_ui::base_dialog::{self, BaseDialog};
use mkrew_ui::board::{self, Board, Card, CardAction, DraggedCard, View};
use mkrew_ui::broadcast::{self, Broadcast, Delivery as BroadcastDelivery};
use mkrew_ui::client::{Connection, Incoming};
use mkrew_ui::clone_dialog::{CloneDialog, Field as CloneField};
use mkrew_ui::comments::{self, Comments, Selection, Thread};
use mkrew_ui::composer::{self, Composers, Draft};
use mkrew_ui::conflicts::{self, Conflicts};
use mkrew_ui::context_menu::{LayoutMenu, PaneMenu, TabMenu};
use mkrew_ui::desktop::{Desktop, TrayEvent};
use mkrew_ui::detached::{self, Detached, DockTab, HostId, SpaceKey, WindowKey};
use mkrew_ui::diagram::Diagram;
use mkrew_ui::dialog_focus::{self, Nav};
use mkrew_ui::doc_dialog::{self, Changes, DocDialog, SendDialog};
use mkrew_ui::docs::{Doc, Docs, Files, Mode, Shown};
use mkrew_ui::documents::{self, Documents, MdMode, Resolution};
use mkrew_ui::editor::{self, EditKey, Pos};
use mkrew_ui::fanout::{self, Compare, FanOutPicker};
use mkrew_ui::field::{Caret, caret_mouse, caret_text, copy_selection};
use mkrew_ui::files_view::{self, FilesView, Requests};
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
use mkrew_ui::inspector::{self, PanelTab};
use mkrew_ui::jira_board::{JiraView, Load, Source};
use mkrew_ui::keymap::{
    self as keymap, BOARD_CONTEXT, BROWSER_CONTEXT, COMMENTS_CONTEXT, Command, DOCS_CONTEXT,
    EDITOR_CONTEXT, FILE_TREE_CONTEXT, GIT_CONTEXT, Keymap, KeymapError, MENU_CONTEXT, Os, Run,
};
use mkrew_ui::launch::Launch;
use mkrew_ui::launcher::Launcher;
use mkrew_ui::lifecycle::{self, CloseAction, CloseChoice, RunWith};
use mkrew_ui::machine_notify::MachineNotices;
use mkrew_ui::machines::{Machine, MachineState, Machines};
use mkrew_ui::menu::{self as menu_model, Dot, Lead, MenuItem, MenuKey, MenuRow, Tone, Trail};
use mkrew_ui::palette::{self, Extra, Palette, Scope};
use mkrew_ui::pane_text;
use mkrew_ui::peek::{self, Peek};
use mkrew_ui::phone_dialog::{self, LinkBlock, Local, Mark, Phone, PushSwitch, SetUp, Switch};
use mkrew_ui::plan::{self, Plans};
use mkrew_ui::plan_dialog::PlanDialog;
use mkrew_ui::plus_row::{NewKind, Reveals, RowItem, default_agent, plus_row_items};
use mkrew_ui::ports::PortsDialog;
use mkrew_ui::preview;
use mkrew_ui::prompt_picker::{self, PromptPicker};
use mkrew_ui::redraw;
use mkrew_ui::router::{Delivery, Router};
use mkrew_ui::run::{self, Ports, RowState, RunAction, Summary};
use mkrew_ui::run_setup::{self, RunSetup};
use mkrew_ui::scrollbar;
use mkrew_ui::settings_dialog::{self, SettingsDialog};
use mkrew_ui::sidebar::{self, Change, Row, SessionRow, Sidebar, WorkspaceRow};
use mkrew_ui::slash_picker::{self, SlashKey, SlashPicker};
use mkrew_ui::split::{Axis, Direction, Divider, Edge, FileKey, Leaf, PaneKey, Preset};
use mkrew_ui::status;
use mkrew_ui::stt;
use mkrew_ui::tabs::{
    AreaItem, FIRST_TAB_TITLE, PaneIcon, Tab, TabStatus, Tabs, WebPlace, area_items, diff_label,
    double_click_renames, file_label, newest_unplaced, on_screen, rename_params, scroll_index,
    session_label, tab_icons, web_label, web_place,
};
use mkrew_ui::task_dialog::{self, DialogKey, Field, TaskDialog};
use mkrew_ui::terminal::find::{FindKey, find_key};
use mkrew_ui::terminal::view::{
    AttachFailed, CopyModeChanged, LocalFiles, NoLinks, OpenLink, TerminalPane,
};
use mkrew_ui::text_box::{self, Input};
use mkrew_ui::text_find::TextFind;
use mkrew_ui::theme::{self, ActiveTheme, Theme, font_size, opacity, radius, space, weight};
use mkrew_ui::theme_catalog::{ThemeCatalog, ThemesStamp};
use mkrew_ui::toast::{self, Toasts};
use mkrew_ui::turn::Turns;
use mkrew_ui::turns_dialog::{self, TurnsDialog};
use mkrew_ui::undo_dialog::{self, LineKind, Preview, UndoDialog};
use mkrew_ui::usage::{self, Usages};
use mkrew_ui::voice;
use mkrew_ui::watches;
use mkrew_ui::window_place::{self, Placement};
use mkrew_ui::workspace_menu::{self, Agents, WorkspaceMenu};
use mkrew_ui::worktree_dialog::{self, WorktreeDialog};
use mkrew_ui::worktree_target;
use mkrew_ui::zed;
use shell::window_mode::Then;
use tracing_subscriber::EnvFilter;
use ulid::Ulid;

mod shell;
use shell::board::*;
use shell::cards::*;
use shell::chat_view::*;
use shell::diff_pane::DiffViewState;
use shell::doc_view::{DocFocus, DocView};
use shell::files_render::*;
use shell::files_split;
use shell::git_diff::*;
use shell::git_menus::*;
use shell::git_rows::*;
use shell::menu_render::*;
use shell::menus::*;
use shell::pane_drop::*;
use shell::pane_render::*;
use shell::plus_row::*;
use shell::reorder::*;
use shell::sidebar::*;
use shell::tear_off::*;
use shell::thread_panel::*;
use shell::tsp_view::*;
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
pub(crate) const CUSTOM_TITLE_BAR: bool = cfg!(any(target_os = "windows", target_os = "macos"));
/// Only Windows draws ─ □ ✕ (macOS keeps its traffic lights).
pub(crate) const DRAWN_CONTROLS: bool = cfg!(target_os = "windows");

fn main() {
    // gpui-pre-windows 0.3.7 draws through a topmost DirectComposition visual
    // (directx_renderer.rs:1017), which covers child HWNDs such as the WebView2 page of a
    // browser pane (ADR 0111). Mkrew uses no transparent or blurred window background, so the
    // HWND swap chain path loses nothing it needs. A user's own setting wins.
    // Without the WebView2 Runtime no page shows, so DirectComposition stays on.
    #[cfg(windows)]
    if std::env::var_os(mkrew_ui::browser::DISABLE_DCOMP_ENV).is_none()
        && mkrew_ui::browser::available().is_ok()
    {
        #[allow(unsafe_code)]
        // SAFETY: no thread of Mkrew exists yet (`available` only reads the registry and the
        // WebView2 install, and caches the answer), so nothing reads or writes the environment
        // concurrently.
        unsafe {
            std::env::set_var(mkrew_ui::browser::DISABLE_DCOMP_ENV, "1");
        }
        // Children (the daemon, Zed, the updater, machine windows) get it removed again: `browser::strip_own_env`.
        mkrew_ui::browser::DCOMP_SET_BY_MKREW.store(true, std::sync::atomic::Ordering::Relaxed);
    }
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
    // `mkrew`, `mkrew <mkrew:// link>` (a toast click, ADR 0079) or `mkrew --machine <name>`.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let launch = mkrew_ui::launch::parse(&args);
    let id = mkrew_ui::launch::instance_id(&launch, &paths.socket);
    let (Launch::Local { jump } | Launch::Machine { jump, .. }) = &launch;
    let jump = *jump;
    // A second launch of the same exe on the same daemon (or of the same machine) shows the
    // running window (it may be hidden in the tray) instead of opening another one; a dev
    // build at another path, or on another `MKREW_SOCKET`, starts on its own.
    if mkrew_ui::desktop::raise_running_instance(&id, jump) {
        return;
    }
    // macOS: warms the login shell's PATH probe for this window's own lookups ("Open in Zed").
    mkrew_core::login_path::warm();
    // The machine list is read once, here (spec §4.5).
    let (target, title) = match &launch {
        Launch::Local { .. } => (Target::Local, "Mkrew".to_owned()),
        Launch::Machine { name, .. } => {
            let found = Machines::load(&paths.config_dir).and_then(|machines| {
                machines
                    .get(name)
                    .cloned()
                    .ok_or_else(|| format!("{name} is no longer in the machine list."))
            });
            let target = match found {
                Ok(machine) => Target::Machine(machine),
                Err(text) => Target::Missing(text),
            };
            (target, format!("Mkrew - {name}"))
        }
    };
    let opening = Opening { target, id, jump };
    let app = gpui_platform::application().with_assets(icons::Assets);
    // macOS: a click on the Dock icon while the app is hidden brings the main window back.
    app.on_reopen(shell::app_menu::reopen);
    app.run(move |cx: &mut App| {
        cx.set_global(ActiveKeymap(keymap.clone()));
        cx.set_global(shell::app_menu::LocalWindow(!opening.target.is_machine()));
        shell::app_menu::bind_all(&keymap, cx);
        shell::app_menu::register(cx);
        cx.set_global(MenuAnchors::default());
        // gpui draws animations in their end state while this is set (ADR 0097).
        cx.set_reduce_motion(mkrew_ui::desktop::reduce_motion());
        // Which char a shifted digit reports depends on the layout: bind again after a switch
        // (e.g. US ↔ Vietnamese), or Ctrl+Shift+1..5 stop working until a restart.
        cx.on_keyboard_layout_change(|cx| {
            let keymap = cx.global::<ActiveKeymap>().0.clone();
            shell::app_menu::bind_all(&keymap, cx);
        })
        .detach();
        // A broken file: the defaults until it is fixed (the daemon reports it, §20.0).
        let settings = mkrew_core::settings::load(&paths.settings_file()).unwrap_or_default();
        cx.set_global(theme::Fonts::from_settings(
            &settings,
            &cx.text_system().all_font_names(),
        ));
        cx.set_global(theme::ThemeNames::from_settings(&settings));
        cx.set_global(mkrew_ui::terminal::input::KeyEnv::current(
            settings.terminal.option_as_meta,
        ));
        // Theme files are read once here (the status tick reads them again when they change).
        cx.set_global(ThemeCatalog::load(&paths.themes_dir()));
        // Confirmations are drawn in the window in Mkrew's style, never the OS dialog (§14).
        cx.set_prompt_builder(shell::confirm_prompt::build);
        // The place the window was left (a machine window has its own files): the Space window
        // used last (ADR 0115), else the legacy main window frame.
        let instance = opening.target.is_machine().then_some(opening.id.as_str());
        let layout = detached::load_layout(
            &paths.data_dir.join(detached::windows_file(instance)),
            &paths.data_dir.join(detached::main_file(instance)),
        );
        let displays: Vec<Bounds<Pixels>> = cx
            .displays()
            .iter()
            .map(|display| display.visible_bounds())
            .collect();
        let primary = cx.primary_display().map(|display| display.visible_bounds());
        let window_bounds = match layout.spaces.last() {
            Some(saved) => window_place::restored(
                window_place::saved_rect(&saved.bounds),
                Placement::from_flags(saved.maximized, saved.fullscreen),
                size(px(theme::WINDOW_MIN_WIDTH), px(theme::WINDOW_MIN_HEIGHT)),
                &displays,
                primary,
            ),
            None => WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(theme::WINDOW_WIDTH), px(theme::WINDOW_HEIGHT)),
                cx,
            )),
        };
        let options = shell::window_mode::space_window_options(
            window_bounds,
            title.into(),
            cx.global::<theme::Fonts>().ui_size,
        );
        let mode = settings.ui.theme;
        let opened = cx.open_window(options, |window, cx| {
            theme::refresh(mode, window, cx);
            let shell = cx
                .new(|cx| Shell::new(launched, paths, keymap, &keymap_errors, opening, window, cx));
            cx.set_global(shell::app_menu::ShellHandle(shell.clone()));
            cx.new(|cx| shell::space_view::SpaceWindow::new(shell, SpaceKey(0), window, cx))
        });
        if let Err(error) = opened {
            eprintln!("mkrew: cannot open window: {error:#}");
            cx.quit();
            return;
        }
        // Sessions live in the daemon: the app ends with its last Space window
        // (`Shell::space_should_close` saved first); detached windows never keep it alone.
        cx.on_window_closed(|cx, _| {
            let none_left = (cx.try_global::<shell::app_menu::ShellHandle>())
                .is_none_or(|handle| handle.0.read(cx).space_handles.is_empty());
            if none_left || cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
}

/// What the window connects to.
pub(crate) enum Target {
    /// This computer's daemon.
    Local,
    /// A machine of `machines.toml`, over ssh.
    Machine(Machine),
    /// `--machine` named a machine that is not in the list (or the list is broken): the text,
    /// nothing else.
    Missing(String),
}

impl Target {
    /// A machine window (also one without its machine): no tray, no Phone access, no update.
    pub(crate) fn is_machine(&self) -> bool {
        !matches!(self, Self::Local)
    }
}

/// What `main` hands the first window: where it connects, its single-instance id and the
/// session a toast link points at.
pub(crate) struct Opening {
    target: Target,
    id: String,
    jump: Option<(Ulid, Ulid)>,
}

enum Status {
    Connecting,
    Connected,
    Disconnected,
    /// The daemon cannot be reached.
    Failed(String),
    /// A machine cannot be reached over ssh: why, and what to try.
    Unreachable {
        reason: String,
        hint: Option<String>,
    },
    /// A machine runs another version of Mkrew: ssh's answer, no automatic retry.
    Version(String),
    /// `Target::Missing`: the text, no retry.
    NoMachine(String),
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
/// New folder, Copy path, Add to .gitignore, Open in File Explorer ("Reveal in Finder" on
/// macOS), Open in Zed.
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

/// The menu of a Board card (SPEC §10): its hover buttons as rows, for the keyboard.
struct CardMenu {
    task_id: Ulid,
    /// Window position: the card's bottom-left.
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
    /// Workspaces already warned about their running sessions (`lifecycle::running_alert`).
    running_warned: HashSet<Ulid>,
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
    /// What this window connects to.
    target: Target,
    /// The last state a machine window reported (stdout, title bar chip); `None` before the
    /// first.
    machine_state: Option<MachineState>,
    /// This computer's window: `machines.toml` as read at start and after each change here. An
    /// `Err` (the text, with file and line) is shown on the Machines page and the file is never
    /// written while it holds. Empty in a machine window.
    machines: Result<Machines, String>,
    /// This computer's window: starts and ends the machine windows (spec §4.5), and the state
    /// lines they write, read on the status tick.
    launcher: Option<(Launcher, async_channel::Receiver<(String, MachineState)>)>,
    /// What each machine window this launcher started last reported.
    machine_states: HashMap<String, MachineState>,
    /// Bumped by every connect attempt; a pending auto-retry timer from an older one is dropped.
    connect_gen: u64,
    /// Failed or lost connects since the last good one (0 = none), indexing the backoff.
    reconnect_attempts: u32,
    /// A machine window was connected at least once (a later attempt reports "Reconnecting…";
    /// before that the last "Not reachable" stays).
    was_connected: bool,
    /// The wait before the next automatic retry, shown in the status line.
    reconnect_in: Option<Duration>,
    /// Set while a lost connection is down: the workspace that was shown. The screen stays
    /// (input off, banner on) until the new connection's lists arrive, then the connection-bound
    /// state is cleared and this workspace is shown again.
    kept_workspace: Option<Ulid>,
    /// Focused while no pane is, so window-level actions (Add workspace) still dispatch.
    focus_handle: FocusHandle,
    /// The tab or pane title being edited inline.
    renaming: Option<Renaming>,
    /// Focused while renaming; losing it commits the draft.
    rename_focus: FocusHandle,
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
    /// `<config>/mkrew/themes`: theme files (ADR 0101), polled with `settings.toml`.
    themes_dir: PathBuf,
    /// Their names, times and sizes at the last poll; a difference rebuilds the catalog.
    themes_stamp: ThemesStamp,
    /// `<config>/mkrew/prompts`, read (seeded the first time) when a prompt picker opens.
    prompts_dir: PathBuf,
    /// `<config>/mkrew`: `mk web` keeps `web_token` there (Phone access link, ADR 0053).
    config_dir: PathBuf,
    /// Phone access (ADR 0053): what `daemon.status` and Tailscale last said; the title bar
    /// icon and the dialog read it.
    phone: Phone,
    /// The "Phone access" dialog is open (modal).
    phone_open: bool,
    /// The Ports dialog (spec 2026-11-01), while open.
    ports_dialog: Option<PortsDialog>,
    /// The open Settings dialog (modal, ADR 0073).
    settings_dialog: Option<SettingsDialog>,
    /// The composer had the keyboard when Settings opened: Esc gives it back there.
    settings_from_composer: bool,
    /// Scrolls its rows.
    settings_scroll: ScrollHandle,
    /// Keeps the open prompt's row in view in Settings → Prompts.
    prompts_scroll: ScrollHandle,
    /// Scrolls the prompt text of Settings → Prompts (not `prompt_scroll`: the task dialog can
    /// be open under Settings).
    prompts_body_scroll: ScrollHandle,
    /// The open workspace menu (Open in Zed, default agent).
    workspace_menu: Option<WorkspaceMenu>,
    /// The open right-click menu of the Files tree.
    file_menu: Option<FileMenu>,
    /// The open tab menu (Pin, Split right / down, Rename, Close).
    tab_menu: Option<TabMenu>,
    /// The open menu of a Board card (Shift+F10 on the selected card).
    card_menu: Option<CardMenu>,
    /// Focused while a popover menu is open (any menu, opened by mouse or keyboard): its key
    /// context is `Menu` and [`Shell::menu_key_down`] drives the rows.
    menu_focus: FocusHandle,
    /// What had the keyboard before the first open menu took it (and the terminal session
    /// whose pane it was); Esc, an action or a click outside gives it back.
    menu_return: Option<(FocusHandle, Option<Ulid>)>,
    /// The row the keyboard stands on, and the swatch cursor inside a swatch row.
    menu_selected: Option<usize>,
    menu_swatch: usize,
    /// A group heading whose first row gets the selection as soon as the rows exist (the
    /// agent lists load after the menu opens).
    menu_pending: Option<&'static str>,
    /// The sidebar-row or tab reorder drag in progress (stale once no drag is active).
    reorder: Option<Reorder>,
    /// The open menu of the title bar's split button (Split right / down, layouts).
    layout_menu: Option<LayoutMenu>,
    /// The open pane menu (Pin, Rename, Change color), from a sidebar session row or Shift+F10.
    pane_menu: Option<PaneMenu>,
    /// The open Share menu of an agent pane (Follow in Zed), at the click.
    share_menu: Option<PaneMenu>,
    /// The open "Run setup" of a workspace (modal).
    run_setup: Option<RunSetup>,
    /// Keeps the Run setup's focused card in view; its scroll bar's drag.
    run_setup_scroll: ScrollHandle,
    /// Keeps the Run setup picker's cursor row in view.
    run_picker_scroll: ScrollHandle,
    run_setup_drag: scrollbar::Drag,
    /// The workspace whose Run menu (▾) is open.
    run_menu: Option<Ulid>,
    /// The title bar's workspace dropdown is open.
    workspace_switcher: bool,
    /// The title bar's "need you" chip menu is open (ADR 0115), in `menu_host`'s window.
    need_you_menu: bool,
    /// The tab area's `+ ▾` dropdown is open.
    new_menu: bool,
    /// The pointer holds the tab strip's `+` row open (`plus_row::Reveal`); a focused item
    /// opens it too (`plus_focus`).
    plus: Reveals<HostId>,
    /// Wraps the `+` row: `contains_focused` keeps it open while `+` or an item has the keyboard.
    plus_focus: FocusHandle,
    /// The sidebar's "N empty workspaces" line is expanded (in memory only).
    empty_workspaces_open: bool,
    /// Sidebar groups (a workspace's TERMINAL / RUN) folded to their label (in memory only).
    collapsed_groups: HashSet<(Ulid, RowGroup)>,
    /// Agent panes start at reading width (`[ui] reading_width`); a pane toggled since holds
    /// its own choice here (in memory only). Shell and run panes default to full width.
    reading_default: bool,
    reading_width: HashMap<Ulid, bool>,
    /// `[ui] chat_default` (ADR 0104): the mode of an agent pane whose layout leaf has no
    /// saved mode.
    chat_default: bool,
    /// The run tab to show once its `tab.updated` arrives (it may trail `project.run_start`).
    run_reveal: Option<Ulid>,
    /// Ports of live run sessions (SPEC §10).
    ports: Ports,
    /// Profile id → `icon` (ADR 0025), reloaded on every refresh.
    agent_icons: HashMap<String, String>,
    /// The agent profiles of the last refresh: what the `+` menu and the palette offer a new pane.
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
    /// `[ui] close_to_tray`: the window's ✕ hides it (to the tray icon on Windows, the whole app
    /// on macOS) instead of quitting. Always false in a machine window.
    close_to_tray: bool,
    /// This computer's `[notify]` settings: the taskbar flash, and a machine window decides its
    /// OS notifications with them.
    notify: NotifySettings,
    /// A machine window's OS notifications of the other computer's sessions; `None` in this
    /// computer's window (its daemon notifies).
    machine_notices: Option<MachineNotices>,
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
    /// Files edited by several sessions, warned about on their cards and pane badges.
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
    /// The Board shows every workspace's tasks (`[ui] board_all_workspaces`, SPEC §10).
    board_all: bool,
    /// The agents' usage limits the title bar's chip shows (events and `limits.get`).
    limits: mkrew_ui::limits::Limits,
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
    /// Keeps the follow-up field's end (where the caret is) in view.
    follow_up_scroll: ScrollHandle,
    /// The prompt composer draft of every agent session (§20B.1).
    composers: Composers,
    /// What is under an OS file drag (§20B.5): a pane, or the task dialog's Prompt box. Tracked
    /// without GPUI's hover check: gpui-pre 0.3.7 keeps keyboard modality during OLE drags, so
    /// `on_drop` misses drops after typing.
    drop_hover: Option<(DropTarget, ExternalPaths)>,
    /// Where the tab or pane dragged over a pane would split it (stale once no drag is active).
    pane_drop: Option<PaneDrop>,
    /// Focused while the focused pane's composer takes keys.
    composer_focus: FocusHandle,
    /// Composing IME chars typed into the focused field so far (see `mkrew_ui::ime`).
    // ponytail: one state for every window (spec §4.3): the OS gives one window the keyboard at
    // a time. Per-window state if IME composition across windows ever matters.
    ime_marked: Rc<ime::Composing<FocusHandle>>,
    /// The text field that had the keyboard at the last paint (voice input types there, §20G).
    typing_target: ime::FocusedField<Shell>,
    /// Voice input (§20G).
    voice: voice::State,
    composer_scroll: ScrollHandle,
    /// The view shown per workspace (Terminals when not set).
    views: HashMap<Ulid, View>,
    /// The panel's side of the panes, from `[ui] panel_side` (ADR 0115).
    panel_side: PanelSide,
    /// `[ui] window_mode` (ADR 0115): one window, or one per workspace.
    window_mode: WindowMode,
    /// `[ui] open_workspace` (ADR 0115): where opening a workspace goes in mode 2.
    open_pref: OpenWorkspace,
    /// The open "Open <workspace>" question of mode 2 (ADR 0115).
    open_ask: Option<shell::window_mode::OpenAsk>,
    /// The Space window the open dialog draws in (set by its first frame).
    modal_host: Option<HostId>,
    /// The open "Open workspaces in windows" picker (ADR 0115); drawn in `palette_host`.
    split_picker: Option<mkrew_ui::window_mode::WorkspacePicker>,
    /// The transcripts being read, one per session (ADR 0104): the session panel's Thread
    /// section and every pane in Chat mode share them.
    reads: HashMap<Ulid, ThreadRead>,
    /// The session panel's Thread section UI state.
    thread: Option<ThreadWatch>,
    /// What each pane in Chat mode remembers (open rows, scroll), ADR 0104.
    chats: HashMap<Ulid, ChatUi>,
    /// What each omp pane with a live native surface keeps: the mirror of the daemon's documents
    /// and the scroll (ADR 0107).
    tsps: HashMap<Ulid, TspUi>,
    /// Focused while a pane's chat list has the keyboard (the composer has its own).
    chat_focus: FocusHandle,
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
    /// The open files of every workspace (SPEC §12A, ADR 0108); one store per workspace.
    documents: HashMap<Ulid, Documents>,
    /// Browser panes' page state, by leaf id (ADR 0111); follows the layouts (`sync_browsers`).
    browsers: HashMap<Ulid, shell::browser::Browser>,
    /// The "pages open in your browser" toast was shown this run (ADR 0111).
    browser_fallback_told: bool,
    /// A new browser pane whose address field takes the keyboard once its page state exists
    /// and it is shown (`prepare_browsers`).
    browser_to_focus: Option<Ulid>,
    /// Page events of every browser pane (`wry` callbacks), read by one task (`page_event`).
    page_events: async_channel::Sender<(Ulid, mkrew_ui::browser::PageEvent)>,
    /// The view state of each open file pane, keyed like the store.
    doc_views: HashMap<(Ulid, FileKey), DocView>,
    /// The Files view of every workspace it was shown for.
    files: HashMap<Ulid, FilesView>,
    /// Focused while the right panel's tree has the keyboard, so arrow keys move its cursor.
    files_focus: FocusHandle,
    /// What waits for a Files save in flight (a "Save" answered while one was already sent):
    /// `(workspace, path, then)`, run after that save succeeds.
    files_after_save: Vec<(Ulid, FileKey, AfterSave)>,
    /// The mode the next Markdown file opens in (`[ui] md_mode`, the last one switched to).
    md_mode: documents::MdMode,
    /// The source's share of the Markdown Split view (`[ui] md_split`, 0.2–0.8).
    md_split: f32,
    /// Its divider was dragged since the last mouse-up: the share is saved when it ends.
    md_split_dragging: bool,
    /// The Docs list filter field.
    docs_filter_focus: FocusHandle,
    /// The Git view of every workspace it was shown for.
    git: HashMap<Ulid, GitView>,
    /// The `git.repos` of each workspace whose Git view was shown (ADR 0029), for the "Repo"
    /// chooser; kept in memory only.
    git_repos: HashMap<Ulid, Vec<String>>,
    /// The daemon watchers this window uses for what it shows (`watches`, ADR 0087).
    watch_needs: watches::Needs,
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
    /// The diff list of each workspace's Diff leaf (`ensure_diff_views` creates it).
    diff_views: HashMap<Ulid, DiffViewState>,
    /// A Diff leaf was drawn at the last `sync_panes`: one that comes on screen refreshes Git.
    diff_drawn: bool,
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
    /// The open "Link plan" dialog of a machine window: a typed path on the other computer
    /// (modal).
    plan_dialog: Option<PlanDialog>,
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
    /// The Mermaid diagram of a Markdown preview shown large (modal).
    diagram_dialog: Option<Diagram>,
    /// The open command palette (modal).
    palette: Option<Palette>,
    /// Keeps the palette pick in view.
    palette_scroll: ScrollHandle,
    /// The commands run from the palette lately, newest first (UI memory only; All lists
    /// them while its query is empty).
    palette_recent: Vec<palette::Action>,
    /// What had the keyboard when the palette opened; a picked command acts on it.
    palette_return: Option<(FocusHandle, Option<Ulid>)>,
    /// The theme mode while the palette is the theme picker (ADR 0078); Esc rebuilds the theme
    /// for it from the current settings. `Some` while the palette is the theme picker.
    theme_pick: Option<mkrew_core::settings::ThemeMode>,
    /// The theme shown for a gallery card (mode it had, name), while one is previewed.
    gallery_preview: Option<(mkrew_core::settings::ThemeMode, SharedString)>,
    /// Set by a key press in the theme gallery: the keyboard's card scrolls into view when it
    /// is next painted.
    gallery_reveal: std::rc::Rc<std::cell::Cell<bool>>,
    /// The open prompt picker (modal; over the task dialog when it types into it).
    prompt_picker: Option<PromptPicker>,
    /// Keeps the prompt picker's pick in view.
    prompt_picker_scroll: ScrollHandle,
    /// The open `/` command picker (modal; over the task dialog when it types into it).
    slash_picker: Option<SlashPicker>,
    /// Keeps the `/` picker's pick in view.
    slash_picker_scroll: ScrollHandle,
    /// The last `slash.list` answer per (agent profile, project, task worktree): shown at once
    /// when the picker opens again, then refreshed (§20B.7). Lives for the connection.
    slash_cache: HashMap<SlashKey, SlashListResult>,
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
    /// Focused while a dialog is open; the dialog's keys edit its draft. A dialog's panel tracks
    /// it (`DialogControl::dialog_root`); its buttons take the keyboard with Tab.
    dialog_focus: FocusHandle,
    /// Scrolls the dialog's prompt field to its end (where the caret is) after edits.
    prompt_scroll: ScrollHandle,
    /// Bumped per opened dialog, so a late reply never changes or closes a newer one.
    dialog_serial: u64,
    /// Process start, until the first frame and the first connected frame are logged
    /// (`scripts/measure.*`, SPEC §15 UI startup).
    launched: Option<Instant>,
    first_frame_logged: bool,
    /// The in-app toasts (SPEC §14): pushed by `toast_error` / `toast_info`, expired on the
    /// status tick.
    toasts: Toasts,
    /// Which terminal tab is in which detached window (ADR 0096).
    detached: Detached,
    /// The Space windows (ADR 0115) and their keys, both ways; mode 1 has one.
    spaces: HashMap<gpui::AnyWindowHandle, SpaceKey>,
    space_handles: HashMap<SpaceKey, gpui::AnyWindowHandle>,
    /// The key of the next Space window (never reused).
    next_space: u64,
    /// The detached OS windows and their keys, both ways.
    windows: HashMap<gpui::AnyWindowHandle, WindowKey>,
    window_handles: HashMap<WindowKey, gpui::AnyWindowHandle>,
    /// Per window: screen bounds, maximized, the strip and content size painted last, the
    /// shown workspace, the right panel and the scroll state of what it draws.
    hosts: HashMap<HostId, shell::hosts::HostUi>,
    /// The right panel a new Space window starts with (`[ui] right_panel`).
    panel_default: inspector::RightPanel,
    /// Scroll state for a window whose `HostUi` is not there yet (drawn with fresh state).
    no_view: shell::hosts::HostView,
    /// The title bar summaries of the workspaces some window shows (ADR 0074).
    git_summaries: HashMap<Ulid, GitSummary>,
    /// Workspaces whose `git.repos` + `git.status` is in flight, and those that changed
    /// meanwhile (asked once more when the answer arrives).
    git_summary_busy: HashSet<Ulid>,
    git_summary_again: HashSet<Ulid>,
    /// The window the open popover menu / the palette belongs to (spec §4.3).
    menu_host: HostId,
    palette_host: HostId,
    /// The window the open `/` picker, prompt picker or `@` file picker is drawn in: the one
    /// whose composer opened it.
    picker_host: HostId,
    /// Mkrew windows, most recently activated first (drop target order, toast placement).
    activation: Vec<HostId>,
    /// A tab drag from another window hovers this strip: before chip slot `index` (its window's
    /// accent line; set by `tear_off_catcher` of the source window).
    drop_hint: Option<(HostId, usize)>,
    /// A `DraggedItem` drag moved over a window (the tear-off catcher saw its payload): only
    /// then may a release tear the dragged tab off.
    tab_drag: bool,
    /// `OpenLink` of each attached pane, kept so a move to another window subscribes again.
    link_subscriptions: HashMap<Ulid, gpui::Subscription>,
    /// A detached window moved, resized or changed tabs: saved on the next status tick.
    windows_dirty: bool,
    /// Saved windows waiting for their workspaces' tabs (startup).
    restore: Option<Vec<detached::SavedWindow>>,
    /// The saved windows were read (the first workspace listing of this process).
    restored_once: bool,
    /// Where `windows.json` lives: this computer's file, or the machine window's own.
    windows_path: PathBuf,
    /// The last write of `windows.json` failed (its toast shows once).
    windows_save_failed: bool,
    /// Where the legacy `main-window.json` lives (read once, when `windows.json` has no Space
    /// window yet): this computer's file, or the machine window's own.
    legacy_main_path: PathBuf,
    /// The saved Space windows of mode 2 waiting for the first listing (`restore_spaces`).
    saved_spaces: Vec<detached::SavedSpace>,
}
