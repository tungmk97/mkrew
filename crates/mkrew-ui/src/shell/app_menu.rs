//! The macOS menu bar's handlers, and the one place that binds keys and sets the menus
//! (SPEC §20L, ADR 0100). The menu items themselves are `mkrew_ui::app_menu`.

use mkrew_ui::app_menu::{
    self, About, CheckForUpdates, Hide, HideOthers, Minimize, Quit, ShowAll, Zoom,
};

use crate::*;

/// The one `Shell`: the actions that work on it whatever window has the keyboard run in its
/// primary Space window (ADR 0115).
pub(crate) struct ShellHandle(pub Entity<Shell>);

impl gpui::Global for ShellHandle {}

/// Whether this process is this computer's window (a machine window has no update check).
pub(crate) struct LocalWindow(pub bool);

impl gpui::Global for LocalWindow {}

/// Binds `keymap` (after the menu-only keys, so the user's keys win) and sets the menu bar,
/// whose shortcut text is read from the bindings. Runs at start, after a keyboard layout
/// change and after the Settings dialog changed the keymap (macOS only; elsewhere the menu is
/// empty and only the keymap is bound).
pub(crate) fn bind_all(keymap: &Keymap, cx: &mut App) {
    let local = cx.try_global::<LocalWindow>().is_none_or(|local| local.0);
    let bindings = keymap.key_bindings(cx.keyboard_mapper().as_ref());
    cx.clear_key_bindings();
    cx.bind_keys(app_menu::with_menu_keys(Os::CURRENT, bindings));
    let menus = app_menu::menus(Os::CURRENT, local);
    if !menus.is_empty() {
        cx.set_menus(menus);
    }
}

/// Runs `f` on the shell in its primary Space window (nothing when none is left). Deferred: an
/// action listener runs while gpui has the dispatching window taken out of the app, so
/// updating that window right away fails silently.
fn with_primary(
    cx: &mut App,
    f: impl FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>) + 'static,
) {
    cx.defer(move |cx| in_primary(cx, f));
}

/// [`with_primary`] now (callers that run outside a dispatch).
fn in_primary(cx: &mut App, f: impl FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>)) {
    let Some(shell) = cx
        .try_global::<ShellHandle>()
        .map(|handle| handle.0.clone())
    else {
        return;
    };
    let primary = shell.read(cx);
    let Some(handle) = (primary.primary()).and_then(|key| primary.space_handles.get(&key).copied())
    else {
        return;
    };
    let _ = handle.update(cx, |_, window, cx| {
        shell.update(cx, |shell, cx| f(shell, window, cx));
    });
}

/// Runs `f` on the window that is active now. Deferred for the reason of [`with_primary`]; the
/// active window is read after the dispatch.
fn with_active(cx: &mut App, f: impl FnOnce(&mut Window) + 'static) {
    cx.defer(move |cx| {
        if let Some(active) = cx.active_window() {
            let _ = active.update(cx, |_, window, _| f(window));
        }
    });
}

/// The handlers of the menu-only actions, and what the app does when it is quit without a
/// question.
pub(crate) fn register(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| with_primary(cx, |shell, window, cx| shell.quit_ui(window, cx)));
    cx.on_action(|_: &CheckForUpdates, cx| {
        with_primary(cx, |shell, window, cx| {
            shell.check_updates(true, window, cx)
        });
    });
    cx.on_action(|_: &About, _| mkrew_ui::desktop::show_about_panel());
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.on_action(|_: &Minimize, cx| with_active(cx, |window| window.minimize_window()));
    cx.on_action(|_: &Zoom, cx| with_active(cx, |window| window.zoom_window()));
    // Dock → Quit and log-out end the app without asking (gpui 0.3.7 cannot veto them, its
    // quit hook runs after the decision and gets 200 ms): keep what can be kept.
    // Not deferred: the app is already quitting, a deferred job would never run. No window is
    // being dispatched here, so the direct update works.
    cx.on_app_quit(|cx| {
        if let Some(shell) = cx
            .try_global::<ShellHandle>()
            .map(|handle| handle.0.clone())
        {
            shell.update(cx, |shell, cx| shell.save_before_exit(cx));
        }
        std::future::ready(())
    })
    .detach();
}

/// A click on the Dock icon while no window shows (macOS `applicationShouldHandleReopen`): the
/// Space windows come back, as from the tray on Windows. Not deferred: gpui runs this callback
/// outside `App::update`, so a deferred job would wait for some later update, and no window is
/// being dispatched, so the direct update works.
pub(crate) fn reopen(cx: &mut App) {
    cx.activate(true);
    in_primary(cx, |shell, window, cx| shell.restore_window(window, cx));
}
