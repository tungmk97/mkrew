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
        AlignCenter => "align-center",
        ArrowDown => "arrow-down",
        ArrowDownToLine => "arrow-down-to-line",
        ArrowLeft => "arrow-left",
        ArrowRight => "arrow-right",
        ArrowUp => "arrow-up",
        ArrowUpFromLine => "arrow-up-from-line",
        Bell => "bell",
        Bot => "bot",
        BookOpen => "book-open",
        Check => "check",
        ChevronDown => "chevron-down",
        ChevronLeft => "chevron-left",
        ChevronRight => "chevron-right",
        Circle => "circle",
        Clock => "clock",
        CornerDownLeft => "corner-down-left",
        CircleArrowUp => "circle-arrow-up",
        CircleAlert => "circle-alert",
        CircleHelp => "circle-help",
        CircleStop => "circle-stop",
        CircleHalf => "circle-half",
        Code => "code",
        Columns2 => "columns-2",
        Copy => "copy",
        Delete => "delete",
        Database => "database",
        Download => "download",
        Ellipsis => "ellipsis",
        ExternalLink => "external-link",
        Eye => "eye",
        EyeOff => "eye-off",
        File => "file",
        FileCode => "file-code",
        FileCog => "file-cog",
        FileDiff => "file-diff",
        FileImage => "file-image",
        FileJson => "file-json",
        Files => "files",
        FileSpreadsheet => "file-spreadsheet",
        FileText => "file-text",
        Folder => "folder",
        FolderOpen => "folder-open",
        FolderPlus => "folder-plus",
        GitCompare => "git-compare",
        GitBranch => "git-branch",
        Globe => "globe",
        GripHorizontal => "grip-horizontal",
        History => "history",
        Hourglass => "hourglass",
        Inbox => "inbox",
        Info => "info",
        Library => "library",
        Layers => "layers",
        Lightbulb => "lightbulb",
        Link => "link",
        ListFilter => "list-filter",
        Lock => "lock",
        Maximize2 => "maximize-2",
        MessageSquare => "message-square",
        MessageSquarePlus => "message-square-plus",
        MessageSquareWarning => "message-square-warning",
        Minimize2 => "minimize-2",
        Minus => "minus",
        Moon => "moon",
        NotebookPen => "notebook-pen",
        OctagonAlert => "octagon-alert",
        PanelLeft => "panel-left",
        PanelLeftClose => "panel-left-close",
        PanelLeftOpen => "panel-left-open",
        PanelRight => "panel-right",
        Pencil => "pencil",
        Pin => "pin",
        Play => "play",
        Plus => "plus",
        RotateCw => "rotate-cw",
        Save => "save",
        Search => "search",
        Send => "send",
        Share2 => "share-2",
        Server => "server",
        Settings => "settings",
        Smartphone => "smartphone",
        Sparkles => "sparkles",
        Square => "square",
        SquareTerminal => "square-terminal",
        SquareX => "square-x",
        SquareCheck => "square-check",
        SquareKanban => "square-kanban",
        Sun => "sun",
        SunMoon => "sun-moon",
        Terminal => "terminal",
        Trash2 => "trash-2",
        TriangleAlert => "triangle-alert",
        Undo2 => "undo-2",
        Unlink => "unlink",
        Users => "users",
        Upload => "upload",
        X => "x",
        Zap => "zap",
    }
}

impl Icon {
    /// The icon of a Files tree file (SPEC §12A): lock files and `.env`, tool config
    /// (dotfiles ending `rc`, `.gitignore`, `*.config.*`), then by extension. Names are
    /// compared without case and without allocating (rows are drawn every frame).
    pub fn for_file(name: &str) -> Icon {
        let ext = name.rsplit_once('.').map_or("", |(_, ext)| ext);
        let ext_in = |list: &[&str]| list.iter().any(|e| ext.eq_ignore_ascii_case(e));
        let ends = |suffix: &str| {
            name.len() >= suffix.len()
                && name.as_bytes()[name.len() - suffix.len()..]
                    .eq_ignore_ascii_case(suffix.as_bytes())
        };
        // `.env` and `.env.*` (`.envrc` is a tool config, not a secret).
        let env = name.eq_ignore_ascii_case(".env")
            || (name.len() > 5 && name.as_bytes()[..5].eq_ignore_ascii_case(b".env."));
        if env || ext_in(&["lock"]) || ends("-lock.json") || ends("-lock.yaml") {
            Icon::Lock
        } else if [".gitignore", ".gitattributes", ".editorconfig"]
            .iter()
            .any(|dotfile| name.eq_ignore_ascii_case(dotfile))
            || (name.starts_with('.') && ends("rc"))
            || name.contains(".config.")
        {
            Icon::Settings
        } else if ext_in(&["json", "jsonc", "json5"]) {
            Icon::FileJson
        } else if ext_in(&["toml", "yaml", "yml", "ini", "cfg", "conf", "properties"]) {
            Icon::FileCog
        } else if ext_in(&["md", "markdown", "mdx", "txt", "rst", "log"]) {
            Icon::FileText
        } else if ext_in(&[
            "png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "bmp", "avif",
        ]) {
            Icon::FileImage
        } else if ext_in(&["csv", "tsv", "xls", "xlsx", "ods"]) {
            Icon::FileSpreadsheet
        } else if ext_in(&[
            "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "go", "java", "kt", "c", "h", "cc",
            "cpp", "hpp", "cs", "rb", "php", "swift", "sh", "bash", "ps1", "bat", "cmd", "html",
            "htm", "css", "scss", "vue", "svelte", "sql", "lua", "zig", "dart",
        ]) {
            Icon::FileCode
        } else {
            Icon::File
        }
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
        if let Some(icon) = Icon::ALL.iter().find(|icon| icon.path() == path) {
            return Ok(Some(Cow::Owned(lucide_stroke(icon.bytes()))));
        }
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

/// A Lucide file with its stroke width set to [`theme::ICON_STROKE`] (ADR 0116). gpui caches
/// what `load` returns, so this runs once per icon.
fn lucide_stroke(svg: &[u8]) -> Vec<u8> {
    String::from_utf8_lossy(svg)
        .replacen(
            "stroke-width=\"2\"",
            &format!("stroke-width=\"{}\"", theme::ICON_STROKE),
            1,
        )
        .into_bytes()
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

    #[test]
    fn files_map_to_icons_by_name_then_extension() {
        let cases = [
            ("main.rs", Icon::FileCode),
            ("App.TSX", Icon::FileCode),
            ("README.MD", Icon::FileText),
            ("notes.txt", Icon::FileText),
            ("package.json", Icon::FileJson),
            ("package-lock.json", Icon::Lock),
            ("pnpm-lock.yaml", Icon::Lock),
            ("Cargo.lock", Icon::Lock),
            (".env", Icon::Lock),
            (".env.local", Icon::Lock),
            (".ENV", Icon::Lock),
            (".Env.Local", Icon::Lock),
            (".envrc", Icon::Settings),
            (".GitIgnore", Icon::Settings),
            (".EditorConfig", Icon::Settings),
            ("Cargo.toml", Icon::FileCog),
            ("ci.yml", Icon::FileCog),
            (".gitignore", Icon::Settings),
            (".bashrc", Icon::Settings),
            ("vite.config.ts", Icon::Settings),
            ("logo.PNG", Icon::FileImage),
            ("data.csv", Icon::FileSpreadsheet),
            ("Makefile", Icon::File),
            ("archive.tar.gz", Icon::File),
            (".hidden", Icon::File),
            ("", Icon::File),
        ];
        for (name, icon) in cases {
            assert_eq!(Icon::for_file(name), icon, "{name}");
        }
    }
}
