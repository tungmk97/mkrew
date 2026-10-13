# Terminals screen simplification: feature inventory

Audit of the UI around terminals before the redesign (ADR 0054). Every row still works after the
change. "Now" is where it lives. Code paths are under `crates/mkrew-ui/src/`.

`[x]` = moved / kept, builds, `cargo clippy -D warnings` and the tests pass. `[x] seen` = also seen
on the running app (isolated daemon, `docs/screenshots/2026-10-13-terminals-simplified-dark.png`).
Clicking through menus by hand is listed in `docs/HUMAN_CHECKS.md`.

## Top bar (`shell/chrome.rs::render_title_bar`)

| # | Feature before | Code | Now | Done |
|---|---|---|---|---|
| T1 | Mkrew logo at the left | `logo()` | top bar, unchanged | [x] seen |
| T2 | Workspace switcher: avatar + name + ▾; menu of every workspace (✓ current), "Add workspace…"; tooltip with Next workspace key | `render_workspace_switcher` | top bar, unchanged; the repo label follows it | [x] seen |
| T3 | Branch chip `⎇ master` / `⎇ 2 repos`; tooltip one line per repo; click → Git view | was `render_branch_chip` | muted label after the switcher, same tooltip and click; the only place it shows | [x] seen |
| T4 | Views Board / Terminals / (Docs) / Files / Git; tooltips with shortcuts | `render_view_tabs` | unchanged | [x] seen |
| T5 | Git badge (changed files, hidden at 0), accent pill | `render_view_tabs` | neutral gray pill | [x] seen |
| T6 | Drag area; macOS double-click / drag | `render_title_bar` | unchanged | [x] |
| T7 | Theme toggle, tooltip with shortcut | `toggle-theme` | gray icon | [x] seen |
| T8 | Phone access icon (accent while on), opens the dialog; hidden without daemon | `phone_access.rs::render_phone_button` | gray icon, green dot while on, tooltip explains it | [x] seen |
| T9 | History (◷) | `history` | gray icon | [x] seen |
| T10 | "+ New ▾": Agent / Shell with keys; switches to Terminals; "Starting…" while a pane starts | `render_new_button` | outlined button, same menu | [x] seen |
| T11 | Run main click: no commands → Run setup (runs after save); live → Show Run tab; idle → start all | was `run_main_click` | Run menu: "Run setup…" (runs after save when none set up), "Show Run tab", "Start all" | [x] |
| T12 | `● Running 2/2 · :8082 :5180` (+N past 4), `✕` + amber when some are down; tooltip per command | `run::Summary::label` / `ports_text` / `tooltip` | pill `● 2 running :8082 :5180 ▾`; amber dot + `1/2 running` when some are down; same tooltip; `Run ▾` when idle | [x] seen (idle) |
| T13 | Restart all (↻), Stop all (■ red) icons | was in `render_run_button` | Run menu "Restart all", "Stop all" (red) | [x] |
| T14 | Run ▾ menu: per command state, ports, ↻ ■ / ▶; Show Run tab; Run setup… | `render_run_menu` | `RUNNING IN <WS>` menu: dot, name, ports (or state in words), ↻ / red ■ / ▶; then the "all" actions, Show Run tab, Run setup… | [x] |
| T15 | Run errors in the sidebar notice | `run_action` | unchanged | [x] |
| T16 | Window controls (Snap Layouts) | `window_control` | unchanged | [x] seen |
| T17 | Controls inert behind a dialog | `inert_layer()` | unchanged | [x] |

## Sidebar (`shell/sidebar.rs`, model `sidebar.rs`)

| # | Feature before | Code | Now | Done |
|---|---|---|---|---|
| S1 | "Workspaces" header label | `render_sidebar` | only while there are no workspaces (with `+`); otherwise the filter row heads the list | [x] seen |
| S2 | Remove idle agents (only when > 0) | `remove_idle_agents` | beside the filter, only when > 0 | [x] |
| S3 | Collapse / expand all | `Sidebar::toggle_all` | beside the filter | [x] seen |
| S4 | Add workspace (+) with key | `add_workspace` | beside the filter | [x] seen |
| S5 | Filter field, search icon, typing, Esc | `filter_key_down` | same field; `/` hint (and key, root context only); × clears while it holds text | [x] seen |
| S6 | Chips All / Working / Needs you | `sidebar::Chip` | segmented control; Needs you count amber when > 0 | [x] seen |
| S7 | Errors as `✕ text` | `render_sidebar` | unchanged | [x] |
| S8 | "No workspaces yet." + Add workspace | `empty_state` | unchanged | [x] |
| S9 | "No sessions match." | `render_sidebar` | unchanged | [x] |
| S10 | Workspace row: chevron, avatar, name, pin, summary text, `?`, tooltip = path | `workspace_row` | chevron, 20 px avatar, bold name, pin, `● N working` pill (pulsing) or muted `N idle`, amber `? N`; tooltip = path | [x] seen |
| S11 | Click: activate; on active toggle expand | `click_workspace_row` | unchanged | [x] |
| S12 | ⋯ menu (hover, always on active) + right-click: Pin, Open in Zed, Explorer, Run setup…, default agent / shell, color, Remove… | `render_workspace_menu` | ⋯ on hover only; right-click unchanged; menu unchanged | [x] |
| S13 | ▶ run indicator + tooltip | was `workspace_row(run_tip)` | removed; the RUN group and the Run pill's tooltip show it | [x] |
| S14 | Order: pinned, then age | `Sidebar::sort_projects` | unchanged (Q6) | [x] |
| S15 | Session row: status mark, status tooltip, agent chip, title, pin, time, plan bar | `session_row` | glyph (accent chip while working; status tooltip on it), title, waiting/done glyph, time, × on hover, plan bar; selected = fill + ring | [x] |
| S16 | Click → open; right-click → pane menu | `open_session`, `open_pane_menu` | unchanged | [x] |
| S17 | × Remove from the sidebar | `remove_session` | unchanged | [x] |
| S18 | Hover peek, keyboard peek | `with_peek`, `hover_row` | unchanged | [x] |
| S19 | Run sessions as `shell · run:FE` rows | `session_row` | RUN group: dot, name, ports in green mono; click opens the pane, right-click pane menu, × on hover; stopped: ▶ on hover; strays listed too | [x] |
| S20 | "No sessions yet." rows | `Row::NoSessions` | not drawn; empty workspaces on one "N empty workspaces ▸" line with tiny avatars; a click lists them as full rows (click, ⋯, right-click) | [x] seen |
| S21 | 1 px guide line | `render_sidebar` | kept (sessions + RUN group) | [x] |
| S22 | Resize grip, `[ui] sidebar_width` | `SidebarGrip` | unchanged | [x] |
| S23 | Compact rail | `render_compact_sidebar` | unchanged, plus footer (Inbox icon, voice meter, expand); run sessions are not in the rail (ADR 0054) | [x] |
| S24 | Sidebar as cached view | `sidebar_view.rs` | unchanged | [x] |

## Tab bar (`shell/pane_render.rs::render_tab_bar`)

| # | Feature before | Code | Now | Done |
|---|---|---|---|---|
| B1 | Tab labels (`Terminal`, `Terminal 2`, `run`, renamed) | `tab.title` | **unchanged** | [x] seen |
| B2 | Tab agent chip | `tab_kind` | kept | [x] seen |
| B3 | Most-urgent status mark + tooltip | `tab_dot` | one dot: pulsing accent working, amber needs input, green done / live run, red exited, hollow idle; same tooltip | [x] seen (idle) |
| B4 | Pin mark | `pin_mark` | unchanged | [x] |
| B5 | Click activates; double-click renames | `start_rename` | unchanged | [x] |
| B6 | Right-click tab menu | `render_tab_menu` | same items, plus the pane items of B14 on the active tab | [x] |
| B7 | Tab × | `close_tab` | unchanged | [x] seen |
| B8 | Tab error notice | `tab_error` | unchanged | [x] |
| B9 | "⤢ Pane zoomed" + "Show all panes" | `ZoomPane` | unchanged | [x] |
| B10 | Running-session warning | `running_warning` | unchanged | [x] |
| B11 | Close focused pane icon | `close_focused_pane` | ⋯ menu "Close pane" (key unchanged) | [x] |
| B12 | New tab (+) | `create_tab` | right after the tabs | [x] seen |
| B13 | Split / layout menu | `render_layout_menu` | unchanged | [x] seen |
| B14 | ⋯ tab menu button | `open_tab_menu` | ⋯ = tab menu + Rename pane, Close pane, Follow in Zed (✓), Turns… | [x] |

## Pane header (`pane_render.rs::pane_frame`, `pane_header`)

| # | Feature before | Code | Now | Done |
|---|---|---|---|---|
| P1 | Agent chip tinted by pane color | `pane_header` | split header | [x] |
| P2 | Breadcrumb `<workspace> › <folder> › <agent>` | was `pane_text::breadcrumb` | workspace = switcher, agent = chip; the task folder is a status part (tab bar or split header) | [x] |
| P3 | Status pill + tooltip | `status_pill` | single pane: tab bar right end; split: compact header | [x] seen |
| P4 | Title; double-click rename; F2 | `pane_title` | split header; single pane: sidebar row shows it, F2 shows the rename field in the tab bar, ⋯ "Rename pane" | [x] |
| P5 | Run pane title `FE · :5180` | `run::pane_title` | split header; single pane: RUN group row shows name + ports | [x] |
| P6 | Conflict badge | `pane_status` | tab bar (single) / split header, only when present | [x] |
| P7 | `⧗ N queued` | `pane_status` | same, only when > 0 | [x] |
| P8 | `ctx 62%`, amber + ⚠ from 80 % | `pane_status` | same, only with usage data | [x] |
| P9 | Share → Follow in Zed | `pane_controls` | tab bar (single) / split header; Follow in Zed also in ⋯ | [x] |
| P10 | Turns… (feature flag) | `open_turns` | ⋯ menu | [x] |
| P11 | Right-click header → pane menu | `open_pane_menu` | split header; any pane via its sidebar row | [x] |
| P12 | Pane color line under header | `PANE_ACCENT_WIDTH` | split: under the header; single: 2 px line on top of the terminal | [x] |
| P13 | Focus border, unfocused dim | `pane_border` | border kept (amber needs input); unfocused split panes dimmed; the active tab gets a 2 px accent bottom line | [x] |
| P14 | Pane placeholder | `pane_placeholder` | unchanged | [x] |
| P15 | OS file drop | `drop_catcher` | unchanged | [x] |
| P16 | Divider drag | `pane_divider` | unchanged | [x] |
| P17 | Composer | `composer_panel` | unchanged | [x] |
| P18 | Empty tab state | `render_tab` | unchanged | [x] |
| P19 | Reading width | `reading_on`, `pane_controls` | **new**: 📖 per agent pane; terminal ≤ 960 px centered; default `[ui] reading_width = true`; shell / run full width | [x] |

## Status bar — removed

| # | Feature before | Code | Now | Done |
|---|---|---|---|---|
| U1 | Hide/show sidebar | `sidebar_toggle` | title bar, left | [x] seen |
| U2 | Compact sidebar toggle | `render_sidebar_footer` | sidebar footer (right) | [x] seen |
| U3 | Inbox state, click → next | `render_sidebar_footer` | sidebar footer (left), same click | [x] seen |
| U4 | Git HEAD (`1a2b3c4` / `2 repos`) + tooltip | removed `head_label` | the repo label's tooltip lists `branch · hash` per repo | [x] |
| U5 | Voice input state | `voice_status` | sidebar footer; title bar while the sidebar is hidden; meter only in the rail | [x] |
| U6 | Agent usage: context meter, tokens, cache, model | removed `status_agent_fields`, `context_meter` | ctx % in the pane status; tokens / cache / model on the session panel's card | [x] |
| U7 | Session panel toggle | `inspector_toggle` | title bar, gray icon (raised fill while open) | [x] seen |

## Keyboard and other

Every `keymap.rs` command is unchanged; `/` is new (focuses the filter while the window root has
the keyboard: not in a terminal, field or dialog). Not persisted before or after: expanded
workspaces, filter, chip, sidebar hidden/compact, the empty-workspaces line, per-pane reading width.
Persisted: `[ui] sidebar_width`, `inspector_width`, `theme`, and new `reading_width`.

## Decisions (answered 2026-10-13)

- **Q1** The status bar goes. Its left part becomes a footer inside the sidebar's width: Inbox,
  compact toggle, voice input state (only while active). Right part: usage context % moves to the
  tab bar (Q2); tokens / cache / model stay in the session panel's card; the panel toggle moves to
  the top bar.
- **Q2** Single pane: pill, Share, conflict, `⧗ N queued`, ctx % at the right end of the tab bar,
  each only when present. Title and rename via the sidebar row (F2 still works). Pane color = 2px
  top line on the terminal area.
- **Q3** Change the dark palette app-wide, mapped onto existing tokens; light unchanged; new
  tokens for the top bar / tab bar and the run pill.
- **Q4** `[ui] reading_width` (default `true`) is the default for agent panes; per-pane toggle in
  memory only.
- **Q5** Add `/` to focus the filter, only in the root key context (never in a terminal or field).
- **Q6** Keep the current order (pinned, then age).
- **Spec** ADR 0054 + SPEC §10 / §11 / §20G updated in the same change.
