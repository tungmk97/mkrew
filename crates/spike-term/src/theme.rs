//! Minimal design tokens for the spike (SPEC §14). The real app moves these into
//! `crates/mkrew-ui/src/theme.rs`. Colors are `0xRRGGBB`.

/// Token set for one appearance (dark or light).
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub bg: u32,
    pub text: u32,
    /// Cursor block.
    pub accent: u32,
    /// ANSI 0–15, harmonized with the tokens above.
    pub ansi: [u32; 16],
    pub terminal_font_family: &'static str,
    /// Used if `terminal_font_family` is not installed.
    pub terminal_font_fallback: &'static str,
    pub terminal_font_size: f32,
    /// Line height as a multiple of the font size.
    pub terminal_line_height: f32,
    /// Padding around the terminal grid (spacing scale).
    pub terminal_padding: f32,
    pub terminal_underline_thickness: f32,
    /// Weight of a dim (SGR 2) foreground blended towards the background, 0–1.
    pub dim_blend: f32,
}

#[cfg(windows)]
const TERMINAL_FONT: &str = "Cascadia Code";
#[cfg(target_os = "macos")]
const TERMINAL_FONT: &str = "SF Mono";
#[cfg(not(any(windows, target_os = "macos")))]
const TERMINAL_FONT: &str = "JetBrains Mono";

#[cfg(windows)]
const TERMINAL_FONT_FALLBACK: &str = "Consolas";
#[cfg(target_os = "macos")]
const TERMINAL_FONT_FALLBACK: &str = "Menlo";
#[cfg(not(any(windows, target_os = "macos")))]
const TERMINAL_FONT_FALLBACK: &str = "DejaVu Sans Mono";

impl Theme {
    pub fn dark() -> Self {
        Self {
            bg: 0x16171B,
            text: 0xE6E7EB,
            accent: 0x7C8CFF,
            ansi: [
                0x25262C, 0xE5676B, 0x4CB782, 0xE3A63E, 0x7C8CFF, 0xC28CF0, 0x5EC4D6, 0xC9CBD1,
                0x6E7079, 0xF2A0A4, 0x7DD3A0, 0xF0C674, 0xA3AFFF, 0xD8B4FE, 0x8BE0EC, 0xF5F6F8,
            ],
            ..Self::shared()
        }
    }

    pub fn light() -> Self {
        Self {
            bg: 0xFAFAF9,
            text: 0x1D1E22,
            accent: 0x4F5BD5,
            ansi: [
                // "White" stays readable on the light background (≥ 3:1).
                0x1D1E22, 0xC53030, 0x2F855A, 0xB7791F, 0x4F5BD5, 0x9B4DCA, 0x1F8A9E, 0x8A8C94,
                0x6B6D76, 0xE5676B, 0x38A169, 0xD69E2E, 0x3B47B8, 0xB37FEB, 0x31A3B8, 0x55575F,
            ],
            ..Self::shared()
        }
    }

    fn shared() -> Self {
        Self {
            bg: 0,
            text: 0,
            accent: 0,
            ansi: [0; 16],
            terminal_font_family: TERMINAL_FONT,
            terminal_font_fallback: TERMINAL_FONT_FALLBACK,
            terminal_font_size: 13.0,
            terminal_line_height: 1.35,
            terminal_padding: 8.0,
            terminal_underline_thickness: 1.0,
            dim_blend: 0.6,
        }
    }
}
