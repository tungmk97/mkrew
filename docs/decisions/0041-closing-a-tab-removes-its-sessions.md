# 0041 — Closing a tab removes its sessions

- Date: 2026-10-10
- Status: accepted
- Milestone: none (outside the milestone loop; user request "khi Terminal tab bị đóng, agent/shell
  của tab đó phải xoá khỏi workspace sidebar")

## Context

- SPEC §7 said `tab.close` deletes the tab and its sessions keep running, reachable from the
  sidebar. The user reported that as a bug: after closing a tab its agents and shells stay listed
  in the sidebar with no tab to show them.
- A session sits in at most one tab (`split_focused` refuses a session already placed).
- `session.remove` (ADR 0019) already hides a session for good and asks a live one to stop.

## Options

1. UI calls `session.remove` for each pane after `tab.close`. `mk` and other clients would still
   leave sessions behind.
2. The daemon's `tab.close` removes the sessions of the tab's layout. (Chosen.)

## Decision

- `tab.close` reads the tab's layout, deletes the tab, then runs `session.remove` for every
  session with a pane in it. A session it cannot remove is logged, not an error (the tab is
  already gone).
- The UI asks first when the tab holds a running agent (same rule as closing its pane); on `ok`
  it drops the sessions from the sidebar at once.

## Consequences

- SPEC §7 (`tab.close`) and §10 (Terminals) are updated in the same change.
- To keep an agent running, hide its pane (pane ✕ → Hide) before closing the tab.
- Sessions in no tab (hidden panes) are not touched by closing a tab.
