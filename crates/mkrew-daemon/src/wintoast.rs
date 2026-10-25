//! Windows toasts (ADR 0078): shown through WinRT with `activationType="protocol"`, so a click
//! on the banner, or later in the Notification Center, opens the session's `mkrew://` link
//! even after this process forgot the toast.

use mkrew_core::link::session_link;
use windows::Data::Xml::Dom::XmlDocument;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
use windows::core::HSTRING;

use crate::notify::Notice;

/// PowerShell's AUMID: an unpackaged app may show toasts under it without a Start Menu
/// shortcut of its own (the toast header reads "Windows PowerShell").
const POWERSHELL_APP_ID: &str =
    "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\WindowsPowerShell\\v1.0\\powershell.exe";

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
}
