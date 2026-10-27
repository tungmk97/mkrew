//! Design tokens (SPEC §14). Every color, spacing, radius, font size and duration used by the
//! UI comes from here; components never hard-code values. Colors are `0xRRGGBB`.

use std::path::Path;
use std::time::Duration;

use gpui::{App, Rems, SharedString, Window, WindowAppearance, rems};
use mkrew_core::settings::Settings;
pub use mkrew_core::settings::ThemeMode;
use mkrew_protocol::WORKSPACE_COLORS;

/// Spacing scale in px: 4 · 8 · 12 · 16 · 24 · 32.
pub mod space {
    pub const XS: f32 = 4.0;
    pub const S: f32 = 8.0;
    /// Card padding.
    pub const M: f32 = 12.0;
    /// Main padding (small).
    pub const L: f32 = 16.0;
    /// Main padding (large).
    pub const XL: f32 = 24.0;
    pub const XXL: f32 = 32.0;
}

/// Corner radius in px (redesign: 6 chips · 8 buttons, inputs, rows · 10 cards · 999 pills).
pub mod radius {
    /// Badges, small buttons.
    pub const SMALL: f32 = 4.0;
    /// Area chips and key hints.
    pub const TAG: f32 = 5.0;
    /// Chips, segments, tree rows.
    pub const CHIP: f32 = 6.0;
    /// Cards, panes, inputs, buttons, rows.
    pub const MEDIUM: f32 = 8.0;
    /// The segmented control's container.
    pub const SEGMENTED: f32 = 9.0;
    /// Cards and framed panels (the Git diff, the branch card).
    pub const CARD: f32 = 10.0;
    /// Dialogs, palette, empty-state tile.
    pub const LARGE: f32 = 12.0;
    /// Pills: fully round ends.
    pub const PILL: f32 = 999.0;
}

/// UI font sizes relative to body text (1 rem = `[ui] font_size`, default 13 px, set per
/// frame with `Window::set_rem_size`): 12 meta, 13 body, 15 card title, 20 view title at the
/// default (SPEC §14, ADR 0024).
pub mod font_size {
    use gpui::{Rems, rems};

    /// Small caps overlines (STAGED, CHANGES) and area chips / key hints.
    pub const OVERLINE: Rems = rems(11.0 / 13.0);
    pub const META: Rems = rems(12.0 / 13.0);
    pub const BODY: Rems = rems(1.0);
    pub const CARD_TITLE: Rems = rems(15.0 / 13.0);
    pub const VIEW_TITLE: Rems = rems(20.0 / 13.0);
}

/// UI font weights (400/500/600 only).
pub mod weight {
    pub const REGULAR: f32 = 400.0;
    pub const MEDIUM: f32 = 500.0;
    pub const SEMIBOLD: f32 = 600.0;
}

/// Motion: panels, view switches, drag & drop (120–180 ms ease-out).
pub mod motion {
    use std::time::Duration;

    pub const FAST: Duration = Duration::from_millis(120);
    pub const SLOW: Duration = Duration::from_millis(180);
}

/// In-app toasts auto-hide after this long (expired on the [`STATUS_TICK`], so up to one tick
/// later).
pub const TOAST_DURATION: Duration = Duration::from_secs(4);

/// Width of a toast card (px at the default 13 px body, drawn with [`rems_px`]) and the lines
/// its text shows before it is cut.
pub const TOAST_WIDTH: f32 = 320.0;
pub const TOAST_MAX_LINES: usize = 4;

/// Status refresh period: one step of the `working` dots and the elapsed/idle time update
/// (SPEC §11, §14).
pub const STATUS_TICK: Duration = Duration::from_millis(500);

/// The Phone access poll clock (ADR 0053): while its dialog is open `daemon.status` and
/// Tailscale are read on every tick; closed, `daemon.status` alone every [`PHONE_POLL_IDLE`]
/// (it keeps the title bar icon true).
pub const PHONE_TICK: Duration = Duration::from_secs(1);
pub const PHONE_POLL_IDLE: Duration = Duration::from_secs(5);

/// Diameter of one dot of the `working` mark (SPEC §11).
pub const STATUS_DOT: f32 = 4.0;
/// Gap between the dots of the `working` mark.
pub const STATUS_DOT_GAP: f32 = 2.0;
/// Width of every status mark, so glyph and dot rows line up (3 × 4 + 2 × 2 = 16). These three
/// are px at the default 13 px body and scale with `[ui] font_size` ([`rems_px`]).
pub const STATUS_MARK_WIDTH: f32 = space::L;
/// The sidebar's narrower mark slot: one blinking dot while working, else the glyph.
pub const SIDEBAR_MARK_WIDTH: f32 = space::S;

/// Voice input level meter (§20G): bars this wide and this far apart, from `MIN` to `MAX` tall
/// (px at the default 13 px body, [`rems_px`]), redrawn this often while recording.
pub const VOICE_BAR_WIDTH: f32 = 2.0;
pub const VOICE_BAR_GAP: f32 = 2.0;
pub const VOICE_BAR_MIN: f32 = 2.0;
pub const VOICE_BAR_MAX: f32 = 12.0;
pub const VOICE_METER_TICK: Duration = Duration::from_millis(60);

/// Narrowest the time column on the right of a session row gets (fits `1h 5m`), so times and
/// titles line up.
pub const SESSION_TIME_WIDTH: f32 = space::XXL;

/// A square icon-only button (theme toggle, sidebar +).
pub const ICON_BUTTON_SIZE: f32 = 26.0;
/// The redesign's controls: icon buttons and buttons are this tall (and icon buttons square).
pub const CONTROL_HEIGHT: f32 = 30.0;
/// A text input (search fields), and the bordered sidebar icon buttons beside one.
pub const INPUT_HEIGHT: f32 = 32.0;
/// One segment of a segmented control.
pub const SEGMENT_HEIGHT: f32 = 26.0;
/// A status pill (Idle, In Progress) and an area chip / Git `M` badge.
pub const PILL_HEIGHT: f32 = 22.0;
pub const CHIP_HEIGHT: f32 = 20.0;
/// A Lucide icon in an icon-only button ([`ICON_BUTTON_SIZE`] square).
pub const ICON_SIZE: f32 = 16.0;
/// A Lucide icon among body / meta text: text buttons, row actions, row chevrons, window
/// controls.
pub const ICON_SIZE_SMALL: f32 = 14.0;
/// The pin before the time of a pinned session row.
pub const PIN_ICON_SIZE: f32 = 13.0;
/// Inset of a segmented control around its segments.
pub const SEGMENT_PADDING: f32 = 3.0;
/// The thin vertical rule before the window controls.
pub const TITLE_RULE_HEIGHT: f32 = 18.0;
/// The app logo tile at the left of the title bar.
pub const APP_LOGO_TILE: f32 = 22.0;
/// The bottom edge of a key hint (a key cap look).
pub const KBD_BOTTOM_BORDER: f32 = 2.0;
/// The Jira list (Board): a row's height, the key column, the toolbar's filter input and the
/// leading type icon (px at the default 13 px body, [`rems_px`]).
pub const BOARD_ROW_HEIGHT: f32 = 38.0;
pub const BOARD_KEY_WIDTH: f32 = 62.0;
pub const BOARD_FILTER_WIDTH: f32 = 240.0;
pub const BOARD_TYPE_ICON: f32 = 15.0;

/// Borders are 1 px and faint.
pub const BORDER_WIDTH: f32 = 1.0;

/// The OS UI font (Segoe UI Variable / SF / …); GPUI resolves this name per platform.
pub const UI_FONT: &str = ".SystemUIFont";
/// The design's UI font, used when installed and `[ui] font_family` is unset (not bundled:
/// a machine without it gets [`UI_FONT`]).
pub const PREFERRED_UI_FONT: &str = "Inter";

/// Font family list of Mermaid diagram text (an SVG font-family value; the first installed
/// one is used).
pub const DIAGRAM_FONT: &str = "Segoe UI, Helvetica Neue, DejaVu Sans, sans-serif";

/// Size of a new main window.
pub const WINDOW_WIDTH: f32 = 1200.0;
pub const WINDOW_HEIGHT: f32 = 760.0;
/// Smallest main window: the title bar's contents fit (ADR 0034).
pub const WINDOW_MIN_WIDTH: f32 = 1100.0;
pub const WINDOW_MIN_HEIGHT: f32 = 560.0;
/// One ─ □ ✕ button of the Windows title bar (the system caption button width).
pub const WINDOW_CONTROL_WIDTH: f32 = 46.0;
/// Room for the macOS traffic lights at the left of the title bar.
pub const MAC_TRAFFIC_LIGHTS_WIDTH: f32 = 72.0;
/// Where the macOS traffic lights sit ((x, y) in px).
pub const MAC_TRAFFIC_LIGHT_POS: (f32, f32) = (12.0, 14.0);
/// Longest workspace name / branch chip in the title bar before it truncates.
pub const TITLE_CHIP_MAX_WIDTH: f32 = 160.0;

/// Opacity of a hovered / pressed filled button, and of secondary text inside a button.
pub mod opacity {
    pub const HOVER: f32 = 0.9;
    pub const ACTIVE: f32 = 0.8;
    pub const SECONDARY: f32 = 0.8;
    /// A control that cannot be used right now (e.g. Save while saving).
    pub const DISABLED: f32 = 0.5;
    /// A changed-file row marked as viewed (SPEC §12B): dimmed, still readable.
    pub const VIEWED: f32 = 0.6;
    /// The unlit dots of the `working` mark.
    pub const STATUS_DOT_DIM: f32 = 0.35;
    /// Hover-revealed actions (SPEC §14 progressive disclosure): hidden, then fully shown.
    pub const HIDDEN: f32 = 0.0;
    pub const FULL: f32 = 1.0;
    /// The lifted copy of a card that follows the pointer while it is dragged.
    pub const DRAGGING: f32 = 0.9;
    /// A sidebar row or tab while it is dragged, left dim in its old place.
    pub const DRAG_SOURCE: f32 = 0.4;
}

/// Half the gap between split panes: each pane is inset by it, so panes sit `space::XS` apart
/// and `space::XS` from the tab's edges.
pub const PANE_HALF_GAP: f32 = space::XS / 2.0;

/// Thickness of the accent line under the header of a colored pane (ADR 0037).
pub const PANE_ACCENT_WIDTH: f32 = 2.0;

/// Thickness of the accent line marking where a dragged sidebar row or tab will land.
pub const DROP_LINE_WIDTH: f32 = 2.0;

/// Narrowest and widest the workspace sidebar gets when dragged; `[ui] sidebar_width` is
/// clamped to them.
pub const SIDEBAR_MIN_WIDTH: f32 = 200.0;
pub const SIDEBAR_MAX_WIDTH: f32 = 480.0;

/// Width of the compact sidebar rail (Toggle compact sidebar): a status mark, a gap and an agent
/// chip plus the rail's padding. In px at the default font size; drawn with [`rems_px`] so it
/// widens with `[ui] font_size` like the avatars and chips inside it.
pub const COMPACT_SIDEBAR_WIDTH: f32 = 56.0;

/// Narrowest and widest the session panel gets when dragged (SPEC §10); `[ui] inspector_width`
/// is clamped to them.
pub const INSPECTOR_MIN_WIDTH: f32 = 240.0;
pub const INSPECTOR_MAX_WIDTH: f32 = 560.0;

/// Width of the grab area of a draggable edge: the sidebar's, the session panel's and the
/// dividers between panes.
pub const GRIP_WIDTH: f32 = 6.0;

/// Width of the label column of the session card's usage rows (fits "Context" at META size).
pub const INSPECTOR_LABEL_WIDTH: f32 = 56.0;

/// Min height of a board column header row: the Todo header's `New task` button at the
/// default UI font size, so every column's cards start at the same height (ADR 0023).
pub const BOARD_COLUMN_HEADER_HEIGHT: f32 = 28.0;

/// Width of a popover menu (every `menu_panel` popover).
pub const MENU_WIDTH: f32 = 240.0;

/// The drawing of a layout in the split button's menu (SPEC §10).
pub const LAYOUT_THUMB_WIDTH: f32 = 24.0;
pub const LAYOUT_THUMB_HEIGHT: f32 = 16.0;

/// Width of the query field in a terminal's find bar.
pub const FIND_INPUT_WIDTH: f32 = 200.0;

/// Width of the Files editor's caret.
pub const CARET_WIDTH: f32 = 2.0;

/// The Files editor's change bar (SPEC §12A): its width, and the height of the mark where
/// lines were removed.
pub const GIT_GUTTER_BAR_WIDTH: f32 = 3.0;
pub const GIT_GUTTER_DELETED_HEIGHT: f32 = 6.0;

/// The scroll bar over the right edge of the Files editor and the Markdown preview: its
/// grab area, the thumb drawn in it (round ends of `radius::SMALL`), and the shortest thumb
/// (a 2 MB file still gets one you can grab).
pub const SCROLLBAR_WIDTH: f32 = space::M;
pub const SCROLLBAR_THUMB_WIDTH: f32 = space::S;
pub const SCROLLBAR_MIN_THUMB: f32 = space::XL;

/// The Files editor highlights its text again this long after the last keystroke.
pub const REHIGHLIGHT_DELAY: Duration = Duration::from_millis(150);

/// Minimum width of a tab title while it is renamed inline.
pub const TAB_RENAME_MIN_WIDTH: f32 = 120.0;

/// Narrowest a board column gets; below that the board scrolls sideways.
pub const BOARD_COLUMN_MIN_WIDTH: f32 = 200.0;

/// Thickness of the accent line that marks where a dragged card lands.
pub const DROP_INDICATOR_HEIGHT: f32 = 2.0;

/// Width of a modal dialog (the task dialog).
pub const DIALOG_WIDTH: f32 = 520.0;

/// The Settings dialog (ADR 0073): its size, the page list width and the value column.
pub const SETTINGS_DIALOG_WIDTH: f32 = 760.0;
pub const SETTINGS_DIALOG_HEIGHT: f32 = 560.0;
pub const SETTINGS_NAV_WIDTH: f32 = 180.0;
pub const SETTINGS_CONTROL_WIDTH: f32 = 240.0;
/// Tallest the font pick's list grows (it scrolls past that).
pub const SETTINGS_FONT_LIST_HEIGHT: f32 = 200.0;
/// An on/off switch: track size and the knob's inset.
pub const SWITCH_WIDTH: f32 = 32.0;
pub const SWITCH_HEIGHT: f32 = 18.0;
pub const SWITCH_INSET: f32 = 2.0;

/// Side of the Phone access QR code (ADR 0053): px at the default 13 px body, scaled with
/// `[ui] font_size` ([`rems_px`]). Big enough for a camera across a desk.
pub const PHONE_QR_SIZE: f32 = 216.0;

/// Width of a confirmation dialog (every `window.prompt`, drawn in the window).
pub const CONFIRM_DIALOG_WIDTH: f32 = 420.0;

/// Height of the Run setup dialog's command list before it scrolls.
pub const RUN_SETUP_MAX_HEIGHT: f32 = 420.0;
/// Widest a folded Run setup card's name or folder gets before it is cut with `…`.
pub const RUN_NAME_MAX_WIDTH: f32 = 140.0;

/// Height of the prompt field in the task dialog (it scrolls past that).
pub const PROMPT_FIELD_HEIGHT: f32 = 140.0;

/// Tallest an agent pane's prompt composer grows (it scrolls past that, ~8 lines; §20B.1).
pub const COMPOSER_MAX_HEIGHT: f32 = 160.0;

/// Tallest the composer's list of queued texts grows (it scrolls past that, ~4 rows; §20B.2).
pub const COMPOSER_QUEUE_MAX_HEIGHT: f32 = 88.0;

/// Width of the "Undo task" dialog (wider: it shows a diff).
pub const UNDO_DIALOG_WIDTH: f32 = 720.0;

/// Heights of the "Undo task" dialog's file list and diff (each scrolls past that).
pub const UNDO_FILES_HEIGHT: f32 = 120.0;
pub const UNDO_DIFF_HEIGHT: f32 = 280.0;

/// Width of the fan-out "Compare" dialog: up to five child columns side by side (§20E.2).
pub const COMPARE_DIALOG_WIDTH: f32 = 880.0;

/// Width of the "Turns" dialog, and the height its list scrolls past (~8 turns).
pub const TURNS_DIALOG_WIDTH: f32 = 560.0;
pub const TURNS_LIST_HEIGHT: f32 = 400.0;

/// Width of the "Ask AI" dialog, and the height its answer scrolls past (ADR 0061).
pub const ASK_DIALOG_WIDTH: f32 = 640.0;
pub const ASK_ANSWER_HEIGHT: f32 = 360.0;

/// Share of the window's width and height the "History" dialog takes (it replays a terminal).
pub const HISTORY_DIALOG_FRACTION: f32 = 0.9;

/// Width of the "History" dialog's session list.
pub const HISTORY_LIST_WIDTH: f32 = 280.0;

/// Widest share of a pane header a conflict warning may take (it ellipsizes past that).
pub const PANE_CONFLICT_MAX_FRACTION: f32 = 0.5;

/// Width of the Docs view's file list.
pub const DOC_LIST_WIDTH: f32 = 240.0;

/// Max width of the rendered Markdown preview, so lines stay readable.
pub const DOC_PREVIEW_MAX_WIDTH: f32 = 720.0;

/// Width of the bar left of a Markdown quote.
pub const DOC_QUOTE_BAR_WIDTH: f32 = 2.0;

/// Width of the accent bar left of diff lines picked for a comment.
pub const DIFF_PICK_BAR_WIDTH: f32 = 2.0;

/// Width of the Docs view's comment margin (right of the preview / source).
pub const DOC_MARGIN_WIDTH: f32 = 300.0;

/// Height of the "Send to agent" session list (it scrolls past that).
pub const SEND_SESSIONS_HEIGHT: f32 = 200.0;

/// Height of the Docs "View changes" diff (it scrolls past that); the dialog is as wide as
/// the undo one.
pub const CHANGES_DIFF_HEIGHT: f32 = 420.0;

/// Width of the Files view's tree.
pub const FILE_TREE_WIDTH: f32 = 280.0;

/// Indent per folder level in the Files tree.
pub const FILE_TREE_INDENT: f32 = space::M;

/// Side of the dot on a Files tree folder holding changes.
pub const FILE_CHANGES_DOT_SIZE: f32 = 6.0;

/// Height of a Files tree row (the tree is a uniform list: every row is this high).
pub const TREE_ROW_HEIGHT: f32 = 28.0;

/// Folder / file icon in a Files tree row.
pub const TREE_ICON_SIZE: f32 = 15.0;

/// Chevron (and the spacer that aligns file rows with it) in a Files tree row.
pub const TREE_CHEVRON_SIZE: f32 = 12.0;

/// The large empty-state tile (Files "No file open").
pub const EMPTY_TILE_LARGE: f32 = 48.0;

/// Side of an agent badge chip (ADR 0025), like the design's workspace avatar.
pub const AGENT_CHIP_SIZE: f32 = 18.0;
/// Side of an agent logo inside its chip (ADR 0058): a little under [`ICON_SIZE_SMALL`], so the
/// solid marks do not crowd the row text.
pub const AGENT_LOGO_SIZE: f32 = 12.0;
/// The fold chevron of a sidebar group label (TERMINAL / RUN): under the row
/// chevrons, so the small caps label leads.
pub const GROUP_CHEVRON_SIZE: f32 = 12.0;

/// A color choice in the workspace menu.
pub const WORKSPACE_SWATCH_SIZE: f32 = space::L;

/// Ring around a workspace menu color choice.
pub const WORKSPACE_SWATCH_BORDER: f32 = 2.0;

/// Height of the quick open match list (it scrolls past that).
pub const QUICK_OPEN_LIST_HEIGHT: f32 = 360.0;

/// Width of the Git view's side panel: header, changed-file list, commit box (right of the diff).
pub const GIT_FILE_LIST_WIDTH: f32 = 320.0;

/// Tallest the Git view's commit message box grows (it scrolls past that, ~8 lines).
pub const GIT_COMMIT_BOX_MAX_HEIGHT: f32 = 160.0;

/// One row of the Git changed-file list (uniform list: every row has this box).
pub const GIT_FILE_ROW_HEIGHT: f32 = 34.0;
/// The change-letter badge (M, A, D …) of a Git file row.
pub const GIT_BADGE_SIZE: f32 = 18.0;

/// The History's lane graph (SPEC §12B): one lane's width, a commit's dot, a line's width.
pub const GIT_GRAPH_LANE_WIDTH: f32 = 12.0;
pub const GIT_GRAPH_DOT_SIZE: f32 = 8.0;
pub const GIT_GRAPH_LINE_WIDTH: f32 = 2.0;

/// The Git diff toolbar's height while it fits on one line (it wraps when narrow).
pub const GIT_DIFF_TOOLBAR_HEIGHT: f32 = 44.0;

/// The hatch of a Split cell with no line on that side: stroke width and spacing in px.
pub const DIFF_HATCH_WIDTH: f32 = 1.0;
pub const DIFF_HATCH_INTERVAL: f32 = 6.0;

/// How far above and below the Git diff's visible rows the list lays rows out ahead, px, so
/// scrolling a wrapped diff does not pop.
pub const GIT_DIFF_OVERDRAW: f32 = 200.0;

/// Git diff lines: monospace size and line height, px (a wrapped line is a multiple of it).
pub const DIFF_FONT_SIZE: f32 = 12.5;
pub const DIFF_LINE_HEIGHT: f32 = 24.0;
/// The line-number gutter of a diff half: its width in px (it grows for 5+ digit numbers at
/// [`DIFF_DIGIT_WIDTH`] per digit) and the sign column after it.
pub const DIFF_GUTTER_WIDTH: f32 = 40.0;
pub const DIFF_DIGIT_WIDTH: f32 = 8.0;
pub const DIFF_SIGN_WIDTH: f32 = space::L;
/// A diff hunk header bar's height.
pub const GIT_HUNK_HEADER_HEIGHT: f32 = 34.0;

/// Rough width of one char of a Git repository tab label, px: the repo strip shows every tab
/// only when their labels fit.
pub const GIT_REPO_TAB_CHAR_WIDTH: f32 = 7.5;
/// The text size of a Git repository tab (12.5 px at the default 13 px body).
pub const GIT_REPO_TAB_TEXT: Rems = rems(12.5 / 13.0);

/// Widest the Git filter row's "By: … ▾" button gets before its session name is cut.
pub const GIT_CHANGED_BY_MAX_WIDTH: f32 = 140.0;

/// Side of a task list item's check box.
pub const DOC_TASK_BOX_SIZE: f32 = space::M;

/// Docs preview typography (SPEC §12, ADR 0021): long-form reading, a step above the UI
/// scale.
pub mod doc {
    pub const BODY: f32 = 14.0;
    pub const CODE: f32 = 13.0;
    /// h1–h6.
    pub const HEADINGS: [f32; 6] = [24.0, 20.0, 17.0, 15.0, 14.0, 14.0];
    /// Line heights as multiples of the font size.
    pub const LINE_HEIGHT: f32 = 1.6;
    pub const HEADING_LINE_HEIGHT: f32 = 1.3;
    pub const CODE_LINE_HEIGHT: f32 = 1.45;
    /// The marker column of list items (bullets, numbers, check boxes line up).
    pub const MARKER_WIDTH: f32 = 20.0;
}

/// Width of the peek popover next to a session row (§20A.1).
pub const PEEK_WIDTH: f32 = 480.0;

/// Widest a broadcast row's result ("failed: …") gets before it is cut (§20A.6); the full
/// text is in its tooltip.
pub const BROADCAST_RESULT_WIDTH: f32 = 200.0;

/// How long the mouse rests on a session row before it peeks (§20A.1).
pub const PEEK_HOVER_DELAY: Duration = Duration::from_millis(300);

/// How long a hover peek stays after the mouse left its row, so it can reach the popover.
pub const PEEK_LEAVE_GRACE: Duration = Duration::from_millis(150);

/// Max width of free-running text blocks (messages, details) so lines stay readable.
pub const READABLE_WIDTH: f32 = 560.0;

/// Terminal and code font families by preference; the first one installed is used (SPEC §14).
#[cfg(windows)]
pub const TERMINAL_FONTS: &[&str] = &[
    "JetBrains Mono",
    "Cascadia Code",
    "Cascadia Mono",
    "Consolas",
];
#[cfg(target_os = "macos")]
pub const TERMINAL_FONTS: &[&str] = &["JetBrains Mono", "SF Mono", "Menlo"];
#[cfg(not(any(windows, target_os = "macos")))]
pub const TERMINAL_FONTS: &[&str] = &["JetBrains Mono", "DejaVu Sans Mono"];

/// Per-glyph fallbacks of the terminal font, tried before the system fallback. Nerd Font
/// icons (private-use code points, used by omp and shell prompts) are in no system font.
/// Missing families are skipped; "Mono" variants keep icons one cell wide.
pub const TERMINAL_GLYPH_FALLBACKS: &[&str] = &[
    "Symbols Nerd Font Mono",
    "CaskaydiaCove Nerd Font Mono",
    "CaskaydiaMono Nerd Font Mono",
    "JetBrainsMono Nerd Font Mono",
    "MesloLGS Nerd Font Mono",
    "MesloLGM Nerd Font Mono",
    "Hack Nerd Font Mono",
    "FiraCode Nerd Font Mono",
];

/// The first of `preferred` that is `installed`, else the first of `preferred`.
pub fn pick_family(preferred: &[&'static str], installed: &[String]) -> &'static str {
    preferred
        .iter()
        .copied()
        .find(|family| installed.iter().any(|name| name == family))
        .or_else(|| preferred.first().copied())
        .unwrap_or_default()
}

/// The fonts of `settings.toml` (SPEC §20.0), as a GPUI global read by every [`Theme`].
#[derive(Clone, Debug, PartialEq)]
pub struct Fonts {
    pub ui_family: SharedString,
    pub ui_size: f32,
    pub terminal_family: SharedString,
    pub terminal_size: f32,
}

impl gpui::Global for Fonts {}

impl Fonts {
    /// `installed` = `cx.text_system().all_font_names()`. A terminal family that is not
    /// installed would silently become a proportional UI font, so it falls back to the list.
    pub fn from_settings(settings: &Settings, installed: &[String]) -> Self {
        let terminal = &settings.terminal.font_family;
        let terminal_family = if !terminal.is_empty() && installed.iter().any(|n| n == terminal) {
            SharedString::from(terminal.clone())
        } else {
            pick_family(TERMINAL_FONTS, installed).into()
        };
        let ui_family = if settings.ui.font_family.is_empty() {
            if installed.iter().any(|name| name == PREFERRED_UI_FONT) {
                PREFERRED_UI_FONT.into()
            } else {
                UI_FONT.into()
            }
        } else {
            settings.ui.font_family.clone().into()
        };
        Self {
            ui_family,
            ui_size: settings.ui.font_size.0.into(),
            terminal_family,
            terminal_size: settings.terminal.font_size.0.into(),
        }
    }
}

/// `[ui] light_theme` / `dark_theme` (ADR 0078), as a GPUI global read by [`refresh`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeNames {
    pub light: String,
    pub dark: String,
}

impl gpui::Global for ThemeNames {}

impl ThemeNames {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            light: settings.ui.light_theme.clone(),
            dark: settings.ui.dark_theme.clone(),
        }
    }

    /// The name in the dark (`true`) or light slot.
    pub fn slot(&self, dark: bool) -> &str {
        if dark { &self.dark } else { &self.light }
    }
}

/// Which slot `mode` shows: `System` follows the OS.
pub fn slot_is_dark(mode: ThemeMode, os_dark: bool) -> bool {
    match mode {
        ThemeMode::System => os_dark,
        ThemeMode::Light => false,
        ThemeMode::Dark => true,
    }
}

/// The `[ui]` key of a theme slot.
pub fn slot_key(dark: bool) -> &'static str {
    if dark { "dark_theme" } else { "light_theme" }
}

/// Whether the OS shows `window` dark.
pub fn os_dark(window: &Window) -> bool {
    matches!(
        window.appearance(),
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

/// The fonts, theme mode and theme names of `path` when any differs from `last` / `mode` /
/// `names` (`Ok(None)` when unchanged). A broken file is an `Err` with `file:line:col`
/// (§20.0); the caller keeps the last good settings.
pub fn reload(
    path: &Path,
    installed: &[String],
    last: &Fonts,
    mode: ThemeMode,
    names: &ThemeNames,
) -> Result<Option<(Fonts, ThemeMode, ThemeNames)>, String> {
    let settings = mkrew_core::settings::load(path).map_err(|error| error.to_string())?;
    let fonts = Fonts::from_settings(&settings, installed);
    let new_names = ThemeNames::from_settings(&settings);
    let changed = fonts != *last || settings.ui.theme != mode || new_names != *names;
    Ok(changed.then_some((fonts, settings.ui.theme, new_names)))
}

/// Color tokens for one appearance (dark or light), plus the terminal settings. A GPUI global
/// (ADR 0027): views read it with [`ActiveTheme::theme`].
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    /// The main area behind panes and views.
    pub bg: u32,
    /// Panels, popovers, inputs.
    pub surface: u32,
    /// A raised control on a surface (selected segment, selected code line).
    pub surface_raised: u32,
    /// The workspace sidebar, a shade off `bg`.
    pub sidebar: u32,
    /// The title bar and the Terminals tab bar (the window chrome around the panes).
    pub chrome: u32,
    /// Board cards and other blocks that sit on `bg`.
    pub card: u32,
    /// A hovered row or quiet button.
    pub hover: u32,
    /// The selected row (sidebar session, list item).
    pub selected: u32,
    /// The 1 px inset ring of the selected sidebar session row.
    pub selected_ring: u32,
    pub border: u32,
    /// Hairlines between sections, fainter than `border`.
    pub divider: u32,
    pub text: u32,
    /// Secondary text that is still read (row titles under a name).
    pub text_dim: u32,
    pub text_muted: u32,
    /// Graphics only (chevrons, idle ring, guide lines); never text.
    pub subtle: u32,
    /// Selection, primary buttons, focus.
    pub accent: u32,
    /// Accent-colored text on `bg` / `accent_bg` (links, the selected view).
    pub accent_text: u32,
    /// A faint accent fill (selected title bar segment, accent chips).
    pub accent_bg: u32,
    /// Text and glyphs on an `accent` fill (primary buttons).
    pub on_accent: u32,
    /// Divider inside an `accent` fill (split primary button), RGBA.
    pub on_accent_divider: u32,
    /// Faint needs-input and done fills behind their text.
    pub warning_bg: u32,
    pub success_bg: u32,
    /// Faint error fill (hover of a Stop control); the red twin of `warning_bg` / `success_bg`.
    pub danger_bg: u32,
    /// The title bar's Run pill while something runs: fill, border and text (status green).
    pub run_pill_bg: u32,
    pub run_pill_border: u32,
    pub run_pill_text: u32,
    /// Alpha of a workspace color tint (avatars, [`Theme::tint`]).
    pub tint_alpha: u8,
    /// The shade behind a modal dialog, RGBA.
    pub scrim: u32,
    /// The QR code of Phone access (SPEC §14): dark modules on a light panel in both
    /// appearances, because phone cameras read dark-on-light only; so both are set in
    /// `shared()`, not in `dark()` / `light()`.
    pub qr_dark: u32,
    pub qr_light: u32,
    /// Faint accent border of the focused split pane (accent halfway to `bg`, SPEC §14).
    pub pane_focus_border: u32,
    /// Background of selected terminal cells (accent mixed into `bg`, text stays readable).
    pub terminal_selection_bg: u32,
    /// Background of find-bar matches (needs-input amber mixed into `bg`).
    pub terminal_match_bg: u32,
    /// Background of the current find-bar match (stronger amber).
    pub terminal_match_current_bg: u32,
    pub status_working: u32,
    pub status_needs_input: u32,
    pub status_done: u32,
    pub status_exited: u32,
    pub status_idle: u32,
    pub diff_add_bg: u32,
    pub diff_add_text: u32,
    pub diff_del_bg: u32,
    pub diff_del_text: u32,
    /// A line changed in place: the paired removed + added rows of the diff (Unified and both
    /// Split halves).
    pub diff_mod_bg: u32,
    pub diff_mod_text: u32,
    pub diff_hunk_header: u32,
    /// The number / sign gutter of added, removed and changed-in-place diff lines: a step
    /// stronger than the row's own fill.
    pub diff_add_gutter: u32,
    pub diff_del_gutter: u32,
    pub diff_mod_gutter: u32,
    /// Git file status letters: modified-like (M R C T) and untracked (?).
    pub git_modified: u32,
    pub git_untracked: u32,
    /// Files and diff viewer syntax colors (harmonized with the ANSI palette; comments use
    /// `text_muted`).
    pub syntax_keyword: u32,
    pub syntax_string: u32,
    pub syntax_number: u32,
    pub syntax_type: u32,
    pub syntax_function: u32,
    /// Redesign tokens (2026-10 restyle). Meta text that is still read: timestamps, counts,
    /// placeholders (never dimmer than this).
    pub text_subtle: u32,
    /// In Progress: the status pill and section icon; `info_text` is its label on the tint,
    /// `info_soft` the BE area chip and JSON keys.
    pub info: u32,
    pub info_text: u32,
    pub info_soft: u32,
    /// The FE area chip, and ANSI cyan's hue.
    pub teal: u32,
    /// Behind the terminal grid.
    pub terminal_bg: u32,
    /// The border of a secondary button.
    pub control_border: u32,
    /// A hovered Board row; the fill of a key hint.
    pub row_hover: u32,
    /// A recessed strip: Git hunk headers, the title bar's repo pill.
    pub well: u32,
    /// Destructive button text; removed-line numbers.
    pub danger_text: u32,
    /// Added-line numbers.
    pub success_text: u32,
    /// Folder icons in the Files tree.
    pub folder: u32,
    /// Dotfiles in the Files tree; a primary button's text while it cannot be used.
    pub text_disabled: u32,
    /// The changed token inside a removed / added diff line.
    pub diff_del_word_bg: u32,
    pub diff_add_word_bg: u32,
    /// ANSI 0–15, harmonized with the tokens above.
    pub ansi: [u32; 16],
    /// Workspace colors, in [`WORKSPACE_COLORS`] order (≥ 3:1 on every surface, ADR 0020).
    pub workspace_colors: [u32; WORKSPACE_COLORS.len()],
    /// Dark appearance (picks diagram images, ADR 0021).
    pub dark: bool,
    /// The chosen mode (`[ui] theme`); `dark` is what it resolved to.
    pub mode: ThemeMode,
    /// The built-in theme this is (ADR 0078), e.g. "Mkrew Dark".
    pub name: &'static str,
    pub ui_font_family: SharedString,
    pub terminal_font_family: SharedString,
    pub terminal_font_size: f32,
    /// Line height of the terminal grid, as a multiple of the font size.
    pub terminal_line_height: f32,
    /// Line height of code rows in the Files editor and preview (tighter than the terminal's).
    pub code_line_height: f32,
    /// Padding left and right of the terminal grid.
    pub terminal_padding_x: f32,
    /// Padding above and below the terminal grid.
    pub terminal_padding_y: f32,
    pub terminal_underline_thickness: f32,
    /// Width of the bar (beam) cursor.
    pub terminal_cursor_width: f32,
    /// Weight of a dim (SGR 2) foreground blended towards the background, 0–1.
    pub dim_blend: f32,
    /// Opacity of the overlay that dims unfocused panes (~5%).
    pub unfocused_pane_dim: f32,
    /// The palette from before the restyle, kept for the sidebar's session list (projects,
    /// session rows, RUN), which the restyle leaves as it was. `None` on that palette itself.
    pub classic: Option<std::sync::Arc<Theme>>,
}

impl gpui::Global for Theme {}

impl Theme {
    /// The theme of `mode`'s slot (SPEC §14, ADR 0078); an unknown name is that slot's Mkrew theme.
    pub fn resolve(mode: ThemeMode, os_dark: bool, names: &ThemeNames) -> Self {
        let dark = slot_is_dark(mode, os_dark);
        let theme = find_theme(names.slot(dark)).map_or_else(
            || if dark { Self::dark() } else { Self::light() },
            ThemeEntry::build,
        );
        Self { mode, ..theme }
    }

    /// `color` at the palette's tint alpha (workspace avatars, ADR 0027).
    pub fn tint(&self, color: u32) -> gpui::Rgba {
        gpui::rgba((color << 8) | u32::from(self.tint_alpha))
    }

    /// A workspace avatar on `bg`: (letter color, the tint blended over `bg`). The letter is
    /// body text, so it is `text`: the workspace color itself is too light on its own tint.
    pub fn avatar_colors(&self, color: u32, bg: u32) -> (u32, u32) {
        let alpha = u32::from(self.tint_alpha);
        let fill = [16, 8, 0].into_iter().fold(0, |fill, shift| {
            let (c, b) = ((color >> shift) & 0xFF, (bg >> shift) & 0xFF);
            fill | (((c * alpha + b * (255 - alpha) + 127) / 255) << shift)
        });
        (self.text, fill)
    }

    /// The expand / collapse chevron of a workspace row: the only sign of the row's state, so
    /// `text_muted` (≥ 3:1), not the graphics-only `subtle`.
    pub fn chevron_color(&self) -> u32 {
        self.text_muted
    }

    /// This theme with the fonts of `settings.toml`.
    pub fn with_fonts(mut self, fonts: &Fonts) -> Self {
        self.ui_font_family = fonts.ui_family.clone();
        self.terminal_font_family = fonts.terminal_family.clone();
        self.terminal_font_size = fonts.terminal_size;
        if let Some(classic) = self.classic.take() {
            let classic = Theme::clone(&classic).with_fonts(fonts);
            self.classic = Some(std::sync::Arc::new(classic));
        }
        self
    }

    /// The pre-restyle palette for the sidebar's session list (see [`Theme::classic`]).
    pub fn classic(&self) -> &Theme {
        self.classic.as_deref().unwrap_or(self)
    }

    pub fn dark() -> Self {
        let theme = Self {
            bg: 0x15171B,
            surface: 0x121418,
            surface_raised: 0x2A2E35,
            sidebar: 0x0F1013,
            chrome: 0x0F1013,
            card: 0x1C1F24,
            hover: 0x1F2227,
            selected: 0x2A2E35,
            selected_ring: 0x3D4380,
            border: 0x23262C,
            divider: 0x1E2126,
            text: 0xECEDEF,
            text_dim: 0xD5D8DD,
            text_muted: 0xA3A8B0,
            subtle: 0x5D616C,
            accent: 0x8E8CF5,
            accent_text: 0xC2C1FA,
            accent_bg: 0x26273A,
            on_accent: 0x0F1013,
            on_accent_divider: 0x0000_0040,
            warning_bg: 0x322B23,
            success_bg: 0x1E302B,
            danger_bg: 0x342526,
            run_pill_bg: 0x18241D,
            run_pill_border: 0x2A4434,
            run_pill_text: 0xC9EED8,
            // ~14 %: chip and pill fills.
            tint_alpha: 0x24,
            pane_focus_border: 0x4A528E,
            terminal_selection_bg: 0x353B60,
            terminal_match_bg: 0x3F3524,
            terminal_match_current_bg: 0x7A5A1F,
            status_working: 0x8E8CF5,
            status_needs_input: 0xE4A452,
            status_done: 0x57C78A,
            status_exited: 0xEF7B6C,
            status_idle: 0x7D838C,
            // Removed / added lines: danger / success at 9 % over the diff card (`surface`).
            diff_add_bg: 0x182422,
            diff_add_text: 0x6FD39B,
            diff_del_bg: 0x261D20,
            diff_del_text: 0xEF8F82,
            diff_mod_bg: 0x25211D,
            diff_mod_text: 0xE4A452,
            diff_hunk_header: 0x17191D,
            diff_add_gutter: 0x1E302B,
            diff_del_gutter: 0x342526,
            diff_mod_gutter: 0x322B23,
            git_modified: 0xE4A452,
            git_untracked: 0xE4A452,
            syntax_keyword: 0xC2C1FA,
            syntax_string: 0x9FD6AE,
            syntax_number: 0xE8B87A,
            syntax_type: 0x8FD8CC,
            syntax_function: 0x93BCF6,
            text_subtle: 0x7D838C,
            info: 0x6AA6F2,
            info_text: 0x8DB9F5,
            info_soft: 0x93BCF6,
            teal: 0x74D6C4,
            terminal_bg: 0x0B0C0E,
            control_border: 0x262A31,
            row_hover: 0x1A1D21,
            well: 0x17191D,
            danger_text: 0xEF8F82,
            success_text: 0x6FD39B,
            folder: 0x8FA3C0,
            text_disabled: 0x8A9099,
            // Danger at 30 % / success at 28 % over the diff card.
            diff_del_word_bg: 0x543331,
            diff_add_word_bg: 0x254638,
            // red → danger, green → success, yellow → attention, blue → info, magenta →
            // accent, white → text, bright black → text_subtle.
            ansi: [
                0x1C1F24, 0xEF7B6C, 0x57C78A, 0xE4A452, 0x6AA6F2, 0x8E8CF5, 0x8FD8CC, 0xECEDEF,
                0x7D838C, 0xF29A8E, 0x7DD3A0, 0xEDBB78, 0x93BCF6, 0xC2C1FA, 0xB0E5DC, 0xF5F6F8,
            ],
            workspace_colors: [
                0xF87171, 0xFB923C, 0xFACC15, 0xA3E635, 0x4ADE80, 0x2DD4BF, 0x60A5FA, 0xC084FC,
                0xF472B6,
            ],
            dark: true,
            name: "Mkrew Dark",
            ..Self::shared()
        };
        Self {
            classic: Some(std::sync::Arc::new(Self::classic_dark(&theme))),
            ..theme
        }
    }

    pub fn light() -> Self {
        let theme = Self {
            bg: 0xFFFFFF,
            surface: 0xFAFAFB,
            surface_raised: 0xE6E8EC,
            sidebar: 0xF7F7F8,
            chrome: 0xF7F7F8,
            card: 0xF7F8FA,
            hover: 0xEEF0F3,
            selected: 0xE6E8EC,
            selected_ring: 0xA7ADEA,
            border: 0xDFE1E6,
            divider: 0xE9EBEE,
            text: 0x1B1D22,
            text_dim: 0x3A3E46,
            text_muted: 0x5A606B,
            subtle: 0xA0A4AF,
            accent: 0x5452D0,
            accent_text: 0x4A48C4,
            accent_bg: 0xEBEBFA,
            on_accent: 0xFFFFFF,
            on_accent_divider: 0xFFFF_FF59,
            warning_bg: 0xF4EDE2,
            success_bg: 0xE5F0EA,
            danger_bg: 0xF8E8E6,
            run_pill_bg: 0xE1F5EC,
            run_pill_border: 0xB4DEC8,
            run_pill_text: 0x1E6B3A,
            // ~12 %: chip and pill fills.
            tint_alpha: 0x1F,
            pane_focus_border: 0xA7ADEA,
            terminal_selection_bg: 0xCACEF2,
            terminal_match_bg: 0xF6EBCF,
            terminal_match_current_bg: 0xE9C46A,
            status_working: 0x5452D0,
            status_needs_input: 0xA4660F,
            status_done: 0x23854F,
            status_exited: 0xC2412F,
            status_idle: 0x8A8F99,
            diff_add_bg: 0xE7EFEC,
            diff_add_text: 0x1E6B3A,
            diff_del_bg: 0xF5E9E9,
            diff_del_text: 0xA61B1B,
            diff_mod_bg: 0xFFF6DB,
            diff_mod_text: 0x7A5200,
            diff_hunk_header: 0xF1F2F4,
            diff_add_gutter: 0xC9E8D2,
            diff_del_gutter: 0xF7D4D4,
            diff_mod_gutter: 0xF8E6AE,
            git_modified: 0x975A16,
            git_untracked: 0x975A16,
            // Darker than the ANSI entries so code stays ≥ 4.5:1 on white.
            syntax_keyword: 0x8A3FB8,
            syntax_string: 0x276749,
            syntax_number: 0x975A16,
            syntax_type: 0x17707F,
            syntax_function: 0x3B47B8,
            text_subtle: 0x6B717C,
            info: 0x2F6FC4,
            info_text: 0x2A62AD,
            info_soft: 0x2F6FC4,
            teal: 0x1A7F70,
            terminal_bg: 0xFCFCFD,
            control_border: 0xD9DCE1,
            row_hover: 0xF4F5F7,
            well: 0xF1F2F4,
            danger_text: 0xB4382A,
            success_text: 0x1E6B3A,
            folder: 0x5F7590,
            text_disabled: 0x6B717C,
            diff_del_word_bg: 0xECCCC8,
            diff_add_word_bg: 0xC4DDD0,
            ansi: [
                // "White" stays readable on the light background (≥ 3:1).
                0x1D1E22, 0xC2412F, 0x23854F, 0xA4660F, 0x2F6FC4, 0x5B59D6, 0x1A7F70, 0x8A8C94,
                0x6B717C, 0xE5676B, 0x38A169, 0xD69E2E, 0x3B47B8, 0xB37FEB, 0x31A3B8, 0x55575F,
            ],
            workspace_colors: [
                0xDC2626, 0xD9560B, 0xA16207, 0x4D7C0F, 0x15803D, 0x0F766E, 0x2563EB, 0x9333EA,
                0xDB2777,
            ],
            dark: false,
            name: "Mkrew Light",
            ..Self::shared()
        };
        Self {
            classic: Some(std::sync::Arc::new(Self::classic_light(&theme))),
            ..theme
        }
    }

    /// The pre-restyle dark palette over `base` (it keeps `base`'s fonts and new tokens).
    fn classic_dark(base: &Self) -> Self {
        Self {
            bg: 0x141519,
            surface: 0x1F2127,
            surface_raised: 0x2C2E37,
            sidebar: 0x191A1F,
            chrome: 0x15161A,
            card: 0x1A1B20,
            hover: 0x22242B,
            selected: 0x262A3D,
            selected_ring: 0x3D4380,
            border: 0x2A2C33,
            divider: 0x24262D,
            text: 0xE4E5E9,
            text_dim: 0xB4B7C0,
            text_muted: 0xA0A3AD,
            subtle: 0x5D616C,
            accent: 0x8B91FF,
            accent_text: 0x9A9FFF,
            accent_bg: 0x252848,
            // The dark accent is light: dark text keeps ≥ 3:1 on it (white would not).
            on_accent: 0x141519,
            on_accent_divider: 0x0000_0040,
            warning_bg: 0x3A3020,
            success_bg: 0x1D3A31,
            danger_bg: 0x3A2327,
            run_pill_bg: 0x18241D,
            run_pill_border: 0x2A4434,
            run_pill_text: 0xC9EED8,
            tint_alpha: 0x2E,
            pane_focus_border: 0x4A528E,
            terminal_selection_bg: 0x353B60,
            terminal_match_bg: 0x3F3524,
            terminal_match_current_bg: 0x7A5A1F,
            status_working: 0x8B91FF,
            status_needs_input: 0xF0B866,
            status_done: 0x5FD28F,
            status_exited: 0xF08A80,
            status_idle: 0x8A8D97,
            diff_add_bg: 0x1E3A2B,
            diff_add_text: 0x7DD3A0,
            diff_del_bg: 0x3D2226,
            diff_del_text: 0xF2A0A4,
            diff_mod_bg: 0x3A3320,
            diff_mod_text: 0xE6C77A,
            diff_hunk_header: 0x2C2E37,
            diff_add_gutter: 0x24503A,
            diff_del_gutter: 0x56292F,
            diff_mod_gutter: 0x4D4326,
            git_modified: 0x8FB3FF,
            git_untracked: 0xF0B866,
            syntax_keyword: 0xC28CF0,
            syntax_string: 0x7DD3A0,
            syntax_number: 0xF0C674,
            syntax_type: 0x8BE0EC,
            syntax_function: 0xA3AFFF,
            ansi: [
                0x25262C, 0xE5676B, 0x4CB782, 0xE3A63E, 0x7C8CFF, 0xC28CF0, 0x5EC4D6, 0xC9CBD1,
                0x6E7079, 0xF2A0A4, 0x7DD3A0, 0xF0C674, 0xA3AFFF, 0xD8B4FE, 0x8BE0EC, 0xF5F6F8,
            ],
            workspace_colors: [
                0xF87171, 0xFB923C, 0xFACC15, 0xA3E635, 0x4ADE80, 0x2DD4BF, 0x60A5FA, 0xC084FC,
                0xF472B6,
            ],
            dark: true,
            classic: None,
            ..base.clone()
        }
    }

    /// The pre-restyle light palette over `base`.
    fn classic_light(base: &Self) -> Self {
        Self {
            bg: 0xFFFFFF,
            surface: 0xF6F7F9,
            surface_raised: 0xEEF0F3,
            sidebar: 0xF4F5F7,
            chrome: 0xFFFFFF,
            card: 0xFFFFFF,
            hover: 0xEEF0F3,
            selected: 0xE9EAFF,
            selected_ring: 0xA7ADEA,
            border: 0xE3E5EA,
            divider: 0xECEEF1,
            text: 0x1B1D22,
            text_dim: 0x474B55,
            text_muted: 0x5F6473,
            subtle: 0xA0A4AF,
            accent: 0x4F5BD5,
            accent_text: 0x4A52D9,
            accent_bg: 0xECEEFF,
            on_accent: 0xFFFFFF,
            on_accent_divider: 0xFFFF_FF59,
            warning_bg: 0xFDF3E1,
            success_bg: 0xE1F5EC,
            danger_bg: 0xFDECEC,
            run_pill_bg: 0xE1F5EC,
            run_pill_border: 0xB4DEC8,
            run_pill_text: 0x1E6B3A,
            tint_alpha: 0x24,
            pane_focus_border: 0xA7ADEA,
            terminal_selection_bg: 0xCACEF2,
            terminal_match_bg: 0xF6EBCF,
            terminal_match_current_bg: 0xE9C46A,
            status_working: 0x4F5BD5,
            status_needs_input: 0xB7791F,
            status_done: 0x2F855A,
            status_exited: 0xC53030,
            status_idle: 0x9A9CA6,
            diff_add_bg: 0xE6F4EA,
            diff_add_text: 0x1E6B3A,
            diff_del_bg: 0xFDEEEE,
            diff_del_text: 0xA61B1B,
            diff_mod_bg: 0xFFF6DB,
            diff_mod_text: 0x7A5200,
            diff_hunk_header: 0xF6F7F9,
            diff_add_gutter: 0xC9E8D2,
            diff_del_gutter: 0xF7D4D4,
            diff_mod_gutter: 0xF8E6AE,
            git_modified: 0x1D4ED8,
            git_untracked: 0x975A16,
            // Darker than the ANSI entries so code stays ≥ 4.5:1 on white.
            syntax_keyword: 0x8A3FB8,
            syntax_string: 0x276749,
            syntax_number: 0x975A16,
            syntax_type: 0x17707F,
            syntax_function: 0x3B47B8,
            ansi: [
                // "White" stays readable on the light background (≥ 3:1).
                0x1D1E22, 0xC53030, 0x2F855A, 0xB7791F, 0x4F5BD5, 0x9B4DCA, 0x1F8A9E, 0x8A8C94,
                0x6B6D76, 0xE5676B, 0x38A169, 0xD69E2E, 0x3B47B8, 0xB37FEB, 0x31A3B8, 0x55575F,
            ],
            workspace_colors: [
                0xDC2626, 0xD9560B, 0xA16207, 0x4D7C0F, 0x15803D, 0x0F766E, 0x2563EB, 0x9333EA,
                0xDB2777,
            ],
            dark: false,
            classic: None,
            ..base.clone()
        }
    }

    fn shared() -> Self {
        Self {
            bg: 0,
            surface: 0,
            surface_raised: 0,
            sidebar: 0,
            chrome: 0,
            card: 0,
            hover: 0,
            selected: 0,
            selected_ring: 0,
            border: 0,
            divider: 0,
            text: 0,
            text_dim: 0,
            text_muted: 0,
            subtle: 0,
            accent: 0,
            accent_text: 0,
            accent_bg: 0,
            on_accent: 0,
            on_accent_divider: 0,
            warning_bg: 0,
            success_bg: 0,
            danger_bg: 0,
            run_pill_bg: 0,
            run_pill_border: 0,
            run_pill_text: 0,
            tint_alpha: 0,
            // Black at ~35 %, in both appearances.
            scrim: 0x0000_0059,
            // The QR code of Phone access: near-black modules on white, in both appearances.
            qr_dark: 0x101114,
            qr_light: 0xFFFFFF,
            pane_focus_border: 0,
            terminal_selection_bg: 0,
            terminal_match_bg: 0,
            terminal_match_current_bg: 0,
            status_working: 0,
            status_needs_input: 0,
            status_done: 0,
            status_exited: 0,
            status_idle: 0,
            diff_add_bg: 0,
            diff_add_text: 0,
            diff_del_bg: 0,
            diff_del_text: 0,
            diff_mod_bg: 0,
            diff_mod_text: 0,
            diff_hunk_header: 0,
            diff_add_gutter: 0,
            diff_del_gutter: 0,
            diff_mod_gutter: 0,
            git_modified: 0,
            git_untracked: 0,
            syntax_keyword: 0,
            syntax_string: 0,
            syntax_number: 0,
            syntax_type: 0,
            syntax_function: 0,
            text_subtle: 0,
            info: 0,
            info_text: 0,
            info_soft: 0,
            teal: 0,
            terminal_bg: 0,
            control_border: 0,
            row_hover: 0,
            well: 0,
            danger_text: 0,
            success_text: 0,
            folder: 0,
            text_disabled: 0,
            diff_del_word_bg: 0,
            diff_add_word_bg: 0,
            ansi: [0; 16],
            workspace_colors: [0; WORKSPACE_COLORS.len()],
            dark: false,
            mode: ThemeMode::System,
            name: "",
            ui_font_family: UI_FONT.into(),
            terminal_font_family: pick_family(TERMINAL_FONTS, &[]).into(),
            terminal_font_size: 13.0,
            terminal_line_height: 1.75,
            code_line_height: 1.35,
            terminal_padding_x: space::L,
            terminal_padding_y: space::L,
            terminal_underline_thickness: 1.0,
            terminal_cursor_width: 2.0,
            dim_blend: 0.6,
            unfocused_pane_dim: 0.05,
            classic: None,
        }
    }

    /// The token of a workspace color name; `None` for an unknown name.
    pub fn workspace_color(&self, name: &str) -> Option<u32> {
        let index = WORKSPACE_COLORS.iter().position(|color| *color == name)?;
        self.workspace_colors.get(index).copied()
    }
}

/// The colors a built-in theme is made of (ADR 0078); [`Theme::from_palette`] derives the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub dark: bool,
    /// The main area and the terminal.
    pub bg: u32,
    /// Sidebar, title bar, tab strip.
    pub chrome: u32,
    /// Panels, popovers, inputs, cards.
    pub surface: u32,
    pub fg: u32,
    pub muted: u32,
    pub accent: u32,
    /// Hues for UI text roles (diff, syntax, git letters); made readable by `readable`.
    pub red: u32,
    pub green: u32,
    pub yellow: u32,
    pub blue: u32,
    pub magenta: u32,
    pub cyan: u32,
    /// The terminal's ANSI 0–15 as the theme publishes them.
    pub ansi: [u32; 16],
}

/// `a` blended toward `b` by `t` (0 = `a`, 1 = `b`), per RGB channel.
pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    [16, 8, 0].into_iter().fold(0, |out, shift| {
        let (x, y) = (((a >> shift) & 0xFF) as f32, ((b >> shift) & 0xFF) as f32);
        out | (((x + (y - x) * t).round() as u32) << shift)
    })
}

/// `color` blended toward `toward` in 5 % steps until it reads at `min` on every ground.
fn readable(color: u32, toward: u32, grounds: &[u32], min: f32) -> u32 {
    (0..=20)
        .map(|step| mix(color, toward, step as f32 * 0.05))
        .find(|c| grounds.iter().all(|g| contrast(*c, *g) >= min))
        .unwrap_or(toward)
}

/// `bg` tinted with `hue` by at most `t`, less (0.05 steps) until `fg` reads ≥ 4.5:1 on it.
fn fill(bg: u32, hue: u32, t: f32, fg: u32) -> u32 {
    let steps = (t / 0.05).round() as i32;
    (0..=steps)
        .rev()
        .map(|step| mix(bg, hue, step as f32 * 0.05))
        .find(|c| contrast(fg, *c) >= 4.5)
        .unwrap_or(bg)
}

impl Theme {
    /// A built-in theme from its palette (ADR 0078; token table in the spec). Status, workspace
    /// and QR colors, the classic sidebar palette, fonts and sizes stay the Mkrew base's.
    pub fn from_palette(name: &'static str, p: &Palette) -> Self {
        let base = if p.dark { Self::dark() } else { Self::light() };
        let tint = |hue, t| mix(p.bg, hue, t);
        let surface_raised = mix(p.surface, p.fg, 0.10);
        let hover = mix(p.bg, p.fg, 0.06);
        let diff = |hue| (tint(hue, 0.14), tint(hue, 0.24), tint(hue, 0.30));
        let (add_bg, add_gutter, add_word) = diff(p.green);
        let (del_bg, del_gutter, del_word) = diff(p.red);
        let (mod_bg, mod_gutter, _) = diff(p.yellow);
        let grounds = [
            p.bg,
            p.chrome,
            p.surface,
            surface_raised,
            hover,
            add_bg,
            del_bg,
            mod_bg,
            add_gutter,
            del_gutter,
            mod_gutter,
            add_word,
            del_word,
        ];
        let text = |color| readable(color, p.fg, &grounds, 4.5);
        let on_accent = if contrast(p.bg, p.accent) >= contrast(p.fg, p.accent) {
            p.bg
        } else {
            p.fg
        };
        Self {
            name,
            bg: p.bg,
            terminal_bg: p.bg,
            sidebar: p.chrome,
            chrome: p.chrome,
            surface: p.surface,
            card: p.surface,
            surface_raised,
            selected: surface_raised,
            hover,
            row_hover: hover,
            well: hover,
            diff_hunk_header: hover,
            border: mix(p.bg, p.fg, 0.14),
            control_border: mix(p.bg, p.fg, 0.14),
            divider: mix(p.bg, p.fg, 0.09),
            selected_ring: mix(p.accent, p.bg, 0.50),
            text: p.fg,
            text_dim: text(mix(p.fg, p.bg, 0.20)),
            text_muted: text(p.muted),
            text_subtle: text(p.muted),
            text_disabled: text(p.muted),
            subtle: mix(p.muted, p.bg, 0.35),
            accent: p.accent,
            accent_text: text(p.accent),
            info: p.blue,
            info_soft: p.blue,
            info_text: text(p.blue),
            accent_bg: tint(p.accent, 0.14),
            pane_focus_border: mix(p.accent, p.bg, 0.50),
            on_accent,
            terminal_selection_bg: fill(p.bg, p.accent, 0.30, p.fg),
            terminal_match_bg: fill(p.bg, p.yellow, 0.20, p.fg),
            terminal_match_current_bg: fill(p.bg, p.yellow, 0.50, p.fg),
            warning_bg: tint(p.yellow, 0.14),
            success_bg: tint(p.green, 0.14),
            danger_bg: tint(p.red, 0.14),
            run_pill_bg: tint(p.green, 0.12),
            run_pill_border: tint(p.green, 0.35),
            run_pill_text: text(p.green),
            diff_add_bg: add_bg,
            diff_del_bg: del_bg,
            diff_mod_bg: mod_bg,
            diff_add_gutter: add_gutter,
            diff_del_gutter: del_gutter,
            diff_mod_gutter: mod_gutter,
            diff_add_word_bg: add_word,
            diff_del_word_bg: del_word,
            diff_add_text: text(p.green),
            success_text: text(p.green),
            diff_del_text: text(p.red),
            danger_text: text(p.red),
            diff_mod_text: text(p.yellow),
            git_modified: text(p.yellow),
            git_untracked: text(p.yellow),
            syntax_number: text(p.yellow),
            syntax_keyword: text(p.magenta),
            syntax_string: text(p.green),
            syntax_type: text(p.cyan),
            syntax_function: text(p.blue),
            teal: text(p.cyan),
            folder: text(p.blue),
            ansi: p.ansi,
            dark: p.dark,
            ..base
        }
    }
}

/// One built-in theme (ADR 0078). The Mkrew ones have no palette: they are `Theme::dark()` /
/// `Theme::light()` exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeEntry {
    pub name: &'static str,
    pub dark: bool,
    palette: Option<Palette>,
}

impl ThemeEntry {
    pub fn build(&self) -> Theme {
        match (&self.palette, self.dark) {
            (Some(palette), _) => Theme::from_palette(self.name, palette),
            (None, true) => Theme::dark(),
            (None, false) => Theme::light(),
        }
    }
}

/// The theme named `name` (exact match).
pub fn find_theme(name: &str) -> Option<&'static ThemeEntry> {
    THEMES.iter().find(|entry| entry.name == name)
}

const fn dark_entry(name: &'static str, palette: Palette) -> ThemeEntry {
    ThemeEntry {
        name,
        dark: true,
        palette: Some(palette),
    }
}

const fn light_entry(name: &'static str, palette: Palette) -> ThemeEntry {
    ThemeEntry {
        name,
        dark: false,
        palette: Some(palette),
    }
}

/// Every built-in theme, in [`mkrew_core::settings::THEME_NAMES`] order.
pub const THEMES: &[ThemeEntry] = &[
    ThemeEntry {
        name: "Mkrew Dark",
        dark: true,
        palette: None,
    },
    dark_entry(
        "One Dark",
        Palette {
            dark: true,
            bg: 0x282C34,
            chrome: 0x21252B,
            surface: 0x292D35,
            fg: 0xBDC3CD,
            muted: 0x9DA5B4,
            accent: 0x61AFEF,
            red: 0xE06C75,
            green: 0x98C379,
            yellow: 0xE5C07B,
            blue: 0x61AFEF,
            magenta: 0xC678DD,
            cyan: 0x56B6C2,
            ansi: [
                0x3F4451, 0xE06C75, 0x98C379, 0xD19A66, 0x61AFEF, 0xC678DD, 0x56B6C2, 0xABB2BF,
                0x5C6370, 0xE06C75, 0x98C379, 0xE5C07B, 0x61AFEF, 0xC678DD, 0x56B6C2, 0xFFFFFF,
            ],
        },
    ),
    dark_entry(
        "Dracula",
        Palette {
            dark: true,
            bg: 0x282A36,
            chrome: 0x21222C,
            surface: 0x242631,
            fg: 0xF8F8F2,
            muted: 0xA0A8CD,
            accent: 0xBD93F9,
            red: 0xFF5555,
            green: 0x50FA7B,
            yellow: 0xF1FA8C,
            blue: 0xBD93F9,
            magenta: 0xFF79C6,
            cyan: 0x8BE9FD,
            ansi: [
                0x21222C, 0xFF5555, 0x50FA7B, 0xF1FA8C, 0xBD93F9, 0xFF79C6, 0x8BE9FD, 0xF8F8F2,
                0x6272A4, 0xFF6E6E, 0x69FF94, 0xFFFFA5, 0xD6ACFF, 0xFF92DF, 0xA4FFFF, 0xFFFFFF,
            ],
        },
    ),
    dark_entry(
        "Catppuccin Mocha",
        Palette {
            dark: true,
            bg: 0x1E1E2E,
            chrome: 0x181825,
            surface: 0x313244,
            fg: 0xCDD6F4,
            muted: 0xA6ADC8,
            accent: 0x89B4FA,
            red: 0xF38BA8,
            green: 0xA6E3A1,
            yellow: 0xF9E2AF,
            blue: 0x89B4FA,
            magenta: 0xCBA6F7,
            cyan: 0x94E2D5,
            ansi: [
                0x45475A, 0xF38BA8, 0xA6E3A1, 0xF9E2AF, 0x89B4FA, 0xF5C2E7, 0x94E2D5, 0xBAC2DE,
                0x585B70, 0xF38BA8, 0xA6E3A1, 0xF9E2AF, 0x89B4FA, 0xF5C2E7, 0x94E2D5, 0xA6ADC8,
            ],
        },
    ),
    dark_entry(
        "Tokyo Night",
        Palette {
            dark: true,
            bg: 0x1A1B26,
            chrome: 0x16161E,
            surface: 0x24283B,
            fg: 0xC0CAF5,
            muted: 0x9AA5CE,
            accent: 0x7AA2F7,
            red: 0xF7768E,
            green: 0x9ECE6A,
            yellow: 0xE0AF68,
            blue: 0x7AA2F7,
            magenta: 0xBB9AF7,
            cyan: 0x7DCFFF,
            ansi: [
                0x15161E, 0xF7768E, 0x9ECE6A, 0xE0AF68, 0x7AA2F7, 0xBB9AF7, 0x7DCFFF, 0xA9B1D6,
                0x414868, 0xF7768E, 0x9ECE6A, 0xE0AF68, 0x7AA2F7, 0xBB9AF7, 0x7DCFFF, 0xC0CAF5,
            ],
        },
    ),
    dark_entry(
        "Gruvbox Dark",
        Palette {
            dark: true,
            bg: 0x282828,
            chrome: 0x1D2021,
            surface: 0x222222,
            fg: 0xEBDBB2,
            muted: 0xBDAE93,
            accent: 0x83A598,
            red: 0xFB4934,
            green: 0xB8BB26,
            yellow: 0xFABD2F,
            blue: 0x83A598,
            magenta: 0xD3869B,
            cyan: 0x8EC07C,
            ansi: [
                0x282828, 0xCC241D, 0x98971A, 0xD79921, 0x458588, 0xB16286, 0x689D6A, 0xA89984,
                0x928374, 0xFB4934, 0xB8BB26, 0xFABD2F, 0x83A598, 0xD3869B, 0x8EC07C, 0xEBDBB2,
            ],
        },
    ),
    ThemeEntry {
        name: "Mkrew Light",
        dark: false,
        palette: None,
    },
    light_entry(
        "One Light",
        Palette {
            dark: false,
            bg: 0xFAFAFA,
            chrome: 0xEAEAEB,
            surface: 0xFCFCFC,
            fg: 0x383A42,
            muted: 0x696C77,
            accent: 0x3564C9,
            red: 0xE45649,
            green: 0x50A14F,
            yellow: 0xC18401,
            blue: 0x4078F2,
            magenta: 0xA626A4,
            cyan: 0x0184BC,
            ansi: [
                0x383A42, 0xE45649, 0x50A14F, 0xC18401, 0x4078F2, 0xA626A4, 0x0184BC, 0xA0A1A7,
                0x4F525E, 0xE06C75, 0x98C379, 0xE5C07B, 0x61AFEF, 0xC678DD, 0x56B6C2, 0xFFFFFF,
            ],
        },
    ),
    light_entry(
        "GitHub Light",
        Palette {
            dark: false,
            bg: 0xFFFFFF,
            chrome: 0xF6F8FA,
            surface: 0xFDFEFE,
            fg: 0x1F2328,
            muted: 0x59636E,
            accent: 0x0965D1,
            red: 0xCF222E,
            green: 0x1A7F37,
            yellow: 0x9A6700,
            blue: 0x0969DA,
            magenta: 0x8250DF,
            cyan: 0x1B7C83,
            ansi: [
                0x24292F, 0xCF222E, 0x116329, 0x4D2D00, 0x0969DA, 0x8250DF, 0x1B7C83, 0x6E7781,
                0x57606A, 0xA40E26, 0x1A7F37, 0x633C01, 0x218BFF, 0xA475F9, 0x3192AA, 0x8C959F,
            ],
        },
    ),
    light_entry(
        "Catppuccin Latte",
        Palette {
            dark: false,
            bg: 0xEFF1F5,
            chrome: 0xDDE1E9,
            surface: 0xF7F8FA,
            fg: 0x4C4F69,
            muted: 0x5C5F77,
            accent: 0x1C5EE1,
            red: 0xD20F39,
            green: 0x40A02B,
            yellow: 0xDF8E1D,
            blue: 0x1E66F5,
            magenta: 0x8839EF,
            cyan: 0x179299,
            ansi: [
                0x5C5F77, 0xD20F39, 0x40A02B, 0xDF8E1D, 0x1E66F5, 0xEA76CB, 0x179299, 0xACB0BE,
                0x6C6F85, 0xD20F39, 0x40A02B, 0xDF8E1D, 0x1E66F5, 0xEA76CB, 0x179299, 0xBCC0CC,
            ],
        },
    ),
    light_entry(
        "Solarized Light",
        Palette {
            dark: false,
            bg: 0xFDF6E3,
            chrome: 0xEEE8D5,
            surface: 0xFEFBF1,
            fg: 0x4A5C62,
            muted: 0x657B83,
            accent: 0x1E6FA8,
            red: 0xDC322F,
            green: 0x859900,
            yellow: 0xB58900,
            blue: 0x268BD2,
            magenta: 0xD33682,
            cyan: 0x2AA198,
            ansi: [
                0x073642, 0xDC322F, 0x859900, 0xB58900, 0x268BD2, 0xD33682, 0x2AA198, 0xEEE8D5,
                0x002B36, 0xCB4B16, 0x586E75, 0x657B83, 0x839496, 0x6C71C4, 0x93A1A1, 0xFDF6E3,
            ],
        },
    ),
    light_entry(
        "Gruvbox Light",
        Palette {
            dark: false,
            bg: 0xFBF1C7,
            chrome: 0xEFE0B7,
            surface: 0xFCF3CF,
            fg: 0x3C3836,
            muted: 0x665C54,
            accent: 0x076678,
            red: 0x9D0006,
            green: 0x79740E,
            yellow: 0xB57614,
            blue: 0x076678,
            magenta: 0x8F3F71,
            cyan: 0x427B58,
            ansi: [
                0xFBF1C7, 0xCC241D, 0x98971A, 0xD79921, 0x458588, 0xB16286, 0x689D6A, 0x7C6F64,
                0x928374, 0x9D0006, 0x79740E, 0xB57614, 0x076678, 0x8F3F71, 0x427B58, 0x3C3836,
            ],
        },
    ),
];

/// Views read the theme with `cx.theme()` (ADR 0027).
pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Theme {
        self.global::<Theme>()
    }
}

/// Rebuilds the theme global from `mode`, the window's OS appearance, the `ThemeNames` and
/// `Fonts` globals. Called at startup, on OS appearance changes, on a settings reload and by
/// the toggle.
pub fn refresh(mode: ThemeMode, window: &Window, cx: &mut App) {
    let fonts = cx.global::<Fonts>().clone();
    let names = cx.global::<ThemeNames>().clone();
    cx.set_global(Theme::resolve(mode, os_dark(window), &names).with_fonts(&fonts));
}

/// System → Light → Dark → System (the title bar toggle, `ToggleTheme`).
pub fn next_mode(mode: ThemeMode) -> ThemeMode {
    match mode {
        ThemeMode::System => ThemeMode::Light,
        ThemeMode::Light => ThemeMode::Dark,
        ThemeMode::Dark => ThemeMode::System,
    }
}

/// The toggle's icon: sun and moon (follows the OS) / sun / moon.
pub fn mode_icon(mode: ThemeMode) -> crate::icons::Icon {
    use crate::icons::Icon;
    match mode {
        ThemeMode::System => Icon::SunMoon,
        ThemeMode::Light => Icon::Sun,
        ThemeMode::Dark => Icon::Moon,
    }
}

/// Marks a pinned workspace or tab (SPEC §10). Plain text like the other glyphs:
/// on Windows GPUI draws it as a color emoji through DirectWrite's font fallback (Segoe UI
/// Emoji), so it ignores `text_color`.
pub const PIN_GLYPH: &str = "📌";

/// A design size in px at the default 13 px body, as rems: it grows with `[ui] font_size`
/// (ADR 0024) instead of clipping text.
pub fn rems_px(px: f32) -> Rems {
    rems(px / 13.0)
}

/// Layout heights from the redesign (ADR 0027), in px at the default font size.
/// The custom title bar (ADR 0034; was the 44 px toolbar of ADR 0027).
pub const TITLE_BAR_HEIGHT: f32 = 48.0;
pub const TAB_STRIP_HEIGHT: f32 = 44.0;
/// The Git view's repository tabs (multi-repo workspaces): a tab strip.
pub const GIT_REPO_STRIP_HEIGHT: f32 = 42.0;
pub const WORKSPACE_ROW_HEIGHT: f32 = 30.0;
pub const SESSION_ROW_HEIGHT: f32 = 30.0;
/// Height of the Plan card's progress bar, and of the thin one under the selected sidebar row
/// (ADR 0039). The track is `Theme::border`, the fill `Theme::accent` (both themes).
pub const PLAN_BAR_HEIGHT: f32 = 4.0;
pub const PLAN_ROW_BAR_HEIGHT: f32 = 2.0;
/// The sidebar's footer row (Inbox, voice input, compact toggle), in px at the default font
/// size; drawn with existing tokens.
pub const SIDEBAR_FOOTER_HEIGHT: f32 = 32.0;
pub const WORKSPACE_AVATAR_SIZE: f32 = 20.0;
/// The tiny avatars on the sidebar's "N empty workspaces" line.
pub const WORKSPACE_AVATAR_TINY: f32 = 14.0;
/// The workspace color stripe on the left edge of a sidebar workspace card (ADR 0062).
pub const WORKSPACE_STRIPE_WIDTH: f32 = 3.0;
/// A status dot (run pill, tab, RUN row, working pill): px at the default font size.
pub const STATUS_DOT_LARGE: f32 = 6.0;
/// One breath of a pulsing status dot (working, running); its lowest opacity.
pub const PULSE_PERIOD: Duration = Duration::from_millis(1600);
pub const PULSE_MIN_OPACITY: f32 = 0.35;
/// A pane's status pill ("Idle · 2m") in the tab strip and the pane header.
pub const STATUS_PILL_HEIGHT: f32 = 24.0;
/// Longest tab title before it truncates.
pub const TAB_TITLE_MAX_WIDTH: f32 = 220.0;
/// Reading width of an agent pane: the terminal is at most this wide (≈ 120–140 columns at
/// the default font) and centered, so the program wraps there (the PTY gets fewer columns).
pub const READING_WIDTH: f32 = 960.0;
/// A pulse redraws at most this often (each redraw renders the window: keep it low).
pub const PULSE_FPS: f32 = 12.0;
pub const EMPTY_STATE_MAX_WIDTH: f32 = 320.0;
pub const EMPTY_STATE_TILE: f32 = 44.0;

/// WCAG contrast ratio of two `0xRRGGBB` colors (1–21).
pub fn contrast(a: u32, b: u32) -> f32 {
    fn luminance(color: u32) -> f32 {
        let channel = |shift: u32| {
            let c = ((color >> shift) & 0xFF) as f32 / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
    }
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// `0xRRGGBB` as the CSS / SVG color `#RRGGBB` (diagram and QR code SVGs).
pub fn hex(color: u32) -> String {
    format!("#{color:06X}")
}

#[cfg(test)]
mod tests {
    use mkrew_core::settings::{FontSize, Settings};

    use super::*;

    fn all_themes() -> Vec<Theme> {
        THEMES.iter().map(ThemeEntry::build).collect()
    }

    #[test]
    fn the_pick_goes_to_the_slot_shown() {
        assert_eq!(
            slot_key(slot_is_dark(ThemeMode::System, true)),
            "dark_theme"
        );
        assert_eq!(
            slot_key(slot_is_dark(ThemeMode::System, false)),
            "light_theme"
        );
        assert_eq!(
            slot_key(slot_is_dark(ThemeMode::Light, true)),
            "light_theme"
        );
        assert_eq!(slot_key(slot_is_dark(ThemeMode::Dark, false)), "dark_theme");
    }

    #[test]
    fn the_theme_table_matches_the_settings_names() {
        let names: Vec<&str> = THEMES.iter().map(|e| e.name).collect();
        assert_eq!(names, mkrew_core::settings::THEME_NAMES);
        for entry in THEMES {
            let theme = entry.build();
            assert_eq!(theme.dark, entry.dark, "{}", entry.name);
            assert_eq!(theme.name, entry.name);
        }
        assert_eq!(
            find_theme("Mkrew Dark").map(ThemeEntry::build),
            Some(Theme::dark())
        );
        assert_eq!(
            find_theme("Mkrew Light").map(ThemeEntry::build),
            Some(Theme::light())
        );
        assert!(find_theme("dracula").is_none(), "exact names only");
    }

    #[test]
    fn mix_blends_per_channel() {
        assert_eq!(mix(0x000000, 0xFFFFFF, 0.5), 0x808080);
        assert_eq!(mix(0x102030, 0x102030, 0.7), 0x102030);
        assert_eq!(mix(0xFF0000, 0x0000FF, 0.0), 0xFF0000);
        assert_eq!(mix(0xFF0000, 0x0000FF, 1.0), 0x0000FF);
    }

    #[test]
    fn the_classic_sidebar_text_reads_on_every_sidebar() {
        for theme in all_themes() {
            let classic = theme.classic();
            for (what, color) in [("text", classic.text), ("text_muted", classic.text_muted)] {
                let ratio = contrast(color, theme.sidebar);
                assert!(ratio >= 4.5, "{} {what}: {ratio:.2}", theme.name);
            }
        }
    }

    #[test]
    fn derived_themes_keep_the_fixed_tokens() {
        for entry in THEMES.iter().filter(|e| e.palette.is_some()) {
            let theme = entry.build();
            let base = if entry.dark {
                Theme::dark()
            } else {
                Theme::light()
            };
            let fixed = |t: &Theme| {
                (
                    t.status_working,
                    t.status_needs_input,
                    t.status_done,
                    t.status_exited,
                    t.status_idle,
                    t.workspace_colors,
                    t.qr_dark,
                    t.qr_light,
                )
            };
            assert_eq!(fixed(&theme), fixed(&base), "{}", entry.name);
            assert_eq!(theme.classic(), base.classic(), "{}", entry.name);
        }
    }

    /// Debug dump of every token, minus fields that are not colors and may differ by platform or
    /// be added later (`name`, font families). Guards "the default look never changes" (ADR 0078).
    fn golden(theme: &Theme) -> String {
        format!("{theme:#?}")
            .lines()
            .filter(|line| {
                let line = line.trim_start();
                !(line.starts_with("name:")
                    || line.starts_with("ui_font_family:")
                    || line.starts_with("terminal_font_family:"))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_mkrew_themes_never_change() {
        insta::assert_snapshot!("mkrew_dark", golden(&Theme::dark()));
        insta::assert_snapshot!("mkrew_light", golden(&Theme::light()));
    }

    #[test]
    fn a_configured_family_is_used_only_when_installed() {
        let mut settings = Settings::default();
        settings.terminal.font_family = "JetBrains Mono".into();
        settings.terminal.font_size = FontSize(18);
        settings.ui.font_family = "Inter".into();
        settings.ui.font_size = FontSize(15);
        let installed = vec!["JetBrains Mono".to_owned(), TERMINAL_FONTS[0].to_owned()];

        let fonts = Fonts::from_settings(&settings, &installed);
        assert_eq!(fonts.terminal_family.as_ref(), "JetBrains Mono");
        assert_eq!((fonts.terminal_size, fonts.ui_size), (18.0, 15.0));
        assert_eq!(
            fonts.ui_family.as_ref(),
            "Inter",
            "UI families fall back in GPUI itself"
        );

        let fonts = Fonts::from_settings(&settings, &installed[1..]);
        assert_eq!(
            fonts.terminal_family.as_ref(),
            TERMINAL_FONTS[0],
            "a missing family never makes the grid proportional"
        );
    }

    #[test]
    fn default_settings_give_the_spec_fonts() {
        let fonts = Fonts::from_settings(&Settings::default(), &[]);
        assert_eq!(fonts.ui_family.as_ref(), UI_FONT);
        assert_eq!(fonts.terminal_family.as_ref(), TERMINAL_FONTS[0]);
        assert_eq!((fonts.ui_size, fonts.terminal_size), (13.0, 13.0));
        let theme = Theme::dark().with_fonts(&fonts);
        assert_eq!(theme.terminal_font_size, 13.0);
        assert_eq!(
            theme,
            Theme::dark(),
            "the default theme already uses the default fonts"
        );
    }

    #[test]
    fn a_broken_settings_file_keeps_the_last_settings_and_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let last = Fonts::from_settings(&Settings::default(), &[]);
        let n = ThemeNames::from_settings(&Settings::default());
        let system = ThemeMode::System;
        std::fs::write(&path, "[terminal]\nfont_size = 18\n").unwrap();
        let (grown, mode, _) = reload(&path, &[], &last, system, &n)
            .unwrap()
            .expect("changed");
        assert_eq!((grown.terminal_size, mode), (18.0, system));
        std::fs::write(&path, "[terminal]\nfont_size = 4\n").unwrap();
        let error = reload(&path, &[], &grown, system, &n).expect_err("out of range");
        assert!(error.contains("settings.toml:2:"), "{error}");
        assert!(error.contains("8–32"), "{error}");
        std::fs::write(&path, "[terminal]\nfont_size = 18\n").unwrap();
        assert_eq!(
            reload(&path, &[], &grown, system, &n),
            Ok(None),
            "unchanged: no redraw"
        );
        // A new theme alone is a change.
        std::fs::write(
            &path,
            "[ui]\ntheme = \"dark\"\n[terminal]\nfont_size = 18\n",
        )
        .unwrap();
        assert_eq!(
            reload(&path, &[], &grown, system, &n),
            Ok(Some((grown.clone(), ThemeMode::Dark, n.clone())))
        );
        // A new theme name alone is a change.
        std::fs::write(
            &path,
            "[ui]\ntheme = \"dark\"\ndark_theme = \"Dracula\"\n[terminal]\nfont_size = 18\n",
        )
        .unwrap();
        let dark = ThemeMode::Dark;
        assert_eq!(
            reload(&path, &[], &grown, dark, &n),
            Ok(Some((grown.clone(), dark, names("Mkrew Light", "Dracula"))))
        );
    }

    #[test]
    fn the_first_installed_terminal_family_wins() {
        let installed = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
        let preferred = &["Cascadia Code", "Cascadia Mono", "Consolas"];
        assert_eq!(
            pick_family(preferred, &installed(&["Arial", "Consolas"])),
            "Consolas"
        );
        assert_eq!(
            pick_family(preferred, &installed(&["Consolas", "Cascadia Mono"])),
            "Cascadia Mono",
            "preference order, not installation order"
        );
        assert_eq!(pick_family(preferred, &installed(&[])), "Cascadia Code");
    }

    #[test]
    fn body_text_contrast_is_at_least_4_5() {
        for theme in all_themes() {
            for bg in [
                theme.bg,
                theme.surface,
                theme.surface_raised,
                theme.sidebar,
                theme.hover,
                theme.selected,
                theme.card,
            ] {
                for fg in [theme.text, theme.text_muted] {
                    let ratio = contrast(fg, bg);
                    assert!(
                        ratio >= 4.5,
                        "{}: {fg:06X} on {bg:06X}: {ratio:.2}",
                        theme.name
                    );
                }
            }
        }
    }

    #[test]
    fn hex_is_a_padded_css_color() {
        assert_eq!(hex(0x17181C), "#17181C");
        assert_eq!(hex(0x00000A), "#00000A", "leading zeros stay");
        assert_eq!(hex(0xFFFFFF), "#FFFFFF");
    }

    #[test]
    fn the_qr_code_is_dark_on_light_in_both_appearances() {
        let (dark, light) = (Theme::dark(), Theme::light());
        assert_eq!(
            (dark.qr_dark, dark.qr_light),
            (light.qr_dark, light.qr_light),
            "phone cameras read dark modules on a light panel in either theme"
        );
        assert!(
            contrast(light.qr_dark, light.qr_light) >= 7.0,
            "modules stand out from the panel"
        );
        assert!(
            contrast(light.qr_light, 0xFFFFFF) < contrast(light.qr_dark, 0xFFFFFF),
            "the panel is the light one, the modules the dark one (not inverted)"
        );
    }

    #[test]
    fn mode_cycles_system_light_dark() {
        assert_eq!(next_mode(ThemeMode::System), ThemeMode::Light);
        assert_eq!(next_mode(ThemeMode::Light), ThemeMode::Dark);
        assert_eq!(next_mode(ThemeMode::Dark), ThemeMode::System);
    }

    fn names(light: &str, dark: &str) -> ThemeNames {
        ThemeNames {
            light: light.into(),
            dark: dark.into(),
        }
    }

    #[test]
    fn mode_resolves_against_the_os() {
        let n = ThemeNames::from_settings(&Settings::default());
        assert!(Theme::resolve(ThemeMode::System, true, &n).dark);
        assert!(!Theme::resolve(ThemeMode::System, false, &n).dark);
        assert!(!Theme::resolve(ThemeMode::Light, true, &n).dark);
        assert!(Theme::resolve(ThemeMode::Dark, false, &n).dark);
        assert_eq!(
            Theme::resolve(ThemeMode::Dark, false, &n).mode,
            ThemeMode::Dark
        );
    }

    #[test]
    fn default_names_resolve_to_exactly_today() {
        let n = ThemeNames::from_settings(&Settings::default());
        assert_eq!(Theme::resolve(ThemeMode::System, true, &n), Theme::dark());
        assert_eq!(Theme::resolve(ThemeMode::System, false, &n), Theme::light());
    }

    #[test]
    fn unknown_names_fall_back_to_mkrew() {
        let n = names("nope", "dracula");
        assert_eq!(Theme::resolve(ThemeMode::System, true, &n), Theme::dark());
        assert_eq!(Theme::resolve(ThemeMode::System, false, &n), Theme::light());
    }

    #[test]
    fn each_slot_picks_its_name() {
        let n = names("Solarized Light", "Dracula");
        assert_eq!(Theme::resolve(ThemeMode::Dark, false, &n).name, "Dracula");
        assert_eq!(
            Theme::resolve(ThemeMode::Light, true, &n).name,
            "Solarized Light"
        );
        assert_eq!(Theme::resolve(ThemeMode::System, true, &n).name, "Dracula");
    }

    #[test]
    fn a_slot_may_hold_the_other_appearance() {
        let n = names("Mkrew Light", "GitHub Light");
        let theme = Theme::resolve(ThemeMode::Dark, false, &n);
        assert_eq!((theme.name, theme.dark), ("GitHub Light", false));
    }

    #[test]
    fn layout_heights_scale_with_the_ui_font() {
        // 44 px at the default 13 px body; at 20 px the title bar is 44 × 20 / 13.
        assert_eq!(rems_px(44.0), gpui::rems(44.0 / 13.0));
        assert_eq!(rems_px(TITLE_BAR_HEIGHT).0 * 20.0, 48.0 * 20.0 / 13.0);
    }

    /// ✕ on the red hover of the Close button must stay readable in both themes.
    #[test]
    fn window_close_hover_is_readable() {
        for theme in all_themes() {
            let ratio = contrast(theme.on_accent, theme.status_exited);
            assert!(ratio >= 3.0, "{}: {ratio:.2}", theme.name);
        }
    }

    #[test]
    fn on_accent_text_is_readable_on_the_accent() {
        for t in all_themes() {
            assert!(
                contrast(t.on_accent, t.accent) >= 3.0,
                "{}: {:06X}",
                t.name,
                t.accent
            );
        }
    }

    #[test]
    fn selected_text_stays_readable() {
        for theme in all_themes() {
            for bg in [
                theme.terminal_selection_bg,
                theme.terminal_match_bg,
                theme.terminal_match_current_bg,
            ] {
                let ratio = contrast(theme.text, bg);
                assert!(ratio >= 4.5, "{}: {bg:06X}: {ratio:.2}", theme.name);
            }
        }
    }

    #[test]
    fn code_stays_readable() {
        for theme in all_themes() {
            for fg in [
                theme.syntax_keyword,
                theme.syntax_string,
                theme.syntax_number,
                theme.syntax_type,
                theme.syntax_function,
                theme.accent,
            ] {
                // `surface_raised` = the Files viewer's selected line.
                for bg in [theme.bg, theme.surface, theme.surface_raised] {
                    let ratio = contrast(fg, bg);
                    assert!(
                        ratio >= 4.5,
                        "{}: {fg:06X} on {bg:06X}: {ratio:.2}",
                        theme.name
                    );
                }
            }
        }
    }

    /// A toast card is `surface_raised`: its text, its two marks (info accent, error red) and
    /// the ✕ must read on it in both appearances.
    #[test]
    fn toast_text_and_marks_read_on_the_card() {
        for theme in all_themes() {
            let ratio = contrast(theme.text, theme.surface_raised);
            assert!(ratio >= 4.5, "{}: text: {ratio:.2}", theme.name);
            for (name, mark) in [
                ("info", theme.accent_text),
                ("error", theme.status_exited),
                ("close", theme.text_muted),
            ] {
                let ratio = contrast(mark, theme.surface_raised);
                assert!(ratio >= 3.0, "{}: {name}: {ratio:.2}", theme.name);
            }
        }
    }

    #[test]
    fn highlighted_diff_lines_stay_readable() {
        for theme in all_themes() {
            for fg in [
                theme.syntax_keyword,
                theme.syntax_string,
                theme.syntax_number,
                theme.syntax_type,
                theme.syntax_function,
                theme.text,
                // Comments and line numbers.
                theme.text_muted,
            ] {
                for bg in [theme.diff_add_bg, theme.diff_del_bg, theme.diff_mod_bg] {
                    let ratio = contrast(fg, bg);
                    assert!(
                        ratio >= 4.5,
                        "{}: {fg:06X} on {bg:06X}: {ratio:.2}",
                        theme.name
                    );
                }
            }
        }
    }

    /// The Files editor's change bars stand out from the editor and its caret row
    /// (WCAG 1.4.11: graphical objects ≥ 3:1).
    #[test]
    fn change_bars_stand_out_from_the_editor() {
        for theme in all_themes() {
            for fg in [
                theme.diff_add_text,
                theme.diff_mod_text,
                theme.diff_del_text,
            ] {
                for bg in [theme.bg, theme.surface, theme.surface_raised] {
                    let ratio = contrast(fg, bg);
                    assert!(
                        ratio >= 3.0,
                        "{}: {fg:06X} on {bg:06X}: {ratio:.2}",
                        theme.name
                    );
                }
            }
        }
    }

    /// Added, removed and modified (a line changed in place, Split diff) lines read on
    /// their own backgrounds, in both appearances.
    #[test]
    fn diff_line_colors_read_on_their_backgrounds() {
        for theme in all_themes() {
            for (fg, bg) in [
                (theme.diff_add_text, theme.diff_add_bg),
                (theme.diff_del_text, theme.diff_del_bg),
                (theme.diff_mod_text, theme.diff_mod_bg),
                // Line numbers in the gutter of a changed line.
                (theme.text_dim, theme.diff_add_gutter),
                (theme.text_dim, theme.diff_del_gutter),
                (theme.text_dim, theme.diff_mod_gutter),
                // Git status letters on the list and on its selected row.
                (theme.git_modified, theme.bg),
                (theme.git_untracked, theme.bg),
                (theme.git_modified, theme.selected),
                (theme.git_untracked, theme.selected),
            ] {
                let ratio = contrast(fg, bg);
                assert!(
                    ratio >= 4.5,
                    "{}: {fg:06X} on {bg:06X}: {ratio:.2}",
                    theme.name
                );
            }
        }
    }

    #[test]
    fn contrast_matches_known_values() {
        assert!((contrast(0x000000, 0xFFFFFF) - 21.0).abs() < 0.01);
        assert!((contrast(0x777777, 0x777777) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn spacing_and_radius_follow_the_scale() {
        assert_eq!(
            [
                space::XS,
                space::S,
                space::M,
                space::L,
                space::XL,
                space::XXL
            ],
            [4.0, 8.0, 12.0, 16.0, 24.0, 32.0]
        );
        assert_eq!(
            [radius::SMALL, radius::MEDIUM, radius::LARGE],
            [4.0, 8.0, 12.0]
        );
        assert!(motion::FAST >= Duration::from_millis(120));
        assert!(motion::SLOW <= Duration::from_millis(180));
    }

    /// Hue angle (0..360) of an `0xRRGGBB` color.
    fn hue(color: u32) -> f32 {
        let [r, g, b] = [16, 8, 0].map(|shift| ((color >> shift) & 0xFF) as f32);
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let d = max - min;
        if d == 0.0 {
            return 0.0;
        }
        let sector = if max == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        sector * 60.0
    }

    #[test]
    fn workspace_colors_stand_out_from_every_surface_and_from_each_other() {
        for theme in all_themes() {
            // Where a swatch or avatar sits: the dialogs, the menu, the sidebar and its row fills.
            let surfaces = [
                theme.bg,
                theme.surface,
                theme.surface_raised,
                theme.sidebar,
                theme.card,
                theme.hover,
                theme.selected,
            ];
            for (name, color) in WORKSPACE_COLORS.iter().zip(theme.workspace_colors) {
                for bg in surfaces {
                    // WCAG 1.4.11: graphical objects ≥ 3:1.
                    let ratio = contrast(color, bg);
                    assert!(
                        ratio >= 3.0,
                        "{}: {name} {color:06X} on {bg:06X}: {ratio:.2}",
                        theme.name
                    );
                }
            }
            // Clearly distinct: neighbouring hues (round the color wheel) differ by ≥ 12°.
            let mut hues: Vec<f32> = theme.workspace_colors.iter().map(|c| hue(*c)).collect();
            hues.sort_by(f32::total_cmp);
            for pair in hues.windows(2) {
                assert!(pair[1] - pair[0] >= 12.0, "hues too close: {hues:?}");
            }
            let wrap = hues[0] + 360.0 - hues[hues.len() - 1];
            assert!(wrap >= 12.0, "hues too close round the wheel: {hues:?}");
            // Every name has a color; a hex value or unknown name has none.
            for name in WORKSPACE_COLORS {
                assert!(theme.workspace_color(name).is_some(), "{name}");
            }
            let teal = WORKSPACE_COLORS.iter().position(|n| *n == "teal").unwrap();
            assert_eq!(
                theme.workspace_color("teal"),
                Some(theme.workspace_colors[teal])
            );
            assert_eq!(theme.workspace_color("#ff0000"), None);
        }
    }

    #[test]
    fn the_palette_keeps_every_name_a_workspace_may_already_have() {
        for old in [
            "red", "orange", "yellow", "green", "teal", "blue", "purple", "pink",
        ] {
            assert!(Theme::dark().workspace_color(old).is_some(), "{old}");
        }
        assert!(Theme::dark().workspace_color("lime").is_some());
    }

    #[test]
    fn avatar_letters_and_chevrons_stay_readable() {
        for theme in all_themes() {
            for bg in [theme.sidebar, theme.hover, theme.selected] {
                for color in theme.workspace_colors {
                    let (letter, fill) = theme.avatar_colors(color, bg);
                    let ratio = contrast(letter, fill);
                    assert!(
                        ratio >= 4.5,
                        "{}: {letter:06X} on {fill:06X}: {ratio:.2}",
                        theme.name
                    );
                }
                // The chevron is the only sign of expanded / collapsed (WCAG 1.4.11).
                let ratio = contrast(theme.chevron_color(), bg);
                assert!(
                    ratio >= 3.0,
                    "{}: chevron on {bg:06X}: {ratio:.2}",
                    theme.name
                );
            }
        }
    }
}
