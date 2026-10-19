//! Windows side of [`super::Desktop`] (ADR 0064): the taskbar overlay and flash of the main
//! window and hiding / showing it, through the `windows` crate gpui's backend already builds
//! with. The tray is in the same file (added by the next task).
#![allow(unsafe_code)]

use std::ffi::c_void;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::Shell::{ITaskbarList3, TaskbarList};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, DestroyIcon, FLASHW_STOP, FLASHW_TIMERNOFG, FLASHW_TRAY, FLASHWINFO,
    FlashWindowEx, GetSystemMetrics, HICON, ICONINFO, IsIconic, SM_CXSMICON, SW_HIDE, SW_RESTORE,
    SW_SHOW, ShowWindow,
};
use windows::core::HSTRING;

use super::TrayEvent;
use crate::attention::{Icon, TrayMenu};
use crate::glyph::{self, Bitmap};
use crate::inbox;

/// An icon this process made; destroyed on drop.
pub struct OwnedIcon(pub HICON);

impl Drop for OwnedIcon {
    fn drop(&mut self) {
        // SAFETY: the handle came from `CreateIconIndirect` and is destroyed once.
        unsafe {
            let _ = DestroyIcon(self.0);
        }
    }
}

/// The small-icon size of the current DPI (16 px at 100 %).
pub fn small_icon_size() -> u32 {
    // SAFETY: plain query, no pointers.
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) };
    u32::try_from(size).unwrap_or(16).max(16)
}

/// A 32-bit alpha icon from `bitmap`.
pub fn icon_from(bitmap: &Bitmap) -> Option<OwnedIcon> {
    let size = i32::try_from(bitmap.size).ok()?;
    // A 1-bit mask, rows padded to 16 bits; all zero = the colour bitmap decides (its alpha).
    let mask_bits = vec![0u8; (bitmap.size.div_ceil(16) * 2 * bitmap.size) as usize];
    // SAFETY: `bgra` holds `size * size * 4` bytes and `mask_bits` one padded row per line;
    // `CreateBitmap` copies both. The bitmaps are freed below, the icon keeps its own copy.
    unsafe {
        let color = CreateBitmap(
            size,
            size,
            1,
            32,
            Some(bitmap.bgra.as_ptr().cast::<c_void>()),
        );
        let mask = CreateBitmap(size, size, 1, 1, Some(mask_bits.as_ptr().cast::<c_void>()));
        let info = ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&info);
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon.map_err(|error| tracing::warn!("cannot make an icon: {error}"))
            .ok()
            .map(OwnedIcon)
    }
}

#[derive(Clone, Copy)]
struct Badge {
    count: usize,
    bg: u32,
    ink: u32,
}

/// The main window's Windows-only extras.
pub struct Os {
    hwnd: Option<HWND>,
    list: Option<ITaskbarList3>,
    flashing: bool,
    badge: Option<Badge>,
}

impl Os {
    pub fn new(window: &gpui::Window) -> (Self, async_channel::Receiver<TrayEvent>) {
        let hwnd = match HasWindowHandle::window_handle(window).map(|handle| handle.as_raw()) {
            Ok(RawWindowHandle::Win32(handle)) => Some(HWND(handle.hwnd.get() as *mut c_void)),
            _ => None,
        };
        // gpui initialised OLE (an STA) on this thread, so the shell's taskbar list can be made.
        let list = hwnd.and_then(|_| {
            // SAFETY: plain COM activation; no pointers are passed in.
            let made: windows::core::Result<ITaskbarList3> =
                unsafe { CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER) };
            // SAFETY: `HrInit` takes no arguments.
            made.and_then(|list| unsafe { list.HrInit() }.map(|()| list))
                .inspect_err(|error| tracing::warn!("taskbar badge unavailable: {error}"))
                .ok()
        });
        let this = Self {
            hwnd,
            list,
            flashing: false,
            badge: None,
        };
        (this, async_channel::unbounded::<TrayEvent>().1)
    }

    pub fn has_tray(&self) -> bool {
        false
    }

    /// The overlay icon with `count` (cleared at 0).
    pub fn set_badge(&mut self, count: usize, bg: u32, ink: u32) {
        self.badge = (count > 0).then_some(Badge { count, bg, ink });
        self.apply_badge();
    }

    fn apply_badge(&self) {
        let (Some(hwnd), Some(list)) = (self.hwnd, &self.list) else {
            return;
        };
        let icon = self
            .badge
            .and_then(|b| icon_from(&glyph::badge(b.count, small_icon_size(), b.bg, b.ink)));
        let description = self
            .badge
            .map_or_else(HSTRING::new, |b| HSTRING::from(inbox::label(b.count)));
        let handle = icon
            .as_ref()
            .map_or(HICON(std::ptr::null_mut()), |icon| icon.0);
        // SAFETY: `hwnd` is the live main window; the shell copies the icon during the call; a
        // null icon clears the overlay.
        if let Err(error) = unsafe { list.SetOverlayIcon(hwnd, handle, &description) } {
            tracing::warn!("cannot set the taskbar badge: {error}");
        }
    }

    /// Starts or stops flashing the taskbar button (until the window is in front).
    pub fn flash(&mut self, on: bool) {
        let Some(hwnd) = self.hwnd else {
            return;
        };
        if std::mem::replace(&mut self.flashing, on) == on {
            return;
        }
        let info = FLASHWINFO {
            cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
            hwnd,
            dwFlags: if on {
                FLASHW_TRAY | FLASHW_TIMERNOFG
            } else {
                FLASHW_STOP
            },
            uCount: 0,
            dwTimeout: 0,
        };
        // SAFETY: `info` is fully initialised and `cbSize` is set.
        unsafe {
            let _ = FlashWindowEx(&info);
        }
    }

    pub fn set_tray(&self, _icon: Icon, _rgb: u32, _menu: TrayMenu) {}

    pub fn remove_tray(&self) {}

    /// Hides the window (and its taskbar button); gpui reports it as `WindowVisibility::Hidden`.
    pub fn hide_window(&mut self) {
        if let Some(hwnd) = self.hwnd {
            // SAFETY: `hwnd` is the live main window.
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    }

    /// Shows a hidden or minimized window; the caller raises it with `Window::activate_window`.
    pub fn show_window(&mut self) {
        let Some(hwnd) = self.hwnd else {
            return;
        };
        // SAFETY: `hwnd` is the live main window.
        unsafe {
            let command = if IsIconic(hwnd).as_bool() {
                SW_RESTORE
            } else {
                SW_SHOW
            };
            let _ = ShowWindow(hwnd, command);
        }
        // A hidden window's taskbar button starts without its overlay.
        self.apply_badge();
    }
}

pub fn raise(_key: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_badge_bitmap_becomes_an_icon() {
        let bitmap = glyph::badge(3, 16, 0xF5A623, 0x141519);
        assert!(icon_from(&bitmap).is_some());
        assert!(icon_from(&glyph::dot(16, 0xF5A623)).is_some());
    }
}
