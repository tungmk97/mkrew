# 0038 — Session panel (right inspector)

- Date: 2026-10-10
- Status: accepted
- Milestone: inbox 2026-10-10
- Amends: 0027

## Context

The redesign asks for a panel on the right of the Terminals view that shows the focused pane's
session: agent, title, state, usage, a way to open its transcript, and Stop (later: the git
changes and the plan). SPEC §10 described one window with two columns (ADR 0027). There was no
resizable handle anywhere (pane dividers are keyboard-only), no UI-side place for a remembered
width, and the UI never learned the transcript path: only the daemon's usage reader found it.

## Options

1. A third column right of Terminals, closed by default; width in `settings.toml [ui]`; the
   transcript path on `session.usage`.
2. Same, with the width in a new UI state file or a daemon table.
3. A dialog or popover per pane instead of a column.

## Decision

Option 1.

- The panel exists in the Terminals view only and is closed at every start; the width is
  remembered (`[ui] inspector_width`, 240–560 px, written once when a drag ends with the
  text-preserving `settings::write_ui_value`, the same path as the theme toggle). No new file
  and no new table: `settings.toml` already holds the UI's own preferences.
- `session.usage` carries `transcript?`, the absolute path the daemon's reader located (daemon
  and UI share a machine, SPEC §3). No new request: the event is stored as JSON, so a reconnect
  gets the path too. `PROTOCOL_VERSION` 36.
- "Open transcript" opens the `.jsonl` in Zed (SPEC §10): no OS default app is reliable for it.
- Stop reuses the close-pane rule: a running agent asks first, a shell stops at once. It does
  not close the pane; the pane leaves when the session exits (ADR 0033).
- Default key `Ctrl+Shift+Y` (unused; plain Ctrl+letter would reach the terminal).

## Consequences

SPEC §10 (Session panel bullet), §13 (row), §20.0 (`inspector_width`) and §20E.3 (`transcript?`)
are updated. The terminals lose the panel's width while it is open and their agents redraw once
per size change. Changes and Plan cards (later tasks) reuse the column. The panel's open state is
not remembered; add `[ui] inspector_open` if that is asked for.
