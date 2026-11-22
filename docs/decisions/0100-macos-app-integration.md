# 0100 - macOS app integration

- Date: 2026-11-22
- Status: accepted (SPEC §4, §8, §10, §20J, §20K, new §20L; extends ADR 0064 and ADR 0071; Plans A and B of the same audit are ADR 0098 and ADR 0099)
- Milestone: none (user request 2026-11-22, macOS audit)

## Context

The audit of the macOS build (`MacWindowChrome`, 2026-11-21) found that the app has no menu bar, quits from the Dock without asking, does not get the user's `PATH` when opened from Finder, forgets its window place, bounces and badges nothing, and ships without an icon, a Developer ID signature or notarization.
Facts checked in `gpui-pre 0.3.7`:

- `cx.set_menus` exists and is never called; its menu items dispatch GPUI actions, so the keymap's own `Run(Command)` actions can be reused.
- `cx.on_app_quit` cannot cancel a quit and gets 200 ms; macOS reaches it from `applicationWillTerminate`, so Dock Quit and log-out cannot be vetoed from Rust without adding `applicationShouldTerminate:` to gpui's private delegate (an `unsafe` runtime call the workspace denies).
- `cx.on_reopen`, `cx.hide`, `Window::request_attention` and `Window::set_traffic_light_position` exist; there is no Dock badge call.
- `objc2-app-kit` 0.3.2 and `objc2-foundation` 0.3.2 are already in `Cargo.lock` (gpui-pre-macos depends on them) and offer safe wrappers for `NSApp.dockTile.badgeLabel` and the standard About panel.
- A Finder or Dock launch gets launchd's `PATH=/usr/bin:/bin:/usr/sbin:/sbin`.

## Decision

1. **Menu bar** through `cx.set_menus`: Mkrew (About, Settings, Check for Updates, Services, Hide, Hide Others, Show All, Quit), Edit (Copy, Paste), View (Enter Full Screen), Window (Minimize, Zoom).
   Settings, Copy, Paste and Full Screen reuse `Run(Command)`, so the menu shows the user's keys.
   New menu-only actions: `Quit` (cmd-q), `Hide` (cmd-h), `HideOthers` (alt-cmd-h), `ShowAll`, `Minimize` (cmd-m), `Zoom`, `About`, `CheckForUpdates`.
   The menu-only keys are bound before the keymap's, so a key the user binds wins.
   Edit has Copy and Paste only: the Files editor and the text fields take key events, not actions, so a menu click cannot reach them (the keys work).
2. **Quit.** Cmd+Q and the Quit item run `quit_ui` (unsaved Files changes asked first).
   Dock Quit and log-out cannot be vetoed (above): `on_app_quit` saves the window places and asks the machine windows to quit; unsaved Files changes are lost on those two paths.
   This is a known gap, listed in SPEC §20L; closing it needs gpui to expose `applicationShouldTerminate:`.
3. **Closing the main window: keep the app alive.**
   Options: (a) keep quit-on-close and document it; (b) really close the window and rebuild it in `on_reopen`; (c) hide the app (`cx.hide()`) and bring it back from the Dock.
   Chosen: (c), on the existing `[ui] close_to_tray` setting (default on; machine windows excluded as today).
   Why: an agent that needs input is only noticed while the process lives, and the Dock badge and bounce need it; Windows already hides to the tray by default for the same reason, so (c) is the Mac form of the same rule; (c) keeps all state (unsaved Files edits, scrollback, detached windows) while (b) would drop `Shell`, which the detached windows hold, and rebuilding it is a large refactor with no way to test it on the dev machine; the daemon keeps the agents running either way, so (a) would only lose the attention signals.
   Cmd+Q quits; `close_to_tray = false` makes the red button quit.
   `on_reopen` (Dock click with no visible window) shows the main window.
4. **Window place.** The main window's restore rect, maximized and full screen state are saved in `<data>/main-window.json` (a machine window: `main-window-<key>.json`) and restored on every OS; saved places on a gone monitor move to the primary display; the size is at least the minimum.
   A detached window now keeps full screen too (`fullscreen` in `windows.json`, absent = false).
   New and restored windows are fitted to `visible_bounds()` so they never open under the menu bar or Dock.
5. **Traffic lights** are placed with `Window::set_traffic_light_position` from the title bar's height at the current `[ui] font_size` (y = (bar - 14) / 2), only when it changes; the 72 px left padding is dropped in full screen.
6. **Dock.** Badge: `NSApp.dockTile.badgeLabel` through safe `objc2` calls, the same count as the Windows overlay, `99+` from 100.
   Bounce: `Window::request_attention` (one bounce) where Windows flashes the taskbar, under the same `[notify] flash_taskbar` setting.
7. **PATH.** The first lookup asks `$SHELL -l -i -c` (5 s limit, `/bin/zsh` if `$SHELL` is unset) for its `PATH` between markers and merges it before the process's own; the result is cached in a `OnceLock` in `mkrew_core::login_path` and warmed on a thread at start of the UI and of the daemon.
   This is the only login-shell probe: it needs no `unsafe` (no `set_var`), and programs the UI starts by name are found through `mkrew_core::lookup::find` (`tailscale` included).
   `lookup::find` and the PTY command use it; the process environment is not changed.
   The default shell starts with `-l` on macOS (Terminal.app does too).
8. **Zed on a Mac** is also looked for in `/usr/local/bin`, `/opt/homebrew/bin`, `~/.local/bin` and inside `Zed.app`; with only the app installed, `open -a <Zed.app> <path>` opens the file (without a line).
9. **`mk` command.** Palette "Global: Install mk command" (only in a `Mkrew.app`) links `/usr/local/bin/mk`, else `~/.local/bin/mk`, to the bundle's `mk`.
   A translocated copy links its real bundle's `mk` (the same mapping as the update, §20K) and is refused only when that mapping failed.
   A link already at that `mk` stays; a link to anything but a bundle's `Contents/MacOS/mk`, or a real file named `mk`, is never replaced.
10. **Packaging.** `Mkrew.icns` from `packaging/macos/mkrew-1024.png` (the lanes tile, `icon/mkrew-1-lanes/tile-dark.svg`) with `sips` and `iconutil`; `CFBundleIconFile`; Developer ID signing with the hardened runtime and the audio-input entitlement, `notarytool` and `stapler`, all gated on repository secrets, else the ad-hoc signature as before.
    The bundle's `agents/` copy leaves out `cmd.toml`, `powershell.toml` and `git-bash.toml`.
11. **Not built.**
    Single instance on macOS: LaunchServices already reuses a running bundle for Finder, Dock and `open`; a second launch of the binary from a terminal shares the daemon and is harmless (as on Linux); the Windows-only parts (toast links, launcher raise) have no macOS counterpart.
    A Dock menu (`set_dock_menu`): not requested.
    `mkrew --machine` children keep their own Dock icon: each machine window is its own process (ADR 0089), and making it an accessory app would remove its menu bar and Cmd+Tab entry.
    Windows-only profiles stay compiled into the daemon and seeded into the config folder (the New menu already hides profiles whose program is not installed); only the bundle's reference copy is filtered.

## Consequences

- SPEC: §20L added; §4, §8, §10, §20J, §20K, the Zed bullet and two settings comments amended.
- New dependencies for the macOS target only: `objc2`, `objc2-app-kit` and `objc2-foundation` (versions already in the tree).
- The repository needs six secrets to sign and notarize (listed in SPEC §20L item 9); without them releases stay ad-hoc signed.
- Risk: several behaviors cannot be run on the dev machine; each has a line in `docs/HUMAN_CHECKS.md` "To verify".
