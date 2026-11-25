# 0115 - Window modes: one window, or a window per workspace

- Date: 2026-11-25
- Status: accepted (amends ADR 0096: there is no special main window any more; SPEC §10, §13, §20.0, §20J, §20L)
- Milestone: none (human request 2026-11-25; spec `docs/superpowers/specs/2026-11-25-window-modes-design.md`)

## Context

Mkrew had one main window with a sidebar listing every workspace, plus detached windows holding torn-off tabs (ADR 0096).
The user wants a second way to work: one OS window per workspace.
The reasons are several screens (one project on each), focus (a window shows one project only) and switching projects with Alt+Tab and the taskbar.
The user also wants to switch back to one window at any time, and the one-window mode must stay exactly as it was.

## Decision

- One `Shell` entity, created once in `main()`, serves every window.
  A workspace window is a Space window (`HostId::Space(SpaceKey)`), drawn by a thin `SpaceWindow` view through `Shell::render_space`, the way `DetachedWindow` draws a detached one.
  `HostId::Main` and the `MainWindow` global are gone; `Shell::primary_host()` (the Space window used last) takes their place for Settings, Turns… and the app menu.
- What a Space window shows is kept per window in `HostUi`: its workspace, its right panel, and the scroll state of the Files, Git and Docs views.
  Git summaries are kept per workspace (`git_summaries`); the palette, dialogs and menus stay one at a time and open in the window that asked.
- `[ui] window_mode = "single" | "per_workspace"`: mode 1 is one Space window with the sidebar; mode 2 is one Space window per opened workspace, at most one per workspace, without a sidebar.
- `[ui] open_workspace = "ask" | "new_window" | "replace"`: where opening a workspace without a window goes in mode 2 (asked each time by default, with "Don't ask again").
  Every way to open a workspace or jump to a session goes through one router (`route_workspace`); a workspace that has a window is focused instead.
- `[ui] panel_side = "right" | "left"`: the Files / Session / Git panel's side, in both modes.
- Split ("Open each workspace in its own window…") picks workspaces in a picker and confirms when several open; Combine ("Combine into one window") keeps the window the user is in and closes the others.
  Changing `window_mode` in Settings or by hand in `settings.toml` runs the same flows.
- A mode 2 window shows its workspace in the OS title (`<workspace> - Mkrew`), a 3 px stripe of the workspace color on top, and a "need you" chip for sessions of the other workspaces.
- `windows.json` goes to version 2 with a `spaces` list (workspace, bounds, maximized, full screen; the one used last at the end).
  `main-window.json` is read once when `spaces` is missing.
- `Desktop` keeps the handle of every Space window: the Windows taskbar badge and flash apply to each, and hiding to the tray hides all of them.
- The last Space window to close ends the app (or hides to the tray with `[ui] close_to_tray`); closing another Space window only closes it, agents keep running.

## Rejected

- A `Shell` per window: N daemon connections, N copies of the sidebar and session state, and the tray, badge and settings watchers would each run N times.
- A UI process per workspace: N tray icons, it breaks the single-instance rule, and tabs could no longer be dragged between windows.
- Amends ADR 0096: "Move all back" and the tab menu's "Move back" (was "Move to main window"; palette "Tab: Move back to its workspace window") go to the Space window showing the tab's workspace; in mode 2 a new Space window opens when none shows it.

## Consequences

- Amends ADR 0096: "Move all back" and the tab menu's "Move back" (was "Move to main window") go to the Space window showing the tab's workspace; in mode 2 a new Space window opens when none shows it.
- About 250 places moved from `self.workspace` and `HostId::Main` to the host-aware forms (`workspace_in(window)`, `space_host(window)`, `in_workspace`).
- A window is never narrower than `WINDOW_MIN_WIDTH`, so the "need you" chip always shows its full text; the tab area gives way first.
- The taskbar badge, Alt+Tab entries and multi-screen restore can only be checked by hand (HUMAN_CHECKS "2026-11-25 Window modes").
