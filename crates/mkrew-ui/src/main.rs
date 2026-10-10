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
use mkrew_ui::scrollbar;
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

mod shell;
use shell::cards::*;
use shell::files_render::*;
use shell::git_diff::*;
use shell::git_rows::*;
use shell::menus::*;
use shell::pane_render::*;
use shell::sidebar::*;
use shell::widgets::*;

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
    /// Where the Files editor's scroll bar thumb is held while it is dragged.
    file_code_drag: scrollbar::Drag,
    /// The Files editor (ADR 0028): typing goes there while it has focus.
    file_editor_focus: FocusHandle,
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
                    let due = board_due || shell.starting_pane.is_some();
                    if window.is_visible() && (due || status::redraw_due(sessions, last, now)) {
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
            starting_pane: None,
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
            doc_page: preview::Page::default(),
            file_page: preview::Page::default(),
            file_code_scroll: UniformListScrollHandle::new(),
            file_code_drag: scrollbar::Drag::default(),
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
        if self.starting_pane.is_some() || self.ask_for_default(project_id, kind, window, cx) {
            return;
        }
        self.starting_pane = Some(kind);
        cx.notify();
        let params = lifecycle::spawn_params(project_id, Some(tab_id), kind);
        let spawned = connection.call::<SessionSpawnResult>(Request::SessionSpawn(params));
        cx.spawn_in(window, async move |this, cx| {
            let spawned = spawned.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.starting_pane = None;
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

    /// `tab.close` for `tab_id` of the shown workspace (the active one, or any tab's ✕). The
    /// daemon removes the sessions of its panes too (ADR 0041), so a running agent among them
    /// asks first; closing the last tab leaves a fresh empty one.
    fn close_tab(&mut self, tab_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(project_id) = self.workspace else {
            return;
        };
        let sessions = self
            .tabs
            .get(&project_id)
            .and_then(|tabs| tabs.tab(tab_id))
            .map(Tab::sessions)
            .unwrap_or_default();
        let running_agent = sessions.iter().any(|id| {
            let known = self.sidebar.sessions().iter().find(|s| s.id == *id);
            lifecycle::close_action(known) == CloseAction::Ask
        });
        let answer = running_agent.then(|| {
            window.prompt(
                PromptLevel::Warning,
                "This tab has a running agent.",
                Some("Closing the tab stops its sessions and removes them from the sidebar."),
                &workspace_menu::REMOVE_ANSWERS,
                cx,
            )
        });
        self.tab_error = None;
        cx.spawn_in(window, async move |this, cx| {
            if let Some(answer) = answer
                && !task_dialog::confirms_delete(answer.await.ok())
            {
                return;
            }
            let closed = connection
                .call::<()>(Request::TabClose(TabRef { tab_id }))
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                match closed {
                    Ok(()) => {
                        for session in &sessions {
                            shell.sidebar.remove_session(*session);
                        }
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
            .child(self.render_title_bar(self.view(), now, &theme, window, cx))
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
