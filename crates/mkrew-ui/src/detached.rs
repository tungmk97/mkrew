//! Which terminal tab sits in which detached window (spec 2026-11-10 §4.1, ADR 0096).
//! Pure: the shell owns one [`Detached`] and draws from it. A tab in no window is in the
//! main window.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

use ulid::Ulid;

use crate::keymap::Command;

/// A detached window, local to this UI process (never reused within it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowKey(pub u64);

/// A Space window (ADR 0115): a top-level window with a workspace, local to this UI process
/// (never reused within it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpaceKey(pub u64);

/// The window a tab, a render or an input belongs to: a Space window (a workspace's window,
/// ADR 0115) or a detached one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostId {
    Space(SpaceKey),
    Detached(WindowKey),
}

impl HostId {
    pub fn space(self) -> Option<SpaceKey> {
        match self {
            HostId::Space(key) => Some(key),
            HostId::Detached(_) => None,
        }
    }

    pub fn detached(self) -> Option<WindowKey> {
        match self {
            HostId::Detached(key) => Some(key),
            HostId::Space(_) => None,
        }
    }
}

/// A tab that can leave the main window. Plan 2 adds file tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockTab {
    Terminal { project: Ulid, tab_id: Ulid },
}

impl DockTab {
    pub fn project(&self) -> Ulid {
        match *self {
            DockTab::Terminal { project, .. } => project,
        }
    }
}

/// One detached window: its strip, left to right, and the tab it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub key: WindowKey,
    pub tabs: Vec<DockTab>,
    /// Index into `tabs`; `tabs` is never empty (an emptied window is dropped).
    pub active: usize,
    /// False while the OS window is minimized or hidden: its tab is detached (SPEC §10).
    pub visible: bool,
}

/// What [`Detached::prune`] asks about each tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    Yes,
    No,
    /// Its workspace's tabs are not loaded (yet, or again after a reconnect): kept.
    Unknown,
}

#[derive(Debug, Default)]
pub struct Detached {
    windows: Vec<Window>,
    next: u64,
}

impl Detached {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn windows(&self) -> &[Window] {
        &self.windows
    }

    pub fn window(&self, key: WindowKey) -> Option<&Window> {
        self.windows.iter().find(|window| window.key == key)
    }

    fn window_mut(&mut self, key: WindowKey) -> Option<&mut Window> {
        self.windows.iter_mut().find(|window| window.key == key)
    }

    /// The detached window holding `tab`; `None` while it is in its workspace's Space window.
    pub fn window_of(&self, tab: &DockTab) -> Option<WindowKey> {
        self.windows
            .iter()
            .find(|window| window.tabs.contains(tab))
            .map(|window| window.key)
    }

    pub fn is_detached(&self, tab: &DockTab) -> bool {
        self.window_of(tab).is_some()
    }

    pub fn active(&self, key: WindowKey) -> Option<DockTab> {
        let window = self.window(key)?;
        window.tabs.get(window.active).copied()
    }

    pub fn tabs_of(&self, key: WindowKey) -> Vec<DockTab> {
        self.window(key)
            .map(|window| window.tabs.clone())
            .unwrap_or_default()
    }

    /// A new window showing `tab` (taken from wherever it was).
    pub fn open(&mut self, tab: DockTab) -> WindowKey {
        self.take(&tab);
        let key = WindowKey(self.next);
        self.next += 1;
        self.windows.push(Window {
            key,
            tabs: vec![tab],
            active: 0,
            visible: true,
        });
        key
    }

    /// Moves `tab` to detached window `to` (at `index`, else the end) and shows it there;
    /// `None` forgets it (back to its workspace's Space window). Returns the window the move
    /// left without tabs (dropped). Moving to its own window reorders it in place; a window
    /// that does not exist changes nothing.
    pub fn move_tab(
        &mut self,
        tab: DockTab,
        to: Option<WindowKey>,
        index: Option<usize>,
    ) -> Option<WindowKey> {
        let current = self.window_of(&tab);
        if let Some(key) = to {
            let window = self.window_mut(key)?;
            if current == to {
                if let Some(pos) = window.tabs.iter().position(|t| *t == tab) {
                    window.tabs.remove(pos);
                    let at = index.unwrap_or(window.tabs.len()).min(window.tabs.len());
                    window.tabs.insert(at, tab);
                    window.active = at;
                }
                return None;
            }
        }
        let emptied = self.take(&tab);
        if let Some(key) = to
            && let Some(window) = self.window_mut(key)
        {
            let at = index.unwrap_or(window.tabs.len()).min(window.tabs.len());
            window.tabs.insert(at, tab);
            window.active = at;
        }
        emptied
    }

    /// Removes `tab` from its window; returns that window if it emptied (dropped).
    fn take(&mut self, tab: &DockTab) -> Option<WindowKey> {
        let at = self
            .windows
            .iter()
            .position(|window| window.tabs.contains(tab))?;
        let window = &mut self.windows[at];
        let key = window.key;
        if let Some(index) = window.tabs.iter().position(|t| t == tab) {
            window.tabs.remove(index);
            // The tab right of a removed active one, else the one left of it.
            if index < window.active || window.active >= window.tabs.len() {
                window.active = window.active.saturating_sub(1);
            }
        }
        if window.tabs.is_empty() {
            self.windows.remove(at);
            return Some(key);
        }
        None
    }

    /// Reorders a window's strip; the moved tab stays selected if it was.
    pub fn move_within(&mut self, key: WindowKey, from: usize, to: usize) {
        let Some(window) = self.window_mut(key) else {
            return;
        };
        if from >= window.tabs.len() || to >= window.tabs.len() {
            return;
        }
        let shown = window.tabs[window.active];
        let tab = window.tabs.remove(from);
        window.tabs.insert(to, tab);
        window.active = window.tabs.iter().position(|t| *t == shown).unwrap_or(0);
    }

    pub fn select(&mut self, key: WindowKey, index: usize) -> bool {
        match self.window_mut(key) {
            Some(window) if index < window.tabs.len() => {
                window.active = index;
                true
            }
            _ => false,
        }
    }

    pub fn cycle(&mut self, key: WindowKey, forward: bool) {
        if let Some(window) = self.window_mut(key) {
            let len = window.tabs.len();
            window.active = if forward {
                (window.active + 1) % len
            } else {
                (window.active + len - 1) % len
            };
        }
    }

    /// Drops the window and returns its tabs, strip order.
    pub fn remove_window(&mut self, key: WindowKey) -> Vec<DockTab> {
        match self.windows.iter().position(|window| window.key == key) {
            Some(at) => self.windows.remove(at).tabs,
            None => Vec::new(),
        }
    }

    /// Drops tabs that `keep` says are gone; returns the windows left empty (dropped).
    pub fn prune(&mut self, keep: impl Fn(&DockTab) -> Keep) -> Vec<WindowKey> {
        let gone: Vec<DockTab> = (self.windows.iter())
            .flat_map(|window| window.tabs.iter().copied())
            .filter(|tab| keep(tab) == Keep::No)
            .collect();
        gone.iter().filter_map(|tab| self.take(tab)).collect()
    }

    pub fn set_visible(&mut self, key: WindowKey, visible: bool) {
        if let Some(window) = self.window_mut(key) {
            window.visible = visible;
        }
    }

    /// The tab each visible window shows (attached; the others are detached, SPEC §10).
    pub fn visible_tabs(&self) -> Vec<DockTab> {
        (self.windows.iter())
            .filter(|window| window.visible)
            .filter_map(|window| window.tabs.get(window.active).copied())
            .collect()
    }

    /// Tabs in every detached window (a change in it means a strip changed).
    pub fn tab_count(&self) -> usize {
        self.windows.iter().map(|window| window.tabs.len()).sum()
    }

    /// Where a tab that came from `from` goes back (a window that failed to open): `from` if
    /// that window is still open (Space windows always are), else `None`.
    pub fn still_open(&self, from: HostId) -> Option<HostId> {
        match from {
            HostId::Detached(key) if self.window(key).is_none() => None,
            other => Some(other),
        }
    }

    /// Rebuilds the windows from a save (keys renumbered). Returns the keys in file order.
    pub fn restore(&mut self, saved: &[SavedWindow]) -> Vec<WindowKey> {
        saved
            .iter()
            .filter_map(|window| {
                let mut tabs: Vec<DockTab> = Vec::with_capacity(window.tabs.len());
                for &tab in &window.tabs {
                    if !self.is_detached(&tab) && !tabs.contains(&tab) {
                        tabs.push(tab);
                    }
                }
                let (&first, rest) = tabs.split_first()?;
                let key = self.open(first);
                for &tab in rest {
                    self.move_tab(tab, Some(key), None);
                }
                let shown = window.tabs.get(window.active);
                if let Some(index) =
                    shown.and_then(|shown| tabs.iter().position(|tab| tab == shown))
                {
                    self.select(key, index);
                }
                Some(key)
            })
            .collect()
    }
}

/// The window that draws the toasts: the most recently active of `activation` that is `shown`
/// (a hidden or minimized window draws nothing), else `fallback`.
pub fn toast_host(
    activation: &[HostId],
    shown: impl Fn(HostId) -> bool,
    fallback: HostId,
) -> HostId {
    (activation.iter().copied())
        .find(|&host| shown(host))
        .unwrap_or(fallback)
}

/// The commands a detached window acts on (spec 2026-11-10 §4.2); every other key there does
/// nothing, so none can act on the main window's tabs. The exception is Settings (Ctrl+, or ⌘,),
/// which opens in the main window on every OS (`run_detached`); it is not in this list.
pub fn handles(command: Command) -> bool {
    matches!(
        command,
        Command::NextTab
            | Command::PreviousTab
            | Command::Tab(_)
            | Command::CloseTab
            | Command::MoveTabLeft
            | Command::MoveTabRight
            | Command::SplitRight
            | Command::SplitDown
            | Command::SplitAgent
            | Command::SplitShell
            | Command::SplitBrowser
            | Command::BrowserAddress
            | Command::BrowserBack
            | Command::BrowserForward
            | Command::BrowserReload
            | Command::BrowserOpenExternal
            | Command::Layout(_)
            | Command::EqualizePanes
            | Command::Focus(_)
            | Command::Resize(_)
            | Command::ZoomPane
            | Command::ClosePane
            | Command::RenamePane
            | Command::RenameTab
            | Command::OpenComposer
            | Command::ToggleChat
            | Command::CommandPalette
            | Command::ContextMenu
            | Command::ToggleFullScreen
            | Command::MoveTabToNewWindow
    )
}

/// This computer's detached windows, in the data dir (UI-only: the daemon never reads it).
pub const WINDOWS_FILE: &str = "windows.json";
const VERSION: u32 = 2;

/// The windows file of a window: [`WINDOWS_FILE`] for this computer's window (`None`), its own
/// `windows-<key>.json` for a machine window (same key as [`crate::inbox::viewed_file`]).
pub fn windows_file(instance: Option<&str>) -> String {
    instance.map_or_else(
        || WINDOWS_FILE.to_owned(),
        |id| format!("windows-{}.json", crate::desktop::instance_key("", id)),
    )
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// One saved window: restore bounds, maximized or full screen, strip, shown tab.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedWindow {
    pub bounds: SavedBounds,
    pub maximized: bool,
    pub fullscreen: bool,
    pub tabs: Vec<DockTab>,
    pub active: usize,
}

impl SavedWindow {
    /// The window with only the tabs `exists` approves; the shown tab stays shown, else the
    /// first one is.
    pub fn keeping(self, exists: impl Fn(&DockTab) -> bool) -> Self {
        let shown = self.tabs.get(self.active).copied();
        let tabs: Vec<DockTab> = self.tabs.into_iter().filter(|tab| exists(tab)).collect();
        let active = shown
            .and_then(|shown| tabs.iter().position(|tab| *tab == shown))
            .unwrap_or(0);
        Self {
            tabs,
            active,
            ..self
        }
    }
}

/// The workspaces of `windows` whose tabs are not `loaded` yet: a saved tab can only be called
/// gone once its workspace answered (a failed or lost answer leaves it waiting).
pub fn waiting_projects(windows: &[SavedWindow], loaded: impl Fn(Ulid) -> bool) -> HashSet<Ulid> {
    (windows.iter())
        .flat_map(|window| window.tabs.iter().map(DockTab::project))
        .filter(|project| !loaded(*project))
        .collect()
}

/// What `windows.json` keeps while a restore is pending: the `live` windows, then the `pending`
/// ones (not opened yet) without any tab a live window has.
pub fn merge_saved(mut live: Vec<SavedWindow>, pending: &[SavedWindow]) -> Vec<SavedWindow> {
    let shown: HashSet<DockTab> = live.iter().flat_map(|w| w.tabs.iter().copied()).collect();
    live.extend(
        (pending.iter().cloned())
            .map(|window| window.keeping(|tab| !shown.contains(tab)))
            .filter(|window| !window.tabs.is_empty()),
    );
    live
}

/// One saved Space window (ADR 0115).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedSpace {
    /// `Ulid::nil()` when not known (read from the legacy `main-window.json`).
    pub workspace: Ulid,
    pub bounds: SavedBounds,
    #[serde(default)]
    pub maximized: bool,
    #[serde(default)]
    pub fullscreen: bool,
}

/// What `windows.json` keeps: the Space windows, the one focused last at the end, and the
/// detached windows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    pub spaces: Vec<SavedSpace>,
    pub windows: Vec<SavedWindow>,
}

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    #[serde(default)]
    spaces: Vec<SavedSpace>,
    windows: Vec<FileWindow>,
}

#[derive(Serialize, Deserialize)]
struct FileWindow {
    bounds: SavedBounds,
    #[serde(default)]
    maximized: bool,
    #[serde(default)]
    fullscreen: bool,
    /// Raw, so a tab kind this build does not know (a newer build's) is skipped, not fatal.
    tabs: Vec<serde_json::Value>,
    #[serde(default)]
    active: usize,
}

pub fn to_json(layout: &Layout) -> String {
    let file = File {
        version: VERSION,
        spaces: layout.spaces.clone(),
        windows: (layout.windows.iter())
            .map(|window| FileWindow {
                bounds: window.bounds.clone(),
                maximized: window.maximized,
                fullscreen: window.fullscreen,
                tabs: (window.tabs.iter())
                    .filter_map(|tab| serde_json::to_value(tab).ok())
                    .collect(),
                active: window.active,
            })
            .collect(),
    };
    // A struct of plain fields always serializes.
    serde_json::to_string(&file).unwrap_or_default()
}

/// Versions 1 (detached windows only) and 2; a newer version is refused.
pub fn from_json(text: &str) -> Result<Layout, String> {
    let file: File = serde_json::from_str(text).map_err(|error| error.to_string())?;
    if !(1..=VERSION).contains(&file.version) {
        return Err(format!("unknown version {}", file.version));
    }
    Ok(Layout {
        spaces: file.spaces,
        windows: saved_windows(file.windows),
    })
}

/// The saved detached windows: unknown and repeated tabs dropped, empty windows skipped.
fn saved_windows(windows: Vec<FileWindow>) -> Vec<SavedWindow> {
    (windows.into_iter())
        .filter_map(|window| {
            let shown = window.tabs.get(window.active).cloned();
            let mut tabs: Vec<DockTab> = Vec::with_capacity(window.tabs.len());
            for tab in window.tabs {
                if let Ok(tab) = serde_json::from_value::<DockTab>(tab)
                    && !tabs.contains(&tab)
                {
                    tabs.push(tab);
                }
            }
            let shown = shown.and_then(|tab| serde_json::from_value::<DockTab>(tab).ok());
            let active = shown
                .and_then(|shown| tabs.iter().position(|tab| *tab == shown))
                .unwrap_or(0);
            (!tabs.is_empty()).then_some(SavedWindow {
                bounds: window.bounds,
                maximized: window.maximized,
                fullscreen: window.fullscreen,
                tabs,
                active,
            })
        })
        .collect()
}

/// Writes through `<file>.tmp` and a rename, so a crash never leaves half a file.
pub fn save(path: &Path, layout: &Layout) -> std::io::Result<()> {
    write_atomic(path, &to_json(layout))
}

/// The saved layout; without saved Space windows, the legacy main window frame becomes the
/// one Space window (its workspace unknown). Read errors are logged and give what is left.
pub fn load_layout(windows_path: &Path, legacy_main: &Path) -> Layout {
    let mut layout = match std::fs::read_to_string(windows_path) {
        Ok(text) => from_json(&text).unwrap_or_else(|error| {
            tracing::warn!("ignoring the windows file: {error}");
            Layout::default()
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Layout::default(),
        Err(error) => {
            tracing::warn!("cannot read the windows file: {error}");
            Layout::default()
        }
    };
    if layout.spaces.is_empty()
        && let Some(frame) = load_frame(legacy_main)
    {
        layout.spaces.push(SavedSpace {
            workspace: Ulid::nil(),
            bounds: frame.bounds,
            maximized: frame.maximized,
            fullscreen: frame.fullscreen,
        });
    }
    layout
}

fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension("json.tmp");
    let result = std::fs::write(&temp, text).and_then(|()| std::fs::rename(&temp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// How the main window was left (`main-window.json`): the rect it restores to, and whether it
/// was maximized or in full screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedFrame {
    pub bounds: SavedBounds,
    #[serde(default)]
    pub maximized: bool,
    #[serde(default)]
    pub fullscreen: bool,
}

impl SavedFrame {
    /// `None` for text that is not a frame or holds a size no window can have: the window then
    /// opens centered as before, never in a broken place.
    pub fn parse(text: &str) -> Option<Self> {
        let frame: Self = serde_json::from_str(text).ok()?;
        let b = &frame.bounds;
        let finite = [b.x, b.y, b.width, b.height].iter().all(|v| v.is_finite());
        (finite && b.width >= 1.0 && b.height >= 1.0).then_some(frame)
    }
}

/// The main-window file of a window: `main-window.json` for this computer's window (`None`), its
/// own `main-window-<key>.json` for a machine window (same key as [`windows_file`]).
pub fn main_file(instance: Option<&str>) -> String {
    instance.map_or_else(
        || "main-window.json".to_owned(),
        |id| format!("main-window-{}.json", crate::desktop::instance_key("", id)),
    )
}

/// The saved frame in `path`; missing, unreadable or broken = none.
pub fn load_frame(path: &Path) -> Option<SavedFrame> {
    SavedFrame::parse(&std::fs::read_to_string(path).ok()?)
}

#[cfg(test)]
mod tests;
