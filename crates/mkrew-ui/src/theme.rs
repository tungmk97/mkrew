//! Design tokens (SPEC §14). Every color, spacing, radius, font size and duration used by the
//! UI comes from here; components never hard-code values. Colors are `0xRRGGBB`.

use std::time::Duration;

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

/// UI font sizes in px.
pub mod font_size {
    pub const META: f32 = 12.0;
    pub const BODY: f32 = 13.0;
    pub const CARD_TITLE: f32 = 15.0;
    pub const VIEW_TITLE: f32 = 20.0;
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

/// Status refresh period: one spinner frame (a small, slow spinner) and the elapsed/idle
/// time update (SPEC §11, §14).
pub const STATUS_TICK: Duration = Duration::from_millis(500);

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

/// Opacity of a hovered / pressed filled button, and of secondary text inside a button.
pub mod opacity {
    pub const HOVER: f32 = 0.9;
    pub const ACTIVE: f32 = 0.8;
    pub const SECONDARY: f32 = 0.8;
    /// A control that cannot be used right now (e.g. Save while saving).
    pub const DISABLED: f32 = 0.5;
    /// Hover-revealed actions (SPEC §14 progressive disclosure): hidden, then fully shown.
    pub const HIDDEN: f32 = 0.0;
    pub const FULL: f32 = 1.0;
    /// The lifted copy of a card that follows the pointer while it is dragged.
    pub const DRAGGING: f32 = 0.9;
}

/// Half the gap between split panes: each pane is inset by it, so panes sit `space::XS` apart
/// and `space::XS` from the tab's edges.
pub const PANE_HALF_GAP: f32 = space::XS / 2.0;

/// Width of the workspace sidebar.
pub const SIDEBAR_WIDTH: f32 = 240.0;

/// Width of a popover menu (the workspace menu).
pub const MENU_WIDTH: f32 = 240.0;

/// Width of the query field in a terminal's find bar.
pub const FIND_INPUT_WIDTH: f32 = 200.0;

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

/// The workspace color dot in front of a sidebar row's name (ADR 0020).
pub const WORKSPACE_DOT_SIZE: f32 = space::S;

/// A color choice in the workspace menu.
pub const WORKSPACE_SWATCH_SIZE: f32 = space::L;

/// Ring around a workspace menu color choice.
pub const WORKSPACE_SWATCH_BORDER: f32 = 2.0;

/// Height of the quick open match list (it scrolls past that).
pub const QUICK_OPEN_LIST_HEIGHT: f32 = 360.0;

/// Width of the Git view's header and changed-file list (right of the diff).
pub const GIT_FILE_LIST_WIDTH: f32 = 300.0;

/// Tallest the Git view's commit message box grows (it scrolls past that, ~8 lines).
pub const GIT_COMMIT_BOX_MAX_HEIGHT: f32 = 160.0;

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

/// The shade behind a modal dialog (RGBA: black at ~35 %), in both appearances.
pub const DIALOG_SCRIM: u32 = 0x0000_0059;

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

/// The terminal family picked from the installed fonts at startup.
static TERMINAL_FONT: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

/// The first of `preferred` that is `installed`, else the first of `preferred`.
pub fn pick_family(preferred: &[&'static str], installed: &[String]) -> &'static str {
    preferred
        .iter()
        .copied()
        .find(|family| installed.iter().any(|name| name == family))
        .or_else(|| preferred.first().copied())
        .unwrap_or_default()
}

/// Picks the terminal family once; call before the first window opens. A family DirectWrite
/// / CoreText cannot find would silently become a proportional UI font.
pub fn init_terminal_font(installed: &[String]) {
    let _ = TERMINAL_FONT.set(pick_family(TERMINAL_FONTS, installed));
}

fn terminal_font_family() -> &'static str {
    TERMINAL_FONT
        .get()
        .copied()
        .unwrap_or_else(|| pick_family(TERMINAL_FONTS, &[]))
}

/// Color tokens for one appearance (dark or light), plus the terminal settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub bg: u32,
    pub surface: u32,
    pub surface_raised: u32,
    pub border: u32,
    pub text: u32,
    pub text_muted: u32,
    /// Selection, primary buttons, focus.
    pub accent: u32,
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
    pub diff_hunk_header: u32,
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
    pub workspace_colors: [u32; 8],
    /// Dark appearance (picks diagram images, ADR 0021).
    pub dark: bool,
    pub terminal_font_family: &'static str,
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

impl Theme {
    /// Follows the OS appearance (SPEC §14).
    pub fn for_appearance(dark: bool) -> Self {
        if dark { Self::dark() } else { Self::light() }
    }

    pub fn dark() -> Self {
        Self {
            bg: 0x16171B,
            surface: 0x1D1E23,
            surface_raised: 0x25262C,
            border: 0x2D2F36,
            text: 0xE6E7EB,
            text_muted: 0x9A9CA6,
            accent: 0x7C8CFF,
            pane_focus_border: 0x49518D,
            terminal_selection_bg: 0x3A406B,
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
            diff_hunk_header: 0x25262C,
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
                0xE5676B, 0xF0955A, 0xE3C14E, 0x4CB782, 0x5EC4D6, 0x7C8CFF, 0xC28CF0, 0xF08CC0,
            ],
            dark: true,
            ..Self::shared()
        }
    }

    pub fn light() -> Self {
        Self {
            bg: 0xFAFAF9,
            surface: 0xFFFFFF,
            surface_raised: 0xF3F3F1,
            border: 0xE5E5E1,
            text: 0x1D1E22,
            text_muted: 0x6B6D76,
            accent: 0x4F5BD5,
            pane_focus_border: 0xA4AAE7,
            terminal_selection_bg: 0xCFD2F0,
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
            diff_hunk_header: 0xF3F3F1,
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
                0xC53030, 0xC05621, 0x975A16, 0x2F855A, 0x1F8A9E, 0x4F5BD5, 0x8A3FB8, 0xB83280,
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
            border: 0,
            text: 0,
            text_muted: 0,
            accent: 0,
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
            diff_hunk_header: 0,
            syntax_keyword: 0,
            syntax_string: 0,
            syntax_number: 0,
            syntax_type: 0,
            syntax_function: 0,
            ansi: [0; 16],
            workspace_colors: [0; 8],
            dark: false,
            terminal_font_family: terminal_font_family(),
            terminal_font_size: font_size::BODY,
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
    use super::*;

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
            for bg in [theme.bg, theme.surface, theme.surface_raised] {
                for fg in [theme.text, theme.text_muted] {
                    let ratio = contrast(fg, bg);
                    assert!(ratio >= 4.5, "{fg:06X} on {bg:06X}: {ratio:.2}");
                }
            }
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
                for bg in [theme.diff_add_bg, theme.diff_del_bg] {
                    let ratio = contrast(fg, bg);
                    assert!(ratio >= 4.5, "{fg:06X} on {bg:06X}: {ratio:.2}");
                }
            }
        }
    }

    #[test]
    fn contrast_matches_known_values() {
        assert!((contrast(0x000000, 0xFFFFFF) - 21.0).abs() < 0.01);
        assert!((contrast(0x777777, 0x777777) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn appearance_picks_the_matching_palette() {
        assert_eq!(Theme::for_appearance(true), Theme::dark());
        assert_eq!(Theme::for_appearance(false), Theme::light());
        assert_ne!(Theme::dark().bg, Theme::light().bg);
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

    #[test]
    fn workspace_colors_stand_out_from_the_surfaces() {
        for theme in [Theme::dark(), Theme::light()] {
            for color in theme.workspace_colors {
                // WCAG 1.4.11: graphical objects ≥ 3:1.
                for bg in [theme.bg, theme.surface, theme.surface_raised] {
                    let ratio = contrast(color, bg);
                    assert!(ratio >= 3.0, "{color:06X} on {bg:06X}: {ratio:.2}");
                }
            }
            assert_eq!(
                theme.workspace_color("teal"),
                Some(theme.workspace_colors[4])
            );
            assert_eq!(theme.workspace_color("#ff0000"), None);
        }
    }
}
