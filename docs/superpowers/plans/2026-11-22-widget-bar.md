# Widget bar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A thin widget bar at the bottom of the main window shows the workspace folder, the git branch with its change count, the Claude and Codex 5-hour and weekly limits, the daemon machine's CPU and RAM and the Mkrew version, each widget hideable and each doing one useful thing on click.

**Architecture:** Protocol 55 adds `Event::AgentLimits`, `Event::MachineStats` and the requests `limits.get` / `limits.report`. The daemon keeps the newest limits per agent in memory (fed by `mk statusline` for Claude and by the usage `track` task reading `rate_limits` through `UsageReader` for Codex) and samples CPU and RAM with `sysinfo` only while a client is connected, through a pure throttle function. The UI keeps a pure model (`mkrew_ui::widget_bar`: widget list, colors, narrow-width dropping, hide toggling) and draws it in `shell/widget_bar.rs` with `pressable` widgets and the shared `render_menu`.

**Tech Stack:** Rust (edition 2024), serde, tokio (`watch`, `select!`, `interval`), `sysinfo` 0.31 (already a daemon dependency), `chrono` 0.4 `clock` (already in `Cargo.lock`, new direct dependency of `mkrew-ui` for the local reset time), clap, gpui-pre 0.3.7.

**Spec:** `docs/superpowers/specs/2026-11-22-widget-bar-design.md` (commit ee991b9). Executors read it and this plan; the plan wins where it states a decision under "Spec gaps".

## Global Constraints

- Work in the worktree `C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/widget-bar` on branch `feat/widget-bar` (base master 895bc4e). Always use absolute paths under it. Never touch the main checkout.
- Commit with `git -c user.email=tungnv975@gmail.com commit -m "<Conventional Commit>"`, only the files of the task, never push, never rewrite history.
- Every cargo command is prefixed `CARGO_BUILD_JOBS=4`, run with the worktree as `cwd`. Windows: C: is low on disk and old test processes may linger; do not kill processes you did not start and never touch the installed Mkrew.
- Verify with `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL). If it passes 600 s, run `CARGO_BUILD_JOBS=4 cargo test --workspace` separately. Intermediate tasks run only the focused commands they list.
- `PROTOCOL_VERSION` 54 -> 55 (checked on `master`, `feat/theme-gallery`, `feat/md-split`, `fix/restart-cleanup`). ADR 0102 (0098-0100 are the macOS plans, 0101 the Theme gallery). ADR 0102 amends ADR 0054.
- Defaults: `[ui] widget_bar = true`, `[ui] widgets_hidden = []`. Toggle key `ctrl-alt-shift-j` (free today; `ctrl-shift-j` is Toggle theme). Palette title `View: Show/hide widget bar`.
- Widget ids: `folder`, `git`, `limits`, `machine`, `version`. Order: left folder, git; right limits, machine, version. Unknown ids in `widgets_hidden` are kept and ignored.
- Limit colors: below 80% muted text; 80% and over `theme.status_needs_input`; 95% and over `theme.danger_text` (levels use the displayed, rounded percent). A window whose `resets_at` has passed shows nothing until a new report arrives. A widget with nothing to show is absent, never "0%".
- Machine stats: `sysinfo` every 2 s, only while at least one client is subscribed to events; sent when CPU moves by 1 percentage point or RAM by 100 MB (100 MiB), or every 10 s otherwise.
- `mk statusline`: stdin at most 1 MB; report budget 300 ms; chained command with a 5 s timeout; exits 0 and prints nothing on any failure.
- UI rules (SPEC section 14, `mkrew-ui-design`): colors, spacing (4/8/12/16/24/32), radius and font sizes only from `theme.rs` tokens; hover, press and focus only through `.pressable(Look::..)` and `.on_press(..)` (ADR 0097), never `.hover(` or `.cursor_pointer()`; dark and light themes; tooltips through `tooltip(..)`.
- No `unwrap`/`expect` outside tests (`unsafe_code` is denied). `thiserror` in libraries, `anyhow` in binaries, `tracing` for logs.
- No test runs a real agent, reads the real `~/.claude`, or touches the network. No test reads source text. Docs: no em dash, one sentence per line in long Markdown.
- Deviating from SPEC needs the ADR and the SPEC update in the same change set (Task 13).

## Review Focus

Input classes the spec implies but its tests do not name, most likely first. Each has a test in the task named in brackets.

- A Claude status line payload that is not what the daemon expects: no `rate_limits` (API-key user, older Claude Code), only one window, garbage, empty stdin, or over 1 MB. Expect: nothing is sent, the chained status line still runs, exit 0. [Task 7]
- The user's own chained status line misbehaves: it ignores stdin (the pipe closes under a 1 MB write), exits non-zero, hangs past 5 s, or its path holds spaces. Expect: `mk statusline` never fails, prints nothing for a failed or hung command, and install quotes the `mk` path. [Tasks 6 and 7]
- Stale or odd limit data: a window whose reset passed, a percentage above 100 or below 0, an agent other than Claude and Codex, an older report arriving after a newer one. Expect: expired windows vanish, values are clamped for display and by the daemon, other agents are refused or ignored, the newer report wins. [Tasks 4 and 8]
- Hand-edited or newer-build settings: `widgets_hidden` with an id this build does not know, and toggling one widget from the menu. Expect: the unknown id survives every toggle. [Tasks 2 and 8]
- A window that is narrow, a very long workspace name, no workspace, no git repository, a multi-repository workspace, or every widget hidden. Expect: limits then machine drop first, long names ellipsize, absent data means absent widgets, the bar strip stays so a right-click can bring widgets back. [Tasks 8 and 11]

---

## File structure

| File | Responsibility |
| --- | --- |
| `crates/mkrew-protocol/src/message.rs` (modify) | `LimitWindow`, `AgentLimits` (+ `supersedes`), `MachineStats`; `Event::AgentLimits`, `Event::MachineStats`; `Request::LimitsGet`, `Request::LimitsReport` |
| `crates/mkrew-protocol/src/lib.rs` (modify) | `PROTOCOL_VERSION = 55` |
| `crates/mkrew-core/src/settings.rs`, `settings_fields.rs` (modify) | `[ui] widget_bar`, `[ui] widgets_hidden`, `Kind::Strings`, `write_widgets_hidden` |
| `crates/mkrew-core/src/usage.rs` (modify) | `UsageReader::limits()` from Codex `rate_limits`; RFC 3339 UTC parser |
| `crates/mkrew-daemon/src/limits.rs` (create) | `Limits` store (newest per agent), `checked` validation |
| `crates/mkrew-daemon/src/machine.rs` (create) | `should_send` throttle, `ClientGuard`, the `sysinfo` sampler task |
| `crates/mkrew-daemon/src/session.rs`, `server.rs`, `usage.rs`, `lib.rs` (modify) | Hub fields `limits` and `clients`, `offer_limits`, dispatch, usage `track` hands limits on, sampler started in `serve` |
| `crates/mkrew-daemon/tests/daemon/widgets.rs` (create) | `limits.report` / `limits.get` / event and machine stats over the real socket |
| `crates/mkrew-cli/src/integration.rs` (modify) | status line install, uninstall, status and the saved file |
| `crates/mkrew-cli/src/statusline.rs` (create), `main.rs` (modify) | `mk statusline` |
| `crates/mkrew-ui/src/widget_bar.rs` (create) | pure model: `WidgetData`, `widget_list`, `fit`, text helpers, `toggled` |
| `crates/mkrew-ui/src/git_view.rs` (modify) | `Key::NextWrapping` |
| `crates/mkrew-ui/src/{keymap,palette,redraw,sidebar,settings_dialog,theme}.rs` (modify) | `Command::ToggleWidgetBar`, redraw rules, `INTEGRATIONS_TAB`, `WIDGET_BAR_*` tokens |
| `crates/mkrew-ui/src/shell/widget_bar.rs` (create) | state, actions, render, right-click menu |
| `crates/mkrew-ui/src/{main.rs,shell.rs,shell/*.rs}` (modify) | Shell fields, event routing, `limits.get`, root column child |
| `docs/decisions/0102-widget-bar.md` (create), `docs/SPEC.md`, `README.md`, `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md` (modify) | docs |

Tasks in dependency order: 1 protocol, 2 core settings, 3 core limits reader, 4 daemon limits, 5 daemon machine stats, 6 CLI integration, 7 CLI statusline, 8 UI model, 9 Git wrap step, 10 UI state, 11 UI render, 12 UI menu, 13 docs, 14 verify. Each task ends committable with its own tests green.

---

### Task 1: Protocol 55

**Files:**
- Modify: `crates/mkrew-protocol/src/message.rs:47-328` (Request), `:397-466` (Event), after `:1741` (new structs)
- Modify: `crates/mkrew-protocol/src/lib.rs:16`
- Modify: `crates/mkrew-protocol/src/message/tests.rs:129-435` (requests list), `:1318-1321` (version test), append new tests
- Modify: `crates/mkrew-ui/src/redraw.rs:98-109`, `crates/mkrew-ui/src/sidebar.rs:662-671` (their `Event` matches are exhaustive)

**Interfaces:**
- Produces (used by every later task):
  - `pub struct LimitWindow { pub used_percent: f32, pub resets_at: Option<i64> }` (`Debug, Clone, Copy, PartialEq, Serialize, Deserialize`)
  - `pub struct AgentLimits { pub agent: Cli, pub five_hour: Option<LimitWindow>, pub weekly: Option<LimitWindow>, pub observed_at: i64 }` (`Debug, Clone, PartialEq, Serialize, Deserialize`); `fn supersedes(&self, old: &AgentLimits) -> bool` = `self.observed_at >= old.observed_at && self != old`
  - `pub struct MachineStats { pub cpu_percent: f32, pub ram_used: u64, pub ram_total: u64 }` (`Debug, Clone, Copy, PartialEq, Serialize, Deserialize`)
  - `Event::AgentLimits(AgentLimits)` wire `agent.limits`; `Event::MachineStats(MachineStats)` wire `machine.stats`
  - `Request::LimitsGet` wire `limits.get` (returns `AgentLimits[]`); `Request::LimitsReport(AgentLimits)` wire `limits.report` (returns `null`)
  - `PROTOCOL_VERSION == 55`

- [ ] **Step 1: Write the failing tests**

In `crates/mkrew-protocol/src/message/tests.rs`, add `Request::LimitsGet,` and `Request::LimitsReport(limits()),` to the `requests` vec of `every_m1_request_round_trips` (after `Request::SessionPlan(...)` at line 344). Replace the last test (`the_palette_protocol_is_version_54`, lines 1318-1321) and append:

```rust
fn limits() -> AgentLimits {
    AgentLimits {
        agent: Cli::Codex,
        five_hour: Some(LimitWindow {
            used_percent: 23.5,
            resets_at: Some(1_778_533_631),
        }),
        weekly: Some(LimitWindow {
            used_percent: 46.0,
            resets_at: None,
        }),
        observed_at: 1_778_519_153,
    }
}

#[test]
fn the_widget_bar_protocol_is_version_55() {
    assert_eq!(crate::PROTOCOL_VERSION, 55);
}

#[test]
fn limits_and_machine_stats_have_their_own_wire_shapes() {
    assert_eq!(
        round_trip(&Message::Event(Event::AgentLimits(limits()))),
        json!({
            "type": "event", "event": "agent.limits",
            "data": {
                "agent": "codex",
                "five_hour": {"used_percent": 23.5, "resets_at": 1_778_533_631},
                "weekly": {"used_percent": 46.0},
                "observed_at": 1_778_519_153
            }
        })
    );
    assert_eq!(
        round_trip(&Message::Event(Event::MachineStats(MachineStats {
            cpu_percent: 23.5,
            ram_used: 11,
            ram_total: 31,
        }))),
        json!({
            "type": "event", "event": "machine.stats",
            "data": {"cpu_percent": 23.5, "ram_used": 11, "ram_total": 31}
        })
    );
    let get = round_trip(&Message::Request {
        id: 1,
        request: Request::LimitsGet,
    });
    assert_eq!(get["method"], "limits.get");
    let report = round_trip(&Message::Request {
        id: 2,
        request: Request::LimitsReport(limits()),
    });
    assert_eq!(report["method"], "limits.report");
    assert_eq!(report["params"]["agent"], "codex");
}

#[test]
fn limits_without_windows_read_with_both_absent() {
    let bare: AgentLimits =
        serde_json::from_value(json!({"agent": "claude", "observed_at": 5})).unwrap();
    assert_eq!((bare.five_hour, bare.weekly), (None, None));
    let window: LimitWindow = serde_json::from_value(json!({"used_percent": 3.0})).unwrap();
    assert_eq!(window.resets_at, None);
}

#[test]
fn a_report_replaces_an_older_different_one_only() {
    let old = limits();
    let newer = AgentLimits {
        observed_at: old.observed_at + 1,
        ..limits()
    };
    let older = AgentLimits {
        observed_at: old.observed_at - 1,
        weekly: None,
        ..limits()
    };
    let same_time = AgentLimits {
        weekly: None,
        ..limits()
    };
    assert!(newer.supersedes(&old));
    assert!(!older.supersedes(&old), "an older report is ignored");
    assert!(same_time.supersedes(&old), "a tie: the later arrival wins");
    assert!(!old.supersedes(&old), "the same content is no news");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-protocol`
Expected: FAIL to compile ("cannot find type `AgentLimits`", "no variant named `LimitsGet`").

- [ ] **Step 3: Implement**

`crates/mkrew-protocol/src/lib.rs:16`: `pub const PROTOCOL_VERSION: u32 = 55;`

`message.rs`, in `enum Request` before the closing brace at line 328 (after `SessionPlan`):

```rust
    /// Returns `AgentLimits[]`: the newest limits each agent reported, so a client that
    /// connects later has them at once (spec 2026-11-22 section 5.1).
    #[serde(rename = "limits.get")]
    LimitsGet,
    /// Returns nothing (`null`): the limits an agent's own status line (`mk statusline`)
    /// saw. `invalid_request` for an agent other than Claude and Codex or a percentage that
    /// is not a number; the daemon keeps the newest `observed_at` per agent and announces
    /// each change as `agent.limits`.
    #[serde(rename = "limits.report")]
    LimitsReport(AgentLimits),
```

In `enum Event`, before its closing brace at line 466 (after `SessionPlan`):

```rust
    /// The newest limits of one agent changed (spec 2026-11-22 section 5.1).
    #[serde(rename = "agent.limits")]
    AgentLimits(AgentLimits),
    /// CPU and RAM of the daemon's machine, sent while a client is connected (section 6).
    #[serde(rename = "machine.stats")]
    MachineStats(MachineStats),
```

After `SessionUsageEvent` (line 1741):

```rust
/// One rate-limit window of an agent account (spec 2026-11-22 section 5.1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LimitWindow {
    /// How much of the window is used, 0-100.
    pub used_percent: f32,
    /// When the window resets, Unix seconds; `None` when the agent does not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<i64>,
}

/// The newest 5-hour and weekly limits one agent reported. Limits belong to the account, not
/// to a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentLimits {
    /// `Claude` or `Codex`; the daemon refuses the other agents.
    pub agent: Cli,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour: Option<LimitWindow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekly: Option<LimitWindow>,
    /// When the agent reported them, Unix seconds.
    pub observed_at: i64,
}

impl AgentLimits {
    /// Whether `self` replaces `old`, the stored limits of the same agent: not older, and not
    /// the same content (so a repeat is no news). The daemon and the UI both use this rule, so
    /// a late answer never undoes a newer event.
    pub fn supersedes(&self, old: &AgentLimits) -> bool {
        self.observed_at >= old.observed_at && self != old
    }
}

/// CPU and RAM of the daemon's machine (spec 2026-11-22 section 6).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MachineStats {
    /// Total CPU use over all cores, 0-100.
    pub cpu_percent: f32,
    pub ram_used: u64,
    /// 0 when the OS reports no total: the client leaves the RAM part out.
    pub ram_total: u64,
}
```

UI exhaustive matches (so the workspace still compiles): in `crates/mkrew-ui/src/redraw.rs` add `| Event::AgentLimits(_)` and `| Event::MachineStats(_)` to the `=> true` group (line 98-109; Task 10 narrows them to the bar). In `crates/mkrew-ui/src/sidebar.rs` add the same two patterns to the `=> Change::None` group (line 662-671).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-protocol` then `CARGO_BUILD_JOBS=4 cargo check -p mkrew-ui -p mkrew-daemon -p mkrew-cli`
Expected: PASS; the check compiles (the daemon answers `limits.*` with its "not implemented yet" arm until Task 4).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-protocol crates/mkrew-ui/src/redraw.rs crates/mkrew-ui/src/sidebar.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(protocol): agent limits, machine stats and limits requests (protocol 55)"
```

---

### Task 2: Core settings `[ui] widget_bar` and `widgets_hidden`

**Files:**
- Modify: `crates/mkrew-core/src/settings.rs:390-452` (struct, Default), `:527` (writer), `:597-624` (template), `:866-883` (test literal), tests
- Modify: `crates/mkrew-core/src/settings_fields.rs:44-57` (Kind), `:95-106` (helpers), `:248-261` (rows), `:458-473` (literal)
- Modify: `crates/mkrew-ui/src/settings_dialog.rs:511`, `crates/mkrew-ui/src/shell/settings.rs:832` (exhaustive `Kind` matches)

**Interfaces:**
- Produces: `UiSettings { widget_bar: bool, widgets_hidden: Vec<String> }` (defaults `true`, empty); `mkrew_core::settings::write_widgets_hidden(path: &Path, ids: &[String]) -> Result<(), SettingsError>`; `settings_fields::Kind::Strings` (a TOML array of strings, never shown); rows `ui.widget_bar` (Appearance, group "Window") and hidden `ui.widgets_hidden`.

- [ ] **Step 1: Write the failing tests**

In `settings.rs` tests (use the existing `settings(..)` and `parse_err(..)` helpers; imports at the top of `mod tests` already bring `parse`, `load`, `write_ui_value`):

```rust
    #[test]
    fn the_widget_bar_defaults_to_shown_and_keeps_ids_it_does_not_know() {
        let ui = |src: &str| parse(Path::new("s.toml"), src).unwrap().ui;
        assert!(ui("").widget_bar);
        assert!(ui("").widgets_hidden.is_empty());
        assert!(!ui("[ui]\nwidget_bar = false\n").widget_bar);
        // A newer build wrote `clock`: this build keeps it.
        assert_eq!(
            ui("[ui]\nwidgets_hidden = [\"git\", \"clock\"]\n").widgets_hidden,
            ["git", "clock"]
        );
        assert!(parse_err("[ui]\nwidgets_hidden = \"git\"\n").contains("invalid type"));
    }

    #[test]
    fn write_widgets_hidden_round_trips_and_keeps_the_rest_of_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "[ui]\n# keep me\ntheme = \"dark\"\n").unwrap();
        let ids = vec!["git".to_owned(), "clock".to_owned()];
        write_widgets_hidden(&path, &ids).unwrap();
        assert_eq!(load(&path).unwrap().ui.widgets_hidden, ids);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# keep me") && text.contains("theme = \"dark\""), "{text}");
        write_widgets_hidden(&path, &[]).unwrap();
        assert!(load(&path).unwrap().ui.widgets_hidden.is_empty());
    }
```

Also update the `full_file_parses` expected literal (lines 866-883): add `widget_bar: true,` and `widgets_hidden: vec![],` after `md_split: MD_SPLIT_DEFAULT,`.

In `settings_fields.rs` tests append:

```rust
    #[test]
    fn the_widget_bar_switch_is_a_setting_and_its_hidden_list_is_not_shown() {
        let defaults = Settings::default();
        assert_eq!(setting("ui.widget_bar").current(&defaults), "true");
        assert!(!setting("ui.widget_bar").hidden);
        assert!(setting("ui.widgets_hidden").hidden);
        assert_eq!(setting("ui.widgets_hidden").current(&defaults), "[]");
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-core settings`
Expected: FAIL to compile (`no field widget_bar`, `write_widgets_hidden` missing).

- [ ] **Step 3: Implement**

`settings.rs`, in `UiSettings` after `md_split` (line 427):

```rust
    /// `[ui] widget_bar` (ADR 0102): the thin bar at the bottom of the main window with the
    /// folder, Git branch, agent limits, machine load and version.
    pub widget_bar: bool,
    /// `[ui] widgets_hidden`: ids of the bar's widgets the user hid (`folder`, `git`,
    /// `limits`, `machine`, `version`). Unknown ids are kept and ignored, so an older build
    /// does not lose a newer build's choice.
    pub widgets_hidden: Vec<String>,
```

In `Default` after `md_split: MD_SPLIT_DEFAULT,`: `widget_bar: true, widgets_hidden: Vec::new(),`.

After `write_ui_value` (line 529):

```rust
/// Saves `[ui] widgets_hidden`; see [`write_value`]. The ids are written as given, so ids this
/// build does not know are never dropped by the caller's toggle.
pub fn write_widgets_hidden(path: &Path, ids: &[String]) -> Result<(), SettingsError> {
    let list = toml::Value::Array(ids.iter().cloned().map(toml::Value::String).collect());
    write_value(path, "ui", "widgets_hidden", &list.to_string())
}
```

Template: in the comment block before `[ui]` (after the `# Markdown split: ...` line 607) add

```
     # Widget bar: the thin bar at the bottom with the folder, Git branch, agent limits, machine load and version.\n\
     # widgets_hidden lists the ids you hid: folder, git, limits, machine, version (the bar's right-click menu writes it).\n\
```

and after `# md_split = 50\n\` (line 624) add `# widget_bar = true\n\` and `# widgets_hidden = []\n\`.

`settings_fields.rs`: add to `enum Kind` (after `List`):

```rust
    /// A TOML array of strings; set by another control (the widget bar's menu), never shown.
    Strings,
```

Add next to `hidden()`:

```rust
const fn hidden_list(section: &'static str, key: &'static str) -> Setting {
    Setting {
        page: Page::Appearance,
        group: "",
        section,
        key,
        kind: Kind::Strings,
        label: "",
        help: "",
        hidden: true,
    }
}
```

In `literal`, add before the `_ =>` arm: `Kind::Strings => text.to_owned(),` (the text is already a TOML array, as `current` shows it). After the Markdown `md_mode` row (line 256) and before `hidden("ui", "md_split")` add:

```rust
    row(
        Appearance,
        "Window",
        "ui",
        "widget_bar",
        Bool,
        "Widget bar",
        "A thin bar at the bottom with the folder, Git branch, agent limits, machine load and version.",
    ),
    hidden_list("ui", "widgets_hidden"),
```

UI exhaustive matches: `settings_dialog.rs:511` becomes `Kind::Font | Kind::List(_) | Kind::Strings => Effect::None,`; in `shell/settings.rs` after the `Kind::Font | Kind::List(_) => { ... }` arm (ends line 852) add `Kind::Strings => div().into_any_element(),` (hidden rows are never drawn).

- [ ] **Step 4: Run to verify pass**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-core settings` then `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui settings_dialog`
Expected: PASS, including `every_template_key_has_exactly_one_setting`, `writing_each_default_reads_it_back_and_keeps_comments` and the template-equals-defaults test.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-core/src/settings.rs crates/mkrew-core/src/settings_fields.rs crates/mkrew-ui/src/settings_dialog.rs crates/mkrew-ui/src/shell/settings.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(core): [ui] widget_bar and widgets_hidden settings"
```

---

### Task 3: `UsageReader` reads the Codex rate limits

**Files:**
- Modify: `crates/mkrew-core/src/usage.rs:8-12` (imports), `:50-66` (struct), `:79-89` (`new`), after `:103` (`limits()`), `:214-248` (`codex_line`), tests at `:292-500`
- Test fixture (unchanged): `crates/mkrew-core/tests/fixtures/usage/codex.jsonl` (line 4 carries `rate_limits` with `"info":null`; lines 6 and 9 carry `"rate_limits":null`)

**Interfaces:**
- Consumes: `AgentLimits`, `LimitWindow`, `Cli` (Task 1).
- Produces: `UsageReader::limits(&self) -> Option<&AgentLimits>` (Codex readers only; `None` for Claude and omp); `observed_at` is the transcript line's own timestamp in Unix seconds (0 when the line has none).

- [ ] **Step 1: Write the failing tests**

In `usage.rs` `mod tests`:

```rust
    fn codex_limits() -> AgentLimits {
        AgentLimits {
            agent: Cli::Codex,
            five_hour: Some(LimitWindow {
                used_percent: 23.0,
                resets_at: Some(1_778_533_631),
            }),
            weekly: Some(LimitWindow {
                used_percent: 46.0,
                resets_at: Some(1_778_571_719),
            }),
            // 2026-05-11T17:05:53.772Z, the timestamp of the line that carries them.
            observed_at: 1_778_519_153,
        }
    }

    /// The fixture's only non-null `rate_limits` sits on a `token_count` line whose `info` is
    /// null; the later lines carry `"rate_limits": null` and keep them.
    #[test]
    fn codex_limits_come_from_the_rate_limits_of_a_token_count_line() {
        let mut r = reader(&UsageFormat::CodexJsonl);
        assert_eq!(r.limits(), None);
        r.feed(CODEX);
        assert_eq!(r.limits(), Some(&codex_limits()));
        let mut chunked = reader(&UsageFormat::CodexJsonl);
        for chunk in CODEX.chunks(7) {
            chunked.feed(chunk);
        }
        assert_eq!(chunked.limits(), Some(&codex_limits()));
    }

    #[test]
    fn windows_of_another_length_are_ignored_and_the_rest_still_read() {
        let line = br#"{"timestamp":"2026-05-11T17:05:53Z","type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"primary":{"used_percent":9.0,"window_minutes":60,"resets_at":1},"secondary":{"used_percent":4.0,"window_minutes":300,"resets_at":2}}}}
"#;
        let mut r = reader(&UsageFormat::CodexJsonl);
        r.feed(line);
        let limits = r.limits().unwrap();
        assert_eq!(limits.weekly, None);
        assert_eq!(limits.five_hour.map(|w| w.used_percent), Some(4.0));
        let only_unknown = br#"{"type":"event_msg","payload":{"type":"token_count","info":null,"rate_limits":{"primary":{"used_percent":9.0,"window_minutes":60}}}}
"#;
        let mut none = reader(&UsageFormat::CodexJsonl);
        none.feed(only_unknown);
        assert_eq!(none.limits(), None);
    }

    #[test]
    fn transcripts_without_limits_still_give_their_usage() {
        for (format, bytes, expected) in fixtures() {
            let mut r = reader(&format);
            assert_eq!(r.feed(bytes), Some(expected), "{format:?}");
            if format != UsageFormat::CodexJsonl {
                assert_eq!(r.limits(), None, "{format:?}");
            }
        }
        let no_limits = br#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":5,"output_tokens":2},"last_token_usage":{"total_tokens":7}}}}
"#;
        let mut r = reader(&UsageFormat::CodexJsonl);
        assert!(r.feed(no_limits).is_some());
        assert_eq!(r.limits(), None);
    }

    #[test]
    fn utc_stamps_become_unix_seconds() {
        assert_eq!(utc_seconds("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(utc_seconds("2024-02-29T00:00:00Z"), Some(1_709_164_800));
        assert_eq!(utc_seconds("2026-05-11T17:05:53.772Z"), Some(1_778_519_153));
        assert_eq!(utc_seconds("2026-05-11T17:05:53+07:00"), None);
        assert_eq!(utc_seconds("yesterday"), None);
        assert_eq!(utc_seconds("2026-13-01T00:00:00Z"), None);
    }
```

(`UsageFormat` must derive `PartialEq` for the `!=`; it already does if `Debug, PartialEq`; if not, compare `matches!(format, UsageFormat::CodexJsonl)`.)

- [ ] **Step 2: Run to verify failure**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-core usage`
Expected: FAIL to compile (`no method named limits`, `utc_seconds` missing).

- [ ] **Step 3: Implement**

`usage.rs` imports (line 8-12): add `use mkrew_protocol::{AgentLimits, Cli, LimitWindow};`.

`UsageReader` gets a field `limits: Option<AgentLimits>,` (after `model`) initialised `limits: None,` in `new`, and:

```rust
    /// The newest limits a Codex transcript named (its `token_count` lines carry them, also
    /// while `info` is null); `None` for the other formats and until a line names them.
    pub fn limits(&self) -> Option<&AgentLimits> {
        self.limits.as_ref()
    }
```

In `codex_line`, replace the two lines

```rust
        if value["type"] != "event_msg" || payload["type"] != "token_count" {
            return Line::Other;
        }
        let info = &payload["info"];
```
with
```rust
        if value["type"] != "event_msg" || payload["type"] != "token_count" {
            return Line::Other;
        }
        // Before the `info` check: the first answer carries them with `info: null`.
        if let Some(limits) = codex_limits(value) {
            self.limits = Some(limits);
        }
        let info = &payload["info"];
```

Free functions after `enum Line`:

```rust
/// `payload.rate_limits` of a Codex `token_count` line: `primary` and `secondary` are told
/// apart by `window_minutes` (300 = 5 hours, 10080 = a week); another length is ignored. `None`
/// when the line has no limits (`null`) or only unknown windows, so the last ones stay.
fn codex_limits(value: &Value) -> Option<AgentLimits> {
    let rate = &value["payload"]["rate_limits"];
    let (mut five_hour, mut weekly) = (None, None);
    for key in ["primary", "secondary"] {
        let window = &rate[key];
        let (Some(used), Some(minutes)) = (
            window["used_percent"].as_f64(),
            window["window_minutes"].as_u64(),
        ) else {
            continue;
        };
        let parsed = LimitWindow {
            used_percent: used as f32,
            resets_at: window["resets_at"].as_i64(),
        };
        match minutes {
            300 => five_hour = Some(parsed),
            10_080 => weekly = Some(parsed),
            _ => {}
        }
    }
    (five_hour.is_some() || weekly.is_some()).then(|| AgentLimits {
        agent: Cli::Codex,
        five_hour,
        weekly,
        observed_at: value["timestamp"]
            .as_str()
            .and_then(utc_seconds)
            .unwrap_or(0),
    })
}

/// `2026-05-11T17:05:53.772Z` (what Codex writes) as Unix seconds; any other form is `None`.
fn utc_seconds(stamp: &str) -> Option<i64> {
    let (date, time) = stamp.strip_suffix('Z')?.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (year, month, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.split('.').next()?.split(':').map(|p| p.parse::<i64>().ok());
    let (hour, minute, second) = (t.next()??, t.next()??, t.next()??);
    let valid = (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && (0..24).contains(&hour)
        && (0..60).contains(&minute)
        && (0..61).contains(&second);
    valid.then(|| days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second)
}

/// Days from 1970-01-01 to the given proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}
```

- [ ] **Step 4: Run to verify pass**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-core usage`
Expected: PASS (the existing fixture, chunk and partial-line tests still pass).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-core/src/usage.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(core): UsageReader reads the Codex rate limits"
```

---

### Task 4: Daemon agent limits (`limits.get`, `limits.report`, Codex hand-off)

**Files:**
- Create: `crates/mkrew-daemon/src/limits.rs`
- Modify: `crates/mkrew-daemon/src/lib.rs:6-37` (add `mod limits;`), `crates/mkrew-daemon/src/session.rs:75-111` (Hub fields), `:326-347` (`Hub::new`), after `:362` (methods), `crates/mkrew-daemon/src/server.rs:503` (two dispatch arms), `crates/mkrew-daemon/src/usage.rs:191-242` (`run`) and `:244-352` (tests)
- Create: `crates/mkrew-daemon/tests/daemon/widgets.rs`; modify `crates/mkrew-daemon/tests/daemon/main.rs:47-63` (add `mod widgets;`)

**Interfaces:**
- Consumes: `AgentLimits`, `LimitWindow`, `Cli`, `Event::AgentLimits`, `Request::LimitsGet/LimitsReport` (Task 1); `UsageReader::limits()` (Task 3).
- Produces: `Hub::offer_limits(&self, AgentLimits) -> Result<bool, String>` (validates, stores when `supersedes`, broadcasts `Event::AgentLimits`; `Err(message)` for an agent other than Claude and Codex or a non-finite percentage; percentages are clamped to 0-100); `Hub::limits(&self) -> Vec<AgentLimits>`; the requests `limits.get` (list) and `limits.report` (`null`, `invalid_request` on `Err`).

- [ ] **Step 1: Write the failing tests**

The unit tests of the store sit in `limits.rs` and are written with it in step 3. The integration tests come first. Create `crates/mkrew-daemon/tests/daemon/widgets.rs`:

```rust
//! Agent limits and machine stats (spec 2026-11-22 sections 5 and 6) over the real socket.

use super::*;

fn window(percent: f32) -> Option<LimitWindow> {
    Some(LimitWindow {
        used_percent: percent,
        resets_at: Some(4_000_000_000),
    })
}

fn report(agent: Cli, percent: f32, at: i64) -> Request {
    Request::LimitsReport(AgentLimits {
        agent,
        five_hour: window(percent),
        weekly: None,
        observed_at: at,
    })
}

fn listed(client: &mut Client) -> Vec<AgentLimits> {
    client.call(Request::LimitsGet).into_result().unwrap()
}

#[test]
fn a_report_is_listed_and_announced_and_the_newest_one_wins() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    assert_eq!(listed(&mut client), vec![]);

    client
        .call(report(Cli::Claude, 40.0, 100))
        .into_result::<()>()
        .unwrap();
    client.wait_event_where(
        |e| matches!(e, Event::AgentLimits(l) if l.agent == Cli::Claude && l.observed_at == 100),
    );
    assert_eq!(listed(&mut client).len(), 1);

    // An older report is ignored and not announced; a newer one replaces.
    client
        .call(report(Cli::Claude, 90.0, 50))
        .into_result::<()>()
        .unwrap();
    client
        .call(report(Cli::Claude, 55.0, 200))
        .into_result::<()>()
        .unwrap();
    client.wait_event_where(|e| matches!(e, Event::AgentLimits(l) if l.observed_at == 200));
    let all = listed(&mut client);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].five_hour.map(|w| w.used_percent), Some(55.0));
    assert!(
        !client
            .events
            .iter()
            .any(|e| matches!(e, Event::AgentLimits(l) if l.observed_at == 50)),
        "an older report is not announced"
    );

    // Limits are per agent; a client that connects later reads them at once.
    client
        .call(report(Cli::Codex, 10.0, 60))
        .into_result::<()>()
        .unwrap();
    let mut later = Client::connect(&env.socket).unwrap();
    later.hello().unwrap();
    assert_eq!(listed(&mut later).len(), 2);
    client.stop();
}

#[test]
fn only_claude_and_codex_may_report_limits() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    let outcome = client.call(report(Cli::Omp, 10.0, 1));
    assert_eq!(error_code(outcome), ErrorCode::InvalidRequest);
    assert_eq!(listed(&mut client), vec![]);
    client.stop();
}

#[test]
fn a_percentage_beyond_the_scale_is_clamped() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    client
        .call(report(Cli::Claude, 140.0, 1))
        .into_result::<()>()
        .unwrap();
    assert_eq!(
        listed(&mut client)[0].five_hour.map(|w| w.used_percent),
        Some(100.0)
    );
    client.stop();
}
```

Add `mod widgets;` to `crates/mkrew-daemon/tests/daemon/main.rs` (after `mod web;`, line 61). If `Cli`, `AgentLimits`, `LimitWindow` are not in scope through `use super::*`, add them to the `use mkrew_protocol::{..}` list at the top of `main.rs`.

- [ ] **Step 2: Run to verify failure**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-daemon --test daemon widgets`
Expected: FAIL (`limits.report is not implemented yet`, the unwrap on the first report panics).

- [ ] **Step 3: Implement**

Create `crates/mkrew-daemon/src/limits.rs`:

```rust
//! The newest rate limits of each agent (spec 2026-11-22 section 5.1). Memory only: limits
//! belong to the account, not to a session, and the next turn or status line run fills them
//! again after a restart.

use mkrew_protocol::{AgentLimits, Cli};

/// At most one entry per agent.
#[derive(Debug, Default)]
pub struct Limits {
    list: Vec<AgentLimits>,
}

impl Limits {
    /// Keeps `new` when it is news (see [`AgentLimits::supersedes`]); returns whether it did.
    pub fn offer(&mut self, new: AgentLimits) -> bool {
        match self.list.iter_mut().find(|old| old.agent == new.agent) {
            Some(old) if new.supersedes(old) => {
                *old = new;
                true
            }
            Some(_) => false,
            None => {
                self.list.push(new);
                true
            }
        }
    }

    pub fn all(&self) -> Vec<AgentLimits> {
        self.list.clone()
    }
}

/// `limits` as the daemon keeps them: only Claude and Codex report, a percentage must be a
/// number and is clamped to 0-100.
pub fn checked(mut limits: AgentLimits) -> Result<AgentLimits, String> {
    if !matches!(limits.agent, Cli::Claude | Cli::Codex) {
        return Err(format!("{:?} does not report limits", limits.agent));
    }
    for window in [&mut limits.five_hour, &mut limits.weekly]
        .into_iter()
        .flatten()
    {
        if !window.used_percent.is_finite() {
            return Err("used_percent is not a number".into());
        }
        window.used_percent = window.used_percent.clamp(0.0, 100.0);
    }
    Ok(limits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Hub;
    use mkrew_protocol::{Event, LimitWindow};
    use mkrew_store::Store;

    fn limits(agent: Cli, five: f32, at: i64) -> AgentLimits {
        AgentLimits {
            agent,
            five_hour: Some(LimitWindow {
                used_percent: five,
                resets_at: Some(9_999),
            }),
            weekly: None,
            observed_at: at,
        }
    }

    #[test]
    fn the_newest_report_of_each_agent_wins() {
        let mut store = Limits::default();
        assert!(store.offer(limits(Cli::Claude, 10.0, 100)));
        assert!(store.offer(limits(Cli::Codex, 5.0, 50)), "another agent");
        assert!(!store.offer(limits(Cli::Claude, 90.0, 99)), "older");
        assert!(!store.offer(limits(Cli::Claude, 10.0, 100)), "same content");
        assert!(store.offer(limits(Cli::Claude, 20.0, 100)), "a tie: the later one");
        assert!(store.offer(limits(Cli::Claude, 30.0, 200)));
        let all = store.all();
        assert_eq!(all.len(), 2);
        let claude = all.iter().find(|l| l.agent == Cli::Claude).unwrap();
        assert_eq!(claude.five_hour.map(|w| w.used_percent), Some(30.0));
    }

    #[test]
    fn other_agents_and_non_numbers_are_refused_and_percentages_clamped() {
        assert!(checked(limits(Cli::Omp, 1.0, 1)).is_err());
        assert!(checked(limits(Cli::Copilot, 1.0, 1)).is_err());
        assert!(checked(limits(Cli::Claude, f32::NAN, 1)).is_err());
        assert!(checked(limits(Cli::Codex, f32::INFINITY, 1)).is_err());
        let clamped = checked(limits(Cli::Claude, -5.0, 1)).unwrap();
        assert_eq!(clamped.five_hour.map(|w| w.used_percent), Some(0.0));
        let clamped = checked(limits(Cli::Claude, 120.0, 1)).unwrap();
        assert_eq!(clamped.five_hour.map(|w| w.used_percent), Some(100.0));
    }

    #[test]
    fn the_hub_announces_a_change_once() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Hub::new(Store::open(&dir.path().join("mkrew.db")).unwrap());
        let mut events = hub.events.subscribe();
        assert_eq!(hub.offer_limits(limits(Cli::Claude, 10.0, 100)), Ok(true));
        assert!(matches!(
            events.try_recv(),
            Ok(Event::AgentLimits(l)) if l.observed_at == 100
        ));
        assert_eq!(hub.offer_limits(limits(Cli::Claude, 10.0, 100)), Ok(false));
        assert!(events.try_recv().is_err(), "a repeat is not announced");
        assert!(hub.offer_limits(limits(Cli::Omp, 1.0, 1)).is_err());
        assert_eq!(hub.limits().len(), 1);
    }
}
```

`lib.rs`: add `mod limits;` (alphabetical, after `mod launch;`).

`session.rs`: add the field to `Hub` (after `plans`, line 110):

```rust
    /// The newest limits of each agent (spec 2026-11-22 section 5.1); memory only.
    limits: Mutex<crate::limits::Limits>,
```

`Hub::new` (after `plans: ...,` line 345): `limits: Mutex::default(),`. After `terminal_title` (line 362) add:

```rust
    /// The newest limits each agent reported, for `limits.get`.
    pub fn limits(&self) -> Vec<AgentLimits> {
        lock(&self.limits).all()
    }

    /// Validates `limits` ([`crate::limits::checked`]), keeps them when they are news and
    /// announces the change as `agent.limits`; returns whether they were news. A repeat or an
    /// older report sends nothing.
    pub fn offer_limits(&self, limits: AgentLimits) -> Result<bool, String> {
        let limits = crate::limits::checked(limits)?;
        let changed = lock(&self.limits).offer(limits.clone());
        if changed {
            let _ = self.events.send(Event::AgentLimits(limits));
        }
        Ok(changed)
    }
```

(import `AgentLimits` in the `mkrew_protocol::{..}` use of `session.rs` if the compiler asks.)

`server.rs`, in `handle` before the `other =>` arm (line 503):

```rust
            Request::LimitsGet => (ok(&self.hub.limits()), After::Continue),
            Request::LimitsReport(params) => match self.hub.offer_limits(params) {
                Ok(_) => (ok(&()), After::Continue),
                Err(message) => (
                    Outcome::error(ErrorCode::InvalidRequest, message),
                    After::Continue,
                ),
            },
```

`usage.rs`: in `run` add `let mut last_limits: Option<AgentLimits> = None;` next to `last` (line 198) and replace the blocking read (lines 201-210) with:

```rust
        let read = tokio::task::spawn_blocking(move || {
            let usage = tracker.read(&native);
            let warning = tracker.reader.take_warning();
            let limits = tracker.reader.limits().cloned();
            (tracker, usage, warning, limits)
        })
        .await;
        let Ok((back, usage, warning, limits)) = read else {
            return;
        };
```

and, right after the `if let Some(warning) = warning { ... }` block (before `let Some(usage) = usage.filter(..)`, whose `continue` would skip it):

```rust
        // Account-wide, so not tied to this session's usage lines: a Codex transcript names
        // the limits on lines without token counts too (spec 2026-11-22 section 5.2).
        if let Some(limits) = limits.filter(|l| last_limits.as_ref() != Some(l)) {
            last_limits = Some(limits.clone());
            if let Err(e) = hub.offer_limits(limits) {
                tracing::debug!(id = %session, "limits not kept: {e}");
            }
        }
```

(`use mkrew_protocol::{AgentLimits, Event, SessionUsageEvent};` in `usage.rs`.)

Add a tracker test to `usage.rs` `mod tests`:

```rust
    const CODEX: &[u8] = include_bytes!("../../mkrew-core/tests/fixtures/usage/codex.jsonl");

    #[test]
    fn the_tracker_hands_on_the_codex_limits_with_the_usage() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("sessions/2026/05/11");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("rollout-2026-05-11T17-04-33-0199a-thread.jsonl"), CODEX).unwrap();
        let mut tracker =
            Tracker::new(UsageFormat::CodexJsonl, Some(root.path().to_owned())).unwrap();
        assert_eq!(tracker.reader.limits(), None);
        assert!(tracker.read("0199a-thread").is_some());
        let limits = tracker.reader.limits().unwrap();
        assert_eq!(limits.five_hour.map(|w| w.used_percent), Some(23.0));
        assert_eq!(limits.weekly.map(|w| w.used_percent), Some(46.0));
    }
```

- [ ] **Step 4: Run to verify pass**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-daemon limits` then `CARGO_BUILD_JOBS=4 cargo test -p mkrew-daemon usage` then `CARGO_BUILD_JOBS=4 cargo test -p mkrew-daemon --test daemon widgets`
Expected: PASS. (The daemon integration tests need `mkrew-daemon` and `mkrew-fakeagent` built; `cargo test -p mkrew-daemon` builds them.)

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-daemon
git -c user.email=tungnv975@gmail.com commit -m "feat(daemon): agent limits store, limits.get and limits.report"
```

---

### Task 5: Daemon machine stats sampler

**Files:**
- Create: `crates/mkrew-daemon/src/machine.rs`
- Modify: `crates/mkrew-daemon/src/lib.rs` (add `mod machine;`), `crates/mkrew-daemon/src/session.rs:75-111` (Hub field `clients`), `:326-347`, `crates/mkrew-daemon/src/server.rs:36` (import), `:126-141` (start and abort the task), `:202`, `:238-242` (forwarder with guard)
- Modify: `crates/mkrew-daemon/tests/daemon/widgets.rs` (one more test)

**Interfaces:**
- Consumes: `MachineStats`, `Event::MachineStats` (Task 1); the `sysinfo` 0.31 dependency the daemon already has.
- Produces: `machine::should_send(last: Option<&MachineStats>, since_last: Duration, now: &MachineStats) -> bool`; `machine::SAMPLE_EVERY` (2 s), `machine::HEARTBEAT` (10 s); `machine::ClientGuard::join(&Arc<Hub>)` (counts one subscribed client until dropped); `machine::start(Arc<Hub>) -> JoinHandle<()>`; `Hub::clients: watch::Sender<usize>`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/mkrew-daemon/tests/daemon/widgets.rs`:

```rust
#[test]
fn machine_stats_arrive_while_a_client_is_connected() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    // Real `sysinfo` on this machine, nothing else: the first reading comes after one
    // sampling period (2 s).
    let Event::MachineStats(stats) =
        client.wait_event_where(|e| matches!(e, Event::MachineStats(_)))
    else {
        unreachable!()
    };
    assert!((0.0..=100.0).contains(&stats.cpu_percent), "{stats:?}");
    assert!(stats.ram_total == 0 || stats.ram_used <= stats.ram_total, "{stats:?}");
    client.stop();
}
```

The unit tests live in `machine.rs` (step 3 file, written together with the code because `should_send` is pure):

- [ ] **Step 2: Run to verify failure**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-daemon --test daemon machine_stats`
Expected: FAIL after the 20 s harness timeout ("no matching event").

- [ ] **Step 3: Implement**

Create `crates/mkrew-daemon/src/machine.rs`:

```rust
//! CPU and RAM of this machine for the widget bar (spec 2026-11-22 section 6). A sampler task
//! reads `sysinfo` every [`SAMPLE_EVERY`] while at least one client is connected and announces
//! `machine.stats` when a value moved enough ([`should_send`]) or [`HEARTBEAT`] has passed.
//! With no client connected the task waits and does no work.
//!
//! Every connection that said `hello` counts, `mk web`'s relay included; a short-lived `mk`
//! call connects and leaves before the first sample is due.

use std::sync::Arc;
use std::time::Duration;

use mkrew_protocol::{Event, MachineStats};
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior};

use crate::session::Hub;

pub const SAMPLE_EVERY: Duration = Duration::from_secs(2);
pub const HEARTBEAT: Duration = Duration::from_secs(10);
/// CPU moves of at least this many percentage points are news.
const CPU_STEP: f32 = 1.0;
/// RAM moves of at least this many bytes (100 MiB) are news.
const RAM_STEP: u64 = 100 * 1024 * 1024;

/// Whether `now` is sent after `last` (what was last sent, `since_last` ago): the first reading,
/// a CPU move of 1 point, a RAM move of 100 MB, another RAM total, or [`HEARTBEAT`] without
/// news. Compared with what was last SENT, so slow drift still gets through.
pub fn should_send(last: Option<&MachineStats>, since_last: Duration, now: &MachineStats) -> bool {
    let Some(last) = last else {
        return true;
    };
    since_last >= HEARTBEAT
        || (now.cpu_percent - last.cpu_percent).abs() >= CPU_STEP
        || now.ram_used.abs_diff(last.ram_used) >= RAM_STEP
        || now.ram_total != last.ram_total
}

/// One connected client; the sampler runs while any exists.
pub struct ClientGuard(Arc<Hub>);

impl ClientGuard {
    pub fn join(hub: &Arc<Hub>) -> Self {
        hub.clients.send_modify(|count| *count += 1);
        Self(Arc::clone(hub))
    }
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.0
            .clients
            .send_modify(|count| *count = count.saturating_sub(1));
    }
}

fn read(system: &mut sysinfo::System) -> MachineStats {
    system.refresh_cpu_usage();
    system.refresh_memory();
    MachineStats {
        cpu_percent: system.global_cpu_usage(),
        ram_used: system.used_memory(),
        ram_total: system.total_memory(),
    }
}

/// The sampler task; runs until aborted.
pub fn start(hub: Arc<Hub>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut clients = hub.clients.subscribe();
        let mut system = sysinfo::System::new();
        loop {
            // Idle: nothing runs until a client is connected.
            while *clients.borrow_and_update() == 0 {
                if clients.changed().await.is_err() {
                    return;
                }
            }
            // A CPU reading needs an earlier one to compare with.
            system.refresh_cpu_usage();
            let mut ticks = tokio::time::interval_at(Instant::now() + SAMPLE_EVERY, SAMPLE_EVERY);
            ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
            let mut last: Option<(MachineStats, Instant)> = None;
            loop {
                let at = tokio::select! {
                    at = ticks.tick() => at,
                    changed = clients.changed() => {
                        if changed.is_err() {
                            return;
                        }
                        if *clients.borrow_and_update() == 0 {
                            break;
                        }
                        // A client that joined needs the figures at the next sample, not at
                        // the next heartbeat.
                        last = None;
                        continue;
                    }
                };
                let now = read(&mut system);
                let since = last.as_ref().map_or(Duration::MAX, |(_, sent)| {
                    at.saturating_duration_since(*sent)
                });
                if should_send(last.as_ref().map(|(stats, _)| stats), since, &now) {
                    last = Some((now, at));
                    let _ = hub.events.send(Event::MachineStats(now));
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_store::Store;

    const MB: u64 = 1024 * 1024;

    fn stats(cpu: f32, used_mb: u64) -> MachineStats {
        MachineStats {
            cpu_percent: cpu,
            ram_used: used_mb * MB,
            ram_total: 32 * 1024 * MB,
        }
    }

    #[test]
    fn the_first_reading_is_always_sent() {
        assert!(should_send(None, Duration::ZERO, &stats(0.0, 0)));
    }

    #[test]
    fn a_small_change_waits_for_the_heartbeat() {
        let last = stats(23.0, 1000);
        let two = Duration::from_secs(2);
        assert!(!should_send(Some(&last), two, &stats(23.5, 1050)));
        assert!(!should_send(Some(&last), two, &stats(23.99, 1000)), "just under a point");
        assert!(should_send(Some(&last), two, &stats(24.0, 1000)), "a point");
        assert!(should_send(Some(&last), two, &stats(22.0, 1000)), "down counts too");
        assert!(!should_send(Some(&last), two, &stats(23.0, 1099)), "just under 100 MB");
        assert!(should_send(Some(&last), two, &stats(23.0, 1100)), "100 MB");
        assert!(should_send(Some(&last), two, &stats(23.0, 900)), "freed RAM counts too");
        assert!(!should_send(Some(&last), Duration::from_millis(9_900), &last));
        assert!(should_send(Some(&last), HEARTBEAT, &last), "the heartbeat");
        let other_total = MachineStats {
            ram_total: 16 * 1024 * MB,
            ..last
        };
        assert!(should_send(Some(&last), two, &other_total), "another total");
    }

    #[test]
    fn the_sampler_runs_only_while_a_client_is_counted() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Arc::new(Hub::new(
            Store::open(&dir.path().join("mkrew.db")).unwrap(),
        ));
        assert_eq!(*hub.clients.borrow(), 0);
        let (a, b) = (ClientGuard::join(&hub), ClientGuard::join(&hub));
        assert_eq!(*hub.clients.borrow(), 2);
        drop(a);
        assert_eq!(*hub.clients.borrow(), 1);
        drop(b);
        assert_eq!(*hub.clients.borrow(), 0);
    }
}
```

`lib.rs`: add `mod machine;` (after `mod lock;`).

`session.rs`: Hub field (after `limits`): 

```rust
    /// Connections that said `hello` and receive events; the machine sampler runs while it is
    /// above 0 ([`crate::machine`]).
    pub clients: tokio::sync::watch::Sender<usize>,
```

`Hub::new`: `clients: tokio::sync::watch::channel(0).0,`.

`server.rs`: import `use crate::machine::{self, ClientGuard};` (add `machine` to the line-36 `use crate::{awake, backup, plan, ...}` group instead of a separate line if preferred). In `serve`, after `let backups = backup::start(...)` (line 131): `let machine = machine::start(Arc::clone(&self.hub));` and after `backups.abort();` (line 137): `machine.abort();`. In `connection`: change line 202 to `let mut forwarder: Option<(AbortOnDrop, ClientGuard)> = None;` and lines 241 to

```rust
                    forwarder = Some((
                        AbortOnDrop(tokio::spawn(forward(events, out.clone()))),
                        ClientGuard::join(&self.hub),
                    ));
```

- [ ] **Step 4: Run to verify pass**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-daemon machine` then `CARGO_BUILD_JOBS=4 cargo test -p mkrew-daemon --test daemon widgets`
Expected: PASS (the sampler event arrives about 2 s after `hello`).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-daemon
git -c user.email=tungnv975@gmail.com commit -m "feat(daemon): machine stats sampler with a throttle rule"
```

---

### Task 6: `mk integration` sets, reports and removes Claude's status line

**Files:**
- Modify: `crates/mkrew-cli/src/integration.rs:22-26` (consts), `:57-62` (`run`), `:128-155` (`run_on`), `:329-408` (`claude_status`, `claude_edit`), new fns, tests `:470-796`
- Modify: `crates/mkrew-cli/tests/cli.rs` (one integration test after `integration_install_status_uninstall`, line 885)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `pub fn saved_statusline_file() -> Option<PathBuf>` (`<CLAUDE_CONFIG_DIR or ~/.claude>/claude-statusline.json`, next to `settings.json`); `claude_edit(text, install, mk, saved: Option<&Value>) -> Result<Edited>` with `Edited { text: Option<String>, saved: Saved }` and `Saved::{Keep, Write(Value), Delete}`; `claude_status` is `Yes` only with the hooks and Mkrew's status line, `Partly` when the hooks are installed but the status line is not. The status line value is `{"type": "command", "command": "\"<mk>\" statusline"}` (the path is quoted: it may hold spaces). The saved file holds the user's own `statusLine` value as it was.

- [ ] **Step 1: Write the failing tests**

In `integration.rs` `mod tests`, add at the top (after the `OLD_MK` consts) a shim so the existing hook tests keep their call shape (a local fn shadows the glob import):

```rust
    /// The text of an edit with no saved status line on disk (most tests do not care).
    fn claude_edit(text: &str, install: bool, mk: &str) -> anyhow::Result<Option<String>> {
        super::claude_edit(text, install, mk, None).map(|edited| edited.text)
    }
```

Change the existing assertion in `claude_install_merges_and_is_idempotent` (line 587) to:

```rust
        assert_eq!(
            keys,
            ["model", "hooks", "env", "statusLine"],
            "key order is kept; the status line goes last"
        );
```

Append new tests:

```rust
    fn statusline(text: &str) -> Value {
        json(text)["statusLine"].clone()
    }

    #[test]
    fn the_status_line_command_quotes_the_path() {
        assert_eq!(statusline_command(MK), format!("\"{MK}\" statusline"));
        assert!(statusline_is_ours(&statusline_value(MK)));
        assert!(statusline_is_ours(&statusline_value(OLD_MK)), "any mk");
        assert!(statusline_is_ours(&json!({"type": "command", "command": "mk statusline"})));
        assert!(!statusline_is_ours(&json!({"type": "command", "command": "my-statusline"})));
        assert!(!statusline_is_ours(&json!({"type": "command", "command": "mk-other statusline"})));
        assert!(!statusline_is_ours(&json!("mk statusline")));
    }

    #[test]
    fn claude_install_sets_the_status_line_and_uninstall_removes_it() {
        let original = r#"{"model": "opus"}"#;
        let edited = super::claude_edit(original, true, MK, None).unwrap();
        let text = edited.text.unwrap();
        assert_eq!(statusline(&text), statusline_value(MK));
        assert_eq!(edited.saved, Saved::Keep, "nothing of the user's to save");
        assert_eq!(claude_status(&text, MK).unwrap(), Installed::Yes);
        assert_eq!(claude_edit(&text, true, MK).unwrap(), None, "idempotent");
        let back = super::claude_edit(&text, false, MK, None).unwrap();
        assert_eq!(json(&back.text.unwrap()), json(original));
        assert_eq!(back.saved, Saved::Keep);
    }

    #[test]
    fn claude_install_keeps_a_users_status_line_for_chaining() {
        let user = json!({"type": "command", "command": "~/.claude/line.sh", "padding": 0});
        let original = json!({"statusLine": user, "model": "opus"}).to_string();
        let edited = super::claude_edit(&original, true, MK, None).unwrap();
        assert_eq!(edited.saved, Saved::Write(user.clone()));
        let text = edited.text.unwrap();
        assert_eq!(statusline(&text), statusline_value(MK));
        let keys: Vec<_> = json(&text).as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["statusLine", "model", "hooks"], "the line keeps its place");

        // Uninstall puts it back and deletes the saved file.
        let back = super::claude_edit(&text, false, MK, Some(&user)).unwrap();
        assert_eq!(back.saved, Saved::Delete);
        assert_eq!(json(&back.text.unwrap()), json(&original));
    }

    #[test]
    fn an_older_mkrew_status_line_is_replaced_without_being_saved() {
        let old = json!({"statusLine": statusline_value(OLD_MK)}).to_string();
        assert_eq!(claude_status(&old, MK).unwrap(), Installed::Partly);
        let edited = super::claude_edit(&old, true, MK, None).unwrap();
        assert_eq!(edited.saved, Saved::Keep);
        assert_eq!(statusline(&edited.text.unwrap()), statusline_value(MK));
    }

    #[test]
    fn a_stale_saved_file_goes_when_there_is_no_status_line_to_chain() {
        let stale = json!({"type": "command", "command": "old.sh"});
        let edited = super::claude_edit("{}", true, MK, Some(&stale)).unwrap();
        assert_eq!(edited.saved, Saved::Delete);
    }

    #[test]
    fn hooks_without_the_status_line_are_a_partial_install() {
        let installed = claude_edit("{}", true, MK).unwrap().unwrap();
        let mut settings = json(&installed);
        settings.as_object_mut().unwrap().shift_remove("statusLine");
        assert_eq!(
            claude_status(&settings.to_string(), MK).unwrap(),
            Installed::Partly
        );
        // Installing again completes it.
        let again = claude_edit(&settings.to_string(), true, MK).unwrap().unwrap();
        assert_eq!(claude_status(&again, MK).unwrap(), Installed::Yes);
        // A status line alone is partial too.
        let only_line = json!({"statusLine": statusline_value(MK)}).to_string();
        assert_eq!(claude_status(&only_line, MK).unwrap(), Installed::Partly);
    }

    #[test]
    fn uninstall_leaves_a_status_line_the_user_set_since() {
        let user = json!({"type": "command", "command": "mine.sh"});
        let text = json!({"statusLine": user}).to_string();
        let saved = json!({"type": "command", "command": "older.sh"});
        let edited = super::claude_edit(&text, false, MK, Some(&saved)).unwrap();
        assert_eq!(edited.text, None);
        assert_eq!(edited.saved, Saved::Keep);
    }

    #[test]
    fn install_and_uninstall_write_the_saved_file_around_settings_json() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let saved = dir.path().join(SAVED_STATUSLINE);
        let user = json!({"type": "command", "command": "line.sh"});
        fs::write(&file, json!({"statusLine": user}).to_string()).unwrap();

        run_on(Action::Install, Agent::Claude, &file, MK).unwrap();
        assert_eq!(json(&fs::read_to_string(&saved).unwrap()), user);
        assert_eq!(statusline(&fs::read_to_string(&file).unwrap()), statusline_value(MK));
        // Installing again does not overwrite the saved line with Mkrew's own.
        run_on(Action::Install, Agent::Claude, &file, MK).unwrap();
        assert_eq!(json(&fs::read_to_string(&saved).unwrap()), user);

        run_on(Action::Uninstall, Agent::Claude, &file, MK).unwrap();
        assert_eq!(statusline(&fs::read_to_string(&file).unwrap()), user);
        assert!(!saved.exists(), "the saved file is deleted");
    }

    #[test]
    fn an_unreadable_saved_file_is_no_chained_command() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let saved = dir.path().join(SAVED_STATUSLINE);
        run_on(Action::Install, Agent::Claude, &file, MK).unwrap();
        fs::write(&saved, "{").unwrap();
        run_on(Action::Uninstall, Agent::Claude, &file, MK).unwrap();
        assert_eq!(json(&fs::read_to_string(&file).unwrap()), json("{}"));
        assert_eq!(fs::read_to_string(&saved).unwrap(), "{", "left alone");
    }
```

In `tests/cli.rs`, after `integration_install_status_uninstall` (line 885):

```rust
/// A status line of the user's own is saved before Mkrew's replaces it, and uninstall puts it back.
#[test]
fn integration_keeps_and_restores_a_users_status_line() {
    let env = Env::new();
    let claude = env.dir.path().join("claude");
    std::fs::create_dir_all(&claude).unwrap();
    let settings = claude.join("settings.json");
    let original = "{\n  \"statusLine\": {\n    \"type\": \"command\",\n    \"command\": \"line.sh\"\n  }\n}\n";
    std::fs::write(&settings, original).unwrap();
    let integration = |args: &[&str]| {
        env.command(args)
            .env("CLAUDE_CONFIG_DIR", &claude)
            .output()
            .unwrap()
    };
    let install = integration(&["integration", "install", "claude"]);
    assert!(install.status.success(), "{install:?}");
    let saved: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(claude.join("claude-statusline.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(saved["command"], "line.sh");
    let installed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
    assert!(
        installed["statusLine"]["command"]
            .as_str()
            .unwrap()
            .ends_with(" statusline"),
        "{installed}"
    );
    let status = integration(&["integration", "status", "claude"]);
    assert!(status.status.success(), "{status:?}");

    let uninstall = integration(&["integration", "uninstall", "claude"]);
    assert!(uninstall.status.success(), "{uninstall:?}");
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), original);
    assert!(!claude.join("claude-statusline.json").exists());
}
```

- [ ] **Step 2: Run to verify failure**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-cli integration`
Expected: FAIL to compile (`statusline_command`, `Saved`, `SAVED_STATUSLINE` missing; `claude_edit` takes 3 arguments).

- [ ] **Step 3: Implement**

`integration.rs`, after `CODEX_ARGS` (line 24):

```rust
/// Where install saves the user's own Claude `statusLine` (next to `settings.json`) so
/// `mk statusline` can run it and uninstall can put it back.
const SAVED_STATUSLINE: &str = "claude-statusline.json";
```

Replace the closure in `run` (lines 58-62) and add the shared helper:

```rust
fn env_dir(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// The file holding the user's own Claude status line, when Claude's config folder is known.
pub fn saved_statusline_file() -> Option<PathBuf> {
    let settings = config_file(Agent::Claude, env_dir).ok()?;
    Some(settings.with_file_name(SAVED_STATUSLINE))
}
```

with `run` starting `let file = config_file(agent, env_dir)?;`.

Replace `run_on` lines 128-155 (from `let install = ...` to the final `Ok(ExitCode::SUCCESS)`) with:

```rust
    let install = action == Action::Install;
    let saved_file = file.with_file_name(SAVED_STATUSLINE);
    let saved = match merged {
        Merged::Claude => read_saved(&saved_file),
        Merged::Codex => None,
    };
    let edited = match merged {
        Merged::Claude => claude_edit(original, install, mk, saved.as_ref()),
        Merged::Codex => codex_edit(original, install, mk).map(|text| Edited {
            text,
            saved: Saved::Keep,
        }),
    }
    .with_context(|| format!("update {}", file.display()))?;
    let verb = if install { "installed" } else { "uninstalled" };
    let Some(updated) = edited.text else {
        println!("{name}: already {verb} ({})", file.display());
        return Ok(ExitCode::SUCCESS);
    };
    let backup = match text {
        Some(_) => Some(back_up(file)?),
        None => None,
    };
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    // The user's own status line is on disk before settings.json stops naming it.
    if let Saved::Write(user) = &edited.saved {
        let text = serde_json::to_string_pretty(user).context("serialize the status line")?;
        fs::write(&saved_file, text + "\n")
            .with_context(|| format!("write {}", saved_file.display()))?;
    }
    fs::write(file, updated).with_context(|| format!("write {}", file.display()))?;
    if edited.saved == Saved::Delete
        && let Err(e) = fs::remove_file(&saved_file)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!("warning: cannot delete {}: {e}", saved_file.display());
    }
    match backup {
        Some(backup) => println!(
            "{name}: {verb} ({}; backup: {})",
            file.display(),
            backup.display()
        ),
        None => println!("{name}: {verb} ({})", file.display()),
    }
    Ok(ExitCode::SUCCESS)
}

/// The user's own `statusLine` saved by an earlier install. A missing file is "none"; an
/// unreadable one is "none" with a warning, so uninstall still removes Mkrew's line.
fn read_saved(file: &Path) -> Option<Value> {
    match fs::read_to_string(file) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(value) => Some(value),
            Err(e) => {
                eprintln!("warning: {} is not valid JSON ({e}); ignoring it", file.display());
                None
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            eprintln!("warning: cannot read {} ({e}); ignoring it", file.display());
            None
        }
    }
}
```

Replace `claude_status` (lines 329-354) with:

```rust
/// `Yes` when every event runs `mk` and the status line is Mkrew's; `Partly` when something
/// of Mkrew's is there but not all of it (older path or form, or the hooks without the line).
fn claude_status(text: &str, mk: &str) -> anyhow::Result<Installed> {
    let settings = parse_settings(text)?;
    let count = |pred: &dyn Fn(&Value, &str) -> bool| {
        CLAUDE_EVENTS
            .iter()
            .filter(|event| {
                settings
                    .get("hooks")
                    .and_then(|hooks| hooks[**event].as_array())
                    .is_some_and(|groups| {
                        groups
                            .iter()
                            .any(|group| group_runs(group, |hook| pred(hook, event)))
                    })
            })
            .count()
    };
    let current = count(&|hook, event| *hook == claude_hook(mk, event));
    let ours = count(&|hook, event| claude_hook_is_ours(hook, event));
    let line = settings.get("statusLine");
    let line_current = line == Some(&statusline_value(mk));
    let line_ours = line.is_some_and(statusline_is_ours);
    Ok(match (current, ours) {
        (n, _) if n == CLAUDE_EVENTS.len() && line_current => Installed::Yes,
        (_, 0) if !line_ours => Installed::No,
        _ => Installed::Partly,
    })
}

/// Claude Code's `statusLine` as Mkrew sets it. Claude runs `command` through its shell, so
/// the path is quoted (it may hold spaces).
fn statusline_value(mk: &str) -> Value {
    json!({"type": "command", "command": statusline_command(mk)})
}

fn statusline_command(mk: &str) -> String {
    format!("\"{mk}\" statusline")
}

/// Whether a `statusLine` value runs some `mk statusline` (any folder, quoted or bare).
fn statusline_is_ours(value: &Value) -> bool {
    let Some(command) = value["command"].as_str() else {
        return false;
    };
    command
        .trim()
        .strip_suffix(" statusline")
        .is_some_and(|program| is_mk(program.trim().trim_matches('"')))
}

/// What an edit does to the saved status line file.
#[derive(Debug, PartialEq)]
enum Saved {
    Keep,
    /// The user's own `statusLine`, saved before Mkrew's replaces it.
    Write(Value),
    /// Uninstall put the saved line back (or install found nothing to chain).
    Delete,
}

/// The edited `settings.json` text (`None` = nothing changes) and what to do with the saved file.
#[derive(Debug, PartialEq)]
struct Edited {
    text: Option<String>,
    saved: Saved,
}

/// Sets Mkrew's `statusLine` (saving a user's own, through the returned [`Saved`]) or, on
/// uninstall, puts the saved one back (`saved` is the saved file's content). A `statusLine`
/// the user set after installing is left alone by uninstall.
fn statusline_edit(
    settings: &mut Map<String, Value>,
    install: bool,
    mk: &str,
    saved: Option<&Value>,
) -> Saved {
    let current = settings.get("statusLine").cloned();
    let ours = current.as_ref().is_some_and(statusline_is_ours);
    if install {
        if current.as_ref() == Some(&statusline_value(mk)) {
            return Saved::Keep;
        }
        settings.insert("statusLine".into(), statusline_value(mk));
        return match current {
            Some(user) if !ours => Saved::Write(user),
            None if saved.is_some() => Saved::Delete,
            _ => Saved::Keep,
        };
    }
    if !ours {
        return Saved::Keep;
    }
    match saved {
        Some(user) => {
            settings.insert("statusLine".into(), user.clone());
            Saved::Delete
        }
        None => {
            // `shift_remove`: `remove` swaps the last key into the gap.
            settings.shift_remove("statusLine");
            Saved::Keep
        }
    }
}
```

`claude_edit` (lines 356-408): change the signature and the tail:

```rust
/// The new `settings.json` and what to do with the saved status line, or no text if nothing
/// changes.
fn claude_edit(
    text: &str,
    install: bool,
    mk: &str,
    saved: Option<&Value>,
) -> anyhow::Result<Edited> {
```

keep the body unchanged down to the `if hooks.is_empty() ... shift_remove("hooks") }` block, then replace from `let after = Value::Object(settings);` to the end with:

```rust
    let saved = statusline_edit(&mut settings, install, mk, saved);
    let after = Value::Object(settings);
    if after == before {
        return Ok(Edited {
            text: None,
            saved: Saved::Keep,
        });
    }
    let mut out = serde_json::to_string_pretty(&after).context("serialize settings")?;
    out.push('\n');
    Ok(Edited {
        text: Some(out),
        saved,
    })
}
```

- [ ] **Step 4: Run to verify pass**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-cli integration` and `CARGO_BUILD_JOBS=4 cargo test -p mkrew-cli --test cli integration`
Expected: PASS (the older hook tests keep passing through the shim; `integration_install_status_uninstall` still restores the original text exactly).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-cli/src/integration.rs crates/mkrew-cli/tests/cli.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(cli): mk integration sets Claude's status line and keeps the user's own"
```

---

### Task 7: `mk statusline`

**Files:**
- Create: `crates/mkrew-cli/src/statusline.rs`
- Modify: `crates/mkrew-cli/src/main.rs:3-12` (mod), `:91-98` (variant), `:290-303` (dispatch), `:419` (unreachable arm)
- Modify: `crates/mkrew-cli/tests/cli.rs` (one test)

**Interfaces:**
- Consumes: `AgentLimits`, `LimitWindow`, `Cli`, `Request::LimitsReport` (Task 1); `integration::saved_statusline_file()` (Task 6); `project::connect(&Paths)` (existing, `crates/mkrew-cli/src/project.rs:16`).
- Produces: `statusline::run() -> ExitCode` (always `SUCCESS`); `limits_of(input: &[u8], now: i64) -> Option<AgentLimits>`; `chain(command: &str, input: &[u8], timeout: Duration) -> Option<Vec<u8>>`. Claude's JSON fields (verified in Claude Code's status line docs): `rate_limits.five_hour.used_percentage`, `rate_limits.five_hour.resets_at` (Unix seconds), the same under `seven_day`.

- [ ] **Step 1: Write the failing tests**

`statusline.rs` tests (file created in step 3; its `mod tests` is part of it):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// A command that copies its stdin to stdout, in `cmd` and in `sh`.
    #[cfg(windows)]
    const ECHO_STDIN: &str = r#"findstr "^""#;
    #[cfg(not(windows))]
    const ECHO_STDIN: &str = "cat";
    /// Runs for about five seconds, in `cmd` and in `sh`.
    #[cfg(windows)]
    const SLEEP_5S: &str = "ping -n 6 127.0.0.1 >nul";
    #[cfg(not(windows))]
    const SLEEP_5S: &str = "sleep 5";

    const FULL: &str = r#"{"session_id":"s1","rate_limits":{"five_hour":{"used_percentage":42.5,"resets_at":1778533631},"seven_day":{"used_percentage":18,"resets_at":1778571719}}}"#;

    fn window(percent: f32, at: i64) -> Option<LimitWindow> {
        Some(LimitWindow {
            used_percent: percent,
            resets_at: Some(at),
        })
    }

    #[test]
    fn both_windows_are_read() {
        let limits = limits_of(FULL.as_bytes(), 77).unwrap();
        assert_eq!(
            limits,
            AgentLimits {
                agent: Cli::Claude,
                five_hour: window(42.5, 1_778_533_631),
                weekly: window(18.0, 1_778_571_719),
                observed_at: 77,
            }
        );
    }

    #[test]
    fn one_window_is_enough_and_a_missing_reset_is_kept_open() {
        let json = br#"{"rate_limits":{"seven_day":{"used_percentage":3.5}}}"#;
        let limits = limits_of(json, 1).unwrap();
        assert_eq!(limits.five_hour, None);
        assert_eq!(
            limits.weekly,
            Some(LimitWindow {
                used_percent: 3.5,
                resets_at: None
            })
        );
    }

    #[test]
    fn a_payload_without_limits_sends_nothing() {
        for input in [
            &b""[..],
            b"not json",
            b"{}",
            br#"{"session_id":"s1"}"#,
            br#"{"rate_limits":{}}"#,
            br#"{"rate_limits":{"five_hour":{"used_percentage":"high"}}}"#,
            br#"{"rate_limits":null}"#,
            b"[1,2]",
        ] {
            assert_eq!(limits_of(input, 1), None, "{}", String::from_utf8_lossy(input));
        }
    }

    #[test]
    fn the_saved_command_is_the_users_status_line_command() {
        let line = serde_json::json!({"type": "command", "command": "line.sh", "padding": 0});
        assert_eq!(saved_command(&line.to_string()), Some("line.sh".to_owned()));
        for other in [
            r#"{"type":"text","command":"x"}"#,
            r#"{"command":""}"#,
            r#"{"type":"command"}"#,
            r#""line.sh""#,
            "{",
            "",
        ] {
            assert_eq!(saved_command(other), None, "{other}");
        }
    }

    #[test]
    fn the_chained_command_gets_the_same_stdin_and_its_output_is_kept() {
        let out = chain(ECHO_STDIN, FULL.as_bytes(), Duration::from_secs(5)).unwrap();
        assert_eq!(String::from_utf8_lossy(&out).trim(), FULL);
    }

    #[test]
    fn a_command_that_ignores_its_stdin_still_answers() {
        let big = vec![b'x'; 1024 * 1024];
        let out = chain("echo hi", &big, Duration::from_secs(5)).unwrap();
        assert_eq!(String::from_utf8_lossy(&out).trim(), "hi");
    }

    #[test]
    fn a_failing_or_missing_command_prints_nothing() {
        assert_eq!(chain("exit 3", b"{}", Duration::from_secs(5)), None);
        assert_eq!(
            chain("mkrew-no-such-program-xyz", b"{}", Duration::from_secs(5)),
            None
        );
    }

    #[test]
    fn a_command_past_its_timeout_is_stopped() {
        let started = std::time::Instant::now();
        assert_eq!(chain(SLEEP_5S, b"{}", Duration::from_millis(300)), None);
        assert!(started.elapsed() < Duration::from_secs(3), "{:?}", started.elapsed());
    }
}
```

`tests/cli.rs` (end of file, using the existing `Env`):

```rust
/// A command that copies its stdin to stdout, in `cmd` and in `sh`.
#[cfg(windows)]
const ECHO_STDIN: &str = r#"findstr "^""#;
#[cfg(not(windows))]
const ECHO_STDIN: &str = "cat";

/// Claude Code runs `mk statusline` on every update: the user's own status line still prints
/// its text, with no daemon running (the report is dropped) and without one saved (nothing
/// printed), and the exit code is 0 either way.
#[test]
fn statusline_chains_the_users_status_line_and_never_fails() {
    let env = Env::new();
    let claude = env.dir.path().join("claude");
    std::fs::create_dir_all(&claude).unwrap();
    let payload = r#"{"session_id":"s1","rate_limits":{"five_hour":{"used_percentage":42.5,"resets_at":1778533631}}}"#;
    let run = || {
        let mut child = env
            .command(&["statusline"])
            .env("CLAUDE_CONFIG_DIR", &claude)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    };
    let none = run();
    assert!(none.status.success(), "{none:?}");
    assert!(none.stdout.is_empty(), "{none:?}");

    let saved = serde_json::json!({"type": "command", "command": ECHO_STDIN});
    std::fs::write(claude.join("claude-statusline.json"), saved.to_string()).unwrap();
    let chained = run();
    assert!(chained.status.success(), "{chained:?}");
    assert_eq!(String::from_utf8_lossy(&chained.stdout).trim(), payload);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-cli statusline`
Expected: FAIL (no such module / `mk statusline` is an unknown subcommand).

- [ ] **Step 3: Implement**

`crates/mkrew-cli/src/statusline.rs`:

```rust
//! `mk statusline` (spec 2026-11-22 section 5.3): Claude Code's status line command, set up by
//! `mk integration install claude`. Claude pipes a JSON document to it on every update. It sends
//! the account's rate limits to the daemon (`limits.report`, a failure is silent), then runs
//! the user's own status line, if they had one, with the same stdin and prints its output.
//! Nothing here may fail or slow Claude down: every error means "nothing", the exit code is 0.

use std::io::{Read, Write};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mkrew_core::Paths;
use mkrew_protocol::{AgentLimits, Cli, LimitWindow, Request};
use serde_json::Value;

/// Claude's document is a few KB; anything past this is not read.
const STDIN_CAP: u64 = 1024 * 1024;
/// Connect and send, together.
const REPORT_BUDGET: Duration = Duration::from_millis(300);
/// The user's own status line gets this long.
const CHAIN_TIMEOUT: Duration = Duration::from_secs(5);

pub fn run() -> ExitCode {
    let mut input = Vec::new();
    // A read error keeps what came before it.
    let _ = std::io::stdin().take(STDIN_CAP).read_to_end(&mut input);
    if let Some(limits) = limits_of(&input, now_secs()) {
        report(limits);
    }
    let saved = crate::integration::saved_statusline_file()
        .and_then(|file| std::fs::read_to_string(file).ok());
    if let Some(command) = saved.as_deref().and_then(saved_command)
        && let Some(output) = chain(&command, &input, CHAIN_TIMEOUT)
    {
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(&output);
        let _ = stdout.flush();
    }
    ExitCode::SUCCESS
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// The limits in Claude's status line document, `None` when it has none (API-key users and
/// older Claude Code send no `rate_limits`), is not JSON, or has no usable window.
fn limits_of(input: &[u8], now: i64) -> Option<AgentLimits> {
    let json: Value = serde_json::from_slice(input).ok()?;
    let rate = json.get("rate_limits")?;
    let five_hour = window(&rate["five_hour"]);
    let weekly = window(&rate["seven_day"]);
    (five_hour.is_some() || weekly.is_some()).then_some(AgentLimits {
        agent: Cli::Claude,
        five_hour,
        weekly,
        observed_at: now,
    })
}

fn window(value: &Value) -> Option<LimitWindow> {
    let used = value["used_percentage"].as_f64().filter(|u| u.is_finite())?;
    Some(LimitWindow {
        used_percent: used as f32,
        resets_at: value["resets_at"].as_f64().map(|at| at as i64),
    })
}

/// Sends `limits` to the daemon the way `mk hook` reaches it (the local socket), within
/// [`REPORT_BUDGET`]; without a daemon, or too slowly, the report is dropped.
fn report(limits: AgentLimits) {
    let Ok(paths) = Paths::from_env() else {
        return;
    };
    let (done, wait) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let sent = crate::project::connect(&paths)
            .and_then(|mut client| client.call::<()>(Request::LimitsReport(limits)));
        let _ = done.send(sent.is_ok());
    });
    let _ = wait.recv_timeout(REPORT_BUDGET);
}

/// The `command` of the user's saved `statusLine`; `None` for anything that is not a command
/// status line (an unreadable saved file is "no chained command", spec section 8).
fn saved_command(text: &str) -> Option<String> {
    let line: Value = serde_json::from_str(text).ok()?;
    if line["type"] != "command" {
        return None;
    }
    let command = line["command"].as_str()?.trim();
    (!command.is_empty()).then(|| command.to_owned())
}

#[cfg(windows)]
fn shell(command: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut shell = Command::new("cmd");
    // The outer quotes are cmd's: it strips the first and last one.
    shell.arg("/C").raw_arg(format!("\"{command}\""));
    shell
}

#[cfg(not(windows))]
fn shell(command: &str) -> Command {
    let mut shell = Command::new("sh");
    shell.arg("-c").arg(command);
    shell
}

/// Runs `command` through the shell with `input` on stdin and returns its stdout; `None` when
/// it cannot start, exits with an error, or is not done within `timeout` (it is killed then).
fn chain(command: &str, input: &[u8], timeout: Duration) -> Option<Vec<u8>> {
    let deadline = Instant::now() + timeout;
    let mut child = shell(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    let mut stdout = child.stdout.take()?;
    let input = input.to_vec();
    // A command that never reads its stdin closes the pipe under the write: not an error.
    std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let (done, output) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.read_to_end(&mut out);
        let _ = done.send(out);
    });
    let out = output.recv_timeout(timeout).ok();
    loop {
        match (&out, child.try_wait()) {
            (Some(out), Ok(Some(status))) => return status.success().then(|| out.clone()),
            (Some(_), Ok(None)) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}
```

(then the `#[cfg(test)] mod tests` from step 1 at the bottom).

`main.rs`: add `mod statusline;` (after `mod session;`); add to `enum Command` after `Hook` (line 98):

```rust
    /// Claude Code's status line command (set up by `mk integration install claude`): reports
    /// the rate limits to the daemon, then runs your own status line, if you had one. Not for
    /// interactive use.
    Statusline,
```

In `run`, right after the `Command::Hook` branch (line 294):

```rust
    // Claude Code runs this on every status line update: nothing else may be set up and
    // nothing may fail.
    if matches!(cli.command, Command::Statusline) {
        return Ok(statusline::run());
    }
```

and the last arm (line 419) becomes `Command::Hook { .. } | Command::Integration { .. } | Command::Statusline => unreachable!("handled above"),` (format as rustfmt wants).

- [ ] **Step 4: Run to verify pass**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-cli statusline` and `CARGO_BUILD_JOBS=4 cargo test -p mkrew-cli --test cli statusline`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-cli
git -c user.email=tungnv975@gmail.com commit -m "feat(cli): mk statusline reports Claude's limits and chains the user's status line"
```

---

### Task 8: UI model `mkrew_ui::widget_bar` (pure widget list)

**Files:**
- Create: `crates/mkrew-ui/src/widget_bar.rs`
- Modify: `crates/mkrew-ui/src/lib.rs:104` area (add `pub mod widget_bar;` in alphabetical order, after `pub mod watches;`)
- Modify: `crates/mkrew-ui/Cargo.toml` (add `chrono`, below)

**Interfaces:**
- Consumes: `AgentLimits`, `LimitWindow`, `MachineStats`, `Cli`, `Event::{AgentLimits, MachineStats}`, `AgentLimits::supersedes` (Task 1); `GitSummary::{branch_label, branch_tooltip, changed}` (`crates/mkrew-ui/src/git_summary.rs:96-123`).
- Produces (used by Tasks 10-12):
  - `WidgetId::{Folder, Git, Limits, Machine, Version}` with `ALL` (bar order), `id() -> &'static str` (`"folder"`, `"git"`, `"limits"`, `"machine"`, `"version"`), `title() -> &'static str`, `side() -> Side`
  - `Side::{Left, Right}`, `Level::{Muted, Warn, Danger}` (`Level::of(percent: u32)`), `Click::{OpenFolder, Git, OpenLimits, CheckUpdate, None}`
  - `Part { text: String, level: Level }`, `Widget { id, parts: Vec<Part>, tooltip: String, click: Click }` with `label() -> String`
  - `WidgetData` (`Default`; `apply(&Event) -> bool`, `load_limits(Vec<AgentLimits>) -> bool`, `clear()`, `limits() -> &[AgentLimits]`, `machine() -> Option<&MachineStats>`)
  - `Input<'a> { workspace: Option<(&str, &str)>, git: Option<&GitSummary>, data: &WidgetData, machine_name: &str, version: &str, hidden: &[String], now_secs: i64 }`
  - `widget_list(&Input<'_>, utc_offset: impl Fn(i64) -> i32) -> Vec<Widget>`, `fit(Vec<Widget>, available_px: f32, em_px: f32) -> Vec<Widget>`, `list_width(&[Widget], em_px: f32) -> f32`
  - `toggled(hidden: &[String], WidgetId) -> Vec<String>`, `is_hidden(hidden: &[String], WidgetId) -> bool`, `in_text(secs: i64) -> String`, `local_clock(at: i64, now: i64, utc_offset: i32) -> String`

- [ ] **Step 1: Write the tests (inside the new file) and the failing build**

Add `pub mod widget_bar;` to `lib.rs` and `chrono = { version = "0.4", default-features = false, features = ["clock"] }` to `crates/mkrew-ui/Cargo.toml` `[dependencies]` with the comment `# The widget bar's reset times in local time (Task 11); already in the tree through gpui.` (no code uses it yet in this task; it is used by Task 11). Create `widget_bar.rs` with only this test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_summary::RepoSummary;

    const NOW: i64 = 1_778_519_153;
    const GIB: f64 = 1_073_741_824.0;

    fn plus_seven(_: i64) -> i32 {
        25_200
    }

    fn repo(branch: Option<&str>, modified: usize, untracked: usize) -> RepoSummary {
        RepoSummary {
            repo: String::new(),
            branch: branch.map(str::to_owned),
            head: Some("1a2b3c4d5e6f".into()),
            modified,
            untracked,
        }
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

    fn stats(cpu: f32, used: f64, total: f64) -> MachineStats {
        MachineStats {
            cpu_percent: cpu,
            ram_used: (used * GIB) as u64,
            ram_total: (total * GIB) as u64,
        }
    }

    fn data(list: Vec<AgentLimits>, machine: Option<MachineStats>) -> WidgetData {
        let mut data = WidgetData::default();
        data.load_limits(list);
        if let Some(machine) = machine {
            data.apply(&Event::MachineStats(machine));
        }
        data
    }

    fn list(
        git: Option<&GitSummary>,
        data: &WidgetData,
        hidden: &[String],
        workspace: Option<(&str, &str)>,
    ) -> Vec<Widget> {
        widget_list(
            &Input {
                workspace,
                git,
                data,
                machine_name: "This computer",
                version: "0.5.3",
                hidden,
                now_secs: NOW,
            },
            plus_seven,
        )
    }

    fn ids(list: &[Widget]) -> Vec<WidgetId> {
        list.iter().map(|w| w.id).collect()
    }

    fn full() -> Vec<Widget> {
        let git = GitSummary {
            repos: vec![repo(Some("master"), 2, 1)],
        };
        let data = data(
            vec![
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
            ],
            Some(stats(23.4, 11.2, 31.8)),
        );
        list(Some(&git), &data, &[], Some(("api", "C:/work/api")))
    }

    #[test]
    fn without_data_only_the_version_shows() {
        let none = list(None, &WidgetData::default(), &[], None);
        assert_eq!(ids(&none), [WidgetId::Version]);
        assert_eq!(none[0].label(), "v0.5.3");
    }

    #[test]
    fn a_full_state_lists_the_widgets_in_bar_order() {
        let list = full();
        assert_eq!(ids(&list), WidgetId::ALL);
        let labels: Vec<String> = list.iter().map(Widget::label).collect();
        assert_eq!(
            labels,
            [
                "api",
                "master · 3",
                "Claude 5h 42% · 7d 18% | Codex 5h 10% · 7d 3%",
                "CPU 23% · RAM 11.2/31.8 GB",
                "v0.5.3"
            ]
        );
        let clicks: Vec<Click> = list.iter().map(|w| w.click).collect();
        assert_eq!(
            clicks,
            [
                Click::OpenFolder,
                Click::Git,
                Click::OpenLimits,
                Click::None,
                Click::CheckUpdate
            ]
        );
        assert_eq!(WidgetId::Folder.side(), Side::Left);
        assert_eq!(WidgetId::Git.side(), Side::Left);
        assert_eq!(WidgetId::Limits.side(), Side::Right);
    }

    #[test]
    fn tooltips_carry_the_full_value_and_the_click() {
        let list = full();
        assert_eq!(
            list[0].tooltip,
            "C:/work/api\nClick: open the folder in the file explorer"
        );
        assert!(list[1].tooltip.starts_with("(root) - master · 1a2b3c4\n"), "{}", list[1].tooltip);
        assert!(list[1].tooltip.ends_with("Click: open Git, again for the next changed file"));
        let limits = &list[2].tooltip;
        assert!(
            limits.contains("Claude 5-hour limit: 42% used, resets 02:15 (in 2 h 10 min)"),
            "{limits}"
        );
        assert!(
            limits.contains("Claude weekly limit: 18% used, resets Fri 00:05 (in 3 d)"),
            "{limits}"
        );
        assert!(limits.ends_with("Click: open Settings"), "{limits}");
        assert_eq!(
            list[3].tooltip,
            "This computer\nCPU 23% of all cores\nRAM 11.2 of 31.8 GB used"
        );
        assert_eq!(list[4].tooltip, "Mkrew 0.5.3\nClick: check for updates");
    }

    #[test]
    fn the_git_widget_counts_changes_and_names_several_repositories() {
        let one = |summary: GitSummary| {
            list(Some(&summary), &WidgetData::default(), &[], None)
                .iter()
                .find(|w| w.id == WidgetId::Git)
                .map(Widget::label)
        };
        assert_eq!(one(GitSummary { repos: vec![repo(Some("master"), 0, 0)] }).as_deref(), Some("master"));
        assert_eq!(one(GitSummary { repos: vec![repo(None, 1, 0)] }).as_deref(), Some("detached · 1"));
        assert_eq!(
            one(GitSummary { repos: vec![repo(Some("a"), 1, 0), repo(Some("b"), 0, 2)] }).as_deref(),
            Some("2 repos · 3")
        );
        assert_eq!(one(GitSummary::default()), None, "no repository, no widget");
    }

    #[test]
    fn limit_colors_follow_the_shown_percent() {
        let level_of = |percent: f32| {
            let data = data(vec![limits(Cli::Claude, window(percent, Some(600)), None)], None);
            let widgets = list(None, &data, &[], None);
            let widget = widgets.iter().find(|w| w.id == WidgetId::Limits).unwrap();
            widget.parts.iter().find(|p| p.text.starts_with("5h")).unwrap().clone()
        };
        for (percent, text, level) in [
            (79.4, "5h 79%", Level::Muted),
            (79.5, "5h 80%", Level::Warn),
            (94.4, "5h 94%", Level::Warn),
            (94.5, "5h 95%", Level::Danger),
            (100.0, "5h 100%", Level::Danger),
            (150.0, "5h 100%", Level::Danger),
            (-5.0, "5h 0%", Level::Muted),
        ] {
            let part = level_of(percent);
            assert_eq!((part.text.as_str(), part.level), (text, level), "{percent}");
        }
    }

    #[test]
    fn a_window_past_its_reset_shows_nothing_until_a_new_report() {
        let label = |five: Option<LimitWindow>, weekly: Option<LimitWindow>| {
            let data = data(vec![limits(Cli::Claude, five, weekly)], None);
            list(None, &data, &[], None)
                .iter()
                .find(|w| w.id == WidgetId::Limits)
                .map(Widget::label)
        };
        assert_eq!(label(window(50.0, Some(-1)), window(18.0, Some(60))).as_deref(), Some("Claude 7d 18%"));
        assert_eq!(label(window(50.0, Some(0)), window(18.0, Some(60))).as_deref(), Some("Claude 7d 18%"), "now is past");
        assert_eq!(label(window(50.0, Some(-1)), window(18.0, Some(-5))), None, "both past: absent");
        assert_eq!(label(window(50.0, None), None).as_deref(), Some("Claude 5h 50%"), "no reset time: kept");
        assert_eq!(label(None, None), None);
    }

    #[test]
    fn only_claude_and_codex_are_listed() {
        let data = data(vec![limits(Cli::Omp, window(50.0, Some(60)), None)], None);
        assert_eq!(ids(&list(None, &data, &[], None)), [WidgetId::Version]);
        let data = self::data(vec![limits(Cli::Codex, window(50.0, Some(60)), None)], None);
        let widgets = list(None, &data, &[], None);
        assert_eq!(widgets[0].label(), "Codex 5h 50%");
    }

    #[test]
    fn ram_is_left_out_when_the_os_reports_no_total() {
        let no_total = MachineStats {
            cpu_percent: 23.4,
            ram_used: 5,
            ram_total: 0,
        };
        let data = data(vec![], Some(no_total));
        let widgets = list(None, &data, &[], None);
        assert_eq!(widgets[0].label(), "CPU 23%");
        assert_eq!(widgets[0].tooltip, "This computer\nCPU 23% of all cores");
    }

    #[test]
    fn hidden_widgets_are_left_out_and_unknown_ids_are_ignored() {
        let hidden = vec!["git".to_owned(), "clock".to_owned(), "limits".to_owned()];
        let widgets = {
            let git = GitSummary { repos: vec![repo(Some("m"), 0, 0)] };
            let data = data(
                vec![limits(Cli::Claude, window(1.0, Some(60)), None)],
                Some(stats(1.0, 1.0, 2.0)),
            );
            list(Some(&git), &data, &hidden, Some(("api", "/api")))
        };
        assert_eq!(
            ids(&widgets),
            [WidgetId::Folder, WidgetId::Machine, WidgetId::Version]
        );
        assert!(is_hidden(&hidden, WidgetId::Git));
        assert!(!is_hidden(&hidden, WidgetId::Folder));
    }

    #[test]
    fn toggling_keeps_ids_this_build_does_not_know_in_order() {
        let hidden = vec!["clock".to_owned(), "git".to_owned()];
        assert_eq!(toggled(&hidden, WidgetId::Git), ["clock"]);
        assert_eq!(toggled(&hidden, WidgetId::Limits), ["clock", "git", "limits"]);
        assert_eq!(toggled(&toggled(&hidden, WidgetId::Limits), WidgetId::Limits), hidden);
        assert_eq!(toggled(&[], WidgetId::Machine), ["machine"]);
    }

    #[test]
    fn a_narrow_window_drops_limits_first_then_the_machine() {
        let list = full();
        let em = 13.0;
        let all = list_width(&list, em);
        assert_eq!(fit(list.clone(), f32::MAX, em), list);
        let without = |drop: &[WidgetId]| -> Vec<Widget> {
            list.iter().filter(|w| !drop.contains(&w.id)).cloned().collect()
        };
        let no_limits = without(&[WidgetId::Limits]);
        let no_machine_either = without(&[WidgetId::Limits, WidgetId::Machine]);
        assert!(list_width(&no_limits, em) < all);
        assert_eq!(fit(list.clone(), all - 1.0, em), no_limits);
        assert_eq!(
            fit(list.clone(), list_width(&no_limits, em) - 1.0, em),
            no_machine_either
        );
        // Folder, git and version stay however narrow.
        assert_eq!(
            ids(&fit(list, 0.0, em)),
            [WidgetId::Folder, WidgetId::Git, WidgetId::Version]
        );
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
            (190_800, "in 2 d 4 h"),
            (259_200, "in 3 d"),
        ] {
            assert_eq!(in_text(secs), text, "{secs}");
        }
    }

    #[test]
    fn reset_times_are_local_and_name_the_day_when_it_is_not_today() {
        // 2026-05-12 00:05 local (UTC+7) now; the 5-hour window resets 04:07 the same day.
        assert_eq!(local_clock(1_778_533_631, NOW, 25_200), "04:07");
        // Three days on: Friday.
        assert_eq!(local_clock(1_778_571_719 + 3 * 86_400, NOW, 25_200), "Fri 14:41");
        // The same instant in UTC is still the previous day for `now`.
        assert_eq!(local_clock(1_778_533_631, NOW, 0), "21:07");
        assert_eq!(local_clock(0, 0, 0), "00:00");
    }

    #[test]
    fn widget_data_keeps_the_newest_limits_and_reports_changes() {
        let mut data = WidgetData::default();
        let old = limits(Cli::Claude, window(10.0, Some(60)), None);
        let newer = AgentLimits {
            observed_at: NOW + 1,
            ..limits(Cli::Claude, window(20.0, Some(60)), None)
        };
        assert!(data.apply(&Event::AgentLimits(old.clone())));
        assert!(!data.apply(&Event::AgentLimits(old.clone())), "a repeat");
        assert!(data.apply(&Event::AgentLimits(newer.clone())));
        assert!(!data.apply(&Event::AgentLimits(old.clone())), "an older event");
        assert!(!data.load_limits(vec![old]), "a late answer never undoes a newer event");
        assert_eq!(data.limits(), [newer]);

        let one = stats(1.0, 1.0, 2.0);
        assert!(data.apply(&Event::MachineStats(one)));
        assert!(!data.apply(&Event::MachineStats(one)));
        assert!(data.apply(&Event::MachineStats(stats(2.0, 1.0, 2.0))));
        assert!(!data.apply(&Event::SessionState(mkrew_protocol::SessionStateEvent {
            session_id: ulid::Ulid::nil(),
            state: mkrew_protocol::SessionState::Idle,
            source: mkrew_protocol::StateSource::Daemon,
            changed_at: 0,
        })));
        data.clear();
        assert_eq!((data.limits().len(), data.machine()), (0, None));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui widget_bar`
Expected: FAIL to compile (`WidgetData`, `widget_list`, ... not defined).

- [ ] **Step 3: Implement**

Put this above the test module in `widget_bar.rs`:

```rust
//! The widget bar's model (spec 2026-11-22, ADR 0102): which widgets show, their text and
//! colors, which drop when the window is narrow and how a hide toggle edits `widgets_hidden`.
//! Pure and tested headlessly; `shell/widget_bar.rs` draws it.

use mkrew_protocol::{AgentLimits, Cli, Event, LimitWindow, MachineStats};

use crate::git_summary::GitSummary;

/// The five widgets of the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WidgetId {
    Folder,
    Git,
    Limits,
    Machine,
    Version,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

impl WidgetId {
    /// Bar order: folder and git on the left, limits, machine and version on the right.
    pub const ALL: [WidgetId; 5] = [
        WidgetId::Folder,
        WidgetId::Git,
        WidgetId::Limits,
        WidgetId::Machine,
        WidgetId::Version,
    ];

    /// The id kept in `[ui] widgets_hidden`.
    pub fn id(self) -> &'static str {
        match self {
            WidgetId::Folder => "folder",
            WidgetId::Git => "git",
            WidgetId::Limits => "limits",
            WidgetId::Machine => "machine",
            WidgetId::Version => "version",
        }
    }

    /// The name in the bar's right-click menu.
    pub fn title(self) -> &'static str {
        match self {
            WidgetId::Folder => "Folder",
            WidgetId::Git => "Git branch",
            WidgetId::Limits => "Agent limits",
            WidgetId::Machine => "CPU and RAM",
            WidgetId::Version => "Version",
        }
    }

    pub fn side(self) -> Side {
        match self {
            WidgetId::Folder | WidgetId::Git => Side::Left,
            WidgetId::Limits | WidgetId::Machine | WidgetId::Version => Side::Right,
        }
    }
}

/// How loud a part of a label is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Muted text.
    Muted,
    /// `theme.status_needs_input` (amber): a limit at 80% or more.
    Warn,
    /// `theme.danger_text`: a limit at 95% or more.
    Danger,
}

impl Level {
    /// `percent` is the shown, rounded value, so 79.5 reads "80%" and is amber.
    pub fn of(percent: u32) -> Self {
        match percent {
            95.. => Level::Danger,
            80..=94 => Level::Warn,
            _ => Level::Muted,
        }
    }
}

/// What a click on a widget does; the shell runs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    /// The workspace folder in the file explorer.
    OpenFolder,
    /// Opens Git; with Git already open, the next changed file (wrapping).
    Git,
    /// Settings at the Integrations pages.
    OpenLimits,
    /// The update check.
    CheckUpdate,
    /// No click, no pointer (the machine widget).
    None,
}

/// A run of label text in one color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub text: String,
    pub level: Level,
}

impl Part {
    fn muted(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            level: Level::Muted,
        }
    }
}

/// One widget to draw: its label (after the icon of its id), tooltip and click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Widget {
    pub id: WidgetId,
    pub parts: Vec<Part>,
    pub tooltip: String,
    pub click: Click,
}

impl Widget {
    /// The label as one line (parts joined by a space).
    pub fn label(&self) -> String {
        self.parts
            .iter()
            .map(|part| part.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// What the daemon told the bar: the newest limits per agent and the machine's load.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WidgetData {
    limits: Vec<AgentLimits>,
    machine: Option<MachineStats>,
}

impl WidgetData {
    /// Takes in `agent.limits` and `machine.stats`; returns whether anything changed.
    pub fn apply(&mut self, event: &Event) -> bool {
        match event {
            Event::AgentLimits(limits) => self.offer(limits.clone()),
            Event::MachineStats(stats) => self.machine.replace(*stats) != Some(*stats),
            _ => false,
        }
    }

    /// The answer to `limits.get`, merged with the rule the daemon uses, so it never undoes a
    /// newer event that arrived first.
    pub fn load_limits(&mut self, listed: Vec<AgentLimits>) -> bool {
        listed
            .into_iter()
            .fold(false, |changed, limits| self.offer(limits) | changed)
    }

    /// A lost connection: the next daemon has its own numbers.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn limits(&self) -> &[AgentLimits] {
        &self.limits
    }

    pub fn machine(&self) -> Option<&MachineStats> {
        self.machine.as_ref()
    }

    fn offer(&mut self, new: AgentLimits) -> bool {
        match self.limits.iter_mut().find(|old| old.agent == new.agent) {
            Some(old) if new.supersedes(old) => {
                *old = new;
                true
            }
            Some(_) => false,
            None => {
                self.limits.push(new);
                true
            }
        }
    }
}

/// Everything [`widget_list`] reads.
pub struct Input<'a> {
    /// The shown workspace's name and root folder.
    pub workspace: Option<(&'a str, &'a str)>,
    /// Its Git summary (already polled for the title bar).
    pub git: Option<&'a GitSummary>,
    pub data: &'a WidgetData,
    /// "This computer", or the machine window's name.
    pub machine_name: &'a str,
    /// `env!("CARGO_PKG_VERSION")`.
    pub version: &'a str,
    /// `[ui] widgets_hidden`; unknown ids are ignored.
    pub hidden: &'a [String],
    pub now_secs: i64,
}

/// The widgets to draw, in bar order. A widget with nothing to show is absent, never "0%".
/// `utc_offset(at)` is the local offset in seconds at the Unix time `at` (for reset times).
pub fn widget_list(input: &Input<'_>, utc_offset: impl Fn(i64) -> i32) -> Vec<Widget> {
    WidgetId::ALL
        .into_iter()
        .filter(|id| !is_hidden(input.hidden, *id))
        .filter_map(|id| match id {
            WidgetId::Folder => folder(input),
            WidgetId::Git => git(input),
            WidgetId::Limits => limits(input, &utc_offset),
            WidgetId::Machine => machine(input),
            WidgetId::Version => Some(version(input)),
        })
        .collect()
}

fn folder(input: &Input<'_>) -> Option<Widget> {
    let (name, path) = input.workspace?;
    Some(Widget {
        id: WidgetId::Folder,
        parts: vec![Part::muted(name)],
        tooltip: format!("{path}\nClick: open the folder in the file explorer"),
        click: Click::OpenFolder,
    })
}

fn git(input: &Input<'_>) -> Option<Widget> {
    let summary = input.git?;
    let branch = summary.branch_label()?;
    let changed = summary.changed();
    let label = if changed > 0 {
        format!("{branch} · {changed}")
    } else {
        branch
    };
    let tooltip = summary.branch_tooltip().unwrap_or_default();
    Some(Widget {
        id: WidgetId::Git,
        parts: vec![Part::muted(label)],
        tooltip: format!("{tooltip}\nClick: open Git, again for the next changed file"),
        click: Click::Git,
    })
}

/// `Claude 5h 42% · 7d 18% | Codex 5h 10% · 7d 3%`: one group per agent that has a live window.
fn limits(input: &Input<'_>, utc_offset: &dyn Fn(i64) -> i32) -> Option<Widget> {
    let mut parts: Vec<Part> = Vec::new();
    let mut tips: Vec<String> = Vec::new();
    for (agent, name) in [(Cli::Claude, "Claude"), (Cli::Codex, "Codex")] {
        let Some(limits) = input.data.limits().iter().find(|l| l.agent == agent) else {
            continue;
        };
        let windows: Vec<(&str, &str, &LimitWindow)> = [
            ("5h", "5-hour limit", limits.five_hour.as_ref()),
            ("7d", "weekly limit", limits.weekly.as_ref()),
        ]
        .into_iter()
        .filter_map(|(short, long, window)| Some((short, long, window?)))
        // Past its reset the number is stale: nothing until a new report.
        .filter(|(_, _, w)| {
            w.used_percent.is_finite() && w.resets_at.is_none_or(|at| at > input.now_secs)
        })
        .collect();
        if windows.is_empty() {
            continue;
        }
        if !parts.is_empty() {
            parts.push(Part::muted("|"));
        }
        parts.push(Part::muted(name));
        for (at, (short, long, window)) in windows.into_iter().enumerate() {
            if at > 0 {
                parts.push(Part::muted("·"));
            }
            let percent = percent_of(window);
            parts.push(Part {
                text: format!("{short} {percent}%"),
                level: Level::of(percent),
            });
            tips.push(match window.resets_at {
                Some(resets) => format!(
                    "{name} {long}: {percent}% used, resets {} ({})",
                    local_clock(resets, input.now_secs, utc_offset(resets)),
                    in_text(resets - input.now_secs)
                ),
                None => format!("{name} {long}: {percent}% used"),
            });
        }
    }
    if parts.is_empty() {
        return None;
    }
    tips.push("Click: open Settings".to_owned());
    Some(Widget {
        id: WidgetId::Limits,
        parts,
        tooltip: tips.join("\n"),
        click: Click::OpenLimits,
    })
}

/// The shown whole percentage: clamped to 0-100 and rounded.
fn percent_of(window: &LimitWindow) -> u32 {
    window.used_percent.clamp(0.0, 100.0).round() as u32
}

fn machine(input: &Input<'_>) -> Option<Widget> {
    let stats = input.data.machine()?;
    let cpu = stats.cpu_percent.clamp(0.0, 100.0).round();
    let mut label = format!("CPU {cpu:.0}%");
    let mut tooltip = format!("{}\nCPU {cpu:.0}% of all cores", input.machine_name);
    // No total from the OS: the RAM part is left out.
    if stats.ram_total > 0 {
        let (used, total) = (gib(stats.ram_used), gib(stats.ram_total));
        label.push_str(&format!(" · RAM {used}/{total} GB"));
        tooltip.push_str(&format!("\nRAM {used} of {total} GB used"));
    }
    Some(Widget {
        id: WidgetId::Machine,
        parts: vec![Part::muted(label)],
        tooltip,
        click: Click::None,
    })
}

/// One decimal, in 1024-based units (what Task Manager calls GB).
fn gib(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1_073_741_824.0)
}

fn version(input: &Input<'_>) -> Widget {
    Widget {
        id: WidgetId::Version,
        parts: vec![Part::muted(format!("v{}", input.version))],
        tooltip: format!("Mkrew {}\nClick: check for updates", input.version),
        click: Click::CheckUpdate,
    }
}

// ponytail: widths are estimated from the character count, not measured; the bar drops whole
// widgets, so a few pixels of error only move the moment a widget drops. Measure with the
// text system if a font makes it visibly early or late.
const CHAR_EM: f32 = 0.55;
const ICON_EM: f32 = 1.6;
const WIDGET_PAD_EM: f32 = 0.6;
const GAP_EM: f32 = 0.9;
const BAR_PAD_EM: f32 = 1.2;
/// The folder and branch labels ellipsize past this (`theme::WIDGET_BAR_LABEL_MAX_WIDTH`).
const LABEL_MAX_CHARS: usize = 20;

fn widget_width(widget: &Widget, em: f32) -> f32 {
    let chars: usize = widget.parts.iter().map(|p| p.text.chars().count() + 1).sum();
    let chars = match widget.id {
        WidgetId::Folder | WidgetId::Git => chars.min(LABEL_MAX_CHARS),
        _ => chars,
    };
    (ICON_EM + chars as f32 * CHAR_EM + WIDGET_PAD_EM) * em
}

/// The estimated width of `list` laid out in the bar, in px at `em` px per em.
pub fn list_width(list: &[Widget], em: f32) -> f32 {
    let widgets: f32 = list.iter().map(|w| widget_width(w, em)).sum();
    let gaps = GAP_EM * em * list.len().saturating_sub(1) as f32;
    widgets + gaps + BAR_PAD_EM * em
}

/// Drops widgets from the middle outward while `list` is wider than `available` px: limits
/// first, then machine. Folder, git and version stay.
pub fn fit(mut list: Vec<Widget>, available: f32, em: f32) -> Vec<Widget> {
    for dropped in [WidgetId::Limits, WidgetId::Machine] {
        if list_width(&list, em) <= available {
            break;
        }
        list.retain(|widget| widget.id != dropped);
    }
    list
}

pub fn is_hidden(hidden: &[String], id: WidgetId) -> bool {
    hidden.iter().any(|h| h == id.id())
}

/// `hidden` with `id` hidden, or shown again when it was hidden. Every other id, known or
/// not, stays where it is.
pub fn toggled(hidden: &[String], id: WidgetId) -> Vec<String> {
    if is_hidden(hidden, id) {
        return hidden.iter().filter(|h| *h != id.id()).cloned().collect();
    }
    let mut out = hidden.to_vec();
    out.push(id.id().to_owned());
    out
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
/// `utc_offset` is the local offset in seconds east of UTC.
pub fn local_clock(at: i64, now: i64, utc_offset: i32) -> String {
    let local = at + i64::from(utc_offset);
    let today = (now + i64::from(utc_offset)).div_euclid(86_400);
    let (day, rest) = (local.div_euclid(86_400), local.rem_euclid(86_400));
    let clock = format!("{:02}:{:02}", rest / 3600, rest % 3600 / 60);
    if day == today {
        return clock;
    }
    // Day 0 (1970-01-01) was a Thursday.
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    format!("{} {clock}", DAYS[day.rem_euclid(7) as usize])
}
```

- [ ] **Step 4: Run to verify pass**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui widget_bar`
Expected: PASS (all 14 tests). If `fit`'s two boundary asserts fail because `list_width(&no_limits) - 1.0` is not below `all - 1.0` ordering, the widths of the sample labels are the cause, not the code: keep the assertions and fix the estimate constants only if `list_width(no_limits) >= all`.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/widget_bar.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/Cargo.toml Cargo.lock
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): widget bar model with a pure widget list"
```

---

### Task 9: The Git view steps to the next file and wraps

**Files:**
- Modify: `crates/mkrew-ui/src/git_view.rs:436-452` (`Key`), `:1366-1383` (`key`), after `:1497` (new fn)
- Modify: `crates/mkrew-ui/src/git_view/tests.rs` (one test after `keys_move_files_and_hunks`, ends line 438)
- Modify: `crates/mkrew-ui/src/shell/git.rs:658-662` (scroll arm)

**Interfaces:**
- Produces: `git_view::Key::NextWrapping`: the next listed file (as ↓), but past the last file it selects the first; in the History it behaves as ↓ (stepping commits, which never wraps). Task 11's Git widget click sends it through `Shell::git_key`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn the_widget_click_steps_to_the_next_file_and_wraps_at_the_end() {
    let mut view = loaded();
    let first = target(Group::Staged, "a.rs");
    assert_eq!(view.selected, Some(first.clone()));
    for (group, path) in [
        (Group::Unstaged, "a.rs"),
        (Group::Unstaged, "b.rs"),
        (Group::Untracked, "new.txt"),
    ] {
        assert_eq!(
            view.key(Key::NextWrapping).diff,
            Some(target(group, path))
        );
    }
    assert_eq!(
        view.key(Key::NextWrapping).diff,
        Some(first.clone()),
        "past the last file: the first"
    );
    assert_eq!(view.selected, Some(first));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui git_view::tests::the_widget_click`
Expected: FAIL to compile (`no variant NextWrapping`).

- [ ] **Step 3: Implement**

`git_view.rs`, in `enum Key` after `Down`:

```rust
    /// The Git widget of the bar (ADR 0102): the next file like ↓, but past the last file the
    /// first. In the History it steps like ↓ (commits do not wrap).
    NextWrapping,
```

In `key()`: `Key::NextWrapping => self.step_file_wrapping(),` and after `step_file` (line 1497):

```rust
    /// [`Key::NextWrapping`].
    fn step_file_wrapping(&mut self) -> Requests {
        if self.history.is_some() {
            return self.step_file(1);
        }
        let files = self.steppable();
        let at = self
            .selected
            .as_ref()
            .and_then(|selected| files.iter().position(|(target, _)| target == selected));
        match (at, files.first()) {
            (Some(at), Some((first, _))) if at + 1 == files.len() => Requests {
                diff: self.select(first.clone()),
                ..Requests::default()
            },
            _ => self.step_file(1),
        }
    }
```

`shell/git.rs:658`: `git_view::Key::Up | git_view::Key::Down | git_view::Key::NextWrapping if requests.diff.is_some() || requests.show.is_some() => { self.scroll_diff_to(0); }` (keeps the file in view like the arrows; the list scroll above the `match` already follows `view.selected`).

- [ ] **Step 4: Run to verify pass**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui git_view`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/git_view.rs crates/mkrew-ui/src/git_view/tests.rs crates/mkrew-ui/src/shell/git.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Git view key that steps to the next file and wraps"
```

---

### Task 10: UI state: settings, events, `limits.get`, redraw rule, toggle command

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs:553` (field), `:663-664` (fields)
- Modify: `crates/mkrew-ui/src/shell.rs:64-72` (add `mod widget_bar;`)
- Create: `crates/mkrew-ui/src/shell/widget_bar.rs`
- Modify: `crates/mkrew-ui/src/shell/connection.rs:46` (read settings), `:397` (init), `:862` (`reset_connection_state`), `:942` (refresh), `:1033` (`apply_settings_file`), `:1166-1176` (`shown`), `:1214` (`dispatch`)
- Modify: `crates/mkrew-ui/src/redraw.rs:17-34`, `:98-109`, tests `:130-134`, `:194-220`
- Modify: `crates/mkrew-ui/src/keymap.rs:180`, `:346`, `:458`, `:593`, tests (after line 1229)
- Modify: `crates/mkrew-ui/src/palette.rs:665`, `:689`; `crates/mkrew-ui/src/palette/tests.rs:395`
- Modify: `crates/mkrew-ui/src/shell/sessions.rs:498`; `crates/mkrew-ui/src/detached/tests.rs:244`

**Interfaces:**
- Consumes: `UiSettings::{widget_bar, widgets_hidden}` and `write_ui_value` (Task 2); `WidgetData` (Task 8); `Request::LimitsGet` (Task 1).
- Produces: `Shell` fields `widget_bar: bool`, `widgets_hidden: Vec<String>`, `widget_data: mkrew_ui::widget_bar::WidgetData`; `Shell::set_widget_bar(bool, cx)`, `Shell::toggle_widget_bar(cx)`, `Shell::load_limits(window, cx)`; `Command::ToggleWidgetBar` (default `ctrl-alt-shift-j`, name `toggle_widget_bar`, palette title `View: Show/hide widget bar`); `redraw::Shown::widget_bar`. The fields `widgets_hidden` and `widget_data` are only read by the drawing in Task 11, so `cargo build` may warn about unread fields until then.

- [ ] **Step 1: Write the failing tests**

`redraw.rs` tests: add `AgentLimits, Cli, MachineStats` to the `use mkrew_protocol::{..}` at line 130-134, add `widget_bar: false,` to both `Shown { .. }` literals (lines 195-205 and 210-220), and append:

```rust
    #[test]
    fn limits_and_machine_stats_redraw_only_while_the_bar_is_drawn() {
        let w = world();
        let mut s = terminals(&w);
        let limits = Event::AgentLimits(AgentLimits {
            agent: Cli::Claude,
            five_hour: None,
            weekly: None,
            observed_at: 1,
        });
        let stats = Event::MachineStats(MachineStats {
            cpu_percent: 1.0,
            ram_used: 1,
            ram_total: 2,
        });
        assert!(!redraws(&w, &s, &limits), "bar hidden");
        assert!(!redraws(&w, &s, &stats), "bar hidden");
        s.widget_bar = true;
        assert!(redraws(&w, &s, &limits));
        assert!(redraws(&w, &s, &stats));
    }
```

`keymap.rs` test (insert after line 1229, inside the same test that asserts the other toggles):

```rust
        assert_eq!(bound("ctrl-alt-shift-j"), Some(Command::ToggleWidgetBar));
        assert_eq!(keymap.label(Command::ToggleWidgetBar), "Ctrl+Alt+Shift+J");
        // Global like the sidebar toggles; `ctrl-shift-j` stays Toggle theme.
        assert_eq!(Command::ToggleWidgetBar.context(), None);
        assert_eq!(bound("ctrl-shift-j"), Some(Command::ToggleTheme));
        assert_eq!(
            Command::from_name("toggle_widget_bar"),
            Some(Command::ToggleWidgetBar)
        );
```

`palette/tests.rs`: in `availability_follows_the_view` (list at lines 384-398) add `Command::ToggleWidgetBar,` after `Command::ToggleCompactSidebar,` and append:

```rust
#[test]
fn the_widget_bar_toggle_is_listed_without_a_workspace() {
    let none = Context {
        workspace: false,
        workspaces: 0,
        tabs: 0,
        ..context(View::Terminals)
    };
    let listed = entries(&Keymap::defaults(), &none, &[], &[], &[]);
    let entry = listed
        .iter()
        .find(|e| e.action == Action::Run(Command::ToggleWidgetBar))
        .expect("listed without a workspace");
    assert_eq!(entry.title, "View: Show/hide widget bar");
    assert_eq!(entry.shortcut, "Ctrl+Alt+Shift+J");
}
```

`detached/tests.rs` (line 244 list of commands a detached window does not handle): add `Command::ToggleWidgetBar,` after `Command::ToggleSidebar,`.

- [ ] **Step 2: Run to verify failure**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui redraw keymap palette detached`
Expected: FAIL to compile (`no field widget_bar`, `no variant ToggleWidgetBar`).

- [ ] **Step 3: Implement**

`keymap.rs`: enum, after `ToggleInspector` (line 180):

```rust
    /// Show or hide the widget bar at the bottom of the main window (ADR 0102); remembered as
    /// `[ui] widget_bar`.
    ToggleWidgetBar,
```

`Command::ToggleWidgetBar,` after `Command::ToggleInspector,` in the list at line 346; `Command::ToggleWidgetBar => "toggle_widget_bar".into(),` after line 458; `Command::ToggleWidgetBar => "ctrl-alt-shift-j".into(),` after line 593.

`palette.rs`: `Command::ToggleWidgetBar => "View: Show/hide widget bar".into(),` after line 665; add `| Command::ToggleWidgetBar` to the always-available group (line 688-691).

`shell/sessions.rs:498`: `Command::ToggleWidgetBar => self.toggle_widget_bar(cx),` after `Command::ToggleInspector => ...`.

`redraw.rs`: add to `Shown` (after `all_board`):

```rust
    /// The widget bar is drawn (`[ui] widget_bar`), so its limits and machine numbers are read.
    pub widget_bar: bool,
```

and in `event_needs_redraw`, remove the two Task 1 patterns from the `=> true` group and add before the `Event::SessionState(_)` group:

```rust
        // The widget bar's limits and CPU / RAM (ADR 0102).
        Event::AgentLimits(_) | Event::MachineStats(_) => shown.widget_bar,
```

`main.rs`: `file_tab_menu` stays; add the three fields after `board_all: bool,` (line 664):

```rust
    /// `[ui] widget_bar` (ADR 0102): the thin bar at the bottom of the main window.
    widget_bar: bool,
    /// `[ui] widgets_hidden`: ids of the bar's hidden widgets (ids this build does not know
    /// are kept).
    widgets_hidden: Vec<String>,
    /// The agent limits and machine stats the bar shows (events and `limits.get`).
    widget_data: mkrew_ui::widget_bar::WidgetData,
```

`shell.rs`: `mod widget_bar;` (before `mod workspace;`).

`connection.rs`: after line 46 `let board_all = ui.board_all_workspaces;` add `let (widget_bar, widgets_hidden) = (ui.widget_bar, ui.widgets_hidden.clone());`; after `board_all,` in the struct literal (line 397): `widget_bar, widgets_hidden, widget_data: mkrew_ui::widget_bar::WidgetData::default(),`; in `reset_connection_state` after `self.git_summary = None;` (line 862): `self.widget_data.clear();`; in `refresh` after `shell.load_plans(window, cx);` (line 942): `shell.load_limits(window, cx);`; in `apply_settings_file` after `self.board_all = ...;` (line 1033): `self.widget_bar = settings.ui.widget_bar; self.widgets_hidden = settings.ui.widgets_hidden.clone();`; in `shown()` literal add `widget_bar: self.widget_bar,`; in `dispatch` after `data_changed |= self.ports.apply(event);` (line 1214): `data_changed |= self.widget_data.apply(event);`.

`shell/widget_bar.rs`:

```rust
//! The widget bar (ADR 0102): its switch, its hide toggles and the `limits.get` read. The
//! drawing and the right-click menu follow in the same file.

use crate::*;

impl Shell {
    /// `[ui] widget_bar` on or off, saved. A settings file that does not parse keeps the
    /// choice for this run; the palette says why (§20.0) and a toast says it was not saved.
    pub(crate) fn set_widget_bar(&mut self, on: bool, cx: &mut Context<'_, Self>) {
        if self.widget_bar == on {
            return;
        }
        self.widget_bar = on;
        if let Err(error) = mkrew_core::settings::write_ui_value(
            &self.settings_file,
            "widget_bar",
            &on.to_string(),
        ) {
            self.settings_write_failed("the widget bar", error, cx);
        }
        cx.notify();
    }

    /// Ctrl+Alt+Shift+J, the palette's "View: Show/hide widget bar".
    pub(crate) fn toggle_widget_bar(&mut self, cx: &mut Context<'_, Self>) {
        self.set_widget_bar(!self.widget_bar, cx);
    }

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
                    if shell.widget_data.load_limits(listed) && shell.widget_bar {
                        cx.notify();
                    }
                }
                Err(error) => tracing::warn!("limits.get failed: {error:#}"),
            });
        })
        .detach();
    }
}
```

(add `AgentLimits` to the `use mkrew_protocol::{..}` list of `main.rs` if you prefer the short name; the full path above compiles without it.)

- [ ] **Step 4: Run to verify pass**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui` (whole crate: the exhaustive `Command` matches, palette, keymap template and detached tests all run)
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): widget bar state, limits.get, redraw rule and toggle command"
```

---

### Task 11: Draw the bar and run its clicks

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs:1902-1930` (tokens), tests near `:2349`
- Modify: `crates/mkrew-ui/src/settings_dialog.rs` (const + test)
- Modify: `crates/mkrew-ui/src/shell/widget_bar.rs` (drawing and clicks)
- Modify: `crates/mkrew-ui/src/main.rs:1238` (root column child)

**Interfaces:**
- Consumes: Task 8 (`widget_list`, `fit`, `Widget`, `Click`, `Side`, `Level`, `WidgetId`), Task 9 (`git_view::Key::NextWrapping`), Task 10 (`Shell` fields). Existing: `open_workspace_in_explorer(project_id, cx)` (`shell/workspace_actions.rs:411`), `open_git(project_id, window, cx)` (`shell/git.rs:124`), `git_shown()` (`shell/focus.rs:177`), `check_updates(true, window, cx)` (`shell/update.rs:17`), `open_settings_dialog(window, cx)` (`shell/settings.rs:11`), `machine_name()` (`shell/connection.rs:1351`), `tooltip(theme, text, keys)` (`shell/widgets.rs:1283`), `pressable(Look::Text, ..)` / `on_press` (`pressable.rs`).
- Produces: theme tokens `WIDGET_BAR_HEIGHT` (24.0), `WIDGET_BAR_PADDING` (`space::S`), `WIDGET_BAR_GAP` (`space::M`), `WIDGET_BAR_LABEL_MAX_WIDTH` (160.0); `settings_dialog::INTEGRATIONS_TAB`; `Shell::render_widget_bar(&self, &Window, &Theme, cx) -> Option<AnyElement>`; `Shell::widget_clicked(Click, window, cx)`.

- [ ] **Step 1: Write the failing tests**

`theme.rs` tests (next to `layout_heights_scale_with_the_ui_font`, line 2349):

```rust
    #[test]
    fn the_widget_bar_tokens_scale_and_sit_on_the_spacing_scale() {
        assert_eq!(
            rems_px(WIDGET_BAR_HEIGHT),
            gpui::rems(WIDGET_BAR_HEIGHT / 13.0)
        );
        for step in [WIDGET_BAR_PADDING, WIDGET_BAR_GAP] {
            assert!([4.0, 8.0, 12.0, 16.0, 24.0, 32.0].contains(&step), "{step}");
        }
    }
```

`settings_dialog.rs` tests (next to the NAV test at line ~1765):

```rust
    #[test]
    fn the_limits_widget_opens_the_first_page_of_the_integrations_group() {
        let (title, tabs) = NAV[1];
        assert_eq!(title, "Integrations");
        assert_eq!(tabs[0], INTEGRATIONS_TAB);
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui widget_bar_tokens integrations`
Expected: FAIL to compile (`WIDGET_BAR_HEIGHT`, `INTEGRATIONS_TAB` missing).

- [ ] **Step 3: Implement**

`theme.rs`, after `STATUS_PILL_HEIGHT` (line 1927):

```rust
/// The widget bar along the bottom of the main window (ADR 0102), in px at the default font
/// size: its height, the padding at its two ends, the gap between widgets, and the widest a
/// folder or branch label gets before it ellipsizes. Font size is `font_size::META`; colors
/// are `chrome`, `border` and `text_muted`.
pub const WIDGET_BAR_HEIGHT: f32 = 24.0;
pub const WIDGET_BAR_PADDING: f32 = space::S;
pub const WIDGET_BAR_GAP: f32 = space::M;
pub const WIDGET_BAR_LABEL_MAX_WIDTH: f32 = 160.0;
```

`settings_dialog.rs`, after the `NAV` definition:

```rust
/// Where the limits widget opens Settings: the first page of the Integrations group.
pub const INTEGRATIONS_TAB: Tab = Tab::Settings(Page::Jira);
```

`shell/widget_bar.rs`: add imports and the drawing (append to the file):

```rust
use mkrew_ui::widget_bar::{self as model, Click, Level, Side, Widget, WidgetId};

/// Seconds east of UTC at Unix time `at`, from the OS time zone (0 when it cannot say).
fn local_offset(at: i64) -> i32 {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_opt(at, 0)
        .single()
        .map_or(0, |time| time.offset().local_minus_utc())
}

fn level_color(level: Level, theme: &Theme) -> u32 {
    match level {
        Level::Muted => theme.text_muted,
        Level::Warn => theme.status_needs_input,
        Level::Danger => theme.danger_text,
    }
}

fn icon_of(id: WidgetId) -> Icon {
    match id {
        WidgetId::Folder => Icon::Folder,
        WidgetId::Git => Icon::GitBranch,
        WidgetId::Limits => Icon::Bot,
        WidgetId::Machine => Icon::Server,
        WidgetId::Version => Icon::Info,
    }
}

impl Shell {
    /// The bar below the sidebar and the main area, or nothing while `[ui] widget_bar` is off.
    /// Detached windows never call this. The strip stays when every widget is hidden or has
    /// nothing to show, so a right-click can bring them back.
    pub(crate) fn render_widget_bar(
        &self,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.widget_bar {
            return None;
        }
        let project = (self.workspace)
            .and_then(|id| self.sidebar.projects().iter().find(|p| p.id == id));
        let input = model::Input {
            workspace: project.map(|p| (p.name.as_str(), p.path.as_str())),
            git: self.git_summary(),
            data: &self.widget_data,
            machine_name: self.machine_name().unwrap_or("This computer"),
            version: env!("CARGO_PKG_VERSION"),
            hidden: &self.widgets_hidden,
            now_secs: status::now_ms() / 1000,
        };
        let em = cx.global::<theme::Fonts>().ui_size;
        let width = f32::from(window.viewport_size().width);
        let shown = model::fit(model::widget_list(&input, local_offset), width, em);
        let (left, right): (Vec<_>, Vec<_>) = shown
            .into_iter()
            .partition(|widget| widget.id.side() == Side::Left);
        let left: Vec<_> = (left.into_iter())
            .map(|widget| self.render_widget(widget, theme, cx))
            .collect();
        let right: Vec<_> = (right.into_iter())
            .map(|widget| self.render_widget(widget, theme, cx))
            .collect();
        let group = || {
            div()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(theme::WIDGET_BAR_GAP))
        };
        Some(
            div()
                .id("widget-bar")
                .flex_none()
                .w_full()
                .h(theme::rems_px(theme::WIDGET_BAR_HEIGHT))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(theme::WIDGET_BAR_GAP))
                .px(px(theme::WIDGET_BAR_PADDING))
                .bg(rgb(theme.chrome))
                .border_t(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.border))
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(group().children(left))
                .child(group().children(right))
                .into_any_element(),
        )
    }

    /// One widget: an icon and its label in a `pressable` element (hover, press and focus from
    /// `widgets.rs`), with its tooltip. The machine widget has no click and no pointer.
    fn render_widget(
        &self,
        widget: Widget,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let (id, click) = (widget.id, widget.click);
        let capped = matches!(id, WidgetId::Folder | WidgetId::Git);
        let parts = widget.parts.into_iter().map(|part| {
            div()
                .flex_none()
                .when(part.level != Level::Muted, |el| {
                    el.text_color(rgb(level_color(part.level, theme)))
                })
                .child(part.text)
        });
        let element = div()
            .id(SharedString::from(format!("widget-{}", id.id())))
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .px(px(space::XS))
            .rounded(px(radius::SMALL))
            .whitespace_nowrap()
            .tooltip(tooltip(theme, &widget.tooltip, ""))
            .child(icon_svg(icon_of(id), theme::ICON_SIZE_SMALL, theme.text_muted))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .overflow_hidden()
                    .text_ellipsis()
                    .when(capped, |el| {
                        el.max_w(theme::rems_px(theme::WIDGET_BAR_LABEL_MAX_WIDTH))
                    })
                    .children(parts),
            );
        if click == Click::None {
            return element.into_any_element();
        }
        element
            .pressable(Look::Text, true, theme)
            .on_press(
                true,
                cx.listener(move |shell, _, window, cx| shell.widget_clicked(click, window, cx)),
            )
            .into_any_element()
    }

    /// What a click on a widget does (spec section 4).
    pub(crate) fn widget_clicked(
        &mut self,
        click: Click,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match click {
            Click::None => {}
            Click::OpenFolder => {
                if let Some(project_id) = self.workspace {
                    self.open_workspace_in_explorer(project_id, cx);
                }
            }
            Click::Git => {
                let Some(project_id) = self.workspace else {
                    return;
                };
                if self.git_shown() {
                    self.git_key(git_view::Key::NextWrapping, window, cx);
                } else {
                    self.open_git(project_id, window, cx);
                }
            }
            Click::OpenLimits => {
                self.open_settings_dialog(window, cx);
                if let Some(dialog) = self.settings_dialog.as_mut() {
                    let _ = dialog.set_tab(settings_dialog::INTEGRATIONS_TAB);
                }
            }
            Click::CheckUpdate => self.check_updates(true, window, cx),
        }
    }
}
```

(`use crate::*;` at the top of the file already exists from Task 10; keep one `use crate::*;`. If `Icon`, `icon_svg`, `status`, `Look`, `tooltip` or `git_view` are not in scope through `crate::*`, import them exactly as `shell/chrome.rs` does.)

`main.rs`: after the `.child( div().flex_1() ... )` of the root column (its closing `)` at line 1238, just before the end of `render`), add the bar as the last `flex_none` child:

```rust
            .children(self.render_widget_bar(window, &theme, cx))
```

- [ ] **Step 4: Run to verify pass**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui` then `CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: PASS and clean (the unread-field warnings of Task 10 are gone). The drawing has no headless test: GPUI elements are not unit tested in this crate; `docs/HUMAN_CHECKS.md` (Task 13) lists the visual checks in dark and light.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui Cargo.lock
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): draw the widget bar with clickable widgets"
```

---

### Task 12: The bar's right-click menu

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs:396-400` (struct `WidgetMenu`), `:553` (field), the bar child (menu)
- Modify: `crates/mkrew-ui/src/shell/connection.rs:337` (init `widget_menu: None`)
- Modify: `crates/mkrew-ui/src/shell/focus.rs:125` (`close_menus`), `crates/mkrew-ui/src/shell/menu_render.rs:318-334` (`menu_open`), `:360-362` (`open_menu_rows`)
- Modify: `crates/mkrew-ui/src/shell/widget_bar.rs`

**Interfaces:**
- Consumes: `WidgetId::{ALL, title}`, `is_hidden`, `toggled` (Task 8); `write_widgets_hidden` (Task 2); `set_widget_bar` (Task 10); the shared menu API: `begin_menu` / `finish_menu` (`menu_render.rs:409-439`), `render_menu(state, id, rows, header, Place::At(point), theme, cx)` (`menu_render.rs:237`), `MenuItem::new(..).checked(on).toggle().row()` (`menu.rs:98-173`), `act(..)` (`menu_render.rs:11`).
- Produces: `WidgetMenu { position: Point<Pixels> }`, `Shell::open_widget_menu`, `widget_menu_rows`, `render_widget_menu`, `toggle_widget`.

- [ ] **Step 1: Write the failing test**

The row model is the pure `toggled`/`is_hidden` of Task 8 (tested there, including ids this build does not know). The wiring is checked by building and clippy plus the HUMAN_CHECKS list; add one more pure assertion to `widget_bar.rs` tests so the menu's per-row state is pinned:

```rust
    #[test]
    fn every_widget_has_a_menu_row_state_and_a_title() {
        let hidden = vec!["machine".to_owned(), "clock".to_owned()];
        let rows: Vec<(&str, bool)> = WidgetId::ALL
            .into_iter()
            .map(|id| (id.title(), !is_hidden(&hidden, id)))
            .collect();
        assert_eq!(
            rows,
            [
                ("Folder", true),
                ("Git branch", true),
                ("Agent limits", true),
                ("CPU and RAM", false),
                ("Version", true)
            ]
        );
    }
```

- [ ] **Step 2: Run to verify** (it passes at once: it pins the Task 8 API the menu uses)

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui widget_bar`
Expected: PASS.

- [ ] **Step 3: Implement**

`main.rs`, next to `FileTabMenu` (line 396):

```rust
/// The open right-click menu of the widget bar (ADR 0102): one checkable row per widget and
/// "Hide widget bar".
struct WidgetMenu {
    /// Window position of the click.
    position: Point<Pixels>,
}
```

Field after `file_tab_menu` (line 553):

```rust
    /// The open right-click menu of the widget bar.
    widget_menu: Option<WidgetMenu>,
```

`connection.rs`: `widget_menu: None,` after `file_tab_menu: None,` (line 337). `focus.rs` `close_menus`: `self.widget_menu = None;` after `self.file_tab_menu = None;` (line 125). `menu_render.rs`: add `|| self.widget_menu.is_some()` to `menu_open` (after line 325) and in `open_menu_rows` after the `file_tab_menu` arm (line 362):

```rust
        if self.widget_menu.is_some() {
            return Some(self.widget_menu_rows());
        }
```

`shell/widget_bar.rs`, append:

```rust
impl Shell {
    /// Right-click anywhere on the bar.
    pub(crate) fn open_widget_menu(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.begin_menu(window, cx);
        self.widget_menu = Some(WidgetMenu { position });
        self.finish_menu(window.last_input_was_keyboard(), window, cx);
    }

    /// Hides or shows one widget and saves `[ui] widgets_hidden`; ids this build does not know
    /// stay in the list. The menu stays open (a multi-select row).
    pub(crate) fn toggle_widget(&mut self, id: WidgetId, cx: &mut Context<'_, Self>) {
        self.widgets_hidden = model::toggled(&self.widgets_hidden, id);
        if let Err(error) =
            mkrew_core::settings::write_widgets_hidden(&self.settings_file, &self.widgets_hidden)
        {
            self.settings_write_failed("the widget bar", error, cx);
        }
        cx.notify();
    }

    /// One checkable row per widget, then "Hide widget bar".
    pub(crate) fn widget_menu_rows(&self) -> Rows {
        let mut rows: Rows = WidgetId::ALL
            .into_iter()
            .map(|id| {
                MenuItem::new(
                    id.title(),
                    act(move |shell, _, cx| shell.toggle_widget(id, cx)),
                )
                .checked(!model::is_hidden(&self.widgets_hidden, id))
                .toggle()
                .row()
            })
            .collect();
        rows.push(MenuRow::Separator);
        rows.push(
            MenuItem::new(
                "Hide widget bar",
                act(|shell, _, cx| shell.set_widget_bar(false, cx)),
            )
            .keys(self.keymap.label(Command::ToggleWidgetBar))
            .row(),
        );
        rows
    }

    pub(crate) fn render_widget_menu(
        &self,
        menu: &WidgetMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        render_menu(
            self.menu_state(),
            "widget-menu",
            self.widget_menu_rows(),
            None,
            Place::At(menu.position),
            theme,
            cx,
        )
    }
}
```

In `render_widget_bar` (Task 11) add the right-click handler and the menu to the bar element, before `.child(group().children(left))`:

```rust
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|shell, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        shell.open_widget_menu(event.position, window, cx);
                    }),
                )
```

and after `.child(group().children(right))`:

```rust
                .children(
                    (self.widget_menu.as_ref())
                        .filter(|_| !self.modal_open())
                        .map(|menu| self.render_widget_menu(menu, theme, cx)),
                )
```

(`cx` is borrowed by `cx.listener` first and by the closure later; if the borrow checker objects, build the menu element into a `let menu = ...;` before the `div()` chain and pass `.children(menu)`.)

- [ ] **Step 4: Run to verify pass**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui` then `CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: PASS and clean.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): widget bar right-click menu"
```

---

### Task 13: Docs (ADR 0102, SPEC, README, HUMAN_CHECKS, PROGRESS)

**Files:**
- Create: `docs/decisions/0102-widget-bar.md`
- Modify: `docs/SPEC.md` (sections below), `README.md`, `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md`

**Interfaces:** none (documentation only). Docs rules: no em dash, one sentence per line, plain wording.

- [ ] **Step 1: Write ADR 0102**

Create `docs/decisions/0102-widget-bar.md`:

```markdown
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
```

- [ ] **Step 2: SPEC edits**

Anchors are line numbers at HEAD (re-find them by the quoted text):

1. **Section 7, requests table:** after the `watch.release` row (line 574) add
   `| \`limits.get\` | - | \`AgentLimits[]\`: the newest 5-hour and weekly limits each agent reported (\`agent\` = claude or codex, \`five_hour?\` / \`weekly?\` = \`{used_percent, resets_at?}\`, \`observed_at\` Unix seconds); kept in daemon memory only (ADR 0102) |`
   `| \`limits.report\` | \`AgentLimits\` | ok (\`null\`) - from \`mk statusline\`; the daemon keeps the newest \`observed_at\` per agent (a tie: the later report) and announces \`agent.limits\`; \`invalid_request\` for an agent other than claude and codex or a percentage that is not a number; percentages are clamped to 0-100 |`
2. **Section 7, events table:** after the `session.usage` row (line 644) add
   `| \`agent.limits\` | an agent's newest limits changed (\`AgentLimits\`, see \`limits.get\`); Codex's come from its transcript at a turn end (§20E.3), Claude's from \`mk statusline\` |`
   `| \`machine.stats\` | CPU and RAM of the daemon's machine: \`cpu_percent\`, \`ram_used\`, \`ram_total\` (0 = unknown); sampled every 2 s while a client is connected, sent when CPU moves 1 point or RAM 100 MB, or every 10 s |`
3. **Section 7, version notes:** after line 697 (`Protocol 54 ...`) add `Protocol 55 (Widget bar spec 2026-11-22 sections 5 and 6, ADR 0102): requests \`limits.get\` and \`limits.report\`, events \`agent.limits\` and \`machine.stats\`.`
4. **Section 9:** after the `mk hook` line (790) add `mk statusline                          # Claude Code's status line command (ADR 0102): reports the rate limits, then runs your own status line; set up by \`mk integration install claude\``; and change the `mk integration` line's comment nowhere (it already lists the agents).
5. **Section 10:** replace the bottom border of the diagram (line 821, the `└──────────────┴───...┘` line) with
   ```
   ├──────────────┴────────────────────────────────────────────────────────────────────────┤
   │ 📁 api  ⎇ master · 3                  Claude 5h 42% · 7d 18%  CPU 23% · RAM 11/32 GB  v0.5.3 │
   └───────────────────────────────────────────────────────────────────────────────────────┘
   ```
   and replace the sentence at lines 823-824 (`... detail on hover / in menus. There is no status bar.`) with `... detail on hover / in menus. It removed the old status bar; ADR 0102 adds back a thin widget bar along the bottom (see "Widget bar" below).` Replace the bullet at lines 945-948 (`- **No status bar** (ADR 0054): ...`) with the same first clause kept and a closing `The widget bar of ADR 0102 is not that bar: it holds only the folder, branch, limits, machine load and version.` Then insert before `### Detached windows (ADR 0096)` (line 1128):
   ```markdown
   ### Widget bar (ADR 0102)

   - One bar, `WIDGET_BAR_HEIGHT` tall, along the bottom of the main window, below the sidebar and the main area (`theme.chrome`, a 1 px `theme.border` top edge, muted text, the same in dark and light).
     Detached windows have none.
   - Left: **folder** (the workspace name; tooltip the root path; click opens it in the file explorer) and **Git** (`⎇ master`, with ` · 3` when 3 files changed, `⎇ 2 repos` for several; click opens the Git view, and with Git already open steps to the next changed file, wrapping at the end).
     Right: **limits** (`Claude 5h 42% · 7d 18% | Codex 5h 10% · 7d 3%`; a window below 80 % is muted, from 80 % amber, from 95 % red; the tooltip gives each reset in local time and as "in 2 h 10 min"; click opens Settings at the Integrations pages), **machine** (`CPU 23% · RAM 11.2/31.8 GB` of the daemon's machine, no click), **version** (`v0.5.3`; click runs the update check).
   - A widget with nothing to show is absent: no limits reported yet, no stats yet, no workspace, no repository.
     A limit window whose reset time passed shows nothing until a new report.
   - A narrow window drops limits first, then machine; folder, Git and version stay.
   - Right-click anywhere on the bar: one checkable row per widget and "Hide widget bar".
     `[ui] widget_bar` and `[ui] widgets_hidden` (ids `folder`, `git`, `limits`, `machine`, `version`; unknown ids are kept) remember the choices; Settings has a "Widget bar" switch on Appearance.
   ```
6. **Section 13 table:** after the `Toggle compact sidebar` row (line 1465) add `| Global | Show / hide the widget bar (ADR 0102) | Ctrl+Alt+Shift+J |`.
7. **Section 20.0 settings example:** after the `md_split = 50 ...` line (2143) add
   `widget_bar = true          # §10; the widget bar at the bottom of the main window; its menu and Ctrl+Alt+Shift+J write it`
   `widgets_hidden = []        # §10; ids of hidden widgets: folder, git, limits, machine, version; its menu writes it`.
8. **Section 20E:** after the usage item that ends `Unknown/changed format → no usage, one warning in the log.` (line 2457) add
   ```markdown
   4. **Agent limits (ADR 0102).** Limits belong to the account, so the daemon keeps the newest `AgentLimits` per agent in memory (`observed_at` wins, a tie goes to the later report) and announces each change as `agent.limits`; `limits.get` returns them.
      Codex: `UsageReader` reads `payload.rate_limits` of each `token_count` line (also while `info` is null): `primary` / `secondary` with `window_minutes` 300 are the 5-hour window and 10080 the weekly one, any other length is ignored, `used_percent` and `resets_at` are taken as written, `observed_at` is the line's timestamp; the `track` task passes them on at each turn end.
      Claude: `mk statusline` reads Claude Code's status line JSON from stdin (at most 1 MB), takes `rate_limits.five_hour` and `rate_limits.seven_day` (`used_percentage`, `resets_at` in Unix seconds) and sends `limits.report` within 300 ms; a failure is silent.
      It then runs the user's own status line, saved by `mk integration install claude` in `claude-statusline.json` next to `settings.json`, through the shell with the same stdin and a 5 s timeout, and prints its output; any failure prints nothing and exits 0.
      Install sets `statusLine` to `"<mk>" statusline`; uninstall puts the saved line back or removes the key.
      `mk integration status claude` says "partly installed" when the hooks are in but the status line is not.
      omp and Copilot record no limits.
   ```
9. **Table of contents / headings:** if section 10's list of subsections in the table of contents (line 7-33) names `Detached windows`, add `Widget bar` beside it.

- [ ] **Step 3: README, HUMAN_CHECKS, PROGRESS**

`README.md`: after the Hooks section (after the paragraph ending `The hooks call \`mk hook …\`, which does nothing when the agent runs outside Mkrew.`) insert:

```markdown
### Widget bar

A thin bar along the bottom of the window shows the workspace folder, the Git branch with its change count, your Claude and Codex 5-hour and weekly limits, the CPU and RAM of the computer the agents run on, and the Mkrew version.
Click the folder to open it in the file explorer, the branch to open Git (again: the next changed file), the limits to open Settings, the version to check for updates.
Right-click the bar to hide single widgets or the whole bar.
`Ctrl+Alt+Shift+J` and the palette's "View: Show/hide widget bar" toggle it; `[ui] widget_bar` and `widgets_hidden` in `settings.toml` keep the choice.
Codex limits come from its transcript after a turn.
Claude limits need its status line: `mk integration install claude` sets Claude Code's `statusLine` to `mk statusline`, which reports the limits to Mkrew and then runs the status line you had before (saved as `claude-statusline.json` next to `settings.json`; `mk integration uninstall claude` puts it back).
Without a Claude subscription login (an API key), or with an older Claude Code, Claude sends no limits and that part of the bar stays absent.
Run `mk integration install claude` again after updating Mkrew to get the status line.
```

`docs/HUMAN_CHECKS.md`: insert a new section as the first "### To verify" block (before `### To verify - 2026-11-21 Pressable audit (P3)`, line 24):

```markdown
### To verify - 2026-11-22 Widget bar (ADR 0102, plan docs/superpowers/plans/2026-11-22-widget-bar.md)

- [ ] The bar in dark and light themes: folder, branch with count, limits, CPU and RAM, version; hover, press and focus feel like the other chips; the text is readable; a long workspace name ends in an ellipsis.
- [ ] Narrow the window: limits drop first, then CPU and RAM; folder, branch and version stay.
- [ ] Right-click the bar: each widget has a check mark and toggling keeps the menu open; "Hide widget bar" hides the bar; `Ctrl+Alt+Shift+J` and the palette entry bring it back; restart Mkrew and the choices are kept; a hidden id you add by hand to `widgets_hidden` (for example `"clock"`) survives toggling.
- [ ] Git widget: click opens the Git view; click again steps through the changed files and wraps from the last to the first; a workspace with several repositories shows `N repos`.
- [ ] Folder widget opens the workspace folder in the file explorer; version widget runs the update check; limits widget opens Settings.
- [ ] Real limits: run `mk integration install claude`, send one turn in Claude Code and one in Codex; the limits widget shows both with plausible percentages and reset times in the tooltip (local time). A status line you had before still prints in Claude Code; `mk integration uninstall claude` brings it back.
- [ ] On Windows, if your own status line uses bash syntax, check that it still prints (it is run through `cmd /C`, not Git Bash).
- [ ] CPU and RAM match Task Manager closely; in a machine window they show that machine's numbers; with no Mkrew window open the daemon does no sampling (check its CPU use at idle).
```

`docs/PROGRESS.md`: under "Notes for the next session" add as the first bullet:

```markdown
- 2026-11-22 widget bar (branch `feat/widget-bar`, spec docs/superpowers/specs/2026-11-22-widget-bar-design.md, plan docs/superpowers/plans/2026-11-22-widget-bar.md, ADR 0102, protocol 55, no migration): `agent.limits` / `machine.stats` events and `limits.get` / `limits.report` requests; daemon `limits.rs` (newest per agent) and `machine.rs` (`sysinfo` sampler, `should_send` throttle, `ClientGuard`); `UsageReader::limits()` for Codex; `mk statusline` and the status line in `mk integration`; UI `mkrew_ui::widget_bar` (pure widget list), `shell/widget_bar.rs` (draw, clicks, right-click menu), `Command::ToggleWidgetBar`, `[ui] widget_bar` / `widgets_hidden`. Checks in HUMAN_CHECKS "2026-11-22 Widget bar".
```

- [ ] **Step 4: Verify the docs tests**

Run: `CARGO_BUILD_JOBS=4 cargo test -p mkrew-cli --test readme`
Expected: PASS (`mk integration install claude` and `mk statusline` are real subcommands). Search the new text for em dashes and long lines: `grep -n "—" docs/decisions/0102-widget-bar.md README.md` must find none in the new paragraphs.

- [ ] **Step 5: Commit**

```bash
git add docs/decisions/0102-widget-bar.md docs/SPEC.md README.md docs/HUMAN_CHECKS.md docs/PROGRESS.md
git -c user.email=tungnv975@gmail.com commit -m "docs: widget bar (ADR 0102, SPEC, README, HUMAN_CHECKS)"
```

---

### Task 14: Full verify

**Files:** none (fixes only, if verify finds any).

- [ ] **Step 1: Format**

Run: `CARGO_BUILD_JOBS=4 cargo fmt --all`
Expected: no output; if it changed files, stage them for the fix commit in step 4.

- [ ] **Step 2: Full verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (set `CARGO_BUILD_JOBS=4` in the environment first)
Expected: `verify: OK` (fmt check, `clippy --workspace --all-targets -D warnings`, `INSTA_UPDATE=no cargo test --workspace`). If it passes 600 s, run `CARGO_BUILD_JOBS=4 cargo test --workspace` separately and report both.

- [ ] **Step 3: Fix what it finds**

Typical findings: rustfmt layout of the long test literals in Tasks 3 to 8, `clippy::needless_borrow` in `shell/widget_bar.rs`, an unused import. Fix the cause, never silence a lint with `#[allow]` without a comment saying why. Re-run step 2 until green.

- [ ] **Step 4: Commit fixes (only if any)**

```bash
git add -u
git -c user.email=tungnv975@gmail.com commit -m "chore(widget-bar): fmt and clippy fixes from the full verify"
```

---

## Self-review

**Spec coverage** (spec sections to tasks):

| Spec section | Task |
| --- | --- |
| 1 Goal (bar content, per-widget and whole hide, survives restart, no invented data) | 2 (settings), 8 (absent widgets), 10 (state), 11 (draw), 12 (menu) |
| 2 Decisions: where, default, toggle key and palette, per widget, order, machine, Claude and Codex sources, limit colors | 1, 2, 8, 10, 11, 12 |
| 3 The bar: file, root-column child, theme tokens, look, pressable widgets, tooltips, narrow dropping, right-click `render_menu` | 8 (`fit`), 11 (tokens, draw, tooltips), 12 (menu); file location see Spec gaps |
| 4 Widgets: folder, git (+ wrap), limits, machine, version, absent rules, expired windows | 8, 9, 11 |
| 5.1 Data, newest wins, broadcast, `limits.get`, not persisted | 1, 4 |
| 5.2 Codex via `UsageReader`, unknown windows, `track` passes on | 3, 4 |
| 5.3 Claude `mk statusline`, install, save, chain, uninstall, `Partly`, same socket as `mk hook` | 6, 7 |
| 6 Machine stats: 2 s, only with a subscriber, throttle and heartbeat, per-connection machine | 5, 10 (`widget_data` per connection) |
| 7 Settings and keys | 2, 10 |
| 8 Error handling table | 7 (no `rate_limits`, no daemon, chained failure), 6 (unreadable saved file), 8 (no limits, no RAM total), 1 (version check by existing hello) |
| 9 Testing | tests in every task; HUMAN_CHECKS in 13 |
| 10 Docs | 13 |
| 11 Out of scope | not built: no detached bar, no omp limits (daemon refuses), no OAuth endpoint, no reordering, no cost |

**Placeholder scan:** no "TBD", "TODO" or "similar to Task N"; every code step shows code. Task 13 gives the exact text for each doc edit and anchors by line and quoted text.

**Type consistency:** `AgentLimits`, `LimitWindow`, `MachineStats` (Task 1) are used with the same fields in Tasks 3, 4, 5, 7, 8. `AgentLimits::supersedes` (Task 1) is the one merge rule in Tasks 4 and 8. `Hub::offer_limits` returns `Result<bool, String>` in Task 4 and the `track` task logs the `Err`. `WidgetId`, `Click`, `Level`, `Side`, `Widget`, `Input`, `WidgetData` names match between Tasks 8, 10, 11, 12. `git_view::Key::NextWrapping` (Task 9) is the key Task 11 sends. `Saved`, `Edited` and `SAVED_STATUSLINE` (Task 6) match `saved_statusline_file()` (Task 7). `Kind::Strings` (Task 2) is matched in the two UI arms listed.

**Review Focus:** each of the five lines has its test in the task named there (Task 7 payload tests, Task 6 and 7 quoting and chain tests, Task 4 clamp and refusal tests with Task 8 expiry and clamp tests, Task 2 and 8 unknown-id tests, Task 8 narrow-width test and Task 11's always-drawn strip).

## Spec gaps

The spec is wrong or silent against the code in these places; each is decided here (the spec is not edited).

1. **`AgentKind` does not exist.** The protocol already has `Cli { Claude, Codex, Omp, Copilot }`; `AgentLimits.agent` is a `Cli`, the daemon refuses Omp and Copilot with `invalid_request`, and the UI ignores them.
2. **Claude's field names are not in the spec.** Claude Code's status line docs name `rate_limits.five_hour.used_percentage`, `rate_limits.five_hour.resets_at` (Unix seconds) and the same under `seven_day`; `mk statusline` reads those. The `session_id` field the spec says it "takes" has no use and is not read.
3. **`limits.report` params and wire names are unspecified.** The params are an `AgentLimits` (so the reporter stamps `observed_at` and tests control ordering); the names are `limits.get`, `limits.report`, `agent.limits`, `machine.stats`; `Event::MachineStats` carries a named `MachineStats` struct (same wire shape as the spec's inline fields).
4. **A tie in `observed_at`:** the spec says "newer wins"; the plan lets the later report win a tie when the content differs, and ignores an identical repeat.
5. **Codex `observed_at`** is the transcript line's own timestamp (parsed by hand in core, which has no time crate), not the daemon's clock, so re-reading an old resumed transcript cannot pass old limits off as fresh.
6. **`<config>/claude-statusline.json`** is read as Claude's config folder (`CLAUDE_CONFIG_DIR` or `~/.claude`, next to `settings.json`), so uninstall and tests stay hermetic with `CLAUDE_CONFIG_DIR`, not Mkrew's config folder. The command string is `"<mk>" statusline` with the path quoted, because Claude runs it through a shell and the path may hold spaces.
7. **Shell for the chained command:** `cmd /C` on Windows, `sh -c` elsewhere. Claude Code on Windows may run status lines through Git Bash, so a status line that needs bash syntax can fail; it is in HUMAN_CHECKS.
8. **No "View menu" exists** in the app. The toggle is the key, the palette entry `View: Show/hide widget bar` and the bar's own menu; Settings also has the switch.
9. **No "Integrations page" exists;** Integrations is a group of pages. The limits click opens the first page of that group (`INTEGRATIONS_TAB`, Jira).
10. **`step_file(1)` is private and does not wrap** (it stops at the last file, and in the History steps commits). A new `git_view::Key::NextWrapping` wraps in the changes list and behaves as ↓ in the History.
11. **Where the widget list lives:** the pure model is `crates/mkrew-ui/src/widget_bar.rs` (library, headless tests, like `redraw.rs` and `usage.rs`); `shell/widget_bar.rs` only draws and acts. The spec puts both in the shell file.
12. **"At least one client subscribed"** counts every connection that said `hello` (including `mk web`'s relay and a short `mk` call). An idle daemon with no connection does no sampling.
13. **Units:** 100 MB is 100 MiB, and "GB" in the label is 1024-based, like Task Manager.
14. **Validation the spec does not state:** the daemon refuses non-finite percentages and clamps to 0-100; the UI clamps for display.
15. **`chrono`** (`clock` feature only) becomes a direct dependency of `mkrew-ui` for the local reset time; it is already in `Cargo.lock`, so no new crate is built.
16. **Bar with nothing to show:** when every widget is hidden or absent the bar stays as an empty strip so a right-click can bring widgets back. Between two agents' limit groups the label shows `|`.
17. **Stale display:** an expired window disappears at the next redraw; the bar redraws on every `machine.stats` heartbeat (at most 10 s) and on any limits event, so no timer is added.
