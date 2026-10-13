# 0054 — A simpler Terminals screen, no status bar

- Date: 2026-10-13
- Status: accepted (changes the layout of ADR 0034's title bar and ADR 0038's panel toggle; replaces
  ADR 0018's split Run button with a pill; removes the SPEC §10 status bar)
- Milestone: none (outside the milestone loop; user request, checklist
  `docs/terminals-simplify-checklist.md`)

## Context

The Terminals screen showed several things twice (the repository count in the title bar and the
status bar, the run state in the title bar and on the workspace row, the session title in the pane
header and the sidebar), used color for plain actions (a blue Run button, amber Restart, an accent
phone icon) and kept empty things on screen ("No sessions yet." rows, a pane header over a single
pane). The user asked for: one place per piece of information, color only for status, hide what is
empty, detail on demand — without losing any feature.

## Decision

- **Theme:** the dark palette takes the requested values on the existing tokens (`bg`, `sidebar`,
  `text`, `text_muted`, `accent`, `accent_bg`, `selected`, the status colors; `status_idle` is the
  `dim` gray); new tokens `chrome` (title bar, tab strip), `selected_ring`, `run_pill_bg` /
  `run_pill_border` / `run_pill_text`, in both appearances. The ANSI palette is unchanged (it is
  the programs' output).
- **Title bar:** sidebar toggle, workspace switcher with the muted `⎇ master` / `⎇ N repos` label
  (its only place), views (gray Git badge); right: the Run pill, History, theme, phone (gray, a
  green dot while on, an explaining tooltip), the session-panel toggle, an outlined `+ New ▾`.
- **Run pill:** `● 2 running :8080 ▾` (green tokens) or a muted `Run ▾`; a click opens one menu
  with every Run action (per command ↻ / ■ / ▶, Restart all, Stop all or Start all, Show Run tab,
  Run setup…). The dot is steady: a pulse redraws the whole window for as long as a dev server
  runs, against SPEC §15's idle CPU budget. Working dots (sidebar pill, tab dot) do pulse
  (`with_animation`, which gpui draws still under reduce-motion); an agent that works prints
  output anyway.
- **Sidebar:** filter field (`/` focuses it in the root key context only, `×` clears it) with
  remove-idle, collapse-all and `+` beside it; a segmented All / Working / Needs you (amber count);
  workspace rows with a working pill or muted idle count, ⋯ on hover; session rows with the glyph
  (an accent chip while working), title, time, ✕ on hover, a ringed selection; a RUN group per
  workspace from the run summary (run sessions are no longer session rows; stopped commands show
  too); workspaces with no session and no run command collapse into one "N empty workspaces" line
  (never the shown or a pinned one); a footer with the Inbox state, the voice input state and the
  compact toggle. The order stays pinned-first then age (the Workspace 1–9 keys follow it).
- **Tab strip and panes:** one status dot per tab; a single pane has no header — its status
  (folder, pill, conflict, queued, context share) and agent controls (Reading width, Share) sit at
  the right end of the strip, F2 shows the rename field there; the active tab has a 2 px accent
  line along its bottom; a split shows compact headers, the panes keep only their thin border and
  the unfocused ones are dimmed. Close pane, Rename pane, Follow in Zed and
  Turns… are in the `⋯` (tab) menu.
- **Reading width:** an agent pane's terminal element is at most 960 px wide and centered, so the
  usual resize gives the PTY fewer columns; nothing reflows output. Default `[ui] reading_width`
  (`true`); the per-pane toggle is in memory only (a per-session stored value would be a new
  persistence format).
- **No status bar:** its items moved (above); the agent's tokens / cache / model stay on the
  session panel's card, the context share moves next to the pill. The status-bar-only code (context
  meter, head label) is deleted.

## Consequences

- Run sessions are not listed in the compact rail any more (the rail has no RUN group); the run
  menu's "Show Run tab" and the `run` tab reach them.
- The voice input state needs the sidebar: it shows in the title bar while the sidebar is hidden,
  so a live microphone is always marked.
- Tab titles, rename behavior, key bindings (plus `/`), terminal emulation, the daemon, the
  protocol and the stored formats are unchanged.
