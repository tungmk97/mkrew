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

/// Corner radius in px.
pub mod radius {
    /// Badges, small buttons.
    pub const SMALL: f32 = 4.0;
    /// Cards, panes, inputs.
    pub const MEDIUM: f32 = 8.0;
    /// Dialogs, palette.
    pub const LARGE: f32 = 12.0;
}

/// UI font sizes relative to body text (1 rem = `[ui] font_size`, default 13 px, set per
/// frame with `Window::set_rem_size`): 12 meta, 13 body, 15 card title, 20 view title at the
/// default (SPEC §14, ADR 0024).
pub mod font_size {
    use gpui::{Rems, rems};

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

/// In-app toasts auto-hide after this long.
pub const TOAST_DURATION: Duration = Duration::from_secs(4);

/// Status refresh period: one step of the `working` dots and the elapsed/idle time update
/// (SPEC §11, §14).
pub const STATUS_TICK: Duration = Duration::from_millis(500);

/// Diameter of one dot of the `working` mark (SPEC §11).
pub const STATUS_DOT: f32 = 4.0;
/// Gap between the dots of the `working` mark.
pub const STATUS_DOT_GAP: f32 = 2.0;
/// Width of every status mark, so glyph and dot rows line up (3 × 4 + 2 × 2 = 16). These three
/// are px at the default 13 px body and scale with `[ui] font_size` ([`rems_px`]).
pub const STATUS_MARK_WIDTH: f32 = space::L;
/// Narrowest the time column on the right of a session row gets (fits `1h 5m`), so times and
/// titles line up.
pub const SESSION_TIME_WIDTH: f32 = space::XXL;

/// A square glyph button (theme toggle, sidebar +).
pub const ICON_BUTTON_SIZE: f32 = 26.0;
/// Inset of the title bar's segmented control around its segments.
pub const SEGMENT_PADDING: f32 = 2.0;

/// Borders are 1 px and faint.
pub const BORDER_WIDTH: f32 = 1.0;

/// The OS UI font (Segoe UI Variable / SF / …); GPUI resolves this name per platform.
pub const UI_FONT: &str = ".SystemUIFont";

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
    /// The unlit dots of the `working` mark.
    pub const STATUS_DOT_DIM: f32 = 0.35;
    /// Hover-revealed actions (SPEC §14 progressive disclosure): hidden, then fully shown.
    pub const HIDDEN: f32 = 0.0;
    pub const FULL: f32 = 1.0;
    /// The lifted copy of a card that follows the pointer while it is dragged.
    pub const DRAGGING: f32 = 0.9;
}

/// Half the gap between split panes: each pane is inset by it, so panes sit `space::XS` apart
/// and `space::XS` from the tab's edges.
pub const PANE_HALF_GAP: f32 = space::XS / 2.0;

/// Thickness of the accent line under the header of a colored pane (ADR 0037).
pub const PANE_ACCENT_WIDTH: f32 = 2.0;

/// Width of the workspace sidebar.
pub const SIDEBAR_WIDTH: f32 = 280.0;

/// Width of the compact sidebar rail (Toggle compact sidebar): a status mark, a gap and an agent
/// chip plus the rail's padding. In px at the default font size; drawn with [`rems_px`] so it
/// widens with `[ui] font_size` like the avatars and chips inside it.
pub const COMPACT_SIDEBAR_WIDTH: f32 = 56.0;

/// Narrowest and widest the session panel gets when dragged (SPEC §10); `[ui] inspector_width`
/// is clamped to them.
pub const INSPECTOR_MIN_WIDTH: f32 = 240.0;
pub const INSPECTOR_MAX_WIDTH: f32 = 560.0;

/// Width of the grab area on the session panel's left edge.
pub const INSPECTOR_GRIP_WIDTH: f32 = 6.0;

/// Width of the label column of the session card's usage rows (fits "Context" at META size).
pub const INSPECTOR_LABEL_WIDTH: f32 = 56.0;

/// Min height of a board column header row: the Todo header's `New task` button at the
/// default UI font size, so every column's cards start at the same height (ADR 0023).
pub const BOARD_COLUMN_HEADER_HEIGHT: f32 = 28.0;

/// Width of a popover menu (every `menu_panel` popover).
pub const MENU_WIDTH: f32 = 240.0;

/// Width of the query field in a terminal's find bar.
pub const FIND_INPUT_WIDTH: f32 = 200.0;

/// Width of the Files editor's caret.
pub const CARET_WIDTH: f32 = 2.0;

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

/// Height of the Run setup dialog's command list before it scrolls.
pub const RUN_SETUP_MAX_HEIGHT: f32 = 420.0;

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

/// Side of an agent badge chip (ADR 0025), like the design's workspace avatar.
pub const AGENT_CHIP_SIZE: f32 = 18.0;

/// Side of the Mkrew logo glyph in the sidebar header.
pub const LOGO_SIZE: f32 = 18.0;

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
pub const GIT_FILE_ROW_HEIGHT: f32 = 30.0;

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
pub const DIFF_LINE_HEIGHT: f32 = 20.0;

/// Rough width of one char of a Git repository tab label, px: the repo strip shows every tab
/// only when their labels fit.
pub const GIT_REPO_TAB_CHAR_WIDTH: f32 = 7.0;

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

/// Terminal font families by preference; the first one installed is used (SPEC §14).
#[cfg(windows)]
pub const TERMINAL_FONTS: &[&str] = &["Cascadia Code", "Cascadia Mono", "Consolas"];
#[cfg(target_os = "macos")]
pub const TERMINAL_FONTS: &[&str] = &["SF Mono", "Menlo"];
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
            UI_FONT.into()
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

/// The fonts and theme mode of `path` when either differs from `last` / `mode` (`Ok(None)`
/// when unchanged). A broken file is an `Err` with `file:line:col` (§20.0); the caller keeps
/// the last good settings.
pub fn reload(
    path: &Path,
    installed: &[String],
    last: &Fonts,
    mode: ThemeMode,
) -> Result<Option<(Fonts, ThemeMode)>, String> {
    let settings = mkrew_core::settings::load(path).map_err(|error| error.to_string())?;
    let fonts = Fonts::from_settings(&settings, installed);
    let changed = fonts != *last || settings.ui.theme != mode;
    Ok(changed.then_some((fonts, settings.ui.theme)))
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
    /// Board cards and other blocks that sit on `bg`.
    pub card: u32,
    /// A hovered row or quiet button.
    pub hover: u32,
    /// The selected row (sidebar session, list item).
    pub selected: u32,
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
    /// Alpha of a workspace color tint (avatars, [`Theme::tint`]).
    pub tint_alpha: u8,
    /// The shade behind a modal dialog, RGBA.
    pub scrim: u32,
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
    /// ANSI 0–15, harmonized with the tokens above.
    pub ansi: [u32; 16],
    /// Workspace colors, in [`WORKSPACE_COLORS`] order (≥ 3:1 on every surface, ADR 0020).
    pub workspace_colors: [u32; WORKSPACE_COLORS.len()],
    /// Dark appearance (picks diagram images, ADR 0021).
    pub dark: bool,
    /// The chosen mode (`[ui] theme`); `dark` is what it resolved to.
    pub mode: ThemeMode,
    pub ui_font_family: SharedString,
    pub terminal_font_family: SharedString,
    pub terminal_font_size: f32,
    /// Line height as a multiple of the font size.
    pub terminal_line_height: f32,
    /// Padding around the terminal grid (spacing scale).
    pub terminal_padding: f32,
    pub terminal_underline_thickness: f32,
    /// Weight of a dim (SGR 2) foreground blended towards the background, 0–1.
    pub dim_blend: f32,
    /// Opacity of the overlay that dims unfocused panes (~5%).
    pub unfocused_pane_dim: f32,
}

impl gpui::Global for Theme {}

impl Theme {
    /// `mode` resolved against the OS appearance: `System` follows `os_dark` (SPEC §14).
    pub fn resolve(mode: ThemeMode, os_dark: bool) -> Self {
        let dark = match mode {
            ThemeMode::System => os_dark,
            ThemeMode::Light => false,
            ThemeMode::Dark => true,
        };
        let theme = if dark { Self::dark() } else { Self::light() };
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
        self
    }

    pub fn dark() -> Self {
        Self {
            bg: 0x17181C,
            surface: 0x1F2127,
            surface_raised: 0x2C2E37,
            sidebar: 0x131418,
            card: 0x1A1B20,
            hover: 0x22242B,
            selected: 0x262938,
            border: 0x2A2C33,
            divider: 0x24262D,
            text: 0xE6E7EB,
            text_dim: 0xB4B7C0,
            text_muted: 0x9A9CA6,
            subtle: 0x5D616C,
            accent: 0x7C8CFF,
            accent_text: 0x9A9FFF,
            accent_bg: 0x25264A,
            // The dark accent is light: dark text keeps ≥ 3:1 on it (white would not).
            on_accent: 0x17181C,
            on_accent_divider: 0x0000_0040,
            warning_bg: 0x3A3020,
            success_bg: 0x1D3A31,
            danger_bg: 0x3A2327,
            tint_alpha: 0x2E,
            pane_focus_border: 0x4A528E,
            terminal_selection_bg: 0x353B60,
            terminal_match_bg: 0x3F3524,
            terminal_match_current_bg: 0x7A5A1F,
            status_working: 0x7C8CFF,
            status_needs_input: 0xE3A63E,
            status_done: 0x4CB782,
            status_exited: 0xE5676B,
            status_idle: 0x6E7079,
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
            ..Self::shared()
        }
    }

    pub fn light() -> Self {
        Self {
            bg: 0xFFFFFF,
            surface: 0xF6F7F9,
            surface_raised: 0xEEF0F3,
            sidebar: 0xF4F5F7,
            card: 0xFFFFFF,
            hover: 0xEEF0F3,
            selected: 0xE9EAFF,
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
            ..Self::shared()
        }
    }

    fn shared() -> Self {
        Self {
            bg: 0,
            surface: 0,
            surface_raised: 0,
            sidebar: 0,
            card: 0,
            hover: 0,
            selected: 0,
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
            tint_alpha: 0,
            // Black at ~35 %, in both appearances.
            scrim: 0x0000_0059,
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
            ansi: [0; 16],
            workspace_colors: [0; WORKSPACE_COLORS.len()],
            dark: false,
            mode: ThemeMode::System,
            ui_font_family: UI_FONT.into(),
            terminal_font_family: pick_family(TERMINAL_FONTS, &[]).into(),
            terminal_font_size: 13.0,
            terminal_line_height: 1.35,
            terminal_padding: space::S,
            terminal_underline_thickness: 1.0,
            dim_blend: 0.6,
            unfocused_pane_dim: 0.05,
        }
    }

    /// The token of a workspace color name; `None` for an unknown name.
    pub fn workspace_color(&self, name: &str) -> Option<u32> {
        let index = WORKSPACE_COLORS.iter().position(|color| *color == name)?;
        self.workspace_colors.get(index).copied()
    }
}

/// Views read the theme with `cx.theme()` (ADR 0027).
pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Theme {
        self.global::<Theme>()
    }
}

/// Rebuilds the theme global from `mode`, the window's OS appearance and the `Fonts` global.
/// Called at startup, on OS appearance changes, on a settings reload and by the toggle.
pub fn refresh(mode: ThemeMode, window: &Window, cx: &mut App) {
    let os_dark = matches!(
        window.appearance(),
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    );
    let fonts = cx.global::<Fonts>().clone();
    cx.set_global(Theme::resolve(mode, os_dark).with_fonts(&fonts));
}

/// System → Light → Dark → System (the title bar toggle, `ToggleTheme`).
pub fn next_mode(mode: ThemeMode) -> ThemeMode {
    match mode {
        ThemeMode::System => ThemeMode::Light,
        ThemeMode::Light => ThemeMode::Dark,
        ThemeMode::Dark => ThemeMode::System,
    }
}

/// The toggle's glyph: half-filled circle (follows the OS) / sun / moon.
pub fn mode_glyph(mode: ThemeMode) -> &'static str {
    match mode {
        ThemeMode::System => "◐",
        ThemeMode::Light => "☀",
        ThemeMode::Dark => "☾",
    }
}

/// Marks a pinned workspace, session row or tab (SPEC §10). Plain text like the other glyphs:
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
pub const TITLE_BAR_HEIGHT: f32 = 44.0;
pub const TAB_STRIP_HEIGHT: f32 = 38.0;
/// The Git view's repository tabs (multi-repo workspaces): a tab strip.
pub const GIT_REPO_STRIP_HEIGHT: f32 = TAB_STRIP_HEIGHT;
pub const WORKSPACE_ROW_HEIGHT: f32 = 30.0;
pub const SESSION_ROW_HEIGHT: f32 = 28.0;
/// Height of the Plan card's progress bar, and of the thin one under the selected sidebar row
/// (ADR 0039). The track is `Theme::border`, the fill `Theme::accent` (both themes).
pub const PLAN_BAR_HEIGHT: f32 = 4.0;
pub const PLAN_ROW_BAR_HEIGHT: f32 = 2.0;
/// The status bar at the window bottom (SPEC §10), in px at the default font size. It draws
/// with existing tokens (`sidebar` background, `divider` top line, `text_muted`,
/// `status_needs_input`, `accent`, `border`), so dark and light need no new values.
pub const STATUS_BAR_HEIGHT: f32 = 24.0;
/// The context meter's track.
pub const STATUS_BAR_METER_WIDTH: f32 = 48.0;
pub const STATUS_BAR_METER_HEIGHT: f32 = 4.0;
pub const WORKSPACE_AVATAR_SIZE: f32 = 18.0;
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

#[cfg(test)]
mod tests {
    use mkrew_core::settings::{FontSize, Settings};

    use super::*;

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
        let system = ThemeMode::System;
        std::fs::write(&path, "[terminal]\nfont_size = 18\n").unwrap();
        let (grown, mode) = reload(&path, &[], &last, system).unwrap().expect("changed");
        assert_eq!((grown.terminal_size, mode), (18.0, system));
        std::fs::write(&path, "[terminal]\nfont_size = 4\n").unwrap();
        let error = reload(&path, &[], &grown, system).expect_err("out of range");
        assert!(error.contains("settings.toml:2:"), "{error}");
        assert!(error.contains("8–32"), "{error}");
        std::fs::write(&path, "[terminal]\nfont_size = 18\n").unwrap();
        assert_eq!(
            reload(&path, &[], &grown, system),
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
            reload(&path, &[], &grown, system),
            Ok(Some((grown.clone(), ThemeMode::Dark)))
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
        for theme in [Theme::dark(), Theme::light()] {
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
                    assert!(ratio >= 4.5, "{fg:06X} on {bg:06X}: {ratio:.2}");
                }
            }
        }
    }

    #[test]
    fn mode_cycles_system_light_dark() {
        assert_eq!(next_mode(ThemeMode::System), ThemeMode::Light);
        assert_eq!(next_mode(ThemeMode::Light), ThemeMode::Dark);
        assert_eq!(next_mode(ThemeMode::Dark), ThemeMode::System);
    }

    #[test]
    fn mode_resolves_against_the_os() {
        assert!(Theme::resolve(ThemeMode::System, true).dark);
        assert!(!Theme::resolve(ThemeMode::System, false).dark);
        assert!(!Theme::resolve(ThemeMode::Light, true).dark);
        assert!(Theme::resolve(ThemeMode::Dark, false).dark);
        assert_eq!(Theme::resolve(ThemeMode::Dark, false).mode, ThemeMode::Dark);
    }

    #[test]
    fn layout_heights_scale_with_the_ui_font() {
        // 44 px at the default 13 px body; at 20 px the title bar is 44 × 20 / 13.
        assert_eq!(rems_px(44.0), gpui::rems(44.0 / 13.0));
        assert_eq!(rems_px(TITLE_BAR_HEIGHT).0 * 20.0, 44.0 * 20.0 / 13.0);
    }

    /// ✕ on the red hover of the Close button must stay readable in both themes.
    #[test]
    fn window_close_hover_is_readable() {
        for theme in [Theme::dark(), Theme::light()] {
            let ratio = contrast(theme.on_accent, theme.status_exited);
            assert!(ratio >= 3.0, "{ratio:.2}");
        }
    }

    #[test]
    fn on_accent_text_is_readable_on_the_accent() {
        for t in [Theme::dark(), Theme::light()] {
            assert!(contrast(t.on_accent, t.accent) >= 3.0, "{:06X}", t.accent);
        }
    }

    #[test]
    fn selected_text_stays_readable() {
        for theme in [Theme::dark(), Theme::light()] {
            for bg in [
                theme.terminal_selection_bg,
                theme.terminal_match_bg,
                theme.terminal_match_current_bg,
            ] {
                let ratio = contrast(theme.text, bg);
                assert!(ratio >= 4.5, "{bg:06X}: {ratio:.2}");
            }
        }
    }

    #[test]
    fn code_stays_readable() {
        for theme in [Theme::dark(), Theme::light()] {
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
                    assert!(ratio >= 4.5, "{fg:06X} on {bg:06X}: {ratio:.2}");
                }
            }
        }
    }

    #[test]
    fn highlighted_diff_lines_stay_readable() {
        for theme in [Theme::dark(), Theme::light()] {
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
                    assert!(ratio >= 4.5, "{fg:06X} on {bg:06X}: {ratio:.2}");
                }
            }
        }
    }

    /// Added, removed and modified (a line changed in place, Split diff) lines read on
    /// their own backgrounds, in both appearances.
    #[test]
    fn diff_line_colors_read_on_their_backgrounds() {
        for theme in [Theme::dark(), Theme::light()] {
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
                assert!(ratio >= 4.5, "{fg:06X} on {bg:06X}: {ratio:.2}");
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
        for theme in [Theme::dark(), Theme::light()] {
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
                        "{name} {color:06X} on {bg:06X} (dark: {}): {ratio:.2}",
                        theme.dark
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
        for theme in [Theme::dark(), Theme::light()] {
            for bg in [theme.sidebar, theme.hover, theme.selected] {
                for color in theme.workspace_colors {
                    let (letter, fill) = theme.avatar_colors(color, bg);
                    let ratio = contrast(letter, fill);
                    assert!(ratio >= 4.5, "{letter:06X} on {fill:06X}: {ratio:.2}");
                }
                // The chevron is the only sign of expanded / collapsed (WCAG 1.4.11).
                let ratio = contrast(theme.chevron_color(), bg);
                assert!(ratio >= 3.0, "chevron on {bg:06X}: {ratio:.2}");
            }
        }
    }
}
