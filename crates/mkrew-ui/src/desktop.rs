//! The OS around the main window (SPEC §20J, ADR 0064): taskbar badge and flashing, the tray
//! icon and its menu, hiding and showing the window. Windows only; elsewhere every call does
//! nothing (gpui-pre 0.3.7 has no dock-badge, tray or hide-window call) and `has_tray()` is false,
//! so a closed window still quits.

use ulid::Ulid;

#[cfg(not(windows))]
mod none;
#[cfg(windows)]
mod win32;

#[cfg(not(windows))]
use none as os;
#[cfg(windows)]
use win32 as os;

pub use os::Os as Desktop;

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
