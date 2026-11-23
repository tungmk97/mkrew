# Tern layout - Design

- Date: 2026-11-23
- Status: design approved in chat by the user (2026-11-23), section by section; this file waits for the user's review.
- ADR: `docs/decisions/0106-tern-layout.md` (0103-0105 are taken; `feat/chat-look-2`, the only unmerged branch, adds no new ADR).
- Supersedes the bar part of ADR 0102 (widget bar). The `agent.limits` path of ADR 0102 stays.
- Protocol: 57 -> 58 (checked on `master` and `feat/chat-look-2`, both at 57).

## 1. Goal

The user likes the layout of Tern: one slim top bar that holds the tabs, no bottom bar, and as much room as possible for the terminals.
Mkrew today has a title bar (42), a terminal tab strip under it (44), pane headers, and a widget bar at the bottom (24).
This change is about the window chrome only; what is drawn inside a terminal, the editor or the chat view does not change.

Success means:

- The widget bar is gone, and with it the CPU / RAM sampling in the daemon.
- The agents' usage limits still warn the user, from a chip on the top bar that only shows at 80% or more.
- There is one top bar, full width (no Tern-style floating bubble), and the terminal tabs live in it.
- The view switch `Terminals | Board | Docs | Editor | Diff` is gone; every view stays one click away.
- The top bar shows only what is used often; the rest stays in the palette, its shortcut and Settings.
- Panes have no header; the tab shows the title of the focused pane and its status as an icon.
- The terminal area gains the 44 px of the old strip, the 24 px of the widget bar and the row every pane header took.

## 2. What the user said, and what was assumed

Said by the user:

- Remove the bottom bar (CPU, RAM, version, Git, ...).
- Keep the top bar full width, not a bubble like Tern.
- Remove top bar buttons that belong in Settings or are not needed.
- Remove the `Terminals | Board` switch and put the terminal tabs there.
- Chosen in the brainstorm: Editor / Diff as chips in the tab area, Board / Docs as icons; keep the limits as a chip shown only at 80% or more; drop the logo, the branch chip, History, the theme toggle and the phone button from the top bar; merge `+ New ▾` into the tab `+`; every pane gets a header; no pane title anywhere; the tab shows the focused pane's title.

Assumed (stated in chat, not contradicted):

- A name the user gave a tab still wins over the pane title.
- The tab strip's notices move to toasts.
- Detached windows use the same top bar without the workspace switcher, Board, Docs and the limits chip.

## 3. Top bar

One bar, `TITLE_BAR_HEIGHT` (42), full width, background `theme.chrome`, 1 px bottom divider, as today.

### 3.1 Order, left to right

| Group | Elements |
|---|---|
| Left | macOS inset (`title_bar_inset`) when on mac; machine chip (machine windows only, as today); sidebar toggle `PanelLeft`; workspace switcher (avatar, name, ▾). |
| Tab area | View chips (Editor, Diff, each only while open) │ pinned tabs │ other tabs │ `+ ▾`. A 1 px rule (`TITLE_RULE_HEIGHT`) separates the view chips from the tabs, and the pinned tabs from the rest. |
| Drag space | `flex_1` spacer with `WindowControlArea::Drag`. |
| Right | Limits chip (only at 80% or more); Run pill ▾; update button (only when an update is known); the focused agent pane's toggles (Terminal │ Chat, Reading width, Share with `[features] share`); Board icon; Docs icon (only with `[features] docs`); Split ▾ (`Columns2`); right panel toggle `PanelRight`. |
| Window | Windows only: 1 px rule, then minimize, maximize, close, as today. |

Removed from the top bar:

| Element | Where it still lives |
|---|---|
| App logo | Nowhere (it had no action). |
| Branch chip `⎇ master` / `⎇ N repos ▾` | Git panel (Ctrl+Shift+4, palette "Git panel"). Its repository strip and Repositories menu (`git_menus.rs:41`, `153`) already cover several repositories, so the title bar's `open_repo_menu` / `repo_menu_rows` (`chrome.rs:18-42`) and the `repo_menu` field go. |
| History `◷` | Palette "Global: History…". |
| Theme toggle | Ctrl+Shift+J, palette "Global: Toggle theme", Settings > Appearance. |
| Phone `Smartphone` | Palette "Global: Phone access…". |
| View switch | Section 3.3. |
| `+ New ▾` | Merged into the tab `+ ▾` (section 3.4). |

The voice status that showed in the title bar while the sidebar was hidden (`chrome.rs:390-392`) stays, placed before the limits chip.

### 3.2 Tab area

- `flex_1`, `min_w_0`, horizontal scroll (`overflow_x_scroll` with a `ScrollHandle`); the active tab is scrolled into view when it changes.
- The tab area is not a drag region: only the spacer is, so a click on a tab never moves the window, on Windows and on mac.
- A chip shrinks down to a new token `TAB_MIN_WIDTH` before the area starts to scroll; the title ellipsizes inside it. `TAB_TITLE_MAX_WIDTH` (220) stays the maximum.
- The `+ ▾` button stays visible: it sits after the scrolled tabs, outside the scroll area.

Tab chip, Tern style:

- Active: solid `theme.card` background, radius 8 (`radius::M`), no `selected` border.
- Inactive: transparent background, `text_muted` title; hover gives a background through `.pressable(..)` (rule 11 of the UI design skill).
- Contents: a status icon only while an agent works (hourglass) or a pane needs input (bell; it wins), then pane icons (shell `SquareTerminal`, run `Play`, an agent's brand logo in its brand color, or its glyph when it has no logo; all 14 px), 📌 when pinned, title, ×. The active tab shows one icon per pane (at most three, then `+N`), an inactive tab its focused pane's icon. The tooltip gives the title, the status in words, the time and the turns. The × shows on the active tab and on hover for the others.
- Behavior as today: drag to reorder, drag to another window, right-click tab menu, double-click to rename, F2.
- A click activates the tab and switches the view to Terminals.

View chips (Editor, Diff):

- Shown while file tabs are open (Editor) or a diff is open (Diff), the same conditions as the old view switch.
- Icon, name, and × which closes all file tabs (Editor) or the diff (Diff).
- Not draggable, not reorderable.
- A click calls `show_view`. The active view's chip uses the active tab look; the terminal tabs then all use the inactive look.

### 3.3 Views

| View | Entry |
|---|---|
| Terminals | A click on any terminal tab; Ctrl+Shift+1; palette "View: Terminals". |
| Board | Board icon on the right (accent color while shown, a second click goes back to Terminals); Ctrl+Shift+2; palette; `NewTask`. |
| Docs | Docs icon, same behavior; Ctrl+Shift+5; palette. |
| Editor | Editor chip; Ctrl+Shift+6; palette. |
| Diff | Diff chip; Ctrl+Shift+7; palette. |

The terminal tabs stay in the top bar in every view, so the way back to a terminal is always one click.
`render_view_tabs` and `View::bar` are deleted with their test (`board/tests.rs:25-40`) and the caller in `workspace.rs:400`.

### 3.4 The `+ ▾` button

One button after the last tab, replacing both the strip `+` (empty tab) and the title bar `+ New ▾` (new pane).
Its menu:

1. Each agent profile, the workspace default first with ✓ (Alt+Shift+A starts the default).
2. Agent, when no profile is listed (the existing fallback).
3. Shell (Alt+Shift+S).
4. Empty tab.

Every row creates a new tab and switches to Terminals.
A new pane in the current tab comes from Split ▾ (Split right, Split down), as today.
Existing shortcuts (`NewAgentTab`, `NewShellTab`, `NewAgentPane`, `NewShellPane`) keep their behavior.
While a session starts, the button shows the starting spinner as `+ New ▾` did.

### 3.5 What leaves the strip

| Item | New place |
|---|---|
| `⋯` tab menu button | Deleted; the same menu is on right-click of the tab. |
| `tab_error` | `toast_error`. The field and its host routing go. |
| `tab_notice` (`show_tab_notice`) | `toast_info`. The field goes. |
| Running warning ("N sessions running…") | One `toast_info` when the count rises above `RUNNING_WARN_ABOVE`, not repeated until it drops back to or below it. |
| `⤢ Pane zoomed` + "Show all panes" | A floating badge in the zoomed pane's top-right corner (section 4). |
| Single pane status and controls | Status: the tab icon and tooltip. Controls: the top bar right group (section 4). |
| Split button | Top bar right group. |

### 3.6 Detached windows

`detached_render.rs` draws its own title bar and strip today (`detached_render.rs:150-240`, `319`).
It switches to the same top bar function with a flag that leaves out the workspace switcher, Board, Docs and the limits chip.
The tab chip keeps its workspace-name prefix there.
The duplicate bar and strip code in `detached_render.rs` is deleted.

## 4. No pane header

The first design gave every pane a compact header without a title; the second UI feedback round removed the header entirely (ADR 0106).

- `pane_header`, `pane_controls`, `pane_status`, `pane_rename`, `header_room`, `PANE_HEADER_FIXED` and the pane status pill, elapsed time, context share and agent chip are deleted.
- The context share stays in the composer.
- Transient signals are a stack of floating badges in the pane's top-right corner (`pane_badges`), drawn only while they apply, over the terminal.
- They are the conflict warning (`conflict_badge`), `N queued`, Copy mode with its hint, and while zoomed `⤢ Pane zoomed` with the Show all panes button.
- Badges that only show text let clicks pass through to the terminal; the conflict Dismiss and Show all panes buttons take their own clicks.
- The agent controls are icon toggles in the title bar right group, before Board, acting on the focused agent pane (`pane_toggles`): Terminal | Chat (`MessageSquare`, only when `chat_available`, accent while Chat shows, Ctrl+Alt+T), Reading width (`AlignCenter`, accent while on) and Share (`Share2`, only with `[features] share`, default false).
- The palette keeps "Agent: Toggle chat view" and "Pane: Reading width (on / off)"; `[ui] chat_default` is the setting for the mode of new agent panes.
- The header's right-click pane menu is deleted from the pane.
- The tab menu (right-click a tab) has Rename pane, Close pane, Zoom pane, Follow in Zed, Turns… and Change color for the focused pane; the palette has Pin / Unpin and Mark unread.
- The sidebar session row keeps the pane menu; Shift+F10 on a terminal opens it under the pane's top edge (`Anchor::Pane`).
- In a split tab each pane has a strip along its top edge, `PANE_GRIP_HEIGHT` (8 px) tall, that occludes the mouse so the terminal under it gets no click.
- Hovering it shows a slim grip (`group_hover`); dragging it starts the pane drag the header used (`draggable_pane`).
- A lone pane has no strip.
- A pane with a color has a 2 px line of it at the top; the focus border stays.
- Renaming a pane: F2 or a double-click on the tab while the tab keeps its default name (section 4.1); the field shows in the tab chip, `tab_renaming`.

### 4.1 Tab title from the focused pane

`tabs::session_label` (`tabs.rs:792`) today returns the session title only for a tab with a single pane.
It changes to the focused pane of the tab:

- A tab that keeps its default name (`Terminal`, `Terminal N`) shows the focused pane's session title.
- A name the user gave the tab always wins.
- When the focus moves to another pane of the tab, the tab title follows.
- A double-click on a tab with its default name renames the focused pane; on a tab the user named, it renames the tab.

The doc comment of `session_label` and its tests change with it.

## 5. Widget bar removal

### 5.1 UI

Deleted:

- `crates/mkrew-ui/src/shell/widget_bar.rs` and the bar in `main.rs:1308`.
- `crates/mkrew-ui/src/widget_bar.rs`, except the limits logic, which moves to a new `crates/mkrew-ui/src/limits.rs` (section 5.2).
- `Command::ToggleWidgetBar` and its bindings (`keymap.rs:214, 497, 610, 738`), the palette entry "View: Show/hide widget bar", the Settings field "Widget bar" on Appearance, the bar's right-click menu, the `WIDGET_BAR_*` theme tokens and their tests.
- Shell fields `widget_bar`, `widgets_hidden`, `widget_data` (renamed to `limits`), and their uses in `connection.rs`, `focus.rs`, `menu_render.rs`, `sidebar.rs:672`.
- The widget bar rule of `theme::problems()` (ADR 0101/0102) becomes the limits chip rule: `status_needs_input` and `danger_text` need 4.5:1 on `chrome`, which the themes already meet.

The other widgets already have a home:

| Widget | Where |
|---|---|
| Folder | Palette "Reveal in Explorer". |
| Git | Git panel, Ctrl+Shift+4. |
| Version | Settings (`shell/settings.rs:421`). |

### 5.2 Limits chip

`limits.rs` keeps from `widget_bar.rs`: the limits store (`apply` for `Event::AgentLimits`, `load_limits`, `clear_limits`), `Level::of` (amber from 80%, danger from 95%, on the rounded percent), `in_text` and `local_clock`.
It adds one function: the chip for a list of limits.

- No window at 80% or more: no chip.
- Otherwise the chip shows the highest window: `⚠ Claude 5h 84%`. Ties go to the one that resets later.
- Color: `status_needs_input` at 80-94%, `danger_text` at 95% and more; always with the ⚠ icon and the text.
- Tooltip: every known window (Claude 5h / 7d, Codex 5h / 7d) with its percent and reset time.
- Click: Settings > Integrations (`settings_dialog::INTEGRATIONS_TAB`).
- `redraw.rs`: `Event::AgentLimits` always redraws the main window (the event is rare); `Shown.widget_bar` goes.

The `agent.limits` event, `limits.get`, `limits.report` and `mk statusline` stay unchanged.

### 5.3 Daemon and protocol

- `crates/mkrew-daemon/src/machine.rs` is deleted with its start in the daemon and the machine tests of `tests/daemon/widgets.rs`. The file keeps its limits tests and is renamed `tests/daemon/limits.rs` (its `mod` line follows).
- `Event::MachineStats` and `MachineStats` leave `mkrew-protocol` (`message.rs`), with their tests.
- `PROTOCOL_VERSION` 57 -> 58.
- `sysinfo` stays a dependency of the daemon: `ports.rs` uses it.

### 5.4 Settings

`UiSettings` uses `deny_unknown_fields`, so a `settings.toml` that has `widget_bar` or `widgets_hidden` would stop loading if the fields were only deleted.
The two keys are kept as retired keys: accepted when present, ignored, never written, with a comment that names ADR 0106.
`write_widgets_hidden` and the two lines of the commented template (`settings.rs:645-666`) go.
A test loads a file with both keys and gets the defaults for everything else.

## 6. Error handling

- Failures that showed in the strip now show as toasts; `Global: Copy last error` keeps working since it reads the toast stack.
- A repeated failure does not stack: `toast::Toasts::push` already refreshes an identical toast instead of adding a copy (`toast.rs:40-41`).
- The limits chip never shows a value it does not have: no data, or nothing at 80%, means no chip.

## 7. Testing

Unit tests, pure logic only:

- `session_label`: focused pane title on a default-named tab; a user-named tab wins; focus change changes the label; empty tab gives none.
- Limits chip: below 80% none; 80% amber; 95% danger; the highest of several wins; tie rule.
- Settings: a file with `widget_bar` and `widgets_hidden` loads.
- Tab icons: active vs inactive, more than three panes give `+N`, needs input beats working, an empty tab, an agent without a brand logo; the `brand logo` contrast rule.

Deleted tests: widget bar model and renderer tests, `ToggleWidgetBar` keymap and palette tests, `MachineStats` protocol and daemon tests, `View::bar`, the widget bar settings tests, `redraw.rs` widget bar tests (replaced by the limits redraw rule).

Smoke on the real app (PrintWindow screenshots, dark and light), saved to `docs/screenshots/tern-layout-<view>-<theme>.png`:

- One pane and four panes in a tab.
- Ten or more tabs in an 1100 px wide window (scroll, active tab in view, `+ ▾` visible).
- Editor and Diff open (chips), Board and Docs shown (icons highlighted).
- A tab dragged to a detached window and back.
- Limits at 85% and 96% through `limits.report` with made-up values.
- Window drag from the spacer, double-click maximize, and a click on a tab that does not move the window.

`HUMAN_CHECKS.md`: remove the widget bar checks (187-195), fix line 1527 ("status bar"), add the new layout check for dark and light, and a macOS check (traffic lights, window drag, tab clicks).

## 8. Docs

- ADR 0106 "Tern layout": supersedes the bar part of ADR 0102; records the strip in the title bar, the removed view switch, the removed pane header with its badges and top bar toggles, the tab icons, the removed machine stats and protocol 58.
- SPEC §7 (protocol 58, `machine.stats` gone), §10 (window diagram, and the title bar height becomes 42 everywhere: §10 said 44, §14 said 48), §11 (no pane header, tab icons and tooltip, tab title from the focused pane), §13 (shortcut rows of the widget bar go), §14 (tab chip look, `TAB_MIN_WIDTH`, pane badges and drag strip), §20.0 (`[ui] widget_bar` and `widgets_hidden` retired, `[features] share`).

## 9. Order of work

Each step is one commit and passes `scripts/verify.sh`.

1. Protocol 58 and daemon: remove `MachineStats` and `machine.rs`.
2. UI: remove the widget bar, add `limits.rs` and the limits chip, retire the two settings keys.
3. Top bar: tab area in the title bar, view chips, Board / Docs icons, merged `+ ▾`, `⋯` removed, strip notices to toasts, detached windows on the same bar, removed buttons.
4. No pane header; floating pane badges, top bar toggles, top edge drag strip; `session_label` from the focused pane.
5. Docs: ADR 0106, SPEC, HUMAN_CHECKS, screenshots.

## 10. Out of scope

- What is drawn inside a terminal, the editor, the chat view or the right panel.
- The sidebar (its filter row, cards and footer stay).
- Making Board, Editor or Diff real tabs in the tab model (approach B of the brainstorm).
- A floating "bubble" top bar like Tern's.
