# 0102 - Widget bar

- Date: 2026-11-22
- Status: accepted (amends ADR 0054, which removed the status bar)
- Milestone: none (Tern-inspired update, sub-project 3; spec `docs/superpowers/specs/2026-11-22-widget-bar-design.md`, plan `docs/superpowers/plans/2026-11-22-widget-bar.md`)

## Context

ADR 0054 removed the old status bar so that each fact had one place.
Tern keeps a thin bar at the bottom of its window with a few small widgets, and the user liked it.
The facts an operator glances at all day (the folder, the Git branch, the machine load, the agents' usage limits, the version) sit in menus and panels today.

## Decision

- One full-width bar at the bottom of the main window, below the sidebar and the main area.
  Detached windows have no bar.
  It is on by default (`[ui] widget_bar = true`).
- Five widgets: folder, Git branch with its change count, agent limits, machine CPU and RAM, version.
  A widget with nothing to show is absent.
  Each does one thing on click (spec section 4).
  The folder and branch labels end with an ellipsis when they pass 160 px.
- The bar is inert while a dialog is open and while the window is reconnecting: clicks and the right-click menu do nothing.
- The bar hides as a whole (`Command::ToggleWidgetBar`, Ctrl+Alt+Shift+J, the palette, the bar's menu) and per widget (the bar's right-click menu, `[ui] widgets_hidden`).
  Unknown ids in `widgets_hidden` are kept.
- The context share stays in the pane header, as ADR 0054 decided.
- Machine load is the daemon machine's, sampled by the daemon with `sysinfo` every 2 seconds, only while a client is connected, and sent when a value moved by 1 point or 100 MB, or every 10 seconds.
- Codex limits come from the transcript (`rate_limits`), read by `UsageReader`.
  Claude limits come from Claude Code's status line JSON through a new `mk statusline` command that `mk integration install claude` sets up.
  If the user had a status line, install saves it and `mk statusline` runs it after reporting; uninstall puts it back.
- Protocol 55: events `agent.limits` and `machine.stats`, requests `limits.get` and `limits.report`.
  The daemon keeps the newest limits per agent in memory and does not store them.

## Alternatives

- Chips in the title bar instead of a bar: the title bar is full, and the widgets would compete with the Run pill and the switcher.
- Machine load of the UI's own computer: wrong in a machine window, where the agents run elsewhere.
  The daemon's machine is the one that matters.
- Claude limits from the OAuth usage endpoint: needs the user's token from Claude's credential file and the network, and the endpoint is not documented for this use.
- Claude limits from nothing (omit Claude): the status line JSON already carries them for Pro and Max users, so omitting them loses the most wanted number.

## Consequences

- The status bar of ADR 0054 comes back in a thin, optional form; its old items did not return.
- `mk integration install claude` now also sets `statusLine`, so `mk integration status claude` reports `Partly` for an install made before this change until it runs again.
- Claude Code runs `mk statusline` on every update, so it must stay fast and silent: the report budget is 300 ms and a failure prints nothing.
- On Windows the chained status line runs through `cmd /C`, not Git Bash, so a status line that needs bash syntax may not run.
- A newer UI against an older daemon is refused by the version check, as for every protocol bump.
