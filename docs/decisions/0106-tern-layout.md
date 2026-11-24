# 0106 - Tern layout: one top bar with the tabs, no bottom bar

- Date: 2026-11-23
- Status: accepted (supersedes the bar part of ADR 0102; amends ADR 0034, 0054, 0096 and SPEC sections 7, 10, 11, 13, 14 and 20.0); superseded in part by 0108 (the Editor and Diff chips, `+ ▾`, the thin pane border and the Alternatives line about Editor and Diff)
- Milestone: none (human request 2026-11-23, spec `docs/superpowers/specs/2026-11-23-tern-layout-design.md`, plan `docs/superpowers/plans/2026-11-23-tern-layout.md`)

## Context

The user likes the layout of Tern: one slim top bar that holds the tabs, no bottom bar, and as much room as possible for the terminals.
Mkrew had a title bar (42 px), a terminal tab strip under it (44 px), pane headers on split panes only, and the widget bar of ADR 0102 at the bottom (24 px).
The widget bar showed the folder, the Git branch, the agents' limits, the machine load and the version; most of it has a home elsewhere, and the user asked to remove the bar.
The view switch `Terminals | Board | Docs | Editor | Diff` and a row of rarely used buttons (logo, branch chip, History, theme toggle, phone, `+ New`) filled the title bar.

## Decision

- One full-width top bar of `TITLE_BAR_HEIGHT` (42).
  Left: macOS inset, machine chip (machine windows), sidebar toggle, workspace switcher.
  Then the tab area, a drag spacer (at least 32 px wide, so the window stays movable with many tabs), and on the right the limits chip, Run pill, update button, the focused agent pane's toggles (Terminal | Chat, Reading width, Share), Board and Docs icons, Split (in the main window only while Terminals show), right panel toggle, and on Windows the window controls.
- The tab strip lives in the tab area: Editor and Diff chips (only while open), pinned tabs, other tabs, and a `+ ▾` button.
  The area scrolls sideways; a chip shrinks to `TAB_MIN_WIDTH` first; `+ ▾` stays visible.
  Only the spacer drags the window, so a click on a tab never moves it.
- Tab chip look (Tern): the active tab has a solid `tab_active` fill (a new theme token: `card` equals `chrome` in Mkrew Light and Classic light, and `selected` is nearly `chrome` in One Light) and radius 8 without a border; inactive tabs are transparent with muted text and a hover fill.
  A click on a tab also shows Terminals.
- The view switch is removed.
  Terminals: any tab; Board and Docs: icons that return to Terminals on a second click; Editor and Diff: their chips.
- `+ ▾` replaces the strip `+` and `+ New ▾`: agent profiles (default first), Shell, Empty tab; each creates a tab and shows Terminals.
  A new pane in the current tab comes from Split ▾ and the existing shortcuts.
- Removed from the bar: logo, branch chip and repo menu (the Git panel has the repository strip), History, theme toggle, phone button, `⋯` tab menu button (right-click on a tab opens the same menu).
  All stay in the palette, their shortcuts and Settings.
- The strip's notices become toasts: errors `toast_error`, notices `toast_info`, and the running warning one `toast_info` when the count rises above `RUNNING_WARN_ABOVE` (again only after it dropped back).
- Panes have no header (UI feedback round 2).
  The status pill, elapsed time, context share, agent chip and task folder are gone from the pane.
  The status lives in the tab icon and its tooltip, and the context share in the composer.
  Transient signals are a small stack of floating badges in the pane's top-right corner, drawn only while they apply.
  They are the conflict warning, `N queued`, Copy mode with its hint, and while zoomed "Pane zoomed" with "Show all panes".
  Badges that only show text let clicks pass through to the terminal; the conflict Dismiss and "Show all panes" buttons take their own clicks.
  The agent controls move to the title bar right group, before Board, as icon toggles that act on the focused agent pane.
  They are Terminal | Chat (only when the pane can chat), Reading width, and Share (only with `[features] share`, default false).
  The Share code stays, and Follow in Zed stays in the tab menu and the palette.
  The header's right-click pane menu is gone from the pane.
  The tab menu has Rename pane, Close pane, Zoom pane, Follow in Zed, Turns… and Change color for the focused pane, and the palette has Pin and Mark unread.
  The sidebar session row keeps the pane menu, and Shift+F10 on a terminal opens it under the pane's top edge.
  A pane in a split tab has an 8 px strip along its top edge (`PANE_GRIP_HEIGHT`) that shows a grip on hover and drags the pane onto another.
  The strip occludes the mouse, so the terminal under it gets no click; a press just below it selects text.
  A lone pane has no strip.
  A pane with a color has a 2 px line of it at the top; the focus border stays.
  A pane is renamed in its tab chip (F2, or a double-click on a tab that keeps its default name).
- The tab title follows the focused pane: a tab that keeps its default name shows the focused pane's session title, a name the user gave always wins.
  A double-click on a default-named tab renames the focused pane, on a named tab the tab.
- The widget bar is deleted.
  Limits become a chip in the bar that shows only at 80% or more: `⚠ Claude 5h 84%`, the highest window, `status_needs_input` at 80-94% and `danger_text` from 95%; the tooltip lists every window; a click opens Settings > Integrations.
  Folder, Git and version are in the palette, the Git panel and Settings.
- Machine stats are removed: `Event::MachineStats`, `MachineStats` and the daemon's `machine.rs` sampler go; protocol 57 -> 58.
  `sysinfo` stays for `ports.rs`.
- Detached windows draw the same title bar function without the workspace controls, `+ ▾`, Board, Docs, Editor and Diff chips, Run pill and limits chip, with Move all back in the right group; their chips keep the workspace name in front.
- `[ui] widget_bar` and `[ui] widgets_hidden` are retired: `settings.toml` files that have them still load (the keys are accepted, ignored and never written).
  `Command::ToggleWidgetBar` and its shortcuts are gone.
- Tab chip icons (Tern style): every icon is 14 px (`ICON_SIZE_SMALL`), a Lucide stroke or a brand logo, centered with the title and one `XS` gap apart.
  A status icon leads the chip only when the user should notice: an hourglass in `status_working` while an agent works, a bell in `status_needs_input` when a pane needs input (needs input wins); idle, done, exited and starting show none.
  Then one icon per pane: a shell is `SquareTerminal` and a run pane is `Play`, both `text_muted`; an agent is its brand logo (claude, codex, copilot, omp) in its brand color, and an agent without a logo (fake) its glyph in `text_muted`.
  The active tab shows every pane in pane order, at most three and then `+N`; an inactive tab shows its focused pane's icon.
  The tab tooltip gives the title, the status in words, the elapsed time and the turns of the most urgent pane, so color is never the only signal.
  Brand colors are theme tokens (`brand_claude`, `brand_codex`, `brand_copilot`, `brand_omp`) for every theme.
  A `theme::problems()` rule ("brand logo") needs 3:1 on `chrome` and on `tab_active`, and a palette theme darkens or lightens its token until it meets both.
  `TAB_MIN_WIDTH` is 120: an inactive chip shows the status icon, one icon and about five letters.
- A 1 px rule sets the title bar groups apart: left group, tab area, drag space, right group (the rule before the window controls stays).
- The sidebar's filter row is gone: the All / Working / Needs you control is its top, the filter box shows only while it is focused or holds text (Alt+0 and `/` show it), and Remove idle agents and terminals, Collapse all and Expand all workspaces are palette entries.
  Adding a workspace stays in the switcher, the palette and the empty state.

## Alternatives

- A floating "bubble" top bar like Tern's: rejected by the user; the bar stays full width.
- Board, Editor and Diff as real tabs in the tab model: more work and a model change; chips and icons give the same reach.
- Keeping the widget bar as an option: rejected, two ways to see limits and a daemon sampler for one number nobody asked to keep.
- A limits chip that always shows: a value below 80% is noise; the chip is a warning.

## Consequences

- The terminal area gains the 44 px of the old strip, the 24 px of the widget bar and the row every pane header took.
- Protocol 58: a UI from before it shows a version mismatch with an old daemon; the user restarts the daemon.
- A UI from before protocol 58 cannot connect to the new daemon (the version check blocks the connection), so no `machine.stats` event ever reaches a client that cannot read it.
- The title bar height is 42 everywhere; SPEC section 10 said 44 and section 14 said 48.
- The widget bar rule of `theme::problems()` (ADR 0102) is now the limits chip rule: `status_needs_input` and `danger_text` need 4.5:1 on `chrome`; the themes already meet it, so the amber darkening of ADR 0102 stays.
- A `theme::problems()` rule ("active tab") reports a theme whose `tab_active` is within 1.1:1 of `chrome` or whose `text` does not read on it, so the active tab chip always shows.
- With many tabs the area scrolls and the active tab is scrolled into view; the drop line and window drops use the recorded strip bounds, which now cover the scrolling area only.
- The Editor view's own file-tab strip and the sidebar are unchanged.

## Amended by 0108

ADR 0108 makes files and the diff leaves of the tab's split tree, so the Editor and Diff chips, the Editor view's file-tab strip and the Diff view are gone, and the Alternatives line "Board, Editor and Diff as real tabs in the tab model" no longer holds (Board and Docs stay icons).
`+ ▾` becomes the `+` row, panes lose their border for a frame gap and a top stripe, and the detached window's tab area has the `+` row.
The rest of this ADR (one top bar, no bottom bar, tab chip look, tab icons, no pane header, floating badges, limits chip, protocol 58) stands.
