//! Design tokens (SPEC §14). Every color, spacing, radius, font size and duration used by the
//! UI comes from here; components never hard-code values. Colors are `0xRRGGBB`.

use std::path::Path;
use std::time::Duration;

use gpui::{App, Rems, SharedString, Window, WindowAppearance, rems};
use mkrew_core::settings::Settings;
pub use mkrew_core::settings::ThemeMode;
use mkrew_protocol::WORKSPACE_COLORS;

use crate::keymap::Os;
use crate::theme_catalog::ThemeCatalog;

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

/// Status refresh period: one step of the loading dots and the elapsed/idle time update
/// (SPEC §11, §14).
pub const STATUS_TICK: Duration = Duration::from_millis(500);

/// A code block's Copy button reads "Copied" this long after a click (ADR 0105).
pub const COPIED_FEEDBACK: Duration = Duration::from_millis(1500);

/// The Phone access poll clock (ADR 0053): while its dialog is open `daemon.status` and
/// Tailscale are read on every tick; closed, `daemon.status` alone every [`PHONE_POLL_IDLE`]
/// (it keeps the title bar icon true).
pub const PHONE_TICK: Duration = Duration::from_secs(1);
pub const PHONE_POLL_IDLE: Duration = Duration::from_secs(5);
/// While the Ports dialog is open, `port.list` is asked on every tick (spec 2026-11-01).
pub const PORTS_TICK: Duration = Duration::from_secs(2);

/// Diameter of one dot of the loading mark (SPEC §11).
pub const STATUS_DOT: f32 = 4.0;
/// Gap between the dots of the loading mark.
pub const STATUS_DOT_GAP: f32 = 2.0;
/// Width of every status mark, so glyph and dot rows line up (3 × 4 + 2 × 2 = 16). These three
/// are px at the default 13 px body and scale with `[ui] font_size` ([`rems_px`]).
pub const STATUS_MARK_WIDTH: f32 = space::L;
/// The sidebar's narrower mark slot: one still dot while working, else the glyph.
pub const SIDEBAR_MARK_WIDTH: f32 = space::S;

/// Voice input waveform in the recording pill (§20G): bars this wide and this far apart, from
/// `MIN` to `MAX` tall (px at the default 13 px body, [`rems_px`]), redrawn this often while
/// recording.
pub const VOICE_BAR_WIDTH: f32 = 3.0;
pub const VOICE_BAR_GAP: f32 = 2.0;
pub const VOICE_BAR_MIN: f32 = 3.0;
pub const VOICE_BAR_MAX: f32 = 16.0;
pub const VOICE_METER_TICK: Duration = Duration::from_millis(60);
/// Per meter tick the drawn level closes this share of the gap to the microphone: quick up,
/// slow down.
pub const VOICE_RISE: f32 = 0.6;
pub const VOICE_FALL: f32 = 0.15;
/// How full the bars stay in silence (they still breathe: the microphone is live).
pub const VOICE_IDLE: f32 = 0.2;
/// One run of the wave across the bars; each bar lags its neighbour by `STEP` radians; the wave
/// takes up to `DEPTH` of a bar's height.
pub const VOICE_WAVE_PERIOD: Duration = Duration::from_millis(900);
pub const VOICE_WAVE_STEP: f32 = 0.9;
pub const VOICE_WAVE_DEPTH: f32 = 0.35;
/// The breathing ring on the field or terminal pane that gets the text, over its own border.
pub const VOICE_RING_WIDTH: f32 = 2.0;
/// The recording pill slides up this far as it fades in.
pub const VOICE_PILL_SLIDE: f32 = space::S;

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
/// The font's ascent is taller than its descent, so one-line UI text sits about 1.5 px below
/// the middle of its box. A control that must read as centered (the tab name, the address
/// field) lifts its text by this much: bottom padding of twice it under `items_center`.
pub const TEXT_LIFT: f32 = 1.5;
/// A pane icon in a tab chip: a step under [`ICON_SIZE_SMALL`], so it sits with the name.
pub const TAB_ICON_SIZE: f32 = 12.0;
/// The pin before the time of a pinned session row.
pub const PIN_ICON_SIZE: f32 = 13.0;
/// Inset of a segmented control around its segments.
pub const SEGMENT_PADDING: f32 = 3.0;
/// The thin vertical rule before the window controls.
pub const TITLE_RULE_HEIGHT: f32 = 18.0;
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

/// Size of a new Space window (with no saved place).
pub const WINDOW_WIDTH: f32 = 1200.0;
pub const WINDOW_HEIGHT: f32 = 760.0;
/// Smallest Space window: the title bar's contents fit (ADR 0034).
pub const WINDOW_MIN_WIDTH: f32 = 1100.0;
pub const WINDOW_MIN_HEIGHT: f32 = 560.0;
/// Smallest detached window (ADR 0096): a title bar with tabs and one pane fit.
pub const DETACHED_MIN_WIDTH: f32 = 480.0;
pub const DETACHED_MIN_HEIGHT: f32 = 320.0;
/// Where a detached window opened from the keyboard goes: this far right and down of the
/// window it came from.
pub const DETACHED_OFFSET: f32 = 32.0;
/// One ─ □ ✕ button of the Windows title bar (the system caption button width).
pub const WINDOW_CONTROL_WIDTH: f32 = 46.0;
/// Room for the macOS traffic lights at the left of the title bar.
pub const MAC_TRAFFIC_LIGHTS_WIDTH: f32 = 72.0;
/// Distance of the macOS traffic lights from the window's left edge, px.
pub const MAC_TRAFFIC_LIGHT_X: f32 = 12.0;
/// Height of one traffic light button (AppKit's standard 14 px circle); the lights are
/// centered on the title bar.
pub const MAC_TRAFFIC_LIGHT_SIZE: f32 = 14.0;
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
    /// A non-matching item that stays readable and clickable (Settings pages without a search
    /// match).
    pub const DIMMED: f32 = 0.6;
    /// The unlit dots of the loading mark.
    pub const STATUS_DOT_DIM: f32 = 0.35;
    /// Hover-revealed actions (SPEC §14 progressive disclosure): hidden, then fully shown.
    pub const HIDDEN: f32 = 0.0;
    pub const FULL: f32 = 1.0;
    /// The lifted copy of a card that follows the pointer while it is dragged.
    pub const DRAGGING: f32 = 0.9;
    /// A sidebar row or tab while it is dragged, left dim in its old place.
    pub const DRAG_SOURCE: f32 = 0.4;
}

/// Least contrast ratio between [`Theme::pane_gap`] and [`Theme::terminal_bg`] (Tern measures
/// about 1.07:1). A unit test holds every built-in theme to it and [`Theme::from_palette`]
/// derives the gap to meet it.
pub const PANE_GAP_MIN_CONTRAST: f32 = 1.06;

/// Thickness of the stripe along the top edge of a pane (needs input, session color or focus
/// among several, ADR 0108) and of an underlined tab (the Git panel's repo and section tabs).
pub const PANE_ACCENT_WIDTH: f32 = 2.0;
/// The drag handle at the top centre of a pane in a split tab (ADR 0110): while the pointer is
/// over the pane a slim bar shows; over the handle it becomes a chip with a six-dot grip, and
/// dragging it moves the pane. Only the handle takes the mouse, not the pane's whole top edge.
pub const PANE_GRIP_WIDTH: f32 = 40.0;
pub const PANE_GRIP_HEIGHT: f32 = 20.0;
/// The slim bar of the handle, shown while the pointer is over the pane.
pub const PANE_GRIP_BAR_WIDTH: f32 = 32.0;
pub const PANE_GRIP_BAR_HEIGHT: f32 = 4.0;

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

/// Smallest and largest share of the Markdown Split view the source takes (SPEC §12A);
/// `[ui] md_split` and the divider drag are clamped to them.
pub const MD_SPLIT_MIN_SHARE: f32 = 0.2;
pub const MD_SPLIT_MAX_SHARE: f32 = 0.8;

/// Width of the grab area of a draggable edge: the sidebar's, the session panel's, the
/// dividers between panes and the Markdown Split view's divider.
pub const GRIP_WIDTH: f32 = 6.0;

/// Width of the label column of the session card's usage rows (fits "Context" at META size).
pub const INSPECTOR_LABEL_WIDTH: f32 = 56.0;

/// Min height of a board column header row: the Todo header's `New task` button at the
/// default UI font size, so every column's cards start at the same height (ADR 0023).
pub const BOARD_COLUMN_HEADER_HEIGHT: f32 = 28.0;

/// Width of a popover menu (`render_menu`'s default).
pub const MENU_WIDTH: f32 = 240.0;

/// Width of the Run menu: a command row also holds its ports and ↻ / ■ / ▶ buttons.
pub const RUN_MENU_WIDTH: f32 = 280.0;

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

/// The palette's Files scope asks `fs.find` this long after the last keystroke (spec 7.2).
pub const PALETTE_FILES_DELAY: Duration = Duration::from_millis(80);

/// The palette's Search scope asks `fs.search` this long after the last keystroke.
pub const PALETTE_SEARCH_DELAY: Duration = Duration::from_millis(150);

/// Minimum width of a tab title while it is renamed inline.
pub const TAB_RENAME_MIN_WIDTH: f32 = 120.0;

/// Narrowest the file name of a file pane's header gets: in a narrow pane its buttons give way.
pub const FILE_NAME_MIN_WIDTH: f32 = 96.0;

/// Narrowest a board column gets; below that the board scrolls sideways.
pub const BOARD_COLUMN_MIN_WIDTH: f32 = 200.0;

/// Thickness of the accent line that marks where a dragged card lands.
pub const DROP_INDICATOR_HEIGHT: f32 = 2.0;

/// Width of a modal dialog (the task dialog).
pub const DIALOG_WIDTH: f32 = 520.0;

/// The Settings dialog (ADR 0073): its size, the page list width and the value column.
pub const SETTINGS_DIALOG_WIDTH: f32 = 880.0;
pub const SETTINGS_DIALOG_HEIGHT: f32 = 640.0;
pub const SETTINGS_NAV_WIDTH: f32 = 200.0;
pub const SETTINGS_CONTROL_WIDTH: f32 = 240.0;
/// Tallest the font pick's list grows (it scrolls past that).
pub const SETTINGS_FONT_LIST_HEIGHT: f32 = 200.0;
/// Settings → Prompts (spec 2026-11-04 section 7.2): the prompt list's width, how tall it grows
/// before it scrolls, and the height of the prompt text field (it scrolls past that).
pub const SETTINGS_PROMPTS_LIST_WIDTH: f32 = 200.0;
pub const SETTINGS_PROMPTS_LIST_HEIGHT: f32 = 360.0;
pub const SETTINGS_PROMPT_BODY_HEIGHT: f32 = 240.0;
/// The theme gallery of Settings → Appearance (ADR 0101): a card is this wide (three fit a
/// row at the default Settings width), the picture of a window in the card's own colors is
/// this tall, and its parts: the top strip, the left strip, the two text bars, the accent
/// pill and the six ANSI dots.
pub const THEME_CARD_WIDTH: f32 = 200.0;
pub const THEME_PICTURE_HEIGHT: f32 = 96.0;
pub const THEME_PICTURE_CHROME: f32 = 12.0;
pub const THEME_PICTURE_SIDEBAR: f32 = 36.0;
pub const THEME_BAR_HEIGHT: f32 = 6.0;
pub const THEME_BAR_LONG: f32 = 72.0;
pub const THEME_BAR_SHORT: f32 = 48.0;
pub const THEME_PILL_HEIGHT: f32 = 10.0;
pub const THEME_PILL_WIDTH: f32 = 28.0;
pub const THEME_DOT: f32 = 8.0;
/// How long the pointer rests on a gallery card before the app shows that theme, so moving
/// across the cards does not flash.
pub const THEME_PREVIEW_DELAY: Duration = Duration::from_millis(120);
/// An on/off switch: track size and the knob's inset.
pub const SWITCH_WIDTH: f32 = 32.0;
pub const SWITCH_HEIGHT: f32 = 18.0;
pub const SWITCH_INSET: f32 = 2.0;

/// Side of the Phone access QR code (ADR 0053): px at the default 13 px body, scaled with
/// `[ui] font_size` ([`rems_px`]). Big enough for a camera across a desk.
pub const PHONE_QR_SIZE: f32 = 216.0;

/// Width of a confirmation dialog (every `window.prompt`, drawn in the window).
pub const CONFIRM_DIALOG_WIDTH: f32 = 420.0;

/// Width of the Run setup dialog and its folder / env file picker (wide: commands and paths are
/// long).
pub const RUN_SETUP_WIDTH: f32 = 760.0;
/// Height of the Run setup dialog's command list before it scrolls (it also shrinks to fit
/// the window).
pub const RUN_SETUP_MAX_HEIGHT: f32 = 560.0;
/// Widest a folded Run setup card's name or folder gets before it is cut with `…`.
pub const RUN_NAME_MAX_WIDTH: f32 = 240.0;

/// Height of the prompt field in the task dialog (it scrolls past that).
pub const PROMPT_FIELD_HEIGHT: f32 = 140.0;

/// Tallest an agent pane's prompt composer grows (it scrolls past that, ~8 lines; §20B.1).
pub const COMPOSER_MAX_HEIGHT: f32 = 160.0;

/// The Chat page's centered column: the list, the tray, the question card and the composer
/// card line up in it (ADR 0105).
pub const CHAT_COLUMN_WIDTH: f32 = 760.0;

/// Width of the todo popover an omp pane opens from its hud (TSP `checklist`, ADR 0107).
pub const TSP_POPOVER_WIDTH: f32 = 420.0;

/// Tallest share of the pane's list area the todo popover takes; it scrolls past that.
pub const TSP_POPOVER_MAX_FRACTION: f32 = 0.6;

/// Shortest height the popover is capped at, before the list has been laid out.
pub const TSP_POPOVER_MIN_HEIGHT: f32 = 120.0;

/// The accent bar on the left of the picked row of an omp `ask` card.
pub const TSP_ASK_BAR_WIDTH: f32 = 2.0;

/// The ring of an omp `ask` timeout.
pub const TSP_RING_SIZE: f32 = 24.0;

/// Space at both sides of the Chat page's centered column (`chat_column`, ADR 0105).
pub const CHAT_COLUMN_PADDING: f32 = space::XL;

/// Width of the key column of a TSP `kv` and of a `progress` or `meter` bar.
pub const TSP_KEY_WIDTH: f32 = 96.0;

/// Widest an `overlay` of `size` `sm` (and `md`) is in a TSP pane; `lg` and `full` take the
/// Chat column.
pub const TSP_OVERLAY_SM_WIDTH: f32 = 384.0;
pub const TSP_OVERLAY_MD_WIDTH: f32 = 576.0;

/// Widest an inline TSP `image` is.
pub const TSP_IMAGE_MAX_WIDTH: f32 = 256.0;

/// Widest share of the Chat page a prompt card takes (it sits at the right).
pub const CHAT_PROMPT_MAX_FRACTION: f32 = 0.8;

/// How far one arrow key scrolls the chat list, in px.
pub const CHAT_SCROLL_STEP: f32 = 48.0;

/// Tallest the composer's list of queued texts grows (it scrolls past that, ~4 rows; §20B.2).
pub const COMPOSER_QUEUE_MAX_HEIGHT: f32 = 88.0;

/// Room the Chat composer's chips are reckoned to take (px at 13 px UI size, the `space::S`
/// gap after each included), to decide which ones a narrow pane drops (ADR 0105). Every part
/// of the row except the stats cell is `flex_none`, so these must not under-estimate:
/// padding, border, icon and gap tokens plus the text of the longest usual value.
/// The model pill: 2 x `space::S` padding, 1 px borders, `opus 5.5` / `sonnet 4.5`, a gap and
/// the chevron.
pub const COMPOSER_CHIP_MODEL_WIDTH: f32 = 108.0;
/// The loading dots (three dots and a gap) before the model name while the agent works.
pub const COMPOSER_CHIP_LOADING_WIDTH: f32 = 20.0;
/// `medium`, `xhigh`.
pub const COMPOSER_CHIP_EFFORT_WIDTH: f32 = 56.0;
/// The lightning icon and `on-request`.
pub const COMPOSER_CHIP_PERMISSION_WIDTH: f32 = 92.0;
/// The branch chip never grows past this (icon and name; the name truncates).
pub const COMPOSER_BRANCH_MAX_WIDTH: f32 = 160.0;
/// One character of the terminal font at `META` size.
pub const COMPOSER_MONO_CHAR_WIDTH: f32 = 7.2;
/// Context tokens, `tok/s` and the elapsed time with their icons and gaps.
pub const COMPOSER_CHIP_STATS_WIDTH: f32 = 190.0;
/// The clock time of the last turn's end (icon and `09:44:27`).
pub const COMPOSER_CHIP_CLOCK_WIDTH: f32 = 68.0;
/// The ✦ button (2 x `space::M` padding and an icon) and its gap.
pub const COMPOSER_ENHANCE_WIDTH: f32 = 46.0;
/// Prompts (icon only: the same button) and the send button, with their gaps.
pub const COMPOSER_FIXED_WIDTH: f32 = 82.0;
/// The Chat composer's round send button.
pub const SEND_BUTTON_SIZE: f32 = 28.0;
/// The thin context meter above the Chat composer card.
pub const CONTEXT_METER_HEIGHT: f32 = 2.0;

/// The hover area of a tick on the context meter of an omp dock (a mark such as the
/// auto-compact point; its tooltip is the mark's title). The 1 px line sits in the middle.
pub const CONTEXT_MARK_HIT_WIDTH: f32 = space::M;
pub const CONTEXT_MARK_HIT_HEIGHT: f32 = CONTEXT_METER_HEIGHT + space::XS;

/// Room reckoned for the bar of an omp dock (px at 13 px UI size, like the `COMPOSER_*` ones):
/// the round send or stop button and its gap.
pub const TSP_BAR_FIXED_WIDTH: f32 = SEND_BUTTON_SIZE + space::S;
/// The model pill without its name: padding, border, chevron, gaps and the gap after it.
pub const TSP_MODEL_CHROME_WIDTH: f32 = 48.0;
/// The model name never truncates below this many characters.
pub const TSP_MODEL_MIN_CHARS: usize = 6;
/// The four-step effort glyph of the effort chip.
pub const TSP_EFFORT_GLYPH_WIDTH: f32 = 28.0;
/// Characters reckoned for a live elapsed time inside a fact (`12m 03s`).
// ponytail: `1h 02m 03s` is 10 characters and clips (the facts cell is `overflow_hidden`, so
// nothing else moves); measure the node's text if long sessions matter.
pub const TSP_ELAPSED_CHARS: usize = 7;
/// One average character of the UI font at `META` size (the effort label, the model name); the
/// facts and the cost are mono and use `COMPOSER_MONO_CHAR_WIDTH`.
pub const TSP_UI_CHAR_WIDTH: f32 = 6.6;

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

/// The Ports dialog (spec 2026-11-01): its width and the height its rows scroll past.
pub const PORTS_DIALOG_WIDTH: f32 = 640.0;
pub const PORTS_LIST_HEIGHT: f32 = 400.0;

/// Width of the "Ask AI" dialog, and the height its answer scrolls past (ADR 0061).
pub const ASK_DIALOG_WIDTH: f32 = 640.0;
pub const ASK_ANSWER_HEIGHT: f32 = 360.0;

/// Share of the window's width and height the "History" dialog takes (it replays a terminal).
pub const HISTORY_DIALOG_FRACTION: f32 = 0.9;

/// Width of the "History" dialog's session list.
pub const HISTORY_LIST_WIDTH: f32 = 280.0;

/// Width of the Docs view's file list.
pub const DOC_LIST_WIDTH: f32 = 240.0;

/// Max width of the rendered Markdown preview: wide enough for tables and diagrams, still a
/// readable line (≈ 120 characters of body text).
pub const DOC_PREVIEW_MAX_WIDTH: f32 = 960.0;

/// An expanded Mermaid diagram (its dialog takes `HISTORY_DIALOG_FRACTION` of the window) is
/// drawn at most this many times its natural size: gpui rasterizes SVGs at 2x, so it stays
/// sharp up to there.
pub const DIAGRAM_ZOOM_MAX: f32 = 2.0;

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

/// Indent per folder level in the Files tree.
pub const FILE_TREE_INDENT: f32 = space::M;

/// Side of the dot on a Files tree folder holding changes.
pub const FILE_CHANGES_DOT_SIZE: f32 = 6.0;

/// Height of a Files tree row (the tree is a uniform list: every row is this high). Dense like
/// Tern's trees (ADR 0110).
pub const TREE_ROW_HEIGHT: f32 = 24.0;

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

/// Floating panels (ADR 0097): how far the accent tint gradient reaches down from the top,
/// its direction (180 = top to bottom in gpui) and color stops, and how far a panel rises
/// while it fades in (gpui has no element scale).
pub const GLASS_TINT_HEIGHT: f32 = 120.0;
pub const GLASS_TINT_ANGLE: f32 = 180.0;
pub const GLASS_TINT_FROM: f32 = 0.0;
pub const GLASS_TINT_TO: f32 = 1.0;
pub const PANEL_RISE: f32 = space::XS;
/// Max height of the composer's prompt and slash pickers (they scroll past that).
pub const PICKER_LIST_HEIGHT: f32 = 360.0;
/// The scoped palette.
pub const PALETTE_WIDTH: f32 = 640.0;
pub const PALETTE_LIST_HEIGHT: f32 = 480.0;
pub const PALETTE_SELECTED_BAR: f32 = 3.0;
pub const PALETTE_GROUP_LIMIT: usize = 5;
/// Distance from the window's top edge to the top of the palette panel (it stays there while
/// its list grows or shrinks between scopes).
pub const PALETTE_TOP_OFFSET: f32 = 64.0;
/// Widest share of a palette row its title may take while a muted detail follows (the title
/// is cut with `…` past that).
pub const PALETTE_TITLE_MAX_FRACTION: f32 = 0.6;

/// How much of a `/` picker row's width its command name and its argument hint may take before
/// they are cut with `…` (the description takes what is left, the source chip never shrinks).
pub const SLASH_NAME_MAX_FRACTION: f32 = 0.4;
pub const SLASH_HINT_MAX_FRACTION: f32 = 0.25;

/// Tallest the Git view's commit message box grows (it scrolls past that, ~8 lines).
pub const GIT_COMMIT_BOX_MAX_HEIGHT: f32 = 160.0;

/// One row of the Git changed-file list (uniform list: every row has this box; ADR 0110).
pub const GIT_FILE_ROW_HEIGHT: f32 = 28.0;
/// The box of the change letter (M, A, D …) of a Git file row: the letter is colored, the box
/// has no fill (ADR 0110); it keeps the names in one column.
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

/// Narrowest the Git tab's "Filter files" field gets before the "By:" button wraps under it.
pub const GIT_FILTER_MIN_WIDTH: f32 = 120.0;

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
    /// The accent dot before an unnumbered list item (ADR 0105).
    pub const BULLET_SIZE: f32 = 6.0;
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

/// How long the tab strip's `+` row stays open after the pointer left `+` and the row, so the
/// pointer can travel to its items (`plus_row::Reveal`).
pub const PLUS_ROW_COLLAPSE_DELAY: Duration = Duration::from_millis(150);

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

/// `text` as a TOML string literal (quoted and escaped): the value `write_ui_value` takes,
/// so a theme name with a quote or a backslash cannot break `settings.toml`.
pub fn toml_string(text: &str) -> String {
    toml::Value::String(text.to_owned()).to_string()
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
    /// Inputs and segmented controls on the title bar, and the sidebar of derived themes. The
    /// title bar itself is `pane_gap`.
    pub chrome: u32,
    /// Board cards and other blocks that sit on `bg`.
    pub card: u32,
    /// The fill of the shown tab chip in the title bar; always differs from `pane_gap` (the
    /// "active tab" rule of `problems`).
    pub tab_active: u32,
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
    /// The shade behind a modal dialog, RGBA: black at 24 % (dark) or 16 % (light), set by
    /// [`Theme::with_glass`].
    pub scrim: u32,
    /// Fill of every floating panel, RGBA: `surface` at 0.96 (dark) / 0.97 (light) or more, as
    /// much as body text needs to keep 4.5:1 over a light or dark page behind it.
    pub glass: u32,
    /// 1 px inner border of a floating panel, RGBA: white (dark) or black (light) at 8 %.
    pub glass_border: u32,
    /// Top of the vertical gradient over the first `GLASS_TINT_HEIGHT` px of a floating panel,
    /// RGBA: `accent` at 5 % (dark) / 4 % (light).
    pub glass_tint: u32,
    /// The QR code of Phone access (SPEC §14): dark modules on a light panel in both
    /// appearances, because phone cameras read dark-on-light only; so both are set in
    /// `shared()`, not in `dark()` / `light()`.
    pub qr_dark: u32,
    pub qr_light: u32,
    /// The title bar fill and what shows behind the panes before they paint (ADR 0108, ADR
    /// 0109): a step off `terminal_bg` (darker, or lighter when `terminal_bg` has no room below
    /// it), at least [`PANE_GAP_MIN_CONTRAST`] away, so the bar reads apart from the panes.
    pub pane_gap: u32,
    /// Faint accent: the top stripe of the focused split pane, and the border of a picked card,
    /// row or comment (accent halfway to `bg`, SPEC §14).
    pub pane_focus_stripe: u32,
    /// The ring of a keyboard-focused dialog control (ADR 0085): a 1 px border in the text
    /// color, so it reads on `surface` and on an accent button in every theme.
    pub focus_ring: u32,
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
    /// The tint of an agent's brand logo in a tab chip (the logo's own color, readable on
    /// `chrome`: 3:1 as an icon, checked by `problems`).
    pub brand_claude: u32,
    pub brand_codex: u32,
    pub brand_copilot: u32,
    pub brand_omp: u32,
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
    /// The theme this is, e.g. "Mkrew Dark" (ADR 0078); owned because the name of a theme
    /// file is not `'static` (ADR 0101).
    pub name: SharedString,
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
    /// Opacity of the overlay that dims unfocused panes among several (20 %, ADR 0109).
    pub unfocused_pane_dim: f32,
    /// The palette from before the restyle, kept for the sidebar's session list (projects,
    /// session rows, RUN), which the restyle leaves as it was. `None` on that palette itself.
    pub classic: Option<std::sync::Arc<Theme>>,
}

impl gpui::Global for Theme {}

impl Theme {
    /// The theme of `mode`'s slot (SPEC §14, ADR 0078, ADR 0101); a name the catalog does
    /// not know (a removed file) is that slot's Mkrew theme.
    pub fn resolve(
        mode: ThemeMode,
        os_dark: bool,
        names: &ThemeNames,
        catalog: &ThemeCatalog,
    ) -> Self {
        let dark = slot_is_dark(mode, os_dark);
        let theme = catalog.find(names.slot(dark)).map_or_else(
            || if dark { Self::dark() } else { Self::light() },
            |listed| listed.theme.clone(),
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

    /// The ink of the taskbar badge on `status_needs_input`: `bg` or `text`, whichever has
    /// the better contrast with it.
    pub fn badge_ink(&self) -> u32 {
        if contrast(self.bg, self.status_needs_input)
            >= contrast(self.text, self.status_needs_input)
        {
            self.bg
        } else {
            self.text
        }
    }

    /// The tint of `logo` in a tab chip.
    pub fn brand_color(&self, logo: crate::icons::AgentLogo) -> u32 {
        use crate::icons::AgentLogo;
        match logo {
            AgentLogo::Claude => self.brand_claude,
            AgentLogo::Codex => self.brand_codex,
            AgentLogo::Copilot => self.brand_copilot,
            AgentLogo::Omp => self.brand_omp,
        }
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
            tab_active: 0x2A2E35,
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
            // Black: 1.07:1 under `terminal_bg` (0B0C0E), the ratio Tern measures.
            pane_gap: 0x000000,
            pane_focus_stripe: 0x4A528E,
            focus_ring: 0xECEDEF,
            terminal_selection_bg: 0x353B60,
            terminal_match_bg: 0x3F3524,
            terminal_match_current_bg: 0x7A5A1F,
            status_working: 0x8E8CF5,
            status_needs_input: 0xE4A452,
            status_done: 0x57C78A,
            status_exited: 0xEF7B6C,
            status_idle: 0x7D838C,
            brand_claude: 0xD97757,
            brand_codex: 0x10A37F,
            brand_copilot: 0xA371F7,
            brand_omp: 0x56B6C2,
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
            name: "Mkrew Dark".into(),
            ..Self::shared()
        }
        .with_glass();
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
            tab_active: 0xE6E8EC,
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
            // 1.07:1 under `terminal_bg` (FCFCFD); 1.11:1 from `tab_active`, 1.04:1 from `hover`.
            pane_gap: 0xF3F4F6,
            pane_focus_stripe: 0xA7ADEA,
            focus_ring: 0x1B1D22,
            terminal_selection_bg: 0xCACEF2,
            terminal_match_bg: 0xF6EBCF,
            terminal_match_current_bg: 0xE9C46A,
            status_working: 0x5452D0,
            status_needs_input: 0x8B570D,
            status_done: 0x23854F,
            status_exited: 0xC2412F,
            status_idle: 0x8A8F99,
            brand_claude: 0xBF5B3A,
            brand_codex: 0x0B7F62,
            brand_copilot: 0x7C4DD6,
            brand_omp: 0x0E7490,
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
            name: "Mkrew Light".into(),
            ..Self::shared()
        }
        .with_glass();
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
            tab_active: 0x262A3D,
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
            pane_focus_stripe: 0x4A528E,
            focus_ring: 0xE4E5E9,
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
            tab_active: 0xE9EAFF,
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
            pane_focus_stripe: 0xA7ADEA,
            focus_ring: 0x1B1D22,
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
            tab_active: 0,
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
            // Glass and scrim are derived from `surface` / `accent` by `with_glass`.
            scrim: 0,
            glass: 0,
            glass_border: 0,
            glass_tint: 0,
            // The QR code of Phone access: near-black modules on white, in both appearances.
            qr_dark: 0x101114,
            qr_light: 0xFFFFFF,
            pane_gap: 0,
            pane_focus_stripe: 0,
            focus_ring: 0,
            terminal_selection_bg: 0,
            terminal_match_bg: 0,
            terminal_match_current_bg: 0,
            status_working: 0,
            status_needs_input: 0,
            status_done: 0,
            status_exited: 0,
            status_idle: 0,
            brand_claude: 0,
            brand_codex: 0,
            brand_copilot: 0,
            brand_omp: 0,
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
            name: "".into(),
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
            unfocused_pane_dim: 0.2,
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
    /// Sidebar, title bar.
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

impl Palette {
    /// The palette a theme can be written back as (the two Mkrew themes have none of their
    /// own): its own background, chrome, surface, text, muted text, accent and ANSI colors,
    /// with the six hues taken from ANSI red to cyan.
    pub fn from_theme(theme: &Theme) -> Self {
        let a = theme.ansi;
        Self {
            dark: theme.dark,
            bg: theme.bg,
            chrome: theme.chrome,
            surface: theme.surface,
            fg: theme.text,
            muted: theme.text_muted,
            accent: theme.accent,
            red: a[1],
            green: a[2],
            yellow: a[3],
            blue: a[4],
            magenta: a[5],
            cyan: a[6],
            ansi: a,
        }
    }
}

/// `a` blended toward `b` by `t` (0 = `a`, 1 = `b`), per RGB channel.
pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    [16, 8, 0].into_iter().fold(0, |out, shift| {
        let (x, y) = (((a >> shift) & 0xFF) as f32, ((b >> shift) & 0xFF) as f32);
        out | (((x + (y - x) * t).round() as u32) << shift)
    })
}

/// `0xRRGGBB` as `0xRRGGBBAA` at `alpha` (0.0 to 1.0).
pub fn with_alpha(color: u32, alpha: f32) -> u32 {
    (color << 8) | (alpha.clamp(0.0, 1.0) * 255.0).round() as u32
}

/// The 1 px border of a control that sits on `surface_raised` (the Chat composer's model pill
/// and round buttons). `control_border` is a shade of `surface`, so on the raised card it
/// vanishes; this one is `text_muted` blended 30 % into the raised fill, visible in both themes.
pub fn raised_border(theme: &Theme) -> u32 {
    mix(theme.surface_raised, theme.text_muted, 0.3)
}

/// `alpha` as the 8-bit value an RGBA color stores (so tests and `with_glass` agree).
fn quantize(alpha: f32) -> f32 {
    (alpha.clamp(0.0, 1.0) * 255.0).round() / 255.0
}

/// The color a floating panel shows at its top over `backdrop`: `surface` at `alpha`, then the
/// accent tint at `tint`.
pub fn glass_over(surface: u32, accent: u32, alpha: f32, tint: f32, backdrop: u32) -> u32 {
    mix(mix(backdrop, surface, alpha), accent, tint)
}

/// The tint gradient of a floating panel: its top color and the same color with no alpha.
pub fn glass_gradient(theme: &Theme) -> (u32, u32) {
    (theme.glass_tint, theme.glass_tint & 0xFFFF_FF00)
}

/// The shadow of a floating panel (palette, Settings, dialogs): two layers, half as strong in
/// light themes.
pub fn shadow_float(theme: &Theme) -> Vec<gpui::BoxShadow> {
    let strength = if theme.dark { 1.0 } else { 0.5 };
    let black = |alpha: f32| gpui::Hsla::from(gpui::rgba(with_alpha(0x000000, alpha * strength)));
    vec![
        gpui::BoxShadow::new(gpui::px(0.0), gpui::px(8.0), black(0.35)).blur_radius(gpui::px(24.0)),
        gpui::BoxShadow::new(gpui::px(0.0), gpui::px(2.0), black(0.20)).blur_radius(gpui::px(6.0)),
    ]
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
    /// Sets `scrim`, `glass`, `glass_border` and `glass_tint` from `surface` and `accent`.
    /// The fill alpha starts at the spec value and grows by 2 % until `text` keeps 4.5:1 on the
    /// panel over both `bg` and `text` behind it (the worst light and dark pages).
    fn with_glass(self) -> Self {
        let (base, edge, tint, scrim) = if self.dark {
            (0.96, 0xFFFFFF, 0.05, 0.24)
        } else {
            (0.97, 0x000000, 0.04, 0.16)
        };
        let tint = quantize(tint);
        let worst = |alpha: f32| {
            [self.bg, self.text]
                .into_iter()
                .map(|backdrop| {
                    let fill = glass_over(self.surface, self.accent, alpha, tint, backdrop);
                    contrast(self.text, fill)
                })
                .fold(f32::MAX, f32::min)
        };
        let mut alpha = quantize(base);
        // Ends: at alpha 1.0 the fill is `surface` plus the tint, which text reads on.
        while worst(alpha) < 4.5 && alpha < 1.0 {
            alpha = quantize(alpha + 0.02);
        }
        Self {
            scrim: with_alpha(0x000000, scrim),
            glass: with_alpha(self.surface, alpha),
            glass_border: with_alpha(edge, 0.08),
            glass_tint: with_alpha(self.accent, tint),
            ..self
        }
    }

    /// A built-in theme from its palette (ADR 0078; token table in the spec). Status, workspace
    /// and QR colors, the classic sidebar palette, fonts and sizes stay the Mkrew base's.
    pub fn from_palette(name: impl Into<SharedString>, p: &Palette) -> Self {
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
        // The frame is a step off `bg` (the terminal ground): toward black, so panes look
        // raised; toward `fg` when `bg` is already so dark that black is under the floor.
        let pane_gap = {
            let toward_black = mix(p.bg, 0x000000, if p.dark { 0.70 } else { 0.065 });
            if contrast(toward_black, p.bg) >= PANE_GAP_MIN_CONTRAST {
                toward_black
            } else {
                mix(p.bg, p.fg, 0.10)
            }
        };
        // The shown tab chip sits on the title bar, which is `pane_gap`.
        let tab_active = mix(pane_gap, p.fg, 0.12);
        // A brand logo sits on the bar and, in the shown tab, on `tab_active`.
        let brand_grounds = [pane_gap, tab_active];
        let text = |color| readable(color, p.fg, &grounds, 4.5);
        let on_accent = if contrast(p.bg, p.accent) >= contrast(p.fg, p.accent) {
            p.bg
        } else {
            p.fg
        };
        Self {
            name: name.into(),
            bg: p.bg,
            terminal_bg: p.bg,
            sidebar: p.chrome,
            chrome: p.chrome,
            surface: p.surface,
            card: p.surface,
            tab_active,
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
            pane_gap,
            pane_focus_stripe: mix(p.accent, p.bg, 0.50),
            focus_ring: p.fg,
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
            brand_claude: readable(base.brand_claude, p.fg, &brand_grounds, 3.0),
            brand_codex: readable(base.brand_codex, p.fg, &brand_grounds, 3.0),
            brand_copilot: readable(base.brand_copilot, p.fg, &brand_grounds, 3.0),
            brand_omp: readable(base.brand_omp, p.fg, &brand_grounds, 3.0),
            ansi: p.ansi,
            dark: p.dark,
            ..base
        }
        .with_glass()
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

    /// The palette the theme is derived from; `None` for the two Mkrew themes, which are
    /// written token by token.
    pub fn palette(&self) -> Option<&Palette> {
        self.palette.as_ref()
    }
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

/// Every built-in theme, in [`mkrew_core::settings::THEME_NAMES`] order: dark ones, then light ones.
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
    // Nord. Source: palette https://www.nordtheme.com/docs/colors-and-palettes (nord0 to
    // nord15); ANSI and the muted text (dim_foreground) from
    // https://github.com/nordtheme/alacritty/blob/main/src/nord.yaml; the flat nord0 surfaces
    // as in https://github.com/arcticicestudio/nord-visual-studio-code/blob/develop/themes/nord-color-theme.json
    dark_entry(
        "Nord",
        Palette {
            dark: true,
            bg: 0x2E3440,
            chrome: 0x2E3440,
            surface: 0x2E3440,
            fg: 0xD8DEE9,
            muted: 0xA5ABB6,
            accent: 0x88C0D0,
            red: 0xBF616A,
            green: 0xA3BE8C,
            yellow: 0xEBCB8B,
            blue: 0x81A1C1,
            magenta: 0xB48EAD,
            cyan: 0x88C0D0,
            ansi: [
                0x3B4252, 0xBF616A, 0xA3BE8C, 0xEBCB8B, 0x81A1C1, 0xB48EAD, 0x88C0D0, 0xE5E9F0,
                0x4C566A, 0xBF616A, 0xA3BE8C, 0xEBCB8B, 0x81A1C1, 0xB48EAD, 0x8FBCBB, 0xECEFF4,
            ],
        },
    ),
    // Solarized Dark. Source: https://ethanschoonover.com/solarized/ (base03 to base3 and the
    // eight accents) and the terminal mapping of
    // https://github.com/altercation/solarized/blob/master/xresources/solarized
    // Changed for the contrast rules (ADR 0101): fg 839496 (base0) -> EEE8D5 (base2), accent
    // 268BD2 -> 62ABDE; muted is base1.
    dark_entry(
        "Solarized Dark",
        Palette {
            dark: true,
            bg: 0x002B36,
            chrome: 0x002B36,
            surface: 0x002B36,
            fg: 0xEEE8D5,
            muted: 0x93A1A1,
            accent: 0x62ABDE,
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
    // GitHub Dark (GitHub Dark Default). Source: https://github.com/primer/github-vscode-theme
    // (src/colors.js: fg e6edf3, muted 7d8590) with the primitives of
    // https://github.com/primer/primitives/blob/main/src/tokens/base/color/dark/dark.json5
    // (canvas 0d1117, inset 010409, overlay 151b23, blue 58a6ff); ANSI as exported in
    // https://gist.github.com/stef-k/aa85769be6b0a080a97cb2b623665137 (its terminal.ansi* keys).
    // Changed for the contrast rules (ADR 0101): accent 2F81F7 (accent.fg) -> 58A6FF.
    dark_entry(
        "GitHub Dark",
        Palette {
            dark: true,
            bg: 0x0D1117,
            chrome: 0x010409,
            surface: 0x151B23,
            fg: 0xE6EDF3,
            muted: 0x7D8590,
            accent: 0x58A6FF,
            red: 0xFF7B72,
            green: 0x3FB950,
            yellow: 0xD29922,
            blue: 0x58A6FF,
            magenta: 0xBC8CFF,
            cyan: 0x39C5CF,
            ansi: [
                0x484F58, 0xFF7B72, 0x3FB950, 0xD29922, 0x58A6FF, 0xBC8CFF, 0x39C5CF, 0xB1BAC4,
                0x6E7681, 0xFFA198, 0x56D364, 0xE3B341, 0x79C0FF, 0xD2A8FF, 0x56D4DD, 0xFFFFFF,
            ],
        },
    ),
    // Rosé Pine. Source: https://github.com/rose-pine/alacritty/blob/main/dist/rose-pine.toml
    // (base 191724, surface 1f1d2e, text e0def4, subtle 908caa, love, gold, rose, pine, foam,
    // iris and the ANSI table) and https://rosepinetheme.com/palette/
    dark_entry(
        "Rosé Pine",
        Palette {
            dark: true,
            bg: 0x191724,
            chrome: 0x191724,
            surface: 0x1F1D2E,
            fg: 0xE0DEF4,
            muted: 0x908CAA,
            accent: 0xC4A7E7,
            red: 0xEB6F92,
            green: 0x31748F,
            yellow: 0xF6C177,
            blue: 0x9CCFD8,
            magenta: 0xC4A7E7,
            cyan: 0xEBBCBA,
            ansi: [
                0x26233A, 0xEB6F92, 0x31748F, 0xF6C177, 0x9CCFD8, 0xC4A7E7, 0xEBBCBA, 0xE0DEF4,
                0x6E6A86, 0xEB6F92, 0x31748F, 0xF6C177, 0x9CCFD8, 0xC4A7E7, 0xEBBCBA, 0xE0DEF4,
            ],
        },
    ),
    // Kanagawa (wave). Source: https://github.com/rebelot/kanagawa.nvim/blob/master/lua/kanagawa/colors.lua
    // (palette) and .../lua/kanagawa/themes.lua (the wave `ui`, `syn` and `term` tables):
    // sumiInk3 bg, sumiInk0 chrome, sumiInk2 surface, fujiWhite fg, oldWhite muted, crystalBlue.
    dark_entry(
        "Kanagawa",
        Palette {
            dark: true,
            bg: 0x1F1F28,
            chrome: 0x16161D,
            surface: 0x1A1A22,
            fg: 0xDCD7BA,
            muted: 0xC8C093,
            accent: 0x7E9CD8,
            red: 0xE46876,
            green: 0x98BB6C,
            yellow: 0xE6C384,
            blue: 0x7E9CD8,
            magenta: 0x957FB8,
            cyan: 0x7AA89F,
            ansi: [
                0x16161D, 0xC34043, 0x76946A, 0xC0A36E, 0x7E9CD8, 0x957FB8, 0x6A9589, 0xC8C093,
                0x727169, 0xE82424, 0x98BB6C, 0xE6C384, 0x7FB4CA, 0x938AA9, 0x7AA89F, 0xDCD7BA,
            ],
        },
    ),
    // Everforest Dark (medium). Source: https://github.com/sainnhe/everforest/blob/master/palette.md
    // (bg0 2d353b, bg_dim 232a2e, grey1 859289 and the accents) and the terminal table of
    // https://github.com/sainnhe/everforest/blob/master/colors/everforest.vim
    // Changed for the contrast rules (ADR 0101): fg D3C6AA -> D5C9AE (the ANSI white stays D3C6AA).
    dark_entry(
        "Everforest Dark",
        Palette {
            dark: true,
            bg: 0x2D353B,
            chrome: 0x232A2E,
            surface: 0x232A2E,
            fg: 0xD5C9AE,
            muted: 0x859289,
            accent: 0xA7C080,
            red: 0xE67E80,
            green: 0xA7C080,
            yellow: 0xDBBC7F,
            blue: 0x7FBBB3,
            magenta: 0xD699B6,
            cyan: 0x83C092,
            ansi: [
                0x475258, 0xE67E80, 0xA7C080, 0xDBBC7F, 0x7FBBB3, 0xD699B6, 0x83C092, 0xD3C6AA,
                0x475258, 0xE67E80, 0xA7C080, 0xDBBC7F, 0x7FBBB3, 0xD699B6, 0x83C092, 0xD3C6AA,
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
    // Rosé Pine Dawn. Source: https://github.com/rose-pine/alacritty/blob/main/dist/rose-pine-dawn.toml
    // (base faf4ed, surface fffaf3, overlay f2e9e1, text 575279, subtle 797593, pine 286983,
    // and the ANSI table) and https://rosepinetheme.com/palette/
    light_entry(
        "Rosé Pine Dawn",
        Palette {
            dark: false,
            bg: 0xFAF4ED,
            chrome: 0xF2E9E1,
            surface: 0xFFFAF3,
            fg: 0x575279,
            muted: 0x797593,
            accent: 0x286983,
            red: 0xB4637A,
            green: 0x286983,
            yellow: 0xEA9D34,
            blue: 0x56949F,
            magenta: 0x907AA9,
            cyan: 0xD7827E,
            ansi: [
                0xF2E9E1, 0xB4637A, 0x286983, 0xEA9D34, 0x56949F, 0x907AA9, 0xD7827E, 0x575279,
                0x9893A5, 0xB4637A, 0x286983, 0xEA9D34, 0x56949F, 0x907AA9, 0xD7827E, 0x575279,
            ],
        },
    ),
    // Everforest Light. Source: https://github.com/sainnhe/everforest/blob/master/palette.md
    // (light: bg0 fdf6e3, bg_dim efebd4, hard bg0 fffbef, grey2 829181 and the accents) and the
    // terminal table of .../colors/everforest.vim (black is fg, white is bg3 in the light scheme).
    // Changed for the contrast rules (ADR 0101): fg 5C6A72 -> 525F66, accent 3A94C5 -> 2A6F94,
    // surface F4F0D9 (bg1) -> FFFBEF (the hard-contrast bg0).
    light_entry(
        "Everforest Light",
        Palette {
            dark: false,
            bg: 0xFDF6E3,
            chrome: 0xEFEBD4,
            surface: 0xFFFBEF,
            fg: 0x525F66,
            muted: 0x829181,
            accent: 0x2A6F94,
            red: 0xF85552,
            green: 0x8DA101,
            yellow: 0xDFA000,
            blue: 0x3A94C5,
            magenta: 0xDF69BA,
            cyan: 0x35A77C,
            ansi: [
                0x5C6A72, 0xF85552, 0x8DA101, 0xDFA000, 0x3A94C5, 0xDF69BA, 0x35A77C, 0xE6E2CC,
                0x5C6A72, 0xF85552, 0x8DA101, 0xDFA000, 0x3A94C5, 0xDF69BA, 0x35A77C, 0xE6E2CC,
            ],
        },
    ),
    // Ayu Light. Source: https://github.com/ayu-theme/ayu-colors/blob/master/themes/light.yaml
    // (surface lift fcfcfc, base f8f9fa, over ffffff, accent f29718, palette) and the terminal
    // table of https://github.com/ayu-theme/vscode-ayu/blob/master/ayu-light.json; fg is the
    // editor text 020a12 at 64 % over fcfcfc (5C6166), muted the ui text 021a3c at 48 % over
    // f8f9fa (828E9F).
    // Changed for the contrast rules (ADR 0101): accent F29718 -> 975E0F.
    light_entry(
        "Ayu Light",
        Palette {
            dark: false,
            bg: 0xFCFCFC,
            chrome: 0xF8F9FA,
            surface: 0xFFFFFF,
            fg: 0x5C6166,
            muted: 0x828E9F,
            accent: 0x975E0F,
            red: 0xF07171,
            green: 0x86B300,
            yellow: 0xEBA400,
            blue: 0x22A4E6,
            magenta: 0xA37ACC,
            cyan: 0x4CBF99,
            ansi: [
                0x000000, 0xF06B6C, 0x6CBF43, 0xE7A100, 0x21A1E2, 0xA176CB, 0x4ABC96, 0xC7C7C7,
                0x686868, 0xF07171, 0x86B300, 0xEBA400, 0x22A4E6, 0xA37ACC, 0x4CBF99, 0xD1D1D1,
            ],
        },
    ),
    // Tokyo Night Day. Source: https://github.com/folke/tokyonight.nvim/blob/main/extras/alacritty/tokyonight_day.toml
    // (bg e1e2e7, fg 3760bf, fg_dark 6172b0, the ANSI table).
    // Changed for the contrast rules (ADR 0101): the Mkrew workspace colors need 3:1 on the
    // hover and selected fills, which a bg this dark cannot give, so bg and surface are
    // E1E2E7 lightened to F1F2F5 and the published bg is the chrome; fg 3760BF -> 2F52A2 and
    // accent 2E7DE9 -> 2461B5 (text and code need 4.5:1).
    light_entry(
        "Tokyo Night Day",
        Palette {
            dark: false,
            bg: 0xF1F2F5,
            chrome: 0xE1E2E7,
            surface: 0xF1F2F5,
            fg: 0x2F52A2,
            muted: 0x6172B0,
            accent: 0x2461B5,
            red: 0xF52A65,
            green: 0x587539,
            yellow: 0x8C6C3E,
            blue: 0x2E7DE9,
            magenta: 0x9854F1,
            cyan: 0x007197,
            ansi: [
                0xB4B5B9, 0xF52A65, 0x587539, 0x8C6C3E, 0x2E7DE9, 0x9854F1, 0x007197, 0x6172B0,
                0xA1A6C5, 0xFF4774, 0x5C8524, 0xA27629, 0x358AFF, 0xA463FF, 0x007EA8, 0x3760BF,
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

/// Rebuilds the theme global from `mode`, the window's OS appearance, the `ThemeNames`,
/// `ThemeCatalog` and `Fonts` globals. Called at startup, on OS appearance changes, on a
/// settings or themes folder reload and by the toggle.
pub fn refresh(mode: ThemeMode, window: &Window, cx: &mut App) {
    let fonts = cx.global::<Fonts>().clone();
    let names = cx.global::<ThemeNames>().clone();
    let theme = Theme::resolve(mode, os_dark(window), &names, cx.global::<ThemeCatalog>());
    cx.set_global(theme.with_fonts(&fonts));
}

/// System → Light → Dark → System (the `ToggleTheme` command; the toggle left the title bar).
pub fn next_mode(mode: ThemeMode) -> ThemeMode {
    match mode {
        ThemeMode::System => ThemeMode::Light,
        ThemeMode::Light => ThemeMode::Dark,
        ThemeMode::Dark => ThemeMode::System,
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

/// The title bar's height in px when 1 rem is `ui_size` px (`[ui] font_size`):
/// `TITLE_BAR_HEIGHT` is in rems (ADR 0024).
pub fn title_bar_px(ui_size: f32) -> f32 {
    TITLE_BAR_HEIGHT * ui_size / 13.0
}

/// Where the first traffic light's top left goes, `None` off macOS: `MAC_TRAFFIC_LIGHT_X` from
/// the left edge, centered on the title bar at this font size.
pub fn traffic_light_pos(os: Os, ui_size: f32) -> Option<(f32, f32)> {
    (os == Os::Mac).then(|| {
        let y = (title_bar_px(ui_size) - MAC_TRAFFIC_LIGHT_SIZE) / 2.0;
        (MAC_TRAFFIC_LIGHT_X, y.max(0.0))
    })
}

/// Left padding of a title bar: room for the macOS traffic lights, none in full screen (AppKit
/// hides them there and shows them over the menu bar).
pub fn title_bar_inset(os: Os, fullscreen: bool) -> f32 {
    if os == Os::Mac && !fullscreen {
        MAC_TRAFFIC_LIGHTS_WIDTH
    } else {
        space::M
    }
}

/// Layout heights from the redesign (ADR 0027), in px at the default font size.
/// The custom title bar (ADR 0034; was the 44 px toolbar of ADR 0027).
pub const TITLE_BAR_HEIGHT: f32 = 42.0;
/// The right panel's tab row (the terminal tabs are in the title bar).
pub const TAB_STRIP_HEIGHT: f32 = 44.0;
/// One icon tab of the right panel's tab row (Files, Session, Git; ADR 0110).
pub const PANEL_TAB_HEIGHT: f32 = 28.0;
/// The Git view's repository tabs (multi-repo workspaces): a tab strip.
pub const GIT_REPO_STRIP_HEIGHT: f32 = 42.0;
/// The Git view's Working / Vs. base / History tabs: underlined text tabs (ADR 0110).
pub const GIT_SECTION_TAB_HEIGHT: f32 = 36.0;
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
/// One breath of the voice input ring ([`crate::voice::pulse`]); its lowest opacity.
pub const PULSE_PERIOD: Duration = Duration::from_millis(1600);
pub const PULSE_MIN_OPACITY: f32 = 0.35;
/// Widest a floating badge over a pane gets: a long conflict warning ellipsizes past it.
pub const PANE_BADGE_MAX_WIDTH: f32 = 320.0;
/// Longest tab title before it truncates.
pub const TAB_TITLE_MAX_WIDTH: f32 = 220.0;
/// Narrowest a terminal tab chip gets: the chips shrink to this before the tab area scrolls.
/// An inactive chip shows the status icon, one pane icon (`TAB_ICON_SIZE` each), about five
/// title letters and the ×.
pub const TAB_MIN_WIDTH: f32 = 120.0;
/// Reading width of an agent pane: the terminal is at most this wide (≈ 120–140 columns at
/// the default font) and centered, so the program wraps there (the PTY gets fewer columns).
pub const READING_WIDTH: f32 = 960.0;
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

/// A contrast rule a theme does not meet (ADR 0101): which rule, and the colors that fail it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub rule: &'static str,
    pub detail: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.rule, self.detail)
    }
}

/// The contrast rules `theme` does not meet, each failing pair once; empty for every built-in
/// theme (a test), and the reason for the "Low contrast" badge of a theme file.
pub fn problems(theme: &Theme) -> Vec<Problem> {
    let t = theme;
    let mut found: Vec<Problem> = Vec::new();
    let mut check = |rule: &'static str, fg: u32, bg: u32, min: f32| {
        let ratio = contrast(fg, bg);
        if ratio < min {
            let problem = Problem {
                rule,
                detail: format!("{fg:06X} on {bg:06X} reads {ratio:.2}:1, needs {min}:1"),
            };
            if !found.contains(&problem) {
                found.push(problem);
            }
        }
    };
    let grounds = [
        t.bg,
        t.surface,
        t.surface_raised,
        t.sidebar,
        t.hover,
        t.selected,
        t.card,
    ];
    for bg in grounds {
        for fg in [t.text, t.text_muted] {
            check("body text", fg, bg, 4.5);
        }
    }
    // Decoded exactly as `with_glass` quantized them; the page behind is a light or a dark one.
    let alpha = (t.glass & 0xFF) as f32 / 255.0;
    let tint = (t.glass_tint & 0xFF) as f32 / 255.0;
    for backdrop in [t.bg, t.text] {
        let fill = glass_over(t.surface, t.accent, alpha, tint, backdrop);
        check("text on glass", t.text, fill, 4.5);
    }
    check("on accent", t.on_accent, t.accent, 3.0);
    // The close button's red hover under its cross.
    check("window close", t.on_accent, t.status_exited, 3.0);
    for bg in [
        t.terminal_selection_bg,
        t.terminal_match_bg,
        t.terminal_match_current_bg,
    ] {
        check("selected text", t.text, bg, 4.5);
    }
    let code = [
        t.syntax_keyword,
        t.syntax_string,
        t.syntax_number,
        t.syntax_type,
        t.syntax_function,
    ];
    // `surface_raised` is the Files viewer's selected line.
    for fg in code.into_iter().chain([t.accent]) {
        for bg in [t.bg, t.surface, t.surface_raised] {
            check("code", fg, bg, 4.5);
        }
    }
    // A toast card is `surface_raised`; its marks are the info accent, the error red and the X.
    check("toast", t.text, t.surface_raised, 4.5);
    for mark in [t.accent_text, t.status_exited, t.text_muted] {
        check("toast mark", mark, t.surface_raised, 3.0);
    }
    // Comments and line numbers are `text_muted`.
    for fg in code.into_iter().chain([t.text, t.text_muted]) {
        for bg in [t.diff_add_bg, t.diff_del_bg, t.diff_mod_bg] {
            check("highlighted diff", fg, bg, 4.5);
        }
    }
    // WCAG 1.4.11: the Files editor's change bars are graphical objects.
    for fg in [t.diff_add_text, t.diff_mod_text, t.diff_del_text] {
        for bg in [t.bg, t.surface, t.surface_raised] {
            check("change bars", fg, bg, 3.0);
        }
    }
    for (fg, bg) in [
        (t.diff_add_text, t.diff_add_bg),
        (t.diff_del_text, t.diff_del_bg),
        (t.diff_mod_text, t.diff_mod_bg),
        // Line numbers in the gutter of a changed line.
        (t.text_dim, t.diff_add_gutter),
        (t.text_dim, t.diff_del_gutter),
        (t.text_dim, t.diff_mod_gutter),
        // Git status letters on the list and on its selected row.
        (t.git_modified, t.bg),
        (t.git_untracked, t.bg),
        (t.git_modified, t.selected),
        (t.git_untracked, t.selected),
    ] {
        check("diff lines", fg, bg, 4.5);
    }
    // The sidebar's session list keeps Mkrew's classic palette on this theme's sidebar.
    let classic = t.classic();
    for fg in [classic.text, classic.text_muted] {
        check("sidebar text", fg, t.sidebar, 4.5);
    }
    // Where a swatch or avatar sits: the dialogs, the menu, the sidebar and its row fills.
    // WCAG 1.4.11: graphical objects >= 3:1.
    for color in t.workspace_colors {
        for bg in grounds {
            check("workspace colors", color, bg, 3.0);
        }
    }
    // Avatars and chevrons sit on the sidebar and its row fills.
    for bg in [t.sidebar, t.hover, t.selected] {
        for color in t.workspace_colors {
            let (letter, fill) = t.avatar_colors(color, bg);
            check("avatar letters", letter, fill, 4.5);
        }
        // The only sign of expanded / collapsed (WCAG 1.4.11).
        check("chevron", t.chevron_color(), bg, 3.0);
    }
    // The taskbar badge numeral on the needs-input amber (WCAG 1.4.11, 3:1).
    check("badge ink", t.badge_ink(), t.status_needs_input, 3.0);
    // The limits chip (ADR 0106) sits on the title bar (`pane_gap`) in body text: 4.5:1 for the
    // amber (80 to 94%) and the red (95% and more).
    for fg in [t.status_needs_input, t.danger_text] {
        check("limits chip", fg, t.pane_gap, 4.5);
    }
    // The shown tab chip of the title bar (ADR 0106) must stand out from the bar, and its
    // text must read on it.
    check("active tab", t.tab_active, t.pane_gap, 1.1);
    check("active tab", t.text, t.tab_active, 4.5);
    // The frame the panes sit on is a step off the terminal ground, so a pane looks raised
    // (Tern panes, ADR 0108).
    check("pane gap", t.pane_gap, t.terminal_bg, PANE_GAP_MIN_CONTRAST);
    // An agent's brand logo in a tab chip is an icon on the bar and, in the shown tab, on
    // `tab_active` (WCAG 1.4.11, 3:1).
    for logo in crate::icons::AgentLogo::ALL {
        for ground in [t.pane_gap, t.tab_active] {
            check("brand logo", t.brand_color(*logo), ground, 3.0);
        }
    }
    found
}

/// `0xRRGGBB` as the CSS / SVG color `#RRGGBB` (diagram and QR code SVGs).
pub fn hex(color: u32) -> String {
    format!("#{color:06X}")
}

#[cfg(test)]
mod tests {
    use mkrew_core::settings::{FontSize, Settings};

    use super::*;

    #[test]
    fn the_raised_border_shows_on_the_raised_surface_in_both_themes() {
        let channel = |color: u32, shift: u32| i32::from(((color >> shift) & 0xFF) as u8);
        for theme in [Theme::dark(), Theme::light()] {
            let border = raised_border(&theme);
            let gap = [16, 8, 0]
                .into_iter()
                .map(|s| (channel(border, s) - channel(theme.surface_raised, s)).abs())
                .max()
                .unwrap();
            assert!(
                gap >= 32,
                "dark={}: border {border:06x} too faint ({gap})",
                theme.dark
            );
            // The old token is the failure this guards against.
            let old = [16, 8, 0]
                .into_iter()
                .map(|s| {
                    (channel(theme.control_border, s) - channel(theme.surface_raised, s)).abs()
                })
                .max()
                .unwrap();
            assert!(old < 32, "control_border became visible; use it instead");
        }
    }

    fn all_themes() -> Vec<Theme> {
        THEMES.iter().map(ThemeEntry::build).collect()
    }

    #[test]
    fn every_built_in_theme_meets_every_contrast_rule() {
        for theme in all_themes() {
            let found = problems(&theme);
            assert!(
                found.is_empty(),
                "{}: {}",
                theme.name,
                found
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }
    }

    /// `a` is darker than `b`: it reads less against black.
    fn darker(a: u32, b: u32) -> bool {
        contrast(a, 0x000000) < contrast(b, 0x000000)
    }

    #[test]
    fn the_pane_gap_sits_a_step_off_the_terminal_ground_in_every_theme() {
        for theme in all_themes() {
            let ratio = contrast(theme.pane_gap, theme.terminal_bg);
            assert!(
                ratio >= PANE_GAP_MIN_CONTRAST,
                "{}: pane_gap {:06X} on terminal_bg {:06X} reads {ratio:.3}:1",
                theme.name,
                theme.pane_gap,
                theme.terminal_bg
            );
            // Light themes: a step darker. Dark themes: darker too, unless the ground is
            // already nearly black (then the gap is lighter).
            if !theme.dark || contrast(theme.terminal_bg, 0x000000) >= 1.2 {
                assert!(
                    darker(theme.pane_gap, theme.terminal_bg),
                    "{}: pane_gap {:06X} should be darker than {:06X}",
                    theme.name,
                    theme.pane_gap,
                    theme.terminal_bg
                );
            }
        }
    }

    #[test]
    fn a_derived_pane_gap_goes_lighter_only_when_bg_has_no_room_below_it() {
        let base = *THEMES[1].palette().expect("One Dark has a palette");
        // (dark, bg, the gap is darker than bg)
        for (dark, bg, below) in [
            (true, 0x000000, false),
            (true, 0x050505, false),
            (true, 0x808080, true),
            (false, 0xFFFFFF, true),
            (false, 0x808080, true),
        ] {
            let theme = Theme::from_palette("Edge", &Palette { dark, bg, ..base });
            let ratio = contrast(theme.pane_gap, theme.terminal_bg);
            assert!(
                ratio >= PANE_GAP_MIN_CONTRAST,
                "bg {bg:06X}: {ratio:.3}:1 from {:06X}",
                theme.pane_gap
            );
            assert_eq!(darker(theme.pane_gap, bg), below, "bg {bg:06X}");
        }
    }

    #[test]
    fn a_palette_with_no_contrast_reports_the_rules_it_breaks() {
        let mut palette = *THEMES[1].palette().expect("One Dark has a palette");
        palette.fg = palette.bg;
        palette.accent = palette.bg;
        let found = problems(&Theme::from_palette("Bad", &palette));
        let rules: Vec<&str> = found.iter().map(|problem| problem.rule).collect();
        for rule in ["body text", "on accent", "code"] {
            assert!(rules.contains(&rule), "{rule} missing from {rules:?}");
        }
        let first = found[0].to_string();
        assert!(first.starts_with(found[0].rule), "{first}");
        assert!(first.contains(" on "), "the colors are named: {first}");
    }

    #[test]
    fn each_contrast_rule_fires_when_its_token_is_broken() {
        let t = Theme::dark();
        let broken = [
            (
                "body text",
                Theme {
                    text_muted: t.bg,
                    ..t.clone()
                },
            ),
            // Alpha 0: the fill is the page itself, and the page can be the text color.
            (
                "text on glass",
                Theme {
                    glass: 0,
                    ..t.clone()
                },
            ),
            (
                "on accent",
                Theme {
                    on_accent: t.accent,
                    ..t.clone()
                },
            ),
            (
                "window close",
                Theme {
                    status_exited: t.on_accent,
                    ..t.clone()
                },
            ),
            (
                "selected text",
                Theme {
                    terminal_selection_bg: t.text,
                    ..t.clone()
                },
            ),
            (
                "code",
                Theme {
                    syntax_keyword: t.bg,
                    ..t.clone()
                },
            ),
            (
                "toast",
                Theme {
                    text: t.surface_raised,
                    ..t.clone()
                },
            ),
            (
                "toast mark",
                Theme {
                    accent_text: t.surface_raised,
                    ..t.clone()
                },
            ),
            (
                "highlighted diff",
                Theme {
                    syntax_keyword: t.diff_add_bg,
                    ..t.clone()
                },
            ),
            (
                "change bars",
                Theme {
                    diff_add_text: t.bg,
                    ..t.clone()
                },
            ),
            (
                "diff lines",
                Theme {
                    git_modified: t.bg,
                    ..t.clone()
                },
            ),
            // A built-in Mkrew theme is its own classic palette.
            (
                "sidebar text",
                Theme {
                    sidebar: t.text,
                    ..t.clone()
                },
            ),
            ("workspace colors", {
                let mut colors = t.workspace_colors;
                colors[0] = t.bg;
                Theme {
                    workspace_colors: colors,
                    ..t.clone()
                }
            }),
            // The letter is `text`; a fill is the tint over the sidebar, so `text` = sidebar.
            (
                "avatar letters",
                Theme {
                    text: t.sidebar,
                    ..t.clone()
                },
            ),
            (
                "chevron",
                Theme {
                    text_muted: t.sidebar,
                    ..t.clone()
                },
            ),
            (
                "badge ink",
                Theme {
                    bg: t.status_needs_input,
                    text: t.status_needs_input,
                    ..t.clone()
                },
            ),
            (
                "limits chip",
                Theme {
                    pane_gap: t.status_needs_input,
                    ..t.clone()
                },
            ),
            (
                "active tab",
                Theme {
                    tab_active: t.pane_gap,
                    ..t.clone()
                },
            ),
            (
                "brand logo",
                Theme {
                    brand_claude: t.pane_gap,
                    ..t.clone()
                },
            ),
            (
                "brand logo",
                Theme {
                    tab_active: t.brand_codex,
                    ..t.clone()
                },
            ),
        ];
        assert!(problems(&t).is_empty(), "the base theme is clean");
        for (rule, theme) in broken {
            let rules: Vec<&str> = problems(&theme).iter().map(|p| p.rule).collect();
            assert!(rules.contains(&rule), "{rule} missing from {rules:?}");
        }
    }

    #[test]
    fn glass_is_derived_from_surface_and_accent_in_every_theme() {
        for theme in all_themes() {
            assert_eq!(theme.glass >> 8, theme.surface, "{}", theme.name);
            assert_eq!(theme.glass_tint >> 8, theme.accent, "{}", theme.name);
            let edge = if theme.dark { 0xFFFFFF } else { 0x000000 };
            assert_eq!(theme.glass_border, with_alpha(edge, 0.08), "{}", theme.name);
            // The base alphas of the spec; the contrast rule may only raise the fill.
            let floor = if theme.dark { 0.96_f32 } else { 0.97_f32 };
            assert!(
                (theme.glass & 0xFF) >= (floor * 255.0).round() as u32,
                "{}",
                theme.name
            );
        }
    }

    #[test]
    fn the_scrim_is_lighter_than_before_and_lighter_in_light_themes() {
        let (dark, light) = (Theme::dark(), Theme::light());
        assert_eq!(dark.scrim, with_alpha(0x000000, 0.24));
        assert_eq!(light.scrim, with_alpha(0x000000, 0.16));
        assert!(dark.scrim & 0xFF < 0x59 && light.scrim & 0xFF < dark.scrim & 0xFF);
    }

    #[test]
    fn the_tint_gradient_fades_to_the_same_color_with_no_alpha() {
        for theme in all_themes() {
            let (top, bottom) = glass_gradient(&theme);
            assert_eq!(top, theme.glass_tint);
            assert_eq!(bottom & 0xFF, 0);
            assert_eq!(bottom >> 8, top >> 8);
        }
    }

    #[test]
    fn the_float_shadow_has_two_layers_and_half_the_alpha_in_light_themes() {
        let (dark, light) = (shadow_float(&Theme::dark()), shadow_float(&Theme::light()));
        assert_eq!(dark.len(), 2);
        assert_eq!(light.len(), 2);
        assert_eq!(dark[0].offset.y, gpui::px(8.0));
        assert_eq!(dark[0].blur_radius, gpui::px(24.0));
        assert_eq!(dark[1].offset.y, gpui::px(2.0));
        assert_eq!(dark[1].blur_radius, gpui::px(6.0));
        for (d, l) in dark.iter().zip(&light) {
            assert!((d.color.a / 2.0 - l.color.a).abs() < 0.01);
        }
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
    fn there_are_22_built_in_themes_grouped_dark_then_light() {
        assert_eq!(THEMES.len(), 22);
        assert_eq!(THEMES.iter().filter(|e| e.dark).count(), 12);
        assert!(
            THEMES.iter().take(12).all(|e| e.dark),
            "picker order: dark first"
        );
        assert!(THEMES.iter().skip(12).all(|e| !e.dark));
        let mut names: Vec<&str> = THEMES.iter().map(|e| e.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 22, "names are unique");
    }

    #[test]
    fn the_theme_table_matches_the_settings_names() {
        let names: Vec<&str> = THEMES.iter().map(|e| e.name).collect();
        assert_eq!(names, mkrew_core::settings::THEME_NAMES);
        for entry in THEMES {
            let theme = entry.build();
            assert_eq!(theme.dark, entry.dark, "{}", entry.name);
            assert_eq!(theme.name.as_ref(), entry.name);
        }
        let catalog = catalog();
        assert_eq!(
            catalog.find("Mkrew Dark").map(|t| t.theme.clone()),
            Some(Theme::dark())
        );
        assert_eq!(
            catalog.find("Mkrew Light").map(|t| t.theme.clone()),
            Some(Theme::light())
        );
        assert!(catalog.find("dracula").is_none(), "exact names only");
    }

    #[test]
    fn mix_blends_per_channel() {
        assert_eq!(mix(0x000000, 0xFFFFFF, 0.5), 0x808080);
        assert_eq!(mix(0x102030, 0x102030, 0.7), 0x102030);
        assert_eq!(mix(0xFF0000, 0x0000FF, 0.0), 0xFF0000);
        assert_eq!(mix(0xFF0000, 0x0000FF, 1.0), 0x0000FF);
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

    fn catalog() -> ThemeCatalog {
        ThemeCatalog::builtin()
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
        assert!(Theme::resolve(ThemeMode::System, true, &n, &catalog()).dark);
        assert!(!Theme::resolve(ThemeMode::System, false, &n, &catalog()).dark);
        assert!(!Theme::resolve(ThemeMode::Light, true, &n, &catalog()).dark);
        assert!(Theme::resolve(ThemeMode::Dark, false, &n, &catalog()).dark);
        assert_eq!(
            Theme::resolve(ThemeMode::Dark, false, &n, &catalog()).mode,
            ThemeMode::Dark
        );
    }

    #[test]
    fn default_names_resolve_to_exactly_today() {
        let n = ThemeNames::from_settings(&Settings::default());
        assert_eq!(
            Theme::resolve(ThemeMode::System, true, &n, &catalog()),
            Theme::dark()
        );
        assert_eq!(
            Theme::resolve(ThemeMode::System, false, &n, &catalog()),
            Theme::light()
        );
    }

    #[test]
    fn unknown_names_fall_back_to_mkrew() {
        let n = names("nope", "dracula");
        assert_eq!(
            Theme::resolve(ThemeMode::System, true, &n, &catalog()),
            Theme::dark()
        );
        assert_eq!(
            Theme::resolve(ThemeMode::System, false, &n, &catalog()),
            Theme::light()
        );
    }

    #[test]
    fn each_slot_picks_its_name() {
        let n = names("Solarized Light", "Dracula");
        let name = |mode, os_dark| {
            Theme::resolve(mode, os_dark, &n, &catalog())
                .name
                .to_string()
        };
        assert_eq!(name(ThemeMode::Dark, false), "Dracula");
        assert_eq!(name(ThemeMode::Light, true), "Solarized Light");
        assert_eq!(name(ThemeMode::System, true), "Dracula");
    }

    #[test]
    fn a_slot_may_hold_the_other_appearance() {
        let n = names("Mkrew Light", "GitHub Light");
        let theme = Theme::resolve(ThemeMode::Dark, false, &n, &catalog());
        assert_eq!((theme.name.as_ref(), theme.dark), ("GitHub Light", false));
    }

    #[test]
    fn a_file_theme_resolves_until_its_file_goes() {
        let file = (
            "aurora.toml".to_owned(),
            Ok(crate::theme_file::render(
                "Aurora",
                &Palette::from_theme(&Theme::dark()),
            )),
        );
        let with = ThemeCatalog::from_files(vec![file]);
        let n = names("Mkrew Light", "Aurora");
        let shown = Theme::resolve(ThemeMode::System, true, &n, &with);
        assert_eq!(shown.name.as_ref(), "Aurora");
        assert_eq!(
            Theme::resolve(ThemeMode::System, true, &n, &catalog()),
            Theme::dark(),
            "the file is gone: the slot's Mkrew theme, like an unknown name"
        );
    }

    #[test]
    fn a_theme_name_is_saved_as_a_valid_toml_string() {
        for name in ["Plain", "Rosé Pine", "He said \"hi\"", "back\\slash"] {
            let text = format!("theme = {}", toml_string(name));
            let table: toml::Table = text.parse().unwrap();
            assert_eq!(table["theme"].as_str(), Some(name), "{text}");
        }
    }

    #[test]
    fn layout_heights_scale_with_the_ui_font() {
        // 44 px at the default 13 px body; at 20 px the title bar is 44 × 20 / 13.
        assert_eq!(rems_px(44.0), gpui::rems(44.0 / 13.0));
        assert_eq!(
            rems_px(TITLE_BAR_HEIGHT).0 * 20.0,
            TITLE_BAR_HEIGHT * 20.0 / 13.0
        );
    }

    #[test]
    fn the_traffic_lights_stay_centered_in_the_title_bar_at_any_font_size() {
        // The default 13 px body: the lights keep the place they always had.
        assert_eq!(traffic_light_pos(Os::Mac, 13.0), Some((12.0, 14.0)));
        // 16 px body: the bar is 42 * 16 / 13 = 51.7 px tall, so the lights sit lower.
        let (x, y) = traffic_light_pos(Os::Mac, 16.0).unwrap();
        assert_eq!(x, MAC_TRAFFIC_LIGHT_X);
        assert!((y - 18.846).abs() < 0.01, "{y}");
        // A tiny font never puts them above the window's top edge.
        assert!(traffic_light_pos(Os::Mac, 1.0).unwrap().1 >= 0.0);
        assert_eq!(traffic_light_pos(Os::Windows, 16.0), None);
        assert_eq!(traffic_light_pos(Os::Linux, 16.0), None);
    }

    #[test]
    fn the_title_bar_leaves_room_for_the_lights_except_in_full_screen() {
        assert_eq!(title_bar_inset(Os::Mac, false), MAC_TRAFFIC_LIGHTS_WIDTH);
        assert_eq!(title_bar_inset(Os::Mac, true), space::M);
        assert_eq!(title_bar_inset(Os::Windows, false), space::M);
        assert_eq!(title_bar_inset(Os::Linux, true), space::M);
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
    fn workspace_colors_are_distinct_and_every_name_has_one() {
        for theme in all_themes() {
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
}
