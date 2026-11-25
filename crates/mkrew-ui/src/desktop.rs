//! The OS around the Space windows (SPEC §20J, ADR 0064): taskbar badge and flashing, the tray
//! icon and its menu, hiding and showing the window: Windows. macOS gets the Dock badge, the
//! Dock bounce and the About panel (SPEC §20L, ADR 0100); elsewhere every call does nothing
//! and `has_tray()` is false.

use ulid::Ulid;

use crate::keymap::Os;

#[cfg(target_os = "macos")]
mod dock;
#[cfg(not(windows))]
mod none;
#[cfg(windows)]
mod win32;

#[cfg(not(windows))]
use none as os;
#[cfg(windows)]
use win32 as os;

pub use os::Os as Desktop;
pub use os::reduce_motion;
pub use os::set_window_shown;

/// The Dock icon's badge text for `count` sessions that need the user: `None` clears it, `99+`
/// from 100 (a Dock badge is a few characters wide).
pub fn badge_label(count: usize) -> Option<String> {
    match count {
        0 => None,
        1..=99 => Some(count.to_string()),
        _ => Some("99+".to_owned()),
    }
}

/// Whether the window's ✕ may hide it instead of quitting: something must bring it back, the
/// tray icon (Windows, `tray_shown`) or the Dock icon (macOS, `on_reopen`).
pub fn can_hide_on_close(os: Os, tray_shown: bool) -> bool {
    os == Os::Mac || tray_shown
}

/// Puts `count` on the Dock icon (macOS); nothing elsewhere.
#[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
pub fn set_dock_badge(count: usize) {
    #[cfg(target_os = "macos")]
    dock::set_badge(badge_label(count).as_deref());
}

/// Bounces the Dock icon once (macOS; gpui skips the bounce when `window` itself is the active
/// window); Windows flashes the taskbar through [`Desktop::flash`], Linux does nothing.
pub fn request_attention(window: &gpui::Window) {
    if cfg!(target_os = "macos") {
        window.request_attention();
    }
}

/// The standard About panel (macOS); nothing elsewhere.
pub fn show_about_panel() {
    #[cfg(target_os = "macos")]
    dock::show_about_panel();
}

/// What a click in the tray asks of the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// Show the window.
    Open,
    /// Show the window on this session's pane.
    Session(Ulid),
    /// Show the window on this session's pane, else its workspace: a toast's `mkrew://` link
    /// (ADR 0079).
    Notification { session: Ulid, project: Ulid },
    /// Open or raise the window of the machine at this place of the tray menu's machine list.
    Machine(u8),
    /// Quit the UI (the daemon and the agents keep running).
    Quit,
    /// Explorer restarted and made the taskbar buttons again: set the badge again.
    TaskbarRestarted,
}

/// FNV-1a of the lower-cased exe path and the instance id (`launch::instance_id`: the daemon
/// socket for this computer's window, `machine:<name>` for a machine's). Two launches of the
/// same exe with the same id share it; a dev build at another path, or the same exe on another
/// `MKREW_SOCKET`, does not (the second launch only forwards to its own kind).
pub fn instance_key(exe: &str, id: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    // NUL cannot occur in a path: "a" + "bc" never collides with "ab" + "c".
    let (exe, id) = (exe.to_lowercase(), id.to_lowercase());
    for byte in exe.bytes().chain([0]).chain(id.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// [`instance_key`] of the running exe and the instance `id`.
pub(crate) fn current_key(id: &str) -> String {
    std::env::current_exe()
        .map(|exe| instance_key(&exe.to_string_lossy(), id))
        .unwrap_or_default()
}

/// Asks another running Mkrew of the same exe and instance `id` to show its window, on `jump`'s
/// session when a toast link started this launch. `true`: it was asked, this launch should end.
pub fn raise_running_instance(id: &str, jump: Option<(Ulid, Ulid)>) -> bool {
    let raised = os::raise(&current_key(id), jump);
    if raised {
        tracing::info!("Mkrew is already running as {id}: asked it to show its window, exiting");
    }
    raised
}

/// Whether a Mkrew of the same exe runs instance `id` (Windows; `false` elsewhere): the check of
/// [`raise_running_instance`] without asking that window for anything.
pub fn instance_running(id: &str) -> bool {
    os::running(&current_key(id))
}

/// A menu line with `&` doubled: Win32 menus read a lone `&` as an accelerator mark.
pub fn escape_menu(text: &str) -> String {
    text.replace('&', "&&")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Os;

    #[test]
    fn only_a_mac_may_hide_on_close_without_a_tray() {
        assert!(
            can_hide_on_close(Os::Mac, false),
            "the Dock icon brings it back"
        );
        assert!(
            !can_hide_on_close(Os::Windows, false),
            "no tray icon: never hide into nothing"
        );
        assert!(!can_hide_on_close(Os::Linux, false));
        assert!(can_hide_on_close(Os::Windows, true));
    }

    #[test]
    fn the_dock_badge_is_the_count_and_clears_at_zero() {
        assert_eq!(badge_label(0), None);
        assert_eq!(badge_label(1).as_deref(), Some("1"));
        assert_eq!(badge_label(99).as_deref(), Some("99"));
        assert_eq!(badge_label(100).as_deref(), Some("99+"));
        assert_eq!(badge_label(5000).as_deref(), Some("99+"));
    }

    #[test]
    fn the_instance_key_is_per_exe_path_and_ignores_case() {
        let pipe = r"\\.\pipe\mkrew-jane";
        let installed = instance_key(r"C:\Program Files\Mkrew\mkrew.exe", pipe);
        assert_eq!(
            installed,
            instance_key(r"c:\program files\mkrew\MKREW.EXE", pipe)
        );
        assert_ne!(
            installed,
            instance_key(r"C:\src\mkrew\target\debug\mkrew.exe", pipe)
        );
        assert_ne!(
            installed,
            instance_key(
                r"C:\Program Files\Mkrew\mkrew.exe",
                r"\\.\pipe\mkrew-sandbox"
            ),
            "same exe, another daemon"
        );
        assert_eq!(installed.len(), 16);
    }

    #[test]
    fn an_ampersand_in_a_menu_line_is_not_an_accelerator() {
        assert_eq!(
            escape_menu("claude · R&D — done, 3m"),
            "claude · R&&D — done, 3m"
        );
        assert_eq!(escape_menu("plain"), "plain");
    }
}
