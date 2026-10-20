# 0064 — Tray icon, taskbar badge, close to tray and Mark unread

- Date: 2026-10-19
- Status: accepted (extends ADR 0036; SPEC §10, §11, §19, §20.0, new §20J)
- Milestone: none (user request 2026-10-19; SPEC §19 backlog line "tray icon / taskbar badge; mark unread" becomes a feature)

## Context

The user often minimizes or closes the window and misses agents that wait for input: the OS toast
fires once per session per 30 s (SPEC §8) and vanishes. "Needs you" (ADR 0036: needs input, plus
finished and not yet viewed) is only visible inside the window. "Viewed" is UI memory
(`inbox::Viewed`); the daemon and the database know nothing about it.

## Decision

- **Taskbar badge (Windows):** the main window's overlay icon (`ITaskbarList3::SetOverlayIcon`)
  shows the Needs-you count, `9+` from 10, cleared at 0. **Flash (Windows):** `FlashWindowEx` while a
  session entered `needs_input` and the window is not focused; `[notify] flash_taskbar` (default
  true) turns it off.
- **Tray icon (Windows)**, owned by the UI process on its own thread: states idle / working /
  needs you, a menu with up to 8 sessions that need you (click opens the pane), `● N working`,
  Open Mkrew, Quit UI (agents keep running).
- **Close to tray:** `[ui] close_to_tray` (default true): ✕ / Alt+F4 hide the window
  (`ShowWindow(SW_HIDE)` on the window's HWND, vetoing gpui's close) while the tray icon exists;
  the tray, an OS notification click or a second launch of the same exe on the same daemon socket shows it. Quit UI asks about unsaved
  Files changes first. The daemon keeps the agents running anyway (SPEC §2.2).
- **Mark unread:** pane menu (also the sidebar row's) and palette "Agent: Mark unread" put a
  finished agent session back into Needs you until its pane is focused again or it changes state.
  UI memory only, like "viewed". **No protocol change, no migration.**
- macOS and Linux: no badge, no tray, no flash; ✕ quits as before.

## Alternatives

- `tray-icon` + `muda` crates: not in `Cargo.lock`, two more crates (SPEC §2.6) and still a message
  loop to own. `Shell_NotifyIconW` through the `windows` crate gpui already builds with is ~150 lines.
- Tray on gpui's own Win32 loop (subclassing its window): its window procedure is private and a
  tray tied to the main window is fragile; a dedicated thread is independent.
- Hide by minimizing: the taskbar button stays, not "to tray". Close and reopen a new window: loses
  in-memory UI state and `main.rs` quits the App when the last window closes. gpui has no public
  hide, so the HWND is hidden directly; gpui already reports `WM_SHOWWINDOW` as window visibility.
- gpui `Window::request_attention`: flashes once (`FLASHW_ALL`, count 1), no stop.
- Persist viewed / unread in the daemon (migration + protocol 44): "viewed" is not persisted today;
  persisting only the unread half would be inconsistent. Rejected.
- macOS dock badge: gpui-pre 0.3.7 has no call; `objc2-app-kit` would be a new direct dependency
  that cannot be built or checked here. No-op until someone can test it on a Mac.

## Consequences

- ✕ no longer quits on Windows: tray → "Quit UI", or `close_to_tray = false`. If the tray icon
  could not be created, ✕ still quits (never hide into nothing).
- mkrew-ui gets Windows-only direct dependencies on `windows 0.62` and `raw-window-handle 0.6`
  (both already in the tree); `Cargo.lock` gains edges, no new package. `unsafe` is confined to
  `crates/mkrew-ui/src/desktop/win32.rs`.
- A hidden window detaches every pane (SPEC §10), so a tray-resident UI costs almost nothing.
- Starting `mkrew.exe` again shows the running window; a dev build at another path, or a UI on
  another daemon socket (`MKREW_SOCKET`), stays independent.
- `NotifySettings` is `deny_unknown_fields` and shared with the daemon: a daemon older than this
  change reports `flash_taskbar` as an unknown key until it is restarted.
- Explorer restarts: the tray icon is re-added on `TaskbarCreated` and the UI re-applies the
  overlay; after the window is shown again the overlay is re-applied one status tick (500 ms)
  later, once Explorer has made the new taskbar button.
