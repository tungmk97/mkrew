//! No web view on this OS (Linux): a `NativeView` cannot exist, so every method is unreachable.

use super::PageEvent;
use gpui::Window;
use std::path::Path;
use ulid::Ulid;

pub enum NativeView {}
pub enum WebSurface {}

impl NativeView {
    pub fn build(
        _window: &Window,
        _id: Ulid,
        _url: &str,
        _keys: &[String],
        _dark: bool,
        _data_dir: Option<&Path>,
        _events: async_channel::Sender<(Ulid, PageEvent)>,
    ) -> Result<NativeView, String> {
        Err(super::Unavailable::Linux.to_string())
    }
    pub fn load(&self, _url: &str) {
        match *self {}
    }
    pub fn script(&self, _js: &str) {
        match *self {}
    }
    pub fn set_shown(&self, _shown: bool) {
        match *self {}
    }
    pub fn set_dark(&self, _dark: bool) {
        match *self {}
    }
    pub fn focus(&self) {
        match *self {}
    }
    pub fn focus_parent(&self) {
        match *self {}
    }
    pub fn move_to(&self, _window: &Window) -> bool {
        match *self {}
    }
    pub fn surface(&self, _show: bool) -> WebSurface {
        match *self {}
    }
}

impl gpui::IntoElement for WebSurface {
    type Element = gpui::Empty;
    fn into_element(self) -> gpui::Empty {
        match self {}
    }
}
