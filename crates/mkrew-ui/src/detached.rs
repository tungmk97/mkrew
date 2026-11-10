//! Which terminal tab sits in which detached window (spec 2026-11-10 §4.1, ADR 0096).
//! Pure: the shell owns one [`Detached`] and draws from it. A tab in no window is in the
//! main window.

use serde::{Deserialize, Serialize};
use ulid::Ulid;

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

/// Where a moved tab came from, and the window it left empty (now dropped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moved {
    pub from: HostId,
    pub emptied: Option<WindowKey>,
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
    /// [`HostId::Main`] forgets it. A window left without tabs is dropped.
    pub fn move_tab(&mut self, tab: DockTab, to: HostId, index: Option<usize>) -> Moved {
        let (from, emptied) = self.take(&tab);
        if let HostId::Detached(key) = to
            && let Some(window) = self.window_mut(key)
        {
            let at = index.unwrap_or(window.tabs.len()).min(window.tabs.len());
            window.tabs.insert(at, tab);
            window.active = at;
        }
        Moved { from, emptied }
    }

    /// Removes `tab` from its window; returns that window and whether it emptied (dropped).
    fn take(&mut self, tab: &DockTab) -> (HostId, Option<WindowKey>) {
        let Some(at) = self
            .windows
            .iter()
            .position(|window| window.tabs.contains(tab))
        else {
            return (HostId::Main, None);
        };
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
            return (HostId::Detached(key), Some(key));
        }
        (HostId::Detached(key), None)
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
        gone.iter().filter_map(|tab| self.take(tab).1).collect()
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

    /// Quitting the app keeps every detached tab alive (spec §6.2): nothing to close.
    pub fn quit_closes(&self) -> Vec<DockTab> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests;
