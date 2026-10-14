//! Lucide icons (ISC, `icon/lucide/`, ADR 0051) for icon buttons, row chevrons and button
//! labels, and agent logos (`icon/brand/`, ADR 0058) for agent chips. One monochrome file per
//! icon: gpui paints an SVG as an alpha mask in the svg element's own `text_color`, so
//! [`icon_svg`] always sets it (the parent's color is not inherited).

use std::borrow::Cow;

use gpui::prelude::*;
use gpui::{AssetSource, SharedString, Svg, rgb, svg};

use crate::theme;

macro_rules! icons {
    ($(#[$doc:meta])* $name:ident in $dir:literal { $($variant:ident => $file:literal,)* }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name {
            $($variant,)*
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant,)*];

            /// The asset path `svg().path(..)` asks [`Assets`] for.
            pub fn path(self) -> &'static str {
                match self {
                    $($name::$variant => concat!("icons/", $dir, "/", $file, ".svg"),)*
                }
            }

            fn bytes(self) -> &'static [u8] {
                match self {
                    $($name::$variant => include_bytes!(concat!("../../../icon/", $dir, "/", $file, ".svg")),)*
                }
            }
        }
    };
}

icons! {
    /// A Lucide icon embedded in the binary.
    Icon in "lucide" {
        ArrowDown => "arrow-down",
        ArrowDownToLine => "arrow-down-to-line",
        ArrowUp => "arrow-up",
        ArrowUpFromLine => "arrow-up-from-line",
        Bot => "bot",
        BookOpen => "book-open",
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
        Inbox => "inbox",
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
        Settings => "settings",
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
}

icons! {
    /// A shipped agent's logo (ADR 0058), shown in its chip instead of the profile `icon`.
    AgentLogo in "brand" {
        Claude => "claude",
        Codex => "codex",
        Copilot => "copilot",
        Omp => "omp",
    }
}

impl AgentLogo {
    /// The logo of the shipped profile `id`; other profiles have none.
    pub fn of(id: &str) -> Option<Self> {
        match id {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "copilot" => Some(Self::Copilot),
            "omp" => Some(Self::Omp),
            _ => None,
        }
    }
}

/// Serves the [`Icon`] and [`AgentLogo`] files to gpui (`Application::with_assets`).
pub struct Assets;

impl Assets {
    fn all() -> impl Iterator<Item = (&'static str, &'static [u8])> {
        let icons = Icon::ALL.iter().map(|icon| (icon.path(), icon.bytes()));
        icons.chain(
            AgentLogo::ALL
                .iter()
                .map(|logo| (logo.path(), logo.bytes())),
        )
    }
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(Self::all()
            .find(|(asset, _)| *asset == path)
            .map(|(_, bytes)| Cow::Borrowed(bytes)))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Self::all()
            .map(|(asset, _)| asset)
            .filter(|asset| asset.starts_with(path))
            .map(SharedString::from)
            .collect())
    }
}

/// `icon` as a `size` px square (a `theme` size, scaled with `[ui] font_size`) in `color`.
pub fn icon_svg(icon: Icon, size: f32, color: u32) -> Svg {
    asset_svg(icon.path(), size, color)
}

/// `logo` like [`icon_svg`].
pub fn logo_svg(logo: AgentLogo, size: f32, color: u32) -> Svg {
    asset_svg(logo.path(), size, color)
}

fn asset_svg(path: &'static str, size: f32, color: u32) -> Svg {
    svg()
        .path(path)
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
        for &logo in AgentLogo::ALL {
            let bytes = Assets.load(logo.path()).unwrap();
            let text = String::from_utf8(bytes.unwrap().into_owned()).unwrap();
            assert!(text.starts_with("<svg"), "{logo:?}: not an SVG");
            // gpui only keeps the alpha: a second colour would paint as the first.
            assert!(!text.contains("fill=\"#"), "{logo:?}: not one colour");
        }
    }
}
