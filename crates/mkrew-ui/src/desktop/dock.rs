//! The Dock tile and the standard About panel (macOS, SPEC §20L, ADR 0100), through objc2's safe
//! wrappers. Main thread only: gpui runs the UI and its actions there, and `MainThreadMarker`
//! refuses anything else.

use objc2_app_kit::NSApplication;
// objc2-foundation 0.3.2 re-exports it as deprecated (`objc2::MainThreadMarker` moved); importing
// from `objc2` would add a third dependency line, so allow it here.
#[allow(deprecated)]
use objc2_foundation::MainThreadMarker;
use objc2_foundation::NSString;

/// Sets the Dock icon's badge to `label`, or clears it.
pub fn set_badge(label: Option<&str>) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let label = label.map(NSString::from_str);
    NSApplication::sharedApplication(mtm)
        .dockTile()
        .setBadgeLabel(label.as_deref());
}

/// The standard About panel: name, version and icon come from the bundle's Info.plist.
pub fn show_about_panel() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    NSApplication::sharedApplication(mtm).orderFrontStandardAboutPanel(None);
}
