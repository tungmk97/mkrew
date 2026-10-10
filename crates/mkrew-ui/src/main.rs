//! `mkrew`: the desktop app (SPEC §10). Connects to the daemon (starting it if needed), lists the
//! workspaces and their sessions in the sidebar and shows the active workspace's terminal tabs,
//! each a split tree of panes. Only the panes on screen are attached.

// No console window next to the app in release builds (debug keeps stderr visible).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use gpui::prelude::*;
use gpui::{
    AnyView, App, Bounds, ClipboardItem, Context, CursorStyle, DispatchPhase, DragMoveEvent,
    EmptyView, Entity, ExternalPaths, FocusHandle, FontWeight, HighlightStyle, Hsla, KeyDownEvent,
    ListHorizontalSizingBehavior, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    PathPromptOptions, Pixels, Point, PromptLevel, ScrollHandle, ScrollStrategy, SharedString,
    StyledText, Subscription, TitlebarOptions, UniformListScrollHandle, Window, WindowBounds,
    WindowControlArea, WindowOptions, anchored, canvas, deferred, div, point, px, relative, rgb,
    size, uniform_list,
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
    ProjectAddParams, ProjectRef, ProjectUpdateParams, RemoveWorktreeResult, Request,
    RunStartParams, RunStarted, RunStopParams, Session, SessionAttachResult, SessionFiles,
    SessionHistory, SessionKillParams, SessionKind, SessionLinkPlanParams, SessionListParams,
    SessionPinParams, SessionPlanEvent, SessionPortsEvent, SessionReadResult, SessionRef,
    SessionSendParams, SessionSendResult, SessionSpawnResult, SessionState, SessionUnqueueParams,
    SessionUsageEvent, StateSource, TabCreateParams, TabListParams, TabPinParams, TabRef,
    TabSetLayoutParams, Task, TaskFanoutParams, TaskListParams, TaskMoveParams, TaskRef,
    TaskStatus, TerminalTab, Turn, UndoPreview, WORKSPACE_COLORS,
};
use mkrew_ui::base_dialog::{self, BaseDialog};
use mkrew_ui::board::{self, Board, Card, CardAction, DraggedCard, View};
use mkrew_ui::broadcast::{self, Broadcast, Delivery as BroadcastDelivery};
use mkrew_ui::client::{Connection, Incoming};
use mkrew_ui::comments::{self, Comments, Selection, Thread};
use mkrew_ui::composer::{self, Composers, Draft};
use mkrew_ui::conflicts::{self, Conflicts};
use mkrew_ui::context_menu::{PaneMenu, TabMenu};
use mkrew_ui::doc_dialog::{self, Changes, DocDialog, SendDialog};
use mkrew_ui::docs::{Doc, Docs, Files, Mode, Shown};
use mkrew_ui::editor::{self, EditKey, Pos};
use mkrew_ui::fanout::{self, Compare, FanOutPicker};
use mkrew_ui::files_view::{self, Content, FilesView, Requests};
use mkrew_ui::follow::{self, Follow};
use mkrew_ui::follow_up::{self, FollowUp};
use mkrew_ui::git_actions::{self, Remote};
use mkrew_ui::git_history::{self, Commits};
use mkrew_ui::git_summary::{GitSummary, RepoSummary};
use mkrew_ui::git_view::{self, GitView, Since};
use mkrew_ui::highlight::{self, Code};
use mkrew_ui::history::{self, HistoryDialog, Listing, Log};
use mkrew_ui::ime;
use mkrew_ui::inbox::{self, Entry, Jump, Viewed};
use mkrew_ui::inspector;
use mkrew_ui::keymap::{
    self as keymap, BOARD_CONTEXT, COMMENTS_CONTEXT, Command, DOCS_CONTEXT, FILES_CONTEXT,
    GIT_CONTEXT, Keymap, KeymapError, Run,
};
use mkrew_ui::lifecycle::{self, CloseAction, CloseChoice, RunWith};
use mkrew_ui::palette::{self, Extra, Palette};
use mkrew_ui::pane_text;
use mkrew_ui::peek::{self, Peek};
use mkrew_ui::plan::{self, Plans};
use mkrew_ui::preview;
use mkrew_ui::prompt_picker::{self, PromptPicker};
use mkrew_ui::quick_open::{self, QuickOpen, Results};
use mkrew_ui::router::{Delivery, Router};
use mkrew_ui::run::{self, Ports, RowState, RunAction, Summary};
use mkrew_ui::run_setup::{self, RunSetup};
use mkrew_ui::sidebar::{self, Change, Row, SessionRow, Sidebar, WorkspaceRow};
use mkrew_ui::split::Axis;
use mkrew_ui::status;
use mkrew_ui::tabs::{Tab, TabKind, Tabs, newest_unplaced, on_screen, rename_params, tab_kind};
use mkrew_ui::task_dialog::{self, DialogKey, Field, TaskDialog};
use mkrew_ui::terminal::find::{FindKey, find_key};
use mkrew_ui::terminal::view::{OpenLink, TerminalPane};
use mkrew_ui::text_box::{self, Input};
use mkrew_ui::text_find::TextFind;
use mkrew_ui::theme::{self, ActiveTheme, Theme, font_size, opacity, radius, space, weight};
use mkrew_ui::turn::Turns;
use mkrew_ui::turns_dialog::{self, TurnsDialog};
use mkrew_ui::undo_dialog::{self, LineKind, Preview, UndoDialog};
use mkrew_ui::usage::{self, ContextShare, Usages};
use mkrew_ui::workspace_menu::{self, Agents, WorkspaceMenu};
use mkrew_ui::worktree_dialog::{self, WorktreeDialog};
use mkrew_ui::worktree_target;
use mkrew_ui::zed;
use tracing_subscriber::EnvFilter;
use ulid::Ulid;

/// Longest wait for the frame showing fed output before draining again anyway (e.g. the
/// window is minimized and never paints).
const PAINT_TIMEOUT: Duration = Duration::from_millis(50);
/// Most incoming frames handled per drain, so one drain never starves painting.
const DRAIN_BATCH: usize = 256;

/// Title of a workspace's first tab; later ones are numbered.
const FIRST_TAB_TITLE: &str = "Terminal";

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
    gpui_platform::application().run(move |cx: &mut App| {
        let bindings = keymap.key_bindings(cx.keyboard_mapper().as_ref());
        cx.bind_keys(bindings);
        // Which char a shifted digit reports depends on the layout: bind again after a switch
        // (e.g. US ↔ Vietnamese), or Ctrl+Shift+1..5 stop working until a restart.
        let rebind = keymap.clone();
        cx.on_keyboard_layout_change(move |cx| {
            let bindings = rebind.key_bindings(cx.keyboard_mapper().as_ref());
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
            cx.new(|cx| Shell::new(launched, paths, keymap, &keymap_errors, window, cx))
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenameTarget {
    Tab(Ulid),
    /// A pane's session (`session.rename`).
    Session(Ulid),
}

/// The inline title field: what it renames and the draft.
#[derive(Debug, Clone)]
struct Renaming {
    target: RenameTarget,
    draft: Input,
}

/// The right-click menu of a Files tree row or of the tree itself (SPEC §12A): Open in File
/// Explorer, Open in Zed.
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

struct Shell {
    status: Status,
    /// Dropping it closes the connection.
    connection: Option<Rc<Connection>>,
    sidebar: Sidebar,
    /// A `project.list` + `session.list` is in flight.
    refreshing: bool,
    /// Another refresh was asked for while one was in flight.
    refresh_again: bool,
    /// Last failure of a sidebar action (adding a workspace), shown under its header.
    sidebar_error: Option<String>,
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
    /// Last failure of a tab action (new tab), shown in the tab bar.
    tab_error: Option<String>,
    /// Which session frames go to a pane, and buffering while attaches are in flight.
    router: Router,
    /// Attached panes of the sessions on screen.
    panes: HashMap<Ulid, Entity<TerminalPane>>,
    /// Failed `session.attach`es of sessions on screen, shown in place of their pane.
    attach_errors: HashMap<Ulid, String>,
    /// Given to panes; signalled after each terminal paint.
    painted: async_channel::Sender<()>,
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
    /// `<config>/mkrew/keymap.toml` (Ctrl+, creates and opens it).
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
    /// The open workspace menu (Open in Zed, default agent).
    workspace_menu: Option<WorkspaceMenu>,
    /// The open right-click menu of the Files tree.
    file_menu: Option<FileMenu>,
    /// The open tab menu (Pin, Split right / down, Rename, Close).
    tab_menu: Option<TabMenu>,
    /// The open pane menu (Pin, Rename, Change color).
    pane_menu: Option<PaneMenu>,
    /// The open "Run setup" of a workspace (modal).
    run_setup: Option<RunSetup>,
    /// The workspace whose Run menu (▾) is open.
    run_menu: Option<Ulid>,
    /// The title bar's workspace dropdown is open.
    workspace_switcher: bool,
    /// The run tab to show once its `tab.updated` arrives (it may trail `project.run_start`).
    run_reveal: Option<Ulid>,
    /// Ports of live run sessions (SPEC §10).
    ports: Ports,
    /// Profile id → `icon` (ADR 0025), reloaded on every refresh.
    agent_icons: HashMap<String, String>,
    /// The open "Broadcast to agents…" dialog (modal).
    broadcast: Option<Broadcast>,
    /// Done sessions already seen, so they leave the Inbox.
    viewed: Viewed,
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
    composer_scroll: ScrollHandle,
    /// The view shown per workspace (Terminals when not set).
    views: HashMap<Ulid, View>,
    /// The session panel right of Terminals is open (closed at every start; SPEC §10).
    inspector_open: bool,
    /// Its width in px, from `[ui] inspector_width` and the drag.
    inspector_width: f32,
    /// The edge was dragged since the last mouse-up: the width is saved when it ends.
    inspector_dragging: bool,
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
    /// The Files viewer's find field (outside `FILES_CONTEXT`'s arrow keys).
    file_find_focus: FocusHandle,
    /// The Files find bar's replace field.
    file_replace_focus: FocusHandle,
    /// Keeps the current find match of the Files viewer in view.
    file_code_scroll: UniformListScrollHandle,
    /// The Files editor (ADR 0028): typing goes there while it has focus.
    file_editor_focus: FocusHandle,
    /// Where the editor's text starts on screen (set while painting), for clicks.
    file_text_left: Rc<Cell<Pixels>>,
    /// The Docs list filter field.
    docs_filter_focus: FocusHandle,
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
    /// Keep the selected changed file and the current hunk in view.
    git_files_scroll: UniformListScrollHandle,
    git_diff_scroll: UniformListScrollHandle,
    /// How the Git diff viewer lays out a file (`[ui] diff_mode`, §12B); global, every repo.
    diff_mode: DiffMode,
    /// The open "Discard changes" confirmation of a Git view file (modal).
    git_discard: Option<git_view::Discard>,
    /// The open base menu of a Git view (modal).
    git_base: Option<BaseDialog>,
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
}

impl Shell {
    fn new(
        launched: Instant,
        paths: Paths,
        keymap: Keymap,
        keymap_errors: &[KeymapError],
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let keymap_file = paths.keymap_file();
        let settings_file = paths.settings_file();
        // A broken settings file keeps the defaults, like the other reads here.
        let ui = mkrew_core::settings::load(&settings_file)
            .unwrap_or_default()
            .ui;
        let diff_mode = ui.diff_mode;
        let inspector_width = inspector::saved_width(ui.inspector_width);
        let prompts_dir = paths.prompts_dir();
        // System mode follows the OS live (ADR 0027).
        cx.observe_window_appearance(window, |_, window, cx| {
            theme::refresh(cx.theme().mode, window, cx);
            cx.notify();
        })
        .detach();
        // Minimized → detach every pane; restored → re-attach from daemon snapshots (§10).
        cx.observe_window_visibility(window, |shell, _, window, cx| {
            shell.sync_panes(window, cx);
            cx.notify();
        })
        .detach();
        // Coming back to the window views the focused pane (a done session leaves the Inbox).
        cx.observe_window_activation(window, |_, _, cx| cx.notify())
            .detach();
        // Unsaved Files changes (§2.5): the close button asks first.
        let this = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            this.update(cx, |shell, cx| shell.window_should_close(window, cx))
                .unwrap_or(true)
        });
        // Spinner and elapsed/idle times move on their own (§11).
        cx.spawn_in(window, async move |this, cx| {
            let mut last = status::now_ms();
            loop {
                cx.background_executor().timer(theme::STATUS_TICK).await;
                let now = status::now_ms();
                let ticked = this.update_in(cx, |shell, window, cx| {
                    // ponytail: mtime poll on the 500 ms tick; a file watcher if it ever shows
                    // in a profile.
                    let modified = std::fs::metadata(&shell.settings_file)
                        .and_then(|m| m.modified())
                        .ok();
                    if modified != shell.settings_modified {
                        shell.settings_modified = modified;
                        let installed = cx.text_system().all_font_names();
                        let last = cx.global::<theme::Fonts>().clone();
                        let mode = cx.theme().mode;
                        match theme::reload(&shell.settings_file, &installed, &last, mode) {
                            Ok(changed) => {
                                shell.settings_error = None;
                                if let Some((fonts, mode)) = changed {
                                    cx.set_global(fonts);
                                    theme::refresh(mode, window, cx);
                                    cx.refresh_windows();
                                }
                                // `theme::reload` only returns fonts and theme; a hand edit of
                                // `diff_mode` applies too (the file just parsed, so this
                                // second read only happens when it changed).
                                if let Ok(settings) =
                                    mkrew_core::settings::load(&shell.settings_file)
                                {
                                    shell.apply_diff_mode(settings.ui.diff_mode);
                                }
                            }
                            // The last good settings stay; the palette lists why (§20.0).
                            Err(error) => shell.settings_error = Some(error),
                        }
                        cx.notify();
                    }
                    let sessions = shell.sidebar.sessions();
                    let board_due = shell.workspace.is_some_and(|project_id| {
                        shell.view() == View::Board
                            && shell.board.redraw_due(project_id, sessions, last, now)
                    });
                    if window.is_visible() && (board_due || status::redraw_due(sessions, last, now))
                    {
                        cx.notify();
                    }
                });
                last = now;
                if ticked.is_err() {
                    return;
                }
            }
        })
        .detach();
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);
        let rename_focus = cx.focus_handle();
        let rename_blur = cx.on_blur(&rename_focus, window, |shell, window, cx| {
            shell.finish_rename(true, window, cx);
        });
        let follow_up_focus = cx.focus_handle();
        let follow_up_blur = cx.on_blur(&follow_up_focus, window, |shell, _, cx| {
            // A typed draft (or one being sent) stays on its card without its caret; an empty
            // one goes.
            shell
                .follow_up
                .take_if(|draft| draft.text.text().is_empty() && !draft.busy);
            cx.notify();
        });
        let (painted, painted_rx) = async_channel::bounded(1);
        cx.spawn_in(window, async move |this, cx| {
            // Blocking (polls a starting daemon for up to 10 s): off the UI thread.
            let connected = cx
                .background_executor()
                .spawn(async move { mkrew_ui::connect::connect(&paths) })
                .await;
            let connection = match connected {
                Ok(connection) => Rc::new(connection),
                Err(error) => {
                    let _ = this.update(cx, |shell, cx| {
                        shell.status = Status::Failed(format!("{error:#}"));
                        cx.notify();
                    });
                    return;
                }
            };
            let incoming = connection.incoming();
            tracing::info!(
                daemon_version = %connection.hello().daemon_version,
                "connected to the daemon"
            );
            let loaded = this.update_in(cx, |shell, window, cx| {
                shell.connection = Some(connection);
                shell.refresh(window, cx);
            });
            if loaded.is_err() {
                return;
            }
            // Drain at once when frames arrive (low key latency); after feeding output, wait
            // until that frame is painted so a flood is fed once per frame.
            while let Ok(first) = incoming.recv().await {
                // A paint from an earlier frame must not end the wait below.
                let _ = painted_rx.try_recv();
                let mut batch = vec![first];
                while batch.len() < DRAIN_BATCH
                    && let Ok(next) = incoming.try_recv()
                {
                    batch.push(next);
                }
                match this.update_in(cx, |shell, window, cx| shell.dispatch(batch, window, cx)) {
                    Ok(true) => {
                        let painted = async {
                            let _ = painted_rx.recv().await;
                        };
                        let timeout = cx.background_executor().timer(PAINT_TIMEOUT);
                        futures_lite::future::or(painted, timeout).await;
                    }
                    Ok(false) => {}
                    Err(_) => return,
                }
            }
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.status = Status::Disconnected;
                shell.connection = None;
                shell.workspace = None;
                shell.tabs.clear();
                shell.loading_tabs.clear();
                shell.early_tab_events.clear();
                shell.reveal = None;
                shell.tab_error = None;
                shell.router.clear();
                shell.panes.clear();
                shell.attach_errors.clear();
                shell.renaming = None;
                shell.close_menus();
                shell.run_setup = None;
                shell.run_reveal = None;
                shell.ports = Ports::default();
                shell.plans = Plans::default();
                shell.plan_error = None;
                shell.broadcast = None;
                shell.task_dialog = None;
                shell.undo_dialog = None;
                shell.worktree_dialog = None;
                shell.compare = None;
                shell.turns_dialog = None;
                shell.turns_return = None;
                shell.git_discard = None;
                shell.git_base = None;
                shell.history = None;
                shell.history_pane = None;
                shell.doc_dialog = None;
                shell.quick_open = None;
                shell.palette = None;
                shell.prompt_picker = None;
                shell.palette_return = None;
                shell.peek = None;
                shell.peek_return = None;
                shell.follow_up = None;
                shell.docs.clear();
                shell.files.clear();
                shell.git.clear();
                shell.git_summary = None;
                window.focus(&shell.focus_handle, cx);
                cx.notify();
            });
        })
        .detach();
        Self {
            status: Status::Connecting,
            connection: None,
            sidebar: Sidebar::default(),
            refreshing: false,
            refresh_again: false,
            sidebar_error: None,
            workspace: None,
            tabs: HashMap::new(),
            loading_tabs: HashSet::new(),
            early_tab_events: Vec::new(),
            reveal: None,
            tab_error: None,
            router: Router::default(),
            panes: HashMap::new(),
            attach_errors: HashMap::new(),
            painted,
            focus_handle,
            renaming: None,
            rename_focus,
            _rename_blur: rename_blur,
            keymap,
            keymap_error: keymap_errors
                .first()
                .map(|first| match keymap_errors.len() {
                    1 => format!("{first}; using the default. Fix keymap.toml and restart."),
                    n => format!(
                        "{first} (+{} more); using the defaults. Fix keymap.toml and restart.",
                        n - 1
                    ),
                }),
            keymap_errors: keymap_errors.to_vec(),
            keymap_file,
            settings_modified: None,
            settings_error: None,
            settings_file,
            prompts_dir,
            workspace_menu: None,
            file_menu: None,
            tab_menu: None,
            pane_menu: None,
            run_setup: None,
            run_menu: None,
            workspace_switcher: false,
            run_reveal: None,
            ports: Ports::default(),
            agent_icons: HashMap::new(),
            broadcast: None,
            viewed: Viewed::default(),
            turns: Turns::default(),
            usages: Usages::default(),
            plans: Plans::default(),
            plan_error: None,
            sidebar_layout: sidebar::Layout::default(),
            inspector_open: false,
            inspector_width,
            inspector_dragging: false,
            board: Board::default(),
            board_error: None,
            conflicts: Conflicts::default(),
            card_error: None,
            selected_card: None,
            starting: HashSet::new(),
            run_with: None,
            fan_out: None,
            follow_up: None,
            follow_up_focus,
            _follow_up_blur: follow_up_blur,
            follow_up_scroll: ScrollHandle::new(),
            composers: Composers::default(),
            drop_hover: None,
            composer_focus: cx.focus_handle(),
            ime_marked: Rc::default(),
            composer_scroll: ScrollHandle::new(),
            views: HashMap::new(),
            docs: HashMap::new(),
            comment_focus: cx.focus_handle(),
            files: HashMap::new(),
            files_focus: cx.focus_handle(),
            files_scroll: UniformListScrollHandle::new(),
            file_find_focus: cx.focus_handle(),
            file_replace_focus: cx.focus_handle(),
            docs_filter_focus: cx.focus_handle(),
            file_code_scroll: UniformListScrollHandle::new(),
            file_editor_focus: cx.focus_handle(),
            file_text_left: Rc::default(),
            git: HashMap::new(),
            git_repos: HashMap::new(),
            git_summary: None,
            git_summary_busy: false,
            git_summary_again: false,
            git_focus: cx.focus_handle(),
            git_commit_focus: cx.focus_handle(),
            git_files_scroll: UniformListScrollHandle::new(),
            git_diff_scroll: UniformListScrollHandle::new(),
            diff_mode,
            git_discard: None,
            git_base: None,
            task_dialog: None,
            undo_dialog: None,
            worktree_dialog: None,
            compare: None,
            turns_dialog: None,
            turns_return: None,
            follow: Follow::default(),
            history: None,
            history_pane: None,
            doc_dialog: None,
            quick_open: None,
            quick_open_scroll: ScrollHandle::new(),
            palette: None,
            palette_scroll: ScrollHandle::new(),
            palette_return: None,
            prompt_picker: None,
            prompt_picker_scroll: ScrollHandle::new(),
            peek: None,
            peek_hover: peek::Hover::default(),
            peek_focus: cx.focus_handle(),
            filter_focus: cx.focus_handle(),
            peek_return: None,
            dialog_focus: cx.focus_handle(),
            prompt_scroll: ScrollHandle::new(),
            dialog_serial: 0,
            launched: Some(launched),
            first_frame_logged: false,
        }
    }

    /// Re-reads workspaces and sessions. The first time, shows the workspace of the most
    /// recently started live session.
    fn refresh(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if self.refreshing {
            self.refresh_again = true;
            return;
        }
        self.refreshing = true;
        self.sidebar.begin_load();
        self.board.begin_load();
        let projects = connection.call::<Vec<Project>>(Request::ProjectList);
        let sessions =
            connection.call::<Vec<Session>>(Request::SessionList(SessionListParams::default()));
        let tasks = connection.call::<Vec<Task>>(Request::TaskList(TaskListParams::default()));
        let profiles = connection.call::<AgentProfilesResult>(Request::AgentProfiles);
        cx.spawn_in(window, async move |this, cx| {
            let listed = match (projects.await, sessions.await) {
                (Ok(projects), Ok(sessions)) => Ok((projects, sessions)),
                (Err(error), _) | (_, Err(error)) => Err(error),
            };
            let tasks = tasks.await;
            let profiles = profiles.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.refreshing = false;
                match profiles {
                    Ok(listed) => {
                        shell.agent_icons = listed
                            .profiles
                            .into_iter()
                            .filter_map(|p| Some((p.id, p.icon?)))
                            .collect();
                    }
                    // Badges fall back to letters; the listing below is unaffected.
                    Err(error) => tracing::warn!("agent.profiles failed: {error:#}"),
                }
                match tasks {
                    Ok(tasks) => {
                        shell.board.load(tasks);
                        shell.drop_gone_targets(window, cx);
                        shell.board_error = None;
                    }
                    Err(error) => {
                        tracing::warn!("task.list failed: {error:#}");
                        shell.board.cancel_load();
                        shell.board_error = Some(format!("{error:#}"));
                    }
                }
                match listed {
                    Ok((projects, sessions)) => {
                        shell.sidebar.load(projects, sessions);
                        shell.load_usage(window, cx);
                        shell.load_ports(window, cx);
                        shell.load_plans(window, cx);
                        shell.status = Status::Connected;
                        let current = shell
                            .workspace
                            .filter(|id| shell.sidebar.projects().iter().any(|p| p.id == *id));
                        match current.or_else(|| shell.initial_workspace()) {
                            Some(project_id) => {
                                // A new session (e.g. spawned by `mk`) fills an empty tab.
                                shell.fill_empty_tab(project_id, window, cx);
                                shell.activate_workspace(project_id, window, cx);
                            }
                            None => {
                                shell.workspace = None;
                                shell.sync_panes(window, cx);
                            }
                        }
                    }
                    Err(error) => {
                        tracing::warn!("listing workspaces failed: {error:#}");
                        shell.sidebar.cancel_load();
                        shell.list_failed(format!("{error:#}"));
                    }
                }
                // Tasks or sessions may have moved on while the lists were read.
                shell
                    .conflicts
                    .prune(&shell.board, shell.sidebar.sessions());
                if shell.peek.is_some() {
                    shell.update_peek(false, window, cx);
                }
                cx.notify();
                if std::mem::take(&mut shell.refresh_again) {
                    shell.refresh(window, cx);
                }
            });
        })
        .detach();
    }

    /// The badge before `agent`'s name (ADR 0025).
    fn agent_badge(&self, agent: &str) -> String {
        sidebar::agent_badge(self.agent_icons.get(agent).map(String::as_str), agent)
    }

    /// Reads the token usage of each listed workspace not read yet (SPEC §20E.3); events keep
    /// it live afterwards.
    fn load_usage(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let projects = self.sidebar.projects().iter().map(|project| project.id);
        for project_id in self.usages.to_request(projects.collect::<Vec<_>>()) {
            let listed = connection
                .call::<Vec<SessionUsageEvent>>(Request::SessionUsage(ProjectRef { project_id }));
            cx.spawn_in(window, async move |this, cx| {
                let listed = listed.await;
                let _ = this.update(cx, |shell, cx| match listed {
                    Ok(listed) => {
                        if shell.usages.load(listed) {
                            cx.notify();
                        }
                    }
                    Err(error) => {
                        tracing::warn!("session.usage failed: {error:#}");
                        // Asked again on the next refresh.
                        shell.usages.request_failed(project_id);
                    }
                });
            })
            .detach();
        }
    }

    /// Reads the ports of each listed workspace not read yet (SPEC §10); events keep them live.
    fn load_ports(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let projects = self.sidebar.projects().iter().map(|project| project.id);
        for project_id in self.ports.to_request(projects.collect::<Vec<_>>()) {
            let listed = connection
                .call::<Vec<SessionPortsEvent>>(Request::SessionPorts(ProjectRef { project_id }));
            cx.spawn_in(window, async move |this, cx| {
                let listed = listed.await;
                let _ = this.update(cx, |shell, cx| match listed {
                    Ok(listed) => {
                        if shell.ports.load(listed) {
                            cx.notify();
                        }
                    }
                    Err(error) => {
                        tracing::warn!("session.ports failed: {error:#}");
                        // Asked again on the next refresh.
                        shell.ports.request_failed(project_id);
                    }
                });
            })
            .detach();
        }
    }

    /// Reads the plans of each listed workspace not read yet (ADR 0039); events keep them live.
    fn load_plans(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let projects = self.sidebar.projects().iter().map(|project| project.id);
        for project_id in self.plans.to_request(projects.collect::<Vec<_>>()) {
            let listed = connection
                .call::<Vec<SessionPlanEvent>>(Request::SessionPlan(ProjectRef { project_id }));
            cx.spawn_in(window, async move |this, cx| {
                let listed = listed.await;
                let _ = this.update(cx, |shell, cx| match listed {
                    Ok(listed) => {
                        if shell.plans.load(listed) {
                            cx.notify();
                        }
                    }
                    Err(error) => {
                        tracing::warn!("session.plan failed: {error:#}");
                        // Asked again on the next refresh.
                        shell.plans.request_failed(project_id);
                    }
                });
            })
            .detach();
        }
    }

    /// The Run state of `project_id` (empty when the workspace is unknown).
    fn run_summary(&self, project_id: Ulid) -> Summary {
        self.sidebar
            .projects()
            .iter()
            .find(|project| project.id == project_id)
            .map(|project| run::summary(project, self.sidebar.sessions(), &self.ports))
            .unwrap_or_default()
    }

    /// The button's main part: Run setup when nothing is configured, the run tab while
    /// something runs, else run everything.
    fn run_main_click(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let summary = self.run_summary(project_id);
        if summary.rows.is_empty() {
            self.open_run_setup(project_id, true, window, cx);
        } else if summary.any_live() {
            self.show_run_tab(project_id, window, cx);
        } else {
            self.run_action(project_id, RunAction::Start, None, window, cx);
        }
    }

    /// `project.run_start` / `run_restart` / `run_stop` (`name` = one command, `None` = all).
    /// Running from all-stopped shows the run tab; restarts keep the view (SPEC §10). A
    /// failure shows in the sidebar notice.
    fn run_action(
        &mut self,
        project_id: Ulid,
        action: RunAction,
        name: Option<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.run_menu = None;
        cx.notify();
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if action == RunAction::Stop {
            let stopped =
                connection.call::<()>(Request::ProjectRunStop(RunStopParams { project_id, name }));
            cx.spawn_in(window, async move |this, cx| {
                let stopped = stopped.await;
                let _ = this.update(cx, |shell, cx| {
                    shell.sidebar_error = stopped.err().map(|error| {
                        tracing::warn!(%project_id, "project.run_stop failed: {error:#}");
                        format!("Cannot stop: {error:#}")
                    });
                    cx.notify();
                });
            })
            .detach();
            return;
        }
        let reveal = action == RunAction::Start
            && name.is_none()
            && !self.run_summary(project_id).any_live();
        let params = RunStartParams {
            project_id,
            name,
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
        };
        let request = match action {
            RunAction::Restart => Request::ProjectRunRestart(params),
            RunAction::Start | RunAction::Stop => Request::ProjectRunStart(params),
        };
        let started = connection.call::<RunStarted>(request);
        cx.spawn_in(window, async move |this, cx| {
            let started = started.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                match started {
                    Ok(run) => {
                        shell.sidebar_error = None;
                        if reveal {
                            shell.views.insert(project_id, View::Terminals);
                            shell.run_reveal = run.tab_id;
                            shell.reveal_run_tab(project_id, window, cx);
                            shell.activate_workspace(project_id, window, cx);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "run failed: {error:#}");
                        shell.sidebar_error = Some(format!("Cannot run: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Switches to the workspace's run tab.
    fn show_run_tab(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.run_menu = None;
        self.run_reveal = self.tabs.get(&project_id).and_then(|tabs| {
            tabs.tabs()
                .iter()
                .find(|tab| tab.title == mkrew_core::run::TAB_TITLE)
                .map(|tab| tab.id)
        });
        self.views.insert(project_id, View::Terminals);
        self.reveal_run_tab(project_id, window, cx);
        self.activate_workspace(project_id, window, cx);
    }

    /// Activates the tab in `run_reveal` once `project_id`'s tabs hold it.
    fn reveal_run_tab(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(tab_id) = self.run_reveal else {
            return;
        };
        let Some(tabs) = self.tabs.get_mut(&project_id) else {
            return;
        };
        if tabs.activate(tab_id) {
            self.run_reveal = None;
            self.sync_panes(window, cx);
            cx.notify();
        }
    }

    fn open_run_setup(
        &mut self,
        project_id: Ulid,
        run_after: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        self.run_menu = None;
        let Some(project) = self.sidebar.projects().iter().find(|p| p.id == project_id) else {
            return;
        };
        self.run_setup = Some(RunSetup::new(project, run_after));
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    fn close_run_setup(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.run_setup = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    fn run_setup_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = task_dialog::dialog_key(&event.keystroke) else {
            return;
        };
        let paste = (key == DialogKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(setup) = self.run_setup.as_mut() else {
            return;
        };
        if let Some(text) = paste {
            setup.insert(&text);
        }
        match setup.key(key) {
            run_setup::Outcome::Cancel => return self.close_run_setup(window, cx),
            run_setup::Outcome::Save => return self.save_run_setup(window, cx),
            run_setup::Outcome::Edited => {}
        }
        cx.notify();
    }

    /// `project.update` with the dialog's commands; on success it closes and, when opened from
    /// ▶ Run, runs everything.
    fn save_run_setup(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let serial = self.dialog_serial;
        let Some(setup) = self.run_setup.as_mut() else {
            return;
        };
        let run_after = setup.run_after;
        let Some(params) = setup.begin() else {
            cx.notify();
            return;
        };
        let project_id = params.project_id;
        let updated = connection.call::<Project>(Request::ProjectUpdate(params));
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let updated = updated.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                if shell.dialog_serial != serial || shell.run_setup.is_none() {
                    return;
                }
                match updated {
                    Ok(project) => {
                        // Seen before `project.updated` arrives, so the run below finds the commands.
                        shell.sidebar.apply(&Event::ProjectUpdated(project.clone()));
                        shell.close_run_setup(window, cx);
                        if run_after && !project.run_commands.is_empty() {
                            shell.run_action(project_id, RunAction::Start, None, window, cx);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "project.update failed: {error:#}");
                        if let Some(setup) = shell.run_setup.as_mut() {
                            setup.failed(format!("{error:#}"));
                        }
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    /// The listed workspace of the most recently started live session, else the first one.
    /// Sessions of an archived workspace may still be live (stopping), so they do not count.
    fn initial_workspace(&self) -> Option<Ulid> {
        let projects = self.sidebar.projects();
        self.sidebar
            .sessions()
            .iter()
            .filter(|session| session.state != SessionState::Exited)
            .filter(|session| projects.iter().any(|p| p.id == session.project_id))
            .max_by_key(|session| session.started_at)
            .map(|session| session.project_id)
            .or_else(|| projects.first().map(|project| project.id))
    }

    /// Shows the tabs of `project_id`, loading them the first time.
    fn activate_workspace(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let switched = self.workspace != Some(project_id);
        self.workspace = Some(project_id);
        self.workspace_switcher = false;
        if switched {
            self.close_menus();
            self.end_inspector_drag();
            // Nothing is kept for the workspace that is no longer shown.
            self.git_summary = None;
        }
        if switched || self.git_summary.is_none() {
            self.load_git_summary(project_id, window, cx);
        }
        if !self.tabs.contains_key(&project_id) {
            self.load_tabs(project_id, window, cx);
        }
        if switched && self.view() == View::Docs {
            self.load_docs(project_id, window, cx);
        }
        if switched && self.view() == View::Files {
            self.refresh_files(project_id, window, cx);
        }
        if switched && self.view() == View::Git {
            self.refresh_git(project_id, window, cx);
        }
        self.sync_panes(window, cx);
        cx.notify();
    }

    /// `tab.list` for a workspace; a workspace without tabs gets one.
    fn load_tabs(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if !self.loading_tabs.insert(project_id) {
            return;
        }
        let listed =
            connection.call::<Vec<TerminalTab>>(Request::TabList(TabListParams { project_id }));
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await;
            let _ = this.update_in(cx, |shell, window, cx| match listed {
                Ok(list) if list.is_empty() => shell.create_tab(project_id, None, window, cx),
                Ok(list) => {
                    let mut tabs = Tabs::new(project_id);
                    tabs.load(list);
                    shell.tabs.insert(project_id, tabs);
                    shell.tabs_loaded(project_id, window, cx);
                }
                Err(error) => {
                    tracing::warn!(%project_id, "tab.list failed: {error:#}");
                    shell.tabs_load_failed(project_id);
                    if shell.project_of_reveal() == Some(project_id) {
                        shell.reveal = None;
                    }
                    shell.list_failed(format!("{error:#}"));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Adds a tab to a workspace and activates it; `start` then spawns a session into it.
    fn create_tab(
        &mut self,
        project_id: Ulid,
        start: Option<SessionKind>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(kind) = start
            && self.ask_for_default(project_id, kind, window, cx)
        {
            return;
        }
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.tab_error = None;
        let count = self
            .tabs
            .get(&project_id)
            .map_or(0, |tabs| tabs.tabs().len());
        let title = match count {
            0 => FIRST_TAB_TITLE.to_owned(),
            count => format!("{FIRST_TAB_TITLE} {}", count + 1),
        };
        let created = connection.call::<TerminalTab>(Request::TabCreate(TabCreateParams {
            project_id,
            title,
            position: None,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let created = created.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let first = shell.loading_tabs.contains(&project_id);
                match created {
                    Ok(tab) => {
                        let tab_id = tab.id;
                        let tabs = shell
                            .tabs
                            .entry(project_id)
                            .or_insert_with(|| Tabs::new(project_id));
                        // Its `tab.updated` echo is then a no-op.
                        tabs.apply(&Event::TabUpdated(tab));
                        tabs.activate(tab_id);
                        if first {
                            shell.tabs_loaded(project_id, window, cx);
                        } else {
                            shell.sync_panes(window, cx);
                        }
                        if let Some(kind) = start
                            && shell.workspace == Some(project_id)
                        {
                            shell.new_pane(kind, Axis::Vertical, window, cx);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "tab.create failed: {error:#}");
                        let error = format!("{error:#}");
                        if first {
                            shell.tabs_load_failed(project_id);
                            shell.list_failed(error);
                        } else {
                            shell.tab_error = Some(format!("Cannot add a tab: {error}"));
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The tabs of a workspace arrived: show the session waiting for them, else fill an empty
    /// first tab, and attach what is on screen.
    fn tabs_loaded(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.loading_tabs.remove(&project_id);
        let (early, rest) = std::mem::take(&mut self.early_tab_events)
            .into_iter()
            .partition(
                |event| matches!(event, Event::TabUpdated(tab) if tab.project_id == project_id),
            );
        self.early_tab_events = rest;
        if let Some(tabs) = self.tabs.get_mut(&project_id) {
            for event in early {
                tabs.apply(&event);
            }
        }
        if let Some(session) = self.reveal
            && self.project_of(session) == Some(project_id)
        {
            self.reveal = None;
            self.place_session(project_id, session, Axis::Vertical, window, cx);
        }
        self.fill_empty_tab(project_id, window, cx);
        self.sync_panes(window, cx);
        cx.notify();
    }

    /// `project_id`'s tabs failed to load: it stops loading and its buffered `tab.updated`
    /// events go (other workspaces still loading keep theirs).
    fn tabs_load_failed(&mut self, project_id: Ulid) {
        self.loading_tabs.remove(&project_id);
        self.early_tab_events.retain(
            |event| !matches!(event, Event::TabUpdated(tab) if tab.project_id == project_id),
        );
    }

    fn project_of(&self, session: Ulid) -> Option<Ulid> {
        self.sidebar
            .sessions()
            .iter()
            .find(|s| s.id == session)
            .map(|s| s.project_id)
    }

    fn project_of_reveal(&self) -> Option<Ulid> {
        self.reveal.and_then(|session| self.project_of(session))
    }

    /// A list failed; ignored once disconnected (that status says more).
    fn list_failed(&mut self, error: String) {
        if self.connection.is_some() {
            self.status = Status::ListFailed(error);
        }
    }

    fn is_exited(&self, session: Ulid) -> bool {
        self.sidebar
            .sessions()
            .iter()
            .any(|s| s.id == session && s.state == SessionState::Exited)
    }

    /// Shows a session picked in the sidebar: focuses its pane, or places it next to the
    /// focused pane of its workspace's active tab.
    fn open_session(&mut self, session: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.project_of(session) else {
            return;
        };
        self.views.insert(project_id, View::Terminals);
        if self.tabs.contains_key(&project_id) {
            self.place_session(project_id, session, Axis::Vertical, window, cx);
        } else {
            self.reveal = Some(session);
        }
        self.activate_workspace(project_id, window, cx);
    }

    /// An OS notification was clicked (§7): raises the window (restoring it if minimized) and
    /// shows the session's pane, or at least its workspace if the session is not listed yet.
    fn notification_clicked(
        &mut self,
        session: Ulid,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.activate_window();
        let (projects, sessions) = (self.sidebar.projects(), self.sidebar.sessions());
        match inbox::jump(projects, sessions, session, project_id) {
            Some(Jump::Session(session)) => self.open_session(session, window, cx),
            Some(Jump::Workspace(project_id)) => self.activate_workspace(project_id, window, cx),
            None => {}
        }
    }

    /// Focuses the pane of `session` (activating its tab) or splits the focused pane along
    /// `axis` to show it.
    fn place_session(
        &mut self,
        project_id: Ulid,
        session: Ulid,
        axis: Axis,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(tabs) = self.tabs.get_mut(&project_id) else {
            return;
        };
        if tabs.focus(session) {
            return;
        }
        if let Some(write) = tabs.split_focused(axis, session) {
            self.save_layout(project_id, write, window, cx);
        }
    }

    /// An active tab without panes shows the workspace's newest live session not in any tab.
    fn fill_empty_tab(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let newest = self
            .tabs
            .get(&project_id)
            .and_then(|tabs| newest_unplaced(tabs, self.sidebar.sessions()));
        if let Some(session) = newest {
            self.place_session(project_id, session, Axis::Vertical, window, cx);
        }
    }

    /// Persists a layout change (`tab.set_layout`).
    fn save_layout(
        &mut self,
        project_id: Ulid,
        write: TabSetLayoutParams,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let saved = connection.call::<()>(Request::TabSetLayout(write.clone()));
        cx.spawn_in(window, async move |this, cx| {
            if let Err(error) = saved.await {
                tracing::warn!(tab_id = %write.tab_id, "tab.set_layout failed: {error:#}");
                let _ = this.update(cx, |shell, cx| {
                    if let Some(tabs) = shell.tabs.get_mut(&project_id) {
                        tabs.write_failed(&write);
                    }
                    shell.tab_error = Some(format!("Cannot save the layout: {error:#}"));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn active_tabs_mut(&mut self) -> Option<&mut Tabs> {
        self.tabs.get_mut(&self.workspace?)
    }

    /// The view of the active workspace.
    fn view(&self) -> View {
        self.workspace
            .and_then(|id| self.views.get(&id).copied())
            .unwrap_or_default()
    }

    /// Switches the active workspace to `view`; leaving Terminals detaches its panes. Showing
    /// Docs re-lists its files (agents may have written new ones); showing Files refreshes the
    /// tree and gives it the keyboard, likewise for Git and its changes.
    fn show_view(&mut self, view: View, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.workspace else {
            return;
        };
        self.close_menus();
        let shown = self.views.insert(project_id, view).unwrap_or_default();
        if view != View::Terminals {
            // The panel (and its grip) leaves with Terminals: end a drag in progress.
            self.end_inspector_drag();
        }
        if view == View::Docs && shown != View::Docs {
            self.load_docs(project_id, window, cx);
        }
        if view == View::Files {
            if shown != View::Files {
                self.refresh_files(project_id, window, cx);
            }
            window.focus(&self.files_focus, cx);
        }
        if view == View::Git {
            if shown != View::Git {
                self.refresh_git(project_id, window, cx);
            }
            window.focus(&self.git_focus, cx);
        }
        if shown != view {
            self.sync_panes(window, cx);
            cx.notify();
        }
    }

    /// `doc.list` for the Docs view of `project_id` (also keeps the daemon watching its docs);
    /// the first listing shows its first file.
    fn load_docs(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let docs = self
            .docs
            .entry(project_id)
            .or_insert_with(|| Docs::new(project_id));
        if !docs.begin_list() {
            return;
        }
        let listed =
            connection.call::<Vec<DocFile>>(Request::DocList(DocListParams { project_id }));
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await.map_err(|error| format!("{error:#}"));
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &listed {
                    tracing::warn!(%project_id, "doc.list failed: {error}");
                }
                let Some(docs) = shell.docs.get_mut(&project_id) else {
                    return;
                };
                let (read, again) = docs.listed(listed);
                if let Some(file_path) = read {
                    shell.read_doc(project_id, file_path, window, cx);
                }
                if again {
                    shell.load_docs(project_id, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Shows `file_path` in the Docs view of `project_id`, reading it if needed.
    fn select_doc(
        &mut self,
        project_id: Ulid,
        file_path: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let read = self
            .docs
            .get_mut(&project_id)
            .and_then(|docs| docs.select(file_path));
        if let Some(file_path) = read {
            self.read_doc(project_id, file_path, window, cx);
        }
        cx.notify();
    }

    /// The Docs filter field: typing filters, ↑ / ↓ show the previous / next shown file,
    /// Esc clears the filter (then leaves the field), Enter leaves it.
    fn docs_filter_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let (Some(project_id), Some(key)) = (self.workspace, quick_open::key(&event.keystroke))
        else {
            return;
        };
        let paste = (key == quick_open::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(docs) = self.docs.get_mut(&project_id) else {
            return;
        };
        match key {
            quick_open::Key::Close if !docs.filter.is_empty() => docs.filter.clear(),
            quick_open::Key::Close | quick_open::Key::Open => {
                window.focus(&self.focus_handle, cx);
            }
            quick_open::Key::Move(by) => {
                if let Some(path) = docs.step_selection(by as isize) {
                    self.select_doc(project_id, &path, window, cx);
                }
            }
            quick_open::Key::Backspace => {
                docs.filter.backspace();
            }
            quick_open::Key::DeleteWord => {
                docs.filter.delete_word();
            }
            quick_open::Key::SelectAll => docs.filter.select_all(),
            quick_open::Key::Paste => {
                docs.filter
                    .insert(&paste.unwrap_or_default().replace(['\r', '\n'], " "));
            }
            quick_open::Key::Insert(text) => {
                docs.filter.insert(&text);
            }
        }
        cx.notify();
    }

    /// `doc.read` (and `comment.list` for a newly shown file); the answer is dropped if
    /// another file got selected meanwhile.
    fn read_doc(
        &mut self,
        project_id: Ulid,
        file_path: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let list = self
            .docs
            .get_mut(&project_id)
            .and_then(|docs| docs.comments.begin_list(&file_path));
        if let Some(file_path) = list {
            self.load_comments(project_id, CommentContext::Doc, file_path, window, cx);
        }
        let read = connection.call::<DocContent>(Request::DocRead(DocReadParams {
            project_id,
            file_path: file_path.clone(),
        }));
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let read = match read.await {
                Ok(content) => Ok(background.spawn(async move { Shown::new(&content) }).await),
                Err(error) => Err(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                if let Err(error) = &read {
                    tracing::warn!(%project_id, file_path, "doc.read failed: {error}");
                }
                if let Some(docs) = shell.docs.get_mut(&project_id) {
                    docs.read(&file_path, read);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Follows `doc.changed` / `comment.updated`: re-reads a shown file that changed and
    /// re-lists the Docs view on screen (hidden ones re-list when shown again).
    fn doc_event(&mut self, event: &Event, window: &mut Window, cx: &mut Context<'_, Self>) {
        let shown = self.workspace.filter(|_| self.view() == View::Docs);
        let reactions: Vec<_> = self
            .docs
            .values_mut()
            .map(|docs| (docs.project_id, docs.apply(event)))
            .collect();
        for (project_id, reaction) in reactions {
            if let Some(file_path) = reaction.reread {
                self.read_doc(project_id, file_path, window, cx);
                cx.notify();
            }
            if reaction.relist && shown == Some(project_id) {
                self.load_docs(project_id, window, cx);
            }
        }
    }

    /// Opens the shown doc of `project_id` in Zed; a failure shows in the Docs header.
    fn open_doc_in_zed(&mut self, project_id: Ulid, cx: &mut Context<'_, Self>) {
        let Some(project) = self.sidebar.projects().iter().find(|p| p.id == project_id) else {
            return;
        };
        let Some(docs) = self.docs.get_mut(&project_id) else {
            return;
        };
        let Some(file_path) = docs.selected.as_deref() else {
            return;
        };
        // Native separators (`file_path` uses `/`), as Zed gets the path verbatim.
        let path = file_path
            .split('/')
            .fold(PathBuf::from(&project.path), |path, part| path.join(part));
        docs.zed_error = match zed::open(&path, None) {
            Ok(()) => None,
            Err(error) => {
                tracing::warn!(%project_id, file_path, "open in Zed failed: {error}");
                Some(error.to_string())
            }
        };
        cx.notify();
    }

    /// The Files view of `project_id`, created on first use.
    fn files_view(&mut self, project_id: Ulid) -> &mut FilesView {
        self.files
            .entry(project_id)
            .or_insert_with(|| FilesView::new(project_id, None))
    }

    /// The Files view of `project_id` while it still shows `target` (an answer sent for
    /// another target is dropped).
    fn files_at(&mut self, project_id: Ulid, target: Option<Ulid>) -> Option<&mut FilesView> {
        self.files
            .get_mut(&project_id)
            .filter(|view| view.worktree_task_id == target)
    }

    /// The chooser picked `target` (SPEC §20E.1): the Files view starts over on it.
    fn set_files_target(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.files_view(project_id).worktree_task_id == target {
            return;
        }
        if self
            .files
            .get(&project_id)
            .and_then(FilesView::unsaved)
            .is_some()
        {
            return self.leave_file(
                project_id,
                Box::new(move |shell, window, cx| {
                    shell.set_files_target(project_id, target, window, cx);
                }),
                window,
                cx,
            );
        }
        self.files
            .insert(project_id, FilesView::new(project_id, target));
        self.files_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.refresh_files(project_id, window, cx);
    }

    /// Lists the root and open folders of the Files view of `project_id` again and re-reads
    /// its opened file.
    fn refresh_files(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let requests = self.files_view(project_id).refresh();
        self.files_requests(project_id, requests, window, cx);
    }

    fn files_requests(
        &mut self,
        project_id: Ulid,
        requests: Requests,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        for dir in requests.list {
            self.list_dir(project_id, dir, window, cx);
        }
        if let Some(path) = requests.read {
            self.read_file(project_id, path, window, cx);
        }
        cx.notify();
    }

    /// `fs.list` of one folder of the Files view of `project_id` (also keeps the daemon
    /// watching the workspace, so `git.changed` follows file changes).
    fn list_dir(
        &mut self,
        project_id: Ulid,
        dir: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let view = self.files_view(project_id);
        if !view.begin_list(&dir) {
            return;
        }
        let target = view.worktree_task_id;
        let listed = connection.call::<Vec<FsEntry>>(Request::FsList(FsListParams {
            project_id,
            worktree_task_id: target,
            dir: dir.clone(),
            show_ignored: view.show_ignored,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await.map_err(|error| format!("{error:#}"));
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &listed {
                    tracing::warn!(%project_id, dir, "fs.list failed: {error}");
                }
                let Some(view) = shell.files_at(project_id, target) else {
                    return;
                };
                for dir in view.listed(&dir, listed) {
                    shell.list_dir(project_id, dir, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// `fs.read` of the opened file, highlighted on the background executor; while one is in
    /// flight, the file is read once more after it answers.
    fn read_file(
        &mut self,
        project_id: Ulid,
        path: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let view = self.files_view(project_id);
        if !view.begin_read() {
            return;
        }
        let target = view.worktree_task_id;
        let read = connection.call::<FsContent>(Request::FsRead(FsReadParams {
            project_id,
            worktree_task_id: target,
            path: path.clone(),
        }));
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let read = match read.await {
                Ok(content) => Ok(background
                    .spawn(async move {
                        let code = Code::new(&content);
                        (content, code)
                    })
                    .await),
                Err(error) => Err(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &read {
                    tracing::warn!(%project_id, path, "fs.read failed: {error}");
                }
                let again = shell
                    .files_at(project_id, target)
                    .and_then(|view| view.read(&path, read));
                if let Some(path) = again {
                    shell.read_file(project_id, path, window, cx);
                }
                shell.show_pending_line(project_id, target);
                shell.build_preview(project_id, window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Parses the `.md` file of the Files view of `project_id` for its preview when it is
    /// shown as one and its text moved on, off the UI thread (highlighting and Mermaid are
    /// slow). Asks again when the text changed while it ran.
    fn build_preview(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(view) = self.files.get_mut(&project_id) else {
            return;
        };
        let target = view.worktree_task_id;
        let Some((stamp, text)) = view.preview_request() else {
            return;
        };
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let page = background
                .spawn(async move { Shown::from_text(&text) })
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Some(view) = shell.files_at(project_id, target) {
                    view.preview_ready(stamp, page);
                }
                shell.build_preview(project_id, window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Follows `git.changed` in the Files view on screen (hidden ones refresh when shown).
    fn files_event(&mut self, event: &Event, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.workspace.filter(|_| self.view() == View::Files) else {
            return;
        };
        let Some(view) = self.files.get_mut(&project_id) else {
            return;
        };
        let requests = view.apply(event);
        self.files_requests(project_id, requests, window, cx);
    }

    /// A click on a tree row: opens or closes a folder, opens a file.
    fn click_file(
        &mut self,
        project_id: Ulid,
        path: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.focus(&self.files_focus, cx);
        if self.files_view(project_id).leaves_unsaved(path) {
            let path = path.to_owned();
            return self.leave_file(
                project_id,
                Box::new(move |shell, window, cx| shell.click_file(project_id, &path, window, cx)),
                window,
                cx,
            );
        }
        let requests = self.files_view(project_id).click(path);
        self.files_requests(project_id, requests, window, cx);
    }

    /// Arrow keys move the tree cursor (kept in view), ← / → close / open folders, Enter opens.
    fn files_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let (Some(key), Some(project_id)) = (files_view::key(&event.keystroke), self.workspace)
        else {
            return;
        };
        cx.stop_propagation();
        if key == files_view::Key::Enter
            && let Some(cursor) = self.files_view(project_id).enter_target()
        {
            return self.click_file(project_id, &cursor, window, cx);
        }
        let view = self.files_view(project_id);
        let requests = view.key(key);
        let cursor = view.cursor.clone();
        let at = view.rows().iter().position(|row| {
            matches!(row, files_view::Row::Entry { entry, .. } if Some(&entry.path) == cursor.as_ref())
        });
        if let Some(at) = at {
            self.files_scroll
                .scroll_to_item(at, ScrollStrategy::Nearest);
        }
        self.files_requests(project_id, requests, window, cx);
    }

    /// Ctrl+F / Ctrl+Shift+F / "Find" (or Ctrl+H for `replace`) in the Files view: opens the
    /// find bar over the opened file and gives its field the keyboard.
    fn open_file_find(&mut self, replace: bool, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.workspace else {
            return;
        };
        if !self.files_view(project_id).open_find(replace) {
            return;
        }
        let field = if replace {
            &self.file_replace_focus
        } else {
            &self.file_find_focus
        };
        window.focus(field, cx);
        self.reveal_file_match(project_id, cx);
    }

    /// Typing edits the query; Enter / Shift+Enter go to the next / previous match; Tab goes
    /// to the replace field when it is shown; Ctrl+Alt+Enter replaces all; Esc closes the bar.
    fn file_find_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(project_id) = self.workspace else {
            return;
        };
        let keystroke = &event.keystroke;
        let m = keystroke.modifiers;
        match keystroke.key.as_str() {
            "enter" if m.control && m.alt => {
                return self.replace_all_in_file(project_id, window, cx);
            }
            "tab" if !m.control && !m.alt => {
                // Tab / Shift+Tab: to the replace field when it is shown.
                let replacing = (self.files_view(project_id).find.as_ref())
                    .is_some_and(|find| find.replace.is_some());
                if replacing {
                    window.focus(&self.file_replace_focus, cx);
                    cx.notify();
                }
                return;
            }
            _ => {}
        }
        if let Some(edit) = field_edit(keystroke, cx) {
            self.edit_file_find(project_id, |query| edit.apply(query), cx);
            return;
        }
        match find_key(keystroke) {
            Some(FindKey::Close) => self.close_file_find(project_id, window, cx),
            // The terminal's names: Enter = "older" = the next match here.
            Some(FindKey::Older) => self.step_file_find(project_id, true, cx),
            Some(FindKey::Newer) => self.step_file_find(project_id, false, cx),
            _ => {}
        }
    }

    fn edit_file_find(
        &mut self,
        project_id: Ulid,
        edit: impl FnOnce(&mut Input),
        cx: &mut Context<'_, Self>,
    ) {
        let view = self.files_view(project_id);
        let Some(find) = &mut view.find else {
            return;
        };
        edit(&mut find.query);
        view.search();
        self.reveal_file_match(project_id, cx);
    }

    fn step_file_find(&mut self, project_id: Ulid, forward: bool, cx: &mut Context<'_, Self>) {
        if let Some(find) = &mut self.files_view(project_id).find {
            find.step(forward);
        }
        self.reveal_file_match(project_id, cx);
    }

    /// Scrolls to the current match and selects it in the editor.
    fn reveal_file_match(&mut self, project_id: Ulid, cx: &mut Context<'_, Self>) {
        let view = self.files_view(project_id);
        let found = view.find.as_ref().and_then(TextFind::current).cloned();
        if let (Some(found), Some(editor)) = (found, view.editor_mut()) {
            let (start, end) = (found.range.start, found.range.end);
            editor.select(Pos::new(found.line, start)..Pos::new(found.line, end));
            self.file_code_scroll
                .scroll_to_item(found.line, ScrollStrategy::Center);
        }
        cx.notify();
    }

    fn close_file_find(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.files_view(project_id).find = None;
        window.focus(&self.file_editor_focus, cx);
        cx.notify();
    }

    /// Keys of the replace field:
    /// - typing edits the replacement;
    /// - Enter replaces the current match and goes to the next;
    /// - Ctrl+Alt+Enter replaces all;
    /// - Tab goes back to the find field;
    /// - Esc closes the bar.
    fn file_replace_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(project_id) = self.workspace else {
            return;
        };
        let keystroke = &event.keystroke;
        let m = keystroke.modifiers;
        match keystroke.key.as_str() {
            "enter" if m.control && m.alt => {
                return self.replace_all_in_file(project_id, window, cx);
            }
            "enter" if !m.shift && !m.control => {
                return self.replace_in_file(project_id, window, cx);
            }
            "tab" if !m.control && !m.alt => {
                window.focus(&self.file_find_focus, cx);
                cx.notify();
                return;
            }
            _ => {}
        }
        if let Some(edit) = field_edit(keystroke, cx) {
            self.edit_file_replace(project_id, |with| edit.apply(with), cx);
        } else if find_key(keystroke) == Some(FindKey::Close) {
            self.close_file_find(project_id, window, cx);
        }
    }

    fn edit_file_replace(
        &mut self,
        project_id: Ulid,
        edit: impl FnOnce(&mut Input),
        cx: &mut Context<'_, Self>,
    ) {
        let find = self.files_view(project_id).find.as_mut();
        if let Some(with) = find.and_then(|find| find.replace.as_mut()) {
            edit(with);
            cx.notify();
        }
    }

    fn replace_in_file(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.files_view(project_id).replace_one() {
            self.file_edited(project_id, window, cx);
            self.reveal_file_match(project_id, cx);
        }
    }

    fn replace_all_in_file(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.files_view(project_id).replace_all() > 0 {
            self.file_edited(project_id, window, cx);
        }
        cx.notify();
    }

    /// Ctrl+S / "Save" (§12A): `fs.write` of the opened file over the version it was read as.
    /// `then` runs after a successful save (dropped if a save is already in flight: the user
    /// asks again).
    fn save_file(
        &mut self,
        project_id: Ulid,
        then: Option<AfterSave>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(view) = self.files.get_mut(&project_id) else {
            return;
        };
        let target = view.worktree_task_id;
        let Some(save) = view.begin_save() else {
            return;
        };
        let written = connection.call::<FsWritten>(Request::FsWrite(FsWriteParams {
            project_id,
            worktree_task_id: target,
            path: save.path.clone(),
            content: save.text,
            expected_hash: save.expected_hash,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let written = written.await.map_err(|error| format!("{error:#}"));
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &written {
                    tracing::warn!(%project_id, path = %save.path, "fs.write failed: {error}");
                }
                let done = matches!(&written, Ok(written) if written.saved);
                if let Some(view) = shell.files_at(project_id, target) {
                    view.saved(&save.path, save.point, written);
                }
                if let (true, Some(then)) = (done, then) {
                    then(shell, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Runs `then` once the unsaved changes of `project_id`'s opened file are dealt with:
    /// - at once if there are none;
    /// - after "Save" and a successful save;
    /// - after "Don't save".
    ///
    /// "Cancel" keeps everything (§2.5).
    fn leave_file(
        &mut self,
        project_id: Ulid,
        then: AfterSave,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(path) = (self.files.get(&project_id))
            .and_then(FilesView::unsaved)
            .map(str::to_owned)
        else {
            return then(self, window, cx);
        };
        let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
        let workspace = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .map_or_else(String::new, |project| format!(" in {}", project.name));
        let detail = format!("{path}{workspace} — your changes are lost if you don't save them.");
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Save changes to {name}?"),
            Some(detail.as_str()),
            &files_view::LEAVE_ANSWERS,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let choice = files_view::leave_choice(answer.await.ok());
            let _ = this.update_in(cx, |shell, window, cx| match choice {
                Some(files_view::Leave::Save) => {
                    shell.save_file(project_id, Some(then), window, cx)
                }
                Some(files_view::Leave::DontSave) => {
                    shell.files_view(project_id).discard();
                    then(shell, window, cx);
                }
                None => {}
            });
        })
        .detach();
    }

    /// The window's close button (§2.5): unsaved Files changes ask first, one file at a time,
    /// and the window closes once none are left. The workspace that holds them is shown first,
    /// so a refused save is visible.
    fn window_should_close(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> bool {
        let unsaved = (self.files.values())
            .find(|view| view.unsaved().is_some())
            .map(|view| view.project_id);
        let Some(project_id) = unsaved else {
            return true;
        };
        if self.workspace != Some(project_id) {
            self.activate_workspace(project_id, window, cx);
        }
        self.show_view(View::Files, window, cx);
        let close: AfterSave = Box::new(|shell, window, cx| {
            if shell.window_should_close(window, cx) {
                window.remove_window();
            }
        });
        self.leave_file(project_id, close, window, cx);
        false
    }

    /// Keys of the Files editor (§12A, VS Code style; not in the keymap, ADR 0028). Keys the
    /// editor does not take (window shortcuts) bubble on to the keymap.
    fn file_editor_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let (Some(key), Some(project_id)) = (editor::edit_key(&event.keystroke), self.workspace)
        else {
            return;
        };
        cx.stop_propagation();
        let page = self.file_page_lines(cx);
        // An empty clipboard pastes nothing (no empty undo step).
        let pasted = (key == EditKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten()
            .filter(|text| !text.is_empty());
        let Some(editor) = self.files_view(project_id).editor_mut() else {
            return;
        };
        let version = editor.version();
        let copied = match key {
            EditKey::Copy => editor.selected_text(),
            EditKey::Cut => editor.cut(),
            EditKey::Paste => {
                if let Some(text) = pasted {
                    editor.insert(&text);
                }
                None
            }
            key => {
                editor.key(key, page);
                None
            }
        };
        let (line, edited) = (editor.cursor().line, editor.version() != version);
        if let Some(text) = copied {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        self.file_code_scroll
            .scroll_to_item(line, ScrollStrategy::Nearest);
        if edited {
            self.file_edited(project_id, window, cx);
        }
        cx.notify();
    }

    /// A click (or a drag, `extend`) at window x `x` on line `line` of the editor.
    fn click_file_text(
        &mut self,
        project_id: Ulid,
        line: usize,
        x: Pixels,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.focus(&self.file_editor_focus, cx);
        let cell_width = mkrew_ui::terminal::view::cell_size(window, cx.theme()).0;
        let cells = ((x - self.file_text_left.get()) / cell_width)
            .round()
            .max(0.0) as usize;
        let Some(editor) = self.files_view(project_id).editor_mut() else {
            return;
        };
        let Some(text) = editor.lines().get(line) else {
            return;
        };
        let col = editor::byte_for_display_col(text, cells);
        editor.set_cursor(Pos { line, col }, extend);
        cx.notify();
    }

    /// After an edit, find follows the new text, and the highlight follows once typing pauses.
    fn file_edited(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let view = self.files_view(project_id);
        view.search();
        let Some(version) = view.shown().map(|shown| shown.editor.version()) else {
            return;
        };
        let target = view.worktree_task_id;
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            background.timer(theme::REHIGHLIGHT_DELAY).await;
            let Ok(Some(content)) = this.update(cx, |shell, _| {
                shell
                    .files_at(project_id, target)
                    .and_then(|view| view.highlight_request(version))
            }) else {
                return;
            };
            let code = background.spawn(async move { Code::new(&content) }).await;
            let _ = this.update(cx, |shell, cx| {
                let view = shell.files_at(project_id, target);
                if view.is_some_and(|view| view.highlighted(version, code)) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Lines the editor shows at once (PageUp / PageDown).
    fn file_page_lines(&self, cx: &App) -> usize {
        let theme = cx.theme();
        let row = px(theme.terminal_font_size * theme.terminal_line_height);
        let shown = self.file_code_scroll.0.borrow().last_item_size;
        shown
            .map_or(1, |size| (size.item.height / row).floor() as usize)
            .max(1)
    }

    /// The absolute path of `path` (relative, `/`-separated) in workspace `project_id`, with
    /// native separators (Zed gets the path verbatim).
    fn workspace_file(&self, project_id: Ulid, path: &str) -> Option<PathBuf> {
        let project = self
            .sidebar
            .projects()
            .iter()
            .find(|p| p.id == project_id)?;
        Some(worktree_target::join_relative(
            PathBuf::from(&project.path),
            path,
        ))
    }

    /// [`Self::workspace_file`] in task worktree `target` when there is one (SPEC §20E.1).
    fn target_file(&self, project_id: Ulid, target: Option<Ulid>, path: &str) -> Option<PathBuf> {
        let Some(task_id) = target else {
            return self.workspace_file(project_id, path);
        };
        let project = self
            .sidebar
            .projects()
            .iter()
            .find(|p| p.id == project_id)?;
        let root = self.board.task(task_id)?.worktree_path.as_deref()?;
        let dir = worktree_target::project_dir(Path::new(&project.path), Path::new(root));
        Some(worktree_target::join_relative(dir, path))
    }

    /// Opens the opened file of the Files view in Zed, at the selected line if any; a failure
    /// shows in the viewer header.
    fn open_file_in_zed(&mut self, project_id: Ulid, cx: &mut Context<'_, Self>) {
        let Some(view) = self.files.get(&project_id) else {
            return;
        };
        let (Some(path), line) = (view.file.clone(), view.line()) else {
            return;
        };
        let Some(full) = self.target_file(project_id, view.worktree_task_id, &path) else {
            return;
        };
        let error = zed::open(&full, line).err().map(|error| {
            tracing::warn!(%project_id, path, "open in Zed failed: {error}");
            error.to_string()
        });
        self.files_view(project_id).zed_error = error;
        cx.notify();
    }

    /// Files and Git views on a worktree that was removed (or whose task was deleted) go back
    /// to the project folder.
    fn drop_gone_targets(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let tasks = self.board.tasks();
        let gone = |project_id, target| !worktree_target::exists(project_id, target, tasks);
        let files: Vec<Ulid> = (self.files.values())
            .filter(|view| gone(view.project_id, view.worktree_task_id))
            .map(|view| view.project_id)
            .collect();
        let git: Vec<Ulid> = (self.git.values())
            .filter(|view| gone(view.project_id, view.worktree_task_id))
            .map(|view| view.project_id)
            .collect();
        // Hidden views read again when shown.
        let shown = self.workspace;
        for project_id in files {
            self.files
                .insert(project_id, FilesView::new(project_id, None));
            if shown == Some(project_id) && self.view() == View::Files {
                self.refresh_files(project_id, window, cx);
            }
        }
        for project_id in git {
            self.retarget_git(project_id, None, None);
            if shown == Some(project_id) && self.view() == View::Git {
                self.refresh_git(project_id, window, cx);
            }
        }
    }

    /// The Git view of `project_id`, created on first use.
    fn git_view(&mut self, project_id: Ulid) -> &mut GitView {
        self.git
            .entry(project_id)
            .or_insert_with(|| GitView::new(project_id, None))
    }

    /// The Git view of `project_id` while it still shows `at` (task worktree, repository; an
    /// answer sent for another is dropped).
    fn git_at(
        &mut self,
        project_id: Ulid,
        at: &(Option<Ulid>, Option<String>),
    ) -> Option<&mut GitView> {
        self.git
            .get_mut(&project_id)
            .filter(|view| view.worktree_task_id == at.0 && view.repo == at.1)
    }

    /// The chooser picked `target` (SPEC §20E.1): the Git view starts over on it, in its
    /// working changes.
    fn set_git_target(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.retarget_git(project_id, target, None) {
            self.refresh_git(project_id, window, cx);
        }
    }

    /// The "Repo" chooser picked `repo` (ADR 0029): the Git view starts over on it.
    fn set_git_repo(
        &mut self,
        project_id: Ulid,
        repo: Option<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.retarget_git(project_id, None, repo) {
            self.refresh_git(project_id, window, cx);
        }
    }

    /// "Review diff" on the Changes card: the Git view of `repo` (`""` = the workspace folder),
    /// in its working changes. `retarget_git` first, so `show_view`'s refresh already asks for
    /// that repository (it stays: `pick_repo` keeps a listed repo). A view that already showed
    /// the repository may sit on History / Against base: `show(Working)` brings it back (its
    /// requests are dropped, the refresh asks status again).
    fn review_diff(
        &mut self,
        project_id: Ulid,
        repo: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.retarget_git(project_id, None, (!repo.is_empty()).then_some(repo));
        let _ = self.git_view(project_id).show(git_view::Section::Working);
        self.show_view(View::Git, window, cx);
    }

    /// Puts a fresh Git view of `target` / `repo` in place (the commit draft is kept); `false`
    /// = it shows them already.
    fn retarget_git(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        repo: Option<String>,
    ) -> bool {
        let old = self.git_view(project_id);
        if old.worktree_task_id == target && old.repo == repo {
            return false;
        }
        let draft = std::mem::take(&mut old.commit.text);
        let mut view = GitView::new(project_id, target);
        view.repo = repo;
        view.commit.text = draft;
        self.git.insert(project_id, view);
        self.git_files_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.git_diff_scroll.scroll_to_item(0, ScrollStrategy::Top);
        true
    }

    /// Asks the Git view of `project_id` for its repositories, status and shown diff again.
    /// Every way the view is shown (show_view, activate_workspace, Retry) goes through it;
    /// `git.changed` does not (it only re-asks status / diff).
    fn refresh_git(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.load_git_repos(project_id, window, cx);
        let requests = self.git_view(project_id).refresh();
        self.git_requests(project_id, requests, window, cx);
    }

    /// `git.repos` of `project_id` (ADR 0029): the "Repo" chips; a shown repository that is
    /// gone, or a workspace folder that is no repository, moves the view to
    /// [`git_view::pick_repo`] (never while a task worktree is shown: the daemon refuses a
    /// repo there).
    fn load_git_repos(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let listed = connection.call::<Vec<String>>(Request::GitRepos(ProjectRef { project_id }));
        cx.spawn_in(window, async move |this, cx| {
            let repos = match listed.await {
                Ok(repos) => repos,
                Err(error) => {
                    tracing::warn!(%project_id, "git.repos failed: {error:#}");
                    return;
                }
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let view = shell.git_view(project_id);
                let worktree = view.worktree_task_id;
                let picked = git_view::pick_repo(view.repo.as_deref(), &repos);
                shell.git_repos.insert(project_id, repos);
                // ponytail: the first status of a non-repo root may fail before this answer
                // and flash "Cannot read the git status"; send status after git.repos if it shows.
                if worktree.is_none() && shell.retarget_git(project_id, None, picked) {
                    let requests = shell.git_view(project_id).refresh();
                    shell.git_requests(project_id, requests, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The summary of the shown workspace (None until loaded, and for other workspaces).
    fn git_summary(&self) -> Option<&GitSummary> {
        self.git_summary
            .as_ref()
            .filter(|(id, _)| Some(*id) == self.workspace)
            .map(|(_, summary)| summary)
    }

    /// `git.repos`, then one `git.status` per repository, for the shown workspace (title bar).
    /// Asked on a workspace switch and on its `git.changed`, never polled and never for a
    /// hidden workspace. One request set at a time: a change that arrives meanwhile asks once
    /// more when it answers.
    fn load_git_summary(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if self.git_summary_busy {
            self.git_summary_again = true;
            return;
        }
        self.git_summary_busy = true;
        cx.spawn_in(window, async move |this, cx| {
            let loaded = git_summary_of(&connection, project_id).await;
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.git_summary_busy = false;
                let again = std::mem::take(&mut shell.git_summary_again);
                let shown = shell.workspace;
                match loaded {
                    // An answer for a workspace that is no longer shown is dropped.
                    Ok(summary) if shown == Some(project_id) => {
                        let same = matches!(
                            &shell.git_summary,
                            Some((id, old)) if *id == project_id && *old == summary
                        );
                        if !same {
                            shell.git_summary = Some((project_id, summary));
                            cx.notify();
                        }
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%project_id, "git summary failed: {error:#}"),
                }
                if let Some(shown) = shown
                    && (again || shown != project_id)
                {
                    shell.load_git_summary(shown, window, cx);
                }
            });
        })
        .detach();
    }

    fn git_requests(
        &mut self,
        project_id: Ulid,
        requests: git_view::Requests,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if requests.status {
            self.load_git_status(project_id, window, cx);
            self.load_session_files(project_id, window, cx);
        }
        if let Some(target) = requests.diff {
            self.load_git_diff(project_id, target, window, cx);
            // Another file: its diff comments are listed too.
            if let Some(path) = self.git_view(project_id).begin_comments() {
                self.load_comments(project_id, CommentContext::Diff, path, window, cx);
            }
        }
        if requests.log {
            self.load_git_log(project_id, window, cx);
        }
        if let Some(hash) = requests.show {
            self.load_git_show(project_id, hash, window, cx);
        }
        cx.notify();
    }

    /// `git.log` of the History of `project_id`; while one is in flight, it is read once more
    /// after it answers.
    fn load_git_log(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(params) = self.git_view(project_id).begin_log() else {
            return;
        };
        let at = (params.worktree_task_id, params.repo.clone());
        let loaded = connection.call::<Vec<Commit>>(Request::GitLog(params));
        cx.spawn_in(window, async move |this, cx| {
            let loaded = loaded.await.map_err(|error| format!("{error:#}"));
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &loaded {
                    tracing::warn!(%project_id, "git.log failed: {error}");
                }
                let Some(view) = shell.git_at(project_id, &at) else {
                    return;
                };
                let requests = view.logged(loaded);
                shell.git_requests(project_id, requests, window, cx);
            });
        })
        .detach();
    }

    /// `git.show` of History commit `hash`: its files, then its first file's diff.
    fn load_git_show(
        &mut self,
        project_id: Ulid,
        hash: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let params = self.git_view(project_id).show_params(hash.clone());
        let at = (params.worktree_task_id, params.repo.clone());
        let loaded = connection.call::<CommitDetail>(Request::GitShow(params));
        cx.spawn_in(window, async move |this, cx| {
            let loaded = loaded.await.map_err(|error| format!("{error:#}"));
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &loaded {
                    tracing::warn!(%project_id, commit = hash, "git.show failed: {error}");
                }
                let Some(view) = shell.git_at(project_id, &at) else {
                    return;
                };
                let requests = view.commit_shown(&hash, loaded);
                shell.git_requests(project_id, requests, window, cx);
            });
        })
        .detach();
    }

    /// A click on a History commit: lists its files (its first file's diff follows).
    fn select_git_commit(
        &mut self,
        project_id: Ulid,
        hash: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.focus(&self.git_focus, cx);
        let show = self.git_view(project_id).select_commit(hash);
        self.git_diff_scroll.scroll_to_item(0, ScrollStrategy::Top);
        let requests = git_view::Requests {
            show,
            ..git_view::Requests::default()
        };
        self.git_requests(project_id, requests, window, cx);
    }

    /// A click on a section of the Git header (Working changes / Against base / History).
    fn show_git_section(
        &mut self,
        project_id: Ulid,
        section: git_view::Section,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.focus(&self.git_focus, cx);
        let requests = self.git_view(project_id).show(section);
        self.git_files_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.git_requests(project_id, requests, window, cx);
    }

    /// `git.status` of the Git view of `project_id` in its shown mode (also keeps the daemon
    /// watching the workspace, so `git.changed` follows file changes).
    fn load_git_status(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(params) = self.git_view(project_id).begin_status() else {
            return;
        };
        let at = (params.worktree_task_id, params.repo.clone());
        let loaded = connection.call::<GitStatus>(Request::GitStatus(params));
        cx.spawn_in(window, async move |this, cx| {
            let loaded = loaded.await.map_err(|error| format!("{error:#}"));
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &loaded {
                    tracing::warn!(%project_id, "git.status failed: {error}");
                }
                let Some(view) = shell.git_at(project_id, &at) else {
                    return;
                };
                let requests = view.status_loaded(loaded);
                shell.git_requests(project_id, requests, window, cx);
                // Everything committed: the box is gone, the list takes the keyboard back.
                if shell.git_commit_focus.is_focused(window) && !shell.git_commit_shown() {
                    window.focus(&shell.git_focus, cx);
                }
            });
        })
        .detach();
    }

    /// `session.files` of the last 24 h for the Git view's "Changed by" filter (SPEC §20C.4);
    /// read with every status, so it follows the same refreshes. A failure keeps the last answer.
    fn load_session_files(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let view = self.git_view(project_id);
        let at = view.at();
        let (params, serial) = view.begin_session_files(status::now_ms());
        let loaded = connection.call::<SessionFiles>(Request::SessionFiles(params));
        cx.spawn_in(window, async move |this, cx| {
            let files = match loaded.await {
                Ok(files) => files,
                Err(error) => {
                    tracing::warn!(%project_id, "session.files failed: {error:#}");
                    return;
                }
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let Some(view) = shell.git_at(project_id, &at) else {
                    return;
                };
                let requests = view.session_files_loaded(serial, files);
                shell.git_requests(project_id, requests, window, cx);
            });
        })
        .detach();
    }

    /// "Changed by" in the Git view: keeps only `session`'s files (`None` = All).
    fn filter_git_changed_by(
        &mut self,
        project_id: Ulid,
        session: Option<Ulid>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let requests = self.git_view(project_id).filter_changed_by(session);
        self.git_files_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.git_requests(project_id, requests, window, cx);
    }

    /// `git.diff` of the selected file; while one is in flight, the selected file's diff is
    /// read once more after it answers.
    fn load_git_diff(
        &mut self,
        project_id: Ulid,
        target: git_view::Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let view = self.git_view(project_id);
        if !view.begin_diff() {
            return;
        }
        let at = view.at();
        let loaded = connection.call::<FileDiff>(Request::GitDiff(target.params(
            project_id,
            at.0,
            at.1.clone(),
        )));
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            // Syntax highlighting is slow on big diffs: lay out off the UI thread.
            let loaded = match loaded.await {
                Ok(diff) => Ok(background
                    .spawn(async move { git_view::Shown::new(diff) })
                    .await),
                Err(error) => Err(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &loaded {
                    tracing::warn!(%project_id, path = target.path, "git.diff failed: {error}");
                }
                let again = shell
                    .git_at(project_id, &at)
                    .and_then(|view| view.diff_loaded(&target, loaded));
                if let Some(target) = again {
                    shell.load_git_diff(project_id, target, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Follows `git.changed` in the Git view on screen (hidden ones refresh when shown).
    fn git_event(&mut self, event: &Event, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.workspace.filter(|_| self.view() == View::Git) else {
            return;
        };
        let Some(view) = self.git.get_mut(&project_id) else {
            return;
        };
        let requests = view.apply(event);
        self.git_requests(project_id, requests, window, cx);
    }

    /// A click on a changed file: shows its diff.
    fn select_git_file(
        &mut self,
        project_id: Ulid,
        target: git_view::Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.focus(&self.git_focus, cx);
        let diff = self.git_view(project_id).select(target);
        self.git_diff_scroll.scroll_to_item(0, ScrollStrategy::Top);
        let requests = git_view::Requests {
            diff,
            ..git_view::Requests::default()
        };
        self.git_requests(project_id, requests, window, cx);
    }

    /// ↑ ↓ / j k move between files; the other Git keys are keymap commands ([`Self::git_key`]).
    fn git_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(key) = git_view::key(&event.keystroke) else {
            return;
        };
        cx.stop_propagation();
        self.git_key(key, window, cx);
    }

    /// A Git key of the active workspace: files move (kept in view), hunks move (scrolled to
    /// the top), the mode toggles, or the selected file / current hunk is (un)staged.
    fn git_key(&mut self, key: git_view::Key, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.workspace else {
            return;
        };
        if let git_view::Key::Stage(stage) = key {
            if let Some(change) = self.git_view(project_id).stage_key(stage) {
                self.stage_git(project_id, change, window, cx);
            }
            return;
        }
        let mode = self.diff_mode;
        let view = self.git_view(project_id);
        let requests = view.key(key);
        let selected = view.selected.clone();
        // The selected file, else (History, files loading) the expanded commit.
        let file_row = view.list_rows().iter().position(|row| match row {
            git_view::ListRow::File(target, _) => selected.as_ref() == Some(target),
            git_view::ListRow::Commit { expanded, .. } => selected.is_none() && *expanded,
            git_view::ListRow::Header(..) | git_view::ListRow::Note(_) => false,
        });
        let hunk_row = match (&view.diff, view.hunk) {
            (git_view::Diff::Shown(shown), Some(hunk)) => shown.hunk_row(hunk, mode),
            _ => None,
        };
        if let Some(at) = file_row {
            self.git_files_scroll
                .scroll_to_item(at, ScrollStrategy::Nearest);
        }
        match key {
            git_view::Key::NextHunk | git_view::Key::PreviousHunk => {
                if let Some(at) = hunk_row {
                    self.git_diff_scroll.scroll_to_item(at, ScrollStrategy::Top);
                }
            }
            git_view::Key::Up | git_view::Key::Down
                if requests.diff.is_some() || requests.show.is_some() =>
            {
                self.git_diff_scroll.scroll_to_item(0, ScrollStrategy::Top);
            }
            _ => {}
        }
        self.git_requests(project_id, requests, window, cx);
    }

    /// Opens the selected changed file of the Git view in Zed; a failure shows in the diff
    /// header.
    fn open_git_file_in_zed(&mut self, project_id: Ulid, cx: &mut Context<'_, Self>) {
        let Some((target, path)) = self.git.get(&project_id).and_then(|view| {
            let selected = view.selected.as_ref()?;
            Some((view.worktree_task_id, selected.path.clone()))
        }) else {
            return;
        };
        let Some(full) = self.target_file(project_id, target, &path) else {
            return;
        };
        let error = zed::open(&full, None).err().map(|error| {
            tracing::warn!(%project_id, path, "open in Zed failed: {error}");
            error.to_string()
        });
        self.git_view(project_id).zed_error = error;
        cx.notify();
    }

    /// `git.stage` / `git.unstage` of a file or hunk; then the list and diff are read again
    /// (a failure shows under the diff header).
    fn stage_git(
        &mut self,
        project_id: Ulid,
        change: git_view::StageRequest,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if !self.git_view(project_id).begin_stage() {
            return;
        }
        cx.notify();
        let stage = change.stage;
        let at = (change.params.worktree_task_id, change.params.repo.clone());
        let sent = connection.request(change.request());
        cx.spawn_in(window, async move |this, cx| {
            let result = match sent.await {
                Ok(Outcome::Result(_)) => Ok(()),
                Ok(Outcome::Error(body)) => Err(body.message),
                Err(error) => Err(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &result {
                    tracing::warn!(%project_id, ?stage, "git stage/unstage failed: {error}");
                }
                let Some(view) = shell.git_at(project_id, &at) else {
                    return;
                };
                let requests = view.staged(result);
                shell.git_requests(project_id, requests, window, cx);
            });
        })
        .detach();
    }

    /// "Discard…" on an Unstaged / Untracked file: selects it (so its diff, what is lost,
    /// loads) and opens the confirmation.
    fn open_git_discard(
        &mut self,
        project_id: Ulid,
        target: git_view::Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(discard) = self.git_view(project_id).discard(&target) else {
            return;
        };
        self.select_git_file(project_id, target, window, cx);
        self.git_discard = Some(discard);
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    fn close_git_discard(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.git_discard = None;
        window.focus(&self.git_focus, cx);
        cx.notify();
    }

    fn git_discard_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        if undo_dialog::cancels(&event.keystroke) {
            self.close_git_discard(window, cx);
        }
    }

    /// The destructive confirm: `git.discard` of the dialog's file, `confirm: true`.
    fn confirm_git_discard(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let serial = self.dialog_serial;
        let Some(dialog) = self.git_discard.as_mut().filter(|dialog| !dialog.busy) else {
            return;
        };
        dialog.busy = true;
        dialog.error = None;
        let project_id = dialog.project_id;
        let discarded = connection.request(Request::GitDiscard(dialog.params()));
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let failure = match discarded.await {
                Ok(Outcome::Result(_)) => None,
                Ok(Outcome::Error(body)) => Some(body.message),
                Err(error) => Some(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let open = shell
                    .git_discard
                    .as_mut()
                    .filter(|_| shell.dialog_serial == serial);
                match failure {
                    None => {
                        if open.is_some() {
                            shell.close_git_discard(window, cx);
                        }
                        shell.refresh_git(project_id, window, cx);
                    }
                    Some(error) => {
                        tracing::warn!(%project_id, "git.discard failed: {error}");
                        if let Some(dialog) = open {
                            dialog.busy = false;
                            dialog.error = Some(error);
                        }
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    /// Keys typed in the commit box: Enter is a new line, Ctrl+Enter commits, Esc goes back
    /// to the changed-file list (the draft stays).
    fn git_commit_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(project_id) = self.workspace else {
            return;
        };
        let Some(key) = task_dialog::dialog_key(&event.keystroke) else {
            return;
        };
        let paste = (key == DialogKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let draft = &mut self.git_view(project_id).commit;
        if let Some(text) = paste {
            draft.insert(&text);
        }
        match draft.key(key) {
            git_actions::Outcome::Edited => cx.notify(),
            git_actions::Outcome::Leave => {
                window.focus(&self.git_focus, cx);
                cx.notify();
            }
            git_actions::Outcome::Commit => self.commit_git(project_id, window, cx),
        }
    }

    /// Whether the active workspace's Git view is shown with its commit box.
    fn git_commit_shown(&self) -> bool {
        self.view() == View::Git
            && self
                .workspace
                .and_then(|id| self.git.get(&id))
                .is_some_and(GitView::commit_box_shown)
    }

    /// `git.commit` of the staged changes with the drafted message; then the view reads its
    /// status again either way (a hook may have changed files; a failure stays under the box).
    fn commit_git(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let view = self.git_view(project_id);
        let staged = match &view.status {
            git_view::Status::Loaded(status) => status.staged.len(),
            git_view::Status::Loading | git_view::Status::Failed(_) => 0,
        };
        let at = view.at();
        let params = view.commit.begin(project_id, staged);
        cx.notify();
        let Some(mut params) = params else {
            return;
        };
        (params.worktree_task_id, params.repo) = at.clone();
        let committed = connection.call::<GitCommitResult>(Request::GitCommit(params));
        cx.spawn_in(window, async move |this, cx| {
            let result = committed
                .await
                .map(|_| ())
                .map_err(|error| format!("{error:#}"));
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &result {
                    tracing::warn!(%project_id, "git.commit failed: {error}");
                }
                let Some(view) = shell.git_at(project_id, &at) else {
                    return;
                };
                view.commit.committed(result);
                shell.refresh_git(project_id, window, cx);
            });
        })
        .detach();
    }

    /// ✦ in the commit box (SPEC §12B): the workspace's default agent writes a Conventional
    /// Commits message from the changes; the answer arrives as `ai.generated`.
    fn generate_commit_message(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let view = self.git_view(project_id);
        let generation_id = Ulid::new();
        if !view.commit.begin_generate(generation_id) {
            return;
        }
        let params = AiGenerateParams {
            generation_id,
            project_id,
            worktree_task_id: view.worktree_task_id,
            repo: view.repo.clone(),
            agent_profile: None,
            kind: AiKind::CommitMessage,
            input: String::new(),
        };
        cx.notify();
        let started = connection.call::<()>(Request::AiGenerate(params));
        cx.spawn_in(window, async move |this, cx| {
            let Err(error) = started.await else {
                return;
            };
            let _ = this.update_in(cx, |shell, _, cx| {
                for view in shell.git.values_mut() {
                    view.commit
                        .generated(generation_id, Err(format!("{error:#}")));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Fetch or Push of the Git view header; then the view reads its status again (base,
    /// ahead/behind). A failure shows under the header.
    fn run_git_remote(
        &mut self,
        project_id: Ulid,
        remote: Remote,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let view = self.git_view(project_id);
        if !view.remotes.begin(remote) {
            return;
        }
        let at = view.at();
        cx.notify();
        let sent = connection.request(remote.request(project_id, at.0, at.1.clone()));
        cx.spawn_in(window, async move |this, cx| {
            let result = match sent.await {
                Ok(Outcome::Result(_)) => Ok(()),
                Ok(Outcome::Error(body)) => Err(body.message),
                Err(error) => Err(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &result {
                    tracing::warn!(%project_id, ?remote, "git fetch/push failed: {error}");
                }
                let Some(view) = shell.git_at(project_id, &at) else {
                    return;
                };
                view.remotes.finished(remote, result);
                shell.refresh_git(project_id, window, cx);
            });
        })
        .detach();
    }

    /// The base menu of the Git view of `project_id`, prefilled with its configured base.
    fn open_git_base(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let status = match self.git.get(&project_id).map(|view| &view.status) {
            Some(git_view::Status::Loaded(status)) => Some(&**status),
            _ => None,
        };
        self.git_base = Some(BaseDialog::new(project_id, status));
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    fn close_git_base(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.git_base = None;
        window.focus(&self.git_focus, cx);
        cx.notify();
    }

    /// Typing edits the ref, Enter uses it (empty = auto-detect), Esc closes.
    fn git_base_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = base_dialog::key(&event.keystroke) else {
            return;
        };
        let paste = (key == base_dialog::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(dialog) = self.git_base.as_mut() else {
            return;
        };
        match key {
            base_dialog::Key::Close => return self.close_git_base(window, cx),
            base_dialog::Key::Save => return self.set_git_base(false, window, cx),
            base_dialog::Key::Paste => {
                if let Some(text) = paste {
                    dialog.insert(&text);
                }
            }
            base_dialog::Key::Backspace
            | base_dialog::Key::DeleteWord
            | base_dialog::Key::SelectAll
            | base_dialog::Key::Insert(_) => {
                dialog.key(&key);
            }
        }
        cx.notify();
    }

    /// `git.set_base` with the typed ref, or back to auto-detection (`auto`); on success the
    /// menu closes and the Git view reads its status again.
    fn set_git_base(&mut self, auto: bool, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let serial = self.dialog_serial;
        let Some(params) = self.git_base.as_mut().and_then(|dialog| dialog.begin(auto)) else {
            return;
        };
        let project_id = params.project_id;
        let sent = connection.request(Request::GitSetBase(params));
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let failure = match sent.await {
                Ok(Outcome::Result(_)) => None,
                Ok(Outcome::Error(body)) => Some(body.message),
                Err(error) => Some(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let open = shell
                    .git_base
                    .as_mut()
                    .filter(|_| shell.dialog_serial == serial);
                match failure {
                    None => {
                        if open.is_some() {
                            shell.close_git_base(window, cx);
                        }
                        if shell.git.contains_key(&project_id) {
                            shell.refresh_git(project_id, window, cx);
                        }
                    }
                    Some(error) => {
                        tracing::warn!(%project_id, "git.set_base failed: {error}");
                        if let Some(dialog) = open {
                            dialog.failed(error);
                        }
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    /// "Review in Docs" for an opened `.md` file: the Docs view shows it.
    fn review_in_docs(
        &mut self,
        project_id: Ulid,
        path: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.docs
            .entry(project_id)
            .or_insert_with(|| Docs::new(project_id));
        self.select_doc(project_id, &path, window, cx);
        self.show_view(View::Docs, window, cx);
    }

    /// The comments of `project_id`'s Docs view (doc comments) or Git view (diff comments).
    fn comments_mut(&mut self, project_id: Ulid, context: CommentContext) -> Option<&mut Comments> {
        match context {
            CommentContext::Doc => self
                .docs
                .get_mut(&project_id)
                .map(|docs| &mut docs.comments),
            CommentContext::Diff => self.git.get_mut(&project_id).map(|view| &mut view.comments),
        }
    }

    /// Whose comments the shown view has: the Docs view's or the Git view's.
    fn comment_context(&self) -> Option<CommentContext> {
        match self.view() {
            View::Docs => Some(CommentContext::Doc),
            View::Git => Some(CommentContext::Diff),
            _ => None,
        }
    }

    /// `comment.list` of the shown doc or diff; `comment.updated` keeps it live afterwards.
    fn load_comments(
        &mut self,
        project_id: Ulid,
        context: CommentContext,
        file_path: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let listed = connection.call::<Vec<DocComment>>(Request::CommentList(CommentListParams {
            project_id,
            file_path: Some(file_path.clone()),
            context: Some(context),
        }));
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await.map_err(|error| format!("{error:#}"));
            let _ = this.update(cx, |shell, cx| {
                if let Err(error) = &listed {
                    tracing::warn!(%project_id, file_path, "comment.list failed: {error}");
                }
                if let Some(comments) = shell.comments_mut(project_id, context) {
                    comments.listed(&file_path, listed);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The comment or reply being typed in the active workspace's Docs or Git view, while its
    /// field is on screen (the Docs margin shows with the doc, also while it is re-read).
    fn comment_draft(&self) -> Option<&comments::Draft> {
        let project_id = self.workspace?;
        match self.comment_context()? {
            CommentContext::Doc => {
                let docs = self.docs.get(&project_id)?;
                if !margin_shown(&docs.doc) {
                    return None;
                }
                docs.comments.draft.as_ref()
            }
            CommentContext::Diff => {
                let view = self.git.get(&project_id)?;
                if !view.comments_shown() {
                    return None;
                }
                view.comments.draft.as_ref()
            }
        }
    }

    /// A click on lines `start..=end` of the shown doc or diff (Shift extends the selection).
    fn pick_lines(
        &mut self,
        project_id: Ulid,
        context: CommentContext,
        start: u32,
        end: u32,
        extend: bool,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(comments) = self.comments_mut(project_id, context) {
            comments.pick(start, end, extend);
            cx.notify();
        }
    }

    /// A click on a diff line (new-side line `line`): picks it, and the Git list takes the
    /// keyboard so Ctrl+Alt+M comments (an open draft keeps it, so typing never stages).
    fn pick_diff_line(
        &mut self,
        project_id: Ulid,
        line: u32,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.git_view(project_id).comments.draft.is_none() {
            window.focus(&self.git_focus, cx);
        }
        self.pick_lines(project_id, CommentContext::Diff, line, line, extend, cx);
    }

    /// "Add comment" (Ctrl+Alt+M): a draft in the margin for the selected lines.
    fn start_comment(
        &mut self,
        project_id: Ulid,
        context: CommentContext,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let started = match context {
            CommentContext::Doc => match self.docs.get_mut(&project_id) {
                Some(Docs {
                    doc: Doc::Shown(shown),
                    comments,
                    ..
                }) => comments.start_comment(&shown.lines),
                _ => false,
            },
            CommentContext::Diff => self
                .git
                .get_mut(&project_id)
                .is_some_and(GitView::start_comment),
        };
        if started {
            window.focus(&self.comment_focus, cx);
        }
        cx.notify();
    }

    fn start_reply(
        &mut self,
        project_id: Ulid,
        context: CommentContext,
        root: CommentId,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(comments) = self.comments_mut(project_id, context) {
            comments.start_reply(root);
            window.focus(&self.comment_focus, cx);
            cx.notify();
        }
    }

    fn close_comment_draft(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let shown = self.workspace.zip(self.comment_context());
        if let Some(comments) = shown.and_then(|(id, context)| self.comments_mut(id, context)) {
            comments.draft = None;
        }
        if self.view() == View::Git {
            window.focus(&self.git_focus, cx);
        } else {
            window.focus(&self.focus_handle, cx);
        }
        cx.notify();
    }

    /// Keys typed in a comment draft: Enter saves, Esc drops it.
    fn comment_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = task_dialog::dialog_key(&event.keystroke) else {
            return;
        };
        let paste = (key == DialogKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let (Some(project_id), Some(context)) = (self.workspace, self.comment_context()) else {
            return;
        };
        let Some(draft) = self
            .comments_mut(project_id, context)
            .and_then(|comments| comments.draft.as_mut())
        else {
            return;
        };
        if let Some(text) = paste {
            draft.insert(&text);
        }
        match draft.key(key) {
            comments::Outcome::Edited => cx.notify(),
            comments::Outcome::Cancel => self.close_comment_draft(window, cx),
            comments::Outcome::Submit => self.submit_comment(project_id, context, window, cx),
        }
    }

    /// `comment.create` / `comment.reply` of the draft; saved → the draft closes.
    fn submit_comment(
        &mut self,
        project_id: Ulid,
        context: CommentContext,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(comments) = self.comments_mut(project_id, context) else {
            return;
        };
        let Some(serial) = comments.draft.as_ref().map(|draft| draft.serial) else {
            return;
        };
        let Some(request) = comments.submit(project_id) else {
            return;
        };
        cx.notify();
        let saved = connection.call::<DocComment>(request);
        cx.spawn_in(window, async move |this, cx| {
            let saved = saved.await.map_err(|error| format!("{error:#}"));
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &saved {
                    tracing::warn!(%project_id, "saving a comment failed: {error}");
                }
                let ok = saved.is_ok();
                if let Some(comments) = shell.comments_mut(project_id, context) {
                    let saved = saved.map_err(|error| format!("Cannot save: {error}"));
                    comments.submitted(serial, saved);
                }
                if ok && shell.comment_focus.is_focused(window) && shell.comment_draft().is_none() {
                    if shell.view() == View::Git {
                        window.focus(&shell.git_focus, cx);
                    } else {
                        window.focus(&shell.focus_handle, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Resolve (or Reopen) the thread of `root`; the answer updates it like `comment.updated`.
    fn set_comment_status(
        &mut self,
        project_id: Ulid,
        context: CommentContext,
        root: CommentId,
        resolve: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let params = CommentRef { comment_id: root };
        let request = if resolve {
            Request::CommentResolve(params)
        } else {
            Request::CommentReopen(params)
        };
        let changed = connection.call::<DocComment>(request);
        cx.spawn_in(window, async move |this, cx| {
            let changed = changed.await;
            let _ = this.update(cx, |shell, cx| {
                let Some(comments) = shell.comments_mut(project_id, context) else {
                    return;
                };
                match changed {
                    Ok(comment) => {
                        comments.error = None;
                        comments.apply_event(project_id, &comment);
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, %root, "comment status change failed: {error:#}");
                        let what = if resolve { "resolve" } else { "reopen" };
                        comments.error = Some(format!("Cannot {what}: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Send to agent" (Ctrl+Shift+S): the shown doc's or diff's open comments (only the
    /// selected ones if the selection covers some) go to the session picked in a modal dialog.
    /// It offers the doc's last modifying session first, or in a task's changes the task's.
    fn open_send_dialog(
        &mut self,
        project_id: Ulid,
        context: CommentContext,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let (comments, file_path, last) = match context {
            CommentContext::Doc => {
                let Some(docs) = self.docs.get(&project_id) else {
                    return;
                };
                let Doc::Shown(_) = &docs.doc else {
                    return;
                };
                let last = docs.selected_file().and_then(|file| file.last_session_id);
                (&docs.comments, docs.selected.clone(), last)
            }
            CommentContext::Diff => {
                let Some(view) = self.git.get(&project_id) else {
                    return;
                };
                let last = view.since_session(self.sidebar.sessions());
                let path = view.selected.as_ref().map(|target| target.path.clone());
                (&view.comments, path, last)
            }
        };
        let (comments::List::Loaded(list), Some(file_path)) = (&comments.list, file_path) else {
            return;
        };
        let comment_ids = doc_dialog::sendable(list, comments.selection);
        if comment_ids.is_empty() || self.connection.is_none() {
            return;
        }
        let dialog = SendDialog {
            task: context == CommentContext::Diff && last.is_some(),
            ..SendDialog::new(project_id, file_path, comment_ids, last)
        };
        self.open_doc_dialog(DocDialog::Send(dialog), window, cx);
    }

    /// "View changes" of a sent thread: its file as sent vs as shown now.
    fn open_changes(
        &mut self,
        project_id: Ulid,
        root: CommentId,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(docs) = self.docs.get(&project_id) else {
            return;
        };
        let (Doc::Shown(shown), comments::List::Loaded(list)) = (&docs.doc, &docs.comments.list)
        else {
            return;
        };
        let changes = list
            .iter()
            .find(|comment| comment.id == root)
            .and_then(|root| Changes::new(root, &shown.lines));
        if let Some(changes) = changes {
            self.open_doc_dialog(DocDialog::Changes(changes), window, cx);
        }
    }

    fn open_doc_dialog(
        &mut self,
        dialog: DocDialog,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        self.doc_dialog = Some(dialog);
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    fn close_doc_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.doc_dialog = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    /// Esc closes a Docs dialog; in "Send to agent" ↑ / ↓ pick the session, Enter sends.
    fn doc_dialog_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let key = doc_dialog::key(&event.keystroke);
        match (self.doc_dialog.as_mut(), key) {
            (_, Some(doc_dialog::Key::Close)) => self.close_doc_dialog(window, cx),
            (Some(DocDialog::Send(dialog)), Some(doc_dialog::Key::Move(by))) => {
                dialog.step(self.sidebar.sessions(), by);
                cx.notify();
            }
            (Some(DocDialog::Send(_)), Some(doc_dialog::Key::Send)) => {
                self.confirm_send_dialog(window, cx);
            }
            _ => {}
        }
    }

    /// `comment.send_to_agent`: sent → the dialog closes (the threads show `sent`); failed →
    /// it stays with the reason.
    fn send_comments(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let serial = self.dialog_serial;
        let Some(DocDialog::Send(dialog)) = self.doc_dialog.as_mut() else {
            return;
        };
        let Some(params) = dialog.send(self.sidebar.sessions()) else {
            return;
        };
        let project_id = dialog.project_id;
        cx.notify();
        let sent = connection.call::<Vec<DocComment>>(Request::CommentSendToAgent(params));
        cx.spawn_in(window, async move |this, cx| {
            let sent = sent.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let open = shell.dialog_serial == serial
                    && matches!(shell.doc_dialog, Some(DocDialog::Send(_)));
                match sent {
                    Ok(comments) => {
                        // Each view keeps only its own context's comments.
                        for context in [CommentContext::Doc, CommentContext::Diff] {
                            if let Some(list) = shell.comments_mut(project_id, context) {
                                for comment in &comments {
                                    list.apply_event(project_id, comment);
                                }
                            }
                        }
                        if open {
                            shell.close_doc_dialog(window, cx);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "comment.send_to_agent failed: {error:#}");
                        if let (true, Some(DocDialog::Send(dialog))) =
                            (open, shell.doc_dialog.as_mut())
                        {
                            dialog.failed(format!("Cannot send: {error:#}"));
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Quick open (Ctrl+Shift+O): a fuzzy finder over the active workspace's files.
    fn open_quick_open(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.workspace.filter(|_| self.connection.is_some()) else {
            return;
        };
        self.workspace_menu = None;
        let target = self
            .files
            .get(&project_id)
            .and_then(|view| view.worktree_task_id);
        self.quick_open = Some(QuickOpen::new(project_id, target));
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        self.find_files(window, cx);
        cx.notify();
    }

    fn close_quick_open(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let open = self.quick_open.take();
        // Esc on an `@` picker types the `@` it replaced (an e-mail, a handle).
        match open.and_then(|open| open.composer) {
            Some(session) => self.composer_typed(session, "@", window, cx),
            None => self.focus_pane(window, cx),
        }
        cx.notify();
    }

    /// `@` in `session`'s composer: quick open over its workspace's files; the pick is typed
    /// as `composer.file_ref` (§20B.1). Without a daemon the `@` is typed as is.
    fn open_file_ref_picker(
        &mut self,
        session: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self
            .sidebar
            .sessions()
            .iter()
            .find(|s| s.id == session)
            .map(|s| s.project_id)
            .filter(|_| self.connection.is_some())
        else {
            self.composers.insert(session, "@");
            cx.notify();
            return;
        };
        // Project-relative paths for the composer, whatever the Files view shows.
        let mut open = QuickOpen::new(project_id, None);
        open.composer = Some(session);
        self.quick_open = Some(open);
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        self.find_files(window, cx);
        cx.notify();
    }

    /// Types `text` into `session`'s draft and gives its composer the keyboard back.
    fn composer_typed(
        &mut self,
        session: Ulid,
        text: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.composers.insert(session, text);
        self.composer_scroll.scroll_to_bottom();
        if self.composer_session() == Some(session) {
            window.focus(&self.composer_focus, cx);
        } else {
            self.focus_pane(window, cx);
        }
    }

    /// Files dropped from the OS on `session`'s pane (§20B.5): into its composer draft while
    /// the composer is shown, else typed into the terminal. No Enter either way. A modal dialog
    /// covers the panes, so a drop on it types nothing.
    fn drop_paths(
        &mut self,
        session: Ulid,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.modal_open() {
            return;
        }
        let paths: Vec<String> = paths
            .paths()
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        let text = composer::typed_paths(paths.iter().map(String::as_str));
        if text.is_empty() {
            return;
        }
        if let Some(tabs) = self.active_tabs_mut() {
            tabs.focus(session);
        }
        if self.composers.shown(session).is_some() {
            self.composer_typed(session, &text, window, cx);
        } else if let Some(pane) = self.panes.get(&session).cloned() {
            pane.update(cx, |pane, cx| pane.type_text(&text, cx));
            self.focus_pane(window, cx);
        }
        cx.notify();
    }

    /// `fs.find` for the finder's query; a reply to an older query or finder is dropped.
    fn find_files(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let dialog = self.dialog_serial;
        let Some(open) = self.quick_open.as_mut() else {
            return;
        };
        let (serial, params) = open.search();
        let project_id = params.project_id;
        let found = connection.call::<Vec<FsMatch>>(Request::FsFind(params));
        cx.spawn_in(window, async move |this, cx| {
            let found = found.await.map_err(|error| format!("{error:#}"));
            let _ = this.update(cx, |shell, cx| {
                if let Err(error) = &found {
                    tracing::warn!(%project_id, "fs.find failed: {error}");
                }
                if shell.dialog_serial != dialog {
                    return;
                }
                if let Some(open) = shell.quick_open.as_mut()
                    && open.found(serial, found)
                {
                    shell.quick_open_scroll.scroll_to_item(0);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Typing searches again, ↑ / ↓ move the pick (kept in view), Enter opens it, Esc closes.
    fn quick_open_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = quick_open::key(&event.keystroke) else {
            return;
        };
        let paste = (key == quick_open::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(open) = self.quick_open.as_mut() else {
            return;
        };
        match key {
            quick_open::Key::Close => return self.close_quick_open(window, cx),
            quick_open::Key::Open => return self.open_quick_pick(window, cx),
            quick_open::Key::Move(_) => {
                open.key(key);
                self.quick_open_scroll.scroll_to_item(open.selected);
            }
            quick_open::Key::Paste => {
                if paste.is_some_and(|text| open.insert(&text)) {
                    self.find_files(window, cx);
                }
            }
            quick_open::Key::Backspace
            | quick_open::Key::DeleteWord
            | quick_open::Key::SelectAll
            | quick_open::Key::Insert(_) => {
                if open.key(key) {
                    self.find_files(window, cx);
                }
            }
        }
        cx.notify();
    }

    /// Opens the picked file in the Files view (its folders open, the cursor lands on it).
    fn open_quick_pick(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(open) = self.quick_open.take() else {
            return;
        };
        let Some(path) = open.picked().map(str::to_owned) else {
            self.quick_open = Some(open);
            return;
        };
        if let Some(session) = open.composer {
            // A missing or broken settings file types the default form (the daemon reports it).
            let template = mkrew_core::settings::load(&self.settings_file)
                .unwrap_or_default()
                .composer
                .file_ref;
            let text = composer::file_ref(&template, &path);
            self.composer_typed(session, &text, window, cx);
            cx.notify();
            return;
        }
        let project_id = open.project_id;
        self.open_revealed(project_id, open.worktree_task_id, path, None, window, cx);
    }

    /// Where links typed in `session`'s terminal point: its project, the task worktree its
    /// agent runs in (`None` = the project folder) and that folder.
    fn link_scope(&self, session: Ulid) -> Option<(Ulid, Option<Ulid>, PathBuf)> {
        let session = self.sidebar.sessions().iter().find(|s| s.id == session)?;
        let project = (self.sidebar.projects().iter()).find(|p| p.id == session.project_id)?;
        let project_dir = PathBuf::from(&project.path);
        let worktree = session
            .task_id
            .and_then(|id| self.board.task(id))
            .and_then(|task| Some((task.id, task.worktree_path.as_deref()?)));
        Some(match worktree {
            Some((task, root)) => (
                project.id,
                Some(task),
                worktree_target::project_dir(&project_dir, Path::new(root)),
            ),
            None => (project.id, None, project_dir),
        })
    }

    /// Ctrl+Click on a terminal link: a URL opens in the browser, a file in the Files view.
    fn open_terminal_link(
        &mut self,
        session: Ulid,
        link: &OpenLink,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match link {
            OpenLink::Url(url) => cx.open_url(url),
            OpenLink::File { path, line } => {
                let Some((project_id, target, _)) = self.link_scope(session) else {
                    return;
                };
                self.open_file_at(project_id, target, path.clone(), *line, window, cx);
            }
        }
    }

    /// Shows `path` of the Files view on `target` (the project, or a task worktree) with the
    /// caret on `line`. Another target starts the view over, so unsaved text is asked about
    /// first (as the chooser does).
    fn open_file_at(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: String,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.files_view(project_id).worktree_task_id != target {
            if self.files_view(project_id).unsaved().is_some() {
                let again: AfterSave = Box::new(move |shell, window, cx| {
                    shell.open_file_at(project_id, target, path, line, window, cx);
                });
                return self.leave_file(project_id, again, window, cx);
            }
            self.set_files_target(project_id, target, window, cx);
        }
        self.open_revealed(project_id, target, path, line, window, cx);
    }

    /// Puts the caret on the line a link asked for and scrolls there, once the file is shown
    /// (after `reveal` when it already was, after the read otherwise).
    fn show_pending_line(&mut self, project_id: Ulid, target: Option<Ulid>) {
        if let Some(line) =
            (self.files_at(project_id, target)).and_then(FilesView::apply_pending_line)
        {
            self.file_code_scroll
                .scroll_to_item(line, ScrollStrategy::Center);
        }
    }

    /// Quick open's file: asks about unsaved changes first (before the workspace or view
    /// switches, so Cancel leaves everything as it was). The answer's continuation comes back
    /// here, because text typed while a save was in flight is unsaved again.
    fn open_revealed(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: String,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let leaves =
            (self.files_at(project_id, target)).is_some_and(|view| view.leaves_unsaved(&path));
        if leaves {
            let reveal: AfterSave = Box::new(move |shell, window, cx| {
                shell.open_revealed(project_id, target, path, line, window, cx);
            });
            return self.leave_file(project_id, reveal, window, cx);
        }
        self.reveal_file(project_id, target, path, line, window, cx);
    }

    /// Shows `path` in the Files view of `project_id` (its folders open, the cursor lands on it).
    fn reveal_file(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: String,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.workspace != Some(project_id) {
            self.activate_workspace(project_id, window, cx);
        }
        self.show_view(View::Files, window, cx);
        // Revealed only while the Files view still shows the folder that was searched.
        let Some(view) = self.files_at(project_id, target) else {
            return;
        };
        let requests = view.reveal(&path, line);
        let at = view.rows().iter().position(
            |row| matches!(row, files_view::Row::Entry { entry, .. } if entry.path == path),
        );
        if let Some(at) = at {
            self.files_scroll
                .scroll_to_item(at, ScrollStrategy::Nearest);
        }
        self.files_requests(project_id, requests, window, cx);
        self.show_pending_line(project_id, target);
    }

    /// The command palette (Ctrl+Shift+P): the commands that can act where the keyboard is,
    /// palette-only actions and "Go to" items.
    fn open_palette(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let workspace = self.workspace;
        let terminal = self.focused_session().filter(|session| {
            self.panes
                .get(session)
                .is_some_and(|pane| pane.read(cx).has_focus(window, cx))
        });
        let discard = workspace
            .and_then(|id| self.git.get(&id))
            .is_some_and(|view| {
                view.selected
                    .as_ref()
                    .is_some_and(|target| view.discard(target).is_some())
            });
        let task_changes = self
            .selected_card
            .and_then(|id| self.board.task(id))
            .is_some_and(|task| {
                Some(task.project_id) == workspace
                    && task.status == TaskStatus::Review
                    && task.checkpoint.is_some()
            });
        let fan_out = self.selected_card.is_some_and(|id| {
            self.board
                .task(id)
                .is_some_and(|task| Some(task.project_id) == workspace)
                && self.board.can_fan_out(id, self.sidebar.sessions())
                && !self.starting.contains(&id)
        });
        let run_state = workspace.map(|id| self.run_summary(id)).unwrap_or_default();
        let context = palette::Context {
            workspace: workspace.is_some(),
            view: self.view(),
            workspaces: self.sidebar.projects().len(),
            tabs: workspace
                .and_then(|id| self.tabs.get(&id))
                .map_or(0, |tabs| tabs.tabs().len()),
            terminal: terminal.is_some(),
            discard,
            task_changes,
            fan_out,
            agents: workspace.is_some_and(|id| broadcast::has_targets(id, self.sidebar.sessions())),
            agent_pane: self.focused_agent().is_some(),
            run_stopped: workspace.is_some()
                && (run_state.rows.is_empty() || run_state.live() < run_state.rows.len()),
            run_live: run_state.any_live(),
        };
        let mut entries = palette::settings_error(self.settings_error.as_deref());
        entries.extend(palette::keymap_errors(&self.keymap, &self.keymap_errors));
        entries.extend(palette::entries(
            &self.keymap,
            &context,
            self.sidebar.projects(),
            self.sidebar.sessions(),
            self.board.tasks(),
        ));
        self.workspace_menu = None;
        self.palette = Some(Palette::new(entries));
        self.palette_return = window.focused(cx).map(|focus| (focus, terminal));
        self.palette_scroll.scroll_to_item(0);
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    /// Closes the palette; the keyboard goes back to where it was (unless that pane went away
    /// meanwhile).
    fn close_palette(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.palette = None;
        match self.palette_return.take() {
            Some((focus, session))
                if session.is_none_or(|session| self.panes.contains_key(&session)) =>
            {
                window.focus(&focus, cx);
            }
            _ => self.focus_pane(window, cx),
        }
        cx.notify();
    }

    /// Typing filters, ↑ / ↓ move the pick (kept in view), Enter runs it, Esc closes.
    fn palette_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = quick_open::key(&event.keystroke) else {
            return;
        };
        let paste = (key == quick_open::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(palette) = self.palette.as_mut() else {
            return;
        };
        match key {
            quick_open::Key::Close => return self.close_palette(window, cx),
            quick_open::Key::Open => return self.run_palette_pick(window, cx),
            quick_open::Key::Paste => {
                if let Some(text) = paste {
                    palette.insert(&text);
                }
            }
            quick_open::Key::Move(_)
            | quick_open::Key::Backspace
            | quick_open::Key::DeleteWord
            | quick_open::Key::SelectAll
            | quick_open::Key::Insert(_) => {
                palette.key(key);
            }
        }
        self.palette_scroll.scroll_to_item(palette.selected);
        cx.notify();
    }

    /// Runs the palette's pick. A command goes through the shell's `run` from where the
    /// keyboard was (terminal commands reach their pane; Git ones need the Git list).
    fn run_palette_pick(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(action) = self.palette.as_ref().and_then(Palette::picked) else {
            return;
        };
        self.close_palette(window, cx);
        let workspace = self.workspace;
        match action {
            palette::Action::Run(command) => {
                if self.view() == View::Git
                    && command.context().is_some_and(|context| {
                        context == GIT_CONTEXT || context == COMMENTS_CONTEXT
                    })
                {
                    window.focus(&self.git_focus, cx);
                }
                // Deferred; gpui-pre captures the focus just restored now and resolves it in
                // the last rendered frame (root if gone), so terminal commands reach the pane.
                window.dispatch_action(Box::new(Run(command)), cx);
            }
            palette::Action::Extra(Extra::OpenSettings) => self.open_settings(cx),
            palette::Action::Extra(extra) => {
                let Some(project_id) = workspace else {
                    return;
                };
                match extra {
                    Extra::GitFetch => self.run_git_remote(project_id, Remote::Fetch, window, cx),
                    Extra::GitPush => self.run_git_remote(project_id, Remote::Push, window, cx),
                    Extra::GitDiscard => {
                        let target = self
                            .git
                            .get(&project_id)
                            .and_then(|view| view.selected.clone());
                        if let Some(target) = target {
                            self.open_git_discard(project_id, target, window, cx);
                        }
                    }
                    Extra::ViewTaskChanges => {
                        if let Some(task_id) = self.selected_card {
                            self.view_task_changes(task_id, window, cx);
                        }
                    }
                    Extra::FanOut => {
                        if let Some(task_id) = self.selected_card {
                            self.open_fan_out(task_id, window, cx);
                        }
                    }
                    Extra::Broadcast => self.open_broadcast(project_id, window, cx),
                    // Into the open composer, else one opened on the focused agent pane.
                    Extra::InsertPrompt => {
                        if let Some(session) = self.focused_agent() {
                            if self.composers.shown(session).is_none() {
                                self.open_composer(window, cx);
                            }
                            let target = prompt_picker::Target::Composer(session);
                            self.open_prompt_picker(target, window, cx);
                        }
                    }
                    Extra::Turns => {
                        if let Some(session) = self.focused_agent() {
                            self.open_turns(session, window, cx);
                        }
                    }
                    Extra::FollowInZed => {
                        if let Some(session) = self.focused_agent() {
                            self.toggle_follow(session, cx);
                        }
                    }
                    // The list is built on open: the run state may have changed since.
                    Extra::RunStart => {
                        if self.run_summary(project_id).rows.is_empty() {
                            self.open_run_setup(project_id, true, window, cx);
                        } else {
                            self.run_action(project_id, RunAction::Start, None, window, cx);
                        }
                    }
                    Extra::RunRestart => {
                        self.run_action(project_id, RunAction::Restart, None, window, cx)
                    }
                    Extra::RunStop => {
                        self.run_action(project_id, RunAction::Stop, None, window, cx)
                    }
                    Extra::RunSetup => self.open_run_setup(project_id, false, window, cx),
                    Extra::RunShowTab => self.show_run_tab(project_id, window, cx),
                    // Handled above: it needs no workspace.
                    Extra::OpenSettings => self.open_settings(cx),
                }
            }
            palette::Action::GoTo(palette::Target::Workspace(project_id)) => {
                self.activate_workspace(project_id, window, cx);
            }
            palette::Action::GoTo(palette::Target::Session(session_id)) => {
                self.open_session(session_id, window, cx);
            }
            palette::Action::GoTo(palette::Target::Task {
                project_id,
                task_id,
            }) => {
                if workspace != Some(project_id) {
                    self.activate_workspace(project_id, window, cx);
                }
                self.selected_card = Some(task_id);
                self.show_view(View::Board, window, cx);
                cx.notify();
            }
        }
    }

    /// Whether a modal dialog is open (it keeps the keyboard).
    fn modal_open(&self) -> bool {
        self.task_dialog.is_some()
            || self.undo_dialog.is_some()
            || self.worktree_dialog.is_some()
            || self.compare.is_some()
            || self.turns_dialog.is_some()
            || self.git_discard.is_some()
            || self.git_base.is_some()
            || self.broadcast.is_some()
            || self.history.is_some()
            || self.doc_dialog.is_some()
            || self.quick_open.is_some()
            || self.palette.is_some()
            || self.prompt_picker.is_some()
            || self.run_setup.is_some()
    }

    /// Closes every transient popover menu. Called by every bound command.
    fn close_menus(&mut self) {
        self.tab_menu = None;
        self.pane_menu = None;
        self.workspace_menu = None;
        self.file_menu = None;
        self.run_menu = None;
        self.workspace_switcher = false;
        self.run_with = None;
        self.fan_out = None;
    }

    /// The pane keyboard input goes to: the focused pane of the active tab (none while the
    /// board is shown).
    fn focused_session(&self) -> Option<Ulid> {
        if self.view() != View::Terminals {
            return None;
        }
        let tabs = self.tabs.get(&self.workspace?)?;
        tabs.active()?.focused()
    }

    /// Attaches the sessions on screen (live ones, or already attached; none while the window
    /// is minimized or the board is shown) and detaches all others (SPEC §10), then focuses
    /// the focused pane.
    fn sync_panes(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let visible = on_screen(
            window.is_visible(),
            self.workspace
                .filter(|_| self.view() == View::Terminals)
                .and_then(|id| self.tabs.get(&id)),
        );
        for session in self.router.wanted() {
            if visible.contains(&session) {
                continue;
            }
            if self.router.forget(session)
                && let Some(connection) = &self.connection
            {
                detach(connection, session, cx);
            }
            self.panes.remove(&session);
        }
        self.attach_errors.retain(|id, _| visible.contains(id));
        for &session in &visible {
            let skip = self.router.is_wanted(session)
                || self.attach_errors.contains_key(&session)
                || self.is_exited(session);
            if !skip {
                self.attach(session, window, cx);
            }
        }
        self.focus_pane(window, cx);
    }

    /// Platform text input (IME, Unikey) for the field focused by `focus`, fed to `on_key`.
    /// Put it inside the field; only the focused field's handler is registered.
    fn typing(
        &self,
        focus: &FocusHandle,
        on_key: ime::OnKey<Self>,
        cx: &Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        typing_input(focus, on_key, &self.ime_marked, cx)
    }

    fn focus_pane(&self, window: &mut Window, cx: &mut Context<'_, Self>) {
        // A modal dialog keeps the keyboard until it closes; the history's replay pane takes it
        // (copy, find, scrollback), its unbound keys bubble to the dialog.
        if self.history.is_some()
            && let Some(pane) = &self.history_pane
        {
            pane.update(cx, |pane, cx| pane.focus(window, cx));
            return;
        }
        if self.modal_open() {
            window.focus(&self.dialog_focus, cx);
            return;
        }
        match self
            .focused_session()
            .and_then(|session| self.panes.get(&session))
        {
            Some(pane) => pane.update(cx, |pane, cx| pane.focus(window, cx)),
            None => window.focus(&self.focus_handle, cx),
        }
    }

    /// `session.attach`; frames arriving before the reply are buffered by the router.
    fn attach(&mut self, session: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(generation) = self.router.begin_attach(session) else {
            return;
        };
        let attach = connection.call::<SessionAttachResult>(Request::SessionAttach(SessionRef {
            session_id: session,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let attached = attach.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let attached = match attached {
                    Ok(attached) => attached,
                    Err(error) => {
                        if shell.router.fail_attach(session, generation) {
                            tracing::warn!(%session, "session.attach failed: {error:#}");
                            shell.attach_errors.insert(session, format!("{error:#}"));
                            cx.notify();
                        }
                        return;
                    }
                };
                let Some(buffered) = shell.router.finish_attach(session, generation) else {
                    // Hidden meanwhile (and not wanted again, or the router would have kept
                    // this attach): drop the daemon's attachment.
                    if !shell.router.is_wanted(session) {
                        detach(&connection, session, cx);
                    }
                    return;
                };
                let snapshot = buffered.snapshot.unwrap_or(attached.snapshot);
                let painted = shell.painted.clone();
                let base = shell.link_scope(session).map(|(_, _, dir)| dir);
                let pane = cx.new(|cx| {
                    let mut pane =
                        TerminalPane::new(session, &snapshot, connection, painted, window, cx);
                    pane.set_links(base);
                    pane.feed(&buffered.output);
                    if let Some(exit_code) = buffered.exited {
                        pane.set_exited(exit_code);
                    }
                    pane
                });
                cx.subscribe_in(
                    &pane,
                    window,
                    move |shell, _, link: &OpenLink, window, cx| {
                        shell.open_terminal_link(session, link, window, cx);
                    },
                )
                .detach();
                shell.panes.insert(session, pane);
                if shell.focused_session() == Some(session) {
                    shell.focus_pane(window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Picks a folder and adds it as a workspace (`project.add`).
    fn add_workspace(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.sidebar_error = None;
        cx.notify();
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add workspace".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let path = match picked.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) | Err(_) => None,
                Ok(Err(error)) => {
                    let _ = this.update(cx, |shell, cx| {
                        shell.sidebar_error =
                            Some(format!("Cannot open the folder picker: {error:#}"));
                        cx.notify();
                    });
                    None
                }
            };
            let Some(path) = path else {
                return;
            };
            let added = connection
                .call::<Project>(Request::ProjectAdd(ProjectAddParams {
                    path: path.to_string_lossy().into_owned(),
                    name: None,
                }))
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                match added {
                    Ok(project) => {
                        let project_id = project.id;
                        shell.sidebar_error = None;
                        shell.sidebar.add_project(project);
                        shell.activate_workspace(project_id, window, cx);
                    }
                    Err(error) => {
                        shell.sidebar_error = Some(format!("Cannot add the workspace: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Picks a Markdown file and links it as `session_id`'s plan (`session.link_plan`).
    fn pick_plan(&mut self, session_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.plan_error = None;
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Link plan".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let path = match picked.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) | Err(_) => None,
                Ok(Err(error)) => {
                    let _ = this.update(cx, |shell, cx| {
                        shell.plan_error = Some((
                            session_id,
                            format!("Cannot open the file picker: {error:#}"),
                        ));
                        cx.notify();
                    });
                    None
                }
            };
            let Some(path) = path else {
                return;
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let path = path.to_string_lossy().into_owned();
                shell.link_plan(session_id, Some(path), window, cx);
            });
        })
        .detach();
    }

    /// Links `path` as the plan of `session_id`, or unlinks with `None`. The daemon answers with
    /// `session.updated` and `session.plan`; only a refusal is kept here.
    fn link_plan(
        &mut self,
        session_id: Ulid,
        path: Option<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.plan_error = None;
        cx.notify();
        let linked = connection.call::<Session>(Request::SessionLinkPlan(SessionLinkPlanParams {
            session_id,
            path,
        }));
        cx.spawn_in(window, async move |this, cx| {
            if let Err(error) = linked.await {
                let _ = this.update(cx, |shell, cx| {
                    shell.plan_error = Some((session_id, format!("{error:#}")));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Opens the menu of `project_id` (closing any other) and lists the agent profiles for it.
    fn open_workspace_menu(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.sidebar_layout.show();
        self.file_menu = None;
        self.workspace_menu = Some(WorkspaceMenu::new(project_id));
        cx.notify();
        let listed = connection.call::<AgentProfilesResult>(Request::AgentProfiles);
        cx.spawn_in(window, async move |this, cx| {
            let agents = match listed.await {
                Ok(listed) => Agents::Loaded(listed.profiles),
                Err(error) => Agents::Failed(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                // Unless the menu closed (or moved to another workspace) meanwhile.
                if let Some(menu) = shell
                    .workspace_menu
                    .as_mut()
                    .filter(|menu| menu.project_id == project_id)
                {
                    menu.agents = agents;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Opens the folder of `project_id` in Zed; a failure (e.g. `zed` missing) shows in the
    /// sidebar.
    fn open_in_zed(&mut self, project_id: Ulid, cx: &mut Context<'_, Self>) {
        self.workspace_menu = None;
        let Some(project) = self.sidebar.projects().iter().find(|p| p.id == project_id) else {
            return;
        };
        match zed::open(Path::new(&project.path), None) {
            Ok(()) => self.sidebar_error = None,
            Err(error) => {
                tracing::warn!(%project_id, "open in Zed failed: {error}");
                self.sidebar_error = Some(error.to_string());
            }
        }
        cx.notify();
    }

    /// "Open in File Explorer" on a workspace: opens its folder in the OS file manager.
    fn open_workspace_in_explorer(&mut self, project_id: Ulid, cx: &mut Context<'_, Self>) {
        self.workspace_menu = None;
        if let Some(project) = self.sidebar.projects().iter().find(|p| p.id == project_id) {
            cx.open_with_system(Path::new(&project.path));
        }
        cx.notify();
    }

    /// Right click on a Files tree row (`path`) or on the tree itself (`""`).
    fn open_file_menu(
        &mut self,
        project_id: Ulid,
        path: String,
        is_dir: bool,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<'_, Self>,
    ) {
        let target = (self.files.get(&project_id)).and_then(|view| view.worktree_task_id);
        self.close_menus();
        self.file_menu = Some(FileMenu {
            project_id,
            target,
            path,
            is_dir,
            position,
        });
        cx.notify();
    }

    /// "Open in File Explorer" on a Files path: a file is shown selected in its folder, a
    /// folder is opened.
    fn open_path_in_explorer(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: &str,
        is_dir: bool,
        cx: &mut Context<'_, Self>,
    ) {
        self.file_menu = None;
        if let Some(full) = self.target_file(project_id, target, path) {
            if is_dir {
                cx.open_with_system(&full);
            } else {
                cx.reveal_path(&full);
            }
        }
        cx.notify();
    }

    /// "Open in Zed" on a Files path (a file or a folder); a failure shows in the sidebar,
    /// like the workspace menu's.
    fn open_path_in_zed(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: &str,
        cx: &mut Context<'_, Self>,
    ) {
        self.file_menu = None;
        if let Some(full) = self.target_file(project_id, target, path) {
            match zed::open(&full, None) {
                Ok(()) => self.sidebar_error = None,
                Err(error) => {
                    tracing::warn!(path = %full.display(), "open in Zed failed: {error}");
                    self.sidebar_error = Some(error.to_string());
                }
            }
        }
        cx.notify();
    }

    /// Ctrl+,: writes the commented `keymap.toml` template if the file is missing and opens it
    /// in Zed; a failure shows in the sidebar. Changes apply after a restart.
    fn open_keymap_settings(&mut self, cx: &mut Context<'_, Self>) {
        let opened = keymap::ensure_file(&self.keymap_file, &keymap::template())
            .map_err(|error| format!("Cannot create keymap.toml: {error}"))
            .and_then(|()| zed::open(&self.keymap_file, None).map_err(|error| error.to_string()));
        match opened {
            Ok(()) => self.sidebar_error = None,
            Err(error) => {
                tracing::warn!(path = %self.keymap_file.display(), "open keymap settings failed: {error}");
                self.sidebar_error = Some(error);
            }
        }
        cx.notify();
    }

    /// Palette "Global: Open settings": writes the commented `settings.toml` template if the
    /// file is missing and opens it in Zed; fonts apply on save (ADR 0024).
    fn open_settings(&mut self, cx: &mut Context<'_, Self>) {
        let opened = keymap::ensure_file(&self.settings_file, mkrew_core::settings::template())
            .map_err(|error| format!("Cannot create settings.toml: {error}"))
            .and_then(|()| zed::open(&self.settings_file, None).map_err(|error| error.to_string()));
        match opened {
            Ok(()) => self.sidebar_error = None,
            Err(error) => {
                tracing::warn!(path = %self.settings_file.display(), "open settings failed: {error}");
                self.sidebar_error = Some(error);
            }
        }
        cx.notify();
    }

    /// `ToggleInspector` / the status-bar button: opens or closes the session panel; from
    /// another view it shows Terminals with the panel open (the panel lives there only).
    fn toggle_inspector(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.workspace.is_none() {
            return;
        }
        if self.view() != View::Terminals {
            self.inspector_open = true;
            self.show_view(View::Terminals, window, cx);
        } else {
            self.inspector_open = !self.inspector_open;
            if !self.inspector_open {
                // The grip is gone: no mouse-up will end a drag in progress.
                self.end_inspector_drag();
            }
        }
        cx.notify();
    }

    /// The edge was released: saves the dragged width as `[ui] inspector_width` (one write per
    /// drag; the file is hand-edited config). A file that does not parse is not written; the
    /// palette lists why (§20.0).
    fn end_inspector_drag(&mut self) {
        if !std::mem::take(&mut self.inspector_dragging) {
            return;
        }
        // Clamped to 240–560 by `width_from_pointer`, so it fits a u16.
        let width = (self.inspector_width.round() as u16).to_string();
        if let Err(error) =
            mkrew_core::settings::write_ui_value(&self.settings_file, "inspector_width", &width)
        {
            self.settings_error = Some(error.to_string());
        }
    }

    /// Cycles System → Light → Dark (ADR 0027) and saves it as `[ui] theme`. A settings file
    /// that does not parse is not written; the mode still changes for this session and the
    /// palette lists why (§20.0).
    fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let mode = theme::next_mode(cx.theme().mode);
        theme::refresh(mode, window, cx);
        cx.notify();
        let value = format!("\"{}\"", mode.name());
        if let Err(error) =
            mkrew_core::settings::write_ui_value(&self.settings_file, "theme", &value)
        {
            self.settings_error = Some(error.to_string());
        }
    }

    /// Makes `mode` the diff layout (every file and repo) and keeps the current hunk in view
    /// (the two layouts have different row counts). False when nothing changed.
    fn apply_diff_mode(&mut self, mode: DiffMode) -> bool {
        if self.diff_mode == mode {
            return false;
        }
        self.diff_mode = mode;
        let hunk_row = self
            .workspace
            .and_then(|project_id| self.git.get(&project_id))
            .and_then(|view| match (&view.diff, view.hunk) {
                (git_view::Diff::Shown(shown), Some(hunk)) => shown.hunk_row(hunk, mode),
                _ => None,
            });
        self.git_diff_scroll
            .scroll_to_item(hunk_row.unwrap_or(0), ScrollStrategy::Top);
        true
    }

    /// Switches the Git diff viewer to `mode` and saves `[ui] diff_mode`. A settings file that
    /// does not parse keeps the choice for this run; the palette says why (§20.0).
    fn set_diff_mode(&mut self, mode: DiffMode, cx: &mut Context<'_, Self>) {
        if !self.apply_diff_mode(mode) {
            return;
        }
        if let Err(error) = mkrew_core::settings::write_ui_value(
            &self.settings_file,
            "diff_mode",
            &format!("\"{}\"", mode.name()),
        ) {
            self.settings_error = Some(error.to_string());
        }
        cx.notify();
    }

    /// The prompt picker's empty state: opens the prompts folder (created if missing) in Zed
    /// to add Markdown prompts; a failure shows in the sidebar.
    fn open_prompts_dir(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let opened = std::fs::create_dir_all(&self.prompts_dir)
            .map_err(|error| format!("Cannot create the prompts folder: {error}"))
            .and_then(|()| zed::open(&self.prompts_dir, None).map_err(|error| error.to_string()));
        match opened {
            Ok(()) => self.sidebar_error = None,
            Err(error) => {
                tracing::warn!(path = %self.prompts_dir.display(), "open prompts folder failed: {error}");
                self.sidebar_error = Some(error);
            }
        }
        self.close_prompt_picker(None, window, cx);
    }

    /// "Remove workspace…": asks, then `project.archive` (its sessions stop; its folder, tasks
    /// and history stay; adding the folder again brings it back). The sidebar drops it on
    /// `project.updated`; a refresh moves to another workspace if it was shown.
    fn archive_workspace(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        cx.notify();
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(name) = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .map(|project| project.name.clone())
        else {
            return;
        };
        let live = (self.sidebar.sessions().iter())
            .filter(|s| s.project_id == project_id && s.state != SessionState::Exited)
            .count();
        let stops = match live {
            0 => String::new(),
            1 => "Its running session stops. ".to_owned(),
            n => format!("Its {n} running sessions stop. "),
        };
        let detail = format!(
            "{stops}The folder, its tasks and history stay. Add the folder again to bring it back."
        );
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Remove “{name}” from Mkrew?"),
            Some(detail.as_str()),
            &workspace_menu::REMOVE_ANSWERS,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if !task_dialog::confirms_delete(answer.await.ok()) {
                return;
            }
            let archived = connection
                .call::<Project>(Request::ProjectArchive(ProjectRef { project_id }))
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                match archived {
                    Ok(_) => {
                        shell.sidebar_error = None;
                        shell.refresh(window, cx);
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "project.archive failed: {error:#}");
                        shell.sidebar_error =
                            Some(format!("Cannot remove the workspace: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Makes `agent` the default agent of `project_id`; the sidebar picks the change up from
    /// `project.updated`.
    fn set_default_agent(
        &mut self,
        project_id: Ulid,
        agent: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let params = workspace_menu::set_default_agent(project_id, &agent);
        self.update_project(params, "Cannot change the default agent", window, cx);
    }

    /// Makes `shell` the default shell of `project_id` (ADR 0032); the sidebar picks the change
    /// up from `project.updated`.
    fn set_default_shell(
        &mut self,
        project_id: Ulid,
        shell: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let params = workspace_menu::set_default_shell(project_id, &shell);
        self.update_project(params, "Cannot change the default shell", window, cx);
    }

    /// Sets (or clears) the workspace color from the menu.
    fn set_workspace_color(
        &mut self,
        project_id: Ulid,
        color: Option<&'static str>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let params = workspace_menu::set_color(project_id, color);
        self.update_project(params, "Cannot change the color", window, cx);
    }

    /// "Pin" / "Unpin" in the workspace menu: `project.update` with `pinned`; the
    /// `project.updated` event re-sorts the sidebar.
    fn pin_workspace(
        &mut self,
        project_id: Ulid,
        pinned: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let params = workspace_menu::set_pinned(project_id, pinned);
        self.update_project(params, "Cannot pin the workspace", window, cx);
    }

    /// Closes the workspace menu and sends `project.update`; a failure shows in the sidebar
    /// after `failure`.
    fn update_project(
        &mut self,
        params: ProjectUpdateParams,
        failure: &'static str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        cx.notify();
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let project_id = params.project_id;
        let updated = connection.call::<Project>(Request::ProjectUpdate(params));
        cx.spawn_in(window, async move |this, cx| {
            let updated = updated.await;
            let _ = this.update(cx, |shell, cx| {
                match updated {
                    Ok(_) => shell.sidebar_error = None,
                    Err(error) => {
                        tracing::warn!(%project_id, "project.update failed: {error:#}");
                        shell.sidebar_error = Some(format!("{failure}: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Broadcast to agents…" over the live agent sessions of `project_id`, all checked.
    fn open_broadcast(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        self.broadcast = Some(Broadcast::new(project_id, self.sidebar.sessions()));
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    fn close_broadcast(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.broadcast = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    /// Typing edits the text, ↑ / ↓ move between sessions, Tab checks one, Enter sends, Esc
    /// closes.
    fn broadcast_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(dialog) = self.broadcast.as_mut() else {
            return;
        };
        match dialog.key(&event.keystroke) {
            broadcast::Outcome::Close => return self.close_broadcast(window, cx),
            broadcast::Outcome::Send => return self.send_broadcast(window, cx),
            broadcast::Outcome::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    dialog.insert(&text);
                }
            }
            broadcast::Outcome::Edited => {}
        }
        cx.notify();
    }

    /// One `session.send` per checked session; each row shows its own result. A session that
    /// is working gets the text queued by the daemon (SPEC §8), shown as "queued".
    fn send_broadcast(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let serial = self.dialog_serial;
        let Some(dialog) = self.broadcast.as_mut() else {
            return;
        };
        for params in dialog.begin() {
            let session_id = params.session_id;
            let sent = connection.call::<SessionSendResult>(Request::SessionSend(params));
            cx.spawn_in(window, async move |this, cx| {
                let result = sent
                    .await
                    .map(|sent| sent.queued)
                    .map_err(|error| format!("{error:#}"));
                let _ = this.update(cx, |shell, cx| {
                    if let Err(error) = &result {
                        tracing::warn!(%session_id, "broadcast session.send failed: {error}");
                    }
                    if shell.dialog_serial != serial {
                        return;
                    }
                    if let Some(dialog) = shell.broadcast.as_mut() {
                        dialog.sent(session_id, result);
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    /// Shows the modal task dialog (replacing any open one) and lists the agent profiles for
    /// its picker.
    fn open_task_dialog(
        &mut self,
        dialog: TaskDialog,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.workspace_menu = None;
        self.undo_dialog = None;
        self.task_dialog = Some(dialog);
        self.dialog_serial += 1;
        let serial = self.dialog_serial;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
        let listed = connection.call::<AgentProfilesResult>(Request::AgentProfiles);
        cx.spawn_in(window, async move |this, cx| {
            let agents = match listed.await {
                Ok(listed) => Agents::Loaded(listed.profiles),
                Err(error) => Agents::Failed(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                if let Some(dialog) = shell.open_dialog(serial) {
                    dialog.agents = agents;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The open task dialog, if it is still the one opened as `serial`.
    fn open_dialog(&mut self, serial: u64) -> Option<&mut TaskDialog> {
        self.task_dialog
            .as_mut()
            .filter(|_| self.dialog_serial == serial)
    }

    fn close_task_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.task_dialog = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    /// Keys typed in the task dialog edit the focused field; Enter (title) or Ctrl+Enter
    /// saves, Esc cancels.
    fn task_dialog_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = task_dialog::dialog_key(&event.keystroke) else {
            return;
        };
        let paste = (key == DialogKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(dialog) = self.task_dialog.as_mut() else {
            return;
        };
        if let Some(text) = paste {
            dialog.insert(&text);
        }
        match dialog.key(key) {
            task_dialog::Outcome::Edited => {
                if dialog.field == Field::Prompt {
                    self.prompt_scroll.scroll_to_bottom();
                }
                cx.notify();
            }
            task_dialog::Outcome::Cancel => self.close_task_dialog(window, cx),
            task_dialog::Outcome::Save => self.save_task(window, cx),
        }
    }

    /// `task.create` / `task.update` for the dialog's draft; the dialog closes once it is
    /// saved and shows the error otherwise.
    fn save_task(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let serial = self.dialog_serial;
        let Some(dialog) = self.task_dialog.as_mut().filter(|dialog| !dialog.busy) else {
            return;
        };
        // Diff against the task as it is now (`mk` or a run may have changed it meanwhile).
        if let Some(task) = dialog.task_id().and_then(|id| self.board.task(id)) {
            dialog.mode = task_dialog::Mode::Edit(Box::new(task.clone()));
        }
        let request = match dialog.save_request() {
            Ok(Some(request)) => request,
            Ok(None) => return self.close_task_dialog(window, cx),
            Err(invalid) => {
                dialog.error = Some(invalid.to_owned());
                cx.notify();
                return;
            }
        };
        dialog.busy = true;
        dialog.error = None;
        cx.notify();
        let saved = connection.call::<Task>(request);
        cx.spawn_in(window, async move |this, cx| {
            let saved = saved.await;
            let _ = this.update_in(cx, |shell, window, cx| match saved {
                Ok(task) => {
                    // Shown at once, without waiting for the `task.updated` echo.
                    shell.board.apply(&Event::TaskUpdated(task));
                    if shell.open_dialog(serial).is_some() {
                        shell.close_task_dialog(window, cx);
                    }
                }
                Err(error) => {
                    tracing::warn!("saving a task failed: {error:#}");
                    if let Some(dialog) = shell.open_dialog(serial) {
                        dialog.busy = false;
                        dialog.error = Some(format!("Cannot save the task: {error:#}"));
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Asks, then `task.delete`s the task being edited. The daemon refuses while its session
    /// still runs; the dialog shows why.
    fn delete_task(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(task_id) = self
            .task_dialog
            .as_ref()
            .filter(|dialog| !dialog.busy)
            .and_then(TaskDialog::task_id)
        else {
            return;
        };
        let serial = self.dialog_serial;
        let answer = window.prompt(
            PromptLevel::Warning,
            "Delete this task?",
            Some("The card and its prompt are removed. Its past sessions stay in the sidebar."),
            &task_dialog::DELETE_ANSWERS,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if !task_dialog::confirms_delete(answer.await.ok()) {
                return;
            }
            let deleted = this.update(cx, |shell, cx| {
                let dialog = shell.open_dialog(serial).filter(|dialog| !dialog.busy)?;
                dialog.busy = true;
                dialog.error = None;
                cx.notify();
                Some(connection.request(Request::TaskDelete(TaskRef { task_id })))
            });
            let Ok(Some(deleted)) = deleted else {
                return;
            };
            let failure = match deleted.await {
                Ok(Outcome::Result(_)) => None,
                Ok(Outcome::Error(body)) => Some(body.message),
                Err(error) => Some(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| match failure {
                None => {
                    if shell.open_dialog(serial).is_some() {
                        shell.close_task_dialog(window, cx);
                    }
                }
                Some(error) => {
                    tracing::warn!(%task_id, "task.delete failed: {error}");
                    if let Some(dialog) = shell.open_dialog(serial) {
                        dialog.busy = false;
                        dialog.error = Some(format!("Cannot delete the task: {error}"));
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// "Undo" on a Review / Done card: shows the modal confirmation and asks the daemon what
    /// undoing would discard (`task.undo_preview`).
    fn open_undo_dialog(&mut self, task_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(task) = self
            .board
            .task(task_id)
            .filter(|_| self.connection.is_some())
        else {
            return;
        };
        self.workspace_menu = None;
        self.task_dialog = None;
        self.undo_dialog = Some(UndoDialog::new(task));
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        self.load_undo_preview(window, cx);
    }

    /// (Re)reads the open undo dialog's preview.
    fn load_undo_preview(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(dialog) = self.undo_dialog.as_mut() else {
            return;
        };
        let serial = self.dialog_serial;
        dialog.preview = Preview::Loading;
        cx.notify();
        let previewed = connection.call::<UndoPreview>(dialog.preview_request());
        cx.spawn_in(window, async move |this, cx| {
            let preview = match previewed.await {
                Ok(preview) => Preview::loaded(preview),
                Err(error) => Preview::Failed(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                if let Some(dialog) = shell.open_undo(serial) {
                    dialog.preview = preview;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The open undo dialog, if it is still the one opened as `serial`.
    fn open_undo(&mut self, serial: u64) -> Option<&mut UndoDialog> {
        self.undo_dialog
            .as_mut()
            .filter(|_| self.dialog_serial == serial)
    }

    /// Closes the undo dialog, back to the Turns dialog it was opened from, if any.
    fn close_undo_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.undo_dialog = None;
        self.turns_dialog = self.turns_return.take();
        self.focus_pane(window, cx);
        cx.notify();
    }

    fn undo_dialog_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        if undo_dialog::cancels(&event.keystroke) {
            self.close_undo_dialog(window, cx);
        }
    }

    /// The destructive confirm: `task.undo` / `turn.restore` with exactly the previewed files.
    /// On failure the dialog shows why and the preview is read again (the daemon refuses a
    /// stale one).
    fn confirm_undo(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let serial = self.dialog_serial;
        let Some(dialog) = self.undo_dialog.as_mut() else {
            return;
        };
        let Ok(request) = dialog.confirm_request(self.sidebar.sessions()) else {
            return;
        };
        let target = dialog.target.clone();
        dialog.busy = true;
        dialog.error = None;
        cx.notify();
        let undone = connection.request(request);
        cx.spawn_in(window, async move |this, cx| {
            let failure = match undone.await {
                Ok(Outcome::Result(_)) => None,
                Ok(Outcome::Error(body)) => Some(body.message),
                Err(error) => Some(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| match failure {
                None => {
                    if shell.open_undo(serial).is_some() {
                        // Done: back to the pane, not to the Turns dialog it may have left.
                        shell.turns_return = None;
                        shell.close_undo_dialog(window, cx);
                    }
                }
                Some(error) => {
                    tracing::warn!(?target, "undo failed: {error}");
                    if let Some(dialog) = shell.open_undo(serial) {
                        dialog.undo_failed(&error);
                        shell.load_undo_preview(window, cx);
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// "Merge back…" / "Remove worktree…" on a Review / Done card (SPEC §20E.1): shows the
    /// modal confirmation and reads what it lists.
    fn open_worktree_dialog(
        &mut self,
        kind: worktree_dialog::Kind,
        task_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(dialog) = self
            .board
            .task(task_id)
            .filter(|_| self.connection.is_some())
            .and_then(|task| WorktreeDialog::new(kind, task))
        else {
            return;
        };
        self.show_worktree_dialog(dialog, window, cx);
    }

    fn show_worktree_dialog(
        &mut self,
        dialog: WorktreeDialog,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        self.task_dialog = None;
        self.undo_dialog = None;
        self.compare = None;
        self.worktree_dialog = Some(dialog);
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        self.load_worktree_dialog(window, cx);
    }

    /// "Compare" on a group card whose children are all in Review (SPEC §20E.2): the modal
    /// with one column per child, each reading its changes since its checkpoint.
    fn open_compare(&mut self, original_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(compare) = self.board.task(original_id).and_then(|original| {
            Compare::new(original, self.board.tasks(), self.sidebar.sessions())
        }) else {
            return;
        };
        let requests = compare.status_requests();
        self.workspace_menu = None;
        self.task_dialog = None;
        self.undo_dialog = None;
        self.worktree_dialog = None;
        self.compare = Some(compare);
        self.dialog_serial += 1;
        let serial = self.dialog_serial;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
        for (task_id, request) in requests {
            let status = connection.call::<GitStatus>(request);
            cx.spawn_in(window, async move |this, cx| {
                let status = status.await.map_err(|error| format!("{error:#}"));
                let _ = this.update(cx, |shell, cx| {
                    if let Some(compare) = shell.compare.as_mut()
                        && shell.dialog_serial == serial
                    {
                        compare.counted(task_id, status);
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }

    fn close_compare(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.compare = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    fn compare_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        if undo_dialog::cancels(&event.keystroke) {
            self.close_compare(window, cx);
        }
    }

    /// "Git view" of a Compare column: the child's changes since its checkpoint, on its
    /// worktree (as the card's "Changes").
    fn compare_git(&mut self, task_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.compare = None;
        self.view_task_changes(task_id, window, cx);
    }

    /// "Pick" of a Compare column: the Merge back dialog of that child, which then moves the
    /// original to Review and offers removing the others' worktrees.
    fn compare_pick(&mut self, task_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(pick) = self
            .compare
            .as_ref()
            .and_then(|compare| compare.pick(task_id))
        else {
            return;
        };
        let Some(dialog) = self
            .board
            .task(task_id)
            .and_then(|task| WorktreeDialog::for_pick(task, pick))
        else {
            return;
        };
        self.show_worktree_dialog(dialog, window, cx);
    }

    /// The Pick's offer: `task.remove_worktree` of every other child; each answer is listed.
    fn remove_other_worktrees(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let serial = self.dialog_serial;
        let sessions = self.sidebar.sessions();
        let Some(pick) = self
            .worktree_dialog
            .as_mut()
            .and_then(|dialog| dialog.pick.as_mut())
        else {
            return;
        };
        let requests = pick.start_removal(sessions);
        cx.notify();
        for (task_id, request) in requests {
            let sent = connection.call::<RemoveWorktreeResult>(request);
            cx.spawn_in(window, async move |this, cx| {
                let result = sent.await.map_err(|error| format!("{error:#}"));
                let _ = this.update(cx, |shell, cx| {
                    if let Err(error) = &result {
                        tracing::warn!(%task_id, "task.remove_worktree failed: {error}");
                    }
                    if let Some(pick) = shell
                        .open_worktree(serial)
                        .and_then(|dialog| dialog.pick.as_mut())
                    {
                        pick.removed(task_id, result);
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }

    /// (Re)reads the open worktree dialog's plan: the worktree's status (and, to merge, the
    /// checkout's status and both logs for the commit list).
    fn load_worktree_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(dialog) = self.worktree_dialog.as_mut() else {
            return;
        };
        let serial = self.dialog_serial;
        dialog.step = worktree_dialog::Step::Loading;
        cx.notify();
        let kind = dialog.kind;
        let worktree = connection.call::<GitStatus>(dialog.status_request(true));
        let merge = (kind == worktree_dialog::Kind::Merge).then(|| {
            (
                connection.call::<GitStatus>(dialog.status_request(false)),
                connection.call::<Vec<Commit>>(dialog.log_request(true)),
                connection.call::<Vec<Commit>>(dialog.log_request(false)),
            )
        });
        cx.spawn_in(window, async move |this, cx| {
            let read = async {
                let worktree = worktree.await?;
                let Some((checkout, worktree_log, checkout_log)) = merge else {
                    return anyhow::Ok(worktree_dialog::Step::Remove(
                        worktree_dialog::remove_plan(&worktree),
                    ));
                };
                let (checkout, worktree_log) = (checkout.await?, worktree_log.await?);
                Ok(worktree_dialog::Step::Merge(worktree_dialog::merge_plan(
                    worktree_log,
                    &checkout_log.await?,
                    &worktree,
                    &checkout,
                )))
            };
            let step = read
                .await
                .unwrap_or_else(|error| worktree_dialog::Step::Failed(format!("{error:#}")));
            let _ = this.update(cx, |shell, cx| {
                if let Some(dialog) = shell.open_worktree(serial) {
                    dialog.step = step;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The open worktree dialog, if it is still the one opened as `serial`.
    fn open_worktree(&mut self, serial: u64) -> Option<&mut WorktreeDialog> {
        self.worktree_dialog
            .as_mut()
            .filter(|_| self.dialog_serial == serial)
    }

    fn close_worktree_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.worktree_dialog = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    fn worktree_dialog_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        if undo_dialog::cancels(&event.keystroke) {
            // Esc on the discard question goes back to the list, like "Keep files".
            match self.worktree_dialog.as_mut() {
                Some(dialog) if matches!(dialog.step, worktree_dialog::Step::ConfirmDiscard(_)) => {
                    dialog.keep_files();
                    cx.notify();
                }
                _ => self.close_worktree_dialog(window, cx),
            }
        }
    }

    /// The confirmed step: `task.merge`, `task.remove_worktree`, or the discard before a
    /// removal (then the worktree is read again). A failure shows why and reads it again.
    fn confirm_worktree(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let serial = self.dialog_serial;
        let Some(dialog) = self.worktree_dialog.as_mut() else {
            return;
        };
        let Ok(request) = dialog.confirm_request(self.sidebar.sessions()) else {
            return;
        };
        let task_id = dialog.task_id;
        dialog.busy = true;
        dialog.error = None;
        cx.notify();
        let sent = connection.call::<serde_json::Value>(request);
        cx.spawn_in(window, async move |this, cx| {
            let answer = sent.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let Some(dialog) = shell.open_worktree(serial) else {
                    return;
                };
                let failure = answer.and_then(|value| {
                    match &dialog.step {
                        worktree_dialog::Step::Merge(_) => {
                            dialog.merged(serde_json::from_value(value)?);
                        }
                        worktree_dialog::Step::Remove(_) => {
                            dialog.removed(serde_json::from_value(value)?);
                        }
                        // Discarded: the worktree is listed again.
                        _ => dialog.busy = false,
                    }
                    anyhow::Ok(())
                });
                let reload = match failure {
                    Ok(()) => !matches!(dialog.step, worktree_dialog::Step::Done(_)),
                    Err(error) => {
                        tracing::warn!(%task_id, "worktree action failed: {error:#}");
                        dialog.failed(&format!("{error:#}"));
                        true
                    }
                };
                // A merged Pick: the fan-out original moves to Review (SPEC §20E.2).
                if let Some(original) = dialog.picked_original().filter(|&id| {
                    shell
                        .board
                        .task(id)
                        .is_some_and(|task| task.status != TaskStatus::Review)
                }) {
                    let moves = shell.board.drop_moves(original, TaskStatus::Review, None);
                    shell.move_cards(original, moves, "Cannot move the task", window, cx);
                }
                if reload {
                    shell.load_worktree_dialog(window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Open Git view" of the worktree dialog: the Git view on the task's worktree, to commit
    /// or unstage there.
    fn worktree_dialog_git(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(dialog) = self
            .worktree_dialog
            .take_if(|dialog| self.workspace == Some(dialog.project_id))
        else {
            return;
        };
        let project_id = dialog.project_id;
        self.retarget_git(project_id, Some(dialog.task_id), None);
        if self.view() == View::Git {
            self.refresh_git(project_id, window, cx);
        }
        self.show_view(View::Git, window, cx);
    }

    /// "Follow in Zed" of an agent pane (SPEC §20C.5): turns following its edits on or off.
    /// The wait is read from `settings.toml` when it turns on.
    fn toggle_follow(&mut self, session_id: Ulid, cx: &mut Context<'_, Self>) {
        // A missing or broken settings file waits the default (the daemon reports it).
        let debounce_ms = mkrew_core::settings::load(&self.settings_file)
            .unwrap_or_default()
            .zed
            .follow_debounce_ms;
        self.follow.toggle(session_id, debounce_ms);
        cx.notify();
    }

    /// Opens a followed session's edit in Zed once `wait` passed without a later edit; a
    /// failure is logged (following stays on).
    fn follow_wait(&mut self, wait: follow::Wait, window: &mut Window, cx: &mut Context<'_, Self>) {
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(wait.delay).await;
            let _ = this.update(cx, |shell, cx| {
                let Some(edit) = shell.follow.due(wait) else {
                    return;
                };
                let session_id = wait.session_id;
                let Some(full) = shell
                    .sidebar
                    .sessions()
                    .iter()
                    .find(|s| s.id == session_id)
                    .and_then(|s| shell.workspace_file(s.project_id, &edit.path))
                else {
                    return;
                };
                // The PATH lookup and process spawn stay off the UI thread: this fires on
                // every followed edit, not on a click.
                cx.background_executor()
                    .spawn(async move {
                        if let Err(error) = zed::open(&full, edit.line) {
                            tracing::warn!(
                                %session_id,
                                path = edit.path,
                                "follow in Zed failed: {error}"
                            );
                        }
                    })
                    .detach();
            });
        })
        .detach();
    }

    /// "Turns…" of an agent pane (SPEC §20C.2): lists the session's turns (`turn.list`).
    fn open_turns(&mut self, session_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(session) = self
            .sidebar
            .sessions()
            .iter()
            .find(|s| s.id == session_id && s.kind == SessionKind::Agent)
        else {
            return;
        };
        self.workspace_menu = None;
        self.task_dialog = None;
        self.undo_dialog = None;
        self.turns_dialog = Some(TurnsDialog::new(session));
        self.dialog_serial += 1;
        let serial = self.dialog_serial;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
        let listed = connection.call::<Vec<Turn>>(Request::TurnList(SessionRef { session_id }));
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await;
            let _ = this.update(cx, |shell, cx| {
                let Some(dialog) = shell
                    .turns_dialog
                    .as_mut()
                    .filter(|_| shell.dialog_serial == serial)
                else {
                    return;
                };
                match listed {
                    Ok(turns) => dialog.loaded(turns),
                    Err(error) => {
                        dialog.listing = turns_dialog::Listing::Failed(format!("{error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn close_turns_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.turns_dialog = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    fn turns_dialog_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        match turns_dialog::key(&event.keystroke) {
            Some(turns_dialog::Key::Close) => self.close_turns_dialog(window, cx),
            Some(turns_dialog::Key::Move(offset)) => {
                if let Some(dialog) = self.turns_dialog.as_mut() {
                    dialog.select_by(offset);
                    cx.notify();
                }
            }
            Some(turns_dialog::Key::Restore) => {
                let Some(dialog) = &self.turns_dialog else {
                    return;
                };
                let state = self.session_state(dialog.session_id);
                let restorable = match &dialog.listing {
                    turns_dialog::Listing::Loaded(turns) => turns
                        .get(dialog.selected)
                        .is_some_and(|turn| turns_dialog::restore_blocked(turn, state).is_none()),
                    _ => false,
                };
                if let Some(n) = dialog.selected_turn().filter(|_| restorable) {
                    self.restore_before_turn(n, window, cx);
                }
            }
            None => {}
        }
    }

    /// `session_id`'s state (exited once it is no longer listed).
    fn session_state(&self, session_id: Ulid) -> SessionState {
        self.sidebar
            .sessions()
            .iter()
            .find(|s| s.id == session_id)
            .map_or(SessionState::Exited, |s| s.state)
    }

    /// "Restore before turn n": replaces the Turns dialog with the undo dialog's preview and
    /// confirm (`turn.restore_preview` / `turn.restore`).
    fn restore_before_turn(&mut self, n: u32, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(dialog) = self.turns_dialog.take() else {
            return;
        };
        let Some(session) = self
            .sidebar
            .sessions()
            .iter()
            .find(|s| s.id == dialog.session_id)
        else {
            self.close_turns_dialog(window, cx);
            return;
        };
        self.undo_dialog = Some(UndoDialog::for_turn(session, n));
        self.turns_return = Some(dialog);
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        self.load_undo_preview(window, cx);
    }

    /// "Diff since" on a Turns dialog row (SPEC §20C.3): closes the dialog and shows the Git
    /// view since the turn's start tree.
    fn diff_since_turn(&mut self, n: u32, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(dialog) = self.turns_dialog.take() else {
            return;
        };
        cx.notify();
        let tree = match &dialog.listing {
            turns_dialog::Listing::Loaded(turns) => turns
                .iter()
                .find(|turn| turn.n == n)
                .and_then(|turn| turn.start_tree.clone()),
            _ => None,
        };
        let sessions = self.sidebar.sessions();
        // The Git view shows the active workspace's changes.
        let session = sessions
            .iter()
            .find(|s| s.id == dialog.session_id && Some(s.project_id) == self.workspace);
        let since = tree.zip(session).map(|(tree, session)| {
            let since = Since::turn(session, sessions, n, tree);
            (session.project_id, session.task_id, since)
        });
        match since {
            Some((project_id, task_id, since)) => {
                // A worktree task's turns ran in its worktree: compared there.
                let target = task_id.filter(|&id| {
                    self.board
                        .task(id)
                        .is_some_and(|task| task.worktree_path.is_some())
                });
                self.retarget_git(project_id, target, None);
                self.show_git_since(project_id, since, window, cx);
            }
            None => self.focus_pane(window, cx),
        }
    }

    /// "History" in the tab bar: lists the workspace's finished sessions (`session.list`) and
    /// replays the newest one's log.
    fn show_history(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(project) = self.sidebar.projects().iter().find(|p| p.id == project_id) else {
            return;
        };
        self.workspace_menu = None;
        self.task_dialog = None;
        self.undo_dialog = None;
        self.history = Some(HistoryDialog::new(project_id, project.name.clone()));
        self.history_pane = None;
        self.dialog_serial += 1;
        let serial = self.dialog_serial;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
        let listed = connection.call::<Vec<Session>>(Request::SessionList(SessionListParams {
            project_id: Some(project_id),
        }));
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let Some(dialog) = shell.open_history(serial) else {
                    return;
                };
                match listed {
                    Ok(sessions) => {
                        if let Some(session) = dialog.load(sessions) {
                            shell.load_history_log(session, window, cx);
                        }
                    }
                    Err(error) => dialog.listing = Listing::Failed(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The open history, if it is still the one opened as `serial`.
    fn open_history(&mut self, serial: u64) -> Option<&mut HistoryDialog> {
        self.history
            .as_mut()
            .filter(|_| self.dialog_serial == serial)
    }

    /// Selects `session` in the open history and replays the tail of its log
    /// (`session.history`, files read off the UI thread).
    fn select_history(&mut self, session: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self
            .history
            .as_mut()
            .is_some_and(|dialog| dialog.select(session))
        {
            self.load_history_log(session, window, cx);
        }
    }

    /// Reads the selected `session`'s log into a new replay pane.
    fn load_history_log(&mut self, session: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let serial = self.dialog_serial;
        self.history_pane = None;
        // The dialog keeps the keyboard while no pane shows.
        window.focus(&self.dialog_focus, cx);
        cx.notify();
        let files = connection.call::<SessionHistory>(Request::SessionHistory(SessionRef {
            session_id: session,
        }));
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let tail = match files.await {
                Ok(history) => background
                    .spawn(async move { history::read_tail(&history.files, history::TAIL_BYTES) })
                    .await
                    .map_err(|error| format!("Cannot read the log: {error}")),
                Err(error) => Err(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let Some(dialog) = shell.open_history(serial) else {
                    return;
                };
                let Some(selected) = dialog
                    .selected_session()
                    .filter(|selected| selected.id == session)
                    .cloned()
                else {
                    // Another session was picked meanwhile.
                    return;
                };
                match tail {
                    Ok(tail) if tail.bytes.is_empty() => dialog.log = Log::Empty,
                    Ok(tail) => {
                        dialog.log = Log::Shown { cut: tail.cut };
                        let painted = shell.painted.clone();
                        let pane = cx.new(|cx| {
                            TerminalPane::replay(
                                &selected,
                                &tail.bytes,
                                connection,
                                painted,
                                window,
                                cx,
                            )
                        });
                        shell.history_pane = Some(pane);
                        shell.focus_pane(window, cx);
                    }
                    Err(error) => {
                        tracing::warn!(%session, "session history failed: {error}");
                        dialog.log = Log::Failed(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn close_history(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.history = None;
        self.history_pane = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    /// Esc closes the history, ↑ / ↓ pick another session (also while its pane has the
    /// keyboard: a finished session's pane passes keys on).
    fn history_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        match history::key(&event.keystroke) {
            Some(history::Key::Close) => self.close_history(window, cx),
            Some(history::Key::Move(offset)) => {
                let next = self
                    .history
                    .as_ref()
                    .and_then(|dialog| dialog.neighbour(offset));
                if let Some(session) = next {
                    self.select_history(session, window, cx);
                }
            }
            None => {}
        }
    }

    /// A card was dropped into the `status` column right before `before` (`None` = at the
    /// end): `task.move` it (and renumber the column if its gap collapsed). Each reply is
    /// shown at once, without waiting for the `task.updated` echo.
    fn drop_card(
        &mut self,
        task_id: Ulid,
        status: TaskStatus,
        before: Option<Ulid>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let moves = self.board.drop_moves(task_id, status, before);
        self.move_cards(task_id, moves, "Cannot move the card", window, cx);
    }

    /// Approve on a Review card: it moves to the end of Done.
    fn approve_task(&mut self, task_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let moves = self.board.approve_moves(task_id);
        self.move_cards(task_id, moves, "Cannot approve the task", window, cx);
    }

    /// "Changes" on a Review card: the Git view with the task's changes since its checkpoint.
    fn view_task_changes(
        &mut self,
        task_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(task) = self.board.task(task_id) else {
            return;
        };
        let Some(checkpoint) = task.checkpoint.clone() else {
            return;
        };
        let project_id = task.project_id;
        // The board only shows the active workspace's cards.
        if self.workspace != Some(project_id) {
            return;
        }
        let since = Since::task(task_id, &task.title, checkpoint);
        // A worktree task's checkpoint is its worktree's HEAD: compared there.
        let target = task.worktree_path.is_some().then_some(task_id);
        self.retarget_git(project_id, target, None);
        self.show_git_since(project_id, since, window, cx);
    }

    /// The Git view listing the changes since `since`'s checkpoint.
    fn show_git_since(
        &mut self,
        project_id: Ulid,
        since: Since,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let requests = self.git_view(project_id).show_since(since);
        self.git_files_scroll.scroll_to_item(0, ScrollStrategy::Top);
        // Showing the Git view refreshes it (the same requests) unless it is shown already.
        if self.view() == View::Git {
            self.git_requests(project_id, requests, window, cx);
        }
        self.show_view(View::Git, window, cx);
    }

    /// Sends the `task.move`s of one action on `task_id`'s card; a failure shows `what` and
    /// the error on its board.
    fn move_cards(
        &mut self,
        task_id: Ulid,
        moves: Vec<TaskMoveParams>,
        what: &'static str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(project_id) = self.board.task(task_id).map(|task| task.project_id) else {
            return;
        };
        let moves: Vec<_> = moves
            .into_iter()
            .map(|params| connection.call::<Task>(Request::TaskMove(params)))
            .collect();
        if moves.is_empty() {
            return;
        }
        // Cleared once per action: a later move of the same batch must not hide a failure.
        self.card_error = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            for moved in moves {
                let moved = moved.await;
                let _ = this.update(cx, |shell, cx| {
                    match moved {
                        Ok(task) => {
                            shell.board.apply(&Event::TaskUpdated(task));
                        }
                        Err(error) => {
                            tracing::warn!(%task_id, "task.move failed: {error:#}");
                            shell.card_error = Some((project_id, format!("{what}: {error:#}")));
                        }
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Run on a Todo card (or Ctrl+Enter on the selected one): `task.run`. The card follows
    /// its session through `task.updated`; clicking it opens the terminal.
    fn run_task(&mut self, task_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(project_id) = self
            .board
            .task(task_id)
            .filter(|_| self.board.can_run(task_id) && !self.starting.contains(&task_id))
            .map(|task| task.project_id)
        else {
            return;
        };
        self.starting.insert(task_id);
        self.card_error = None;
        cx.notify();
        let ran =
            connection.call::<SessionSpawnResult>(Request::TaskRun(lifecycle::run_params(task_id)));
        cx.spawn_in(window, async move |this, cx| {
            let ran = ran.await;
            let _ = this.update(cx, |shell, cx| {
                shell.starting.remove(&task_id);
                if let Err(error) = ran {
                    tracing::warn!(%task_id, "task.run failed: {error:#}");
                    shell.card_error =
                        Some((project_id, format!("Cannot run the task: {error:#}")));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Run with…" on a card: opens its profile picker (closing any other) and lists the agent
    /// profiles for it.
    fn open_run_with(&mut self, task_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.selected_card = Some(task_id);
        self.run_with = Some(RunWith::new(task_id));
        self.fan_out = None;
        cx.notify();
        let listed = connection.call::<AgentProfilesResult>(Request::AgentProfiles);
        cx.spawn_in(window, async move |this, cx| {
            let agents = match listed.await {
                Ok(listed) => Agents::Loaded(listed.profiles),
                Err(error) => Agents::Failed(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                // Unless the picker closed (or moved to another card) meanwhile.
                if let Some(menu) = shell
                    .run_with
                    .as_mut()
                    .filter(|menu| menu.task_id == task_id)
                {
                    menu.agents = agents;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// A profile picked in "Run with…" (SPEC §20B.6): `task.update` of the task's agent, then
    /// `task.run`; the run waits for the update, and a failure stops the sequence and shows on
    /// the board.
    fn run_task_with(
        &mut self,
        task_id: Ulid,
        profile: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.run_with = None;
        cx.notify();
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(task) = self
            .board
            .task(task_id)
            .filter(|_| {
                // A run may have started since the picker opened.
                self.board.can_run_with(task_id, self.sidebar.sessions())
                    && !self.starting.contains(&task_id)
            })
            .cloned()
        else {
            return;
        };
        let project_id = task.project_id;
        let requests = lifecycle::run_with_requests(&task, &profile);
        self.starting.insert(task_id);
        self.card_error = None;
        cx.spawn_in(window, async move |this, cx| {
            let mut failed = None;
            for request in requests {
                let result = match request {
                    Request::TaskUpdate(_) => match connection.call::<Task>(request).await {
                        Ok(updated) => {
                            // Shown at once, without waiting for the `task.updated` echo.
                            let _ = this.update(cx, |shell, cx| {
                                shell.board.apply(&Event::TaskUpdated(updated));
                                cx.notify();
                            });
                            Ok(())
                        }
                        Err(error) => Err(format!("Cannot change the task's agent: {error:#}")),
                    },
                    request => connection
                        .call::<SessionSpawnResult>(request)
                        .await
                        .map(|_| ())
                        .map_err(|error| format!("Cannot run the task: {error:#}")),
                };
                if let Err(error) = result {
                    failed = Some(error);
                    break;
                }
            }
            let _ = this.update(cx, |shell, cx| {
                shell.starting.remove(&task_id);
                if let Some(error) = failed {
                    tracing::warn!(%task_id, "run with {profile} failed: {error}");
                    shell.card_error = Some((project_id, error));
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Run with several agents…" on a card: opens its multi-agent picker (closing any other)
    /// and lists the agent profiles for it.
    fn open_fan_out(&mut self, task_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.selected_card = Some(task_id);
        self.fan_out = Some(FanOutPicker::new(task_id));
        self.run_with = None;
        cx.notify();
        let listed = connection.call::<AgentProfilesResult>(Request::AgentProfiles);
        cx.spawn_in(window, async move |this, cx| {
            let agents = match listed.await {
                Ok(listed) => Agents::Loaded(listed.profiles),
                Err(error) => Agents::Failed(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                // Unless the picker closed (or moved to another card) meanwhile.
                if let Some(menu) = shell
                    .fan_out
                    .as_mut()
                    .filter(|menu| menu.task_id == task_id)
                {
                    menu.agents = agents;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The picker's run button (SPEC §20E.2): `task.fanout` with the ticked agents; the daemon
    /// creates and runs one child task each, and moves the card to Running. A failure shows on
    /// the board.
    fn run_fan_out(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(params) = self.fan_out.as_ref().and_then(FanOutPicker::request) else {
            return;
        };
        self.fan_out = None;
        cx.notify();
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let task_id = params.task_id;
        let Some(project_id) = self
            .board
            .task(task_id)
            .filter(|_| {
                // A run may have started since the picker opened.
                self.board.can_fan_out(task_id, self.sidebar.sessions())
                    && !self.starting.contains(&task_id)
            })
            .map(|task| task.project_id)
        else {
            return;
        };
        self.starting.insert(task_id);
        self.card_error = None;
        let call = connection.call::<Vec<Task>>(Request::TaskFanout(params));
        cx.spawn_in(window, async move |this, cx| {
            let result = call.await;
            let _ = this.update(cx, |shell, cx| {
                shell.starting.remove(&task_id);
                match result {
                    // Shown at once, without waiting for the `task.updated` echoes.
                    Ok(children) => {
                        for child in children {
                            shell.board.apply(&Event::TaskUpdated(child));
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%task_id, "task.fanout failed: {error:#}");
                        shell.card_error = Some((
                            project_id,
                            format!("Cannot run with several agents: {error:#}"),
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A click on a card selects it and shows its session's terminal (none for Todo cards).
    fn card_clicked(
        &mut self,
        task_id: Ulid,
        session: Option<Ulid>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.selected_card = Some(task_id);
        // Cards take no focus: leave another card's follow-up field.
        window.focus(&self.focus_handle, cx);
        cx.notify();
        if let Some(session) = session {
            self.open_session(session, window, cx);
        }
    }

    /// Gives the follow-up field of `task_id`'s card the keyboard (keeping its draft).
    fn open_follow_up(
        &mut self,
        task_id: Ulid,
        session_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.selected_card = Some(task_id);
        if self
            .follow_up
            .as_ref()
            .is_none_or(|draft| draft.task_id != task_id || draft.session_id != session_id)
        {
            self.follow_up = Some(FollowUp::new(task_id, session_id));
        }
        window.focus(&self.follow_up_focus, cx);
        cx.notify();
    }

    /// Whether the follow-up draft's card still shows its field.
    fn follow_up_shown(&self) -> bool {
        self.follow_up.as_ref().is_some_and(|draft| {
            self.board
                .takes_follow_up(draft.task_id, draft.session_id, self.sidebar.sessions())
        })
    }

    fn close_follow_up(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.follow_up = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    /// Keys typed in the follow-up field: Enter is a new line, Ctrl+Enter sends, Esc drops the
    /// draft.
    fn follow_up_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = task_dialog::dialog_key(&event.keystroke) else {
            return;
        };
        let paste = (key == DialogKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(draft) = self.follow_up.as_mut() else {
            return;
        };
        if let Some(text) = paste {
            draft.insert(&text);
        }
        match draft.key(key) {
            follow_up::Outcome::Edited => {
                self.follow_up_scroll.scroll_to_bottom();
                cx.notify();
            }
            follow_up::Outcome::Cancel => self.close_follow_up(window, cx),
            follow_up::Outcome::Send => self.send_follow_up(window, cx),
        }
    }

    /// `session.send` of the draft (text + Enter); the field empties once it is written.
    fn send_follow_up(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(draft) = self.follow_up.as_mut() else {
            return;
        };
        let Some(params) = draft.send_params() else {
            return;
        };
        let (task_id, session_id) = (draft.task_id, draft.session_id);
        draft.busy = true;
        draft.error = None;
        cx.notify();
        let sent = connection.request(Request::SessionSend(params));
        cx.spawn_in(window, async move |this, cx| {
            let failure = match sent.await {
                Ok(Outcome::Result(_)) => None,
                Ok(Outcome::Error(body)) => Some(body.message),
                Err(error) => Some(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                // The field was closed or moved to another session meanwhile.
                let Some(draft) = shell
                    .follow_up
                    .as_mut()
                    .filter(|d| (d.task_id, d.session_id) == (task_id, session_id))
                else {
                    return;
                };
                draft.busy = false;
                match failure {
                    None => draft.text.clear(),
                    Some(error) => {
                        tracing::warn!(%session_id, "session.send failed: {error}");
                        draft.error = Some(format!("Cannot send: {error}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The focused pane's session while its composer is shown.
    fn composer_session(&self) -> Option<Ulid> {
        self.focused_session()
            .filter(|&session| self.composers.shown(session).is_some())
    }

    /// The focused pane's session when it runs an agent.
    fn focused_agent(&self) -> Option<Ulid> {
        self.focused_session().filter(|&id| {
            self.sidebar
                .sessions()
                .iter()
                .any(|s| s.id == id && s.kind == SessionKind::Agent)
        })
    }

    /// Shows the focused agent pane's composer (keeping its draft) and gives it the keyboard.
    fn open_composer(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(session) = self.focused_agent() else {
            return;
        };
        self.composers.open(session);
        self.composer_scroll.scroll_to_bottom();
        window.focus(&self.composer_focus, cx);
        cx.notify();
    }

    /// Keys typed in the composer: Enter is a new line, Ctrl+Enter sends, Esc hides it.
    fn composer_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(session) = self.composer_session() else {
            return;
        };
        let Some(key) = task_dialog::dialog_key(&event.keystroke) else {
            return;
        };
        if key == DialogKey::Insert("@".into())
            && self.composers.shown(session).is_some_and(|d| !d.busy)
        {
            return self.open_file_ref_picker(session, window, cx);
        }
        if key == DialogKey::Paste
            && let Some(item) = cx.read_from_clipboard()
        {
            match item.text() {
                Some(text) => self.composers.insert(session, &text),
                None => self.composer_paste_image(session, item, window, cx),
            }
        }
        match self.composers.key(session, key) {
            text_box::Outcome::Edited => {
                self.composer_scroll.scroll_to_bottom();
                cx.notify();
            }
            text_box::Outcome::Cancel => {
                self.composers.hide(session);
                self.focus_pane(window, cx);
                cx.notify();
            }
            text_box::Outcome::Submit => self.send_composer(session, window, cx),
        }
    }

    /// An image pasted into the composer (§20B.5): saved through `attachment.save` (a BMP as
    /// PNG), then its path goes into the draft. A failure only logs, like a pane's paste.
    fn composer_paste_image(
        &mut self,
        session: Ulid,
        item: gpui::ClipboardItem,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some((ext, data)) = item.into_entries().find_map(|entry| match entry {
            gpui::ClipboardEntry::Image(image) => composer::attachment(image),
            _ => None,
        }) else {
            return;
        };
        let saved = connection.call::<mkrew_protocol::AttachmentSaveResult>(
            Request::AttachmentSave(mkrew_protocol::AttachmentSaveParams {
                session_id: session,
                ext: ext.to_owned(),
                data,
            }),
        );
        cx.spawn_in(window, async move |this, cx| match saved.await {
            Ok(saved) => {
                let _ = this.update(cx, |shell, cx| {
                    let text = composer::typed_paths([saved.path.as_str()]);
                    shell.composers.insert(session, &text);
                    shell.composer_scroll.scroll_to_bottom();
                    cx.notify();
                });
            }
            Err(error) => tracing::warn!(%session, "attachment.save failed: {error:#}"),
        })
        .detach();
    }

    /// ✦ in the composer (SPEC §20B.1): the session's own agent rewrites the draft, reading
    /// the code in the project folder if it needs to; the answer arrives as `ai.generated`.
    fn enhance_composer(&mut self, session: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(info) = self.sidebar.sessions().iter().find(|s| s.id == session) else {
            return;
        };
        // ponytail: the project folder even for a worktree session; same code, other branch.
        let (project_id, agent_profile) = (info.project_id, info.agent_profile.clone());
        let generation_id = Ulid::new();
        let Some(input) = self.composers.begin_enhance(session, generation_id) else {
            return;
        };
        cx.notify();
        let started = connection.call::<()>(Request::AiGenerate(AiGenerateParams {
            generation_id,
            project_id,
            worktree_task_id: None,
            repo: None,
            agent_profile: Some(agent_profile),
            kind: AiKind::EnhancePrompt,
            input,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let Err(error) = started.await else {
                return;
            };
            let _ = this.update_in(cx, |shell, _, cx| {
                shell
                    .composers
                    .enhanced(generation_id, Err(format!("{error:#}")));
                cx.notify();
            });
        })
        .detach();
    }

    /// `session.send` of `session`'s draft with Enter (one bracketed paste when multi-line,
    /// §7); the draft empties once it is written or queued.
    fn send_composer(&mut self, session: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(params) = self.composers.begin_send(session) else {
            return;
        };
        cx.notify();
        let sent = connection.request(Request::SessionSend(params));
        cx.spawn_in(window, async move |this, cx| {
            let result = match sent.await {
                Ok(Outcome::Result(_)) => Ok(()),
                Ok(Outcome::Error(body)) => Err(body.message),
                Err(error) => Err(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                if let Err(error) = &result {
                    tracing::warn!(%session, "session.send failed: {error}");
                }
                shell.composers.sent(session, result);
                cx.notify();
            });
        })
        .detach();
    }

    /// The prompt variables known for `project_id` (§20B.3): its name, `task` (a task title)
    /// and the file open in its Files view, else the one selected in its Git view.
    fn prompt_vars(&self, project_id: Ulid, task: Option<String>) -> PromptVars {
        let project = self
            .sidebar
            .projects()
            .iter()
            .find(|project| project.id == project_id)
            .map(|project| project.name.clone());
        let file = self
            .files
            .get(&project_id)
            .and_then(|view| view.file.clone())
            .or_else(|| {
                self.git
                    .get(&project_id)
                    .and_then(|view| view.selected.as_ref())
                    .map(|target| target.path.clone())
            });
        // The shown view's pick first (one picked earlier elsewhere may linger).
        let views = if self.view() == View::Git {
            [View::Git, View::Files]
        } else {
            [View::Files, View::Git]
        };
        let selection = views
            .into_iter()
            .find_map(|view| self.picked_lines(project_id, view))
            .map(|(_, _, _, text)| text);
        PromptVars {
            file,
            selection,
            task,
            project,
        }
    }

    /// The lines picked in `project_id`'s Files view (one line, §12A) or Git diff (new side):
    /// the file, first and last line, and their text (§20B.4).
    fn picked_lines(&self, project_id: Ulid, view: View) -> Option<(String, u32, u32, String)> {
        match view {
            View::Files => {
                let files = self.files.get(&project_id)?;
                let (Some(path), Some(shown)) = (&files.file, files.shown()) else {
                    return None;
                };
                // The selected lines, else the caret's line.
                let editor = &shown.editor;
                let (first, last) = match editor.selection() {
                    // A selection ending at a line's start does not include that line.
                    Some(range) if range.end.col == 0 => (range.start.line, range.end.line - 1),
                    Some(range) => (range.start.line, range.end.line),
                    None => (editor.cursor().line, editor.cursor().line),
                };
                let text = editor.lines().get(first..=last)?.join("\n");
                let line = |index: usize| u32::try_from(index + 1).ok();
                Some((path.clone(), line(first)?, line(last)?, text))
            }
            View::Git => {
                let git = self.git.get(&project_id)?;
                let (Some(target), Some(selection), git_view::Diff::Shown(shown)) =
                    (&git.selected, git.comments.selection, &git.diff)
                else {
                    return None;
                };
                let text = shown.new_side_text(selection)?;
                Some((target.path.clone(), selection.start, selection.end, text))
            }
            _ => None,
        }
    }

    /// "Send selection" (Ctrl+Shift+L, §20B.4): the picked lines of Files or the Git diff, or
    /// the focused terminal's selected text, go to a session picked in the send dialog (the
    /// task's session for Git task changes, else the most recently active agent).
    fn open_selection_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.workspace.filter(|_| self.connection.is_some()) else {
            return;
        };
        let sessions = self.sidebar.sessions();
        let (file_path, prefill, task) = match self.view() {
            view @ (View::Files | View::Git) => {
                let picked = self
                    .picked_lines(project_id, view)
                    .filter(|(_, _, _, text)| !text.trim().is_empty());
                let Some((path, start, end, text)) = picked else {
                    return;
                };
                let task = self
                    .git
                    .get(&project_id)
                    .filter(|_| view == View::Git)
                    .and_then(|git| git.since_session(sessions));
                let prefill = composer::selection_prefill(Some((&path, start, end)), &text);
                (format!("{path}:{start}-{end}"), prefill, task)
            }
            View::Terminals => {
                let text = self
                    .focused_session()
                    .and_then(|session| self.panes.get(&session))
                    .and_then(|pane| pane.read(cx).selection_text())
                    .filter(|text| !text.trim().is_empty());
                let Some(text) = text else {
                    return;
                };
                (
                    String::new(),
                    composer::selection_prefill(None, &text),
                    None,
                )
            }
            _ => return,
        };
        let dialog = SendDialog::selection(project_id, file_path, prefill, task, sessions);
        self.open_doc_dialog(DocDialog::Send(dialog), window, cx);
    }

    /// The send dialog's Send: comments go out, a selection opens the picked session's
    /// composer with it.
    fn confirm_send_dialog(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        match &self.doc_dialog {
            Some(DocDialog::Send(dialog)) if dialog.prefill.is_some() => {
                self.send_selection(window, cx);
            }
            _ => self.send_comments(window, cx),
        }
    }

    /// Shows the selected session's pane with its composer, the selection typed after its
    /// draft; the user adds the instruction and sends.
    fn send_selection(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(DocDialog::Send(dialog)) = &self.doc_dialog else {
            return;
        };
        let (Some(prefill), Some(session)) = (
            dialog.prefill.clone(),
            dialog.target(self.sidebar.sessions()),
        ) else {
            return;
        };
        self.doc_dialog = None;
        self.composers.open(session);
        let before = self.composers.shown(session).map_or("", |d| d.text.text());
        let text = prompt_picker::on_new_line(before, &prefill);
        self.composers.insert(session, &text);
        self.composer_scroll.scroll_to_bottom();
        self.open_session(session, window, cx);
        if self.composer_session() == Some(session) {
            window.focus(&self.composer_focus, cx);
        }
        cx.notify();
    }

    /// Opens the prompt picker typing into `target`; the library is read (seeded the first
    /// time) now, so edited prompt files show without a restart.
    fn open_prompt_picker(
        &mut self,
        target: prompt_picker::Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let vars = match target {
            prompt_picker::Target::Composer(session) => {
                let Some(session) = self.sidebar.sessions().iter().find(|s| s.id == session) else {
                    return;
                };
                let task = session
                    .task_id
                    .and_then(|id| self.board.task(id))
                    .map(|task| task.title.clone());
                self.prompt_vars(session.project_id, task)
            }
            prompt_picker::Target::TaskDialog => {
                let Some(dialog) = &self.task_dialog else {
                    return;
                };
                let title = dialog.title.trim();
                let task = (!title.is_empty()).then(|| title.to_owned());
                self.prompt_vars(dialog.project_id(), task)
            }
        };
        let library = mkrew_core::prompt::load_or_seed(&self.prompts_dir);
        for error in &library.errors {
            tracing::warn!(file = %error.file.display(), "cannot read prompt: {}", error.message);
        }
        self.workspace_menu = None;
        self.prompt_picker = Some(PromptPicker::new(target, library, vars));
        self.prompt_picker_scroll.scroll_to_item(0);
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    /// Closes the prompt picker: `text` (the rendered pick) is typed into its target, which
    /// gets the keyboard back.
    fn close_prompt_picker(
        &mut self,
        text: Option<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(picker) = self.prompt_picker.take() else {
            return;
        };
        match (picker.target, text) {
            (prompt_picker::Target::Composer(session), Some(text)) => {
                let before = self.composers.shown(session).map_or("", |d| d.text.text());
                let text = prompt_picker::on_new_line(before, &text);
                self.composer_typed(session, &text, window, cx);
            }
            (prompt_picker::Target::Composer(session), None)
                if self.composer_session() == Some(session) =>
            {
                window.focus(&self.composer_focus, cx);
            }
            (prompt_picker::Target::TaskDialog, Some(text)) => {
                if let Some(dialog) = self.task_dialog.as_mut() {
                    let text = prompt_picker::on_new_line(dialog.prompt.text(), &text);
                    dialog.insert_prompt(&text);
                    self.prompt_scroll.scroll_to_bottom();
                }
                self.focus_pane(window, cx);
            }
            _ => self.focus_pane(window, cx),
        }
        cx.notify();
    }

    /// ↑ / ↓ move the pick (kept in view), Enter types it, Esc closes.
    fn prompt_picker_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = quick_open::key(&event.keystroke) else {
            return;
        };
        let Some(picker) = self.prompt_picker.as_mut() else {
            return;
        };
        match key {
            quick_open::Key::Close => self.close_prompt_picker(None, window, cx),
            quick_open::Key::Open => self.insert_prompt_pick(window, cx),
            quick_open::Key::Move(by) => {
                picker.step(by);
                self.prompt_picker_scroll.scroll_to_item(picker.selected);
                cx.notify();
            }
            quick_open::Key::Backspace
            | quick_open::Key::DeleteWord
            | quick_open::Key::SelectAll
            | quick_open::Key::Paste
            | quick_open::Key::Insert(_) => {}
        }
    }

    /// Types the picked prompt, rendered, into the picker's target.
    fn insert_prompt_pick(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(rendered) = self.prompt_picker.as_ref().and_then(PromptPicker::preview) else {
            return;
        };
        self.close_prompt_picker(Some(rendered.text), window, cx);
    }

    /// `session.unqueue`: the queued text is not typed; the `session.queue` event that follows
    /// updates the list (a text written meanwhile fails with `not_found` and leaves it too).
    fn unqueue(&mut self, session: Ulid, send_id: Ulid, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let removed = connection.request(Request::SessionUnqueue(SessionUnqueueParams {
            session_id: session,
            send_id,
        }));
        cx.spawn(async move |_, _| match removed.await {
            Ok(Outcome::Result(_)) => {}
            Ok(Outcome::Error(body)) => {
                tracing::warn!(%session, "session.unqueue failed: {}", body.message);
            }
            Err(error) => tracing::warn!(%session, "session.unqueue failed: {error:#}"),
        })
        .detach();
    }

    /// Starts an agent or a shell (the workspace's default of that kind) in the workspace folder
    /// and shows it next to the focused pane of the active tab, split along `axis`.
    fn new_pane(
        &mut self,
        kind: SessionKind,
        axis: Axis,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(project_id) = self.workspace else {
            return;
        };
        let Some(tab_id) = self
            .tabs
            .get(&project_id)
            .and_then(|tabs| tabs.active())
            .map(|tab| tab.id)
        else {
            return;
        };
        if self.ask_for_default(project_id, kind, window, cx) {
            return;
        }
        cx.notify();
        let params = lifecycle::spawn_params(project_id, Some(tab_id), kind);
        let spawned = connection.call::<SessionSpawnResult>(Request::SessionSpawn(params));
        cx.spawn_in(window, async move |this, cx| {
            let spawned = spawned.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                match spawned {
                    Ok(spawned) => {
                        // Next to the focused pane of the tab active now (the user may have
                        // switched tabs meanwhile), unless the daemon placed it already.
                        shell.place_session(project_id, spawned.session_id, axis, window, cx);
                        shell.sync_panes(window, cx);
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "session.spawn failed: {error:#}");
                        let what = match kind {
                            SessionKind::Agent => "an agent",
                            SessionKind::Shell => "a shell",
                        };
                        shell.tab_error = Some(format!("Cannot start {what}: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// True (after telling the user and opening the workspace menu) when the workspace has no
    /// default profile of `kind`; the daemon would refuse to start one.
    fn ask_for_default(
        &mut self,
        project_id: Ulid,
        kind: SessionKind,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        self.tab_error = None;
        let project = (self.sidebar.projects().iter()).find(|project| project.id == project_id);
        let (default, what) = match kind {
            SessionKind::Agent => (project.map(|p| &p.default_agent), "agent"),
            SessionKind::Shell => (project.map(|p| &p.default_shell), "shell"),
        };
        let missing = default.is_some_and(Option::is_none);
        if missing {
            self.tab_error = Some(format!(
                "Pick a default {what} for this workspace, then try again."
            ));
            self.open_workspace_menu(project_id, window, cx);
        }
        missing
    }

    /// A shortcut (SPEC §13). Terminal commands (paste) are handled by the focused pane.
    fn run(&mut self, Run(command): &Run, window: &mut Window, cx: &mut Context<'_, Self>) {
        // A shortcut ends every open popover (they never own the keyboard).
        self.close_menus();
        // Shortcuts still fire while a title is edited: save it first (closing its tab, …).
        self.finish_rename(true, window, cx);
        // Its own shortcut closes the palette again.
        if *command == Command::CommandPalette && self.palette.is_some() {
            return self.close_palette(window, cx);
        }
        // The dialogs are modal: bound keys go on to their key handler (e.g. Ctrl+Alt+digit
        // is AltGr on many layouts) instead of acting behind them.
        if self.modal_open() {
            cx.propagate();
            return;
        }
        // Same for the follow-up field while it has the keyboard, unless its card stopped
        // showing it (session exited, card moved on): then the shortcut acts as usual.
        if self.follow_up_focus.is_focused(window) {
            if self.follow_up_shown() {
                cx.propagate();
                return;
            }
            self.follow_up = None;
            window.focus(&self.focus_handle, cx);
        }
        // Same for a Docs comment draft (Ctrl+Alt+M is AltGr+M on many layouts).
        if self.comment_focus.is_focused(window) {
            if self.comment_draft().is_some() {
                cx.propagate();
                return;
            }
            window.focus(&self.focus_handle, cx);
        }
        // Same for the Git commit box (Esc leaves it, then shortcuts act again), unless it went
        // away (everything committed, mode switched): then the list takes the keyboard back.
        if self.git_commit_focus.is_focused(window) {
            if self.git_commit_shown() {
                cx.propagate();
                return;
            }
            window.focus(&self.git_focus, cx);
        }
        // Same for the Files find / replace fields while they have the keyboard (Esc gives it
        // back), except the Files commands (Ctrl+S, Ctrl+F, Ctrl+H act from there).
        if self.file_find_focus.is_focused(window) || self.file_replace_focus.is_focused(window) {
            let open = self.view() == View::Files
                && (self.workspace)
                    .and_then(|id| self.files.get(&id))
                    .is_some_and(|view| view.find.is_some() && !view.previewing());
            let files_command = matches!(
                command,
                Command::SaveFile | Command::FindInFile | Command::ReplaceInFile
            );
            if open && !files_command {
                cx.propagate();
                return;
            }
            if !open {
                window.focus(&self.focus_handle, cx);
            }
        }
        // Same for the Docs filter while it has the keyboard.
        if self.docs_filter_focus.is_focused(window) {
            if self.view() == View::Docs {
                cx.propagate();
                return;
            }
            window.focus(&self.focus_handle, cx);
        }
        // Same for the sidebar filter while it has the keyboard, except the shortcuts that
        // leave it (Peek, Toggle sidebar, Toggle compact sidebar).
        if self.filter_focus.is_focused(window) {
            if matches!(
                command,
                Command::Peek | Command::ToggleSidebar | Command::ToggleCompactSidebar
            ) {
                self.focus_pane(window, cx);
            } else {
                cx.propagate();
                return;
            }
        }
        // Same for the focused pane's composer (§20B.1), unless it went away. Its own shortcut
        // keeps it focused (Ctrl+Shift+Enter would read as Ctrl+Enter: send).
        if self.composer_focus.is_focused(window) {
            if self.composer_session().is_some() {
                if *command != Command::OpenComposer {
                    cx.propagate();
                }
                return;
            }
            self.focus_pane(window, cx);
        }
        // A shortcut that acts ends the peek too (it restores the keyboard first, so a shortcut
        // typed in the peek still runs); its own shortcut toggles it below.
        if *command != Command::Peek {
            self.close_peek(window, cx);
        }
        if self.view() != View::Terminals {
            match *command {
                // Opening or picking a tab or pane shows the terminals.
                Command::NewShellTab
                | Command::NewAgentTab
                | Command::NextTab
                | Command::PreviousTab
                | Command::Tab(_)
                | Command::SplitRight
                | Command::SplitDown
                | Command::SplitAgent
                | Command::SplitShell => self.show_view(View::Terminals, window, cx),
                // The rest act on panes that are not on screen.
                Command::RenameTab
                | Command::RenamePane
                | Command::CloseTab
                | Command::Focus(_)
                | Command::Resize(_)
                | Command::ZoomPane
                | Command::ClosePane => return,
                _ => {}
            }
        }
        match *command {
            Command::AddWorkspace => self.add_workspace(window, cx),
            Command::Workspace(n) => {
                let nth = self.sidebar.projects().get(usize::from(n) - 1);
                if let Some(project_id) = nth.map(|project| project.id) {
                    self.activate_workspace(project_id, window, cx);
                }
            }
            Command::NextWorkspace => self.cycle_workspace(1, window, cx),
            Command::PreviousWorkspace => self.cycle_workspace(-1, window, cx),
            Command::NewShellTab => self.new_tab(SessionKind::Shell, window, cx),
            Command::NewAgentTab => self.new_tab(SessionKind::Agent, window, cx),
            Command::NextTab => self.switch_tab(|tabs| tabs.activate_offset(1), window, cx),
            Command::PreviousTab => self.switch_tab(|tabs| tabs.activate_offset(-1), window, cx),
            Command::Tab(n) => self.switch_tab(
                |tabs| {
                    tabs.activate_nth(usize::from(n) - 1);
                },
                window,
                cx,
            ),
            Command::RenameTab => self.start_rename(window, cx),
            Command::RenamePane => {
                if let Some(session_id) = self.focused_session() {
                    self.start_pane_rename(session_id, window, cx);
                }
            }
            Command::CloseTab => {
                let active = self
                    .workspace
                    .and_then(|project_id| self.tabs.get(&project_id))
                    .and_then(|tabs| tabs.active())
                    .map(|tab| tab.id);
                if let Some(tab_id) = active {
                    self.close_tab(tab_id, window, cx);
                }
            }
            Command::SplitRight => self.split(Axis::Vertical, window, cx),
            Command::SplitDown => self.split(Axis::Horizontal, window, cx),
            // Exactly the + Agent / + Shell buttons.
            Command::SplitAgent => self.new_pane(SessionKind::Agent, Axis::Vertical, window, cx),
            Command::SplitShell => self.new_pane(SessionKind::Shell, Axis::Vertical, window, cx),
            Command::Focus(direction) => {
                if self
                    .active_tabs_mut()
                    .is_some_and(|tabs| tabs.focus_neighbour(direction))
                {
                    // A focus move ends a zoom: the hidden panes come back (and re-attach).
                    self.sync_panes(window, cx);
                    cx.notify();
                }
            }
            Command::Resize(direction) => {
                let Some(project_id) = self.workspace else {
                    return;
                };
                let write = self
                    .active_tabs_mut()
                    .and_then(|tabs| tabs.resize_focused(direction));
                if let Some(write) = write {
                    self.save_layout(project_id, write, window, cx);
                    cx.notify();
                }
            }
            Command::ZoomPane => {
                if self.active_tabs_mut().is_some_and(Tabs::toggle_zoom) {
                    self.sync_panes(window, cx);
                    cx.notify();
                }
            }
            Command::ClosePane => self.close_focused_pane(window, cx),
            Command::OpenInZed => match self.workspace {
                // Docs open the shown file (SPEC §13 "workspace or current file").
                Some(project_id)
                    if self.view() == View::Docs
                        && self
                            .docs
                            .get(&project_id)
                            .is_some_and(|docs| docs.selected.is_some()) =>
                {
                    self.open_doc_in_zed(project_id, cx);
                }
                // Files open the opened file, at the selected line.
                Some(project_id)
                    if self.view() == View::Files
                        && self
                            .files
                            .get(&project_id)
                            .is_some_and(|view| view.file.is_some()) =>
                {
                    self.open_file_in_zed(project_id, cx);
                }
                // Git opens the selected changed file.
                Some(project_id)
                    if self.view() == View::Git
                        && self
                            .git
                            .get(&project_id)
                            .is_some_and(|view| view.selected.is_some()) =>
                {
                    self.open_git_file_in_zed(project_id, cx);
                }
                Some(project_id) => self.open_in_zed(project_id, cx),
                None => {}
            },
            Command::NextInbox => self.jump_next_inbox(window, cx),
            // Again while it has the keyboard closes it.
            Command::Peek => {
                if self.peek.as_ref().is_some_and(|peek| peek.focused) {
                    self.close_peek(window, cx);
                } else if let Some(first) = self.inbox_entries().first() {
                    self.open_peek(first.session_id, true, window, cx);
                }
            }
            Command::OpenComposer => self.open_composer(window, cx),
            Command::SendSelection => self.open_selection_dialog(window, cx),
            Command::ShowBoard => self.show_view(View::Board, window, cx),
            Command::ShowTerminals => self.show_view(View::Terminals, window, cx),
            Command::ShowDocs => self.show_view(View::Docs, window, cx),
            Command::ShowFiles => self.show_view(View::Files, window, cx),
            Command::ShowGit => self.show_view(View::Git, window, cx),
            Command::QuickOpen => self.open_quick_open(window, cx),
            Command::CommandPalette => self.open_palette(window, cx),
            Command::OpenKeymapSettings => self.open_keymap_settings(cx),
            Command::ToggleTheme => self.toggle_theme(window, cx),
            Command::ToggleFullScreen => window.toggle_fullscreen(),
            Command::ToggleSidebar => self.toggle_sidebar(window, cx),
            Command::ToggleCompactSidebar => self.toggle_compact_sidebar(window, cx),
            Command::ToggleInspector => self.toggle_inspector(window, cx),
            // Only bound while the Git list has focus.
            Command::GitToggleMode => self.git_key(git_view::Key::ToggleMode, window, cx),
            Command::GitToggleHistory => self.git_key(git_view::Key::ToggleHistory, window, cx),
            Command::GitNextHunk => self.git_key(git_view::Key::NextHunk, window, cx),
            Command::GitPreviousHunk => self.git_key(git_view::Key::PreviousHunk, window, cx),
            Command::GitStage => {
                self.git_key(git_view::Key::Stage(git_view::Stage::Stage), window, cx);
            }
            Command::GitUnstage => {
                self.git_key(git_view::Key::Stage(git_view::Stage::Unstage), window, cx);
            }
            Command::GitFocusCommit => {
                if self.git_commit_shown() {
                    window.focus(&self.git_commit_focus, cx);
                    cx.notify();
                }
            }
            Command::GitToggleDiffMode => self.set_diff_mode(self.diff_mode.other(), cx),
            Command::SaveFile => {
                // Only bound while Files are shown.
                if let Some(project_id) = self.workspace {
                    self.save_file(project_id, None, window, cx);
                }
            }
            Command::ToggleIgnored => {
                // Only bound while Files are shown.
                if let Some(project_id) = self.workspace {
                    let requests = self.files_view(project_id).toggle_ignored();
                    self.files_requests(project_id, requests, window, cx);
                }
            }
            Command::ToggleDocSource => {
                // Only bound while Docs are shown.
                if let Some(docs) = self.workspace.and_then(|id| self.docs.get_mut(&id)) {
                    docs.toggle_mode();
                    cx.notify();
                }
            }
            Command::AddComment => {
                // Only bound while Docs are shown or the Git list has focus.
                if let (Some(project_id), Some(context)) = (self.workspace, self.comment_context())
                {
                    self.start_comment(project_id, context, window, cx);
                }
            }
            Command::SendComments => {
                // Only bound while Docs are shown or the Git list has focus.
                if let (Some(project_id), Some(context)) = (self.workspace, self.comment_context())
                {
                    self.open_send_dialog(project_id, context, window, cx);
                }
            }
            Command::NewTask => {
                if let Some(project_id) = self.workspace {
                    // The new card shows up on the board.
                    self.show_view(View::Board, window, cx);
                    self.open_task_dialog(TaskDialog::new(project_id), window, cx);
                }
            }
            Command::RunTask => {
                // Only bound while a board is shown; the selection may be another workspace's.
                let target = self
                    .workspace
                    .and_then(|project_id| self.board.run_target(self.selected_card, project_id));
                if let Some(task_id) = target {
                    self.run_task(task_id, window, cx);
                }
            }
            // Handled by the focused terminal pane.
            Command::Copy | Command::Paste | Command::PageUp | Command::PageDown => {}
            // A focused terminal pane handles it itself.
            Command::Find => match self.view() {
                View::Files => self.open_file_find(false, window, cx),
                View::Docs => {
                    window.focus(&self.docs_filter_focus, cx);
                    cx.notify();
                }
                _ => {}
            },
            Command::FindInFile => self.open_file_find(false, window, cx),
            Command::ReplaceInFile => self.open_file_find(true, window, cx),
        }
    }

    fn cycle_workspace(&mut self, offset: isize, window: &mut Window, cx: &mut Context<'_, Self>) {
        if let Some(project_id) = self.sidebar.workspace_at_offset(self.workspace, offset) {
            self.activate_workspace(project_id, window, cx);
        }
    }

    /// Changes the active tab of the shown workspace.
    fn switch_tab(
        &mut self,
        change: impl FnOnce(&mut Tabs),
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(tabs) = self.active_tabs_mut() {
            change(tabs);
            self.sync_panes(window, cx);
            cx.notify();
        }
    }

    /// A new tab in the shown workspace with an agent or a shell in it.
    fn new_tab(&mut self, kind: SessionKind, window: &mut Window, cx: &mut Context<'_, Self>) {
        if let Some(project_id) = self.workspace
            && self.tabs.contains_key(&project_id)
        {
            self.create_tab(project_id, Some(kind), window, cx);
        }
    }

    /// Splits the focused pane with a new session of the same kind.
    fn split(&mut self, axis: Axis, window: &mut Window, cx: &mut Context<'_, Self>) {
        let focused = self
            .focused_session()
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));
        let kind = lifecycle::split_kind(focused);
        self.new_pane(kind, axis, window, cx);
    }

    /// Starts editing the active tab's title inline, starting from the current one.
    fn start_rename(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(tab) = self
            .workspace
            .and_then(|id| self.tabs.get(&id))
            .and_then(|tabs| tabs.active())
        else {
            return;
        };
        self.renaming = Some(Renaming {
            target: RenameTarget::Tab(tab.id),
            draft: tab.title.clone().into(),
        });
        window.focus(&self.rename_focus, cx);
        cx.notify();
    }

    /// Opens the menu of `tab_id` at `position`: the tab becomes the active one first, because
    /// Split and Rename act on the active tab.
    fn open_tab_menu(
        &mut self,
        tab_id: Ulid,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        self.pane_menu = None;
        if let Some(tabs) = self.active_tabs_mut() {
            tabs.activate(tab_id);
        }
        self.tab_menu = Some(TabMenu { tab_id, position });
        self.sync_panes(window, cx);
        cx.notify();
    }

    /// Starts editing `session_id`'s title inline in its pane header (F2, double-click),
    /// from the user's own title. Run sessions are named by their command: left alone.
    fn start_pane_rename(
        &mut self,
        session_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(session) = self.sidebar.sessions().iter().find(|s| s.id == session_id) else {
            return;
        };
        if mkrew_core::run::command_name(session).is_some() {
            return;
        }
        self.renaming = Some(Renaming {
            target: RenameTarget::Session(session_id),
            draft: session.title.clone().unwrap_or_default().into(),
        });
        window.focus(&self.rename_focus, cx);
        cx.notify();
    }

    /// Ends the inline edit: `commit` sends `tab.rename` / `session.rename`; the reply shows
    /// at once, the `tab.updated` / `session.updated` event tells other clients. Unchanged
    /// drafts and blank tab titles are dropped (a blank pane title clears it). Focus goes
    /// back to the pane.
    fn finish_rename(&mut self, commit: bool, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(Renaming { target, draft }) = self.renaming.take() else {
            return;
        };
        if commit {
            match target {
                RenameTarget::Tab(tab_id) => self.rename_tab(tab_id, &draft, window, cx),
                RenameTarget::Session(session_id) => {
                    self.rename_session(session_id, &draft, window, cx);
                }
            }
        }
        if self.rename_focus.is_focused(window) {
            self.focus_pane(window, cx);
        }
        cx.notify();
    }

    fn rename_tab(
        &mut self,
        tab_id: Ulid,
        draft: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        // Any workspace: switching workspaces mid-edit commits through the blur.
        let params = self
            .tabs
            .values()
            .find_map(|tabs| tabs.tab(tab_id))
            .and_then(|tab| rename_params(tab, draft));
        if let Some(params) = params {
            // Shown at once, without waiting for the `tab.updated` echo.
            let shown = |shell: &mut Self, tab: TerminalTab| {
                if let Some(tabs) = shell.tabs.get_mut(&tab.project_id) {
                    tabs.apply(&Event::TabUpdated(tab));
                }
            };
            self.send_edit(
                Request::TabRename(params),
                "Cannot rename the tab",
                shown,
                window,
                cx,
            );
        }
    }

    fn rename_session(
        &mut self,
        session_id: Ulid,
        draft: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let session = (self.sidebar.sessions().iter()).find(|s| s.id == session_id);
        // The pane may have exited while its rename field was open.
        if session.is_some_and(|s| s.state == SessionState::Exited) {
            return;
        }
        let params = session.and_then(|session| lifecycle::session_rename_params(session, draft));
        if let Some(params) = params {
            // Shown at once (header, sidebar, Inbox), without waiting for the echo.
            let shown = |shell: &mut Self, session: Session| {
                // Title only: the reply may predate state events already applied.
                shell.sidebar.set_title(session.id, session.title);
            };
            self.send_edit(
                Request::SessionRename(params),
                "Cannot rename the pane",
                shown,
                window,
                cx,
            );
        }
    }

    /// "Pin" / "Unpin" of a session (sidebar row or pane menu): `session.pin`. The
    /// `session.updated` event that follows moves the row.
    fn pin_session(
        &mut self,
        session_id: Ulid,
        pinned: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.pane_menu = None;
        let params = SessionPinParams { session_id, pinned };
        self.send_edit(
            Request::SessionPin(params),
            "Cannot pin the pane",
            |_: &mut Self, _: serde_json::Value| {},
            window,
            cx,
        );
    }

    /// "Pin" / "Unpin" of a tab (tab menu): `tab.pin`. The `tab.updated` event that follows
    /// moves the tab.
    fn pin_tab(
        &mut self,
        tab_id: Ulid,
        pinned: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.tab_menu = None;
        let params = TabPinParams { tab_id, pinned };
        self.send_edit(
            Request::TabPin(params),
            "Cannot pin the tab",
            |_: &mut Self, _: serde_json::Value| {},
            window,
            cx,
        );
    }

    /// Sets (or clears) the color of `session_id`'s pane from its menu.
    fn set_pane_color(
        &mut self,
        session_id: Ulid,
        color: Option<&'static str>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.pane_menu = None;
        let params = (self.sidebar.sessions().iter())
            .find(|s| s.id == session_id)
            .and_then(|session| lifecycle::session_color_params(session, color));
        if let Some(params) = params {
            // Shown at once (header, chip), without waiting for the echo.
            let shown = |shell: &mut Self, session: Session| {
                shell.sidebar.set_color(session.id, session.color);
            };
            self.send_edit(
                Request::SessionSetColor(params),
                "Cannot change the pane color",
                shown,
                window,
                cx,
            );
        }
        cx.notify();
    }

    /// Opens the menu of `session_id`'s pane at `position`, focusing the pane.
    fn open_pane_menu(
        &mut self,
        session_id: Ulid,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        self.tab_menu = None;
        if let Some(tabs) = self.active_tabs_mut() {
            tabs.focus(session_id);
        }
        // Keyboard focus follows the highlighted pane (typed keys, F2, Ctrl+W).
        self.focus_pane(window, cx);
        self.pane_menu = Some(PaneMenu {
            session_id,
            position,
        });
        cx.notify();
    }

    /// Sends a `tab.rename` / `session.rename` / `session.set_color` / `session.pin` / `tab.pin`; `shown` applies the
    /// reply, a failure goes to the tab bar's error line after `failure`.
    fn send_edit<T: serde::de::DeserializeOwned + Send + 'static>(
        &mut self,
        request: Request,
        failure: &'static str,
        shown: impl FnOnce(&mut Self, T) + 'static,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.tab_error = None;
        let edited = connection.call::<T>(request);
        cx.spawn_in(window, async move |this, cx| {
            let edited = edited.await;
            let _ = this.update(cx, |shell, cx| {
                match edited {
                    Ok(edited) => shown(shell, edited),
                    Err(error) => {
                        tracing::warn!("{failure}: {error:#}");
                        shell.tab_error = Some(format!("{failure}: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Keys typed while renaming edit the draft; Enter commits, Esc cancels.
    fn rename_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = find_key(&event.keystroke) else {
            return;
        };
        let Some(renaming) = self.renaming.as_mut() else {
            return;
        };
        match key {
            FindKey::Close => self.finish_rename(false, window, cx),
            FindKey::Older | FindKey::Newer => self.finish_rename(true, window, cx),
            FindKey::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    renaming.draft.insert(&text.replace(['\r', '\n'], " "));
                    cx.notify();
                }
            }
            FindKey::Insert(text) => {
                renaming.draft.insert(&text);
                cx.notify();
            }
            // Backspace, Ctrl+Backspace, Ctrl+A.
            key => {
                key.edit(&mut renaming.draft);
                cx.notify();
            }
        }
    }

    /// Keys typed in the sidebar's filter box edit it; Esc clears it and gives the keyboard
    /// back to the pane, Enter just gives it back.
    fn filter_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = find_key(&event.keystroke) else {
            return;
        };
        match key {
            FindKey::Close => {
                self.sidebar.filter_mut().clear();
                self.focus_pane(window, cx);
            }
            FindKey::Older | FindKey::Newer => self.focus_pane(window, cx),
            FindKey::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.sidebar
                        .filter_mut()
                        .insert(&text.replace(['\r', '\n'], " "));
                }
            }
            FindKey::Insert(text) => {
                self.sidebar.filter_mut().insert(&text);
            }
            // Backspace, Ctrl+Backspace, Ctrl+A.
            key => {
                key.edit(self.sidebar.filter_mut());
            }
        }
        cx.notify();
    }

    /// `tab.close` for `tab_id` of the shown workspace (the active one, or any tab's ✕). Its
    /// sessions keep running (reachable from the sidebar); closing the last tab leaves a fresh
    /// empty one.
    fn close_tab(&mut self, tab_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(project_id) = self.workspace else {
            return;
        };
        self.tab_error = None;
        let closed = connection.call::<()>(Request::TabClose(TabRef { tab_id }));
        cx.spawn_in(window, async move |this, cx| {
            let closed = closed.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                match closed {
                    Ok(()) => {
                        let Some(tabs) = shell.tabs.get_mut(&project_id) else {
                            return;
                        };
                        tabs.remove(tab_id);
                        if tabs.is_empty() {
                            shell.create_tab(project_id, None, window, cx);
                        }
                        shell.sync_panes(window, cx);
                    }
                    Err(error) => {
                        tracing::warn!(%tab_id, "tab.close failed: {error:#}");
                        shell.tab_error = Some(format!("Cannot close the tab: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn close_focused_pane(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if let Some(session) = self.focused_session() {
            self.close_pane(session, window, cx);
        }
    }

    /// Closes the pane of `session`: an exited session's pane just goes, a shell is killed,
    /// a running agent asks whether to kill it or only hide the pane (SPEC §10).
    fn close_pane(&mut self, session: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let known = self.sidebar.sessions().iter().find(|s| s.id == session);
        match lifecycle::close_action(known) {
            CloseAction::Remove => self.remove_pane(session, window, cx),
            CloseAction::Kill => {
                self.remove_pane(session, window, cx);
                self.kill(session, window, cx);
            }
            CloseAction::Ask => {
                let answer = window.prompt(
                    PromptLevel::Warning,
                    "This session is still running.",
                    Some("Kill it, or hide the pane and keep it running in the sidebar."),
                    &CloseChoice::ANSWERS,
                    cx,
                );
                cx.spawn_in(window, async move |this, cx| {
                    let choice = CloseChoice::from_answer(answer.await.ok());
                    let _ = this.update_in(cx, |shell, window, cx| match choice {
                        CloseChoice::Kill => {
                            shell.remove_pane(session, window, cx);
                            // It may have exited while the dialog was open.
                            if !shell.is_exited(session) {
                                shell.kill(session, window, cx);
                            }
                        }
                        CloseChoice::Hide => shell.remove_pane(session, window, cx),
                        CloseChoice::Cancel => {}
                    });
                })
                .detach();
            }
        }
    }

    /// Takes the pane of `session` out of its tab's layout (the session itself is untouched).
    fn remove_pane(&mut self, session: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let closed = self
            .tabs
            .values_mut()
            .find_map(|tabs| Some((tabs.project_id(), tabs.close_pane(session)?)));
        if let Some((project_id, write)) = closed {
            self.save_layout(project_id, write, window, cx);
        }
        self.sync_panes(window, cx);
        cx.notify();
    }

    /// `session.kill`; a failure is shown in the tab bar.
    fn kill(&mut self, session_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let killed = connection.call::<()>(Request::SessionKill(SessionKillParams {
            session_id,
            force: false,
        }));
        cx.spawn_in(window, async move |this, cx| {
            if let Err(error) = killed.await {
                tracing::warn!(%session_id, "session.kill failed: {error:#}");
                let _ = this.update(cx, |shell, cx| {
                    shell.tab_error = Some(format!("Cannot kill the session: {error:#}"));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// ✕ on a sidebar session: a running one asks first (removing stops it); then
    /// `session.remove` hides it for good and its pane closes. Its log stays on disk.
    fn remove_session(
        &mut self,
        session_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let answer = (!self.is_exited(session_id)).then(|| {
            window.prompt(
                PromptLevel::Warning,
                "This session is still running.",
                Some("Removing it from the sidebar stops it."),
                &workspace_menu::REMOVE_ANSWERS,
                cx,
            )
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Some(answer) = answer
                && !task_dialog::confirms_delete(answer.await.ok())
            {
                return;
            }
            let removed = connection
                .call::<()>(Request::SessionRemove(SessionRef { session_id }))
                .await;
            let _ = this.update_in(cx, |shell, window, cx| match removed {
                Ok(()) => {
                    shell.sidebar.remove_session(session_id);
                    shell.remove_pane(session_id, window, cx);
                }
                Err(error) => {
                    tracing::warn!(%session_id, "session.remove failed: {error:#}");
                    shell.sidebar_error = Some(format!("Cannot remove the session: {error:#}"));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn inbox_entries(&self) -> Vec<Entry> {
        inbox::entries(
            self.sidebar.projects(),
            self.sidebar.sessions(),
            &self.viewed,
        )
    }

    /// Hiding also closes what only the sidebar draws (the peek, the workspace menu) and takes
    /// the keyboard out of the filter box, so it never stays in something invisible (the `◧`
    /// click does not pass `Shell::run`'s filter guard). Showing brings back the last width.
    fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.sidebar_layout.toggle_hidden();
        if self.sidebar_layout.hidden() {
            if self.filter_focus.is_focused(window) {
                self.focus_pane(window, cx);
            }
            self.close_peek(window, cx);
            self.workspace_menu = None;
        }
        cx.notify();
    }

    /// Full width ↔ compact rail; a hidden sidebar comes back as the rail. The rail draws no
    /// filter box or chips, so entering it clears them (nothing stays narrowed unseen) and the
    /// keyboard leaves the filter box. A peek or menu hung on a full-width row closes.
    fn toggle_compact_sidebar(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.sidebar_layout.toggle_compact();
        if self.sidebar_layout.compact() {
            if self.filter_focus.is_focused(window) {
                self.focus_pane(window, cx);
            }
            self.sidebar.clear_filter();
            self.close_peek(window, cx);
            self.workspace_menu = None;
        }
        cx.notify();
    }

    /// Opens the Inbox entry after the focused pane's session (wrapping); nothing while the
    /// Inbox is empty. The key and the status bar's Inbox state both land here.
    fn jump_next_inbox(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let entries = self.inbox_entries();
        let focused = self.focused_session().and_then(|id| {
            self.sidebar
                .sessions()
                .iter()
                .find(|session| session.id == id)
        });
        if let Some(session) = inbox::next(&entries, focused) {
            self.open_session(session, window, cx);
        }
    }

    /// Whether `session_id` can be peeked: listed and live (`session.read` needs a live one).
    fn peekable(&self, session_id: Ulid) -> bool {
        self.sidebar
            .sessions()
            .iter()
            .any(|session| session.id == session_id && session.state != SessionState::Exited)
    }

    /// Opens the peek of `session_id` next to its row (§20A.1). With `focused` the keyboard
    /// moves into it and its row is revealed; a peek that already had the keyboard keeps it.
    fn open_peek(
        &mut self,
        session_id: Ulid,
        focused: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.modal_open() || !self.peekable(session_id) {
            return;
        }
        // A keyboard peek hangs on the session's row: open its workspace, drop what hides it.
        if focused && let Some(project_id) = self.project_of(session_id) {
            self.sidebar.reveal(project_id);
        }
        let had_keyboard = self.peek.as_ref().is_some_and(|peek| peek.focused);
        self.sidebar_layout.show();
        self.peek = Some(Peek::new(session_id, focused || had_keyboard));
        if focused && !had_keyboard {
            self.take_peek_focus(window, cx);
        }
        self.read_peek(window, cx);
        cx.notify();
    }

    /// Moves the keyboard into the open peek, remembering where it was.
    fn take_peek_focus(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let terminal = self.focused_session().filter(|session| {
            self.panes
                .get(session)
                .is_some_and(|pane| pane.read(cx).has_focus(window, cx))
        });
        let focused = window.focused(cx);
        if focused.as_ref() != Some(&self.peek_focus) {
            self.peek_return = focused.map(|focus| (focus, terminal));
        }
        if let Some(peek) = self.peek.as_mut() {
            peek.focused = true;
        }
        window.focus(&self.peek_focus, cx);
        cx.notify();
    }

    /// Closes the peek; if it had the keyboard, the keyboard goes back to where it was
    /// (unless that pane went away meanwhile).
    fn close_peek(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(peek) = self.peek.take() else {
            return;
        };
        if peek.focused {
            match self.peek_return.take() {
                Some((focus, session))
                    if session.is_none_or(|session| self.panes.contains_key(&session)) =>
                {
                    window.focus(&focus, cx);
                }
                _ => self.focus_pane(window, cx),
            }
        }
        cx.notify();
    }

    /// Reads the peeked session's last screen lines (`session.read`, never attaching).
    fn read_peek(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(peek) = self.peek.as_mut() else {
            return;
        };
        let session_id = peek.session_id;
        let (serial, params) = peek.read_params();
        let read = connection.call::<SessionReadResult>(Request::SessionRead(params));
        cx.spawn_in(window, async move |this, cx| {
            let read = read
                .await
                .map(|read| read.lines)
                .map_err(|error| format!("{error:#}"));
            let _ = this.update(cx, |shell, cx| {
                if let Some(peek) = shell
                    .peek
                    .as_mut()
                    .filter(|peek| peek.session_id == session_id)
                {
                    peek.read(serial, read);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// After sessions changed: the peek closes once its session ended or its row is no longer
    /// shown (a chip or filter now hides it), and reads the screen again when its session's
    /// state `changed`.
    fn update_peek(&mut self, changed: bool, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(peek) = self.peek.as_ref() else {
            return;
        };
        let session_id = peek.session_id;
        // The popover only draws on its row: without the row it would be invisible but keep
        // the keyboard.
        let row_shown = self
            .sidebar
            .view(&self.viewed)
            .rows
            .iter()
            .any(|row| matches!(row, Row::Session(row) if row.session_id == session_id));
        if !self.peekable(session_id) || !row_shown {
            return self.close_peek(window, cx);
        }
        if changed {
            self.read_peek(window, cx);
        }
    }

    /// The mouse entered or left a session row: resting on it for the hover delay peeks it
    /// (unless the open peek is in use); leaving lets an unused hover peek close.
    fn hover_row(
        &mut self,
        session_id: Ulid,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if !hovered {
            self.peek_hover.leave(session_id);
            self.close_unused_peek(window, cx);
            return;
        }
        let serial = self.peek_hover.enter(session_id);
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(theme::PEEK_HOVER_DELAY)
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let Some(session_id) = shell.peek_hover.fire(serial) else {
                    return;
                };
                let shown = shell
                    .peek
                    .as_ref()
                    .is_some_and(|peek| peek.session_id == session_id);
                if !shown && shell.peek.as_ref().is_none_or(Peek::yields_to_hover) {
                    shell.open_peek(session_id, false, window, cx);
                }
            });
        })
        .detach();
    }

    /// The mouse entered or left the peek popover.
    fn hover_peek(&mut self, hovered: bool, window: &mut Window, cx: &mut Context<'_, Self>) {
        if let Some(peek) = self.peek.as_mut() {
            peek.hovered = hovered;
            if !hovered {
                self.close_unused_peek(window, cx);
            }
        }
    }

    /// Closes a peek nobody uses once the mouse is neither on its row nor on it, after a short
    /// grace so the mouse can travel from the row to the popover.
    fn close_unused_peek(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.peek.is_none() {
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(theme::PEEK_LEAVE_GRACE)
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let Some(peek) = shell.peek.as_ref() else {
                    return;
                };
                if !peek.stays(shell.peek_hover.is_over(peek.session_id)) {
                    shell.close_peek(window, cx);
                }
            });
        })
        .detach();
    }

    /// Typing edits the reply, Enter sends it, ↑ / ↓ peek the other sessions that need you, Esc
    /// closes.
    fn peek_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(peek) = self.peek.as_mut() else {
            return;
        };
        match peek.key(&event.keystroke) {
            peek::Outcome::Edited => cx.notify(),
            peek::Outcome::Close => self.close_peek(window, cx),
            peek::Outcome::Send => {
                if let Some(params) = peek.send_params() {
                    self.send_peek(params, true, window, cx);
                }
            }
            peek::Outcome::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    peek.insert(&text);
                    cx.notify();
                }
            }
            peek::Outcome::Cycle(by) => {
                let current = peek.session_id;
                if let Some(next) = peek::cycle(&self.inbox_entries(), current, by)
                    && next != current
                {
                    self.open_peek(next, true, window, cx);
                }
            }
        }
    }

    /// A `y` / `n` button: sends that letter with submit while the session needs input.
    fn answer_peek(&mut self, letter: char, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(peek) = self.peek.as_ref() else {
            return;
        };
        let state = self
            .sidebar
            .sessions()
            .iter()
            .find(|session| session.id == peek.session_id)
            .map(|session| session.state);
        if let Some(params) = state.and_then(|state| peek.answer(letter, state)) {
            self.send_peek(params, false, window, cx);
        }
    }

    /// `session.send` from the peek; a sent `draft` empties the reply field.
    fn send_peek(
        &mut self,
        params: SessionSendParams,
        draft: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(peek) = self.peek.as_mut() else {
            return;
        };
        let session_id = peek.session_id;
        peek.busy = true;
        peek.error = None;
        cx.notify();
        let sent = connection.request(Request::SessionSend(params));
        cx.spawn_in(window, async move |this, cx| {
            let result = match sent.await {
                Ok(Outcome::Result(_)) => Ok(()),
                Ok(Outcome::Error(body)) => Err(body.message),
                Err(error) => Err(format!("{error:#}")),
            };
            let _ = this.update(cx, |shell, cx| {
                if let Err(error) = &result {
                    tracing::warn!(%session_id, "session.send failed: {error}");
                }
                // The peek was closed or moved to another session meanwhile.
                if let Some(peek) = shell
                    .peek
                    .as_mut()
                    .filter(|peek| peek.session_id == session_id)
                {
                    peek.sent(result, draft);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Routes a batch of incoming frames; returns whether a pane got new content.
    fn dispatch(
        &mut self,
        batch: Vec<Incoming>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let mut fed: Vec<Entity<TerminalPane>> = Vec::new();
        let mut layout_changed = false;
        let mut peek_changed = false;
        for incoming in batch {
            if let Incoming::Event(event) = &incoming {
                if self.board.apply(event) {
                    self.drop_gone_targets(window, cx);
                    cx.notify();
                }
                if self.turns.apply(event) {
                    cx.notify();
                }
                if self.usages.apply(event) {
                    cx.notify();
                }
                if self.plans.apply(event) {
                    cx.notify();
                }
                if self.ports.apply(event) {
                    cx.notify();
                }
                if self.composers.apply(event) {
                    cx.notify();
                }
                if let Some(wait) = self.follow.apply(event) {
                    self.follow_wait(wait, window, cx);
                }
                if let Event::SessionState(state) = event {
                    peek_changed |= self
                        .peek
                        .as_ref()
                        .is_some_and(|peek| peek.session_id == state.session_id);
                }
                match self.sidebar.apply(event) {
                    Change::None => {}
                    Change::Redraw => cx.notify(),
                    // A new session (e.g. spawned by `mk`): re-list.
                    Change::Refresh => self.refresh(window, cx),
                }
                // After the board and sidebar, so a task leaving Running clears its warning.
                let detected = self.conflicts.apply(event);
                if self.conflicts.prune(&self.board, self.sidebar.sessions()) || detected {
                    cx.notify();
                }
                for tabs in self.tabs.values_mut() {
                    layout_changed |= tabs.apply(event);
                }
                if let Event::TabUpdated(tab) = event
                    && self.loading_tabs.contains(&tab.project_id)
                {
                    self.early_tab_events.push(event.clone());
                }
                if let Event::TabUpdated(tab) = event
                    && self.run_reveal == Some(tab.id)
                {
                    self.reveal_run_tab(tab.project_id, window, cx);
                }
                if let Event::NotificationClicked(clicked) = event {
                    self.notification_clicked(clicked.session_id, clicked.project_id, window, cx);
                }
                if matches!(event, Event::DocChanged(_) | Event::CommentUpdated(_)) {
                    self.doc_event(event, window, cx);
                }
                if let Event::CommentUpdated(comment) = event {
                    for view in self.git.values_mut() {
                        view.comments.apply_event(view.project_id, comment);
                    }
                    cx.notify();
                }
                if matches!(event, Event::GitChanged(_)) {
                    self.files_event(event, window, cx);
                    self.git_event(event, window, cx);
                    if let Event::GitChanged(changed) = event
                        && changed.worktree_task_id.is_none()
                        && self.workspace == Some(changed.project_id)
                    {
                        self.load_git_summary(changed.project_id, window, cx);
                    }
                }
                if let Event::AiGenerated(done) = event {
                    let result = ai_result(done);
                    for view in self.git.values_mut() {
                        view.commit.generated(done.generation_id, result.clone());
                    }
                    self.composers.enhanced(done.generation_id, result);
                    cx.notify();
                }
            }
            let Some(delivery) = self.router.route(incoming) else {
                continue;
            };
            let Some(pane) = self.panes.get(&delivery.session_id()).cloned() else {
                continue;
            };
            pane.update(cx, |pane, _| match delivery {
                Delivery::Output { data, .. } => pane.feed(&data),
                Delivery::Snapshot { snapshot, .. } => pane.load_snapshot(&snapshot),
                Delivery::Exited { exit_code, .. } => pane.set_exited(exit_code),
            });
            if !fed.iter().any(|p| p.entity_id() == pane.entity_id()) {
                fed.push(pane);
            }
        }
        if layout_changed {
            self.sync_panes(window, cx);
            cx.notify();
        }
        if self.peek.is_some() {
            self.update_peek(peek_changed, window, cx);
        }
        for pane in &fed {
            pane.update(cx, |_, cx| cx.notify());
        }
        !fed.is_empty()
    }

    /// One short sentence, plus optional detail shown smaller below it.
    fn status_line(&self) -> (SharedString, Option<SharedString>) {
        match &self.status {
            Status::Connecting => ("Connecting to the daemon…".into(), None),
            Status::Connected if self.sidebar.is_empty() => (
                "No workspaces yet.".into(),
                Some("Add a project folder from the sidebar to get started.".into()),
            ),
            Status::Connected => ("Opening the workspace…".into(), None),
            Status::Disconnected => (
                "The daemon connection closed. Restart Mkrew to reconnect.".into(),
                None,
            ),
            Status::Failed(error) => (
                "Cannot reach the daemon.".into(),
                Some(error.clone().into()),
            ),
            Status::ListFailed(error) => (
                "Cannot load the workspace.".into(),
                Some(error.clone().into()),
            ),
        }
    }

    fn render_sidebar(
        &self,
        now: i64,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let shown = self.focused_session();
        let connected = self.connection.is_some();
        let add_workspace_keys = self.keymap.label(Command::AddWorkspace);
        let view = self.sidebar.view(&self.viewed);
        let any_open = self.sidebar.any_expanded();
        let header = div()
            .flex_none()
            .h(theme::rems_px(theme::WORKSPACE_ROW_HEIGHT))
            .flex()
            .items_center()
            .justify_between()
            .px(px(space::M))
            .child(
                div()
                    .text_size(font_size::META)
                    .font_weight(FontWeight(weight::MEDIUM))
                    .text_color(rgb(theme.text_muted))
                    .child("Workspaces"),
            );
        let header = header.when(connected && self.sidebar.is_empty(), |header| {
            header.child(
                icon_button("add-workspace", "+", theme)
                    .tooltip(tooltip(theme, "Add workspace", add_workspace_keys.clone()))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.add_workspace(window, cx);
                    })),
            )
        });
        let header = header.when(connected && !self.sidebar.is_empty(), |header| {
            header.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .child(
                        icon_button(
                            "collapse-workspaces",
                            if any_open { "▾" } else { "▸" },
                            theme,
                        )
                        .tooltip(tooltip(
                            theme,
                            if any_open {
                                "Collapse all workspaces"
                            } else {
                                "Expand all workspaces"
                            },
                            "",
                        ))
                        .on_click(cx.listener(|shell, _, _, cx| {
                            shell.sidebar.toggle_all();
                            cx.notify();
                        })),
                    )
                    .child(
                        icon_button("add-workspace", "+", theme)
                            .tooltip(tooltip(theme, "Add workspace", add_workspace_keys.clone()))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.add_workspace(window, cx);
                            })),
                    ),
            )
        });
        let filter_focused = self.filter_focus.is_focused(window);
        let controls = (connected && !self.sidebar.is_empty()).then(|| {
            let filter = self.sidebar.filter();
            let hover = theme.hover;
            let chip =
                |id: &'static str, label: &'static str, count: usize, which: sidebar::Chip| {
                    let active = self.sidebar.chip() == which;
                    div()
                        .id(id)
                        .flex_none()
                        .px(px(space::S))
                        .rounded_full()
                        .cursor_pointer()
                        .text_size(font_size::META)
                        .when(active, |el| {
                            el.bg(rgb(theme.accent_bg))
                                .text_color(rgb(theme.accent_text))
                        })
                        .when(!active, |el| {
                            el.text_color(rgb(theme.text_muted))
                                .hover(move |style| style.bg(rgb(hover)))
                        })
                        .child(format!("{label} {count}"))
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            shell.sidebar.set_chip(which);
                            cx.notify();
                        }))
                };
            div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(space::S))
                .px(px(space::M))
                .pb(px(space::S))
                .child(
                    // A tracked element takes the keyboard on a click.
                    text_field(
                        "sidebar-filter",
                        filter.as_str(),
                        "Filter sessions…",
                        filter_focused,
                        filter.all_selected(),
                        theme,
                    )
                    .track_focus(&self.filter_focus)
                    .on_key_down(cx.listener(Self::filter_key_down))
                    .when(filter_focused, |el| {
                        el.relative().child(self.typing(
                            &self.filter_focus,
                            Self::filter_key_down,
                            cx,
                        ))
                    }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(space::XS))
                        .child(chip("chip-all", "All", view.counts.all, sidebar::Chip::All))
                        .child(chip(
                            "chip-working",
                            "Working",
                            view.counts.working,
                            sidebar::Chip::Working,
                        ))
                        .child(chip(
                            "chip-needs-you",
                            "Needs you",
                            view.counts.needs_you,
                            sidebar::Chip::NeedsYou,
                        )),
                )
        });
        let errors = [&self.sidebar_error, &self.keymap_error].map(|error| {
            error.clone().map(|error| {
                div()
                    .px(px(space::M))
                    .pb(px(space::S))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(format!("✕ {error}"))
            })
        });
        let body = if connected && self.sidebar.is_empty() {
            // Empty state: one sentence plus the primary action with its shortcut.
            empty_state(theme, None, None, "No workspaces yet.".into(), None)
                .child(
                    button("add-first-workspace", "Add workspace", theme, true)
                        .child(kbd(add_workspace_keys, theme, true))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.add_workspace(window, cx);
                        })),
                )
                .into_any_element()
        } else {
            // A workspace's sessions hang under a faint guide line.
            let guide = |rows: Vec<gpui::AnyElement>| {
                div()
                    .ml(px(space::L))
                    .pl(px(space::XS))
                    .border_l(px(theme::BORDER_WIDTH))
                    .border_color(rgb(theme.subtle))
                    .flex()
                    .flex_col()
                    .children(rows)
                    .into_any_element()
            };
            let mut list = Vec::new();
            let mut sessions = Vec::new();
            for row in view.rows {
                match row {
                    Row::Workspace(row) => {
                        if !sessions.is_empty() {
                            list.push(guide(std::mem::take(&mut sessions)));
                        }
                        let (project_id, active) =
                            (row.project_id, self.workspace == Some(row.project_id));
                        let summary = self.run_summary(project_id);
                        let run_tip = summary.any_live().then(|| summary.tooltip());
                        let row = workspace_row(row, active, run_tip, theme, cx);
                        list.push(self.with_workspace_menu(row, project_id, theme, cx));
                    }
                    Row::Session(row) => {
                        let (selected, session_id) =
                            (shown == Some(row.session_id), row.session_id);
                        // One lookup, for the selected row only.
                        let progress = selected.then(|| self.plans.fraction(session_id)).flatten();
                        let badge = self.agent_badge(&row.agent);
                        let row = session_row(row, badge, selected, progress, now, theme, cx);
                        sessions.push(self.with_peek(row, session_id, now, theme, cx));
                    }
                    Row::NoSessions { .. } => sessions.push(
                        div()
                            .px(px(space::S))
                            .py(px(space::XS))
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child("No sessions yet.")
                            .into_any_element(),
                    ),
                }
            }
            if !sessions.is_empty() {
                list.push(guide(sessions));
            }
            if list.is_empty() && self.sidebar.filtering() {
                // A query or chip that nothing matches.
                list.push(
                    div()
                        .px(px(space::S))
                        .py(px(space::XS))
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child("No sessions match.")
                        .into_any_element(),
                );
            }
            div()
                .flex()
                .flex_col()
                .px(px(space::XS))
                .children(list)
                .into_any_element()
        };
        // The header and the filter stay in view; the workspace list scrolls below them.
        div()
            .flex_none()
            .w(px(theme::SIDEBAR_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.sidebar))
            .border_r(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .child(header)
            .children(controls)
            .child(
                div()
                    .id("sidebar")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .children(errors.into_iter().flatten())
                    .child(body),
            )
    }

    /// The compact rail (Toggle compact sidebar): per workspace its avatar, per open workspace
    /// its sessions' status marks and agent chips; each row's tooltip spells out the rest. Same
    /// rows, clicks, right-click menus and keyboard peek as the full sidebar; no filter, chips
    /// or hover peek (the tooltip replaces it). Without workspaces only `+` shows.
    fn render_compact_sidebar(
        &self,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let shown = self.focused_session();
        let header = div()
            .flex_none()
            .h(theme::rems_px(theme::WORKSPACE_ROW_HEIGHT))
            .flex()
            .items_center()
            .justify_center()
            .when(self.connection.is_some(), |header| {
                header.child(
                    icon_button("add-workspace", "+", theme)
                        .tooltip(tooltip(
                            theme,
                            "Add workspace",
                            self.keymap.label(Command::AddWorkspace),
                        ))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.add_workspace(window, cx);
                        })),
                )
            });
        // An error is too long for the rail: its mark, with the text on hover.
        let mut list = Vec::new();
        for (i, error) in [&self.sidebar_error, &self.keymap_error]
            .into_iter()
            .enumerate()
        {
            if let Some(error) = error {
                list.push(
                    div()
                        .id(SharedString::from(format!("sidebar-error-{i}")))
                        .flex()
                        .justify_center()
                        .py(px(space::XS))
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .tooltip(tooltip(theme, error, ""))
                        .child("✕")
                        .into_any_element(),
                );
            }
        }
        for row in self.sidebar.view(&self.viewed).rows {
            match row {
                Row::Workspace(row) => {
                    let (project_id, active) =
                        (row.project_id, self.workspace == Some(row.project_id));
                    let row = compact_workspace_row(row, active, theme, cx);
                    list.push(self.with_workspace_menu(row, project_id, theme, cx));
                }
                Row::Session(row) => {
                    let session_id = row.session_id;
                    let badge = self.agent_badge(&row.agent);
                    let row =
                        compact_session_row(row, badge, shown == Some(session_id), now, theme, cx);
                    list.push(self.with_peek(row, session_id, now, theme, cx));
                }
                // The workspace's tooltip already shows it has nothing running (no summary).
                Row::NoSessions { .. } => {}
            }
        }
        div()
            .flex_none()
            .w(theme::rems_px(theme::COMPACT_SIDEBAR_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.sidebar))
            .border_r(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .child(header)
            .child(
                div()
                    .id("sidebar")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .px(px(space::XS))
                    .overflow_y_scroll()
                    .children(list),
            )
    }

    /// The status bar (SPEC §10): sidebar toggles (hide, compact), Inbox state, git HEAD; then the focused agent
    /// pane's usage. Everything is read from state the shell already keeps: it never asks the
    /// daemon and has no timer (its elapsed time refreshes with the `STATUS_TICK` redraw).
    /// The last child is the right-panel toggle (`inspector_toggle`).
    fn render_status_bar(&self, now: i64, theme: &Theme, cx: &mut Context<'_, Self>) -> gpui::Div {
        let hover = theme.hover;
        // Behind a dialog the buttons do nothing, like the keys (`Shell::run`).
        let modal = self.modal_open();
        let toggle = div()
            .id("toggle-sidebar")
            .flex_none()
            .px(px(space::XS))
            .rounded(px(radius::SMALL))
            .cursor_pointer()
            .text_color(rgb(theme.text_muted))
            .hover(move |style| style.bg(rgb(hover)))
            .tooltip(tooltip(
                theme,
                if self.sidebar_layout.hidden() {
                    "Show sidebar"
                } else {
                    "Hide sidebar"
                },
                self.keymap.label(Command::ToggleSidebar),
            ))
            .on_click(cx.listener(|shell, _, window, cx| shell.toggle_sidebar(window, cx)))
            .child("◧")
            .when(modal, |el| el.relative().child(inert_layer()));
        // Full width ↔ compact rail (a hidden sidebar comes back as the rail).
        let compact = self.sidebar_layout.compact();
        let compact_toggle = div()
            .id("toggle-compact-sidebar")
            .flex_none()
            .px(px(space::XS))
            .rounded(px(radius::SMALL))
            .cursor_pointer()
            .text_color(rgb(theme.text_muted))
            .hover(move |style| style.bg(rgb(hover)))
            .tooltip(tooltip(
                theme,
                if compact {
                    "Expand sidebar"
                } else {
                    "Compact sidebar"
                },
                self.keymap.label(Command::ToggleCompactSidebar),
            ))
            .on_click(cx.listener(|shell, _, window, cx| {
                shell.toggle_compact_sidebar(window, cx);
            }))
            .child(if compact { "⇥" } else { "⇤" })
            .when(modal, |el| el.relative().child(inert_layer()));

        let waiting = self.inbox_entries().len();
        let inbox = div()
            .id("status-inbox")
            .flex_none()
            .px(px(space::XS))
            .rounded(px(radius::SMALL))
            .text_color(rgb(if waiting > 0 {
                theme.status_needs_input
            } else {
                theme.text_muted
            }))
            .child(inbox::label(waiting))
            // Nothing to jump to while clear: no pointer, no hover, no click.
            .when(waiting > 0, |el| {
                el.cursor_pointer()
                    .hover(move |style| style.bg(rgb(hover)))
                    .tooltip(tooltip(
                        theme,
                        "Jump to next item needing attention",
                        self.keymap.label(Command::NextInbox),
                    ))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.jump_next_inbox(window, cx);
                    }))
            })
            .when(modal, |el| el.relative().child(inert_layer()));

        let git = self
            .git_summary()
            .and_then(|git| Some((git.head_label()?, git.branch_tooltip()?)))
            .map(|(label, lines)| {
                div()
                    .id("status-git")
                    .flex_none()
                    .text_color(rgb(theme.text_muted))
                    .tooltip(tooltip(theme, &lines, ""))
                    .child(label)
            });

        let agent = self
            .focused_session()
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id))
            .filter(|session| session.kind == SessionKind::Agent)
            .map(|session| self.status_agent_fields(session, now, theme))
            .unwrap_or_default();

        div()
            .flex_none()
            .h(theme::rems_px(theme::STATUS_BAR_HEIGHT))
            .flex()
            .items_center()
            .gap(px(space::M))
            .px(px(space::S))
            .overflow_hidden()
            .bg(rgb(theme.sidebar))
            .border_t(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .text_size(font_size::META)
            .child(toggle)
            .child(compact_toggle)
            .child(inbox)
            .children(git)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(space::M))
                    .children(agent),
            )
            .child(self.inspector_toggle(theme, cx))
    }

    /// The right-end status-bar button: shows or hides the session panel.
    fn inspector_toggle(
        &self,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let hover = theme.hover;
        div()
            .id("toggle-inspector")
            .flex_none()
            .px(px(space::XS))
            .rounded(px(radius::SMALL))
            .cursor_pointer()
            .text_color(rgb(if self.inspector_open {
                theme.accent_text
            } else {
                theme.text_muted
            }))
            .when(self.inspector_open, |el| el.bg(rgb(theme.accent_bg)))
            .when(!self.inspector_open, |el| {
                el.hover(move |style| style.bg(rgb(hover)))
            })
            .tooltip(tooltip(
                theme,
                if self.inspector_open {
                    "Hide session panel"
                } else {
                    "Show session panel"
                },
                self.keymap.label(Command::ToggleInspector),
            ))
            .on_click(cx.listener(|shell, _, window, cx| shell.toggle_inspector(window, cx)))
            .child("◨")
            .when(self.modal_open(), |el| el.relative().child(inert_layer()))
    }

    /// The focused agent's fields, in order: context meter, tokens, cache, model, elapsed.
    /// Each is left out without data (never `0`): no usage entry yet, no window, no cache,
    /// no model, a state without a time (`needs input`, `done`, `exited`).
    fn status_agent_fields(
        &self,
        session: &Session,
        now: i64,
        theme: &Theme,
    ) -> Vec<gpui::AnyElement> {
        let text = |text: String| {
            div()
                .flex_none()
                .text_color(rgb(theme.text_muted))
                .child(text)
                .into_any_element()
        };
        let mut fields = Vec::new();
        if let Some(shown) = self.usages.get(session.id).map(usage::fields) {
            fields.extend(
                shown
                    .context
                    .map(|share| context_meter(&share, theme).into_any_element()),
            );
            fields.extend(shown.tokens.map(text));
            fields.extend(shown.cache.map(text));
            fields.extend(shown.model.map(text));
        }
        // The pane header's time (`status::time`), so the two never disagree.
        if let Some(time) = status::time(session.state, session.state_changed_at, now) {
            fields.push(
                div()
                    .id("status-elapsed")
                    .flex_none()
                    .text_color(rgb(theme.text_muted))
                    .tooltip(tooltip(
                        theme,
                        &status::hover(
                            session.state,
                            session.state_source,
                            session.state_changed_at,
                            now,
                        ),
                        "",
                    ))
                    .child(time)
                    .into_any_element(),
            );
        }
        fields
    }

    /// The workspace menu: "Pin", "Open in Zed", "Run setup…", the default agent and the default shell
    /// for new panes, the workspace color and "Remove workspace…". Floats over the sidebar; a
    /// click outside closes it.
    fn render_workspace_menu(
        &self,
        menu: &WorkspaceMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let project_id = menu.project_id;
        let current = self
            .sidebar
            .projects()
            .iter()
            .find(|project| project.id == project_id)
            .and_then(|project| project.default_agent.as_deref());
        let current_color = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .and_then(|project| project.color.as_deref());
        let pinned = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .is_some_and(|project| project.pinned);
        let separator = || {
            div()
                .h(px(theme::BORDER_WIDTH))
                .my(px(space::XS))
                .bg(rgb(theme.border))
        };
        let agents = agent_rows(
            &menu.agents,
            SessionKind::Agent,
            current,
            "default-agent",
            theme,
            cx,
            move |shell, agent, window, cx| shell.set_default_agent(project_id, agent, window, cx),
        );
        let current_shell = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .and_then(|project| project.default_shell.as_deref());
        // Loading and errors already show once, under the agents.
        let shells = match &menu.agents {
            Agents::Loaded(_) => agent_rows(
                &menu.agents,
                SessionKind::Shell,
                current_shell,
                "default-shell",
                theme,
                cx,
                move |shell, id, window, cx| shell.set_default_shell(project_id, id, window, cx),
            ),
            _ => Vec::new(),
        };
        let heading = |text: &'static str| {
            div()
                .px(px(space::S))
                .pt(px(space::XS))
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let panel = menu_panel("workspace-menu", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.workspace_menu = None;
                cx.notify();
            }))
            .child(
                menu_item("pin-workspace".into(), theme)
                    .child(div().flex_1().text_color(rgb(theme.text)).child(if pinned {
                        "Unpin"
                    } else {
                        "Pin"
                    }))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.pin_workspace(project_id, !pinned, window, cx);
                    })),
            )
            .child(separator())
            .child(
                menu_item("open-in-zed".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child("Open in Zed"),
                    )
                    .child(
                        div()
                            .text_size(font_size::META)
                            .child(self.keymap.label(Command::OpenInZed)),
                    )
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        shell.open_in_zed(project_id, cx);
                    })),
            )
            .child(
                menu_item("open-in-explorer".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child("Open in File Explorer"),
                    )
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        shell.open_workspace_in_explorer(project_id, cx);
                    })),
            )
            .child(separator())
            .child(
                menu_item("workspace-run-setup".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child("Run setup…"),
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.open_run_setup(project_id, false, window, cx);
                    })),
            )
            .child(separator())
            .child(heading("Default agent for new panes"))
            .children(agents)
            .child(separator())
            .child(heading("Default shell for new panes"))
            .children(shells)
            .child(separator())
            .child(heading("Color"))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(space::XS))
                    .px(px(space::S))
                    .py(px(space::S))
                    .child(color_swatch(
                        "workspace-color",
                        None,
                        current_color,
                        theme,
                        cx,
                        move |shell, color, window, cx| {
                            shell.set_workspace_color(project_id, color, window, cx);
                        },
                    ))
                    .children(WORKSPACE_COLORS.iter().map(|name| {
                        color_swatch(
                            "workspace-color",
                            Some(*name),
                            current_color,
                            theme,
                            cx,
                            move |shell, color, window, cx| {
                                shell.set_workspace_color(project_id, color, window, cx);
                            },
                        )
                    })),
            )
            .child(separator())
            .child(
                menu_item("remove-workspace".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            // The destructive color of `danger_button`.
                            .text_color(rgb(theme.diff_del_text))
                            .child("Remove workspace…"),
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.archive_workspace(project_id, window, cx);
                    })),
            );
        deferred(
            anchored()
                .snap_to_window_with_margin(px(space::S))
                .child(div().pt(px(space::XS)).child(panel)),
        )
        .into_any_element()
    }

    /// The Files tree's right-click menu at the click position; a click outside closes it.
    fn render_file_menu(
        &self,
        menu: &FileMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let (project_id, target, is_dir) = (menu.project_id, menu.target, menu.is_dir);
        let (explorer_path, zed_path) = (menu.path.clone(), menu.path.clone());
        let item = |id: &'static str, label: &'static str| {
            menu_item(id.into(), theme)
                .child(div().flex_1().text_color(rgb(theme.text)).child(label))
        };
        let panel = menu_panel("file-menu", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.file_menu = None;
                cx.notify();
            }))
            .child(
                item("file-menu-explorer", "Open in File Explorer").on_click(cx.listener(
                    move |shell, _, _, cx| {
                        shell.open_path_in_explorer(project_id, target, &explorer_path, is_dir, cx);
                    },
                )),
            )
            .child(item("file-menu-zed", "Open in Zed").on_click(cx.listener(
                move |shell, _, _, cx| {
                    shell.open_path_in_zed(project_id, target, &zed_path, cx);
                },
            )));
        deferred(
            anchored()
                .position(menu.position)
                .snap_to_window_with_margin(px(space::S))
                .child(panel),
        )
        .into_any_element()
    }

    /// The tab menu: Pin, Split right, Split down, Rename, Close (shortcuts on the right). Floats at
    /// the click; a click outside closes it.
    fn render_tab_menu(
        &self,
        menu: &TabMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let tab_id = menu.tab_id;
        let pinned = (self.tabs.values())
            .find_map(|tabs| tabs.tab(tab_id))
            .is_some_and(|tab| tab.pinned);
        let separator = || {
            div()
                .h(px(theme::BORDER_WIDTH))
                .my(px(space::XS))
                .bg(rgb(theme.border))
        };
        let item = |id: &'static str, label: &'static str, command: Command| {
            menu_item(id.into(), theme)
                .child(div().flex_1().text_color(rgb(theme.text)).child(label))
                .child(
                    div()
                        .text_size(font_size::META)
                        .child(self.keymap.label(command)),
                )
        };
        let panel = menu_panel("tab-menu", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.tab_menu = None;
                cx.notify();
            }))
            .child(
                menu_item("pin-tab".into(), theme)
                    .child(div().flex_1().text_color(rgb(theme.text)).child(if pinned {
                        "Unpin"
                    } else {
                        "Pin"
                    }))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.pin_tab(tab_id, !pinned, window, cx);
                    })),
            )
            .child(separator())
            .child(
                item("tab-menu-split-right", "Split right", Command::SplitRight).on_click(
                    cx.listener(|shell, _, window, cx| {
                        shell.tab_menu = None;
                        shell.run(&Run(Command::SplitRight), window, cx);
                    }),
                ),
            )
            .child(
                item("tab-menu-split-down", "Split down", Command::SplitDown).on_click(
                    cx.listener(|shell, _, window, cx| {
                        shell.tab_menu = None;
                        shell.run(&Run(Command::SplitDown), window, cx);
                    }),
                ),
            )
            .child(
                item("tab-menu-rename", "Rename", Command::RenameTab).on_click(cx.listener(
                    |shell, _, window, cx| {
                        shell.tab_menu = None;
                        shell.start_rename(window, cx);
                    },
                )),
            )
            .child(
                item("tab-menu-close", "Close", Command::CloseTab).on_click(cx.listener(
                    move |shell, _, window, cx| {
                        shell.tab_menu = None;
                        shell.close_tab(tab_id, window, cx);
                    },
                )),
            );
        deferred(
            anchored()
                .position(menu.position)
                .snap_to_window_with_margin(px(space::S))
                .child(panel),
        )
        .into_any_element()
    }

    /// The pane menu: Pin, Rename (inline, like F2; greyed for a run pane, which is named by its
    /// command) and the color swatches of the workspace palette. Floats at the click.
    fn render_pane_menu(
        &self,
        menu: &PaneMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        let session_id = menu.session_id;
        let session = self
            .sidebar
            .sessions()
            .iter()
            .find(|s| s.id == session_id)?;
        let renamable = mkrew_core::run::command_name(session).is_none();
        let current = session.color.as_deref();
        let pinned = session.pinned;
        let separator = || {
            div()
                .h(px(theme::BORDER_WIDTH))
                .my(px(space::XS))
                .bg(rgb(theme.border))
        };
        let panel = menu_panel("pane-menu", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.pane_menu = None;
                cx.notify();
            }))
            .child(
                menu_item("pin-pane".into(), theme)
                    .child(div().flex_1().text_color(rgb(theme.text)).child(if pinned {
                        "Unpin"
                    } else {
                        "Pin"
                    }))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.pin_session(session_id, !pinned, window, cx);
                    })),
            )
            .child(separator())
            .child(
                menu_item("pane-menu-rename".into(), theme)
                    .child(div().flex_1().text_color(rgb(theme.text)).child("Rename"))
                    .child(
                        div()
                            .text_size(font_size::META)
                            .child(self.keymap.label(Command::RenamePane)),
                    )
                    .when(renamable, |el| {
                        el.on_click(cx.listener(move |shell, _, window, cx| {
                            shell.pane_menu = None;
                            shell.start_pane_rename(session_id, window, cx);
                        }))
                    })
                    .when(!renamable, |el| el.opacity(opacity::DISABLED)),
            )
            .child(separator())
            .child(
                div()
                    .px(px(space::S))
                    .pt(px(space::XS))
                    .text_size(font_size::META)
                    .font_weight(FontWeight(weight::MEDIUM))
                    .text_color(rgb(theme.text_muted))
                    .child("Change color"),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(space::S))
                    .p(px(space::S))
                    .children(
                        std::iter::once(None)
                            .chain(WORKSPACE_COLORS.iter().map(|name| Some(*name)))
                            .map(|color| {
                                color_swatch(
                                    "pane-color",
                                    color,
                                    current,
                                    theme,
                                    cx,
                                    move |shell, color, window, cx| {
                                        shell.set_pane_color(session_id, color, window, cx);
                                    },
                                )
                            }),
                    ),
            );
        Some(
            deferred(
                anchored()
                    .position(menu.position)
                    .snap_to_window_with_margin(px(space::S))
                    .child(panel),
            )
            .into_any_element(),
        )
    }

    /// `row` with the workspace menu dropping down below it while the menu is open for
    /// `project_id` (both sidebar widths).
    fn with_workspace_menu(
        &self,
        row: gpui::AnyElement,
        project_id: Ulid,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        match self
            .workspace_menu
            .as_ref()
            .filter(|menu| menu.project_id == project_id)
        {
            Some(menu) => div()
                .child(row)
                .child(self.render_workspace_menu(menu, theme, cx))
                .into_any_element(),
            None => row,
        }
    }

    /// A left click on a workspace row: the first shows the workspace; clicking the shown one
    /// expands/collapses it.
    fn click_workspace_row(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.workspace == Some(project_id) {
            self.sidebar.toggle(project_id);
            cx.notify();
        } else {
            self.activate_workspace(project_id, window, cx);
        }
    }

    /// `row` with the peek popover hung at its top right when the peek shows `session_id`.
    fn with_peek(
        &self,
        row: gpui::AnyElement,
        session_id: Ulid,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let popover = self
            .peek
            .as_ref()
            .filter(|peek| peek.session_id == session_id && !self.modal_open())
            .and_then(|peek| self.render_peek(peek, now, theme, cx));
        match popover {
            Some(popover) => div()
                .relative()
                .child(row)
                .child(div().absolute().top_0().right_0().child(popover))
                .into_any_element(),
            None => row,
        }
    }

    /// The peek popover (§20A.1): the session's name, state and its age, its last screen
    /// lines, a one-line reply and `y` / `n` while it needs input. Floats right of its row; a
    /// click outside closes it.
    fn render_peek(
        &self,
        peek: &Peek,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        let session = self
            .sidebar
            .sessions()
            .iter()
            .find(|session| session.id == peek.session_id)?;
        let workspace = self
            .sidebar
            .projects()
            .iter()
            .find(|project| project.id == session.project_id)
            .map_or("", |project| project.name.as_str());
        let name = match history::title(session) {
            Some(title) => format!("{workspace}: {} · {title}", session.agent_profile),
            None => format!("{workspace}: {}", session.agent_profile),
        };
        let muted = |text: String| {
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let screen = match &peek.screen {
            peek::Screen::Reading => muted("Reading the screen…".into()),
            peek::Screen::Failed(error) => muted(format!("Cannot read the screen: {error}")),
            peek::Screen::Read(lines) if lines.is_empty() => muted("The screen is empty.".into()),
            peek::Screen::Read(lines) => div()
                .flex()
                .flex_col()
                .font_family(theme.terminal_font_family.clone())
                .text_size(font_size::META)
                .text_color(rgb(theme.text))
                .children(lines.iter().map(|line| {
                    div()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(line.clone())
                })),
        };
        let needs_input = session.state == SessionState::NeedsInput;
        let answer = |id: &'static str, letter: char| {
            let (label, tip) = match letter {
                'y' => ("y", "Send y + Enter"),
                _ => ("n", "Send n + Enter"),
            };
            if peek.busy {
                disabled_button(id, label, theme, false)
            } else {
                button(id, label, theme, false)
                    .tooltip(tooltip(theme, tip, ""))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.answer_peek(letter, window, cx);
                    }))
            }
        };
        let reply = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(
                div().flex_1().min_w_0().child(
                    text_field(
                        "peek-reply",
                        &peek.text,
                        "Reply to the agent",
                        peek.focused,
                        peek.text.all_selected(),
                        theme,
                    )
                    .when(peek.busy, |el| el.opacity(opacity::DISABLED))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        if shell.peek.as_ref().is_some_and(|peek| !peek.focused) {
                            shell.take_peek_focus(window, cx);
                        }
                    })),
                ),
            )
            .when(needs_input, |el| {
                el.child(answer("peek-yes", 'y'))
                    .child(answer("peek-no", 'n'))
            });
        let hint = if peek.busy {
            "Sending…".to_owned()
        } else if !peek.focused {
            format!(
                "Click the field to reply · {} peeks the first session that needs you",
                self.keymap.label(Command::Peek)
            )
        } else {
            "Enter sends · ↑ ↓ other sessions that need you · Esc closes".to_owned()
        };
        let panel = div()
            .id("peek")
            .w(px(theme::PEEK_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::S))
            .p(px(space::M))
            .rounded(px(radius::MEDIUM))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            // Only once it has the keyboard: a tracked element takes focus on any mouse down,
            // so a click on a hover peek would steal the keyboard before `take_peek_focus`.
            .when(peek.focused, |el| el.track_focus(&self.peek_focus))
            .on_key_down(cx.listener(Self::peek_key_down))
            .when(peek.focused, |el| {
                el.relative()
                    .child(self.typing(&self.peek_focus, Self::peek_key_down, cx))
            })
            .on_mouse_down_out(cx.listener(|shell, _, window, cx| {
                shell.close_peek(window, cx);
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(
                        div()
                            .flex_none()
                            .text_size(font_size::BODY)
                            .child(status_mark(session.state, now, theme)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_size(font_size::BODY)
                            .font_weight(FontWeight(weight::MEDIUM))
                            .text_color(rgb(theme.text))
                            .child(name),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child(status::text(
                                session.state,
                                session.exit_code,
                                session.state_changed_at,
                                now,
                            )),
                    ),
            )
            .child(
                div()
                    .p(px(space::S))
                    .rounded(px(radius::SMALL))
                    .bg(rgb(theme.bg))
                    .child(screen),
            )
            .child(reply)
            .children(peek.error.clone().map(|error| {
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.status_exited))
                    .child(error)
            }))
            .child(muted(hint));
        // The padding bridges the row's edge, so the mouse can travel onto the popover.
        let area = div()
            .id("peek-area")
            .pl(px(space::S))
            .on_hover(cx.listener(|shell, hovered: &bool, window, cx| {
                shell.hover_peek(*hovered, window, cx);
            }))
            .child(panel);
        Some(
            deferred(
                anchored()
                    .snap_to_window_with_margin(px(space::S))
                    .child(area),
            )
            .into_any_element(),
        )
    }

    fn render_main(
        &self,
        now: i64,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let Some(project_id) = self.workspace.filter(|_| self.connection.is_some()) else {
            let (sentence, detail) = self.status_line();
            return message(theme, Some("Mkrew"), sentence, detail)
                .flex_1()
                .h_full()
                .into_any_element();
        };
        let view = self.view();
        let main = div().flex_1().min_w_0().h_full().flex().flex_col();
        match (view, self.tabs.get(&project_id)) {
            (View::Board, _) => main.child(self.render_board(project_id, now, theme, window, cx)),
            (View::Docs, _) => main.child(self.render_docs(project_id, theme, window, cx)),
            (View::Files, _) => main.child(self.render_files(project_id, theme, window, cx)),
            (View::Git, _) => main.child(self.render_git(project_id, theme, window, cx)),
            (View::Terminals, Some(tabs)) => {
                let terminals = div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(self.render_tab_bar(project_id, tabs, now, theme, cx))
                    .child(self.render_tab(
                        tabs,
                        now,
                        self.composer_focus.is_focused(window),
                        theme,
                        cx,
                    ));
                main.child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .child(terminals)
                        .when(self.inspector_open, |el| {
                            el.child(self.render_inspector(project_id, now, theme, cx))
                        }),
                )
            }
            (View::Terminals, None) => {
                let (sentence, detail) = self.status_line();
                main.child(message(theme, None, sentence, detail).flex_1())
            }
        }
        .into_any_element()
    }

    /// The title bar above everything (SPEC §10, ADR 0034): logo, workspace switcher,
    /// branch chip and views on the left; a drag area; the theme toggle, History, "+ Agent",
    /// "+ Shell" and Run; on Windows ─ □ ✕. Without a workspace only the logo, the drag area and
    /// the controls show, so the window can always be moved and closed.
    fn render_title_bar(
        &self,
        shown: View,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let workspace = self.workspace.filter(|_| self.connection.is_some());
        let mode = match theme.mode {
            theme::ThemeMode::System => "Theme: System",
            theme::ThemeMode::Light => "Theme: Light",
            theme::ThemeMode::Dark => "Theme: Dark",
        };
        // An open dialog keeps the mouse too: the bar's controls are inert, only the drag area
        // and the window controls stay live (they never touch the app's state).
        let modal = self.modal_open();
        let mut left = div()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(div().flex_none().child(logo(theme)));
        if let Some(project_id) = workspace {
            left = left
                .child(self.render_workspace_switcher(project_id, theme, cx))
                .children(self.render_branch_chip(theme, cx))
                .child(self.render_view_tabs(shown, theme, cx));
        }
        // A sibling of the controls, never their parent: the first area a hit test finds wins.
        let drag = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .window_control_area(WindowControlArea::Drag)
            .when(cfg!(target_os = "macos"), |el| {
                el.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, event: &MouseDownEvent, window, _| {
                        if event.click_count == 2 {
                            window.titlebar_double_click();
                        } else {
                            window.start_window_move();
                        }
                    }),
                )
            });
        let mut bar = div()
            .flex_none()
            .h(theme::rems_px(theme::TITLE_BAR_HEIGHT))
            .flex()
            .items_center()
            .gap(px(space::S))
            .pl(px(if cfg!(target_os = "macos") {
                theme::MAC_TRAFFIC_LIGHTS_WIDTH
            } else {
                space::M
            }))
            .bg(rgb(theme.bg))
            .border_b(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .child(left.when(modal, |el| el.relative().child(inert_layer())))
            .child(drag);
        let controls = DRAWN_CONTROLS && !window.is_fullscreen();
        let mut right = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::S))
            .when(!controls, |el| el.pr(px(space::M)))
            .child(
                icon_button("toggle-theme", theme::mode_glyph(theme.mode), theme)
                    .tooltip(tooltip(
                        theme,
                        mode,
                        self.keymap.label(Command::ToggleTheme),
                    ))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.toggle_theme(window, cx);
                    })),
            );
        if let Some(project_id) = workspace {
            right = right
                .child(
                    button("history", "History", theme, false)
                        .flex_none()
                        .tooltip(tooltip(
                            theme,
                            "Logs of the workspace's finished sessions",
                            "",
                        ))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.show_history(project_id, window, cx);
                        })),
                )
                .child(new_pane_button(
                    "new-agent-pane",
                    "+ Agent",
                    SessionKind::Agent,
                    self.keymap.label(Command::SplitAgent),
                    theme,
                    cx,
                ))
                .child(new_pane_button(
                    "new-shell-pane",
                    "+ Shell",
                    SessionKind::Shell,
                    self.keymap.label(Command::SplitShell),
                    theme,
                    cx,
                ))
                .child(self.render_run_button(project_id, theme, cx));
        }
        bar = bar.child(right.when(modal, |el| el.relative().child(inert_layer())));
        if controls {
            // No click handlers: Windows hit-tests these areas (`HTMINBUTTON` / `HTMAXBUTTON` /
            // `HTCLOSE`) and runs the action itself; `HTMAXBUTTON` also brings the Snap
            // Layouts flyout.
            let max_glyph = if window.is_maximized() { "❐" } else { "□" };
            bar = bar.child(
                div()
                    .flex_none()
                    .h_full()
                    .flex()
                    .child(window_control(
                        "win-min",
                        "─",
                        WindowControlArea::Min,
                        theme,
                    ))
                    .child(window_control(
                        "win-max",
                        max_glyph,
                        WindowControlArea::Max,
                        theme,
                    ))
                    .child(window_control(
                        "win-close",
                        "✕",
                        WindowControlArea::Close,
                        theme,
                    )),
            );
        }
        bar
    }

    /// The views as a segmented control; the Git one carries the `Git N` badge (N = changed
    /// files over all repositories of the workspace, hidden at 0).
    fn render_view_tabs(
        &self,
        shown: View,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let hover = theme.hover;
        let changed = self.git_summary().map_or(0, GitSummary::changed);
        div()
            .flex_none()
            .flex()
            .items_center()
            .p(px(theme::SEGMENT_PADDING))
            .rounded(px(radius::MEDIUM))
            .bg(rgb(theme.surface))
            .children(View::ALL.into_iter().map(|view| {
                let active = view == shown;
                div()
                    .id(SharedString::from(format!("view-{}", view.title())))
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .px(px(space::M))
                    .py(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .border(px(theme::BORDER_WIDTH))
                    .cursor_pointer()
                    .whitespace_nowrap()
                    .text_size(font_size::BODY)
                    .when(active, |el| {
                        el.bg(rgb(theme.surface_raised))
                            .border_color(rgb(theme.border))
                            .font_weight(FontWeight(weight::MEDIUM))
                            .text_color(rgb(theme.text))
                    })
                    .when(!active, |el| {
                        el.border_color(gpui::transparent_black())
                            .text_color(rgb(theme.text_muted))
                            .hover(move |style| style.bg(rgb(hover)))
                            .active(|style| style.opacity(opacity::ACTIVE))
                    })
                    .tooltip(tooltip(
                        theme,
                        view.title(),
                        self.keymap.label(view.command()),
                    ))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.show_view(view, window, cx);
                    }))
                    .child(view.title())
                    .when(view == View::Git && changed > 0, |el| {
                        el.child(
                            div()
                                .px(px(space::XS))
                                .rounded(px(radius::SMALL))
                                .bg(rgb(theme.accent_bg))
                                .text_size(font_size::META)
                                .text_color(rgb(theme.accent_text))
                                .child(changed.to_string()),
                        )
                    })
            }))
    }

    /// `⎇ master` / `⎇ 2 repos` (tooltip: one line per repository); a click shows the Git
    /// view. Nothing while the summary is not loaded or the folder holds no repository.
    fn render_branch_chip(
        &self,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        let summary = self.git_summary()?;
        let label = summary.branch_label()?;
        let tip = summary.branch_tooltip().unwrap_or_default();
        let hover = theme.hover;
        Some(
            div()
                .id("title-branch")
                .min_w_0()
                .max_w(theme::rems_px(theme::TITLE_CHIP_MAX_WIDTH))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .cursor_pointer()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text_muted))
                .hover(move |style| style.bg(rgb(hover)))
                .active(|style| style.opacity(opacity::ACTIVE))
                .tooltip(tooltip(theme, &tip, ""))
                .on_click(cx.listener(|shell, _, window, cx| {
                    shell.show_view(View::Git, window, cx);
                }))
                .child(label)
                .into_any_element(),
        )
    }

    /// The workspace name with ▾: opens a dropdown of every workspace (click switches) and
    /// "Add workspace…".
    fn render_workspace_switcher(
        &self,
        project_id: Ulid,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let projects = self.sidebar.projects();
        let current = projects.iter().find(|project| project.id == project_id);
        let name = current.map_or("Workspace", |project| project.name.as_str());
        let color = current
            .and_then(|project| project.color.as_deref())
            .and_then(|name| theme.workspace_color(name));
        let hover = theme.hover;
        let button = div()
            .id("workspace-switch")
            .max_w(theme::rems_px(theme::TITLE_CHIP_MAX_WIDTH))
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(radius::SMALL))
            .cursor_pointer()
            .text_size(font_size::BODY)
            .font_weight(FontWeight(weight::MEDIUM))
            .text_color(rgb(theme.text))
            .hover(move |style| style.bg(rgb(hover)))
            .active(|style| style.opacity(opacity::ACTIVE))
            .tooltip(tooltip(
                theme,
                "Switch workspace",
                self.keymap.label(Command::NextWorkspace),
            ))
            // An open menu already closed on this mouse down (outside it): the click reopens it.
            .on_click(cx.listener(|shell, _, _, cx| {
                shell.workspace_switcher = true;
                cx.notify();
            }))
            .child(workspace_avatar(name, color, theme))
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(name.to_owned()),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child("▾"),
            );
        let menu = (self.workspace_switcher && !self.modal_open()).then(|| {
            let rows = projects.iter().map(|project| {
                let id = project.id;
                let color = project
                    .color
                    .as_deref()
                    .and_then(|name| theme.workspace_color(name));
                menu_item(SharedString::from(format!("switch-{id}")), theme)
                    .child(workspace_avatar(&project.name, color, theme))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(rgb(theme.text))
                            .child(project.name.clone()),
                    )
                    .when(id == project_id, |el| {
                        el.child(div().text_color(rgb(theme.accent_text)).child("✓"))
                    })
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        cx.stop_propagation();
                        shell.activate_workspace(id, window, cx);
                    }))
            });
            let panel = menu_panel("workspace-switch-panel", theme)
                .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                    shell.workspace_switcher = false;
                    cx.notify();
                }))
                .children(rows)
                .child(
                    div()
                        .h(px(theme::BORDER_WIDTH))
                        .my(px(space::XS))
                        .bg(rgb(theme.border)),
                )
                .child(
                    menu_item("switch-add".into(), theme)
                        .child(
                            div()
                                .flex_1()
                                .text_color(rgb(theme.text))
                                .child("Add workspace…"),
                        )
                        .on_click(cx.listener(|shell, _, window, cx| {
                            cx.stop_propagation();
                            shell.workspace_switcher = false;
                            shell.add_workspace(window, cx);
                        })),
                );
            deferred(
                anchored()
                    .snap_to_window_with_margin(px(space::S))
                    .child(div().pt(px(space::XS)).child(panel)),
            )
        });
        div().child(button).children(menu).into_any_element()
    }

    /// The Run button (SPEC §10): a split accent button, `▶ Run` (or the running count and
    /// ports, with ↻ and ■ beside it while live) and `▾` for the per-command menu below it.
    fn render_run_button(
        &self,
        project_id: Ulid,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let summary = self.run_summary(project_id);
        let live = summary.any_live();
        let tip = if summary.rows.is_empty() {
            "Set up the commands to run".to_owned()
        } else {
            summary.tooltip()
        };
        let main = if live {
            button("run-main", summary.label(), theme, false)
        } else {
            run_fill_button("run-main", summary.label(), theme)
        }
        .flex_none()
        .when(!live, |el| el.rounded_r_none())
        .when(summary.warning(), |el| {
            el.text_color(rgb(theme.status_needs_input))
        })
        .tooltip(tooltip(theme, &tip, ""))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.run_main_click(project_id, window, cx);
        }));
        let open = self.run_menu == Some(project_id) && !self.modal_open();
        let mut bar = div()
            .flex_none()
            .flex()
            .items_center()
            .when(live, |el| el.gap(px(space::XS)))
            .child(main);
        if live {
            for (id, label, tip, action) in [
                (
                    "run-restart",
                    "↻",
                    "Restart all run commands",
                    RunAction::Restart,
                ),
                ("run-stop", "■", "Stop all run commands", RunAction::Stop),
            ] {
                bar = bar.child(
                    run_icon_button(id, label, action, theme)
                        .flex_none()
                        .tooltip(tooltip(theme, tip, ""))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.run_action(project_id, action, None, window, cx);
                        })),
                );
            }
        }
        // The ▾ half of the split button: same fill, a faint divider on its left.
        let menu = if live {
            button("run-menu", "▾", theme, false)
        } else {
            run_fill_button("run-menu", "▾", theme)
        }
        .when(!live, |el| {
            el.rounded_l_none()
                .border_l(px(theme::BORDER_WIDTH))
                .border_color(gpui::rgba(theme.on_accent_divider))
        });
        bar.child(
            div()
                .child(
                    menu.tooltip(tooltip(theme, "Run commands", ""))
                        // An open menu already closed on this mouse down (outside it): the
                        // click reopens it.
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            shell.run_menu = Some(project_id);
                            cx.notify();
                        })),
                )
                .when(open, |el| {
                    el.child(self.render_run_menu(project_id, &summary, theme, cx))
                }),
        )
        .into_any_element()
    }

    /// One row per command (state, ports, ↻ ■ or ▶), then "Show Run tab" and "Run setup…".
    fn render_run_menu(
        &self,
        project_id: Ulid,
        summary: &Summary,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let separator = || {
            div()
                .h(px(theme::BORDER_WIDTH))
                .my(px(space::XS))
                .bg(rgb(theme.border))
        };
        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        for row in &summary.rows {
            let running = matches!(row.state, RowState::Running(_));
            let color = match row.state {
                RowState::Running(_) => theme.status_done,
                RowState::Exited(_) => theme.status_exited,
                RowState::Stopped => theme.text_muted,
            };
            let ports: Vec<String> = row.ports.iter().map(|p| format!(":{p}")).collect();
            let controls: &[(&'static str, &'static str, RunAction)] = if running {
                &[
                    ("↻", "Restart this command", RunAction::Restart),
                    ("■", "Stop this command", RunAction::Stop),
                ]
            } else {
                &[("▶", "Run this command", RunAction::Start)]
            };
            let mut line = div()
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::S))
                .py(px(space::XS))
                .text_size(font_size::BODY)
                .child(
                    div()
                        .flex_1()
                        .text_color(rgb(theme.text))
                        .child(row.name.clone()),
                )
                .child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(color))
                        .child(run::state_text(&row.state)),
                )
                .child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(ports.join(" ")),
                );
            for &(label, tip, action) in controls {
                let name = row.name.clone();
                let colors = run::action_colors(action, theme);
                line = line.child(
                    div()
                        .id(SharedString::from(format!("run-{label}-{}", row.name)))
                        .px(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .cursor_pointer()
                        .text_color(rgb(colors.fg))
                        .hover(move |style| style.bg(rgb(colors.tint)))
                        .tooltip(tooltip(theme, tip, ""))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            // Not a click on what lies under the menu (a card, a pane).
                            cx.stop_propagation();
                            shell.run_action(project_id, action, Some(name.clone()), window, cx);
                        }))
                        .child(label),
                );
            }
            rows.push(line.into_any_element());
        }
        let panel = menu_panel("run-menu-panel", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.run_menu = None;
                cx.notify();
            }))
            .when(!rows.is_empty(), |el| el.children(rows).child(separator()))
            .child(
                menu_item("run-show-tab".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child("Show Run tab"),
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        cx.stop_propagation();
                        shell.show_run_tab(project_id, window, cx);
                    })),
            )
            .child(
                menu_item("run-setup".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child("Run setup…"),
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        cx.stop_propagation();
                        shell.open_run_setup(project_id, false, window, cx);
                    })),
            );
        deferred(
            anchored()
                .snap_to_window_with_margin(px(space::S))
                .child(div().pt(px(space::XS)).child(panel)),
        )
        .into_any_element()
    }

    /// "Run setup": one card per command, "+ Add command", "Start with the daemon", errors,
    /// Cancel and Save (Ctrl+Enter).
    fn render_run_setup(
        &self,
        setup: &RunSetup,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        use run_setup::{Field, Focus};
        let label = |text: &'static str| {
            div()
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let field = |cx: &mut Context<'_, Self>,
                     i: usize,
                     which: Field,
                     id: &'static str,
                     text: &str,
                     placeholder: &'static str| {
            text_field(
                id,
                text,
                placeholder,
                setup.focus == Focus::Field(i, which),
                setup.cards[i].all_selected(which),
                theme,
            )
            .on_click(cx.listener(move |shell, _, _, cx| {
                if let Some(setup) = shell.run_setup.as_mut() {
                    setup.focus = Focus::Field(i, which);
                }
                cx.notify();
            }))
        };
        let mut cards = div()
            .id("run-cards")
            .flex()
            .flex_col()
            .gap(px(space::S))
            .max_h(px(theme::RUN_SETUP_MAX_HEIGHT))
            .overflow_y_scroll();
        for (i, card) in setup.cards.iter().enumerate() {
            let name = field(cx, i, Field::Name, "run-name", &card.name, "e.g. BE");
            let dir = field(cx, i, Field::Dir, "run-dir", &card.dir, ". or EA-BE");
            let command = field(
                cx,
                i,
                Field::Command,
                "run-command",
                &card.command,
                "e.g. npm run dev",
            );
            let env_file = field(
                cx,
                i,
                Field::EnvFile,
                "run-env-file",
                &card.env_file,
                "e.g. .env",
            );
            let env = field(
                cx,
                i,
                Field::Env,
                "run-env",
                card.env.text(),
                "e.g. ENVIRONMENT=dev",
            );
            cards = cards.child(
                // The id scopes the card's field and button ids (the same in every card).
                div()
                    .id(("run-card", i))
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .p(px(space::S))
                    .rounded(px(radius::MEDIUM))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(theme.border))
                    .child(
                        div()
                            .flex()
                            .gap(px(space::S))
                            .child(div().flex_1().child(label("Name")).child(name))
                            .child(div().flex_1().child(label("Folder")).child(dir)),
                    )
                    .child(label("Command"))
                    .child(command)
                    .child(label("Env file (optional, relative to the folder)"))
                    .child(env_file)
                    .child(label("Env (KEY=VALUE per line, wins over the env file)"))
                    .child(env)
                    .child(div().flex().justify_end().child(
                        quiet_button("run-remove", "Remove", theme, theme.surface_raised).on_click(
                            cx.listener(move |shell, _, _, cx| {
                                if let Some(setup) = shell.run_setup.as_mut() {
                                    setup.remove_card(i);
                                }
                                cx.notify();
                            }),
                        ),
                    )),
            );
        }
        let error_row = |error: String| {
            div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                .child(div().text_color(rgb(theme.text)).child(error))
        };
        let hover = theme.surface_raised;
        let autostart = div()
            .id("run-autostart")
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::XS))
            .py(px(space::XS))
            .rounded(px(radius::SMALL))
            .border(px(theme::BORDER_WIDTH))
            .border_color(gpui::transparent_black())
            .when(setup.focus == Focus::Autostart, |el| {
                el.border_color(rgb(theme.pane_focus_border))
            })
            .cursor_pointer()
            .text_size(font_size::BODY)
            .text_color(rgb(theme.text))
            .hover(move |style| style.bg(rgb(hover)))
            .active(|style| style.opacity(opacity::ACTIVE))
            .tooltip(tooltip(
                theme,
                "Start the run commands when the daemon starts",
                "Space",
            ))
            .on_click(cx.listener(|shell, _, _, cx| {
                if let Some(setup) = shell.run_setup.as_mut() {
                    setup.autostart = !setup.autostart;
                }
                cx.notify();
            }))
            .child(
                div()
                    .size(px(space::M))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(radius::SMALL))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(theme.border))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.accent))
                    .child(if setup.autostart { "✓" } else { "" }),
            )
            .child(div().flex_1().child("Start with the daemon"))
            .child(
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child("Space"),
            );
        let panel = div()
            .id("run-setup")
            .w(px(theme::DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::run_setup_key_down))
            .relative()
            .child(self.typing(&self.dialog_focus, Self::run_setup_key_down, cx))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Run setup"),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child(
                        "Each command runs in its own pane of the “run” tab, in a shell started \
                         in its folder.",
                    ),
            )
            .child(cards)
            .child(
                div()
                    .flex()
                    .child(
                        button("run-add", "+ Add command", theme, false).on_click(cx.listener(
                            |shell, _, _, cx| {
                                if let Some(setup) = shell.run_setup.as_mut() {
                                    setup.add_card();
                                }
                                cx.notify();
                            },
                        )),
                    ),
            )
            .child(autostart)
            .children(setup.error.clone().map(error_row))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(space::S))
                    .pt(px(space::S))
                    .child(
                        button("run-cancel", "Cancel", theme, false)
                            .tooltip(tooltip(theme, "Keep the current commands", "Esc"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.close_run_setup(window, cx);
                            })),
                    )
                    .child(if setup.busy {
                        disabled_button("run-save", "Saving…", theme, true)
                    } else {
                        button("run-save", "Save", theme, true)
                            .tooltip(tooltip(theme, "Save the run commands", "Ctrl+Enter"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.save_run_setup(window, cx);
                            }))
                    }),
            );
        Self::modal_shade("run-setup-shade", panel, cx)
    }

    /// The workspace's task board: one column per status (SPEC §10); cards drag between and
    /// within columns.
    fn render_board(
        &self,
        project_id: Ulid,
        now: i64,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let project = self.sidebar.projects().iter().find(|p| p.id == project_id);
        let empty = |sentence: &str, detail: Option<String>| {
            message(
                theme,
                None,
                sentence.to_owned().into(),
                detail.map(Into::into),
            )
            .flex_1()
            .into_any_element()
        };
        if !self.board.is_loaded() {
            return match &self.board_error {
                Some(error) => empty("Cannot load the tasks.", Some(error.clone())),
                None => empty("Loading tasks…", None),
            };
        }
        if !self.board.has_tasks(project_id) {
            return message(theme, None, "No tasks yet.".into(), None)
                .flex_1()
                .child(self.new_task_button(project_id, true, theme, cx))
                .into_any_element();
        }
        let default_agent = project.and_then(|p| p.default_agent.as_deref());
        let columns = self
            .board
            .columns(project_id, default_agent, self.sidebar.sessions());
        let cards = CardContext {
            selected: self.selected_card,
            starting: &self.starting,
            follow_up: self.follow_up.as_ref(),
            follow_up_focus: &self.follow_up_focus,
            follow_up_scroll: &self.follow_up_scroll,
            ime_marked: &self.ime_marked,
            follow_up_focused: self.follow_up_focus.is_focused(window),
            run_keys: self.keymap.label(Command::RunTask),
            conflicts: &self.conflicts,
            run_with: self.run_with.as_ref(),
            fan_out: self.fan_out.as_ref(),
            usages: &self.usages,
            sessions: self.sidebar.sessions(),
            agent_icons: &self.agent_icons,
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .font_family(theme.ui_font_family.clone())
            // A later `task.list` failed: the cards may be stale.
            .children(self.board_error.clone().map(|error| {
                notice(theme.text, format!("✕ Cannot reload the tasks: {error}"))
                    .px(px(space::L))
                    .pt(px(space::S))
            }))
            .children(self.card_error.as_ref().and_then(|(project, error)| {
                (*project == project_id).then(|| {
                    notice(theme.text, format!("✕ {error}"))
                        .px(px(space::L))
                        .pt(px(space::S))
                })
            }))
            .child(
                div()
                    .id("board")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap(px(space::M))
                    .px(px(space::L))
                    .pb(px(space::L))
                    .pt(px(space::S))
                    .overflow_x_scroll()
                    .children(columns.into_iter().map(|column| {
                        let new_task = (column.status == TaskStatus::Todo).then(|| {
                            self.new_task_button(project_id, false, theme, cx)
                                .into_any_element()
                        });
                        board_column(column, &cards, now, new_task, theme, cx)
                    })),
            )
            .into_any_element()
    }

    /// The workspace's Markdown docs (SPEC §12): the file list with open-comment counts, and
    /// the selected file rendered or as source with line numbers.
    fn render_docs(
        &self,
        project_id: Ulid,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let empty = |sentence: &str, detail: Option<String>| {
            message(
                theme,
                None,
                sentence.to_owned().into(),
                detail.map(Into::into),
            )
            .flex_1()
        };
        let Some(docs) = self.docs.get(&project_id) else {
            return empty("Loading docs…", None).into_any_element();
        };
        match &docs.files {
            Files::Loading => return empty("Loading docs…", None).into_any_element(),
            Files::Failed(error) => {
                return empty("Cannot list the docs.", Some(error.clone()))
                    .child(
                        button("docs-retry", "Retry", theme, true).on_click(cx.listener(
                            move |shell, _, window, cx| shell.load_docs(project_id, window, cx),
                        )),
                    )
                    .into_any_element();
            }
            Files::Loaded(files) if files.is_empty() => {
                let keys = self.keymap.label(Command::ShowTerminals);
                return empty(
                    "No Markdown files to review yet.",
                    Some("Plans and docs written by agents show up here.".into()),
                )
                .child(
                    button("docs-terminals", "Show terminals", theme, true)
                        .tooltip(tooltip(theme, "Show terminals", keys.clone()))
                        .child(
                            div()
                                .text_size(font_size::META)
                                .opacity(opacity::SECONDARY)
                                .child(keys),
                        )
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.show_view(View::Terminals, window, cx);
                        })),
                )
                .into_any_element();
            }
            Files::Loaded(_) => {}
        }
        let selected = docs.selected.as_deref();
        let shown_files = docs.shown_files();
        let filter_focused = self.docs_filter_focus.is_focused(window);
        let list = div()
            .flex_none()
            .w(px(theme::DOC_LIST_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .child(
                div().px(px(space::S)).pb(px(space::XS)).child(
                    text_field(
                        "docs-filter",
                        &docs.filter,
                        "Filter files…",
                        filter_focused,
                        docs.filter.all_selected(),
                        theme,
                    )
                    .track_focus(&self.docs_filter_focus)
                    .on_key_down(cx.listener(Self::docs_filter_key_down))
                    .relative()
                    .child(self.typing(&self.docs_filter_focus, Self::docs_filter_key_down, cx))
                    .tooltip(tooltip(
                        theme,
                        "Filter the files",
                        self.keymap.label(Command::Find),
                    ))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        window.focus(&shell.docs_filter_focus, cx);
                        cx.notify();
                    })),
                ),
            )
            .child(
                div()
                    .id("doc-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .px(px(space::S))
                    .pb(px(space::S))
                    .when(shown_files.is_empty(), |el| {
                        el.child(
                            div()
                                .px(px(space::S))
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child("No files match the filter."),
                        )
                    })
                    .children(shown_files.into_iter().map(|(file, matched)| {
                        let shown = selected == Some(file.file_path.as_str());
                        doc_row(project_id, file, &matched, shown, theme, cx)
                    })),
            );
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .font_family(theme.ui_font_family.clone())
            .child(list)
            .child(self.render_doc(project_id, docs, theme, window, cx))
            .into_any_element()
    }

    /// The selected doc: its path, Preview / Source, Comment and Open in Zed, then its content
    /// (blocks / lines are picked by clicking) with the comment threads in the right margin.
    fn render_doc(
        &self,
        project_id: Ulid,
        docs: &Docs,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let panel = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .mr(px(space::S))
            .mb(px(space::S))
            .rounded(px(radius::MEDIUM))
            .overflow_hidden()
            .bg(rgb(theme.surface));
        let Some(file_path) = docs.selected.clone() else {
            return panel
                .child(message(theme, None, "Select a file to read it.".into(), None).flex_1());
        };
        let zed_keys = self.keymap.label(Command::OpenInZed);
        let header = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::L))
            .py(px(space::S))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(file_path.clone()),
            )
            .child(self.mode_toggle(
                "doc-mode",
                docs.mode,
                [(Mode::Preview, "Preview"), (Mode::Source, "Source")],
                "Toggle preview / source",
                self.keymap.label(Command::ToggleDocSource).into(),
                move |shell, _, _, cx| {
                    if let Some(docs) = shell.docs.get_mut(&project_id) {
                        docs.toggle_mode();
                        cx.notify();
                    }
                },
                theme,
                cx,
            ))
            .child(self.add_comment_button(
                project_id,
                CommentContext::Doc,
                matches!(docs.doc, Doc::Shown(_)) && docs.comments.selection.is_some(),
                theme,
                cx,
            ))
            .child(self.send_comments_button(
                project_id,
                CommentContext::Doc,
                matches!(docs.doc, Doc::Shown(_)).then_some(&docs.comments),
                theme,
                cx,
            ))
            .child(
                button("doc-zed", "Open in Zed", theme, false)
                    .flex_none()
                    .tooltip(tooltip(theme, "Open this file in Zed", zed_keys))
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        shell.open_doc_in_zed(project_id, cx);
                    })),
            );
        let body = match &docs.doc {
            Doc::None | Doc::Loading => message(theme, None, "Loading…".into(), None)
                .flex_1()
                .into_any_element(),
            Doc::Failed(error) => message(
                theme,
                None,
                "Cannot read this file.".into(),
                Some(error.clone().into()),
            )
            .flex_1()
            .child(
                button("doc-retry", "Retry", theme, true).on_click(cx.listener(
                    move |shell, _, window, cx| {
                        shell.select_doc(project_id, &file_path, window, cx);
                    },
                )),
            )
            .into_any_element(),
            Doc::Deleted => message(
                theme,
                None,
                "This file was deleted.".into(),
                Some("It shows again here if it is written back.".into()),
            )
            .flex_1()
            .into_any_element(),
            Doc::Shown(shown) => match docs.mode {
                mode if shown.lines.is_empty()
                    || (mode == Mode::Preview && shown.blocks.is_empty()) =>
                {
                    message(theme, None, "This file is empty.".into(), None)
                        .flex_1()
                        .into_any_element()
                }
                Mode::Source => {
                    let this = cx.entity().downgrade();
                    let on_line: preview::OnLine = Rc::new(move |line, extend, _, cx| {
                        let _ = this.update(cx, |shell, cx| {
                            shell.pick_lines(
                                project_id,
                                CommentContext::Doc,
                                line,
                                line,
                                extend,
                                cx,
                            );
                        });
                    });
                    preview::source(
                        "doc-source",
                        Rc::clone(shown),
                        docs.comments.selection,
                        on_line,
                        theme,
                    )
                    .flex_1()
                    .min_h_0()
                    .px(px(space::L))
                    .pb(px(space::L))
                    .into_any_element()
                }
                Mode::Preview => preview_page(
                    "doc-preview",
                    preview_blocks(project_id, shown, docs.comments.selection, theme, cx),
                ),
            },
        };
        let margin = margin_shown(&docs.doc)
            .then(|| self.render_comments(project_id, &docs.comments, theme, window, cx));
        panel
            .child(header)
            .children(docs.zed_error.clone().map(|error| {
                notice(theme.text, format!("✕ {error}"))
                    .px(px(space::L))
                    .pb(px(space::S))
            }))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            .child(body),
                    )
                    .children(margin),
            )
    }

    /// The workspace's files (SPEC §12A): the lazy tree with git markers, and the opened file
    /// read-only with line numbers and syntax colors.
    fn render_files(
        &self,
        project_id: Ulid,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let empty = |sentence: &str, detail: Option<String>| {
            message(
                theme,
                None,
                sentence.to_owned().into(),
                detail.map(Into::into),
            )
            .flex_1()
        };
        let Some(view) = self.files.get(&project_id) else {
            return empty("Loading files…", None).into_any_element();
        };
        let ignored_keys = self.keymap.label(Command::ToggleIgnored);
        let (ignored_label, ignored_tip) = if view.show_ignored {
            ("Hide ignored", "Hide git-ignored files")
        } else {
            ("Show ignored", "Show git-ignored files")
        };
        let header = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::S))
            .pb(px(space::XS))
            .child(
                div()
                    .id("files-search")
                    .flex_1()
                    .min_w_0()
                    .cursor_pointer()
                    .child(text_field(
                        "files-search-field",
                        "",
                        "Search files…",
                        false,
                        false,
                        theme,
                    ))
                    .tooltip(tooltip(
                        theme,
                        "Find a file of this workspace",
                        self.keymap.label(Command::QuickOpen),
                    ))
                    .on_click(
                        cx.listener(|shell, _, window, cx| shell.open_quick_open(window, cx)),
                    ),
            )
            .child(
                button("files-ignored", ignored_label, theme, false)
                    .flex_none()
                    .tooltip(tooltip(theme, ignored_tip, ignored_keys.clone()))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        let requests = shell.files_view(project_id).toggle_ignored();
                        shell.files_requests(project_id, requests, window, cx);
                    })),
            );
        let tree = match view.root() {
            files_view::Listing::Loading => empty("Loading files…", None).into_any_element(),
            files_view::Listing::Failed(error) => {
                empty("Cannot list the files.", Some(error.clone()))
                    .child(
                        button("files-retry", "Retry", theme, true).on_click(cx.listener(
                            move |shell, _, window, cx| shell.refresh_files(project_id, window, cx),
                        )),
                    )
                    .into_any_element()
            }
            files_view::Listing::Loaded(entries) if entries.is_empty() && view.show_ignored => {
                empty("This workspace has no files yet.", None).into_any_element()
            }
            files_view::Listing::Loaded(entries) if entries.is_empty() => {
                empty("No files here yet (ignored files are hidden).", None)
                    .child(
                        button("files-empty-ignored", "Show ignored", theme, true)
                            .tooltip(tooltip(
                                theme,
                                "Show git-ignored files",
                                ignored_keys.clone(),
                            ))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                let requests = shell.files_view(project_id).toggle_ignored();
                                shell.files_requests(project_id, requests, window, cx);
                            })),
                    )
                    .into_any_element()
            }
            files_view::Listing::Loaded(_) => {
                let rows = view.rows();
                let cursor = view.cursor.clone();
                let opened = view.file.clone();
                let focused = self.files_focus.is_focused(window);
                let row_theme = theme.clone();
                uniform_list(
                    "file-tree",
                    rows.len(),
                    cx.processor(move |_, range: std::ops::Range<usize>, _, cx| {
                        range
                            .filter_map(|index| rows.get(index))
                            .map(|row| {
                                let shown = FileRowState {
                                    cursor: cursor.as_deref(),
                                    opened: opened.as_deref(),
                                    focused,
                                };
                                file_row(project_id, row, &shown, &row_theme, cx)
                            })
                            .collect()
                    }),
                )
                .track_scroll(&self.files_scroll)
                .flex_1()
                .min_h_0()
                .px(px(space::S))
                .into_any_element()
            }
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .font_family(theme.ui_font_family.clone())
            .child(
                div()
                    .id("files-tree")
                    .flex_none()
                    .w(px(theme::FILE_TREE_WIDTH))
                    .h_full()
                    .flex()
                    .flex_col()
                    .pb(px(space::S))
                    .track_focus(&self.files_focus)
                    .on_key_down(cx.listener(Self::files_key_down))
                    // Right click on the header or the blank part of the tree = the folder
                    // itself (rows stop the event first).
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |shell, event: &gpui::MouseDownEvent, _, cx| {
                            shell.open_file_menu(
                                project_id,
                                String::new(),
                                true,
                                event.position,
                                cx,
                            );
                        }),
                    )
                    .child(header)
                    .children(
                        self.target_chooser(project_id, view.worktree_task_id, false, theme, cx)
                            .map(|chooser| chooser.px(px(space::S)).pb(px(space::S))),
                    )
                    .child(tree),
            )
            .child(self.render_file(project_id, view, window, theme, cx))
            .into_any_element()
    }

    /// The opened file: its path (with `●` while unsaved), Save, Review in Docs (Markdown) and
    /// Open in Zed, then the editor, or why it is not shown.
    fn render_file(
        &self,
        project_id: Ulid,
        view: &FilesView,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let panel = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .mr(px(space::S))
            .mb(px(space::S))
            .rounded(px(radius::MEDIUM))
            .overflow_hidden()
            .bg(rgb(theme.surface));
        let Some(path) = view.file.clone() else {
            return panel.child(
                message(
                    theme,
                    None,
                    "Select a file to view it.".into(),
                    Some(format!(
                        "Use ↑ ↓ to move, → to open a folder, Enter to open a file · {} to find a file.",
                        self.keymap.label(Command::QuickOpen)
                    ).into()),
                )
                .flex_1(),
            );
        };
        let dirty = view.unsaved().is_some();
        let editable = view
            .shown()
            .is_some_and(|shown| !shown.code.binary && !shown.code.too_large);
        let zed_keys = self.keymap.label(Command::OpenInZed);
        let zed_tip = match view.line() {
            Some(line) => format!("Open this file in Zed at line {line}"),
            None => "Open this file in Zed".to_owned(),
        };
        let mode_toggle = (view.is_markdown() && editable).then(|| {
            self.mode_toggle(
                "file-mode",
                view.mode,
                [(Mode::Preview, "Preview"), (Mode::Source, "Source")],
                "Toggle preview / source",
                "".into(),
                move |shell, _, window, cx| {
                    shell.files_view(project_id).toggle_mode();
                    shell.build_preview(project_id, window, cx);
                    // The find fields are not drawn in Preview: give the keyboard back.
                    if shell.files_view(project_id).previewing()
                        && (shell.file_find_focus.is_focused(window)
                            || shell.file_replace_focus.is_focused(window))
                    {
                        window.focus(&shell.focus_handle, cx);
                    }
                    cx.notify();
                },
                theme,
                cx,
            )
        });
        let zed_button = |id: &'static str, primary: bool| {
            button(id, "Open in Zed", theme, primary)
                .flex_none()
                .tooltip(tooltip(theme, &zed_tip, zed_keys.clone()))
                .on_click(cx.listener(move |shell, _, _, cx| {
                    shell.open_file_in_zed(project_id, cx);
                }))
        };
        let markdown = view.is_markdown();
        let review_path = path.clone();
        let header = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::L))
            .py(px(space::S))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(format!("{}{path}", if dirty { "● " } else { "" })),
            )
            .children(view.shown().filter(|_| editable).and_then(|shown| {
                let at = shown.editor.cursor();
                // The caret is always inside the text (the editor clamps it).
                let line = shown.editor.lines().get(at.line)?;
                let col = editor::display_col(line, at.col);
                Some(
                    div()
                        .flex_none()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(format!("Ln {}, Col {}", at.line + 1, col + 1)),
                )
            }))
            .children(mode_toggle)
            // The Docs view reviews the project folder's files, not a worktree's.
            .when(markdown && view.worktree_task_id.is_none(), |el| {
                el.child(
                    button("file-review", "Review in Docs", theme, false)
                        .flex_none()
                        .tooltip(tooltip(theme, "Review and comment on this file", ""))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.review_in_docs(project_id, review_path.clone(), window, cx);
                        })),
                )
            })
            .when(editable, |el| {
                el.child(
                    button("file-save", "Save", theme, dirty)
                        .flex_none()
                        .tooltip(tooltip(
                            theme,
                            "Save this file",
                            self.keymap.label(Command::SaveFile),
                        ))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.save_file(project_id, None, window, cx);
                        })),
                )
            })
            .child(
                button("file-find", "Find", theme, false)
                    .flex_none()
                    .tooltip(tooltip(
                        theme,
                        "Find in this file",
                        self.keymap.label(Command::Find),
                    ))
                    .on_click(
                        cx.listener(|shell, _, window, cx| shell.open_file_find(false, window, cx)),
                    ),
            )
            .child(zed_button("file-zed", false));
        let find_bar = view
            .find
            .as_ref()
            .filter(|_| !view.previewing())
            .map(|find| {
                let focused = self.file_find_focus.is_focused(window);
                let find_row = div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .px(px(space::L))
                    .pb(px(space::S))
                    .child(
                        button(
                            "file-find-toggle-replace",
                            if find.replace.is_some() { "▾" } else { "▸" },
                            theme,
                            false,
                        )
                        .tooltip(tooltip(
                            theme,
                            "Toggle replace",
                            self.keymap.label(Command::ReplaceInFile),
                        ))
                        .on_click(cx.listener(
                            move |shell, _, window, cx| {
                                let shown = (shell.files_view(project_id).find.as_ref())
                                    .is_some_and(|find| find.replace.is_some());
                                if !shown {
                                    return shell.open_file_find(true, window, cx);
                                }
                                if let Some(find) = &mut shell.files_view(project_id).find {
                                    find.replace = None;
                                }
                                window.focus(&shell.file_find_focus, cx);
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        text_field(
                            "file-find-field",
                            &find.query,
                            "Find in file",
                            focused,
                            find.query.all_selected(),
                            theme,
                        )
                        .w(px(theme::FIND_INPUT_WIDTH))
                        .track_focus(&self.file_find_focus)
                        .on_key_down(cx.listener(Self::file_find_key_down))
                        .relative()
                        .child(self.typing(&self.file_find_focus, Self::file_find_key_down, cx))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            window.focus(&shell.file_find_focus, cx);
                            cx.notify();
                        })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child(find.label()),
                    )
                    .child(
                        button("file-find-previous", "↑", theme, false)
                            .tooltip(tooltip(theme, "Previous match", "Shift+Enter"))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                window.focus(&shell.file_find_focus, cx);
                                shell.step_file_find(project_id, false, cx);
                            })),
                    )
                    .child(
                        button("file-find-next", "↓", theme, false)
                            .tooltip(tooltip(theme, "Next match", "Enter"))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                window.focus(&shell.file_find_focus, cx);
                                shell.step_file_find(project_id, true, cx);
                            })),
                    )
                    .child(
                        button("file-find-close", "✕", theme, false)
                            .tooltip(tooltip(theme, "Close", "Esc"))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                shell.close_file_find(project_id, window, cx);
                            })),
                    );
                let replacing = self.file_replace_focus.is_focused(window);
                let replace_row = find.replace.as_ref().map(|with| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .px(px(space::L))
                        .pb(px(space::S))
                        .child(
                            text_field(
                                "file-replace-field",
                                with,
                                "Replace",
                                replacing,
                                with.all_selected(),
                                theme,
                            )
                            .w(px(theme::FIND_INPUT_WIDTH))
                            .track_focus(&self.file_replace_focus)
                            .on_key_down(cx.listener(Self::file_replace_key_down))
                            .relative()
                            .child(self.typing(
                                &self.file_replace_focus,
                                Self::file_replace_key_down,
                                cx,
                            ))
                            .on_click(cx.listener(
                                |shell, _, window, cx| {
                                    window.focus(&shell.file_replace_focus, cx);
                                    cx.notify();
                                },
                            )),
                        )
                        .child(
                            button("file-replace-one", "Replace", theme, false)
                                .tooltip(tooltip(
                                    theme,
                                    "Replace this match and go to the next",
                                    "Enter",
                                ))
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    window.focus(&shell.file_replace_focus, cx);
                                    shell.replace_in_file(project_id, window, cx);
                                })),
                        )
                        .child(
                            button("file-replace-all", "Replace all", theme, false)
                                .tooltip(tooltip(theme, "Replace every match", "Ctrl+Alt+Enter"))
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    window.focus(&shell.file_replace_focus, cx);
                                    shell.replace_all_in_file(project_id, window, cx);
                                })),
                        )
                });
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .child(find_row)
                    .children(replace_row)
            });
        let body = match &view.content {
            Content::None | Content::Loading => message(theme, None, "Loading…".into(), None)
                .flex_1()
                .into_any_element(),
            Content::Failed(error) => message(
                theme,
                None,
                "Cannot read this file.".into(),
                Some(error.clone().into()),
            )
            .flex_1()
            .child(
                button("file-retry", "Retry", theme, true).on_click(cx.listener(
                    move |shell, _, window, cx| {
                        let read = shell.files_view(project_id).open(&path);
                        let requests = Requests { list: vec![], read };
                        shell.files_requests(project_id, requests, window, cx);
                    },
                )),
            )
            .into_any_element(),
            Content::Shown(shown) if shown.code.binary || shown.code.too_large => {
                let sentence = if shown.code.binary {
                    "This is a binary file."
                } else {
                    "This file is too large to show here."
                };
                message(
                    theme,
                    None,
                    sentence.into(),
                    Some(
                        format!("{} — open it in Zed instead.", size_label(shown.code.size)).into(),
                    ),
                )
                .flex_1()
                .child(zed_button("file-zed-notice", true))
                .into_any_element()
            }
            Content::Shown(_) if view.previewing() => self.render_md_preview(view, theme),
            Content::Shown(shown) => self.render_editor(project_id, shown, window, theme, cx),
        };
        let disk_banner = view
            .shown()
            .filter(|shown| shown.disk_changed.is_some())
            .map(|_| {
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .px(px(space::L))
                    .pb(px(space::S))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text))
                            .child("⚠ Changed on disk — Reload · Keep mine"),
                    )
                    .child(
                        button("file-reload", "Reload", theme, false)
                            .tooltip(tooltip(
                                theme,
                                "Show the file on disk; your changes are dropped",
                                "",
                            ))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                let read = shell.files_view(project_id).reload();
                                let requests = files_view::Requests { list: vec![], read };
                                shell.files_requests(project_id, requests, window, cx);
                            })),
                    )
                    .child(
                        button("file-keep-mine", "Keep mine", theme, true)
                            .tooltip(tooltip(
                                theme,
                                "Keep your text; Save overwrites the file on disk",
                                "",
                            ))
                            .on_click(cx.listener(move |shell, _, _, cx| {
                                shell.files_view(project_id).keep_mine();
                                cx.notify();
                            })),
                    )
            });
        panel
            .child(header)
            .children(find_bar)
            .children(disk_banner)
            .children(view.zed_error.clone().map(|error| {
                notice(theme.text, format!("✕ {error}"))
                    .px(px(space::L))
                    .pb(px(space::S))
            }))
            .children(
                view.shown()
                    .and_then(|shown| shown.save_error.clone())
                    .map(|error| {
                        notice(theme.text, format!("✕ {error}"))
                            .px(px(space::L))
                            .pb(px(space::S))
                    }),
            )
            .child(body)
    }

    /// The Files editor (§12A, ADR 0028): the lines with the caret, selection, syntax colors
    /// and find matches. A click places the caret; Shift+click and drag select.
    fn render_editor(
        &self,
        project_id: Ulid,
        shown: &files_view::Shown,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let focused = self.file_editor_focus.is_focused(window);
        let cell_width = mkrew_ui::terminal::view::cell_size(window, theme).0;
        let lines = shown.editor.lines();
        let total = lines.len();
        // Widest row in screen cells (tabs and multibyte chars differ from the byte length).
        let longest = (lines.iter().enumerate())
            .max_by_key(|(_, line)| editor::display_col(line, line.len()))
            .map(|(index, _)| index);
        let text_left = Rc::clone(&self.file_text_left);
        let row_theme = theme.clone();
        let list = uniform_list(
            "file-code",
            total,
            cx.processor(move |shell, range: std::ops::Range<usize>, _, cx| {
                let Some(view) = shell.files.get(&project_id) else {
                    return Vec::new();
                };
                let Some(shown) = view.shown() else {
                    return Vec::new();
                };
                let (editor, code) = (&shown.editor, &shown.code);
                let hits = view.find.as_ref().map(TextFind::hits);
                let (cursor, selection) = (editor.cursor(), editor.selection());
                range
                    .filter_map(|index| {
                        let line = editor.lines().get(index)?;
                        let text = editor::display_line(line);
                        // Spans of an older text could point past the line: only when it matches.
                        let fresh = code.lines.get(index).is_some_and(|old| *old == *text);
                        let spans = (code.spans.get(index))
                            .filter(|_| fresh)
                            .map(Vec::as_slice)
                            .unwrap_or(&[]);
                        let found = (hits.as_ref())
                            .map(|hits| hits.on_line(index))
                            .unwrap_or_default()
                            .into_iter()
                            .map(|(range, current)| {
                                let start = editor::display_byte(line, range.start);
                                (start..editor::display_byte(line, range.end), current)
                            })
                            .collect();
                        let marks = preview::RowMarks {
                            caret: (focused && cursor.line == index)
                                .then(|| editor::display_col(line, cursor.col)),
                            selected: (selection.as_ref()).and_then(|selection| {
                                editor::selected_cells(line, index, selection)
                            }),
                            found,
                        };
                        let row = preview::code_row(
                            index,
                            total,
                            text.into_owned(),
                            spans,
                            &marks,
                            cell_width,
                            &text_left,
                            &row_theme,
                        );
                        Some(
                            row.on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                                    let x = event.position.x;
                                    let extend = event.modifiers.shift;
                                    shell.click_file_text(project_id, index, x, extend, window, cx);
                                }),
                            )
                            .on_mouse_move(cx.listener(
                                move |shell, event: &MouseMoveEvent, window, cx| {
                                    let dragging = event.pressed_button == Some(MouseButton::Left)
                                        && shell.file_editor_focus.is_focused(window);
                                    if dragging {
                                        let x = event.position.x;
                                        shell.click_file_text(
                                            project_id, index, x, true, window, cx,
                                        );
                                    }
                                },
                            ))
                            .into_any_element(),
                        )
                    })
                    .collect()
            }),
        )
        .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
        .with_width_from_item(longest)
        .track_scroll(&self.file_code_scroll)
        .flex_1()
        .min_h_0()
        .px(px(space::L))
        .pb(px(space::L))
        .font_family(theme.terminal_font_family.clone())
        .text_size(px(theme.terminal_font_size));
        div()
            .id("file-editor")
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .track_focus(&self.file_editor_focus)
            .on_key_down(cx.listener(Self::file_editor_key_down))
            // Below the last line too: a click there gives the editor the keyboard.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|shell, _: &MouseDownEvent, window, cx| {
                    window.focus(&shell.file_editor_focus, cx);
                    cx.notify();
                }),
            )
            .child(self.typing(&self.file_editor_focus, Self::file_editor_key_down, cx))
            .child(list)
            .into_any_element()
    }

    /// The workspace's changes (SPEC §12B): the selected file's unified diff, and on its right
    /// the branch header and mode toggle over the changed-file list and the commit box.
    fn render_git(
        &self,
        project_id: Ulid,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let empty = |sentence: String, detail: Option<String>| {
            message(theme, None, sentence.into(), detail.map(Into::into)).flex_1()
        };
        let Some(view) = self.git.get(&project_id) else {
            return empty("Loading changes…".into(), None).into_any_element();
        };
        let status = match &view.status {
            git_view::Status::Loaded(status) => Some(&**status),
            git_view::Status::Loading | git_view::Status::Failed(_) => None,
        };
        let column = div()
            .id("git-files")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .pb(px(space::S))
            .track_focus(&self.git_focus)
            .key_context(GIT_CONTEXT)
            .on_key_down(cx.listener(Self::git_key_down))
            .child(self.git_header(project_id, view, status, theme, cx));
        let rows = view.list_rows();
        let any_files = !rows.is_empty();
        let commits = view.history.as_ref().map(|history| &history.commits);
        let list = match (commits, &view.status) {
            (Some(Commits::Loading), _) => {
                empty("Loading history…".into(), None).into_any_element()
            }
            (Some(Commits::Failed(error)), _) => {
                empty("Cannot read the history.".into(), Some(error.clone()))
                    .child(
                        button("git-history-retry", "Retry", theme, true).on_click(cx.listener(
                            move |shell, _, window, cx| shell.refresh_git(project_id, window, cx),
                        )),
                    )
                    .into_any_element()
            }
            (Some(Commits::Loaded(_)), _) if !any_files => empty(
                "No commits yet.".into(),
                Some("Commits on this branch show here.".into()),
            )
            .child(
                button("git-history-back", "Show changes", theme, true)
                    .tooltip(tooltip(
                        theme,
                        "Back to the changes",
                        self.keymap.label(Command::GitToggleHistory),
                    ))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        window.focus(&shell.git_focus, cx);
                        let requests = shell.git_view(project_id).toggle_history();
                        shell.git_requests(project_id, requests, window, cx);
                    })),
            )
            .into_any_element(),
            (Some(Commits::Loaded(_)), _) => {
                self.git_list(project_id, view, rows, theme, window, cx)
            }
            (None, git_view::Status::Loading) => {
                empty("Loading changes…".into(), None).into_any_element()
            }
            (None, git_view::Status::Failed(error)) if view.since.is_some() => {
                // Most likely the checkpoint is gone (gc, rewritten history): Retry won't help.
                let sentence = match view.since.as_ref().map(|since| since.origin) {
                    Some(git_view::Origin::Turn { n, .. }) => {
                        format!("Cannot read the changes since turn {n}.")
                    }
                    _ => "Cannot read the task's changes.".to_owned(),
                };
                empty(sentence, Some(error.clone()))
                    .child(
                        button("git-task-back", "Working changes", theme, true).on_click(
                            cx.listener(move |shell, _, window, cx| {
                                let working = git_view::Section::Working;
                                shell.show_git_section(project_id, working, window, cx);
                            }),
                        ),
                    )
                    .into_any_element()
            }
            (None, git_view::Status::Failed(error)) => {
                empty("Cannot read the git status.".into(), Some(error.clone()))
                    .child(
                        button("git-retry", "Retry", theme, true).on_click(cx.listener(
                            move |shell, _, window, cx| shell.refresh_git(project_id, window, cx),
                        )),
                    )
                    .into_any_element()
            }
            (None, git_view::Status::Loaded(_))
                if !any_files && view.changed_by.session.is_some() =>
            {
                // "Changed by" hides every listed file; All brings them back.
                let who = view
                    .changed_by_label(self.sidebar.sessions())
                    .unwrap_or_else(|| "this session".to_owned());
                empty(format!("No changes here by {who}."), None)
                    .child(
                        button("git-changed-by-show-all", "Show all", theme, true).on_click(
                            cx.listener(move |shell, _, window, cx| {
                                shell.filter_git_changed_by(project_id, None, window, cx);
                            }),
                        ),
                    )
                    .into_any_element()
            }
            (None, git_view::Status::Loaded(status)) if !any_files => {
                // No merge-base with a commit checked out: the base names nothing usable.
                let unusable = status.head.is_some() && status.merge_base.is_none();
                let no_base = view.since.is_none()
                    && view.mode == GitMode::Base
                    && (status.base_ref.is_none() || unusable);
                let since = view.since.as_ref().map(|since| match since.origin {
                    git_view::Origin::Turn { n, .. } => (
                        format!("No changes since turn {n} started."),
                        Some("Its changes may have been restored or undone.".to_owned()),
                    ),
                    git_view::Origin::Task(_) => (
                        "No changes since the task's run started.".to_owned(),
                        Some("Its changes may have been undone or committed elsewhere.".to_owned()),
                    ),
                });
                let (sentence, detail) = match (since, view.mode, &status.base_ref) {
                    (Some(since), _, _) => since,
                    (None, GitMode::Working, _) => ("No uncommitted changes.".to_owned(), None),
                    (None, GitMode::Base, None) => (
                        "No base branch found.".to_owned(),
                        Some("None of origin/HEAD, main or master exists here.".to_owned()),
                    ),
                    (None, GitMode::Base, Some(base)) if unusable => (
                        format!("Cannot compare with {base}."),
                        Some("It names no commit here, or shares no history with HEAD.".to_owned()),
                    ),
                    (None, GitMode::Base, Some(base)) => {
                        (format!("No changes against {base}."), None)
                    }
                };
                let (label, tip) = match view.mode {
                    GitMode::Working => ("Against base", "Show everything on this branch"),
                    GitMode::Base => ("Working changes", "Show uncommitted changes"),
                };
                // Without a base, picking one is the primary action.
                let buttons = div()
                    .flex()
                    .gap(px(space::S))
                    .when(no_base, |el| {
                        el.child(
                            button("git-empty-base", "Set base…", theme, true)
                                .tooltip(tooltip(theme, "Choose the branch to compare with", ""))
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    shell.open_git_base(project_id, window, cx);
                                })),
                        )
                    })
                    .child(
                        button("git-empty-mode", label, theme, !no_base)
                            .tooltip(tooltip(
                                theme,
                                tip,
                                self.keymap.label(Command::GitToggleMode),
                            ))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                window.focus(&shell.git_focus, cx);
                                let requests = shell.git_view(project_id).toggle_mode();
                                shell.git_requests(project_id, requests, window, cx);
                            })),
                    );
                empty(sentence, detail).child(buttons).into_any_element()
            }
            (None, git_view::Status::Loaded(_)) => {
                self.git_list(project_id, view, rows, theme, window, cx)
            }
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            // The list column stays at the right edge even when no diff is shown.
            .justify_end()
            .font_family(theme.ui_font_family.clone())
            .when(any_files, |el| {
                el.child(self.render_git_diff(project_id, view, theme, window, cx))
            })
            .child(
                // The commit box sits outside the `GIT_CONTEXT` column: typing never stages.
                div()
                    .flex_none()
                    .w(px(theme::GIT_FILE_LIST_WIDTH))
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(column.child(list))
                    .when_some(status.filter(|_| view.commit_box_shown()), |el, status| {
                        el.child(self.git_commit_box(project_id, view, status, theme, window, cx))
                    }),
            )
            .into_any_element()
    }

    /// The changed-file list (in the History: the commits, the expanded one with its files).
    fn git_list(
        &self,
        project_id: Ulid,
        view: &GitView,
        rows: Vec<git_view::ListRow>,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let rows = Rc::new(rows);
        let selected = view.selected.clone();
        let focused = self.git_focus.is_focused(window);
        let row_theme = theme.clone();
        let now = status::now_ms();
        uniform_list(
            "git-file-list",
            rows.len(),
            cx.processor(move |_, range: std::ops::Range<usize>, _, cx| {
                range
                    .filter_map(|index| rows.get(index))
                    .map(|row| {
                        git_file_row(
                            project_id,
                            row,
                            selected.as_ref(),
                            focused,
                            now,
                            &row_theme,
                            cx,
                        )
                    })
                    .collect()
            }),
        )
        .track_scroll(&self.git_files_scroll)
        .flex_1()
        .min_h_0()
        .px(px(space::S))
        .into_any_element()
    }

    /// Branch, upstream and ahead/behind, then the Working changes / Against base / History
    /// switch and the base it compares to.
    fn git_header(
        &self,
        project_id: Ulid,
        view: &GitView,
        status: Option<&GitStatus>,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let branch = match status {
            Some(status) => status
                .branch
                .clone()
                .unwrap_or_else(|| "Detached HEAD".to_owned()),
            None => "Git".to_owned(),
        };
        let tracking = status.map(|status| {
            let upstream = match &status.upstream {
                Some(upstream) => format!("→ {upstream}"),
                None => "No upstream".to_owned(),
            };
            match git_view::ahead_behind(status) {
                Some(counts) => format!("{upstream}  {counts}"),
                None => upstream,
            }
        });
        let base = status.map(|status| match (&status.base_ref, status.base_auto) {
            (Some(base), true) => format!("Base: {base} (auto-detected)"),
            (Some(base), false) => format!("Base: {base}"),
            (None, _) => "No base branch".to_owned(),
        });
        // A task's or turn's changes: what is compared replaces the base line.
        let since = view
            .since
            .as_ref()
            .filter(|_| view.section() == git_view::Section::Since);
        let base = match since {
            Some(since) => Some(since.label.clone()),
            None => base,
        };
        let shows_since = since.is_some();
        // The upstream line ends with Fetch and Push.
        let tracking = tracking.map(|tracking| {
            div()
                .flex()
                .items_center()
                .gap(px(space::XS))
                .child(
                    meta_line(theme, tracking.into())
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap(),
                )
                .children([Remote::Fetch, Remote::Push].map(|remote| {
                    self.git_remote_action(project_id, view, status, remote, theme, cx)
                }))
        });
        let chooser = self.target_chooser(project_id, view.worktree_task_id, true, theme, cx);
        let repos = self.repo_chooser(project_id, view, theme, cx);
        let toggle_keys = SharedString::from(self.keymap.label(Command::GitToggleMode));
        let history_keys = SharedString::from(self.keymap.label(Command::GitToggleHistory));
        // Lighter than the active segment's fill, so hovering never looks selected.
        let hover = theme.surface;
        let shown = view.section();
        let segment = |section: git_view::Section,
                       id: &'static str,
                       label: &'static str,
                       tip: &'static str| {
            let active = shown == section;
            let keys = if section == git_view::Section::History {
                history_keys.clone()
            } else {
                toggle_keys.clone()
            };
            // Sized from the label: three equal segments would cut "Against base".
            div()
                .id(id)
                .flex_auto()
                .whitespace_nowrap()
                .flex()
                .justify_center()
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .text_size(font_size::BODY)
                .when(active, |el| {
                    el.bg(rgb(theme.surface_raised))
                        .font_weight(FontWeight(weight::MEDIUM))
                        .text_color(rgb(theme.text))
                })
                .when(!active, |el| {
                    el.cursor_pointer()
                        .text_color(rgb(theme.text_muted))
                        .hover(move |style| style.bg(rgb(hover)))
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.show_git_section(project_id, section, window, cx);
                        }))
                })
                .tooltip(tooltip(theme, tip, keys))
                .child(label)
        };
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .px(px(space::M))
            .pt(px(space::XS))
            .children(chooser)
            .children(repos)
            .child(
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(branch),
            )
            .children(tracking)
            // Wrapped, not cut: git's push / fetch errors say what to do.
            .children(view.remotes.error.clone().map(|error| {
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text))
                    .child(format!("✕ {error}"))
            }))
            .child(
                div()
                    .mt(px(space::XS))
                    .flex()
                    .gap(px(space::XS))
                    .child(segment(
                        git_view::Section::Working,
                        "git-mode-working",
                        "Working",
                        "Uncommitted changes: staged, unstaged, untracked",
                    ))
                    .child(segment(
                        git_view::Section::Base,
                        "git-mode-base",
                        "Against base",
                        "Everything on this branch since it left the base",
                    ))
                    .child(segment(
                        git_view::Section::History,
                        "git-mode-history",
                        "History",
                        "The last 50 commits and their changes",
                    )),
            )
            .children(base.map(|base| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(
                        meta_line(theme, base.into())
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap(),
                    )
                    .when(shows_since, |el| {
                        el.child(
                            row_action("git-task-close".into(), "Close", theme)
                                .tooltip(tooltip(
                                    theme,
                                    "Back to the working changes",
                                    toggle_keys.clone(),
                                ))
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    shell.show_git_section(
                                        project_id,
                                        git_view::Section::Working,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                    })
                    .when(!shows_since, |el| {
                        el.child(
                            row_action(
                                "git-base-change".into(),
                                if status.is_some_and(|status| status.base_ref.is_some()) {
                                    "Change…"
                                } else {
                                    "Set base…"
                                },
                                theme,
                            )
                            .tooltip(tooltip(theme, "Choose the branch to compare with", ""))
                            .on_click(cx.listener(
                                move |shell, _, window, cx| {
                                    shell.open_git_base(project_id, window, cx);
                                },
                            )),
                        )
                    })
            }))
            .children(self.git_changed_by(project_id, view, theme, cx))
            .children(
                view.refresh_error
                    .clone()
                    .map(|error| notice(theme.text, format!("✕ Refresh failed: {error}"))),
            )
    }

    /// "Project | <task worktree>…" (SPEC §20E.1): what the Files (`git: false`) or Git view
    /// shows; hidden while no task of the workspace has a worktree.
    fn target_chooser(
        &self,
        project_id: Ulid,
        shown: Option<Ulid>,
        git: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Div> {
        let choices = worktree_target::choices(project_id, self.board.tasks());
        if choices.is_empty() {
            return None;
        }
        let view = if git { "git" } else { "files" };
        let chips = choices
            .into_iter()
            .map(|choice| {
                let target = choice.worktree_task_id;
                Chip {
                    id: match target {
                        Some(task_id) => SharedString::from(format!("{view}-target-{task_id}")),
                        None => SharedString::from(format!("{view}-target-project")),
                    },
                    tip: match &choice.branch {
                        Some(branch) => format!("This task's worktree: {branch}"),
                        None => "The workspace folder".to_owned(),
                    },
                    active: shown == target,
                    label: choice.label.into(),
                    pick: target,
                }
            })
            .collect();
        Some(Self::chip_row(
            "Showing",
            chips,
            theme,
            move |shell, target, window, cx| {
                if git {
                    window.focus(&shell.git_focus, cx);
                    shell.set_git_target(project_id, target, window, cx);
                } else {
                    window.focus(&shell.files_focus, cx);
                    shell.set_files_target(project_id, target, window, cx);
                }
            },
            cx,
        ))
    }

    /// "Repo: <workspace> | fe | be" (ADR 0029): which repository of the workspace the Git view
    /// shows; hidden with fewer than two and while a task worktree is shown (one repository).
    fn repo_chooser(
        &self,
        project_id: Ulid,
        view: &GitView,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Div> {
        if view.worktree_task_id.is_some() {
            return None;
        }
        let repos = self.git_repos.get(&project_id)?;
        let workspace = self
            .sidebar
            .projects()
            .iter()
            .find(|p| p.id == project_id)
            .map_or("Workspace", |p| p.name.as_str());
        let chips: Vec<_> = git_view::repo_choices(repos, workspace)
            .into_iter()
            .enumerate()
            .map(|(at, (repo, label))| Chip {
                id: SharedString::from(format!("git-repo-{at}")),
                label: SharedString::from(label.to_owned()),
                tip: match &repo {
                    Some(repo) => format!("The git repository in {repo}/"),
                    None => "The workspace folder's git repository".to_owned(),
                },
                active: view.repo == repo,
                pick: repo,
            })
            .collect();
        if chips.is_empty() {
            return None;
        }
        Some(Self::chip_row(
            "Repo",
            chips,
            theme,
            move |shell, repo, window, cx| {
                window.focus(&shell.git_focus, cx);
                shell.set_git_repo(project_id, repo, window, cx);
            },
            cx,
        ))
    }

    /// A chooser row: `heading`, then one chip per choice. A click on an inactive chip calls
    /// `on_pick` with its `pick`.
    fn chip_row<P: Clone + 'static>(
        heading: &'static str,
        chips: Vec<Chip<P>>,
        theme: &Theme,
        on_pick: impl Fn(&mut Self, P, &mut Window, &mut Context<'_, Self>) + Clone + 'static,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let hover = theme.surface;
        let chips = chips.into_iter().map(|chip| {
            let Chip {
                id,
                label,
                tip,
                active,
                pick,
            } = chip;
            let on_pick = on_pick.clone();
            div()
                .id(id)
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .tooltip(tooltip(theme, &tip, ""))
                .when(active, |el| {
                    el.bg(rgb(theme.surface_raised))
                        .font_weight(FontWeight(weight::MEDIUM))
                        .text_color(rgb(theme.text))
                })
                .when(!active, |el| {
                    el.cursor_pointer()
                        .text_color(rgb(theme.text_muted))
                        .hover(move |style| style.bg(rgb(hover)))
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            on_pick(shell, pick.clone(), window, cx);
                        }))
                })
                .child(label)
        });
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(space::XS))
            .text_size(font_size::META)
            .child(div().text_color(rgb(theme.text_muted)).child(heading))
            .children(chips)
    }

    /// "Changed by: All | claude #1 | …" (SPEC §20C.4): keeps only the files a session changed
    /// in the last 24 h; hidden while no session changed any (and in the History).
    fn git_changed_by(
        &self,
        project_id: Ulid,
        view: &GitView,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Div> {
        let changers = view.changers(self.sidebar.sessions());
        if changers.is_empty() {
            return None;
        }
        let filtered = view.changed_by.session;
        let hover = theme.surface;
        let chip = |id: SharedString, label: String, session: Option<Ulid>| {
            let active = filtered == session;
            let tip = match session {
                None => "Every changed file".to_owned(),
                Some(_) => format!("Only the files {label} changed in the last 24 h"),
            };
            div()
                .id(id)
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .tooltip(tooltip(theme, &tip, ""))
                .whitespace_nowrap()
                .when(active, |el| {
                    el.bg(rgb(theme.surface_raised))
                        .font_weight(FontWeight(weight::MEDIUM))
                        .text_color(rgb(theme.text))
                })
                .when(!active, |el| {
                    el.cursor_pointer()
                        .text_color(rgb(theme.text_muted))
                        .hover(move |style| style.bg(rgb(hover)))
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            window.focus(&shell.git_focus, cx);
                            shell.filter_git_changed_by(project_id, session, window, cx);
                        }))
                })
                .child(label)
        };
        Some(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.text_muted)).child("Changed by"))
                .child(chip("git-changed-by-all".into(), "All".into(), None))
                .children(changers.into_iter().map(|changer| {
                    let id = SharedString::from(format!("git-changed-by-{}", changer.session_id));
                    chip(id, changer.label, Some(changer.session_id))
                })),
        )
    }

    /// "Fetch" / "Push" on the header's upstream line: shows "Fetching…" while it runs, and is
    /// faded while another runs (or, for Push, without a branch and upstream).
    fn git_remote_action(
        &self,
        project_id: Ulid,
        view: &GitView,
        status: Option<&GitStatus>,
        remote: Remote,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let id = SharedString::from(format!("git-{}", remote.label().to_lowercase()));
        let running = view.remotes.running;
        let no_upstream = status.is_none_or(|s| s.branch.is_none() || s.upstream.is_none());
        let tip = match remote {
            Remote::Fetch => "Fetch from the remotes (refreshes the base)",
            Remote::Push if no_upstream => "This branch has no upstream to push to",
            Remote::Push => "Push this branch to its upstream (never forced)",
        };
        let idle = |label: &'static str, opacity: f32| {
            div()
                .id(id.clone())
                .flex_none()
                .px(px(space::XS))
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .opacity(opacity)
                .child(label)
        };
        if running == Some(remote) {
            return idle(remote.running_label(), 1.0).into_any_element();
        }
        if running.is_some() || (remote == Remote::Push && no_upstream) {
            return idle(remote.label(), opacity::DISABLED)
                .tooltip(tooltip(theme, tip, ""))
                .into_any_element();
        }
        row_action(id, remote.label(), theme)
            .tooltip(tooltip(theme, tip, ""))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.run_git_remote(project_id, remote, window, cx);
            }))
            .into_any_element()
    }

    /// The commit message box under the changed-file list (Working changes): Enter breaks a
    /// line, Ctrl+Enter or the button commits the staged changes.
    fn git_commit_box(
        &self,
        project_id: Ulid,
        view: &GitView,
        status: &GitStatus,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let draft = &view.commit;
        let generating = draft.generating.is_some();
        let focused = self.git_commit_focus.is_focused(window);
        let staged = status.staged.len();
        let summary = match staged {
            0 => "Nothing staged".to_owned(),
            1 => "1 staged file".to_owned(),
            n => format!("{n} staged files"),
        };
        let label = if draft.busy {
            "Committing…"
        } else {
            "Commit"
        };
        let commit = if draft.busy || generating || staged == 0 {
            disabled_button("git-commit", label, theme, true)
        } else {
            button("git-commit", label, theme, true).on_click(
                cx.listener(move |shell, _, window, cx| shell.commit_git(project_id, window, cx)),
            )
        };
        let tip = if staged == 0 {
            "Stage changes to commit them"
        } else {
            "Commit the staged changes"
        };
        let write = if draft.busy || generating {
            let label = if generating { "✦ Writing…" } else { "✦" };
            disabled_button("git-commit-generate", label, theme, false)
        } else {
            quiet_button("git-commit-generate", "✦", theme, theme.surface_raised).on_click(
                cx.listener(move |shell, _, window, cx| {
                    shell.generate_commit_message(project_id, window, cx)
                }),
            )
        };
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .p(px(space::M))
            .border_t(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .child(
                text_field(
                    "git-commit-message",
                    draft.text.text(),
                    "Commit message",
                    focused,
                    draft.text.all_selected(),
                    theme,
                )
                .max_h(px(theme::GIT_COMMIT_BOX_MAX_HEIGHT))
                .overflow_y_scroll()
                .when(draft.busy || generating, |el| el.opacity(opacity::DISABLED))
                .tooltip(tooltip(
                    theme,
                    "Message of the next commit (Enter: new line)",
                    self.keymap.label(Command::GitFocusCommit),
                ))
                .track_focus(&self.git_commit_focus)
                .on_key_down(cx.listener(Shell::git_commit_key_down))
                .relative()
                .child(self.typing(&self.git_commit_focus, Shell::git_commit_key_down, cx))
                .on_click(cx.listener(|shell, _, window, cx| {
                    window.focus(&shell.git_commit_focus, cx);
                    cx.notify();
                })),
            )
            // Wrapped like the Fetch / Push error: hook output says what to fix.
            .children(draft.error.clone().map(|error| {
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text))
                    .child(format!("✕ {error}"))
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(meta_line(theme, summary.into()).flex_1().min_w_0())
                    .child(write.flex_none().tooltip(tooltip(
                        theme,
                        "Write the message with the workspace's default agent (Conventional Commits)",
                        "",
                    )))
                    .child(commit.tooltip(tooltip(theme, tip, "Ctrl+Enter"))),
            )
    }

    /// The selected file's diff: its path, counts, current hunk and Open in Zed, then the
    /// unified diff (or why it is not shown).
    fn render_git_diff(
        &self,
        project_id: Ulid,
        view: &GitView,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let panel = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .ml(px(space::S))
            .mb(px(space::S))
            .rounded(px(radius::MEDIUM))
            .overflow_hidden()
            .bg(rgb(theme.surface));
        let Some(selected) = view.selected.clone() else {
            let history = view.history.as_ref();
            let (sentence, detail) = match history.map(|history| &history.detail) {
                Some(git_history::Detail::Loading) => ("Loading this commit's files…", None),
                Some(git_history::Detail::Loaded(_)) => ("This commit changed no files.", None),
                Some(git_history::Detail::Failed(error)) => {
                    ("Cannot list this commit's files.", Some(error.clone()))
                }
                Some(git_history::Detail::None) => (
                    "Select a commit to see its files.",
                    Some("Use ↑ ↓ (or j k) to move between commits.".to_owned()),
                ),
                None => (
                    "Select a file to see its changes.",
                    Some("Use ↑ ↓ (or j k) to move between files.".to_owned()),
                ),
            };
            let retry = history
                .filter(|history| matches!(history.detail, git_history::Detail::Failed(_)))
                .and_then(|history| history.selected.clone());
            return panel.child(
                message(theme, None, sentence.into(), detail.map(Into::into))
                    .flex_1()
                    .children(retry.map(|hash| {
                        button("git-show-retry", "Retry", theme, true).on_click(cx.listener(
                            move |shell, _, window, cx| {
                                shell.select_git_commit(project_id, hash.clone(), window, cx);
                            },
                        ))
                    })),
            );
        };
        let comment_button = self.add_comment_button(
            project_id,
            CommentContext::Diff,
            matches!(view.diff, git_view::Diff::Shown(_)) && view.comments.selection.is_some(),
            theme,
            cx,
        );
        let send_button = self.send_comments_button(
            project_id,
            CommentContext::Diff,
            Some(&view.comments),
            theme,
            cx,
        );
        let diff_toggle = self.mode_toggle(
            "git-diff-mode",
            self.diff_mode,
            [(DiffMode::Unified, "Unified"), (DiffMode::Split, "Split")],
            "Unified (one column) or Split (old | new) diff",
            self.keymap.label(Command::GitToggleDiffMode).into(),
            |shell, mode, window, cx| {
                window.focus(&shell.git_focus, cx);
                shell.set_diff_mode(mode, cx);
            },
            theme,
            cx,
        );
        let zed_keys = self.keymap.label(Command::OpenInZed);
        let zed_button = |id: &'static str, primary: bool| {
            button(id, "Open in Zed", theme, primary)
                .flex_none()
                .tooltip(tooltip(theme, "Open this file in Zed", zed_keys.clone()))
                .on_click(cx.listener(move |shell, _, _, cx| {
                    shell.open_git_file_in_zed(project_id, cx);
                }))
        };
        let shown = match &view.diff {
            git_view::Diff::Shown(shown) => Some(Rc::clone(shown)),
            _ => None,
        };
        let renamed = shown
            .as_ref()
            .and_then(|shown| shown.diff.old_path.clone())
            .map(|old| format!("from {old}"));
        let hunk_label = shown.as_ref().and_then(|shown| {
            let count = shown.diff.hunks.len();
            match view.hunk {
                Some(hunk) => Some(format!("Hunk {} of {count}", hunk + 1)),
                None if count > 1 => Some(format!("{count} hunks")),
                None => None,
            }
        });
        let counts = shown
            .as_ref()
            .filter(|shown| !shown.diff.binary)
            .map(|shown| (shown.diff.added, shown.diff.removed));
        // Unified | Split (global, remembered): only for a diff that has lines to lay out.
        let has_lines = shown.as_ref().is_some_and(|shown| {
            !shown.diff.binary && !shown.diff.too_large && !shown.rows.is_empty()
        });
        let header = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::L))
            .py(px(space::S))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(selected.path.clone()),
            )
            .children(renamed.map(|renamed| meta_line(theme, renamed.into()).flex_none()))
            .children(counts.map(|(added, removed)| change_counts(added, removed, theme)))
            .children(hunk_label.map(|label| {
                let keys = format!(
                    "{} / {}",
                    self.keymap.label(Command::GitNextHunk),
                    self.keymap.label(Command::GitPreviousHunk)
                );
                div()
                    .id("git-hunks")
                    .flex_none()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .tooltip(tooltip(theme, "Next / previous hunk", keys))
                    .child(label)
            }))
            .when(has_lines, |header| header.child(diff_toggle))
            .children(view.file_change(&selected).map(|change| {
                let (label, tip, command) = match change.stage {
                    git_view::Stage::Stage => {
                        ("Stage file", "Stage the whole file", Command::GitStage)
                    }
                    git_view::Stage::Unstage => (
                        "Unstage file",
                        "Unstage the whole file",
                        Command::GitUnstage,
                    ),
                };
                button("git-stage-file", label, theme, false)
                    .flex_none()
                    .tooltip(tooltip(theme, tip, self.keymap.label(command)))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        window.focus(&shell.git_focus, cx);
                        shell.stage_git(project_id, change.clone(), window, cx);
                    }))
            }))
            .children(view.discard(&selected).map(|_| {
                let target = selected.clone();
                button("git-discard", "Discard…", theme, false)
                    .flex_none()
                    .tooltip(tooltip(
                        theme,
                        "Discard this file's changes (asks first)",
                        "",
                    ))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.open_git_discard(project_id, target.clone(), window, cx);
                    }))
            }))
            // No comments on a past commit's lines: no dead buttons in the History.
            .when(view.can_comment(), |header| {
                header.child(comment_button).child(send_button)
            })
            .child(zed_button("git-zed", false));
        let body = match &view.diff {
            git_view::Diff::None | git_view::Diff::Loading => {
                message(theme, None, "Loading…".into(), None)
                    .flex_1()
                    .into_any_element()
            }
            git_view::Diff::Failed(error) => message(
                theme,
                None,
                "Cannot show this diff.".into(),
                Some(error.clone().into()),
            )
            .flex_1()
            .child(
                button("git-diff-retry", "Retry", theme, true).on_click(cx.listener(
                    move |shell, _, window, cx| {
                        let diff = shell.git_view(project_id).select(selected.clone());
                        let requests = git_view::Requests {
                            diff,
                            ..git_view::Requests::default()
                        };
                        shell.git_requests(project_id, requests, window, cx);
                    },
                )),
            )
            .into_any_element(),
            git_view::Diff::Shown(shown) if shown.diff.binary || shown.diff.too_large => {
                let sentence = if shown.diff.binary {
                    "This is a binary file."
                } else {
                    "This diff is too large to show here."
                };
                message(
                    theme,
                    None,
                    sentence.into(),
                    Some("Open it in Zed instead.".into()),
                )
                .flex_1()
                .child(zed_button("git-zed-notice", true))
                .into_any_element()
            }
            git_view::Diff::Shown(shown) if shown.rows.is_empty() => message(
                theme,
                None,
                "No line changes to show.".into(),
                Some("Only the file's mode or name changed.".into()),
            )
            .flex_1()
            .into_any_element(),
            git_view::Diff::Shown(shown) => {
                let shown = Rc::clone(shown);
                let mode = self.diff_mode;
                let current = view.hunk;
                // Hunk headers get a stage / unstage action when the file can go by hunk.
                let hunk_stage = view.hunk_change(0).map(|change| change.stage);
                let picked = view.comments.selection;
                let can_comment = view.can_comment();
                let line_height = theme.terminal_font_size * theme.terminal_line_height;
                let row_theme = theme.clone();
                let rows = shown.list_len(mode);
                uniform_list(
                    "git-diff",
                    rows,
                    cx.processor(move |_, range: std::ops::Range<usize>, _, cx| {
                        range
                            .filter_map(|index| {
                                Some(match mode {
                                    DiffMode::Unified => {
                                        let row = shown.rows.get(index)?;
                                        let frame = git_diff_row(
                                            &shown,
                                            index,
                                            current,
                                            picked,
                                            line_height,
                                            &row_theme,
                                        );
                                        match (row, hunk_stage) {
                                            (git_view::DiffRow::Hunk(hunk), Some(stage)) => frame
                                                .child(hunk_stage_button(
                                                    project_id, *hunk, stage, &row_theme, cx,
                                                ))
                                                .into_any_element(),
                                            // New-side lines can be picked for a comment; hover
                                            // previews the pick bar.
                                            (git_view::DiffRow::Line(line), _) => {
                                                match line.new_line.filter(|_| can_comment) {
                                                    Some(number) => pick_on_click(
                                                        frame,
                                                        project_id,
                                                        number,
                                                        picked,
                                                        row_theme.border,
                                                        cx,
                                                    )
                                                    .into_any_element(),
                                                    None => frame.into_any_element(),
                                                }
                                            }
                                            _ => frame.into_any_element(),
                                        }
                                    }
                                    DiffMode::Split => match shown.split.get(index)? {
                                        // Gap and hunk rows span both halves.
                                        git_view::SplitRow::Span(at) => {
                                            let frame = git_diff_row(
                                                &shown,
                                                *at,
                                                current,
                                                picked,
                                                line_height,
                                                &row_theme,
                                            );
                                            match (shown.rows.get(*at)?, hunk_stage) {
                                                (git_view::DiffRow::Hunk(hunk), Some(stage)) => {
                                                    frame
                                                        .child(hunk_stage_button(
                                                            project_id, *hunk, stage, &row_theme,
                                                            cx,
                                                        ))
                                                        .into_any_element()
                                                }
                                                _ => frame.into_any_element(),
                                            }
                                        }
                                        // Only the new (right) half is pickable: comments
                                        // anchor on new-side lines.
                                        row => git_split_row(
                                            &shown,
                                            row,
                                            picked,
                                            line_height,
                                            &row_theme,
                                            |half, number| match number.filter(|_| can_comment) {
                                                Some(number) => pick_on_click(
                                                    half,
                                                    project_id,
                                                    number,
                                                    picked,
                                                    row_theme.border,
                                                    cx,
                                                ),
                                                None => half,
                                            },
                                        )
                                        .into_any_element(),
                                    },
                                })
                            })
                            .collect()
                    }),
                )
                .track_scroll(&self.git_diff_scroll)
                .flex_1()
                .min_h_0()
                .pb(px(space::L))
                .font_family(theme.terminal_font_family.clone())
                .text_size(px(theme.terminal_font_size))
                .into_any_element()
            }
        };
        panel
            .child(header)
            .children(view.zed_error.clone().map(|error| {
                notice(theme.text, format!("✕ {error}"))
                    .px(px(space::L))
                    .pb(px(space::S))
            }))
            .children(view.stage_error.clone().map(|error| {
                notice(theme.text, format!("✕ Staging failed: {error}"))
                    .px(px(space::L))
                    .pb(px(space::S))
            }))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    // Thread cards never touch the tinted diff rows.
                    .gap(px(space::M))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            .child(body),
                    )
                    .children(view.comments_shown().then(|| {
                        self.render_comments(project_id, &view.comments, theme, window, cx)
                    })),
            )
    }

    /// "Comment" (Ctrl+Alt+M): enabled once lines are selected.
    fn add_comment_button(
        &self,
        project_id: Ulid,
        context: CommentContext,
        ready: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let keys = self.keymap.label(Command::AddComment);
        if !ready {
            return disabled_button("doc-comment", "Comment", theme, false)
                .flex_none()
                .tooltip(tooltip(theme, "Select lines first, then comment", keys));
        }
        button("doc-comment", "Comment", theme, false)
            .flex_none()
            .tooltip(tooltip(theme, "Comment on the selected lines", keys))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.start_comment(project_id, context, window, cx);
            }))
    }

    /// "Send to agent" (Ctrl+Shift+S): enabled while the file has open comments; names how
    /// many go (the selected ones if the selection covers some).
    fn send_comments_button(
        &self,
        project_id: Ulid,
        context: CommentContext,
        comments: Option<&Comments>,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let keys = self.keymap.label(Command::SendComments);
        let count = match comments.map(|comments| (&comments.list, comments.selection)) {
            Some((comments::List::Loaded(list), selection)) => {
                doc_dialog::sendable(list, selection).len()
            }
            _ => 0,
        };
        if count == 0 {
            return disabled_button("doc-send", "Send to agent", theme, false)
                .flex_none()
                .tooltip(tooltip(theme, "No open comments to send", keys));
        }
        let what = if count == 1 {
            "Send 1 open comment to an agent".to_owned()
        } else {
            format!("Send {count} open comments to an agent")
        };
        button("doc-send", "Send to agent", theme, false)
            .flex_none()
            .tooltip(tooltip(theme, &what, keys))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.open_send_dialog(project_id, context, window, cx);
            }))
    }

    /// The right margin (SPEC §12, §12B): the new-comment draft, then every thread of the file
    /// in anchor order with its replies, Reply and Resolve / Reopen.
    fn render_comments(
        &self,
        project_id: Ulid,
        comments: &Comments,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let context = comments.context;
        let focused = self.comment_focus.is_focused(window);
        let margin = div()
            .id("doc-comments")
            .flex_none()
            .w(px(theme::DOC_MARGIN_WIDTH))
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .pr(px(space::L))
            .pb(px(space::L))
            .text_size(font_size::BODY);
        let new_draft = comments
            .draft
            .as_ref()
            .and_then(|draft| match draft.target {
                comments::Target::New(selection) => Some((draft, selection)),
                comments::Target::Reply(_) => None,
            });
        let margin = margin.children(new_draft.map(|(draft, selection)| {
            div()
                .flex()
                .flex_col()
                .gap(px(space::S))
                .p(px(space::M))
                .rounded(px(radius::MEDIUM))
                .bg(rgb(theme.surface_raised))
                .child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(comments::lines_label(selection.start, selection.end)),
                )
                .child(self.comment_field(
                    draft,
                    "Add a comment… (Enter saves)",
                    focused,
                    theme,
                    cx,
                ))
        }));
        let list =
            match &comments.list {
                comments::List::Loading => {
                    return margin.child(meta_line(theme, "Loading comments…".into()));
                }
                comments::List::Failed(error) => {
                    let file_path = comments.file_path.clone().unwrap_or_default();
                    return margin
                        .child(notice(
                            theme.text,
                            format!("✕ Cannot load comments: {error}"),
                        ))
                        .child(button("comments-retry", "Retry", theme, false).on_click(
                            cx.listener(move |shell, _, window, cx| {
                                let list = shell
                                    .comments_mut(project_id, context)
                                    .and_then(|comments| comments.begin_list(&file_path));
                                if let Some(file_path) = list {
                                    shell.load_comments(project_id, context, file_path, window, cx);
                                }
                            }),
                        ));
                }
                comments::List::Loaded(list) => list,
            };
        let threads = comments::threads(list);
        let margin = margin.children(
            comments
                .error
                .clone()
                .map(|error| notice(theme.text, format!("✕ {error}"))),
        );
        if threads.is_empty() && new_draft.is_none() {
            let keys = self.keymap.label(Command::AddComment);
            let hint = match comments.selection {
                Some(picked) => format!(
                    "{} selected. Comment on them ({keys}).",
                    comments::lines_label(picked.start, picked.end)
                ),
                None => format!("Select lines, then comment ({keys})."),
            };
            return margin.child(meta_line(theme, hint.into()));
        }
        margin.children(
            threads.into_iter().map(|thread| {
                self.comment_thread(project_id, &thread, comments, focused, theme, cx)
            }),
        )
    }

    /// One thread: anchor (click selects its lines), status, messages, then Reply and
    /// Resolve / Reopen (revealed on hover, like other secondary actions).
    fn comment_thread(
        &self,
        project_id: Ulid,
        thread: &Thread<'_>,
        comments: &Comments,
        focused: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let context = comments.context;
        let root = thread.root;
        let root_id = root.id;
        let group = SharedString::from(format!("thread-{root_id}"));
        let (icon, word) = comments::status_label(root.status);
        let resolved = root.status == mkrew_protocol::CommentStatus::Resolved;
        let lines = root.start_line.zip(root.end_line);
        let anchor = lines.map_or_else(String::new, |(start, end)| {
            comments::lines_label(start, end)
        });
        let picked =
            lines.is_some_and(|(start, end)| comments.selection == Some(Selection { start, end }));
        let reply_draft = comments
            .draft
            .as_ref()
            .filter(|draft| draft.target == comments::Target::Reply(root_id));
        // The icon and word carry the status; only "outdated" warns (like conflict badges).
        let status_color = match root.status {
            mkrew_protocol::CommentStatus::Outdated => theme.status_needs_input,
            _ => theme.text_muted,
        };
        let entry = |comment: &DocComment| {
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(if comment.author == DocComment::USER {
                            "You".to_owned()
                        } else {
                            comment.author.clone()
                        }),
                )
                .child(
                    div()
                        .text_color(rgb(theme.text))
                        .child(comment.body.clone()),
                )
        };
        let actions = div()
            .flex()
            .gap(px(space::XS))
            // Also shown on the picked thread (its anchor was clicked) and while replying.
            .opacity(if picked || reply_draft.is_some() {
                opacity::FULL
            } else {
                opacity::HIDDEN
            })
            .group_hover(group.clone(), |style| style.opacity(opacity::FULL))
            .when(!resolved && reply_draft.is_none(), |el| {
                el.child(
                    button("comment-reply", "Reply", theme, false).on_click(cx.listener(
                        move |shell, _, window, cx| {
                            shell.start_reply(project_id, context, root_id, window, cx)
                        },
                    )),
                )
            })
            // Diff comments have no whole-file text to compare with the snapshot.
            .when(
                root.sent_snapshot.is_some() && context == CommentContext::Doc,
                |el| {
                    el.child(
                        button("comment-changes", "View changes", theme, false)
                            .tooltip(tooltip(theme, "Compare the file as sent with now", ""))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                shell.open_changes(project_id, root_id, window, cx);
                            })),
                    )
                },
            )
            .child(
                button(
                    "comment-status",
                    if comments::can_reopen(root.status) {
                        "Reopen"
                    } else {
                        "Resolve"
                    },
                    theme,
                    false,
                )
                .on_click(cx.listener(move |shell, _, window, cx| {
                    shell.set_comment_status(project_id, context, root_id, !resolved, window, cx);
                })),
            );
        div()
            .id(group.clone())
            .group(group)
            .flex()
            .flex_col()
            .gap(px(space::S))
            .p(px(space::M))
            .rounded(px(radius::MEDIUM))
            .bg(rgb(theme.surface_raised))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(if picked {
                theme.pane_focus_border
            } else {
                theme.surface_raised
            }))
            .when(resolved, |el| el.opacity(opacity::SECONDARY))
            .child(
                div()
                    .id("comment-anchor")
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .text_size(font_size::META)
                    .cursor_pointer()
                    .when_some(lines, |el, (start, end)| {
                        el.on_click(cx.listener(move |shell, _, _, cx| {
                            shell.pick_lines(project_id, context, start, end, false, cx);
                        }))
                    })
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text_muted))
                            .child(anchor),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(rgb(status_color))
                            .child(format!("{icon} {word}")),
                    ),
            )
            .children(root.quote.clone().filter(|q| !q.is_empty()).map(|quote| {
                div()
                    .pl(px(space::S))
                    .border_l(px(theme::DOC_QUOTE_BAR_WIDTH))
                    .border_color(rgb(theme.border))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(quote.lines().next().unwrap_or_default().to_owned())
            }))
            .child(entry(root))
            .children(thread.replies.iter().map(|reply| entry(reply)))
            .children(
                reply_draft.map(|draft| {
                    self.comment_field(draft, "Reply… (Enter sends)", focused, theme, cx)
                }),
            )
            .child(actions)
    }

    /// The draft's input (takes the keyboard when clicked) and its saving / failure line.
    fn comment_field(
        &self,
        draft: &comments::Draft,
        placeholder: &'static str,
        focused: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let status = if draft.busy {
            Some(("…", theme.text_muted, "Saving".to_owned()))
        } else {
            draft
                .error
                .clone()
                .map(|error| ("✕", theme.status_exited, error))
        };
        div()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .child(
                text_field(
                    "comment-draft",
                    &draft.text,
                    placeholder,
                    focused,
                    draft.text.all_selected(),
                    theme,
                )
                .bg(rgb(theme.surface))
                .track_focus(&self.comment_focus)
                .on_key_down(cx.listener(Shell::comment_key_down))
                .relative()
                .child(self.typing(&self.comment_focus, Shell::comment_key_down, cx))
                .on_click(cx.listener(|shell, _, window, cx| {
                    window.focus(&shell.comment_focus, cx);
                    cx.notify();
                })),
            )
            .children(status.map(|(icon, color, text)| {
                div()
                    .flex()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .child(div().text_color(rgb(color)).child(icon))
                    .child(div().text_color(rgb(theme.text)).child(text))
            }))
    }

    /// A segmented control over two modes (Docs and Files Preview / Source, Git Unified /
    /// Split); the inactive segment runs `switch` with its mode. `id` prefixes the segments'
    /// element ids (`(id, index)`).
    #[allow(clippy::too_many_arguments)]
    fn mode_toggle<M: Copy + PartialEq + 'static>(
        &self,
        id: &'static str,
        mode: M,
        options: [(M, &'static str); 2],
        tip: &'static str,
        keys: SharedString,
        switch: impl Fn(&mut Self, M, &mut Window, &mut Context<'_, Self>) + Clone + 'static,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let hover = theme.surface_raised;
        div()
            .flex_none()
            .flex()
            .gap(px(space::XS))
            .children(
                options
                    .into_iter()
                    .enumerate()
                    .map(|(index, (target, label))| {
                        let active = target == mode;
                        let switch = switch.clone();
                        div()
                            .id((id, index))
                            .px(px(space::S))
                            .py(px(space::XS))
                            .rounded(px(radius::SMALL))
                            .text_size(font_size::BODY)
                            .tooltip(tooltip(theme, tip, keys.clone()))
                            .when(active, |el| {
                                el.bg(rgb(theme.surface_raised))
                                    .font_weight(FontWeight(weight::MEDIUM))
                                    .text_color(rgb(theme.text))
                            })
                            .when(!active, |el| {
                                el.cursor_pointer()
                                    .text_color(rgb(theme.text_muted))
                                    .hover(move |style| style.bg(rgb(hover)))
                                    .active(|style| style.opacity(opacity::ACTIVE))
                                    .on_click(cx.listener(move |shell, _, window, cx| {
                                        switch(shell, target, window, cx);
                                    }))
                            })
                            .child(label)
                    }),
            )
    }

    /// A `.md` file of the Files view as the Docs preview: the page of the editor's text
    /// (unsaved edits included).
    fn render_md_preview(&self, view: &FilesView, theme: &Theme) -> gpui::AnyElement {
        match view.preview() {
            None => message(theme, None, "Rendering the preview…".into(), None)
                .flex_1()
                .into_any_element(),
            Some(shown) if shown.blocks.is_empty() => {
                message(theme, None, "This file is empty.".into(), None)
                    .flex_1()
                    .into_any_element()
            }
            Some(shown) => {
                preview_page("file-preview", preview::blocks(&shown.blocks, shown, theme))
            }
        }
    }

    /// "New task" with its shortcut: the primary action of the empty board, else quiet.
    fn new_task_button(
        &self,
        project_id: Ulid,
        primary: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let keys = self.keymap.label(Command::NewTask);
        button("new-task", "New task", theme, primary)
            .flex_none()
            .tooltip(tooltip(theme, "New task", keys.clone()))
            .child(
                div()
                    .text_size(font_size::META)
                    .opacity(opacity::SECONDARY)
                    .child(keys),
            )
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.open_task_dialog(TaskDialog::new(project_id), window, cx);
            }))
    }

    /// The modal new / edit task dialog (a stray click on its shade never throws a draft
    /// away; Esc or Cancel does).
    fn render_task_dialog(
        &self,
        dialog: &TaskDialog,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let editing = dialog.task_id().is_some();
        let default_agent = self
            .sidebar
            .projects()
            .iter()
            .find(|project| project.id == dialog.project_id())
            .and_then(|project| project.default_agent.as_deref());
        let label = |text: &'static str| {
            div()
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let field = |id: &'static str, which: Field, text: &str, placeholder: &'static str| {
            let selected = match which {
                Field::Title => dialog.title.all_selected(),
                Field::Prompt => dialog.prompt.all_selected(),
            };
            text_field(
                id,
                text,
                placeholder,
                dialog.field == which,
                selected,
                theme,
            )
            .on_click(cx.listener(move |shell, _, _, cx| {
                if let Some(dialog) = shell.task_dialog.as_mut() {
                    dialog.field = which;
                    cx.notify();
                }
            }))
        };
        let agents = match &dialog.agents {
            Agents::Loading => vec![label("Loading agents…").into_any_element()],
            Agents::Failed(error) => vec![
                notice(theme.text, format!("✕ Cannot list the agents: {error}")).into_any_element(),
            ],
            Agents::Loaded(profiles) => dialog
                .agent_choices(profiles, default_agent)
                .into_iter()
                .map(|choice| {
                    let hover = theme.surface_raised;
                    let agent = choice.id;
                    div()
                        .id(SharedString::from(format!("task-agent-{agent}")))
                        .px(px(space::S))
                        .py(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .border(px(theme::BORDER_WIDTH))
                        .cursor_pointer()
                        .text_size(font_size::BODY)
                        .when(choice.selected, |el| {
                            el.border_color(rgb(theme.accent))
                                .text_color(rgb(theme.text))
                                .font_weight(FontWeight(weight::MEDIUM))
                                .child(format!("✓ {}", choice.label))
                        })
                        .when(!choice.selected, |el| {
                            el.border_color(rgb(theme.border))
                                .text_color(rgb(theme.text_muted))
                                .hover(move |style| style.bg(rgb(hover)))
                                .active(|style| style.opacity(opacity::ACTIVE))
                                .child(choice.label)
                        })
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            if let Some(dialog) = shell.task_dialog.as_mut() {
                                dialog.choose_agent(&agent);
                                cx.notify();
                            }
                        }))
                        .into_any_element()
                })
                .collect(),
        };
        let hover = theme.surface_raised;
        let locked = dialog.worktree_locked();
        let worktree = div()
            .id("task-worktree")
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::XS))
            .py(px(space::XS))
            .rounded(px(radius::SMALL))
            .text_size(font_size::BODY)
            .tooltip(tooltip(
                theme,
                if locked && !dialog.busy {
                    "This task already has its worktree"
                } else {
                    "The agent works on its own branch; your checkout stays untouched"
                },
                "",
            ))
            .when(locked, |el| el.text_color(rgb(theme.text_muted)))
            .when(!locked, |el| {
                el.cursor_pointer()
                    .text_color(rgb(theme.text))
                    .hover(move |style| style.bg(rgb(hover)))
                    .active(|style| style.opacity(opacity::ACTIVE))
                    .on_click(cx.listener(|shell, _, _, cx| {
                        if let Some(dialog) = shell.task_dialog.as_mut() {
                            dialog.toggle_worktree();
                        }
                        cx.notify();
                    }))
            })
            .child(
                div()
                    .size(px(space::M))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(radius::SMALL))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(theme.border))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.accent))
                    .child(if dialog.worktree { "✓" } else { "" }),
            )
            .child(div().flex_1().child("Run in a worktree"));
        let save_label = match (dialog.busy, editing) {
            (true, _) => "Saving…",
            (false, true) => "Save",
            (false, false) => "Create task",
        };
        let panel = div()
            .id("task-dialog")
            .w(px(theme::DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            // The prompt picker over it has the keyboard meanwhile.
            .when(self.prompt_picker.is_none(), |el| {
                el.track_focus(&self.dialog_focus)
                    .on_key_down(cx.listener(Self::task_dialog_key_down))
                    .relative()
                    .child(self.typing(&self.dialog_focus, Self::task_dialog_key_down, cx))
            })
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(if editing { "Edit task" } else { "New task" }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .child(label("Title"))
                    .child(field(
                        "task-title",
                        Field::Title,
                        &dialog.title,
                        "What should be done?",
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(label("Prompt"))
                            .when(!dialog.busy, |el| {
                                el.child(
                                    button("task-from-prompt", "From prompt…", theme, false)
                                        .tooltip(tooltip(
                                            theme,
                                            "Add a prompt from the library",
                                            "",
                                        ))
                                        .on_click(cx.listener(|shell, _, window, cx| {
                                            shell.open_prompt_picker(
                                                prompt_picker::Target::TaskDialog,
                                                window,
                                                cx,
                                            );
                                        })),
                                )
                            }),
                    )
                    .child(
                        field(
                            "task-prompt",
                            Field::Prompt,
                            dialog.prompt.text(),
                            "Sent to the agent when the task runs.",
                        )
                        .h(px(theme::PROMPT_FIELD_HEIGHT))
                        .overflow_y_scroll()
                        .track_scroll(&self.prompt_scroll),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .child(label("Agent"))
                    .child(div().flex().flex_wrap().gap(px(space::XS)).children(agents)),
            )
            .child(worktree)
            .children(dialog.error.clone().map(|error| {
                div()
                    .flex()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                    .child(div().text_color(rgb(theme.text)).child(error))
            }))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .pt(px(space::S))
                    .when(editing && dialog.busy, |el| {
                        el.child(disabled_button("task-delete", "Delete", theme, false))
                    })
                    .when(editing && !dialog.busy, |el| {
                        el.child(
                            button("task-delete", "Delete", theme, false)
                                .tooltip(tooltip(theme, "Delete this task", ""))
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    shell.delete_task(window, cx);
                                })),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        button("task-cancel", "Cancel", theme, false)
                            .tooltip(tooltip(theme, "Discard changes", "Esc"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.close_task_dialog(window, cx);
                            })),
                    )
                    .child(if dialog.busy {
                        disabled_button("task-save", save_label, theme, true)
                    } else {
                        button("task-save", save_label, theme, true)
                            .tooltip(tooltip(theme, save_label, "Ctrl+Enter"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.save_task(window, cx);
                            }))
                    }),
            );
        Self::modal_shade("task-dialog-shade", panel, cx)
    }

    /// The "Undo task" confirmation: what is discarded (files, then the diff), and a
    /// destructive confirm that stays disabled until the preview allows it.
    fn render_undo_dialog(
        &self,
        dialog: &UndoDialog,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let label = |text: String| {
            div()
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        // No border: the raised surface sets the boxes apart inside the dialog (§14).
        let scroll_box = |id: &'static str, height: f32| {
            div()
                .id(id)
                .max_h(px(height))
                .flex()
                .flex_col()
                .py(px(space::XS))
                .rounded(px(radius::MEDIUM))
                .bg(rgb(theme.surface_raised))
                .overflow_scroll()
        };
        let error_row = |error: String| {
            div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                .child(div().text_color(rgb(theme.text)).child(error))
        };
        let nothing_changed = dialog.nothing_changed();
        let mut body: Vec<gpui::AnyElement> = Vec::new();
        let mut intro = format!(
            "Restores the workspace folder to how it was before {} ran",
            dialog.subject
        );
        // A task's checkpoint is a commit, a turn's a snapshot tree.
        let (checkpoint, cli) = match &dialog.target {
            undo_dialog::Target::Task(task_id) => {
                ("commit", Some(format!("mk task undo {task_id}")))
            }
            undo_dialog::Target::Turn(_) => ("snapshot", None),
        };
        match &dialog.preview {
            Preview::Loading => body.push(label("Loading the changes…".into()).into_any_element()),
            Preview::Failed(error) => body.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(
                        error_row(format!("Cannot list the changes: {error}"))
                            .flex_1()
                            .min_w_0(),
                    )
                    .child(
                        button("undo-retry", "Retry", theme, false)
                            .tooltip(tooltip(theme, "List the changes again", ""))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.load_undo_preview(window, cx);
                            })),
                    )
                    .into_any_element(),
            ),
            Preview::Loaded(_) if nothing_changed => {
                intro = format!("Nothing changed since {} ran", dialog.subject);
            }
            Preview::Loaded(shown) => {
                let commit: String = shown.preview.checkpoint.chars().take(7).collect();
                intro = format!("{intro} ({checkpoint} {commit})");
                body.push(label(format!("Files ({})", shown.files.len())).into_any_element());
                body.push(
                    scroll_box("undo-files", theme::UNDO_FILES_HEIGHT)
                        .children(shown.files.iter().map(|file| {
                            let (marker, effect) = file.change.label();
                            div()
                                .flex()
                                .gap(px(space::S))
                                .px(px(space::S))
                                .text_size(font_size::META)
                                .child(
                                    div()
                                        .flex_none()
                                        .font_family(theme.terminal_font_family.clone())
                                        .text_color(rgb(theme.text_muted))
                                        .child(marker),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .text_color(rgb(theme.text))
                                        .child(file.path.clone()),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .text_color(rgb(theme.text_muted))
                                        .child(effect),
                                )
                        }))
                        .into_any_element(),
                );
                body.push(label("Diff".into()).into_any_element());
                body.push(
                    scroll_box("undo-diff", theme::UNDO_DIFF_HEIGHT)
                        // `surface`, so hunk rows (`diff_hunk_header`) show as a band.
                        .bg(rgb(theme.surface))
                        .font_family(theme.terminal_font_family.clone())
                        .text_size(font_size::META)
                        .when(shown.lines.is_empty(), |el| {
                            el.child(
                                div()
                                    .px(px(space::S))
                                    .font_family(theme.ui_font_family.clone())
                                    .text_color(rgb(theme.text_muted))
                                    .child("No tracked file changed."),
                            )
                        })
                        .children(shown.lines.iter().map(|line| diff_row(line, theme)))
                        .into_any_element(),
                );
                if shown.cut() {
                    let more = match cli {
                        Some(cli) => {
                            format!("The diff is cut short here; `{cli}` prints all of it.")
                        }
                        None => "The diff is cut short here.".to_owned(),
                    };
                    body.push(label(more).into_any_element());
                }
            }
        }
        let confirm = dialog.confirm_request(self.sidebar.sessions());
        // Only the reason the user can act on (a live terminal, a working agent); loading,
        // failures and "nothing changed" already show in the body, "Discarding…" on the button.
        let hint = confirm.as_ref().err().copied().filter(|_| {
            matches!(dialog.preview, Preview::Loaded(_)) && !dialog.busy && !nothing_changed
        });
        let panel = div()
            .id("undo-dialog")
            .w(px(theme::UNDO_DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::undo_dialog_key_down))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(dialog.heading()),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child(if nothing_changed {
                        format!("{intro}: there is nothing to undo.")
                    } else {
                        format!("{intro}. The changes below are discarded and cannot be recovered.")
                    }),
            )
            .children(body)
            .children(dialog.error.clone().map(error_row))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .pt(px(space::S))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .children(hint),
                    )
                    .when(nothing_changed, |el| {
                        el.child(
                            button("undo-close", "Close", theme, true)
                                .tooltip(tooltip(theme, "Close", "Esc"))
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    shell.close_undo_dialog(window, cx);
                                })),
                        )
                    })
                    .when(!nothing_changed, |el| {
                        el.child(
                            button("undo-cancel", "Cancel", theme, false)
                                .tooltip(tooltip(theme, "Keep the changes", "Esc"))
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    shell.close_undo_dialog(window, cx);
                                })),
                        )
                        .child(match confirm {
                            Ok(_) => danger_button("undo-confirm", "Discard changes", theme)
                                .tooltip(tooltip(theme, "Discard every change listed above", ""))
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    shell.confirm_undo(window, cx);
                                })),
                            Err(_) => {
                                let label = if dialog.busy {
                                    "Discarding…"
                                } else {
                                    "Discard changes"
                                };
                                disabled_button("undo-confirm", label, theme, false)
                                    .bg(rgb(theme.diff_del_bg))
                                    .text_color(rgb(theme.diff_del_text))
                            }
                        })
                    }),
            );
        Self::modal_shade("undo-dialog-shade", panel, cx)
    }

    /// "Merge back…" / "Remove worktree…" (SPEC §20E.1): the commits to merge or the
    /// uncommitted files blocking the removal, the confirm of the current step, then the result.
    fn render_worktree_dialog(
        &self,
        dialog: &WorktreeDialog,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        use worktree_dialog::{Done, Kind, Step};
        let label = |text: String| {
            div()
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let error_row = |error: String| {
            div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                .child(div().text_color(rgb(theme.text)).child(error))
        };
        // One row per file or commit: a muted mono marker, then the text (ellipsized).
        let row = |marker: Option<String>, text: String| {
            div()
                .flex()
                .gap(px(space::S))
                .px(px(space::S))
                .text_size(font_size::META)
                .children(marker.map(|marker| {
                    div()
                        .flex_none()
                        .font_family(theme.terminal_font_family.clone())
                        .text_color(rgb(theme.text_muted))
                        .child(marker)
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_color(rgb(theme.text))
                        .child(text),
                )
        };
        // No border: the raised surface sets the list apart inside the dialog (§14).
        let list = |title: String, rows: Vec<gpui::Div>| {
            div()
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .child(label(title))
                .child(
                    div()
                        .id("worktree-list")
                        .max_h(px(theme::UNDO_FILES_HEIGHT))
                        .flex()
                        .flex_col()
                        .py(px(space::XS))
                        .rounded(px(radius::MEDIUM))
                        .bg(rgb(theme.surface_raised))
                        .overflow_scroll()
                        .children(rows),
                )
        };
        let paths = |paths: &[String]| -> Vec<gpui::Div> {
            paths.iter().map(|path| row(None, path.clone())).collect()
        };
        let body = match &dialog.step {
            Step::Loading => Some(label("Loading…".into())),
            Step::Failed(error) => Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(error_row(format!("Cannot read the worktree: {error}")).flex_1())
                    .child(
                        button("worktree-retry", "Retry", theme, false)
                            .tooltip(tooltip(theme, "Read the worktree again", ""))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.load_worktree_dialog(window, cx);
                            })),
                    ),
            ),
            Step::Merge(plan) => Some(list(
                format!("Commits ({})", plan.commits.len()),
                plan.commits
                    .iter()
                    .map(|commit| {
                        let short: String = commit.hash.chars().take(7).collect();
                        row(Some(short), commit.summary.clone())
                    })
                    .collect(),
            )),
            Step::Remove(plan) if plan.uncommitted.is_empty() => None,
            Step::Remove(plan) => Some(list(
                format!("Uncommitted ({})", plan.uncommitted.len()),
                paths(&plan.uncommitted),
            )),
            Step::ConfirmDiscard(plan) => Some(list(
                format!("Discarded ({})", plan.discardable.len()),
                paths(&plan.discardable),
            )),
            Step::Done(Done::Conflicts(conflicts)) => Some(list(
                format!("Conflicting files ({})", conflicts.len()),
                paths(conflicts),
            )),
            // A merged Pick: the original's move, then how removing the others went.
            Step::Done(Done::Merged { .. }) => dialog.pick.as_ref().map(|pick| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .children(pick.lines().into_iter().map(|line| div().child(line)))
            }),
            Step::Done(_) => None,
        };
        let confirm = dialog.confirm_request(self.sidebar.sessions());
        // Why the confirm is off, when the user can act on it.
        let hint = confirm
            .as_ref()
            .err()
            .filter(|why| !why.is_empty() && !dialog.busy)
            // A removal blocked by files already says so in the intro.
            .filter(|_| match &dialog.step {
                Step::Merge(_) => true,
                Step::Remove(plan) => plan.uncommitted.is_empty(),
                _ => false,
            })
            .cloned();
        // Commits and unstaging happen in the worktree's Git view.
        let open_git = match &dialog.step {
            Step::Merge(plan) => !plan.worktree_dirty.is_empty(),
            Step::Remove(plan) => !plan.uncommitted.is_empty(),
            _ => false,
        };
        let git_button = open_git.then(|| {
            button("worktree-git", "Open Git view", theme, false)
                .tooltip(tooltip(theme, "The Git view on this task's worktree", ""))
                .on_click(cx.listener(|shell, _, window, cx| {
                    shell.worktree_dialog_git(window, cx);
                }))
        });
        let cancel = |id: &'static str, text: &'static str| {
            button(id, text, theme, false)
                .tooltip(tooltip(theme, "Close", "Esc"))
                .on_click(cx.listener(|shell, _, window, cx| {
                    shell.close_worktree_dialog(window, cx);
                }))
        };
        let confirm_listener = cx.listener(|shell, _, window, cx| {
            shell.confirm_worktree(window, cx);
        });
        let mut buttons: Vec<gpui::AnyElement> = Vec::new();
        buttons.extend(git_button.map(IntoElement::into_any_element));
        match &dialog.step {
            Step::Done(done) => {
                let offer = dialog
                    .pick
                    .as_ref()
                    .filter(|_| matches!(done, Done::Merged { .. }))
                    .and_then(|pick| pick.offer());
                if let Some(offer) = offer {
                    buttons.push(
                        danger_button("worktree-remove-others", offer, theme)
                            .tooltip(tooltip(
                                theme,
                                "Delete the other agents' worktree folders (unmerged branches are kept)",
                                "",
                            ))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.remove_other_worktrees(window, cx);
                            }))
                            .into_any_element(),
                    );
                }
                buttons.push(cancel("worktree-close", "Close").into_any_element());
            }
            Step::ConfirmDiscard(_) => {
                buttons.push(
                    button("worktree-keep", "Keep files", theme, false)
                        .tooltip(tooltip(theme, "Back to the list; nothing is discarded", ""))
                        .on_click(cx.listener(|shell, _, _, cx| {
                            if let Some(dialog) = shell.worktree_dialog.as_mut() {
                                dialog.keep_files();
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                );
                buttons.push(if confirm.is_ok() {
                    danger_button("worktree-confirm", "Discard files", theme)
                        .tooltip(tooltip(theme, "Discard every file listed above", ""))
                        .on_click(confirm_listener)
                        .into_any_element()
                } else {
                    disabled_button("worktree-confirm", "Discarding…", theme, false)
                        .bg(rgb(theme.diff_del_bg))
                        .text_color(rgb(theme.diff_del_text))
                        .into_any_element()
                });
            }
            Step::Remove(plan) if !plan.uncommitted.is_empty() => {
                buttons.push(cancel("worktree-cancel", "Cancel").into_any_element());
                if !plan.discardable.is_empty() {
                    buttons.push(
                        danger_button("worktree-discard", "Discard…", theme)
                            .tooltip(tooltip(theme, "Choose to discard these changes", ""))
                            .on_click(cx.listener(|shell, _, _, cx| {
                                if let Some(dialog) = shell.worktree_dialog.as_mut() {
                                    dialog.ask_discard();
                                }
                                cx.notify();
                            }))
                            .into_any_element(),
                    );
                }
            }
            step => {
                buttons.push(cancel("worktree-cancel", "Cancel").into_any_element());
                let remove = dialog.kind == Kind::Remove;
                let text = match (remove, dialog.busy) {
                    (false, false) => "Merge",
                    (false, true) => "Merging…",
                    (true, false) => "Remove worktree",
                    (true, true) => "Removing…",
                };
                let ready = confirm.is_ok() && !matches!(step, Step::Loading | Step::Failed(_));
                buttons.push(match (ready, remove) {
                    (true, true) => danger_button("worktree-confirm", text, theme)
                        .tooltip(tooltip(theme, "Delete the worktree folder", ""))
                        .on_click(confirm_listener)
                        .into_any_element(),
                    (true, false) => button("worktree-confirm", text, theme, true)
                        .tooltip(tooltip(
                            theme,
                            "Merge the commits above into your checkout",
                            "",
                        ))
                        .on_click(confirm_listener)
                        .into_any_element(),
                    // A disabled removal keeps the destructive tint, like Undo's.
                    (false, true) => disabled_button("worktree-confirm", text, theme, false)
                        .bg(rgb(theme.diff_del_bg))
                        .text_color(rgb(theme.diff_del_text))
                        .into_any_element(),
                    (false, false) => {
                        disabled_button("worktree-confirm", text, theme, true).into_any_element()
                    }
                });
            }
        }
        let panel = div()
            .id("worktree-dialog")
            .w(px(theme::UNDO_DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::worktree_dialog_key_down))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(dialog.heading()),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child(dialog.intro()),
            )
            .children(body)
            .children(dialog.error.clone().map(error_row))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .pt(px(space::S))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .children(hint),
                    )
                    .children(buttons),
            );
        Self::modal_shade("worktree-dialog-shade", panel, cx)
    }

    /// "Compare" of a group card (SPEC §20E.2): one column per child with its changes since
    /// its checkpoint, run time, its Git view and "Pick".
    fn render_compare(
        &self,
        compare: &Compare,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let meta = |text: String| {
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let columns = compare.columns.iter().map(|column| {
            let task_id = column.task_id;
            let git = if column.checkpoint.is_some() && column.has_worktree {
                quiet_button("compare-git", "Git view", theme, theme.border)
                    .tooltip(tooltip(
                        theme,
                        "Its changes in the Git view, on its worktree",
                        "",
                    ))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.compare_git(task_id, window, cx);
                    }))
            } else {
                let why = if column.has_worktree {
                    "No run to compare"
                } else {
                    "Its worktree was removed"
                };
                disabled_button("compare-git", "Git view", theme, false)
                    .tooltip(tooltip(theme, why, ""))
            };
            // Quiet: up to five columns would otherwise show five accent buttons.
            let pick = if column.has_worktree {
                quiet_button("compare-pick", "Pick", theme, theme.border)
                    .tooltip(tooltip(
                        theme,
                        "Merge this result back into your checkout",
                        "",
                    ))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.compare_pick(task_id, window, cx);
                    }))
            } else {
                disabled_button("compare-pick", "Pick", theme, false).tooltip(tooltip(
                    theme,
                    "Its worktree was removed: nothing to merge",
                    "",
                ))
            };
            let changes_color = match column.changes {
                fanout::Changes::Counted { .. } => theme.text,
                _ => theme.text_muted,
            };
            let failed = matches!(column.changes, fanout::Changes::Failed(_));
            div()
                .id(SharedString::from(format!("compare-{task_id}")))
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .p(px(space::M))
                .rounded(px(radius::MEDIUM))
                .bg(rgb(theme.surface_raised))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_size(font_size::BODY)
                        .font_weight(FontWeight(weight::MEDIUM))
                        .text_color(rgb(theme.text))
                        .child(column.agent.clone()),
                )
                .children(column.branch.clone().map(meta))
                .child(meta(match column.run_time(now) {
                    Some(time) => format!("Ran {time}"),
                    None => "Did not run".into(),
                }))
                .child(
                    div()
                        .flex()
                        .gap(px(space::XS))
                        .text_size(font_size::META)
                        .text_color(rgb(changes_color))
                        .when(failed, |el| {
                            el.child(div().text_color(rgb(theme.status_exited)).child("✕"))
                        })
                        .child(column.changes.text()),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .justify_end()
                        .gap(px(space::S))
                        .pt(px(space::S))
                        .child(git)
                        .child(pick),
                )
        });
        let panel = div()
            .id("compare-dialog")
            .w(px(theme::COMPARE_DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::compare_key_down))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Compare agents"),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child(format!(
                        "“{}”: changes since each run started. Pick one to merge it back into \
                         your checkout.",
                        compare.title
                    )),
            )
            .child(div().flex().gap(px(space::M)).children(columns))
            .child(
                div().flex().justify_end().pt(px(space::S)).child(
                    button("compare-close", "Close", theme, false)
                        .tooltip(tooltip(theme, "Close", "Esc"))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_compare(window, cx);
                        })),
                ),
            );
        Self::modal_shade("compare-dialog-shade", panel, cx)
    }

    /// "Turns": the session's turns, newest first, each with "Restore before" (disabled, with
    /// the reason as its tooltip, while the agent works or without a snapshot).
    fn render_turns_dialog(
        &self,
        dialog: &TurnsDialog,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let muted = |text: String| {
            div()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let state = self.session_state(dialog.session_id);
        let hover = theme.surface_raised;
        let list: gpui::AnyElement = match &dialog.listing {
            turns_dialog::Listing::Loading => muted("Loading the turns…".into()).into_any_element(),
            turns_dialog::Listing::Failed(error) => div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                .child(
                    div()
                        .text_color(rgb(theme.text))
                        .child(format!("Cannot list the turns: {error}")),
                )
                .into_any_element(),
            turns_dialog::Listing::Loaded(turns) if turns.is_empty() => {
                muted("No turns yet: they are recorded as the agent works.".into())
                    .into_any_element()
            }
            turns_dialog::Listing::Loaded(turns) => div()
                .id("turns-list")
                .max_h(px(theme::TURNS_LIST_HEIGHT))
                .flex()
                .flex_col()
                .overflow_y_scroll()
                .children(
                    turns_dialog::rows(turns, state, now)
                        .into_iter()
                        .enumerate()
                        .map(|(index, row)| {
                            let n = row.n;
                            let selected = index == dialog.selected;
                            let details = [Some(row.when), Some(row.files), row.lines]
                                .into_iter()
                                .flatten()
                                .collect::<Vec<_>>()
                                .join(" · ");
                            let what = format!("Restore the files to before turn {n} (preview)");
                            let restore = match row.blocked {
                                None => button("turn-restore", "Restore before", theme, false)
                                    .tooltip(tooltip(
                                        theme,
                                        &what,
                                        if selected { "Enter" } else { "" },
                                    ))
                                    .on_click(cx.listener(move |shell, _, window, cx| {
                                        shell.restore_before_turn(n, window, cx);
                                    })),
                                Some(reason) => {
                                    disabled_button("turn-restore", "Restore before", theme, false)
                                        .tooltip(tooltip(theme, reason, ""))
                                }
                            };
                            let diff = match row.diff_blocked {
                                None => button("turn-diff", "Diff since", theme, false)
                                    .tooltip(tooltip(
                                        theme,
                                        &format!("Show the changes since turn {n} started"),
                                        "",
                                    ))
                                    .on_click(cx.listener(move |shell, _, window, cx| {
                                        shell.diff_since_turn(n, window, cx);
                                    })),
                                Some(reason) => {
                                    disabled_button("turn-diff", "Diff since", theme, false)
                                        .tooltip(tooltip(theme, reason, ""))
                                }
                            };
                            div()
                                .id(SharedString::from(format!("turn-{n}")))
                                .flex()
                                .items_center()
                                .gap(px(space::S))
                                .px(px(space::S))
                                .py(px(space::XS))
                                .rounded(px(radius::SMALL))
                                .when(selected, |el| el.bg(rgb(theme.surface_raised)))
                                .when(!selected, |el| el.hover(move |style| style.bg(rgb(hover))))
                                .on_click(cx.listener(move |shell, _, _, cx| {
                                    if let Some(dialog) = shell.turns_dialog.as_mut() {
                                        dialog.selected = index;
                                        cx.notify();
                                    }
                                }))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .child(
                                            div()
                                                .text_size(font_size::BODY)
                                                .font_weight(FontWeight(weight::MEDIUM))
                                                .text_color(rgb(theme.text))
                                                .child(row.title),
                                        )
                                        .child(
                                            div()
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .whitespace_nowrap()
                                                .text_size(font_size::META)
                                                .text_color(rgb(theme.text_muted))
                                                .child(details),
                                        ),
                                )
                                .child(diff.flex_none())
                                .child(restore.flex_none())
                        }),
                )
                .into_any_element(),
        };
        let panel = div()
            .id("turns-dialog")
            .w(px(theme::TURNS_DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::turns_dialog_key_down))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(format!("Turns of {}", dialog.agent)),
            )
            .child(list)
            .child(
                div().flex().justify_end().pt(px(space::S)).child(
                    button("turns-close", "Close", theme, true)
                        .tooltip(tooltip(theme, "Close", "Esc"))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_turns_dialog(window, cx);
                        })),
                ),
            );
        Self::modal_shade("turns-dialog-shade", panel, cx)
    }

    /// "Discard changes?" of a Git view file: what is lost (the file's diff), Cancel and the
    /// danger confirm, enabled once that diff is shown. Looks like the undo dialog.
    fn render_git_discard(
        &self,
        dialog: &git_view::Discard,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let view = self.git.get(&dialog.project_id);
        let shown = view.and_then(|view| view.shown_diff(&dialog.target));
        let muted = |text: String| {
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let error_row = |error: String| {
            div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                .child(div().text_color(rgb(theme.text)).child(error))
        };
        // The file left its group meanwhile (e.g. an agent reverted or staged it).
        let gone = view.is_none_or(|view| view.file(&dialog.target).is_none());
        let failed = view.and_then(|view| match &view.diff {
            git_view::Diff::Failed(error) if view.selected.as_ref() == Some(&dialog.target) => {
                Some(error.clone())
            }
            _ => None,
        });
        let body = match (&shown, failed) {
            _ if gone => muted("This file no longer has these changes.".into()).into_any_element(),
            (Some(shown), _) if shown.diff.binary || shown.diff.too_large => muted(format!(
                "{} {} lines added, {} removed.",
                if shown.diff.binary {
                    "A binary file."
                } else {
                    "Too large to show here."
                },
                shown.diff.added,
                shown.diff.removed
            ))
            .into_any_element(),
            (Some(shown), _) => {
                let line_height = theme.terminal_font_size * theme.terminal_line_height;
                let more = shown
                    .rows
                    .len()
                    .saturating_sub(git_view::DISCARD_PREVIEW_ROWS);
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .child(
                        // `surface`, so hunk rows (`diff_hunk_header`) show as a band.
                        div()
                            .id("git-discard-diff")
                            .max_h(px(theme::UNDO_DIFF_HEIGHT))
                            .flex()
                            .flex_col()
                            .py(px(space::XS))
                            .rounded(px(radius::MEDIUM))
                            .bg(rgb(theme.surface))
                            .overflow_scroll()
                            .font_family(theme.terminal_font_family.clone())
                            .text_size(px(theme.terminal_font_size))
                            .children(
                                (0..shown.rows.len().min(git_view::DISCARD_PREVIEW_ROWS)).map(
                                    |index| {
                                        git_diff_row(shown, index, None, None, line_height, theme)
                                    },
                                ),
                            ),
                    )
                    .when(more > 0, |el| {
                        el.child(muted(format!("… and {more} more lines.")))
                    })
                    .into_any_element()
            }
            (None, Some(error)) => {
                let project_id = dialog.project_id;
                let target = dialog.target.clone();
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(
                        error_row(format!("Cannot show what is lost: {error}"))
                            .flex_1()
                            .min_w_0(),
                    )
                    .child(
                        button("git-discard-retry", "Retry", theme, false)
                            .tooltip(tooltip(theme, "Read the changes again", ""))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                let diff = shell.git_view(project_id).select(target.clone());
                                let requests = git_view::Requests {
                                    diff,
                                    ..git_view::Requests::default()
                                };
                                shell.git_requests(project_id, requests, window, cx);
                            })),
                    )
                    .into_any_element()
            }
            (None, None) => muted("Loading the changes…".into()).into_any_element(),
        };
        let ready = shown.is_some() && !gone && !dialog.busy;
        let panel = div()
            .id("git-discard-dialog")
            .w(px(theme::UNDO_DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::git_discard_key_down))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Discard changes?"),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child(dialog.sentence()),
            )
            .child(body)
            .children(dialog.error.clone().map(error_row))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(space::S))
                    .pt(px(space::S))
                    .child(
                        button("git-discard-cancel", "Cancel", theme, false)
                            .tooltip(tooltip(theme, "Keep the changes", "Esc"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.close_git_discard(window, cx);
                            })),
                    )
                    .child(if ready {
                        danger_button("git-discard-confirm", "Discard changes", theme)
                            .tooltip(tooltip(theme, "Discard the changes shown above", ""))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.confirm_git_discard(window, cx);
                            }))
                    } else {
                        let label = if dialog.busy {
                            "Discarding…"
                        } else {
                            "Discard changes"
                        };
                        disabled_button("git-discard-confirm", label, theme, false)
                            .bg(rgb(theme.diff_del_bg))
                            .text_color(rgb(theme.diff_del_text))
                    }),
            );
        Self::modal_shade("git-discard-shade", panel, cx)
    }

    /// The Git base menu: the base in use, a field for another ref, then Cancel, "Auto-detect"
    /// and "Use this base" (Enter; an empty field also auto-detects).
    fn render_git_base(
        &self,
        dialog: &BaseDialog,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let error_row = |error: String| {
            div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                .child(div().text_color(rgb(theme.text)).child(error))
        };
        let use_label = if dialog.busy {
            "Saving…"
        } else {
            "Use this base"
        };
        let panel = div()
            .id("git-base-dialog")
            .w(px(theme::DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::git_base_key_down))
            .relative()
            .child(self.typing(&self.dialog_focus, Self::git_base_key_down, cx))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Base branch"),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child(
                        "“Against base” shows everything on this branch since it left the base.",
                    ),
            )
            .when(!dialog.current.is_empty(), |el| {
                el.child(meta_line(theme, dialog.current.clone().into()))
            })
            .child(text_field(
                "git-base-ref",
                &dialog.text,
                "A branch or ref, e.g. develop or origin/main",
                true,
                dialog.text.all_selected(),
                theme,
            ))
            .children(dialog.error.clone().map(error_row))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(space::S))
                    .pt(px(space::S))
                    .child(
                        button("git-base-cancel", "Cancel", theme, false)
                            .tooltip(tooltip(theme, "Keep the current base", "Esc"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.close_git_base(window, cx);
                            })),
                    )
                    // Both look disabled while a save is in flight.
                    .child(if dialog.busy {
                        disabled_button("git-base-auto", "Auto-detect", theme, false)
                    } else {
                        button("git-base-auto", "Auto-detect", theme, false)
                            .tooltip(tooltip(
                                theme,
                                "Use origin/HEAD, else main, else master",
                                "",
                            ))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.set_git_base(true, window, cx);
                            }))
                    })
                    .child(if dialog.busy {
                        disabled_button("git-base-use", use_label, theme, true)
                    } else {
                        button("git-base-use", use_label, theme, true)
                            .tooltip(tooltip(theme, "Compare with the typed ref", "Enter"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.set_git_base(false, window, cx);
                            }))
                    }),
            );
        Self::modal_shade("git-base-shade", panel, cx)
    }

    /// "Broadcast to agents": a checkbox row per live agent session (with its last result),
    /// the text, then Cancel and Send (Enter).
    fn render_broadcast(
        &self,
        dialog: &Broadcast,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let hover = theme.surface_raised;
        let busy = dialog.busy();
        let rows = dialog.rows.iter().enumerate().map(|(index, row)| {
            let result = row.delivery.as_ref().map(|delivery| {
                let icon_color = match delivery {
                    BroadcastDelivery::Failed(_) => theme.status_exited,
                    _ => theme.text_muted,
                };
                div()
                    .id(("broadcast-result", index))
                    .flex_none()
                    .max_w(px(theme::BROADCAST_RESULT_WIDTH))
                    .flex()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(div().text_color(rgb(icon_color)).child(delivery.icon()))
                    .child(div().min_w_0().truncate().child(delivery.text()))
                    .tooltip(tooltip(theme, &delivery.text(), ""))
            });
            div()
                .id(("broadcast-row", index))
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::XS))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .border(px(theme::BORDER_WIDTH))
                .border_color(gpui::transparent_black())
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text))
                .when(index == dialog.cursor, |el| {
                    el.border_color(rgb(theme.accent))
                })
                .when(busy, |el| el.opacity(opacity::DISABLED))
                .when(!busy, |el| {
                    el.cursor_pointer()
                        .hover(move |style| style.bg(rgb(hover)))
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .tooltip(tooltip(theme, "Check or uncheck", "Tab"))
                })
                .on_click(cx.listener(move |shell, _, _, cx| {
                    if let Some(dialog) = shell.broadcast.as_mut() {
                        dialog.toggle(index);
                    }
                    cx.notify();
                }))
                .child(
                    div()
                        .size(px(space::M))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(radius::SMALL))
                        .border(px(theme::BORDER_WIDTH))
                        .border_color(rgb(theme.border))
                        .text_size(font_size::META)
                        .text_color(rgb(theme.accent))
                        .child(if row.checked { "✓" } else { "" }),
                )
                .child(div().flex_1().min_w_0().truncate().child(row.label.clone()))
                .children(result)
        });
        let send_label = if busy { "Sending…" } else { "Send" };
        let footer = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(space::S))
            .pt(px(space::S));
        let footer = if dialog.rows.is_empty() {
            footer
                .child(
                    button("broadcast-close", "Close", theme, false)
                        .tooltip(tooltip(theme, "Close", "Esc"))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_broadcast(window, cx);
                        })),
                )
                .child(
                    button("broadcast-terminals", "Show terminals", theme, true)
                        .tooltip(tooltip(theme, "Start an agent there, then broadcast", ""))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_broadcast(window, cx);
                            shell.show_view(View::Terminals, window, cx);
                        })),
                )
        } else {
            footer
                .child(
                    button("broadcast-cancel", "Cancel", theme, false)
                        .tooltip(tooltip(theme, "Close the dialog", "Esc"))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_broadcast(window, cx);
                        })),
                )
                .child(if dialog.can_send() {
                    button("broadcast-send", send_label, theme, true)
                        .tooltip(tooltip(theme, "Send to the checked sessions", "Enter"))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.send_broadcast(window, cx);
                        }))
                } else {
                    disabled_button("broadcast-send", send_label, theme, true).tooltip(tooltip(
                        theme,
                        "Type a message and check a session",
                        "",
                    ))
                })
        };
        let panel = div()
            .id("broadcast-dialog")
            .w(px(theme::DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::broadcast_key_down))
            .relative()
            .child(self.typing(&self.dialog_focus, Self::broadcast_key_down, cx))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Broadcast to agents"),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child(if dialog.rows.is_empty() {
                        "No live agent sessions in this workspace."
                    } else {
                        "Sends the text, then Enter, to every checked session. A working agent \
                         gets it when its turn ends."
                    }),
            )
            .when(!dialog.rows.is_empty(), |el| {
                el.child(
                    div()
                        .id("broadcast-sessions")
                        .max_h(px(theme::SEND_SESSIONS_HEIGHT))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children(rows),
                )
                .child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child("↑↓ move · Tab check · Enter send"),
                )
                .child(text_field(
                    "broadcast-text",
                    &dialog.text,
                    "e.g. run the tests",
                    true,
                    dialog.text.all_selected(),
                    theme,
                ))
            })
            .child(footer);
        Self::modal_shade("broadcast-shade", panel, cx)
    }

    /// A Docs dialog: its title, a sentence, its body and the buttons, in the shared modal
    /// frame (Esc closes it; "Send to agent" also takes ↑ / ↓ / Enter).
    fn doc_dialog_panel(
        &self,
        id: &'static str,
        width: f32,
        title: String,
        sentence: String,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .w(px(width))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::doc_dialog_key_down))
            .child(
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(title),
            )
            .child(
                div()
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .child(sentence),
            )
    }

    /// "Send to agent": the workspace's live agent sessions to pick from (the file's last
    /// modifying session first selected), then Cancel / Send.
    fn render_send_dialog(
        &self,
        dialog: &SendDialog,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let choices = dialog.choices(self.sidebar.sessions());
        let selected = dialog.selected(&choices);
        let selection = dialog.prefill.is_some();
        let count = dialog.comment_ids.len();
        let what = if selection {
            "the selection".to_owned()
        } else if count == 1 {
            "1 open comment".to_owned()
        } else {
            format!("{count} open comments")
        };
        let sentence = if choices.is_empty() {
            format!("No agent is running in this workspace to take {what}.")
        } else if selection {
            "Adds the selection to the composer of the session below; type your \
             instruction, then send."
                .to_owned()
        } else {
            format!(
                "Types {what} into the session below as one prompt (queued while it is \
                 working). It replies to each with mk comment reply."
            )
        };
        let hover = theme.surface_raised;
        let rows: Vec<gpui::AnyElement> = choices
            .iter()
            .enumerate()
            .map(|(index, choice)| {
                let session_id = choice.session_id;
                let picked = selected == Some(index);
                let last = dialog.last_session_id == Some(session_id);
                div()
                    .id(SharedString::from(format!("send-{session_id}")))
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .px(px(space::S))
                    .py(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .text_size(font_size::BODY)
                    // The pick has a focus-colored ring, so it stays visible under hover.
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(if picked {
                        theme.pane_focus_border
                    } else {
                        theme.surface
                    }))
                    .when(picked, |el| el.bg(rgb(theme.surface_raised)))
                    .when(!picked && !dialog.busy, |el| {
                        el.cursor_pointer()
                            .hover(move |style| style.bg(rgb(hover)))
                            .active(|style| style.opacity(opacity::ACTIVE))
                    })
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        if let Some(DocDialog::Send(dialog)) = shell.doc_dialog.as_mut() {
                            dialog.pick(session_id);
                            cx.notify();
                        }
                    }))
                    .child(status_mark(choice.state, now, theme))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(rgb(theme.text))
                            .when(picked, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                            .child(choice.label.clone()),
                    )
                    .when(last, |el| {
                        el.child(
                            div()
                                .flex_none()
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child(dialog.last_label()),
                        )
                    })
                    .child(
                        div()
                            .flex_none()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child(status::text(
                                choice.state,
                                None,
                                choice.state_changed_at,
                                now,
                            )),
                    )
                    .into_any_element()
            })
            .collect();
        let footer = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(space::S))
            .pt(px(space::S));
        let footer = if choices.is_empty() {
            footer
                .child(
                    button("send-close", "Close", theme, false)
                        .tooltip(tooltip(theme, "Close", "Esc"))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_doc_dialog(window, cx);
                        })),
                )
                .child(
                    button("send-terminals", "Show terminals", theme, true)
                        .tooltip(tooltip(theme, "Start an agent there, then send", ""))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_doc_dialog(window, cx);
                            shell.show_view(View::Terminals, window, cx);
                        })),
                )
        } else {
            footer
                .child(
                    button("send-cancel", "Cancel", theme, false)
                        .tooltip(tooltip(
                            theme,
                            if selection {
                                "Close without sending"
                            } else {
                                "Keep the comments open"
                            },
                            "Esc",
                        ))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_doc_dialog(window, cx);
                        })),
                )
                .child(if dialog.busy {
                    disabled_button("send-confirm", "Sending…", theme, true)
                } else if selection {
                    button("send-confirm", "Open composer", theme, true)
                        .tooltip(tooltip(
                            theme,
                            "Add the selection to this session's composer",
                            "Enter",
                        ))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.send_selection(window, cx);
                        }))
                } else {
                    button("send-confirm", "Send", theme, true)
                        .tooltip(tooltip(theme, "Send to the selected session", "Enter"))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.send_comments(window, cx);
                        }))
                })
        };
        let panel = self
            .doc_dialog_panel(
                "send-dialog",
                theme::DIALOG_WIDTH,
                match (selection, dialog.file_path.as_str()) {
                    (true, "") => "Send selection · terminal".to_owned(),
                    (true, path) => format!("Send selection · {path}"),
                    (false, path) => format!("Send to agent · {path}"),
                },
                sentence,
                theme,
                cx,
            )
            .when(!rows.is_empty(), |el| {
                el.child(
                    div()
                        .id("send-sessions")
                        .max_h(px(theme::SEND_SESSIONS_HEIGHT))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children(rows),
                )
            })
            .children(dialog.error.clone().map(|error| {
                div()
                    .flex()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                    .child(div().text_color(rgb(theme.text)).child(error))
            }))
            .child(footer);
        Self::modal_shade("send-dialog-shade", panel, cx)
    }

    /// Quick open: the query field over the best matches (matched chars in the accent color,
    /// the pick ringed); a click or Enter opens one in the Files view.
    fn render_quick_open(
        &self,
        open: &QuickOpen,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let workspace = self
            .sidebar
            .projects()
            .iter()
            .find(|project| project.id == open.project_id)
            .map_or_else(String::new, |project| format!(" · {}", project.name));
        let note = match &open.results {
            Results::Loading => Some("Searching…".to_owned()),
            Results::Loaded(matches) if matches.is_empty() && open.query.trim().is_empty() => {
                Some("This workspace has no files yet.".to_owned())
            }
            Results::Loaded(matches) if matches.is_empty() => {
                Some(format!("No file matches “{}”.", open.query.trim()))
            }
            Results::Loaded(_) => None,
            Results::Failed(error) => Some(format!("Cannot search the files: {error}")),
        };
        let accent = gpui::Hsla::from(rgb(theme.accent));
        let hover = theme.surface_raised;
        let rows: Vec<gpui::AnyElement> = open
            .matches()
            .iter()
            .enumerate()
            .map(|(index, found)| {
                let picked = index == open.selected;
                let highlights = quick_open::match_ranges(&found.path, &found.indices)
                    .into_iter()
                    .map(|range| {
                        let style = gpui::HighlightStyle {
                            color: Some(accent),
                            font_weight: Some(FontWeight(weight::SEMIBOLD)),
                            ..gpui::HighlightStyle::default()
                        };
                        (range, style)
                    });
                div()
                    .id(("quick-open-match", index))
                    .flex_none()
                    .px(px(space::S))
                    .py(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    // The pick has a focus-colored ring, so it stays visible under hover.
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(if picked {
                        theme.pane_focus_border
                    } else {
                        theme.surface
                    }))
                    .when(picked, |el| el.bg(rgb(theme.surface_raised)))
                    .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover))))
                    .cursor_pointer()
                    .active(|style| style.opacity(opacity::ACTIVE))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        if let Some(open) = shell.quick_open.as_mut() {
                            open.pick(index);
                            shell.open_quick_pick(window, cx);
                        }
                    }))
                    .child(gpui::StyledText::new(found.path.clone()).with_highlights(highlights))
                    .into_any_element()
            })
            .collect();
        let panel = div()
            .id("quick-open")
            .w(px(theme::DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::quick_open_key_down))
            .relative()
            .child(self.typing(&self.dialog_focus, Self::quick_open_key_down, cx))
            .child(
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(if open.composer.is_some() {
                        format!("Insert file{workspace}")
                    } else {
                        format!("Open file{workspace}")
                    }),
            )
            .child(text_field(
                "quick-open-query",
                &open.query,
                "Type part of a file name or path",
                true,
                open.query.all_selected(),
                theme,
            ))
            .when(!rows.is_empty(), |el| {
                el.child(
                    div()
                        .id("quick-open-matches")
                        .max_h(px(theme::QUICK_OPEN_LIST_HEIGHT))
                        .overflow_y_scroll()
                        .track_scroll(&self.quick_open_scroll)
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children(rows),
                )
            })
            .children(note.map(|note| {
                let failed = matches!(open.results, Results::Failed(_));
                div()
                    .flex()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .when(failed, |el| {
                        el.child(div().text_color(rgb(theme.status_exited)).child("✕"))
                    })
                    .child(
                        div()
                            .text_color(rgb(if failed { theme.text } else { theme.text_muted }))
                            .child(note),
                    )
            }))
            .child(
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(if open.composer.is_some() {
                        "↑ ↓ to pick · Enter types its path in the composer · Esc types @"
                    } else {
                        "↑ ↓ to pick · Enter opens it in Files · Esc closes"
                    }),
            );
        Self::modal_shade("quick-open-shade", panel, cx)
    }

    /// The command palette: the query field over the matching entries (matched chars in the
    /// accent color, shortcuts muted on the right, the pick ringed); a click or Enter runs one.
    fn render_palette(
        &self,
        palette: &Palette,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let accent = gpui::Hsla::from(rgb(theme.accent));
        let hover = theme.surface_raised;
        let rows: Vec<gpui::AnyElement> = palette
            .rows()
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                let picked = index == palette.selected;
                let highlights = row.ranges.into_iter().map(|range| {
                    let style = gpui::HighlightStyle {
                        color: Some(accent),
                        font_weight: Some(FontWeight(weight::SEMIBOLD)),
                        ..gpui::HighlightStyle::default()
                    };
                    (range, style)
                });
                div()
                    .id(("palette-entry", index))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .px(px(space::S))
                    .py(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    // The pick has a focus-colored ring, so it stays visible under hover.
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(if picked {
                        theme.pane_focus_border
                    } else {
                        theme.surface
                    }))
                    .when(picked, |el| el.bg(rgb(theme.surface_raised)))
                    .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover))))
                    .cursor_pointer()
                    .active(|style| style.opacity(opacity::ACTIVE))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        if let Some(palette) = shell.palette.as_mut() {
                            palette.pick(index);
                            shell.run_palette_pick(window, cx);
                        }
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(
                                gpui::StyledText::new(row.entry.title.clone())
                                    .with_highlights(highlights),
                            ),
                    )
                    .when(!row.entry.shortcut.is_empty(), |el| {
                        el.child(
                            div()
                                .flex_none()
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child(row.entry.shortcut.clone()),
                        )
                    })
                    .into_any_element()
            })
            .collect();
        let empty = rows.is_empty();
        let panel = div()
            .id("palette")
            .w(px(theme::DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::palette_key_down))
            .relative()
            .child(self.typing(&self.dialog_focus, Self::palette_key_down, cx))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Command palette"),
            )
            .child(text_field(
                "palette-query",
                palette.query(),
                "Type a command, workspace, session or task",
                true,
                palette.query_selected(),
                theme,
            ))
            .when(!empty, |el| {
                el.child(
                    div()
                        .id("palette-entries")
                        .max_h(px(theme::QUICK_OPEN_LIST_HEIGHT))
                        .overflow_y_scroll()
                        .track_scroll(&self.palette_scroll)
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children(rows),
                )
            })
            .when(empty, |el| {
                el.child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(format!("Nothing matches “{}”.", palette.query().trim())),
                )
            })
            .child(
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child("↑ ↓ to pick · Enter runs it · Esc closes"),
            );
        Self::modal_shade("palette-shade", panel, cx)
    }

    /// The prompt picker (§20B.3): the library's prompts (the pick ringed), the pick rendered
    /// with the known variables, a muted note naming the ones left empty; a click or Enter
    /// types it.
    fn render_prompt_picker(
        &self,
        picker: &PromptPicker,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let hover = theme.surface_raised;
        let rows: Vec<gpui::AnyElement> = picker
            .library
            .prompts
            .iter()
            .enumerate()
            .map(|(index, prompt)| {
                let picked = index == picker.selected;
                div()
                    .id(("prompt-entry", index))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .px(px(space::S))
                    .py(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    // The pick has a focus-colored ring, so it stays visible under hover.
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(if picked {
                        theme.pane_focus_border
                    } else {
                        theme.surface
                    }))
                    .when(picked, |el| el.bg(rgb(theme.surface_raised)))
                    .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover))))
                    .cursor_pointer()
                    .active(|style| style.opacity(opacity::ACTIVE))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        if let Some(picker) = shell.prompt_picker.as_mut() {
                            picker.pick(index);
                            shell.insert_prompt_pick(window, cx);
                        }
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(prompt.title.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child(prompt.name.clone()),
                    )
                    .into_any_element()
            })
            .collect();
        let preview = picker.preview();
        let note = preview
            .as_ref()
            .and_then(|rendered| prompt_picker::missing_note(&rendered.missing));
        let muted = |text: String| {
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let unreadable = picker.library.errors.len();
        let into = match picker.target {
            prompt_picker::Target::Composer(_) => {
                "↑ ↓ to pick · Enter types it in the composer · Esc closes"
            }
            prompt_picker::Target::TaskDialog => {
                "↑ ↓ to pick · Enter adds it to the task prompt · Esc closes"
            }
        };
        let panel = div()
            .id("prompt-picker")
            .w(px(theme::DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::prompt_picker_key_down))
            .child(
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child("Insert prompt"),
            )
            .when(rows.is_empty(), |el| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(space::M))
                        .child(div().flex_1().min_w_0().child(muted(
                            "No prompts yet: add Markdown files to the prompts folder.".to_owned(),
                        )))
                        .child(
                            button("prompt-open-dir", "Open prompts folder", theme, true)
                                .flex_none()
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    shell.open_prompts_dir(window, cx);
                                })),
                        ),
                )
            })
            .when(!rows.is_empty(), |el| {
                el.child(
                    div()
                        .id("prompt-entries")
                        .max_h(px(theme::QUICK_OPEN_LIST_HEIGHT))
                        .overflow_y_scroll()
                        .track_scroll(&self.prompt_picker_scroll)
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children(rows),
                )
            })
            .children(preview.map(|rendered| {
                // No border: the raised surface sets the preview apart (§14).
                div()
                    .id("prompt-preview")
                    .max_h(px(theme::PROMPT_FIELD_HEIGHT))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .px(px(space::S))
                    .py(px(space::S))
                    .rounded(px(radius::MEDIUM))
                    .bg(rgb(theme.surface_raised))
                    .text_size(font_size::BODY)
                    .text_color(rgb(theme.text))
                    .children(rendered.text.split('\n').map(|line| {
                        // An empty line still takes a line's height.
                        div().child(if line.is_empty() { " " } else { line }.to_owned())
                    }))
            }))
            .children(note.map(muted))
            .when(unreadable > 0, |el| {
                el.child(
                    div()
                        .flex()
                        .gap(px(space::XS))
                        .text_size(font_size::META)
                        .child(div().text_color(rgb(theme.status_needs_input)).child("⚠"))
                        .child(muted(format!(
                            "{unreadable} prompt file(s) could not be read (see the log)."
                        ))),
                )
            })
            .child(muted(into.to_owned()));
        Self::modal_shade("prompt-picker-shade", panel, cx)
    }

    /// "View changes": the sent thread's file as sent vs as shown now.
    fn render_changes(
        &self,
        changes: &Changes,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let session = changes.sent_to.map(|id| {
            self.sidebar
                .sessions()
                .iter()
                .find(|session| session.id == id)
                .map_or_else(|| "an agent".to_owned(), history::label)
        });
        let anchor = if changes.anchor.is_empty() {
            String::new()
        } else {
            format!(" ({})", changes.anchor.to_lowercase())
        };
        let sent = session.map_or_else(String::new, |session| format!(" to {session}"));
        let sentence = if changes.lines.is_empty() {
            format!("Nothing changed since the comment{anchor} was sent{sent}.")
        } else {
            format!("The file as it was when the comment{anchor} was sent{sent} (−) vs now (+).")
        };
        let panel = self
            .doc_dialog_panel(
                "changes-dialog",
                theme::UNDO_DIALOG_WIDTH,
                format!("Changes · {}", changes.file_path),
                sentence,
                theme,
                cx,
            )
            .when(!changes.lines.is_empty(), |el| {
                el.child(
                    div()
                        .id("changes-diff")
                        .max_h(px(theme::CHANGES_DIFF_HEIGHT))
                        .flex()
                        .flex_col()
                        .py(px(space::XS))
                        .rounded(px(radius::MEDIUM))
                        // `surface`, so hunk rows (`diff_hunk_header`) show as a band.
                        .bg(rgb(theme.surface))
                        .overflow_scroll()
                        .font_family(theme.terminal_font_family.clone())
                        .text_size(font_size::META)
                        .children(changes.lines.iter().map(|line| diff_row(line, theme))),
                )
            })
            .when(changes.hidden > 0, |el| {
                el.child(meta_line(
                    theme,
                    format!("{} more lines are left out.", changes.hidden).into(),
                ))
            })
            .child(
                div().flex().justify_end().pt(px(space::S)).child(
                    button("changes-close", "Close", theme, true)
                        .tooltip(tooltip(theme, "Close", "Esc"))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_doc_dialog(window, cx);
                        })),
                ),
            );
        Self::modal_shade("changes-dialog-shade", panel, cx)
    }

    /// The workspace's finished sessions (left) and the selected one's log, replayed
    /// read-only (right).
    fn render_history(
        &self,
        dialog: &HistoryDialog,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let muted = |text: String| {
            div()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let error_row = |error: String| {
            div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                .child(div().text_color(rgb(theme.text)).child(error))
        };
        let hover = theme.surface_raised;
        let list: Vec<gpui::AnyElement> = match &dialog.listing {
            Listing::Loading => vec![muted("Loading the sessions…".into()).into_any_element()],
            Listing::Failed(error) => {
                vec![error_row(format!("Cannot list the sessions: {error}")).into_any_element()]
            }
            Listing::Loaded(sessions) if sessions.is_empty() => {
                vec![muted("No finished sessions in this workspace yet.".into()).into_any_element()]
            }
            Listing::Loaded(sessions) => sessions
                .iter()
                .map(|session| {
                    let id = session.id;
                    let selected = dialog.selected == Some(id);
                    div()
                        .id(SharedString::from(format!("history-{id}")))
                        .flex()
                        .flex_col()
                        .px(px(space::S))
                        .py(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .cursor_pointer()
                        .when(selected, |el| el.bg(rgb(theme.surface_raised)))
                        .when(!selected, |el| {
                            el.hover(move |style| style.bg(rgb(hover)))
                                .active(|style| style.opacity(opacity::ACTIVE))
                        })
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.select_history(id, window, cx);
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(space::XS))
                                .text_size(font_size::BODY)
                                .child(
                                    div()
                                        .flex_none()
                                        .text_color(rgb(if history::failed(session) {
                                            sidebar::status_color(theme, SessionState::Exited)
                                        } else {
                                            theme.text_muted
                                        }))
                                        .child(status::icon(SessionState::Exited)),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .whitespace_nowrap()
                                        .text_color(rgb(theme.text))
                                        .when(selected, |el| {
                                            el.font_weight(FontWeight(weight::MEDIUM))
                                        })
                                        .child(history::label(session)),
                                ),
                        )
                        .child(
                            div()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child(history::details(session, now)),
                        )
                        .into_any_element()
                })
                .collect(),
        };
        let log: Option<gpui::AnyElement> = match (&dialog.log, dialog.selected_session()) {
            (_, None) => None,
            (Log::Loading, Some(_)) => Some(muted("Loading the log…".into()).into_any_element()),
            (Log::Empty, Some(_)) => Some(muted("No output was logged.".into()).into_any_element()),
            (Log::Failed(error), Some(_)) => Some(error_row(error.clone()).into_any_element()),
            (Log::Shown { cut }, Some(session)) => Some(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap(px(space::S))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .rounded(px(radius::MEDIUM))
                            .overflow_hidden()
                            .children(self.history_pane.clone()),
                    )
                    .when(*cut, |el| {
                        el.child(
                            div()
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child(format!(
                                    "Older output is left out; mk session history {} prints \
                                     all of it.",
                                    session.id
                                )),
                        )
                    })
                    .into_any_element(),
            ),
        };
        let panel = div()
            .id("history-dialog")
            .w(relative(theme::HISTORY_DIALOG_FRACTION))
            .h(relative(theme::HISTORY_DIALOG_FRACTION))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::history_key_down))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_size(font_size::CARD_TITLE)
                            .font_weight(FontWeight(weight::SEMIBOLD))
                            .text_color(rgb(theme.text))
                            .child(format!("History · {}", dialog.workspace)),
                    )
                    .child(
                        button("history-close", "Close", theme, false)
                            .tooltip(tooltip(theme, "Close", "Esc"))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.close_history(window, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap(px(space::M))
                    .child(
                        div()
                            .id("history-list")
                            .flex_none()
                            .w(px(theme::HISTORY_LIST_WIDTH))
                            .flex()
                            .flex_col()
                            .gap(px(space::XS))
                            .overflow_y_scroll()
                            .children(list),
                    )
                    .child(div().flex_1().min_w_0().flex().flex_col().children(log)),
            );
        Self::modal_shade("history-shade", panel, cx)
    }

    /// The shade behind a modal dialog: it blocks the window behind it (a stray click there
    /// never closes the dialog; Esc or Cancel does) and keeps the keyboard in the dialog.
    fn modal_shade(
        id: &'static str,
        panel: impl IntoElement,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        div()
            .id(id)
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(cx.theme().scrim))
            .occlude()
            // The keyboard stays in the dialog.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|shell, _, window, cx| {
                    cx.stop_propagation();
                    shell.focus_pane(window, cx);
                }),
            )
            .child(panel)
            .into_any_element()
    }

    /// The session panel (SPEC §10): the focused pane's cards, right of Terminals. Rendered
    /// only while open; it is a sibling of the panes, never inside the terminal element.
    fn render_inspector(
        &self,
        project_id: Ulid,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let session = self
            .focused_session()
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));
        let grip = div()
            .id("inspector-grip")
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .w(px(theme::INSPECTOR_GRIP_WIDTH))
            .cursor(CursorStyle::ResizeLeftRight)
            .on_drag(InspectorGrip, |_, _, _, cx| cx.new(|_| EmptyView))
            .on_drag_move(
                cx.listener(|shell, event: &DragMoveEvent<InspectorGrip>, window, cx| {
                    cx.set_active_drag_cursor_style(CursorStyle::ResizeLeftRight, window);
                    // ponytail: live width, one `session.resize` per changed column count;
                    // throttle if an agent TUI flickers while dragging.
                    shell.inspector_width = inspector::width_from_pointer(
                        f32::from(window.viewport_size().width),
                        f32::from(event.event.position.x),
                    );
                    shell.inspector_dragging = true;
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|shell, _, _, _| shell.end_inspector_drag()),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|shell, _, _, _| shell.end_inspector_drag()),
            );
        let cards = div()
            .id("inspector-body")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::M))
            .children(match session {
                Some(session) => vec![self.render_session_card(session, now, theme, cx)],
                None => vec![meta_line(theme, "No session focused".into()).into_any_element()],
            })
            // Follows the workspace, not the focused pane.
            .children(
                self.render_changes_card(project_id, theme, cx)
                    .map(IntoElement::into_any_element),
            )
            .children(
                session
                    .filter(|s| s.kind == SessionKind::Agent && s.state != SessionState::Exited)
                    .map(|s| self.render_plan_card(s, theme, cx)),
            );
        div()
            .flex_none()
            .w(px(self.inspector_width))
            .h_full()
            .relative()
            .border_l(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .bg(rgb(theme.bg))
            .font_family(theme.ui_font_family.clone())
            .text_size(font_size::BODY)
            .text_color(rgb(theme.text))
            .child(cards)
            .child(grip)
            .into_any_element()
    }

    /// One line per repository with changes (`M 23 ?3` + Review diff); `None` when the
    /// workspace has no repository (the card is hidden).
    fn render_changes_card(
        &self,
        project_id: Ulid,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Div> {
        let workspace = self
            .sidebar
            .projects()
            .iter()
            .find(|p| p.id == project_id)
            .map_or("workspace", |p| p.name.as_str());
        let lines = inspector::changes_card(self.git_summary(), workspace)?;
        let card = inspector_card(theme, "Changes");
        if lines.is_empty() {
            return Some(card.child(meta_line(theme, "Working tree clean".into())));
        }
        Some(
            card.children(lines.into_iter().enumerate().map(|(i, line)| {
                let repo = line.repo.clone();
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(div().flex_1().min_w_0().truncate().child(line.label))
                    .children(
                        line.modified.map(|text| {
                            div().flex_none().text_color(rgb(theme.accent)).child(text)
                        }),
                    )
                    .children(line.untracked.map(|text| {
                        div()
                            .flex_none()
                            .text_color(rgb(theme.diff_add_text))
                            .child(text)
                    }))
                    .child(
                        row_action(
                            SharedString::from(format!("inspector-review-{i}")),
                            "Review diff",
                            theme,
                        )
                        .flex_none()
                        .tooltip(tooltip(theme, "Open the Git view on this repository", ""))
                        .on_click(cx.listener(
                            move |shell, _, window, cx| {
                                shell.review_diff(project_id, repo.clone(), window, cx);
                            },
                        )),
                    )
            })),
        )
    }

    /// The right panel's Plan card for an agent session (ADR 0039): the linked file's
    /// progress and checklist, or a button to link one.
    fn render_plan_card(
        &self,
        session: &Session,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let id = session.id;
        let linked = session.plan_path.is_some();
        let card = self.plans.get(id).map(plan::card);
        let title = match (&card, &session.plan_path) {
            (Some(card), _) => format!("Plan {}", card.name),
            (None, Some(path)) => format!("Plan {}", plan::name_of(path)),
            (None, None) => "Plan".to_owned(),
        };
        let header = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(font_size::BODY)
                    .font_weight(FontWeight(weight::MEDIUM))
                    .text_color(rgb(theme.text))
                    .child(title),
            )
            .children(
                card.as_ref()
                    .filter(|card| card.error.is_none())
                    .map(|card| meta_line(theme, card.count.clone().into()).flex_none()),
            );
        let body = match &card {
            Some(card) => match &card.error {
                Some(error) => div().child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.diff_del_text))
                        .child(error.clone()),
                ),
                None => div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .child(progress_bar(card.fraction, theme::PLAN_BAR_HEIGHT, theme))
                    .children(card.rows.iter().map(|row| plan_row(row, theme)))
                    .children(
                        (card.more > 0)
                            .then(|| meta_line(theme, format!("{} more", card.more).into())),
                    ),
            },
            None if linked => div().child(meta_line(theme, "Reading the plan…".into())),
            None => div().child(meta_line(
                theme,
                "Link a Markdown checklist to follow this session's progress.".into(),
            )),
        };
        let actions = if linked {
            div()
                .flex()
                .gap(px(space::S))
                .child(
                    quiet_button("plan-change", "Change…", theme, theme.hover).on_click(
                        cx.listener(move |shell, _, window, cx| shell.pick_plan(id, window, cx)),
                    ),
                )
                .child(
                    quiet_button("plan-unlink", "Unlink", theme, theme.hover).on_click(
                        cx.listener(move |shell, _, window, cx| {
                            shell.link_plan(id, None, window, cx);
                        }),
                    ),
                )
        } else {
            div().flex().child(
                button("plan-link", "Link plan…", theme, true).on_click(
                    cx.listener(move |shell, _, window, cx| shell.pick_plan(id, window, cx)),
                ),
            )
        };
        inspector_card(theme, "Plan")
            .child(header)
            .child(body)
            .child(actions)
            .children(
                self.plan_error
                    .as_ref()
                    .filter(|(sid, _)| *sid == id)
                    .map(|(_, error)| {
                        div()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.diff_del_text))
                            .child(error.clone())
                    }),
            )
            .into_any_element()
    }

    /// Agent, title, state + time, usage, Open transcript and Stop of `session`.
    fn render_session_card(
        &self,
        session: &Session,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let card = inspector::session_card(session, self.usages.get(session.id), now);
        let id = session.id;
        let open = match &card.transcript {
            Some(_) => button("inspector-transcript", "Open transcript", theme, false)
                .tooltip(tooltip(
                    theme,
                    "Open the agent's transcript file in Zed",
                    "",
                ))
                .on_click(cx.listener(move |shell, _, _, cx| shell.open_transcript(id, cx)))
                .into_any_element(),
            None => disabled_button("inspector-transcript", "Open transcript", theme, false)
                .tooltip(tooltip(theme, "No transcript yet", ""))
                .into_any_element(),
        };
        let stop = match inspector::stop_action(Some(session)) {
            inspector::StopAction::Nothing => None,
            action => Some(action),
        }
        .map(|action| {
            let hint = if action == inspector::StopAction::Ask {
                "Stop this session (asks first)"
            } else {
                "Stop this shell"
            };
            danger_button("inspector-stop", "Stop", theme)
                .tooltip(tooltip(theme, hint, ""))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    shell.stop_session(id, window, cx);
                }))
        });
        inspector_card(theme, "Session")
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(agent_chip(self.agent_badge(&session.agent_profile), theme))
                    .child(div().min_w_0().truncate().child(card.agent.clone()))
                    .child(div().flex_1())
                    .child(status_mark(session.state, now, theme)),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight(weight::MEDIUM))
                    .child(card.title.clone()),
            )
            .child(meta_line(theme, card.state.clone().into()))
            .children(card.usage.iter().map(|(label, value)| {
                div()
                    .flex()
                    .gap(px(space::S))
                    .text_size(font_size::META)
                    .child(
                        div()
                            .w(theme::rems_px(theme::INSPECTOR_LABEL_WIDTH))
                            .flex_none()
                            .text_color(rgb(theme.text_muted))
                            .child(*label),
                    )
                    .child(div().min_w_0().truncate().child(value.clone()))
            }))
            .child(div().flex().gap(px(space::S)).child(open).children(stop))
            .into_any_element()
    }

    /// "Open transcript": the agent's `.jsonl` in Zed. A failure (Zed missing, file gone) shows
    /// in the sidebar like the other Zed actions.
    fn open_transcript(&mut self, session: Ulid, cx: &mut Context<'_, Self>) {
        let Some(path) = self
            .usages
            .get(session)
            .and_then(|usage| usage.transcript.clone())
        else {
            return;
        };
        match zed::open(Path::new(&path), None) {
            Ok(()) => self.sidebar_error = None,
            Err(error) => {
                tracing::warn!(%session, path, "open transcript failed: {error}");
                self.sidebar_error = Some(error.to_string());
            }
        }
        cx.notify();
    }

    /// The card's Stop: `session.kill`. A running agent asks first; the pane stays until the
    /// session exits (ADR 0033).
    fn stop_session(&mut self, session: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let action =
            inspector::stop_action(self.sidebar.sessions().iter().find(|s| s.id == session));
        match action {
            inspector::StopAction::Nothing => {}
            inspector::StopAction::Kill => self.kill(session, window, cx),
            inspector::StopAction::Ask => {
                let answer = window.prompt(
                    PromptLevel::Warning,
                    "This session is still running.",
                    Some("Stops the agent. Its pane closes when it exits."),
                    &inspector::STOP_ANSWERS,
                    cx,
                );
                cx.spawn_in(window, async move |this, cx| {
                    if !task_dialog::confirms_delete(answer.await.ok()) {
                        return;
                    }
                    let _ = this.update_in(cx, |shell, window, cx| {
                        // It may have exited while the dialog was open.
                        if !shell.is_exited(session) {
                            shell.kill(session, window, cx);
                        }
                    });
                })
                .detach();
            }
        }
    }

    fn render_tab_bar(
        &self,
        project_id: Ulid,
        tabs: &Tabs,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let active = tabs.active().map(|tab| tab.id);
        let hover = theme.hover;
        div()
            .flex_none()
            .h(theme::rems_px(theme::TAB_STRIP_HEIGHT))
            .flex()
            .items_center()
            .gap(px(space::XS))
            .px(px(space::S))
            .bg(rgb(theme.bg))
            .border_b(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .children(tabs.tabs().iter().map(|tab| {
                let tab_id = tab.id;
                let is_active = active == Some(tab_id);
                if let Some(renaming) = self
                    .renaming
                    .as_ref()
                    .filter(|r| r.target == RenameTarget::Tab(tab_id))
                {
                    return self.render_rename(&renaming.draft, theme, cx);
                }
                // The most urgent state among the tab's panes (SPEC §11).
                let urgent = status::most_urgent(self.sidebar.sessions(), &tab.sessions());
                let group = SharedString::from(format!("tab-{tab_id}"));
                let icon = match tab_kind(tab, self.sidebar.sessions()) {
                    TabKind::Agent(profile) => Some(agent_chip(self.agent_badge(&profile), theme)),
                    TabKind::Shell => Some(agent_chip(">_".to_owned(), theme)),
                    TabKind::Empty => None,
                };
                div()
                    .id(group.clone())
                    .group(group.clone())
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .pl(px(space::S))
                    .pr(px(space::XS))
                    .py(px(space::XS))
                    .rounded(px(radius::MEDIUM))
                    // Same box as the rename field, so the bar does not shift while renaming.
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(gpui::transparent_black())
                    .cursor_pointer()
                    .whitespace_nowrap()
                    .text_size(font_size::BODY)
                    .when(is_active, |el| {
                        el.bg(rgb(theme.surface_raised))
                            .border_color(rgb(theme.border))
                            .font_weight(FontWeight(weight::MEDIUM))
                            .text_color(rgb(theme.text))
                    })
                    .when(!is_active, |el| {
                        el.text_color(rgb(theme.text_muted))
                            .hover(move |style| style.bg(rgb(hover)))
                            .active(|style| style.opacity(opacity::ACTIVE))
                    })
                    .on_click(
                        cx.listener(move |shell, event: &gpui::ClickEvent, window, cx| {
                            if let Some(tabs) = shell.active_tabs_mut() {
                                tabs.activate(tab_id);
                            }
                            shell.sync_panes(window, cx);
                            if event.click_count() == 2 {
                                shell.start_rename(window, cx);
                            }
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                            shell.open_tab_menu(tab_id, event.position, window, cx);
                        }),
                    )
                    .children(icon)
                    .when_some(urgent, |el, session| {
                        el.tooltip(status_tooltip(
                            theme,
                            session.state,
                            session.state_source,
                            session.state_changed_at,
                        ))
                        .child(status_mark(session.state, now, theme))
                    })
                    .when(tab.pinned, |el| el.child(pin_mark()))
                    .child(tab.title.clone())
                    .child(
                        row_action(
                            SharedString::from(format!("tab-close-{tab_id}")),
                            "✕",
                            theme,
                        )
                        // Always on the active tab, on hover for the others.
                        .when(!is_active, |el| {
                            el.opacity(opacity::HIDDEN)
                                .group_hover(group, |style| style.opacity(opacity::FULL))
                        })
                        .tooltip(tooltip(
                            theme,
                            "Close tab",
                            if is_active {
                                self.keymap.label(Command::CloseTab)
                            } else {
                                String::new()
                            },
                        ))
                        .on_click(cx.listener(
                            move |shell, _, window, cx| {
                                // Never activate the tab being closed.
                                cx.stop_propagation();
                                shell.close_tab(tab_id, window, cx);
                            },
                        )),
                    )
                    .into_any_element()
            }))
            // Pushes the notices, Close pane and the + / split / ⋯ group to the right.
            .child(div().flex_1())
            .children(
                self.tab_error
                    .clone()
                    .map(|error| notice(theme.text, format!("✕ {error}"))),
            )
            .when(
                tabs.active().and_then(|tab| tab.zoomed()).is_some(),
                |bar| {
                    bar.child(notice(theme.text_muted, "⤢ Pane zoomed".to_owned()))
                        .child(
                            button("unzoom", "Show all panes", theme, false)
                                .tooltip(tooltip(
                                    theme,
                                    "Show all panes",
                                    self.keymap.label(Command::ZoomPane),
                                ))
                                .on_click(cx.listener(|shell, _, window, cx| {
                                    shell.run(&Run(Command::ZoomPane), window, cx);
                                })),
                        )
                },
            )
            .children(
                lifecycle::running_warning(self.sidebar.sessions(), project_id)
                    .map(|warning| notice(theme.text_muted, format!("⚠ {warning}"))),
            )
            .when(self.focused_session().is_some(), |bar| {
                bar.child(
                    button("close-pane", "Close pane", theme, false)
                        .tooltip(tooltip(
                            theme,
                            "Close the focused pane",
                            self.keymap.label(Command::ClosePane),
                        ))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.close_focused_pane(window, cx);
                        })),
                )
            })
            .child(
                button("new-tab", "+", theme, false)
                    .tooltip(tooltip(theme, "New tab", ""))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.create_tab(project_id, None, window, cx);
                    })),
            )
            .child(
                button("tab-split", "◫", theme, false)
                    .tooltip(tooltip(
                        theme,
                        "Split right",
                        self.keymap.label(Command::SplitRight),
                    ))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.run(&Run(Command::SplitRight), window, cx);
                    })),
            )
            .when_some(active, |bar, tab_id| {
                bar.child(
                    button("tab-menu-button", "⋯", theme, false)
                        .tooltip(tooltip(theme, "Tab menu", ""))
                        .on_click(cx.listener(
                            move |shell, event: &gpui::ClickEvent, window, cx| {
                                shell.open_tab_menu(tab_id, event.position(), window, cx);
                            },
                        )),
                )
            })
    }

    /// The title being renamed inline: the draft with a caret, in the active tab's place.
    fn render_rename(
        &self,
        draft: &Input,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let selected = draft.all_selected();
        div()
            .id("tab-rename")
            .flex()
            .items_center()
            .min_w(px(theme::TAB_RENAME_MIN_WIDTH))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(radius::SMALL))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.pane_focus_border))
            .bg(rgb(theme.surface_raised))
            .whitespace_nowrap()
            .text_size(font_size::BODY)
            .font_weight(FontWeight(weight::MEDIUM))
            .text_color(rgb(theme.text))
            .cursor(CursorStyle::IBeam)
            .track_focus(&self.rename_focus)
            .on_key_down(cx.listener(Self::rename_key_down))
            .relative()
            .child(self.typing(&self.rename_focus, Self::rename_key_down, cx))
            .child(
                div()
                    .when(selected, |el| el.bg(rgb(theme.terminal_selection_bg)))
                    .child(draft.as_str().to_owned()),
            )
            .when(!selected, |el| {
                el.child(
                    div()
                        .flex_none()
                        .w(px(theme::BORDER_WIDTH))
                        .h(font_size::BODY)
                        .bg(rgb(theme.text)),
                )
            })
            .into_any_element()
    }

    /// The active tab's split tree: each pane placed at its fraction of the area.
    /// `composer_focused`: the focused pane's composer has the keyboard.
    fn render_tab(
        &self,
        tabs: &Tabs,
        now: i64,
        composer_focused: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        if tabs.active().and_then(|tab| tab.layout()).is_none() {
            return empty_state(
                theme,
                Some(">_"),
                Some("No panes in this tab"),
                "Start an agent or a shell, or pick a session from the sidebar.".into(),
                None,
            )
            .flex_1()
            .child(
                div()
                    .flex()
                    .gap(px(space::S))
                    .child(
                        button("empty-new-agent", "New agent", theme, true)
                            .child(kbd(self.keymap.label(Command::SplitAgent), theme, true))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.new_pane(SessionKind::Agent, Axis::Vertical, window, cx);
                            })),
                    )
                    .child(
                        secondary_button("empty-new-shell", "New shell", theme)
                            .child(kbd(self.keymap.label(Command::SplitShell), theme, false))
                            .tooltip(tooltip(theme, "New shell pane in the workspace folder", ""))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.new_pane(SessionKind::Shell, Axis::Vertical, window, cx);
                            })),
                    ),
            );
        }
        let focused = tabs.active().and_then(|tab| tab.focused());
        let rects = tabs.active().map(Tab::rects).unwrap_or_default();
        let several = rects.len() > 1;
        let half_gap = px(theme::PANE_HALF_GAP);
        div()
            .flex_1()
            .min_h_0()
            .p(half_gap)
            .child(div().size_full().relative().children(rects.into_iter().map(
                |(session, rect)| {
                    let is_focused = focused == Some(session);
                    div()
                        .absolute()
                        .left(relative(rect.x as f32))
                        .top(relative(rect.y as f32))
                        .w(relative(rect.w as f32))
                        .h(relative(rect.h as f32))
                        .p(half_gap)
                        .child(self.pane_frame(
                            session,
                            several,
                            is_focused,
                            is_focused && composer_focused,
                            now,
                            theme,
                            cx,
                        ))
                },
            )))
    }

    /// One pane: the terminal (or why it is not shown), its composer, its border, and the
    /// unfocused dim.
    #[allow(clippy::too_many_arguments)]
    fn pane_frame(
        &self,
        session: Ulid,
        several: bool,
        is_focused: bool,
        composer_focused: bool,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let content = match self.panes.get(&session) {
            Some(pane) => pane.clone().into_any_element(),
            None => {
                let (sentence, detail) = self.pane_placeholder(session);
                message(theme, None, sentence, detail)
                    .size_full()
                    .into_any_element()
            }
        };
        let info = self.sidebar.sessions().iter().find(|s| s.id == session);
        let border = status::pane_border(theme, info.map(|s| s.state), several, is_focused);
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _, window, cx| {
                    if let Some(tabs) = shell.active_tabs_mut() {
                        tabs.focus(session);
                    }
                    shell.focus_pane(window, cx);
                    cx.notify();
                }),
            )
            .on_drag_move::<ExternalPaths>(cx.listener(
                move |shell, event: &DragMoveEvent<ExternalPaths>, _, cx| {
                    if event.bounds.contains(&event.event.position) {
                        shell.drop_hover = Some((session, event.drag(cx).clone()));
                    } else if shell
                        .drop_hover
                        .as_ref()
                        .is_some_and(|(hovered, _)| *hovered == session)
                    {
                        shell.drop_hover = None;
                    }
                },
            ))
            .child(drop_catcher(session, cx))
            .children(info.map(|info| {
                let paths = self.conflicts.for_session(session);
                let conflict = (!paths.is_empty()).then(|| {
                    conflict_badge(
                        format!("pane-{session}"),
                        &paths,
                        theme,
                        move |shell| shell.conflicts.dismiss_session(session),
                        cx,
                    )
                });
                let queued = self.composers.queue(session).len();
                // Keyed by session: several panes share the nearest keyed parent.
                let following = self.follow.is_on(session);
                let turns = (info.kind == SessionKind::Agent).then(|| {
                    let follow_tip = if following {
                        "Following this agent's edits in Zed: click to stop"
                    } else {
                        "Open each file this agent edits in Zed"
                    };
                    div()
                        .id(SharedString::from(format!("pane-turns-{session}")))
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(space::S))
                        .child(
                            button_frame(
                                "pane-follow",
                                if following {
                                    "Following in Zed"
                                } else {
                                    "Follow in Zed"
                                },
                            )
                            .px(px(space::XS))
                            .py_0()
                            .text_size(font_size::META)
                            .active(|style| style.opacity(opacity::ACTIVE))
                            .when(following, |el| {
                                el.bg(rgb(theme.surface_raised))
                                    .font_weight(FontWeight(weight::MEDIUM))
                                    .text_color(rgb(theme.text))
                            })
                            .when(!following, |el| {
                                let hover = theme.surface_raised;
                                el.text_color(rgb(theme.text_muted))
                                    .hover(move |style| style.bg(rgb(hover)))
                            })
                            .tooltip(tooltip(theme, follow_tip, ""))
                            .on_click(cx.listener(
                                move |shell, _, _, cx| {
                                    shell.toggle_follow(session, cx);
                                },
                            )),
                        )
                        .child(
                            quiet_button("pane-turns", "Turns…", theme, theme.surface_raised)
                                .px(px(space::XS))
                                .py_0()
                                .text_size(font_size::META)
                                .tooltip(tooltip(
                                    theme,
                                    "The agent's turns: restore the files to before one",
                                    "",
                                ))
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    shell.open_turns(session, window, cx);
                                })),
                        )
                });
                let renaming = self
                    .renaming
                    .as_ref()
                    .filter(|r| r.target == RenameTarget::Session(session));
                let title = match renaming {
                    // Mouse downs stay in the field: the pane's handler would focus the
                    // terminal, and the blur would commit half-typed text.
                    Some(renaming) => div()
                        .flex_1()
                        .min_w_0()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|_, _, _, cx| cx.stop_propagation()),
                        )
                        .child(self.render_rename(&renaming.draft, theme, cx))
                        .into_any_element(),
                    None => {
                        let renamable = mkrew_core::run::command_name(info).is_none();
                        div()
                            .id(SharedString::from(format!("pane-title-{session}")))
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(rgb(theme.text_muted))
                            .children(run::pane_title(info, self.ports.get(info.id)))
                            // Also the empty space after the agent name of an untitled pane.
                            .when(renamable, |el| {
                                el.on_click(cx.listener(
                                    move |shell, event: &gpui::ClickEvent, window, cx| {
                                        if event.click_count() == 2 {
                                            shell.start_pane_rename(session, window, cx);
                                        }
                                    },
                                ))
                            })
                            .into_any_element()
                    }
                };
                let workspace = (self.sidebar.projects().iter())
                    .find(|project| project.id == info.project_id)
                    .map(|project| project.name.as_str());
                let folder = pane_text::folder(info.task_id.and_then(|id| self.board.task(id)));
                let breadcrumb =
                    pane_text::breadcrumb(workspace, folder.as_deref(), &info.agent_profile);
                let pill = pane_text::pill_text(
                    info.state,
                    info.exit_code,
                    info.state_changed_at,
                    now,
                    self.turns.get(session),
                );
                let color = info
                    .color
                    .as_deref()
                    .and_then(|name| theme.workspace_color(name));
                pane_header(
                    info,
                    self.agent_badge(&info.agent_profile),
                    breadcrumb,
                    pill,
                    title,
                    self.usages.get(session).and_then(usage::context_share),
                    conflict,
                    turns,
                    queued,
                    color,
                    now,
                    theme,
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                        shell.open_pane_menu(session, event.position, window, cx);
                    }),
                )
            }))
            .child(div().flex_1().min_h_0().child(content))
            .children(self.composers.shown(session).map(|draft| {
                self.composer_panel(session, draft, is_focused, composer_focused, theme, cx)
            }))
            .when(several && !is_focused, |el| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(rgb(theme.bg))
                        .opacity(theme.unfocused_pane_dim),
                )
            })
            // Drawn above the terminal so its corners stay rounded.
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(px(radius::MEDIUM))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(border)),
            )
    }

    /// The composer docked under `session`'s pane (§20B.1): the draft, then why the last send
    /// failed or the keys. Only the focused pane's composer takes the keyboard (`focused`: it
    /// has it now); a click on another pane's composer focuses that pane and its composer.
    fn composer_panel(
        &self,
        session: Ulid,
        draft: &Draft,
        is_focused: bool,
        focused: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let field = text_field(
            "composer",
            draft.text.text(),
            "Message the agent…",
            focused && !draft.busy && draft.enhancing.is_none(),
            draft.text.all_selected(),
            theme,
        )
        .max_h(px(theme::COMPOSER_MAX_HEIGHT))
        .overflow_y_scroll()
        .when(draft.busy || draft.enhancing.is_some(), |el| {
            el.opacity(opacity::DISABLED)
        })
        .when(is_focused, |el| {
            el.track_focus(&self.composer_focus)
                .track_scroll(&self.composer_scroll)
                .on_key_down(cx.listener(Self::composer_key_down))
                .relative()
                .child(self.typing(&self.composer_focus, Self::composer_key_down, cx))
        });
        // The keys only while it has the keyboard (they do nothing on other panes).
        let status = if draft.busy {
            Some(("↑", theme.text_muted, "Sending…".to_owned()))
        } else if draft.enhancing.is_some() {
            Some(("✦", theme.text_muted, "Improving the prompt…".to_owned()))
        } else if let Some(error) = &draft.error {
            Some(("✕", theme.status_exited, error.clone()))
        } else {
            focused.then(|| {
                (
                    "",
                    theme.text_muted,
                    "Ctrl+Enter to send · @ for a file · Esc to hide".to_owned(),
                )
            })
        };
        // The texts waiting for the agent (§20B.2), first line each, ✕ removes one.
        let hover = theme.border;
        let queued = self
            .composers
            .queue(session)
            .iter()
            .map(|item| {
                let send_id = item.send_id;
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .child(
                        div()
                            .flex_none()
                            .text_color(rgb(theme.text_muted))
                            .child("⧗"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(rgb(theme.text))
                            .child(composer::first_line(&item.text).to_owned()),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("unqueue-{send_id}")))
                            .flex_none()
                            .px(px(space::S))
                            .rounded(px(radius::SMALL))
                            .cursor_pointer()
                            .text_color(rgb(theme.text_muted))
                            .hover(move |style| style.bg(rgb(hover)))
                            .active(|style| style.opacity(opacity::ACTIVE))
                            .tooltip(tooltip(theme, "Remove from the queue (not sent)", ""))
                            .on_click(cx.listener(move |shell, _, _, cx| {
                                cx.stop_propagation();
                                shell.unqueue(session, send_id, cx);
                            }))
                            .child("✕"),
                    )
            })
            .collect::<Vec<_>>();
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(space::XS))
            .p(px(space::S))
            .bg(rgb(theme.surface))
            .font_family(theme.ui_font_family.clone())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, _, window, cx| {
                    cx.stop_propagation();
                    if let Some(tabs) = shell.active_tabs_mut() {
                        tabs.focus(session);
                    }
                    window.focus(&shell.composer_focus, cx);
                    cx.notify();
                }),
            )
            .when(!queued.is_empty(), |el| {
                el.child(
                    div()
                        .id("composer-queue")
                        .flex_none()
                        .max_h(px(theme::COMPOSER_QUEUE_MAX_HEIGHT))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children(queued),
                )
            })
            .child(field)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .children(status.map(|(icon, color, text)| {
                                div()
                                    .flex()
                                    .gap(px(space::XS))
                                    .text_size(font_size::META)
                                    .when(!icon.is_empty(), |el| {
                                        el.child(div().text_color(rgb(color)).child(icon))
                                    })
                                    .child(
                                        div()
                                            .text_color(rgb(if draft.error.is_some() {
                                                theme.text
                                            } else {
                                                theme.text_muted
                                            }))
                                            .child(text),
                                    )
                            })),
                    )
                    // ✦ (§20B.1): the agent rewrites the draft.
                    .when(is_focused && !draft.busy && !draft.text.is_blank(), |el| {
                        let improve = if draft.enhancing.is_some() {
                            disabled_button("composer-enhance", "✦", theme, false)
                        } else {
                            quiet_button("composer-enhance", "✦", theme, theme.surface_raised)
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    shell.enhance_composer(session, window, cx)
                                }))
                        };
                        el.child(improve.flex_none().tooltip(tooltip(
                            theme,
                            "Improve this prompt with the agent (it may read the code)",
                            "",
                        )))
                    })
                    // The prompt library (§20B.3), typed into this draft.
                    .when(is_focused && !draft.busy, |el| {
                        el.child(
                            quiet_button(
                                "composer-prompts",
                                "Prompts…",
                                theme,
                                theme.surface_raised,
                            )
                            .flex_none()
                            .tooltip(tooltip(theme, "Insert a prompt from the library", ""))
                            .on_click(cx.listener(
                                move |shell, _, window, cx| {
                                    shell.open_prompt_picker(
                                        prompt_picker::Target::Composer(session),
                                        window,
                                        cx,
                                    );
                                },
                            )),
                        )
                    }),
            )
    }

    /// Why a pane on screen has no terminal yet.
    fn pane_placeholder(&self, session: Ulid) -> (SharedString, Option<SharedString>) {
        // An exit explains a failed attach better than its error does.
        if let Some(s) = self
            .sidebar
            .sessions()
            .iter()
            .find(|s| s.id == session && s.state == SessionState::Exited)
        {
            let text = match s.exit_code {
                Some(code) => format!("✕ Session exited (code {code})."),
                None => "✕ Session exited.".to_owned(),
            };
            return (text.into(), None);
        }
        match self.attach_errors.get(&session) {
            Some(error) => (
                "✕ Cannot attach the session.".into(),
                Some(error.clone().into()),
            ),
            None => ("Attaching…".into(), None),
        }
    }
}

/// A centered message: optional title, one sentence, optional smaller detail (an
/// [`empty_state`] without a glyph).
fn message(
    theme: &Theme,
    title: Option<&'static str>,
    sentence: SharedString,
    detail: Option<SharedString>,
) -> gpui::Div {
    empty_state(theme, None, title, sentence, detail)
}

/// A calm empty state (ADR 0027): an optional glyph tile, an optional title, one muted
/// sentence and an optional smaller detail, centered in a narrow column.
fn empty_state(
    theme: &Theme,
    glyph: Option<&'static str>,
    title: Option<&'static str>,
    sentence: SharedString,
    detail: Option<SharedString>,
) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(space::S))
        .p(px(space::XL))
        .font_family(theme.ui_font_family.clone())
        .when_some(glyph, |el, glyph| {
            el.child(
                div()
                    .size(theme::rems_px(theme::EMPTY_STATE_TILE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(radius::MEDIUM))
                    .bg(rgb(theme.surface))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(theme.border))
                    .text_color(rgb(theme.text_muted))
                    .child(glyph),
            )
        })
        .children(title.map(|title| {
            div()
                .text_size(font_size::CARD_TITLE)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text))
                .child(title)
        }))
        .child(
            div()
                .max_w(px(theme::EMPTY_STATE_MAX_WIDTH))
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text_muted))
                .child(sentence),
        )
        .children(detail.map(|detail| {
            div()
                .max_w(px(theme::READABLE_WIDTH))
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(detail)
        }))
}

/// A one-line notice in the tab bar, shortened to fit; `color` sets errors apart from warnings.
fn notice(color: u32, text: String) -> gpui::Div {
    div()
        .min_w_0()
        .overflow_hidden()
        .text_ellipsis()
        .whitespace_nowrap()
        .text_size(font_size::META)
        .text_color(rgb(color))
        .child(text)
}

/// A quiet button starting a new agent or shell pane; `keys` = its shortcut label.
fn new_pane_button(
    id: &'static str,
    label: &'static str,
    kind: SessionKind,
    keys: String,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Stateful<gpui::Div> {
    let tip = match kind {
        SessionKind::Agent => "New pane with the workspace's default agent",
        SessionKind::Shell => "New pane with the workspace's default shell",
    };
    button(id, label, theme, false)
        .tooltip(tooltip(theme, tip, keys))
        .on_click(cx.listener(move |shell, _, window, cx| {
            // The title bar shows it in every view; the new pane is seen in Terminals.
            shell.show_view(View::Terminals, window, cx);
            shell.new_pane(kind, Axis::Vertical, window, cx);
        }))
}

/// The repositories of `project_id` and each one's Working-mode status. Asking `git.repos`
/// first means a root without `.git` never sends the status that would fail. The status
/// requests go out together; a repository whose status fails is left out (logged).
async fn git_summary_of(connection: &Connection, project_id: Ulid) -> anyhow::Result<GitSummary> {
    let names = connection
        .call::<Vec<String>>(Request::GitRepos(ProjectRef { project_id }))
        .await?;
    let pending: Vec<_> = names
        .into_iter()
        .map(|name| {
            let params = GitStatusParams {
                project_id,
                worktree_task_id: None,
                repo: (!name.is_empty()).then(|| name.clone()),
                mode: GitMode::Working,
                since: None,
            };
            let status = connection.call::<GitStatus>(Request::GitStatus(params));
            (name, status)
        })
        .collect();
    let mut repos = Vec::new();
    for (name, status) in pending {
        match status.await {
            Ok(status) => repos.push(RepoSummary::of(&name, &status)),
            Err(error) => tracing::warn!(%project_id, repo = name, "git.status failed: {error:#}"),
        }
    }
    Ok(GitSummary { repos })
}

/// Fire-and-forget `session.detach`; a failure is logged.
fn detach(connection: &Connection, session_id: Ulid, cx: &mut Context<'_, Shell>) {
    let reply = connection.request(Request::SessionDetach(SessionRef { session_id }));
    cx.background_executor()
        .spawn(async move {
            match reply.await {
                Ok(Outcome::Result(_)) => {}
                Ok(Outcome::Error(body)) => {
                    tracing::warn!(%session_id, "session.detach failed: {}", body.message);
                }
                Err(error) => tracing::warn!(%session_id, "session.detach failed: {error:#}"),
            }
        })
        .detach();
}

/// The shared frame of [`button`], [`quiet_button`] and [`danger_button`]: no colors, no hover
/// (gpui allows one `hover` per element).
fn button_frame(id: &'static str, label: impl Into<SharedString>) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .text_size(font_size::BODY)
        .child(label.into())
}

/// A small text button: primary (accent, `on_accent` text) or ghost (muted, hover bg).
fn button(
    id: &'static str,
    label: impl Into<SharedString>,
    theme: &Theme,
    primary: bool,
) -> gpui::Stateful<gpui::Div> {
    if primary {
        button_frame(id, label)
            .bg(rgb(theme.accent))
            .text_color(rgb(theme.on_accent))
            .font_weight(FontWeight(weight::MEDIUM))
            .hover(|style| style.opacity(opacity::HOVER))
            .active(|style| style.opacity(opacity::ACTIVE))
    } else {
        quiet_button(id, label, theme, theme.hover)
    }
}

/// The idle Run button: a green fill (`Start`'s color) with `on_accent` text, like a primary
/// [`button`] in green.
fn run_fill_button(
    id: &'static str,
    label: impl Into<SharedString>,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    button_frame(id, label)
        .bg(rgb(run::action_colors(RunAction::Start, theme).fg))
        .text_color(rgb(theme.on_accent))
        .font_weight(FontWeight(weight::MEDIUM))
        .hover(|style| style.opacity(opacity::HOVER))
        .active(|style| style.opacity(opacity::ACTIVE))
}

/// A ghost glyph button of a live Run: the glyph in the action's color, its tint on hover.
fn run_icon_button(
    id: &'static str,
    label: &'static str,
    action: RunAction,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let colors = run::action_colors(action, theme);
    button_frame(id, label)
        .text_color(rgb(colors.fg))
        .hover(move |style| style.bg(rgb(colors.tint)))
        .active(|style| style.opacity(opacity::ACTIVE))
}

/// A secondary button: surface background with a faint border (the empty state's New shell).
fn secondary_button(
    id: &'static str,
    label: impl Into<SharedString>,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let hover = theme.hover;
    button_frame(id, label)
        .bg(rgb(theme.surface))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.border))
        .text_color(rgb(theme.text))
        .hover(move |style| style.bg(rgb(hover)))
        .active(|style| style.opacity(opacity::ACTIVE))
}

/// A quiet [`button`] whose hover shows `hover` (e.g. one step above a raised card).
fn quiet_button(
    id: &'static str,
    label: impl Into<SharedString>,
    theme: &Theme,
    hover: u32,
) -> gpui::Stateful<gpui::Div> {
    button_frame(id, label)
        .text_color(rgb(theme.text_muted))
        .hover(move |style| style.bg(rgb(hover)))
        .active(|style| style.opacity(opacity::ACTIVE))
}

/// A [`button`] that cannot be used right now: faded, no hover, no click.
fn disabled_button(
    id: &'static str,
    label: &'static str,
    theme: &Theme,
    primary: bool,
) -> gpui::Stateful<gpui::Div> {
    let base = div()
        .id(id)
        .flex()
        .items_center()
        .px(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::SMALL))
        .opacity(opacity::DISABLED)
        .text_size(font_size::BODY)
        .child(label);
    if primary {
        base.bg(rgb(theme.accent)).text_color(rgb(theme.on_accent))
    } else {
        base.text_color(rgb(theme.text_muted))
    }
}

/// A [`button`] that discards work (Undo's confirm): drawn in the diff "deleted" colors, so
/// it never reads as the accent's safe default.
fn danger_button(
    id: &'static str,
    label: impl Into<SharedString>,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let text = theme.text;
    button_frame(id, label)
        .bg(rgb(theme.diff_del_bg))
        .text_color(rgb(theme.diff_del_text))
        .font_weight(FontWeight(weight::MEDIUM))
        // Stronger than an opacity change on the faint background.
        .hover(move |style| style.text_color(rgb(text)))
        .active(|style| style.opacity(opacity::ACTIVE))
}

/// A keyboard shortcut chip (`Keymap::label` text). On an accent button it is drawn in
/// `on_accent` at reduced opacity.
fn kbd(text: String, theme: &Theme, on_accent: bool) -> gpui::Div {
    let base = div()
        .flex_none()
        .px(px(space::XS))
        .rounded(px(radius::SMALL))
        .text_size(font_size::META);
    if on_accent {
        base.text_color(rgb(theme.on_accent))
            .opacity(opacity::SECONDARY)
            .child(text)
    } else {
        base.bg(rgb(theme.surface_raised))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .text_color(rgb(theme.text_muted))
            .child(text)
    }
}

/// Covers a group of title bar controls while a dialog is open: it swallows the mouse, so the
/// controls neither click nor hover.
fn inert_layer() -> gpui::Div {
    div().absolute().inset_0().occlude()
}

/// One ─ □ ✕ button of the Windows title bar. `area` makes Windows treat it as the system
/// caption button (hover shows here; the click and the Snap Layouts flyout are the system's).
fn window_control(
    id: &'static str,
    glyph: &'static str,
    area: WindowControlArea,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let close = matches!(area, WindowControlArea::Close);
    let (hover, on_hover) = (theme.hover, theme.on_accent);
    let red = theme.status_exited;
    div()
        .id(id)
        .window_control_area(area)
        .flex_none()
        .w(theme::rems_px(theme::WINDOW_CONTROL_WIDTH))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(font_size::BODY)
        .text_color(rgb(theme.text_muted))
        .hover(move |style| {
            if close {
                style.bg(rgb(red)).text_color(rgb(on_hover))
            } else {
                style.bg(rgb(hover))
            }
        })
        .child(glyph)
}

/// A square ghost button holding one glyph (theme toggle, sidebar +).
fn icon_button(
    id: impl Into<gpui::ElementId>,
    glyph: &'static str,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let hover = theme.hover;
    div()
        .id(id)
        .flex_none()
        .size(theme::rems_px(theme::ICON_BUTTON_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .text_color(rgb(theme.text_muted))
        .hover(move |style| style.bg(rgb(hover)))
        .active(|style| style.opacity(opacity::ACTIVE))
        .child(glyph)
}

/// Delivers an OS file drop on `session`'s pane (see `Shell::drop_hover`).
// ponytail: works around gpui-pre 0.3.7 keeping keyboard modality during OLE drags
// (window.rs:5415 skips FileDrop), which makes every hover-gated mouse-up listener (`on_drop`,
// `capture_any_mouse_up`) miss drops after typing. Use plain `.on_drop` on the pane when
// upstream treats FileDrop as mouse input.
fn drop_catcher(session: Ulid, cx: &Context<'_, Shell>) -> impl IntoElement + use<> {
    let view = cx.entity();
    canvas(
        |_, _, _| (),
        move |_, (), window, _| {
            let view = view.clone();
            window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                // A plain click has no active drag: stale paths are never typed.
                if phase != DispatchPhase::Capture || !cx.has_active_drag() {
                    return;
                }
                view.update(cx, |shell, cx| {
                    let hovered = shell.drop_hover.take_if(|(hovered, _)| *hovered == session);
                    if let Some((_, paths)) = hovered {
                        cx.stop_propagation();
                        shell.drop_paths(session, &paths, window, cx);
                    }
                });
            });
        },
    )
    .absolute()
    .size_full()
}

/// What a key does to a one-line field of the Files find bar.
enum FieldEdit {
    /// Backspace, Ctrl+Backspace or Ctrl+A.
    Key(FindKey),
    Insert(String),
}

impl FieldEdit {
    fn apply(self, input: &mut Input) {
        match self {
            FieldEdit::Key(key) => {
                key.edit(input);
            }
            FieldEdit::Insert(text) => {
                input.insert(&text);
            }
        }
    }
}

/// The edit `keystroke` makes to a find / replace field: Ctrl+V (a clipboard without text
/// pastes nothing; line breaks become spaces), Backspace, Ctrl+Backspace, Ctrl+A, or typed
/// text.
fn field_edit(keystroke: &gpui::Keystroke, cx: &App) -> Option<FieldEdit> {
    match find_key(keystroke)? {
        FindKey::Paste => {
            let text = cx.read_from_clipboard().and_then(|item| item.text());
            Some(FieldEdit::Insert(
                text.unwrap_or_default().replace(['\r', '\n'], " "),
            ))
        }
        FindKey::Insert(text) => Some(FieldEdit::Insert(text)),
        key @ (FindKey::Backspace | FindKey::DeleteWord | FindKey::SelectAll) => {
            Some(FieldEdit::Key(key))
        }
        FindKey::Close | FindKey::Older | FindKey::Newer => None,
    }
}

/// Platform text input (IME, Unikey) for the field focused by `focus`, fed to `on_key` (see
/// [`ime::Typing`]). An invisible layer over the field: only the focused field registers.
fn typing_input(
    focus: &FocusHandle,
    on_key: ime::OnKey<Shell>,
    marked: &Rc<ime::Composing<FocusHandle>>,
    cx: &Context<'_, Shell>,
) -> impl IntoElement + use<> {
    let (focus, view, marked) = (focus.clone(), cx.entity(), marked.clone());
    canvas(
        |_, _, _| (),
        move |bounds, (), window, cx| {
            let typing = ime::Typing {
                view,
                on_key,
                marked,
                focus: focus.clone(),
                caret: bounds,
            };
            window.handle_input(&focus, typing, cx);
        },
    )
    .absolute()
    .size_full()
}

/// A text input (task dialog, follow-up, composer): its lines with a caret at the end while
/// `focused`, the lines highlighted while `selected` (Ctrl+A), or a muted placeholder when
/// empty. Callers make clicks focus it.
fn text_field(
    id: &'static str,
    text: &str,
    placeholder: &'static str,
    focused: bool,
    selected: bool,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let caret = || {
        div()
            .flex_none()
            .w(px(theme::BORDER_WIDTH))
            .h(font_size::BODY)
            .bg(rgb(theme.text))
    };
    let lines: Vec<&str> = text.split('\n').collect();
    let last = lines.len() - 1;
    div()
        .id(id)
        .flex()
        .flex_col()
        .px(px(space::S))
        .py(px(space::S))
        .rounded(px(radius::MEDIUM))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(if focused {
            theme.pane_focus_border
        } else {
            theme.border
        }))
        .bg(rgb(theme.surface_raised))
        .text_size(font_size::BODY)
        .text_color(rgb(theme.text))
        .cursor(CursorStyle::IBeam)
        .when(text.is_empty(), |el| {
            el.child(
                div()
                    .flex()
                    .items_center()
                    .when(focused, |row| row.child(caret()))
                    .child(div().text_color(rgb(theme.text_muted)).child(placeholder)),
            )
        })
        .when(!text.is_empty(), |el| {
            el.children(lines.into_iter().enumerate().map(|(i, line)| {
                // An empty line still takes a line's height.
                let line = if line.is_empty() { " " } else { line };
                div()
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .min_w_0()
                            .when(selected, |el| el.bg(rgb(theme.terminal_selection_bg)))
                            .child(line.to_owned()),
                    )
                    .when(focused && !selected && i == last, |row| row.child(caret()))
            }))
        })
}

/// The preview's page: a scrolling, centered column of `blocks` (ADR 0021), shared by Docs
/// and the Files view's `.md` files.
fn preview_page(id: &'static str, blocks: Vec<gpui::AnyElement>) -> gpui::AnyElement {
    div()
        .id(id)
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .px(px(space::XL))
        .py(px(space::L))
        .child(
            div()
                .w_full()
                .max_w(px(theme::DOC_PREVIEW_MAX_WIDTH))
                .mx_auto()
                .flex()
                .flex_col()
                .gap(px(space::M))
                .text_size(px(theme::doc::BODY))
                .line_height(relative(theme::doc::LINE_HEIGHT))
                .children(blocks),
        )
        .into_any_element()
}

/// The preview's top-level blocks, each picked by a click (Shift extends) to comment on its
/// lines; picked ones are highlighted.
fn preview_blocks(
    project_id: Ulid,
    shown: &Shown,
    selection: Option<Selection>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> Vec<gpui::AnyElement> {
    let hover = theme.surface_raised;
    shown
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| {
            let element = preview::block(block, shown, theme)?;
            let (start, end) = (block.lines.start, block.lines.end);
            let picked = selection.is_some_and(|s| s.contains(start, end));
            let wrapped = div()
                .id(("doc-block", index))
                .px(px(space::XS))
                .rounded(px(radius::SMALL))
                .cursor_pointer()
                .when(picked, |el| el.bg(rgb(theme.terminal_selection_bg)))
                .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover))))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |shell, event: &gpui::MouseDownEvent, _, cx| {
                        let extend = event.modifiers.shift;
                        shell.pick_lines(project_id, CommentContext::Doc, start, end, extend, cx);
                    }),
                )
                .child(element);
            Some(wrapped.into_any_element())
        })
        .collect()
}

/// Whether the Docs comment margin shows next to `doc`: with a shown file, and while it is
/// re-read after a change (so an open draft keeps its field).
fn margin_shown(doc: &Doc) -> bool {
    matches!(doc, Doc::Shown(_) | Doc::Loading)
}

/// One row of a unified diff (undo preview, Docs "View changes").
fn diff_row(line: &undo_dialog::DiffLine, theme: &Theme) -> gpui::Div {
    let (bg, fg) = match line.kind {
        LineKind::Added => (Some(theme.diff_add_bg), theme.diff_add_text),
        LineKind::Removed => (Some(theme.diff_del_bg), theme.diff_del_text),
        LineKind::Hunk => (Some(theme.diff_hunk_header), theme.text_muted),
        LineKind::Header => (None, theme.text_muted),
        LineKind::Context => (None, theme.text),
    };
    // An empty line still takes a line's height.
    let text = if line.text.is_empty() {
        " ".to_owned()
    } else {
        line.text.clone()
    };
    div()
        .px(px(space::S))
        .whitespace_nowrap()
        .text_color(rgb(fg))
        .when_some(bg, |el, bg| el.bg(rgb(bg)))
        .child(text)
}

/// The text of an `ai.generated`, or why there is none.
fn ai_result(done: &AiGeneratedEvent) -> Result<String, String> {
    match (&done.text, &done.error) {
        (Some(text), _) => Ok(text.clone()),
        (None, error) => Err(error.clone().unwrap_or_else(|| "no answer".to_owned())),
    }
}

/// Payload of a drag of the session panel's left edge (a resize, nothing is dropped).
struct InspectorGrip;

/// A card of the session panel: a muted title over its content (a block on `card`, like a board
/// card). The Changes and Plan cards use it too.
fn inspector_card(theme: &Theme, title: &'static str) -> gpui::Div {
    div()
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(space::S))
        .p(px(space::M))
        .rounded(px(radius::MEDIUM))
        .bg(rgb(theme.card))
        .child(
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(title),
        )
}

/// A muted one-line note (e.g. "Loading comments…").
fn meta_line(theme: &Theme, text: SharedString) -> gpui::Div {
    div()
        .text_size(font_size::META)
        .text_color(rgb(theme.text_muted))
        .child(text)
}

/// A file of the Docs list: its name, its folder below, and its open-comment count.
/// What a Files tree row needs to know about the view.
struct FileRowState<'a> {
    /// The tree cursor's path.
    cursor: Option<&'a str>,
    /// The opened file's path.
    opened: Option<&'a str>,
    /// The tree has the keyboard (the cursor shows a focus ring).
    focused: bool,
}

/// One Files tree row: indented by depth, a chevron on folders, the name (muted when
/// ignored), and on the right the git marker letter (files) or a dot (folders with changes).
/// Every row is one body line high (the tree is a uniform list).
fn file_row(
    project_id: Ulid,
    row: &files_view::Row,
    state: &FileRowState<'_>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let (entry, depth, expanded) = match row {
        files_view::Row::Note { depth, text } => {
            return div()
                .w_full()
                .pl(px(space::S + theme::FILE_TREE_INDENT * *depth as f32))
                .py(px(space::XS))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text_muted))
                .child(text.clone())
                .into_any_element();
        }
        files_view::Row::Entry {
            entry,
            depth,
            expanded,
        } => (entry, *depth, *expanded),
    };
    let at_cursor = state.cursor == Some(entry.path.as_str());
    let opened = state.opened == Some(entry.path.as_str());
    let hover = theme.surface_raised;
    let fill = if at_cursor || opened {
        theme.surface_raised
    } else {
        theme.bg
    };
    let chevron = match (entry.is_dir, expanded) {
        (false, _) => "",
        (true, false) => "▸",
        (true, true) => "▾",
    };
    let marker = entry.marker.map(|marker| match marker {
        GitMarker::Modified => ("M", theme.accent, "Modified"),
        GitMarker::Added => ("A", theme.diff_add_text, "Added"),
        GitMarker::Deleted => ("D", theme.diff_del_text, "Deleted"),
        GitMarker::Untracked => ("?", theme.diff_add_text, "Untracked"),
    });
    let path = entry.path.clone();
    let (menu_path, is_dir) = (entry.path.clone(), entry.is_dir);
    div()
        .id(SharedString::from(format!("file-{}", entry.path)))
        .w_full()
        .flex()
        .items_center()
        .gap(px(space::XS))
        .pl(px(space::S + theme::FILE_TREE_INDENT * depth as f32))
        .pr(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::SMALL))
        .border(px(theme::BORDER_WIDTH))
        // The border is always there (so rows never shift); it only shows as the focus ring.
        .border_color(rgb(if at_cursor && state.focused {
            theme.pane_focus_border
        } else {
            fill
        }))
        .cursor_pointer()
        .text_size(font_size::BODY)
        .when(at_cursor || opened, |el| el.bg(rgb(fill)))
        .when(!at_cursor && !opened, |el| {
            el.hover(move |style| style.bg(rgb(hover)))
        })
        .active(|style| style.opacity(opacity::ACTIVE))
        .child(
            div()
                .flex_none()
                .w(px(space::M))
                .text_color(rgb(theme.text_muted))
                .child(chevron),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_color(rgb(if entry.ignored {
                    theme.text_muted
                } else {
                    theme.text
                }))
                .when(opened, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                .child(entry.name.clone()),
        )
        .children(marker.map(|(letter, color, what)| {
            div()
                .id(SharedString::from(format!("file-marker-{}", entry.path)))
                .flex_none()
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(color))
                .tooltip(tooltip(theme, what, ""))
                .child(letter)
        }))
        .when(entry.is_dir && entry.has_changes, |el| {
            el.child(
                div()
                    .id(SharedString::from(format!("file-changes-{}", entry.path)))
                    .flex_none()
                    .size(px(theme::FILE_CHANGES_DOT_SIZE))
                    .rounded_full()
                    .bg(rgb(theme.accent))
                    .tooltip(tooltip(theme, "Contains changes", "")),
            )
        })
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.click_file(project_id, &path, window, cx);
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |shell, event: &gpui::MouseDownEvent, _, cx| {
                shell.open_file_menu(project_id, menu_path.clone(), is_dir, event.position, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
}

/// "+3 −1" in the diff colors.
fn change_counts(added: u32, removed: u32, theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .flex()
        .gap(px(space::XS))
        .text_size(font_size::META)
        .child(
            div()
                .text_color(rgb(theme.diff_add_text))
                .child(format!("+{added}")),
        )
        .child(
            div()
                .text_color(rgb(theme.diff_del_text))
                .child(format!("−{removed}")),
        )
}

/// One row of the Git changed-file list: a group title with its count, a file (change letter,
/// name, folder muted, +/− counts; indented under its History commit), a History commit
/// ([`git_commit_row`], ages at `now`) or a note under it. Every row is one body line high
/// (uniform list); the selected file is raised, with a focus ring while the list has the
/// keyboard.
fn git_file_row(
    project_id: Ulid,
    row: &git_view::ListRow,
    selected: Option<&git_view::Target>,
    focused: bool,
    now: i64,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let (target, file) = match row {
        git_view::ListRow::Header(group, count) => {
            // The same box as a file row (uniform list: rows are sized from the first one).
            return div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(space::XS))
                .px(px(space::S))
                .py(px(space::XS))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.bg))
                .text_size(font_size::BODY)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_muted))
                .child(group.title())
                .child(count.to_string())
                .into_any_element();
        }
        git_view::ListRow::Note(note) => {
            // Indented under its commit, like the commit's files.
            return div()
                .w_full()
                .flex()
                .items_center()
                .px(px(space::S))
                .pl(px(space::S + space::M))
                .py(px(space::XS))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.bg))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text_muted))
                .child(note.clone())
                .into_any_element();
        }
        git_view::ListRow::File(target, file) => (target.clone(), file),
        git_view::ListRow::Commit { commit, expanded } => {
            // Raised while none of its files is selected (they are listing).
            let picked = *expanded && selected.is_none();
            return git_commit_row(
                project_id, commit, *expanded, picked, focused, now, theme, cx,
            );
        }
    };
    let group = target.group;
    let picked = selected == Some(&target);
    let (letter, color, what) = match file.kind {
        ChangeKind::Added => ("A", theme.diff_add_text, "Added"),
        ChangeKind::Modified => ("M", theme.accent, "Modified"),
        ChangeKind::Deleted => ("D", theme.diff_del_text, "Deleted"),
        ChangeKind::Renamed => ("R", theme.accent, "Renamed"),
        ChangeKind::Copied => ("C", theme.accent, "Copied"),
        ChangeKind::TypeChanged => ("T", theme.accent, "File type changed"),
        ChangeKind::Untracked => ("?", theme.diff_add_text, "Untracked"),
        ChangeKind::Conflicted => ("!", theme.diff_del_text, "Conflicted"),
    };
    let (dir, name) = match file.path.rsplit_once('/') {
        Some((dir, name)) => (Some(dir.to_owned()), name.to_owned()),
        None => (None, file.path.clone()),
    };
    let fill = if picked {
        theme.surface_raised
    } else {
        theme.bg
    };
    let hover = theme.surface_raised;
    let id = format!("git-file-{group:?}-{}", file.path);
    let id = SharedString::from(id);
    div()
        .id(id.clone())
        .group(id.clone())
        .w_full()
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .py(px(space::XS))
        // Indented under its commit inside the full-width row (a margin would overflow it).
        .when(group == git_view::Group::Commit, |el| {
            el.pl(px(space::S + space::M))
        })
        .rounded(px(radius::SMALL))
        .border(px(theme::BORDER_WIDTH))
        // The border is always there (so rows never shift); it only shows as the focus ring.
        .border_color(rgb(if picked && focused {
            theme.pane_focus_border
        } else {
            fill
        }))
        .cursor_pointer()
        .text_size(font_size::BODY)
        .when(picked, |el| el.bg(rgb(fill)))
        .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover))))
        .active(|style| style.opacity(opacity::ACTIVE))
        .child(
            div()
                .id(SharedString::from(format!("{id}-kind")))
                .flex_none()
                .w(px(space::M))
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(color))
                .tooltip(tooltip(theme, what, ""))
                .child(letter),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .gap(px(space::XS))
                .overflow_hidden()
                .whitespace_nowrap()
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(theme.text))
                        .when(picked, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                        .child(name),
                )
                .children(dir.map(|dir| {
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(dir)
                })),
        )
        .children(
            git_view::counts(file).map(|(added, removed)| change_counts(added, removed, theme)),
        )
        // Shown on hover and on the selected file (SPEC §14 progressive disclosure).
        .children(git_view::Stage::of(group).map(|stage| {
            let target = target.clone();
            row_action(
                SharedString::from(format!("{id}-stage")),
                stage.label(),
                theme,
            )
            .when(!picked, |el| {
                el.opacity(opacity::HIDDEN)
                    .group_hover(id.clone(), |style| style.opacity(opacity::FULL))
            })
            .tooltip(tooltip(
                theme,
                match stage {
                    git_view::Stage::Stage => "Stage this file",
                    git_view::Stage::Unstage => "Unstage this file",
                },
                "",
            ))
            .on_click(cx.listener(move |shell, _, window, cx| {
                // Not a click on the row (which would select it).
                cx.stop_propagation();
                window.focus(&shell.git_focus, cx);
                if let Some(change) = shell.git_view(project_id).file_change(&target) {
                    shell.stage_git(project_id, change, window, cx);
                }
            }))
        }))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.select_git_file(project_id, target.clone(), window, cx);
        }))
        .into_any_element()
}

/// One History commit row: a chevron (open on the expanded commit), the summary, then its
/// short hash and age muted; the full message and author in the tooltip. Raised (with the
/// focus ring while the list has the keyboard) while it is `picked`.
#[allow(clippy::too_many_arguments)]
fn git_commit_row(
    project_id: Ulid,
    commit: &Commit,
    expanded: bool,
    picked: bool,
    focused: bool,
    now: i64,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let fill = if picked {
        theme.surface_raised
    } else {
        theme.bg
    };
    let hover = theme.surface_raised;
    let short = git_history::short_hash(&commit.hash);
    let meta = format!("{short} · {}", git_history::age(commit.authored_at, now));
    let message = if commit.body.is_empty() {
        commit.summary.clone()
    } else {
        format!("{}\n\n{}", commit.summary, commit.body.trim_end())
    };
    let tip = format!(
        "{message}\n\n{} <{}> · {short}",
        commit.author_name, commit.author_email
    );
    let hash = commit.hash.clone();
    div()
        .id(SharedString::from(format!("git-commit-{}", commit.hash)))
        .w_full()
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::SMALL))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(if picked && focused {
            theme.pane_focus_border
        } else {
            fill
        }))
        .cursor_pointer()
        .text_size(font_size::BODY)
        .when(picked, |el| el.bg(rgb(fill)))
        .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover))))
        .active(|style| style.opacity(opacity::ACTIVE))
        .tooltip(tooltip(theme, &tip, ""))
        .child(
            div()
                .flex_none()
                .w(px(space::M))
                .text_color(rgb(theme.text_muted))
                .child(if expanded { "▾" } else { "▸" }),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_color(rgb(theme.text))
                .when(expanded, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                .child(commit.summary.clone()),
        )
        .child(
            div()
                .flex_none()
                .whitespace_nowrap()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(meta),
        )
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.select_git_commit(project_id, hash.clone(), window, cx);
        }))
        .into_any_element()
}

/// A small text action inside a list row ("Stage" on a file row, "Stage hunk" on a hunk
/// header): no vertical padding, so the row keeps its height.
fn row_action(id: SharedString, label: &'static str, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    // One step above the rows' own `surface_raised` hover / selection fill.
    let hover = theme.border;
    div()
        .id(id)
        .flex_none()
        .px(px(space::XS))
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .font_family(theme.ui_font_family.clone())
        .text_size(font_size::META)
        .text_color(rgb(theme.text_muted))
        .hover(move |style| style.bg(rgb(hover)))
        .active(|style| style.opacity(opacity::ACTIVE))
        .child(label)
}

/// "Stage hunk" / "Unstage hunk" at the right end of a hunk header.
fn hunk_stage_button(
    project_id: Ulid,
    hunk: usize,
    stage: git_view::Stage,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Stateful<gpui::Div> {
    let (label, tip) = match stage {
        git_view::Stage::Stage => ("Stage hunk", "Stage this hunk"),
        git_view::Stage::Unstage => ("Unstage hunk", "Unstage this hunk"),
    };
    row_action(
        SharedString::from(format!("git-hunk-stage-{hunk}")),
        label,
        theme,
    )
    .ml_auto()
    // No key: `s` / `u` act on the current hunk (n / p), not the hovered one.
    .tooltip(tooltip(theme, tip, ""))
    .on_click(cx.listener(move |shell, _, window, cx| {
        window.focus(&shell.git_focus, cx);
        if let Some(change) = shell.git_view(project_id).hunk_change(hunk) {
            shell.stage_git(project_id, change, window, cx);
        }
    }))
}

/// One row of the Git diff viewer, `line_height` high: a collapsed gap of unchanged lines, a
/// hunk header (the current hunk in the accent), or a line with its old / new numbers, its
/// `+` / `−` sign, the diff colors and syntax colors. Lines whose new-side number is `picked`
/// (for a comment) get an accent bar and an accent number.
fn git_diff_row(
    shown: &git_view::Shown,
    index: usize,
    current: Option<usize>,
    picked: Option<Selection>,
    line_height: f32,
    theme: &Theme,
) -> gpui::Div {
    // Every row has the bar's width, so the gutters line up.
    let frame = div()
        .w_full()
        .h(px(line_height))
        .flex()
        .items_center()
        .gap(px(space::M))
        .px(px(space::L))
        .border_l(px(theme::DIFF_PICK_BAR_WIDTH))
        .border_color(gpui::transparent_black())
        .whitespace_nowrap();
    let Some(row) = shown.rows.get(index) else {
        return frame;
    };
    let width = shown.number_width;
    let number = |n: Option<u32>| match n {
        Some(n) => format!("{n:>width$}"),
        None => " ".repeat(width),
    };
    match row {
        git_view::DiffRow::Gap(count) => {
            let lines = if *count == 1 { "line" } else { "lines" };
            frame
                .text_color(rgb(theme.text_muted))
                .child(format!("⋯ {count} unchanged {lines}"))
        }
        git_view::DiffRow::Hunk(index) => {
            let Some(hunk) = shown.diff.hunks.get(*index) else {
                return frame;
            };
            let at = current == Some(*index);
            // The current hunk gets a marker, not only the accent color.
            let text = format!(
                "{} @@ -{},{} +{},{} @@ {}",
                if at { "▸" } else { " " },
                hunk.old_start,
                hunk.old_lines,
                hunk.new_start,
                hunk.new_lines,
                hunk.header
            );
            frame
                .bg(rgb(theme.diff_hunk_header))
                .text_color(rgb(if at { theme.accent } else { theme.text_muted }))
                // Shrinks, so a hunk action after it stays on screen.
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(text),
                )
        }
        git_view::DiffRow::Line(line) => {
            let modified = shown.modified.get(index).copied().unwrap_or(false);
            let (bg, fg, sign) = match (line.kind, modified) {
                (DiffLineKind::Add, false) => (Some(theme.diff_add_bg), theme.diff_add_text, "+"),
                (DiffLineKind::Del, false) => (Some(theme.diff_del_bg), theme.diff_del_text, "−"),
                (DiffLineKind::Add, true) => (Some(theme.diff_mod_bg), theme.diff_mod_text, "+"),
                (DiffLineKind::Del, true) => (Some(theme.diff_mod_bg), theme.diff_mod_text, "−"),
                (DiffLineKind::Context, _) => (None, theme.text, " "),
            };
            let is_picked = line
                .new_line
                .zip(picked)
                .is_some_and(|(n, picked)| picked.contains(n, n));
            frame
                .when_some(bg, |el, bg| el.bg(rgb(bg)))
                .when(is_picked, |el| el.border_color(rgb(theme.accent)))
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .gap(px(space::S))
                        .text_color(rgb(theme.text_muted))
                        .child(number(line.old_line))
                        .child(
                            div()
                                .when(is_picked, |el| el.text_color(rgb(theme.accent)))
                                .child(number(line.new_line)),
                        ),
                )
                .child(diff_code(shown, index, line, sign, fg, theme))
                .when(line.no_newline, |el| {
                    el.child(
                        div()
                            .text_color(rgb(theme.text_muted))
                            .child("(no newline at end of file)"),
                    )
                })
        }
    }
}

/// A diff line's `sign` and text (tabs expanded, syntax colors of `shown.spans[index]`): the
/// part of a row after the numbers, in Unified and in each Split half. In a highlighted diff,
/// text outside tokens is plain text (the sign and background still tell added from removed);
/// unhighlighted diffs keep the diff text color `fg`.
fn diff_code(
    shown: &git_view::Shown,
    index: usize,
    line: &mkrew_protocol::DiffLine,
    sign: &'static str,
    fg: u32,
    theme: &Theme,
) -> gpui::Div {
    let spans = shown.spans.get(index).map(Vec::as_slice).unwrap_or(&[]);
    let text_color = if shown.highlighted { theme.text } else { fg };
    div()
        .flex()
        .gap(px(space::S))
        .child(div().text_color(rgb(fg)).child(sign))
        .child(div().text_color(rgb(text_color)).child(preview::code_line(
            line.text.replace('\t', highlight::TAB),
            spans,
            &[],
            theme,
        )))
}

/// Which half of a Split row.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DiffSide {
    Old,
    New,
}

/// One half of a Split row: the old (left) or new (right) line `index` of `shown.rows`, or an
/// empty padding cell. `modified` = the row pairs a removed with an added line (amber on both
/// halves); otherwise removed / added lines keep the delete / add colors. Both halves have the
/// pick bar's width, so the columns line up; only the new side shows the picked bar.
fn split_half(
    shown: &git_view::Shown,
    index: Option<usize>,
    side: DiffSide,
    modified: bool,
    picked: Option<Selection>,
    theme: &Theme,
) -> gpui::Div {
    let half = div()
        .flex_1()
        .min_w_0()
        .h_full()
        .overflow_hidden()
        .flex()
        .items_center()
        .gap(px(space::M))
        .px(px(space::M))
        .border_l(px(theme::DIFF_PICK_BAR_WIDTH))
        .border_color(gpui::transparent_black())
        .whitespace_nowrap();
    let Some((at, git_view::DiffRow::Line(line))) =
        index.and_then(|at| Some((at, shown.rows.get(at)?)))
    else {
        return half;
    };
    let (bg, fg, sign) = match (line.kind, modified) {
        (DiffLineKind::Add, false) => (Some(theme.diff_add_bg), theme.diff_add_text, "+"),
        (DiffLineKind::Del, false) => (Some(theme.diff_del_bg), theme.diff_del_text, "−"),
        (DiffLineKind::Add, true) => (Some(theme.diff_mod_bg), theme.diff_mod_text, "+"),
        (DiffLineKind::Del, true) => (Some(theme.diff_mod_bg), theme.diff_mod_text, "−"),
        (DiffLineKind::Context, _) => (None, theme.text, " "),
    };
    let number = match side {
        DiffSide::Old => line.old_line,
        DiffSide::New => line.new_line,
    };
    let is_picked = side == DiffSide::New
        && line
            .new_line
            .zip(picked)
            .is_some_and(|(n, picked)| picked.contains(n, n));
    half.when_some(bg, |el, bg| el.bg(rgb(bg)))
        .when(is_picked, |el| el.border_color(rgb(theme.accent)))
        .child(
            div()
                .flex_none()
                .text_color(rgb(if is_picked {
                    theme.accent
                } else {
                    theme.text_muted
                }))
                .child(match number {
                    Some(n) => format!("{n:>width$}", width = shown.number_width),
                    None => " ".repeat(shown.number_width),
                }),
        )
        .child(diff_code(shown, at, line, sign, fg, theme))
        .when(line.no_newline, |el| {
            el.child(
                div()
                    .text_color(rgb(theme.text_muted))
                    .child("(no newline at end of file)"),
            )
        })
}

/// A Split row that pairs lines (`Same` / `Change`; spans are drawn by [`git_diff_row`]):
/// old half, a 1 px divider, new half. `on_new_half` gets the new half and its new-side line
/// number (`None` for padding) so the caller can make it pickable for a comment.
fn git_split_row(
    shown: &git_view::Shown,
    row: &git_view::SplitRow,
    picked: Option<Selection>,
    line_height: f32,
    theme: &Theme,
    on_new_half: impl FnOnce(gpui::Div, Option<u32>) -> gpui::Div,
) -> gpui::Div {
    let (old, new, modified) = match row {
        git_view::SplitRow::Same(at) => (Some(*at), Some(*at), false),
        git_view::SplitRow::Change { old, new } => (*old, *new, old.is_some() && new.is_some()),
        git_view::SplitRow::Span(_) => (None, None, false),
    };
    let new_number = new.and_then(|at| match shown.rows.get(at) {
        Some(git_view::DiffRow::Line(line)) => line.new_line,
        _ => None,
    });
    div()
        .w_full()
        .h(px(line_height))
        .flex()
        .items_center()
        .child(split_half(
            shown,
            old,
            DiffSide::Old,
            modified,
            picked,
            theme,
        ))
        .child(
            div()
                .flex_none()
                .w(px(theme::BORDER_WIDTH))
                .h_full()
                .bg(rgb(theme.border)),
        )
        .child(on_new_half(
            split_half(shown, new, DiffSide::New, modified, picked, theme),
            new_number,
        ))
}

/// Makes `el` (a diff row, or the new half of a Split row) pick new-side line `number` for a
/// comment on click; hover previews the pick bar unless the line is already picked.
fn pick_on_click(
    el: gpui::Div,
    project_id: Ulid,
    number: u32,
    picked: Option<Selection>,
    border: u32,
    cx: &Context<'_, Shell>,
) -> gpui::Div {
    el.cursor_pointer()
        .when(!picked.is_some_and(|p| p.contains(number, number)), |el| {
            el.hover(move |style| style.border_color(rgb(border)))
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |shell, event: &gpui::MouseDownEvent, window, cx| {
                shell.pick_diff_line(project_id, number, event.modifiers.shift, window, cx);
            }),
        )
}

/// A file size for people: bytes, KB or MB (1024-based, one decimal).
fn size_label(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let bytes_f = bytes as f64;
    if bytes == 1 {
        "1 byte".to_owned()
    } else if bytes_f < KB {
        format!("{bytes} bytes")
    } else if bytes_f < KB * KB {
        format!("{:.1} KB", bytes_f / KB)
    } else {
        format!("{:.1} MB", bytes_f / (KB * KB))
    }
}

fn doc_row(
    project_id: Ulid,
    file: &DocFile,
    matched: &[u32],
    selected: bool,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Stateful<gpui::Div> {
    let (folder, name) = match file.file_path.rsplit_once('/') {
        Some((folder, name)) => (Some(folder), name),
        None => (None, file.file_path.as_str()),
    };
    let hover = theme.surface_raised;
    let file_path = file.file_path.clone();
    // `matched` indexes chars of the whole path: split it over the folder and the name.
    let name_from = folder.map_or(0, |folder| folder.chars().count() + 1);
    let name_from = u32::try_from(name_from).unwrap_or(u32::MAX);
    let name_hits: Vec<u32> = matched
        .iter()
        .filter_map(|i| i.checked_sub(name_from))
        .collect();
    let folder_hits: Vec<u32> = matched.iter().copied().filter(|i| *i < name_from).collect();
    let marked = |text: &str, hits: &[u32]| {
        let style = HighlightStyle {
            color: Some(Hsla::from(rgb(theme.accent))),
            font_weight: Some(FontWeight(weight::SEMIBOLD)),
            ..HighlightStyle::default()
        };
        StyledText::new(text.to_owned()).with_highlights(
            quick_open::match_ranges(text, hits)
                .into_iter()
                .map(move |range| (range, style)),
        )
    };
    let line = |el: gpui::Div| el.overflow_hidden().text_ellipsis().whitespace_nowrap();
    div()
        .id(SharedString::from(format!("doc-{}", file.file_path)))
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::MEDIUM))
        .cursor_pointer()
        .when(selected, |el| el.bg(rgb(theme.surface_raised)))
        .when(!selected, |el| el.hover(move |style| style.bg(rgb(hover))))
        .active(|style| style.opacity(opacity::ACTIVE))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(line(
                    div()
                        .text_size(font_size::BODY)
                        .text_color(rgb(theme.text))
                        .when(selected, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                        .child(marked(name, &name_hits)),
                ))
                .children(folder.map(|folder| {
                    line(
                        div()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child(marked(folder, &folder_hits)),
                    )
                })),
        )
        .when(file.open_comments > 0, |el| {
            el.child(
                div()
                    .flex_none()
                    .px(px(space::XS))
                    .rounded(px(radius::SMALL))
                    // Stands out on the selected row's raised background too.
                    .bg(rgb(if selected {
                        theme.bg
                    } else {
                        theme.surface_raised
                    }))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(format!("{} open", file.open_comments)),
            )
        })
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.select_doc(project_id, &file_path, window, cx);
        }))
}

/// A full-width row of a popover menu; callers add the label (and a shortcut on the right).
fn menu_item(id: SharedString, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    // One step above the menu's own `surface_raised`.
    let hover = theme.border;
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .text_size(font_size::BODY)
        .text_color(rgb(theme.text_muted))
        .hover(move |style| style.bg(rgb(hover)))
        .active(|style| style.opacity(opacity::ACTIVE))
}

/// The floating panel of a menu drawn in a `deferred(anchored(..))` (workspace, switcher, Run ▾,
/// Run with, Several agents, tab, pane and Files menus). It occludes the mouse: gpui hit-tests every hitbox
/// until one blocks, so without `.occlude()` a click on the menu also reaches the row or card
/// painted under it (a color pick would navigate to the row behind the swatch).
fn menu_panel(id: impl Into<gpui::ElementId>, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .w(px(theme::MENU_WIDTH))
        .flex()
        .flex_col()
        .p(px(space::XS))
        .rounded(px(radius::MEDIUM))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.border))
        .bg(rgb(theme.surface_raised))
        .shadow_md()
        .font_family(theme.ui_font_family.clone())
        .occlude()
}

/// One color choice of a color menu: a filled dot (`None` = an empty ring = no color); the
/// current one is ringed in the text color. A click calls `pick`; the element id is
/// `<id_prefix>-<color>`.
fn color_swatch(
    id_prefix: &'static str,
    color: Option<&'static str>,
    current: Option<&str>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
    pick: impl Fn(&mut Shell, Option<&'static str>, &mut Window, &mut Context<'_, Shell>) + 'static,
) -> gpui::AnyElement {
    let fill = color.and_then(|name| theme.workspace_color(name));
    let ring = if color == current {
        theme.text
    } else {
        fill.unwrap_or(theme.text_muted)
    };
    div()
        .id(SharedString::from(format!(
            "{id_prefix}-{}",
            color.unwrap_or("none")
        )))
        .flex_none()
        .size(px(theme::WORKSPACE_SWATCH_SIZE))
        .rounded_full()
        .cursor_pointer()
        .border(px(theme::WORKSPACE_SWATCH_BORDER))
        .border_color(rgb(ring))
        .when_some(fill, |el, fill| el.bg(rgb(fill)))
        .hover(|style| style.opacity(opacity::HOVER))
        .tooltip(tooltip(theme, color.unwrap_or("No color"), ""))
        .on_click(cx.listener(move |shell, _, window, cx| {
            pick(shell, color, window, cx);
        }))
        .into_any_element()
}

/// The rows of a profile list (agents or shells) in a menu (`current` checked): loading, the
/// error, an empty hint or one item per profile; a click calls `pick` with the profile id.
fn agent_rows(
    agents: &Agents,
    kind: SessionKind,
    current: Option<&str>,
    id_prefix: &str,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
    pick: impl Fn(&mut Shell, String, &mut Window, &mut Context<'_, Shell>) + Clone + 'static,
) -> Vec<gpui::AnyElement> {
    let muted_line = |text: String| {
        div()
            .px(px(space::S))
            .py(px(space::XS))
            .text_size(font_size::META)
            .text_color(rgb(theme.text_muted))
            .child(text)
    };
    match agents {
        Agents::Loading => vec![muted_line("Loading agents…".into()).into_any_element()],
        Agents::Failed(error) => {
            vec![
                div()
                    .flex()
                    .pl(px(space::S))
                    .child(
                        div()
                            .py(px(space::XS))
                            .text_size(font_size::META)
                            .text_color(rgb(theme.status_exited))
                            .child("✕"),
                    )
                    .child(muted_line(format!("Cannot list the agents: {error}")).pl(px(space::XS)))
                    .into_any_element(),
            ]
        }
        Agents::Loaded(profiles) => {
            let choices = workspace_menu::agent_choices(profiles, kind, current);
            if choices.is_empty() {
                let empty = match kind {
                    SessionKind::Agent => "No agent profiles. Add one to the profiles folder.",
                    SessionKind::Shell => {
                        "No shell found on PATH. Add a shell profile to the profiles folder."
                    }
                };
                vec![muted_line(empty.into()).into_any_element()]
            } else {
                choices
                    .into_iter()
                    .map(|choice| {
                        let id = SharedString::from(format!("{id_prefix}-{}", choice.id));
                        let agent = choice.id;
                        let pick = pick.clone();
                        menu_item(id, theme)
                            .child(
                                div()
                                    .w(px(space::M))
                                    .text_color(rgb(theme.accent))
                                    .child(if choice.selected { "✓" } else { "" }),
                            )
                            .when(!agent.is_empty(), |el| {
                                el.child(agent_chip(
                                    sidebar::agent_badge(choice.icon.as_deref(), &agent),
                                    theme,
                                ))
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .when(choice.selected, |el| {
                                        el.text_color(rgb(theme.text))
                                            .font_weight(FontWeight(weight::MEDIUM))
                                    })
                                    .child(choice.label),
                            )
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                pick(shell, agent.clone(), window, cx);
                            }))
                            .into_any_element()
                    })
                    .collect()
            }
        }
    }
}

fn workspace_row(
    row: WorkspaceRow,
    active: bool,
    run_tip: Option<String>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let project_id = row.project_id;
    let hover = theme.hover;
    let group = SharedString::from(format!("workspace-{project_id}"));
    let color = row
        .color
        .as_deref()
        .and_then(|name| theme.workspace_color(name));
    div()
        .id(group.clone())
        .group(group.clone())
        .h(theme::rems_px(theme::WORKSPACE_ROW_HEIGHT))
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .when(active, |el| el.bg(rgb(theme.selected)))
        .hover(move |style| style.bg(rgb(hover)))
        .tooltip(tooltip(theme, &row.path, ""))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.click_workspace_row(project_id, window, cx);
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |shell, _, window, cx| {
                shell.open_workspace_menu(project_id, window, cx);
            }),
        )
        .child(
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.chevron_color()))
                .child(if row.expanded { "▾" } else { "▸" }),
        )
        .child(workspace_avatar(&row.name, color, theme))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(font_size::BODY)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text))
                .child(row.name),
        )
        .when(row.pinned, |el| el.child(pin_mark()))
        .when_some(run_tip, |el, tip| {
            el.child(
                div()
                    .id(SharedString::from(format!("workspace-run-{project_id}")))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .tooltip(tooltip(theme, &tip, ""))
                    .child("▶"),
            )
        })
        .when_some(row.summary.clone(), |el, summary| {
            el.child(
                div()
                    .flex_none()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(summary),
            )
        })
        .when(row.needs_input, |el| {
            el.child(
                div()
                    .text_size(font_size::META)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.status_needs_input))
                    .child(sidebar::status_display(SessionState::NeedsInput).icon),
            )
        })
        .child(
            row_action(
                SharedString::from(format!("workspace-menu-{project_id}")),
                "⋯",
                theme,
            )
            // Shown on hover, and always on the active row (SPEC §14 progressive disclosure).
            .when(!active, |el| {
                el.opacity(opacity::HIDDEN)
                    .group_hover(group, |style| style.opacity(opacity::FULL))
            })
            .tooltip(tooltip(
                theme,
                "Workspace menu (or right-click the row)",
                "",
            ))
            .on_click(cx.listener(move |shell, _, window, cx| {
                // Not a click on the row (which would collapse it).
                cx.stop_propagation();
                shell.open_workspace_menu(project_id, window, cx);
            })),
        )
        .into_any_element()
}

/// The pin before the name of a pinned workspace, session or tab.
fn pin_mark() -> gpui::Div {
    div()
        .flex_none()
        .text_size(font_size::META)
        .child(theme::PIN_GLYPH)
}

/// A workspace's first letter on a tint of its color (SPEC §10, ADR 0020); muted without one.
/// The letter is `text` (see [`Theme::avatar_colors`]); the tint carries the color.
fn workspace_avatar(name: &str, color: Option<u32>, theme: &Theme) -> gpui::Div {
    let color = color.unwrap_or(theme.text_muted);
    let (letter, _) = theme.avatar_colors(color, theme.sidebar);
    div()
        .flex_none()
        .size(theme::rems_px(theme::WORKSPACE_AVATAR_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::SMALL))
        .bg(theme.tint(color))
        .text_color(rgb(letter))
        .text_size(font_size::META)
        .font_weight(FontWeight(weight::SEMIBOLD))
        .child(
            name.chars()
                .next()
                .map(|c| c.to_uppercase().collect::<String>())
                .unwrap_or_default(),
        )
}

/// A thin progress bar: `fraction` (0–1) of the track in the accent color.
fn progress_bar(fraction: f32, height: f32, theme: &Theme) -> gpui::Div {
    div()
        .w_full()
        .h(px(height))
        .rounded(px(radius::SMALL))
        .overflow_hidden()
        .bg(rgb(theme.border))
        .child(div().h_full().w(relative(fraction)).bg(rgb(theme.accent)))
}

/// One checklist line of the Plan card: a check box and the item text; the first unchecked
/// item is in the accent text color, checked ones are muted.
fn plan_row(row: &plan::Row, theme: &Theme) -> gpui::Div {
    let color = match (row.done, row.current) {
        (true, _) => theme.text_muted,
        (false, true) => theme.accent_text,
        (false, false) => theme.text_dim,
    };
    let check = div()
        .flex_none()
        .mt(px(space::XS / 2.0))
        .w(px(theme::DOC_TASK_BOX_SIZE))
        .h(px(theme::DOC_TASK_BOX_SIZE))
        .rounded(px(radius::SMALL))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(if row.done { theme.accent } else { theme.subtle }))
        .when(row.done, |el| el.bg(rgb(theme.accent)));
    div()
        .flex()
        .items_start()
        .gap(px(space::S))
        .child(check)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(font_size::BODY)
                .text_color(rgb(color))
                .when(row.current, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                .child(row.text.clone()),
        )
}

fn session_row(
    row: SessionRow,
    badge: String,
    selected: bool,
    progress: Option<f32>,
    now: i64,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let session_id = row.session_id;
    let time = row.time_text(now);
    // Only the selected row spells the title out on a second line under the agent (SPEC §11).
    // The other agent rows show just the title (the agent chip already names the agent);
    // terminal rows keep `powershell · title` (their chip does not say which shell).
    // `progress` is the plan bar of the selected row, last in the column under the name /
    // title lines.
    let (name, subtitle) = match (&row.title, selected) {
        (Some(title), true) => (row.agent.clone(), Some(title.clone())),
        (Some(title), false) if row.shell => (format!("{} · {title}", row.agent), None),
        (Some(title), false) => (title.clone(), None),
        (None, _) => (row.agent.clone(), None),
    };
    let hover = theme.hover;
    let row_id = SharedString::from(format!("session-{session_id}"));
    div()
        .id(row_id.clone())
        .group(row_id.clone())
        .min_h(theme::rems_px(theme::SESSION_ROW_HEIGHT))
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .when(subtitle.is_some() || progress.is_some(), |el| {
            el.py(px(space::XS))
        })
        .rounded(px(radius::SMALL))
        .when(selected, |el| el.bg(rgb(theme.selected)))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(hover)))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.open_session(session_id, window, cx);
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                // Show the pane first: the menu's Rename edits the pane header's title, which
                // exists only for the pane on screen (same as a left click).
                shell.open_session(session_id, window, cx);
                shell.open_pane_menu(session_id, event.position, window, cx);
            }),
        )
        .on_hover(cx.listener(move |shell, hovered: &bool, window, cx| {
            shell.hover_row(session_id, *hovered, window, cx);
        }))
        .child(
            // The icon says the state; its hover says it in words and when it changed.
            div()
                .id(SharedString::from(format!("session-status-{session_id}")))
                .flex_none()
                .tooltip(status_tooltip(
                    theme,
                    row.state,
                    row.state_source,
                    row.state_changed_at,
                ))
                .child(status_mark(row.state, now, theme)),
        )
        .child(agent_chip(badge, theme))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .truncate()
                        .text_size(font_size::BODY)
                        .text_color(rgb(theme.text))
                        .when(selected, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                        .child(name),
                )
                .children(subtitle.map(|title| {
                    div()
                        .truncate()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(title)
                }))
                .children(
                    progress
                        .map(|fraction| progress_bar(fraction, theme::PLAN_ROW_BAR_HEIGHT, theme)),
                ),
        )
        .when(row.pinned, |el| el.child(pin_mark()))
        // A right-aligned slot of a minimum width, kept without a time, so every row's time
        // and title end in one column.
        .child(
            div()
                .flex_none()
                .min_w(theme::rems_px(theme::SESSION_TIME_WIDTH))
                .flex()
                .justify_end()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .children(time),
        )
        .child(
            row_action(
                SharedString::from(format!("session-remove-{session_id}")),
                "✕",
                theme,
            )
            // Shown on hover (SPEC §14 progressive disclosure).
            .opacity(opacity::HIDDEN)
            .group_hover(row_id.clone(), |style| style.opacity(opacity::FULL))
            .tooltip(tooltip(theme, "Remove from the sidebar", ""))
            .on_click(cx.listener(move |shell, _, window, cx| {
                // Not a click on the row (which would open the session).
                cx.stop_propagation();
                shell.remove_session(session_id, window, cx);
            })),
        )
        .into_any_element()
}

/// A workspace in the compact rail: its avatar, then `?` while a session needs input, else its
/// working count. Clicks as [`workspace_row`]; the tooltip has the name, summary and folder.
fn compact_workspace_row(
    row: WorkspaceRow,
    active: bool,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let project_id = row.project_id;
    let hover = theme.hover;
    let color = row
        .color
        .as_deref()
        .and_then(|name| theme.workspace_color(name));
    let mark = if row.needs_input {
        let icon = sidebar::status_display(SessionState::NeedsInput).icon;
        Some((icon.to_owned(), theme.status_needs_input))
    } else {
        (row.working > 0).then(|| (row.working.to_string(), theme.text_muted))
    };
    div()
        .id(SharedString::from(format!("workspace-{project_id}")))
        .h(theme::rems_px(theme::WORKSPACE_ROW_HEIGHT))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(space::XS))
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .when(active, |el| el.bg(rgb(theme.selected)))
        .hover(move |style| style.bg(rgb(hover)))
        .tooltip(tooltip(theme, &sidebar::workspace_tip(&row), ""))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.click_workspace_row(project_id, window, cx);
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |shell, _, window, cx| {
                shell.open_workspace_menu(project_id, window, cx);
            }),
        )
        .child(workspace_avatar(&row.name, color, theme))
        // A fixed-width slot, empty without a mark, so every avatar sits in one column.
        .child(
            div()
                .flex_none()
                .w(theme::rems_px(theme::STATUS_MARK_WIDTH))
                .flex()
                .justify_center()
                .children(mark.map(|(text, color)| {
                    div()
                        .text_size(font_size::META)
                        .font_weight(FontWeight(weight::SEMIBOLD))
                        .text_color(rgb(color))
                        .child(text)
                })),
        )
        .into_any_element()
}

/// A session in the compact rail: its status mark and agent chip. Clicks as [`session_row`];
/// the tooltip has `agent · full title` and the status in words. No hover peek (the tooltip
/// replaces it); the keyboard peek still hangs on this row.
fn compact_session_row(
    row: SessionRow,
    badge: String,
    selected: bool,
    now: i64,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let session_id = row.session_id;
    let hover = theme.hover;
    div()
        .id(SharedString::from(format!("session-{session_id}")))
        .h(theme::rems_px(theme::SESSION_ROW_HEIGHT))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(space::XS))
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .when(selected, |el| el.bg(rgb(theme.selected)))
        .hover(move |style| style.bg(rgb(hover)))
        .tooltip(tooltip(theme, &sidebar::session_tip(&row, now), ""))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.open_session(session_id, window, cx);
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                // As in `session_row`: show the pane first (Rename edits its header).
                shell.open_session(session_id, window, cx);
                shell.open_pane_menu(session_id, event.position, window, cx);
            }),
        )
        .child(status_mark(row.state, now, theme))
        .child(agent_chip(badge, theme))
        .into_any_element()
}

/// Shell state the cards of a board render with.
struct CardContext<'a> {
    selected: Option<Ulid>,
    /// Tasks whose run is starting.
    starting: &'a HashSet<Ulid>,
    follow_up: Option<&'a FollowUp>,
    follow_up_focus: &'a FocusHandle,
    follow_up_scroll: &'a ScrollHandle,
    /// Composing IME chars of the follow-up field (see [`Shell::typing`]).
    ime_marked: &'a Rc<ime::Composing<FocusHandle>>,
    /// The follow-up field has the keyboard (shows its caret).
    follow_up_focused: bool,
    /// Shortcut label of Run on the selected card.
    run_keys: String,
    conflicts: &'a Conflicts,
    run_with: Option<&'a RunWith>,
    fan_out: Option<&'a FanOutPicker>,
    usages: &'a Usages,
    sessions: &'a [Session],
    /// Profile icons by id, for the agent chip (see [`Shell::agent_badge`]).
    agent_icons: &'a HashMap<String, String>,
}

/// A board column: its title and card count over the cards, or a quiet "No tasks". Every
/// card is a drop slot (land before it), and so is the free space under the last one.
fn board_column(
    column: board::Column,
    cards: &CardContext<'_>,
    now: i64,
    new_task: Option<gpui::AnyElement>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let count = column.cards.len();
    let status = column.status;
    div()
        .flex_1()
        .min_w(px(theme::BOARD_COLUMN_MIN_WIDTH))
        .flex()
        .flex_col()
        .gap(px(space::S))
        .child(
            div()
                .min_h(px(theme::BOARD_COLUMN_HEADER_HEIGHT))
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::XS))
                .child(
                    div()
                        .text_size(font_size::BODY)
                        .font_weight(FontWeight(weight::MEDIUM))
                        .text_color(rgb(theme.text))
                        .child(board::column_title(column.status)),
                )
                .child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(count.to_string()),
                )
                .children(new_task.map(|button| div().ml_auto().child(button))),
        )
        .child(
            div()
                .id(SharedString::from(format!(
                    "board-column-{:?}",
                    column.status
                )))
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                // The slots' own vertical padding makes the `S` gap between cards.
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::MEDIUM))
                .bg(rgb(theme.surface))
                .overflow_y_scroll()
                .when(count == 0, |el| {
                    el.child(
                        div()
                            .p(px(space::S))
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child("No tasks"),
                    )
                })
                .children(column.cards.into_iter().map(|card| {
                    let task_id = card.task_id;
                    drop_slot(status, Some(task_id), theme, cx)
                        .py(px(space::XS))
                        .child(task_card(card, cards, now, theme, cx))
                }))
                .child(
                    drop_slot(status, None, theme, cx)
                        .flex_1()
                        .min_h(px(space::XL)),
                ),
        )
        .into_any_element()
}

/// Where a dragged card lands in the `status` column: right before the card `before`, or at
/// the end. Hovering it with a card shows an accent line along its top edge (SPEC §14).
fn drop_slot(
    status: TaskStatus,
    before: Option<Ulid>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Stateful<gpui::Div> {
    let name = match before {
        Some(task_id) => format!("drop-{task_id}"),
        None => format!("drop-end-{status:?}"),
    };
    let group = SharedString::from(name.clone());
    let accent = theme.accent;
    div()
        .id(SharedString::from(name))
        .group(group.clone())
        .relative()
        .flex_none()
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                // Centred on the slot's top edge, i.e. in the gap above the card.
                .top(px(-theme::DROP_INDICATOR_HEIGHT / 2.0))
                .h(px(theme::DROP_INDICATOR_HEIGHT))
                .rounded(px(radius::SMALL))
                .group_drag_over::<DraggedCard>(group, move |style| style.bg(rgb(accent)))
                // No-op: gpui applies group drag styles only to elements with a hitbox, and a
                // drag style gives it one.
                .drag_over::<DraggedCard>(|style, _, _, _| style),
        )
        .on_drop(
            cx.listener(move |shell, dragged: &DraggedCard, window, cx| {
                shell.drop_card(dragged.task_id, status, before, window, cx);
            }),
        )
}

/// A task card: title (Edit on hover), agent, tokens used and run time, then its session's status
/// (SPEC §10, §11), then its column's action or a follow-up field. Dragging it carries a
/// [`DraggedCard`]; clicking it selects it and opens its session's terminal.
fn task_card(
    card: Card,
    cards: &CardContext<'_>,
    now: i64,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let run_time = card.run_time(now);
    // A group card runs no agent of its own (SPEC §20E.3).
    let tokens = card
        .children
        .is_empty()
        .then(|| cards.usages.task_total(card.task_id, cards.sessions))
        .flatten()
        .map(usage::format_tokens);
    let shows_error_row = card.shows_error_row();
    let action = card.action();
    let can_undo = card.can_undo();
    let can_view_changes = card.can_view_changes();
    let manages_worktree = card.manages_worktree();
    let can_compare = card.can_compare();
    let can_run_with = card.can_run_with() && !cards.starting.contains(&card.task_id);
    let can_fan_out = card.can_fan_out() && !cards.starting.contains(&card.task_id);
    let current_agent = card.agent_profile().map(str::to_owned);
    let opens = card.opens_session();
    let follow_up_session = card.follow_up_session();
    let task_id = card.task_id;
    let conflict_paths = cards.conflicts.for_task(task_id);
    let selected = cards.selected == Some(task_id);
    let group = SharedString::from(format!("card-{task_id}"));
    let edit_hover = theme.border;
    let preview_style = PopupStyle::of(theme);
    div()
        .id(group.clone())
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.card_clicked(task_id, opens, window, cx);
        }))
        .on_drag(
            DraggedCard {
                task_id,
                title: card.title.clone(),
            },
            move |dragged, _, _, cx| {
                let style = preview_style.clone();
                let title = dragged.title.clone().into();
                cx.new(|_| CardPreview { style, title })
            },
        )
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(space::XS))
        .p(px(space::M))
        .rounded(px(radius::MEDIUM))
        .bg(rgb(theme.surface_raised))
        // Always bordered, so selecting a card does not shift its content.
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(if selected {
            theme.pane_focus_border
        } else {
            theme.surface_raised
        }))
        .when(!selected, |el| {
            el.hover(move |style| style.border_color(rgb(edit_hover)))
        })
        .cursor_pointer()
        .group(group.clone())
        .child(
            div()
                .flex()
                .items_start()
                .gap(px(space::S))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(font_size::CARD_TITLE)
                        .font_weight(FontWeight(weight::MEDIUM))
                        .text_color(rgb(theme.text))
                        .child(card.title),
                )
                // Shown on hover (SPEC §14 progressive disclosure).
                .child(
                    div()
                        .id(SharedString::from(format!("card-edit-{task_id}")))
                        .flex_none()
                        .px(px(space::S))
                        .py(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .cursor_pointer()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .opacity(opacity::HIDDEN)
                        .group_hover(group.clone(), |style| style.opacity(opacity::FULL))
                        // One step above the card's own `surface_raised`.
                        .hover(move |style| style.bg(rgb(edit_hover)))
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .tooltip(tooltip(theme, "Edit or delete this task", ""))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            // Not a click on the card (which would open its terminal).
                            cx.stop_propagation();
                            if let Some(task) = shell.board.task(task_id).cloned() {
                                shell.open_task_dialog(TaskDialog::edit(task), window, cx);
                            }
                        }))
                        .child("Edit"),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(space::S))
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .children(current_agent.as_deref().map(|agent| {
                    agent_chip(
                        sidebar::agent_badge(
                            cards.agent_icons.get(agent).map(String::as_str),
                            agent,
                        ),
                        theme,
                    )
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(card.agent),
                )
                .children(tokens.map(|tokens| {
                    div()
                        .id(SharedString::from(format!("card-tokens-{task_id}")))
                        .flex_none()
                        .tooltip(tooltip(
                            theme,
                            "Tokens used by this task's agent (input + output)",
                            "",
                        ))
                        .child(tokens)
                }))
                .children(run_time.map(|time| div().flex_none().child(time))),
        )
        .children(card.branch.map(|branch| {
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(branch)
        }))
        // A group card: one line per child (SPEC §20E.2).
        .children(
            card.children
                .into_iter()
                .map(|child| fan_out_child_row(child, now, theme)),
        )
        .when_some(card.session, |el, session| {
            el.child(
                div()
                    .id(SharedString::from(format!("card-status-{task_id}")))
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .tooltip(status_tooltip(
                        theme,
                        session.state,
                        session.state_source,
                        session.state_changed_at,
                    ))
                    .child(status_mark(session.state, now, theme))
                    .child(status::text(
                        session.state,
                        session.exit_code,
                        session.state_changed_at,
                        now,
                    )),
            )
        })
        .when(!conflict_paths.is_empty(), |el| {
            el.child(conflict_badge(
                format!("card-{task_id}"),
                &conflict_paths,
                theme,
                move |shell| shell.conflicts.dismiss_task(task_id),
                cx,
            ))
        })
        .when(shows_error_row, |el| {
            let exited = sidebar::status_display(SessionState::Exited);
            el.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(
                        div()
                            .text_color(rgb(theme.status_exited))
                            .child(exited.icon),
                    )
                    .child("Last run failed"),
            )
        })
        .when_some(follow_up_session, |el, session_id| {
            el.child(follow_up_field(task_id, session_id, cards, theme, cx))
        })
        .when(
            action.is_some() || can_undo || manages_worktree || can_compare,
            |el| {
                // Secondary: shown on hover, left of the column's action (SPEC §14).
                let undo = can_undo.then(|| {
                    quiet_button("card-undo", "Undo", theme, edit_hover)
                        .opacity(opacity::HIDDEN)
                        .group_hover(group.clone(), |style| style.opacity(opacity::FULL))
                        .tooltip(tooltip(
                            theme,
                            "Undo task: review and discard its changes",
                            "",
                        ))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            cx.stop_propagation();
                            shell.selected_card = Some(task_id);
                            shell.open_undo_dialog(task_id, window, cx);
                        }))
                });
                let changes = can_view_changes.then(|| {
                    quiet_button("card-changes", "Changes", theme, edit_hover)
                        .opacity(opacity::HIDDEN)
                        .group_hover(group.clone(), |style| style.opacity(opacity::FULL))
                        .tooltip(tooltip(
                            theme,
                            "View changes: the task's diff since its run started",
                            "",
                        ))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            cx.stop_propagation();
                            shell.selected_card = Some(task_id);
                            shell.view_task_changes(task_id, window, cx);
                        }))
                });
                // The worktree's two actions (SPEC §20E.1), hover-shown like Undo.
                let worktree_actions = manages_worktree.then(|| {
                    [
                        (
                            "card-merge",
                            "Merge back…",
                            "Merge back: merge the task's branch into your checkout",
                            worktree_dialog::Kind::Merge,
                        ),
                        (
                            "card-remove-worktree",
                            "Remove worktree…",
                            "Remove worktree: delete its folder (an unmerged branch is kept)",
                            worktree_dialog::Kind::Remove,
                        ),
                    ]
                    .map(|(id, text, tip, kind)| {
                        quiet_button(id, text, theme, edit_hover)
                            .opacity(opacity::HIDDEN)
                            .group_hover(group.clone(), |style| style.opacity(opacity::FULL))
                            .tooltip(tooltip(theme, tip, ""))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                cx.stop_propagation();
                                shell.selected_card = Some(task_id);
                                shell.open_worktree_dialog(kind, task_id, window, cx);
                            }))
                    })
                });
                let run_with = can_run_with.then(|| {
                    run_with_button(
                        task_id,
                        current_agent.as_deref(),
                        group.clone(),
                        cards,
                        theme,
                        cx,
                    )
                });
                let fan_out =
                    can_fan_out.then(|| fan_out_button(task_id, group.clone(), cards, theme, cx));
                // Every child finished: the group card's next step (SPEC §20E.2), always shown.
                let compare = can_compare.then(|| {
                    // Accent only on the selected card, like Run.
                    let compare = if selected {
                        button("card-compare", "Compare", theme, true)
                    } else {
                        quiet_button("card-compare", "Compare", theme, edit_hover)
                    };
                    compare
                        .tooltip(tooltip(
                            theme,
                            "Compare the agents' results and pick one to merge back",
                            "",
                        ))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            cx.stop_propagation();
                            shell.selected_card = Some(task_id);
                            shell.open_compare(task_id, window, cx);
                        }))
                });
                el.child(
                    div()
                        .flex()
                        // A Review card can show seven buttons; a narrow column wraps them.
                        .flex_wrap()
                        .justify_end()
                        .gap(px(space::S))
                        .children(run_with)
                        .children(fan_out)
                        .children(changes)
                        .children(worktree_actions.into_iter().flatten())
                        .children(undo)
                        .children(compare)
                        .children(action.map(|action| {
                            card_action(action, task_id, selected, cards, theme, cx)
                        })),
                )
            },
        )
        .into_any_element()
}

/// "Run with…" on a Todo / Review card (SPEC §20B.6): shown on hover (always while its picker
/// is open); the picker of agent profiles drops down from below it, a click outside closes it.
fn run_with_button(
    task_id: Ulid,
    current: Option<&str>,
    group: SharedString,
    cards: &CardContext<'_>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Div {
    let menu = cards.run_with.filter(|menu| menu.task_id == task_id);
    // One step above the card's own `surface_raised`, like its other quiet buttons.
    let button = quiet_button("card-run-with", "Run with…", theme, theme.border)
        .when(menu.is_none(), |el| {
            el.opacity(opacity::HIDDEN)
                .group_hover(group, |style| style.opacity(opacity::FULL))
        })
        .tooltip(tooltip(
            theme,
            "Run with another agent: changes the task's agent, then runs it",
            "",
        ))
        // An open picker already closed on this mouse down (outside it): the click reopens it.
        .on_click(cx.listener(move |shell, _, window, cx| {
            cx.stop_propagation();
            shell.open_run_with(task_id, window, cx);
        }));
    let Some(menu) = menu else {
        return div().child(button);
    };
    let agents = agent_rows(
        &menu.agents,
        SessionKind::Agent,
        current,
        "run-with",
        theme,
        cx,
        move |shell, agent, window, cx| {
            // Not a click on the card under the menu (which would open its terminal).
            cx.stop_propagation();
            shell.run_task_with(task_id, agent, window, cx);
        },
    );
    let panel = menu_panel("run-with-menu", theme)
        .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
            shell.run_with = None;
            cx.notify();
        }))
        .child(
            div()
                .px(px(space::S))
                .pt(px(space::XS))
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_muted))
                .child("Run with"),
        )
        .children(agents);
    div().child(button).child(deferred(
        anchored()
            .snap_to_window_with_margin(px(space::S))
            .child(div().pt(px(space::XS)).child(panel)),
    ))
}

/// "Several agents…" on a Todo / Review card (SPEC §20E.2), next to "Run with…" and shown the
/// same way; its picker ticks 2–5 agent profiles, then runs them, each in its own worktree.
fn fan_out_button(
    task_id: Ulid,
    group: SharedString,
    cards: &CardContext<'_>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Div {
    let menu = cards.fan_out.filter(|menu| menu.task_id == task_id);
    let button = quiet_button("card-fan-out", "Several agents…", theme, theme.border)
        .when(menu.is_none(), |el| {
            el.opacity(opacity::HIDDEN)
                .group_hover(group, |style| style.opacity(opacity::FULL))
        })
        .tooltip(tooltip(
            theme,
            "Run with several agents: each in its own worktree, to compare (git repos only)",
            "",
        ))
        .on_click(cx.listener(move |shell, _, window, cx| {
            cx.stop_propagation();
            shell.open_fan_out(task_id, window, cx);
        }));
    let Some(menu) = menu else {
        return div().child(button);
    };
    let choices = match &menu.agents {
        Agents::Loaded(profiles) => {
            workspace_menu::agent_choices(profiles, SessionKind::Agent, None)
        }
        _ => Vec::new(),
    };
    let rows = if choices.is_empty() {
        // Loading, the error or the empty hint; there is nothing to tick.
        agent_rows(
            &menu.agents,
            SessionKind::Agent,
            None,
            "fan-out",
            theme,
            cx,
            |_, _, _, _| {},
        )
    } else if choices.len() < TaskFanoutParams::MIN_PROFILES {
        vec![
            div()
                .px(px(space::S))
                .py(px(space::XS))
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child("Needs at least 2 agent profiles. Add one to the profiles folder.")
                .into_any_element(),
        ]
    } else {
        choices
            .into_iter()
            .map(|choice| {
                let picked = menu.is_picked(&choice.id);
                let blocked = !picked && menu.is_full();
                let id = SharedString::from(format!("fan-out-{}", choice.id));
                let agent = choice.id;
                menu_item(id, theme)
                    .child(
                        div()
                            .w(px(space::M))
                            .text_color(rgb(theme.accent))
                            .child(if picked { "✓" } else { "" }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .when(picked, |el| {
                                el.text_color(rgb(theme.text))
                                    .font_weight(FontWeight(weight::MEDIUM))
                            })
                            .child(choice.label),
                    )
                    .when(blocked, |el| el.opacity(opacity::DISABLED).cursor_default())
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        // Not a click on the card under the menu (which would open its terminal).
                        cx.stop_propagation();
                        if let Some(menu) = shell.fan_out.as_mut() {
                            menu.toggle(&agent);
                            cx.notify();
                        }
                    }))
                    .into_any_element()
            })
            .collect()
    };
    let label = menu.button_label();
    let run = div()
        .id("fan-out-run")
        .flex()
        .items_center()
        .px(px(space::S))
        .py(px(space::XS))
        .rounded(px(radius::SMALL))
        .text_size(font_size::BODY)
        .bg(rgb(theme.accent))
        .text_color(rgb(theme.on_accent))
        .child(label);
    let run = if menu.request().is_some() {
        run.cursor_pointer()
            .hover(|style| style.opacity(opacity::HOVER))
            .active(|style| style.opacity(opacity::ACTIVE))
            .on_click(cx.listener(|shell, _, window, cx| {
                cx.stop_propagation();
                shell.run_fan_out(window, cx);
            }))
    } else {
        run.opacity(opacity::DISABLED)
    };
    let panel = menu_panel("fan-out-menu", theme)
        .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
            shell.fan_out = None;
            cx.notify();
        }))
        .child(
            div()
                .px(px(space::S))
                .pt(px(space::XS))
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_muted))
                .child("Run with several agents"),
        )
        .children(rows)
        .child(
            div()
                .flex()
                .justify_end()
                .px(px(space::S))
                .pt(px(space::S))
                .pb(px(space::XS))
                .child(run),
        );
    div().child(button).child(deferred(
        anchored()
            .snap_to_window_with_margin(px(space::S))
            .child(div().pt(px(space::XS)).child(panel)),
    ))
}

/// One child on a group card (SPEC §20E.2): its status icon, agent and state.
fn fan_out_child_row(child: fanout::ChildLine, now: i64, theme: &Theme) -> gpui::Div {
    let state = child.state_text(now);
    let icon = child
        .session
        .as_ref()
        .map(|session| status_mark(session.state, now, theme));
    div()
        .flex()
        .items_center()
        .gap(px(space::XS))
        .text_size(font_size::META)
        .text_color(rgb(theme.text_muted))
        .child(
            div()
                .flex_none()
                .w(theme::rems_px(theme::STATUS_MARK_WIDTH))
                .children(icon),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_color(rgb(theme.text))
                .child(child.agent),
        )
        .child(div().flex_none().child(state))
}

/// The button of a card's column (Run / Approve); Run is disabled while its run starts.
fn card_action(
    action: CardAction,
    task_id: Ulid,
    selected: bool,
    cards: &CardContext<'_>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Stateful<gpui::Div> {
    // Ctrl+Enter runs only the selected card.
    let run_keys = if selected {
        cards.run_keys.clone()
    } else {
        String::new()
    };
    let (id, label, tip, keys) = match action {
        CardAction::Run => ("card-run", "Run", "Run this task", run_keys),
        CardAction::Approve => (
            "card-approve",
            "Approve",
            "Accept the result and move the task to Done",
            String::new(),
        ),
    };
    // Accent only on the selected card: a full Todo column stays calm.
    let primary = action == CardAction::Run && selected;
    if cards.starting.contains(&task_id) {
        return disabled_button(id, "Starting…", theme, primary);
    }
    // A quiet button's surface hover would vanish on the raised card: one step above it.
    let action_button = if primary {
        button(id, label, theme, true)
    } else {
        quiet_button(id, label, theme, theme.border)
    };
    action_button
        .tooltip(tooltip(theme, tip, keys))
        .on_click(cx.listener(move |shell, _, window, cx| {
            cx.stop_propagation();
            shell.selected_card = Some(task_id);
            match action {
                CardAction::Run => shell.run_task(task_id, window, cx),
                CardAction::Approve => shell.approve_task(task_id, window, cx),
            }
        }))
}

/// The follow-up input of a running / needs-input card: the multi-line text box that takes the
/// keyboard when clicked; Ctrl+Enter sends it to the card's session (SPEC §10, §20B.1).
fn follow_up_field(
    task_id: Ulid,
    session_id: Ulid,
    cards: &CardContext<'_>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Div {
    let draft = cards
        .follow_up
        .filter(|draft| draft.task_id == task_id && draft.session_id == session_id);
    let text = draft.map_or("", |draft| draft.text.text());
    let focused = draft.is_some() && cards.follow_up_focused;
    let field = text_field(
        "card-follow-up",
        text,
        "Follow up… (Ctrl+Enter sends)",
        focused,
        draft.is_some_and(|draft| draft.text.all_selected()),
        theme,
    )
    .bg(rgb(theme.surface))
    .max_h(px(theme::PROMPT_FIELD_HEIGHT))
    .overflow_y_scroll()
    .on_click(cx.listener(move |shell, _, window, cx| {
        cx.stop_propagation();
        shell.open_follow_up(task_id, session_id, window, cx);
    }))
    .when(draft.is_some(), |el| {
        el.track_focus(cards.follow_up_focus)
            .track_scroll(cards.follow_up_scroll)
            .on_key_down(cx.listener(Shell::follow_up_key_down))
            .relative()
            .child(typing_input(
                cards.follow_up_focus,
                Shell::follow_up_key_down,
                cards.ime_marked,
                cx,
            ))
    });
    let status = draft.and_then(|draft| {
        if draft.busy {
            Some(("↑", theme.text_muted, "Sending…".to_owned()))
        } else {
            draft
                .error
                .clone()
                .map(|error| ("✕", theme.status_exited, error))
        }
    });
    div()
        .flex()
        .flex_col()
        .gap(px(space::XS))
        .child(field)
        .children(status.map(|(icon, color, text)| {
            div()
                .flex()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(color)).child(icon))
                .child(div().text_color(rgb(theme.text)).child(text))
        }))
}

/// A conflict warning (SPEC §11, M5): ⚠ and the first file edited by several sessions, every
/// file on hover; "Dismiss" (shown on hover) clears it on every card and pane showing it.
fn conflict_badge(
    id: String,
    paths: &[&str],
    theme: &Theme,
    dismiss: impl Fn(&mut Shell) + 'static,
    cx: &mut Context<'_, Shell>,
) -> gpui::Stateful<gpui::Div> {
    let group = SharedString::from(format!("conflict-{id}"));
    let hover = theme.border;
    div()
        .id(group.clone())
        .group(group.clone())
        .flex()
        .items_center()
        .gap(px(space::XS))
        .min_w_0()
        .text_size(font_size::META)
        .tooltip(tooltip(theme, &conflicts::hover(paths), ""))
        .child(
            div()
                .flex_none()
                .text_color(rgb(theme.status_needs_input))
                .child(conflicts::ICON),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .text_color(rgb(theme.text))
                .child(format!("Conflict: {}", conflicts::label(paths))),
        )
        .child(
            div()
                .id(SharedString::from(format!("conflict-dismiss-{id}")))
                .flex_none()
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .cursor_pointer()
                .text_color(rgb(theme.text_muted))
                .opacity(opacity::HIDDEN)
                .group_hover(group, |style| style.opacity(opacity::FULL))
                .hover(move |style| style.bg(rgb(hover)))
                .active(|style| style.opacity(opacity::ACTIVE))
                .tooltip(tooltip(
                    theme,
                    "Hide this warning on every card and pane",
                    "",
                ))
                .on_click(cx.listener(move |shell, _, _, cx| {
                    // Not a click on the card (which would open its terminal).
                    cx.stop_propagation();
                    dismiss(shell);
                    cx.notify();
                }))
                .child("Dismiss"),
        )
}

/// The status bar's context meter: a thin track filled to the share, then `ctx 62%`. From
/// 80 % (`ContextShare::warning`) the fill and the text turn amber, as in the pane header.
fn context_meter(share: &ContextShare, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    let color = if share.warning {
        theme.status_needs_input
    } else {
        theme.accent
    };
    div()
        .id("status-context")
        .flex_none()
        .flex()
        .items_center()
        .gap(px(space::XS))
        .text_color(rgb(if share.warning {
            theme.status_needs_input
        } else {
            theme.text_muted
        }))
        .tooltip(tooltip(theme, &share.hover, ""))
        .child(
            div()
                .w(theme::rems_px(theme::STATUS_BAR_METER_WIDTH))
                .h(px(theme::STATUS_BAR_METER_HEIGHT))
                .rounded_full()
                .bg(rgb(theme.border))
                .child(
                    div()
                        .h_full()
                        .w(relative(f32::from(share.fill_percent) / 100.0))
                        .rounded_full()
                        .bg(rgb(color)),
                ),
        )
        .child(share.label.clone())
}

/// A pane's header (SPEC §11): the agent badge, the `<workspace> › <folder> › <agent>`
/// breadcrumb, one status pill (status mark + `Working · 1h 25m`, with the last turn's summary
/// when done; hovering it shows the status source and last change), the title (renamed inline
/// with F2 or a double-click, ADR 0031), an optional conflict warning, the number of queued
/// sends (§20B.2), the context share (`⚠` and amber from 80 %, §20E.3), and on the right an
/// agent's "Follow in Zed" and "Turns…" (§20C.5, §20C.2). A colored pane (ADR 0037) tints its
/// chip and draws an accent line under the header.
#[allow(clippy::too_many_arguments)]
fn pane_header(
    session: &Session,
    badge: String,
    breadcrumb: String,
    pill: String,
    title: gpui::AnyElement,
    context: Option<ContextShare>,
    conflict: Option<gpui::Stateful<gpui::Div>>,
    turns: Option<gpui::Stateful<gpui::Div>>,
    queued: usize,
    color: Option<u32>,
    now: i64,
    theme: &Theme,
) -> gpui::Div {
    let (state, changed_at) = (session.state, session.state_changed_at);
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .py(px(space::XS))
        // Transparent without a color, so the header keeps its height.
        .border_b(px(theme::PANE_ACCENT_WIDTH))
        .border_color(match color {
            Some(color) => Hsla::from(rgb(color)),
            None => gpui::transparent_black(),
        })
        .font_family(theme.ui_font_family.clone())
        .text_size(font_size::META)
        .child(agent_chip(badge, theme).when_some(color, |chip, color| {
            chip.bg(theme.tint(color)).border_color(rgb(color))
        }))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text))
                .child(breadcrumb),
        )
        .child(
            div()
                .id(SharedString::from(format!("pane-status-{}", session.id)))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(space::XS))
                .px(px(space::S))
                .rounded_full()
                .bg(rgb(theme.surface_raised))
                .text_color(rgb(theme.text_muted))
                .tooltip(status_tooltip(
                    theme,
                    state,
                    session.state_source,
                    changed_at,
                ))
                .child(status_mark(state, now, theme))
                .child(pill),
        )
        .child(title)
        .children(conflict.map(|badge| {
            badge
                .flex_none()
                .max_w(relative(theme::PANE_CONFLICT_MAX_FRACTION))
        }))
        .when(queued > 0, |el| {
            el.child(
                div()
                    .flex_none()
                    .text_color(rgb(theme.text_muted))
                    .child(format!("⧗ {queued} queued")),
            )
        })
        .children(context.map(|share| {
            div()
                .id(SharedString::from(format!("pane-context-{}", session.id)))
                .flex_none()
                .text_color(rgb(if share.warning {
                    theme.status_needs_input
                } else {
                    theme.text_muted
                }))
                .tooltip(tooltip(theme, &share.hover, ""))
                .child(if share.warning {
                    format!("⚠ {}", share.label)
                } else {
                    share.label
                })
        }))
        .children(turns)
}

/// A session's status mark (SPEC §11): three dots taking turns while working, else the glyph.
/// Fixed width, so rows with different states line up.
fn status_mark(state: SessionState, now: i64, theme: &Theme) -> gpui::AnyElement {
    let color = rgb(sidebar::status_color(theme, state));
    let mark = div()
        .flex_none()
        .w(theme::rems_px(theme::STATUS_MARK_WIDTH))
        .flex()
        .items_center()
        .justify_center();
    if state != SessionState::Working {
        return mark
            .text_color(color)
            .child(status::icon(state))
            .into_any_element();
    }
    let lit = status::working_dot(now);
    mark.gap(theme::rems_px(theme::STATUS_DOT_GAP))
        .children((0..status::WORKING_DOTS).map(|i| {
            div()
                .size(theme::rems_px(theme::STATUS_DOT))
                .rounded_full()
                .bg(color)
                .opacity(if i == lit {
                    opacity::FULL
                } else {
                    opacity::STATUS_DOT_DIM
                })
        }))
        .into_any_element()
}

/// The Mkrew logo glyph for the current appearance (`icon/mkrew-1-lanes/glyph-*.svg`): the dark
/// accent on dark, the light accent on light.
fn logo(theme: &Theme) -> gpui::Img {
    static DARK: LazyLock<Arc<gpui::Image>> = LazyLock::new(|| {
        Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Svg,
            include_bytes!("../../../icon/mkrew-1-lanes/glyph-dark.svg").to_vec(),
        ))
    });
    static LIGHT: LazyLock<Arc<gpui::Image>> = LazyLock::new(|| {
        Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Svg,
            include_bytes!("../../../icon/mkrew-1-lanes/glyph-light.svg").to_vec(),
        ))
    });
    let image = if theme.dark { &DARK } else { &LIGHT };
    gpui::img(Arc::clone(image))
        .flex_none()
        .size(theme::rems_px(theme::LOGO_SIZE))
}

/// An agent badge (ADR 0025) as a small square chip, so glyphs and letters line up and never
/// read as part of the name.
fn agent_chip(badge: String, theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .size(theme::rems_px(theme::AGENT_CHIP_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::SMALL))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.border))
        .text_size(font_size::META)
        .font_weight(FontWeight(weight::SEMIBOLD))
        .text_color(rgb(theme.text))
        .child(badge)
}

/// Tooltip of a status: its source and how long ago it changed, aged when it opens (a
/// done or needs-input status is not redrawn while it sits there).
fn status_tooltip(
    theme: &Theme,
    state: SessionState,
    source: StateSource,
    changed_at: i64,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let style = PopupStyle::of(theme);
    move |_, cx| {
        let text = status::hover(state, source, changed_at, status::now_ms()).into();
        let style = style.clone();
        cx.new(|_| Tooltip {
            style,
            text,
            keys: SharedString::default(),
        })
        .into()
    }
}

/// Builds a tooltip showing `text` and, if any, its keyboard shortcut.
fn tooltip(
    theme: &Theme,
    text: &str,
    keys: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let style = PopupStyle::of(theme);
    let text = SharedString::from(text.to_owned());
    let keys = keys.into();
    move |_, cx| {
        let (style, text, keys) = (style.clone(), text.clone(), keys.clone());
        cx.new(|_| Tooltip { style, text, keys }).into()
    }
}

/// The theme tokens a tooltip or drag preview draws with: these closures are built for every
/// hoverable element on every frame, so they carry this instead of a whole [`Theme`].
#[derive(Clone)]
struct PopupStyle {
    border: u32,
    surface_raised: u32,
    text: u32,
    text_muted: u32,
    ui_font_family: SharedString,
}

impl PopupStyle {
    fn of(theme: &Theme) -> Self {
        Self {
            border: theme.border,
            surface_raised: theme.surface_raised,
            text: theme.text,
            text_muted: theme.text_muted,
            ui_font_family: theme.ui_font_family.clone(),
        }
    }
}

/// What follows the pointer while a card is dragged: a lifted copy of its title.
struct CardPreview {
    style: PopupStyle,
    title: SharedString,
}

impl Render for CardPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = &self.style;
        div()
            .w(px(theme::BOARD_COLUMN_MIN_WIDTH))
            .p(px(space::M))
            .rounded(px(radius::MEDIUM))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface_raised))
            .shadow_sm()
            .opacity(opacity::DRAGGING)
            .font_family(theme.ui_font_family.clone())
            .text_size(font_size::CARD_TITLE)
            .font_weight(FontWeight(weight::MEDIUM))
            .text_color(rgb(theme.text))
            .child(self.title.clone())
    }
}

struct Tooltip {
    style: PopupStyle,
    text: SharedString,
    keys: SharedString,
}

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = &self.style;
        div()
            .flex()
            .gap(px(space::S))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(radius::SMALL))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface_raised))
            .shadow_sm()
            .font_family(theme.ui_font_family.clone())
            .text_size(font_size::META)
            .text_color(rgb(theme.text))
            .child(self.text.clone())
            .when(!self.keys.is_empty(), |el| {
                el.child(
                    div()
                        .text_color(rgb(theme.text_muted))
                        .child(self.keys.clone()),
                )
            })
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
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
        // the Inbox): every state change and window activation re-renders.
        if window.is_window_active()
            && let Some(id) = self.focused_session()
            && let Some(session) = self.sidebar.sessions().iter().find(|s| s.id == id)
        {
            self.viewed.view(session);
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .track_focus(&self.focus_handle)
            .when(self.view() == View::Board, |el| {
                el.key_context(BOARD_CONTEXT)
            })
            .when(self.view() == View::Docs, |el| el.key_context(DOCS_CONTEXT))
            .when(self.view() == View::Files, |el| {
                el.key_context(FILES_CONTEXT)
            })
            .on_action(cx.listener(Self::run))
            .child(self.render_title_bar(self.view(), &theme, window, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .relative()
                    // Hidden: not built at all (nothing to lay out or paint).
                    .children((!self.sidebar_layout.hidden()).then(|| {
                        if self.sidebar_layout.compact() {
                            self.render_compact_sidebar(now, &theme, cx)
                                .into_any_element()
                        } else {
                            self.render_sidebar(now, &theme, window, cx)
                                .into_any_element()
                        }
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
                        self.pane_menu
                            .as_ref()
                            .filter(|_| self.view() == View::Terminals && !self.modal_open())
                            .and_then(|menu| self.render_pane_menu(menu, &theme, cx)),
                    )
                    .children(
                        (self.file_menu.as_ref())
                            .filter(|menu| {
                                self.workspace == Some(menu.project_id)
                                    && self.view() == View::Files
                                    && !self.modal_open()
                            })
                            .map(|menu| self.render_file_menu(menu, &theme, cx)),
                    ),
            )
            .child(self.render_status_bar(now, &theme, cx))
    }
}
