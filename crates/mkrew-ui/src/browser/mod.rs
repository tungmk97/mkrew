//! Browser panes (spec 2026-11-24, ADR 0111): what the Shell keeps per page, what a page
//! reports, and whether this machine can show pages at all. Pure, no gpui, except
//! `native` (Windows and macOS), which wraps the `wry` view.

pub mod address;
pub mod keys;

#[cfg(any(windows, target_os = "macos"))]
mod native;
#[cfg(not(any(windows, target_os = "macos")))]
#[path = "no_native.rs"]
mod native;
pub use native::{NativeView, WebSurface};

use std::fmt;
use std::sync::LazyLock;

/// The env var that makes GPUI draw without DirectComposition (Windows: a DComp visual would
/// cover the page's child window).
pub const DISABLE_DCOMP_ENV: &str = "GPUI_DISABLE_DIRECT_COMPOSITION";
/// Mkrew set [`DISABLE_DCOMP_ENV`] itself (the user had not): children must not inherit it.
pub static DCOMP_SET_BY_MKREW: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Takes the variable Mkrew set for itself out of a child's environment (the daemon, Zed);
/// a value the user set is kept.
pub fn strip_own_env(command: &mut std::process::Command) -> &mut std::process::Command {
    if DCOMP_SET_BY_MKREW.load(std::sync::atomic::Ordering::Relaxed) {
        command.env_remove(DISABLE_DCOMP_ENV);
    }
    command
}

/// The page a browser pane shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Page {
    /// The address loaded or loading; `""` before the first one.
    pub url: String,
    pub title: String,
    pub loading: bool,
}

impl Page {
    pub fn new(url: impl Into<String>) -> Page {
        Page {
            url: url.into(),
            ..Page::default()
        }
    }

    /// What names the pane: its title, else its URL, else "New browser".
    pub fn label(&self) -> &str {
        [self.title.as_str(), self.url.as_str()]
            .into_iter()
            .find(|text| !text.trim().is_empty())
            .unwrap_or("New browser")
    }
}

/// What a page tells the Shell (wry callbacks and the init script's IPC messages).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageEvent {
    Started,
    /// Loaded; the page's URL.
    Finished(String),
    Title(String),
    /// The page asked for a new window (`target=_blank`, `window.open`): load it here.
    Open(String),
    /// The page took the keyboard or a click.
    Focus,
    /// A Mkrew key combo pressed in the page (`key_name` form).
    Key(String),
}

/// An IPC message of the init script: `{"t":"focus"}` or `{"t":"key","k":"alt-shift-b"}`.
pub fn parse_ipc(body: &str) -> Option<PageEvent> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    match value.get("t")?.as_str()? {
        "focus" => Some(PageEvent::Focus),
        "key" => Some(PageEvent::Key(value.get("k")?.as_str()?.to_owned())),
        _ => None,
    }
}

/// Why pages open in the system browser instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unavailable {
    Linux,
    NoRuntime,
    Failed(String),
}

impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unavailable::Linux => f.write_str("Linux opens web pages in your browser"),
            Unavailable::NoRuntime => {
                f.write_str("The Microsoft Edge WebView2 Runtime is not installed")
            }
            Unavailable::Failed(error) => f.write_str(error),
        }
    }
}

/// Whether this machine shows pages in Mkrew; checked once per run.
pub fn available() -> Result<(), Unavailable> {
    static CHECK: LazyLock<Result<(), Unavailable>> = LazyLock::new(check);
    CHECK.clone()
}

#[cfg(windows)]
fn check() -> Result<(), Unavailable> {
    wry::webview_version()
        .map(drop)
        .map_err(|_| Unavailable::NoRuntime)
}

#[cfg(target_os = "macos")]
fn check() -> Result<(), Unavailable> {
    Ok(())
}

#[cfg(not(any(windows, target_os = "macos")))]
fn check() -> Result<(), Unavailable> {
    Err(Unavailable::Linux)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_named_by_its_title_then_its_url() {
        let mut page = Page::new("");
        assert_eq!(page.label(), "New browser");
        page.url = "https://x.dev/".into();
        assert_eq!(page.label(), "https://x.dev/");
        page.title = "  ".into();
        assert_eq!(
            page.label(),
            "https://x.dev/",
            "a blank title does not count"
        );
        page.title = "X".into();
        assert_eq!(page.label(), "X");
    }

    #[test]
    fn ipc_messages_parse_and_noise_is_ignored() {
        assert_eq!(parse_ipc(r#"{"t":"focus"}"#), Some(PageEvent::Focus));
        assert_eq!(
            parse_ipc(r#"{"t":"key","k":"alt-shift-b"}"#),
            Some(PageEvent::Key("alt-shift-b".into()))
        );
        for noise in [
            "",
            "{",
            r#"{"t":"key"}"#,
            r#"{"t":3}"#,
            r#"{"t":"other"}"#,
            "[]",
        ] {
            assert_eq!(parse_ipc(noise), None, "{noise}");
        }
    }

    #[test]
    fn the_reasons_read_as_the_spec_says() {
        assert_eq!(
            Unavailable::Linux.to_string(),
            "Linux opens web pages in your browser"
        );
        assert_eq!(
            Unavailable::NoRuntime.to_string(),
            "The Microsoft Edge WebView2 Runtime is not installed"
        );
    }
}
