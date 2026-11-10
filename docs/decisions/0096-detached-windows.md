# 0096 - Terminal tabs (later files) move into their own windows

- Date: 2026-11-10
- Status: accepted (amends SPEC §10 "One window." and §13)
- Milestone: none (human request 2026-11-10, spec `docs/superpowers/specs/2026-11-10-detached-windows-design.md`)

## Context

The user wants a terminal tab or an Editor file on a second screen, like IntelliJ's "Open in New Window".
`Shell` is the root view of the only window and owns every piece of UI state; many functions read "the shown workspace's active tab".

## Decision

- Approach A: `Shell` stays the single owner of state; each detached window is a thin `DetachedWindow` root view that asks `Shell::render_host` to draw it.
- The host of a render or an input is `Shell::host_of(window)`, a lookup of the window handle; no "current window" is stored.
- A detached window's keys go through `Shell::run`, which routes them to `run_detached`: an explicit list of tab and pane commands, so no key there can act on the main window's tabs.
- `mkrew_ui::detached` records which tab sits in which window; `Tabs.active` stays the main window's and never names a detached tab.
- The source window decides a drop (gpui-pre-windows captures the mouse on button down), from the windows' screen bounds and the strips recorded at paint.
- Detached windows are saved locally in `<data>/windows.json`, not in the daemon (no protocol change).
- Closing a detached window closes its tabs (ADR 0041 confirmation); quitting keeps them.
- Plan 1 detaches terminal tabs; Plan 2 adds Editor file tabs.

## Alternatives

- One `Workbench` entity per window: cleanest boundaries, but rewrites most of `shell/`.
- One process per window (like machine windows, ADR 0089): unsaved file text cannot move between processes.
- A stored "current host" set at each entry point: async continuations and listeners would read a stale host.

## Consequences

- `focused_session`, `composer_session` and `focused_agent` take the window.
- Menu anchors are recorded per window.
- Voice input in a detached window is not handled (Plan 1).
- Tests: `detached::tests`, `window_place::tests`, `tabs::tests` (`with_active`, `focus_pane`), keymap and palette tests.
