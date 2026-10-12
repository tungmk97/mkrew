//! Lucide icons (ISC, `icon/lucide/`, ADR 0051) for icon buttons, row chevrons and button
//! labels. One monochrome file per icon: gpui paints an SVG as an alpha mask in the svg
//! element's own `text_color`, so [`icon_svg`] always sets it (the parent's color is not
//! inherited).

use std::borrow::Cow;

use gpui::prelude::*;
use gpui::{AssetSource, SharedString, Svg, rgb, svg};

use crate::theme;

macro_rules! icons {
    ($($variant:ident => $file:literal,)*) => {
        /// A Lucide icon embedded in the binary.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Icon {
            $($variant,)*
        }

        impl Icon {
            pub const ALL: &'static [Icon] = &[$(Icon::$variant,)*];

            /// The asset path `svg().path(..)` asks [`Assets`] for.
            pub fn path(self) -> &'static str {
                match self {
                    $(Icon::$variant => concat!("icons/", $file, ".svg"),)*
                }
            }

            fn bytes(self) -> &'static [u8] {
                match self {
                    $(Icon::$variant => include_bytes!(concat!("../../../icon/lucide/", $file, ".svg")),)*
                }
            }
        }
    };
}

icons! {
    ArrowDown => "arrow-down",
    ArrowDownToLine => "arrow-down-to-line",
    ArrowUp => "arrow-up",
    ArrowUpFromLine => "arrow-up-from-line",
    Bot => "bot",
    ChevronDown => "chevron-down",
    ChevronLeft => "chevron-left",
    ChevronRight => "chevron-right",
    CircleStop => "circle-stop",
    Columns2 => "columns-2",
    Copy => "copy",
    Delete => "delete",
    Ellipsis => "ellipsis",
    ExternalLink => "external-link",
    FileDiff => "file-diff",
    FileText => "file-text",
    FolderOpen => "folder-open",
    FolderPlus => "folder-plus",
    GitCompare => "git-compare",
    History => "history",
    Library => "library",
    Link => "link",
    MessageSquarePlus => "message-square-plus",
    Minimize2 => "minimize-2",
    Minus => "minus",
    Moon => "moon",
    PanelLeft => "panel-left",
    PanelLeftClose => "panel-left-close",
    PanelLeftOpen => "panel-left-open",
    PanelRight => "panel-right",
    Play => "play",
    Plus => "plus",
    RotateCw => "rotate-cw",
    Save => "save",
    Search => "search",
    Send => "send",
    Share2 => "share-2",
    Smartphone => "smartphone",
    Sparkles => "sparkles",
    Square => "square",
    SquareTerminal => "square-terminal",
    SquareX => "square-x",
    Sun => "sun",
    SunMoon => "sun-moon",
    Trash2 => "trash-2",
    Undo2 => "undo-2",
    Unlink => "unlink",
    Users => "users",
    X => "x",
}

/// Serves the [`Icon`] files to gpui (`Application::with_assets`).
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(Icon::ALL
            .iter()
            .find(|icon| icon.path() == path)
            .map(|icon| Cow::Borrowed(icon.bytes())))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Icon::ALL
            .iter()
            .map(|icon| icon.path())
            .filter(|asset| asset.starts_with(path))
            .map(SharedString::from)
            .collect())
    }
}

/// `icon` as a `size` px square (a `theme` size, scaled with `[ui] font_size`) in `color`.
pub fn icon_svg(icon: Icon, size: f32, color: u32) -> Svg {
    svg()
        .path(icon.path())
        .flex_none()
        .size(theme::rems_px(size))
        .text_color(rgb(color))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// gpui draws nothing for a path the asset source does not serve, or for bytes that are not
    /// an SVG: the button would be blank but still clickable.
    #[test]
    fn every_icon_is_served_and_is_a_lucide_svg() {
        for &icon in Icon::ALL {
            let bytes = Assets
                .load(icon.path())
                .unwrap()
                .unwrap_or_else(|| panic!("{icon:?} is not served"));
            let text = std::str::from_utf8(&bytes).unwrap();
            assert!(text.contains("<svg"), "{icon:?}: not an SVG");
            assert!(
                text.contains("viewBox=\"0 0 24 24\""),
                "{icon:?}: not a 24 px Lucide icon"
            );
        }
        assert!(Assets.load("icons/missing.svg").unwrap().is_none());
    }
}
