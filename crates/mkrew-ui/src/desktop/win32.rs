//! Windows side of [`super::Desktop`] (ADR 0064): the taskbar overlay and flash of the main
//! window and hiding / showing it, through the `windows` crate gpui's backend already builds
//! with. The tray icon lives here too, on its own thread.
#![allow(unsafe_code)]

use std::ffi::c_void;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use ulid::Ulid;
use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::UI::Shell::{ITaskbarList3, TaskbarList};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, DestroyIcon, FLASHW_STOP, FLASHW_TIMERNOFG, FLASHW_TRAY, FLASHWINFO,
    FlashWindowEx, GetSystemMetrics, HICON, ICONINFO, IsIconic, SM_CXSMICON, SW_HIDE, SW_RESTORE,
    SW_SHOW, ShowWindow,
};
use windows::core::HSTRING;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFY_ICON_MESSAGE,
    NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
    DestroyMenu, DispatchMessageW, FindWindowW, GetCursorPos, GetMessageW,
    GetWindowThreadProcessId, IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW, MF_GRAYED, MF_SEPARATOR,
    MF_STRING, MSG, PostMessageW, RegisterClassW, RegisterWindowMessageW, SMTO_ABORTIFHUNG,
    SendMessageTimeoutW, SetForegroundWindow, TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TrackPopupMenu, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_COPYDATA,
    WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WNDCLASSW,
};
use windows::core::{PCWSTR, w};

use super::escape_menu;
use crate::status;

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

/// Tray icon callback (`uCallbackMessage`); `lParam` is the mouse message.
const WM_TRAY: u32 = WM_APP + 1;
/// UI → tray thread: `wParam` icon index (0 idle, 1 working, 2 needs you), `lParam` `0xRRGGBB`.
const WM_TRAY_STATE: u32 = WM_APP + 2;
/// A second launch asks the first to show its window.
const WM_OPEN: u32 = WM_APP + 3;
/// `dwData` of the `WM_COPYDATA` a link launch sends ("MKJP"); `lpData` is [`encode_jump`].
const COPYDATA_JUMP: usize = 0x4D4B_4A50;
/// How long a link launch waits for the running UI before giving up (a hung UI).
const COPYDATA_TIMEOUT_MS: u32 = 2000;
const ICON_ID: u32 = 1;
const CMD_OPEN: i32 = 1;
const CMD_QUIT: i32 = 2;
const CMD_SESSION: i32 = 100;
/// First command id of the machine items (one per machine, `CMD_MACHINE + index`).
const CMD_MACHINE: i32 = 200;

/// What the UI thread shares with the tray thread.
struct Shared {
    menu: Mutex<TrayMenu>,
    /// The icon is in the notification area.
    shown: AtomicBool,
    /// The tray window, 0 until it exists.
    hwnd: AtomicIsize,
    events: async_channel::Sender<TrayEvent>,
}

/// The tray icon: its hidden window and message loop live on the `mkrew-tray` thread.
pub struct Tray {
    shared: Arc<Shared>,
}

fn class_name(key: &str) -> Vec<u16> {
    format!("MkrewTray.{key}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

/// Session then project ULID, 16 bytes each.
fn encode_jump((session, project): (Ulid, Ulid)) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..16].copy_from_slice(&session.to_bytes());
    out[16..].copy_from_slice(&project.to_bytes());
    out
}

fn decode_jump(bytes: &[u8]) -> Option<(Ulid, Ulid)> {
    let bytes: &[u8; 32] = bytes.try_into().ok()?;
    let (session, project) = bytes.split_at(16);
    Some((
        Ulid::from_bytes(session.try_into().ok()?),
        Ulid::from_bytes(project.try_into().ok()?),
    ))
}

fn icon_index(icon: Icon) -> usize {
    match icon {
        Icon::Idle => 0,
        Icon::Working => 1,
        Icon::NeedsYou => 2,
    }
}

fn icon_of(index: usize) -> Icon {
    match index {
        1 => Icon::Working,
        2 => Icon::NeedsYou,
        _ => Icon::Idle,
    }
}

impl Tray {
    /// Starts the tray thread; `None` if its window could not be made.
    pub fn start(key: &str) -> Option<(Tray, async_channel::Receiver<TrayEvent>)> {
        let (events, receiver) = async_channel::unbounded();
        let shared = Arc::new(Shared {
            menu: Mutex::new(TrayMenu::default()),
            shown: AtomicBool::new(false),
            hwnd: AtomicIsize::new(0),
            events,
        });
        let (ready, started) = std::sync::mpsc::sync_channel(1);
        let (class, thread) = (class_name(key), Arc::clone(&shared));
        std::thread::Builder::new()
            .name("mkrew-tray".into())
            .spawn(move || run(&class, &thread, &ready))
            .ok()?;
        started.recv_timeout(Duration::from_secs(2)).ok()?;
        (shared.hwnd.load(Ordering::Acquire) != 0).then_some((Tray { shared }, receiver))
    }

    /// The icon is in the notification area (a failed `NIM_ADD` leaves it false).
    pub fn shown(&self) -> bool {
        self.shared.shown.load(Ordering::Relaxed)
    }

    /// New menu data and icon; the tray thread applies them.
    pub fn set(&self, icon: Icon, rgb: u32, menu: TrayMenu) {
        *self
            .shared
            .menu
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = menu;
        let hwnd = HWND(self.shared.hwnd.load(Ordering::Acquire) as *mut c_void);
        // SAFETY: posting to a window of this process; a dead window just fails.
        let posted = unsafe {
            PostMessageW(
                Some(hwnd),
                WM_TRAY_STATE,
                WPARAM(icon_index(icon)),
                LPARAM(rgb as isize),
            )
        };
        if let Err(error) = posted {
            tracing::warn!("cannot update the tray icon: {error}");
        }
    }

    /// Takes the icon out of the notification area (the process is about to exit).
    pub fn remove(&self) {
        let hwnd = HWND(self.shared.hwnd.load(Ordering::Acquire) as *mut c_void);
        let data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: ICON_ID,
            ..Default::default()
        };
        // SAFETY: `data` is initialised and names the icon `run` added.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
        }
        self.shared.shown.store(false, Ordering::Relaxed);
    }
}

/// The icon goes with the `Tray` (dropped when the window closes or the app shuts down), so no
/// ghost is left in the notification area; [`Tray::remove`] again is harmless.
impl Drop for Tray {
    fn drop(&mut self) {
        self.remove();
    }
}

/// Tray-thread state, reached from the window procedure.
struct State {
    shared: Arc<Shared>,
    /// `RegisterWindowMessageW("TaskbarCreated")`: Explorer restarted, add the icon again.
    taskbar_created: u32,
    /// The icon currently in the notification area.
    current: RefCell<Option<OwnedIcon>>,
    /// The last (icon, color), for the re-add.
    kind: Cell<(Icon, u32)>,
}

thread_local! {
    static STATE: RefCell<Option<Rc<State>>> = const { RefCell::new(None) };
}

fn app_icon(size: u32) -> Option<OwnedIcon> {
    let side = i32::try_from(size).ok()?;
    // SAFETY: resource id 1 of this exe is the app icon (`mkrew.rc`); the handle is ours to destroy.
    unsafe {
        let module = GetModuleHandleW(PCWSTR::null()).ok()?;
        let handle = LoadImageW(
            Some(HINSTANCE(module.0)),
            PCWSTR(std::ptr::without_provenance::<u16>(1)),
            IMAGE_ICON,
            side,
            side,
            LR_DEFAULTCOLOR,
        )
        .ok()?;
        Some(OwnedIcon(HICON(handle.0)))
    }
}

impl State {
    fn send(&self, event: TrayEvent) {
        let _ = self.shared.events.try_send(event);
    }

    /// `NIM_ADD` or `NIM_MODIFY` with the icon for `icon` / `rgb` and the current tooltip.
    fn apply(&self, hwnd: HWND, message: NOTIFY_ICON_MESSAGE, icon: Icon, rgb: u32) {
        self.kind.set((icon, rgb));
        let size = small_icon_size();
        let made = match icon {
            Icon::Idle => app_icon(size),
            Icon::Working | Icon::NeedsYou => icon_from(&glyph::dot(size, rgb)),
        };
        let tip = (self.shared.menu.lock())
            .unwrap_or_else(PoisonError::into_inner)
            .tip();
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: ICON_ID,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: made
                .as_ref()
                .map_or(HICON(std::ptr::null_mut()), |icon| icon.0),
            ..Default::default()
        };
        for (slot, unit) in data.szTip.iter_mut().zip(tip.encode_utf16().take(127)) {
            *slot = unit;
        }
        // SAFETY: `data` is fully initialised and `cbSize` is set.
        let ok = unsafe { Shell_NotifyIconW(message, &data) }.as_bool();
        if message == NIM_ADD {
            self.shared.shown.store(ok, Ordering::Relaxed);
            if !ok {
                tracing::warn!("the tray icon could not be added; ✕ will quit");
            }
        }
        if ok {
            // The shell has the new icon: the old one can go.
            *self.current.borrow_mut() = made;
        }
    }

    /// The right-click menu: sessions that need you, `● N working`, Open, Quit UI.
    fn show_menu(&self, hwnd: HWND) {
        let menu = (self.shared.menu.lock())
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let now = status::now_ms();
        // SAFETY: a popup menu made, shown and destroyed on this thread; `hwnd` is our window.
        let picked = unsafe {
            let Ok(popup) = CreatePopupMenu() else {
                return;
            };
            for (index, item) in menu.items.iter().enumerate() {
                let text = HSTRING::from(escape_menu(&item.text(now)));
                let _ = AppendMenuW(popup, MF_STRING, CMD_SESSION as usize + index, &text);
            }
            if !menu.items.is_empty() {
                let _ = AppendMenuW(popup, MF_SEPARATOR, 0, PCWSTR::null());
            }
            let working = HSTRING::from(format!("● {} working", menu.working));
            let _ = AppendMenuW(popup, MF_STRING | MF_GRAYED, 0, &working);
            let _ = AppendMenuW(popup, MF_SEPARATOR, 0, PCWSTR::null());
            for (index, (name, state)) in menu.machines.iter().enumerate().take(256) {
                let text = HSTRING::from(escape_menu(&format!("{name} - {state}")));
                let _ = AppendMenuW(popup, MF_STRING, CMD_MACHINE as usize + index, &text);
            }
            if !menu.machines.is_empty() {
                let _ = AppendMenuW(popup, MF_SEPARATOR, 0, PCWSTR::null());
            }
            let _ = AppendMenuW(popup, MF_STRING, CMD_OPEN as usize, w!("Open Mkrew"));
            let _ = AppendMenuW(
                popup,
                MF_STRING,
                CMD_QUIT as usize,
                w!("Quit UI (agents keep running)"),
            );
            let mut at = POINT::default();
            let _ = GetCursorPos(&mut at);
            // A tray menu only closes when its owner is the foreground window; the WM_NULL
            // after it lets the menu go away (the documented tray-menu recipe).
            let _ = SetForegroundWindow(hwnd);
            let picked = TrackPopupMenu(
                popup,
                TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
                at.x,
                at.y,
                None,
                hwnd,
                None,
            );
            let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(popup);
            picked.0
        };
        match picked {
            0 => {}
            CMD_OPEN => self.send(TrayEvent::Open),
            CMD_QUIT => self.send(TrayEvent::Quit),
            id if id >= CMD_MACHINE => {
                if let Ok(index) = u8::try_from(id - CMD_MACHINE)
                    && usize::from(index) < menu.machines.len()
                {
                    self.send(TrayEvent::Machine(index));
                }
            }
            id if id >= CMD_SESSION => {
                let index = usize::try_from(id - CMD_SESSION).unwrap_or(usize::MAX);
                if let Some(item) = menu.items.get(index) {
                    self.send(TrayEvent::Session(item.session_id));
                }
            }
            _ => {}
        }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Cloned out so no borrow is held while a menu pumps messages back into this procedure.
    let state = STATE.with(|state| state.borrow().clone());
    let Some(state) = state else {
        // SAFETY: forwarding the unchanged message.
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    };
    match message {
        WM_TRAY => match lparam.0 as u32 {
            WM_LBUTTONUP => state.send(TrayEvent::Open),
            WM_RBUTTONUP => state.show_menu(hwnd),
            _ => {}
        },
        WM_TRAY_STATE => state.apply(hwnd, NIM_MODIFY, icon_of(wparam.0), lparam.0 as u32),
        WM_OPEN => state.send(TrayEvent::Open),
        WM_COPYDATA => {
            // SAFETY: for WM_COPYDATA `lParam` points to a COPYDATASTRUCT, and `lpData` to its
            // `cbData` bytes, both valid for this call.
            let jump = unsafe {
                let data = &*(lparam.0 as *const COPYDATASTRUCT);
                (data.dwData == COPYDATA_JUMP && !data.lpData.is_null())
                    .then(|| {
                        std::slice::from_raw_parts(data.lpData.cast::<u8>(), data.cbData as usize)
                    })
                    .and_then(decode_jump)
            };
            let Some((session, project)) = jump else {
                return LRESULT(0);
            };
            state.send(TrayEvent::Notification { session, project });
            return LRESULT(1);
        }
        other if other == state.taskbar_created => {
            let (icon, rgb) = state.kind.get();
            state.apply(hwnd, NIM_ADD, icon, rgb);
            // The new taskbar button has no overlay either.
            state.send(TrayEvent::TaskbarRestarted);
        }
        // SAFETY: forwarding the unchanged message.
        _ => return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
    LRESULT(0)
}

/// The tray thread: register the class, make the hidden window, add the icon, pump messages.
fn run(class: &[u16], shared: &Arc<Shared>, ready: &SyncSender<()>) {
    // SAFETY: Win32 window creation on this thread; `class` outlives every call that reads it.
    unsafe {
        let Ok(module) = GetModuleHandleW(PCWSTR::null()) else {
            let _ = ready.send(());
            return;
        };
        let hinstance = HINSTANCE(module.0);
        let class_name = PCWSTR(class.as_ptr());
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: hinstance,
            lpszClassName: class_name,
            ..Default::default()
        };
        if RegisterClassW(&window_class) == 0 {
            tracing::warn!("cannot register the tray window class");
            let _ = ready.send(());
            return;
        }
        // A plain hidden top-level window (never shown): only top-level windows get the
        // `TaskbarCreated` broadcast, and `FindWindowW` of a second launch finds only those.
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("Mkrew tray"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinstance),
            None,
        ) else {
            tracing::warn!("cannot make the tray window");
            let _ = ready.send(());
            return;
        };
        let state = Rc::new(State {
            shared: Arc::clone(shared),
            taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
            current: RefCell::new(None),
            kind: Cell::new((Icon::Idle, 0)),
        });
        STATE.with(|slot| *slot.borrow_mut() = Some(Rc::clone(&state)));
        state.apply(hwnd, NIM_ADD, Icon::Idle, 0);
        shared.hwnd.store(hwnd.0 as isize, Ordering::Release);
        let _ = ready.send(());
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

/// Another running Mkrew of the same exe was asked to show its window, on `jump`'s session
/// when given. A running UI that does not take the jump (an older build, or hung) is still
/// asked to show its window: this launch ends either way, never a second UI.
pub fn raise(key: &str, jump: Option<(Ulid, Ulid)>) -> bool {
    let class = class_name(key);
    // SAFETY: `class` is NUL-terminated and outlives `FindWindowW`; `bytes` and `data` outlive
    // the synchronous `SendMessageTimeoutW`, which copies them into the other process.
    unsafe {
        let Ok(hwnd) = FindWindowW(PCWSTR(class.as_ptr()), PCWSTR::null()) else {
            return false;
        };
        let open = || PostMessageW(Some(hwnd), WM_OPEN, WPARAM(0), LPARAM(0)).is_ok();
        let Some(jump) = jump else {
            return open();
        };
        // The toast click let this launch come to the front; pass that right to the running UI.
        let mut pid = 0u32;
        if GetWindowThreadProcessId(hwnd, Some(&mut pid)) != 0 {
            let _ = AllowSetForegroundWindow(pid);
        }
        let bytes = encode_jump(jump);
        let data = COPYDATASTRUCT {
            dwData: COPYDATA_JUMP,
            cbData: bytes.len() as u32,
            lpData: bytes.as_ptr() as *mut c_void,
        };
        let mut result = 0usize;
        let sent = SendMessageTimeoutW(
            hwnd,
            WM_COPYDATA,
            WPARAM(0),
            LPARAM(&raw const data as isize),
            SMTO_ABORTIFHUNG,
            COPYDATA_TIMEOUT_MS,
            Some(&mut result),
        );
        if sent.0 != 0 && result == 1 {
            return true;
        }
        tracing::warn!("the running Mkrew did not take the session; showing its window only");
        open()
    }
}

/// Whether a tray window of this `key` exists: a Mkrew of the same exe and instance runs.
pub fn running(key: &str) -> bool {
    let class = class_name(key);
    // SAFETY: `class` is NUL-terminated and outlives `FindWindowW`.
    unsafe { FindWindowW(PCWSTR(class.as_ptr()), PCWSTR::null()) }.is_ok()
}

/// The main window's Windows-only extras.
pub struct Os {
    hwnd: Option<HWND>,
    list: Option<ITaskbarList3>,
    flashing: bool,
    badge: Option<Badge>,
    tray: Option<Tray>,
    /// Off in a machine window: the hidden tray window stays (it is the single-instance
    /// target) but no icon shows.
    tray_icon: bool,
}

impl Os {
    pub fn new(
        window: &gpui::Window,
        id: &str,
        tray_icon: bool,
    ) -> (Self, async_channel::Receiver<TrayEvent>) {
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
        let (tray, events) = match Tray::start(&super::current_key(id)) {
            Some((tray, events)) => (Some(tray), events),
            None => (None, async_channel::unbounded::<TrayEvent>().1),
        };
        let this = Self {
            hwnd,
            list,
            flashing: false,
            badge: None,
            tray,
            tray_icon,
        };
        (this, events)
    }

    /// The overlay icon with `count` (cleared at 0).
    pub fn set_badge(&mut self, count: usize, bg: u32, ink: u32) {
        self.badge = (count > 0).then_some(Badge { count, bg, ink });
        self.reapply_badge();
    }

    /// Sets the overlay again: a button Explorer made after `show_window` or a restart has none.
    pub fn reapply_badge(&self) {
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

    pub fn has_tray(&self) -> bool {
        self.tray_icon && self.tray.as_ref().is_some_and(Tray::shown)
    }

    pub fn set_tray(&self, icon: Icon, rgb: u32, menu: TrayMenu) {
        if let Some(tray) = self.tray.as_ref().filter(|_| self.tray_icon) {
            tray.set(icon, rgb, menu);
        }
    }

    pub fn remove_tray(&self) {
        if let Some(tray) = &self.tray {
            tray.remove();
        }
    }

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
        // The taskbar button comes back asynchronously, without its overlay: the UI calls
        // `reapply_badge` shortly after (`Shell::restore_window`).
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_launch_reaches_the_tray_window() {
        let key = format!("test-{}", std::process::id());
        assert!(!raise(&key, None), "nobody listens yet");
        let (tray, events) = Tray::start(&key).expect("the tray window");
        assert!(raise(&key, None));
        let started = std::time::Instant::now();
        let event = loop {
            if let Ok(event) = events.try_recv() {
                break Some(event);
            }
            if started.elapsed() > std::time::Duration::from_secs(2) {
                break None;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert_eq!(event, Some(TrayEvent::Open));
        tray.remove();
    }

    #[test]
    fn a_jump_survives_the_copy() {
        let jump = (Ulid::new(), Ulid::new());
        assert_eq!(decode_jump(&encode_jump(jump)), Some(jump));
        assert_eq!(decode_jump(&encode_jump(jump)[..31]), None);
        assert_eq!(decode_jump(&[0u8; 33]), None);
    }

    #[test]
    fn a_link_launch_reaches_the_tray_window_with_its_session() {
        let key = format!("test-jump-{}", std::process::id());
        let jump = (Ulid::new(), Ulid::new());
        assert!(!raise(&key, Some(jump)), "nobody listens yet");
        let (tray, events) = Tray::start(&key).expect("the tray window");
        // `SendMessageTimeoutW` returns once the tray thread handled it: no polling needed.
        assert!(raise(&key, Some(jump)));
        assert_eq!(
            events.try_recv().ok(),
            Some(TrayEvent::Notification {
                session: jump.0,
                project: jump.1
            })
        );
        tray.remove();
    }

    /// Set by [`older_ui_proc`] when the "show the window" message arrives.
    static OLDER_UI_OPENED: AtomicBool = AtomicBool::new(false);

    /// A tray window of a Mkrew without links: `WM_COPYDATA` goes to `DefWindowProcW` (0).
    unsafe extern "system" fn older_ui_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_OPEN {
            OLDER_UI_OPENED.store(true, Ordering::SeqCst);
        }
        // SAFETY: forwarding the unchanged message.
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    /// After an update the running UI may be an older build: the link launch must still raise
    /// it and end, not start a second UI on the same daemon.
    #[test]
    fn an_older_ui_that_ignores_the_jump_is_still_raised() {
        let key = format!("test-older-{}", std::process::id());
        let class = class_name(&key);
        let (ready, created) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            // SAFETY: Win32 window creation and a message loop on this thread; `class` lives
            // as long as the thread.
            unsafe {
                let module = GetModuleHandleW(PCWSTR::null()).unwrap();
                let hinstance = HINSTANCE(module.0);
                let window_class = WNDCLASSW {
                    lpfnWndProc: Some(older_ui_proc),
                    hInstance: hinstance,
                    lpszClassName: PCWSTR(class.as_ptr()),
                    ..Default::default()
                };
                assert_ne!(RegisterClassW(&window_class), 0);
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    PCWSTR(class.as_ptr()),
                    w!("older Mkrew tray"),
                    WINDOW_STYLE::default(),
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    Some(hinstance),
                    None,
                )
                .unwrap();
                ready.send(()).unwrap();
                let mut message = MSG::default();
                while GetMessageW(&mut message, None, 0, 0).0 > 0 {
                    DispatchMessageW(&message);
                }
            }
        });
        created.recv().unwrap();
        assert!(raise(&key, Some((Ulid::new(), Ulid::new()))));
        let started = std::time::Instant::now();
        while !OLDER_UI_OPENED.load(Ordering::SeqCst) {
            assert!(
                started.elapsed() < std::time::Duration::from_secs(2),
                "never asked to show its window"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[test]
    fn a_badge_bitmap_becomes_an_icon() {
        let bitmap = glyph::badge(3, 16, 0xF5A623, 0x141519);
        assert!(icon_from(&bitmap).is_some());
        assert!(icon_from(&glyph::dot(16, 0xF5A623)).is_some());
    }
}
