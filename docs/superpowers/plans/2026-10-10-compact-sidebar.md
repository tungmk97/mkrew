# Compact Sidebar Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a third sidebar mode next to full width and hidden: a narrow **compact rail**. It has its own shortcut (Ctrl+Alt+B), its own status-bar icon and its own palette entry. The rail still shows each row's main information: the workspace avatar with `?` or its working count, and each session's status mark with its agent chip. Hovering a row shows its full title in a tooltip.

**Architecture:** A pure `sidebar::Layout` (a hidden flag and a compact flag) replaces `Shell.sidebar_hidden`, so hiding remembers which width the sidebar comes back at. The pure tooltip texts (`sidebar::workspace_tip`, `sidebar::session_tip`) and `Sidebar::clear_filter` get unit tests in `sidebar.rs`. `main.rs` gets the `Command::ToggleCompactSidebar` wiring, `Shell::render_compact_sidebar`, and two row renderers. The rows reuse the existing click handlers, the workspace menu and the keyboard peek.

**Tech Stack:** Rust, GPUI (`gpui-pre 0.3.7`), crate `mkrew-ui` only. No protocol, daemon or migration change.

**Spec:** the user's request in this conversation (2026-10-10), verbatim: *"for the left workspace sidebar, I don't want it hide is totaly hidde, we juz change ui to like compact mode with small width for leftside bar but still can show main information of each tab. And also when hover over to 1 tab show full tiltle kind of tools tip. Ah I thikn instead of change total, add 1 more feature 1 shortcut 1 new icon for this compact mode for workspace"*. The canonical spec sections this plan updates are `docs/SPEC.md` §10 (Sidebar bullet), §13 (shortcut table) and §20A.1 (peek on hover).

## Global Constraints

- Toggle sidebar keeps working exactly as it does today: Ctrl+Shift+B, the `◧` button and the palette entry "Global: Toggle sidebar". Compact is an **added** mode, not a replacement.
- New default key: `ctrl-alt-b` (label `Ctrl+Alt+B`), global (context `None`). It is unbound today: `default_keys()` in `keymap.rs` has no `ctrl-alt-b`.
- Every color, spacing, radius, font size and width comes from `crates/mkrew-ui/src/theme.rs` (SPEC §14). The only new token is `theme::COMPACT_SIDEBAR_WIDTH`. Do not put hex colors or raw pixel numbers in `main.rs`.
- Spacing comes only from `space::XS/S/M/L/XL/XXL`, and radius only from `radius::*`.
- The layout lives in memory only and is not remembered across restarts, the same as hidden today (SPEC §10).
- Tooltips use the existing `tooltip(theme, text, keys)` helper. The status-bar button's tooltip shows its shortcut.
- **User preference: never commit.** Leave every change uncommitted. The last step of each task prints a Conventional Commit message the user can use. Do not run `git commit`.
- Leave the human's uncommitted edits alone: `.github/workflows/*`, `packaging/`, `docs/BUILD_VI.md`, `Cargo.lock`, and the x11/wayland lines in `crates/mkrew-ui/Cargo.toml`. The working tree also holds the uncommitted results of earlier plans. That is the baseline: do not revert it.
- `crates/mkrew-ui/src/main.rs` is about 20k lines. Read it in ranges and find each function by name (`grep -n "fn render_sidebar"`). The line numbers below are from 2026-10-10 and drift as edits land.
- GPUI rule: call `.hover()` at most once per element. `icon_button()` and `row_action()` already call it.
- Disk C: is 90 % full, so do not create new target dirs. Build with `-j 4`. If a build fails with "Access is denied" on an `.exe`, rename that file to `*.locked-old.exe`. Never kill the daemon or the agents.

## Review Focus

1. **The filter box has the keyboard when compact turns on** (by key or by clicking the icon). The keyboard must go to the focused pane, not stay in a field that is no longer drawn. Pinned by a HUMAN_CHECKS smoke line in Task 2. The fix sits in `Shell::toggle_compact_sidebar` and in the filter guard in `Shell::run`.
2. **A filter or chip is active when compact turns on.** The rail has no filter controls, so every live session must show again. Nothing may stay narrowed where the user cannot see why. Pinned by `clear_filter_drops_the_query_and_the_chip` in Task 1.
3. **Width memory across hide and show.** Hidden, then the compact key, must show the rail. Rail, then hide, then show must bring the rail back, not the full sidebar. Pinned by `layout_keeps_its_width_while_hidden` in Task 1.
4. **A session without a title, or a workspace without a summary.** The tooltip must still read cleanly, with no empty lines and no dangling `·`. Pinned by `compact_tips_spell_out_what_the_rail_leaves_out` in Task 1.
5. **Peek and menus next to the narrow rail, and large fonts.** From the rail, the keyboard peek (Ctrl+Shift+Space), the workspace right-click menu and the session right-click menu must open fully visible and work. Rail content must not clip at a large `[ui] font_size`. Pinned by HUMAN_CHECKS smoke lines in Task 2.

---

### Task 1: Pure sidebar model: layout, filter reset, compact tooltips

**Files:**
- Modify: `crates/mkrew-ui/src/sidebar.rs` (new `Layout`, `workspace_tip`, `session_tip`, `Sidebar::clear_filter`; `Sidebar::reveal` reuses `clear_filter`; tests go in the existing `mod tests`)

**Interfaces:**
- Consumes these existing items: `WorkspaceRow { name, path, summary, .. }`, `SessionRow { agent, title, state, state_changed_at, .. }`, `status::text(state, exit_code, changed_at, now_ms) -> String`, the `Sidebar` fields `filter` (`Input`) and `chip`.
- Produces (used by Task 2):
  - `pub struct sidebar::Layout` (`Copy`; `Default` = full width, shown) with:
    - `fn hidden(self) -> bool`
    - `fn compact(self) -> bool`: true only while the rail is **on screen**
    - `fn toggle_hidden(&mut self)`
    - `fn show(&mut self)`
    - `fn toggle_compact(&mut self)`
  - `pub fn sidebar::workspace_tip(row: &WorkspaceRow) -> String`
  - `pub fn sidebar::session_tip(row: &SessionRow, now_ms: i64) -> String`
  - `pub fn Sidebar::clear_filter(&mut self)`

- [ ] **Step 1: Write the failing tests**

Add these inside `mod tests` in `crates/mkrew-ui/src/sidebar.rs`, after the last test (`toggle_all_closes_when_any_is_open_and_reveal_clears_what_hides_a_session`):

```rust
    #[test]
    fn layout_keeps_its_width_while_hidden() {
        let mut layout = Layout::default();
        assert!(!layout.hidden() && !layout.compact(), "starts full width");

        layout.toggle_compact();
        assert!(!layout.hidden() && layout.compact());
        layout.toggle_hidden();
        assert!(layout.hidden() && !layout.compact(), "hidden: no rail on screen");
        layout.show();
        assert!(layout.compact(), "shown again at the rail's width");

        layout.toggle_compact();
        assert!(!layout.compact(), "rail → full width");
        layout.toggle_hidden();
        layout.toggle_hidden();
        assert!(!layout.hidden() && !layout.compact(), "hide/show keeps full width");

        layout.toggle_hidden();
        layout.toggle_compact();
        assert!(
            !layout.hidden() && layout.compact(),
            "the compact key on a hidden full-width sidebar shows the rail"
        );
    }

    #[test]
    fn clear_filter_drops_the_query_and_the_chip() {
        let api = project("api");
        let idle = session(&api, 1, SessionState::Idle);
        let mut sidebar = Sidebar::default();
        sidebar.load(vec![api], vec![idle.clone()]);
        sidebar.filter_mut().insert("zzz");
        sidebar.set_chip(Chip::Working);
        assert!(sidebar.rows().is_empty());

        sidebar.clear_filter();
        assert!(!sidebar.filtering());
        assert_eq!(session_ids(&sidebar.rows()), vec![idle.id]);
    }

    #[test]
    fn compact_tips_spell_out_what_the_rail_leaves_out() {
        let (api, web) = (project("api"), project("web"));
        let mut titled = session(&api, 1_000, SessionState::Working);
        titled.title = Some("Fix the very long login redirect bug".into());
        let untitled = session(&web, 1_000, SessionState::Done);
        let mut sidebar = Sidebar::default();
        sidebar.load(vec![api, web], vec![titled, untitled]);

        // The first load opens both (each has a live session): api, its session, web, its session.
        let rows = sidebar.rows();
        let sessions: Vec<&SessionRow> = rows
            .iter()
            .filter_map(|row| match row {
                Row::Session(row) => Some(row),
                _ => None,
            })
            .collect();
        assert_eq!(
            workspace_tip(workspace(&rows[0])),
            "api\n1 working\nC:\\src\\api"
        );
        assert_eq!(
            session_tip(sessions[0], 121_000),
            "claude · Fix the very long login redirect bug\nworking · 2m"
        );
        // No summary (done is not summarised), no title: no empty lines, no dangling `·`.
        assert_eq!(workspace_tip(workspace(&rows[2])), "web\nC:\\src\\web");
        assert_eq!(session_tip(sessions[1], 121_000), "claude\ndone");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mkrew-ui --lib sidebar -j 4`
Expected: FAIL to compile with `cannot find struct Layout`, `no method named clear_filter`, and `cannot find function workspace_tip` / `session_tip`.

- [ ] **Step 3: Implement**

In `crates/mkrew-ui/src/sidebar.rs`, add this after `pub fn workspace_summary(..)` (before the `/// One visible sidebar row` doc comment of `enum Row`):

```rust
/// Hover text of a workspace in the compact rail: the name, then its summary (if any) and its
/// folder, one per line.
pub fn workspace_tip(row: &WorkspaceRow) -> String {
    let mut lines = vec![row.name.as_str()];
    lines.extend(row.summary.as_deref());
    lines.push(&row.path);
    lines.join("\n")
}

/// Hover text of a session in the compact rail: `agent · full title` (or just the agent), then
/// the status in words, `working · 2m`. Rows are live sessions only, so there is no exit code.
pub fn session_tip(row: &SessionRow, now_ms: i64) -> String {
    let name = match &row.title {
        Some(title) => format!("{} · {title}", row.agent),
        None => row.agent.clone(),
    };
    let state = status::text(row.state, None, row.state_changed_at, now_ms);
    format!("{name}\n{state}")
}

/// How the sidebar shows (SPEC §10): full width, the compact rail, or hidden. Hiding keeps the
/// width it comes back at. In memory only: not remembered across restarts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Layout {
    hidden: bool,
    compact: bool,
}

impl Layout {
    pub fn hidden(self) -> bool {
        self.hidden
    }

    /// The compact rail is on screen (false while hidden).
    pub fn compact(self) -> bool {
        !self.hidden && self.compact
    }

    /// Toggle sidebar: hide, or show again at the last width.
    pub fn toggle_hidden(&mut self) {
        self.hidden = !self.hidden;
    }

    /// Something the sidebar draws needs it (the peek, the workspace menu): shown at the last width.
    pub fn show(&mut self) {
        self.hidden = false;
    }

    /// Toggle compact sidebar: full width ↔ rail; a hidden sidebar comes back as the rail.
    pub fn toggle_compact(&mut self) {
        self.compact = self.hidden || !self.compact;
        self.hidden = false;
    }
}
```

In `impl Sidebar`, replace `reveal` and add `clear_filter` right after it:

```rust
    /// Makes `project_id`'s session rows visible: opened, and no query or chip hides them
    /// (the keyboard peek hangs on a row).
    pub fn reveal(&mut self, project_id: Ulid) {
        self.expanded.insert(project_id);
        self.clear_filter();
    }

    /// Drops the filter text and the chip (the compact rail draws neither control).
    pub fn clear_filter(&mut self) {
        self.filter.clear();
        self.chip = Chip::All;
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p mkrew-ui --lib sidebar -j 4`
Expected: PASS. That includes the three new tests and the unchanged `toggle_all_closes_when_any_is_open_and_reveal_clears_what_hides_a_session`.

- [ ] **Step 5: Commit message (do not commit)**

Print this for the user:

```
feat(ui): sidebar layout model and compact-rail tooltips
```

---

### Task 2: Toggle compact sidebar: command, rail, status-bar icon, docs

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs` (enum, `all()`, `name()`, `default_keys()`, test `spec_defaults`)
- Modify: `crates/mkrew-ui/src/palette.rs` (`title()`, `available()`, test `availability_follows_the_view`)
- Modify: `crates/mkrew-ui/src/theme.rs` (`COMPACT_SIDEBAR_WIDTH` after `SIDEBAR_WIDTH`)
- Modify: `crates/mkrew-ui/src/main.rs`:
  - Changed: the field `sidebar_hidden` (becomes `sidebar_layout`), `toggle_sidebar`, `open_peek`, `open_workspace_menu`, `Shell::run` (filter guard and dispatch), `render_sidebar` (menu wrapping), `render_status_bar`, `workspace_row`, `Render for Shell`
  - New: `toggle_compact_sidebar`, `with_workspace_menu`, `click_workspace_row`, `render_compact_sidebar`, and the free fns `compact_workspace_row` and `compact_session_row`
- Create: `docs/decisions/0040-compact-sidebar.md`
- Modify: `docs/SPEC.md` (§10 Sidebar bullet, §13 table, §20A.1)
- Modify: `docs/HUMAN_CHECKS.md` (a new "To verify" section and one "Questions with assumed defaults" bullet)
- Modify: `docs/PROGRESS.md` (one note line)

**Interfaces:**
- Consumes from Task 1: `sidebar::Layout { hidden(), compact(), toggle_hidden(), show(), toggle_compact() }`, `sidebar::workspace_tip(&WorkspaceRow) -> String`, `sidebar::session_tip(&SessionRow, i64) -> String`, `Sidebar::clear_filter()`.
- Consumes these existing items in main.rs: `tooltip(theme, &str, keys)`, `icon_button(id, glyph, theme)`, `workspace_avatar(name, color, theme)`, `status_mark(state, now, theme)`, `agent_chip(badge, theme)`, `inert_layer()`, `Shell::with_peek(row, session_id, now, theme, cx)`, `Shell::render_workspace_menu(menu, theme, cx)`, `Shell::open_session`, `Shell::open_pane_menu`, `Shell::open_workspace_menu`, `Shell::activate_workspace`, `Shell::add_workspace`, `Shell::close_peek`, `Shell::focus_pane`, `Shell::agent_badge`, `Shell::focused_session`.
- Produces:
  - `Command::ToggleCompactSidebar` (name `toggle_compact_sidebar`, default `ctrl-alt-b`, context `None`)
  - `theme::COMPACT_SIDEBAR_WIDTH: f32`
  - `Shell.sidebar_layout: sidebar::Layout`
  - `Shell::toggle_compact_sidebar(&mut self, window, cx)`

- [ ] **Step 1: Write the failing tests (keymap + palette)**

In `crates/mkrew-ui/src/keymap.rs`, test `spec_defaults`, add right after `assert_eq!(Command::ToggleSidebar.context(), None);`:

```rust
        assert_eq!(bound("ctrl-alt-b"), Some(Command::ToggleCompactSidebar));
        assert_eq!(keymap.label(Command::ToggleCompactSidebar), "Ctrl+Alt+B");
        // Global like Toggle sidebar: it works while a terminal has focus.
        assert_eq!(Command::ToggleCompactSidebar.context(), None);
```

In `crates/mkrew-ui/src/palette.rs`, test `availability_follows_the_view`, in the `commands(&none)` expected list, replace

```rust
                Command::ToggleFullScreen,
                Command::ToggleSidebar
            ]
```

with

```rust
                Command::ToggleFullScreen,
                Command::ToggleSidebar,
                Command::ToggleCompactSidebar
            ]
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mkrew-ui --lib keymap -j 4`
Expected: FAIL to compile with `no variant or associated item named ToggleCompactSidebar`.

- [ ] **Step 3: Command, palette and theme token**

In the `keymap.rs` enum, after `ToggleSidebar,`:

```rust
    /// Narrow the workspace sidebar to the compact rail, or widen it back (a hidden sidebar
    /// comes back as the rail). Not remembered across restarts.
    ToggleCompactSidebar,
```

Then in `keymap.rs`:
- `all()`: add `Command::ToggleCompactSidebar,` after `Command::ToggleSidebar,`.
- `name()`: add `Command::ToggleCompactSidebar => "toggle_compact_sidebar".into(),` after the `ToggleSidebar` arm.
- `default_keys()`: add `Command::ToggleCompactSidebar => "ctrl-alt-b".into(),` after the `ToggleSidebar` arm.
- `context()`: no change (`_ => None` already covers it).

In `palette.rs`:
- `title()`: add `Command::ToggleCompactSidebar => "Global: Toggle compact sidebar".into(),` after the `ToggleSidebar` arm.
- `available()`: change `| Command::ToggleSidebar => true,` to:

```rust
        | Command::ToggleSidebar
        | Command::ToggleCompactSidebar => true,
```

In `theme.rs`, after `pub const SIDEBAR_WIDTH: f32 = 280.0;`:

```rust
/// Width of the compact sidebar rail (Toggle compact sidebar): a status mark, a gap and an agent
/// chip plus the rail's padding. In px at the default font size; drawn with [`rems_px`] so it
/// widens with `[ui] font_size` like the avatars and chips inside it.
pub const COMPACT_SIDEBAR_WIDTH: f32 = 56.0;
```

Run: `cargo test -p mkrew-ui --lib keymap -j 4` and `cargo test -p mkrew-ui --lib palette -j 4`
Expected: PASS. `every_default_parses_and_is_unique`, `names_round_trip` and the template test cover the new entry through `all()`. If the `mkrew-ui` binary target blocks the lib test build with a non-exhaustive match in `Shell::run`, do Step 4 first and run both tests then.

- [ ] **Step 4: Shell state: `sidebar_hidden` → `sidebar_layout`**

`grep -n sidebar_hidden crates/mkrew-ui/src/main.rs` lists every use of the old field: the field itself, `Shell::new`, `open_workspace_menu`, `toggle_sidebar`, `open_peek`, the status-bar tooltip and `Render for Shell`. Change each one as follows.

The field (struct `Shell`, about line 335):

```rust
    /// Full width, compact rail or hidden (Toggle sidebar / Toggle compact sidebar). Not
    /// remembered across restarts.
    sidebar_layout: sidebar::Layout,
```

The `Shell::new` initializer: `sidebar_layout: sidebar::Layout::default(),` replaces `sidebar_hidden: false,`.

`open_workspace_menu` and `open_peek`: replace `self.sidebar_hidden = false;` with `self.sidebar_layout.show();`. The sidebar comes back at the width it had.

Replace `toggle_sidebar` and add `toggle_compact_sidebar` after it:

```rust
    /// Hiding also closes what only the sidebar draws (the peek, the workspace menu), so the
    /// keyboard never stays in an invisible peek. Showing brings back the last width.
    fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.sidebar_layout.toggle_hidden();
        if self.sidebar_layout.hidden() {
            self.close_peek(window, cx);
            self.workspace_menu = None;
        }
        cx.notify();
    }

    /// Full width ↔ compact rail; a hidden sidebar comes back as the rail. The rail draws no
    /// filter box or chips, so entering it clears them (nothing stays narrowed unseen) and the
    /// keyboard leaves the filter box. A peek or menu hung on a full-width row closes.
    fn toggle_compact_sidebar(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.sidebar_layout.toggle_compact();
        if self.sidebar_layout.compact() {
            if self.filter_focus.is_focused(window) {
                self.focus_pane(window, cx);
            }
            self.sidebar.clear_filter();
            self.close_peek(window, cx);
            self.workspace_menu = None;
        }
        cx.notify();
    }
```

In `Shell::run`, the filter guard

```rust
            if matches!(command, Command::Peek | Command::ToggleSidebar) {
```

becomes

```rust
            if matches!(
                command,
                Command::Peek | Command::ToggleSidebar | Command::ToggleCompactSidebar
            ) {
```

and the comment above it ends `// leave it (Peek, Toggle sidebar, Toggle compact sidebar).`. In the dispatch match, after `Command::ToggleSidebar => self.toggle_sidebar(window, cx),` add:

```rust
            Command::ToggleCompactSidebar => self.toggle_compact_sidebar(window, cx),
```

In `Render for Shell`, replace the children under `// Hidden: not built at all`:

```rust
                    // Hidden: not built at all (nothing to lay out or paint).
                    .children((!self.sidebar_layout.hidden()).then(|| {
                        if self.sidebar_layout.compact() {
                            self.render_compact_sidebar(now, &theme, cx)
                                .into_any_element()
                        } else {
                            self.render_sidebar(now, &theme, window, cx)
                                .into_any_element()
                        }
                    }))
```

- [ ] **Step 5: Shared row pieces (no behavior change)**

Add these to `impl Shell`, next to `with_peek`:

```rust
    /// `row` with the workspace menu dropping down below it while the menu is open for
    /// `project_id` (both sidebar widths).
    fn with_workspace_menu(
        &self,
        row: gpui::AnyElement,
        project_id: Ulid,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        match self
            .workspace_menu
            .as_ref()
            .filter(|menu| menu.project_id == project_id)
        {
            Some(menu) => div()
                .child(row)
                .child(self.render_workspace_menu(menu, theme, cx))
                .into_any_element(),
            None => row,
        }
    }

    /// A left click on a workspace row: the first shows the workspace; clicking the shown one
    /// expands/collapses it.
    fn click_workspace_row(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.workspace == Some(project_id) {
            self.sidebar.toggle(project_id);
            cx.notify();
        } else {
            self.activate_workspace(project_id, window, cx);
        }
    }
```

In `render_sidebar`, the `Row::Workspace(row)` arm becomes:

```rust
                    Row::Workspace(row) => {
                        if !sessions.is_empty() {
                            list.push(guide(std::mem::take(&mut sessions)));
                        }
                        let (project_id, active) =
                            (row.project_id, self.workspace == Some(row.project_id));
                        let summary = self.run_summary(project_id);
                        let run_tip = summary.any_live().then(|| summary.tooltip());
                        let row = workspace_row(row, active, run_tip, theme, cx);
                        list.push(self.with_workspace_menu(row, project_id, theme, cx));
                    }
```

In the free fn `workspace_row`, the `.on_click(..)` becomes the code below. Delete its comment line `// The first click shows the workspace; …`: that doc now lives on `click_workspace_row`.

```rust
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.click_workspace_row(project_id, window, cx);
        }))
```

- [ ] **Step 6: The compact rail**

Add this to `impl Shell`, right after `render_sidebar`:

```rust
    /// The compact rail (Toggle compact sidebar): per workspace its avatar, per open workspace
    /// its sessions' status marks and agent chips; each row's tooltip spells out the rest. Same
    /// rows, clicks, right-click menus and keyboard peek as the full sidebar; no filter, chips
    /// or hover peek (the tooltip replaces it). Without workspaces only `+` shows.
    fn render_compact_sidebar(
        &self,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let shown = self.focused_session();
        let header = div()
            .flex_none()
            .h(theme::rems_px(theme::WORKSPACE_ROW_HEIGHT))
            .flex()
            .items_center()
            .justify_center()
            .when(self.connection.is_some(), |header| {
                header.child(
                    icon_button("add-workspace", "+", theme)
                        .tooltip(tooltip(
                            theme,
                            "Add workspace",
                            self.keymap.label(Command::AddWorkspace),
                        ))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            shell.add_workspace(window, cx);
                        })),
                )
            });
        // An error is too long for the rail: its mark, with the text on hover.
        let mut list = Vec::new();
        for (i, error) in [&self.sidebar_error, &self.keymap_error]
            .into_iter()
            .enumerate()
        {
            if let Some(error) = error {
                list.push(
                    div()
                        .id(SharedString::from(format!("sidebar-error-{i}")))
                        .flex()
                        .justify_center()
                        .py(px(space::XS))
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .tooltip(tooltip(theme, error, ""))
                        .child("✕")
                        .into_any_element(),
                );
            }
        }
        for row in self.sidebar.view(&self.viewed).rows {
            match row {
                Row::Workspace(row) => {
                    let (project_id, active) =
                        (row.project_id, self.workspace == Some(row.project_id));
                    let row = compact_workspace_row(row, active, theme, cx);
                    list.push(self.with_workspace_menu(row, project_id, theme, cx));
                }
                Row::Session(row) => {
                    let session_id = row.session_id;
                    let badge = self.agent_badge(&row.agent);
                    let row =
                        compact_session_row(row, badge, shown == Some(session_id), now, theme, cx);
                    list.push(self.with_peek(row, session_id, now, theme, cx));
                }
                // The workspace's tooltip already shows it has nothing running (no summary).
                Row::NoSessions { .. } => {}
            }
        }
        div()
            .flex_none()
            .w(theme::rems_px(theme::COMPACT_SIDEBAR_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.sidebar))
            .border_r(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .child(header)
            .child(
                div()
                    .id("sidebar")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .px(px(space::XS))
                    .overflow_y_scroll()
                    .children(list),
            )
    }
```

Add these free fns right after `fn session_row(..)`:

```rust
/// A workspace in the compact rail: its avatar, then `?` while a session needs input, else its
/// working count. Clicks as [`workspace_row`]; the tooltip has the name, summary and folder.
fn compact_workspace_row(
    row: WorkspaceRow,
    active: bool,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let project_id = row.project_id;
    let hover = theme.hover;
    let color = row
        .color
        .as_deref()
        .and_then(|name| theme.workspace_color(name));
    let mark = if row.needs_input {
        let icon = sidebar::status_display(SessionState::NeedsInput).icon;
        Some((icon.to_owned(), theme.status_needs_input))
    } else {
        (row.working > 0).then(|| (row.working.to_string(), theme.text_muted))
    };
    div()
        .id(SharedString::from(format!("workspace-{project_id}")))
        .h(theme::rems_px(theme::WORKSPACE_ROW_HEIGHT))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(space::XS))
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .when(active, |el| el.bg(rgb(theme.selected)))
        .hover(move |style| style.bg(rgb(hover)))
        .tooltip(tooltip(theme, &sidebar::workspace_tip(&row), ""))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.click_workspace_row(project_id, window, cx);
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |shell, _, window, cx| {
                shell.open_workspace_menu(project_id, window, cx);
            }),
        )
        .child(workspace_avatar(&row.name, color, theme))
        .children(mark.map(|(text, color)| {
            div()
                .flex_none()
                .text_size(font_size::META)
                .font_weight(FontWeight(weight::SEMIBOLD))
                .text_color(rgb(color))
                .child(text)
        }))
        .into_any_element()
}

/// A session in the compact rail: its status mark and agent chip. Clicks as [`session_row`];
/// the tooltip has `agent · full title` and the status in words. No hover peek (the tooltip
/// replaces it); the keyboard peek still hangs on this row.
fn compact_session_row(
    row: SessionRow,
    badge: String,
    selected: bool,
    now: i64,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let session_id = row.session_id;
    let hover = theme.hover;
    div()
        .id(SharedString::from(format!("session-{session_id}")))
        .h(theme::rems_px(theme::SESSION_ROW_HEIGHT))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(space::XS))
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .when(selected, |el| el.bg(rgb(theme.selected)))
        .hover(move |style| style.bg(rgb(hover)))
        .tooltip(tooltip(theme, &sidebar::session_tip(&row, now), ""))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.open_session(session_id, window, cx);
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                // As in `session_row`: show the pane first (Rename edits its header).
                shell.open_session(session_id, window, cx);
                shell.open_pane_menu(session_id, event.position, window, cx);
            }),
        )
        .child(status_mark(row.state, now, theme))
        .child(agent_chip(badge, theme))
        .into_any_element()
}
```

- [ ] **Step 7: The status-bar icon**

In `render_status_bar`, change the `◧` tooltip condition from `self.sidebar_hidden` to `self.sidebar_layout.hidden()`. After the `let toggle = …;` statement, add:

```rust
        // Full width ↔ compact rail (a hidden sidebar comes back as the rail).
        let compact = self.sidebar_layout.compact();
        let compact_toggle = div()
            .id("toggle-compact-sidebar")
            .flex_none()
            .px(px(space::XS))
            .rounded(px(radius::SMALL))
            .cursor_pointer()
            .text_color(rgb(theme.text_muted))
            .hover(move |style| style.bg(rgb(hover)))
            .tooltip(tooltip(
                theme,
                if compact {
                    "Expand sidebar"
                } else {
                    "Compact sidebar"
                },
                self.keymap.label(Command::ToggleCompactSidebar),
            ))
            .on_click(cx.listener(|shell, _, window, cx| {
                shell.toggle_compact_sidebar(window, cx);
            }))
            .child(if compact { "⇥" } else { "⇤" })
            .when(modal, |el| el.relative().child(inert_layer()));
```

In the bar's child chain, `.child(toggle)` becomes `.child(toggle).child(compact_toggle)`. Change the method doc's first line to `/// The status bar (SPEC §10): sidebar toggles (hide, compact), Inbox state, git HEAD; then the focused agent`.

- [ ] **Step 8: Build, lint, test**

Run: `cargo fmt --all`
Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`
Expected: no warnings. A leftover `sidebar_hidden` is a compile error, so the build itself proves none remains.
Run: `cargo test -p mkrew-ui -j 4`
Expected: PASS. Known trap: rustc can crash on the mkrew-ui test build (`STATUS_STACK_BUFFER_OVERRUN`). If it does, run `cargo test -p mkrew-ui --lib -j 2`.
Run: `git diff -U0 -- crates/mkrew-ui/src/main.rs | grep -nE "0x[0-9A-Fa-f]{6}|px\([0-9]"`
Expected: no output, meaning no literal colors or pixels outside `theme.rs`.

- [ ] **Step 9: Smoke run**

Run: `cargo run -p mkrew-ui -j 4` (it starts the daemon if needed). If no window can be driven in this session, say so in the report; the Step 12 HUMAN_CHECKS lines then cover the manual check. With a window:
- Press Ctrl+Alt+B → the rail (about 56 px) shows avatars and session marks, and `⇤` has become `⇥`. Press it again → full width.
- Hover a rail session → the tooltip reads `claude · <full title>`, then `working · 2m`. Hover a workspace → name, summary, folder.
- Rail, then Ctrl+Shift+B twice → the rail is back. Hidden, then Ctrl+Alt+B → the rail.

- [ ] **Step 10: ADR 0040**

Create `docs/decisions/0040-compact-sidebar.md`:

```markdown
# 0040 — Compact sidebar rail

- Date: 2026-10-10
- Status: accepted
- Milestone: none (outside the milestone loop; user request "compact mode for the workspace sidebar")

## Context

- SPEC §10 has two sidebar states: full width (280 px) and hidden (Toggle sidebar, Ctrl+Shift+B).
  Hidden loses every status at a glance; full width takes room from the terminals.
- The user asked for a third, narrow mode that still shows each row's main information, with the
  full title on hover, as an added feature with its own shortcut and icon (not replacing hide).
- §20A.1 opens the peek after hovering a session row 300 ms. On a narrow rail a hover tooltip
  with the full title is what the user asked for, and the two would fight for the same hover.

## Options

1. Replace hidden with compact. Rejected by the user ("instead of change total, add 1 more").
2. A draggable sidebar width. More state and a resize handle; no designed narrow layout.
3. A separate compact mode, toggled by its own key and status-bar icon. (Chosen.)

## Decision

- `Toggle compact sidebar` (Ctrl+Alt+B, a `⇤` / `⇥` status-bar button, palette "Global: Toggle
  compact sidebar") switches full width ↔ a rail of `COMPACT_SIDEBAR_WIDTH` (56 px at the
  default font size, scaled with `[ui] font_size`). On a hidden sidebar it shows the rail.
  Toggle sidebar keeps hiding and shows again at the last width. In memory only.
- Rail rows: a workspace is its avatar plus `?` (needs input) or its working count; a session is
  its status mark plus agent chip. Clicks, right-click menus and expand/collapse are the full
  sidebar's. Each row's tooltip has the full text: workspace name / summary / folder; session
  `agent · title` / status in words.
- The rail draws no filter box or chips: entering it clears them, so no row stays hidden unseen.
- In the rail, hovering a session row shows the tooltip, not the peek. The keyboard peek
  (Ctrl+Shift+Space) still opens next to the row.

## Consequences

- SPEC §10, §13 and §20A.1 are updated in the same change.
- Pin marks, times, the run `▶` and the hover `✕` / `⋯` actions are not on the rail (the
  right-click menus still reach Pin, Remove and the workspace actions).
- Without workspaces the rail shows only `+` (with its tooltip and shortcut); the full sidebar
  keeps the empty-state sentence.
```

- [ ] **Step 11: SPEC**

In `docs/SPEC.md` §10, Sidebar bullet, append after `Toggle sidebar (Ctrl+Shift+B, status bar, palette) hides it; not remembered across restarts.`:

```markdown
  Toggle compact sidebar (Ctrl+Alt+B, the status bar's `⇤` / `⇥`, palette) narrows it to a rail:
  workspace avatars with `?` or the working count, and per open workspace its sessions' status
  mark + agent chip; hovering a rail row shows its full name, status and folder as a tooltip
  (instead of the peek). Entering the rail clears the filter and chip; a hidden sidebar comes back
  at the width it had, and Ctrl+Alt+B on a hidden sidebar shows the rail. Not remembered
  across restarts (ADR 0040).
```

In the §13 table, after `| Global | Toggle sidebar | Ctrl+Shift+B |`:

```markdown
| Global | Toggle compact sidebar | Ctrl+Alt+B |
```

In §20A.1, change `Hovering a sidebar session row for 300 ms, or` to `Hovering a sidebar session row for 300 ms (full-width sidebar; the compact rail shows a tooltip instead, ADR 0040), or`.

- [ ] **Step 12: HUMAN_CHECKS and PROGRESS**

In `docs/HUMAN_CHECKS.md`, insert this directly under the intro paragraph of `## To verify (non-blocking)`, before the `### To verify — 2026-10-10 inbox: redesign` heading:

```markdown
### To verify — 2026-10-10 compact sidebar (plan docs/superpowers/plans/2026-10-10-compact-sidebar.md)

- [ ] **Toggle:** Ctrl+Alt+B (also with a terminal focused), the status bar's `⇤` and the palette's
  "Global: Toggle compact sidebar" switch full width ↔ the rail; the icon flips `⇤` / `⇥` and its
  tooltip "Compact sidebar" / "Expand sidebar" shows Ctrl+Alt+B; panes re-wrap. `◧` /
  Ctrl+Shift+B still hide and show; hidden → Ctrl+Alt+B shows the rail; rail → hide → show
  returns the rail. After a restart the sidebar is full width.
- [ ] **Rail content (dark and light):** each workspace shows its avatar in its color, `?` in the
  needs-input color while a session asks, else its working count; the active workspace and the
  focused session are highlighted; session rows show the working dots / status glyph and the agent
  chip; the `⇤` / `⇥` glyphs render (not boxes). Nothing is clipped at `[ui] font_size = 18`.
- [ ] **Tooltips:** hover a session with a long title → `claude · <the full title>` and
  `working · 2m` on a second line; a session without a title → agent and status only. Hover a
  workspace → name, summary (if any) and folder. No peek opens on hover in the rail.
- [ ] **Clicks and menus from the rail:** click a workspace → shown; click the shown one →
  collapses/expands its sessions; right-click → the workspace menu opens fully visible; click a
  session → its pane; right-click a session → the pane menu; `+` adds a workspace.
- [ ] **Filter and keyboard:** type in the filter and pick `Working`, keep the filter focused, press
  Ctrl+Alt+B → the rail shows every live session and typing goes to the focused terminal (not to
  an invisible filter); same when clicking `⇤` instead. Ctrl+Shift+Space in the rail → the peek
  opens beside the row and reply / Esc work.
```

In `## Questions with assumed defaults`, add this as the first bullet:

```markdown
- (compact sidebar) **Rail details:** key Ctrl+Alt+B (Ctrl+Shift+B stays hide/show); icon
  `⇤` / `⇥` in the status bar next to `◧`; width 56 px scaled with the font size; entering the rail
  clears the filter and chip; no hover peek on the rail (the tooltip replaces it, keyboard peek
  stays); pin marks, times, `▶` and hover actions are left out (menus still reach them); no
  workspaces = only `+`; not remembered across restarts. ADR 0040.
```

In `docs/PROGRESS.md`, "Notes for the next session", add after the `2026-10-10 inbox redesign` line:

```markdown
- 2026-10-10 compact sidebar (plan docs/superpowers/plans/2026-10-10-compact-sidebar.md, ADR 0040): `sidebar::Layout` replaces `Shell.sidebar_hidden`; `Command::ToggleCompactSidebar` (Ctrl+Alt+B, status-bar `⇤`/`⇥`); rail = `render_compact_sidebar` + `compact_workspace_row` / `compact_session_row`, tooltips `sidebar::workspace_tip` / `session_tip`. UI only. Manual checks in HUMAN_CHECKS "2026-10-10 compact sidebar". Outside the milestone loop.
```

- [ ] **Step 13: Commit message (do not commit)**

Print this for the user:

```
feat(ui): compact sidebar rail with tooltips (Ctrl+Alt+B)
```
