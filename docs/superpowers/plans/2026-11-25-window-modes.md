# Window modes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Two window modes: today's single window with the workspace sidebar, and one OS window per workspace (no sidebar, panel on either side, a "need you" chip), with smooth switching both ways.

**Architecture:** One `Shell` entity serves every window.
The main window stops being special: every top-level window is a Space window (`HostId::Space(SpaceKey)`) whose thin root view (`SpaceWindow`) asks `Shell` to draw it, like detached windows (`DetachedWindow`) do today.
The shown workspace, the right panel and the scroll state move from `Shell` into the per-window `HostUi`, so several Space windows can show different workspaces at once; mode 1 keeps exactly one Space window.

**Tech Stack:** Rust 2024, gpui-pre 0.3.7, serde / serde_json / toml, `windows` crate (taskbar), insta not installed (no new snapshots).

**Spec:** `docs/superpowers/specs/2026-11-25-window-modes-design.md` (in this branch). Executors read it and this plan; the spec wins on behavior, this plan wins on names and order.

## Global Constraints

- Work only in the worktree `C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/window-modes` (branch `feat/window-modes`, base `master` 164d885). Use it as `cwd` for every command and give tool paths as absolute paths or paths starting with `.worktrees/window-modes/`. Never edit the main checkout or other worktrees.
- Commit with `git -c user.email=tungnv975@gmail.com commit -m "<Conventional Commit>"`, staging the task's files by explicit path. Never push, tag, rewrite history or add a co-author line.
- Cargo: `-j 4` on every command; set `CARGO_TARGET_DIR=C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target` (shared target; C: has about 40 GB free, do not create a new target folder). If an exe is locked, rename it to `*.locked-old.exe`. Never kill the installed daemon or the user's sessions.
- Final verify: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL). If it runs past 600 s or rustc crashes on mkrew-ui test builds (STATUS_STACK_BUFFER_OVERRUN): `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -j 4 -- -D warnings`, then `cargo test --workspace -j 2`. Intermediate tasks run only their focused commands plus fmt and clippy.
- ADR 0113 (master ends at 0112). No protocol change, no migration. Re-check the ADR number at merge.
- Settings keys, exact: `[ui] window_mode = "single" | "per_workspace"` (default `single`), `[ui] open_workspace = "ask" | "new_window" | "replace"` (default `ask`), `[ui] panel_side = "right" | "left"` (default `right`).
- New commands, exact ids and palette titles, no default keys: `split_windows` "Window: Open each workspace in its own window…", `combine_windows` "Window: Combine into one window", `toggle_panel_side` "Panel: Move to the other side".
- Mode 1 must look and behave exactly as on master; Tasks 5 and 6 change no behavior.
- New Space windows open at the asking window's size, each offset 32 px (`space::XXL`) right and down from the previous one.
- Mode 2 window title: `<workspace> - Mkrew`. Stripe: 3 px (`theme::WORKSPACE_STRIPE_WIDTH`) along the top in `theme.workspace_color(name)`, none without a color.
- UI rules (SPEC §14, skill `mkrew-ui-design`): colors, spacing (4/8/12/16/24/32), radius and font sizes only from `theme.rs`; hover, press and focus only through `shell/widgets.rs` widgets or `.pressable(Look::..)` + `.on_press(..)`, never `.hover(`, `.active(` or `.cursor_pointer()` elsewhere; dark and light themes; tooltips through `tooltip(..)`.
- No `unwrap`/`expect` outside tests unless provably infallible (comment why). No test runs a real agent or touches the network; no test reads source text. Docs: no em dash, one sentence per line in long Markdown. Never edit CHANGELOG.md.
- Clean cutover: `HostId::Main`, `MainWindow`, `Shell.workspace`, `Shell.main_window`, `main-window.json` writing and every obsolete helper are deleted, not aliased.

## Review Focus

Input classes the spec implies but no task's named tests cover, most likely first. Each line has a test or smoke step in the task in brackets.

- The same workspace opened twice before its window exists (double click in the switcher with "New window", Alt+3 pressed twice, a notification click while the split is opening windows). Expect: one window; the second request focuses the pending one. [Task 2 `open_target_counts_pending_windows`, Task 7 Step 9.2]
- Closing a Space window in mode 2 while it holds unsaved file edits, running agents or an open dialog. Expect: a non-last window closes without a question (edits stay in memory with the workspace, agents run on), its dialog, palette and menus close first; only the last window asks about unsaved edits, as master does. [Task 6 Step 5 `space_should_close`, Task 7 Step 9.5]
- A workspace removed or archived while it has a Space window, and a daemon reconnect where it disappeared. Expect: its window closes, or shows the empty state when it was the last one; no window stays on "Opening the workspace…". [Task 7 Step 7 `prune_spaces`, Step 9.6]
- Hand-edited settings: `window_mode = "tabs"`, a valid mode while the picker is open, the file switching modes twice within one tick. Expect: an unknown value shows the settings error and changes nothing; a valid change runs the flow once. [Task 1 parse tests, Task 8 Step 7.5]
- Saved windows files: version 1 with `main-window.json`, version 2 whose workspace is gone, a corrupt file, bounds on an unplugged screen, a newer version. Expect: always at least one Space window, on a connected screen, never a crash. [Task 3 tests, Task 6 Step 5 restore]

---

## Task order

| # | Task | Depends on |
|---|---|---|
| 1 | Settings keys in core | - |
| 2 | Pure model `window_mode.rs` | 1 |
| 3 | Saved layout version 2 (`detached.rs`) | - |
| 4 | Panel side (both modes) | 1 |
| 5 | Per-window workspace state (`HostId::Space`), mode 1 unchanged | 3, 4 |
| 6 | `SpaceWindow`: `Shell` off the window, restore from layout version 2, mode 1 unchanged | 5 |
| 7 | Mode 2: open, route, ask, close, title, stripe | 2, 6 |
| 8 | Split and combine flows | 7 |
| 9 | "Need you" chip | 7 |
| 10 | ADR, SPEC, HUMAN_CHECKS, final verify | 1-9 |

---

### Task 1: Settings keys in core

**Files:**
- Modify: `crates/mkrew-core/src/settings.rs` (enums near `SidebarMode` at 339; `UiSettings` 432-480; `Default` 482-504; template `[ui]` 655-685; test `full_file_parses` ~905-990; new tests next to `ui_sidebar_parses_defaults_to_full_and_round_trips_through_the_file` ~1382)
- Modify: `crates/mkrew-core/src/settings_fields.rs` (`SETTINGS`, new rows; tests)

**Interfaces:**
- Produces: `mkrew_core::settings::{WindowMode, OpenWorkspace, PanelSide}` (each `Copy + Default + Serialize + Deserialize` with `fn name(self) -> &'static str`; `PanelSide::other`), `UiSettings::{window_mode, open_workspace, panel_side}`.

- [ ] **Step 1: Write the failing tests** in `settings.rs` `mod tests`, next to the sidebar test:

```rust
    #[test]
    fn window_keys_parse_default_and_round_trip_through_the_file() {
        let ui = |src: &str| parse(Path::new("s.toml"), src).unwrap().ui;
        let defaults = ui("");
        assert_eq!(defaults.window_mode, WindowMode::Single);
        assert_eq!(defaults.open_workspace, OpenWorkspace::Ask);
        assert_eq!(defaults.panel_side, PanelSide::Right);
        assert_eq!(
            ui("[ui]\nwindow_mode = \"per_workspace\"").window_mode,
            WindowMode::PerWorkspace
        );
        assert_eq!(
            ui("[ui]\nopen_workspace = \"new_window\"").open_workspace,
            OpenWorkspace::NewWindow
        );
        assert_eq!(ui("[ui]\npanel_side = \"left\"").panel_side, PanelSide::Left);
        assert!(parse_err("[ui]\nwindow_mode = \"tabs\"").contains("unknown variant `tabs`"));
        assert!(parse_err("[ui]\nopen_workspace = \"never\"").contains("unknown variant"));
        assert!(parse_err("[ui]\npanel_side = \"top\"").contains("unknown variant"));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, template()).unwrap();
        for mode in [WindowMode::PerWorkspace, WindowMode::Single] {
            write_ui_value(&path, "window_mode", &format!("\"{}\"", mode.name())).unwrap();
            assert_eq!(load(&path).unwrap().ui.window_mode, mode);
        }
        for pref in [OpenWorkspace::NewWindow, OpenWorkspace::Replace, OpenWorkspace::Ask] {
            write_ui_value(&path, "open_workspace", &format!("\"{}\"", pref.name())).unwrap();
            assert_eq!(load(&path).unwrap().ui.open_workspace, pref);
        }
        for side in [PanelSide::Left, PanelSide::Right] {
            write_ui_value(&path, "panel_side", &format!("\"{}\"", side.name())).unwrap();
            assert_eq!(load(&path).unwrap().ui.panel_side, side);
        }
        assert_eq!(PanelSide::Left.other(), PanelSide::Right);
    }
```

In `settings_fields.rs` tests add:

```rust
    #[test]
    fn window_settings_are_visible_choices_with_their_defaults() {
        let mode = setting("ui.window_mode");
        assert_eq!(mode.kind, Kind::Choice(&["single", "per_workspace"]));
        assert_eq!(mode.current(&Settings::default()), "single");
        assert_eq!(mode.page, Page::General);
        assert!(!mode.hidden);
        let open = setting("ui.open_workspace");
        assert_eq!(open.kind, Kind::Choice(&["ask", "new_window", "replace"]));
        assert_eq!(open.current(&Settings::default()), "ask");
        let side = setting("ui.panel_side");
        assert_eq!(side.kind, Kind::Choice(&["right", "left"]));
        assert_eq!(side.current(&Settings::default()), "right");
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p mkrew-core -j 4 window_`
Expected: compile errors (`WindowMode` not found).

- [ ] **Step 3: Implement.** After `MdMode` in `settings.rs`:

```rust
/// `[ui] window_mode` (ADR 0113): one window with every workspace in the sidebar, or one
/// window per workspace.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowMode {
    #[default]
    Single,
    PerWorkspace,
}

impl WindowMode {
    pub fn name(self) -> &'static str {
        match self {
            WindowMode::Single => "single",
            WindowMode::PerWorkspace => "per_workspace",
        }
    }
}

/// `[ui] open_workspace` (ADR 0113): what opening a workspace without a window does while
/// `window_mode` is `per_workspace`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenWorkspace {
    #[default]
    Ask,
    NewWindow,
    Replace,
}

impl OpenWorkspace {
    pub fn name(self) -> &'static str {
        match self {
            OpenWorkspace::Ask => "ask",
            OpenWorkspace::NewWindow => "new_window",
            OpenWorkspace::Replace => "replace",
        }
    }
}

/// `[ui] panel_side` (ADR 0113): the Files / Session / Git panel sits right or left of the
/// panes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PanelSide {
    #[default]
    Right,
    Left,
}

impl PanelSide {
    pub fn name(self) -> &'static str {
        match self {
            PanelSide::Right => "right",
            PanelSide::Left => "left",
        }
    }

    pub fn other(self) -> Self {
        match self {
            PanelSide::Right => PanelSide::Left,
            PanelSide::Left => PanelSide::Right,
        }
    }
}
```

In `UiSettings`, after `close_to_tray`:

```rust
    /// `[ui] window_mode` (ADR 0113).
    pub window_mode: WindowMode,
    /// `[ui] open_workspace` (ADR 0113).
    pub open_workspace: OpenWorkspace,
    /// `[ui] panel_side` (ADR 0113).
    pub panel_side: PanelSide,
```

In `Default`: `window_mode: WindowMode::Single, open_workspace: OpenWorkspace::Ask, panel_side: PanelSide::Right,` after `close_to_tray: true,`.
In `full_file_parses` add the same three fields.
In the template, add after the "Close to tray" comment line:

```
     # Windows: single (one window, workspaces in the sidebar) or per_workspace (a window each).\n\
     # Open workspace (per_workspace): ask, new_window or replace the current window's workspace.\n\
     # Panel side: the Files / Session / Git panel on the right or the left of the panes.\n\
```

and after `# close_to_tray = true\n\`:

```
     # window_mode = \"single\"\n\
     # open_workspace = \"ask\"\n\
     # panel_side = \"right\"\n\
```

In `SETTINGS` (`settings_fields.rs`), right after the `close_to_tray` row (end of group "System"), add two contiguous groups:

```rust
    row(
        General,
        "Windows",
        "ui",
        "window_mode",
        Choice(&["single", "per_workspace"]),
        "Windows",
        "Single: one window, every workspace in the sidebar. Per workspace: each workspace has its own window.",
    ),
    row(
        General,
        "Windows",
        "ui",
        "open_workspace",
        Choice(&["ask", "new_window", "replace"]),
        "Open workspace",
        "With a window per workspace: ask each time, open a new window, or replace the workspace of the current one.",
    ),
    row(
        General,
        "Layout",
        "ui",
        "panel_side",
        Choice(&["right", "left"]),
        "Panel side",
        "Where the Files / Session / Git panel sits next to the panes.",
    ),
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-core -j 4 settings`
Expected: PASS, including `the_template_parses_to_the_defaults_commented_or_not`, `every_template_key_has_exactly_one_setting`, `every_setting_has_a_group_and_a_groups_rows_are_contiguous`, `writing_each_default_reads_it_back_and_keeps_comments`.

- [ ] **Step 5: fmt, clippy, commit**

```bash
cargo fmt --all && cargo clippy -p mkrew-core --all-targets -j 4 -- -D warnings
git add crates/mkrew-core/src/settings.rs crates/mkrew-core/src/settings_fields.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(core): window_mode, open_workspace and panel_side settings"
```

---

### Task 2: Pure model `window_mode.rs`

**Files:**
- Create: `crates/mkrew-ui/src/window_mode.rs`
- Create: `crates/mkrew-ui/src/window_mode/tests.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (add `pub mod window_mode;` before `pub mod window_place;`)
- Modify: `crates/mkrew-ui/src/detached.rs` (add `SpaceKey` only; `HostId` changes in Task 5)

**Interfaces:**
- Consumes: `mkrew_core::settings::{WindowMode, OpenWorkspace}` (Task 1), `inbox::Entry`, `sidebar::status_display`, `text_box::Input`, `task_dialog::{DialogKey, dialog_key}`.
- Produces (all `pub` in `mkrew_ui::window_mode`):
  - `struct SplitPlan { pub keep: Ulid, pub open: Vec<Ulid> }`
  - `fn plan_split(listed: &[Ulid], ticked: &[Ulid], shown: Option<Ulid>) -> Option<SplitPlan>`
  - `fn plan_combine(keep: SpaceKey, spaces: &[SpaceKey]) -> Vec<SpaceKey>`
  - `enum OpenTarget { Switch, Focus(SpaceKey), New, Replace, Ask }`
  - `fn open_target(workspace: Ulid, mode: WindowMode, pref: OpenWorkspace, ctrl: bool, spaces: &[(SpaceKey, Option<Ulid>)]) -> OpenTarget`
  - `enum Choice { NewWindow, ThisWindow }` with `fn remembered(self) -> OpenWorkspace`
  - `struct PickRow { pub id: Ulid, pub name: String, pub color: Option<String>, pub working: usize, pub asking: usize }`
  - `struct WorkspacePicker` with `new`, `rows`, `visible`, `key`, `insert`, `toggle`, `toggle_all_shown`, `is_ticked`, `cursor`, `in_list`, `filter`, `filter_input`, `confirming`, `plan`, `button_label`, `can_confirm`, `confirm`, `confirmation`, `back`
  - `enum PickOutcome { Edited, Close, Done(SplitPlan), Paste }`
  - `struct NeedYou { pub count: usize, pub label: String, pub short: String, pub rows: Vec<NeedRow> }`, `struct NeedRow { pub session: Ulid, pub project: Ulid, pub text: String }`
  - `fn need_you(entries: &[inbox::Entry], shown: Option<Ulid>) -> Option<NeedYou>`
- Produces in `mkrew_ui::detached`: `pub struct SpaceKey(pub u64)` (`Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord`).

- [ ] **Step 1: Add `SpaceKey`** to `detached.rs` under `WindowKey`:

```rust
/// A Space window (ADR 0113): a top-level window with a workspace, local to this UI process
/// (never reused within it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpaceKey(pub u64);
```

- [ ] **Step 2: Write the failing tests** in `window_mode/tests.rs`:

```rust
use gpui::Keystroke;
use mkrew_core::settings::{OpenWorkspace, WindowMode};
use mkrew_protocol::{SessionState, StateSource};
use ulid::Ulid;

use super::*;
use crate::detached::SpaceKey;
use crate::inbox::Entry;

fn ids<const N: usize>() -> [Ulid; N] {
    std::array::from_fn(|_| Ulid::new())
}

fn press(picker: &mut WorkspacePicker, keys: &str) -> PickOutcome {
    picker.key(&Keystroke::parse(keys).unwrap())
}

fn rows(names: &[&str]) -> Vec<PickRow> {
    names
        .iter()
        .map(|name| PickRow {
            id: Ulid::new(),
            name: (*name).to_owned(),
            color: None,
            working: 0,
            asking: 0,
        })
        .collect()
}

#[test]
fn split_keeps_the_shown_workspace_when_ticked_else_the_first_ticked() {
    let [a, b, c] = ids();
    let listed = [a, b, c];
    assert_eq!(
        plan_split(&listed, &[c, b], Some(b)),
        Some(SplitPlan { keep: b, open: vec![c] })
    );
    assert_eq!(
        plan_split(&listed, &[c, b], Some(a)),
        Some(SplitPlan { keep: b, open: vec![c] }),
        "list order, not tick order"
    );
    assert_eq!(plan_split(&listed, &[], Some(a)), None);
    assert_eq!(
        plan_split(&listed, &[a], None),
        Some(SplitPlan { keep: a, open: vec![] })
    );
}

#[test]
fn combine_closes_every_other_space() {
    let spaces = [SpaceKey(0), SpaceKey(3), SpaceKey(7)];
    assert_eq!(plan_combine(SpaceKey(3), &spaces), vec![SpaceKey(0), SpaceKey(7)]);
    assert_eq!(plan_combine(SpaceKey(0), &[SpaceKey(0)]), Vec::<SpaceKey>::new());
}

#[test]
fn open_target_follows_mode_window_preference_and_ctrl() {
    let [w, other] = ids();
    let none = [(SpaceKey(0), Some(other))];
    let shown = [(SpaceKey(0), Some(other)), (SpaceKey(4), Some(w))];
    use OpenTarget::*;
    use OpenWorkspace::{Ask as AskPref, NewWindow, Replace as ReplacePref};
    use WindowMode::*;
    assert_eq!(open_target(w, Single, AskPref, false, &none), Switch);
    assert_eq!(open_target(w, Single, NewWindow, true, &none), Switch);
    assert_eq!(open_target(w, PerWorkspace, AskPref, false, &shown), Focus(SpaceKey(4)));
    assert_eq!(open_target(w, PerWorkspace, ReplacePref, true, &shown), Focus(SpaceKey(4)));
    assert_eq!(open_target(w, PerWorkspace, AskPref, false, &none), Ask);
    assert_eq!(open_target(w, PerWorkspace, AskPref, true, &none), New);
    assert_eq!(open_target(w, PerWorkspace, NewWindow, false, &none), New);
    assert_eq!(open_target(w, PerWorkspace, NewWindow, true, &none), Replace);
    assert_eq!(open_target(w, PerWorkspace, ReplacePref, false, &none), Replace);
    assert_eq!(open_target(w, PerWorkspace, ReplacePref, true, &none), New);
}

#[test]
fn open_target_counts_pending_windows() {
    // A window being opened is already listed (its HostUi exists before the OS window):
    // a second request focuses it instead of opening another.
    let w = Ulid::new();
    let pending = [(SpaceKey(9), Some(w))];
    assert_eq!(
        open_target(w, WindowMode::PerWorkspace, OpenWorkspace::NewWindow, false, &pending),
        OpenTarget::Focus(SpaceKey(9))
    );
}

#[test]
fn a_remembered_choice_is_the_matching_preference() {
    assert_eq!(Choice::NewWindow.remembered(), OpenWorkspace::NewWindow);
    assert_eq!(Choice::ThisWindow.remembered(), OpenWorkspace::Replace);
}

#[test]
fn the_picker_starts_on_the_shown_workspace_ticked() {
    let list = rows(&["api", "web", "docs"]);
    let picker = WorkspacePicker::new(list.clone(), Some(list[1].id));
    assert!(picker.is_ticked(list[1].id));
    assert!(!picker.is_ticked(list[0].id));
    assert_eq!(picker.button_label(), "Switch");
    assert!(picker.can_confirm());
}

#[test]
fn arrows_hand_the_keyboard_to_the_list_and_space_ticks_there() {
    let list = rows(&["api", "web", "docs"]);
    let mut picker = WorkspacePicker::new(list.clone(), None);
    assert_eq!(picker.button_label(), "Pick a workspace");
    assert!(!picker.can_confirm());
    press(&mut picker, "space");
    assert_eq!(picker.filter(), " ", "space types while the filter has the keyboard");
    press(&mut picker, "backspace");
    press(&mut picker, "down");
    assert!(picker.in_list());
    assert_eq!(picker.cursor(), 1);
    press(&mut picker, "space");
    assert!(picker.is_ticked(list[1].id));
    press(&mut picker, "down");
    press(&mut picker, "down");
    assert_eq!(picker.cursor(), 2, "no wrap");
    press(&mut picker, "space");
    assert_eq!(picker.button_label(), "Open 2 windows");
    press(&mut picker, "w");
    assert!(!picker.in_list(), "typing takes the keyboard back");
    assert_eq!(picker.filter(), "w");
}

#[test]
fn the_filter_matches_every_word_and_keeps_hidden_ticks() {
    let list = rows(&["api server", "web", "api docs"]);
    let mut picker = WorkspacePicker::new(list.clone(), Some(list[1].id));
    picker.insert("api d");
    let shown: Vec<&str> = picker.visible().iter().map(|row| row.name.as_str()).collect();
    assert_eq!(shown, ["api docs"]);
    assert!(picker.is_ticked(list[1].id), "a hidden tick stays");
}

#[test]
fn alt_a_ticks_every_shown_row_then_unticks_them() {
    let list = rows(&["api", "web", "docs"]);
    let mut picker = WorkspacePicker::new(list.clone(), Some(list[0].id));
    press(&mut picker, "alt-a");
    assert!(list.iter().all(|row| picker.is_ticked(row.id)));
    press(&mut picker, "alt-a");
    assert!(list.iter().all(|row| !picker.is_ticked(row.id)));
}

#[test]
fn enter_with_several_ticked_asks_first_then_splits() {
    let list = rows(&["api", "web", "docs"]);
    let mut picker = WorkspacePicker::new(list.clone(), Some(list[0].id));
    picker.toggle(list[1].id);
    picker.toggle(list[2].id);
    assert_eq!(press(&mut picker, "enter"), PickOutcome::Edited);
    assert!(picker.confirming());
    let name = |id: Ulid| list.iter().find(|row| row.id == id).unwrap().name.clone();
    assert_eq!(
        picker.confirmation(name),
        "api stays in this window. web and docs each open in a new window."
    );
    press(&mut picker, "escape");
    assert!(!picker.confirming(), "Esc on the confirmation goes back");
    press(&mut picker, "enter");
    assert_eq!(
        press(&mut picker, "enter"),
        PickOutcome::Done(SplitPlan { keep: list[0].id, open: vec![list[1].id, list[2].id] })
    );
}

#[test]
fn enter_with_one_ticked_splits_at_once_and_with_none_does_nothing() {
    let list = rows(&["api", "web"]);
    let mut one = WorkspacePicker::new(list.clone(), Some(list[1].id));
    assert_eq!(
        press(&mut one, "enter"),
        PickOutcome::Done(SplitPlan { keep: list[1].id, open: vec![] })
    );
    let mut none = WorkspacePicker::new(list, None);
    assert_eq!(press(&mut none, "enter"), PickOutcome::Edited);
    assert!(!none.confirming());
    assert_eq!(press(&mut none, "escape"), PickOutcome::Close);
}

#[test]
fn confirmation_names_one_two_and_three_new_windows() {
    let list = rows(&["a", "b", "c", "d"]);
    let name = |id: Ulid| list.iter().find(|row| row.id == id).unwrap().name.clone();
    let mut picker = WorkspacePicker::new(list.clone(), Some(list[0].id));
    picker.toggle(list[1].id);
    assert_eq!(picker.confirmation(name), "a stays in this window. b opens in a new window.");
    picker.toggle(list[2].id);
    picker.toggle(list[3].id);
    assert_eq!(
        picker.confirmation(name),
        "a stays in this window. b, c and d each open in a new window."
    );
}

fn entry(project: Ulid, workspace: &str, title: &str, at: i64) -> Entry {
    Entry {
        session_id: Ulid::new(),
        project_id: project,
        workspace: workspace.to_owned(),
        agent: "claude".to_owned(),
        title: Some(title.to_owned()),
        state: SessionState::NeedsInput,
        state_source: StateSource::Hook,
        state_changed_at: at,
    }
}

#[test]
fn need_you_counts_other_workspaces_only_newest_first() {
    let [api, web, docs] = ids();
    let entries = [
        entry(api, "api", "fix login", 1),
        entry(web, "web", "tests", 2),
        entry(web, "web", "lint", 5),
    ];
    let one = need_you(&entries, Some(api)).unwrap();
    assert_eq!(one.count, 2);
    assert_eq!(one.label, "2 need you · web");
    assert_eq!(one.short, "2");
    assert_eq!(one.rows[0].text, "web · lint · needs input");
    let mixed = [entries[0].clone(), entry(docs, "docs", "x", 3), entries[1].clone()];
    assert_eq!(need_you(&mixed, Some(web)).unwrap().label, "2 need you · 2 workspaces");
    assert_eq!(need_you(&entries[..1], Some(api)), None);
}
```

- [ ] **Step 3: Run them to see them fail**

Run: `cargo test -p mkrew-ui --lib -j 4 window_mode`
Expected: compile errors (module not found).

- [ ] **Step 4: Implement `window_mode.rs`**

```rust
//! Window modes (ADR 0113): the pure decisions behind one window per workspace. Which window
//! keeps which workspace on a split, which windows close on a combine, where opening a
//! workspace goes, the "Open workspaces in windows" picker and the "need you" chip. Pure and
//! tested headlessly; `shell/window_mode.rs` draws and runs them.

use gpui::Keystroke;
use mkrew_core::settings::{OpenWorkspace, WindowMode};
use ulid::Ulid;

use crate::detached::SpaceKey;
use crate::inbox::Entry;
use crate::sidebar;
use crate::task_dialog::{DialogKey, dialog_key};
use crate::text_box::Input;

#[cfg(test)]
mod tests;

/// What a split does: the asking window shows `keep`, each of `open` gets a new window, in
/// this order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitPlan {
    pub keep: Ulid,
    pub open: Vec<Ulid>,
}

/// The split of the `ticked` workspaces (any order) of `listed` (list order). The asking
/// window keeps `shown` when it is ticked, else the first ticked one. `None` with none ticked.
pub fn plan_split(listed: &[Ulid], ticked: &[Ulid], shown: Option<Ulid>) -> Option<SplitPlan> {
    let ordered: Vec<Ulid> = (listed.iter().copied())
        .filter(|id| ticked.contains(id))
        .collect();
    let keep = shown
        .filter(|id| ordered.contains(id))
        .or_else(|| ordered.first().copied())?;
    let open = ordered.into_iter().filter(|id| *id != keep).collect();
    Some(SplitPlan { keep, open })
}

/// The Space windows a combine into `keep` closes.
pub fn plan_combine(keep: SpaceKey, spaces: &[SpaceKey]) -> Vec<SpaceKey> {
    spaces.iter().copied().filter(|key| *key != keep).collect()
}

/// Where opening a workspace goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenTarget {
    /// Mode 1: the single window switches to it.
    Switch,
    /// Its window (open or being opened) comes forward.
    Focus(SpaceKey),
    New,
    /// The asking window shows it instead of its workspace.
    Replace,
    Ask,
}

/// Where `workspace` opens. `spaces` lists every Space window with its workspace, the ones
/// being opened included. Ctrl takes the other way of a saved choice and skips the question.
pub fn open_target(
    workspace: Ulid,
    mode: WindowMode,
    pref: OpenWorkspace,
    ctrl: bool,
    spaces: &[(SpaceKey, Option<Ulid>)],
) -> OpenTarget {
    if mode == WindowMode::Single {
        return OpenTarget::Switch;
    }
    if let Some((key, _)) = spaces.iter().find(|(_, shown)| *shown == Some(workspace)) {
        return OpenTarget::Focus(*key);
    }
    match (pref, ctrl) {
        (OpenWorkspace::Ask, false) => OpenTarget::Ask,
        (OpenWorkspace::Ask | OpenWorkspace::Replace, true) | (OpenWorkspace::NewWindow, false) => {
            OpenTarget::New
        }
        (OpenWorkspace::NewWindow, true) | (OpenWorkspace::Replace, false) => OpenTarget::Replace,
    }
}

/// A button of the "Open <workspace>" question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    NewWindow,
    ThisWindow,
}

impl Choice {
    /// What "Don't ask again" saves.
    pub fn remembered(self) -> OpenWorkspace {
        match self {
            Choice::NewWindow => OpenWorkspace::NewWindow,
            Choice::ThisWindow => OpenWorkspace::Replace,
        }
    }
}

/// One workspace row of the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickRow {
    pub id: Ulid,
    pub name: String,
    pub color: Option<String>,
    /// Working agents and sessions asking for input, as the sidebar card shows them.
    pub working: usize,
    pub asking: usize,
}

/// What a key press in the picker means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickOutcome {
    Edited,
    Close,
    Done(SplitPlan),
    /// Ctrl+V: the caller inserts the clipboard text.
    Paste,
}

/// The open "Open workspaces in windows" picker. Keys follow the Broadcast dialog: typing
/// edits the filter; ↑ / ↓ move the highlight and hand the keyboard to the list, where Space
/// ticks; any other key gives it back to the filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePicker {
    rows: Vec<PickRow>,
    ticked: Vec<Ulid>,
    shown: Option<Ulid>,
    filter: Input,
    /// Index into `visible()`.
    cursor: usize,
    in_list: bool,
    confirming: bool,
}

impl WorkspacePicker {
    /// `rows` in list order; `shown` (the asking window's workspace) starts ticked.
    pub fn new(rows: Vec<PickRow>, shown: Option<Ulid>) -> Self {
        let ticked = (shown.into_iter())
            .filter(|id| rows.iter().any(|row| row.id == *id))
            .collect();
        Self {
            rows,
            ticked,
            shown,
            filter: Input::default(),
            cursor: 0,
            in_list: false,
            confirming: false,
        }
    }

    pub fn rows(&self) -> &[PickRow] {
        &self.rows
    }

    pub fn filter(&self) -> &str {
        self.filter.as_str()
    }

    pub fn filter_input(&self) -> &Input {
        &self.filter
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn in_list(&self) -> bool {
        self.in_list
    }

    pub fn confirming(&self) -> bool {
        self.confirming
    }

    /// The rows whose name holds every word of the filter (any case).
    pub fn visible(&self) -> Vec<&PickRow> {
        let words: Vec<String> = (self.filter.split_whitespace())
            .map(str::to_lowercase)
            .collect();
        (self.rows.iter())
            .filter(|row| {
                let name = row.name.to_lowercase();
                words.iter().all(|word| name.contains(word.as_str()))
            })
            .collect()
    }

    pub fn is_ticked(&self, id: Ulid) -> bool {
        self.ticked.contains(&id)
    }

    /// Ticks or unticks `id` and moves the highlight there when it is shown.
    pub fn toggle(&mut self, id: Ulid) {
        if let Some(at) = self.ticked.iter().position(|ticked| *ticked == id) {
            self.ticked.remove(at);
        } else {
            self.ticked.push(id);
        }
        if let Some(at) = self.visible().iter().position(|row| row.id == id) {
            self.cursor = at;
        }
        self.confirming = false;
    }

    /// Ticks every shown row, or unticks them when all are ticked.
    pub fn toggle_all_shown(&mut self) {
        let shown: Vec<Ulid> = self.visible().iter().map(|row| row.id).collect();
        if shown.iter().all(|id| self.ticked.contains(id)) {
            self.ticked.retain(|id| !shown.contains(id));
        } else {
            for id in shown {
                if !self.ticked.contains(&id) {
                    self.ticked.push(id);
                }
            }
        }
        self.confirming = false;
    }

    pub fn plan(&self) -> Option<SplitPlan> {
        let listed: Vec<Ulid> = self.rows.iter().map(|row| row.id).collect();
        plan_split(&listed, &self.ticked, self.shown)
    }

    pub fn can_confirm(&self) -> bool {
        !self.ticked.is_empty()
    }

    /// The primary button: what is missing, or what happens.
    pub fn button_label(&self) -> String {
        match self.ticked.len() {
            0 => "Pick a workspace".to_owned(),
            1 => "Switch".to_owned(),
            n => format!("Open {n} windows"),
        }
    }

    /// The confirmation step's sentence, naming workspaces with `name`.
    pub fn confirmation(&self, name: impl Fn(Ulid) -> String) -> String {
        let Some(plan) = self.plan() else {
            return String::new();
        };
        let kept = name(plan.keep);
        let names: Vec<String> = plan.open.iter().map(|id| name(*id)).collect();
        match names.as_slice() {
            [] => format!("{kept} stays in this window."),
            [one] => format!("{kept} stays in this window. {one} opens in a new window."),
            [rest @ .., last] => format!(
                "{kept} stays in this window. {} and {last} each open in a new window.",
                rest.join(", ")
            ),
        }
    }

    /// Back from the confirmation step.
    pub fn back(&mut self) {
        self.confirming = false;
    }

    /// Enter or the primary button: nothing with none ticked, the confirmation step first
    /// with several, else the plan.
    pub fn confirm(&mut self) -> PickOutcome {
        let several = self.ticked.len() > 1;
        if several && !self.confirming {
            self.confirming = true;
            return PickOutcome::Edited;
        }
        self.plan().map_or(PickOutcome::Edited, PickOutcome::Done)
    }

    pub fn key(&mut self, keystroke: &Keystroke) -> PickOutcome {
        let m = &keystroke.modifiers;
        if m.alt && !m.control && !m.platform && !m.shift && keystroke.key == "a" {
            self.toggle_all_shown();
            return PickOutcome::Edited;
        }
        if !m.modified() {
            let last = self.visible().len().saturating_sub(1);
            match keystroke.key.as_str() {
                "up" => {
                    self.cursor = self.cursor.saturating_sub(1);
                    self.in_list = true;
                    return PickOutcome::Edited;
                }
                "down" => {
                    self.cursor = (self.cursor + 1).min(last);
                    self.in_list = true;
                    return PickOutcome::Edited;
                }
                "space" if self.in_list => {
                    if let Some(id) = self.visible().get(self.cursor).map(|row| row.id) {
                        self.toggle(id);
                    }
                    return PickOutcome::Edited;
                }
                _ => {}
            }
        }
        self.in_list = false;
        match dialog_key(keystroke) {
            Some(DialogKey::Cancel) if self.confirming => {
                self.confirming = false;
                PickOutcome::Edited
            }
            Some(DialogKey::Cancel) => PickOutcome::Close,
            Some(DialogKey::Enter | DialogKey::Save) => self.confirm(),
            Some(DialogKey::Paste) => PickOutcome::Paste,
            Some(DialogKey::Insert(text)) => {
                self.insert(&text);
                PickOutcome::Edited
            }
            Some(DialogKey::Copy | DialogKey::SwitchField) | None => PickOutcome::Edited,
            Some(key) => {
                if self.filter.edit(&key) {
                    self.cursor = 0;
                }
                PickOutcome::Edited
            }
        }
    }

    /// Typed or pasted filter text (one line).
    pub fn insert(&mut self, text: &str) {
        let line: String = text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
        self.filter.insert(&line);
        self.cursor = 0;
        self.confirming = false;
    }
}

/// One session of the "need you" menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeedRow {
    pub session: Ulid,
    pub project: Ulid,
    /// `<workspace> · <title> · <state>`.
    pub text: String,
}

/// The chip of a Space window in mode 2: the sessions of other workspaces that need the
/// user ([`crate::inbox::needs_you`], as listed by [`crate::inbox::entries`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeedYou {
    pub count: usize,
    /// `N need you · web`, or `N need you · M workspaces`.
    pub label: String,
    /// `N`: what stays when the title bar runs out of room.
    pub short: String,
    /// Newest first.
    pub rows: Vec<NeedRow>,
}

/// `None` while no other workspace than `shown` needs the user.
pub fn need_you(entries: &[Entry], shown: Option<Ulid>) -> Option<NeedYou> {
    let mut others: Vec<&Entry> = (entries.iter())
        .filter(|entry| Some(entry.project_id) != shown)
        .collect();
    if others.is_empty() {
        return None;
    }
    others.sort_by_key(|entry| std::cmp::Reverse(entry.state_changed_at));
    let mut workspaces: Vec<Ulid> = others.iter().map(|entry| entry.project_id).collect();
    workspaces.sort();
    workspaces.dedup();
    let count = others.len();
    let place = match workspaces.len() {
        1 => others[0].workspace.clone(),
        n => format!("{n} workspaces"),
    };
    let rows = (others.iter())
        .map(|entry| NeedRow {
            session: entry.session_id,
            project: entry.project_id,
            text: format!(
                "{} · {} · {}",
                entry.workspace,
                entry.title.as_deref().unwrap_or(&entry.agent),
                sidebar::status_display(entry.state).label
            ),
        })
        .collect();
    Some(NeedYou {
        count,
        label: format!("{count} need you · {place}"),
        short: count.to_string(),
        rows,
    })
}
```

`Input` derefs to `str` (`split_whitespace` works on it); `Input::edit` returns whether the text changed.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p mkrew-ui --lib -j 4 window_mode`
Expected: PASS (13 tests).

- [ ] **Step 6: fmt, clippy, commit**

```bash
cargo fmt --all && cargo clippy -p mkrew-ui --lib --tests -j 4 -- -D warnings
git add crates/mkrew-ui/src/window_mode.rs crates/mkrew-ui/src/window_mode/tests.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/detached.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): pure window mode model (split, combine, open target, picker, need you)"
```

---

### Task 3: Saved layout version 2 (`detached.rs`)

**Files:**
- Modify: `crates/mkrew-ui/src/detached.rs` (saved formats 342-526)
- Modify: `crates/mkrew-ui/src/detached/tests.rs`
- Modify: `crates/mkrew-ui/src/shell/hosts.rs` (`save_windows`, `start_restore`: adapt to `Layout`)

**Interfaces:**
- Produces:
  - `pub struct SavedSpace { pub workspace: Ulid, pub bounds: SavedBounds, pub maximized: bool, pub fullscreen: bool }` (`Debug, Clone, PartialEq, Serialize, Deserialize`)
  - `pub struct Layout { pub spaces: Vec<SavedSpace>, pub windows: Vec<SavedWindow> }` (`Debug, Clone, PartialEq, Default`)
  - `pub fn to_json(layout: &Layout) -> String`, `pub fn from_json(text: &str) -> Result<Layout, String>` (versions 1 and 2), `pub fn save(path: &Path, layout: &Layout) -> io::Result<()>`
  - `pub fn load_layout(windows_path: &Path, legacy_main: &Path) -> Layout`: version 2 as is; without `spaces`, the legacy `main-window.json` frame becomes one `SavedSpace` with `workspace: Ulid::nil()` (unknown: the caller picks one); a missing or bad windows file gives `Layout::default()` plus that frame.
- Keeps `SavedFrame`, `load_frame`, `main_file` for the legacy read; `save_frame` is deleted in Task 6.

- [ ] **Step 1: Write the failing tests** in `detached/tests.rs`:

```rust
fn space(workspace: Ulid) -> SavedSpace {
    SavedSpace {
        workspace,
        bounds: SavedBounds { x: 10.0, y: 20.0, width: 1400.0, height: 900.0 },
        maximized: true,
        fullscreen: false,
    }
}

#[test]
fn a_version_2_layout_round_trips() {
    let tab = DockTab::Terminal { project: Ulid::new(), tab_id: Ulid::new() };
    let layout = Layout {
        spaces: vec![space(Ulid::new()), space(Ulid::new())],
        windows: vec![SavedWindow {
            bounds: SavedBounds { x: 0.0, y: 0.0, width: 600.0, height: 400.0 },
            maximized: false,
            fullscreen: false,
            tabs: vec![tab],
            active: 0,
        }],
    };
    let json = to_json(&layout);
    assert!(json.contains("\"version\":2"), "{json}");
    assert_eq!(from_json(&json), Ok(layout));
}

#[test]
fn a_version_1_file_reads_as_windows_without_spaces() {
    let tab = DockTab::Terminal { project: Ulid::new(), tab_id: Ulid::new() };
    let v1 = format!(
        r#"{{"version":1,"windows":[{{"bounds":{{"x":1,"y":2,"width":300,"height":200}},"tabs":[{}],"active":0}}]}}"#,
        serde_json::to_string(&tab).unwrap()
    );
    let layout = from_json(&v1).unwrap();
    assert!(layout.spaces.is_empty());
    assert_eq!(layout.windows[0].tabs, vec![tab]);
}

#[test]
fn a_newer_version_is_refused_and_unknown_fields_are_ignored() {
    assert!(from_json(r#"{"version":3,"windows":[]}"#).is_err());
    let w = Ulid::new();
    let text = format!(
        r#"{{"version":2,"spaces":[{{"workspace":"{w}","bounds":{{"x":0,"y":0,"width":800,"height":600}},"zoom":2}}],"windows":[],"later":true}}"#
    );
    assert_eq!(from_json(&text).unwrap().spaces[0].workspace, w);
}

#[test]
fn load_layout_takes_the_legacy_main_frame_when_spaces_are_missing() {
    let dir = tempfile::tempdir().unwrap();
    let windows = dir.path().join(WINDOWS_FILE);
    let legacy = dir.path().join(main_file(None));
    assert_eq!(load_layout(&windows, &legacy), Layout::default(), "nothing saved yet");
    std::fs::write(&legacy, serde_json::to_string(&frame()).unwrap()).unwrap();
    let layout = load_layout(&windows, &legacy);
    assert_eq!(layout.spaces.len(), 1);
    assert_eq!(layout.spaces[0].workspace, Ulid::nil());
    assert_eq!(layout.spaces[0].bounds, frame().bounds);
    assert!(layout.spaces[0].fullscreen);
    std::fs::write(&windows, "not json").unwrap();
    assert_eq!(load_layout(&windows, &legacy).spaces.len(), 1, "a bad file keeps the frame");
    let saved = Layout { spaces: vec![space(Ulid::new())], windows: vec![] };
    save(&windows, &saved).unwrap();
    assert_eq!(load_layout(&windows, &legacy), saved, "version 2 wins over the legacy frame");
}

#[test]
fn spaces_keep_their_order() {
    let [a, b] = [Ulid::new(), Ulid::new()];
    let layout = Layout { spaces: vec![space(a), space(b)], windows: vec![] };
    let back = from_json(&to_json(&layout)).unwrap();
    assert_eq!(back.spaces.last().map(|s| s.workspace), Some(b));
}
```

Update the existing tests that call `to_json(&windows)`, compare `from_json(..)` with `Ok(windows)` or call `save(&path, &windows)`: wrap the windows in `Layout { spaces: vec![], windows }` and compare `.windows`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p mkrew-ui --lib -j 4 detached`
Expected: compile errors (`Layout`, `SavedSpace` not found).

- [ ] **Step 3: Implement.** `const VERSION: u32 = 2;`, then:

```rust
/// One saved Space window (ADR 0113).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedSpace {
    /// `Ulid::nil()` when not known (read from the legacy `main-window.json`).
    pub workspace: Ulid,
    pub bounds: SavedBounds,
    #[serde(default)]
    pub maximized: bool,
    #[serde(default)]
    pub fullscreen: bool,
}

/// What `windows.json` keeps: the Space windows, the one focused last at the end, and the
/// detached windows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    pub spaces: Vec<SavedSpace>,
    pub windows: Vec<SavedWindow>,
}

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    #[serde(default)]
    spaces: Vec<SavedSpace>,
    windows: Vec<FileWindow>,
}

pub fn to_json(layout: &Layout) -> String {
    let file = File {
        version: VERSION,
        spaces: layout.spaces.clone(),
        windows: (layout.windows.iter())
            .map(|window| FileWindow {
                bounds: window.bounds.clone(),
                maximized: window.maximized,
                fullscreen: window.fullscreen,
                tabs: (window.tabs.iter())
                    .filter_map(|tab| serde_json::to_value(tab).ok())
                    .collect(),
                active: window.active,
            })
            .collect(),
    };
    // A struct of plain fields always serializes.
    serde_json::to_string(&file).unwrap_or_default()
}

/// Versions 1 (detached windows only) and 2; a newer version is refused.
pub fn from_json(text: &str) -> Result<Layout, String> {
    let file: File = serde_json::from_str(text).map_err(|error| error.to_string())?;
    if !(1..=VERSION).contains(&file.version) {
        return Err(format!("unknown version {}", file.version));
    }
    let windows = saved_windows(file.windows);
    Ok(Layout { spaces: file.spaces, windows })
}

/// The saved detached windows: unknown and repeated tabs dropped, empty windows skipped.
fn saved_windows(windows: Vec<FileWindow>) -> Vec<SavedWindow> {
    // The body of today's `from_json` after its version check, unchanged:
    // `(file.windows.into_iter()).filter_map(|window| { .. }).collect()` with `windows`
    // in place of `file.windows`.
}

pub fn save(path: &Path, layout: &Layout) -> std::io::Result<()> {
    write_atomic(path, &to_json(layout))
}

/// The saved layout; without saved Space windows, the legacy main window frame becomes the
/// one Space window (its workspace unknown). Read errors are logged and give what is left.
pub fn load_layout(windows_path: &Path, legacy_main: &Path) -> Layout {
    let mut layout = match std::fs::read_to_string(windows_path) {
        Ok(text) => from_json(&text).unwrap_or_else(|error| {
            tracing::warn!("ignoring the windows file: {error}");
            Layout::default()
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Layout::default(),
        Err(error) => {
            tracing::warn!("cannot read the windows file: {error}");
            Layout::default()
        }
    };
    if layout.spaces.is_empty()
        && let Some(frame) = load_frame(legacy_main)
    {
        layout.spaces.push(SavedSpace {
            workspace: Ulid::nil(),
            bounds: frame.bounds,
            maximized: frame.maximized,
            fullscreen: frame.fullscreen,
        });
    }
    layout
}
```

`saved_windows` gets today's loop verbatim (it is the only moved code; do not change it).
Adapt the two callers so the crate builds: `Shell::save_windows` saves `Layout { spaces: vec![], windows }` (Task 6 fills `spaces`); `start_restore` takes `from_json(&text)?.windows`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-ui --lib -j 4 detached`
Expected: PASS.

- [ ] **Step 5: Build, fmt, clippy, commit**

```bash
cargo build -p mkrew-ui -j 4
cargo fmt --all && cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings
git add crates/mkrew-ui/src/detached.rs crates/mkrew-ui/src/detached/tests.rs crates/mkrew-ui/src/shell/hosts.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): windows.json version 2 keeps the Space windows"
```

---

### Task 4: Panel side (both modes)

**Files:**
- Modify: `crates/mkrew-ui/src/inspector.rs` (`width_from_pointer`, tests)
- Modify: `crates/mkrew-ui/src/shell/inspector.rs` (grip ~251, frame ~395, `inspector_toggle` 196, new `toggle_panel_side`)
- Modify: `crates/mkrew-ui/src/shell/chrome.rs` (`render_main` 175-256)
- Modify: `crates/mkrew-ui/src/main.rs` (field `panel_side: PanelSide` next to `right_panel`)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (init from `ui.panel_side` near 419; `apply_settings_file` reads it)
- Modify: `crates/mkrew-ui/src/keymap.rs` (`Command::TogglePanelSide`: `all()` after `MoveTabToNewWindow`, `name()` = `"toggle_panel_side"`, the "Palette only" `String::new()` arm of `pc_keys`)
- Modify: `crates/mkrew-ui/src/palette.rs`, `palette/tests.rs` (`title` = `"Panel: Move to the other side"`, `available` = `context.workspace`)
- Modify: `crates/mkrew-ui/src/shell/sessions.rs` (`Shell::run` arm)

**Interfaces:**
- Consumes: `PanelSide` (Task 1).
- Produces: `inspector::width_from_pointer(window_width: f32, pointer_x: f32, side: PanelSide) -> f32`; `Shell::panel_side: PanelSide`; `Shell::toggle_panel_side(&mut self, cx)`.

- [ ] **Step 1: Write the failing test** in `inspector.rs` tests:

```rust
    #[test]
    fn a_left_panel_takes_its_width_from_its_own_left_edge() {
        use mkrew_core::settings::PanelSide;
        assert_eq!(width_from_pointer(1600.0, 1300.0, PanelSide::Right), 300.0);
        assert_eq!(width_from_pointer(1600.0, 300.0, PanelSide::Left), 300.0);
        assert_eq!(width_from_pointer(1600.0, 10.0, PanelSide::Left), theme::INSPECTOR_MIN_WIDTH);
        assert_eq!(
            width_from_pointer(1000.0, 900.0, PanelSide::Left),
            theme::INSPECTOR_MAX_WIDTH.min(500.0),
            "at most half the window"
        );
    }
```

  Add `PanelSide::Right` to the existing `dragging_the_edge_sets_the_width_from_the_pointer` calls.
  Palette test:

```rust
#[test]
fn the_panel_side_command_shows_with_a_workspace() {
    assert!(available(Command::TogglePanelSide, &context(View::Terminals)));
    assert_eq!(title(Command::TogglePanelSide), "Panel: Move to the other side");
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p mkrew-ui --lib -j 4 inspector palette`
Expected: compile errors.

- [ ] **Step 3: Implement**

```rust
/// The width a drag of the panel's inner edge gives: from the window's right edge for a right
/// panel; for a left one, `pointer_x` is measured from the panel's own left edge.
pub fn width_from_pointer(window_width: f32, pointer_x: f32, side: PanelSide) -> f32 {
    let max = theme::INSPECTOR_MAX_WIDTH
        .min(window_width / 2.0)
        .max(theme::INSPECTOR_MIN_WIDTH);
    let width = match side {
        PanelSide::Right => window_width - pointer_x,
        PanelSide::Left => pointer_x,
    };
    width.clamp(theme::INSPECTOR_MIN_WIDTH, max)
}
```

  In `shell/inspector.rs`:
  - grip: `.when(side == PanelSide::Right, |el| el.left_0()).when(side == PanelSide::Left, |el| el.right_0())`; in `on_drag_move`, for a left panel subtract the drawn sidebar width (0 while hidden; the full width `self.sidebar_width` or the compact rail's width constant that `render_compact_sidebar` uses: read it there, do not guess) from `event.event.position.x` before calling `width_from_pointer(.., side)`;
  - frame: `border_l` for `Right`, `border_r` for `Left`;
  - toggle: `Icon::PanelRight` for `Right`, `Icon::PanelLeft` for `Left`; tooltip "Hide panel" / "Show panel".

```rust
    /// Moves the panel to the other side and saves `[ui] panel_side` (ADR 0113).
    pub(crate) fn toggle_panel_side(&mut self, cx: &mut Context<'_, Self>) {
        self.panel_side = self.panel_side.other();
        let value = format!("\"{}\"", self.panel_side.name());
        if let Err(error) =
            mkrew_core::settings::write_ui_value(&self.settings_file, "panel_side", &value)
        {
            self.settings_write_failed("the panel side", error, cx);
        }
        cx.notify();
    }
```

  `chrome.rs` `render_main`, Terminals arm:

```rust
                let row = div().flex_1().min_h_0().flex();
                let row = match self.panel_side {
                    PanelSide::Right => row.child(terminals).children(panel),
                    PanelSide::Left => row.children(panel).child(terminals),
                };
                main.child(row)
```

  `Shell::run`: `Command::TogglePanelSide => self.toggle_panel_side(cx),`. `apply_settings_file`: `self.panel_side = settings.ui.panel_side;`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-ui --lib -j 4 inspector palette keymap`
Expected: PASS.

- [ ] **Step 5: Look at it.** `powershell -File scripts/smoke.ps1 start -Theme dark`; open the panel, palette "Panel: Move to the other side", drag the left panel's edge with the sidebar full, compact and hidden; `shot -Out docs/screenshots/window-modes-panel-left-dark.png`; `stop`; repeat with `-Theme light`. The border, grip and icon follow the side; the sidebar stays leftmost; the width follows the pointer exactly.

- [ ] **Step 6: fmt, clippy, commit**

```bash
cargo fmt --all && cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings
git add crates/mkrew-ui/src/inspector.rs crates/mkrew-ui/src/shell/inspector.rs crates/mkrew-ui/src/shell/chrome.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette/tests.rs crates/mkrew-ui/src/shell/sessions.rs docs/screenshots/window-modes-panel-left-dark.png docs/screenshots/window-modes-panel-left-light.png
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): the right panel can sit on the left ([ui] panel_side)"
```

---

### Task 5: Per-window workspace state (`HostId::Space`), mode 1 unchanged

A refactor: no behavior changes, the existing suite is the test.
Three commits; each builds, passes `cargo clippy --workspace --all-targets -j 4 -- -D warnings` and `cargo test -p mkrew-ui -j 2`.

**Files:**
- Modify: `crates/mkrew-ui/src/detached.rs`, `detached/tests.rs`
- Modify: `crates/mkrew-ui/src/main.rs` (Shell fields)
- Modify: `crates/mkrew-ui/src/shell/hosts.rs` (`HostUi`, `HostView`, helpers)
- Modify: `crates/mkrew-ui/src/watches.rs` (`Needs::union` and its test)
- Modify: every file in `git grep -l -E "HostId::Main|self\.workspace\b|self\.view\(\)|right_panel|git_summary|files_scroll|git_files_scroll|doc_page|doc_source_scroll|comments_scroll|jira_scroll|sidebar_view|main_visible" -- crates/mkrew-ui/src`

**Interfaces:**
- Produces:
  - `pub enum HostId { Space(SpaceKey), Detached(WindowKey) }`, `impl HostId { pub fn space(self) -> Option<SpaceKey> }`.
  - `Detached::window_of(&self, tab: &DockTab) -> Option<WindowKey>` (replaces `host_of`); `Detached::move_tab(&mut self, tab: DockTab, to: Option<WindowKey>, index: Option<usize>) -> Option<WindowKey>` (`None` = back to its workspace's Space window); `still_open(&self, host: HostId) -> HostId` keeps a Space host as is.
  - `HostUi` gains `workspace: Option<Ulid>`, `right_panel: inspector::RightPanel`, `view: HostView`, `sidebar_view: Option<Entity<SidebarView>>`, `visible: bool`.
  - `pub(crate) struct HostView { pub(crate) files_scroll: UniformListScrollHandle, pub(crate) git_files_scroll: UniformListScrollHandle, pub(crate) doc_page: preview::Page, pub(crate) doc_source_scroll: UniformListScrollHandle, pub(crate) comments_scroll: ScrollHandle, pub(crate) jira_scroll: ScrollHandle }` with a hand-written `Default`.
  - `Shell` gains `spaces: HashMap<AnyWindowHandle, SpaceKey>`, `space_handles: HashMap<SpaceKey, AnyWindowHandle>`, `next_space: u64`, `panel_default: RightPanel`, `git_summaries: HashMap<Ulid, GitSummary>`, `git_summary_busy: HashSet<Ulid>`, `git_summary_again: HashSet<Ulid>`; loses `workspace`, `right_panel`, `git_summary`, the six scroll fields, `sidebar_view`, `main_visible`.
  - `Shell` helpers in `shell/hosts.rs`:

```rust
    /// The window `window` is: a detached one, else its Space window. A window this Shell
    /// does not know yet (its first frame) counts as the primary Space window.
    pub(crate) fn host_of(&self, window: &Window) -> HostId {
        let handle = window.window_handle();
        if let Some(key) = self.windows.get(&handle) {
            return HostId::Detached(*key);
        }
        HostId::Space(self.spaces.get(&handle).copied().unwrap_or_else(|| self.primary_or_first()))
    }

    /// The workspace a window shows: a Space window's, or a detached window's shown tab's.
    pub(crate) fn workspace_of(&self, host: HostId) -> Option<Ulid> {
        match host {
            HostId::Space(_) => self.hosts.get(&host)?.workspace,
            HostId::Detached(key) => self.detached.active(key).map(|tab| tab.project()),
        }
    }

    /// Every Space window (opened or being opened) with its workspace, by key.
    pub(crate) fn spaces_shown(&self) -> impl Iterator<Item = (SpaceKey, Option<Ulid>)> + use<> {
        let mut spaces: Vec<(SpaceKey, Option<Ulid>)> = (self.hosts.iter())
            .filter_map(|(host, ui)| Some((host.space()?, ui.workspace)))
            .collect();
        spaces.sort_by_key(|(key, _)| *key);
        spaces.into_iter()
    }

    /// The Space window showing `project` (one at most in either mode).
    pub(crate) fn space_of(&self, project: Ulid) -> Option<SpaceKey> {
        self.spaces_shown()
            .find_map(|(key, shown)| (shown == Some(project)).then_some(key))
    }

    /// The workspaces some Space window shows.
    pub(crate) fn on_screen_workspaces(&self) -> Vec<Ulid> {
        let mut shown: Vec<Ulid> = self.spaces_shown().filter_map(|(_, shown)| shown).collect();
        shown.sort();
        shown.dedup();
        shown
    }

    /// The Space window used last: where Settings, Turns… and app menu actions open.
    pub(crate) fn primary(&self) -> Option<SpaceKey> {
        (self.activation.iter())
            .find_map(|host| host.space())
            .or_else(|| self.spaces_shown().next().map(|(key, _)| key))
    }

    /// [`Self::primary`]; a Shell has a Space window from `main()` until it quits, and
    /// `SpaceKey(0)` is the first one's key.
    pub(crate) fn primary_or_first(&self) -> SpaceKey {
        self.primary().unwrap_or(SpaceKey(0))
    }

    /// Where a terminal tab is shown: its detached window, else the Space window of its
    /// workspace (none in mode 2 while that workspace has no window).
    pub(crate) fn terminal_host(&self, project: Ulid, tab_id: Ulid) -> Option<HostId> {
        let tab = DockTab::Terminal { project, tab_id };
        match self.detached.window_of(&tab) {
            Some(key) => Some(HostId::Detached(key)),
            None => self.space_of(project).map(HostId::Space),
        }
    }
```

  - `view(&self, host: HostId) -> View` (today's body on `self.workspace_of(host)`); `panel(&self, host) -> RightPanel` (closed when unknown) and `panel_mut(&mut self, host) -> Option<&mut RightPanel>`; `panel_shown`, `panel_tab`, `file_tree_shown`, `git_panel_shown`, `files_on_screen`, `git_shown`, `panel_session`, `panel_view` take `host: HostId`.
  - `activate_workspace(&mut self, host: HostId, project_id: Ulid, window: &mut Window, cx)`.
  - `git_summary(&self, project: Ulid) -> Option<&GitSummary>`; `load_git_summary(project_id, window, cx)` keeps one request per workspace in flight and stores into `git_summaries`.
  - `watches::Needs::union(self, other: Needs) -> Needs`.

- [ ] **Step 1 (commit 5a): `HostId::Space` and `Detached` without a main window.**
  `detached.rs`: the enum, `space()`, `window_of`, `move_tab(.., to: Option<WindowKey>, ..)`, `is_detached` = `window_of(tab).is_some()`; `detached/tests.rs`: moves to `HostId::Main` become `None`, toast tests use `HostId::Space(SpaceKey(0))`.
  `Shell::new` registers its window: `spaces.insert(handle, SpaceKey(0))`, `space_handles.insert(SpaceKey(0), handle)`, `next_space: 1`, `hosts` starts with `HostId::Space(SpaceKey(0))`, and `menu_host`/`palette_host`/`picker_host`/`activation` start with it.
  Replace each of the ~134 `HostId::Main` uses (`git grep -n "HostId::Main" -- crates/mkrew-ui/src`) by its meaning:

| Old pattern | New form |
|---|---|
| `host == HostId::Main` / `!= HostId::Main` (which kind of window) | `host.space().is_some()` / `matches!(host, HostId::Detached(_))` |
| `self.terminal_host(p, t) == HostId::Main` / `!= ..` (is the tab detached) | `!self.detached.is_detached(&DockTab::Terminal { project: p, tab_id: t })` / `self.detached.is_detached(..)` |
| `self.in_window(HostId::Main, cx, f)` | `self.in_window(HostId::Space(self.primary_or_first()), cx, f)` (Task 7 turns the workspace ones into `in_workspace`) |
| `self.hosts.get(&HostId::Main)` while drawing | `self.hosts.get(&host)` with the host being drawn (`let host = self.host_of(window);` at the top of `Render for Shell`) |
| `menu_host`/`palette_host`/`picker_host` reset to `HostId::Main` | `HostId::Space(self.primary_or_first())` |
| `HostId::Main` as a default in `focus.rs:202`, `tear_off.rs:66,79,89`, `hosts.rs:80` | the Space window of the tab's workspace (`self.space_of(project).map(HostId::Space)`) or the host being drawn |
| `self.main_visible` | `HostUi.visible` of the Space host (the visibility observer writes it; `toast_host` reads it) |
| `in_window`'s `HostId::Main => Some(self.main_window)` | `HostId::Space(key) => self.space_handles.get(&key).copied()`; delete `main_window` |

  Commit `refactor(ui): HostId::Space replaces HostId::Main`.

- [ ] **Step 2 (commit 5b): the shown workspace per window.**
  Delete `Shell.workspace`; `activate_workspace(host, project, ..)` writes `self.hosts.entry(host).or_default().workspace = Some(project)`; `switched` compares with that host's previous workspace; it closes menus only when `self.menu_host == host`; the old workspace's git summary is dropped only when no other Space window shows it.
  Each read of `self.workspace` (~120) takes the host:
  - with `window` in scope: `self.workspace_of(self.host_of(window))`;
  - render code: pass `host` down (`render_main`, `render_tab_area`, `render_right_panel`, `render_board`, `render_docs`, `render_title_bar` already has it, and their helpers that read the workspace);
  - event handlers without a window (`connection.rs` `git.changed`, `fs.changed`, `session.*`, `tab.*`): act on the event's project when `self.on_screen_workspaces().contains(&project)`, and on its Space window through `self.space_of(project)` when a window is needed (`in_window`).
  - `reset_connection_state` no longer sets the workspace to `None` (connection.rs:853); in `refresh` (~992-1016) each Space window keeps its workspace when it is still listed, else takes `kept` / `initial_workspace()`, else `None` (empty state); `pending_jump` and `start_restore` act on the primary window.
  - `view()` becomes `view(host)`; `tabs_mut()`-style helpers that used `self.workspace?` take the host.
  - `sync_watches`: one `watches::Needs::of(Some(project), files, git, [])` per on-screen workspace (`files`/`git` from that workspace's views when they are drawn in its window), plus `documents::open_worktrees(&self.tabs)` once, merged with `Needs::union`. Test in `watches.rs`:

```rust
    #[test]
    fn union_keeps_both_projects_and_their_worktrees() {
        let (a, b, task) = (Ulid::new(), Ulid::new(), Ulid::new());
        let both = Needs::of(Some(a), Some(task), None, []).union(Needs::of(Some(b), None, None, []));
        assert!(both.released(&Needs::default()).len() == 3);
        assert!(Needs::of(Some(a), None, None, []).released(&both).is_empty());
    }
```

  ```rust
    pub fn union(mut self, other: Needs) -> Needs {
        self.0.extend(other.0);
        self
    }
  ```

  Commit `refactor(ui): each window keeps its shown workspace`.

- [ ] **Step 3 (commit 5c): panel and scroll state per window, git summaries per workspace.**
  `right_panel` moves into `HostUi` (seeded from `self.panel_default = RightPanel::from_mode(ui.right_panel)` for every new Space window; `panel_changed` writes `[ui] right_panel` from the host that changed).
  The six scroll fields move into `HostView`; readers use `self.hosts[&host].view.X` (render code has `host`; actions use `self.host_of(window)`; `HashMap` indexing panics on a missing key, so use `if let Some(ui) = self.hosts.get(&host)`).
  `git_summaries` replaces `git_summary` (readers listed in the Lifecycle map: `composer.rs:1190`, `git_menus.rs:128,147`, `git_render.rs:452,644`, `inspector.rs:282,410`; they pass the workspace they draw); `load_git_summary` uses `git_summary_busy.insert(project)` / `git_summary_again` per project and, when an answer arrives, keeps it only while `self.on_screen_workspaces().contains(&project)`.
  `sidebar_view` moves into `HostUi.sidebar_view` (created lazily per Space window where `Render for Shell` creates it today).
  Commit `refactor(ui): panel, scroll and git summary state per window`.

- [ ] **Step 4: Prove mode 1 is unchanged.** `powershell -File scripts/smoke.ps1 start -Theme dark`, two workspaces with fakeagent sessions: switch workspaces from the switcher and with Alt+1/2, open Files and Git in the panel and scroll the tree, open Docs and Board, tear a tab off with Shift+F4 and move it back, open a file link from the torn-off window, stop and start the sandbox daemon (reconnect keeps the workspace), quit and start again. Everything must behave as on master. `stop`.

---

### Task 6: `SpaceWindow`: `Shell` off the window, restore from layout version 2, mode 1 unchanged

**Files:**
- Create: `crates/mkrew-ui/src/shell/space_view.rs` (`SpaceWindow`)
- Create: `crates/mkrew-ui/src/shell/space_render.rs` (`Shell::render_space`: the old `Render for Shell` body)
- Modify: `crates/mkrew-ui/src/shell.rs` (module list)
- Modify: `crates/mkrew-ui/src/main.rs` (`fn main` 280-347; delete `impl Render for Shell`)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (`Shell::new`)
- Modify: `crates/mkrew-ui/src/shell/app_menu.rs` (`MainWindow` → `ShellHandle`)
- Modify: `crates/mkrew-ui/src/shell/hosts.rs` (`save_windows`, `start_restore`, `restore_progress`; delete `save_main_frame`, `main_dirty`, `main_frame_path`)
- Modify: `crates/mkrew-ui/src/shell/files.rs` (`window_should_close` → `space_should_close`)
- Modify: `crates/mkrew-ui/src/shell/attention.rs` (`restore_window`)
- Modify: `crates/mkrew-ui/src/desktop.rs`, `desktop/win32.rs`, `desktop/none.rs` and the macOS desktop file if its `Os::new` takes a window
- Modify: `crates/mkrew-ui/src/detached.rs` (delete `save_frame`)

**Interfaces:**
- Consumes: Task 3 `load_layout`, `Layout`, `SavedSpace`; Task 5 helpers.
- Produces:
  - `pub(crate) struct SpaceWindow { shell: WeakEntity<Shell>, key: SpaceKey, focus_handle: FocusHandle, _subscriptions: Vec<Subscription> }`; `SpaceWindow::new(shell: Entity<Shell>, key: SpaceKey, window: &mut Window, cx: &mut Context<'_, Self>) -> Self`.
  - `Shell::render_space(&mut self, key: SpaceKey, window: &mut Window, cx: &mut Context<'_, Self>) -> AnyElement`.
  - `pub(crate) type AfterOpen = Box<dyn FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>)>`.
  - `Shell::open_space_window(&mut self, project: Option<Ulid>, bounds: Bounds<Pixels>, placement: Placement, then: Option<AfterOpen>, cx) -> SpaceKey`; `Shell::space_opened(&mut self, key, handle: AnyWindowHandle, window, cx)`; `Shell::close_space_window(&mut self, key, cx)`; `Shell::space_should_close(&mut self, key, window, cx) -> bool`; `Shell::project_name(&self, id: Ulid) -> Option<&str>` (if no such helper exists yet).
  - `Shell::with_primary<R>(this: &WeakEntity<Shell>, cx: &mut AsyncApp, f: impl FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>) -> R) -> Option<R>`.
  - `fn space_window_options(bounds: WindowBounds, ui_size: f32) -> WindowOptions` (shared by `main()` and `open_space_window`).
  - `shell::app_menu::ShellHandle(pub Entity<Shell>)`.
  - `Desktop::new(id: &str, local: bool) -> (Desktop, Receiver<TrayEvent>)`, `Desktop::attach(&mut self, window: &Window)`, `Desktop::detach(&mut self, window: &Window)`.

- [ ] **Step 1: Desktop keeps a list of windows.** `desktop/win32.rs` `Os`: `hwnds: Vec<HWND>` replaces `hwnd`; `new(id, tray_icon)` makes the taskbar list (`CoCreateInstance` as today, no window needed) and the tray; `attach(window)` reads the HWND as `new` does today, pushes it (once) and calls `reapply_badge`; `detach(window)` removes it; `reapply_badge` loops `SetOverlayIcon` over `hwnds`; `flash` loops `FlashWindowEx`; `hide_window`/`show_window` loop `ShowWindow`. The SAFETY comments say "each `hwnd` is a live Space window (detached before it closes)". `desktop/none.rs` and the macOS file get no-op `attach`/`detach` and a `new` without a window. `desktop.rs` forwards.

- [ ] **Step 2: `SpaceWindow`.** Copy `DetachedWindow` (`shell/detached_view.rs`) and change:
  - activation: `shell.activated(HostId::Space(key)); shell.viewed.activated(); shell.desktop.flash(false);`; bounds: `shell.record_bounds(HostId::Space(key), window)`; visibility: set `HostUi.visible`, then `shell.sync_panes(window, cx)`; appearance: as `DetachedWindow`;
  - `window.on_window_should_close(cx, move |window, cx| close.update(cx, |shell, cx| shell.space_should_close(key, window, cx)).unwrap_or(true))`;
  - the per-window focus subscriptions `Shell::new` makes on its window (`rename_blur`, `follow_up_blur`, `dialog_focus_lost`, connection.rs ~257-276) are made here, inside `shell.update(cx, |shell, cx| ..)`, and kept in `_subscriptions`, so they work in every Space window;
  - render: `shell.render_space(key, window, cx)` inside a root `div` tracking `self.focus_handle`, forwarding `Run` and the preview actions exactly as `DetachedWindow` does.
  `space_render.rs`: the whole `impl Render for Shell` body becomes `render_space(key, ..)` with `let host = HostId::Space(key);` and `HostUi.sidebar_view`; delete `impl Render for Shell`.

- [ ] **Step 3: `main()` creates the Shell once.** Replace main.rs 323-345 by:

```rust
        let mode = settings.ui.theme;
        let opened = cx.open_window(options, |window, cx| {
            theme::refresh(mode, window, cx);
            let shell = cx.new(|cx| {
                Shell::new(launched, paths, keymap, &keymap_errors, opening, window, cx)
            });
            cx.set_global(shell::app_menu::ShellHandle(shell.clone()));
            cx.new(|cx| shell::space_view::SpaceWindow::new(shell, SpaceKey(0), window, cx))
        });
        if let Err(error) = opened {
            eprintln!("mkrew: cannot open window: {error:#}");
            cx.quit();
            return;
        }
        // Sessions live in the daemon: the app ends with its last Space window
        // (`Shell::space_should_close` saved first); detached windows never keep it alone.
        cx.on_window_closed(|cx, _| {
            let none_left = (cx.try_global::<shell::app_menu::ShellHandle>())
                .is_none_or(|handle| handle.0.read(cx).space_handles.is_empty());
            if none_left || cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
```

  `options` is `space_window_options(window_bounds, ui_size)`; `window_bounds` comes from `detached::load_layout(&windows_path, &legacy_main_path)`: its last `spaces` entry through `window_place::restored` as today (else centered). The other saved Space windows open in Step 5.
  `Shell::new`: no window observers (they live in `SpaceWindow`), no `on_window_should_close`; `let (mut desktop, tray_events) = Desktop::new(&id, local); desktop.attach(window);`.
  The long-lived loops in `Shell::new` (page events ~80, tray events ~96, stdin quit ~123, status tick ~140, phone poll ~218, update check ~239) become `cx.spawn(async move |this, cx| ..)` and call `Shell::with_primary(&this, cx, |shell, window, cx| ..)` where they called `this.update_in(cx, ..)`; a `None` (no Space window left) ends the loop.

```rust
    /// Runs `f` in the primary Space window; `None` once no Space window is left.
    pub(crate) fn with_primary<R>(
        this: &WeakEntity<Shell>,
        cx: &mut AsyncApp,
        f: impl FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>) -> R,
    ) -> Option<R> {
        let handle = this
            .read_with(cx, |shell, _| {
                (shell.primary()).and_then(|key| shell.space_handles.get(&key).copied())
            })
            .ok()??;
        handle
            .update(cx, |_, window, cx| this.update(cx, |shell, cx| f(shell, window, cx)).ok())
            .ok()?
    }
```

  The other `cx.spawn_in(window, ..)` request callbacks (~117) stay: if their window closes first, the answer is dropped and the daemon's next event refreshes the state. Put `// ponytail: an answer to a window that closed meanwhile is dropped; the next tab event reloads it.` on the one in `load_tabs`.

- [ ] **Step 4: `ShellHandle` in the app menu.** `pub(crate) struct ShellHandle(pub Entity<Shell>); impl gpui::Global for ShellHandle {}`. `with_main` becomes:

```rust
fn with_primary(
    cx: &mut App,
    f: impl FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>) + 'static,
) {
    cx.defer(move |cx| {
        let Some(shell) = cx.try_global::<ShellHandle>().map(|handle| handle.0.clone()) else {
            return;
        };
        let primary = shell.read(cx);
        let Some(handle) = (primary.primary()).and_then(|key| primary.space_handles.get(&key).copied())
        else {
            return;
        };
        let _ = handle.update(cx, |_, window, cx| shell.update(cx, |shell, cx| f(shell, window, cx)));
    });
}
```

  `on_app_quit`: `shell.update(cx, |shell, cx| shell.save_before_exit(cx))`; `reopen`: `cx.activate(true); with_primary(cx, |shell, window, cx| shell.restore_window(window, cx));`.

- [ ] **Step 5: Open, close and save Space windows.**

```rust
    /// Opens a Space window showing `project`. Its HostUi exists at once, so `space_of`
    /// already finds it while the OS window opens (a second request focuses it). `then` runs
    /// in it once open.
    pub(crate) fn open_space_window(
        &mut self,
        project: Option<Ulid>,
        bounds: Bounds<Pixels>,
        placement: Placement,
        then: Option<AfterOpen>,
        cx: &mut Context<'_, Self>,
    ) -> SpaceKey {
        let key = SpaceKey(self.next_space);
        self.next_space += 1;
        let ui = HostUi {
            bounds,
            restore: bounds,
            placement,
            workspace: project,
            right_panel: self.panel_default,
            visible: true,
            ..HostUi::default()
        };
        self.hosts.insert(HostId::Space(key), ui);
        self.windows_dirty = true;
        let shell = cx.entity();
        let mode = cx.theme().mode;
        let ui_size = cx.global::<theme::Fonts>().ui_size;
        cx.spawn(async move |this, cx| {
            let options = space_window_options(placement.window_bounds(bounds), ui_size);
            let opened = cx.open_window(options, |window, cx| {
                theme::refresh(mode, window, cx);
                cx.new(|cx| SpaceWindow::new(shell, key, window, cx))
            });
            match opened {
                Ok(handle) => {
                    let handle: gpui::AnyWindowHandle = handle.into();
                    let _ = handle.update(cx, |_, window, cx| {
                        let _ = this.update(cx, |shell, cx| {
                            shell.space_opened(key, handle, window, cx);
                            if let Some(project) = project {
                                shell.activate_workspace(HostId::Space(key), project, window, cx);
                            }
                            if let Some(then) = then {
                                then(shell, window, cx);
                            }
                        });
                    });
                }
                Err(error) => {
                    let _ = this.update(cx, |shell, cx| {
                        tracing::warn!("cannot open a window: {error:#}");
                        shell.hosts.remove(&HostId::Space(key));
                        shell.windows_dirty = true;
                        let name = (project.and_then(|id| shell.project_name(id)))
                            .unwrap_or("Mkrew")
                            .to_owned();
                        shell.toast_error(format!("Cannot open a window for {name}: {error:#}"), cx);
                    });
                }
            }
        })
        .detach();
        key
    }
```

  `space_opened`: insert into `spaces`/`space_handles`, `activated(HostId::Space(key))`, `record_bounds`, `self.desktop.attach(window)`, `sync_panes`, `cx.notify()`.
  `close_space_window(key, cx)`: like `close_os_window` (hosts.rs ~330) for a Space host: remove it from `spaces`, `space_handles`, `hosts`, `activation`; close the palette, pickers, menus and `open_ask` (Task 7) whose host it was and move their hosts to `primary_or_first()`; `sync_watches`; in the deferred window update call `shell.desktop.detach(window)` then `window.remove_window()`.
  `space_should_close(key, window, cx)`: when another Space window is open, do `close_space_window`'s bookkeeping (without the deferred remove), `self.desktop.detach(window)`, and return `true`; when it is the last one, run today's `window_should_close` body (hide every window to the tray, ask about unsaved edits, or `save_before_exit` and return `true`).
  `save_windows` saves `Layout { spaces, windows }`: `spaces` lists every opened Space window that shows a workspace, ordered so that the one focused last (first in `activation`) is last.
  Delete `save_main_frame`, `main_dirty`, `main_frame_path` and `detached::save_frame`.
  `start_restore` reads `load_layout(&self.windows_path, &legacy_main_path)` (keep the legacy path in `Shell` as `legacy_main_path: PathBuf`, read only); detached windows restore as today; after the first project listing (`refresh`), each saved Space entry except the last (already open) opens with `open_space_window(Some(workspace), ..)` through `window_place::restored`, skipping workspaces that are not listed and `Ulid::nil()`; in `window_mode = single` none of them opens. The open first window takes the last entry's workspace when it is listed, else `initial_workspace()`.
  `restore_window` (attention.rs:55): `self.desktop.show_window()` now shows every Space window; then `window.activate_window()` on the primary one.

- [ ] **Step 6: Tests.** `cargo test -p mkrew-ui -j 2` PASS.

- [ ] **Step 7: Prove mode 1 is unchanged.** The Task 5 Step 4 smoke, plus: quit and start again (the window comes back at the same place: the first start reads `main-window.json`, later ones `windows.json` version 2); with Close to tray on, ✕ hides to the tray and tray Open brings it back with its badge. The macOS app menu cannot be checked here: Task 10 lists it in HUMAN_CHECKS.

- [ ] **Step 8: fmt, clippy, commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -j 4 -- -D warnings
git status --short
git add <every file of this task listed by git status, by path>
git -c user.email=tungnv975@gmail.com commit -m "refactor(ui): Space windows draw through Shell like detached ones"
```

---

### Task 7: Mode 2: open, route, ask, close, title, stripe

**Files:**
- Create: `crates/mkrew-ui/src/shell/window_mode.rs`
- Modify: `crates/mkrew-ui/src/shell.rs` (module list)
- Modify: `crates/mkrew-ui/src/main.rs` (fields `window_mode: WindowMode`, `open_pref: OpenWorkspace`, `open_ask: Option<OpenAsk>`)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (init; `apply_settings_file` updates `open_pref`; `refresh` calls `prune_spaces`)
- Modify: `crates/mkrew-ui/src/shell/space_render.rs` (no sidebar in mode 2, stripe, title, `open_ask` overlay)
- Modify: `crates/mkrew-ui/src/shell/chrome.rs` (title bar; `switcher_rows`)
- Modify: `crates/mkrew-ui/src/shell/sessions.rs`, `shell/workspace.rs`, `shell/detached_actions.rs`, `shell/navigation.rs`, `shell/places.rs`, `shell/files_split.rs`, `shell/diff_pane.rs`, `shell/history.rs`, `shell/browser.rs`, `shell/diagram_dialog.rs`, `shell/attention.rs`
- Modify: `crates/mkrew-ui/src/palette.rs`, `palette/tests.rs`
- Modify: `crates/mkrew-ui/src/shell/focus.rs` (`modal_open`)

**Interfaces:**
- Consumes: `window_mode::{open_target, OpenTarget, Choice}` (Task 2); Tasks 5-6 helpers.
- Produces:
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub(crate) enum Then { Nothing, Session(Ulid) }`
  - `pub(crate) struct OpenAsk { pub(crate) project: Ulid, pub(crate) host: HostId, pub(crate) then: Then, pub(crate) at: Option<Point<Pixels>>, pub(crate) remember: bool }`
  - `Shell::route_workspace(&mut self, project: Ulid, then: Then, ctrl: bool, at: Option<Point<Pixels>>, window, cx)`
  - `Shell::in_workspace(&mut self, project: Ulid, cx, f: impl FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>) + 'static)`
  - `Shell::show_session_here(&mut self, session: Ulid, window, cx)`
  - `Shell::answer_open_ask(&mut self, choice: Choice, window, cx)`, `Shell::cancel_open_ask(&mut self, window, cx)`, `Shell::open_ask_key_down`
  - `Shell::new_space_bounds(&self, from: HostId, step: usize) -> (Bounds<Pixels>, Placement)`
  - `Shell::prune_spaces(&mut self, cx)`
  - `palette::Context { per_workspace: bool, .. }`

- [ ] **Step 1: Palette visibility, test first.** Add `per_workspace: false` to the `context()` fixture in `palette/tests.rs` and:

```rust
#[test]
fn sidebar_commands_hide_with_a_window_per_workspace() {
    let single = context(View::Terminals);
    let spaces = Context { per_workspace: true, ..context(View::Terminals) };
    for command in [Command::ToggleSidebar, Command::ToggleCompactSidebar, Command::FocusSidebar] {
        assert!(available(command, &single), "{command:?}");
        assert!(!available(command, &spaces), "{command:?}");
    }
    for extra in [Extra::SidebarToggleAll, Extra::SidebarRemoveIdle] {
        assert!(!extra.available(&spaces), "{extra:?}");
    }
    assert!(available(Command::TogglePanelSide, &spaces));
}
```

  Fails; add `pub per_workspace: bool` to `palette::Context`, fill it in `navigation.rs:337` from `self.window_mode == WindowMode::PerWorkspace`, add `&& !context.per_workspace` to the five entries' availability; PASS (`cargo test -p mkrew-ui --lib -j 4 palette`).

- [ ] **Step 2: Mode state.** `window_mode` and `open_pref` from `settings.ui` in `Shell::new`; `apply_settings_file` sets `open_pref` (the `window_mode` change is Task 8). In `Shell::run`, the `ToggleSidebar`, `ToggleCompactSidebar` and `FocusSidebar` arms do nothing in mode 2 (`if self.window_mode == WindowMode::PerWorkspace { return; }` with `// ADR 0113: no sidebar in a window per workspace`).

- [ ] **Step 3: Routing** in `shell/window_mode.rs`:

```rust
    /// Opens `project` the way the window mode says (spec 4.2): mode 1 switches the window
    /// that asked (the primary one from a detached window); mode 2 focuses its window, opens
    /// a new one, replaces the asking window's workspace, or asks. `then` runs where it lands.
    pub(crate) fn route_workspace(
        &mut self,
        project: Ulid,
        then: Then,
        ctrl: bool,
        at: Option<Point<Pixels>>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let space = (self.host_of(window).space()).unwrap_or_else(|| self.primary_or_first());
        let spaces: Vec<(SpaceKey, Option<Ulid>)> = self.spaces_shown().collect();
        match open_target(project, self.window_mode, self.open_pref, ctrl, &spaces) {
            OpenTarget::Switch | OpenTarget::Replace => self.show_in(space, project, then, cx),
            OpenTarget::Focus(key) => {
                self.in_window(HostId::Space(key), cx, move |shell, window, cx| {
                    window.activate_window();
                    shell.run_then(then, window, cx);
                });
            }
            OpenTarget::New => self.open_new_space(HostId::Space(space), project, then, cx),
            OpenTarget::Ask => {
                self.close_menus();
                self.open_ask = Some(OpenAsk {
                    project,
                    host: HostId::Space(space),
                    then,
                    at,
                    remember: false,
                });
                self.dialog_serial += 1;
                window.focus(&self.dialog_focus, cx);
                cx.notify();
            }
        }
    }

    /// `space` shows `project`, comes forward and runs `then`.
    fn show_in(&mut self, space: SpaceKey, project: Ulid, then: Then, cx: &mut Context<'_, Self>) {
        self.in_window(HostId::Space(space), cx, move |shell, window, cx| {
            window.activate_window();
            shell.activate_workspace(HostId::Space(space), project, window, cx);
            shell.run_then(then, window, cx);
        });
    }

    /// A new Space window for `project`, cascading from `from`, running `then` once open.
    fn open_new_space(&mut self, from: HostId, project: Ulid, then: Then, cx: &mut Context<'_, Self>) {
        let (bounds, placement) = self.new_space_bounds(from, 1);
        let after: AfterOpen = Box::new(move |shell, window, cx| shell.run_then(then, window, cx));
        self.open_space_window(Some(project), bounds, placement, Some(after), cx);
    }

    fn run_then(&mut self, then: Then, window: &mut Window, cx: &mut Context<'_, Self>) {
        if let Then::Session(session) = then {
            self.show_session_here(session, window, cx);
        }
    }

    /// Runs `f` in the Space window showing `project`, making one show it first: mode 1
    /// switches the single window, mode 2 opens a new one without asking (the user asked for
    /// something inside it: a file, a diff, a tab moved back).
    pub(crate) fn in_workspace(
        &mut self,
        project: Ulid,
        cx: &mut Context<'_, Self>,
        f: impl FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>) + 'static,
    ) {
        if let Some(key) = self.space_of(project) {
            return self.in_window(HostId::Space(key), cx, f);
        }
        let primary = self.primary_or_first();
        if self.window_mode == WindowMode::Single {
            return self.in_window(HostId::Space(primary), cx, move |shell, window, cx| {
                shell.activate_workspace(HostId::Space(primary), project, window, cx);
                f(shell, window, cx);
            });
        }
        let (bounds, placement) = self.new_space_bounds(HostId::Space(primary), 1);
        self.open_space_window(Some(project), bounds, placement, Some(Box::new(f)), cx);
    }

    /// Where the `step`-th new window goes: the size of `from`, offset `step` × 32 px right
    /// and down; always windowed. `open_space_window` keeps it on a connected screen.
    pub(crate) fn new_space_bounds(&self, from: HostId, step: usize) -> (Bounds<Pixels>, Placement) {
        let fallback = Bounds::new(
            point(px(0.0), px(0.0)),
            size(px(theme::WINDOW_WIDTH), px(theme::WINDOW_HEIGHT)),
        );
        let base = self.hosts.get(&from).map_or(fallback, |ui| ui.restore);
        let offset = px(space::XXL * step as f32);
        let origin = point(base.origin.x + offset, base.origin.y + offset);
        (Bounds::new(origin, base.size), Placement::Windowed)
    }
```

  In `open_space_window` (Task 6), pass the bounds through `window_place::restored(bounds, placement, min, &displays, primary)` inside the spawned task (`cx.displays()` there) so a cascade never leaves the screen.
  `show_session_here(session, window, cx)`: the tail of today's `open_session` (`self.views.insert(project_id, View::Terminals)`, `place_session` or `reveal`, then `activate_workspace(self.host_of(window), project_id, ..)`).
  `open_session`: keep the detached branch; replace the `HostId::Main` reroute and the tail by `self.route_workspace(project_id, Then::Session(session), false, None, window, cx)`.
  `notification_clicked`: `Jump::Workspace(p)` → `route_workspace(p, Then::Nothing, false, None, ..)`.

- [ ] **Step 4: Every entry point routes.** For `git grep -n "activate_workspace(" -- crates/mkrew-ui/src`:
  - a user picking a workspace (switcher rows `chrome.rs:22`, `Command::Workspace(n)` `sessions.rs:370`, Next/Previous workspace, palette workspace entries in `navigation.rs`, sidebar workspace rows (mode 1 gives `Switch`), end of Add workspace, tray `Session`): `route_workspace(project, Then::Nothing, ctrl, at, ..)`. Ctrl is the platform's secondary modifier: a click passes `event.modifiers().secondary()`, a key passes `window.modifiers().secondary()` (if `secondary()` does not exist in this gpui, use `control || platform`); switcher rows pass `at = Some(self.menu_anchor(&Anchor::Switcher, window, cx))`, the rest `None`.
  - internal (connection `refresh`, restore, `space_opened`, `show_in`, `in_workspace`): keep `activate_workspace(host, ..)`.
  Reroutes that meant "do it in the main window" become `in_workspace(project, cx, ..)`: `places.rs:19`, `navigation.rs:210`, `navigation.rs:1123`, `files_split.rs:365` and `:449`, `diff_pane.rs:84`, `history.rs:124`, `browser.rs:252`. `diagram_dialog.rs:16` reroutes to `HostId::Space(self.primary_or_first())` (a dialog).
  Detached "Move back": `move_tab_to(tab, None, ..)` and `move_all_back` do `detached.move_tab(tab, None, None)`, then `in_workspace(project, cx, move |shell, window, cx| { shell.focus_pane_key(project, &key); window.activate_window(); shell.sync_panes(window, cx); })`; the tab menu row (`detached_actions.rs:269`) reads "Move back"; palette `Extra::TabMoveToMainWindow` is renamed `Extra::TabMoveBack` with title "Tab: Move back to its workspace window" (update its test in `palette/tests.rs`).

- [ ] **Step 5: The question.** `render_open_ask(&self, ask: &OpenAsk, theme, window, cx) -> AnyElement`, drawn by `render_space` only when `ask.host == host`:
  - `floating_panel("open-ask", theme).w(px(theme::CONFIRM_DIALOG_WIDTH)).p(px(space::L)).flex().flex_col().gap(px(space::M))`, placed with `deferred(anchored().position(at).snap_to_window_with_margin(px(space::S)))` when `at` is set, else centered with `appear("open-ask", panel, true)`;
  - title "Open <workspace>" (`font_size::CARD_TITLE`, `weight::SEMIBOLD`);
  - a "Don't ask again" check row drawn like the worktree row in `task_dialog.rs:359-395` (`.pressable(Look::Row, true, theme)`, `.focus_stop(theme)`, toggles `remember`);
  - buttons right-aligned: `button("open-ask-this", "This window", theme, false)` and `button("open-ask-new", "New window", theme, true)`, each `.on_click` → `answer_open_ask`;
  - `.dialog_root(&self.dialog_focus, false, cx.listener(Self::dialog_guard), cx.listener(Self::open_ask_key_down))` and `self.typing(&self.dialog_focus, Self::open_ask_key_down, cx)`; keys: `dialog_nav` first (Tab moves between the check row and buttons, Space or Enter on a focused control presses it), then Enter → `Choice::NewWindow`, Esc → `cancel_open_ask`.

```rust
    pub(crate) fn answer_open_ask(&mut self, choice: Choice, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(ask) = self.open_ask.take() else {
            return;
        };
        if ask.remember {
            self.open_pref = choice.remembered();
            let value = format!("\"{}\"", self.open_pref.name());
            if let Err(error) =
                mkrew_core::settings::write_ui_value(&self.settings_file, "open_workspace", &value)
            {
                self.settings_write_failed("the open workspace choice", error, cx);
            }
        }
        let space = ask.host.space().unwrap_or_else(|| self.primary_or_first());
        match choice {
            Choice::NewWindow => self.open_new_space(ask.host, ask.project, ask.then, cx),
            Choice::ThisWindow => self.show_in(space, ask.project, ask.then, cx),
        }
        self.focus_pane(window, cx);
        cx.notify();
    }

    pub(crate) fn cancel_open_ask(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.open_ask = None;
        self.focus_pane(window, cx);
        cx.notify();
    }
```

  `modal_open()` includes `self.open_ask.is_some()`; `close_popups` and `close_space_window` clear it when its host goes.

- [ ] **Step 6: Mode 2 look.** In `render_space` when `self.window_mode == WindowMode::PerWorkspace`: skip the sidebar child; the root's first child is the stripe:

```rust
            .children(stripe.map(|color| {
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(theme::WORKSPACE_STRIPE_WIDTH))
                    .bg(rgb(color))
            }))
```

  (`stripe` = the shown workspace's `color` through `theme.workspace_color`); the window title `"{name} - Mkrew"` set through `HostUi.title` as `render_host` does (mode 1 sets nothing, as today).
  `render_title_bar`: no `sidebar_toggle` in mode 2. `switcher_rows(project_id)` takes the host: a row whose workspace has another Space window gets `.keys("◳")`; every row's action is `route_workspace(id, Then::Nothing, false, Some(anchor), ..)` (`anchor` captured from `menu_anchor(&Anchor::Switcher, ..)` when the rows are built, or read again in the action).

- [ ] **Step 7: Workspaces that go away.** `prune_spaces(cx)`, called after each project listing in `refresh` and on project archive/remove events: in mode 2 a Space window whose workspace is no longer listed closes (`close_space_window`) unless it is the last open one, which takes `initial_workspace()` (`None` without workspaces: the empty state with Add workspace).

- [ ] **Step 8: Tests.** `cargo test -p mkrew-ui -j 2` PASS.

- [ ] **Step 9: Smoke in mode 2** (sandbox `settings.toml`: `window_mode = "per_workspace"`), `scripts/smoke.ps1 start -Theme dark`, three workspaces, fakeagent sessions; `shot` each step into `docs/screenshots/window-modes-<n>-dark.png`, then again with `-Theme light`:
  1. switcher → `web`: the question shows under the switcher; Enter opens `web` in a new window at +32 px, titled `web - Mkrew`, with its stripe and no sidebar;
  2. from the first window, switcher → `web` again: the `web` window comes forward, no question; switcher → `docs`, Enter, and at once Alt+3 (`docs`): exactly one `docs` window;
  3. tick "Don't ask again", This window: the window shows the workspace; next time no question; Ctrl+click takes New window;
  4. a file link clicked in a detached window of a workspace without a window opens that workspace's window with the file;
  5. close a non-last window holding an edited file: no question, the edit is still there when the workspace opens again; close the last window: today's unsaved-edits question;
  6. `mk project remove <id>` (sandbox `mk`) while its window is open: the window closes;
  7. Board in one window and Docs in another scroll independently;
  8. the taskbar shows the badge on every window's button.
  `stop`.

- [ ] **Step 10: fmt, clippy, commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -j 4 -- -D warnings
git status --short
git add <this task's files by path> docs/screenshots/window-modes-*.png
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): a window per workspace (route, ask, title, stripe, close rules)"
```

---

### Task 8: Split and combine flows

**Files:**
- Modify: `crates/mkrew-ui/src/shell/window_mode.rs`
- Modify: `crates/mkrew-ui/src/main.rs` (field `split_picker: Option<WorkspacePicker>`)
- Modify: `crates/mkrew-ui/src/keymap.rs` (`SplitWindows` = `"split_windows"`, `CombineWindows` = `"combine_windows"`; `all()`; `String::new()` in `pc_keys`)
- Modify: `crates/mkrew-ui/src/palette.rs`, `palette/tests.rs`
- Modify: `crates/mkrew-ui/src/shell/sessions.rs` (`run`), `shell/settings.rs` (`apply_settings_effect`), `shell/connection.rs` (`apply_settings_file`), the Split ▾ layout menu rows (grep `layout_menu` in `shell/menus.rs` / `menu_render.rs`), `shell/space_render.rs` (picker overlay), `shell/focus.rs` (`modal_open`)

**Interfaces:**
- Consumes: `WorkspacePicker`, `PickOutcome`, `PickRow`, `SplitPlan`, `plan_combine` (Task 2); `open_space_window`, `close_space_window`, `new_space_bounds` (Tasks 6-7).
- Produces: `Shell::open_split_picker(window, cx)`, `Shell::split_picker_key_down`, `Shell::split_into_windows(plan: SplitPlan, window, cx)`, `Shell::combine_windows(window, cx)`, `Shell::set_window_mode(mode: WindowMode, cx)`.

- [ ] **Step 1: Palette test first.**

```rust
#[test]
fn split_and_combine_show_in_their_mode() {
    let single = Context { workspaces: 2, ..context(View::Terminals) };
    let alone = Context { workspaces: 1, ..context(View::Terminals) };
    let spaces = Context { workspaces: 2, per_workspace: true, ..context(View::Terminals) };
    assert!(available(Command::SplitWindows, &single));
    assert!(!available(Command::SplitWindows, &alone));
    assert!(!available(Command::SplitWindows, &spaces));
    assert!(available(Command::CombineWindows, &spaces));
    assert!(!available(Command::CombineWindows, &single));
    assert_eq!(title(Command::SplitWindows), "Window: Open each workspace in its own window…");
    assert_eq!(title(Command::CombineWindows), "Window: Combine into one window");
}
```

  Fails; add the commands, titles and availability (`SplitWindows`: `!context.per_workspace && context.workspaces > 1`; `CombineWindows`: `context.per_workspace`); PASS. `detached::handles` does not list them.

- [ ] **Step 2: Open the picker.**

```rust
    /// "Open workspaces in windows" (spec 4.1); with one workspace or none the mode changes
    /// at once.
    pub(crate) fn open_split_picker(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let host = self.host_of(window);
        let shown = self.workspace_of(host);
        let listed = self.shown_workspaces();
        if listed.len() <= 1 {
            let keep = shown.or_else(|| listed.first().copied()).unwrap_or_default();
            return self.split_into_windows(SplitPlan { keep, open: vec![] }, window, cx);
        }
        let projects = self.sidebar.projects();
        let rows = (listed.iter())
            .filter_map(|id| projects.iter().find(|project| project.id == *id))
            .map(|project| PickRow {
                id: project.id,
                name: project.name.clone(),
                color: project.color.clone(),
                working: self.working_count(project.id),
                asking: self.asking_count(project.id),
            })
            .collect();
        self.close_menus();
        self.split_picker = Some(WorkspacePicker::new(rows, shown));
        self.palette_host = host;
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }
```

  `working_count(project)`: agent sessions of the workspace with `SessionState::Working`; `asking_count(project)`: sessions with `SessionState::NeedsInput`; both over `self.sidebar.sessions()` (if `sidebar.rs` already computes these for `WorkspaceRow`, call that instead; grep `working:` in `sidebar.rs`).
  `Ulid::default()` is nil: `split_into_windows` skips `activate_workspace` for a nil `keep`.
  `modal_open()` includes `self.split_picker.is_some()`; `render_space` draws it when `self.palette_host == host`.

- [ ] **Step 3: Draw the picker** like `render_broadcast` (`shell/dialogs.rs` 468-620): `floating_panel("split-picker", theme).w(px(theme::DIALOG_WIDTH)).p(px(space::XL)).gap(px(space::M))`, `dialog_root(&self.dialog_focus, true, cx.listener(Self::dialog_guard), cx.listener(Self::split_picker_key_down))`, `self.typing(&self.dialog_focus, Self::split_picker_key_down, cx)`; title "Open workspaces in windows"; a `search_box` with `picker.filter_input()`; one row per `picker.visible()` entry: `list_row(("split-row", index), index == picker.cursor() && picker.in_list(), theme)` holding the ✓ box from `task_dialog.rs:359-395`, `workspace_avatar(&row.name, color, theme)`, the name, the sidebar's working pill (`status_dot(theme.status_working, false)` + `"{n} working"`) when `working > 0` and `? N` in `theme.status_needs_input` when `asking > 0`; a click on the row calls `picker.toggle(row.id)`; buttons: `button("split-cancel", "Cancel", theme, false)` and `button("split-go", picker.button_label(), theme, true)` (`disabled_button` while `!picker.can_confirm()`).
  While `picker.confirming()`: the body is `picker.confirmation(|id| name)` and the buttons are `button("split-back", "Back", theme, false)` and `button("split-open", picker.button_label(), theme, true)`.

```rust
    pub(crate) fn split_picker_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.dialog_nav(event, true, window, cx, |_, _| {}) {
            return;
        }
        cx.stop_propagation();
        let Some(picker) = self.split_picker.as_mut() else {
            return;
        };
        match picker.key(&event.keystroke) {
            PickOutcome::Close => {
                self.split_picker = None;
                self.focus_pane(window, cx);
            }
            PickOutcome::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    picker.insert(&text);
                }
            }
            PickOutcome::Done(plan) => {
                self.split_picker = None;
                self.split_into_windows(plan, window, cx);
            }
            PickOutcome::Edited => {}
        }
        cx.notify();
    }
```

- [ ] **Step 4: Split and combine.**

```rust
    /// The asking window shows `plan.keep`; every other ticked workspace opens in a new
    /// window, cascading from it; the mode is saved.
    pub(crate) fn split_into_windows(&mut self, plan: SplitPlan, window: &mut Window, cx: &mut Context<'_, Self>) {
        let space = (self.host_of(window).space()).unwrap_or_else(|| self.primary_or_first());
        let host = HostId::Space(space);
        self.set_window_mode(WindowMode::PerWorkspace, cx);
        if !plan.keep.is_nil() {
            self.activate_workspace(host, plan.keep, window, cx);
        }
        let mut step = 0;
        for project in plan.open {
            if self.space_of(project).is_some() {
                continue;
            }
            step += 1;
            let (bounds, placement) = self.new_space_bounds(host, step);
            self.open_space_window(Some(project), bounds, placement, None, cx);
        }
        // The asking window keeps the keyboard once the new ones are up.
        self.in_window(host, cx, |_, window, _| window.activate_window());
    }

    /// Mode 1 again (spec 4.3): the asking window keeps its workspace and shows the sidebar;
    /// every other Space window closes. Detached windows stay.
    pub(crate) fn combine_windows(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let keep = (self.host_of(window).space()).unwrap_or_else(|| self.primary_or_first());
        let spaces: Vec<SpaceKey> = self.spaces_shown().map(|(key, _)| key).collect();
        let closing = plan_combine(keep, &spaces);
        for key in &closing {
            self.close_space_window(*key, cx);
        }
        self.set_window_mode(WindowMode::Single, cx);
        if !closing.is_empty() {
            self.toast_info(format!("Combined {} windows", closing.len()), cx);
        }
        cx.notify();
    }

    /// Sets and saves `[ui] window_mode`.
    pub(crate) fn set_window_mode(&mut self, mode: WindowMode, cx: &mut Context<'_, Self>) {
        self.window_mode = mode;
        let value = format!("\"{}\"", mode.name());
        if let Err(error) =
            mkrew_core::settings::write_ui_value(&self.settings_file, "window_mode", &value)
        {
            self.settings_write_failed("the window mode", error, cx);
        }
        self.windows_dirty = true;
        cx.notify();
    }
```

  `Shell::run`: `Command::SplitWindows => self.open_split_picker(window, cx), Command::CombineWindows => self.combine_windows(window, cx),`.
  The Split ▾ layout menu gets a separator and one row at its end: "Open each workspace in its own window…" in mode 1 with 2+ workspaces, "Combine into one window" in mode 2.

- [ ] **Step 5: Settings dialog.** In `apply_settings_effect`, `Effect::Write { setting, text }` with `setting.id() == "ui.window_mode"`: `per_workspace` while in mode 1 closes the Settings dialog (the `Effect::Close` path) and calls `open_split_picker` without writing (the split writes it); `single` while in mode 2 closes the dialog and calls `combine_windows`; the current value does nothing. Other settings unchanged.

- [ ] **Step 6: Hand edits.** In `apply_settings_file`, after the load: when `settings.ui.window_mode != self.window_mode`: to `PerWorkspace`, `self.window_mode = PerWorkspace` (the open windows become Space windows of their workspaces; no picker); to `Single`, `self.in_window(HostId::Space(self.primary_or_first()), cx, |shell, window, cx| shell.combine_windows(window, cx))`. `set_window_mode` writes the value the file already holds, so the next tick sees no change.

- [ ] **Step 7: Tests and smoke.** `cargo test -p mkrew-ui -j 2` PASS. Smoke, dark and light, screenshots `window-modes-picker-<theme>.png` and `window-modes-confirm-<theme>.png`:
  1. mode 1, palette "Window: Open each workspace in its own window…": the picker shows with the shown workspace ticked; type to filter; ↓ then Space ticks; Alt+A; three ticked, Enter → confirmation; Esc goes back; Enter, Enter → three windows, the asking one keeps the keyboard;
  2. Esc in the picker: still mode 1, `settings.toml` unchanged;
  3. Settings → Windows → per_workspace: the Settings dialog closes and the picker opens;
  4. palette "Window: Combine into one window" from the second window: it becomes the single window with the sidebar, toast "Combined 2 windows", torn-off windows stay;
  5. edit `settings.toml` by hand to `per_workspace` (switches without a picker), then to `"tabs"` (settings error shown, nothing changes).

- [ ] **Step 8: fmt, clippy, commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -j 4 -- -D warnings
git status --short
git add <this task's files by path> docs/screenshots/window-modes-*.png
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): open each workspace in its own window, combine back into one"
```

---

### Task 9: "Need you" chip

**Files:**
- Modify: `crates/mkrew-ui/src/shell/window_mode.rs` (chip and menu)
- Modify: `crates/mkrew-ui/src/shell/chrome.rs` (`render_title_bar` right group, before the limits chip)
- Modify: `crates/mkrew-ui/src/main.rs` (field `need_you_menu: bool`)
- Modify: `crates/mkrew-ui/src/shell/menu_render.rs` (`Anchor::NeedYou`; `menu_open()` and `open_menu_rows` include the menu)
- Modify: `crates/mkrew-ui/src/theme.rs` (`NEED_YOU_FULL_MIN_WIDTH`)

**Interfaces:**
- Consumes: `window_mode::need_you` (Task 2), `Shell::inbox_entries()`, `route_workspace` (Task 7).
- Produces: `Shell::render_need_you_chip(&self, host: HostId, theme: &Theme, window: &Window, cx) -> Option<AnyElement>`, `Shell::open_need_you(window, cx)`, `Shell::need_you_rows(&self, host: HostId) -> Rows`.

- [ ] **Step 1:** The decisions are tested in Task 2 (`need_you_counts_other_workspaces_only_newest_first`); this task only draws them.

- [ ] **Step 2: Chip.** Only for a Space host in mode 2 while `need_you(&self.inbox_entries(), self.workspace_of(host))` is `Some(need)`:

```rust
        let narrow = window.viewport_size().width < px(theme::NEED_YOU_FULL_MIN_WIDTH);
        let chip = div()
            .id("need-you")
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .h(theme::rems_px(theme::CONTROL_HEIGHT))
            .px(px(space::S))
            .rounded(px(radius::PILL))
            .bg(rgb(theme.warning_bg))
            .text_size(font_size::META)
            .text_color(rgb(theme.status_needs_input))
            .pressable(Look::Chip, true, theme)
            .tooltip(tooltip(
                theme,
                "Sessions in other workspaces need you",
                self.keymap.label(Command::NextInbox),
            ))
            .on_press(true, cx.listener(|shell, _, window, cx| shell.open_need_you(window, cx)))
            .child(status_dot(theme.status_needs_input, false))
            .child(if narrow { need.short } else { need.label })
            .child(record_bounds(Anchor::NeedYou, cx));
```

  `theme.rs`: `/// Below this window width the need-you chip shows only its count (ADR 0113).` `pub const NEED_YOU_FULL_MIN_WIDTH: f32 = 960.0;`.

- [ ] **Step 3: Menu.** `open_need_you`: `self.begin_menu(window, cx); self.need_you_menu = true; self.menu_host = self.host_of(window); self.finish_menu(window.last_input_was_keyboard(), window, cx);`. `need_you_rows(host)`: one `MenuItem::new(row.text, act(move |shell, window, cx| shell.route_workspace(row.project, Then::Session(row.session), false, None, window, cx))).lead(Lead::Avatar { name, color })` per `NeedRow`, newest first (name and color from the project). `render_space` draws it with `render_menu(self.menu_state(), "need-you-menu", rows, None, Place::At(self.menu_anchor(&Anchor::NeedYou, window, cx)), theme, cx)` when `self.need_you_menu && self.menu_host == host`. `close_menus` resets `need_you_menu`.

- [ ] **Step 4: Smoke.** Mode 2, two windows; a fakeagent in `web` asks for input while `api` is focused: `● 1 need you · web` shows in `api`'s window, not in `web`'s; click → the row → the `web` window comes forward with the session focused; below 960 px only `● 1`. Screenshots `window-modes-need-you-<theme>.png`, dark and light.

- [ ] **Step 5: fmt, clippy, tests, commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -j 4 -- -D warnings && cargo test -p mkrew-ui -j 2
git status --short
git add <this task's files by path> docs/screenshots/window-modes-need-you-*.png
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): need-you chip for other workspaces in a window per workspace"
```

---

### Task 10: ADR, SPEC, HUMAN_CHECKS, final verify

**Files:**
- Create: `docs/decisions/0113-window-modes.md`
- Modify: `docs/SPEC.md` (§10 new "### Window modes (ADR 0113)" after "### Detached windows (ADR 0096)" ~1194, and the Detached windows bullets whose "main window" rule changed; §13 the three commands; §20.0 the three keys)
- Modify: `docs/HUMAN_CHECKS.md` ("To verify": "2026-11-25 Window modes")
- Modify: `docs/PROGRESS.md` ("Notes for the next session", newest first)

- [ ] **Step 1: ADR 0113.** Follow the shape of `docs/decisions/0112-*.md`. Context: the user wants a window per workspace for several screens, focus and Alt+Tab, and to switch back to one window. Decision: one `Shell`; Space windows (`HostId::Space`) drawn like detached ones; per-window workspace, panel and scroll state; `[ui] window_mode`, `open_workspace`, `panel_side`; `windows.json` version 2 (`main-window.json` read once); `Desktop` badges every Space window. Rejected: a `Shell` per window (N daemon connections, duplicated tray and badge state), a UI process per workspace (N tray icons, breaks single instance, no tab drags between windows), a hidden reusable main window. Consequences: amends ADR 0096 (no main window; "Move back" goes to the workspace's window); an answer for a window that closed meanwhile is dropped.

- [ ] **Step 2: SPEC.** §10 "Window modes" from spec sections 3.4, 3.5, 4 and 5 (one sentence per line, no em dash); §13 rows for `split_windows`, `combine_windows`, `toggle_panel_side` (no default keys); §20.0 rows for the three keys and defaults.

- [ ] **Step 3: HUMAN_CHECKS "2026-11-25 Window modes"**: Alt+Tab and the taskbar show one entry per workspace window with the badge on each; windows dragged to other screens come back there after a restart; macOS: app menu Quit and Check for updates from any window, Dock reopen shows every window, full screen per window; Linux: title bar unchanged; screenshots in `docs/screenshots/window-modes-*` look right in both themes.

- [ ] **Step 4: PROGRESS note**, one line at the top of "Notes for the next session":
  `- 2026-11-25 Window modes (branch feat/window-modes, spec docs/superpowers/specs/2026-11-25-window-modes-design.md, plan docs/superpowers/plans/2026-11-25-window-modes.md, ADR 0113, no protocol/migration): HostId::Space replaces HostId::Main, SpaceWindow, per-window workspace/panel/scroll state, windows.json v2, [ui] window_mode / open_workspace / panel_side, route_workspace / in_workspace, split picker, need-you chip. Checks in HUMAN_CHECKS "2026-11-25 Window modes".`

- [ ] **Step 5: Final verify.** `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (or the fallback in Global Constraints). Fix every failure, including ones this branch did not cause.

- [ ] **Step 6: Commit**

```bash
git add docs/decisions/0113-window-modes.md docs/SPEC.md docs/HUMAN_CHECKS.md docs/PROGRESS.md
git -c user.email=tungnv975@gmail.com commit -m "docs: ADR 0113 window modes, SPEC, human checks"
```
