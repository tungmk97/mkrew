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

/// The window a tab, a render or an input belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HostId {
    Main,
    Detached(WindowKey),
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

    pub fn host_of(&self, tab: &DockTab) -> HostId {
        self.windows
            .iter()
            .find(|window| window.tabs.contains(tab))
            .map_or(HostId::Main, |window| HostId::Detached(window.key))
    }

    pub fn is_detached(&self, tab: &DockTab) -> bool {
        self.host_of(tab) != HostId::Main
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

    /// Moves `tab` to `to` (at `index`, else the end) and shows it there. Moving to
    /// [`HostId::Main`] forgets it. Returns the window the move left without tabs (dropped).
    /// Moving to its own window reorders it in place; a window that does not exist changes
    /// nothing.
    pub fn move_tab(
        &mut self,
        tab: DockTab,
        to: HostId,
        index: Option<usize>,
    ) -> Option<WindowKey> {
        let current = self.host_of(&tab);
        if let HostId::Detached(key) = to {
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
        if let HostId::Detached(key) = to
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
    /// that window is still open, else the main window.
    pub fn still_open(&self, from: HostId) -> HostId {
        match from {
            HostId::Detached(key) if self.window(key).is_none() => HostId::Main,
            other => other,
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
                    self.move_tab(tab, HostId::Detached(key), None);
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
/// (a hidden or minimized window draws nothing), else the main window.
pub fn toast_host(activation: &[HostId], shown: impl Fn(HostId) -> bool) -> HostId {
    (activation.iter().copied())
        .find(|&host| shown(host))
        .unwrap_or(HostId::Main)
}

/// The commands a detached window acts on (spec 2026-11-10 §4.2); every other key there does
/// nothing, so none can act on the main window's tabs.
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
            | Command::Layout(_)
            | Command::EqualizePanes
            | Command::Focus(_)
            | Command::Resize(_)
            | Command::ZoomPane
            | Command::ClosePane
            | Command::RenamePane
            | Command::RenameTab
            | Command::OpenComposer
            | Command::CommandPalette
            | Command::ContextMenu
            | Command::ToggleFullScreen
            | Command::MoveTabToNewWindow
    )
}

/// This computer's detached windows, in the data dir (UI-only: the daemon never reads it).
pub const WINDOWS_FILE: &str = "windows.json";
const VERSION: u32 = 1;

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

/// One saved window: restore bounds, maximized, strip, shown tab.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedWindow {
    pub bounds: SavedBounds,
    pub maximized: bool,
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

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    windows: Vec<FileWindow>,
}

#[derive(Serialize, Deserialize)]
struct FileWindow {
    bounds: SavedBounds,
    #[serde(default)]
    maximized: bool,
    /// Raw, so a tab kind this build does not know (a newer build's) is skipped, not fatal.
    tabs: Vec<serde_json::Value>,
    #[serde(default)]
    active: usize,
}

pub fn to_json(windows: &[SavedWindow]) -> String {
    let file = File {
        version: VERSION,
        windows: (windows.iter())
            .map(|window| FileWindow {
                bounds: window.bounds.clone(),
                maximized: window.maximized,
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

pub fn from_json(text: &str) -> Result<Vec<SavedWindow>, String> {
    let file: File = serde_json::from_str(text).map_err(|error| error.to_string())?;
    if file.version != VERSION {
        return Err(format!("unknown version {}", file.version));
    }
    Ok((file.windows.into_iter())
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
                tabs,
                active,
            })
        })
        .collect())
}

/// Writes through `<file>.tmp` and a rename, so a crash never leaves half a file.
pub fn save(path: &Path, windows: &[SavedWindow]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension("json.tmp");
    let result =
        std::fs::write(&temp, to_json(windows)).and_then(|()| std::fs::rename(&temp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests;
