# Files tree in the right panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the top-level Files view: the file tree lives in a `Files │ Session` right panel, a clicked file opens in a center `Editor` view, and tree rows drag onto agent panes as `@path` references.

**Architecture:** The per-workspace `FilesView` model stays as it is and is drawn in two places: `render_file_tree` (the right panel's Files tab) and `render_editor` (the center `View::Editor`).
The session panel state `inspector_open: bool` becomes a pure `RightPanel { open, tab }`, persisted as `[ui] right_panel`.
Tree rows carry a gpui `on_drag(DraggedPath)`; panes accept it with a plain `on_drop` and type `composer::dropped_ref(...)` through the existing composer / terminal delivery.

**Tech Stack:** Rust, gpui-pre 0.3.7, `mkrew-core` settings (TOML, serde), `insta` snapshots.

**Spec:** `docs/superpowers/specs/2026-10-27-files-right-panel-design.md`

## Global Constraints

- Every color, spacing, radius and font size comes from `crates/mkrew-ui/src/theme.rs` (SPEC §14); no literals in render code.
- No `unwrap` / `expect` outside tests unless provably infallible (comment why).
- `[ui] right_panel` values: `"files"`, `"session"`, `"hidden"`; default `"hidden"`.
- Panel width: `[ui] inspector_width`, kept 240-560 px (`theme::INSPECTOR_MIN_WIDTH` / `INSPECTOR_MAX_WIDTH`).
- Keys: `ShowFileTree` = `ctrl-shift-3` (name `show_file_tree`), `ShowEditor` = `ctrl-shift-6` (name `show_editor`), `ToggleInspector` = `ctrl-shift-y` (unchanged).
- Drop text: agent pane `file_ref([composer] file_ref, path)` + one space; shell pane `file_ref("{path}", path)` + one space; folders end with `/`; never an Enter.
- Clean cutover: no `View::Files`, `Command::ShowFiles`, `FILES_CONTEXT`, `FILE_TREE_WIDTH` or `inspector_open` left anywhere (code, tests, comments).
- Verify with `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL). On OOM / linker errors re-run cargo with `-j 2`.
- Commit with `git -c user.email=tungnv975@gmail.com commit`, staging only the task's files. Never push.
- Docs: one sentence per line in new Markdown; never the em dash character.

## Review Focus

1. A click on a tree row must still open the file (gpui starts a drag only past its threshold); a press-and-move must not also open it. Smoke-check in Task 6.
2. `git.changed` while only the tree is on screen (Terminals view, panel on Files) must update tree markers: `files_event` keys off `files_on_screen()`, not the view. Smoke-check in Task 6.
3. Hiding the panel while the tree has the keyboard must give the keyboard back (editor in Editor, pane in Terminals), never to an invisible tree. Covered in Task 3 Step 6.
4. Switching worktree from the tree's target chooser with unsaved tabs must ask first, then (no tabs left) leave the Editor view for Terminals. Covered in Task 4 Step 7.
5. A drop while the pane's composer is open goes into the draft, not the terminal. Covered by reusing `type_into_pane` in Task 5; smoke-check in Task 6.

---

### Task 1: `[ui] right_panel` setting

**Files:**
- Modify: `crates/mkrew-core/src/settings.rs` (near `SidebarMode` ~305-326, `UiSettings` ~337-382, template ~536-548, tests ~1120-1140)
- Modify: `crates/mkrew-core/src/settings_fields.rs:219-221`

**Interfaces:**
- Produces: `mkrew_core::settings::RightPanelMode { Files, Session, #[default] Hidden }` with `fn name(self) -> &'static str`; field `UiSettings::right_panel: RightPanelMode`.

- [ ] **Step 1: Write the failing test** (in `settings.rs` tests, next to the `sidebar` parse test)

```rust
#[test]
fn right_panel_parses_defaults_to_hidden_and_round_trips() {
    let ui = |src: &str| parse(Path::new("s.toml"), src).unwrap().ui;
    assert_eq!(ui("").right_panel, RightPanelMode::Hidden);
    assert_eq!(ui("[ui]\nright_panel = \"files\"").right_panel, RightPanelMode::Files);
    assert_eq!(ui("[ui]\nright_panel = \"session\"").right_panel, RightPanelMode::Session);
    assert!(parse_err("[ui]\nright_panel = \"left\"").contains("unknown variant `left`"));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.toml");
    std::fs::write(&path, template()).unwrap();
    for mode in [RightPanelMode::Files, RightPanelMode::Session, RightPanelMode::Hidden] {
        write_ui_value(&path, "right_panel", &format!("\"{}\"", mode.name())).unwrap();
        assert_eq!(load(&path).unwrap().ui.right_panel, mode);
    }
}
```

- [ ] **Step 2: Run it, expect a compile failure** (`RightPanelMode` unknown)

Run: `cargo test -p mkrew-core right_panel_parses`

- [ ] **Step 3: Implement**

After `impl SidebarMode`:

```rust
/// `[ui] right_panel`: the right panel next to Terminals / Editor at start, and its tab
/// (SPEC §10). The panel toggle and its tab row write it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RightPanelMode {
    /// Open on the file tree.
    Files,
    /// Open on the session cards.
    Session,
    #[default]
    Hidden,
}

impl RightPanelMode {
    pub fn name(self) -> &'static str {
        match self {
            RightPanelMode::Files => "files",
            RightPanelMode::Session => "session",
            RightPanelMode::Hidden => "hidden",
        }
    }
}
```

In `UiSettings`, after `sidebar`:

```rust
    /// `[ui] right_panel`: hidden, or open on the Files or Session tab at start.
    pub right_panel: RightPanelMode,
```

In `Default for UiSettings` add `right_panel: RightPanelMode::Hidden,`.
Fix the other struct literal (~line 796, a test expectation) the same way.
In the template, after `# sidebar_width = 280\n\` add `# right_panel = \"hidden\"\n\`.
Update the `inspector_width` doc comment: "Width of the right panel (Files / Session tabs), px (SPEC §10); dragging its edge saves it."
In `settings_fields.rs` add `hidden("ui", "right_panel"),` after `hidden("ui", "sidebar"),`.

- [ ] **Step 4: Run the crate tests**

Run: `cargo test -p mkrew-core`
Expected: PASS (if a template snapshot changed, review it with `cargo insta review` and accept only the added `right_panel` line).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-core/src
git -c user.email=tungnv975@gmail.com commit -m "feat(core): [ui] right_panel setting"
```

---

### Task 2: Pure models - `RightPanel`, `View::bar`, `dropped_ref`

**Files:**
- Modify: `crates/mkrew-ui/src/inspector.rs` (top + tests module)
- Modify: `crates/mkrew-ui/src/composer.rs` (after `file_ref` ~183, tests ~468)

No `View` change here (Task 4 renames it together with its callers).

**Interfaces:**
- Produces:
  - `mkrew_ui::inspector::PanelTab { #[default] Files, Session }`
  - `mkrew_ui::inspector::RightPanel { pub open: bool, pub tab: PanelTab }` (`Default` = closed on Files) with `from_mode(RightPanelMode) -> Self`, `mode(self) -> RightPanelMode`, `show(&mut self, PanelTab)`, `shows(self, PanelTab) -> bool`.
  - `mkrew_ui::composer::dropped_ref(template: &str, path: &str, is_dir: bool, agent: bool) -> String`.

- [ ] **Step 1: Write the failing tests**

In `inspector.rs` tests:

```rust
#[test]
fn the_right_panel_round_trips_through_its_saved_mode() {
    use mkrew_core::settings::RightPanelMode;
    for mode in [RightPanelMode::Files, RightPanelMode::Session, RightPanelMode::Hidden] {
        assert_eq!(RightPanel::from_mode(mode).mode(), mode);
    }
    // Hidden at start: shown again on the file tree.
    let mut panel = RightPanel::from_mode(RightPanelMode::Hidden);
    assert!(!panel.shows(PanelTab::Files));
    panel.open = true;
    assert!(panel.shows(PanelTab::Files));
    // Hiding keeps the tab for the next show.
    panel.show(PanelTab::Session);
    panel.open = false;
    assert_eq!(panel.mode(), RightPanelMode::Hidden);
    panel.open = true;
    assert!(panel.shows(PanelTab::Session));
}
```

In `composer.rs` tests:

```rust
#[test]
fn dropped_refs_follow_the_pane_kind_and_mark_folders() {
    assert_eq!(dropped_ref("@{path}", "src/main.rs", false, true), "@src/main.rs ");
    assert_eq!(dropped_ref("@{path}", "src", true, true), "@src/ ");
    assert_eq!(dropped_ref("@{path}", "src/main.rs", false, false), "src/main.rs ");
    assert_eq!(dropped_ref("@{path}", "docs/my notes", true, false), "\"docs/my notes/\" ");
    assert!(!dropped_ref("@{path}", "a.rs", false, true).contains('\n'), "never an Enter");
}
```

- [ ] **Step 2: Run them, expect compile failures**

Run: `cargo test -p mkrew-ui right_panel_round_trips dropped_refs_follow`

- [ ] **Step 3: Implement**

`inspector.rs`, after `width_from_pointer`:

```rust
/// The right panel's tabs (SPEC §10, ADR 0081).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PanelTab {
    #[default]
    Files,
    Session,
}

/// The right panel next to Terminals / Editor: open or not, and its tab (kept while hidden).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RightPanel {
    pub open: bool,
    pub tab: PanelTab,
}

impl RightPanel {
    /// The panel `[ui] right_panel` asks for at start.
    pub fn from_mode(mode: RightPanelMode) -> Self {
        match mode {
            RightPanelMode::Hidden => Self::default(),
            RightPanelMode::Files => Self { open: true, tab: PanelTab::Files },
            RightPanelMode::Session => Self { open: true, tab: PanelTab::Session },
        }
    }

    /// What `[ui] right_panel` is saved as.
    pub fn mode(self) -> RightPanelMode {
        match (self.open, self.tab) {
            (false, _) => RightPanelMode::Hidden,
            (true, PanelTab::Files) => RightPanelMode::Files,
            (true, PanelTab::Session) => RightPanelMode::Session,
        }
    }

    /// Opens the panel on `tab`.
    pub fn show(&mut self, tab: PanelTab) {
        self.open = true;
        self.tab = tab;
    }

    /// The panel is open on `tab`.
    pub fn shows(self, tab: PanelTab) -> bool {
        self.open && self.tab == tab
    }
}
```

Add `use mkrew_core::settings::RightPanelMode;` to the imports.
Update the module doc: "The right panel next to Terminals / Editor (SPEC §10, ADR 0038, ADR 0081): its tabs, width, the Stop rule and the text of the Session card."

`composer.rs`, after `file_ref`:

```rust
/// What a tree row dropped on a pane types (§20B.5, ADR 0081): `composer.file_ref` on an
/// agent pane, the bare path on a shell (it has no `@`); a folder ends with `/`. One space
/// after it keeps two drops in a row apart; never an Enter.
pub fn dropped_ref(template: &str, path: &str, is_dir: bool, agent: bool) -> String {
    let path = if is_dir {
        format!("{}/", path.trim_end_matches('/'))
    } else {
        path.to_owned()
    };
    let template = if agent { template } else { "{path}" };
    format!("{} ", file_ref(template, &path))
}
```

- [ ] **Step 4: Run them**

Run: `cargo test -p mkrew-ui right_panel_round_trips dropped_refs_follow`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/inspector.rs crates/mkrew-ui/src/composer.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): right panel model and dropped tree refs"
```

---

### Task 3: The right panel with `Files │ Session` tabs; the tree moves into it

After this task the Files view (still `View::Files`) shows only the editor; the tree shows in the panel in the Terminals and Files views.

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs` (`struct Shell` fields ~543-551)
- Modify: `crates/mkrew-ui/src/shell/connection.rs:24-31,246-250`
- Modify: `crates/mkrew-ui/src/shell/inspector.rs:5-177`
- Modify: `crates/mkrew-ui/src/shell/files_render.rs:5-187`
- Modify: `crates/mkrew-ui/src/shell/chrome.rs:21-70`
- Modify: `crates/mkrew-ui/src/shell/thread_panel.rs:33-38`
- Modify: `crates/mkrew-ui/src/shell/focus.rs` (new helper next to `focused_session`)
- Modify: `crates/mkrew-ui/src/shell/workspace.rs:20-53,343-380`
- Modify: `crates/mkrew-ui/src/shell/files.rs:206-221,830-849`
- Modify: `crates/mkrew-ui/src/main.rs:908-916` (tree menu condition)
- Modify: `crates/mkrew-ui/src/theme.rs:370-371` (remove `FILE_TREE_WIDTH`)

**Interfaces:**
- Consumes: `RightPanel`, `PanelTab`, `RightPanelMode` (Tasks 1-2).
- Produces (all `pub(crate)` on `Shell`):
  - field `right_panel: inspector::RightPanel` (replaces `inspector_open`)
  - `fn panel_shown(&self) -> bool` - open and the view is one the panel lives in
  - `fn file_tree_shown(&self) -> bool` - `panel_shown()` on the Files tab
  - `fn files_on_screen(&self) -> bool` - the Files view or the tree is drawn
  - `fn panel_session(&self) -> Option<Ulid>` - the active tab's focused pane, whatever the view
  - `fn show_panel_tab(&mut self, tab: PanelTab, window, cx)`
  - `fn render_right_panel(&self, project_id, now, theme, window, cx) -> AnyElement` (was `render_inspector`)
  - `fn render_file_tree(&self, project_id, theme, window, cx) -> AnyElement`
  - `fn render_editor(&self, project_id, theme, window, cx) -> AnyElement` (was `render_files`)

- [ ] **Step 1: State and start-up**

In `main.rs` replace the `inspector_open` field with:

```rust
    /// The right panel (Files / Session tabs) next to Terminals and the Editor; from
    /// `[ui] right_panel`, written by its toggle and tab row (SPEC §10, ADR 0081).
    right_panel: inspector::RightPanel,
```

In `connection.rs` read `let right_panel = inspector::RightPanel::from_mode(ui.right_panel);` next to `sidebar_mode` and replace `inspector_open: false,` with `right_panel,`.

- [ ] **Step 2: Helpers** (`focus.rs`, after `focused_session`)

```rust
    /// The views the right panel is drawn next to.
    pub(crate) fn panel_shown(&self) -> bool {
        self.right_panel.open && matches!(self.view(), View::Terminals | View::Files)
    }

    /// The right panel shows the file tree.
    pub(crate) fn file_tree_shown(&self) -> bool {
        self.panel_shown() && self.right_panel.tab == PanelTab::Files
    }

    /// Some of the active workspace's Files view is drawn (the editor or the tree), so it
    /// follows `git.changed` and refreshes when it comes back.
    pub(crate) fn files_on_screen(&self) -> bool {
        self.view() == View::Files || self.file_tree_shown()
    }

    /// The pane the Session tab describes: the active tab's focused pane, also while the
    /// Editor is shown (unlike `focused_session`, which is where keys go).
    pub(crate) fn panel_session(&self) -> Option<Ulid> {
        let tabs = self.tabs.get(&self.workspace?)?;
        tabs.active()?.focused()
    }
```

Task 4 replaces `View::Files` with `View::Editor` in these.
Add `use mkrew_ui::inspector::{PanelTab, RightPanel};` where `main.rs` imports `inspector` (`use mkrew_ui::inspector;` ~line 75 becomes `use mkrew_ui::inspector::{self, PanelTab};`).

- [ ] **Step 3: Toggle, tab switch, persistence** (`shell/inspector.rs`, replace `toggle_inspector`)

```rust
    /// `ToggleInspector` / the title bar's `◨`: hides or shows the right panel (its tab is
    /// kept); from a view without it, shows Terminals with the panel open.
    pub(crate) fn toggle_inspector(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.workspace.is_none() {
            return;
        }
        if !matches!(self.view(), View::Terminals | View::Files) {
            self.right_panel.open = true;
            self.show_view(View::Terminals, window, cx);
        } else {
            self.right_panel.open = !self.right_panel.open;
        }
        self.panel_changed(window, cx);
    }

    /// A click on the panel's `Files` / `Session` tab.
    pub(crate) fn show_panel_tab(
        &mut self,
        tab: PanelTab,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.right_panel.show(tab);
        self.panel_changed(window, cx);
    }

    /// After the panel opened, closed or switched tab: a tree that came on screen is
    /// refreshed, a hidden one gives the keyboard back, the grip of a hidden panel ends its
    /// drag, and `[ui] right_panel` is saved.
    fn panel_changed(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if !self.right_panel.open {
            // The grip is gone: no mouse-up will end a drag in progress.
            self.end_inspector_drag(cx);
        }
        if self.file_tree_shown() {
            if let Some(project_id) = self.workspace {
                self.refresh_files(project_id, window, cx);
            }
        } else if self.files_focus.is_focused(window) {
            if self.view() == View::Files {
                window.focus(&self.file_editor_focus, cx);
            } else {
                self.focus_pane(window, cx);
            }
        }
        let value = format!("\"{}\"", self.right_panel.mode().name());
        if let Err(error) =
            mkrew_core::settings::write_ui_value(&self.settings_file, "right_panel", &value)
        {
            self.settings_write_failed("the right panel", error, cx);
        }
        cx.notify();
    }
```

In `inspector_toggle`, use `let open = self.panel_shown();` and the tooltips `"Hide right panel"` / `"Show right panel"`.
In `thread_panel.rs::sync_thread` use `self.panel_session()` instead of `self.focused_session()` and pass `self.panel_shown() && self.right_panel.tab == PanelTab::Session` to `inspector::wanted_thread`.

- [ ] **Step 4: Split the Files rendering** (`shell/files_render.rs`)

Rename `render_files` to `render_editor` and reduce it to the editor column:

```rust
    /// The Files view (SPEC §12A): the opened files' tabs, header and editor / preview. The
    /// tree is in the right panel (`render_file_tree`).
    pub(crate) fn render_editor(
        &self,
        project_id: Ulid,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let Some(view) = self.files.get(&project_id) else {
            return message(theme, None, "Loading files…".into(), None)
                .flex_1()
                .into_any_element();
        };
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .font_family(theme.ui_font_family.clone())
            .child(self.render_file(project_id, view, window, theme, cx))
            .into_any_element()
    }
```

Add `render_file_tree` holding the rest of the old body verbatim (the `empty` closure, `search`, `ignored`, `header`, the `tree` match) and returning the old tree column, minus the fixed width, fill and border (the panel draws those):

```rust
    /// The right panel's Files tab (SPEC §12A): search, Ignored, the worktree chooser, a
    /// "New file…" field and the lazy tree with git markers.
    pub(crate) fn render_file_tree(
        &self,
        project_id: Ulid,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        // (old lines 15-143 of `render_files`, unchanged, building `header` and `tree`)
        div()
            .id("files-tree")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .py(px(space::S))
            .track_focus(&self.files_focus)
            .on_key_down(cx.listener(Self::files_key_down))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |shell, event: &gpui::MouseDownEvent, _, cx| {
                    shell.open_file_menu(project_id, String::new(), true, event.position, cx);
                }),
            )
            .child(header)
            .children(
                self.target_chooser(project_id, view.worktree_task_id, false, theme, cx)
                    .map(|chooser| chooser.px(px(space::S)).pb(px(space::S))),
            )
            .children(self.render_new_entry(project_id, view.worktree_task_id, theme, cx))
            .child(tree)
            .into_any_element()
    }
```

The "(old lines …)" marker above is an instruction, not code: move those statements in, keeping the comment "Right click on the header or the blank part of the tree = the folder itself (rows stop the event first)." on the right-click handler.
The `let Some(view) = self.files.get(&project_id) else { return empty("Loading files…", None).into_any_element(); };` guard moves too.
Change the search tooltip comment "the 280 px tree has no room" to "the narrow panel has no room".
Delete `theme::FILE_TREE_WIDTH` and its doc comment from `theme.rs`.

- [ ] **Step 5: The panel with its tab row** (`shell/inspector.rs`)

Rename `render_inspector` to `render_right_panel`, add a `window: &Window` parameter, and build the body per tab:

```rust
        let tab = self.right_panel.tab;
        let tabs = div()
            .flex_none()
            .p(px(space::S))
            .child(segmented(theme).children([PanelTab::Files, PanelTab::Session].map(
                |each| {
                    let title = match each {
                        PanelTab::Files => "Files",
                        PanelTab::Session => "Session",
                    };
                    segment(SharedString::from(format!("panel-{title}")), each == tab, theme)
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.show_panel_tab(each, window, cx);
                        }))
                        .child(title)
                },
            )));
        let body = match tab {
            PanelTab::Files => self.render_file_tree(project_id, theme, window, cx),
            PanelTab::Session => {
                // (the old `session`, `thread`, `cards` code and the `match thread` element)
            }
        };
```

The "(the old … code)" marker is an instruction: the old `session` lookup (now `self.panel_session()`), `thread`, `cards` and the `match thread { … }` element move into that arm unchanged, ending in `.into_any_element()`.
The outer column keeps its old styling and grip and becomes `.flex().flex_col().child(tabs).child(div().flex_1().min_h_0().flex().flex_col().child(body))`.
If `segmented` stretches wrong in a 240 px column, give the row `.w_full()` children via `.flex_1()` on each segment; use theme tokens only.

- [ ] **Step 6: Layout** (`shell/chrome.rs::render_main`)

```rust
        let panel = self
            .panel_shown()
            .then(|| self.render_right_panel(project_id, now, theme, window, cx));
```

Compute `panel` before the `match`; in the Terminals arm replace `.when(self.inspector_open, …)` with `.children(panel)`; the Files arm becomes:

```rust
            (View::Files, _) => main.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.render_editor(project_id, theme, window, cx))
                    .children(panel),
            ),
```

(`panel` is an `Option<AnyElement>`; move it into whichever arm uses it.)

- [ ] **Step 7: Who refreshes and follows the tree**

- `files.rs::files_event`: `self.workspace.filter(|_| self.files_on_screen())`.
- `files.rs::drop_gone_targets`: `if shown == Some(project_id) && self.files_on_screen()`.
- `workspace.rs::activate_workspace`: `if switched && self.files_on_screen()`.
- `workspace.rs::show_view`: replace the `view != View::Terminals` drag guard with `!matches!(view, View::Terminals | View::Files)`; replace the Files block with

```rust
        let tree_before = self.files_on_screen();
        // (the existing `let shown = self.views.insert(...)` line)
        if self.files_on_screen() && !tree_before {
            self.refresh_files(project_id, window, cx);
        }
        // A tree click keeps the keyboard in the tree; any other way in types in the editor.
        if view == View::Files && !self.files_focus.is_focused(window) {
            window.focus(&self.file_editor_focus, cx);
        }
```

  (`tree_before` is read before the insert; keep `shown` for the Docs / Git blocks.)
- `main.rs` ~912: the tree menu shows while `self.workspace == Some(menu.project_id) && self.file_tree_shown() && !self.modal_open()`.

- [ ] **Step 8: Build and run the UI tests**

Run: `cargo test -p mkrew-ui`
Expected: PASS (no test referenced the moved render code).

- [ ] **Step 9: Smoke**

Launch the UI (`cargo run -p mkrew-ui`; a locked exe is renamed to `*.locked-old.exe`, never kill the daemon).
Check: `◨` opens the panel on Files; the tree lists; `Session` shows the cards; Ctrl+Shift+Y hides it; restart restores the tab; the Files view shows the editor full width with the panel beside it.

- [ ] **Step 10: Commit**

```bash
git add crates/mkrew-ui/src
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): file tree in the right panel's Files tab"
```

---

### Task 4: `View::Files` → `View::Editor`, shown only with open tabs; new commands

**Files:**
- Modify: `crates/mkrew-ui/src/board.rs:22-67`, `crates/mkrew-ui/src/board/tests.rs:5-19`
- Modify: `crates/mkrew-ui/src/keymap.rs` (`ShowFiles` ~89, `all()` ~253, `name()` ~343, `default_keys()` ~445, contexts ~179-190, `context()` ~507-511, tests ~984-1138)
- Modify: `crates/mkrew-ui/src/palette.rs` (`Context` ~23-54, labels ~239, availability ~313-358), `crates/mkrew-ui/src/palette/tests.rs`
- Modify: `crates/mkrew-ui/src/main.rs:78,747-750,920-925`
- Modify: every remaining `View::Files` / `Command::ShowFiles` / `FILES_CONTEXT` use: `shell/chrome.rs`, `shell/composer.rs:231-256`, `shell/files.rs:641`, `shell/navigation.rs:310,369-373`, `shell/send_dialog.rs:180`, `shell/sessions.rs:184,243,376,412,539`, `shell/workspace.rs`, `shell/focus.rs`, `shell/inspector.rs`, `shell/file_tabs.rs`
- Snapshots: `crates/mkrew-ui/src/**/snapshots/*.snap` that list commands

**Interfaces:**
- Produces:
  - `View::Editor` (no `View::Files`), `View::ALL = [Terminals, Board, Git, Docs, Editor]`, `View::bar(docs: bool, editor: bool) -> impl Iterator<Item = View>`
  - `Command::ShowFileTree` (`show_file_tree`, `ctrl-shift-3`), `Command::ShowEditor` (`show_editor`, `ctrl-shift-6`)
  - `keymap::EDITOR_CONTEXT = "Editor"`, `keymap::FILE_TREE_CONTEXT = "FileTree"`, `FIND_CONTEXT = "Terminal || Editor || Docs"`
  - `palette::Context::file_tabs: usize`
  - `Shell::has_file_tabs(&self, project_id) -> bool`, `Shell::follow_file_tabs(&mut self, project_id, window, cx)`, `Shell::show_file_tree(&mut self, window, cx)`

- [ ] **Step 1: Write the failing tests**

Replace `board/tests.rs::each_view_maps_to_its_own_show_command`:

```rust
#[test]
fn each_view_maps_to_its_own_show_command() {
    use crate::keymap::Command;
    let commands: Vec<Command> = View::ALL.into_iter().map(View::command).collect();
    assert_eq!(
        commands,
        [
            Command::ShowTerminals,
            Command::ShowBoard,
            Command::ShowGit,
            Command::ShowDocs,
            Command::ShowEditor
        ]
    );
}

#[test]
fn the_view_bar_lists_the_editor_only_with_open_files() {
    let bar = |docs, editor| View::bar(docs, editor).collect::<Vec<_>>();
    assert_eq!(bar(true, false), [View::Terminals, View::Board, View::Git, View::Docs]);
    assert_eq!(
        bar(true, true),
        [View::Terminals, View::Board, View::Git, View::Docs, View::Editor]
    );
    assert_eq!(bar(false, true), [View::Terminals, View::Board, View::Git, View::Editor]);
}
```

In `keymap.rs` tests replace the `ctrl-shift-3` assertion and the `FILES_CONTEXT` ones:

```rust
        assert_eq!(bound("ctrl-shift-3"), Some(Command::ShowFileTree));
        assert_eq!(bound("ctrl-shift-6"), Some(Command::ShowEditor));
        assert_eq!(Command::SaveFile.context(), Some(EDITOR_CONTEXT));
        // Ctrl+Shift+H only toggles ignored files while the tree has the keyboard.
        assert_eq!(Command::ToggleIgnored.context(), Some(FILE_TREE_CONTEXT));
```

and `FILES_CONTEXT` → `EDITOR_CONTEXT` in the other asserts (`FindInFile`, `ReplaceInFile`, `CloseFileTab`, the plain-Ctrl-letter rule).
In `palette/tests.rs`: `View::Files` → `View::Editor`; `ShowFiles` → `ShowFileTree`; replace the `ToggleIgnored` assertion with `assert!(commands(&context(View::Terminals)).contains(&Command::ToggleIgnored));`; add `file_tabs: 1` to the `context` helper and

```rust
    let no_files = Context { file_tabs: 0, ..context(View::Terminals) };
    assert!(!commands(&no_files).contains(&Command::ShowEditor));
    assert!(commands(&context(View::Terminals)).contains(&Command::ShowEditor));
```

- [ ] **Step 2: Run them, expect compile failures**

Run: `cargo test -p mkrew-ui view_bar each_view_maps`

- [ ] **Step 3: `View`** (`board.rs`)

```rust
/// What the main area of a workspace shows (SPEC §10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    Board,
    /// The terminal tabs; the default, so a workspace opens on its sessions.
    #[default]
    Terminals,
    /// The Markdown review (SPEC §12).
    Docs,
    /// The opened files' tabs and editor (SPEC §12A); the tree is in the right panel.
    Editor,
    /// Changed files and their diffs (SPEC §12B).
    Git,
}

impl View {
    pub const ALL: [View; 5] = [
        View::Terminals,
        View::Board,
        View::Git,
        View::Docs,
        View::Editor,
    ];

    /// The view bar's views: Docs while `[features] docs` is on, Editor while files are open.
    pub fn bar(docs: bool, editor: bool) -> impl Iterator<Item = View> {
        View::ALL
            .into_iter()
            .filter(move |&view| (view != View::Docs || docs) && (view != View::Editor || editor))
    }
```

`title()`: `View::Editor => "Editor"`; `command()`: `View::Editor => Command::ShowEditor`.

- [ ] **Step 4: Keymap**

- `ShowFiles` → `ShowFileTree` with doc "Open the right panel on the file tree and give it the keyboard."; add `ShowEditor` after it with doc "Show the Editor view (the opened files) while a file is open.".
- `all()`: `Command::ShowFileTree, Command::ShowEditor,` in place of `Command::ShowFiles,`.
- `name()`: `"show_file_tree"`, `"show_editor"`; `default_keys()`: `"ctrl-shift-3"`, `"ctrl-shift-6"`.
- Constants:

```rust
/// Key context of the window while the Editor view is shown.
pub const EDITOR_CONTEXT: &str = "Editor";

/// Key context of the file tree in the right panel while it has the keyboard.
pub const FILE_TREE_CONTEXT: &str = "FileTree";

/// Find: in a terminal's scrollback, the Editor's opened file, and the Docs list filter.
pub const FIND_CONTEXT: &str = "Terminal || Editor || Docs";
```

- `context()`: `Command::ToggleIgnored => Some(FILE_TREE_CONTEXT),` and `SaveFile | FindInFile | ReplaceInFile | CloseFileTab => Some(EDITOR_CONTEXT)`.
- Doc comments "Files only:" → "Editor only:" on `SaveFile`, `FindInFile`, `ReplaceInFile`, `CloseFileTab`; "Tree only:" on `ToggleIgnored`.
- `files_render.rs::render_file_tree`: add `.key_context(FILE_TREE_CONTEXT)` on the `files-tree` div (import it in `main.rs`).
- `main.rs:747`: `.when(self.view() == View::Editor, |el| el.key_context(EDITOR_CONTEXT))`.

- [ ] **Step 5: Palette**

- `Context`: add `/// The active workspace's open file tabs. pub file_tabs: usize,`.
- Labels: `ShowFileTree => "View: File tree"`, `ShowEditor => "View: Editor"`, `ToggleInspector => "View: Toggle right panel"` (keep the existing wording style), `ToggleIgnored => "File tree: Toggle ignored files"`, and "Files:" → "Editor:" on save / find / replace / close.
- Availability: `ShowFileTree` joins the `context.workspace` group; `Command::ShowEditor => context.workspace && context.file_tabs > 0,`; `Command::ToggleIgnored => context.workspace,`; every `in_view(View::Files)` → `in_view(View::Editor)`.
- `navigation.rs::open_palette`: `tabs` uses `View::Editor` in its match; add `file_tabs: workspace.map_or(0, |id| self.files.get(&id).map_or(0, |view| view.tabs().len())),`.

- [ ] **Step 6: Shell**

Mechanical: `View::Files` → `View::Editor` everywhere left (`rg "View::Files|ShowFiles|FILES_CONTEXT"` must return nothing at the end), including the Task 3 helpers in `focus.rs`, `inspector.rs`, `workspace.rs`, `chrome.rs`.
The view bar (`chrome.rs::render_view_tabs`):

```rust
        let editor = self.workspace.is_some_and(|id| self.has_file_tabs(id));
        segmented(theme).children(View::bar(self.features.docs, editor).map(|view| {
            // (the existing segment body)
        }))
```

`workspace.rs`:

```rust
    /// The workspace has open file tabs (the Editor view exists).
    pub(crate) fn has_file_tabs(&self, project_id: Ulid) -> bool {
        self.files.get(&project_id).is_some_and(|view| !view.tabs().is_empty())
    }
```

`view()` reads an Editor without tabs as Terminals:

```rust
            .filter(|&view| view != View::Docs || self.features.docs)
            .filter(|&view| view != View::Editor || self.workspace.is_some_and(|id| self.has_file_tabs(id)))
```

`sessions.rs::run`: `Command::ShowFileTree => self.show_file_tree(window, cx),` and `Command::ShowEditor => { if self.workspace.is_some_and(|id| self.has_file_tabs(id)) { self.show_view(View::Editor, window, cx) } }`.
`shell/inspector.rs`:

```rust
    /// Ctrl+Shift+3: the right panel on the file tree, which gets the keyboard; from a view
    /// without the panel, Terminals first.
    pub(crate) fn show_file_tree(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.workspace.is_none() {
            return;
        }
        if !matches!(self.view(), View::Terminals | View::Editor) {
            self.show_view(View::Terminals, window, cx);
        }
        self.show_panel_tab(PanelTab::Files, window, cx);
        window.focus(&self.files_focus, cx);
    }
```

- [ ] **Step 7: Enter and leave the Editor with the tabs** (`shell/file_tabs.rs`)

```rust
    /// The Editor view comes with the first open file and leaves with the last one (closed,
    /// or another worktree started the files over). Only for the shown workspace.
    pub(crate) fn follow_file_tabs(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.workspace != Some(project_id) {
            return;
        }
        let stored = self.views.get(&project_id).copied().unwrap_or_default();
        match (self.has_file_tabs(project_id), stored == View::Editor) {
            (true, false) => self.show_view(View::Editor, window, cx),
            (false, true) => self.show_view(View::Terminals, window, cx),
            _ => {}
        }
    }
```

Call it at the end of `file_tab_shown` (every open, switch and close lands there; check its callers with `rg "file_tab_shown"`) and at the end of `files.rs::set_files_target` after the new `FilesView` is inserted.
`navigation.rs::reveal_file` keeps its `show_view(View::Editor, …)` before revealing.
`files.rs::ask_unsaved` keeps `show_view(View::Editor, …)` (unsaved tabs exist, so the view does).
`main.rs` ~922: the file tab menu shows while `self.view() == View::Editor`.

- [ ] **Step 8: Run the tests, accept the intended snapshot changes**

Run: `cargo test -p mkrew-ui`
Then `cargo insta review`: accept only renamed / added commands (`show_file_tree`, `show_editor`, labels); reject anything else and fix the code.
Expected: PASS.

- [ ] **Step 9: Smoke**

Click a tree file in Terminals: `Editor` appears in the view bar and shows it; the tree keeps the keyboard (↑/↓ move the cursor).
Ctrl+W until no tab is left: Terminals shows, `Editor` is gone.
Ctrl+Shift+3 from Board: Terminals with the tree focused. Ctrl+Shift+6 with a file open: the Editor.
Ctrl+Shift+O, pick a file: the Editor shows it.

- [ ] **Step 10: Commit**

```bash
git add crates/mkrew-ui/src
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Editor view replaces the Files view"
```

---

### Task 5: Drag tree rows onto panes

**Files:**
- Modify: `crates/mkrew-ui/src/shell/reorder.rs:222-231` (`DragChip` fields `pub(crate)`)
- Modify: `crates/mkrew-ui/src/shell/files_render.rs` (`file_row` ~895-1032)
- Modify: `crates/mkrew-ui/src/shell/navigation.rs:80-112`
- Modify: `crates/mkrew-ui/src/shell/pane_render.rs:474-503`

**Interfaces:**
- Consumes: `composer::dropped_ref` (Task 2), `render_file_tree` (Task 3).
- Produces: `pub(crate) struct DraggedPath { pub(crate) project_id: Ulid, pub(crate) path: String, pub(crate) is_dir: bool, pub(crate) name: SharedString }` (in `files_render.rs`), `Shell::type_into_pane(&mut self, session: Ulid, text: &str, window, cx)`, `Shell::drop_tree_path(&mut self, session: Ulid, dragged: &DraggedPath, window, cx)`.

- [ ] **Step 1: Delivery shared by OS and tree drops** (`navigation.rs`)

Split `drop_paths`:

```rust
    /// Files dropped from the OS on `session`'s pane (§20B.5): their absolute paths.
    pub(crate) fn drop_paths(
        &mut self,
        session: Ulid,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let paths: Vec<String> = paths
            .paths()
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        let text = composer::typed_paths(paths.iter().map(String::as_str));
        self.type_into_pane(session, &text, window, cx);
    }

    /// A file tree row dropped on `session`'s pane (§20B.5, ADR 0081): `composer.file_ref`
    /// on an agent, the bare relative path on a shell.
    pub(crate) fn drop_tree_path(
        &mut self,
        session: Ulid,
        dragged: &DraggedPath,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let agent = (self.sidebar.sessions().iter())
            .any(|s| s.id == session && s.kind == SessionKind::Agent);
        // A missing or broken settings file types the default form (the daemon reports it).
        let template = mkrew_core::settings::load(&self.settings_file)
            .unwrap_or_default()
            .composer
            .file_ref;
        let text = composer::dropped_ref(&template, &dragged.path, dragged.is_dir, agent);
        self.type_into_pane(session, &text, window, cx);
    }

    /// Types a drop into `session`'s pane: into its composer draft while the composer is
    /// shown, else into the terminal; no Enter either way. A modal dialog covers the panes,
    /// so a drop on it types nothing.
    pub(crate) fn type_into_pane(
        &mut self,
        session: Ulid,
        text: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.modal_open() || text.is_empty() {
            return;
        }
        if let Some(tabs) = self.active_tabs_mut() {
            tabs.focus(session);
        }
        if self.composers.shown(session).is_some() {
            self.composer_typed(session, text, window, cx);
        } else if let Some(pane) = self.panes.get(&session).cloned() {
            pane.update(cx, |pane, cx| pane.type_text(text, cx));
            self.focus_pane(window, cx);
        }
        cx.notify();
    }
```

- [ ] **Step 2: Drag source** (`files_render.rs`)

```rust
/// Payload of a file tree row dragged onto a pane (ADR 0081): its path relative to the
/// tree's root.
#[derive(Debug, Clone)]
pub(crate) struct DraggedPath {
    pub(crate) project_id: Ulid,
    pub(crate) path: String,
    pub(crate) is_dir: bool,
    pub(crate) name: SharedString,
}
```

In `file_row`, before `.on_click(…)`:

```rust
        .on_drag(
            DraggedPath {
                project_id,
                path: entry.path.clone(),
                is_dir: entry.is_dir,
                name: entry.name.clone().into(),
            },
            move |dragged, offset, _, cx| {
                cx.new(|_| DragChip {
                    style: style.clone(),
                    accent,
                    title: dragged.name.clone(),
                    offset,
                })
            },
        )
```

with `let style = PopupStyle::of(theme); let accent = theme.accent;` at the top of `file_row`.
In `reorder.rs` make the four `DragChip` fields `pub(crate)`.

- [ ] **Step 3: Drop target** (`pane_render.rs`, the pane root `div()` ~474)

```rust
            .drag_over::<DraggedPath>(move |style, _, _, _| style.border_color(rgb(accent_border)))
            .on_drop(cx.listener(move |shell, dragged: &DraggedPath, window, cx| {
                if shell.workspace == Some(dragged.project_id) {
                    shell.drop_tree_path(session, dragged, window, cx);
                }
            }))
```

with `let accent_border = theme.accent;` before the element and `.border(px(theme::BORDER_WIDTH))` present on the root (add it with `border_color(gpui::transparent_black())` if the root has none, so the ring never shifts content; if a border is already set by `status::pane_border`, apply `drag_over` to that same border).

- [ ] **Step 4: Build and lint**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 5: Smoke**

Panel on Files, Terminals with a Claude pane and a shell pane.
Drag `src/main.rs` onto the Claude pane: `@src/main.rs ` is typed, no Enter, the pane ringed while hovered.
Drag the `src` folder: `@src/ `. Onto the shell pane: `src/main.rs `.
Open the composer (Ctrl+Shift+Enter), drop: the text lands in the draft.
A plain click on a tree row still opens the file.
Drop an Explorer file onto a pane: absolute path, as before.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): drag file tree rows onto panes"
```

---

### Task 6: SPEC, ADR, human check, full verify

**Files:**
- Create: `docs/decisions/0081-files-tree-in-the-right-panel.md`
- Modify: `docs/SPEC.md` (§10 ~745-910, §12A ~1097-1170, §13 ~1290-1335, §20B.5 ~1977, config sample ~1843, the §12A link sentence ~958, the M7 line ~1693 stays)
- Modify: `docs/decisions/0050-files-tree-on-the-right.md`, `docs/decisions/0038-session-panel.md` (Status line only)
- Modify: `docs/HUMAN_CHECKS.md` ("To verify" section, newest first)

- [ ] **Step 1: ADR 0081** (template `docs/decisions/0000-template.md`)

```markdown
# 0081 - Files tree in the right panel

- Date: 2026-10-27
- Status: accepted
- Milestone: M7

## Context

The Files view took the whole main area: to open a file or hand one to an agent, the user left the terminals.
The session panel (ADR 0038) already sat right of the terminals with a resizable width.

## Options

1. Keep the Files view - pros: no change / cons: files and agents never on screen together.
2. Draw the existing `FilesView` in two places: the tree in the right panel, the editor in the center - pros: no new state, the tree stays next to the agents / cons: the panel holds two tabs.
3. A separate tree entity synced with the editor - pros: separate code / cons: two-way sync of cursor, tabs and worktree target.
4. A drawer over the panes - pros: less layout / cons: cannot stay open, covers the drop target.

## Decision

Option 2.
The right panel has `Files │ Session` tabs, one toggle (`◨`, Ctrl+Shift+Y) and one width; it shows next to Terminals and the Editor and is remembered as `[ui] right_panel`.
The center `Editor` view exists only while files are open.
Tree rows drag onto panes and type `composer.file_ref` (agents) or the bare path (shells).

## Consequences

Supersedes in part ADR 0050 (tree right of the editor) and ADR 0038 (the session panel is a tab, and no longer closed at every start).
SPEC §10, §12A, §13, §20B.5 and the config sample change in the same commit.
Moving Git into the panel is a follow-up (`GIT_PANEL_PROMPT.md`).
```

Set the Status of ADR 0050 and 0038 to `accepted; superseded in part by 0081`.

- [ ] **Step 2: SPEC**

- §10 sketch: view bar `[Terminals│Board│Git│Docs│Editor*]`; replace "The session panel (below) is a third column right of Terminals while open." with "The right panel (below) is a third column right of Terminals and the Editor while open."; rewrite the **Session panel** bullet as **Right panel** (tabs `Files │ Session`, `◨` / Ctrl+Shift+Y hide / show and keep the tab, from another view it shows Terminals, `[ui] right_panel` remembered, `inspector_width` 240-560 px, the Session tab = the old content following the active tab's focused pane).
- §12A title "UI: Files (tree and Editor)"; **Tree**: in the right panel's Files tab (ADR 0081), Ctrl+Shift+3, rows drag onto panes (§20B.5); **Editor**: the center `Editor` view, listed only while a file tab is open, a click / Enter / quick open / terminal link shows it, closing the last tab shows Terminals; replace every "Files view" with "Editor" or "tree" as meant.
- §13: `View | Show file tree (right panel) | Ctrl+Shift+3`, `View | Show editor | Ctrl+Shift+6`, `View | Toggle the right panel | Ctrl+Shift+Y`; the Tabs rows say "while the Editor is shown"; the `Files` rows become `Editor` (save, find, close tab) and `File tree` (move, toggle ignored).
- §20B.5: add "Dragging a row of the file tree (§12A) onto a pane types `composer.file_ref` with the path relative to the tree's folder on an agent pane, the bare path on a shell, a folder ending with `/`, then one space; no Enter."
- Config sample after `sidebar_width`: `right_panel = "hidden"   # §10; "files" | "session" | "hidden"; the right panel's toggle and tabs write it`; fix the `inspector_width` comment to "the right panel's width".
- The terminal link sentence (~958): "it opens in the Editor (§12A)".

- [ ] **Step 3: HUMAN_CHECKS** (top of "To verify")

```markdown
### To verify - 2026-10-27 Files tree in the right panel (ADR 0081)

- [ ] **Panel (dark + light):** `◨` opens the right panel on `Files │ Session`; the tab row, tree rows, markers and search fit 240 px without clipping; dragging the edge resizes both tabs.
- [ ] **Editor:** clicking a tree file shows `Editor` in the view bar and the file; closing every tab hides it and shows Terminals; the tree keeps the keyboard after a click.
- [ ] **Drag:** dragging a file / folder onto a Claude pane types `@path ` / `@folder/ `; onto a shell the bare path; the pane shows an accent ring while hovered; a plain click still opens.
- [ ] **Restart:** the panel comes back on the tab it was left on, or hidden.
```

- [ ] **Step 4: Full verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt, clippy `-D warnings` and every test pass.
Run: `rg -n "View::Files|ShowFiles|FILES_CONTEXT|FILE_TREE_WIDTH|inspector_open|show_files" crates docs/SPEC.md`
Expected: no matches.

- [ ] **Step 5: Final smoke on the real app**

Repeat the Task 4 and Task 5 smoke lists in one run, plus: an agent edits a file while only the tree is on screen (Terminals) and its marker appears (`git.changed`); switch worktree from the tree's chooser with an unsaved tab: the prompt comes first, then Terminals; hide the panel while the tree has the keyboard: the pane gets it.

- [ ] **Step 6: Commit**

```bash
git add docs/SPEC.md docs/decisions/0081-files-tree-in-the-right-panel.md docs/decisions/0050-files-tree-on-the-right.md docs/decisions/0038-session-panel.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "docs: files tree in the right panel (ADR 0081)"
```
