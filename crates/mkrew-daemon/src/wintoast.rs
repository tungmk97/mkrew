//! Windows toasts (ADR 0078): shown through WinRT with `activationType="protocol"`, so a click
//! on the banner, or later in the Notification Center, opens the session's `mkrew://` link
//! even after this process forgot the toast.

use mkrew_core::link::session_link;
use windows::Data::Xml::Dom::XmlDocument;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
use windows::core::HSTRING;

use std::io;
use std::path::{Path, PathBuf};

use crate::notify::Notice;

/// PowerShell's AUMID: an unpackaged app may show toasts under it without a Start Menu
/// shortcut of its own (the toast header reads "Windows PowerShell").
const POWERSHELL_APP_ID: &str =
    "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe";
/// Where `mkrew://` is registered for the user.
const CLASSES_KEY: &str = r"Software\Classes\mkrew";

/// The toast for `notice`: title, body, the OS default sound or silence, and the link a click
/// opens.
pub(crate) fn toast_xml(notice: &Notice) -> String {
    let audio = if notice.sound {
        r#"<audio src="ms-winsoundevent:Notification.Default"/>"#
    } else {
        r#"<audio silent="true"/>"#
    };
    format!(
        r#"<toast activationType="protocol" launch="{}"><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual>{audio}</toast>"#,
        escape(&session_link(notice.session_id, notice.project_id)),
        escape(&notice.title),
        escape(&notice.body),
    )
}

/// Shows `notice`; returns once Windows has it (nothing waits for a click).
pub(crate) fn show(notice: &Notice) -> windows::core::Result<()> {
    let doc = XmlDocument::new()?;
    doc.LoadXml(&HSTRING::from(toast_xml(notice)))?;
    let toast = ToastNotification::CreateToastNotification(&doc)?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(POWERSHELL_APP_ID))?
        .Show(&toast)
}

/// XML text / attribute escaping.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// At daemon start (OS notifier on): points `mkrew://` at the UI next to this daemon.
pub(crate) fn register_link_handler() {
    let socket_override = std::env::var_os(mkrew_core::paths::ENV_SOCKET).is_some();
    let Some(ui) = std::env::current_exe()
        .ok()
        .and_then(|exe| link_target(&exe, socket_override))
    else {
        return;
    };
    match register(CLASSES_KEY, &ui) {
        Ok(()) => tracing::info!(ui = %ui.display(), "mkrew:// links open the UI"),
        Err(e) => tracing::warn!("registering mkrew:// failed, toast clicks do nothing: {e}"),
    }
}

/// `mkrew.exe` next to `daemon_exe` when this daemon owns the link: the user's own daemon on
/// the default pipe (tests and dev daemons on their own `MKREW_SOCKET` never take it), with
/// the UI beside it.
fn link_target(daemon_exe: &Path, socket_override: bool) -> Option<PathBuf> {
    if socket_override {
        return None;
    }
    let ui = daemon_exe.parent()?.join("mkrew.exe");
    ui.is_file().then_some(ui)
}

/// The handler command line: both parts quoted (install paths may hold spaces).
fn open_command(ui: &Path) -> String {
    format!("\"{}\" \"%1\"", ui.display())
}

/// Writes the `mkrew://` handler under `HKCU\<key>`.
fn register(key: &str, ui: &Path) -> io::Result<()> {
    set(key, None, "URL:Mkrew")?;
    set(key, Some("URL Protocol"), "")?;
    set(
        &format!(r"{key}\shell\open\command"),
        None,
        &open_command(ui),
    )
}

/// One `REG_SZ` value under `HKCU\<key>` (created if missing); `None` = the key's default.
#[allow(unsafe_code)]
fn set(key: &str, name: Option<&str>, value: &str) -> io::Result<()> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegSetKeyValueW};
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(std::iter::once(0)).collect() };
    let (key, name, value) = (wide(key), name.map(wide), wide(value));
    let bytes = u32::try_from(value.len() * 2).map_err(io::Error::other)?;
    // SAFETY: every pointer is a NUL-terminated UTF-16 buffer alive for the call; `bytes` is
    // `value`'s size including its NUL.
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ref().map_or(std::ptr::null(), |n| n.as_ptr()),
            REG_SZ,
            value.as_ptr().cast(),
            bytes,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(status as i32))
    }
}

#[cfg(test)]
mod tests {
    use ulid::Ulid;

    use super::*;

    fn notice(title: &str, sound: bool) -> Notice {
        Notice {
            session_id: Ulid::new(),
            project_id: Ulid::new(),
            title: title.into(),
            body: "Needs input".into(),
            sound,
        }
    }

    #[test]
    fn a_click_opens_the_session_link() {
        let notice = notice("repo · claude", true);
        let xml = toast_xml(&notice);
        let launch = format!(
            r#"activationType="protocol" launch="{}""#,
            session_link(notice.session_id, notice.project_id)
        );
        assert!(xml.contains(&launch), "{xml}");
        assert!(
            xml.contains("<text>repo · claude</text><text>Needs input</text>"),
            "{xml}"
        );
    }

    #[test]
    fn sound_follows_the_notice() {
        assert!(
            toast_xml(&notice("a", true))
                .contains(r#"<audio src="ms-winsoundevent:Notification.Default"/>"#)
        );
        assert!(toast_xml(&notice("a", false)).contains(r#"<audio silent="true"/>"#));
    }

    /// A name may hold any character; Windows must still parse the toast.
    #[test]
    fn markup_in_names_is_escaped_and_parses() {
        let xml = toast_xml(&notice(r#"a & <b> "c" 'd'"#, false));
        assert!(
            xml.contains("<text>a &amp; &lt;b&gt; &quot;c&quot; &apos;d&apos;</text>"),
            "{xml}"
        );
        let doc = XmlDocument::new().unwrap();
        doc.LoadXml(&HSTRING::from(xml)).unwrap();
    }

    #[test]
    fn only_the_default_pipe_daemon_with_the_ui_beside_it_owns_the_link() {
        let dir = tempfile::tempdir().unwrap();
        let daemon = dir.path().join("mkrew-daemon.exe");
        assert_eq!(link_target(&daemon, false), None, "no mkrew.exe yet");
        let ui = dir.path().join("mkrew.exe");
        std::fs::write(&ui, b"").unwrap();
        assert_eq!(link_target(&daemon, false), Some(ui));
        assert_eq!(link_target(&daemon, true), None, "own MKREW_SOCKET");
    }

    #[test]
    fn the_command_quotes_the_exe_and_the_link() {
        assert_eq!(
            open_command(Path::new(r"C:\Program Files\Mkrew\bin\mkrew.exe")),
            r#""C:\Program Files\Mkrew\bin\mkrew.exe" "%1""#
        );
    }

    /// Writes only under a throwaway `HKCU\Software\MkrewTest-<pid>`, deleted before asserting.
    #[test]
    fn the_handler_is_written_under_the_key() {
        let key = format!(r"Software\MkrewTest-{}", std::process::id());
        let ui = Path::new(r"C:\Program Files\Mkrew\bin\mkrew.exe");
        let written = register(&key, ui);
        let read = (
            get(&key, None),
            get(&key, Some("URL Protocol")),
            get(&format!(r"{key}\shell\open\command"), None),
        );
        delete_tree(&key);
        written.unwrap();
        assert_eq!(
            read,
            (
                Some("URL:Mkrew".into()),
                Some(String::new()),
                Some(open_command(ui))
            )
        );
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    #[allow(unsafe_code)]
    fn get(key: &str, name: Option<&str>) -> Option<String> {
        use windows_sys::Win32::System::Registry::{
            HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW,
        };
        let (key, name) = (wide(key), name.map(wide));
        let mut buf = [0u16; 512];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: NUL-terminated buffers alive for the call; `len` is `buf`'s size in bytes.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ref().map_or(std::ptr::null(), |n| n.as_ptr()),
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut len,
            )
        };
        (status == 0)
            .then(|| String::from_utf16_lossy(&buf[..(len as usize / 2).saturating_sub(1)]))
    }

    #[allow(unsafe_code)]
    fn delete_tree(key: &str) {
        use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RegDeleteTreeW};
        let key = wide(key);
        // SAFETY: NUL-terminated buffer alive for the call.
        unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, key.as_ptr()) };
    }
}
