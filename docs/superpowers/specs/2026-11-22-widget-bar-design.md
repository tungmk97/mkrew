# Widget bar - Design

- Date: 2026-11-22
- Status: approved under the user's standing approval of the controller's recommendations (2026-11-22, "assume OK with your recommendations"); sub-project 3 of the Tern-inspired update (Polish was 1, Theme gallery 2).
- ADR: `docs/decisions/0102-widget-bar.md` (0098-0100 are taken by the macOS plans, 0101 by the Theme gallery).
- Amends ADR 0054, which removed the old status bar.
- Protocol: 54 -> 55 (checked on `master`, `feat/theme-gallery`, `feat/md-split` and `fix/restart-cleanup`).

## 1. Goal

Tern keeps a thin bar at the bottom of the window with small widgets: the folder, the git branch, the machine's CPU and RAM, the agents' usage limits and the app version.
The user liked it.
Mkrew gets the same bar so the facts an operator glances at all day sit in one place, without opening a panel.

Success means:

- The bar shows the workspace folder, the git branch with its change count, the daemon machine's CPU and RAM, the Claude and Codex 5-hour and weekly limits, and the Mkrew version.
- Each widget does one useful thing on click.
- The bar can be hidden as a whole (Ctrl+Alt+Shift+J, palette, View menu) and per widget (right-click menu), and the choice survives a restart.
- No widget invents data: a widget with nothing to show is absent, not "0%".

## 2. Decisions (standing approval)

| Topic | Decision |
|---|---|
| Where | One full-width bar at the bottom of the main window, below the sidebar and the main area. Detached windows have no bar. |
| Default | Shown. `[ui] widget_bar = true`. |
| Toggle | `Command::ToggleWidgetBar`, default key `ctrl-alt-shift-j` (free today), a palette entry "Show/hide widget bar", and an item in the bar's right-click menu. |
| Per widget | The right-click menu lists every widget with a check mark; `[ui] widgets_hidden = []` holds the ids of hidden ones. |
| Order | Left: folder, git. Right: limits, machine, version. |
| Context share | Not a widget: the pane header already shows it (ADR 0054). |
| Machine | CPU and RAM of the daemon's machine (the one the agents run on), sampled by the daemon, not the UI's own machine. |
| Claude limits | From Claude Code's status line JSON (`rate_limits.five_hour`, `rate_limits.seven_day`), through a new `mk statusline` command that `mk integration install claude` sets up. |
| Codex limits | From the Codex transcript (`rate_limits.primary` / `secondary`), read by the existing `UsageReader`. |
| omp limits | None: omp does not record them. |
| Limit colors | Below 80% muted text; 80% and over `theme.status_needs_input` (amber); 95% and over `theme.danger_text`. |

## 3. The bar

- A new file `crates/mkrew-ui/src/shell/widget_bar.rs` holds `render_widget_bar` and the widget list.
- `impl Render for Shell` adds the bar as the last `flex_none` child of the root column, after the `flex_1` row.
- Height, padding, gap and font size are new `theme.rs` tokens (`WIDGET_BAR_HEIGHT`, using the existing spacing scale and the small font size).
- Background `theme.chrome`, a 1 px `theme.border` top edge, text `theme.text_muted`, the same look in dark and light.
- Each widget is an icon plus a short label in one `pressable(Look::Text, ..)` element, so hover, press and focus come from `widgets.rs` (SPEC §14, ADR 0097).
- Each widget has a tooltip with the full value and the click action.
- When the window is too narrow, widgets drop from the middle outward: limits first, then machine; folder, git and version stay.
- Right-click anywhere on the bar opens `render_menu` with one checkable row per widget and a last row "Hide widget bar".

## 4. Widgets

| Widget | Shows | Click | Source |
|---|---|---|---|
| Folder | Workspace name; tooltip the root path. | Opens the root folder in the file explorer (the existing open-in-file-explorer helper). | `Shell` workspace state. |
| Git | `⎇ branch` and, if anything changed, the count (`⎇ master · 3`); a workspace with several repos shows `⎇ N repos`. | Opens the Git view; when the Git view is already open, steps to the next changed file (`step_file(1)`), wrapping at the end. | `GitSummary` (`branch_label`, `changed()`), already polled. |
| Limits | One group per agent that has reported: `Claude 5h 42% · 7d 18%`, `Codex 5h 10% · 7d 3%`; tooltip shows each reset time as local time and "in 2 h 10 min". | Opens Settings at the Integrations page. | `Event::AgentLimits` (section 5). |
| Machine | `CPU 23% · RAM 11.2/31.8 GB`; tooltip names the machine. | None (no click, no pointer). | `Event::MachineStats` (section 6). |
| Version | `v0.5.3`. | Runs the existing update check. | `env!("CARGO_PKG_VERSION")`. |

- A widget with no data is absent: no limits reported yet means no limits widget; a daemon that has not sent stats yet means no machine widget.
- A limit whose `resets_at` has passed shows nothing for that window until a new report arrives.

## 5. Agent limits

### 5.1 Data

```rust
// mkrew-protocol
pub struct LimitWindow { pub used_percent: f32, pub resets_at: Option<i64> } // epoch seconds
pub struct AgentLimits {
    pub agent: AgentKind,              // Claude or Codex
    pub five_hour: Option<LimitWindow>,
    pub weekly: Option<LimitWindow>,
    pub observed_at: i64,              // epoch seconds
}
```

- The daemon keeps the newest `AgentLimits` per agent kind in memory (limits are per account, not per session), newer `observed_at` wins.
- It broadcasts `Event::AgentLimits` on every change.
- A new request `limits.get` returns the current list, so a UI that connects later gets them at once.
- Limits are not persisted: the next turn or status line run refills them.

### 5.2 Codex

- `UsageReader` parses `rate_limits.primary` (`window_minutes` 300) as `five_hour` and `secondary` (`window_minutes` 10080) as `weekly`, taking `used_percent` and `resets_at`.
- A window whose `window_minutes` is neither is ignored.
- The daemon's usage `track` task passes the limits on with the usage it already sends.

### 5.3 Claude

- A new CLI command `mk statusline` is Claude Code's status line command.
- It reads the status line JSON from stdin (at most 1 MB), takes `rate_limits.five_hour` and `rate_limits.seven_day`, and sends `limits.report` to the daemon with a 300 ms connect and send budget; a failure is silent.
- Then, if the user had a status line of their own, it runs that command with the same stdin, a 5 s timeout, and prints its stdout; otherwise it prints nothing.
- The command runs in the shell Claude Code itself uses: on Windows Git Bash (`CLAUDE_CODE_GIT_BASH_PATH`, else the `bash.exe` of the Git found on `PATH`), else PowerShell; elsewhere `sh -c`.
- On timeout the whole process tree of that command is killed.
- `mk integration install claude` sets `statusLine = {"type": "command", "command": "<mk> statusline"}` next to the hooks.
- If the user already has a `statusLine` that is not Mkrew's, install saves it to `claude-statusline.json` next to Claude's `settings.json` before replacing it, and `mk statusline` chains to it.
- `mk integration uninstall claude` puts the saved status line back (or removes the key if there was none) and deletes the saved file.
- `mk integration status` reports `Partly` when the hooks are installed but the status line is not.
- `mk statusline` reaches the daemon the same way `mk hook` does (the local socket), so it needs no new transport.

## 6. Machine stats

- The daemon samples CPU (total, all cores) and RAM (used, total) with `sysinfo` every 2 s.
- Sampling runs only while at least one client is subscribed to events, so an idle daemon does no work.
- It broadcasts `Event::MachineStats { cpu_percent: f32, ram_used: u64, ram_total: u64 }` when a value changes by at least 1 percentage point or 100 MB, or every 10 s otherwise.
- The UI shows the stats of the machine the window is connected to.

## 7. Settings and keys

- `[ui] widget_bar: bool` (default `true`) and `[ui] widgets_hidden: Vec<String>` (default empty) in `UiSettings`, with entries in `settings_fields::SETTINGS` on the Appearance page ("Widget bar" bool).
- Unknown ids in `widgets_hidden` are kept and ignored, so an older build does not lose a newer build's choice.
- `Command::ToggleWidgetBar` with the default key `ctrl-alt-shift-j` and a palette entry; SPEC §13 lists it.

## 8. Error handling

| Case | Behavior |
|---|---|
| No limits reported | Limits widget absent. |
| Status line JSON without `rate_limits` (API key users, older Claude Code) | `mk statusline` sends nothing; the chained command still runs. |
| Daemon not running when the status line runs | The report is dropped; the chained output is unchanged. |
| Chained command fails or times out | `mk statusline` prints nothing and exits 0, so Claude Code is not disturbed. |
| Saved status line file unreadable | Treated as "no chained command"; uninstall leaves `statusLine` removed and logs a warning. |
| `sysinfo` reports no total RAM | The RAM part is left out. |
| Daemon older than protocol 55 | The existing version check refuses the connection, as today. |

## 9. Testing

- Core: `UsageReader` reads the Codex limits from the existing fixture (`tests/fixtures/usage/codex.jsonl`), ignores unknown windows, and keeps working on a transcript without limits.
- CLI: install, status and uninstall of the status line with and without a user status line (pure JSON edits, like the existing hook tests); `mk statusline` parses the JSON and chains the user's command (a test command that echoes stdin).
- Daemon: `limits.report` then `limits.get` returns it; a newer `observed_at` replaces an older one and an older one is ignored; `Event::AgentLimits` is broadcast.
- Daemon: the machine stats throttle rule as a pure function (change thresholds and the 10 s heartbeat).
- UI: the widget list for a given state (which widgets show, labels, colors by percent, narrow-width dropping) as a pure function; settings round-trip of `widget_bar` and `widgets_hidden`.
- No test runs a real agent, reads the real `~/.claude`, or touches the network.
- `docs/HUMAN_CHECKS.md`: the bar in dark and light, the right-click menu, the toggle key, the Git step-through, and real Claude and Codex limits after one turn each.

## 10. Docs

- ADR 0102: decision, options (bar vs title-bar chips; UI vs daemon machine; status line vs OAuth usage endpoint vs nothing for Claude), consequences; amends 0054.
- SPEC: §10 gets the bar, §13 the key, the protocol table the two events and two requests, §20E the limits.
- README: the widget bar and the Claude status line note.

## 11. Out of scope

- A bar in detached windows.
- Limits for omp or other agents.
- Reading the Claude OAuth usage endpoint.
- User-defined widgets or reordering.
- Per-session cost.
