//! Everything but Windows: nothing to show or hide, and no tray, so `has_tray` is false and the
//! window's ✕ quits as before (macOS dock badge: gpui-pre 0.3.7 has no call, ADR 0064).

use super::TrayEvent;
use crate::attention::{Icon, TrayMenu};

pub struct Os;

impl Os {
    /// The receiver is closed at once: there is no tray to listen to.
    pub fn new(_window: &gpui::Window) -> (Self, async_channel::Receiver<TrayEvent>) {
        (Self, async_channel::unbounded::<TrayEvent>().1)
    }

    pub fn has_tray(&self) -> bool {
        false
    }

    pub fn set_badge(&mut self, _count: usize, _bg: u32, _ink: u32) {}

    pub fn flash(&mut self, _on: bool) {}

    pub fn set_tray(&self, _icon: Icon, _rgb: u32, _menu: TrayMenu) {}

    pub fn remove_tray(&self) {}

    pub fn hide_window(&mut self) {}

    pub fn show_window(&mut self) {}
}

pub fn raise(_key: &str) -> bool {
    false
}
