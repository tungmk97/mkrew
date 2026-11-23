# Tern layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One full-width top bar that holds the terminal tabs, no bottom bar, a slim title-less header on every pane, and the agents' usage limits as a chip that only shows at 80% or more.

**Architecture:** The UI-only change moves the existing tab strip (`render_tab_bar`) into the title bar (`render_title_bar`) and removes the view switch, the widget bar and several title bar buttons; the tab model, session persistence and the `agent.limits` path stay as they are. The daemon's CPU / RAM sampler and `Event::MachineStats` go (protocol 58), since nothing shows them any more. Detached windows reuse the same title bar function.

**Tech Stack:** Rust (edition 2024), gpui-pre 0.3.7, serde / toml, tokio (daemon), insta (theme goldens only).

**Spec:** `docs/superpowers/specs/2026-11-23-tern-layout-design.md` (commit 7cf71e0). Executors read it and this plan.

## Global Constraints

- Work in the worktree `C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/tern-layout` on branch `feat/tern-layout` (base master 7cf71e0). Use absolute paths under it. Never touch the main checkout.
- Commit with `git -c user.email=tungnv975@gmail.com commit -m "<Conventional Commit>"`, only the files of the task, never push, never rewrite history.
- Every cargo command is prefixed `CARGO_BUILD_JOBS=4`, run with the worktree as `cwd`. C: is low on disk; do not kill processes you did not start; never touch the installed Mkrew.
- Verify with `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL). If it passes 600 s, run `CARGO_BUILD_JOBS=4 cargo test --workspace` separately. Intermediate tasks run only the focused commands they list.
- `PROTOCOL_VERSION` 57 -> 58 (checked on `master` and `feat/chat-look-2`). ADR 0106 (0103-0105 taken). ADR 0106 supersedes the bar part of ADR 0102.
- Title bar height stays `TITLE_BAR_HEIGHT` (42). New token `TAB_MIN_WIDTH`. `TAB_TITLE_MAX_WIDTH` (220) stays the maximum.
- Limits chip: none below 80%; `theme.status_needs_input` at 80-94%; `theme.danger_text` at 95% and more (on the rounded percent); highest window wins, ties go to the later reset; always icon + text; click opens `settings_dialog::INTEGRATIONS_TAB`.
- Retired settings keys `[ui] widget_bar` and `[ui] widgets_hidden`: accepted, ignored, never written.
- A name the user gave a tab wins over the focused pane's title.
- UI rules (SPEC §14, `mkrew-ui-design`): colors, spacing (4/8/12/16/24/32), radius and font sizes only from `theme.rs` tokens; hover, press and focus only through widgets in `shell/widgets.rs` or `.pressable(Look::..)` + `.on_press(..)`, never `.hover(`, `.active(` or `.cursor_pointer()` elsewhere; dark and light themes; tooltips through `tooltip(..)` with the shortcut.
- No `unwrap`/`expect` outside tests. `thiserror` in libraries, `anyhow` in binaries, `tracing` for logs.
- No test runs a real agent or touches the network. No test reads source text. Docs: no em dash, one sentence per line in long Markdown. Never edit CHANGELOG.md.
- Deviating from SPEC needs the ADR and the SPEC update in the same change set (Task 7).

## Review Focus

Input classes the spec implies but its tests do not name, most likely first. Each has a test in the task named in brackets.

- A hand-edited or older `settings.toml` that still has `widget_bar` / `widgets_hidden` (any TOML type), next to a truly unknown key. Expect: the retired keys load and are ignored, nothing is written back, the unknown key is still refused. [Task 2]
- Odd limit data: a window whose reset passed, two windows at the same percent, a window without a reset time, a percent that rounds up to 80 or 95. Expect: expired windows are ignored, the later reset wins a tie, a window without a reset loses a tie, the level follows the rounded percent. [Task 2]
- Tab names when panes change: a default-named tab whose focus moves between panes, a user-named tab, an empty tab, a focused run pane on double-click. Expect: the default-named tab follows the focused pane, the user's name always wins, an empty tab shows its own name, a double-click on a run pane renames the tab. [Task 3]
- Many tabs in a narrow (1100 px) window, and the active tab changing to one that is scrolled out of view. Expect: the active chip (or the shown view chip) is the scroll target, scrolled into view once so the wheel still works afterwards, chips shrink to `TAB_MIN_WIDTH` and `+ ▾` stays visible (smoke). [Task 4]
- The running session count crossing `RUNNING_WARN_ABOVE` several times (rise above, stay above, drop to it, rise again), in two workspaces. Expect: one toast per upward crossing per workspace, none while it stays above. [Task 5]

---

## File structure

| File | Responsibility |
| --- | --- |
| `crates/mkrew-protocol/src/{lib.rs,message.rs}` (modify) | `PROTOCOL_VERSION = 58`; `MachineStats` and `Event::MachineStats` removed |
| `crates/mkrew-daemon/src/machine.rs` (delete), `lib.rs` / `server.rs` / `session.rs` (modify) | sampler and its client count removed |
| `crates/mkrew-daemon/tests/daemon/widgets.rs` -> `limits.rs` (rename) | limits tests only |
| `crates/mkrew-core/src/{settings.rs,settings_fields.rs}` (modify) | retired `[ui]` keys, Settings field removed |
| `crates/mkrew-ui/src/limits.rs` (create) | limits store, `Level`, `Chip`, `chip`, time helpers |
| `crates/mkrew-ui/src/widget_bar.rs`, `shell/widget_bar.rs` (delete) | widget bar |
| `crates/mkrew-ui/src/shell/limits.rs` (create) | `render_limits_chip`, `load_limits`, `local_offset` |
| `crates/mkrew-ui/src/{keymap,palette,redraw,theme,sidebar,git_view}.rs` (modify) | `ToggleWidgetBar`, widget bar tokens and rules removed; `TAB_MIN_WIDTH` |
| `crates/mkrew-ui/src/{tabs.rs,pane_text.rs}` (modify) | `session_label` from the focused pane, `double_click_renames`, `header_room` without title |
| `crates/mkrew-ui/src/shell/pane_render.rs` (modify) | pane header on every pane, `render_tab_area`, `tab_area_frame`, Tern chip look, merged `+ ▾` |
| `crates/mkrew-ui/src/shell/chrome.rs` (modify) | new `render_title_bar(host, ..)`, view chips, Board / Docs icons, removed buttons |
| `crates/mkrew-ui/src/shell/{sessions,tabs,workspace,pane_drop,focus,connection,toasts}.rs` (modify) | strip notices to toasts |
| `crates/mkrew-ui/src/lifecycle.rs` (modify) | `running_alert` crossing rule |
| `crates/mkrew-ui/src/shell/detached_render.rs` (modify) | detached windows on the shared title bar |
| `docs/decisions/0106-tern-layout.md` (create), `docs/SPEC.md`, `docs/HUMAN_CHECKS.md`, `README.md`, `docs/PROGRESS.md`, `docs/screenshots/` | docs and screenshots |

Tasks in dependency order: 1 protocol and daemon, 2 widget bar removal and limits chip, 3 pane header and tab title, 4 tabs in the title bar, 5 merged `+ ▾` and toasts, 6 detached windows, 7 docs, smoke and verify.
Each task ends committable with its own tests green.

---

### Task 1: Protocol 58 and daemon machine stats removal

All line numbers are master 7cf71e0. Several steps edit the same file twice or delete lines: apply the edits of one file from the bottom of the file upward, or find each edit by the quoted text, so earlier edits never shift later ones.

**Files:**
- Modify: `crates/mkrew-protocol/src/lib.rs:16`
- Modify: `crates/mkrew-protocol/src/message.rs:479-481` (variant), `:1804-1813` (struct)
- Modify (test): `crates/mkrew-protocol/src/message/tests.rs:1337`, `:1350-1360`, `:1405-1408`
- Delete: `crates/mkrew-daemon/src/machine.rs`
- Modify: `crates/mkrew-daemon/src/lib.rs:20`
- Modify: `crates/mkrew-daemon/src/server.rs:33`, `:133`, `:140`, `:205`, `:244-247`
- Modify: `crates/mkrew-daemon/src/session.rs:113-115`, `:352` (the `Hub.clients` counter only the sampler read)
- Rename + modify (test): `crates/mkrew-daemon/tests/daemon/widgets.rs` -> `crates/mkrew-daemon/tests/daemon/limits.rs`; `crates/mkrew-daemon/tests/daemon/main.rs:52,62` (`mod` line)
- Modify (UI compile fixes, Task 2 deletes most of it again): `crates/mkrew-ui/src/widget_bar.rs`, `crates/mkrew-ui/src/shell/widget_bar.rs`, `crates/mkrew-ui/src/redraw.rs:100-101,136,538-558`, `crates/mkrew-ui/src/sidebar.rs:672`, `crates/mkrew-ui/src/shell/connection.rs:750-752`

**Interfaces:**
- Consumes: nothing new.
- Produces: `PROTOCOL_VERSION = 58`; `Event` without `MachineStats`; no `mkrew_protocol::MachineStats`; `WidgetId` without `Machine`; `WidgetData` without machine stats (`apply` takes only `Event::AgentLimits`). `sysinfo` stays a dependency of the daemon (`ports.rs` uses it).

- [ ] **Step 1: Write the failing version test and drop the machine half of the wire test**

In `crates/mkrew-protocol/src/message/tests.rs`, replace lines 1405-1408:

```rust
#[test]
fn the_tern_layout_protocol_is_version_58() {
    assert_eq!(crate::PROTOCOL_VERSION, 58);
}
```

Delete lines 1350-1360 (the `Event::MachineStats` assertion, from `assert_eq!(` to its closing `);`), and rename line 1337:

```rust
fn limits_have_their_own_wire_shapes() {
```

- [ ] **Step 2: Run it and see it fail**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-protocol the_tern_layout_protocol_is_version_58`
Expected: FAIL, `assertion `left == right` failed` with `left: 57` and `right: 58`.

- [ ] **Step 3: Protocol: bump the version and remove the event and its struct**

`crates/mkrew-protocol/src/lib.rs` line 16:

```rust
pub const PROTOCOL_VERSION: u32 = 58;
```

`crates/mkrew-protocol/src/message.rs`: delete lines 1804-1813 (the doc comment `/// CPU and RAM of the daemon's machine (spec 2026-11-22 section 6).`, the derive, `pub struct MachineStats { .. }` and the blank line after it), then delete lines 479-481:

```rust
    /// CPU and RAM of the daemon's machine, sent while a client is connected (section 6).
    #[serde(rename = "machine.stats")]
    MachineStats(MachineStats),
```

- [ ] **Step 4: Daemon: delete the sampler and the client counter**

Delete the file: `git rm crates/mkrew-daemon/src/machine.rs` (its tests go with it: `should_send`, `ClientGuard`, the two sampler tests).

`crates/mkrew-daemon/src/lib.rs`: delete line 20, `mod machine;`.

`crates/mkrew-daemon/src/session.rs`: delete line 352, `            clients: tokio::sync::watch::channel(0).0,`, then delete lines 113-115:

```rust
    /// Connections that said `hello` and receive events; the machine sampler runs while it is
    /// above 0 ([`crate::machine`]).
    pub clients: tokio::sync::watch::Sender<usize>,
```

`crates/mkrew-daemon/src/server.rs` (bottom-up):

Replace lines 244-247 (inside `if conn.greeted && forwarder.is_none() {`):

```rust
                    forwarder = Some(AbortOnDrop(tokio::spawn(forward(events, out.clone()))));
```

Replace line 205:

```rust
        let mut forwarder: Option<AbortOnDrop> = None;
```

Delete line 140 (`        machine.abort();`), line 133 (`        let machine = machine::start(Arc::clone(&self.hub));`) and line 33 (`use crate::machine;`).

- [ ] **Step 5: Daemon test: rename `widgets.rs` to `limits.rs` and drop the machine test**

Run: `git mv crates/mkrew-daemon/tests/daemon/widgets.rs crates/mkrew-daemon/tests/daemon/limits.rs`

In `crates/mkrew-daemon/tests/daemon/limits.rs` replace line 1:

```rust
//! Agent limits (spec 2026-11-22 section 5) over the real socket.
```

and delete lines 97-115 (the blank line and `fn machine_stats_arrive_while_a_client_is_connected`), so the file ends with the closing brace of `a_percentage_beyond_the_scale_is_clamped` (line 96).

`crates/mkrew-daemon/tests/daemon/main.rs`: delete line 62 (`mod widgets;`), then insert after line 52 (`mod io;`):

```rust
mod limits;
```

- [ ] **Step 6: UI compile fix, model: remove the machine widget from `crates/mkrew-ui/src/widget_bar.rs`**

Apply bottom-up. Test module first:

Replace lines 943-959 (`every_widget_has_a_menu_row_state_and_a_title`; keep the `#[test]` on 942):

```rust
    fn every_widget_has_a_menu_row_state_and_a_title() {
        let hidden = vec!["limits".to_owned(), "clock".to_owned()];
        let rows: Vec<(&str, bool)> = WidgetId::ALL
            .into_iter()
            .map(|id| (id.title(), !is_hidden(&hidden, id)))
            .collect();
        assert_eq!(
            rows,
            [
                ("Folder", true),
                ("Git branch", true),
                ("Agent limits", false),
                ("Version", true)
            ]
        );
    }
```

Replace lines 933-939 (the `clear_limits` tail of `widget_data_keeps_the_newest_limits_and_reports_changes`; line 940 `}` stays):

```rust
        data.clear_limits();
        assert!(data.limits().is_empty());
```

Delete lines 921-924 (`let one = stats(..)` through `assert!(data.apply(&Event::MachineStats(stats(2.0, 1.0, 2.0))));`).

Replace lines 886-898 (`the_folder_and_git_sit_left_and_the_rest_right`, body and closing brace; the fn line is 886):

```rust
    fn the_folder_and_git_sit_left_and_the_rest_right() {
        let sides: Vec<Side> = WidgetId::ALL.into_iter().map(WidgetId::side).collect();
        assert_eq!(sides, [Side::Left, Side::Left, Side::Right, Side::Right]);
    }
```

Delete lines 868-884 (`a_cpu_reading_that_is_not_a_number_is_left_out_and_ram_stays` with its `#[test]` and the blank line after).

Replace lines 797-821 (`a_narrow_window_drops_limits_first_then_the_machine`):

```rust
    fn a_narrow_window_drops_limits() {
        let list = full();
        let em = 13.0;
        let all = list_width(&list, em);
        assert_eq!(fit(list.clone(), f32::MAX, em), list);
        let no_limits: Vec<Widget> = list
            .iter()
            .filter(|w| w.id != WidgetId::Limits)
            .cloned()
            .collect();
        assert!(list_width(&no_limits, em) < all);
        assert_eq!(fit(list.clone(), all - 1.0, em), no_limits);
        // Folder, git and version stay however narrow.
        assert_eq!(
            ids(&fit(list, 0.0, em)),
            [WidgetId::Folder, WidgetId::Git, WidgetId::Version]
        );
    }
```

Replace line 793:

```rust
        assert_eq!(toggled(&[], WidgetId::Version), ["version"]);
```

Replace lines 767-776 (inside `hidden_widgets_are_left_out_and_unknown_ids_are_ignored`):

```rust
            let data = data(vec![limits(Cli::Claude, window(1.0, Some(60)), None)]);
            list(Some(&git), &data, &hidden, Some(("api", "/api")))
        };
        assert_eq!(ids(&widgets), [WidgetId::Folder, WidgetId::Version]);
```

Delete lines 747-759 (`ram_is_left_out_when_the_os_reports_no_total` with its `#[test]` and the blank line after).

Replace line 742:

```rust
        let data = self::data(vec![limits(Cli::Codex, window(50.0, Some(60)), None)]);
```

Replace line 740:

```rust
        let data = data(vec![limits(Cli::Omp, window(50.0, Some(60)), None)]);
```

Replace line 710:

```rust
            let data = data(vec![limits(Cli::Claude, five, weekly)]);
```

Replace lines 680-683:

```rust
            let data = data(vec![limits(Cli::Claude, window(percent, Some(600)), None)]);
```

Replace lines 626-630 (the `list[3]` machine tooltip and the `list[4]` version tooltip):

```rust
        assert_eq!(list[3].tooltip, "Mkrew 0.5.3\nClick: check for updates");
```

Replace lines 581-591:

```rust
        let clicks: Vec<Click> = list.iter().map(|w| w.click).collect();
        assert_eq!(
            clicks,
            [
                Click::OpenFolder,
                Click::Git,
                Click::OpenLimits,
                Click::CheckUpdate
            ]
        );
```

Delete line 577 (`                "CPU 23% · RAM 11.2/31.8 GB",`).

Replace lines 541-555 (inside `full()`):

```rust
        let data = data(vec![
            limits(
                Cli::Claude,
                window(42.4, Some(7_800)),
                window(18.0, Some(3 * 86_400)),
            ),
            limits(
                Cli::Codex,
                window(10.0, Some(4 * 3600)),
                window(3.0, Some(6 * 86_400)),
            ),
        ]);
```

Delete line 524 (`                machine_name: "This computer",`).

Replace lines 496-511 (`stats` and the two-argument `data`):

```rust
    fn data(list: Vec<AgentLimits>) -> WidgetData {
        let mut data = WidgetData::default();
        data.load_limits(list);
        data
    }
```

Delete line 464 (`    const GIB: f64 = 1_073_741_824.0;`).

Then the code above the test module, still bottom-up:

Replace lines 394-404 (`fit` and its doc):

```rust
/// Drops the limits widget while `list` is wider than `available` px. Folder, git and version
/// stay.
pub fn fit(mut list: Vec<Widget>, available: f32, em: f32) -> Vec<Widget> {
    if list_width(&list, em) > available {
        list.retain(|widget| widget.id != WidgetId::Limits);
    }
    list
}
```

Delete lines 319-353 (`fn machine`, the blank line, `fn gib` and the blank line after it).

Delete line 224 (`            WidgetId::Machine => machine(input),`).

Delete lines 205-206 (`/// "This computer", or the machine window's name.` and `pub machine_name: &'a str,`).

Delete lines 178-181 (the blank line and `pub fn machine(&self) -> Option<&MachineStats>`).

Replace lines 163-173 (`clear_limits` and `clear_machine`):

```rust
    /// The new connection's list is about to replace the limits (events since the connect
    /// stay in the daemon's answer).
    pub fn clear_limits(&mut self) {
        self.limits.clear();
    }
```

Replace lines 138-153 (the struct and `apply`):

```rust
/// What the daemon told the bar: the newest limits per agent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WidgetData {
    limits: Vec<AgentLimits>,
}

impl WidgetData {
    /// Takes in `agent.limits`; returns whether anything changed.
    pub fn apply(&mut self, event: &Event) -> bool {
        match event {
            Event::AgentLimits(limits) => self.offer(limits.clone()),
            _ => false,
        }
    }
```

Delete lines 98-99 (`    /// No click, no pointer (the machine widget).` and `    None,`).

Replace line 60:

```rust
            WidgetId::Limits | WidgetId::Version => Side::Right,
```

Delete line 52 (`            WidgetId::Machine => "CPU and RAM",`), line 41 (`            WidgetId::Machine => "machine",`) and line 31 (`        WidgetId::Machine,`).

Replace lines 26-27:

```rust
    /// Bar order: folder and git on the left, limits and version on the right.
    pub const ALL: [WidgetId; 4] = [
```

Delete line 15 (`    Machine,`), replace line 9 with `/// The four widgets of the bar.` and replace line 5:

```rust
use mkrew_protocol::{AgentLimits, Cli, Event, LimitWindow};
```

- [ ] **Step 7: UI compile fix, shell, redraw, sidebar, connection**

`crates/mkrew-ui/src/shell/widget_bar.rs` (bottom-up): delete line 225 (`            Click::None => {}`); delete lines 205-207 (`if click == Click::None { return element.into_any_element(); }`); replace lines 156-157 with

```rust
    /// One widget: an icon and its label in a `pressable` element (hover, press and focus from
    /// `widgets.rs`), with its tooltip.
```

delete line 96 (`            machine_name: self.machine_name().unwrap_or("This computer"),`); delete line 72 (`        WidgetId::Machine => Icon::Server,`).

`crates/mkrew-ui/src/sidebar.rs`: delete line 672 (`            | Event::MachineStats(_)`).

`crates/mkrew-ui/src/shell/connection.rs`: delete lines 750-752:

```rust
                // The load figures belong to the daemon that went away; the limits stay
                // until the new connection's list replaces them (`reset_connection_state`).
                shell.widget_data.clear_machine();
```

`crates/mkrew-ui/src/redraw.rs` (bottom-up): replace lines 538-558 (the test; `#[test]` is line 538):

```rust
    #[test]
    fn limits_redraw_only_while_the_bar_is_drawn() {
        let w = world();
        let mut s = terminals(&w);
        let limits = Event::AgentLimits(AgentLimits {
            agent: Cli::Claude,
            five_hour: None,
            weekly: None,
            observed_at: 1,
        });
        assert!(!redraws(&w, &s, &limits), "bar hidden");
        s.widget_bar = true;
        assert!(redraws(&w, &s, &limits));
    }
```

Line 136: `        DocComment, SessionExitedEvent, SessionKind, SessionPlanEvent,`. Lines 100-101:

```rust
        // The widget bar's limits (ADR 0102).
        Event::AgentLimits(_) => shown.widget_bar,
```

- [ ] **Step 8: Run the tests and see them pass**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-protocol`
Expected: PASS, including `the_tern_layout_protocol_is_version_58` and `limits_have_their_own_wire_shapes`.

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-daemon --lib`
Expected: PASS (the machine tests are gone with the file).

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-daemon --test daemon limits`
Expected: PASS, 3 tests (`a_report_is_listed_and_announced_and_the_newest_one_wins`, `only_claude_and_codex_may_report_limits`, `a_percentage_beyond_the_scale_is_clamped`).

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib widget_bar redraw`
Expected: PASS. (If cargo rejects two filters, run `widget_bar` and `redraw` as two commands.)

- [ ] **Step 9: Clippy**

Run: `CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-protocol -p mkrew-daemon -p mkrew-cli -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings. `cargo fmt --all` first if it complains about the shortened lines.

- [ ] **Step 10: Commit**

```bash
git add crates/mkrew-protocol crates/mkrew-daemon crates/mkrew-ui/src/widget_bar.rs crates/mkrew-ui/src/shell/widget_bar.rs crates/mkrew-ui/src/redraw.rs crates/mkrew-ui/src/sidebar.rs crates/mkrew-ui/src/shell/connection.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(protocol)!: protocol 58, remove machine.stats and the daemon sampler"
```

**Risks for the reviewer:**
- The daemon test suite no longer covers a client that connects and stays: the `Hub.clients` counter and the `STAY` delay were only for the sampler, so nothing replaces their timing tests.
- A UI that speaks 57 against this daemon gets `version_mismatch` on `hello` (covered by the existing `PROTOCOL_VERSION + 1` test in `basics.rs`, not re-run on a real 57 build).
- The widget bar model edits are bookkeeping that Task 2 deletes; a leftover `MachineStats` mention outside the files above would break `cargo clippy`, which is the check for it.

---

### Task 2: Widget bar removed, limits chip in the title bar, retired settings keys

Line numbers are master 7cf71e0. Files that Task 1 already touched (`redraw.rs`, `sidebar.rs`, `shell/connection.rs`) have shifted by a few lines: find those edits by the quoted text. Within one file apply edits bottom-up.

**Files:**
- Create: `crates/mkrew-ui/src/limits.rs`
- Create: `crates/mkrew-ui/src/shell/limits.rs`
- Delete: `crates/mkrew-ui/src/widget_bar.rs`, `crates/mkrew-ui/src/shell/widget_bar.rs`
- Modify: `crates/mkrew-ui/src/lib.rs:64,112`; `crates/mkrew-ui/src/shell.rs:52,73`; `crates/mkrew-ui/Cargo.toml:24`
- Modify: `crates/mkrew-ui/src/main.rs:432-438,593-594,710-716,1308`
- Modify: `crates/mkrew-ui/src/shell/connection.rs:48,362,426-428,902,1083-1084,1220,1260`; `shell/focus.rs:126`; `shell/menu_render.rs:326,364-366`; `shell/sessions.rs:507`; `shell/composer.rs:1134`; `shell/chrome.rs:298,394`; `shell/git.rs:677`
- Modify: `crates/mkrew-ui/src/keymap.rs:213-215,383,497,610,738,1583-1591`; `palette.rs:688,712`; `palette/tests.rs:401,416-432`; `detached/tests.rs:246`
- Modify: `crates/mkrew-ui/src/theme.rs:2348-2355,2511-2515,2714-2720,3151-3161`
- Modify: `crates/mkrew-ui/src/redraw.rs` (`Shown.widget_bar`, its arm, constructors, test)
- Modify: `crates/mkrew-ui/src/settings_dialog.rs:55,741,2511,2676`; `shell/settings.rs:887-888`
- Modify: `crates/mkrew-ui/src/git_view.rs:441-443,1476,1599-1617`; `git_view/tests.rs:589-608` (`Key::NextWrapping` only served the Git widget)
- Modify: `crates/mkrew-core/src/settings.rs:449-455,478-479,561-567,645-646,665-666,940-942,1131-1160`; `settings_fields.rs:57-58,110-122,290-299,514,675-683`

**Interfaces:**
- Consumes: `mkrew_protocol::{AgentLimits, Cli, Event, LimitWindow}`; `AgentLimits::supersedes`; `settings_dialog::INTEGRATIONS_TAB`; `Shell::open_settings_dialog`; `status::now_ms`.
- Produces (contract names):
  - `mkrew_ui::limits::Limits` with `apply(&mut self, &Event) -> bool`, `load(&mut self, Vec<AgentLimits>) -> bool`, `clear(&mut self)`, `all(&self) -> &[AgentLimits]`.
  - `mkrew_ui::limits::Level { Warn, Danger }`, `Level::of(percent: u32) -> Option<Level>`.
  - `mkrew_ui::limits::Chip { text, level, tooltip }` and `chip(limits: &[AgentLimits], now_secs: i64, utc_offset: impl Fn(i64) -> i32) -> Option<Chip>`.
  - `mkrew_ui::limits::{in_text, local_clock}` (moved unchanged).
  - Shell field `limits: mkrew_ui::limits::Limits`; `Shell::render_limits_chip(&self, theme: &Theme, cx: &mut Context<'_, Self>) -> Option<gpui::AnyElement>` in `shell/limits.rs`; `shell::limits::local_offset(at: i64) -> i32`.
  - Retired `[ui] widget_bar` and `widgets_hidden`: accepted by `UiSettings`, ignored, never written.

- [ ] **Step 1: Create `limits.rs` with the moved store and helpers (no chip yet)**

Create `crates/mkrew-ui/src/limits.rs`:

```rust
//! The agents' usage limits (spec 2026-11-23 section 5.2, ADR 0106; the `agent.limits` path is
//! ADR 0102): the store that events and `limits.get` fill, and the chip the title bar draws from
//! it. Pure and tested headlessly; `shell/limits.rs` draws it.

use mkrew_protocol::{AgentLimits, Event};

/// The newest limits per agent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Limits {
    all: Vec<AgentLimits>,
}

impl Limits {
    /// Takes in `agent.limits`; returns whether anything changed.
    pub fn apply(&mut self, event: &Event) -> bool {
        match event {
            Event::AgentLimits(limits) => self.offer(limits.clone()),
            _ => false,
        }
    }

    /// The answer to `limits.get`, merged with the rule the daemon uses, so it never undoes a
    /// newer event that arrived first.
    pub fn load(&mut self, listed: Vec<AgentLimits>) -> bool {
        listed
            .into_iter()
            .fold(false, |changed, limits| self.offer(limits) | changed)
    }

    /// The new connection's list is about to replace these (events since the connect stay in
    /// the daemon's answer).
    pub fn clear(&mut self) {
        self.all.clear();
    }

    pub fn all(&self) -> &[AgentLimits] {
        &self.all
    }

    fn offer(&mut self, new: AgentLimits) -> bool {
        match self.all.iter_mut().find(|old| old.agent == new.agent) {
            Some(old) if new.supersedes(old) => {
                *old = new;
                true
            }
            Some(_) => false,
            None => {
                self.all.push(new);
                true
            }
        }
    }
}

/// `in 50 min`, `in 2 h 10 min`, `in 3 d 4 h` (rounded up to the minute); `now` when due.
pub fn in_text(secs: i64) -> String {
    if secs <= 0 {
        return "now".to_owned();
    }
    let mins = (secs + 59) / 60;
    if mins < 60 {
        return format!("in {mins} min");
    }
    if mins < 48 * 60 {
        return match (mins / 60, mins % 60) {
            (hours, 0) => format!("in {hours} h"),
            (hours, rest) => format!("in {hours} h {rest} min"),
        };
    }
    match (mins / 1440, mins % 1440 / 60) {
        (days, 0) => format!("in {days} d"),
        (days, hours) => format!("in {days} d {hours} h"),
    }
}

/// The local clock time of Unix time `at`: `04:07` on `now`'s local day, else `Fri 14:41`.
/// `utc_offset(t)` is the local offset in seconds east of UTC at Unix time `t`; `now`'s day
/// uses the offset at `now`, so a daylight saving change between the two does not move it.
pub fn local_clock(at: i64, now: i64, utc_offset: &dyn Fn(i64) -> i32) -> String {
    let local = at + i64::from(utc_offset(at));
    let today = (now + i64::from(utc_offset(now))).div_euclid(86_400);
    let (day, rest) = (local.div_euclid(86_400), local.rem_euclid(86_400));
    let clock = format!("{:02}:{:02}", rest / 3600, rest % 3600 / 60);
    if day == today {
        return clock;
    }
    // Day 0 (1970-01-01) was a Thursday.
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    format!("{} {clock}", DAYS[day.rem_euclid(7) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_protocol::{Cli, LimitWindow};

    const NOW: i64 = 1_778_519_153;

    fn plus_seven(_: i64) -> i32 {
        25_200
    }

    fn window(percent: f32, resets_in: Option<i64>) -> Option<LimitWindow> {
        Some(LimitWindow {
            used_percent: percent,
            resets_at: resets_in.map(|s| NOW + s),
        })
    }

    fn limits(agent: Cli, five: Option<LimitWindow>, weekly: Option<LimitWindow>) -> AgentLimits {
        AgentLimits {
            agent,
            five_hour: five,
            weekly,
            observed_at: NOW,
        }
    }

    #[test]
    fn the_store_keeps_the_newest_limits_and_reports_changes() {
        let mut store = Limits::default();
        let old = limits(Cli::Claude, window(10.0, Some(60)), None);
        let newer = AgentLimits {
            observed_at: NOW + 1,
            ..limits(Cli::Claude, window(20.0, Some(60)), None)
        };
        assert!(store.apply(&Event::AgentLimits(old.clone())));
        assert!(!store.apply(&Event::AgentLimits(old.clone())), "a repeat");
        assert!(store.apply(&Event::AgentLimits(newer.clone())));
        assert!(
            !store.apply(&Event::AgentLimits(old.clone())),
            "an older event"
        );
        assert!(
            !store.load(vec![old]),
            "a late answer never undoes a newer event"
        );
        assert_eq!(store.all(), [newer]);
        assert!(
            !store.apply(&Event::SessionState(mkrew_protocol::SessionStateEvent {
                session_id: ulid::Ulid::nil(),
                state: mkrew_protocol::SessionState::Idle,
                source: mkrew_protocol::StateSource::Daemon,
                changed_at: 0,
            }))
        );
        store.clear();
        assert!(store.all().is_empty());
    }

    #[test]
    fn durations_read_as_minutes_hours_and_days() {
        for (secs, text) in [
            (0, "now"),
            (-30, "now"),
            (1, "in 1 min"),
            (60, "in 1 min"),
            (61, "in 2 min"),
            (3_000, "in 50 min"),
            (7_200, "in 2 h"),
            (7_800, "in 2 h 10 min"),
            (172_800, "in 2 d"),
            (190_800, "in 2 d 5 h"),
            (259_200, "in 3 d"),
        ] {
            assert_eq!(in_text(secs), text, "{secs}");
        }
    }

    #[test]
    fn reset_times_are_local_and_name_the_day_when_it_is_not_today() {
        // 2026-05-12 00:05 local (UTC+7) now; the 5-hour window resets 04:07 the same day.
        assert_eq!(local_clock(1_778_533_631, NOW, &plus_seven), "04:07");
        // Three days on: Friday.
        assert_eq!(
            local_clock(1_778_571_719 + 3 * 86_400, NOW, &plus_seven),
            "Fri 14:41"
        );
        // The same instant in UTC is still the previous day for `now`.
        assert_eq!(local_clock(1_778_533_631, NOW, &|_| 0), "21:07");
        assert_eq!(local_clock(0, 0, &|_| 0), "00:00");
    }

    /// `now` is judged by the offset at `now`: a clock that moves forward an hour between `now`
    /// (23:59 on day 999) and the reset (01:30 on day 1000) still calls the reset tomorrow.
    #[test]
    fn today_uses_the_offset_at_now_across_a_daylight_saving_change() {
        let change = 1_000 * 86_400;
        let offset = |t: i64| if t < change { 0 } else { 3_600 };
        assert_eq!(
            local_clock(change + 1_800, change - 60, &offset),
            "Wed 01:30"
        );
    }
}
```

Register it. `crates/mkrew-ui/src/lib.rs`: insert after line 64 (`pub mod lifecycle;`):

```rust
pub mod limits;
```

- [ ] **Step 2: Run the moved tests**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib limits::`
Expected: PASS, 4 tests.

- [ ] **Step 3: Write the failing chip tests**

In `crates/mkrew-ui/src/limits.rs`, change the tests' import to `use mkrew_protocol::{Cli, LimitWindow};` (unchanged) and append before the closing `}` of `mod tests`:

```rust
    fn chip_of(list: &[AgentLimits]) -> Option<Chip> {
        chip(list, NOW, plus_seven)
    }

    fn text_of(list: &[AgentLimits]) -> Option<String> {
        chip_of(list).map(|chip| chip.text)
    }

    #[test]
    fn the_chip_is_amber_from_80_and_red_from_95_on_the_rounded_percent() {
        let at = |percent: f32| {
            chip_of(&[limits(Cli::Claude, window(percent, Some(600)), None)])
        };
        assert_eq!(at(79.4), None, "79% is below the line");
        assert_eq!(at(-5.0), None);
        assert_eq!(at(f32::NAN), None, "not a number: nothing to show");
        for (percent, text, level) in [
            (79.5, "Claude 5h 80%", Level::Warn),
            (80.0, "Claude 5h 80%", Level::Warn),
            (94.4, "Claude 5h 94%", Level::Warn),
            (94.5, "Claude 5h 95%", Level::Danger),
            (100.0, "Claude 5h 100%", Level::Danger),
            (150.0, "Claude 5h 100%", Level::Danger),
        ] {
            let chip = at(percent).unwrap_or_else(|| panic!("{percent} shows a chip"));
            assert_eq!((chip.text.as_str(), chip.level), (text, level), "{percent}");
        }
    }

    #[test]
    fn no_data_and_nothing_at_80_show_no_chip() {
        assert_eq!(chip_of(&[]), None);
        let calm = [
            limits(Cli::Claude, window(42.0, Some(600)), window(18.0, None)),
            limits(Cli::Codex, window(79.0, Some(600)), None),
        ];
        assert_eq!(chip_of(&calm), None);
        let omp = [limits(Cli::Omp, window(99.0, Some(600)), None)];
        assert_eq!(chip_of(&omp), None, "only Claude and Codex are listed");
    }

    #[test]
    fn the_highest_window_wins_across_agents() {
        let list = [
            limits(Cli::Claude, window(82.0, Some(600)), window(88.0, Some(600))),
            limits(Cli::Codex, window(91.0, Some(600)), window(10.0, None)),
        ];
        assert_eq!(text_of(&list).as_deref(), Some("Codex 5h 91%"));
        let chip = chip_of(&list).unwrap();
        assert_eq!(chip.level, Level::Warn);
    }

    #[test]
    fn a_tie_goes_to_the_window_that_resets_later() {
        let sooner = limits(Cli::Claude, window(90.0, Some(600)), None);
        let later = limits(Cli::Codex, None, window(90.0, Some(7_200)));
        for list in [
            [sooner.clone(), later.clone()],
            [later.clone(), sooner.clone()],
        ] {
            assert_eq!(text_of(&list).as_deref(), Some("Codex 7d 90%"));
        }
        let unknown = limits(Cli::Claude, window(90.0, None), None);
        let known = limits(Cli::Codex, window(90.0, Some(60)), None);
        assert_eq!(
            text_of(&[unknown, known]).as_deref(),
            Some("Codex 5h 90%"),
            "a known reset time beats an unknown one"
        );
    }

    #[test]
    fn a_window_past_its_reset_is_ignored_until_a_new_report() {
        let text = |five: Option<LimitWindow>, weekly: Option<LimitWindow>| {
            text_of(&[limits(Cli::Claude, five, weekly)])
        };
        assert_eq!(
            text(window(99.0, Some(-1)), window(85.0, Some(60))).as_deref(),
            Some("Claude 7d 85%"),
            "the expired 5h is not the top"
        );
        assert_eq!(
            text(window(99.0, Some(0)), window(85.0, Some(60))).as_deref(),
            Some("Claude 7d 85%"),
            "now is past"
        );
        assert_eq!(
            text(window(99.0, Some(-1)), window(90.0, Some(-5))),
            None,
            "both past: no chip"
        );
        assert_eq!(
            text(window(99.0, None), None).as_deref(),
            Some("Claude 5h 99%"),
            "no reset time: kept"
        );
        assert_eq!(text(None, None), None);
    }

    #[test]
    fn the_tooltip_lists_every_live_window_with_its_reset_time() {
        let list = [
            limits(Cli::Claude, window(84.0, Some(3_000)), window(40.0, None)),
            limits(Cli::Codex, window(10.0, Some(7_200)), window(95.0, Some(-1))),
        ];
        let chip = chip_of(&list).unwrap();
        assert_eq!(chip.text, "Claude 5h 84%", "the expired 95% is not the top");
        assert_eq!(
            chip.tooltip,
            "Claude 5-hour limit: 84% used, resets 00:55 (in 50 min)\n\
             Claude weekly limit: 40% used\n\
             Codex 5-hour limit: 10% used, resets 02:05 (in 2 h)\n\
             Click: open Settings"
        );
    }
```

- [ ] **Step 4: Run and see the compile failure**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib limits::`
Expected: FAIL to compile: `error[E0425]: cannot find function `chip` in this scope`, `error[E0433]: failed to resolve: use of undeclared type `Level``, `error[E0412]: cannot find type `Chip` in this scope`.

- [ ] **Step 5: Implement the chip**

In `crates/mkrew-ui/src/limits.rs`: change the import line to

```rust
use mkrew_protocol::{AgentLimits, Cli, Event, LimitWindow};
```

and insert after `impl Limits { .. }` (before `in_text`):

```rust
/// How loud the chip is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// `theme.status_needs_input` (amber): 80 to 94%.
    Warn,
    /// `theme.danger_text`: 95% and more.
    Danger,
}

impl Level {
    /// `percent` is the shown, rounded value, so 79.5 reads "80%" and is amber. Below 80 there
    /// is nothing to say.
    pub fn of(percent: u32) -> Option<Level> {
        match percent {
            95.. => Some(Level::Danger),
            80..=94 => Some(Level::Warn),
            _ => None,
        }
    }
}

/// What the title bar draws: `text` after a warning icon, colored by `level`, with `tooltip`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    /// `Claude 5h 84%`.
    pub text: String,
    pub level: Level,
    /// One line per live window with its reset time, then the click.
    pub tooltip: String,
}

/// One window whose number is still true.
struct Live<'a> {
    agent: &'static str,
    short: &'static str,
    long: &'static str,
    window: &'a LimitWindow,
    percent: u32,
}

impl Live<'_> {
    fn tip(&self, now_secs: i64, utc_offset: &dyn Fn(i64) -> i32) -> String {
        match self.window.resets_at {
            Some(resets) => format!(
                "{} {}: {}% used, resets {} ({})",
                self.agent,
                self.long,
                self.percent,
                local_clock(resets, now_secs, utc_offset),
                in_text(resets - now_secs)
            ),
            None => format!("{} {}: {}% used", self.agent, self.long, self.percent),
        }
    }
}

/// Claude's, then Codex's, 5-hour then weekly window, each one that has a number and has not
/// reset yet. Past its reset the number is stale: nothing until a new report.
fn live_windows(limits: &[AgentLimits], now_secs: i64) -> Vec<Live<'_>> {
    let mut live = Vec::new();
    for (cli, agent) in [(Cli::Claude, "Claude"), (Cli::Codex, "Codex")] {
        let Some(entry) = limits.iter().find(|l| l.agent == cli) else {
            continue;
        };
        for (short, long, window) in [
            ("5h", "5-hour limit", entry.five_hour.as_ref()),
            ("7d", "weekly limit", entry.weekly.as_ref()),
        ] {
            let Some(window) = window else {
                continue;
            };
            if window.used_percent.is_finite() && window.resets_at.is_none_or(|at| at > now_secs) {
                live.push(Live {
                    agent,
                    short,
                    long,
                    window,
                    percent: percent_of(window),
                });
            }
        }
    }
    live
}

/// The shown whole percentage: clamped to 0-100 and rounded.
fn percent_of(window: &LimitWindow) -> u32 {
    window.used_percent.clamp(0.0, 100.0).round() as u32
}

/// The chip for the title bar: the live window with the highest percent, only from 80% on; a
/// tie goes to the one that resets later (a window without a reset time loses a tie). `None`
/// when nothing is at 80%, so the chip never shows a value it does not have. `utc_offset(at)` is
/// the local offset in seconds at the Unix time `at` (for reset times).
pub fn chip(
    limits: &[AgentLimits],
    now_secs: i64,
    utc_offset: impl Fn(i64) -> i32,
) -> Option<Chip> {
    let live = live_windows(limits, now_secs);
    let (level, top) = live
        .iter()
        .filter_map(|w| Some((Level::of(w.percent)?, w)))
        .max_by_key(|(_, w)| (w.percent, w.window.resets_at.unwrap_or(i64::MIN)))?;
    let mut lines: Vec<String> = live.iter().map(|w| w.tip(now_secs, &utc_offset)).collect();
    lines.push("Click: open Settings".to_owned());
    Some(Chip {
        text: format!("{} {} {}%", top.agent, top.short, top.percent),
        level,
        tooltip: lines.join("\n"),
    })
}
```

- [ ] **Step 6: Run the chip tests and see them pass**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib limits::`
Expected: PASS, 10 tests.

- [ ] **Step 7: Write the failing tests for the retired keys, the settings page and the contrast rule**

`crates/mkrew-core/src/settings.rs`: replace lines 1131-1160 (the two tests `the_widget_bar_defaults_to_shown_and_keeps_ids_it_does_not_know` and `write_widgets_hidden_round_trips_and_keeps_the_rest_of_the_file`) with:

```rust
    /// ADR 0106: `widget_bar` and `widgets_hidden` belonged to the removed widget bar. A file
    /// that still has them loads, they change nothing and they are never written back.
    #[test]
    fn a_file_with_the_retired_widget_bar_keys_still_loads_and_never_writes_them_back() {
        let src = "[ui]\ntheme = \"dark\"\nwidget_bar = false\nwidgets_hidden = [\"git\", \"clock\"]\n";
        let settings = parse(Path::new("settings.toml"), src).unwrap();
        assert_eq!(
            settings.ui,
            UiSettings {
                theme: ThemeMode::Dark,
                ..UiSettings::default()
            }
        );
        let written = toml::Value::try_from(&settings).unwrap().to_string();
        assert!(!written.contains("widget"), "{written}");
        // Whatever the value, it is ignored.
        let odd = "[ui]\nwidget_bar = \"x\"\nwidgets_hidden = 3\n";
        assert_eq!(parse(Path::new("settings.toml"), odd).unwrap().ui, UiSettings::default());
        // Other unknown keys are still refused.
        assert!(parse_err("[ui]\nwidget_baz = true\n").contains("unknown field `widget_baz`"));
    }
```

`crates/mkrew-ui/src/settings_dialog.rs` line 2676:

```rust
        assert_eq!(titles, ["Theme", "Fonts", "Diffs", "Markdown"]);
```

`crates/mkrew-ui/src/theme.rs` lines 2714-2720 (the `"widget bar"` entry of `each_contrast_rule_fires_when_its_token_is_broken`):

```rust
            (
                "limits chip",
                Theme {
                    chrome: t.status_needs_input,
                    ..t.clone()
                },
            ),
```

`crates/mkrew-ui/src/redraw.rs`: replace the test `limits_redraw_only_while_the_bar_is_drawn` (written in Task 1) with:

```rust
    #[test]
    fn limits_always_redraw_the_main_window() {
        let w = world();
        let limits = Event::AgentLimits(AgentLimits {
            agent: Cli::Claude,
            five_hour: None,
            weekly: None,
            observed_at: 1,
        });
        for shown in [terminals(&w), nothing_shown(&w)] {
            assert!(redraws(&w, &shown, &limits));
        }
    }
```

- [ ] **Step 8: Run them and see them fail**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-core a_file_with_the_retired_widget_bar_keys`
Expected: FAIL: `the `widget_bar` key is written`, the assertion `!written.contains("widget")` (the fields still serialize).

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib rows_group_into_cards each_contrast_rule limits_always_redraw`
Expected: FAIL (if cargo takes only one filter, run each name separately): the titles still end with `"Window"`; `limits chip missing from [..]`; the limits event does not redraw while `Shown.widget_bar` is false.

- [ ] **Step 9: Core: retire the two keys in `UiSettings`, remove the write helper, the template lines and the Settings row**

`crates/mkrew-core/src/settings.rs` (bottom-up):

Replace lines 940-942 (inside the `UiSettings { .. }` literal of `full_file_parses`; line 940 `md_split: MD_SPLIT_DEFAULT,` stays, lines 941-942 are `widget_bar: true,` and `widgets_hidden: vec![],`) with:

```rust
                    ..UiSettings::default()
```

Delete lines 665-666 (`    # widget_bar = true\n\` and `    # widgets_hidden = []\n\`) and lines 645-646 (the two comment lines `# Widget bar: ...` and `# widgets_hidden lists ...`).

Delete lines 561-567 (`write_widgets_hidden`, its doc and the blank line after it).

Replace lines 478-479 in `impl Default for UiSettings`:

```rust
            retired_widget_bar: Retired,
            retired_widgets_hidden: Retired,
```

Replace lines 449-455 in the struct:

```rust
    /// `[ui] widget_bar` and `[ui] widgets_hidden` belonged to the widget bar that ADR 0106
    /// removed. `deny_unknown_fields` would refuse a file that still has them, so they are
    /// read, ignored and never written (`skip_serializing`).
    #[serde(rename = "widget_bar", skip_serializing)]
    retired_widget_bar: Retired,
    #[serde(rename = "widgets_hidden", skip_serializing)]
    retired_widgets_hidden: Retired,
```

Insert before line 407 (the doc of `UiSettings`):

```rust
/// A `[ui]` key that an older build wrote and this build ignores, whatever its value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Retired;

impl<'de> Deserialize<'de> for Retired {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        serde::de::IgnoredAny::deserialize(deserializer).map(|_| Retired)
    }
}

```

`crates/mkrew-core/src/settings_fields.rs` (bottom-up): delete lines 675-683 (the blank line and `the_widget_bar_switch_is_a_setting_and_its_hidden_list_is_not_shown`); delete line 514 (`            Kind::Strings => text.to_owned(),`); delete lines 290-299 (the `row( Appearance, "Window", "ui", "widget_bar", .. )` and `hidden_list("ui", "widgets_hidden"),`); delete lines 110-122 (`hidden_list` and the blank line after it); delete lines 57-58 (the `Strings` doc and variant).

- [ ] **Step 10: UI: drop the `Kind::Strings` arms and fix the Settings doc and test names**

`crates/mkrew-ui/src/settings_dialog.rs`: line 741 becomes

```rust
                Kind::Font | Kind::List(_) => Effect::None,
```

line 55 becomes `/// Where the limits chip opens Settings: the first page of the Integrations group.`, and line 2511 becomes `    fn the_limits_chip_opens_the_first_page_of_the_integrations_group() {`.

`crates/mkrew-ui/src/shell/settings.rs`: delete lines 887-888 (`            // Hidden rows are never drawn.` and `            Kind::Strings => div().into_any_element(),`).

- [ ] **Step 11: UI: remove the command, its bindings, palette entry and theme tokens; add the limits chip rule**

`crates/mkrew-ui/src/keymap.rs` (bottom-up): replace lines 1583-1591 with

```rust
        // Global like the sidebar toggles; `ctrl-shift-j` stays Toggle theme.
        assert_eq!(bound("ctrl-shift-j"), Some(Command::ToggleTheme));
```

delete line 738 (`            Command::ToggleWidgetBar => "ctrl-alt-shift-j".into(),`), line 610 (`            Command::ToggleWidgetBar => "cmd-shift-b",`), line 497 (`            Command::ToggleWidgetBar => "toggle_widget_bar".into(),`), line 383 (`            Command::ToggleWidgetBar,`) and lines 213-215 (the doc and `ToggleWidgetBar,`).

`crates/mkrew-ui/src/palette.rs`: delete line 712 (`        | Command::ToggleWidgetBar`) and line 688 (`        Command::ToggleWidgetBar => "View: Show/hide widget bar".into(),`).

`crates/mkrew-ui/src/palette/tests.rs`: delete lines 416-432 (`the_widget_bar_toggle_is_listed_without_a_workspace`, its `#[test]` and the blank line after it) and line 401 (`            Command::ToggleWidgetBar,`).

`crates/mkrew-ui/src/detached/tests.rs`: delete line 246 (`        Command::ToggleWidgetBar,`).

`crates/mkrew-ui/src/shell/sessions.rs`: delete line 507 (`            Command::ToggleWidgetBar => self.toggle_widget_bar(cx),`).

`crates/mkrew-ui/src/theme.rs` (bottom-up): delete lines 3151-3161 (`the_widget_bar_tokens_scale_and_sit_on_the_spacing_scale` and the blank line after it); the `"limits chip"` test entry is already in place from Step 7; replace lines 2511-2515:

```rust
    // The limits chip (ADR 0106) sits on `chrome` in the title bar in body text: 4.5:1 for the
    // amber (80 to 94%) and the red (95% and more).
    for fg in [t.status_needs_input, t.danger_text] {
        check("limits chip", fg, t.chrome, 4.5);
    }
```

delete lines 2348-2355 (the `WIDGET_BAR_*` doc and the four constants).

- [ ] **Step 12: UI: `redraw.rs`, `Key::NextWrapping`**

`crates/mkrew-ui/src/redraw.rs` (find by text; bottom-up): delete the two `            widget_bar: false,` lines in the `terminals` and `nothing_shown` helpers; delete the Task 1 comment and arm

```rust
        // The widget bar's limits (ADR 0102).
        Event::AgentLimits(_) => shown.widget_bar,
```

and add `| Event::AgentLimits(_)` to the `=> true` group so it reads

```rust
        // Rare, and the title bar's limits chip is always drawn in the main window (ADR 0106).
        Event::AgentLimits(_)
        | Event::SessionState(_)
```

(the group continues unchanged down to `| Event::GitChanged(_) => true,`); delete the field and its doc in `Shown`:

```rust
    /// The widget bar is drawn (`[ui] widget_bar`), so its limits and machine numbers are read.
    pub widget_bar: bool,
```

`crates/mkrew-ui/src/git_view.rs` (bottom-up): delete lines 1599-1617 (`step_file_wrapping`, its doc and the blank line after it); delete line 1476 (`            Key::NextWrapping => self.step_file_wrapping(),`); delete lines 441-443 (the doc and `NextWrapping,`).

`crates/mkrew-ui/src/git_view/tests.rs`: delete lines 589-608 (`the_widget_click_steps_to_the_next_file_and_wraps_at_the_end` and the blank line after it).

`crates/mkrew-ui/src/shell/git.rs` lines 677-678:

```rust
            git_view::Key::Up | git_view::Key::Down
                if requests.diff.is_some() || requests.show.is_some() =>
```

- [ ] **Step 13: UI shell: the chip, the module swap and the field renames**

Create `crates/mkrew-ui/src/shell/limits.rs`:

```rust
//! The limits chip of the title bar (spec 2026-11-23 section 5.2, ADR 0106) and the
//! `limits.get` read that fills it for a window that connects late.

use crate::*;
use mkrew_ui::limits::Level;

impl Shell {
    /// The limits the daemon holds, for a window that connects after they were reported;
    /// events keep them live afterwards.
    pub(crate) fn load_limits(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let listed = connection.call::<Vec<mkrew_protocol::AgentLimits>>(Request::LimitsGet);
        cx.spawn_in(window, async move |this, cx| {
            let listed = listed.await;
            let _ = this.update(cx, |shell, cx| match listed {
                Ok(listed) => {
                    if shell.limits.load(listed) {
                        cx.notify();
                    }
                }
                Err(error) => tracing::warn!("limits.get failed: {error:#}"),
            });
        })
        .detach();
    }
}

/// Seconds east of UTC at Unix time `at`, from the OS time zone (0 when it cannot say).
pub(crate) fn local_offset(at: i64) -> i32 {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(at, 0)
        .single()
        .map_or(0, |time| time.offset().local_minus_utc())
}

fn level_color(level: Level, theme: &Theme) -> u32 {
    match level {
        Level::Warn => theme.status_needs_input,
        Level::Danger => theme.danger_text,
    }
}

impl Shell {
    /// The chip in the title bar's right group, just before the Run pill: the agent window
    /// closest to its limit, only from 80% on (nothing otherwise). Amber from 80%, red from
    /// 95%, always with the warning icon and the text. The hover lists every window with its
    /// reset time; a click opens Settings at the Integrations pages.
    pub(crate) fn render_limits_chip(
        &self,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        let chip = mkrew_ui::limits::chip(
            self.limits.all(),
            status::now_ms() / 1000,
            local_offset,
        )?;
        let color = level_color(chip.level, theme);
        Some(
            div()
                .id("limits-chip")
                .flex_none()
                .flex()
                .items_center()
                .gap(px(space::S))
                .h(theme::rems_px(theme::CONTROL_HEIGHT))
                .px(px(space::M))
                .rounded(px(radius::MEDIUM))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.control_border))
                .whitespace_nowrap()
                .text_size(font_size::BODY)
                .text_color(rgb(color))
                .pressable(Look::Chip, true, theme)
                .tooltip(tooltip(theme, &chip.tooltip, ""))
                .on_press(
                    true,
                    cx.listener(|shell, _, window, cx| shell.open_limits_settings(window, cx)),
                )
                .child(icon_svg(Icon::TriangleAlert, theme::ICON_SIZE_SMALL, color))
                .child(chip.text)
                .into_any_element(),
        )
    }

    /// Settings at the Integrations pages (`settings_dialog::INTEGRATIONS_TAB`).
    fn open_limits_settings(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.open_settings_dialog(window, cx);
        if let Some(dialog) = self.settings_dialog.as_mut() {
            let _ = dialog.set_tab(settings_dialog::INTEGRATIONS_TAB);
        }
    }
}
```

`crates/mkrew-ui/src/shell.rs`: delete line 73 (`mod widget_bar;`) and insert after line 52 (`mod jira;`):

```rust
mod limits;
```

`crates/mkrew-ui/src/shell/chrome.rs`: insert before line 394 (`        if let Some(project_id) = workspace {` of the right group, the one that adds `render_run_button`):

```rust
        right = right.children(self.render_limits_chip(theme, cx));
```

and replace the end of line 298 so the doc reads `...a drag area; the limits chip (from 80% of an agent limit), the Run pill,`.

`crates/mkrew-ui/src/shell/composer.rs` line 1134:

```rust
            &super::limits::local_offset,
```

`crates/mkrew-ui/src/main.rs` (bottom-up): delete line 1308 (`            .children(self.render_widget_bar(window, &theme, cx))`); replace lines 710-716 (`widget_bar`, `widgets_hidden` and `widget_data` with their docs):

```rust
    /// The agents' usage limits the title bar's chip shows (events and `limits.get`).
    limits: mkrew_ui::limits::Limits,
```

delete lines 593-594 (the `widget_menu` doc and field) and lines 432-438 (`WidgetMenu`, its doc and the blank line after it).

`crates/mkrew-ui/src/shell/connection.rs` (find by text; bottom-up): `data_changed |= self.widget_data.apply(event);` becomes `data_changed |= self.limits.apply(event);`; delete `            widget_bar: self.widget_bar,` in the `Shown` builder; delete the two lines `self.widget_bar = settings.ui.widget_bar;` and `self.widgets_hidden = settings.ui.widgets_hidden.clone();`; `self.widget_data.clear_limits();` becomes `self.limits.clear();`; in the struct literal replace the three lines `widget_bar,` / `widgets_hidden,` / `widget_data: mkrew_ui::widget_bar::WidgetData::default(),` with `            limits: mkrew_ui::limits::Limits::default(),`; delete `            widget_menu: None,`; delete `        let (widget_bar, widgets_hidden) = (ui.widget_bar, ui.widgets_hidden);` (line 48).

`crates/mkrew-ui/src/shell/focus.rs`: delete line 126 (`        self.widget_menu = None;`).

`crates/mkrew-ui/src/shell/menu_render.rs`: delete lines 364-366 (`if self.widget_menu.is_some() { return Some(self.widget_menu_rows()); }`) then line 326 (`            || self.widget_menu.is_some()`).

`crates/mkrew-ui/Cargo.toml` line 24:

```toml
# The limits chip's reset times in local time; already in the tree through gpui.
```

Delete the old bar: `git rm crates/mkrew-ui/src/widget_bar.rs crates/mkrew-ui/src/shell/widget_bar.rs`, and delete `pub mod widget_bar;` (line 112) from `crates/mkrew-ui/src/lib.rs`.

- [ ] **Step 14: Run the tests and see them pass**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-core`
Expected: PASS, including `a_file_with_the_retired_widget_bar_keys_still_loads_and_never_writes_them_back`, `every_template_key_has_exactly_one_setting` and `writing_each_default_reads_it_back_and_keeps_comments`.

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib`
Expected: PASS, including `limits::`, `limits_always_redraw_the_main_window`, `rows_group_into_cards_and_each_result_is_its_own_card`, `each_contrast_rule_fires_when_its_token_is_broken`, the keymap and palette tests.

- [ ] **Step 15: Clippy over the workspace (also compiles the `mkrew-ui` binary where the shell lives)**

Run: `CARGO_BUILD_JOBS=4 cargo fmt --all && CARGO_BUILD_JOBS=4 cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings. If rustc reports the two `Retired` fields as never read, the derived `PartialEq` was not enough: add `#[expect(dead_code, reason = "retired keys, read only so the file loads")]` above each of the two fields and re-run.

- [ ] **Step 16: Commit**

```bash
git add crates/mkrew-core crates/mkrew-ui
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): replace the widget bar with a limits chip in the title bar"
```

**Risks for the reviewer:**
- The chip only redraws on `agent.limits` events and normal repaints: a window that expires (its reset time passes) keeps the chip until the next repaint; no test covers a timer.
- `render_limits_chip`, its click and tooltip are drawn code with no headless test; they need the dark and light screenshot smoke of Task 7.
- A `keymap.toml` that still binds `toggle_widget_bar` now reports "unknown command" in the palette (the entry keeps nothing); no migration.
- `Retired` ignores any TOML type, so `widget_bar = "x"` loads silently; the template and Settings page no longer mention the keys.
- `Key::NextWrapping`, the Git widget's only caller, is deleted with its test; nothing else steps through files with wrap-around.
### Task 3: Pane header on every pane, tab title from the focused pane

Line numbers are those of master 7cf71e0 in files that Tasks 1 and 2 may have shifted; find each place by the quoted text, not by number.
Task 3 runs before Task 4, so the tab strip is still the strip under the title bar (`render_tab_bar`).
Contract deviation: `render_tab_bar` loses its `now` parameter (its only use, the single pane status, goes), so its one caller in `chrome.rs` changes.
Task 4 moves the strip into the title bar and rewrites `render_tab_bar`; it must keep the rule of this task: the strip only renders the rename field for `RenameTarget::Tab`, never for `RenameTarget::Session`.

**Files:**

- Modify `crates/mkrew-ui/src/tabs.rs`: `Tab::single` (73-79) deleted; `session_label` (826-842) follows the focused pane; new `has_default_title` and `double_click_renames`.
- Modify `crates/mkrew-ui/src/tabs/tests.rs`: replace the test at 720-759; add one test after it.
- Modify `crates/mkrew-ui/src/pane_text.rs`: `HeaderRoom::ALL` (44-52) deleted; `header_room` (67-70) without a title; one test in `mod tests` (after 202).
- Modify `crates/mkrew-ui/src/shell/pane_render.rs`: `render_tab_bar` (6-180) loses the single pane block, the zoom notice and the pane rename branch; `tab_chip` (197-199, 249-262) uses `double_click_renames`; `render_tab` (428-457) passes `zoomed`; `pane_frame` (501-518, 577-590) always draws the header; `pane_status` doc (678-681); `pane_title` (879-924) becomes `pane_rename`; `pane_header` (926-981) drops the title, adds the chip tooltip, the zoom notice and the rename slot.
- Modify `crates/mkrew-ui/src/shell/detached_render.rs` (281-287): the strip no longer renders a pane rename field.
- Modify `crates/mkrew-ui/src/shell/chrome.rs` (251): `render_tab_bar` call without `now`.
- Modify `crates/mkrew-ui/src/shell/widgets.rs` (89): doc of `notice`.
- Modify `crates/mkrew-ui/src/main.rs` (111-114): import `double_click_renames`.
- No change, checked: `Anchor::PaneHeader` in `detached_actions.rs:434` and `menu_render.rs:680,726,738,750,754` (a single or zoomed pane now records it from `pane_header`, as several panes already do); F2 (`Command::RenamePane` in `sessions.rs:378` and `detached_actions.rs:419`) already renames `focused_session`, which is the right target; `hosts.rs:58` and `detached_render.rs:294` use `session_label` and follow it.

**Interfaces:**

Consumes: `history::title(&Session) -> Option<&str>`, `mkrew_core::run::command_name(&Session) -> Option<&str>`, `Tab::focused() -> Option<Ulid>`, `Shell::start_pane_rename(&mut self, Ulid, &mut Window, &mut Context<Self>)`, `Shell::start_rename`, `Shell::render_rename`, `widgets::{button_frame, icon_text, notice, tooltip, agent_chip}`, `Pressable::{pressable, on_press}`.

Produces:

```rust
// crates/mkrew-ui/src/tabs.rs
pub fn session_label<'a>(tab: &Tab, sessions: &'a [Session]) -> Option<&'a str>; // signature unchanged, now the focused pane
pub fn double_click_renames(tab: &Tab, sessions: &[Session]) -> Option<Ulid>;

// crates/mkrew-ui/src/pane_text.rs
pub fn header_room(width: f32, em: f32, context_chars: usize, switch: bool) -> HeaderRoom; // signature unchanged, no title

// crates/mkrew-ui/src/shell/pane_render.rs
impl Shell {
    pub(crate) fn render_tab_bar(&self, project_id: Ulid, tabs: &Tabs, theme: &Theme, cx: &mut Context<'_, Self>) -> impl IntoElement; // no `now`
    pub(crate) fn pane_frame(&self, session: Ulid, several: bool, zoomed: bool, width: f32, is_focused: bool, composer_focused: bool, voice_ring: Option<Hsla>, now: i64, theme: &Theme, cx: &mut Context<'_, Self>) -> impl IntoElement;
    pub(crate) fn pane_header(&self, info: &Session, color: Option<u32>, width: f32, zoomed: bool, now: i64, theme: &Theme, cx: &mut Context<'_, Self>) -> gpui::AnyElement;
    fn pane_rename(&self, session: Ulid, theme: &Theme, cx: &mut Context<'_, Self>) -> Option<gpui::AnyElement>;
}
```

Removed: `Tab::single`, `HeaderRoom::ALL`, `Shell::pane_title`.

Design of the rename field: the tab chip has no title row for a pane, so while a pane is renamed (F2, pane menu "Rename pane", double-click on a default-named tab) the field shows in the pane's header, in the free space after the status parts.
That is where `pane_title` showed it for several panes already, so a single and a zoomed pane now work the same way, and the strip stays free for `RenameTarget::Tab` only.

- [ ] **Step 1: Write the failing `session_label` test**

In `crates/mkrew-ui/src/tabs/tests.rs` replace the whole test `a_default_named_tab_with_one_pane_on_screen_shows_its_session_title` (lines 720-759, from `#[test]` to its closing `}`) with:

```rust
/// A tab that keeps its default name shows the session title of its focused pane, also among
/// several panes; a name the user gave the tab wins; the label follows the focus.
#[test]
fn a_default_named_tab_shows_the_session_title_of_its_focused_pane() {
    let project = Ulid::new();
    let mut titled = session(project, SessionState::Idle, 1);
    titled.terminal_title = Some("Fix login".into());
    let mut named = session(project, SessionState::Idle, 2);
    named.title = Some("Docs".into());
    let untitled = session(project, SessionState::Idle, 3);
    let sessions = [titled.clone(), named.clone(), untitled.clone()];
    let tree = json!({"split":"vertical","ratio":0.5,"children":[
        pane(titled.id),
        {"split":"horizontal","ratio":0.5,"children":[pane(named.id), pane(untitled.id)]}
    ]});
    // The label of a tab loaded with `layout`; `load` focuses its first pane.
    let label = |title: &str, layout| {
        let mut tabs = Tabs::new(project);
        tabs.load(vec![wire(project, title, 1.0, layout)]);
        session_label(tabs.active().unwrap(), &sessions).map(str::to_owned)
    };

    assert_eq!(
        label("Terminal", pane(titled.id)).as_deref(),
        Some("Fix login")
    );
    assert_eq!(
        label("Terminal 12", pane(titled.id)).as_deref(),
        Some("Fix login")
    );
    assert_eq!(label("Terminal", pane(untitled.id)), None, "no session title");
    assert_eq!(label("Terminal", serde_json::Value::Null), None, "empty tab");
    // A name the user gave the tab wins, with one pane and with several.
    for title in ["Backend", "Terminal 3b", "Terminal ", "Terminals"] {
        assert_eq!(label(title, pane(titled.id)), None, "{title}");
        assert_eq!(label(title, tree.clone()), None, "{title}");
    }

    // Several panes: the label follows the focus; the user's pane title is the pane's title.
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "Terminal 2", 1.0, tree)]);
    let shown = |tabs: &Tabs| session_label(tabs.active().unwrap(), &sessions);
    assert_eq!(shown(&tabs), Some("Fix login"));
    assert!(tabs.focus(named.id));
    assert_eq!(shown(&tabs), Some("Docs"));
    assert!(tabs.focus(untitled.id));
    assert_eq!(shown(&tabs), None);
    assert!(tabs.focus(titled.id));
    assert_eq!(shown(&tabs), Some("Fix login"));
}
```

- [ ] **Step 2: Run it and see it fail**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib tabs::tests::a_default_named_tab_shows_the_session_title_of_its_focused_pane`
Expected: FAIL at `assert_eq!(shown(&tabs), Some("Fix login"))` with `left: None`, because `session_label` still needs a single pane.

- [ ] **Step 3: `session_label` follows the focused pane; delete `Tab::single`**

In `crates/mkrew-ui/src/tabs.rs` delete lines 73-79 (the doc comment and `pub fn single`, and the blank line before `/// Sessions on screen`):

```rust
    /// The one pane on screen (zoomed, or the only pane), which then has no header.
    pub fn single(&self) -> Option<Ulid> {
        match (self.zoomed, self.layout.as_ref()) {
            (Some(session), _) | (None, Some(&Node::Pane { session, .. })) => Some(session),
            _ => None,
        }
    }

```

Replace lines 826-842 (`session_label` and its doc) with:

```rust
/// Whether `tab` still has its default name: `Terminal` or `Terminal N`.
fn has_default_title(tab: &Tab) -> bool {
    match tab.title.strip_prefix(FIRST_TAB_TITLE) {
        Some("") => true,
        Some(rest) => rest
            .strip_prefix(' ')
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())),
        None => false,
    }
}

/// The session title a tab shows instead of its own: the title of its focused pane (no pane
/// header shows a title), while the tab keeps its default `Terminal` / `Terminal N` name. A
/// name the user gave the tab always wins. It follows the focus; `None` for a tab without
/// panes or a focused pane without a title.
pub fn session_label<'a>(tab: &Tab, sessions: &'a [Session]) -> Option<&'a str> {
    let focused = tab.focused().filter(|_| has_default_title(tab))?;
    sessions
        .iter()
        .find(|s| s.id == focused)
        .and_then(crate::history::title)
}
```

`Tab::single` still has callers in `shell/pane_render.rs` and `shell/detached_render.rs` until Step 12, so the workspace does not build between Step 3 and Step 12. Steps 4 to 11 only run `--lib` tests of `mkrew-ui`, which do not compile the `shell` module of the binary, so they are unaffected.

- [ ] **Step 4: Run it and see it pass**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib tabs::tests`
Expected: PASS (all tab tests, including `a_default_named_tab_shows_the_session_title_of_its_focused_pane`).

- [ ] **Step 5: Write the failing double-click rule test**

In `crates/mkrew-ui/src/tabs/tests.rs`, right after the test of Step 1, add:

```rust
/// A double-click renames the focused pane while the tab keeps its default name; the tab
/// itself when the user named it, when it has no panes, or when the focused pane is a run pane
/// (named by its command).
#[test]
fn a_double_click_renames_the_focused_pane_of_a_default_named_tab() {
    let project = Ulid::new();
    let agent = session(project, SessionState::Idle, 1);
    let mut run = session(project, SessionState::Idle, 2);
    run.kind = SessionKind::Shell;
    run.title = Some("run:BE".into());
    let sessions = [agent.clone(), run.clone()];
    let split =
        json!({"split":"vertical","ratio":0.5,"children":[pane(agent.id), pane(run.id)]});
    let renames = |title: &str, focus: Ulid| {
        let mut tabs = Tabs::new(project);
        tabs.load(vec![wire(project, title, 1.0, split.clone())]);
        assert!(tabs.focus(focus));
        double_click_renames(tabs.active().unwrap(), &sessions)
    };

    assert_eq!(renames("Terminal 2", agent.id), Some(agent.id));
    assert_eq!(renames("Terminal", run.id), None, "a run pane is not renamed");
    assert_eq!(renames("Backend", agent.id), None, "the user's tab name");

    let mut empty = Tabs::new(project);
    empty.load(vec![wire(project, "Terminal", 1.0, serde_json::Value::Null)]);
    assert_eq!(double_click_renames(empty.active().unwrap(), &sessions), None);
}
```

- [ ] **Step 6: Run it and see it fail**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib tabs::tests::a_double_click_renames`
Expected: FAIL to compile with `error[E0425]: cannot find function \`double_click_renames\` in this scope`.

- [ ] **Step 7: Add `double_click_renames`**

In `crates/mkrew-ui/src/tabs.rs`, directly after `session_label`, add:

```rust
/// The pane a double-click on `tab` renames (SPEC §11): its focused pane while the tab keeps
/// its default name, because the tab then shows that pane's title. `None` renames the tab
/// itself: a name the user gave it, a tab without panes, or a focused run pane (named by its
/// command, never renamed).
pub fn double_click_renames(tab: &Tab, sessions: &[Session]) -> Option<Ulid> {
    let focused = tab.focused().filter(|_| has_default_title(tab))?;
    let session = sessions.iter().find(|s| s.id == focused)?;
    mkrew_core::run::command_name(session)
        .is_none()
        .then_some(focused)
}
```

- [ ] **Step 8: Run it and see it pass**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib tabs::tests`
Expected: PASS.

- [ ] **Step 9: Write the failing `header_room` test**

In `crates/mkrew-ui/src/pane_text.rs`, inside `mod tests`, after `a_narrow_header_drops_its_parts_in_order` (closing `}` at line 202), add:

```rust
    /// The header has no title: its room is the padding, the chip, one gap and the pill, so at
    /// 13 px per em the switch fits from 16 + (1.85 + 4.0) em + 8 + (8 + 10.15 em) = 240 px.
    #[test]
    fn the_header_room_has_no_title_in_it() {
        let switch = |width: f32| header_room(width, 13.0, 0, true).switch;
        assert!(!switch(239.0));
        assert!(switch(241.0));
    }
```

- [ ] **Step 10: Run it and see it fail**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib pane_text::tests::the_header_room_has_no_title_in_it`
Expected: FAIL at `assert!(switch(241.0))`, because the title's gap still makes the switch need 248 px.

- [ ] **Step 11: `header_room` without the title; delete `HeaderRoom::ALL`**

In `crates/mkrew-ui/src/pane_text.rs` delete lines 44-52 and the blank line after them:

```rust
impl HeaderRoom {
    /// Room for every part (the tab bar of a single pane).
    pub const ALL: Self = Self {
        context: true,
        share: true,
        reading: true,
        switch: true,
    };
}

```

Replace lines 69-70:

```rust
    // The padding on both sides, then the chip, the (maybe empty) title and the pill.
    let mut used = 2.0 * gap + (CHIP_EM + PILL_MIN_EM) * em + 2.0 * gap;
```

with:

```rust
    // The padding on both sides, then the chip, the gap and the pill (the header has no title).
    let mut used = 2.0 * gap + (CHIP_EM + PILL_MIN_EM) * em + gap;
```

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib pane_text`
Expected: PASS, including the unchanged `a_narrow_header_drops_its_parts_in_order` (every threshold moves down by 8 px; its 304 px and 500 px cases and its order of states do not change).

- [ ] **Step 12: Header on every pane in `pane_render.rs`**

All edits are in `crates/mkrew-ui/src/shell/pane_render.rs`.

(a) Replace `render_tab_bar` (lines 6-180, doc comment through the closing `}`) with the full function below. It has no `single`, no `now`, no zoom notice, no pane status, and its rename branch is the tab's only.

```rust
    /// The tab strip: the tabs (kind icon, title, status dot, pin, ×) and `+`; on the right
    /// the notices, the split / layout button and the ⋯ menu. A pane's status, its controls and
    /// the zoom notice are in its header.
    pub(crate) fn render_tab_bar(
        &self,
        project_id: Ulid,
        tabs: &Tabs,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let active = tabs.active().map(|tab| tab.id);
        // Pinned tabs lead the strip; each kind is dragged among its own.
        let pinned_count = tabs.tabs().iter().filter(|tab| tab.pinned).count();
        let strip = (self.hosts.get(&HostId::Main))
            .map(|ui| ui.strip.clone())
            .unwrap_or_default();
        div()
            .flex_none()
            .relative()
            .h(theme::rems_px(theme::TAB_STRIP_HEIGHT))
            .flex()
            .items_center()
            .gap(px(space::XS))
            .px(px(space::S))
            .bg(rgb(theme.surface))
            .border_b(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .on_drop(cx.listener(|shell, item: &DraggedItem, window, cx| {
                shell.drop_reordered(item, window, cx);
            }))
            // First: the strip records before its chips (prepaint goes parent first).
            .child(record_strip(strip.clone(), None))
            .children(
                tabs.tabs()
                    .iter()
                    .enumerate()
                    .filter(|(_, tab)| self.terminal_host(project_id, tab.id) == HostId::Main)
                    .enumerate()
                    .map(|(slot, (place, tab))| {
                        let tab_id = tab.id;
                        let is_active = active == Some(tab_id);
                        // The tab's own rename field takes its place; a pane is renamed in
                        // its header.
                        if let Some(renaming) = (self.renaming.as_ref())
                            .filter(|r| r.target == RenameTarget::Tab(tab_id))
                        {
                            return self.render_rename(&renaming.draft, theme, cx);
                        }
                        // The focused pane's title stands in for a default tab name.
                        let label = session_label(tab, self.sidebar.sessions());
                        let item = DraggedItem {
                            list: ReorderList::Tabs(project_id, tab.pinned),
                            index: if tab.pinned {
                                place
                            } else {
                                place - pinned_count
                            },
                            title: label.unwrap_or(&tab.title).to_owned().into(),
                        };
                        let select = move |shell: &mut Shell| {
                            if let Some(tabs) = shell.active_tabs_mut() {
                                tabs.activate(tab_id);
                            }
                        };
                        let chip = self
                            .tab_chip(project_id, tab, is_active, None, select, theme, cx)
                            .child(record_strip(strip.clone(), Some(slot)));
                        self.reorderable(chip.into_any_element(), item, true, space::XS, theme, cx)
                    }),
            )
            .child(
                icon_button("new-tab", Icon::Plus, theme)
                    .tooltip(tooltip(
                        theme,
                        &format!(
                            "New empty tab ({} starts an agent tab, {} a shell tab)",
                            self.keymap.label(Command::NewAgentTab),
                            self.keymap.label(Command::NewShellTab)
                        ),
                        "",
                    ))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.create_tab(project_id, None, window, cx);
                    })),
            )
            // Pushes the notices and the split / ⋯ group to the right.
            .child(div().flex_1())
            .children(
                (self.tab_error.as_ref())
                    .filter(|(host, _)| *host == HostId::Main)
                    .map(|(_, error)| notice(theme.text, format!("✕ {error}"))),
            )
            .children(
                (self.tab_notice.as_ref())
                    .filter(|(host, _)| *host == HostId::Main)
                    .map(|(_, text)| notice(theme.text_muted, text.clone())),
            )
            .children(
                lifecycle::running_warning(self.sidebar.sessions(), project_id)
                    .map(|warning| notice(theme.text_muted, format!("⚠ {warning}"))),
            )
            .child(
                icon_button("tab-split", Icon::Columns2, theme)
                    .tooltip(tooltip(
                        theme,
                        "Split or pick a layout",
                        self.keymap.label(Command::SplitRight),
                    ))
                    .on_click(cx.listener(|shell, event: &gpui::ClickEvent, window, cx| {
                        shell.open_layout_menu(event.position(), window, cx);
                    })),
            )
            .when_some(active, |bar, tab_id| {
                bar.child(
                    icon_button("tab-menu-button", Icon::Ellipsis, theme)
                        .tooltip(tooltip(theme, "Tab and pane menu", ""))
                        .on_click(cx.listener(
                            move |shell, event: &gpui::ClickEvent, window, cx| {
                                shell.open_tab_menu(tab_id, event.position(), window, cx);
                            },
                        )),
                )
            })
            .children(self.drop_hint_line(HostId::Main, theme))
    }
```

(b) In `tab_chip`, replace lines 197-199:

```rust
        // A lone pane's title stands in for a default tab name.
        let label = session_label(tab, self.sidebar.sessions());
        let labeled = tab.single().filter(|_| label.is_some());
```

with:

```rust
        // The focused pane's title stands in for a default tab name.
        let label = session_label(tab, self.sidebar.sessions());
        // A double-click renames that pane while the tab keeps its default name, else the tab.
        let rename_pane = double_click_renames(tab, self.sidebar.sessions());
```

and in the `.on_press(...)` handler replace lines 254-259:

```rust
                    if event.click_count() == 2 {
                        match labeled {
                            Some(session) => shell.start_pane_rename(session, window, cx),
                            None => shell.start_rename(window, cx),
                        }
                    }
```

with:

```rust
                    if event.click_count() == 2 {
                        match rename_pane {
                            Some(session) => shell.start_pane_rename(session, window, cx),
                            None => shell.start_rename(window, cx),
                        }
                    }
```

(c) In `render_tab`, after `let several = rects.len() > 1;` (line 431) add:

```rust
        let zoomed = tab.is_some_and(|tab| tab.zoomed().is_some());
```

and in the `pane_frame` call (lines 447-457) pass it after `several`:

```rust
                    .child(self.pane_frame(
                        session,
                        several,
                        zoomed,
                        width,
                        is_focused,
                        is_focused && composer_focused,
                        voice_ring.filter(|_| is_focused),
                        now,
                        theme,
                        cx,
                    ))
```

(d) Replace the doc and signature of `pane_frame` (lines 501-518) with:

```rust
    /// One pane: its header, the terminal (or why it is not shown; at reading width, centered,
    /// for an agent pane with it on), its composer, its border. Every pane has the compact
    /// header, a single and a zoomed one too; among several panes the unfocused ones get a dim.
    /// `zoomed`: the pane fills the tab, its header shows the zoom notice. `voice_ring`: the
    /// listening ring over the border; `width`: the pane's width in px in the last frame (the
    /// header's room).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pane_frame(
        &self,
        session: Ulid,
        several: bool,
        zoomed: bool,
        width: f32,
        is_focused: bool,
        composer_focused: bool,
        voice_ring: Option<Hsla>,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
```

and replace lines 577-590 (the `.when(several, ..)` and `.when(!several, ..)` blocks, the top accent line of a single pane included) with:

```rust
            .children(
                info.map(|info| self.pane_header(info, color, width, zoomed, now, theme, cx)),
            )
```

(e) Replace the doc of `pane_status` (lines 678-681):

```rust
    /// The status of `info`'s pane, each part only when it has something to say: the task
    /// folder, the status pill, the conflict warning, `⧗ N queued` sends and the context share.
    /// The tab bar shows them for a single pane, the compact header among several; `context`:
    /// whether there is room for the context share.
```

with:

```rust
    /// The status of `info`'s pane, each part only when it has something to say: the task
    /// folder, the status pill, the conflict warning, `⧗ N queued` sends and the context share.
    /// The pane header shows them; `context`: whether there is room for the context share.
```

(f) Replace `pane_title` (lines 879-924, doc through closing `}`) with:

```rust
    /// The inline rename field of `session`'s pane while it is edited (F2, the pane menu, or a
    /// double-click on a tab that keeps its default name, ADR 0031); `None` otherwise. The
    /// header has no title, so the field takes the header's free space.
    fn pane_rename(
        &self,
        session: Ulid,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        let renaming = (self.renaming.as_ref())
            .filter(|r| r.target == RenameTarget::Session(session))?;
        // Mouse downs stay in the field: the pane's handler would focus the terminal, and the
        // blur would commit half-typed text.
        Some(
            div()
                .flex_1()
                .min_w_0()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _, _, cx| cx.stop_propagation()),
                )
                .child(self.render_rename(&renaming.draft, theme, cx))
                .into_any_element(),
        )
    }
```

(g) Replace `pane_header` (lines 926-981, doc through closing `}`) with the full function:

```rust
    /// A pane's compact header (SPEC §11), on every pane, a single and a zoomed one too: the
    /// agent chip (tinted by the pane color, which also draws a line under the header; its
    /// tooltip is the pane's full title), the status parts, then the zoom notice while `zoomed`
    /// and the agent controls. It has no title: the tab shows the focused pane's. While the pane
    /// is renamed its field takes the free space. Right-click: the pane menu.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pane_header(
        &self,
        info: &Session,
        color: Option<u32>,
        width: f32,
        zoomed: bool,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let session = info.id;
        let context_chars = (self.usages.get(session))
            .and_then(usage::context_share)
            .map_or(0, |share| share.label.chars().count());
        let room = pane_text::header_room(
            width,
            cx.global::<theme::Fonts>().ui_size,
            context_chars,
            self.chat_available(info),
        );
        let title = (run::pane_title(info, self.ports.get(session)))
            .unwrap_or_else(|| info.agent_profile.clone());
        let zoom_notice = zoomed.then(|| {
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(space::S))
                .child(notice(theme.text_muted, "⤢ Pane zoomed".to_owned()))
                .child(
                    button_frame(
                        "pane-unzoom",
                        icon_text(Icon::Minimize2, "Show all panes", theme.text_muted),
                    )
                    .px(px(space::S))
                    .py_0()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .pressable(Look::Chip, true, theme)
                    .tooltip(tooltip(
                        theme,
                        "Show all panes",
                        self.keymap.label(Command::ZoomPane),
                    ))
                    .on_press(
                        true,
                        cx.listener(|shell, _: &gpui::ClickEvent, window, cx| {
                            shell.run(&Run(Command::ZoomPane), window, cx);
                        }),
                    ),
                )
        });
        let header = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::S))
            .py(px(space::XS))
            // Transparent without a color, so the header keeps its height.
            .border_b(px(theme::PANE_ACCENT_WIDTH))
            .border_color(match color {
                Some(color) => Hsla::from(rgb(color)),
                None => gpui::transparent_black(),
            })
            .font_family(theme.ui_font_family.clone())
            .text_size(font_size::META)
            .child(
                agent_chip(self.agent_badge(&info.agent_profile), theme.text, theme)
                    .when_some(color, |chip, color| {
                        chip.bg(theme.tint(color)).border_color(rgb(color))
                    })
                    .id(SharedString::from(format!("pane-chip-{session}")))
                    .tooltip(tooltip(theme, &title, "")),
            )
            .children(self.pane_status(info, room.context, now, theme, cx))
            // The free space pushes the controls right; the rename field takes it.
            .child(
                self.pane_rename(session, theme, cx)
                    .unwrap_or_else(|| div().flex_1().min_w_0().into_any_element()),
            )
            .children(zoom_notice)
            .children(self.pane_controls(info, room, theme, cx))
            .relative()
            .child(record_bounds(Anchor::PaneHeader(session), cx))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                    shell.open_pane_menu(session, event.position, window, cx);
                }),
            );
        self.draggable_pane(header, info, theme)
    }
```

- [ ] **Step 13: Detached strip, `chrome.rs`, `widgets.rs`, `main.rs`**

In `crates/mkrew-ui/src/shell/detached_render.rs` replace lines 281-287:

```rust
                // F2 on a single pane: its rename field takes the tab's place (no header).
                if let Some(renaming) = self.renaming.as_ref().filter(|r| {
                    r.target == RenameTarget::Tab(tab_id)
                        || tab.single().map(RenameTarget::Session).as_ref() == Some(&r.target)
                }) {
                    return Some(self.render_rename(&renaming.draft, theme, cx));
                }
```

with:

```rust
                // The tab's own rename field takes its place; a pane is renamed in its header.
                if let Some(renaming) = (self.renaming.as_ref())
                    .filter(|r| r.target == RenameTarget::Tab(tab_id))
                {
                    return Some(self.render_rename(&renaming.draft, theme, cx));
                }
```

In `crates/mkrew-ui/src/shell/chrome.rs` line 251 replace:

```rust
                    .child(self.render_tab_bar(project_id, tabs, now, theme, cx))
```

with:

```rust
                    .child(self.render_tab_bar(project_id, tabs, theme, cx))
```

(`now` stays used by `render_tab` in the same function.)

In `crates/mkrew-ui/src/shell/widgets.rs` replace line 89:

```rust
/// A one-line notice in the tab bar, shortened to fit; `color` sets errors apart from warnings.
```

with:

```rust
/// A one-line notice (tab bar, pane header), shortened to fit; `color` sets errors apart from
/// warnings.
```

In `crates/mkrew-ui/src/main.rs` lines 111-114 the import becomes (run `cargo fmt` to place it):

```rust
use mkrew_ui::tabs::{
    FIRST_TAB_TITLE, Tab, TabKind, Tabs, double_click_renames, newest_unplaced, on_screen,
    rename_params, session_label, tab_kind,
};
```

- [ ] **Step 14: Format and run the tests**

Run: `cargo fmt -p mkrew-ui`
Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui`
Expected: builds (no use of `Tab::single`, `HeaderRoom::ALL` or `Shell::pane_title` is left) and PASS, including `tabs::tests`, `pane_text::tests`, `shell::menu_render` anchor tests.

- [ ] **Step 15: Clippy**

Run: `CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 16: Commit**

```
git add crates/mkrew-ui/src/tabs.rs crates/mkrew-ui/src/tabs/tests.rs crates/mkrew-ui/src/pane_text.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/detached_render.rs crates/mkrew-ui/src/shell/chrome.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/main.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): pane header on every pane, tab title follows the focused pane"
```

**Risks for the reviewer:**

- Drawing is not unit tested: the header of a single pane, the zoom notice and its button in a zoomed and in a detached window, the chip tooltip, and the rename field in the header (its CONTROL_HEIGHT can make the header taller while renaming, as it already did for several panes) need the Task 7 screenshots.
- A default-named tab whose focused pane is a run pane shows the raw `run:NAME` title through `history::title` (as a lone run pane already did); the double-click then renames the tab.
- Right-click "Rename pane" on an unfocused pane renames that pane, while F2 and the tab double-click target the focused pane; the field is always in the renamed pane's header.
- `header_room` does not reserve room for the zoom notice, so a zoomed pane in a very narrow window can clip it (the notice truncates, the controls keep their room).
- An `Exited` focused pane of a default-named tab opens the pane rename and the commit is silently dropped by `rename_session`, as before.
### Task 4: Top bar part 1: tabs in the title bar, view chips, removed buttons

Line numbers are those of master 7cf71e0 in files that Tasks 1 to 3 may have shifted; find each place by the quoted text, not by number.
Tasks 1 to 3 are done: no widget bar, `shell/limits.rs` exists (`render_limits_chip`), `Tab::single` is gone, `render_tab_bar` has no `now` and renders only the `RenameTarget::Tab` rename field.

Contract deviations (everything else in the contract holds):

- `render_title_bar(&self, host: HostId, now: i64, theme: &Theme, window: &Window, cx: &mut Context<'_, Self>)`: the existing parameter style (`now`, `theme`, `&Window`) is kept; the `shown: View` parameter goes (the bar reads `self.view()`).
- `render_tab_area(&self, project_id: Ulid, tabs: &Tabs, theme: &Theme, cx: &mut Context<'_, Self>)`: main window only, so no `host` parameter. Detached windows use `tab_area_frame` (agreed with the Task 6 author): `tab_area_frame(&self, host: HostId, chips: Vec<gpui::AnyElement>, active: Option<(SharedString, usize)>, trailing: Option<gpui::AnyElement>, theme: &Theme, cx: &mut Context<'_, Self>)`.
- The `+ New ▾` button and the plain `+` stay as they are in this task (`+ New ▾` at the end of the right group, a plain `+` icon button after the tabs). Task 5 merges them. The `⋯` button goes in this task, with `render_tab_bar`.
- The Split button shows only while the main window shows Terminals (it acts on the shown terminal tab: `tab_scope` is `None` in other views). Detached windows always show it.
- Radius: the spec's `radius::M` is `radius::MEDIUM` (8) in the code.
- Not changed, verified: `menu_render.rs:616-628` (`menu_anchor` already falls back to just under `TITLE_BAR_HEIGHT`); `workspace.rs:400` is `View::view()`, not a caller of `View::bar` (`View::bar` has only `render_view_tabs` and its test); `file_tabs_render.rs` (the Editor's own file tab strip, 44 px, stays inside the Editor view); `inspector.rs:297` and `sidebar.rs:575` keep `TAB_STRIP_HEIGHT` (only their comments change); `app_logo` and `APP_LOGO_TILE` stay until Task 6 deletes the detached bar that still uses them.

**Files:**

- Modify `crates/mkrew-ui/src/tabs.rs`: `AreaItem`, `area_items`, `scroll_index` after `double_click_renames`; `use crate::board::View;`.
- Modify `crates/mkrew-ui/src/tabs/tests.rs`: two tests at the end.
- Modify `crates/mkrew-ui/src/theme.rs`: `TAB_MIN_WIDTH` after `TAB_TITLE_MAX_WIDTH` (2357); delete `mode_icon` (2275-2283); doc of `TAB_STRIP_HEIGHT` (2324).
- Create `icon/lucide/square-kanban.svg`; modify `crates/mkrew-ui/src/icons.rs` (the `icons!` list, after `SquareCheck`, line 111).
- Modify `crates/mkrew-ui/src/shell/widgets.rs`: add `tab_frame` and `title_rule` after `segment_in` (578).
- Modify `crates/mkrew-ui/src/shell/hosts.rs`: `HostUi` gets `scroll` and `scrolled` (7-23).
- Modify `crates/mkrew-ui/src/shell/pane_render.rs`: delete `render_tab_bar` (9-180 on master; its post-Task-3 form); add `tab_area_frame`, `render_tab_area`, `view_chip`, `view_icon`; edit `tab_chip` (186-323).
- Modify `crates/mkrew-ui/src/shell/chrome.rs`: `render_main` (194-207, 245-274); `render_title_bar` (297-475) replaced; delete `open_repo_menu` (18-22), `repo_menu_rows` (24-43), `render_view_tabs` (518-543); trim `render_workspace_switcher` (545-701).
- Modify `crates/mkrew-ui/src/shell/menu_render.rs` (329, 372-376), `focus.rs` (129), `connection.rs` (380), `main.rs` (630-631 field; 1097 call; 111-114 import).
- Modify `crates/mkrew-ui/src/shell/phone_access.rs`: delete `render_phone_button` (162-201).
- Modify `crates/mkrew-ui/src/git_summary.rs`: delete what only the branch chip and repo menu used.
- Modify `crates/mkrew-ui/src/board.rs` (47-56) and `board/tests.rs` (21-42): `View::bar` and its test deleted.
- Modify comments: `shell/sidebar.rs` (566-567), `shell/inspector.rs` (293-294).

**Interfaces:**

Consumes: `Tabs::tabs() -> &[Tab]`, `Tabs::active() -> Option<&Tab>`, `Shell::{view, has_file_tabs, diff_open, terminal_host, show_view, close_file_tabs, close_git_diff, create_tab, open_layout_menu, open_tab_menu, drop_hint_line, reorderable, render_rename, render_limits_chip, render_run_button, render_update_button, inspector_toggle, sidebar_toggle, machine_chip, voice_status, render_workspace_switcher, render_new_button}`, `widgets::{icon_button, icon_button_in, row_action, window_control, inert_layer, tooltip, pin_mark, status_dot, agent_chip}`, `record_strip`, `Pressable::{pressable, pressable_unless, on_press}`.

Produces:

```rust
// crates/mkrew-ui/src/tabs.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AreaItem { View(View), Rule, Tab(usize) }
pub fn area_items(editor: bool, diff: bool, pinned: &[bool]) -> Vec<AreaItem>;
pub fn scroll_index(items: &[AreaItem], shown: View, active: Option<usize>) -> Option<usize>;

// crates/mkrew-ui/src/theme.rs
pub const TAB_MIN_WIDTH: f32 = 120.0;

// crates/mkrew-ui/src/shell/widgets.rs
pub(crate) fn tab_frame(id: SharedString, shown: bool, theme: &Theme) -> gpui::Stateful<gpui::Div>;
pub(crate) fn title_rule(theme: &Theme) -> gpui::Div;

// crates/mkrew-ui/src/shell/pane_render.rs (impl Shell)
pub(crate) fn render_tab_area(&self, project_id: Ulid, tabs: &Tabs, theme: &Theme, cx: &mut Context<'_, Self>) -> impl IntoElement;
pub(crate) fn tab_area_frame(&self, host: HostId, chips: Vec<gpui::AnyElement>, active: Option<(SharedString, usize)>, trailing: Option<gpui::AnyElement>, theme: &Theme, cx: &mut Context<'_, Self>) -> impl IntoElement;

// crates/mkrew-ui/src/shell/chrome.rs (impl Shell)
pub(crate) fn render_title_bar(&self, host: HostId, now: i64, theme: &Theme, window: &Window, cx: &mut Context<'_, Self>) -> impl IntoElement;
```

Removed: `Shell::{render_tab_bar, render_view_tabs, open_repo_menu, repo_menu_rows, render_phone_button}`, `View::bar`, `theme::mode_icon`, the `repo_menu` field, `GitSummary::branch_tooltip`, `RepoSummary::{label, state, line}`.

Design of the tab area (GPUI API checked in `gpui-pre-0.3.7/src/elements/div.rs`):

- `overflow_x_scroll()` (1523) and `track_scroll(&ScrollHandle)` (1543) need a `Stateful` div; `ScrollHandle::scroll_to_item(ix)` (4316) records the child index and the next prepaint moves the offset (`scroll_to_active_item`, 4337) the minimal amount into view. The index counts the scroll container's children, in order, so the `record_strip` canvas is NOT a child of the scroll container but of the frame around it.
- `scroll_to_item` stays pending until it is applied, and while it is pending the wheel cannot move the strip, so it is called only when the active chip changes (`HostUi::scrolled` holds the last key).
- The vertical wheel scrolls an `overflow_x_scroll` element sideways (`restrict_scroll_to_axis` off, line 3351).
- Only the spacer carries `WindowControlArea::Drag` (as `render_title_bar` does today); the tab area is a sibling, so a click on a tab never reaches a drag region.
- The frame is `min_w_0` and does not grow; the bar's `flex_1` spacer takes the free width. A chip is `flex_shrink` with `min_w(TAB_MIN_WIDTH)` and an ellipsizing title, so the chips shrink to the minimum before the container scrolls. The `+` sits outside the scroll container, so it stays visible.

- [ ] **Step 1: Write the failing tab area tests**

Append to `crates/mkrew-ui/src/tabs/tests.rs`:

```rust
/// The tab area's children, in paint order: the view chips, a rule, the tabs, a rule between
/// the pinned tabs and the rest; never a rule at an end.
#[test]
fn the_tab_area_orders_view_chips_pinned_tabs_and_the_rest() {
    use AreaItem::{Rule, Tab, View as Chip};
    assert_eq!(area_items(false, false, &[]), []);
    assert_eq!(
        area_items(false, false, &[true, true, false]),
        [Tab(0), Tab(1), Rule, Tab(2)]
    );
    assert_eq!(
        area_items(false, false, &[true, true]),
        [Tab(0), Tab(1)],
        "no rule after the last group"
    );
    assert_eq!(
        area_items(true, true, &[false, false]),
        [Chip(View::Editor), Chip(View::Diff), Rule, Tab(0), Tab(1)]
    );
    assert_eq!(
        area_items(false, true, &[true, false]),
        [Chip(View::Diff), Rule, Tab(0), Rule, Tab(1)]
    );
    assert_eq!(area_items(false, true, &[]), [Chip(View::Diff)], "no tabs, no rule");
}

/// The child to scroll into view: the active tab while Terminals shows, else the chip of the
/// shown view; nothing for Board and Docs (they are icons, not children).
#[test]
fn the_scroll_target_is_the_shown_chip_or_the_active_tab() {
    let items = area_items(true, false, &[true, false, false]);
    // [Editor, Rule, Tab(0), Rule, Tab(1), Tab(2)]
    assert_eq!(scroll_index(&items, View::Terminals, Some(2)), Some(5));
    assert_eq!(scroll_index(&items, View::Terminals, Some(0)), Some(2));
    assert_eq!(scroll_index(&items, View::Editor, Some(2)), Some(0));
    assert_eq!(scroll_index(&items, View::Board, Some(2)), None);
    assert_eq!(scroll_index(&items, View::Terminals, None), None);
    assert_eq!(scroll_index(&items, View::Diff, Some(1)), None, "no Diff chip");
}
```

- [ ] **Step 2: Run them and see them fail**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib tabs::tests::the_`
Expected: FAIL to compile with `error[E0433]: failed to resolve: use of undeclared type \`AreaItem\`` (and `cannot find function \`area_items\``).

- [ ] **Step 3: Add `AreaItem`, `area_items`, `scroll_index`**

In `crates/mkrew-ui/src/tabs.rs` add to the imports (after `use crate::split::...`):

```rust
use crate::board::View;
```

and directly after `double_click_renames` (added by Task 3) add:

```rust
/// One child of the title bar's tab area, in paint order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AreaItem {
    /// The Editor or Diff chip.
    View(View),
    /// The 1 px rule between two groups.
    Rule,
    /// The `n`th terminal tab this window draws (detached tabs are not counted).
    Tab(usize),
}

/// The children of the tab area: the Editor / Diff chips (each only while open), a rule, the
/// tabs (`pinned[n]` is whether the `n`th drawn tab is pinned; pinned ones lead), and a rule
/// between the pinned tabs and the rest. A rule only sits between two groups.
pub fn area_items(editor: bool, diff: bool, pinned: &[bool]) -> Vec<AreaItem> {
    let mut items = Vec::new();
    if editor {
        items.push(AreaItem::View(View::Editor));
    }
    if diff {
        items.push(AreaItem::View(View::Diff));
    }
    for (at, &pin) in pinned.iter().enumerate() {
        let after_chips = at == 0 && !items.is_empty();
        let after_pinned = at > 0 && pinned[at - 1] && !pin;
        if after_chips || after_pinned {
            items.push(AreaItem::Rule);
        }
        items.push(AreaItem::Tab(at));
    }
    items
}

/// The index in `items` of the child to scroll into view: the active tab (`active` is its
/// drawn slot) while Terminals shows, else the chip of the shown view. `None` for Board and
/// Docs, which are icons on the right.
pub fn scroll_index(items: &[AreaItem], shown: View, active: Option<usize>) -> Option<usize> {
    items.iter().position(|item| match *item {
        AreaItem::View(view) => view == shown,
        AreaItem::Tab(at) => shown == View::Terminals && active == Some(at),
        AreaItem::Rule => false,
    })
}
```

- [ ] **Step 4: Run them and see them pass**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib tabs::tests`
Expected: PASS (all tab tests including the two new ones).

- [ ] **Step 5: Token, icon, widgets**

`crates/mkrew-ui/src/theme.rs`, after `pub const TAB_TITLE_MAX_WIDTH: f32 = 220.0;` (line 2357) add:

```rust
/// Narrowest a terminal tab chip gets: the chips shrink to this before the tab area scrolls.
pub const TAB_MIN_WIDTH: f32 = 120.0;
```

Replace line 2324 `pub const TAB_STRIP_HEIGHT: f32 = 44.0;` with:

```rust
/// The Editor's file tab strip, the right panel's tab row and the sidebar's filter row (the
/// terminal tabs are in the title bar).
pub const TAB_STRIP_HEIGHT: f32 = 44.0;
```

Delete `mode_icon` with its doc comment (lines 2275-2283 and the blank line after): the theme toggle left the title bar.

Create `icon/lucide/square-kanban.svg`:

```svg
<!-- @license lucide-static v1.55.0 - ISC -->
<svg
  class="lucide lucide-square-kanban"
  xmlns="http://www.w3.org/2000/svg"
  width="24"
  height="24"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <rect width="18" height="18" x="3" y="3" rx="2" />
  <path d="M8 7v7" />
  <path d="M12 7v4" />
  <path d="M16 7v9" />
</svg>
```

`crates/mkrew-ui/src/icons.rs`, in the `Icon` list after `SquareCheck => "square-check",` add:

```rust
        SquareKanban => "square-kanban",
```

(`every_icon_is_served_and_is_a_lucide_svg` covers the new file.)

`crates/mkrew-ui/src/shell/widgets.rs`, after `segment_in` (ends line 578) add:

```rust
/// A tab of the top bar (a terminal tab, the Editor / Diff chip), Tern style: the shown one is
/// a solid `card` with full text and no border; the others are transparent and muted and get
/// the `pressable` hover fill. `id` is also the hover group of a ✕ inside.
pub(crate) fn tab_frame(
    id: SharedString,
    shown: bool,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id.clone())
        .group(id)
        .relative()
        .flex()
        .items_center()
        .gap(px(space::XS))
        .h(theme::rems_px(theme::CONTROL_HEIGHT))
        .pl(px(space::M))
        .pr(px(space::S))
        .rounded(px(radius::MEDIUM))
        // The same (transparent) border as the rename field, so the bar does not shift while
        // a tab is renamed.
        .border(px(theme::BORDER_WIDTH))
        .border_color(gpui::transparent_black())
        .whitespace_nowrap()
        .text_size(font_size::BODY)
        .when(shown, |el| {
            el.bg(rgb(theme.card))
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text))
        })
        .when(!shown, |el| el.text_color(rgb(theme.text_muted)))
        // The shown tab is picked, not disabled: the pointer for the double-click rename.
        .pressable_unless(shown, Look::Chip, theme)
}

/// The 1 px rule between two groups of the tab area.
pub(crate) fn title_rule(theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .w(px(theme::BORDER_WIDTH))
        .h(theme::rems_px(theme::TITLE_RULE_HEIGHT))
        .mx(px(space::XS))
        .bg(rgb(theme.divider))
}
```

- [ ] **Step 6: `HostUi` keeps each window's scroll state**

`crates/mkrew-ui/src/shell/hosts.rs`, in `HostUi` after the `strip` field (line 15) add:

```rust
    /// Scrolls the tab area of this window's title bar.
    pub(crate) scroll: ScrollHandle,
    /// The chip last scrolled into view (a tab or view key): it scrolls again only when the
    /// active chip changes, so the wheel is free in between.
    pub(crate) scrolled: RefCell<Option<SharedString>>,
```

(`HostUi` derives `Default`; `ScrollHandle` and `RefCell<Option<_>>` are `Default`.)

- [ ] **Step 7: `pane_render.rs`: the tab area replaces the strip**

Delete `render_tab_bar` (on master lines 6-180, from its doc comment `/// The tab strip: the tabs ...` to its closing `}`; Task 3 left it as the shorter function ending with `.children(self.drop_hint_line(HostId::Main, theme))`).

In its place add (inside `impl Shell`):

```rust
    /// The tab area of the main window's title bar: the Editor / Diff chips (while open), the
    /// pinned tabs, the other tabs, a rule between groups; then `+` outside the scrolled part.
    /// With a view other than Terminals shown, its chip is the active one and every terminal
    /// tab looks inactive. A click on a tab shows Terminals (`tab_chip`).
    pub(crate) fn render_tab_area(
        &self,
        project_id: Ulid,
        tabs: &Tabs,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let view = self.view();
        let active = tabs.active().map(|tab| tab.id);
        // `place` counts the tabs of other windows too: it is the reorder index.
        let drawn: Vec<(usize, &Tab)> = tabs
            .tabs()
            .iter()
            .enumerate()
            .filter(|(_, tab)| self.terminal_host(project_id, tab.id) == HostId::Main)
            .collect();
        let pinned_count = tabs.tabs().iter().filter(|tab| tab.pinned).count();
        let pinned: Vec<bool> = drawn.iter().map(|(_, tab)| tab.pinned).collect();
        let items = area_items(
            self.has_file_tabs(project_id),
            self.diff_open(project_id),
            &pinned,
        );
        let strip = (self.hosts.get(&HostId::Main))
            .map(|ui| ui.strip.clone())
            .unwrap_or_default();
        let chips: Vec<gpui::AnyElement> = items
            .iter()
            .map(|item| match *item {
                AreaItem::View(chip) => self
                    .view_chip(chip, project_id, view, theme, cx)
                    .into_any_element(),
                AreaItem::Rule => title_rule(theme).into_any_element(),
                AreaItem::Tab(slot) => {
                    let (place, tab) = drawn[slot];
                    let tab_id = tab.id;
                    // The tab's own rename field takes its place; a pane is renamed in its
                    // header.
                    if let Some(renaming) = (self.renaming.as_ref())
                        .filter(|r| r.target == RenameTarget::Tab(tab_id))
                    {
                        self.render_rename(&renaming.draft, theme, cx)
                    } else {
                        // The focused pane's title stands in for a default tab name.
                        let label = session_label(tab, self.sidebar.sessions());
                        let item = DraggedItem {
                            list: ReorderList::Tabs(project_id, tab.pinned),
                            index: if tab.pinned {
                                place
                            } else {
                                place - pinned_count
                            },
                            title: label.unwrap_or(&tab.title).to_owned().into(),
                        };
                        let select = move |shell: &mut Shell| {
                            if let Some(tabs) = shell.active_tabs_mut() {
                                tabs.activate(tab_id);
                            }
                        };
                        let chip = self
                            .tab_chip(
                                project_id,
                                tab,
                                active == Some(tab_id) && view == View::Terminals,
                                None,
                                select,
                                theme,
                                cx,
                            )
                            .child(record_strip(strip.clone(), Some(slot)));
                        self.reorderable(chip.into_any_element(), item, true, space::XS, theme, cx)
                    }
                }
            })
            .collect();
        let active_slot = active.and_then(|id| drawn.iter().position(|(_, tab)| tab.id == id));
        let key = SharedString::from(format!("{view:?}/{active:?}"));
        let scroll_to =
            scroll_index(&items, view, active_slot).map(|index| (key, index));
        let new_tab = icon_button("new-tab", Icon::Plus, theme)
            .tooltip(tooltip(
                theme,
                &format!(
                    "New empty tab ({} starts an agent tab, {} a shell tab)",
                    self.keymap.label(Command::NewAgentTab),
                    self.keymap.label(Command::NewShellTab)
                ),
                "",
            ))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.show_view(View::Terminals, window, cx);
                shell.create_tab(project_id, None, window, cx);
            }));
        self.tab_area_frame(
            HostId::Main,
            chips,
            scroll_to,
            Some(new_tab.into_any_element()),
            theme,
            cx,
        )
    }

    /// The frame both windows' tab areas share: `chips` in a sideways-scrolling row (the chip
    /// at `active.1` scrolls into view when `active.0` changes), then `trailing` (the `+`)
    /// outside the scrolled part. It records the strip and its chip slots for drops from other
    /// windows (`HostUi::strip`), takes tab drops and draws the drop line.
    pub(crate) fn tab_area_frame(
        &self,
        host: HostId,
        chips: Vec<gpui::AnyElement>,
        active: Option<(SharedString, usize)>,
        trailing: Option<gpui::AnyElement>,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let ui = self.hosts.get(&host);
        let strip = ui.map(|ui| ui.strip.clone()).unwrap_or_default();
        let scroll = ui.map(|ui| ui.scroll.clone()).unwrap_or_default();
        if let (Some(ui), Some((key, index))) = (ui, active) {
            let mut seen = ui.scrolled.borrow_mut();
            if seen.as_ref() != Some(&key) {
                scroll.scroll_to_item(index);
                *seen = Some(key);
            }
        }
        div()
            .relative()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .font_family(theme.ui_font_family.clone())
            .on_drop(cx.listener(|shell, item: &DraggedItem, window, cx| {
                shell.drop_reordered(item, window, cx);
            }))
            // First: the strip records before its chips (prepaint goes parent first).
            .child(record_strip(strip, None))
            .child(
                div()
                    .id("tab-area-scroll")
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .overflow_x_scroll()
                    .track_scroll(&scroll)
                    .children(chips),
            )
            .children(trailing)
            .children(self.drop_hint_line(host, theme))
    }

    /// The Editor / Diff chip: icon, name and ✕ (all file tabs / the diff). The chip of the
    /// shown view takes the active tab look. Not draggable.
    fn view_chip(
        &self,
        chip: View,
        project_id: Ulid,
        shown: View,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let active = chip == shown;
        let name = chip.title();
        let (glyph, close_tip) = match chip {
            View::Diff => (Icon::FileDiff, "Close diff"),
            _ => (Icon::Code, "Close all files"),
        };
        let group = SharedString::from(format!("view-{name}"));
        tab_frame(group.clone(), active, theme)
            .flex_none()
            .tooltip(tooltip(theme, name, self.keymap.label(chip.command())))
            .on_press(
                true,
                cx.listener(move |shell, _, window, cx| shell.show_view(chip, window, cx)),
            )
            .child(icon_svg(
                glyph,
                theme::ICON_SIZE_SMALL,
                if active { theme.text } else { theme.text_muted },
            ))
            .child(name)
            .child(
                row_action(
                    SharedString::from(format!("view-close-{name}")),
                    icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted),
                    theme,
                )
                // Always on the shown chip, on hover for the other.
                .when(!active, |el| {
                    el.opacity(opacity::HIDDEN)
                        .group_hover(group, |style| style.opacity(opacity::FULL))
                })
                .tooltip(tooltip(theme, close_tip, ""))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    cx.stop_propagation();
                    match chip {
                        View::Diff => shell.close_git_diff(project_id, window, cx),
                        _ => {
                            let path = (shell.files.get(&project_id))
                                .and_then(FilesView::file)
                                .map(str::to_owned);
                            if let Some(path) = path {
                                // Pinned file tabs stay (`Close::All`), as in the tab menu.
                                shell.close_file_tabs(project_id, &path, Close::All, window, cx);
                            }
                        }
                    }
                })),
            )
    }

    /// The Board / Docs icon on the right of the title bar: accent while that view shows (a
    /// second click goes back to Terminals).
    fn view_icon(
        &self,
        target: View,
        glyph: Icon,
        shown: View,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let on = target == shown;
        icon_button_in(
            SharedString::from(format!("view-{}", target.title())),
            glyph,
            if on { theme.accent } else { theme.text_muted },
            theme,
        )
        .tooltip(tooltip(
            theme,
            target.title(),
            self.keymap.label(target.command()),
        ))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.show_view(if on { View::Terminals } else { target }, window, cx);
        }))
    }
```

Edit `tab_chip` (same file; line numbers of master, anchored by text):

(a) Replace the frame, lines 224-248, from `div()` through `.pressable_unless(is_active, Look::Chip, theme)`:

```rust
        div()
            .id(group.clone())
            .group(group.clone())
            .relative()
            ...
            .when(!is_active, |el| el.text_color(rgb(theme.text_muted)))
            // The shown tab is picked, not disabled: the pointer for the double-click rename.
            .pressable_unless(is_active, Look::Chip, theme)
```

with:

```rust
        tab_frame(group.clone(), is_active, theme)
            // Shrinks to `TAB_MIN_WIDTH` before the tab area scrolls.
            .flex_shrink()
            .min_w(px(theme::TAB_MIN_WIDTH))
```

(b) In the `.on_press(true, cx.listener(move |shell, event: &gpui::ClickEvent, window, cx| { ... }))` block, directly after the line `select(shell);` add (the rest of the block, which Task 3 left, stays):

```rust
                    // A tab of the main window shows its terminals, also from Board, Docs,
                    // the Editor or the Diff.
                    if shell.host_of(window) == HostId::Main {
                        shell.show_view(View::Terminals, window, cx);
                    }
```

(c) Replace the title child, lines 290-295:

```rust
            .child(
                div()
                    .max_w(px(theme::TAB_TITLE_MAX_WIDTH))
                    .truncate()
                    .child(label.unwrap_or(&tab.title).to_owned()),
            )
```

with:

```rust
            .child(
                div()
                    .min_w_0()
                    .max_w(px(theme::TAB_TITLE_MAX_WIDTH))
                    .truncate()
                    .child(label.unwrap_or(&tab.title).to_owned()),
            )
```

Update the doc comment of `tab_chip`: "One tab of the title bar's tab area (main or detached): ...".

- [ ] **Step 8: `chrome.rs`: the title bar**

Replace `render_title_bar` (its doc comment `/// The title bar above everything ...` through the closing `}` before `/// In a machine window, right after the logo`; lines 297-475 on master) with the full function:

```rust
    /// The title bar above everything (SPEC §10, ADR 0106), one `TITLE_BAR_HEIGHT` bar: on the
    /// left (main window) the machine chip, the sidebar toggle and the workspace switcher; the
    /// tab area (Editor / Diff chips, the terminal tabs, `+`); a drag area; on the right the
    /// voice status, the limits chip (only at 80% or more), the Run pill, the update button,
    /// the Board and Docs icons, Split, the panel toggle and "+ New"; on Windows minimize,
    /// maximize, close. A detached window (`host` is not `Main`) has no left group, no
    /// workspace controls and no limits chip. Every icon is gray: color is for status only.
    /// Without a workspace only the sidebar toggle, the drag area and the controls show, so
    /// the window can always be moved and closed.
    pub(crate) fn render_title_bar(
        &self,
        host: HostId,
        now: i64,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let main = host == HostId::Main;
        let workspace = self
            .workspace
            .filter(|_| main && self.shows_workspaces());
        let view = self.view();
        // An open dialog keeps the mouse too: the bar's controls are inert, only the drag area
        // and the window controls stay live (they never touch the app's state).
        let modal = self.modal_open() || self.reconnecting();
        let mut left = div().min_w_0().flex().items_center().gap(px(space::S));
        if main {
            left = left
                .children(self.machine_chip(theme))
                .child(self.sidebar_toggle(theme, cx));
        }
        if let Some(project_id) = workspace {
            left = left.child(self.render_workspace_switcher(project_id, theme, cx));
        }
        // The tab area; its strip is only recorded (for drops from other windows) while it is
        // painted.
        let tab_area = workspace.and_then(|project_id| {
            let tabs = self.tabs.get(&project_id)?;
            Some(self.render_tab_area(project_id, tabs, theme, cx).into_any_element())
        });
        if main && tab_area.is_none() && let Some(ui) = self.hosts.get(&HostId::Main) {
            *ui.strip.borrow_mut() = None;
        }
        // The only drag region: the tab area is a sibling, so a click on a tab never moves
        // the window.
        let drag = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .window_control_area(WindowControlArea::Drag)
            .when(cfg!(target_os = "macos"), |el| {
                el.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, event: &MouseDownEvent, window, _| {
                        if event.click_count == 2 {
                            window.titlebar_double_click();
                        } else {
                            window.start_window_move();
                        }
                    }),
                )
            });
        let mut bar = div()
            .flex_none()
            .h(theme::rems_px(theme::TITLE_BAR_HEIGHT))
            .flex()
            .items_center()
            .gap(px(space::S))
            .pl(px(theme::title_bar_inset(
                Os::CURRENT,
                window.is_fullscreen(),
            )))
            .bg(rgb(theme.chrome))
            .border_b(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .child(left.when(modal, |el| el.relative().child(inert_layer())))
            .children(tab_area.map(|area| {
                div()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .child(area)
                    .when(modal, |el| el.relative().child(inert_layer()))
            }))
            .child(drag);
        let controls = DRAWN_CONTROLS && !window.is_fullscreen();
        let mut right = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .when(!controls, |el| el.pr(px(space::M)))
            // The sidebar footer shows voice input; with the sidebar hidden it shows here, so a
            // live microphone is never unmarked.
            .when(self.sidebar_layout.hidden(), |el| {
                el.children(self.voice_status(false, theme))
            });
        if main {
            right = right.children(self.render_limits_chip(theme, cx));
        }
        if let Some(project_id) = workspace {
            right = right.child(self.render_run_button(project_id, theme, cx));
        }
        if main {
            right = right.children(self.render_update_button(theme, cx));
        }
        if workspace.is_some() {
            right = right
                .child(self.view_icon(View::Board, Icon::SquareKanban, view, theme, cx))
                .children(
                    self.features
                        .docs
                        .then(|| self.view_icon(View::Docs, Icon::BookOpen, view, theme, cx)),
                );
        }
        // Split acts on the shown terminal tab: only while Terminals shows in the main window.
        if !main || (workspace.is_some() && view == View::Terminals) {
            right = right.child(
                icon_button("tab-split", Icon::Columns2, theme)
                    .tooltip(tooltip(
                        theme,
                        "Split or pick a layout",
                        self.keymap.label(Command::SplitRight),
                    ))
                    .on_click(cx.listener(|shell, event: &gpui::ClickEvent, window, cx| {
                        shell.open_layout_menu(event.position(), window, cx);
                    })),
            );
        }
        if workspace.is_some() {
            right = right.child(self.inspector_toggle(theme, cx)).child(
                div()
                    .pl(px(space::XS))
                    .child(self.render_new_button(now, theme, cx)),
            );
        }
        bar = bar.child(right.when(modal, |el| el.relative().child(inert_layer())));
        if controls {
            // No click handlers: Windows hit-tests these areas (`HTMINBUTTON` / `HTMAXBUTTON` /
            // `HTCLOSE`) and runs the action itself; `HTMAXBUTTON` also brings the Snap
            // Layouts flyout.
            let max_icon = if window.is_maximized() {
                Icon::Copy
            } else {
                Icon::Square
            };
            bar = bar.child(
                div()
                    .flex_none()
                    .h_full()
                    .flex()
                    .items_center()
                    // A thin rule sets the window's own controls apart from the app's.
                    .child(
                        div()
                            .w(px(theme::BORDER_WIDTH))
                            .h(theme::rems_px(theme::TITLE_RULE_HEIGHT))
                            .mr(px(space::XS))
                            .bg(rgb(theme.divider)),
                    )
                    .child(window_control(
                        "win-min",
                        Icon::Minus,
                        WindowControlArea::Min,
                        theme,
                    ))
                    .child(window_control(
                        "win-max",
                        max_icon,
                        WindowControlArea::Max,
                        theme,
                    ))
                    .child(window_control(
                        "win-close",
                        Icon::X,
                        WindowControlArea::Close,
                        theme,
                    )),
            );
        }
        bar
    }
```

(If Task 2 placed the limits chip or voice status slightly differently in the old function, this body replaces it as a whole: the right group order is voice status, limits chip, Run, update, Board, Docs, Split, panel toggle, `+ New`.)

Delete from `chrome.rs`:

- `open_repo_menu` (18-22) and `repo_menu_rows` (24-43, with its doc comment).
- `render_view_tabs` (518-543, with its doc comment).

In `render_workspace_switcher` (545-701): delete lines 611-681 (`let repos = ...` through the end of `let repo_menu = ...;`) and replace the last block (693-700) with the shorter tail; update its doc comment to "The workspace name with ▾: opens a dropdown of every workspace (click switches) and \"Add workspace…\"."

```rust
        div()
            .min_w_0()
            .child(button)
            .children(menu)
            .into_any_element()
```

In `render_main`:

(a) Delete lines 194-201 (the comment `// The tab strip is hit-tested by drops ...`, `let strip_shown = ...;` and the `if !strip_shown && let Some(ui) ... { ... }` block): the title bar now owns the strip record.

(b) Replace the `terminals` block (lines 245-274) with:

```rust
                let terminals = div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(if every_tab_detached {
                        empty_state(
                            theme,
                            Some(">_"),
                            Some("Every tab of this workspace is in another window"),
                            "Press + for a new tab here.".into(),
                            Some("Or use Move all back in the other window.".into()),
                        )
                        .flex_1()
                    } else {
                        self.render_tab(
                            tabs.active(),
                            now,
                            self.composer_focus.is_focused(window),
                            voice_ring,
                            theme,
                            self.hosts
                                .get(&HostId::Main)
                                .map(|ui| ui.content.clone())
                                .unwrap_or_default(),
                            cx,
                        )
                    });
```

Update the top doc of the file (line 1) to "Window chrome: the main area, title bar, branch-less workspace switcher." and replace the `render_main` call site in `main.rs:1097`:

```rust
            .child(self.render_title_bar(HostId::Main, now, &theme, window, cx))
```

- [ ] **Step 9: Delete the repo menu state**

- `main.rs` 630-631: delete the field `repo_menu: bool` and its doc comment; `connection.rs` 380 `repo_menu: false,`; `focus.rs` 129 `self.repo_menu = false;`.
- `menu_render.rs`: delete `|| self.repo_menu` (line 329) and the arm (372-376):

```rust
        if self.repo_menu {
            return self
                .workspace
                .map(|project_id| self.repo_menu_rows(project_id));
        }
```

- `main.rs` imports (111-114): add `AreaItem`, `area_items`, `scroll_index`:

```rust
use mkrew_ui::tabs::{
    AreaItem, FIRST_TAB_TITLE, Tab, TabKind, Tabs, area_items, double_click_renames,
    newest_unplaced, on_screen, rename_params, scroll_index, session_label, tab_kind,
};
```

(keep the other names exactly as Task 3 left that list; only add the three.)

- [ ] **Step 10: Delete the buttons that left the bar and what only they used**

`phone_access.rs`: delete `render_phone_button` with its doc comment (162-201). The palette entry "Global: Phone access…" stays.

`board.rs`: delete `View::bar` with its doc comment (47-56 and the blank line). `board/tests.rs`: delete `the_view_bar_lists_the_editor_and_the_diff_only_while_open` (21-42 and the blank line after).

`git_summary.rs`: the branch chip tooltip and the repository menu rows are gone.

- Line 1-4 doc: `//! The git state of the shown workspace (SPEC §10, ADR 0029): the `Git N` badge and the branch name of the chat's branch chip. One `RepoSummary` per repository ... Pure; `main.rs` loads it.`
- Delete `use crate::git_history::short_hash;` (10), `const ROOT_NAME` with its doc (12-13), `RepoSummary::label` (52-59), `RepoSummary::state` (66-73), `RepoSummary::line` (75-87), `GitSummary::branch_tooltip` (111-123).
- Tests: in `one_repo_label_is_its_branch` delete the `branch_tooltip` assert (163-166); in `many_repos_label_counts_them_and_the_tooltip_lists_each` delete 176-179 and rename it `many_repos_label_counts_them`; in `a_detached_head_still_has_a_label` delete 231-239 (keep the `branch_label` assert); delete `dropdown_rows_name_each_repo_with_its_branch_and_state` (242-270); in `no_repository_means_no_chip_and_no_badge` use `(s.branch_label(), s.changed())` against `(None, 0)`.

Comments: `sidebar.rs` 566-567 becomes `// The filter row has the height of the right panel's tab row and the Editor's file tab strip: same height, same bottom divider.`; `inspector.rs` 293-294 becomes `// The tab row has the height of the Editor's file tab strip and the sidebar's filter row on its left: same height, same bottom divider.`

- [ ] **Step 11: Build, test, lint**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui`
Expected: PASS (the two new tab tests, the icon test with the new svg, `git_summary` tests, `board` tests without the view bar test).

Run: `CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings. Typical failures to fix, not to allow: unused import (`segmented` is still used by Board and the sidebar; `short_hash` import removed above), unused variable after the deletions.

- [ ] **Step 12: Smoke on the real app (UI cannot be unit tested)**

Run `CARGO_BUILD_JOBS=4 cargo run -p mkrew-ui` against a daemon with one workspace, then:

1. The strip row under the title bar is gone; the tabs are in the title bar after the workspace switcher; no logo, branch chip, History, theme or phone button.
2. Open 10 tabs in a window about 1100 px wide: the chips shrink to the minimum, then the row scrolls; the wheel scrolls it sideways; creating or activating a tab scrolls it into view; `+` stays visible.
3. Click a tab from Board, Docs (when enabled), Editor and Diff: Terminals shows. Board and Docs icons turn accent while shown and a second click goes back.
4. Open a file and a diff: the Editor and Diff chips appear, the shown one has the card look and every tab looks inactive; ✕ on Editor closes the file tabs, ✕ on Diff closes the diff.
5. Drag a tab to reorder, drag it out to a detached window; a drop line shows in the bar; right-click menu and double-click rename still work.
6. Drag the window by the empty bar space and double-click it to maximize; a click on a tab does not move the window.
7. Dark and light theme: the active chip is visibly raised, inactive chips have a hover fill.

- [ ] **Step 13: Commit**

```
git add crates/mkrew-ui/src/tabs.rs crates/mkrew-ui/src/tabs/tests.rs crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/icons.rs icon/lucide/square-kanban.svg crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/shell/hosts.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/chrome.rs crates/mkrew-ui/src/shell/menu_render.rs crates/mkrew-ui/src/shell/focus.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/phone_access.rs crates/mkrew-ui/src/shell/sidebar.rs crates/mkrew-ui/src/shell/inspector.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/board.rs crates/mkrew-ui/src/board/tests.rs crates/mkrew-ui/src/git_summary.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): move the tab strip into the title bar, view chips and Board/Docs icons"
```

**Risks for the reviewer:**

- Scrolled chip bounds: `record_strip` records chip bounds at prepaint; I did not verify that gpui gives a canvas inside an `overflow_x_scroll` element its scrolled bounds, which drag-out targets and the drop line depend on [INFERENCE]. Smoke step 5 with a scrolled strip covers it.
- `scroll_to_item` of a chip that is still pending when the tab list changes (a tab created and activated before its chip is laid out): it stays pending until the child exists (`div.rs:4380`); not covered by tests.
- The Editor chip ✕ uses `Close::All`, which leaves pinned file tabs open, so the chip can stay.
- The `{view:?}/{active:?}` scroll key re-scrolls on workspace switch (new active tab id), which is intended but untested.
- The Split button is hidden outside Terminals in the main window, a small change from the spec's "right group".

---

### Task 5: Top bar part 2: merged `+ ▾` menu, toasts for the strip's notices

Line numbers are those of master 7cf71e0; Tasks 1 to 4 shifted them, so find each place by the quoted text.

Contract notes:

- The `⋯` button already went with `render_tab_bar` in Task 4; Task 5 does the merge of `+` and `+ New ▾`, and the notices.
- `Shell::still_starting` loses its `window` parameter (it no longer needs the host of a strip), so its two callers change.
- `create_tab` takes `start: Option<(SessionKind, Option<String>)>` (kind and optional agent profile) so a profile row can start a tab with that profile.
- Keymap names used: `Command::NewAgentTab` (starts the default agent in a new tab), `Command::NewShellTab`. The old rows used `Command::SplitAgent` / `SplitShell`; those commands and Split ▾ keep their behavior.

**Files:**

- Modify `crates/mkrew-ui/src/lifecycle.rs`: `running_alert` after `running_warning` (219-225), test after the one at 429-444, import `HashSet` (4-8).
- Modify `crates/mkrew-ui/src/shell/workspace.rs`: `create_tab` (98-168), `save_layout` (361-386).
- Modify `crates/mkrew-ui/src/shell/tabs.rs`: `new_tab` (39-50) and new `open_new_tab`; `apply_layout` (65-167); `send_edit` (469-499); `close_tabs` (644-722); `kill` (829-854).
- Modify `crates/mkrew-ui/src/shell/sessions.rs`: `new_pane_with` (20-79), `still_starting` (91-104), `ask_for_default` (106-134).
- Modify `crates/mkrew-ui/src/shell/pane_drop.rs` (210-249), `focus.rs` (384-387), `connection.rs` (315-316, 856, tick 158-161, init), `toasts.rs` (139-157), `detached_render.rs` (336-345).
- Modify `crates/mkrew-ui/src/shell/chrome.rs`: `new_menu_rows` (76-135), `render_new_button` (703-751) becomes `render_new_tab_button`, `render_title_bar` (the `+ New` lines), `open_new_menu` doc.
- Modify `crates/mkrew-ui/src/shell/pane_render.rs`: `render_tab_area` (Task 4).
- Modify `crates/mkrew-ui/src/main.rs` (490-494 fields, 632-633 doc, new field), `crates/mkrew-ui/src/theme.rs` (623-624), `crates/mkrew-ui/src/shell/widgets.rs` (doc of `notice`, line 89).

**Interfaces:**

Consumes: `lifecycle::{running_warning, running_count, RUNNING_WARN_ABOVE, STILL_STARTING, kind_noun}`, `Shell::{toast_error, toast_info, show_view, create_tab, new_pane_with, pane_agents}`, `menu::MenuItem` (`new`, `lead`, `checked`, `keys`, `row`), `act`, `render_menu`, `loading_dots`, `keymap::Keymap::label`.

Produces:

```rust
// crates/mkrew-ui/src/lifecycle.rs
pub fn running_alert(sessions: &[Session], project_id: Ulid, warned: &mut HashSet<Ulid>) -> Option<String>;

// crates/mkrew-ui/src/shell/workspace.rs
pub(crate) fn create_tab(&mut self, project_id: Ulid, start: Option<(SessionKind, Option<String>)>, window: &mut Window, cx: &mut Context<'_, Self>);

// crates/mkrew-ui/src/shell/tabs.rs
pub(crate) fn open_new_tab(&mut self, start: Option<(SessionKind, Option<String>)>, window: &mut Window, cx: &mut Context<'_, Self>);

// crates/mkrew-ui/src/shell/sessions.rs
pub(crate) fn still_starting(&mut self, cx: &mut Context<'_, Self>) -> bool;

// crates/mkrew-ui/src/shell/toasts.rs
pub(crate) fn warn_running_sessions(&mut self, cx: &mut Context<'_, Self>);

// crates/mkrew-ui/src/shell/chrome.rs
pub(crate) fn render_new_tab_button(&self, now: i64, theme: &Theme, cx: &mut Context<'_, Self>) -> gpui::AnyElement;
// render_tab_area gains `now: i64` (after `tabs`)
```

Removed: `Shell::{tab_error, tab_notice}` fields, `Shell::{show_tab_notice, render_new_button}`, `theme::TAB_NOTICE_TIMEOUT`.

Design of the running warning: the count of running sessions of the shown workspace is checked on the 500 ms status tick. `running_alert` returns the text the first time the count is above `RUNNING_WARN_ABOVE` and records the workspace in `warned`; a count at or below the limit removes it, so the next crossing warns again. One `toast_info` per crossing; `Toasts::push` already refreshes an identical toast instead of stacking.

- [ ] **Step 1: Write the failing running alert test**

In `crates/mkrew-ui/src/lifecycle.rs`, inside `mod tests`, after the test that ends at line 444 (`assert_eq!(running_warning(&sessions, other), None);` `}`), add:

```rust
    /// One alert per upward crossing of the limit: not repeated while the count stays above
    /// it, armed again once it is back at or below the limit; per workspace.
    #[test]
    fn the_running_alert_fires_once_per_crossing_and_rearms_at_the_limit() {
        let (project, other) = (Ulid::new(), Ulid::new());
        let mut sessions: Vec<Session> = (0..RUNNING_WARN_ABOVE)
            .map(|_| session(project, SessionKind::Agent, SessionState::Working))
            .collect();
        let mut warned = HashSet::new();
        assert_eq!(running_alert(&sessions, project, &mut warned), None, "at the limit");

        sessions.push(session(project, SessionKind::Agent, SessionState::Working));
        let text = running_alert(&sessions, project, &mut warned).unwrap();
        assert!(text.starts_with("13 sessions running"), "{text}");
        assert_eq!(running_alert(&sessions, project, &mut warned), None, "not repeated");
        sessions.push(session(project, SessionKind::Agent, SessionState::Working));
        assert_eq!(running_alert(&sessions, project, &mut warned), None, "still above");

        sessions.truncate(RUNNING_WARN_ABOVE);
        assert_eq!(running_alert(&sessions, project, &mut warned), None, "back at the limit");
        sessions.push(session(project, SessionKind::Agent, SessionState::Working));
        assert!(running_alert(&sessions, project, &mut warned).is_some(), "armed again");

        assert_eq!(running_alert(&sessions, other, &mut warned), None, "another workspace");
    }
```

- [ ] **Step 2: Run it and see it fail**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib lifecycle::tests::the_running_alert`
Expected: FAIL to compile with `error[E0425]: cannot find function \`running_alert\` in this scope` (and `HashSet` not found).

- [ ] **Step 3: Add `running_alert`**

`crates/mkrew-ui/src/lifecycle.rs`: add `use std::collections::HashSet;` above the `use mkrew_protocol::{` line, change the doc of `RUNNING_WARN_ABOVE` to "Above this many running sessions in one workspace a toast warns.", the doc of `STILL_STARTING` to "Start of the toast shown while a pane is still starting (\"Still starting an agent…\").", and after `running_warning` add:

```rust
/// The warning to toast for `project_id` now: its text the first time the running count is
/// above [`RUNNING_WARN_ABOVE`], then `None` until the count has been at or below it (`warned`
/// remembers the workspaces already told).
pub fn running_alert(
    sessions: &[Session],
    project_id: Ulid,
    warned: &mut HashSet<Ulid>,
) -> Option<String> {
    match running_warning(sessions, project_id) {
        Some(text) => warned.insert(project_id).then_some(text),
        None => {
            warned.remove(&project_id);
            None
        }
    }
}
```

- [ ] **Step 4: Run it and see it pass**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib lifecycle`
Expected: PASS.

- [ ] **Step 5: The warning toast on the status tick**

`main.rs`: delete the fields `tab_error` and `tab_notice` with their doc comments (490-494) and add in their place:

```rust
    /// Workspaces already warned about their running sessions (`lifecycle::running_alert`).
    running_warned: HashSet<Ulid>,
```

`connection.rs` init (315-316): replace `tab_error: None,` / `tab_notice: None,` with `running_warned: HashSet::new(),`; in `reset_connection_state` replace `self.tab_error = None;` (856) with `self.running_warned.clear();`.

`connection.rs` tick, after the toast expiry block (`if shell.toasts.expire(Instant::now()) { cx.notify(); }`) add:

```rust
                    shell.warn_running_sessions(cx);
```

`toasts.rs`: replace `show_tab_notice` with its doc (139-157) by:

```rust
    /// One toast when the shown workspace's running sessions cross
    /// `lifecycle::RUNNING_WARN_ABOVE` upward; not again until the count is back at or below it.
    pub(crate) fn warn_running_sessions(&mut self, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.workspace else {
            return;
        };
        let alert = lifecycle::running_alert(
            self.sidebar.sessions(),
            project_id,
            &mut self.running_warned,
        );
        if let Some(text) = alert {
            self.toast_info(text, cx);
        }
    }
```

`theme.rs`: delete `TAB_NOTICE_TIMEOUT` and its doc (623-624 and the blank line). `widgets.rs` line 89 doc of `notice`: "A one-line notice, shortened to fit; `color` sets errors apart from warnings."

`detached_render.rs`: delete the two `.children(` blocks that read `self.tab_error` and `self.tab_notice` (336-345; Task 6 deletes the whole strip later, this keeps the build green).

- [ ] **Step 6: Strip errors become toasts**

Every `tab_error` write becomes `toast_error`; every `tab_error = None` and `take_if(STILL_STARTING)` is deleted (the toast stack replaces them; `Toasts::push` refreshes a repeat instead of stacking). The `host` locals that only fed the error go.

`sessions.rs`, `new_pane_with`: delete lines 34-36 (`if profile.is_some() { self.tab_error = None; }`); line 37 `self.still_starting(window, cx)` becomes `self.still_starting(cx)`; delete line 44 `let host = self.host_of(window);`; delete lines 51-53 (`shell.tab_error.take_if(...)`); line 72 becomes:

```rust
                        shell.toast_error(format!("Cannot start {what}: {error:#}"), cx);
```

`still_starting` becomes:

```rust
    /// True (after a toast) while another pane is still starting: one `session.spawn` at a
    /// time.
    pub(crate) fn still_starting(&mut self, cx: &mut Context<'_, Self>) -> bool {
        let Some(kind) = self.starting_pane else {
            return false;
        };
        let (still, what) = (lifecycle::STILL_STARTING, lifecycle::kind_noun(kind));
        self.toast_error(format!("{still} {what}; try again once it shows."), cx);
        true
    }
```

`ask_for_default`: delete line 115 (`self.tab_error = None;`) and replace the `if missing { ... }` block (122-132) with:

```rust
        if missing {
            self.toast_error(
                format!("Pick a default {what} for this workspace, then try again."),
                cx,
            );
            // The menu hangs on the main window's title bar: not opened from a detached one.
            if self.detached_key(window).is_none() {
                self.open_workspace_menu_at_switcher(project_id, window, cx);
            }
        }
```

`tabs.rs`:

- `apply_layout`: delete `self.tab_error = None;` (83); replace the block 84-95 with
  ```rust
          if have > preset.panes() {
              self.toast_error(
                  format!(
                      "This tab has {have} panes; the {} layout holds {}. Close panes first.",
                      preset.label(),
                      preset.panes()
                  ),
                  cx,
              );
              return;
          }
  ```
  line 110 `self.still_starting(window, cx)` becomes `self.still_starting(cx)`; delete `let host = self.host_of(window);` (115); delete lines 134-136 (`shell.tab_error.take_if(...)`); line 160 becomes `shell.toast_error(format!("Cannot start {what}: {error}"), cx);`.
- `send_edit`: delete `self.tab_error = None;` (482) and `let host = self.host_of(window);` (484); line 492 becomes `shell.toast_error(format!("{failure}: {error:#}"), cx);`. Its doc: "a failure toasts after `failure`." The `window` parameter stays (the callers and `cx.spawn_in(window, ..)` use it).
- `close_tabs`: delete `self.tab_error = None;` (681); lines 709-711 become `shell.toast_error(format!("Cannot close the tab: {error:#}"), cx);`.
- `kill`: delete `let host = self.host_of(window);` (843); line 848 becomes `shell.toast_error(format!("Cannot kill the session: {error:#}"), cx);` and drop the following `cx.notify();` (849); doc "a failure toasts."

`workspace.rs`:

- `create_tab`: delete `self.tab_error = None;` (113); lines 159-160 become `shell.toast_error(format!("Cannot add a tab: {error}"), cx);` (delete the `let host = shell.host_of(window);`).
- `save_layout`: delete `let host = self.host_of(window);` (372); line 380 becomes `shell.toast_error(format!("Cannot save the layout: {error:#}"), cx);`.

`pane_drop.rs` (`drop_tab_on_pane`): delete `self.tab_error = None;` (210) and `let host = self.host_of(window);` (213); lines 223-225 become `shell.toast_error(format!("Cannot save the layout: {error:#}"), cx);`; delete `let host = shell.host_of(window);` (242) and make line 243 `shell.toast_error(format!("Cannot close the tab: {error:#}"), cx);`.

`focus.rs` 384-387 becomes:

```rust
                cx.subscribe(&pane, |shell, _, _: &NoLinks, cx| {
                    shell.toast_info("No links on screen.", cx);
                })
                .detach();
```

- [ ] **Step 7: `create_tab` takes a profile; `open_new_tab`**

`workspace.rs`, replace the head of `create_tab` (the doc, signature and the `ask_for_default` check, lines 97-109):

```rust
    /// Adds a tab to a workspace and activates it; `start` (a kind, and an agent profile or
    /// the workspace's default of that kind) then spawns a session into it.
    pub(crate) fn create_tab(
        &mut self,
        project_id: Ulid,
        start: Option<(SessionKind, Option<String>)>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some((kind, None)) = &start
            && self.ask_for_default(project_id, *kind, window, cx)
        {
            return;
        }
```

and the spawn after the tab exists (lines 146-150):

```rust
                        if let Some((kind, profile)) = start
                            && shell.workspace == Some(project_id)
                        {
                            shell.new_pane_with(kind, profile, Axis::Vertical, window, cx);
                        }
```

`tabs.rs`: replace `new_tab` (38-50) with:

```rust
    /// A new tab in the shown workspace with the default agent or shell in it.
    pub(crate) fn new_tab(
        &mut self,
        kind: SessionKind,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(project_id) = self.workspace
            && self.tabs.contains_key(&project_id)
        {
            self.create_tab(project_id, Some((kind, None)), window, cx);
        }
    }

    /// A row of the `+ ▾` menu: a new tab in the shown workspace (`None` = an empty one,
    /// else a kind and an agent profile), and Terminals shows it.
    pub(crate) fn open_new_tab(
        &mut self,
        start: Option<(SessionKind, Option<String>)>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(project_id) = self.workspace
            && self.tabs.contains_key(&project_id)
        {
            self.show_view(View::Terminals, window, cx);
            self.create_tab(project_id, start, window, cx);
        }
    }
```

The other `create_tab(project_id, None, ..)` calls (`tabs.rs:703`, `workspace.rs:76`) are unchanged; the old strip `+` was in the deleted `render_tab_bar`.

- [ ] **Step 8: The merged `+ ▾` button and menu**

`chrome.rs`: replace `new_menu_rows` and its doc (76-135) with:

```rust
    /// The `+ ▾` menu: one row per installed agent profile (the default first and checked,
    /// with the agent tab shortcut), the plain "Agent" row while none is listed, "Shell" and
    /// "Empty tab". Every row opens a new tab and shows Terminals; a new pane in the current
    /// tab comes from Split.
    pub(crate) fn new_menu_rows(&self) -> Rows {
        let mut rows: Rows = Vec::new();
        for agent in self.pane_agents() {
            let profile = agent.id.clone();
            let item = MenuItem::new(
                agent.name.clone(),
                act(move |shell, window, cx| {
                    shell.open_new_tab(
                        Some((SessionKind::Agent, Some(profile.clone()))),
                        window,
                        cx,
                    );
                }),
            )
            .lead(Lead::Agent(sidebar::agent_badge(
                agent.icon.as_deref(),
                &agent.id,
            )))
            .checked(agent.default);
            rows.push(
                if agent.default {
                    item.keys(self.keymap.label(Command::NewAgentTab))
                } else {
                    item
                }
                .row(),
            );
        }
        if rows.is_empty() {
            rows.push(
                MenuItem::new(
                    "Agent",
                    act(|shell, window, cx| {
                        shell.open_new_tab(Some((SessionKind::Agent, None)), window, cx);
                    }),
                )
                .keys(self.keymap.label(Command::NewAgentTab))
                .row(),
            );
        }
        rows.push(
            MenuItem::new(
                "Shell",
                act(|shell, window, cx| {
                    shell.open_new_tab(Some((SessionKind::Shell, None)), window, cx);
                }),
            )
            .keys(self.keymap.label(Command::NewShellTab))
            .row(),
        );
        rows.push(
            MenuItem::new(
                "Empty tab",
                act(|shell, window, cx| shell.open_new_tab(None, window, cx)),
            )
            .row(),
        );
        rows
    }
```

Doc of `open_new_menu` (line 12) stays; the doc of the `new_menu` field in `main.rs` (632) becomes "The tab area's `+ ▾` dropdown is open."

Replace `render_new_button` with its doc (703-751) by:

```rust
    /// `+ ▾` after the last tab: a dropdown to start a tab with an agent profile, the shell or
    /// nothing (`new_menu_rows`). While a pane starts it shows the loading dots instead (one
    /// start at a time).
    pub(crate) fn render_new_tab_button(
        &self,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        if let Some(kind) = self.starting_pane {
            let what = match kind {
                SessionKind::Agent => "Starting agent",
                SessionKind::Shell => "Starting shell",
            };
            return div()
                .id("new-tab")
                .flex_none()
                .min_w(theme::rems_px(theme::CONTROL_HEIGHT))
                .h(theme::rems_px(theme::CONTROL_HEIGHT))
                .px(px(space::S))
                .flex()
                .items_center()
                .justify_center()
                .tooltip(tooltip(theme, what, ""))
                .child(loading_dots(now, rgb(theme.text_muted)))
                .into_any_element();
        }
        let button = div()
            .id("new-tab")
            .flex_none()
            .h(theme::rems_px(theme::CONTROL_HEIGHT))
            .px(px(space::S))
            .flex()
            .items_center()
            .gap(px(space::XS))
            .rounded(px(radius::MEDIUM))
            .text_color(rgb(theme.text_muted))
            .pressable(Look::Icon, true, theme)
            // Not while its dropdown is open: the tooltip would cover the rows.
            .when(!self.new_menu, |el| {
                el.tooltip(tooltip(
                    theme,
                    "New tab: agent, shell or empty",
                    format!(
                        "{} / {}",
                        self.keymap.label(Command::NewAgentTab),
                        self.keymap.label(Command::NewShellTab)
                    ),
                ))
            })
            // An open menu already closed on this mouse down (outside it): the click reopens it.
            .on_press(
                true,
                cx.listener(|shell, _, window, cx| {
                    shell.open_new_menu(window, cx);
                }),
            )
            .child(icon_svg(Icon::Plus, theme::ICON_SIZE, theme.text_muted))
            .child(icon_svg(
                Icon::ChevronDown,
                theme::ICON_SIZE_SMALL,
                theme.text_muted,
            ));
        let menu = (self.new_menu && !self.modal_open()).then(|| {
            render_menu(
                self.menu_state(),
                "new-tab-panel",
                self.new_menu_rows(),
                None,
                Place::Dropdown,
                theme,
                cx,
            )
        });
        div().flex_none().child(button).children(menu).into_any_element()
    }
```

`pane_render.rs`, `render_tab_area`: add `now: i64` after `tabs` in the signature, and replace the `let new_tab = icon_button("new-tab", ...)...;` block and the `Some(new_tab.into_any_element())` argument with:

```rust
        self.tab_area_frame(
            HostId::Main,
            chips,
            scroll_to,
            Some(self.render_new_tab_button(now, theme, cx)),
            theme,
            cx,
        )
```

`chrome.rs`, `render_title_bar`: the call becomes `self.render_tab_area(project_id, tabs, now, theme, cx)` and the `if workspace.is_some()` block at the end of the right group loses `+ New`:

```rust
        if workspace.is_some() {
            right = right.child(self.inspector_toggle(theme, cx));
        }
```

- [ ] **Step 9: Build, test, lint**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui`
Expected: PASS (`lifecycle` tests including the alert, no test referenced `tab_error`).

Run: `CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings (an unused `window` parameter or `host` local left behind by Step 6 is the likely failure; delete it).

- [ ] **Step 10: Smoke on the real app**

1. Click `+ ▾`: rows are the agent profiles (default first with a check and the shortcut), Shell, Empty tab. Each row opens a new tab, also when started from Board or the Editor, and Terminals shows. The title bar's `+ New ▾` and the `⋯` button are gone; right-click on a tab still opens its menu. The tooltip of `+ ▾` shows the keymap labels of the agent tab and shell tab shortcuts.
2. Click an agent row twice quickly: the button shows the loading dots while the pane starts; the second start gives a "Still starting an agent; try again once it shows." error toast.
3. Stop the daemon's ability to spawn (use a workspace whose default agent is not installed, or point it at a missing exe): the row gives an error toast "Cannot start agent: ..."; `Global: Copy last error` copies it.
4. Make a workspace with no default shell and press Alt+Shift+S / the Shell row: a toast says to pick a default and the workspace menu opens.
5. Press the link hint key in a pane with no links on screen: an info toast "No links on screen.".
6. Start 13 sessions in one workspace: one toast "13 sessions running in this workspace. ..."; start a 14th: no new toast; close down to 12, then start a 13th: the toast shows again.
7. Drop a tab on a pane edge with the daemon stopped: an error toast "Cannot save the layout: ..." appears and does not stack on a repeat.

- [ ] **Step 11: Commit**

```
git add crates/mkrew-ui/src/lifecycle.rs crates/mkrew-ui/src/shell/workspace.rs crates/mkrew-ui/src/shell/tabs.rs crates/mkrew-ui/src/shell/sessions.rs crates/mkrew-ui/src/shell/pane_drop.rs crates/mkrew-ui/src/shell/focus.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/toasts.rs crates/mkrew-ui/src/shell/detached_render.rs crates/mkrew-ui/src/shell/chrome.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/theme.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): merge + New into the tab + menu, strip notices become toasts"
```

**Risks for the reviewer:**

- The running warning is checked on the 500 ms tick for the shown workspace only; a crossing in a workspace that is not shown warns when it is shown next (its flag is not set before). Not covered by a test; `running_alert` itself is.
- `Still starting` used to clear itself when the pane was up; as a toast it now lasts the toast duration.
- A failure in a detached window now toasts in the window `toast_host()` picks (the most recently activated visible one), not necessarily the window of the action.
- `create_tab` with a profile skips `ask_for_default` by design (a picked profile needs no default); the spawn can still fail and then toasts.
- `Look::Icon` on the `+ ▾` button is assumed to give the same hover fill as `icon_button`; check it in both themes.
### Task 6: Detached windows use the shared title bar

Contract deviations, read first:

- A detached window holds tabs of several workspaces (`DockTab::Terminal { project, tab_id }` from `self.detached.tabs_of(key)`), so Task 4's `render_tab_area(project_id, tabs, now, theme, cx)` (main only, one workspace, `+ ▾`) cannot draw it.
  This task uses Task 4's final (agreed with PlanT4T5): `pub(crate) fn tab_area_frame(&self, host: HostId, chips: Vec<gpui::AnyElement>, active: Option<(SharedString, usize)>, trailing: Option<gpui::AnyElement>, theme: &Theme, cx: &mut Context<'_, Self>) -> impl IntoElement` in `pane_render.rs`.
  It is the scroll container (per-window scroll handle in `HostUi`), `record_strip(strip, None)`, the `on_drop` that calls `drop_reordered`, `drop_hint_line(host, theme)` and the `trailing` slot outside the scroll area; `active` is (key `format!("tab-{tab_id}")` of the active chip, its index in `chips`) and the frame scrolls it into view when the key changes.
- `render_title_bar(&self, host: HostId, now: i64, theme: &Theme, window: &Window, cx)` keeps the existing parameter style (no `shown`); it is gated by `let main = host == HostId::Main`, and its `tab_area` is an `Option<AnyElement>` from a `match host` whose `HostId::Detached(_)` arm is `None` today.
- `tab_chip(project_id, tab, is_active, prefix: Option<&str>, select, theme, cx)` keeps its signature; it switches to Terminals itself only for the main window.
- Task 4 leaves `app_logo` (`widgets.rs`) and `theme::APP_LOGO_TILE` because the old detached bar used them; this task deletes both (Step 5).
- Task 5 deletes the `tab_error` / `tab_notice` children of the old strip (master 336-345) to keep the build green; Step 2 deletes the strip anyway.
- Task 3 (runs before) left `detached_render.rs` with only the `RenameTarget::Tab` rename check in the strip; the code below starts from that.

**Files:**

- Modify `crates/mkrew-ui/src/shell/detached_render.rs`: `render_host` (85-87: title bar call, strip call), delete `render_detached_title_bar` (139-257) and `render_detached_strip` (259-370), add `render_detached_tab_area`; module doc line 1.
- Modify `crates/mkrew-ui/src/shell/chrome.rs`: Task 4's `render_title_bar`, the `Detached` arm of the `tab_area` match and a Move all back button next to the Split button (Steps 2 and 3).
- Modify `crates/mkrew-ui/src/shell/widgets.rs` (delete `app_logo`, 1275-1289) and `crates/mkrew-ui/src/theme.rs` (delete `APP_LOGO_TILE`, 164-165), Step 5.
- No change, checked: `detached_view.rs` (`DetachedWindow::render` only calls `shell.render_host(key, window, cx)`; its `Shell` context is the same `Entity<Shell>`, so `render_title_bar(&self, ..)` needs nothing there), `hosts.rs` (`HostUi.strip` is created per host, `tab_scope` and `host_of` unchanged), `tear_off.rs` (`drop_hint_line(HostId::Detached(key))` and `record_strip` are reused as they are), `reorder.rs` (`ReorderList::Detached(key)` and `drop_reordered` unchanged), `detached_actions.rs` (`move_all_back` unchanged).
- Test: none new. The code is GPUI drawing; the pure parts it uses (`detached::Detached`, `window_place::drop_line`, `Reorder`) are already tested. The step "run tests" proves nothing broke; behavior is smoke-tested in Step 7.

**Interfaces:**

Consumes (Task 4): `render_title_bar`, `tab_area_frame` (above), `tab_chip`.
Consumes (existing): `Detached::tabs_of(key) -> Vec<DockTab>`, `Detached::active(key) -> Option<DockTab>`, `Detached::select(key, index) -> bool`, `Shell::move_all_back(&mut self, key: WindowKey, window: &mut Window, cx)`, `record_strip(strip: Rc<RefCell<Option<Strip>>>, chip: Option<usize>)`, `Shell::reorderable(..)`, `DraggedItem { list, index, title }`, `session_label(tab, sessions) -> Option<&str>`.
Produces: `pub(crate) fn render_detached_tab_area(&self, key: WindowKey, theme: &Theme, cx: &mut Context<'_, Self>) -> gpui::AnyElement` in `detached_render.rs`.

What differs in a detached bar (all keyed on `host`):

| Part | Main | Detached |
|---|---|---|
| macOS inset, drag spacer, window controls, Split ▾ | yes | yes |
| Machine chip, sidebar toggle, workspace switcher | yes | no |
| View chips (Editor, Diff) | yes | no |
| Tab area | `render_tab_area` (one workspace, `+ ▾`) | `render_detached_tab_area` (every dock tab, workspace prefix on each chip, no `+`) |
| Limits chip, Run pill, update button, Board, Docs, right panel toggle | yes | no |
| Move all back (`Minimize2`) | no | yes, right group before Split |

- [ ] **Step 1: Check the Task 4 pieces exist**

Seek_grep in `crates/mkrew-ui/src/shell` for `fn tab_area_frame`, `fn render_tab_area` and `fn render_title_bar`.
Expected: all three with the signatures above, and `render_title_bar` has the `main` gate and the `tab_area` match.
If one is missing or different, stop and fix Task 4 first; this task does not re-create it.

- [ ] **Step 2: Add `render_detached_tab_area` and use it in the title bar**

In `crates/mkrew-ui/src/shell/detached_render.rs` replace the module doc (line 1) with:

```rust
//! What a detached window shows (ADR 0096, ADR 0106): the shared title bar with its tabs, the active tab.
```

Replace `render_detached_strip` (lines 259-370, from its doc comment to the closing brace of the function) with:

```rust
    /// The tab area of a detached window's title bar: a chip per tab, its workspace name in
    /// front. No `+`: new tabs are made in the main window. The strip and chip bounds are kept
    /// in `HostUi::strip` for drops from other windows (`record_strip`).
    pub(crate) fn render_detached_tab_area(
        &self,
        key: WindowKey,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let host = HostId::Detached(key);
        let tabs = self.detached.tabs_of(key);
        let active = self.detached.active(key);
        let strip = (self.hosts.get(&host))
            .map(|ui| ui.strip.clone())
            .unwrap_or_default();
        let sessions = self.sidebar.sessions();
        let chips: Vec<gpui::AnyElement> = tabs
            .iter()
            .enumerate()
            .filter_map(|(index, dock)| {
                let DockTab::Terminal { project, tab_id } = *dock;
                let tab = self.tabs.get(&project)?.tab(tab_id)?;
                if let Some(renaming) = (self.renaming.as_ref())
                    .filter(|r| r.target == RenameTarget::Tab(tab_id))
                {
                    return Some(self.render_rename(&renaming.draft, theme, cx));
                }
                let workspace = (self.sidebar.projects().iter())
                    .find(|p| p.id == project)
                    .map(|p| p.name.as_str());
                let item = DraggedItem {
                    list: ReorderList::Detached(key),
                    index,
                    title: session_label(tab, sessions)
                        .unwrap_or(&tab.title)
                        .to_owned()
                        .into(),
                };
                let chip = self
                    .tab_chip(
                        project,
                        tab,
                        active == Some(*dock),
                        workspace,
                        move |shell: &mut Shell| {
                            shell.detached.select(key, index);
                            shell.windows_dirty = true;
                        },
                        theme,
                        cx,
                    )
                    .child(record_strip(strip.clone(), Some(index)));
                Some(self.reorderable(chip.into_any_element(), item, true, space::XS, theme, cx))
            })
            .collect();
        let active_chip = active.and_then(|DockTab::Terminal { tab_id, .. }| {
            let index = tabs.iter().position(|dock| Some(*dock) == active)?;
            Some((SharedString::from(format!("tab-{tab_id}")), index))
        });
        self.tab_area_frame(host, chips, active_chip, None, theme, cx)
            .into_any_element()
    }
```

Delete `render_detached_title_bar` (lines 139-257, from `/// The title bar of a detached window` to its closing brace).

In `render_host` replace lines 84-88:

```rust
            .child(tear_off_catcher(cx))
            .child(self.render_detached_title_bar(key, &title, &theme, window, cx))
            .children(self.render_reconnect_banner(&theme, cx))
            .child(self.render_detached_strip(key, &theme, cx))
            .child(body)
```

with:

```rust
            .child(tear_off_catcher(cx))
            .child(self.render_title_bar(host, now, &theme, window, cx))
            .children(self.render_reconnect_banner(&theme, cx))
            .child(body)
```

`title` stays: `set_window_title` still uses it.

In Task 4's `render_title_bar` (`chrome.rs`) find the `match host` that builds the `tab_area: Option<AnyElement>` (its `HostId::Detached(_)` arm is `None`) and give that arm the detached tab area:

```rust
            HostId::Detached(key) => Some(self.render_detached_tab_area(key, theme, cx)),
```

Add `let detached = match host { HostId::Main => None, HostId::Detached(key) => Some(key) };` next to `let main`, for Step 3.

Task 4's bar already gates everything main-only on `let main = host == HostId::Main` (left group, view chips, limits, Run, update, Board, Docs, inspector toggle), so nothing else in `render_title_bar` changes: the drag spacer, Split button and window controls stay common.

- [ ] **Step 3: Move all back in the right group**

In `render_title_bar`'s right group, before the Split button, add:

```rust
        let right = match detached {
            Some(key) => right.child(
                icon_button("move-all-back", Icon::Minimize2, theme)
                    .tooltip(tooltip(theme, "Move all tabs back to the main window", ""))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.move_all_back(key, window, cx);
                    })),
            ),
            None => right,
        };
```

The existing `modal` inert layer on the right group (`when(modal, ..)`) already covers it: the button is inert while a dialog is open or the window reconnects, as before.

- [ ] **Step 4: Run the build**

Run: `CARGO_BUILD_JOBS=4 cargo check -p mkrew-ui --all-targets`
Expected: compiles with no errors; warnings about now-unused imports (`notice`, `window_control`, `DRAWN_CONTROLS` only if Task 4's bar does not use them, which it does) are fixed in Step 5.

- [ ] **Step 5: Delete what the cutover left behind**

Run (seek_grep, not shell grep): pattern `render_detached_title_bar|render_detached_strip|TAB_STRIP_HEIGHT|tab_error|tab_notice` in `crates/mkrew-ui/src/shell/detached_render.rs`.
Expected: no match (`tab_error` and `tab_notice` went with Task 5; `TAB_STRIP_HEIGHT` stays in `file_tabs_render.rs`, `inspector.rs`, `sidebar.rs`).
Run the same pattern on all of `crates/mkrew-ui/src`: `render_detached_*` must have no match anywhere.
Delete `app_logo` from `crates/mkrew-ui/src/shell/widgets.rs` (doc comment and function, 1275-1289 on master) and `APP_LOGO_TILE` from `crates/mkrew-ui/src/theme.rs` (164-165), after seek_grep `app_logo|APP_LOGO_TILE` over `crates` shows no caller left (the old detached bar was the last one).
Fix any unused import the compiler reports in `detached_render.rs`.

- [ ] **Step 6: Run tests and clippy**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui`
Expected: PASS.
Run: `CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 7: Smoke on the real app (Windows)**

Build `CARGO_BUILD_JOBS=4 cargo build -p mkrew-ui -p mkrew-cli -p mkrew-daemon` and start with the sandbox of Task 7 Step 1.
Open a workspace with two tabs (one agent, one shell) and a second workspace with one tab.
Drag the agent tab's chip out of the main window: a detached window opens at the pointer.
Check, dark and light:

- Its title bar shows the macOS inset or nothing at the left, the tab chip with the workspace name in front, a drag spacer, Move all back, Split ▾, and the window controls.
- It shows no sidebar toggle, no workspace switcher, no Board, Docs, Run pill, limits chip or right panel toggle.
- A click on the chip does not move the window; dragging the spacer does; double-click on the spacer maximizes.
- Drag the shell tab out of the main window onto this window's tab area: the accent drop line shows between chips, the tab lands there.
- Drag a chip back onto the main window's tab area: it returns; with the last tab gone the detached window closes.
- Move all back returns every tab and closes the window.
- F2 on a default-named tab renames the focused pane in the pane header; double-click on a user-named tab shows the tab rename field in the bar.

- [ ] **Step 8: Commit**

```
git add crates/mkrew-ui/src/shell/detached_render.rs crates/mkrew-ui/src/shell/chrome.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/theme.rs
git -c user.email=tungnv975@gmail.com commit -m "refactor(ui): detached windows draw the shared title bar and tab area"
```

Risks for the reviewer:

- The merge of the detached gate into Task 4's `render_title_bar` depends on how Task 4 laid out the left group and tab area; the gates above are by meaning, not by line.
- A tab of a workspace that is not loaded yet (`self.tabs.get(&project)?` is `None`) draws no chip but its slot index stays in `ReorderList::Detached`, as before.
- Many detached tabs rely on Task 4's scroll frame: the active chip being scrolled into view in a detached window is not covered by a test.
- Linux keeps the OS title bar and shows the same bar under it; not exercised on this machine.

---

### Task 7: Docs, smoke and verify

**Files:**

- Create: `docs/decisions/0106-tern-layout.md`.
- Modify: `docs/SPEC.md` (§7 647-648 and 707, §10 822-842, 911-943, 962-966, 980-996, 1097, 1109, 1149-1161, 1166, §11 1177-1178, 1191-1210, §13 1532, 1563, 1633, §14 1756, 1786, 1791, 1868, 1880, 1888, §20.0 2235, 2247-2248).
- Modify: `docs/HUMAN_CHECKS.md` (remove 205-217, insert a section above line 24, fix 1545).
- Modify: `README.md` (161-171).
- Modify: `docs/PROGRESS.md` (note at the top of "Notes for the next session").
- Create: `docs/screenshots/tern-layout-<view>-<dark|light>.png` (Step 3).
- No change: `CHANGELOG.md` (never edited here), ADR 0102 (its status line is amended, see Step 2).

Line numbers are those of master at 7cf71e0; docs are not touched by Tasks 1-6, so find each place by its quoted old text.
Repo rules for the prose: no em dash, one sentence per line.

**Interfaces:** none (docs).

- [ ] **Step 1: Write ADR 0106**

Create `docs/decisions/0106-tern-layout.md`:

```markdown
# 0106 - Tern layout: one top bar with the tabs, no bottom bar

- Date: 2026-11-23
- Status: accepted (supersedes the bar part of ADR 0102; amends ADR 0034, 0054, 0096 and SPEC sections 7, 10, 11, 13, 14 and 20.0)
- Milestone: none (human request 2026-11-23, spec `docs/superpowers/specs/2026-11-23-tern-layout-design.md`, plan `docs/superpowers/plans/2026-11-23-tern-layout.md`)

## Context

The user likes the layout of Tern: one slim top bar that holds the tabs, no bottom bar, and as much room as possible for the terminals.
Mkrew had a title bar (42 px), a terminal tab strip under it (44 px), pane headers on split panes only, and the widget bar of ADR 0102 at the bottom (24 px).
The widget bar showed the folder, the Git branch, the agents' limits, the machine load and the version; most of it has a home elsewhere, and the user asked to remove the bar.
The view switch `Terminals | Board | Docs | Editor | Diff` and a row of rarely used buttons (logo, branch chip, History, theme toggle, phone, `+ New`) filled the title bar.

## Decision

- One full-width top bar of `TITLE_BAR_HEIGHT` (42).
  Left: macOS inset, machine chip (machine windows), sidebar toggle, workspace switcher.
  Then the tab area, a drag spacer, and on the right the limits chip, Run pill, update button, Board and Docs icons, Split, right panel toggle, and on Windows the window controls.
- The tab strip lives in the tab area: Editor and Diff chips (only while open), pinned tabs, other tabs, and a `+ ▾` button.
  The area scrolls sideways; a chip shrinks to `TAB_MIN_WIDTH` first; `+ ▾` stays visible.
  Only the spacer drags the window, so a click on a tab never moves it.
- Tab chip look (Tern): the active tab has a solid `card` fill and radius 8 without a border; inactive tabs are transparent with muted text and a hover fill.
  A click on a tab also shows Terminals.
- The view switch is removed.
  Terminals: any tab; Board and Docs: icons that return to Terminals on a second click; Editor and Diff: their chips.
- `+ ▾` replaces the strip `+` and `+ New ▾`: agent profiles (default first), Shell, Empty tab; each creates a tab and shows Terminals.
  A new pane in the current tab comes from Split ▾ and the existing shortcuts.
- Removed from the bar: logo, branch chip and repo menu (the Git panel has the repository strip), History, theme toggle, phone button, `⋯` tab menu button (right-click on a tab opens the same menu).
  All stay in the palette, their shortcuts and Settings.
- The strip's notices become toasts: errors `toast_error`, notices `toast_info`, and the running warning one `toast_info` when the count rises above `RUNNING_WARN_ABOVE` (again only after it dropped back).
- Every pane, a single one too, has the compact header; the header has no title.
  The agent chip's tooltip gives the full pane title; the zoom notice and "Show all panes" live in the zoomed pane's header.
- The tab title follows the focused pane: a tab that keeps its default name shows the focused pane's session title, a name the user gave always wins.
  A double-click on a default-named tab renames the focused pane, on a named tab the tab.
- The widget bar is deleted.
  Limits become a chip in the bar that shows only at 80% or more: `⚠ Claude 5h 84%`, the highest window, `status_needs_input` at 80-94% and `danger_text` from 95%; the tooltip lists every window; a click opens Settings > Integrations.
  Folder, Git and version are in the palette, the Git panel and Settings.
- Machine stats are removed: `Event::MachineStats`, `MachineStats` and the daemon's `machine.rs` sampler go; protocol 57 -> 58.
  `sysinfo` stays for `ports.rs`.
- Detached windows draw the same title bar function without the workspace controls, Board, Docs, Editor and Diff chips, Run pill and limits chip, with Move all back in the right group; their chips keep the workspace name in front.
- `[ui] widget_bar` and `[ui] widgets_hidden` are retired: `settings.toml` files that have them still load (the keys are accepted, ignored and never written).
  `Command::ToggleWidgetBar` and its shortcuts are gone.

## Alternatives

- A floating "bubble" top bar like Tern's: rejected by the user; the bar stays full width.
- Board, Editor and Diff as real tabs in the tab model: more work and a model change; chips and icons give the same reach.
- Keeping the widget bar as an option: rejected, two ways to see limits and a daemon sampler for one number nobody asked to keep.
- A limits chip that always shows: a value below 80% is noise; the chip is a warning.

## Consequences

- The terminal area gains the 44 px of the old strip and the 24 px of the widget bar, minus one pane header row for a single pane.
- Protocol 58: a UI from before it shows a version mismatch with an old daemon; the user restarts the daemon.
- An old daemon never sends `machine.stats`; a new daemon never does either.
- The title bar height is 42 everywhere; SPEC section 10 said 44 and section 14 said 48.
- The widget bar rule of `theme::problems()` (ADR 0102) is now the limits chip rule: `status_needs_input` and `danger_text` need 4.5:1 on `chrome`; the themes already meet it, so the amber darkening of ADR 0102 stays.
- With many tabs the area scrolls and the active tab is scrolled into view; the drop line and window drops use the recorded strip bounds, which now cover the scrolling area only.
- The Editor view's own file-tab strip and the sidebar are unchanged.
```

Also in `docs/decisions/0102-widget-bar.md` replace the status line

old: `- Status: accepted (amends ADR 0054, which removed the status bar)`
new: `- Status: accepted (amends ADR 0054, which removed the status bar); the bar was removed by ADR 0106, the limits path (`agent.limits`, `limits.get`, `limits.report`, `mk statusline`) stays`

- [ ] **Step 2: SPEC edits (old -> new)**

Apply in this order (top to bottom).

**§7 events table, line 648.** Delete the whole row that starts `| \`machine.stats\` |`.

**§7 protocol notes, after line 707** (`Protocol 57 (ADR 0105): ...`) add:

```
Protocol 58 (ADR 0106): event `machine.stats` and `MachineStats` removed; `agent.limits`, `limits.get` and `limits.report` stay.
```

Line 703 (`Protocol 55 ... events \`agent.limits\` and \`machine.stats\`.`) stays as history.

**§10 window diagram, lines 822-841** (the fenced block from `` ``` `` to the closing `` ``` ``), new:

```
┌───────────────────────────────────────────────────────────────────────────────────────┐
│ ◧ A api ▾ │[Editor*][Diff*]│ [▣ ● tab ✕] [>_ ○ tab ✕] +▾   ⚠ Claude 5h 84% (● 2 running ▾) ▦ ▤ ◫ ◨ ─ □ ✕ │
├──────────────┬────────────────────────────────────────────────────────────────────────┤
│ [⌕ Filter… /]⇲+│ ┌ ▣ Claude ● Working · 4s  📖 Share ┐                                 │
│ [All│Working│Needs you]│                                                              │
│ ▾ A api (● 1 working)│  Todo │ Running │ Needs input │ …                              │
│ │ ✻ fix login    2m │  …or the terminal tabs/panes                                    │
│ │ RUN               │  …or the Markdown review view                                   │
│ │ ● FE      :5180   │                                                                 │
│ ▸ W web       1 idle│                                                                 │
│ ▸ 2 empty workspaces I D │                                                            │
│ 📥 Inbox clear     ⇤ │                                                                │
└──────────────┴────────────────────────────────────────────────────────────────────────┘
```

**§10 lines 839-842**, old:

```
ADR 0054 simplified this screen: one place per piece of information, color for status only,
empty things hidden, detail on hover / in menus. It removed the old status bar; ADR 0102 adds back a thin widget bar along the bottom (see "Widget bar" below).
The right panel (below) is a third column right of Terminals, the Editor and the Diff while open.
Its tab row is `[Files│Session│Git]`; `Editor*` and `Diff*` are in the view bar only while a file tab or a diff is open.
```

new:

```
ADR 0054 simplified this screen: one place per piece of information, color for status only,
empty things hidden, detail on hover / in menus. It removed the old status bar; ADR 0102 added a thin widget bar along the bottom and ADR 0106 removed it again: there is no bottom bar, the terminal tabs live in the title bar, and the agents' limits show as a chip in it (see "Limits chip" below).
The right panel (below) is a third column right of Terminals, the Editor and the Diff while open.
Its tab row is `[Files│Session│Git]`; `Editor*` and `Diff*` are chips in the title bar's tab area only while a file tab or a diff is open.
```

**§10 line 911** (`- **Views per workspace:** ...`), new:

```
- **Views per workspace:** Terminals (a click on any terminal tab), Board and Docs (icons in the title bar, only with `[features] docs` for Docs; a second click returns to Terminals), Editor (a chip, only while a file tab is open), Diff (a chip, only while a diff is open).
```

**§10 title bar bullet, lines 912-940** (from `- **Title bar** (44 px, ...` through `the logo, the sidebar toggle, the drag area and the window controls show.`), new:

```
- **Title bar** (42 px, ADR 0027, ADR 0034, ADR 0054, ADR 0106), one bar across both columns, left to right:
  in a machine window (ADR 0089) the machine chip (below); the sidebar toggle `◧`; the workspace avatar + name ▾ (a dropdown of the workspaces, click switches, plus "Add workspace…");
  the tab area; a drag space; the right group; on Windows the window controls.
  - **Tab area:** the Editor and Diff chips (icon, name, × that closes all file tabs or the diff; only while open; a click shows the view), a 1 px rule, the pinned tabs, a rule, the other tabs, and `+ ▾` (below).
    The area scrolls sideways when the tabs do not fit; the active tab is scrolled into view; a chip shrinks to `TAB_MIN_WIDTH` before the area scrolls, `TAB_TITLE_MAX_WIDTH` is its maximum; `+ ▾` stays visible after the scrolled tabs.
    Only the drag space moves the window, so a click on a tab never does.
    A tab shows its kind icon, 📌 when pinned, its title, a status dot and ×; the active tab has a solid `card` fill with radius 8 and no border, the others are transparent with muted text and a hover fill; a click activates the tab and shows Terminals.
  - **`+ ▾`:** one row per installed agent profile (the workspace's default first with ✓ and Alt+Shift+A), "Agent" when none is listed, Shell (Alt+Shift+S), Empty tab; each row opens a new tab and shows Terminals; while a session starts the button shows the starting spinner.
  - **Right group:** the limits chip (only at 80% or more, see "Limits chip"), the voice status while the sidebar is hidden, the Run pill, the update button (only when an update is known), Board (`LayoutGrid`), Docs (`BookOpen`, only with `[features] docs`), Split ▾ (`◫`: Split right, Split down, Equalize panes, layouts, as before) and the right panel toggle (`◨`, ADR 0038, ADR 0081).
    Board and Docs show in the accent color while their view is shown.
  - History (palette "Global: History…"), the theme toggle (Ctrl+Shift+J, palette, Settings > Appearance), Phone access (palette "Global: Phone access…"), the branch chip and repository menu (the Git panel, Ctrl+Shift+4) and the logo are not in the bar.
  - Every icon is gray (`text-muted`, `text` on hover): color only for status.
    The Git tab's count comes from `git.repos` and one `git.status` per repository, asked for the shown workspace only, on a workspace switch and on its `git.changed`; no polling, nothing for hidden workspaces.
  - Window: the empty part (the drag space) is the drag area.
    **Windows:** double-click maximizes or restores, ─ □ ✕ at the right (hovering □ shows the Windows 11 Snap Layouts flyout), all edges resize; ✕ and Alt+F4 hide the window to the tray while `[ui] close_to_tray` is on (§20J); the controls are hidden in full screen.
    **macOS:** native traffic lights at the left, centered in the bar at any `[ui] font_size` (no gap is left for them in full screen), drag and double-click as the OS does.
    **Linux:** the OS title bar stays. Minimum window size 1100 × 560.
    Without a workspace only the sidebar toggle, the drag space and the window controls show.
```

**§10 line 943**, old: `The **machine chip** follows the logo: Lucide` new: `The **machine chip** is the first element of the bar (after the macOS inset): Lucide`.

**§10 "No status bar", lines 962-966**, old:

```
- **No status bar** (ADR 0054): its sidebar toggle is in the title bar, its Inbox state and
  compact toggle in the sidebar footer, its git HEAD in the title bar's repository tooltip, the
  focused agent's context share in the tab strip (single pane) or pane header (split), and the
  other usage fields (tokens, cache, model) on the Session tab's card.
  The widget bar of ADR 0102 is not that bar: it holds only the folder, branch, limits, machine load and version.
```

new:

```
- **No status bar** (ADR 0054, ADR 0106): its sidebar toggle is in the title bar, its Inbox state and
  compact toggle in the sidebar footer, its git HEAD in the Git panel's repository strip, the
  focused agent's context share in the pane header, and the
  other usage fields (tokens, cache, model) on the Session tab's card.
  There is no bottom bar of any kind: the folder is in the palette ("Reveal in Explorer"), the Git branch in the Git panel, the version in Settings, the agents' limits in the title bar chip.
```

**§10 Terminals bullet, lines 980-996**, old start `- **Terminals:** tab bar on top; each tab is a split tree of panes; each pane is a session.` -> new `- **Terminals:** the tabs are in the title bar (above); each tab is a split tree of panes; each pane is a session with its own header.`
Old `activates the tab first. \`+\` (new tab) follows the tabs. At the right end of the strip: for a` through `single pane its status (§11) and agent controls, then the split button and \`⋯\`. The split button opens` -> new `activates the tab first. \`+ ▾\` (new tab) follows the tabs. The split button (title bar, right group) opens`.
Old `` `⋯`, and a right-click on a tab, open the tab menu: `` -> new `` A right-click on a tab opens the tab menu: ``.
In the same bullet old `` (once for Close others / to the right). Drag a tab along the strip to reorder it `` -> new `` (once for Close others / to the right). Drag a tab along the tab area to reorder it ``.

**§10 lines 1097 and 1109**, old `the pane header (the tab bar for a single pane) shows` -> new `the pane header shows`; old `No link on screen: the tab bar says "No links on screen." for 3 seconds.` -> new `No link on screen: a toast says "No links on screen.".`

**§10 "Widget bar (ADR 0102)" section, lines 1149-1161**, replace the whole section (heading to the line before `### Detached windows`) with:

```
### Limits chip (ADR 0106, replaces the widget bar of ADR 0102)

- A chip in the title bar's right group, before the Run pill, shown only while an agent's limit window is at 80 % or more (the rounded percent); no data, or nothing at 80 %, means no chip.
- It shows the highest window, `⚠ Claude 5h 84%`; a tie goes to the window that resets later.
  Amber (`status_needs_input`) from 80 % to 94 %, red (`danger_text`) from 95 %; always the ⚠ icon and the text.
- The tooltip lists every known window (Claude 5h and 7d, Codex 5h and 7d) with its percent and its reset in local time and as "in 2 h 10 min".
  A window whose reset time passed counts as unknown until a new report.
- A click opens Settings at the Integrations page.
- The chip is inert while a dialog is open and while the window is reconnecting.
- Only the main window has it; detached windows have none.
- Sources are unchanged: Codex from its transcript at a turn end (§20E.3), Claude from `mk statusline` (`limits.report`).
- `[ui] widget_bar` and `[ui] widgets_hidden` are retired keys (§20.0).
```

**§10 Detached windows, line 1166**, old:
`- A detached window has a title bar (the shown tab and its workspace, Move all back, the window controls), a tab strip with each tab's workspace, and the tab's panes.`
new:
`- A detached window draws the same title bar (ADR 0106): its tabs as chips with the workspace name in front, Move all back, Split ▾ and the window controls; it has no workspace switcher, Board, Docs, Editor and Diff chips, Run pill or limits chip. Below it are the tab's panes.`

**§11 lines 1177-1178**, old:

```
Status shows in three places: the tab strip (a single pane) or pane header (a split), the tab
dot, the sidebar row.
```

new:

```
Status shows in three places: the pane header, the tab
dot, the sidebar row.
```

**§11 "Pane status" bullet, lines 1191-1209.** Old opening `- Pane status (ADR 0054), shown at the right end of the tab strip for a single pane (it has no` / `  header) and in the compact header of each pane of a split: the task folder` -> new `- Pane status (ADR 0054, ADR 0106), shown in the compact header every pane has, a single pane too: the task folder`.
Old `Zed", ✓ while on; the button is highlighted while following). Each part shows only when it has` / `something to say. "Turns…" is in the tab strip's \`⋯\` menu. The split header starts with the` / `agent badge and the session/task title (F2 or a double-click renames it inline: Enter saves,` through `` order is: user title > terminal title > agent name). `` -> new:

```
Zed", ✓ while on; the button is highlighted while following). Each part shows only when it has
something to say. "Turns…" is in the tab menu (right-click a tab) and the pane menu.
The header starts with the agent badge and has no title; the badge's tooltip gives the pane's full title.
A pane is renamed with F2, the pane menu's "Rename pane" or a double-click on a tab that keeps its default name: the field shows in the pane's header (Enter saves, Esc cancels, blank clears; run panes keep their command name, ADR 0031).
A tab still named `Terminal` / `Terminal N` shows its focused pane's session title (cut with `…`, full on hover) and follows the focus to another pane; a name the user gave the tab always wins; a double-click on a default-named tab renames the focused pane, on a named tab the tab.
Without a title an agent pane shows the title its agent gave the terminal, e.g. Claude's conversation summary; everywhere a session is named the order is: user title > terminal title > agent name.
While a pane is zoomed its header shows `⤢ Pane zoomed` and a "Show all panes" button.
```

and keep the sentence that continues `The active tab has a 2 px accent line ...` replaced by: `The active tab has a solid card fill (the Tern chip look); in a split the panes keep only their thin border, the unfocused ones are dimmed.` (drop the 2 px accent line text up to `are dimmed.`), then the unchanged `The sidebar session row also puts the badge ...`.

**§11 line 1210**, old `has a \`Terminal | Chat\` switch in its header (for a single pane, in the tab strip next to its status pill)` -> new `has a \`Terminal | Chat\` switch in its header`.

**§13 line 1532**, delete the row `| Global | Show / hide the widget bar (ADR 0102) | Ctrl+Alt+Shift+J |`.
**§13 line 1563**, old `like + New ▾ → Agent / Shell` -> new `like the tab + ▾ menu's Agent / Shell`.
**§13 line 1633**, old `| Sidebar / right panel / widget bar / context menu | ⌘B / ⇧⌘Y / ⇧⌘B / ⌃⌘M |` -> new `| Sidebar / right panel / context menu | ⌘B / ⇧⌘Y / ⌃⌘M |`.

**§14 line 1756**, old `- Heights: controls 30, inputs 32, title bar 48, tab strip 44, Git repo strip 42 (px at 13 px body).` -> new `- Heights: controls 30, inputs 32, title bar 42 (it holds the tabs, ADR 0106), the Editor's file-tab strip and the sidebar and right panel tab rows 44, Git repo strip 42 (px at 13 px body).`
Add after it a bullet:

```
- Tab chip (Tern look, ADR 0106): height `CONTROL_HEIGHT`, radius 8 (`radius::MEDIUM`), no border; the active chip has a solid `card` fill and `text`; the others are transparent with `text_muted` and a hover fill through `.pressable(Look::Chip, ..)`; the × shows on the active chip and on hover for the others.
  Chips shrink to `TAB_MIN_WIDTH` before the tab area scrolls; `TAB_TITLE_MAX_WIDTH` is the maximum.
```

**§14 line 1786**, old `Close` / `  pane is an icon-only \`square-x\` in the tab strip.` -> new `  pane is an icon-only \`square-x\` in the pane header.` (line 1785-1786: the text reads `Close\n  pane is an icon-only \`square-x\` in the tab strip.`).
**§14 line 1787**, old `(Share, \`+ New\`, idle Run,` -> new `(Share, idle Run,`.
**§14 line 1791**, old `a Lucide \`smartphone\` icon button in the title bar, right of the` / `  theme toggle (accent while \`mk web\` runs, \`text-muted\` otherwise; tooltip "Phone access: on" /` / `  "Phone access: off"), and the palette command "Global: Phone access…", open the "Phone` -> new `the palette command "Global: Phone access…" (ADR 0106 removed the title bar's \`smartphone\` icon button) opens the "Phone` and delete the sentence in line 1802 `the title bar icon reads` / `\`daemon.status\` every 5 s.` (the title bar icon is gone; the dialog still re-reads about once a second while open).
**§14 line 1868**, old `in \`accent-text\` in the title bar right before` / `  the phone icon while a newer release is known` -> new `in \`accent-text\` in the title bar right before` / `  the Board icon while a newer release is known`.
**§14 line 1880**, old `a dialog's error row, the commit box, the tab bar notice, a card notice) stays there.` -> new `a dialog's error row, the commit box, a card notice) stays there; the tab strip's errors and notices are toasts (ADR 0106).`
**§14 line 1888**, old `- Theme: System (follows the OS live), Light or Dark - \`[ui] theme\`, cycled by the title bar toggle` -> new `- Theme: System (follows the OS live), Light or Dark - \`[ui] theme\`, cycled by Ctrl+Shift+J and the palette`.

**§20.0 line 2235**, old `# §14; "system" | "light" | "dark"; the title bar toggle writes it` -> new `# §14; "system" | "light" | "dark"; Ctrl+Shift+J and Settings write it`.
**§20.0 lines 2247-2248**, old:

```
widget_bar = true          # §10; the widget bar at the bottom of the main window; its menu and Ctrl+Alt+Shift+J write it
widgets_hidden = []        # §10; ids of hidden widgets: folder, git, limits, machine, version; its menu writes it
```

new:

```
# widget_bar, widgets_hidden: retired (ADR 0106). A file that still has them loads; they are ignored and never written.
```

- [ ] **Step 3: HUMAN_CHECKS edits**

(The spec said lines 187-195 and 1527; on master the widget bar section is 205-217 and the "status bar" text is at 1544-1545.)

1. Delete the whole section from `### To verify - 2026-11-22 Widget bar (ADR 0102, plan docs/superpowers/plans/2026-11-22-widget-bar.md)` (205) to the blank line before `### To verify - 2026-11-22 Theme gallery` (218), but first move three still valid checks into the new section below: the tray-amber check (209), the real-limits checks (214-215) and the Windows status line check (216).
2. Line 1544-1545, old:

```
  clock), sidebar header (+, collapse chevron, remove-idle), tab strip (+, split, ⋯, tab ✕),
  status bar (sidebar, compact, session panel toggles), Git (repos ▾, fetch, ‹ ›, ⋯), Files
```

new:

```
  clock), sidebar header (+, collapse chevron, remove-idle), title bar tabs (+ ▾, tab ✕) and Split,
  title bar toggles (sidebar, right panel), Git (repos ▾, fetch, ‹ ›, ⋯), Files
```

3. Insert above `### To verify - 2026-11-23 Chat look` (line 24):

```markdown
### To verify - 2026-11-23 Tern layout (ADR 0106, plan docs/superpowers/plans/2026-11-23-tern-layout.md)

Screenshots: `docs/screenshots/tern-layout-<view>-<dark|light>.png`.

- [ ] **Look (dark + light, Mkrew Light and one palette theme):** one 42 px bar across the window: sidebar toggle, workspace switcher, tabs, `+ ▾`, a drag space, then Run, Board, Docs (with `[features] docs`), Split and the right panel toggle; no bottom bar; the sidebar and the terminals reach the window bottom.
  The active tab is a solid lighter card with rounded corners and no border, inactive tabs are muted with a hover fill, the × shows on the active tab and on hover.
- [ ] **Views:** a click on a terminal tab shows Terminals from Board, Docs, Editor and Diff; Board and Docs icons turn accent while shown and a second click goes back; open a file: an Editor chip appears, × closes all file tabs; open a diff: a Diff chip appears, × closes it.
- [ ] **Many tabs:** open 10 or more tabs in an 1100 px wide window: the area scrolls sideways, the active tab stays in view when you switch with Ctrl+Tab, `+ ▾` stays visible, chips shrink to a minimum before scrolling.
- [ ] **`+ ▾`:** rows are the agent profiles (default first with a check), Shell, Empty tab; each opens a new tab and shows Terminals, also from Board; the button shows the spinner while the agent starts; Alt+Shift+A and Alt+Shift+S still add a pane to the current tab.
- [ ] **Window drag:** drag the empty space: the window moves; double-click it: maximize and restore; a click or a drag on a tab never moves the window (Windows and macOS).
- [ ] **Pane header:** a single pane has the compact header with no title; the agent chip's tooltip shows the full pane title; Terminal | Chat, Reading width and Share work there; zoom a pane (Ctrl+Shift+M): the header shows `Pane zoomed` and "Show all panes".
- [ ] **Tab title:** a tab named `Terminal` shows the focused pane's session title and follows when you move focus in a split; rename the tab (F2 on a named tab, or double-click): the name wins; double-click a default-named tab: the focused pane's rename field shows in its header.
- [ ] **Notices as toasts:** a failed action that used to print in the strip (close a tab while the daemon is down, open links with none on screen) shows a toast; open 13 sessions in one workspace: one toast "sessions running", not repeated until the count drops to 12 or fewer and rises again.
- [ ] **Limits chip:** with no data or below 80 % there is no chip; report 85 % (amber `⚠ Claude 5h 85%`), then 96 % (red); the tooltip lists the windows with reset times in local time; a click opens Settings > Integrations; it is inert behind a dialog.
- [ ] **Detached window:** drag a tab out: same bar with the workspace name on its chip, Move all back, Split; no switcher, Board, Docs or limits chip; drag a tab out and back onto the main tab area (drop line shows); a click on its chip does not move the window.
- [ ] **Old settings:** a `settings.toml` with `widget_bar = false` and `widgets_hidden = ["git"]` still loads and Mkrew rewrites other keys without them.
- [ ] **macOS:** the traffic lights are centered in the 42 px bar (also with `[ui] font_size = 16`), dragging the empty space moves the window, double-click zooms it, a click on a tab never drags, full screen leaves no gap at the left.
- [ ] In a light theme, the tray icon's needs-input amber (8B570D) is still visible on a dark Windows taskbar, and the taskbar badge number reads on it.
- [ ] Real limits: run `mk integration install claude`, send one turn in Claude Code and one in Codex; the chip shows when a window is at 80 % or more, with plausible percentages and local reset times in the tooltip.
  A status line you had before still prints in Claude Code; `mk integration uninstall claude` brings it back.
- [ ] On Windows, check that your own status line still prints with Git for Windows installed (it runs through Git Bash); without Git Bash it runs through PowerShell.
- [ ] The daemon no longer samples CPU or RAM: its CPU use at idle with a window open is 0 %.

```

- [ ] **Step 4: README**

Replace `README.md` lines 161-171 (`### Widget bar` through the line `Without a Claude subscription login ... stays absent.`) with:

```markdown
### Limits chip

When one of your Claude or Codex limit windows (5 hours or weekly) is at 80% or more, a chip shows in the top bar, for example `⚠ Claude 5h 84%`.
It is amber from 80% and red from 95%.
Hover it to see every window with its reset time; click it to open Settings.
With nothing at 80% there is no chip.
Codex limits come from its transcript after a turn.
Claude limits need its status line: `mk integration install claude` sets Claude Code's `statusLine` to `mk statusline`, which reports the limits to Mkrew and then runs the status line you had before (saved as `claude-statusline.json` next to `settings.json`; `mk integration uninstall claude` puts it back).
Without a Claude subscription login (an API key), or with an older Claude Code, Claude sends no limits and the chip stays absent for Claude.
```

Then search the README for other text about the title bar or tabs: seek_grep `title bar|tab strip|New ▾|view switch|Terminals \|` in `README.md`; for each hit, rewrite it to the new layout (tabs in the top bar, `+ ▾` for a new tab, Board and Docs as icons) in the same one-sentence-per-line style.
If there is no hit, nothing more changes.

- [ ] **Step 5: PROGRESS note**

In `docs/PROGRESS.md` insert as the first bullet under `## Notes for the next session` (above the `2026-11-23 Chat look` bullet), one line:

```
- 2026-11-23 Tern layout (branch `feat/tern-layout`, spec docs/superpowers/specs/2026-11-23-tern-layout-design.md, plan docs/superpowers/plans/2026-11-23-tern-layout.md, ADR 0106, protocol 58, no migration): `Event::MachineStats` and daemon `machine.rs` removed (`tests/daemon/limits.rs` keeps the limits tests); widget bar deleted, `mkrew_ui::limits` (`Limits`, `Level`, `Chip`, `chip`) and `shell/limits.rs` draw a chip at 80% or more, `[ui] widget_bar` / `widgets_hidden` retired but accepted; pane header on every pane without a title, `tabs::session_label` follows the focused pane; one 42 px title bar holds the tabs (`render_title_bar(host, ..)`, `render_tab_area`, `tab_area_frame`, view chips, merged `+ ▾`, Board / Docs icons), strip notices are toasts, detached windows use the same bar (`render_detached_tab_area`). Checks in HUMAN_CHECKS "2026-11-23 Tern layout"; screenshots `docs/screenshots/tern-layout-*.png`.
```

- [ ] **Step 6: Capture the screenshots (Windows, PowerShell)**

Build: `CARGO_BUILD_JOBS=4 cargo build -p mkrew-ui -p mkrew-cli -p mkrew-daemon` (the three binaries are `target\debug\mkrew.exe`, `mkrew-daemon.exe`, `mk.exe`).
Use a sandbox so the real data stays untouched.
Run in PowerShell from the worktree root; set `$theme` to `dark`, then to `light` and repeat Steps 6 and 7:

```powershell
$theme = "dark"
$root = "$env:TEMP\tern-smoke"
New-Item -ItemType Directory -Force "$root\data","$root\config" | Out-Null
$env:MKREW_DATA_DIR = "$root\data"; $env:MKREW_CONFIG_DIR = "$root\config"; $env:MKREW_SOCKET = "$root\mkrew.sock"
Set-Content "$root\config\settings.toml" "[ui]`ntheme = `"$theme`"`n"
Start-Process "$PWD\target\debug\mkrew.exe"
```

The UI starts the daemon with the same variables.
Add two workspaces (folders with a `git init`), open the cases of the list in Step 7 and capture each with this script, saved outside the repo as `$env:TEMP\shot.ps1`:

```powershell
param([string]$Out, [int]$Width = 0, [int]$Height = 0, [string]$Mode = "print")
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class W {
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int hh, uint f);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  public struct RECT { public int L, T, R, B; } }
"@
$p = Get-Process mkrew | Where-Object { $_.Path -like "*target\debug*" -and $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $p) { Write-Output "no window"; exit 1 }
$h = $p.MainWindowHandle
if ($Width -gt 0) { [W]::SetWindowPos($h, [IntPtr]::Zero, 40, 40, $Width, $Height, 0x0004) | Out-Null; Start-Sleep -Milliseconds 800 }
[W]::SetForegroundWindow($h) | Out-Null; Start-Sleep -Milliseconds 400
$r = New-Object W+RECT; [W]::GetWindowRect($h, [ref]$r) | Out-Null
$w = $r.R - $r.L; $hh = $r.B - $r.T
$bmp = New-Object System.Drawing.Bitmap $w, $hh
$g = [System.Drawing.Graphics]::FromImage($bmp)
if ($Mode -eq "print") {
  # 2 = PW_RENDERFULLCONTENT (needed for DirectX windows).
  $dc = $g.GetHdc(); [W]::PrintWindow($h, $dc, 2) | Out-Null; $g.ReleaseHdc($dc)
} else {
  $g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
}
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
Write-Output "saved $w x $hh"
```

If a PrintWindow image is all black (look at it), repeat with `-Mode screen` (it copies the screen; the window must be on top and not overlapped).
Save as `docs\screenshots\tern-layout-<view>-$theme.png`, with these `<view>` names:

| `<view>` | Content | Size |
|---|---|---|
| `one-pane` | one agent pane in the tab | default |
| `four-panes` | four panes in a tab (2 x 2) | default |
| `many-tabs` | 10 or more tabs, the active one in view, `+ ▾` visible | `-Width 1100 -Height 700` |
| `editor-diff` | Editor and Diff chips open, Diff shown | default |
| `board` | Board shown, Board icon accent | default |
| `docs` | Docs shown (`[features] docs = true` in the sandbox `config.toml`) | default |
| `detached` | a detached window next to the main window (capture each window; the detached one needs its own handle: use `-Mode screen` of the whole desktop region if `Get-Process` finds only the main window) | default |
| `limits-amber` | limits at 85% | default |
| `limits-red` | limits at 96% | default |

Then `git add docs/screenshots/tern-layout-*.png` (check each image is not black or empty before adding).

- [ ] **Step 7: Smoke checklist on the running app (spec section 7)**

Fake limits, with the same three `MKREW_*` variables set in the shell (so `mk` reaches the sandbox daemon), `resets_at` in the future (Unix seconds):

```powershell
$at = [int][double]::Parse((Get-Date -UFormat %s)) + 7200
'{"rate_limits":{"five_hour":{"used_percentage":85,"resets_at":' + $at + '},"seven_day":{"used_percentage":40,"resets_at":' + ($at + 200000) + '}}}' | & "$PWD\target\debug\mk.exe" statusline
```

This is the real path of Claude Code's status line (`mk statusline` reads that JSON on stdin and sends `limits.report`; it prints nothing and exits 0).
Change both `used_percentage` to `96` for the red chip, to `50` to see the chip leave only after the next report below 80 (the daemon keeps the newest report per agent).

Walk each item in dark, then light:

- [ ] One pane and four panes in a tab: the header is on every pane, no title, the tab shows the focused pane's title.
- [ ] Ten or more tabs in an 1100 px wide window: scroll, the active tab in view (Ctrl+Tab through them), `+ ▾` visible.
- [ ] Editor and Diff open (chips, Diff shown), Board and Docs shown (icons highlighted, a second click returns to Terminals).
- [ ] A tab dragged to a detached window and back (Task 6 Step 7).
- [ ] Limits at 85% (amber chip) and 96% (red chip) through the command above.
- [ ] Window drag from the spacer, double-click maximize, and a click on a tab that does not move the window.
- [ ] `+ ▾` rows, a failed close shown as a toast, the 13th running session toast once.
- [ ] A `settings.toml` with `widget_bar = false` and `widgets_hidden = ["git"]` starts Mkrew normally.

Close Mkrew, `target\debug\mk.exe daemon stop` with the sandbox variables, and delete `$env:TEMP\tern-smoke` and `$env:TEMP\shot.ps1`.
Do not touch any process you did not start.

- [ ] **Step 8: Full verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt, clippy `-D warnings` and tests all PASS.
If it runs past 600 s, run `CARGO_BUILD_JOBS=4 cargo test --workspace` separately and report its result; also run `CARGO_BUILD_JOBS=4 cargo clippy --workspace --all-targets -- -D warnings`.
A failing insta snapshot: review it with `cargo insta review`, accept only changes that follow from this work (the title bar and tab look do not change theme snapshots).

- [ ] **Step 9: Commit**

```
git add docs/decisions/0106-tern-layout.md docs/decisions/0102-widget-bar.md docs/SPEC.md docs/HUMAN_CHECKS.md docs/PROGRESS.md README.md docs/screenshots/tern-layout-*.png
git -c user.email=tungnv975@gmail.com commit -m "docs: ADR 0106 Tern layout, SPEC, HUMAN_CHECKS, README and screenshots"
```

Risks for the reviewer:

- The SPEC edits are quoted by old text; two passages (the pane status bullet in section 11, the title bar bullet in section 10) are long, so check that no sentence about the removed strip, widget bar or `+ New` survives (seek_grep `widget|tab strip|tab bar|\+ New|status bar` in `docs/SPEC.md`).
- PrintWindow on a GPUI DirectX window can return black; the `-Mode screen` fallback needs the window uncovered. The detached window has its own handle that `Get-Process mkrew` does not return, so capture it by screen region.
- `used_percentage` in the fake report must stay above the old value's `observed_at`; `mk statusline` stamps the current time, so repeated runs are newest-wins.
- HUMAN_CHECKS line numbers in the task text (205-217, 1544-1545) are those of master 7cf71e0; the chat-look check section at line 24 may have moved if other branches merged.
- macOS and Linux title bar behavior is only in the manual checks.
