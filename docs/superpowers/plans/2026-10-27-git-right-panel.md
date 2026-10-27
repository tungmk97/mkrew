# Git in the right panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the top-level Git view: the Git list (header, changes or History, commit box) lives in the right panel's `Git` tab, and a clicked file's diff shows in a center `Diff` view listed only while a diff is open.

**Architecture:** The per-workspace `GitView` model stays and is drawn in two places: `render_git_panel` (the panel's Git tab) and `render_diff_view` (the center `View::Diff`).
`GitView` gains one flag, `open` ("a diff is open"), which gates the `Diff` view the way open file tabs gate `Editor`.
"Git is on screen" (`git_shown()`) replaces every `view() == View::Git` guard: the Git tab is on screen, or the Diff view is shown.

**Tech Stack:** Rust, gpui-pre 0.3.7, `mkrew-core` settings (TOML, serde), `insta` snapshots.

**Spec:** `docs/superpowers/specs/2026-10-27-git-right-panel-design.md`

## Global Constraints

- Work only in the worktree `.worktrees/git-right-panel` (branch `feat/git-right-panel`).
- Read `skill://mkrew-ui-design` before the first UI edit: every color, spacing, radius and font size comes from `crates/mkrew-ui/src/theme.rs` (SPEC §14); no literals in render code.
- No `unwrap` / `expect` outside tests unless provably infallible (comment why).
- `[ui] right_panel` values: `"files"`, `"session"`, `"git"`, `"hidden"`; default `"hidden"`.
- Panel width: `[ui] inspector_width`, kept 240-560 px (`theme::INSPECTOR_MIN_WIDTH` / `INSPECTOR_MAX_WIDTH`), one width for all three tabs.
- Keys: `ShowGitPanel` = `ctrl-shift-4` (name `show_git_panel`), `ShowDiff` = `ctrl-shift-7` (name `show_diff`); Git single keys unchanged, in `GIT_CONTEXT`.
- Clean cutover: no `View::Git`, `Command::ShowGit`, `"show_git"`, `render_git(` or `GIT_FILE_LIST_WIDTH` left anywhere (code, tests, comments); no alias for `show_git`.
- Verify with `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL).
  On OOM / linker errors re-run cargo with `-j 2`.
  The C: drive is low on space: set `CARGO_TARGET_DIR` to the main checkout's `target` folder (`C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/target`) for every cargo command.
  If an exe there is locked, rename it to `*.locked-old.exe`; never kill a running daemon or agent.
- Commit with `git -c user.email=tungnv975@gmail.com commit`, staging only the task's files. Never push.
- Docs: one sentence per line in new Markdown; never the em dash character.

## Review Focus

1. Hiding the panel (or switching it to Files / Session) while the Diff view is shown must keep every Git key working: the diff card takes `git_focus` and `GIT_CONTEXT` when the list is not drawn. Covered in Task 2 Step 4 (`render_diff_view`); smoke-check in Task 4.
2. Switching repository chip or worktree from the Git tab while a diff is open must keep the Diff view open on the new repository's first file, not drop to Terminals: `retarget_git` carries `open` over. Covered in Task 3 Step 5.
3. A commit, a discard of the last file, or a clean worktree must close the diff and show Terminals with the panes attached (`sync_panes` runs), never leave a blank center. Covered by `follow_git_diff` in `git_requests` (Task 3 Step 5) and the model test in Task 1.
4. ↑ / ↓ in the list while no diff is open must only move the selection, never open the Diff view; with a diff open, the diff follows. Covered by the model test in Task 1 (`key` leaves `open` alone).
5. `git.changed` while only the Git tab is on screen (Terminals view) must update the list: `git_event` keys off `git_shown()`. Smoke-check in Task 4.

---

### Task 1: `GitView` knows whether a diff is open

**Files:**
- Modify: `crates/mkrew-ui/src/git_view.rs` (`struct GitView` ~568-629, `GitView::new` ~632-666, `select_none` ~885, `reselect` ~751-774)
- Test: `crates/mkrew-ui/src/git_view/tests.rs`

**Interfaces:**
- Produces: `GitView::open_diff(&mut self)`, `GitView::close_diff(&mut self)`, `GitView::diff_open(&self) -> bool`.
  `reselect` clears it when no file is left; nothing else in the model touches it.

- [ ] **Step 1: Write the failing test** (in `git_view/tests.rs`, after `the_selection_follows_its_file_across_groups_or_falls_back_to_the_first`)

```rust
#[test]
fn an_open_diff_follows_its_selection_and_closes_when_nothing_is_left() {
    let mut view = loaded();
    assert!(!view.diff_open(), "selecting the first file does not open the diff");
    view.key(Key::Down);
    assert!(!view.diff_open(), "moving the selection does not open it");
    view.open_diff();
    // `new.txt` got staged: the selection moves with it, the diff stays open.
    view.select(target(Group::Untracked, "new.txt"));
    let mut status = working();
    status.untracked.clear();
    status.staged.push(file("new.txt", ChangeKind::Added));
    view.begin_status();
    view.status_loaded(Ok(status));
    assert_eq!(view.selected, Some(target(Group::Staged, "new.txt")));
    assert!(view.diff_open());
    view.key(Key::Up);
    assert!(view.diff_open(), "moving the selection keeps it open");
    // Everything committed: nothing to show, the diff closes.
    view.begin_status();
    view.status_loaded(Ok(GitStatus::default()));
    assert!(!view.diff_open());
    // Closed by hand.
    let mut view = loaded();
    view.open_diff();
    view.close_diff();
    assert!(!view.diff_open());
}
```

- [ ] **Step 2: Run it, expect a compile failure** (`open_diff` unknown)

Run: `cargo test -p mkrew-ui an_open_diff_follows`

- [ ] **Step 3: Implement**

In `struct GitView`, after `pub viewed: Rc<ViewedSet>,`:

```rust
    /// The Diff view is open (SPEC §12B, ADR 0084): a click / Enter on a file or an entry
    /// point opens it; `×`, `Esc` and an empty list close it. A selection moving by itself
    /// (staging, ↑ ↓, a refresh) keeps it.
    open: bool,
```

In `GitView::new`, after `viewed: Rc::new(ViewedSet::default()),` add `open: false,`.

After `fn select_none`:

```rust
    /// Opens the Diff view on the selection (or the first file once listed).
    pub fn open_diff(&mut self) {
        self.open = true;
    }

    /// Closes the Diff view; the selection stays for the next open.
    pub fn close_diff(&mut self) {
        self.open = false;
    }

    /// The Diff view is open.
    pub fn diff_open(&self) -> bool {
        self.open
    }
```

In `reselect`, the `None` arm:

```rust
            None => {
                // No file left (committed, discarded, clean): the open diff closes with it.
                self.open = false;
                self.select_none();
                None
            }
```

- [ ] **Step 4: Run the module's tests**

Run: `cargo test -p mkrew-ui git_view`
Expected: PASS (every existing `git_view` test unchanged).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/git_view.rs crates/mkrew-ui/src/git_view/tests.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): git view tracks an open diff"
```

---

### Task 2: The right panel's `Git` tab; the Git view keeps only the diff

After this task the panel has `Files │ Session │ Git`; the Git list is drawn only in the Git tab; `View::Git` (still its name) shows the diff card with the panel beside it.
Entry points and commands are unchanged until Task 3.

**Files:**
- Modify: `crates/mkrew-core/src/settings.rs` (`RightPanelMode` ~328-349, `UiSettings::right_panel` doc ~379, test ~1149-1172)
- Modify: `crates/mkrew-ui/src/inspector.rs` (`PanelTab`, `RightPanel` ~27-77, test ~206-226)
- Modify: `crates/mkrew-ui/src/shell/focus.rs:54-68`
- Modify: `crates/mkrew-ui/src/shell/inspector.rs:5-87,194-262`
- Modify: `crates/mkrew-ui/src/shell/workspace.rs:44-50,353-399`
- Modify: `crates/mkrew-ui/src/shell/chrome.rs:21-37`
- Modify: `crates/mkrew-ui/src/shell/git_render.rs:1-186`
- Modify: `crates/mkrew-ui/src/shell/git_menus.rs:58,318-371`
- Modify: `crates/mkrew-ui/src/shell/git.rs:454-469,702-709`
- Modify: `crates/mkrew-ui/src/shell/files.rs:859-864`
- Modify: `crates/mkrew-ui/src/shell/navigation.rs:589-596`
- Modify: `crates/mkrew-ui/src/theme.rs:406-407` (and a new token next to `GIT_CHANGED_BY_MAX_WIDTH` ~451)

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces:
  - `mkrew_core::settings::RightPanelMode::Git` (name `"git"`).
  - `mkrew_ui::inspector::PanelTab::Git`.
  - `Shell::panel_view(&self) -> bool` (the view draws the right panel), `Shell::git_panel_shown(&self) -> bool` (the Git tab is on screen), `Shell::git_shown(&self) -> bool` (the Git tab or the Git / Diff view is on screen).
  - `Shell::render_git_panel(&self, project_id: Ulid, theme: &Theme, window: &Window, cx: &mut Context<'_, Self>) -> gpui::AnyElement`.
  - `Shell::render_diff_view(&self, project_id: Ulid, theme: &Theme, window: &Window, cx: &mut Context<'_, Self>) -> gpui::AnyElement`.
  - `Shell::panel_changed(&mut self, tree_before: bool, git_before: bool, mode_before: RightPanelMode, window, cx)` (new `git_before` argument).
  - `theme::GIT_FILTER_MIN_WIDTH: f32`.

- [ ] **Step 1: Write the failing tests**

`settings.rs`, in `right_panel_parses_defaults_to_hidden_and_round_trips`, after the `"session"` assert:

```rust
        assert_eq!(
            ui("[ui]\nright_panel = \"git\"").right_panel,
            RightPanelMode::Git
        );
```

and add `RightPanelMode::Git,` to the round-trip `for mode in [...]` list.

`inspector.rs` tests, in `the_right_panel_round_trips_through_its_saved_mode`: add `RightPanelMode::Git,` to the `for mode in [...]` list, and at the end:

```rust
    // The Git tab is kept while hidden, like the others.
    panel.show(PanelTab::Git);
    panel.open = false;
    assert_eq!(panel.mode(), RightPanelMode::Hidden);
    panel.open = true;
    assert_eq!(panel.mode(), RightPanelMode::Git);
```

- [ ] **Step 2: Run them, expect compile failures** (`Git` variant unknown)

Run: `cargo test -p mkrew-core right_panel_parses` and `cargo test -p mkrew-ui right_panel_round_trips`

- [ ] **Step 3: Implement the setting and the model**

`settings.rs`, `RightPanelMode`, after `Session,`:

```rust
    /// Open on the Git changes.
    Git,
```

In `name`: `RightPanelMode::Git => "git",`.
Doc of `UiSettings::right_panel`: "`[ui] right_panel`: hidden, or open on the Files, Session or Git tab at start."
Doc of the enum: "`[ui] right_panel`: the right panel next to Terminals / Editor / Diff at start, and its tab (SPEC §10). The panel toggle and its tab row write it."

`inspector.rs`, `PanelTab`, after `Session,` add `Git,`.
`from_mode`: `RightPanelMode::Git => Self { open: true, tab: PanelTab::Git },`.
`mode`: `(true, PanelTab::Git) => RightPanelMode::Git,`.
Module doc: "The right panel next to Terminals / Editor / Diff (SPEC §10, ADR 0038, ADR 0081, ADR 0084): its tabs, width, the Stop rule and the text of the Session card. Pure; `main.rs` draws it."

`cargo test -p mkrew-core right_panel_parses` passes now; the `mkrew-ui` test passes once Step 4 makes the binary's `match tab` exhaustive.

- [ ] **Step 4: Implement the shell**

`shell/focus.rs`, replace `panel_shown`:

```rust
    /// The views the right panel is drawn next to.
    pub(crate) fn panel_view(&self) -> bool {
        matches!(self.view(), View::Terminals | View::Editor | View::Git)
    }

    /// The right panel is drawn.
    pub(crate) fn panel_shown(&self) -> bool {
        self.right_panel.open && self.panel_view()
    }
```

and after `files_on_screen`:

```rust
    /// The right panel shows the Git list (header, changes or History, commit box).
    pub(crate) fn git_panel_shown(&self) -> bool {
        self.panel_shown() && self.right_panel.tab == PanelTab::Git
    }

    /// Some of the active workspace's Git is drawn (the panel's list or the diff), so it
    /// follows `git.changed` and refreshes when it comes back (SPEC §12B).
    pub(crate) fn git_shown(&self) -> bool {
        self.view() == View::Git || self.git_panel_shown()
    }
```

`shell/inspector.rs`:
- `toggle_inspector`: `if matches!(self.view(), View::Terminals | View::Editor)` becomes `if self.panel_view()`.
  Both branches yield `(tree_before, git_before) = (self.files_on_screen(), self.git_shown())`, taken before the toggle in the first branch and after `show_view` in the second (as `tree_before` is today).
  Call `self.panel_changed(tree_before, git_before, mode_before, window, cx)`.
- `show_panel_tab`: `let (tree_before, git_before, mode_before) = (self.files_on_screen(), self.git_shown(), self.right_panel.mode());` and pass `git_before`.
- `show_file_tree`: `if !matches!(self.view(), View::Terminals | View::Editor)` becomes `if !self.panel_view()`.
- `panel_changed` gets the `git_before: bool` argument after `tree_before`; doc "After the panel opened, closed or switched tab: a tree or Git list that came on screen is refreshed, a hidden one gives the keyboard back, the grip of a hidden panel ends its drag, and `[ui] right_panel` is saved."
  Its body up to the settings write:

```rust
        if !self.right_panel.open {
            // The grip is gone: no mouse-up will end a drag in progress.
            self.end_inspector_drag(cx);
        }
        if self.file_tree_shown() {
            if !tree_before && let Some(project_id) = self.workspace {
                self.refresh_files(project_id, window, cx);
            }
        } else if self.files_focus.is_focused(window) {
            self.focus_center(window, cx);
        }
        if self.git_shown() {
            if !git_before && let Some(project_id) = self.workspace {
                self.refresh_git(project_id, window, cx);
            }
        } else if self.git_focus.is_focused(window) {
            self.focus_center(window, cx);
        }
```

  and a private helper after it:

```rust
    /// The keyboard back to the center after the panel's focused tab went away: the editor
    /// in the Editor view, else the focused pane. (In the Git / Diff view the diff card holds
    /// `git_focus` itself: `git_shown` stays true there.)
    fn focus_center(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.view() == View::Editor {
            window.focus(&self.file_editor_focus, cx);
        } else {
            self.focus_pane(window, cx);
        }
    }
```

- `render_right_panel`: doc "The right panel (SPEC §10, §12A, §12B): `Files | Session | Git` tabs, right of Terminals, the Editor and the diff. Rendered only while open; it is a sibling of the panes, never inside the terminal element."
  The tab row:

```rust
        let tab = self.right_panel.tab;
        let changed = self.git_summary().map_or(0, GitSummary::changed);
        let tabs = div().flex_none().p(px(space::S)).child(segmented(theme).children(
            [PanelTab::Files, PanelTab::Session, PanelTab::Git].map(|each| {
                let title = match each {
                    PanelTab::Files => "Files",
                    PanelTab::Session => "Session",
                    PanelTab::Git => "Git",
                };
                segment(
                    SharedString::from(format!("panel-{title}")),
                    each == tab,
                    theme,
                )
                .flex_1()
                .on_click(cx.listener(move |shell, _, window, cx| {
                    shell.show_panel_tab(each, window, cx);
                }))
                .child(title)
                // The workspace's changed files over all repositories, hidden at 0.
                .when(each == PanelTab::Git && changed > 0, |el| {
                    el.child(tinted_chip(changed.to_string(), theme.accent_text, theme))
                })
            }),
        ));
```

  The body `match tab` gets `PanelTab::Git => self.render_git_panel(project_id, theme, window, cx),`.
  Module doc: "The right panel (file tree, session cards, Git list) and its title bar toggle."

`shell/git_render.rs`: split `render_git`.
- Rename it `render_git_panel`; doc "The right panel's Git tab (SPEC §12B): the worktree chooser, repository tabs, branch header, Working / Vs. base / History, the file filter, the changed-file list and the commit box."
  Keep everything up to and including `let column = …;`, then return the old side column filling the tab (the old outer row with `justify_end` and `render_git_diff` goes):

```rust
        // The header, filter and commit box sit outside the `GIT_CONTEXT` column: typing in
        // their fields never stages.
        div()
            .size_full()
            .flex()
            .flex_col()
            .font_family(theme.ui_font_family.clone())
            .child(self.git_header(project_id, view, status, theme, window, cx))
            .when(!clean, |el| {
                el.child(self.git_filter_row(project_id, view, theme, window, cx))
            })
            .child(column)
            .when_some(status.filter(|_| view.commit_box_shown()), |el, status| {
                el.child(self.git_commit_box(project_id, view, status, theme, window, cx))
            })
            .into_any_element()
```

- Add after it:

```rust
    /// The center of the Git view (SPEC §12B): the selected file's diff card. While the
    /// panel's Git tab is not drawn, the card holds the Git keyboard (`git_focus`,
    /// `GIT_CONTEXT`) so its keys keep working; one element tracks `git_focus` per frame.
    pub(crate) fn render_diff_view(
        &self,
        project_id: Ulid,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let Some(view) = self.git.get(&project_id) else {
            return message(theme, None, "Loading changes…".into(), None)
                .flex_1()
                .into_any_element();
        };
        let card = self.render_git_diff(project_id, view, theme, window, cx);
        let card = if self.git_panel_shown() {
            card
        } else {
            card.track_focus(&self.git_focus)
                .key_context(GIT_CONTEXT)
                .on_key_down(cx.listener(Self::git_key_down))
        };
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .font_family(theme.ui_font_family.clone())
            .child(card)
            .into_any_element()
    }
```

  Check `track_focus` / `key_context` / `on_key_down` on a plain `gpui::Div` in the gpui-pre 0.3.7 source before relying on it.
  If `track_focus` needs a `Stateful` div, give the card `.id("git-diff-card")` in `render_git_diff` and change its return type to `gpui::Stateful<gpui::Div>`.
  Module doc: "Git rendering: the right panel's Git tab (header, tabs, list, commit box), the diff view's frame, and the empty states."

`shell/chrome.rs`, `render_main`:

```rust
            (View::Git, _) => main.child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.render_diff_view(project_id, theme, window, cx))
                    .children(panel),
            ),
```

`shell/workspace.rs`:
- `activate_workspace` (~48): `if switched && self.view() == View::Git` becomes `if switched && self.git_shown()`.
- `show_view`: take `let git_before = self.git_shown();` next to `tree_before`.
  `if !matches!(view, View::Terminals | View::Editor)` becomes `if !self.panel_view()`, comment "The panel (and its grip) leaves with the views that draw it: end a drag in progress."
  Replace the `if view == View::Git { … }` block with:

```rust
        if self.git_shown() && !git_before {
            self.refresh_git(project_id, window, cx);
        }
        if view == View::Git {
            window.focus(&self.git_focus, cx);
        }
```

  Doc: "Switches the active workspace to `view`; leaving Terminals detaches its panes. Showing Docs re-lists its files (agents may have written new ones); showing the Editor refreshes the tree and gives the editor the keyboard; Git (the panel's tab or the diff) coming on screen refreshes its changes."

`shell/git.rs`:
- `git_event`: `self.workspace.filter(|_| self.git_shown())`; doc "Follows `git.changed` while the active workspace's Git is on screen (the panel's tab or the diff); hidden ones refresh when shown."
- `git_commit_shown`: `self.git_panel_shown() && …`; doc "Whether the panel's Git tab is shown with its commit box."

`shell/files.rs:861`: `self.view() == View::Git` becomes `self.git_shown()`.
`shell/navigation.rs:590`: `self.view() == View::Git` becomes `self.git_shown()`.

`shell/git_menus.rs`:
- `git_repo_strip` line 58: `let room = self.inspector_width - space::M * 2.0 - theme::ICON_BUTTON_SIZE;` (the tab is as wide as the panel).
- `git_filter_row` doc "The "Filter files" field and, while sessions changed files, the ghost "All ▾"; the button wraps under the field when both do not fit."
  The row:

```rust
        div()
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(space::S))
            .gap_y(px(space::XS))
            .px(px(space::M))
            .pb(px(space::S))
```

  and on the `caret_field`, `.flex_1().min_w_0()` becomes `.flex_1().min_w(px(theme::GIT_FILTER_MIN_WIDTH))`.

`theme.rs`: delete `GIT_FILE_LIST_WIDTH` and its doc; next to `GIT_CHANGED_BY_MAX_WIDTH` add:

```rust
/// Narrowest the Git tab's "Filter files" field gets before the "By:" button wraps under it.
pub const GIT_FILTER_MIN_WIDTH: f32 = 120.0;
```

- [ ] **Step 5: Build, test, lint**

Run: `cargo test -p mkrew-core right_panel_parses`, `cargo test -p mkrew-ui right_panel_round_trips`, `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: PASS, no warnings.
Run: `rg -n "GIT_FILE_LIST_WIDTH|render_git\(" crates`
Expected: no matches.

- [ ] **Step 6: Smoke on the real app**

Build and start the daemon and UI the way the last plan's smoke steps did (`docs/superpowers/plans/2026-10-27-files-right-panel.md`, Task 4).
- Open the panel (`◨`), click `Git`: the header, list and commit box show; the tab shows the changed count.
- Drag the panel to 240 px: the filter field and "By:" wrap onto two lines; the repo strip does not overflow (multi-repo workspace).
- `Ctrl+Shift+4` (still the old Git view): the center shows only the diff card, the panel beside it; `s`, `n`, `d` work with the list focused.
- Hide the panel in the Git view: `n` / `p` / `d` still work (the card took the keyboard).
- Restart with the panel on Git: it comes back on Git.

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-core/src/settings.rs crates/mkrew-ui/src
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): git list in the right panel's Git tab"
```

---

### Task 3: `View::Git` becomes `View::Diff`, shown only while a diff is open; new commands and entry points

**Files:**
- Modify: `crates/mkrew-ui/src/board.rs:22-74`, `crates/mkrew-ui/src/board/tests.rs:10-42`
- Modify: `crates/mkrew-ui/src/keymap.rs` (`Command` ~102-103, list ~265, names ~356, keys ~459, context docs ~187-192, test ~1006)
- Modify: `crates/mkrew-ui/src/palette.rs` (`Context` ~23-56, `title` ~243, `available` ~296-379), `crates/mkrew-ui/src/palette/tests.rs`
- Modify: `crates/mkrew-ui/src/shell/workspace.rs:331-348`
- Modify: `crates/mkrew-ui/src/shell/focus.rs` (`panel_view`, `git_shown`)
- Modify: `crates/mkrew-ui/src/shell/git.rs` (`review_diff` ~52-67, `retarget_git` ~69-89, `git_requests` ~201-226, `select_git_commit` ~290-306, `select_git_file` ~471-487, `git_key_down` ~489-517)
- Modify: `crates/mkrew-ui/src/shell/git_diff.rs` (`git_diff_toolbar` ~434-462)
- Modify: `crates/mkrew-ui/src/shell/inspector.rs` (new `show_git_panel`)
- Modify: `crates/mkrew-ui/src/shell/board.rs:57-72`, `crates/mkrew-ui/src/shell/worktrees.rs:342-357`
- Modify: `crates/mkrew-ui/src/shell/chrome.rs` (`render_main`, `render_view_tabs` ~262-291, branch chip ~371-373, switcher doc ~293-296)
- Modify: `crates/mkrew-ui/src/shell/sessions.rs:418`, `crates/mkrew-ui/src/shell/navigation.rs:395-421`
- Modify: `crates/mkrew-ui/src/shell/comments.rs:25,164,253`, `crates/mkrew-ui/src/shell/composer.rs:231-234,273`, `crates/mkrew-ui/src/shell/send_dialog.rs:180,190`

**Interfaces:**
- Consumes: `GitView::{open_diff, close_diff, diff_open}` (Task 1); `panel_view`, `git_panel_shown`, `git_shown`, `render_diff_view`, `show_panel_tab` (Task 2).
- Produces:
  - `View::Diff` (title "Diff", command `ShowDiff`), `View::ALL = [Terminals, Board, Docs, Editor, Diff]`, `View::bar(docs: bool, editor: bool, diff: bool)`.
  - `Command::ShowGitPanel` (`show_git_panel`, `ctrl-shift-4`), `Command::ShowDiff` (`show_diff`, `ctrl-shift-7`).
  - `palette::Context { git: bool, diff: bool, .. }`.
  - `Shell::diff_open(&self, project_id: Ulid) -> bool`, `Shell::open_git(&mut self, project_id: Ulid, window, cx)`, `Shell::open_git_diff(&mut self, project_id: Ulid, window, cx)`, `Shell::close_git_diff(&mut self, project_id: Ulid, window, cx)`, `Shell::show_git_panel(&mut self, window, cx)`.

- [ ] **Step 1: Write the failing tests**

`board/tests.rs`: in the `View::command` test replace `Command::ShowGit` with `Command::ShowDiff`, ordered like the new `ALL` (`ShowTerminals, ShowBoard, ShowDocs, ShowEditor, ShowDiff`).
Replace `the_view_bar_lists_the_editor_only_with_open_files` with:

```rust
#[test]
fn the_view_bar_lists_the_editor_and_the_diff_only_while_open() {
    let bar = |docs, editor, diff| View::bar(docs, editor, diff).collect::<Vec<_>>();
    assert_eq!(
        bar(true, false, false),
        [View::Terminals, View::Board, View::Docs]
    );
    assert_eq!(
        bar(true, true, true),
        [
            View::Terminals,
            View::Board,
            View::Docs,
            View::Editor,
            View::Diff
        ]
    );
    assert_eq!(
        bar(false, false, true),
        [View::Terminals, View::Board, View::Diff]
    );
}
```

`keymap.rs` test (~1006): `assert_eq!(bound("ctrl-shift-4"), Some(Command::ShowGitPanel));` and add `assert_eq!(bound("ctrl-shift-7"), Some(Command::ShowDiff));`.

`palette/tests.rs`:
- the `context(view)` helper gets `git: view == View::Diff,` and `diff: true,` (like `file_tabs: 1`: present unless a test says otherwise);
- every `context(View::Git)` becomes `context(View::Diff)`; `Command::ShowGit` becomes `Command::ShowGitPanel`;
- the fuzzy test (~683-716) types a query that should now pick "View: Git panel": keep the query, expect `Action::Run(Command::ShowGitPanel)` and `rows[0].entry.title == "View: Git panel"`, and re-check its `ranges`;
- the `View::Git` entry in the `Extra::PhoneAccess` view list becomes `View::Diff`;
- add:

```rust
#[test]
fn diff_and_git_commands_follow_what_is_on_screen() {
    let no_diff = Context {
        diff: false,
        ..context(View::Terminals)
    };
    assert!(!commands(&no_diff).contains(&Command::ShowDiff));
    assert!(commands(&context(View::Terminals)).contains(&Command::ShowDiff));
    // The Git tab in the panel next to the terminals: the Git keys act on its list.
    let git_tab = Context {
        git: true,
        ..context(View::Terminals)
    };
    assert!(commands(&git_tab).contains(&Command::GitStage));
    assert!(!commands(&context(View::Terminals)).contains(&Command::GitStage));
    // Diff comments only where the diff is drawn.
    assert!(!commands(&git_tab).contains(&Command::AddComment));
    assert!(commands(&context(View::Diff)).contains(&Command::AddComment));
}
```

- [ ] **Step 2: Run them, expect compile failures** (`View::Diff`, `ShowGitPanel` unknown)

Run: `cargo test -p mkrew-ui --lib view_bar palette keymap`

- [ ] **Step 3: Implement the pure parts**

`board.rs`:

```rust
    /// The opened files' tabs and editor (SPEC §12A); the tree is in the right panel.
    Editor,
    /// The open diff (SPEC §12B); the Git list is in the right panel.
    Diff,
}

impl View {
    pub const ALL: [View; 5] = [
        View::Terminals,
        View::Board,
        View::Docs,
        View::Editor,
        View::Diff,
    ];

    /// The view bar's views: Docs while `[features] docs` is on, Editor while files are
    /// open, Diff while a diff is open.
    pub fn bar(docs: bool, editor: bool, diff: bool) -> impl Iterator<Item = View> {
        View::ALL.into_iter().filter(move |&view| match view {
            View::Docs => docs,
            View::Editor => editor,
            View::Diff => diff,
            View::Board | View::Terminals => true,
        })
    }
```

`title`: `View::Diff => "Diff",`; `command`: `View::Diff => Command::ShowDiff,`.

`keymap.rs`:
- `Command`: replace `ShowGit` (and its doc) with

```rust
    /// Open the right panel on the Git tab and give its list the keyboard.
    ShowGitPanel,
    /// Show the Diff view while a diff is open.
    ShowDiff,
```

- the commands list: `Command::ShowGitPanel, Command::ShowDiff,` where `Command::ShowGit,` was;
- names: `Command::ShowGitPanel => "show_git_panel".into(), Command::ShowDiff => "show_diff".into(),`;
- keys: `Command::ShowGitPanel => "ctrl-shift-4".into(), Command::ShowDiff => "ctrl-shift-7".into(),`;
- `GIT_CONTEXT` doc: "Key context of the Git list (right panel) while it has focus, or of the diff card while the list is hidden (so its single-key shortcuts never reach a text field).";
  `COMMENTS_CONTEXT` doc: "Comment commands work in the Docs view and with the Git keyboard (diff comments)."

`palette.rs`:
- `Context`, after `discard`:

```rust
    /// The active workspace's Git is on screen (the panel's Git tab or the Diff view): the
    /// Git keys act.
    pub git: bool,
    /// The active workspace has an open diff (the Diff view exists).
    pub diff: bool,
```

- `title`: `Command::ShowGitPanel => "View: Git panel".into(), Command::ShowDiff => "View: Diff".into(),`.
- `available`:
  - `Command::ShowGit` in the `context.workspace` group becomes `Command::ShowGitPanel`;
  - add `Command::ShowDiff => context.workspace && context.diff,`;
  - `SendSelection`: `in_view(View::Editor) || in_view(View::Diff) || (on_panes && context.terminal)`;
  - `AddComment | SendComments`: `in_view(View::Docs) || in_view(View::Diff)`;
  - the `Git*` group: `context.workspace && context.git`;
  - `GitDiscard`: `context.workspace && context.git && context.discard`.

Run: `cargo test -p mkrew-ui --lib view_bar palette keymap`
Expected: PASS; review snapshot changes with `cargo insta review` and accept only the renamed / added commands.

- [ ] **Step 4: Rename through the shell**

Mechanical renames (`View::Git` → `View::Diff`):
- `shell/focus.rs` `panel_view` and `git_shown`;
- `shell/chrome.rs` `render_main` arm;
- `shell/workspace.rs` `show_view` (`if view == View::Diff { window.focus(&self.git_focus, cx); }`);
- `shell/comments.rs:25` (`View::Diff => Some(CommentContext::Diff)`), `:164`, `:253`;
- `shell/composer.rs:231-234` (`[View::Diff, View::Editor]` / `[View::Editor, View::Diff]`) and `:273`;
- `shell/send_dialog.rs:180,190`.

`shell/workspace.rs`, `view()`:

```rust
    /// The view of the active workspace; Docs reads as Terminals while `[features] docs` is
    /// off (ADR 0046), the Editor while the workspace has no open file, the Diff while it has
    /// no open diff.
    pub(crate) fn view(&self) -> View {
        let Some(id) = self.workspace else {
            return View::default();
        };
        self.views
            .get(&id)
            .copied()
            .filter(|&view| match view {
                View::Docs => self.features.docs,
                View::Editor => self.has_file_tabs(id),
                View::Diff => self.diff_open(id),
                View::Board | View::Terminals => true,
            })
            .unwrap_or_default()
    }
```

and after `has_file_tabs`:

```rust
    /// The workspace has an open diff (the Diff view exists).
    pub(crate) fn diff_open(&self, project_id: Ulid) -> bool {
        self.git.get(&project_id).is_some_and(GitView::diff_open)
    }
```

`shell/chrome.rs`, `render_view_tabs`: drop `changed` and the `.when(view == View::Git …)` chip (the count moved to the panel tab in Task 2); add `let diff = self.workspace.is_some_and(|id| self.diff_open(id));` and call `View::bar(self.features.docs, editor, diff)`.
Doc: "The views as a segmented control; Editor and Diff only while open."

`shell/navigation.rs`, `open_palette`: the `Context` literal gets

```rust
            git: self.git_shown(),
            diff: workspace.is_some_and(|id| self.diff_open(id)),
```

- [ ] **Step 5: Opening and closing a diff**

`shell/git.rs`, new functions after `review_diff`:

```rust
    /// A click or Enter on a file (or History commit) of the Git list: the Diff view shows
    /// it; the keyboard stays in the list.
    pub(crate) fn open_git_diff(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.git_view(project_id).open_diff();
        self.show_view(View::Diff, window, cx);
    }

    /// `×` on the diff, or `Esc` in the Diff view: the diff closes and Terminals shows.
    pub(crate) fn close_git_diff(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.git_view(project_id).close_diff();
        self.follow_git_diff(project_id, window, cx);
    }

    /// The Diff view was left on screen with its diff closed (by hand, or the list
    /// emptied): Terminals shows instead, with its panes attached again.
    fn follow_git_diff(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.workspace == Some(project_id)
            && self.views.get(&project_id) == Some(&View::Diff)
            && !self.diff_open(project_id)
        {
            self.show_view(View::Terminals, window, cx);
        }
    }

    /// The entry points (Review diff, View changes, Diff since, the worktree dialog, the
    /// branch chip), after the caller retargeted the view or picked its section: the right
    /// panel on its Git tab and the Diff view. Git coming on screen refreshes itself
    /// (`show_view`); one on screen already is refreshed here.
    pub(crate) fn open_git(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let git_before = self.git_shown();
        self.open_git_diff(project_id, window, cx);
        self.show_panel_tab(PanelTab::Git, window, cx);
        if git_before {
            self.refresh_git(project_id, window, cx);
        }
    }
```

- `review_diff`: `self.show_view(View::Git, window, cx);` becomes `self.open_git(project_id, window, cx);`; doc "'Review diff' on the Changes card: the Git tab and Diff view of `repo` (`""` = the workspace folder), in its working changes. `retarget_git` first, so the refresh asks for that repository …" (keep the rest).
- `retarget_git`: keep the open state with the draft:

```rust
        let draft = std::mem::take(&mut old.commit.text);
        let open = old.diff_open();
        let mut view = GitView::new(project_id, target);
        view.repo = repo;
        view.commit.text = draft;
        if open {
            // Another repository or worktree: its first file shows (or the diff closes when
            // it has none).
            view.open_diff();
        }
```

- `git_requests`: at the end, before `cx.notify()`:

```rust
        // The list emptied (commit, discard, clean worktree): the open diff closed with it.
        self.follow_git_diff(project_id, window, cx);
```

- `select_git_file` (doc "A click on a changed file: shows its diff in the Diff view."): after its `self.git_requests(…)` add `self.open_git_diff(project_id, window, cx);`.
- `select_git_commit` (doc "A click on a History commit: lists its files; the Diff view shows its first file's diff."): likewise after its `git_requests`.
- `git_key_down`: after the existing Esc steps (menu, picked lines), still inside the `escape` branch:

```rust
            if let Some(project_id) = self.workspace
                && self.view() == View::Diff
            {
                cx.stop_propagation();
                return self.close_git_diff(project_id, window, cx);
            }
```

  and before `let Some(key) = git_view::key(…)`:

```rust
        if event.keystroke.key == "enter"
            && !event.keystroke.modifiers.modified()
            && let Some(project_id) = self.workspace
        {
            cx.stop_propagation();
            return self.open_git_diff(project_id, window, cx);
        }
```

  Doc: "↑ ↓ / j k move between files; Enter opens the selection's diff; Esc closes an open Git menu, else unselects the picked diff lines, else closes the diff; the other Git keys are keymap commands ([`Self::git_key`])."

`shell/git_diff.rs`, `git_diff_toolbar`: after the `.when(view.can_comment() && !collapsed, …)` child:

```rust
            .child(
                icon_button("git-diff-close", Icon::X, theme)
                    .tooltip(tooltip(theme, "Close diff", "Esc"))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.close_git_diff(project_id, window, cx);
                    })),
            )
```

Same close button as the find bar's (`files_render.rs:562`, `icon_button("file-find-close", Icon::X, theme)`).
Doc: "… Discard, Stage file, "⋯" (comment, send to agent) and × (close the diff)."

`shell/inspector.rs`, after `show_file_tree`:

```rust
    /// Ctrl+Shift+4: the right panel on the Git tab, whose list gets the keyboard; from a
    /// view without the panel, Terminals first.
    pub(crate) fn show_git_panel(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.workspace else {
            return;
        };
        if !self.panel_view() {
            self.show_view(View::Terminals, window, cx);
        }
        self.show_panel_tab(PanelTab::Git, window, cx);
        // Without loaded tabs the Terminals view draws no panel: nothing to focus.
        if self.view() != View::Terminals || self.tabs.contains_key(&project_id) {
            window.focus(&self.git_focus, cx);
        }
    }
```

`shell/sessions.rs:418`:

```rust
            Command::ShowGitPanel => self.show_git_panel(window, cx),
            Command::ShowDiff => {
                if self.workspace.is_some_and(|id| self.diff_open(id)) {
                    self.show_view(View::Diff, window, cx);
                }
            }
```

`shell/board.rs`, `show_git_since` (doc "The Git tab and Diff view listing the changes since `since`'s checkpoint."):

```rust
        let _ = self.git_view(project_id).show_since(since);
        self.git_files_scroll.scroll_to_item(0, ScrollStrategy::Top);
        // `open_git` refreshes either way, asking the status `show_since` would.
        self.open_git(project_id, window, cx);
```

`shell/worktrees.rs`, `worktree_dialog_git`: after `retarget_git`, only `self.open_git(project_id, window, cx);`.
Doc: "'Open Git view' of the worktree dialog: the Git tab and Diff view on the task's worktree, to commit or unstage there."
If the button's label says "Open Git view", rename it "Open in Git" (grep `Open Git view`).

`shell/chrome.rs`, branch chip: `shell.show_view(View::Git, window, cx);` becomes `if let Some(id) = shell.workspace { shell.open_git(id, window, cx); }`; switcher doc "… a click opens the Git tab and the diff …".

- [ ] **Step 6: Build, test, lint, grep**

Run: `cargo test -p mkrew-ui`, then `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.
Run: `rg -n "View::Git|ShowGit\b|\"show_git\"|Git view" crates`
Expected: no `View::Git` / `ShowGit` / `"show_git"`; reword any "Git view" comment that now means the Git tab or the Diff view.

- [ ] **Step 7: Smoke on the real app**

- `Ctrl+Shift+4`: the panel opens on Git; ↑ ↓ move the selection; no `Diff` in the view bar yet.
- Click a file (or Enter): the Diff view shows it; `Diff` is in the view bar after `Editor`.
- `s` stages: the diff stays open on the moved file; `n`, `d` work.
- Pick lines, comment, Send to agent.
- `×`, then reopen and `Esc`: `Diff` leaves the view bar, Terminals shows with live panes.
- Commit everything with the diff open: the diff closes, Terminals shows.
- Switch repo chip with the diff open: the diff stays open on the other repo's first file.
- History (`h`): click a commit, its first file's diff shows.
- Session tab's Changes card "Review diff", a Review card's "View changes", the title-bar branch chip: each opens the Git tab and the diff.
- Palette: "View: Git panel", "View: Diff" (only with a diff open).

- [ ] **Step 8: Commit**

```bash
git add crates/mkrew-ui/src
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Diff view replaces the Git view"
```

---

### Task 4: SPEC, ADR, human check, full verify

**Files:**
- Create: `docs/decisions/0084-git-in-the-right-panel.md`
- Modify: `docs/decisions/0081-files-tree-in-the-right-panel.md` (Status line only)
- Modify: `docs/SPEC.md` (§10 ~745-930, §12A line "what the Git view calls unstaged", §12B ~1189-1270, §13 ~1271-1340, config sample `right_panel` line)
- Modify: `docs/HUMAN_CHECKS.md` ("To verify" section, newest first)

- [ ] **Step 1: ADR 0084** (list `docs/decisions/` first: if 0084 is taken, use the next free number everywhere in this task)

```markdown
# 0084 - Git in the right panel

- Date: 2026-10-27
- Status: accepted
- Milestone: M7

## Context

The Git view took the whole main area: reviewing what an agent changed meant leaving its terminal.
ADR 0081 moved the file tree into a right panel beside the terminals; the Git view's side column (header, list, commit box) has the same shape.

## Options

1. Keep the Git view - pros: no change / cons: changes and agents never on screen together.
2. Draw the existing `GitView` in two places: the list in a `Git` tab of the right panel, the diff in a center `Diff` view shown only while a diff is open - pros: no new model, like VS Code's Source Control / cons: the list must fit 240 px.
3. Diffs as tabs of the Editor view - pros: one center tab strip / cons: `FileTab` assumes a file (save, unsaved prompts, change gutter), so every tab path needs a tab kind.

## Decision

Option 2.
The right panel has `Files │ Session │ Git` tabs; `[ui] right_panel` accepts `"git"`.
A click or Enter on a file opens the `Diff` view; `×`, `Esc` or an emptied list close it.
History stays in the Git tab.
`Ctrl+Shift+4` shows the Git tab (`show_git_panel`), `Ctrl+Shift+7` the diff (`show_diff`).
Git refreshes while its tab or the Diff view is on screen.

## Consequences

Supersedes in part ADR 0081 (two panel tabs).
SPEC §10, §12A, §12B, §13 and the config sample change in the same commit.
`keymap.toml` entries named `show_git` are reported as unknown (no alias).
```

Set the Status of ADR 0081 to `accepted; superseded in part by 0084`.

- [ ] **Step 2: SPEC**

- §10 sketch: view bar `[Terminals│Board│Docs│Editor*│Diff*]`, the panel's tab row `[Files│Session│Git]`, and "Diff: while a diff is open" under it.
  "The right panel (below) is a third column right of Terminals, the Editor and the Diff while open."
  The **Right panel** bullet: tabs `Files │ Session │ Git`; `[ui] right_panel` (`"files"`, `"session"`, `"git"` or `"hidden"`); from the Board or Docs view the toggle shows Terminals; the `Git` tab shows the changed count.
  The Changes card's **Review diff** "opens the Git tab and the diff of that repository (§12B)".
- §12A: "what the Git view calls unstaged" → "what the Git tab calls unstaged".
- §12B title "UI: Git (right panel tab and Diff view)".
  First lines: the Git tab holds the header, Working / Vs. base / History, the filter and "Changed by", the list and the commit box, as wide as the panel (240-560 px; the filter row wraps).
  The `Diff` view holds the toolbar, unified / split, hunks, the comment margin and Send to agent, and is listed in the view bar only while a diff is open.
  A click or Enter opens it; `×`, `Esc` or an emptied list close it; a selection moving by itself keeps it open.
  Refresh: "when its tab or the Diff view comes on screen"; "Never run git for workspaces whose Git tab or Diff view is not on screen, except the cheap status …".
  "Changed-file list (right of the diff …)" → "Changed-file list (the Git tab) with +/− counts; selecting one shows its diff."
  "The side panel header shows `Viewed N/M`" → "The Git tab's header shows `Viewed N/M`".
  "'View changes' opens the Git view …" → "opens the Git tab and the diff …".
- §13: `View | Show Git panel | Ctrl+Shift+4`, `View | Show diff | Ctrl+Shift+7`; the Git rows say "while the Git list (or the diff, with the list hidden) has the keyboard"; add `Enter | Open the selected file's diff` and extend `Esc` with "else close the diff".
- Config sample: `right_panel = "hidden"   # §10; "files" | "session" | "git" | "hidden"; the right panel's toggle and tabs write it`.

- [ ] **Step 3: HUMAN_CHECKS** (top of "To verify")

```markdown
### To verify - 2026-10-27 Git in the right panel (ADR 0084)

- [ ] **Git tab (dark + light):** `Files │ Session │ Git 3` fits the tab row; at 240 px the header, branch card, segments, filter (with "By:" wrapped) and commit box fit without clipping; the repo strip falls back to one tab when crowded.
- [ ] **Diff view:** a click shows `Diff` in the view bar and the diff with its × at the toolbar's end; `Esc` / × hide it and Terminals shows; the diff card has the same frame as before.
- [ ] **Keys with the panel hidden:** in the Diff view with the panel closed, `n` / `p` / `d` / `s` still act.
- [ ] **Restart:** the panel comes back on the Git tab when left there.
```

- [ ] **Step 4: Full verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt, clippy `-D warnings` and every test pass.
Run: `rg -n "View::Git|ShowGit\b|\"show_git\"|GIT_FILE_LIST_WIDTH|render_git\(" crates docs/SPEC.md`
Expected: no matches.

- [ ] **Step 5: Final smoke on the real app**

Repeat the Task 3 smoke list in one run, plus:
- an agent edits a file while only the Git tab is on screen (Terminals view): the list updates (`git.changed`);
- hide the panel while the list has the keyboard in the Terminals view: the pane gets it;
- hide it in the Diff view: the Git keys keep working.

- [ ] **Step 6: Commit**

```bash
git add docs/SPEC.md docs/decisions/0084-git-in-the-right-panel.md docs/decisions/0081-files-tree-in-the-right-panel.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "docs: git in the right panel (ADR 0084)"
```
