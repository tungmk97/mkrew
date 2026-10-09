//! App theme with dark and light palettes.
//!
//! Usage in any view:
//!     let t = cx.theme();
//!     div().bg(t.bg).text_color(t.text)
//!
//! Setup (once, at startup):
//!     theme::init(ThemeMode::System, window, cx);
//! And in the root view, follow OS changes:
//!     cx.observe_window_appearance(window, |_, window, cx| theme::sync_with_system(window, cx)).detach();

use gpui::{rgb, rgba, App, Global, Hsla, Window, WindowAppearance};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Appearance {
    Light,
    Dark,
}

#[derive(Clone)]
pub struct Theme {
    pub mode: ThemeMode,
    pub appearance: Appearance,

    pub sidebar: Hsla,
    pub bg: Hsla,
    pub surface: Hsla,
    pub card: Hsla,
    pub hover: Hsla,
    pub selected: Hsla,
    pub segment_on: Hsla,
    pub border: Hsla,
    pub divider: Hsla,

    pub text: Hsla,
    pub text_dim: Hsla,
    pub muted: Hsla,
    pub subtle: Hsla,

    pub accent: Hsla,
    pub accent_hover: Hsla,
    pub accent_text: Hsla,
    pub accent_bg: Hsla,
    pub on_accent: Hsla,
    pub on_accent_divider: Hsla,

    pub warning: Hsla,
    pub warning_bg: Hsla,
    pub success: Hsla,
    pub success_bg: Hsla,

    /// Alpha used for avatar background tints (works on both backgrounds).
    pub avatar_tint_alpha: u8,
}

impl Global for Theme {}

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

impl Theme {
    pub fn dark(mode: ThemeMode) -> Self {
        Self {
            mode,
            appearance: Appearance::Dark,
            sidebar: c(0x131418),
            bg: c(0x17181c),
            surface: c(0x1f2127),
            card: c(0x1a1b20),
            hover: c(0x22242b),
            selected: c(0x262938),
            segment_on: c(0x2c2e37),
            border: c(0x2a2c33),
            divider: c(0x24262d),
            text: c(0xe6e7eb),
            text_dim: c(0xb4b7c0),
            muted: c(0x8b8f9a),
            subtle: c(0x5d616c),
            accent: c(0x7c83ff),
            accent_hover: c(0x8e94ff),
            accent_text: c(0x9a9fff),
            accent_bg: c(0x25264a),
            on_accent: c(0xffffff),
            on_accent_divider: rgba(0xffffff40).into(),
            warning: c(0xe5b45a),
            warning_bg: c(0x3a3020),
            success: c(0x4fd1a1),
            success_bg: c(0x1d3a31),
            avatar_tint_alpha: 0x2e,
        }
    }

    pub fn light(mode: ThemeMode) -> Self {
        Self {
            mode,
            appearance: Appearance::Light,
            sidebar: c(0xf4f5f7),
            bg: c(0xffffff),
            surface: c(0xf6f7f9),
            card: c(0xffffff),
            hover: c(0xeef0f3),
            selected: c(0xe9eaff),
            segment_on: c(0xffffff),
            border: c(0xe3e5ea),
            divider: c(0xeceef1),
            text: c(0x1b1d22),
            text_dim: c(0x474b55),
            muted: c(0x6b7080),
            subtle: c(0xa0a4af),
            accent: c(0x5b63f0),
            accent_hover: c(0x4c54e6),
            accent_text: c(0x4a52d9),
            accent_bg: c(0xeceeff),
            on_accent: c(0xffffff),
            on_accent_divider: rgba(0xffffff59).into(),
            warning: c(0xb7791f),
            warning_bg: c(0xfdf3e1),
            success: c(0x1f9d6f),
            success_bg: c(0xe1f5ec),
            avatar_tint_alpha: 0x24,
        }
    }

    pub fn for_appearance(appearance: Appearance, mode: ThemeMode) -> Self {
        match appearance {
            Appearance::Dark => Self::dark(mode),
            Appearance::Light => Self::light(mode),
        }
    }

    pub fn is_dark(&self) -> bool {
        self.appearance == Appearance::Dark
    }

    /// Per-item color (e.g. workspace avatar) tinted for the background.
    pub fn tint(&self, hex: u32) -> Hsla {
        rgba((hex << 8) | self.avatar_tint_alpha as u32).into()
    }
}

fn system_appearance(window: &Window) -> Appearance {
    match window.appearance() {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => Appearance::Dark,
        WindowAppearance::Light | WindowAppearance::VibrantLight => Appearance::Light,
    }
}

fn resolve(mode: ThemeMode, window: &Window) -> Appearance {
    match mode {
        ThemeMode::Dark => Appearance::Dark,
        ThemeMode::Light => Appearance::Light,
        ThemeMode::System => system_appearance(window),
    }
}

/// Install the theme global. Call once before opening/rendering views.
pub fn init(mode: ThemeMode, window: &Window, cx: &mut App) {
    cx.set_global(Theme::for_appearance(resolve(mode, window), mode));
}

/// Switch mode (from a settings menu or toolbar toggle) and re-render.
pub fn set_mode(mode: ThemeMode, window: &mut Window, cx: &mut App) {
    cx.set_global(Theme::for_appearance(resolve(mode, window), mode));
    window.refresh();
}

/// Cycle System → Light → Dark → System.
pub fn cycle_mode(window: &mut Window, cx: &mut App) {
    let next = match cx.theme().mode {
        ThemeMode::System => ThemeMode::Light,
        ThemeMode::Light => ThemeMode::Dark,
        ThemeMode::Dark => ThemeMode::System,
    };
    set_mode(next, window, cx);
}

/// Call from `observe_window_appearance` so "System" follows the OS.
pub fn sync_with_system(window: &mut Window, cx: &mut App) {
    if cx.theme().mode == ThemeMode::System {
        set_mode(ThemeMode::System, window, cx);
    }
}

pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Theme {
        self.global::<Theme>()
    }
}
