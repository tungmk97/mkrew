//! The macOS menu bar and its menu-only keys (SPEC §20L, ADR 0100). Pure: the shell registers
//! the handlers (`shell::app_menu`) and hands the result to `App::set_menus`.
//!
//! Settings, Copy, Paste and Enter Full Screen are the keymap's own `Run(Command)` actions, so
//! the menu shows the user's keys; the actions below exist only for the menu.

use gpui::{KeyBinding, Menu, MenuItem, OsAction, SystemMenuType, actions};

use crate::keymap::{Command, Os, Run};

actions!(
    mkrew,
    [
        /// The standard About panel.
        About,
        /// The palette's "Check for updates", from the menu.
        CheckForUpdates,
        /// Quit UI (asks about unsaved Files changes first).
        Quit,
        /// Hide the app (Cmd+H).
        Hide,
        /// Hide the other apps.
        HideOthers,
        /// Show the other apps again.
        ShowAll,
        /// Minimize the active window.
        Minimize,
        /// Zoom (maximize) the active window.
        Zoom,
    ]
);

/// The menu bar of `os`: empty off macOS. `local` = this computer's window; a machine window
/// has no update check (the update belongs to this computer's Mkrew).
pub fn menus(os: Os, local: bool) -> Vec<Menu> {
    if os != Os::Mac {
        return Vec::new();
    }
    let mut app = vec![
        MenuItem::action("About Mkrew", About),
        MenuItem::separator(),
        MenuItem::action("Settings…", Run(Command::OpenSettings)),
    ];
    if local {
        app.push(MenuItem::action("Check for Updates…", CheckForUpdates));
    }
    app.extend([
        MenuItem::separator(),
        MenuItem::os_submenu("Services", SystemMenuType::Services),
        MenuItem::separator(),
        MenuItem::action("Hide Mkrew", Hide),
        MenuItem::action("Hide Others", HideOthers),
        MenuItem::action("Show All", ShowAll),
        MenuItem::separator(),
        MenuItem::action("Quit Mkrew", Quit),
    ]);
    vec![
        Menu::new("Mkrew").items(app),
        Menu::new("Edit").items([
            MenuItem::os_action("Copy", Run(Command::Copy), OsAction::Copy),
            MenuItem::os_action("Paste", Run(Command::Paste), OsAction::Paste),
        ]),
        Menu::new("View").items([MenuItem::action(
            "Enter Full Screen",
            Run(Command::ToggleFullScreen),
        )]),
        Menu::new("Window").items([
            MenuItem::action("Minimize", Minimize),
            MenuItem::action("Zoom", Zoom),
        ]),
    ]
}

/// The keys only the menu has: Cmd+Q, Cmd+H, Option+Cmd+H, Cmd+M. Empty off macOS.
pub fn bindings(os: Os) -> Vec<KeyBinding> {
    if os != Os::Mac {
        return Vec::new();
    }
    vec![
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
        KeyBinding::new("cmd-m", Minimize, None),
    ]
}

/// `keymap_bindings` after the menu-only keys: gpui lets the later binding of a key win, so a
/// key the user bound in `keymap.toml` beats the menu's.
pub fn with_menu_keys(os: Os, keymap_bindings: Vec<KeyBinding>) -> Vec<KeyBinding> {
    let mut all = bindings(os);
    all.extend(keymap_bindings);
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The item names of `menu`: "-" is a separator, "> x" a submenu.
    fn names(menu: &Menu) -> Vec<String> {
        menu.items
            .iter()
            .map(|item| match item {
                MenuItem::Separator => "-".to_owned(),
                MenuItem::Submenu(menu) => format!("> {}", menu.name),
                MenuItem::SystemMenu(os) => format!("> {}", os.name),
                MenuItem::Action { name, .. } => name.to_string(),
            })
            .collect()
    }

    /// The `Run(Command)` items of `menu`: the ones that reuse the keymap.
    fn runs(menu: &Menu) -> Vec<Run> {
        menu.items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action { action, .. } => action.as_any().downcast_ref::<Run>().cloned(),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn only_a_mac_has_a_menu_bar_and_its_keys() {
        for os in [Os::Windows, Os::Linux] {
            assert!(menus(os, true).is_empty());
            assert!(bindings(os).is_empty());
        }
    }

    #[test]
    fn the_mac_menu_bar_has_the_standard_menus_in_order() {
        let bar = menus(Os::Mac, true);
        let titles: Vec<String> = bar.iter().map(|menu| menu.name.to_string()).collect();
        assert_eq!(titles, ["Mkrew", "Edit", "View", "Window"]);
        assert_eq!(
            names(&bar[0]),
            [
                "About Mkrew",
                "-",
                "Settings…",
                "Check for Updates…",
                "-",
                "> Services",
                "-",
                "Hide Mkrew",
                "Hide Others",
                "Show All",
                "-",
                "Quit Mkrew"
            ]
        );
        assert_eq!(names(&bar[1]), ["Copy", "Paste"]);
        assert_eq!(names(&bar[2]), ["Enter Full Screen"]);
        assert_eq!(names(&bar[3]), ["Minimize", "Zoom"]);
    }

    #[test]
    fn a_machine_window_has_no_update_check() {
        let bar = menus(Os::Mac, false);
        assert!(!names(&bar[0]).contains(&"Check for Updates…".to_owned()));
        assert!(names(&bar[0]).contains(&"Quit Mkrew".to_owned()));
    }

    #[test]
    fn settings_copy_paste_and_full_screen_are_the_keymaps_own_commands() {
        let bar = menus(Os::Mac, true);
        assert_eq!(runs(&bar[0]), [Run(Command::OpenSettings)]);
        assert_eq!(runs(&bar[1]), [Run(Command::Copy), Run(Command::Paste)]);
        assert_eq!(runs(&bar[2]), [Run(Command::ToggleFullScreen)]);
    }

    #[test]
    fn the_menu_only_keys_are_cmd_q_cmd_h_alt_cmd_h_and_cmd_m() {
        let bound = bindings(Os::Mac);
        let keys: Vec<(String, bool, bool)> = bound
            .iter()
            .map(|binding| {
                let key = binding.keystrokes()[0].inner();
                (key.key.clone(), key.modifiers.platform, key.modifiers.alt)
            })
            .collect();
        assert_eq!(
            keys,
            [
                ("q".to_owned(), true, false),
                ("h".to_owned(), true, false),
                ("h".to_owned(), true, true),
                ("m".to_owned(), true, false)
            ]
        );
        assert!(bound[0].action().as_any().is::<Quit>());
        assert!(bound[1].action().as_any().is::<Hide>());
        assert!(bound[2].action().as_any().is::<HideOthers>());
        assert!(bound[3].action().as_any().is::<Minimize>());
    }

    #[test]
    fn the_users_keys_come_after_the_menu_keys_so_they_win() {
        let user = vec![KeyBinding::new("cmd-q", Run(Command::OpenSettings), None)];
        let all = with_menu_keys(Os::Mac, user);
        assert_eq!(all.len(), 5);
        assert!(all[0].action().as_any().is::<Quit>());
        assert!(
            all[4].action().as_any().is::<Run>(),
            "the later binding of a key wins in gpui"
        );
        assert_eq!(
            with_menu_keys(
                Os::Windows,
                vec![KeyBinding::new("ctrl-q", Run(Command::OpenSettings), None)]
            )
            .len(),
            1
        );
    }
}
